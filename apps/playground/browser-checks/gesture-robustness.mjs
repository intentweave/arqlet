// Gestures that end badly, with real mouse and key input in a real Chrome: a release outside the window, a cancelled
// pointer, Escape, a model change in the middle of a drag, and an engine error. After each one the drawing must show the
// engine's state (no stuck preview) and the next drag must work.
// usage: node gesture-robustness.mjs [example=workflow-dag]        (starts its own server and Chrome; see launch.mjs)
//        node gesture-robustness.mjs <example> <debugPort> <url>    (uses a running pair)
import {launch} from './launch.mjs';

const [example = 'workflow-dag', portArg, urlArg] = process.argv.slice(2);
const running = portArg ? null : await launch();
const port = portArg ?? running.debugPort;
const url = urlArg ?? running.url;
const sleep = ms => new Promise(r => setTimeout(r, ms));

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const ws = new WebSocket(targets.find(t => t.type === 'page').webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
let id = 0;
const pending = new Map();
ws.onmessage = event => {
  const msg = JSON.parse(event.data);
  if (msg.id && pending.has(msg.id)) {
    const {resolve, reject} = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
  }
};
const send = (method, params = {}) => new Promise((resolve, reject) => {
  const req = ++id;
  pending.set(req, {resolve, reject});
  ws.send(JSON.stringify({id: req, method, params}));
});
const evaluate = async expression => {
  const result = await send('Runtime.evaluate', {expression, awaitPromise: true, returnByValue: true});
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? 'evaluation failed');
  return result.result.value;
};
const mouse = (type, x, y) => send('Input.dispatchMouseEvent', {type, x, y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1});
const key = (type, k) => send('Input.dispatchKeyEvent', {type, key: k, code: k, windowsVirtualKeyCode: k === 'Escape' ? 27 : 0});

