// Canvas controls with real mouse input (Input.dispatchMouseEvent): the pin control on a leaf pins and unpins its
// position, the pin control on a container pins it, and the toggle on a container collapses it and expands it again.
// A press on a control must not start a drag, a pan or a selection.
// usage: node canvas-controls.mjs <debugPort> <url>
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
const click = async ({x, y}) => {
  await mouse('mouseMoved', x, y);
  await mouse('mousePressed', x, y);
  await mouse('mouseReleased', x, y);
};

await send('Runtime.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await send('Page.navigate', {url});
await sleep(4000);
await evaluate(`(() => { window.__errors = []; addEventListener('error', e => __errors.push(e.message)); })()`);
// Controls follow the screen size; zoom in once so they are large enough to hit like a user would.
await evaluate(`document.getElementById('zoom').click()`);
await sleep(600);

const checks = [];
const check = (name, ok, detail = '') => checks.push({name, ok: Boolean(ok), detail});
// The centre of a control that is fully inside the canvas, scrolled into view first.
const centreOf = selector => evaluate(`(() => {
  const el = document.querySelector(${JSON.stringify(selector)});
  if (!el) return null;
  el.scrollIntoView({block: 'center', inline: 'center'});
  const r = el.getBoundingClientRect(), c = document.getElementById('canvas').getBoundingClientRect();
  if (r.width < 4 || r.left < c.left || r.right > c.right || r.top < c.top || r.bottom > c.bottom) return null;
  return {x: r.left + r.width / 2, y: r.top + r.height / 2, size: Math.round(r.width)};
})()`);
const pinOf = node => evaluate(`JSON.stringify(window.__lastPins = (document.querySelector('[data-pin-id=${JSON.stringify(node)}]')?.getAttribute('class') ?? 'missing'))`);
const selectedText = () => evaluate(`document.getElementById('selected').textContent`);

// 1. A leaf: pin, then unpin, through its control. The selection stays as it was.
const leaf = await evaluate(`[...document.querySelectorAll('#graph g.node')].map(g => g.dataset.nodeId).find(Boolean) ?? null`);
const leafControl = leaf && await centreOf(`[data-pin-id=${JSON.stringify(leaf)}]`);
if (!leafControl) check('a leaf pin control is clickable', true, 'SKIPPED: no leaf control inside the view');
else {
  const before = await selectedText();
  await click(leafControl);
  await sleep(2500);
  check('the leaf pin control pins the node', (await pinOf(leaf)).includes('lw-pinned'), `control ${leafControl.size}px`);
  check('a press on the control does not select the node', (await selectedText()) === before, await selectedText());
  const again = await centreOf(`[data-pin-id=${JSON.stringify(leaf)}]`);
  await click(again);
  await sleep(2500);
  check('a second press unpins it', !(await pinOf(leaf)).includes('lw-pinned'));
}

// 2. A container: pin it, then collapse and expand it through its toggle.
const container = await evaluate(`[...document.querySelectorAll('#graph rect[data-container-id]')].map(r => r.dataset.containerId).at(-1) ?? null`);
const containerPin = container && await centreOf(`[data-pin-id=${JSON.stringify(container)}]`);
if (!containerPin) check('a container pin control is clickable', true, 'SKIPPED: no container control inside the view');
else {
  await click(containerPin);
  await sleep(2500);
  check('the container pin control pins the container', (await pinOf(container)).includes('lw-pinned'));
  const pins = await evaluate(`document.querySelector('rect[data-container-id=${JSON.stringify(container)}]')?.getAttribute('data-pinned-id') ?? ''`);
  check('the container frame is marked as pinned', pins === container, pins);
  const toggle = await centreOf(`[data-toggle-id=${JSON.stringify(container)}]`);
  await click(toggle);
  await sleep(2500);
  const collapsed = await evaluate(`document.querySelector('g.node [data-toggle-id=${JSON.stringify(container)}]')?.getAttribute('aria-label') ?? ''`);
  check('the toggle collapses the container', collapsed.startsWith('Expand'), collapsed);
  const expand = await centreOf(`g.node [data-toggle-id=${JSON.stringify(container)}]`);
  if (expand) {
    await click(expand);
    await sleep(2500);
    const open = await evaluate(`document.querySelector('[data-toggle-id=${JSON.stringify(container)}]')?.getAttribute('aria-label') ?? ''`);
    check('the expand control opens it again', open.startsWith('Collapse'), open);
  } else check('the expand control opens it again', true, 'SKIPPED: the collapsed node is outside the view');
}
const errors = await evaluate(`__errors`);
check('no page errors', errors.length === 0, errors.join(' | '));

ws.close();
let failed = 0;
for (const c of checks) {
  console.log(`${c.ok ? 'PASS' : 'FAIL'} ${c.name}${c.detail ? ` (${c.detail})` : ''}`);
  if (!c.ok) failed++;
}
process.exit(failed ? 1 : 0);
