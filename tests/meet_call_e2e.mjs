// The built-in video meeting in real browsers: a host and an invited guest in one room with a fake camera and microphone,
// and a person who was not invited, who is refused. See README.md.
//   node meet_call_e2e.mjs        (SHOTS=dir keeps screenshots; CHROMIUM=/usr/bin/chromium picks the browser)
// Needs [sfu] in aether.toml (setup.sh writes it) and the built web app.
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
const browser = await chromium.launch({
	executablePath: process.env.CHROMIUM ?? '/usr/bin/chromium',
	headless: true,
	args: ['--no-sandbox', '--use-fake-ui-for-media-stream', '--use-fake-device-for-media-stream=device-count=2', '--autoplay-policy=no-user-gesture-required', '--auto-select-desktop-capture-source=Entire screen']
});
const shot = async (page, name) => SHOTS && page.screenshot({ path: path.join(SHOTS, name + '.png') });

async function person(username) {
	const context = await browser.newContext({ viewport: { width: 1100, height: 750 } });
	await context.request.get(BASE + '/api/auth/methods');
	const csrf = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value;
	const login = await context.request.post(BASE + '/api/auth/login', { data: { username, password: 'Test-pass-123' }, headers: { 'X-CSRF-Token': csrf, Origin: BASE } });
	if (!login.ok()) throw new Error(`login ${username}: ${login.status()}`);
	const call = async (plugin, fn, data = {}) => {
		// the token is renewed at login, so read it again each time
		const token = (await context.cookies()).find((c) => c.name === 'aether_csrf')?.value;
		const hdr = { 'X-CSRF-Token': token, Origin: BASE };
		const r = await context.request.post(`${BASE}/api/plugins/${plugin}/${fn}`, { data, headers: hdr });
		return { ok: r.ok(), status: r.status(), body: await r.json().catch(() => null) };
	};
	return { context, call };
}

