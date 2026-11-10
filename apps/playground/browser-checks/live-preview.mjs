// Live preview of a node drag in a real Chrome (real mouse input; not part of CI).
// During the drag, the routes of the moved node must change before the mouse is released (tier 1, drawn dashed),
// and the status line must show the drop after release.
// usage: node live-preview.mjs <debugPort> <url>
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
const mouse = async (type, x, y) => {
  await send('Input.dispatchMouseEvent', {type, x, y, button: 'left', buttons: type === 'mouseReleased' ? 0 : 1, clickCount: 1, modifiers: 0});
};
const centreOf = expression => evaluate(`(() => { const el = ${expression}; if (!el) return null; const r = el.getBoundingClientRect(); return {x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2}; })()`);
const status = () => evaluate(`document.getElementById('status').textContent`);
// The points of every visible route, as one string: it changes when a preview is drawn.
const routePoints = () => evaluate(`[...document.querySelectorAll('polyline[data-route-id]:not(.route-hit)')].map(el => el.getAttribute('points') + '|' + (el.getAttribute('stroke-dasharray') ?? '')).join(' ;; ')`);

await send('Runtime.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await send('Page.navigate', {url});
await sleep(4000);
await evaluate(`(async () => { for (let i = 0; i < 100 && document.getElementById('curate').disabled; i++) await new Promise(r => setTimeout(r, 100)); document.getElementById('curate').click(); })()`);
await sleep(2500);

const results = [];
const record = (name, ok, detail = '') => results.push({name, ok: Boolean(ok), detail});

const before = await routePoints();
const body = await centreOf(`[...document.querySelectorAll('g.node > rect')][0]`);
await mouse('mouseMoved', body.x, body.y);
await mouse('mousePressed', body.x, body.y);
for (let step = 1; step <= 6; step++) {
  await mouse('mouseMoved', body.x + step * 4, body.y + step * 2);
  await sleep(120);
}
const during = await routePoints();
const dashed = await evaluate(`[...document.querySelectorAll('polyline[data-route-id]:not(.route-hit)')].some(el => el.getAttribute('stroke-dasharray'))`);
record('routes change while the node is still dragged (live preview)', during !== before && dashed, dashed ? 'dashed preview drawn' : 'no dashed preview');
await sleep(1500);
await mouse('mouseReleased', body.x + 24, body.y + 12);
await sleep(2500);
const text = await status();
record('release commits the move; the automatic re-route then finishes', /Node moved|Selection moved|Re-routing pending routes in/.test(text) || /already at the border/.test(text), text.slice(0, 110));
record('no page errors', errors.length === 0, errors.slice(0, 3).join(' | '));

ws.close();
for (const r of results) console.log(`${r.ok ? 'PASS' : 'FAIL'}  ${r.name}${r.detail ? `  (${r.detail})` : ''}`);
process.exit(results.every(r => r.ok) ? 0 : 1);
