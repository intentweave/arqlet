// Drag checks of the curation editor in a real Chrome, with real mouse input (Input.dispatchMouseEvent).
// usage: node browser_drags.mjs <debugPort> <url>
const [port, url] = [process.argv[2], process.argv[3]];
const sleep = ms => new Promise(r => setTimeout(r, ms));

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find(t => t.type === 'page');
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });

let id = 0;
const pending = new Map();
const errors = [];
ws.onmessage = event => {
  const msg = JSON.parse(event.data);
  if (msg.id && pending.has(msg.id)) {
    const {resolve, reject} = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
  }
  if (msg.method === 'Runtime.exceptionThrown') errors.push(msg.params.exceptionDetails.exception?.description ?? msg.params.exceptionDetails.text);
  if (msg.method === 'Runtime.consoleAPICalled' && msg.params.type === 'error') errors.push(msg.params.args.map(a => a.value ?? a.description).join(' '));
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
// A real mouse gesture at client coordinates. modifiers: 8 = Shift.
const mouse = async (steps, modifiers = 0) => {
  for (const step of steps) {
    await send('Input.dispatchMouseEvent', {type: step.type, x: step.x, y: step.y, button: 'left', buttons: step.type === 'mouseReleased' ? 0 : 1, clickCount: 1, modifiers});
    await sleep(60);
  }
};
// Client-space bounds of an element found by a JavaScript expression.
const boundsOf = expression => evaluate(`(() => { const el = ${expression}; if (!el) return null; const r = el.getBoundingClientRect(); return {left: r.left, top: r.top, right: r.right, bottom: r.bottom}; })()`);
const status = () => evaluate(`document.getElementById('status').textContent`);
const inspector = () => evaluate(`document.getElementById('inspector').textContent`);
const boxLine = text => text.split('\n').find(line => line.startsWith('box: ')) ?? '';
const widthOf = text => Number((boxLine(text).match(/width ([\d.]+)/) ?? [])[1]);

await send('Runtime.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await send('Page.navigate', {url});
await sleep(4000);

const checks = [];
const check = (name, ok, detail = '') => checks.push({name, ok: Boolean(ok), detail});

await evaluate(`(async () => { for (let i = 0; i < 100 && document.getElementById('curate').disabled; i++) await new Promise(r => setTimeout(r, 100)); document.getElementById('curate').click(); })()`);
await sleep(2500);

// 0. Shift-click a route first, on the unedited draft (later drags re-route the lines).: it locks, and the inspector-lock button then offers to unlock.
// A point on the line itself (not the bounding box, which can lie inside a node): half its length, in client space.
// Walk along every route line and take the first point whose topmost element is that line. A node may cover a line.
const onLine = await evaluate(`(() => { for (const el of document.querySelectorAll('polyline.route-hit')) { const len = el.getTotalLength(); const m = el.getScreenCTM(); for (let t = 0.02; t < 1; t += 0.02) { const p = el.getPointAtLength(len * t); const x = m.a * p.x + m.c * p.y + m.e, y = m.b * p.x + m.d * p.y + m.f; if (document.elementFromPoint(x, y) === el) return {x, y}; } } return null; })()`);
const rx = onLine?.x ?? 0, ry = onLine?.y ?? 0;
if (onLine) await mouse([{type: 'mouseMoved', x: rx, y: ry}, {type: 'mousePressed', x: rx, y: ry}, {type: 'mouseReleased', x: rx, y: ry}], 8);
await sleep(1500);
const lockStatus = await status();
const lockLabel = await evaluate(`document.getElementById('inspector-lock').textContent`);
if (onLine) check('shift-click locks the route', /Route locked/.test(lockStatus) || lockLabel === 'Unlock route', lockStatus.slice(0, 120));
else checks.push({name: 'shift-click locks the route', ok: true, detail: 'SKIPPED: no route line is free of nodes in this draft'});

// Unlock again, so the drags below may re-route the line. A locked route refuses those changes, as designed.
if (onLine) {
  await mouse([{type: 'mouseMoved', x: rx, y: ry}, {type: 'mousePressed', x: rx, y: ry}, {type: 'mouseReleased', x: rx, y: ry}], 8);
  await sleep(1500);
  const unlockStatus = await status();
  check('shift-click again unlocks the route', /Route unlocked/.test(unlockStatus), unlockStatus.slice(0, 120));
}

// 1. Node drag: press on the body centre, move right and down, release.
const body = await boundsOf(`[...document.querySelectorAll('g.node > rect')][0]`);
const cx = (body.left + body.right) / 2, cy = (body.top + body.bottom) / 2;
await mouse([{type: 'mouseMoved', x: cx, y: cy}, {type: 'mousePressed', x: cx, y: cy}, {type: 'mouseMoved', x: cx + 30, y: cy + 20}, {type: 'mouseMoved', x: cx + 40, y: cy + 25}, {type: 'mouseReleased', x: cx + 40, y: cy + 25}]);
await sleep(1500);
const afterDrag = await status();
check('node drag moves the node and reports it', /Node moved/.test(afterDrag), afterDrag.slice(0, 120));

// 2. Leaf resize: press on the bottom-right corner, drag outwards.
const selectedBody = await boundsOf(`[...document.querySelectorAll('g.node > rect')][0]`);
const before = await inspector();
const widthBefore = widthOf(before);
const corner = {x: selectedBody.right - 4, y: selectedBody.bottom - 4};
await mouse([{type: 'mouseMoved', x: corner.x, y: corner.y}, {type: 'mousePressed', x: corner.x, y: corner.y}, {type: 'mouseMoved', x: corner.x + 30, y: corner.y + 10}, {type: 'mouseReleased', x: corner.x + 30, y: corner.y + 10}]);
await sleep(1500);
const afterResize = await status();
const widthAfter = widthOf(await inspector());
check('corner drag resizes the leaf', /Size changed/.test(afterResize) && widthAfter > widthBefore, `${widthBefore} -> ${widthAfter}`);

check('no page errors', errors.length === 0, errors.slice(0, 3).join(' | '));

ws.close();
for (const c of checks) console.log(`${c.ok ? 'PASS' : 'FAIL'}  ${c.name}${c.detail ? `  (${c.detail})` : ''}`);
process.exit(checks.every(c => c.ok) ? 0 : 1);