try {
	const run = Math.random().toString(36).slice(2, 6);
	const owner = await person('owner');
	const bob = await person('bob');
	const cy = await person('cy');
	const names = Object.fromEntries((await owner.call('atrium_chat', 'list_people')).body.map((p) => [p.name, p.id]));

	// --- the meeting
	const made = await owner.call('atrium_meet', 'create_meeting', { title: 'Call ' + run, users: [names.bob] });
	check('the host makes a meeting, held on the built-in server by default', made.ok && (await owner.call('atrium_meet', 'get_meeting', { id: made.body.id })).body.provider === 'builtin', made);
	const id = made.body.id;
	const joined = await bob.call('atrium_meet', 'join_meeting', { id });
	check('joining it sends a guest to the room page, not to another site', joined.ok && joined.body.open_page === `/meet/room/${id}` && !joined.body.open_url, joined);
	const odd = await owner.call('atrium_meet', 'create_meeting', { title: 'x', provider: 'zoom' });
	check('an unknown place to hold a meeting is refused', !odd.ok, odd);
	const now = await owner.call('atrium_meet', 'start_now', { title: 'Now ' + run });
	check('a meeting started now also goes to its room', now.ok && now.body.open_page === `/meet/room/${now.body.id}`, now);

	// --- the room, in browsers
	const room = `${BASE}/web/meet/room/${id}`;
	const open = async (who) => {
		const page = await who.context.newPage();
		await page.addInitScript(() => { window.__pcs = []; const O = window.RTCPeerConnection; window.RTCPeerConnection = function (...a) { const p = new O(...a); window.__pcs.push(p); return p; }; window.RTCPeerConnection.prototype = O.prototype; });
		page.on('pageerror', (e) => console.log('pageerror', e.message));
		await page.goto(room);
		return page;
	};
	const A = await open(owner);
	await A.getByRole('button', { name: 'Join now' }).waitFor({ timeout: 30000 });
	check('the host first sees the check-your-devices step', await A.getByText('Ready to join?').isVisible());
	await shot(A, 'lobby');
	await A.getByRole('button', { name: 'Join now' }).click();
	const tilesOf = (p) => p.locator('[data-part=call-tile]').count();
	check('the host is in the room, alone', await until(async () => (await tilesOf(A)) === 1));

	const B = await open(bob);
	await B.getByRole('button', { name: 'Join now' }).waitFor({ timeout: 30000 });
	const camSelect = (p) => p.locator('select[aria-label=Camera]');
	const options = async (p) => p.locator('select[aria-label=Camera] option').evaluateAll((o) => o.map((x) => x.value));
	check('the check-your-devices step lists the cameras and microphones', await until(async () => (await options(B)).length >= 2 && (await B.locator('select[aria-label=Microphone] option').count()) >= 1), await options(B));
	const cams = await options(B);
	const trackDevice = (p) => p.evaluate(() => window.__pcs[0].getSenders().find((s) => s.track?.kind === 'video')?.track.getSettings().deviceId);
	await camSelect(B).selectOption(cams[1]);
	check('choosing another camera there shows it as the chosen one', await until(async () => (await camSelect(B).inputValue()) === cams[1]));
	check('and the preview still shows a picture', await until(async () => (await B.evaluate(() => { const v = document.querySelector('[data-part=call-lobby] video'); return !!v && v.videoWidth > 0; }))));
	// (Chromium's second fake camera plays on the page but never produces frames for a call, so the call uses the first.)
	await camSelect(B).selectOption(cams[0]);
	await B.getByRole('button', { name: 'Join now' }).click();
	check('the guest joins, and both see two tiles', await until(async () => (await tilesOf(A)) === 2 && (await tilesOf(B)) === 2), [await tilesOf(A), await tilesOf(B)]);
	const picture = (p) => p.evaluate(() => [...document.querySelectorAll('[data-part=call-tile] video')].filter((v) => v.videoWidth > 0 && !v.paused).length);
	if (process.env.DEBUG_PC) for (const [n, p] of [['A', A], ['B', B]]) console.log(n, JSON.stringify(await p.evaluate(() => window.__pcs.map((c) => ({ conn: c.connectionState, ice: c.iceConnectionState, sig: c.signalingState, recv: c.getReceivers().filter((r) => r.track).map((r) => r.track.kind + ':' + r.track.readyState + ':' + r.track.muted), tx: c.getTransceivers().map((t) => t.mid + t.currentDirection) })))));
	if (process.env.DEBUG_STATS) for (const [n, p] of [['A', A], ['B', B]]) console.log(n, JSON.stringify(await p.evaluate(async () => {
		const out = [];
		for (const [i, c] of window.__pcs.entries()) for (const r of (await c.getStats()).values()) {
			if ((r.type === 'outbound-rtp' || r.type === 'inbound-rtp') && r.kind === 'video') out.push({ pc: i, type: r.type, frames: r.framesEncoded ?? r.framesDecoded, w: r.frameWidth, pli: r.pliCount, keyf: r.keyFramesEncoded ?? r.keyFramesDecoded });
		}
		return out;
	})));
	check('each sees pictures of both (their own and the other)', await until(async () => (await picture(A)) >= 2 && (await picture(B)) >= 2), [await picture(A), await picture(B)]);
	await shot(B, 'room');

	// --- muting shows on the other side
	await A.getByRole('button', { name: 'Mute microphone' }).click();
	check('muting shows as a mark on the tile', await until(async () => (await B.locator('[data-part=call-tile] [aria-label=Muted]').count()) === 1));
	check('and the button offers to unmute', await until(async () => (await A.getByRole('button', { name: 'Unmute microphone' }).count()) > 0));

	// --- the host's controls
	check('the host has buttons for the others and the guest has none', (await A.getByRole('button', { name: 'Remove bob' }).count()) === 1 && (await B.locator('[data-part=call-moderation]').count()) === 0);
	check('the guest cannot moderate through the plugin either', !(await bob.call('atrium_meet', 'moderate_call', { meeting: id, action: 'end' })).ok);
	await A.getByRole('button', { name: 'Unmute microphone' }).click();
	await A.getByRole('button', { name: 'Mute bob' }).click();
	check('the host mutes the guest, who goes quiet', await until(async () => (await B.getByRole('button', { name: 'Unmute microphone' }).count()) > 0));
	check('and the guest can switch the microphone on again', await (async () => { await B.getByRole('button', { name: 'Unmute microphone' }).click(); return until(async () => (await B.getByRole('button', { name: 'Mute microphone' }).count()) > 0); })());

	// --- changing devices in the call
	check('the guest sends the camera chosen last in the lobby', await until(async () => (await trackDevice(B)) === cams[0]), [await trackDevice(B), cams]);
	await A.getByRole('button', { name: 'Choose microphone, camera and speaker' }).click();
	const before = await trackDevice(A);
	const other = (await options(A)).find((id) => id !== before);
	await camSelect(A).selectOption(other);
	check('the host switches camera in the call', await until(async () => (await trackDevice(A)) === other), [before, other, await trackDevice(A)]);
	await camSelect(A).selectOption(before);
	check('and back again', await until(async () => (await trackDevice(A)) === before), [before, await trackDevice(A)]);
	const remote = (p) => p.locator('[data-part=call-tile]:not(:has-text("(you)")) video').first().evaluate((v) => v.getVideoPlaybackQuality().totalVideoFrames);
	const f0 = await remote(B);
	check('the guest keeps receiving the host\'s picture without a new join', await until(async () => (await remote(B)) > f0 + 10) && (await tilesOf(B)) === 2, [f0, await remote(B)]);
	check('the old camera was let go (one video track on the host)', (await A.evaluate(() => window.__pcs[0].getSenders().filter((s) => s.track?.kind === 'video' && s.track.readyState === 'live').length)) === 1);
	// the microphone: another one is sent, and the guest keeps hearing audio
	const micSelect = (p) => p.locator('select[aria-label=Microphone]');
	const micDevice = (p) => p.evaluate(() => window.__pcs[0].getSenders().find((s) => s.track?.kind === 'audio')?.track.getSettings().deviceId);
	const micBefore = await micDevice(A);
	const mics = await micSelect(A).locator('option').evaluateAll((o) => o.map((x) => x.value));
	const micOther = mics.find((id) => id !== micBefore);
	await micSelect(A).selectOption(micOther);
	check('the host switches microphone in the call', await until(async () => (await micDevice(A)) === micOther), [micBefore, micOther, await micDevice(A)]);
	const audioIn = (p) => p.evaluate(async () => { let n = 0; for (const c of window.__pcs) for (const r of (await c.getStats()).values()) if (r.type === 'inbound-rtp' && r.kind === 'audio') n += r.packetsReceived; return n; });
	const a0 = await audioIn(B);
	check('and the guest keeps hearing the host', await until(async () => (await audioIn(B)) > a0 + 20), [a0, await audioIn(B)]);
	// the speaker: the guest chooses where the voices play
	await B.getByRole('button', { name: 'Choose microphone, camera and speaker' }).click();
	const speakers = await B.locator('select[aria-label=Speaker] option').evaluateAll((o) => o.map((x) => x.value));
	check('the guest can choose a speaker', speakers.length >= 2, speakers);
	await B.locator('select[aria-label=Speaker]').selectOption(speakers[1]);
	check('and the voices play there', await until(async () => B.evaluate((id) => [...document.querySelectorAll('[data-part=call-tile] video')].some((v) => v.sinkId === id), speakers[1])));
	await B.getByRole('button', { name: 'Choose microphone, camera and speaker' }).click();
	await A.getByRole('button', { name: 'Choose microphone, camera and speaker' }).click();

	// --- sharing a screen
	await A.getByRole('button', { name: 'Share your screen' }).click();
	const widths = (p) => p.evaluate(async () => { const w = []; for (const c of window.__pcs) for (const r of (await c.getStats()).values()) if (r.type === 'inbound-rtp' && r.kind === 'video' && r.frameWidth) w.push(r.frameWidth); return w; });
	const screenPictures = (p) => p.evaluate(() => [...document.querySelectorAll('[data-part=call-screen] video')].filter((v) => v.videoWidth > 0 && !v.paused).length);
	check('the guest sees the host\'s screen as a picture of its own', await until(async () => (await screenPictures(B)) === 1, 25000), await screenPictures(B));
	check('the sharer is told the screen is shared, not shown it back', await A.getByText('You are sharing your screen').count() > 0);
	check('while a screen is shared, the guest is given the small camera picture and the screen big', await until(async () => { const w = await widths(B); return w.length >= 2 && Math.min(...w) <= 320 && Math.max(...w) > 320; }, 30000), await widths(B));
	check('and the camera tiles are still there', (await tilesOf(B)) === 2 && (await picture(B)) >= 2, [await tilesOf(B), await picture(B)]);
	await shot(B, 'screen');
	await A.getByRole('button', { name: 'Stop sharing your screen' }).click();
	check('stopping the share gives the camera back at full size', await until(async () => { const w = await widths(B); return Math.max(...w) >= 960; }, 30000), await widths(B));
	check('stopping removes it for the guest', await until(async () => (await B.locator('[data-part=call-screen]').count()) === 0));
	check('the guest still sees the host\'s camera', await until(async () => (await picture(B)) >= 2), await picture(B));

	const lines = (p) => p.evaluate(() => window.__pcs[0].getTransceivers().filter((t) => t.currentDirection !== 'stopped' && !t.stopped).length);
	check('stopping a share closes its video line (audio and camera stay)', await until(async () => (await lines(A)) === 2), await lines(A));
	await A.getByRole('button', { name: 'Share your screen' }).click();
	check('a second share works too', await until(async () => (await screenPictures(B)) === 1, 25000), await screenPictures(B));
	await A.getByRole('button', { name: 'Stop sharing your screen' }).click();
	check('and stops again', await until(async () => (await B.locator('[data-part=call-screen]').count()) === 0 && (await lines(A)) === 2), await lines(A));
	check('the camera still reaches the guest', await until(async () => (await picture(B)) >= 2), await picture(B));

	// --- someone who was not invited
	const C = await open(cy);
	await C.getByRole('button', { name: 'Join now' }).click();
	check('a person who was not invited is refused, without learning more', await until(async () => (await C.getByText('Could not join').count()) > 0));
	check('and is not in the room', (await tilesOf(A)) === 2);

	// --- removing and leaving
	await A.getByRole('button', { name: 'Remove bob' }).click();
	check('the host removes the guest, who is told why', await until(async () => (await B.getByText('The call has ended').count()) > 0) && (await B.getByText('removed you').count()) > 0);
	check('and the host is alone again', await until(async () => (await tilesOf(A)) === 1));
	const B2 = await open(bob);
	await B2.getByRole('button', { name: 'Join now' }).click();
	check('the guest may come back', await until(async () => (await tilesOf(A)) === 2 && (await tilesOf(B2)) === 2));
	await B2.getByRole('button', { name: 'Leave the meeting' }).click();
	check('the guest leaves and the host sees one tile again', await until(async () => (await tilesOf(A)) === 1));
	check('the guest is told the call has ended', await B2.getByText('The call has ended').isVisible());
	// someone who comes while a screen is already shared sees it as a screen, not as a second camera
	await A.getByRole('button', { name: 'Share your screen' }).click();
	await until(async () => (await A.getByText('You are sharing your screen').count()) > 0);
	const B3 = await open(bob);
	await B3.getByRole('button', { name: 'Join now' }).click();
	check('a guest who joins during a share sees the screen as a screen', await until(async () => (await screenPictures(B3)) === 1, 25000), await screenPictures(B3));
	await A.getByRole('button', { name: 'Stop sharing your screen' }).click();
	check('the guest comes once more', await until(async () => (await tilesOf(A)) === 2));
	await A.getByRole('button', { name: 'End the meeting for everyone' }).click();
	check('the host ends the meeting for everyone', await until(async () => (await A.getByText('The call has ended').count()) > 0 && (await B3.getByText('The call has ended').count()) > 0));
	await owner.call('atrium_meet', 'cancel_meeting', { id });
} finally {
	await browser.close();
}
console.log(failures.length ? `\n${failures.length} failed` : '\nall ok');
process.exit(failures.length ? 1 : 0);
