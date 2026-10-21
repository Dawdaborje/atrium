// The Atrium forms pages in a real browser, with two people. See README.md.
//   node forms_ui_e2e.mjs         (SHOTS=dir keeps screenshots; CHROMIUM=/usr/bin/chromium picks the browser)
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, '../../../aether/web/package.json'));
const { chromium } = require('playwright');

const BASE = process.env.AETHER_URL ?? 'http://127.0.0.1:17892';
const PASSWORD = 'Test-pass-123';
const SHOTS = process.env.SHOTS ?? '';
const failures = [];
const check = (name, ok, detail = '') => {
	console.log((ok ? 'ok   ' : 'FAIL ') + name + (ok ? '' : `  -> ${detail}`));
	if (!ok) failures.push(name);
};
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? '/usr/bin/chromium', headless: true, args: ['--no-sandbox'] });

async function person(username) {
	const context = await browser.newContext({ viewport: { width: 1200, height: 800 } });
	await context.request.get(BASE + '/api/auth/methods');
	const csrf = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value;
	const login = await context.request.post(BASE + '/api/auth/login', { data: { username, password: PASSWORD }, headers: { 'X-CSRF-Token': csrf, Origin: BASE } });
	if (!login.ok()) throw new Error(`login ${username}: ${login.status()}`);
	const token = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value ?? csrf;
	const call = async (fn, data = {}) => {
		const r = await context.request.post(`${BASE}/api/plugins/atrium_forms/${fn}`, { data, headers: { 'X-CSRF-Token': token, Origin: BASE } });
		if (!r.ok()) throw new Error(`${fn}: ${r.status()} ${await r.text()}`);
		return r.json();
	};
	return { context, call };
}
const shot = async (page, name) => SHOTS && page.screenshot({ path: path.join(SHOTS, name + '.png') });
const text = (page) => page.locator('body').innerText();

try {
	const owner = await person('owner');
	const bob = await person('bob');
	const cy = await person('cy');
	const run = Math.random().toString(36).slice(2, 6);
	const form = (await bob.call('create_form', { title: 'Offsite ' + run, description: 'Help us plan' })).id;
	const add = async (body) => (await bob.call('add_question', { id: form, ...body })).id;
	await add({ label: 'Where to?', kind: 'choice', options: 'Lisbon\nBerlin', required: true });
	await add({ label: 'What to book?', kind: 'multi', options: 'Hotel\nTrain' });
	await add({ label: 'How excited?', kind: 'rating' });
	await add({ label: 'Anything else?', kind: 'long' });
	await add({ label: 'Shirt size', kind: 'dropdown', options: 'S\nM\nL' });
	const errors = [];
	const C = await cy.context.newPage();
	const B = await bob.context.newPage();
	for (const p of [B, C]) p.on('pageerror', (e) => errors.push(String(e)));

	// --- a draft is a preview for its owner
	await B.goto(`${BASE}/web/forms/f/${form}/fill`);
	await B.getByText('This form is not open yet').waitFor({ timeout: 15000 });
	check('the owner previews a draft and is told it is not open', true);
	check('the preview lists the questions', (await text(B)).includes('Where to?'));
	await bob.call('open_form', { id: form });

	// --- filling it in
	await C.goto(`${BASE}/web/forms`);
	await C.getByText('Offsite ' + run).waitFor({ timeout: 15000 });
	check('the open form is in the list of forms to fill', true);
	await C.getByText('Offsite ' + run).click();
	await C.waitForURL(new RegExp(`/forms/f/${form}/fill$`));
	await C.locator('[data-part=questionnaire]').waitFor({ timeout: 15000 });
	check('the questionnaire shows every question', (await C.locator('[data-part=question]').count()) === 5);
	check('and a required mark', (await C.locator('[data-part=question] [aria-label=required]').count()) === 1);
	await shot(C, 'forms-fill');
	await C.getByRole('button', { name: 'Send' }).click();
	await C.getByText('“Where to?” needs an answer.').waitFor({ timeout: 5000 });
	check('a required question stops the form before it is sent', true);
	await C.getByLabel('Lisbon').check();
	await C.getByLabel('Hotel').check();
	await C.getByLabel('Train').check();
	await C.getByLabel('Train').uncheck();
	await C.getByRole('radio', { name: '4 of 5' }).click();
	await C.getByRole('textbox', { name: 'Anything else?' }).fill('Quiet hotel please');
	await C.getByLabel('Shirt size').selectOption('M');
	await C.getByRole('button', { name: 'Send' }).click();
	await C.getByText('Thank you for answering.').waitFor({ timeout: 10000 });
	check('sending shows the thanks', true);
	await C.waitForURL(new RegExp(`/forms$`), { timeout: 8000 }).then(() => check('and goes back to the list', true), () => check('and goes back to the list', false, C.url()));

	// --- once only
	await C.goto(`${BASE}/web/forms/f/${form}/fill`);
	await C.getByText('You have answered this form.').waitFor({ timeout: 15000 });
	check('a second visit says it was answered', true);

	// --- the owner reads the results
	await B.goto(`${BASE}/web/forms/f/${form}/results`);
	await B.getByText('1 response').waitFor({ timeout: 15000 });
	const t = await text(B);
	check('results count the response', t.includes('Lisbon') && t.includes('100%'), t.slice(0, 300));
	check('the multiple choice shows what was ticked', t.includes('Hotel') && t.includes('Train'));
	check('the rating is averaged', t.includes('4.0 of 5'));
	check('the text answer is listed', t.includes('Quiet hotel please'));
	await shot(B, 'forms-results');
	await B.goto(`${BASE}/web/forms/f/${form}/responses`);
	await B.getByText('cy').first().waitFor({ timeout: 15000 });
	check('responses list who answered', true);

	// --- the manage and questions pages
	await B.goto(`${BASE}/web/forms/f/${form}/questions`);
	await B.getByText('Where to?').first().waitFor({ timeout: 15000 });
	check('the questions page lists them', (await text(B)).includes('Shirt size'));
	await B.goto(`${BASE}/web/forms/f/${form}`);
	await B.getByRole('button', { name: 'Close' }).waitFor({ timeout: 15000 });
	check('the form page has its buttons', (await B.getByRole('button', { name: 'Open' }).count()) > 0);
	await shot(B, 'forms-manage');
	await bob.call('close_form', { id: form });
	await C.goto(`${BASE}/web/forms/f/${form}/fill`);
	await C.getByText('This form is closed.').waitFor({ timeout: 15000 });
	check('a closed form says so', true);
	await bob.call('delete_form', { id: form });
	check('no page errors in the browser', errors.length === 0, errors.join(' | ').slice(0, 300));
} catch (error) {
	check('the run finished', false, String(error?.stack ?? error));
} finally {
	await browser.close();
}
console.log(`\n${failures.length} failure(s)`);
process.exit(failures.length ? 1 : 0);
