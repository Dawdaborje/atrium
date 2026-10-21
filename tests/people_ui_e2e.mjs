// The Atrium directory in a real browser. See README.md.
//   node people_ui_e2e.mjs        (SHOTS=dir keeps screenshots; CHROMIUM=/usr/bin/chromium picks the browser)
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
	const context = await browser.newContext({ viewport: { width: 1200, height: 800 } });
	await context.request.get(BASE + '/api/auth/methods');
	const csrf = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value;
	const login = await context.request.post(BASE + '/api/auth/login', { data: { username, password: 'Test-pass-123' }, headers: { 'X-CSRF-Token': csrf, Origin: BASE } });
	if (!login.ok()) throw new Error(`login ${username}: ${login.status()}`);
	return context;
}

try {
	const run = Math.random().toString(36).slice(2, 6);
	const bobCtx = await person('bob');
	const ownerCtx = await person('owner');
	const B = await bobCtx.newPage();
	const A = await ownerCtx.newPage();
	const errors = [];
	for (const p of [A, B]) p.on('pageerror', (e) => errors.push(String(e)));

	B.on('request', (r) => { if (process.env.DEBUG_UI && r.url().includes('save_profile')) console.log('POST', r.postData()); });
	// --- my own profile
	await B.goto(`${BASE}/web/people/me`);
	await B.getByLabel('Job title').waitFor({ timeout: 15000 });
	await B.waitForFunction(() => [...document.querySelectorAll('input')].some((i) => i.value === 'bob'), null, { timeout: 15000 });
	await B.waitForTimeout(1000);
	await B.getByLabel('Job title').fill(`Designer ${run}`);
	await B.getByLabel('Location').fill(`Oslo ${run}`);
	await B.waitForTimeout(500);
	await B.getByRole('button', { name: 'Save' }).click();
	await B.getByText('Saved').first().waitFor({ timeout: 10000 }).then(
		() => check('a person saves their own profile', true),
		() => check('a person saves their own profile', false, 'no confirmation')
	);

	// --- the directory
	await A.goto(`${BASE}/web/people`);
	const search = A.getByPlaceholder(/search/i).first();
	await search.waitFor({ timeout: 15000 });
	await search.fill(`Oslo ${run}`);
	await search.press('Enter');
	await A.getByText(`Designer ${run}`).first().waitFor({ timeout: 15000 }).then(
		() => check('searching finds the profile by what it says', true),
		() => check('searching finds the profile by what it says', false, 'not listed')
	);
	await A.waitForTimeout(500);
	check('and narrows the list to it', (await A.locator('table tbody tr').count()) === 1, String(await A.locator('table tbody tr').count()));
	await shot(A, 'people');

	// --- a profile
	await A.locator('table tbody tr').first().getByText('bob', { exact: true }).click();
	await A.waitForURL(/\/people\/[^/]+$/, { timeout: 15000 });
	await A.getByLabel('Location').waitFor({ timeout: 15000 });
	await A.waitForFunction((v) => [...document.querySelectorAll('input')].some((i) => i.value === v), `Oslo ${run}`, { timeout: 15000 }).then(
		() => check('opening a person shows their profile', true),
		() => check('opening a person shows their profile', false, 'values not shown')
	);
	await shot(A, 'profile');
	const editButton = A.getByRole('link', { name: /Edit this profile/ }).or(A.getByRole('button', { name: /Edit this profile/ })).first();
	check('the profile links to its editor', (await editButton.count()) === 1);
	await editButton.click();
	await A.waitForURL(/\/edit$/, { timeout: 15000 });
	await A.getByLabel('Job title').waitFor({ timeout: 15000 });
	await A.waitForFunction(() => [...document.querySelectorAll('input')].some((i) => i.value === 'bob'), null, { timeout: 15000 });
	check('an administrator opens the editor of another person', (await A.getByLabel('Job title').inputValue()) === `Designer ${run}`, await A.getByLabel('Job title').inputValue());

	// --- the chart
	await A.goto(`${BASE}/web/people/chart`);
	await A.locator('[data-part="org-node"]').first().waitFor({ timeout: 15000 });
	check('the org chart lists people', (await A.locator('[data-part="org-node"]').count()) >= 3);
	await shot(A, 'chart');
	check('no page errors in the browser', errors.length === 0, errors.join(' | ').slice(0, 300));
} catch (error) {
	check('the run finished', false, String(error?.stack ?? error));
} finally {
	await browser.close();
}
console.log(`\n${failures.length} failure(s)`);
process.exit(failures.length ? 1 : 0);