await send('Runtime.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
// Keep every worker so that a test can make one fail.
await send('Page.addScriptToEvaluateOnNewDocument', {source: `(() => { window.__workers = []; const O = window.Worker; window.Worker = class extends O { constructor(...a) { super(...a); __workers.push(this); } }; window.__errors = []; addEventListener('error', e => __errors.push(e.message)); })();`});
await send('Page.navigate', {url});
await sleep(800);

const load = async name => evaluate(`(async () => {
  const select = document.getElementById('example');
  const sleep = ms => new Promise(r => setTimeout(r, ms));
  const auto = document.getElementById('auto'); if (auto) { auto.checked = false; auto.dispatchEvent(new Event('change')); }
  // The example list is fetched after the page loads and an import takes longer on a busy machine: wait for both.
  if (${JSON.stringify(name)} !== 'default') for (let i = 0; i < 360 && !select.querySelector('option[value=${name}]'); i++) await sleep(250);
  select.value = ${JSON.stringify(name)}; select.dispatchEvent(new Event('change'));
  await sleep(500);
  for (let i = 0; i < 360 && !/imported|loaded/i.test(document.getElementById('status').textContent); i++) await sleep(250);
  // The drawing, not the status text, says that the import is done: wait for the nodes and for a quiet engine.
  for (let i = 0; i < 240 && document.querySelectorAll('#graph g.node').length < 6; i++) await sleep(250);
  await sleep(2500);
  return document.querySelectorAll('#graph g.node').length;
})()`);
const nodes = await load(example);

const pickLeaf = (skip = 0) => evaluate(`(() => {
  const list = [...document.querySelectorAll('#graph g.node')].map(g => ({id: g.dataset.nodeId, r: g.getBoundingClientRect()}))
    .filter(n => n.r.width > 8 && n.r.height > 6 && n.r.left > 40 && n.r.top > 40 && n.r.right < innerWidth - 80 && n.r.bottom < innerHeight - 80);
  const n = list[Math.floor(list.length / 2) + ${skip}];
  return n ? {id: n.id, x: n.r.left + n.r.width / 2, y: n.r.top + n.r.height / 2} : null;
})()`);
const centre = nodeId => evaluate(`(() => { const g = document.querySelector('g.node[data-node-id=${JSON.stringify(nodeId)}]'); if (!g) return null; const r = g.getBoundingClientRect(); return {x: r.left + r.width / 2, y: r.top + r.height / 2}; })()`);
// A leaf that has stopped moving: the pick is read twice around the wait, because a node placed again moves the coordinates.
const pickSettled = async (skip = 0) => {
  const early = await pickLeaf(skip);
  if (early) await settled(early.id);
  return pickLeaf(skip);
};
const dist = (a, b) => Math.hypot(a.x - b.x, a.y - b.y);
const dragBy = async (pick, dx, dy, steps = 6) => {
  await mouse('mouseMoved', pick.x, pick.y);
  await mouse('mousePressed', pick.x, pick.y);
  for (let k = 1; k <= steps; k++) await mouse('mouseMoved', pick.x + dx * k / steps, pick.y + dy * k / steps);
  await sleep(250);
};
const checks = [];
const check = (name, ok, detail = '') => { checks.push({name, ok: Boolean(ok), detail}); console.log(`${ok ? 'PASS' : 'FAIL'} ${name}${detail ? ` (${detail})` : ''}`); };
const activeGesture = () => evaluate(`document.querySelector('.resize-ghost') !== null`);

// Without a pin a layered area places the node again after the drop, so a drag is judged while it runs: the preview must
// follow the pointer. After the gesture has ended, a pointer that moves without a button must not move the node.
const follows = async (pick, dx, dy) => {
  const now = await centre(pick.id);
  return now && dist(now, {x: pick.x + dx, y: pick.y + dy}) < 8;
};
// The layout places a node again after a drop and the engine settles for a few steps: wait until the node stays where it is.
async function settled(id) {
  let last = null, stable = 0;
  for (let i = 0; i < 80 && stable < 4; i++) {
    const now = await centre(id);
    stable = now && last && dist(now, last) < 0.5 ? stable + 1 : 0;
    last = now;
    await sleep(400);
  }
}
async function nextDragWorks(label) {
  { const next = await pickLeaf(1); if (next) await settled(next.id); }
  const pick = await pickLeaf(1);
  if (!pick) return check(`${label}: next drag works`, true, 'SKIPPED: no second node');
  await dragBy(pick, 40, 20);
  const ok = await follows(pick, 40, 20);
  await mouse('mouseReleased', pick.x + 40, pick.y + 20);
  await sleep(800);
  check(`${label}: next drag works`, ok);
}
// After the end, moving the pointer with no button must leave the node where it is.
async function nothingStuck(label, pick) {
  await settled(pick.id);
  const before = await centre(pick.id);
  await mouse('mouseMoved', pick.x + 220, pick.y + 90);
  await sleep(400);
  const after = await centre(pick.id);
  check(`${label}: no drag is left running`, before && after && dist(before, after) < 4 && dist(after, {x: pick.x + 220, y: pick.y + 90}) > 30, JSON.stringify({before, after, pointer: {x: pick.x + 220, y: pick.y + 90}}));
}

const first = await pickLeaf();
if (!first) { console.log(`SKIPPED: no node inside the view (${nodes} drawn)`); (running ?? {close() {}}).close(); process.exit(0); }
console.log(`${nodes} nodes drawn (${example})`);

// 1. Release outside the window.
await dragBy(first, 60, 30);
check('the preview follows the pointer', await follows(first, 60, 30));
await mouse('mouseReleased', -40, 2000);
await sleep(800);
let at;
await nothingStuck('release outside the window', first);
await nextDragWorks('release outside the window');

// 2. A cancelled pointer abandons the gesture: the node returns.
const second = await pickSettled(2);
const startAt = await centre(second.id);
await dragBy(second, 70, 0);
check('pointercancel: the drag was running', await follows(second, 70, 0));
await evaluate(`window.dispatchEvent(new PointerEvent('pointercancel', {pointerId: 1, bubbles: true}))`);
await sleep(900);
at = await centre(second.id);
check('pointercancel does not commit the drag', at && dist(at, startAt) < 8, `distance from start ${at ? Math.round(dist(at, startAt)) : '?'} px`);
await mouse('mouseReleased', second.x + 70, second.y);
await nextDragWorks('pointercancel');

// 3. Escape returns the node.
const third = await pickSettled(3) ?? await pickSettled();
const thirdStart = await centre(third.id);
await dragBy(third, 0, 60);
check('Escape: the drag was running', await follows(third, 0, 60));
await key('keyDown', 'Escape'); await key('keyUp', 'Escape');
await sleep(900);
at = await centre(third.id);
check('Escape returns the node to its start', at && dist(at, thirdStart) < 8, `distance from start ${at ? Math.round(dist(at, thirdStart)) : '?'} px`);
await mouse('mouseReleased', third.x, third.y + 60);
await nextDragWorks('Escape');

// 4. An engine error ends the gesture and the next drag works.
const fourth = await pickSettled();
await dragBy(fourth, 30, 30, 2);
await evaluate(`__workers.forEach(w => w.dispatchEvent(new ErrorEvent('error', {message: 'injected worker failure'})))`);
await mouse('mouseMoved', fourth.x + 50, fourth.y + 40);
await sleep(600);
await mouse('mouseReleased', fourth.x + 50, fourth.y + 40);
await sleep(800);
check('an engine error leaves no stuck drag', !(await activeGesture()));
await nextDragWorks('engine error');

// 5. The model changes in the middle of a drag.
const options = await evaluate(`[...document.querySelectorAll('#example option')].map(o => o.value)`);
const other = options.find(o => o !== example);
if (!other) check('model change in the middle of a drag', true, 'SKIPPED: only one example');
else {
  const fifth = await pickSettled();
  await dragBy(fifth, 40, 0, 3);
  await load(other);
  await mouse('mouseReleased', fifth.x + 40, fifth.y);
  await sleep(1200);
  const errors = await evaluate('__errors.filter(m => !/injected/.test(m))');
  check('a model change in the middle of a drag leaves no error', errors.length === 0, errors.join('; '));
  await nextDragWorks('model change');
}

const failed = checks.filter(c => !c.ok).length;
ws.close();
running?.close();
console.log(failed ? `${failed} check(s) failed` : 'all checks passed');
process.exit(failed ? 1 : 0);
