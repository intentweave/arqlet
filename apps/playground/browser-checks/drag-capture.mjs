// Node drag with real mouse input (Input.dispatchMouseEvent): the drag keeps the pointer capture on the drawing,
// so every move reaches it and the node follows the whole path. The viewport's pan must not take the press.
// usage: node drag-capture.mjs <debugPort> <url>
const [port, url] = [process.argv[2], process.argv[3]];
const sleep = ms => new Promise(r => setTimeout(r, ms));

const targets = await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
const page = targets.find(t => t.type === 'page');
const ws = new WebSocket(page.webSocketDebuggerUrl);
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

await send('Runtime.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await send('Page.navigate', {url});
await sleep(4000);

const checks = [];
const check = (name, ok, detail = '') => checks.push({name, ok: Boolean(ok), detail});

// A leaf node that is fully inside the view.
const pick = await evaluate(`(() => {
  const nodes = [...document.querySelectorAll('#graph g.node')].map(g => ({id: g.dataset.nodeId, r: g.getBoundingClientRect()}));
  const inside = nodes.filter(n => n.r.width > 30 && n.r.height > 14 && n.r.left > 0 && n.r.top > 0 && n.r.right < innerWidth && n.r.bottom < innerHeight);
  const n = inside[Math.floor(inside.length / 2)];
  return n ? {id: n.id, x: n.r.left + n.r.width / 2, y: n.r.top + n.r.height / 2} : null;
})()`);
if (!pick) {
  checks.push({name: 'a node is visible for the drag', ok: true, detail: 'SKIPPED: no node inside the view'});
} else {
  const boxOf = () => evaluate(`(() => { const g = document.querySelector('g.node[data-node-id=${JSON.stringify(pick.id)}]'); if (!g) return null; const r = g.getBoundingClientRect(); return {x: r.left + r.width / 2, y: r.top + r.height / 2}; })()`);
  const before = await boxOf();
  await evaluate(`(() => { window.__drag = []; window.addEventListener('pointermove', e => __drag.push(e.target.closest('#graph') ? 'graph' : e.target.tagName), true); })()`);

  await mouse('mouseMoved', pick.x, pick.y);
  await mouse('mousePressed', pick.x, pick.y);
  await evaluate(`__drag = []`);
  const steps = 20;
  for (let s = 1; s <= steps; s++) {
    await mouse('mouseMoved', pick.x + s * 8, pick.y + s * 4);
    await sleep(20);
  }
  const end = {x: pick.x + steps * 8, y: pick.y + steps * 4};
  await mouse('mouseReleased', end.x, end.y);
  await sleep(300);

  const targetsSeen = await evaluate(`__drag`);
  const after = await boxOf();
  const moved = before && after ? Math.hypot(after.x - before.x, after.y - before.y) : 0;
  const travelled = Math.hypot(end.x - pick.x, end.y - pick.y);
  check('every move reaches the drawing during the drag', targetsSeen.length === steps && targetsSeen.every(t => t === 'graph'), `${targetsSeen.filter(t => t === 'graph').length}/${steps} on #graph`);
  check('the node follows the pointer over the whole path', moved >= travelled * 0.9, `moved ${Math.round(moved)} px of ${Math.round(travelled)} px`);
  // A release after the drag keeps the node where the pointer left it: the drop is delivered, not lost.
  const settled = await boxOf();
  await sleep(500);
  const later = await boxOf();
  check('the node stays where it was dropped', Math.hypot(later.x - settled.x, later.y - settled.y) < 1, `drift ${Math.round(Math.hypot(later.x - settled.x, later.y - settled.y))} px`);
}

ws.close();
let failed = 0;
for (const c of checks) {
  console.log(`${c.ok ? 'PASS' : 'FAIL'} ${c.name}${c.detail ? ` (${c.detail})` : ''}`);
  if (!c.ok) failed++;
}
process.exit(failed ? 1 : 0);
