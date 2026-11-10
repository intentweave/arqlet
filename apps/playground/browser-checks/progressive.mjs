// Latency of progressive refinement in a real Chrome over the DevTools protocol. No npm dependencies.
// Runs the browser-only mode (the refinement runs in the background worker), records when the first improvement
// is shown and when the final result is shown, and prints one JSON object. Timings are only meaningful on an idle
// machine; record the load average with them. Not part of CI.
// usage: node progressive.mjs <debugPort> <url>
// The optional third argument selects a demo example (its id in generated/examples); the default graph may have
// nothing to improve, which would make the timings meaningless.
const [port, url, example] = [process.argv[2], process.argv[3], process.argv[4]];
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

const timing = await evaluate(`(async () => {
  const wait = ms => new Promise(r => setTimeout(r, ms));
  const example = ${JSON.stringify(example ?? null)};
  // Automatic refinement would optimize the example while it loads; the measured click must start from the raw layout.
  document.getElementById('auto').checked = false;
  document.getElementById('auto').dispatchEvent(new Event('change'));
  if (example) {
    document.getElementById('example').value = example;
    document.getElementById('example').dispatchEvent(new Event('change'));
    await wait(4000);
  }
  for (let i = 0; i < 100 && document.getElementById('refine').disabled; i++) await wait(100);
  document.getElementById('mode').value = 'browser';
  const start = performance.now();
  document.getElementById('refine').click();
  let firstShownMs = null, finalMs = null, lastText = '', textChanges = 0;
  const deadline = start + 180000;
  while (performance.now() < deadline) {
    await wait(10);
    const text = document.getElementById('backend').textContent;
    if (text !== lastText) { textChanges++; lastText = text; }
    const shown = text.match(/First improvement shown after (\\d+) ms/);
    if (shown && firstShownMs === null) firstShownMs = Number(shown[1]);
    if (/layouts checked|Refinement complete|Stale refinement|Refinement:/.test(text)) { finalMs = Math.round(performance.now() - start); break; }
  }
  return {firstShownMs, finalMs, textChanges, finalText: lastText};
})()`);

const load = await evaluate(`navigator.hardwareConcurrency`);
// Per shown publication: computeMs (request to emission in the worker), transferMs (emission to page), displayMs (page
// receipt to drawn) and shownMs (request to drawn), with the payload size in characters.
const timeline = await evaluate(`globalThis.lwRefineTimeline ?? []`);
console.log(JSON.stringify({url, chromeVersion: version.Browser, hardwareConcurrency: load, ...timing, timeline, errors}));
ws.close();
process.exit(errors.length || timing.finalMs === null ? 1 : 0);
