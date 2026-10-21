// The Atrium drive in a real browser, with two people. See README.md.
//   node drive_ui_e2e.mjs        (SHOTS=dir keeps screenshots; CHROMIUM=/usr/bin/chromium picks the browser)
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';

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
	return { context };
}
const shot = async (page, name) => SHOTS && page.screenshot({ path: path.join(SHOTS, name + '.png') });
const row = (page, name) => page.locator('[data-part=file-row]', { hasText: name }).first();
const menu = async (page, name, action) => {
	await row(page, name).getByRole('button', { name: /^Actions for/ }).click();
	await page.getByRole('menuitem', { name: action, exact: true }).click();
};

try {
	const owner = await person('owner');
	const bob = await person('bob');
	const run = Math.random().toString(36).slice(2, 6);
	const A = await owner.context.newPage();
	const B = await bob.context.newPage();
	const errors = [];
	for (const p of [A, B]) {
		p.on('pageerror', (e) => errors.push(String(e)));
		p.on('dialog', (d) => d.accept());
	}
	const file1 = path.join(os.tmpdir(), `report-${run}.txt`);
	fs.writeFileSync(file1, 'first draft ' + run);

	await A.goto(`${BASE}/web/drive`);
	await A.locator('[data-part=files]').waitFor({ timeout: 15000 });
	await A.locator('[data-pane=nav] a[data-rail-item]').first().waitFor({ timeout: 10000 });
	check('the drive shows its places on the left', (await A.locator('[data-pane=nav] a[data-rail-item]').count()) === 5, String(await A.locator('[data-pane=nav] a[data-rail-item]').allInnerTexts()));
	check('"My files" is marked in the list of places when none was chosen', (await A.locator('[data-pane=nav] a[data-rail-item][data-active=true]').innerText()).includes('My files'));
	await A.locator('[data-part=usage]').waitFor({ timeout: 10000 });
	await A.locator('[data-pane=nav] a[data-rail-item]').first().waitFor({ timeout: 10000 });
	check('the administrator sees a link to the settings', (await A.locator('[data-part=files-settings]').count()) === 1);
	check('the header says how much is stored', (await A.locator('[data-part=usage]').innerText()).includes('used'));
	check('"My files" is the first place shown', (await A.locator('[data-part=breadcrumbs]').innerText()).includes('My files'));
	await shot(A, 'drive-empty');

	// --- upload with the button
	await A.locator('[data-part=files] input[type=file]').setInputFiles(file1);
	await row(A, `report-${run}.txt`).waitFor({ timeout: 15000 });
	check('an uploaded file appears in the list', true);

	// --- upload by dropping
	await A.evaluate((n) => {
		const dt = new DataTransfer();
		dt.items.add(new File(['dropped text'], n, { type: 'text/plain' }));
		const el = document.querySelector('[data-part=files]');
		el.dispatchEvent(new DragEvent('dragover', { dataTransfer: dt, bubbles: true, cancelable: true }));
		el.dispatchEvent(new DragEvent('drop', { dataTransfer: dt, bubbles: true, cancelable: true }));
	}, `dropped-${run}.txt`);
	await row(A, `dropped-${run}.txt`).waitFor({ timeout: 15000 });
	check('a file dropped on the list is uploaded', true);

	// --- folders
	await A.getByRole('button', { name: 'New folder' }).click();
	await A.getByRole('textbox').fill('Projects ' + run);
	await A.getByRole('button', { name: 'Create' }).click();
	await row(A, 'Projects ' + run).waitFor({ timeout: 10000 });
	check('a new folder appears', true);
	await row(A, 'Projects ' + run).getByRole('button', { name: 'Projects ' + run, exact: true }).click();
	await A.waitForURL(/\/drive\/[a-z0-9]+$/);
	await A.getByText('This folder is empty').waitFor({ timeout: 10000 });
	check('opening a folder shows it empty, and the path in the breadcrumbs', (await A.locator('[data-part=breadcrumbs]').innerText()).includes('Projects ' + run));
	await A.locator('[data-part=files] input[type=file]').setInputFiles(file1);
	await row(A, `report-${run}.txt`).waitFor({ timeout: 15000 });
	check('files go into the open folder', true);
	await shot(A, 'drive-folder');
	await A.getByRole('link', { name: 'Projects ' + run }).waitFor();

	// --- preview
	await row(A, `report-${run}.txt`).getByRole('button', { name: `report-${run}.txt`, exact: true }).click();
	await A.waitForFunction((t) => document.body.innerText.includes(t), 'first draft ' + run, { timeout: 10000 }).then(
		() => check('opening a file shows it in the viewer panel', true),
		() => check('opening a file shows it in the viewer panel', false, 'content not shown')
	);
	await A.keyboard.press('Escape');

	// --- rename and share
	await menu(A, `report-${run}.txt`, 'Rename');
	await A.getByRole('textbox').fill(`final-${run}.txt`);
	await A.getByRole('button', { name: 'Save' }).click();
	await row(A, `final-${run}.txt`).waitFor({ timeout: 10000 });
	check('renaming changes the name in the list', true);
	await menu(A, `final-${run}.txt`, 'Share…');
	await A.getByLabel('Person').selectOption({ label: 'bob' });
	await A.getByRole('button', { name: 'Share', exact: true }).click();
	await A.locator('[data-part=shares] li', { hasText: 'bob' }).waitFor({ timeout: 10000 });
	check('sharing with a person lists them with their access', true);
	await shot(A, 'drive-share');
	await A.getByRole('button', { name: 'Done' }).click();
	await row(A, `final-${run}.txt`).locator('svg[aria-label=Shared]').waitFor({ timeout: 8000 });
	check('a shared file shows the shared mark', true);

	// --- the other person
	await B.goto(`${BASE}/web/drive?view=shared`);
	await B.waitForTimeout(3000);
	await shot(B, 'b-shared');
	if (process.env.DEBUG_UI) console.log((await B.locator('body').innerText()).slice(0, 500));
	await row(B, `final-${run}.txt`).waitFor({ timeout: 15000 });
	check('the person sees it under Shared with me', true);
	await row(B, `final-${run}.txt`).getByRole('button', { name: /^Actions for/ }).click();
	const offered = await B.getByRole('menuitem').allInnerTexts();
	check('with view access they may open and download but not change it', offered.includes('Open') && offered.includes('Download') && !offered.includes('Rename') && !offered.includes('Move to trash'), offered.join(','));
	await B.keyboard.press('Escape');
	await row(B, `final-${run}.txt`).getByRole('button', { name: new RegExp(`^final-${run}\\.txt`) }).click();
	await B.waitForFunction((t) => document.body.innerText.includes(t), 'first draft ' + run, { timeout: 10000 }).then(
		() => check('they can read it in the viewer', true),
		() => check('they can read it in the viewer', false, 'not shown')
	);
	await shot(B, 'drive-shared-preview');
	await B.keyboard.press('Escape');
	await B.goto(`${BASE}/web/drive`);
	await B.locator('[data-part=files]').waitFor({ timeout: 15000 });
	check("their own files do not include it", (await row(B, `final-${run}.txt`).count()) === 0);

	// --- versions
	await A.goto(`${BASE}/web/drive/${(await A.url()).split('/drive/')[1]}`);
	await row(A, `final-${run}.txt`).waitFor({ timeout: 15000 });
	await menu(A, `final-${run}.txt`, 'Versions');
	await A.locator('[data-part=versions] li').first().waitFor({ timeout: 10000 });
	const second = path.join(os.tmpdir(), `second-${run}.txt`);
	fs.writeFileSync(second, 'second draft ' + run);
	await A.locator('input[type=file]').last().setInputFiles(second);
	await A.waitForFunction(() => document.querySelectorAll('[data-part=versions] li').length === 2, null, { timeout: 15000 }).then(
		() => check('uploading a new version adds it to the list', true),
		() => check('uploading a new version adds it to the list', false, 'still one')
	);
	check('the newest is marked current', (await A.locator('[data-part=versions] li').first().innerText()).includes('current'));
	await shot(A, 'drive-versions');
	await A.locator('[data-part=versions] li').nth(1).getByRole('button', { name: 'Make current' }).click();
	await A.waitForFunction(() => document.querySelectorAll('[data-part=versions] li').length === 3, null, { timeout: 15000 }).then(
		() => check('making an old version current adds a new version, losing nothing', true),
		() => check('making an old version current adds a new version, losing nothing', false, 'not three')
	);
	await A.keyboard.press('Escape');

	// --- favourites
	const folderUrl = A.url();
	await menu(A, `final-${run}.txt`, 'Add to favourites');
	await row(A, `final-${run}.txt`).getByLabel('Favourite').waitFor({ timeout: 10000 }).then(
		() => check('a starred file shows a star', true),
		() => check('a starred file shows a star', false, 'no star')
	);
	await A.getByRole('link', { name: 'Favourites' }).click();
	await row(A, `final-${run}.txt`).waitFor({ timeout: 10000 });
	check('the Favourites place lists it', true);
	await A.waitForTimeout(800);
	await menu(A, `final-${run}.txt`, 'Remove from favourites');
	await A.waitForFunction((n) => ![...document.querySelectorAll('[data-part=file-row]')].some((r) => r.textContent.includes(n)), `final-${run}.txt`, { timeout: 10000 });
	check('unstarring takes it off', true);
	await A.goto(folderUrl);
	await row(A, `final-${run}.txt`).waitFor({ timeout: 10000 });
	await A.waitForTimeout(800);

	// --- comments
	await menu(A, `final-${run}.txt`, 'Comments');
	await A.getByLabel('Write a comment').fill(`Needs a second look ${run}`);
	await A.getByRole('button', { name: 'Comment', exact: true }).click();
	await A.locator('[data-part=comments]', { hasText: `Needs a second look ${run}` }).waitFor({ timeout: 10000 }).then(
		() => check('a comment is added and shown', true),
		() => check('a comment is added and shown', false, 'not shown')
	);
	await A.getByRole('button', { name: 'Delete this comment' }).click();
	await A.locator('[data-part=comments]', { hasText: 'No comments yet.' }).waitFor({ timeout: 10000 }).then(
		() => check('its author deletes it', true),
		() => check('its author deletes it', false, 'still there')
	);
	await A.getByRole('button', { name: 'Close' }).first().click();

	// --- a link
	await menu(A, `final-${run}.txt`, 'Share…');
	await A.getByRole('button', { name: 'Create a link' }).click();
	const url = await A.locator('[data-part=link-url]').first().innerText({ timeout: 10000 });
	check('a link is made and shown', url.includes('/drive/link/'), url);
	await A.getByRole('button', { name: 'Done' }).click();
	await B.goto(url);
	await B.locator('[data-part=files]').waitFor({ timeout: 15000 });
	await row(B, `final-${run}.txt`).waitFor({ timeout: 10000 }).then(
		() => check('someone else opens the link and sees the file', true),
		() => check('someone else opens the link and sees the file', false, 'not listed')
	);
	check('...with no way to upload or change it', (await B.locator('[data-part=files] input[type=file]').count()) === 0);
	await row(B, `final-${run}.txt`).getByRole('button', { name: 'Actions for' }).click().catch(() => {});
	await B.keyboard.press('Escape');
	await shot(B, 'drive-link');

	// --- trash
	await menu(A, `final-${run}.txt`, 'Move to trash');
	await A.waitForFunction((n) => ![...document.querySelectorAll('[data-part=file-row]')].some((r) => r.textContent.includes(n)), `final-${run}.txt`, { timeout: 10000 });
	check('trashing removes it from the folder', true);
	await A.goto(`${BASE}/web/drive?view=trash`);
	await row(A, `final-${run}.txt`).waitFor({ timeout: 15000 });
	check('it is in the trash', true);
	await menu(A, `final-${run}.txt`, 'Restore');
	await A.waitForFunction(() => document.querySelector('[data-part=files-empty]')?.textContent.includes('trash is empty'), null, { timeout: 10000 });
	check('restoring empties the trash', true);

	// --- search
	await A.goto(`${BASE}/web/drive`);
	await A.getByLabel('Search files').fill('dropped');
	await A.getByLabel('Search files').press('Enter');
	await A.locator('[data-part=breadcrumbs]', { hasText: 'Search results' }).waitFor({ timeout: 10000 }).then(
		() => check('searching shows the results', true),
		() => check('searching shows the results', false, 'still the folder')
	);
	await A.waitForFunction(() => { const r = [...document.querySelectorAll('[data-part=file-row]')]; return r.length > 0 && r.every((x) => x.textContent.includes('dropped')); }, null, { timeout: 10000 }).then(
		() => check('...and finds files by name', true),
		async () => check('...and finds files by name', false, (await A.locator('[data-part=file-row]').allInnerTexts()).join(' | ').slice(0, 300))
	);

	// --- a narrow screen still works
	await A.setViewportSize({ width: 420, height: 800 });
	await A.goto(`${BASE}/web/drive`);
	await A.locator('[data-part=files]').waitFor({ timeout: 15000 });
	await A.locator('[data-part=file-row]').first().waitFor({ timeout: 10000 }).catch(() => {});
	check('on a narrow screen the list is shown', (await A.locator('[data-part=file-row]').count()) > 0);
	check('no page errors in the browser', errors.length === 0, errors.join(' | ').slice(0, 300));
} catch (error) {
	check('the run finished', false, String(error?.stack ?? error));
} finally {
	await browser.close();
}
console.log(`\n${failures.length} failure(s)`);
process.exit(failures.length ? 1 : 0);
