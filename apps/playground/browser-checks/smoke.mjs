// Smoke check of the curation editor in a real Chrome over the DevTools protocol. No npm dependencies.
// usage: node browser_check.mjs <debugPort> <url>
const [port, url] = [process.argv[2], process.argv[3]];
const sleep = ms => new Promise(r => setTimeout(r, ms));

const version = await (await fetch(`http://127.0.0.1:${port}/json/version`)).json().catch(() => null);
if (!version) throw new Error('Chrome is not listening on the debugging port');
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

await send('Runtime.enable');
await send('Page.enable');
await send('Emulation.setDeviceMetricsOverride', {width: 1400, height: 900, deviceScaleFactor: 1, mobile: false});
await send('Page.navigate', {url});
await sleep(4000);

const checks = [];
const check = (name, ok, detail = '') => checks.push({name, ok: Boolean(ok), detail});

// Create a draft from the loaded snapshot.
await evaluate(`(async () => { for (let i = 0; i < 100 && document.getElementById('curate').disabled; i++) await new Promise(r => setTimeout(r, 100)); document.getElementById('curate').click(); })()`);
await sleep(2500);
const drafted = await evaluate(`document.querySelectorAll('g.node').length`);
check('draft created and nodes drawn', drafted > 0, `nodes: ${drafted}`);

// Plain click selects one node; the inspector names it.
const first = await evaluate(`(() => { const g = document.querySelector('g.node'); g.dispatchEvent(new MouseEvent('click', {bubbles: true})); return g.querySelector('title')?.textContent ?? null; })()`);
await sleep(300);
const single = await evaluate(`document.getElementById('inspector').textContent`);
check('click selects a node and the inspector names it', single.startsWith('Node '), single.split('\n')[0]);

// Ctrl-click adds a second node: the inspector lists two.
await evaluate(`(() => { const g = [...document.querySelectorAll('g.node')][1]; g.dispatchEvent(new MouseEvent('click', {bubbles: true, ctrlKey: true})); })()`);
await sleep(300);
const multi = await evaluate(`document.getElementById('inspector').textContent`);
check('ctrl-click adds a node to the selection', multi.startsWith('2 nodes selected'), multi.split('\n')[0]);

// Escape clears the selection.
await evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', {key: 'Escape'}))`);
await sleep(300);
const cleared = await evaluate(`document.getElementById('inspector').textContent`);
check('escape clears the selection', cleared.startsWith('Nothing selected'), cleared.split('\n')[0]);

// A note on a selected node appears in the inspector.
await evaluate(`(() => { const g = document.querySelector('g.node'); g.dispatchEvent(new MouseEvent('click', {bubbles: true})); })()`);
await sleep(300);
await evaluate(`(() => { const input = document.getElementById('note-text'); input.value = 'browser check note'; document.getElementById('note-add').click(); })()`);
await sleep(2000);
const noted = await evaluate(`document.getElementById('inspector').textContent`);
check('a note is added to the selected node', noted.includes('browser check note'), noted.split('\n').filter(l => l.includes('note')).join(' | '));

// The lock and pin buttons follow the selection.
const pinEnabled = await evaluate(`!document.getElementById('inspector-pin').disabled`);
check('pin button is enabled for a single selected node', pinEnabled);

check('no page errors', errors.length === 0, errors.slice(0, 3).join(' | '));

ws.close();
for (const c of checks) console.log(`${c.ok ? 'PASS' : 'FAIL'}  ${c.name}${c.detail ? `  (${c.detail})` : ''}`);
console.log(`first node: ${first}`);
process.exit(checks.every(c => c.ok) ? 0 : 1);
