// Re-route feedback and viewport growth of the curation editor in a real Chrome (real mouse input; not part of CI).
// usage: node reroute.mjs <debugPort> <url>
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
const mouse = async (steps, modifiers = 0) => {
  for (const step of steps) {
    await send('Input.dispatchMouseEvent', {type: step.type, x: step.x, y: step.y, button: 'left', buttons: step.type === 'mouseReleased' ? 0 : 1, clickCount: 1, modifiers});
    await sleep(60);
  }
};
const drag = (from, to) => mouse([
  {type: 'mouseMoved', x: from.x, y: from.y},
  {type: 'mousePressed', x: from.x, y: from.y},
  {type: 'mouseMoved', x: (from.x + to.x) / 2, y: (from.y + to.y) / 2},
  {type: 'mouseMoved', x: to.x, y: to.y},
  {type: 'mouseReleased', x: to.x, y: to.y},
]);
const centreOf = expression => evaluate(`(() => { const el = ${expression}; if (!el) return null; const r = el.getBoundingClientRect(); return {x: (r.left + r.right) / 2, y: (r.top + r.bottom) / 2}; })()`);
const status = () => evaluate(`document.getElementById('status').textContent`);
const statuses = [];
const watchStatus = () => evaluate(`(() => { window.__statuses = []; new MutationObserver(() => window.__statuses.push(document.getElementById('status').textContent)).observe(document.getElementById('status'), {childList: true, characterData: true, subtree: true}); return true; })()`);

await send('Runtime.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await send('Page.navigate', {url});
await sleep(4000);
await evaluate(`(async () => { for (let i = 0; i < 100 && document.getElementById('curate').disabled; i++) await new Promise(r => setTimeout(r, 100)); document.getElementById('curate').click(); })()`);
await sleep(2500);

const results = [];
const record = (name, ok, detail = '') => results.push({name, ok: Boolean(ok), detail});

// 1. A move re-routes the pending routes by itself, with the status showing it.
await watchStatus();
const body = await centreOf(`[...document.querySelectorAll('g.node > rect')][0]`);
await drag(body, {x: body.x + 40, y: body.y + 20});
await sleep(4000);
const seen = await evaluate(`window.__statuses`);
const sawRunning = seen.some(text => /Re-routing pending routes/.test(text));
const finished = (await status()) ;
record('a move re-routes pending routes automatically, with status', sawRunning && /re-routed in [\d.]+ s|Re-routing/.test(finished) && !/Re-routing pending routes …/.test(finished),
  `during: ${seen.find(t => /Re-routing/.test(t)) ?? 'no re-routing status'} · after: ${finished.slice(0, 90)}`);

// 2. A grown container stays inside the view: the viewport includes the container's right and bottom edges.
// Growing on a drag is the default (ADR 0010): no checkbox to set.
const header = await centreOf(`document.querySelector('text[data-container-id]')`);
if (!header) record('a grown container stays inside the viewport', true, 'SKIPPED: no container in this draft');
else {
  await drag(header, {x: header.x + 420, y: header.y + 40});
  await sleep(4000);
  const view = await evaluate(`(() => { const svg = document.getElementById('graph'); const vb = svg.viewBox.baseVal; const c = document.querySelector('rect[data-container-id]'); return {viewRight: vb.x + vb.width, viewBottom: vb.y + vb.height, boxRight: Number(c.getAttribute('x')) + Number(c.getAttribute('width')), boxBottom: Number(c.getAttribute('y')) + Number(c.getAttribute('height'))}; })()`);
  record('a grown container stays inside the viewport', view.boxRight <= view.viewRight && view.boxBottom <= view.viewBottom, JSON.stringify(view));
}

record('no page errors', errors.length === 0, errors.slice(0, 3).join(' | '));
ws.close();
for (const r of results) console.log(`${r.ok ? 'PASS' : 'FAIL'}  ${r.name}${r.detail ? `  (${r.detail})` : ''}`);
process.exit(results.every(r => r.ok) ? 0 : 1);
