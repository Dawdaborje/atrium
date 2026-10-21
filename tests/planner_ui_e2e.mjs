// The planner board in a real browser: adding a task in a column, dragging it to another column, a blocked task, the task page.
//   node planner_ui_e2e.mjs        (SHOTS=dir keeps screenshots; CHROMIUM=/usr/bin/chromium picks the browser)
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, '../../../aether/web/package.json'));
const { chromium } = require('playwright');
const BASE = process.env.AETHER_URL ?? 'http://127.0.0.1:17892';
const SHOTS = process.env.SHOTS ?? '';
const failures = [];
const check = (name, ok, detail = '') => {
	console.log((ok ? 'ok   ' : 'FAIL ') + name + (ok ? '' : `  -> ${detail}`));
	if (!ok) failures.push(name);
};
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? '/usr/bin/chromium', headless: true, args: ['--no-sandbox'] });
const shot = async (page, name) => SHOTS && page.screenshot({ path: path.join(SHOTS, name + '.png') });

async function person(username) {
	const context = await browser.newContext({ viewport: { width: 1400, height: 900 } });
	await context.request.get(BASE + '/api/auth/methods');
	const csrf = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value;
	const login = await context.request.post(BASE + '/api/auth/login', { data: { username, password: 'Test-pass-123' }, headers: { 'X-CSRF-Token': csrf, Origin: BASE } });
	if (!login.ok()) throw new Error(`login ${username}: ${login.status()}`);
	return context;
}

async function api(context, fn, data = {}) {
	const csrf = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value;
	const r = await context.request.post(`${BASE}/api/plugins/atrium_planner/${fn}`, { data, headers: { 'X-CSRF-Token': csrf, Origin: BASE } });
	const body = await r.json().catch(() => null);
	if (!r.ok()) throw new Error(`${fn}: ${r.status()} ${JSON.stringify(body)}`);
	return body;
}

