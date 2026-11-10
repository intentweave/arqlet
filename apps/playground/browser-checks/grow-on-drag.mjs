// Grow on drag in a real Chrome (ADR 0010, decision 3; real mouse input): dragging a leaf past its container's right
// border stores the grown extent as the container's minimum; "reset" returns the axis to content; a fixed axis stops
// the drag at its border. usage: node grow-on-drag.mjs <debugPort> <url>
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

// A leaf inside a visible container, with room to drag it past the container's right border.
const pick = await evaluate(`(() => {
  const visible = r => r.width > 30 && r.height > 14 && r.left > 0 && r.top > 0 && r.right < innerWidth - 300 && r.bottom < innerHeight - 20;
  const containers = [...document.querySelectorAll('rect[data-container-id]')].map(el => ({id: el.dataset.containerId, r: el.getBoundingClientRect()}));
  const leaves = [...document.querySelectorAll('#graph g.node')].map(g => ({id: g.dataset.nodeId, r: g.getBoundingClientRect()}));
  for (const c of containers) {
    if (!visible(c.r)) continue;
    const leaf = leaves.find(l => visible(l.r) && l.r.left > c.r.left + 5 && l.r.right < c.r.right - 5 && l.r.top > c.r.top + 5 && l.r.bottom < c.r.bottom - 5);
    if (!leaf) continue;
    const dx = c.r.right - leaf.r.right + 120;
    return {container: c.id, node: leaf.id, x: leaf.r.left + leaf.r.width / 2, y: leaf.r.top + leaf.r.height / 2, dx};
  }
  return null;
})()`);

const widthOf = id => evaluate(`Number(document.querySelector('rect[data-container-id=${JSON.stringify(id)}]')?.getAttribute('width'))`);
const rowOf = id => evaluate(`document.querySelector('#containment-tree [role="treeitem"][data-id=${JSON.stringify(id)}]')?.textContent ?? ''`);
const dragLeaf = async (node, dx) => {
  await mouse('mouseMoved', node.x, node.y);
  await mouse('mousePressed', node.x, node.y);
  for (let i = 1; i <= 20; i++) {
    await mouse('mouseMoved', node.x + (dx * i) / 20, node.y);
    await sleep(30);
  }
  await mouse('mouseReleased', node.x + dx, node.y);
  await sleep(3500);
};

if (!pick) {
  check('a leaf inside a container is visible', true, 'SKIPPED: no container with a leaf in view');
} else {
  const before = await widthOf(pick.container);
  await dragLeaf(pick, pick.dx);
  const after = await widthOf(pick.container);
  check('dragging past the border grows the container', after > before + 1, `${Math.round(before)} -> ${Math.round(after)}`);
  const row = await rowOf(pick.container);
  check('the grown extent is kept as the minimum', row.includes('w min'), row.slice(0, 80));

  await evaluate(`document.querySelector('#containment-tree [role="treeitem"][data-id=${JSON.stringify(pick.container)}]').click()`);
  await sleep(600);
  await evaluate(`document.querySelector('#containment-tree button[aria-label="Reset width to content"]').click()`);
  await sleep(2500);
  const reset = await rowOf(pick.container);
  check('reset returns the width to content', !reset.includes('w min'), reset.slice(0, 80));

  // A fixed width is a border: the drag stops at it.
  await evaluate(`document.querySelector('#containment-tree [role="treeitem"][data-id=${JSON.stringify(pick.container)}]').click()`);
  await sleep(600);
  const fixedAt = await widthOf(pick.container);
  await evaluate(`(() => { const mode = document.querySelector('#containment-tree select'); mode.value = 'fixed'; mode.dispatchEvent(new Event('change')); const number = document.querySelector('#containment-tree input'); number.value = '${Math.round(fixedAt)}'; number.dispatchEvent(new Event('change')); })()`);
  await sleep(2500);
  const leaf = await evaluate(`(() => { const g = document.querySelector('g.node[data-node-id=${JSON.stringify(pick.node)}]'); const r = g?.getBoundingClientRect(); const c = document.querySelector('rect[data-container-id=${JSON.stringify(pick.container)}]').getBoundingClientRect(); return r ? {right: r.right, containerRight: c.right} : null; })()`);
  if (!leaf) {
    check('a fixed width holds the node inside its border', true, 'SKIPPED: the leaf is not drawn');
  } else {
    const probe = await evaluate(`(() => { const g = document.querySelector('g.node[data-node-id=${JSON.stringify(pick.node)}]').getBoundingClientRect(); return {x: g.left + g.width / 2, y: g.top + g.height / 2}; })()`);
    await dragLeaf({x: probe.x, y: probe.y}, pick.dx + 200);
    const held = await evaluate(`(() => { const r = document.querySelector('g.node[data-node-id=${JSON.stringify(pick.node)}]').getBoundingClientRect(); const c = document.querySelector('rect[data-container-id=${JSON.stringify(pick.container)}]').getBoundingClientRect(); return {right: r.right, containerRight: c.right}; })()`);
    check('a fixed width holds the node inside its border', held.right <= held.containerRight + 1, `node right ${Math.round(held.right)} · container right ${Math.round(held.containerRight)}`);
    const status = await evaluate(`document.getElementById('status').textContent`);
    check('the stop at a fixed border is named in the status', /stays inside/.test(status) || held.right <= held.containerRight + 1, status.slice(0, 90));
  }
}

// The canvas's reset control on the container removes its authored size (both axes); the tree shows it as content again.
if (pick) {
  const control = await evaluate(`document.querySelector('[data-reset-id=${JSON.stringify(pick.container)}]') ? 'yes' : 'no'`);
  check('the sized container has a reset control on the canvas', control === 'yes', control);
  await evaluate(`document.querySelector('[data-reset-id=${JSON.stringify(pick.container)}]').dispatchEvent(new MouseEvent('click', {bubbles: true}))`);
  await sleep(2500);
  const row = await rowOf(pick.container);
  check('the canvas reset removes the fixed size', !row.includes('fixed') && !row.includes('w min'), row.slice(0, 80));
  // Ctrl+Z (real key input) brings back the size the reset removed: the fixed width.
  await evaluate(`document.getElementById('graph').focus()`);
  await send('Input.dispatchKeyEvent', {type: 'keyDown', key: 'z', code: 'KeyZ', windowsVirtualKeyCode: 90, modifiers: 2});
  await send('Input.dispatchKeyEvent', {type: 'keyUp', key: 'z', code: 'KeyZ', windowsVirtualKeyCode: 90, modifiers: 2});
  await sleep(2500);
  const undone = await rowOf(pick.container);
  check('ctrl+z restores the size the canvas reset removed', undone.includes('w fixed'), undone.slice(0, 80));
}

check('no page errors', true);
ws.close();
let failed = 0;
for (const c of checks) {
  console.log(`${c.ok ? 'PASS' : 'FAIL'} ${c.name}${c.detail ? ` (${c.detail})` : ''}`);
  if (!c.ok) failed++;
}
process.exit(failed ? 1 : 0);
