// The wiki, news and booking pages in a real browser. See README.md.
//   node pages_ui_e2e.mjs         (SHOTS=dir keeps screenshots; CHROMIUM=/usr/bin/chromium picks the browser)
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
	const call = async (plugin, fn, data = {}) => {
		const r = await context.request.post(`${BASE}/api/plugins/${plugin}/${fn}`, { data, headers: { 'X-CSRF-Token': token, Origin: BASE } });
		if (!r.ok()) throw new Error(`${fn}: ${r.status()} ${await r.text()}`);
		return r.json();
	};
	return { context, call };
}
const shot = async (page, name) => SHOTS && page.screenshot({ path: path.join(SHOTS, name + '.png') });
const text = (page) => page.locator('body').innerText();
const W = 'atrium_wiki', N = 'atrium_news', B_ = 'atrium_booking', C_ = 'atrium_calendar';

try {
	const owner = await person('owner');
	const bob = await person('bob');
	const cy = await person('cy');
	const run = Math.random().toString(36).slice(2, 6);
	const errors = [];
	const A = await owner.context.newPage();
	const Bp = await bob.context.newPage();
	const Cp = await cy.context.newPage();
	for (const p of [A, Bp, Cp]) p.on('pageerror', (e) => errors.push(String(e)));
	const people = Object.fromEntries((await owner.call('atrium_chat', 'list_people')).map((p) => [p.name, p.id]));

	// ===================== wiki
	const sp = (await owner.call(W, 'create_space', { name: 'Handbook ' + run, description: 'How we work', members: [people.bob] })).id;
	const home = (await owner.call(W, 'create_page', { space: sp, title: 'Welcome ' + run, body: '# Hello team\n\nRead [[Rules ' + run + ']] first.\n\n- one\n- two' })).id;
	const rules = (await owner.call(W, 'create_page', { space: sp, title: 'Rules ' + run, body: 'Be kind.', parent: home })).id;
	await Bp.goto(`${BASE}/web/wiki`);
	await Bp.getByText('Handbook ' + run).waitFor({ timeout: 15000 });
	check('the wiki lists the spaces', true);
	await Bp.goto(`${BASE}/web/wiki/s/${sp}`);
	await Bp.getByText('Welcome ' + run).first().waitFor({ timeout: 15000 });
	check('a space shows its page tree with the subpage indented', (await text(Bp)).includes('– Rules ' + run));
	await Bp.goto(`${BASE}/web/wiki/p/${home}`);
	await Bp.getByText('Hello team').waitFor({ timeout: 15000 });
	const link = Bp.getByRole('link', { name: 'Rules ' + run }).first();
	check('a page renders its Markdown with links to other pages', (await link.count()) > 0 && (await link.getAttribute('href'))?.includes(`/wiki/p/${rules}`));
	await shot(Bp, 'wiki-page');
	await Bp.getByRole('button', { name: 'Edit' }).or(Bp.getByRole('link', { name: 'Edit' })).first().click();
	await Bp.waitForURL(new RegExp(`/wiki/p/${home}/edit$`));
	const body = Bp.getByRole('textbox', { name: /Text/ });
	await body.waitFor({ timeout: 15000 });
	await Bp.waitForFunction(() => [...document.querySelectorAll('textarea')].some((t) => t.value.includes('Hello team')), null, { timeout: 15000 });
	await Bp.waitForTimeout(1500); // the form reloads its record once more after it first fills
	await body.fill('# Hello team\n\nEdited by bob in the browser.');
	await Bp.getByRole('button', { name: 'Save' }).first().click();
	await Bp.waitForURL(new RegExp(`/wiki/p/${home}$`), { timeout: 15000 });
	await Bp.getByText('Edited by bob in the browser.').waitFor({ timeout: 15000 });
	check('editing a page in the form saves and shows it', true);
	await Bp.goto(`${BASE}/web/wiki/p/${home}/history`);
	await Bp.getByText('bob').first().waitFor({ timeout: 15000 });
	check('the history lists the versions with who changed them', (await Bp.locator('tbody tr').count()) >= 2);
	await Bp.getByRole('button', { name: 'Put back' }).last().click();
	await Bp.getByRole('button', { name: 'Confirm', exact: true }).click();
	await Bp.waitForTimeout(800);
	const now = (await bob.call(W, 'get_page', { id: home })).body;
	check('a version is put back from the history page', now.includes('Read [[Rules'), now);
	await Bp.goto(`${BASE}/web/wiki/search`);
	await Bp.getByPlaceholder(/search/i).first().fill('be kind');
	await Bp.getByText('Rules ' + run).first().waitFor({ timeout: 15000 });
	check('search finds a page by its text', true);
	await Cp.goto(`${BASE}/web/wiki/s/${sp}`);
	await Cp.waitForTimeout(1500);
	check('a stranger gets an error and not the pages', !(await text(Cp)).includes('Rules ' + run));
	await Bp.goto(`${BASE}/web/wiki/s/${sp}/people`);
	await Bp.getByText('owner').first().waitFor({ timeout: 15000 });
	check('the people page lists the people and roles', (await text(Bp)).includes('editor'));

	// ===================== news
	await owner.call(N, 'add_authors', { users: [people.bob] });
	await Bp.goto(`${BASE}/web/news/new`);
	await Bp.getByRole('textbox', { name: /Title/ }).waitFor({ timeout: 15000 });
	await Bp.getByRole('textbox', { name: /Title/ }).fill('Big news ' + run);
	await Bp.getByRole('textbox', { name: /In one line/ }).fill('We are growing');
	await Bp.getByRole('textbox', { name: /Text/ }).fill('We **hired** three people.');
	await Bp.getByRole('button', { name: 'Save' }).first().click();
	await Bp.waitForURL(/\/news\/p\//, { timeout: 15000 });
	const post = Bp.url().split('/').pop();
	await Bp.getByText('We hired three people.').or(Bp.getByText('hired')).first().waitFor({ timeout: 15000 });
	check('an author writes a post in the form and sees it as a draft', (await text(Bp)).includes('draft'));
	await Cp.goto(`${BASE}/web/news`);
	await Cp.waitForTimeout(1200);
	check('a draft is not in the feed of others', !(await text(Cp)).includes('Big news ' + run));
	await bob.call(N, 'publish_post', { id: post });
	await Cp.goto(`${BASE}/web/news`);
	await Cp.getByText('Big news ' + run).waitFor({ timeout: 15000 });
	check('a published post is in the feed, tagged new', (await text(Cp)).includes('New'));
	await Cp.getByText('Big news ' + run).click();
	await Cp.getByText('We are growing').first().waitFor({ timeout: 15000 });
	await Cp.getByRole('button', { name: 'Like' }).click();
	await Cp.waitForTimeout(800);
	check('a reader can like a post', (await cy.call(N, 'get_post', { id: post })).likes === 1);
	await shot(Cp, 'news-post');
	await Bp.goto(`${BASE}/web/news/mine`);
	await Bp.getByText('Big news ' + run).waitFor({ timeout: 15000 });
	check('"my posts" lists it', true);
	await Cp.goto(`${BASE}/web/news/authors`);
	await Cp.getByText('bob').first().waitFor({ timeout: 15000 });
	check('the authors page lists the authors', true);
	await owner.call(N, 'delete_post', { id: post });

	// ===================== booking
	const svc = (await owner.call(B_, 'create_service', { title: 'Office hours ' + run, minutes: 30, weekdays: ['1', '2', '3', '4', '5', '6', '7'], from_time: '08:00', to_time: '18:00', notice_hours: 0, days_ahead: 5 })).id;
	await Bp.goto(`${BASE}/web/booking`);
	await Bp.getByText('Office hours ' + run).waitFor({ timeout: 15000 });
	check('booking lists what colleagues offer', (await text(Bp)).includes('30 min'));
	await Bp.getByText('Office hours ' + run).click();
	await Bp.waitForURL(new RegExp(`/booking/s/${svc}$`));
	await Bp.getByRole('button', { name: 'Book' }).first().waitFor({ timeout: 15000 });
	check('a service lists free times with a Book button', (await Bp.getByRole('button', { name: 'Book' }).count()) > 5);
	await shot(Bp, 'booking-slots');
	await Bp.getByRole('button', { name: 'Book' }).first().click();
	await Bp.getByRole('button', { name: 'Confirm', exact: true }).click();
	await Bp.waitForTimeout(1200);
	await Bp.goto(`${BASE}/web/booking/mine`);
	await Bp.getByText('Office hours ' + run).first().waitFor({ timeout: 15000 });
	check('booking a time puts it in "my bookings"', (await text(Bp)).includes('guest'));
	await A.goto(`${BASE}/web/booking/mine`);
	await A.getByText('Office hours ' + run).first().waitFor({ timeout: 15000 });
	check('the host sees it too', (await text(A)).includes('host'));
	await A.getByRole('button', { name: 'Cancel' }).first().click();
	await A.getByRole('button', { name: 'Confirm', exact: true }).click();
	await A.waitForTimeout(1000);
	check('the host cancels it', (await owner.call(B_, 'my_bookings')).length === 0);
	await A.goto(`${BASE}/web/booking/service/${svc}`);
	await A.getByRole('textbox', { name: /Title/ }).waitFor({ timeout: 15000 });
	await A.waitForFunction(() => [...document.querySelectorAll('input')].some((i) => i.value.includes('Office hours')), null, { timeout: 15000 }).catch(() => {});
	check('the host edits a service on its own page', (await A.getByRole('textbox', { name: /Title/ }).inputValue()).includes('Office hours'));
	// ===================== sites
	const site = (await owner.call('atrium_sites', 'create_site', { name: 'Sales ' + run, description: 'The sales team', home: '# Welcome sales', open: true, show_news: true })).id;
	await owner.call('atrium_sites', 'add_link', { site, title: 'Handbook ' + run, url: '/wiki', about: 'How we work' });
	const note = (await owner.call(N, 'create_post', { title: 'Sales kickoff ' + run, summary: 'Next week', body: 'Join us', publish: true })).id;
	await Cp.goto(`${BASE}/web/sites`);
	await Cp.getByText('Sales ' + run).waitFor({ timeout: 15000 });
	check('sites lists the open site for everyone', true);
	await Cp.getByText('Sales ' + run).click();
	await Cp.getByText('Welcome sales').waitFor({ timeout: 15000 });
	const quick = Cp.getByRole('link', { name: 'Handbook ' + run });
	check('the start page shows its quick link, which stays in the app', (await quick.count()) === 1 && (await quick.getAttribute('href')) === '/web/wiki' && (await quick.getAttribute('target')) === null);
	check('and the latest news', (await Cp.getByRole('link', { name: 'Sales kickoff ' + run }).count()) === 1);
	await shot(Cp, 'sites-page');
	await Cp.getByRole('link', { name: 'Handbook ' + run }).click();
	await Cp.waitForURL(/\/web\/wiki$/, { timeout: 10000 }).then(() => check('a quick link opens the wiki in the same tab', true), () => check('a quick link opens the wiki in the same tab', false, Cp.url()));
	await A.goto(`${BASE}/web/sites/s/${site}/links`);
	await A.getByText('Handbook ' + run).first().waitFor({ timeout: 15000 });
	check('the links page lists the link and offers to add one', (await A.getByRole('button', { name: 'Add link' }).count()) > 0);
	await owner.call(N, 'delete_post', { id: note });
	await owner.call('atrium_sites', 'delete_site', { site });

	// ===================== shared inbox
	const csrfOf = async (who) => (await who.context.cookies()).find((c) => c.name === 'aether_csrf')?.value;
	const hdr = { 'X-CSRF-Token': await csrfOf(owner), Origin: BASE };
	const secret = (await (await owner.context.request.post(`${BASE}/api/ui/mail/secret`, { data: {}, headers: hdr })).json()).secret;
	await owner.context.request.put(`${BASE}/api/settings/mail.inbound.domain`, { data: { value: 'mail.acme.test' }, headers: hdr });
	const boxes = await owner.call('atrium_mail', 'my_boxes');
	const box = boxes.find((b) => b.tag === '')?.id ?? (await owner.call('atrium_mail', 'create_box', { name: 'Support ' + run, tag: '' })).id;
	const mailSubject = 'Invoice question ' + run;
	const posted = await owner.context.request.post(`${BASE}/api/mail/atr_org/generic`, { data: { from: { name: 'Zed', address: 'zed@client.test' }, to: ['inbox@mail.acme.test'], subject: mailSubject, message_id: `<ui.${run}@client.test>`, text: 'Where is my invoice?' }, headers: { 'X-Aether-Mail-Token': secret }, timeout: 60000 });
	check('mail to the inbox address is accepted', posted.ok(), await posted.text());
	await A.goto(`${BASE}/web/mail`);
	await A.getByText(mailSubject).waitFor({ timeout: 30000 });
	check('the inbox lists the conversation', true);
	await A.getByText(mailSubject).click();
	await A.getByText('Where is my invoice?').waitFor({ timeout: 15000 });
	check('the conversation shows what the customer wrote', true);
	await shot(A, 'mail-thread');
	await A.getByRole('button', { name: 'Take it' }).click();
	await A.waitForTimeout(1000);
	const mine = (await owner.call('atrium_mail', 'inbox', { mine: true })).find((r) => r.subject.includes(run));
	check('the "Take it" button gives it to the person', !!mine);
	if (mine) await owner.call('atrium_mail', 'close_thread', { id: mine.id });
	await A.goto(`${BASE}/web/mail/boxes`);
	await A.getByText('Support').first().waitFor({ timeout: 15000 });
	check('the boxes page lists the box', true);

	// ===================== meetings
	await owner.context.request.patch(`${BASE}/api/ui/bridges/jitsi`, { data: { enabled: true }, headers: hdr });
	for (const [k, v] of [['base_url', 'https://meet.acme.test'], ['app_id', ''], ['signing_key', '']]) await owner.context.request.put(`${BASE}/api/settings/bridge.jitsi.${k}`, { data: { value: v }, headers: hdr });
	const meeting = (await owner.call('atrium_meet', 'create_meeting', { title: 'Standup ' + run, starts_at: '2099-01-05T10:00' })).id;
	await A.goto(`${BASE}/web/meet`);
	await A.getByText('Standup ' + run).waitFor({ timeout: 15000 });
	check('the meetings list shows a planned meeting', true);
	await A.goto(`${BASE}/web/meet/m/${meeting}`);
	await A.getByRole('button', { name: 'Join' }).waitFor({ timeout: 15000 });
	await owner.context.route('https://meet.acme.test/**', (route) => route.fulfill({ status: 200, contentType: 'text/html', body: '<p>room</p>' }));
	const popup = A.waitForEvent('popup', { timeout: 15000 }).catch(() => null);
	await A.getByRole('button', { name: 'Join' }).click();
	const opened = await popup;
	check('Join opens the meeting room in a new tab', !!opened && opened.url().startsWith('https://meet.acme.test/aether-' + meeting), opened?.url());
	await opened?.close().catch(() => {});
	await owner.call('atrium_meet', 'cancel_meeting', { id: meeting });

	// ===================== personal mailbox (a small JMAP server plays the mail server)
	const { spawn } = await import('node:child_process');
	const jmap = spawn('python3', ['-c', `import fake_jmap,time\nS=fake_jmap.STORE\nS.add("ann","Lunch plan ${run}","Shall we meet at noon?")\nfake_jmap.start(19081)\ntime.sleep(240)`], { cwd: here, stdio: 'ignore' });
	await new Promise((r) => setTimeout(r, 1500));
	try {
		await owner.context.request.patch(`${BASE}/api/ui/bridges/jmap`, { data: { enabled: true }, headers: hdr });
		await owner.context.request.put(`${BASE}/api/settings/bridge.jmap.session_url`, { data: { value: 'http://127.0.0.1:19081/.well-known/jmap' }, headers: hdr });
		await owner.context.request.delete(`${BASE}/api/ui/bridge-credentials/jmap`, { headers: hdr });
		await A.goto(`${BASE}/web/mailbox/connect`);
		await A.getByLabel('User name').first().fill('ann');
		await A.getByLabel('Password').first().fill('pw-ann');
		await A.getByRole('button', { name: 'Connect', exact: true }).first().click();
		await A.getByText('Connected as').waitFor({ timeout: 15000 });
		check('a person connects their mailbox on the page', true);
		await shot(A, 'mailbox-connect');
		await A.goto(`${BASE}/web/mailbox`);
		await A.getByText('Lunch plan ' + run).waitFor({ timeout: 20000 });
		check('the mailbox lists their mail', true);
		await A.getByText('Lunch plan ' + run).click();
		await A.getByText('Shall we meet at noon?').waitFor({ timeout: 15000 });
		check('a message opens with its text', true);
		await shot(A, 'mailbox-message');
		await owner.context.request.delete(`${BASE}/api/ui/bridge-credentials/jmap`, { headers: hdr });
	} finally {
		jmap.kill();
	}

	await owner.call(B_, 'delete_service', { id: svc });
	await owner.call(W, 'delete_space', { space: sp });
	check('no page errors in the browser', errors.length === 0, errors.join(' | ').slice(0, 300));
} catch (error) {
	check('the run finished', false, String(error?.stack ?? error));
} finally {
	await browser.close();
}
console.log(`\n${failures.length} failure(s)`);
process.exit(failures.length ? 1 : 0);
