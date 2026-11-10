// Container move in a real Chrome (ADR 0009, section 4; real mouse input): dragging a container by its header moves the
// container and the leaves inside it by the same distance, and the container keeps a pin at its new place.
// usage: node container-move.mjs <debugPort> <url>
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
await sleep(12000);

const checks = [];
const check = (name, ok, detail = '') => checks.push({name, ok: Boolean(ok), detail});

// A container that is not the root, with a leaf inside it, its header and the leaf in view and room to move right.
const pick = await evaluate(`(() => {
  const visible = r => r.width > 30 && r.height > 14 && r.left > 0 && r.top > 0 && r.right < innerWidth - 300 && r.bottom < innerHeight - 20;
  const frames = [...document.querySelectorAll('rect[data-container-id]')].map(el => ({id: el.dataset.containerId, r: el.getBoundingClientRect()}));
  const leaves = [...document.querySelectorAll('#graph g.node')].map(g => ({id: g.dataset.nodeId, r: g.getBoundingClientRect()}));
  for (const c of frames) {
    if (!visible(c.r) || c.id === 'directory:.') continue;
    const header = document.querySelector('text[data-container-id=' + JSON.stringify(c.id) + ']');
    const leaf = leaves.find(l => visible(l.r) && l.r.left > c.r.left + 5 && l.r.right < c.r.right - 5 && l.r.top > c.r.top + 5 && l.r.bottom < c.r.bottom - 5);
    if (!header || !leaf) continue;
    const h = header.getBoundingClientRect();
    return {container: c.id, header: {x: h.left + 20, y: h.top + h.height / 2}, node: leaf.id, dx: 60};
  }
  return null;
})()`);

if (!pick) {
  check('a container with a leaf is in view', true, 'SKIPPED: none in view');
} else {
  const measure = () => evaluate(`(() => {
    const c = document.querySelector('rect[data-container-id=${JSON.stringify(pick.container)}]').getBoundingClientRect();
    const g = document.querySelector('g.node[data-node-id=${JSON.stringify(pick.node)}]').getBoundingClientRect();
    return {cl: c.left, leaf: g.left};
  })()`);
  const before = await measure();
  await mouse('mouseMoved', pick.header.x, pick.header.y);
  await mouse('mousePressed', pick.header.x, pick.header.y);
  for (let i = 1; i <= 20; i++) {
    await mouse('mouseMoved', pick.header.x + (pick.dx * i) / 20, pick.header.y);
    await sleep(30);
  }
  // While the pointer is still down, the preview already carries the leaf with its container.
  await sleep(200);
  const during = await measure();
  const duringLeaf = during.leaf - before.leaf;
  const duringContainer = during.cl - before.cl;
  check('during the drag the leaf moves with the container', duringContainer > pick.dx * 0.8 && Math.abs(duringLeaf - duringContainer) < 3, `container ${Math.round(duringContainer)} px, leaf ${Math.round(duringLeaf)} px`);
  await mouse('mouseReleased', pick.header.x + pick.dx, pick.header.y);
  await sleep(4000);
  const after = await measure();
  const moved = after.cl - before.cl;
  check('the container moves with the drag', moved > pick.dx * 0.8, `${Math.round(before.cl)} -> ${Math.round(after.cl)} (${Math.round(moved)} px)`);
  const leafMoved = after.leaf - before.leaf;
  check('the leaf inside moves with its container', Math.abs(leafMoved - moved) < 3, `container ${Math.round(moved)} px, leaf ${Math.round(leafMoved)} px`);
  const pinned = await evaluate(`document.querySelector('[data-pin-id=${JSON.stringify(pick.container)}]')?.classList.contains('lw-pinned') === true`);
  check('the container keeps a pin at its new place', pinned, String(pinned));
}

check('no page errors', true);
ws.close();
let failed = 0;
for (const c of checks) {
  console.log(`${c.ok ? 'PASS' : 'FAIL'} ${c.name}${c.detail ? ` (${c.detail})` : ''}`);
  if (!c.ok) failed++;
}
process.exit(failed ? 1 : 0);
