// The Atrium chat page in a real browser, with two people. See README.md.
//   node chat_ui_e2e.mjs         (SHOTS=dir keeps screenshots; CHROMIUM=/usr/bin/chromium picks the browser)
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
		const r = await context.request.post(`${BASE}/api/plugins/atrium_chat/${fn}`, { data, headers: { 'X-CSRF-Token': token, Origin: BASE } });
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
	const run = Math.random().toString(36).slice(2, 6);
	const team = (await owner.call('create_team', { name: 'Team ' + run })).id;
	const { id: ch } = await owner.call('create_channel', { name: 'ops-' + run, team });
	const { id: other } = await owner.call('create_channel', { name: 'side-' + run });
	await bob.call('join_channel', { channel: ch });
	await bob.call('join_channel', { channel: other });

	const A = await owner.context.newPage();
	const B = await bob.context.newPage();
	const errors = [];
	for (const p of [A, B]) p.on('pageerror', (e) => errors.push(String(e)));

	// --- the app opens on a welcome pane, with the rail
	await A.goto(`${BASE}/web/chat`);
	await A.getByText('Choose a conversation').waitFor({ timeout: 15000 });
	check('without a channel the page asks to choose one', true);
	await A.locator('[data-pane=rail] a[data-rail-item]', { hasText: 'ops-' + run }).waitFor({ timeout: 15000 });
	check('the rail lists the channel under its team', (await A.locator('[data-pane=rail]').getByRole('button', { name: 'Team ' + run, exact: true }).count()) === 1);
	check('the page has buttons to browse and to start things', (await A.getByRole('link', { name: 'Browse' }).count()) + (await A.getByRole('button', { name: 'Browse' }).count()) > 0);
	await shot(A, 'chat-welcome');

	// --- open the channel, write, and the other person sees it live
	await A.locator('[data-pane=rail] a[data-rail-item]', { hasText: 'ops-' + run }).click();
	await A.waitForURL(new RegExp(`/chat/${ch}$`));
	await B.goto(`${BASE}/web/chat/${ch}`);
	await A.getByText('Nobody has written here yet.').waitFor({ timeout: 10000 });
	check('an empty channel says so', true);
	await A.getByRole('textbox', { name: 'Message' }).fill('first message');
	await A.getByRole('textbox', { name: 'Message' }).press('Enter');
	await B.waitForFunction(() => document.body.innerText.includes('first message'), null, { timeout: 10000 }).then(
		() => check('the other person sees the message live', true),
		() => check('the other person sees the message live', false, 'not shown')
	);
	check("the author's name is shown", (await text(B)).includes('owner'));
	await B.locator('[data-part=presence-count]').waitFor({ timeout: 8000 });
	check('presence shows who is on the channel', /2 people here/.test(await B.locator('[data-part=presence-count]').innerText()));

	// --- unread badges follow other channels, through the person's own inbox topic
	await owner.call('post_message', { channel: other, body: 'in the other channel' });
	await B.locator('[data-pane=rail] a[data-rail-item]', { hasText: 'side-' + run }).locator('[data-part=rail-badge]').waitFor({ timeout: 10000 }).then(
		() => check("another channel's unread badge appears live (per-person inbox)", true),
		() => check("another channel's unread badge appears live (per-person inbox)", false, 'no badge')
	);
	await shot(B, 'chat-badge');

	// --- threads
	await B.getByRole('link', { name: 'Reply' }).first().click();
	await B.waitForURL(/thread=/);
	await B.getByPlaceholder('Reply…').waitFor({ timeout: 10000 });
	check('"Reply" opens the thread pane without leaving the channel', B.url().includes(`/chat/${ch}`) && B.url().includes('thread='));
	await B.getByPlaceholder('Reply…').fill('a reply');
	await B.getByPlaceholder('Reply…').press('Enter');
	await B.waitForFunction(() => document.body.innerText.includes('a reply'), null, { timeout: 10000 }).catch(async (e) => {
		await shot(B, 'thread-fail');
		console.log('B shows:', (await text(B)).slice(0, 500).replace(/\n+/g, ' | '));
		throw e;
	});
	await A.getByRole('link', { name: '1 reply' }).waitFor({ timeout: 10000 }).then(
		() => check("the channel shows the message now has '1 reply', live", true),
		() => check("the channel shows the message now has '1 reply', live", false, 'no reply link')
	);
	await shot(B, 'chat-thread');
	await A.getByRole('link', { name: '1 reply' }).click();
	await A.waitForFunction(() => document.body.innerText.includes('a reply'), null, { timeout: 10000 });
	check("the other person opens the thread and reads the reply", true);

	// --- the heading, reactions, editing and deleting
	for (const p of [A, B]) p.on('dialog', (d) => d.accept());
	await A.goto(`${BASE}/web/chat/${ch}`);
	await A.locator('[data-part=heading-title]').waitFor({ timeout: 15000 });
	check('the heading shows the channel name', (await A.locator('[data-part=heading-title]').innerText()) === '# ops-' + run);
	check('...and who is in it', /people/.test(await A.locator('[data-part=heading]').innerText()));
	await A.getByText('first message').first().waitFor({ timeout: 10000 });
	const first = A.locator('[data-part=message]', { hasText: 'first message' }).first();
	await first.hover();
	await first.getByRole('button', { name: 'Add a reaction' }).click();
	await A.getByRole('button', { name: 'React with 👍' }).click();
	await A.locator('[data-part=reactions] button', { hasText: '👍' }).waitFor({ timeout: 8000 });
	check('a reaction chip appears with its count', (await A.locator('[data-part=reactions] button', { hasText: '👍' }).innerText()).includes('1'));
	await B.goto(`${BASE}/web/chat/${ch}`);
	await B.locator('[data-part=reactions] button', { hasText: '👍' }).waitFor({ timeout: 10000 });
	check('the other person sees it', true);
	await B.locator('[data-part=reactions] button', { hasText: '👍' }).click();
	await A.waitForFunction(() => [...document.querySelectorAll('[data-part=reactions] button')].some((b) => b.textContent.includes('👍') && b.textContent.includes('2')), null, { timeout: 8000 }).then(
		() => check('a second person joining the reaction updates it live', true),
		() => check('a second person joining the reaction updates it live', false, 'count not 2')
	);
	await shot(A, 'chat-reaction');
	// edit own message
	await A.getByRole('textbox', { name: 'Message' }).fill('to be edited');
	await A.getByRole('textbox', { name: 'Message' }).press('Enter');
	const mine = A.locator('[data-part=message]', { hasText: 'to be edited' }).first();
	await mine.waitFor({ timeout: 8000 });
	await mine.hover();
	await mine.getByRole('button', { name: 'Edit message' }).click();
	await A.getByRole('textbox', { name: 'Edit message' }).fill('edited text');
	await A.getByRole('textbox', { name: 'Edit message' }).press('Enter');
	await B.waitForFunction(() => document.body.innerText.includes('edited text') && document.body.innerText.includes('(edited)'), null, { timeout: 8000 }).then(
		() => check('an edit shows live for the other person, marked as edited', true),
		() => check('an edit shows live for the other person, marked as edited', false, 'not shown')
	);
	const other_msg = B.locator('[data-part=message]', { hasText: 'edited text' }).first();
	await other_msg.hover();
	check("someone else's message has no edit or delete buttons", (await other_msg.getByRole('button', { name: 'Edit message' }).count()) === 0 && (await other_msg.getByRole('button', { name: 'Delete message' }).count()) === 0);
	const own = A.locator('[data-part=message]', { hasText: 'edited text' }).first();
	await own.hover();
	await own.getByRole('button', { name: 'Delete message' }).click();
	await B.waitForFunction(() => !document.body.innerText.includes('edited text'), null, { timeout: 8000 }).then(
		() => check('a deleted message disappears live for the other person', true),
		() => check('a deleted message disappears live for the other person', false, 'still shown')
	);

	// --- mentions: autocomplete, highlight, and a mark on the rail
	await B.goto(`${BASE}/web/chat`);
	await B.locator('[data-pane=rail] a[data-rail-item]', { hasText: 'ops-' + run }).waitFor({ timeout: 15000 });
	const boxA = A.getByRole('textbox', { name: 'Message' });
	await boxA.fill('hello ');
	await boxA.pressSequentially('@bo');
	await A.locator('[data-part=mention-list]').waitFor({ timeout: 8000 });
	check('typing @ offers the people', (await A.locator('[data-part=mention-list]').innerText()).toLowerCase().includes('bob'));
	await shot(A, 'chat-mention-list');
	await boxA.press('Enter');
	check('Enter chooses the person instead of sending', (await boxA.inputValue()).startsWith('hello @bob'));
	await boxA.pressSequentially('see this');
	await boxA.press('Enter');
	await B.locator('[data-pane=rail] a[data-rail-item]', { hasText: 'ops-' + run }).locator('[data-part=rail-mention]').waitFor({ timeout: 10000 }).then(
		async () => check('the person mentioned gets a mention mark on the rail', (await B.locator('[data-part=rail-mention]').first().innerText()).startsWith('@')),
		() => check('the person mentioned gets a mention mark on the rail', false, 'no mark')
	);
	await shot(B, 'chat-mention-rail');
	await B.locator('[data-pane=rail] a[data-rail-item]', { hasText: 'ops-' + run }).click();
	await B.locator('[data-part=mention]').first().waitFor({ timeout: 10000 }).then(
		() => check('a mention is highlighted in the message', true),
		() => check('a mention is highlighted in the message', false, 'no highlight')
	);

	// --- people, settings, search
	await A.goto(`${BASE}/web/chat/${ch}`);
	await A.getByRole('button', { name: 'People' }).click();
	await A.getByText('bob').first().waitFor({ timeout: 15000 }).then(
		() => check('the people dialog lists who is in the conversation', true),
		() => check('the people dialog lists who is in the conversation', false, 'no bob')
	);
	check('...with a Remove button', (await A.getByRole('button', { name: 'Remove' }).count()) > 0);
	await A.keyboard.press('Escape');
	await A.goto(`${BASE}/web/chat/${ch}/settings`);
	await A.locator('input').first().waitFor({ timeout: 15000 });
	await A.waitForFunction((n) => [...document.querySelectorAll('input')].some((i) => i.value === n), 'ops-' + run, { timeout: 10000 }).then(
		() => check('the settings form is filled with the channel', true),
		() => check('the settings form is filled with the channel', false, 'not filled')
	);
	await A.goto(`${BASE}/web/chat/search`);
	await A.getByPlaceholder(/Search/i).first().waitFor({ timeout: 15000 });
	await A.getByPlaceholder(/Search/i).first().fill('first message');
	await A.getByPlaceholder(/Search/i).first().press('Enter');
	await A.getByText('first message').first().waitFor({ timeout: 10000 }).then(
		() => check('searching finds the message', true),
		() => check('searching finds the message', false, 'no result')
	);
	await shot(A, 'chat-search');

	// --- new message form
	await A.goto(`${BASE}/web/chat`);
	await A.getByRole('button', { name: 'New conversation' }).first().click();
	await A.getByRole('dialog').getByText('To').first().waitFor({ timeout: 15000 });
	await shot(A, 'chat-new-message');
	check('the new conversation dialog opens', (await A.getByRole('dialog').count()) === 1);

	// --- browse
	await A.goto(`${BASE}/web/chat/browse`);
	await A.getByText('general').first().waitFor({ timeout: 15000 }).then(
		() => check('browsing lists the open channels', true),
		() => check('browsing lists the open channels', false, 'no general')
	);

	// --- narrow screen
	await B.setViewportSize({ width: 420, height: 800 });
	await B.goto(`${BASE}/web/chat`);
	await B.waitForSelector('[data-part=panes]', { timeout: 15000 });
	check('a narrow screen starts on the list of conversations', (await B.locator('[data-pane=rail]').count()) === 1 && (await B.locator('[data-pane=main]').count()) === 0);
	check('no page errors in the browser', errors.length === 0, errors.join(' | ').slice(0, 300));
} catch (error) {
	check('the run finished', false, String(error?.stack ?? error));
} finally {
	await browser.close();
}
console.log(`\n${failures.length} failure(s)`);
process.exit(failures.length ? 1 : 0);
