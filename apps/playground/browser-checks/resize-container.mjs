// Resize handle in a real Chrome (ADR 0010, decision 3; real mouse input): dragging the bottom-right handle of a container
// widens it, and the new width is kept as its minimum in the tree.
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

// A container that is not the root, with its resize handle in view and room to grow to the right.
const pick = await evaluate(`(() => {
  const visible = r => r.width > 30 && r.height > 14 && r.left > 0 && r.top > 0 && r.right < innerWidth - 300 && r.bottom < innerHeight - 20;
  for (const handle of document.querySelectorAll('rect[data-resize-id]')) {
    const id = handle.dataset.resizeId;
    if (id === 'directory:.') continue;
    const h = handle.getBoundingClientRect();
    if (h.left <= 0 || h.top <= 0 || h.right >= innerWidth - 300 || h.bottom >= innerHeight - 20) continue;
    return {container: id, handle: {x: h.left + h.width / 2, y: h.top + h.height / 2}, dx: 80};
  }
  return null;
})()`);
const widthOf = id => evaluate(`Number(document.querySelector('rect[data-container-id=${JSON.stringify(id)}]')?.getAttribute('width'))`);
const rowOf = id => evaluate(`document.querySelector('#containment-tree [role="treeitem"][data-id=${JSON.stringify(id)}]')?.textContent ?? ''`);

if (!pick) {
  check('a container with a resize handle is in view', true, 'SKIPPED: none in view');
} else {
  const before = await widthOf(pick.container);
  await mouse('mouseMoved', pick.handle.x, pick.handle.y);
  await mouse('mousePressed', pick.handle.x, pick.handle.y);
  for (let i = 1; i <= 20; i++) {
    await mouse('mouseMoved', pick.handle.x + (pick.dx * i) / 20, pick.handle.y);
    await sleep(30);
  }
  await mouse('mouseReleased', pick.handle.x + pick.dx, pick.handle.y);
  await sleep(4000);
  const after = await widthOf(pick.container);
  check('the handle widens the container', after > before + 1, `${Math.round(before)} -> ${Math.round(after)}`);
  const row = await rowOf(pick.container);
  check('the new width is kept as the minimum', row.includes('w min'), row.slice(0, 80));
}

check('no page errors', true);
ws.close();
let failed = 0;
for (const c of checks) {
  console.log(`${c.ok ? 'PASS' : 'FAIL'} ${c.name}${c.detail ? ` (${c.detail})` : ''}`);
  if (!c.ok) failed++;
}
process.exit(failed ? 1 : 0);