try {
	const run = Math.random().toString(36).slice(2, 6);
	const ctx = await person('owner');
	const page = await ctx.newPage();
	const errors = [];
	page.on('pageerror', (e) => errors.push(String(e)));

	const plan = (await api(ctx, 'create_plan', { name: `UI plan ${run}` })).id;
	const buckets = await api(ctx, 'board_buckets', { plan });
	const [todo, doing, done] = buckets.map((b) => b.id);
	const board = async () => api(ctx, 'board_tasks', { plan });
	const column = (label) => page.locator('[data-part="kanban.column"]', { has: page.getByRole('heading', { name: label, exact: true }) });
	const card = (title) => page.locator('[data-part="kanban.card"]', { hasText: title });

	await page.goto(`${BASE}/web/planner/${plan}`);
	await column('To do').waitFor({ timeout: 20000 });
	check('the board shows the three buckets of a new plan', (await page.locator('[data-part="kanban.column"]').count()) === 3);
	await shot(page, 'planner-board-empty');

	// --- adding a task at the top of a column
	await column('To do').getByRole('button', { name: 'Add', exact: true }).click();
	await column('To do').getByLabel('Quick create title').fill(`Write plan ${run}`);
	await column('To do').getByLabel('Quick create title').press('Enter');
	await card(`Write plan ${run}`).waitFor({ timeout: 10000 }).then(
		() => check('a task added in a column appears there', true),
		() => check('a task added in a column appears there', false, 'no card')
	);
	let rows = await board();
	const first = rows.find((r) => r.title === `Write plan ${run}`);
	check('and was saved in that bucket', first && first.bucket === todo, JSON.stringify(rows));

	// --- dragging it to another bucket
	await card(`Write plan ${run}`).dragTo(column('In progress'));
	await column('In progress').locator('[data-part="kanban.card"]', { hasText: `Write plan ${run}` }).waitFor({ timeout: 10000 }).then(
		() => check('dragging a card to another column moves it', true),
		() => check('dragging a card to another column moves it', false, 'card stayed')
	);
	rows = await board();
	check('and the move was saved', rows.find((r) => r.title === `Write plan ${run}`)?.bucket === doing, JSON.stringify(rows));

	// --- a task that waits for another
	const second = (await api(ctx, 'quick_add', { plan, title: `Review plan ${run}`, bucket: todo })).id;
	await api(ctx, 'update_task', { id: second, blocked_by: [first.id] });
	await page.reload();
	await card(`Review plan ${run}`).waitFor({ timeout: 20000 });
	check('a task that waits for another is marked Blocked', (await card(`Review plan ${run}`).getByText('Blocked').count()) === 1);
	check('the one it waits for is not', (await card(`Write plan ${run}`).getByText('Blocked').count()) === 0);
	await shot(page, 'planner-board-blocked');

	// --- the task page
	await card(`Review plan ${run}`).click();
	await page.waitForURL(/\/planner\/task\//, { timeout: 15000 });
	await page.getByText(`Write plan ${run}`).first().waitFor({ timeout: 15000 }).then(
		() => check('the task page shows what the task waits for', true),
		() => check('the task page shows what the task waits for', false, 'not shown')
	);
	await page.getByRole('button', { name: 'Complete', exact: true }).click();
	await page.getByText(/waits|blocked|first/i).first().waitFor({ timeout: 10000 }).then(
		() => check('completing a blocked task is refused with a reason', true),
		() => check('completing a blocked task is refused with a reason', false, 'no message')
	);
	check('and it is still open', (await board()).find((r) => r.id === second)?.progress !== 'completed');
	await shot(page, 'planner-task-blocked');

	// --- freeing it
	await api(ctx, 'complete_task', { id: first.id });
	await page.goto(`${BASE}/web/planner/${plan}`);
	await card(`Review plan ${run}`).waitFor({ timeout: 20000 });
	check('once the other is done, the Blocked mark goes', (await card(`Review plan ${run}`).getByText('Blocked').count()) === 0);

	// --- editing a task
	await card(`Review plan ${run}`).click();
	await page.waitForURL(/\/planner\/task\//, { timeout: 15000 });
	const title = page.getByLabel('Title');
	await title.waitFor({ timeout: 15000 });
	await page.waitForFunction((t) => [...document.querySelectorAll('input')].some((i) => i.value === t), `Review plan ${run}`, { timeout: 15000 });
	await page.waitForTimeout(1500);
	await title.fill(`Reviewed plan ${run}`);
	// assign it to a person with the picker of the field
	await page.getByRole('combobox', { name: /add people|people/i }).or(page.getByText(/^Add people/)).first().click();
	await page.getByRole('option', { name: /^owner/i }).first().click();
	await page.locator('[data-part="form.many2many"]').first().getByText(/owner/i).waitFor({ timeout: 10000 }).then(
		() => check('a plan member picked for "Assigned to" shows by name', true),
		() => check('a plan member picked for "Assigned to" shows by name', false, 'no chip')
	);
	await page.getByRole('button', { name: 'Save' }).click();
	await page.getByText('Saved').first().waitFor({ timeout: 10000 }).then(
		() => check('a task is renamed on its page', true),
		() => check('a task is renamed on its page', false, 'no confirmation')
	);
	const saved = (await board()).find((r) => r.title === `Reviewed plan ${run}`);
	check('and the board has the new name', Boolean(saved));
	check('and the person is assigned', saved && JSON.stringify(saved.assignee_names ?? saved.assignees ?? '').toLowerCase().includes('owner'), JSON.stringify(saved));
	await page.reload();
	await page.getByLabel('Title').waitFor({ timeout: 15000 });
	await page.waitForTimeout(2000);
	check('the page shows the names, not ids, when opened again', (await page.locator('[data-part="form.many2many"]').first().innerText()).toLowerCase().includes('owner'));
	await shot(page, 'planner-task-assigned');
	check('no page errors', errors.length === 0, errors.join(' | '));
} catch (e) {
	check('the script ran to the end', false, String(e));
	await shot(globalThis.lastPage ?? (await browser.contexts()[0]?.pages()[0]), 'planner-failure');
}
await browser.close();
console.log(`\n${failures.length} failure(s)`);
process.exit(failures.length ? 1 : 0);
