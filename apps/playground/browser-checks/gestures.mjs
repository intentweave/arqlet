// More gesture checks of the curation editor in a real Chrome, with real mouse and keyboard modifiers.
// A check whose precondition is missing in the draft is reported as SKIPPED, never as a pass.
// usage: node browser_more.mjs <debugPort> <url>
const [port, url] = [process.argv[2], process.argv[3]];
const sleep = ms => new Promise(r => setTimeout(r, ms));
// On macOS Ctrl with the primary button is a right-click, so the multi-select uses Cmd (Meta).
const SHIFT = 8, ALT = 1, META = 4;

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
const mouse = async (steps, modifiers = 0) => {
  for (const step of steps) {
    await send('Input.dispatchMouseEvent', {type: step.type, x: step.x, y: step.y, button: 'left', buttons: step.type === 'mouseReleased' ? 0 : 1, clickCount: 1, modifiers});
    await sleep(60);
  }
};
const drag = (from, to, modifiers = 0) => mouse([
  {type: 'mouseMoved', x: from.x, y: from.y},
  {type: 'mousePressed', x: from.x, y: from.y},
  {type: 'mouseMoved', x: (from.x + to.x) / 2, y: (from.y + to.y) / 2},
  {type: 'mouseMoved', x: to.x, y: to.y},
  {type: 'mouseReleased', x: to.x, y: to.y},
], modifiers);
const centreOf = expression => evaluate(`(() => { const el = ${expression}; if (!el) return null; const r = el.getBoundingClientRect(); return {x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2}; })()`);
const status = () => evaluate(`document.getElementById('status').textContent`);
const inspector = () => evaluate(`document.getElementById('inspector').textContent`);
// A point on a route line that is the topmost element there, so the click reaches the line and not a node.
const freeLinePoint = () => evaluate(`(() => { for (const el of document.querySelectorAll('polyline.route-hit')) { const len = el.getTotalLength(); const m = el.getScreenCTM(); for (let t = 0.02; t < 1; t += 0.02) { const p = el.getPointAtLength(len * t); const x = m.a * p.x + m.c * p.y + m.e, y = m.b * p.x + m.d * p.y + m.f; if (document.elementFromPoint(x, y) === el) return {x, y, route: el.getAttribute('data-route-id')}; } } return null; })()`);

