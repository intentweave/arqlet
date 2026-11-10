// Latency of a node drag with real mouse input (Input.dispatchMouseEvent), in a real Chrome. Prints one JSON line.
//   pointer -> preview:  time from a mouse move until the dragged node is drawn at the pointer
//   pointer -> result:   time from the release until the engine's last reply (no reply for 400 ms)
//   requests/replies:    messages to and from the page's workers during the drag (counted, not sized, so that sizing does
//                        not disturb the timing); `--bytes` sizes them in a second run
// usage: node latency.mjs <debugPort> <url> [example] [moves] [--bytes]
const [port, url, example = 'default', movesArg = '20'] = process.argv.slice(2).filter(a => !a.startsWith('--'));
const withBytes = process.argv.includes('--bytes');
const moves = Number(movesArg);
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
// Count the messages of every worker before the page script runs.
await send('Page.addScriptToEvaluateOnNewDocument', {source: `(() => {
  window.__lat = {requests: 0, replies: 0, requestBytes: 0, replyBytes: 0, lastReply: 0};
  const Original = window.Worker;
  const bytes = ${withBytes ? 'true' : 'false'};
  window.Worker = class extends Original {
    constructor(...args) {
      super(...args);
      this.addEventListener('message', e => { __lat.replies++; __lat.lastReply = performance.now(); if (bytes) __lat.replyBytes += JSON.stringify(e.data).length; });
    }
    postMessage(message, ...rest) { __lat.requests++; if (bytes) __lat.requestBytes += JSON.stringify(message).length; return super.postMessage(message, ...rest); }
  };
})();`});
await send('Page.navigate', {url});
await sleep(3000);

// Load the example and wait for its import.
const loaded = await evaluate(`(async () => {
  const select = document.getElementById('example');
  if (!select.querySelector('option[value=${JSON.stringify(example)}]')) return {ok: false, reason: 'no such example'};
  select.value = ${JSON.stringify(example)};
  select.dispatchEvent(new Event('change'));
  const start = performance.now();
  while (performance.now() - start < 60000) {
    await new Promise(r => setTimeout(r, 200));
    if (/imported/i.test(document.getElementById('status').textContent)) break;
  }
  await new Promise(r => setTimeout(r, 1500));
  const nodes = document.querySelectorAll('#graph g.node').length;
  return {ok: true, status: document.getElementById('status').textContent.slice(0, 100), visibleNodes: nodes};
})()`);
if (!loaded.ok) { console.log(JSON.stringify({example, error: loaded.reason})); process.exit(1); }

// A leaf near the middle of the view that the pointer really reaches (not a container, nothing on top of it).
const pick = await evaluate(`(() => {
  const nodes = [...document.querySelectorAll('#graph g.node')]
    .filter(g => !g.querySelector('[data-container-id]'))
    .map(g => ({id: g.dataset.nodeId, g, r: g.getBoundingClientRect()}))
    .filter(n => n.r.width > 8 && n.r.height > 6 && n.r.left > 20 && n.r.top > 20 && n.r.right < innerWidth - 80 && n.r.bottom < innerHeight - 20)
    .filter(n => document.elementFromPoint(n.r.left + n.r.width / 2, n.r.top + n.r.height / 2)?.closest('g.node') === n.g);
  const n = nodes[Math.floor(nodes.length / 2)];
  return n ? {id: n.id, x: n.r.left + n.r.width / 2, y: n.r.top + n.r.height / 2} : null;
})()`);
if (!pick) { console.log(JSON.stringify({example, ...loaded, error: 'no leaf that the pointer reaches'})); process.exit(1); }
const centre = () => evaluate(`(() => { const g = document.querySelector('g.node[data-node-id=${JSON.stringify(pick.id)}]'); if (!g) return null; const r = g.getBoundingClientRect(); return {x: r.left + r.width / 2, y: r.top + r.height / 2}; })()`);
// Resolves with the milliseconds until the node's centre is within 3 px of (x, y), or null after 3 s.
const untilAt = (x, y) => evaluate(`new Promise(resolve => {
  const start = performance.now();
  const check = () => {
    const g = document.querySelector('g.node[data-node-id=${JSON.stringify(pick.id)}]');
    if (g) { const r = g.getBoundingClientRect(); if (Math.hypot(r.left + r.width / 2 - ${x}, r.top + r.height / 2 - ${y}) <= 3) return resolve(performance.now() - start); }
    if (performance.now() - start > 3000) return resolve(null);
    requestAnimationFrame(check);
  };
  check();
})`);

await evaluate(`Object.assign(__lat, {requests: 0, replies: 0, requestBytes: 0, replyBytes: 0})`);
await mouse('mouseMoved', pick.x, pick.y);
await mouse('mousePressed', pick.x, pick.y);
const preview = [];
const stepX = 6, stepY = 2;
for (let k = 1; k <= moves; k++) {
  const x = pick.x + stepX * k, y = pick.y + stepY * k;
  await mouse('mouseMoved', x, y);
  const ms = await untilAt(x, y);
  if (ms !== null) preview.push(ms);
}
const releaseAt = await evaluate('performance.now()');
await mouse('mouseReleased', pick.x + stepX * moves, pick.y + stepY * moves);
// The result is complete when no reply arrived for 400 ms.
const settled = await evaluate(`new Promise(resolve => {
  const release = ${releaseAt};
  const check = () => {
    const quiet = performance.now() - Math.max(__lat.lastReply, release);
    if (quiet > 400) return resolve(Math.max(__lat.lastReply, release) - release);
    setTimeout(check, 50);
  };
  check();
})`);
const counts = await evaluate('({requests: __lat.requests, replies: __lat.replies, requestBytes: __lat.requestBytes, replyBytes: __lat.replyBytes})');
const end = await centre();
const sorted = [...preview].sort((a, b) => a - b);
const median = sorted.length ? sorted[Math.floor(sorted.length / 2)] : null;
console.log(JSON.stringify({
  example, visibleNodes: loaded.visibleNodes, moves,
  previewMs: {median, max: sorted.at(-1) ?? null, missed: moves - preview.length},
  resultAfterReleaseMs: Math.round(settled),
  ...counts,
  nodeMoved: end ? Math.round(Math.hypot(end.x - pick.x, end.y - pick.y)) : null,
  bytes: withBytes,
}));
ws.close();
