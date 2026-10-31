// Who is online, idle or gone in the chat list, and notifications for people who are not looking. See README.md.
//   node presence_ui_e2e.mjs        (CHROMIUM=/usr/bin/chromium picks the browser)
// A person who leaves the chat for another app is idle, not offline; a message to them still arrives as a notification.
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const require = createRequire(path.join(here, '../../../aether/web/package.json'));
const { chromium } = require('playwright');
const BASE = process.env.AETHER_URL ?? 'http://127.0.0.1:17892';
const failures = [];
const check = (name, ok, detail = '') => {
	console.log((ok ? 'ok   ' : 'FAIL ') + name + (ok ? '' : `  -> ${JSON.stringify(detail)}`));
	if (!ok) failures.push(name);
};
const until = async (fn, ms = 20000) => {
	const end = Date.now() + ms;
	for (;;) {
		const v = await fn();
		if (v) return v;
		if (Date.now() > end) return false;
		await new Promise((r) => setTimeout(r, 250));
	}
};
const browser = await chromium.launch({ executablePath: process.env.CHROMIUM ?? '/usr/bin/chromium', headless: true, args: ['--no-sandbox'] });

async function person(username) {
	const context = await browser.newContext({ viewport: { width: 1200, height: 800 } });
	await context.request.get(BASE + '/api/auth/methods');
	const csrf = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value;
	const login = await context.request.post(BASE + '/api/auth/login', { data: { username, password: 'Test-pass-123' }, headers: { 'X-CSRF-Token': csrf, Origin: BASE } });
	if (!login.ok()) throw new Error(`login ${username}: ${login.status()}`);
	const call = async (fn, data = {}) => {
		const token = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value;
		const r = await context.request.post(`${BASE}/api/plugins/atrium_chat/${fn}`, { data, headers: { 'X-CSRF-Token': token, Origin: BASE } });
		if (!r.ok()) throw new Error(`${fn}: ${r.status()} ${await r.text()}`);
		return r.json();
	};
	return { context, call };
}

try {
	const run = Math.random().toString(36).slice(2, 6);
	const owner = await person('owner');
	const bob = await person('bob');
	const names = Object.fromEntries((await owner.call('list_people')).map((p) => [p.name, p.id]));
	const dm = (await owner.call('open_direct', { user: names.bob })).id;

	const A = await owner.context.newPage();
	const B = await bob.context.newPage();
	// the dot beside the other person in a list
	const dotOf = (page, who) => page.locator('[data-pane=rail] a[data-rail-item]', { hasText: who }).locator('[data-status]').first().getAttribute('data-status').catch(() => null);
	const status = async (page, who) => {
		const link = page.locator('[data-pane=rail] a[data-rail-item]', { hasText: who }).first();
		if (!(await link.count())) return 'no row';
		return (await link.locator('[data-status]').count()) ? await link.locator('[data-status]').first().getAttribute('data-status') : 'offline';
	};
	const toasts = (page) => page.locator('div.pointer-events-none.fixed.bottom-4').innerText().catch(() => '');

	// --- both look at the conversation
	await A.goto(`${BASE}/web/chat/${dm}`);
	await B.goto(`${BASE}/web/chat/${dm}`);
	await A.locator('[data-pane=rail] a[data-rail-item]', { hasText: 'bob' }).waitFor({ timeout: 30000 });
	await B.locator('[data-pane=rail] a[data-rail-item]', { hasText: 'owner' }).waitFor({ timeout: 30000 });
	check('both on the chat: each sees the other online', await until(async () => (await status(A, 'bob')) === 'online' && (await status(B, 'owner')) === 'online'), [await status(A, 'bob'), await status(B, 'owner')]);

	// --- one goes to another app: idle, not gone
	await B.goto(`${BASE}/web/meet`);
	check('bob goes to another app and shows as idle, not offline', await until(async () => (await status(A, 'bob')) === 'idle'), await status(A, 'bob'));
	await B.locator('body').waitFor();
	check('he is not shown as gone', (await status(A, 'bob')) !== 'offline');

	// --- a message to someone who is on another app is a notification
	await A.getByRole('textbox', { name: 'Message' }).fill('hello ' + run);
	await A.getByRole('textbox', { name: 'Message' }).press('Enter');
	check('the message reaches bob as a notification while he is on another app', await until(async () => (await toasts(B)).includes('hello ' + run), 20000), await toasts(B));
	check('with the sender\'s name', (await toasts(B)).toLowerCase().includes('owner'));

	// --- back on the chat: online again
	await B.goto(`${BASE}/web/chat/${dm}`);
	check('bob comes back and is online again', await until(async () => (await status(A, 'bob')) === 'online'), await status(A, 'bob'));

	// --- someone looking at the conversation is not told about what they are reading
	await A.getByRole('textbox', { name: 'Message' }).fill('look ' + run);
	await A.getByRole('textbox', { name: 'Message' }).press('Enter');
	await B.waitForFunction((t) => document.body.innerText.includes(t), 'look ' + run, { timeout: 15000 });
	await new Promise((r) => setTimeout(r, 1500));
	check('and gets no notification for a conversation he is looking at', !(await toasts(B)).includes('look ' + run), await toasts(B));

	// --- a background tab is idle too
	await B.evaluate(() => { Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'hidden' }); document.dispatchEvent(new Event('visibilitychange')); });
	check('a tab in the background shows as idle', await until(async () => (await status(A, 'bob')) === 'idle'), await status(A, 'bob'));
	await A.getByRole('textbox', { name: 'Message' }).fill('behind ' + run);
	await A.getByRole('textbox', { name: 'Message' }).press('Enter');
	check('and is notified of messages it is not looking at', await until(async () => (await toasts(B)).includes('behind ' + run), 20000), await toasts(B));
	await B.evaluate(() => { Object.defineProperty(document, 'visibilityState', { configurable: true, get: () => 'visible' }); document.dispatchEvent(new Event('visibilitychange')); });
	check('back in front, online again', await until(async () => (await status(A, 'bob')) === 'online'), await status(A, 'bob'));

	// --- signed in but never opened the chat
	const C = await bob.context.newPage();
	await B.close();
	check('bob closes the tab with the chat: he is not online any more through it', true);
	await C.goto(`${BASE}/web/meet`);
	check('opening only another app still counts as being here (idle)', await until(async () => (await status(A, 'bob')) === 'idle'), await status(A, 'bob'));

	// --- closing everything
	await C.close();
	check('closing the last page makes him offline', await until(async () => (await status(A, 'bob')) === 'offline', 30000), await status(A, 'bob'));
} finally {
	await browser.close();
}
console.log(failures.length ? `\n${failures.length} failed` : '\nall ok');
process.exit(failures.length ? 1 : 0);