await send('Runtime.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await send('Page.navigate', {url});
await sleep(4000);
// Turn on the edge labels so the draft has label boxes to drag.
await evaluate(`(async () => { for (let i = 0; i < 100 && document.getElementById('edge-labels').disabled; i++) await new Promise(r => setTimeout(r, 100)); const box = document.getElementById('edge-labels'); if (!box.checked) box.click(); })()`);
await sleep(3000);
await evaluate(`(async () => { for (let i = 0; i < 100 && document.getElementById('curate').disabled; i++) await new Promise(r => setTimeout(r, 100)); document.getElementById('curate').click(); })()`);
await sleep(2500);

const results = [];
const record = (name, state, detail = '') => results.push({name, state, detail});
const expectStatus = (name, text, pattern) => (pattern.test(text) ? record(name, 'PASS', text.slice(0, 110)) : record(name, 'FAIL', text.slice(0, 110)));

// 1. Shift-click a node pins its size; Shift-click again unpins it.
const nodeCentre = await centreOf(`[...document.querySelectorAll('g.node > rect')][0]`);
await drag(nodeCentre, nodeCentre, SHIFT);
await sleep(1200);
expectStatus('shift-click pins a node size', await status(), /Size pinned/);
await drag(nodeCentre, nodeCentre, SHIFT);
await sleep(1200);
expectStatus('shift-click unpins it again', await status(), /Size unpinned/);

// 2. Alt-click a route segment adds a dogleg (no drag).
const line = await freeLinePoint();
if (!line) record('alt-click adds a dogleg', 'SKIPPED', 'no route line is free of nodes');
else {
  await drag(line, line, ALT);
  await sleep(1200);
  const text = await status();
  if (/Dogleg added/.test(text)) record('alt-click adds a dogleg', 'PASS', text.slice(0, 110));
  else if (/Only inner/.test(text)) record('alt-click adds a dogleg', 'SKIPPED', 'the nearest segment is an end segment at a port');
  else record('alt-click adds a dogleg', 'FAIL', text.slice(0, 110));
}

// 3. Drag a segment of a route perpendicular to itself.
const line2 = await freeLinePoint();
if (!line2) record('drag a segment', 'SKIPPED', 'no route line is free of nodes');
else {
  await drag(line2, {x: line2.x + 25, y: line2.y + 25});
  await sleep(1200);
  const text = await status();
  if (/Segment moved/.test(text)) record('drag a segment', 'PASS', text.slice(0, 110));
  else if (/Only inner/.test(text)) record('drag a segment', 'SKIPPED', 'the nearest segment is an end segment at a port');
  else record('drag a segment', 'FAIL', text.slice(0, 110));
}

// 4. Drag a container by its header.
const header = await centreOf(`document.querySelector('text[data-container-id]')`);
if (!header) record('drag a container by its header', 'SKIPPED', 'no container in this draft');
else {
  await drag(header, {x: header.x + 30, y: header.y + 20});
  await sleep(1500);
  const text = await status();
  if (/Container moved/.test(text)) record('drag a container by its header', 'PASS', text.slice(0, 110));
  else record('drag a container by its header', 'FAIL', text.slice(0, 110));
}

// 5. Drag a port along its side (or to another side). Explicit ports refuse; the first automatic port that moves counts.
const portCount = await evaluate(`document.querySelectorAll('circle[data-port-id]').length`);
let portResult = null;
for (let i = 0; i < Math.min(portCount, 6) && !portResult; i++) {
  const port = await centreOf(`document.querySelectorAll('circle[data-port-id]')[${i}]`);
  await drag(port, {x: port.x + 15, y: port.y + 15});
  await sleep(1200);
  const text = await status();
  if (/Port moved/.test(text)) portResult = {pass: true, text};
  else if (/Explicit port|already at the end/.test(text)) continue;
  else portResult = {pass: false, text};
}
if (!portResult) record('drag a port', 'SKIPPED', 'no movable port in the first six');
else record('drag a port', portResult.pass ? 'PASS' : 'FAIL', portResult.text.slice(0, 110));

// 6. Label drag: only if the draft has a label box.
const label = await centreOf(`document.querySelector('rect[data-label-id]')`);
if (!label) record('drag a label box', 'SKIPPED', 'the draft has no label box');
else {
  await drag(label, {x: label.x + 20, y: label.y + 10});
  await sleep(1500);
  expectStatus('drag a label box', await status(), /Label moved/);
}

// 7. Joint selection: Ctrl-click a second node, then drag the first selected node; the status says the selection moved.
const first = await centreOf(`[...document.querySelectorAll('g.node > rect')][0]`);
const second = await centreOf(`[...document.querySelectorAll('g.node > rect')][1]`);
if (!second) record('drag a joint selection', 'SKIPPED', 'fewer than two nodes');
else {
  // A plain click selects the first node (replacing any selection), then Cmd-click adds the second.
  await drag(first, first, 0);
  await sleep(400);
  await drag(second, second, META);
  await sleep(600);
  await drag(first, {x: first.x + 30, y: first.y + 10});
  await sleep(1500);
  const text = await status();
  if (/Selection moved/.test(text)) record('drag a joint selection', 'PASS', text.slice(0, 110));
  else record('drag a joint selection', 'FAIL', text.slice(0, 110));
}

record('no page errors', errors.length === 0 ? 'PASS' : 'FAIL', errors.slice(0, 3).join(' | '));
ws.close();
for (const r of results) console.log(`${r.state.padEnd(7)} ${r.name}${r.detail ? `  (${r.detail})` : ''}`);
process.exit(results.some(r => r.state === 'FAIL') ? 1 : 0);
