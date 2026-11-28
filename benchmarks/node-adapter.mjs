#!/usr/bin/env node
// The native Node module (crates/node, ADR 0030) against the WASM engine in a `worker_threads` worker, on the same machine:
// time per operation on generated graphs of a few sizes, and memory. Each engine and size runs in a fresh child process
// (peak resident memory is per process); every operation is run `--repeat` times after one warm-up, and the median is
// reported. Times are round trips as a host sees them: the native call includes JSON.stringify and JSON.parse on the main
// thread; the worker's includes the structured clone of request and answer and the JSON inside the worker.
//
//   cargo build --release -p arqlet-node --features query --locked
//   cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --features query --locked
//   ARQLET_FEATURES=query node benchmarks/node-adapter.mjs [--sizes 100,1000,4000] [--repeat 5] > results.jsonl
//
// Also measured: the peak resident memory of one session's import alone (`--peak`), and four sessions laid out at once on the native module (libuv's pool, default 4 threads) against the same
// four one after the other, and the same four in the one WASM worker.
import {execFileSync} from 'node:child_process';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {Worker} from 'node:worker_threads';

const root = join(import.meta.dirname, '..');
const args = process.argv.slice(2);
const option = (name, fallback) => (args.includes(name) ? args[args.indexOf(name) + 1] : fallback);
const repeat = Number(option('--repeat', 5));
const addon = join(root, 'target/release', {darwin: 'libarqlet_node.dylib', win32: 'arqlet_node.dll'}[process.platform] ?? 'libarqlet_node.so');
const wasmFile = join(root, 'target/wasm32-unknown-unknown/release/arqlet_wasm.wasm');

// A deterministic graph of `n` nodes in containers of ten, with cycles and parallel edges (as packages/node's tests).
function graph(n, seed = 1) {
  let s = seed;
  const next = () => (s = (s * 1103515245 + 12345) % 2147483648) / 2147483648;
  const groups = Math.ceil(n / 10);
  const nodes = [
    ...Array.from({length: groups}, (_, g) => ({id: `g${g}`, kind: 'group', label: `Group ${g}`})),
    ...Array.from({length: n}, (_, i) => ({id: `n${i}`, kind: 'service', label: `Node ${i}`, parentId: `g${Math.floor(i / 10)}`})),
  ];
  const edges = Array.from({length: Math.round(n * 1.3)}, (_, i) => ({
    id: `e${i}`, kind: 'calls', source: `n${Math.floor(next() * n)}`, target: `n${Math.floor(next() * n)}`,
  }));
  return {nodes, edges};
}

const median = xs => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];

// The WASM engine in a worker: requests by message, sessions by handle (the protocol of engine-client's worker.js).
function wasmWorker() {
  const source = `
    import {parentPort} from 'node:worker_threads';
    import {readFileSync} from 'node:fs';
    import {createEngine} from ${JSON.stringify(pathToFileURL(join(root, 'packages/engine-client/wasm-client.js')).href)};
    const run = await createEngine(readFileSync(${JSON.stringify(wasmFile)}));
    const sessions = new Map(); let next = 0;
    parentPort.on('message', ({id, session, request, open, close}) => {
      if (open) { const k = ++next; sessions.set(k, run.openSession()); parentPort.postMessage({id, answer: {ok: true, session: k}}); return; }
      if (close) { sessions.get(session).close(); sessions.delete(session); parentPort.postMessage({id, answer: {ok: true}}); return; }
      const answer = session ? sessions.get(session).run(request) : run(request);
      parentPort.postMessage({id, answer});
    });
    parentPort.postMessage({ready: true});`;
  const worker = new Worker(new URL(`data:text/javascript,${encodeURIComponent(source)}`));
  const waiting = new Map();
  let next = 0;
  const ready = new Promise(resolve => worker.once('message', resolve));
  worker.on('message', ({id, answer}) => { if (id) { waiting.get(id)(answer); waiting.delete(id); } });
  const post = message => new Promise(resolve => { const id = ++next; waiting.set(id, resolve); worker.postMessage({id, ...message}); });
  const run = request => post({request});
  run.openSession = async () => {
    const {session} = await post({open: true});
    return {run: request => post({session, request}), close: () => post({session, close: true})};
  };
  run.ready = ready;
  run.end = () => worker.terminate();
  return run;
}

async function engineOf(kind) {
  if (kind === 'native') {
    const {loadNativeEngine} = await import(pathToFileURL(join(root, 'packages/node/index.js')).href);
    const run = loadNativeEngine({path: addon});
    run.end = () => {};
    return run;
  }
  const run = wasmWorker();
  await run.ready;
  return run;
}

async function timed(step) {
  const t = performance.now();
  const answer = await step();
  const ms = performance.now() - t;
  if (!answer.ok) throw Error(answer.error);
  return {ms, answer};
}

// One engine, one size: the operations, each `repeat` times after a warm-up, in a fresh process.
async function child(kind, n) {
  const run = await engineOf(kind);
  const g = graph(n);
  const limit = Math.min(n + Math.ceil(n / 10), 10000);
  const out = {kind, nodes: n, limit};
  const sample = async (name, step) => {
    await step();
    const times = [];
    for (let i = 0; i < repeat; i++) times.push((await timed(step)).ms);
    out[name] = Number(median(times).toFixed(2));
  };
  await sample('dispatch import', () => run({op: 'import', graph: g, limit}));
  const session = await run.openSession();
  await sample('session import', () => session.run({op: 'import', graph: g, limit, scene: false}));
  let collapsed = false;
  await sample('session toggle', () => { collapsed = !collapsed; return session.run({op: 'toggle', nodeId: 'g0', scene: false}); });
  await sample('session scene', () => session.run({op: 'scene'}));
  const memory = await session.run({op: 'memory'});
  out.sessionEstimate = memory.total;
  out.heapBytes = memory.heapBytes ?? null;
  out.linearMemoryBytes = memory.linearMemoryBytes ?? null;
  await session.close();
  // Four sessions of this size: one after the other, then all at once.
  const four = await Promise.all([1, 2, 3, 4].map(() => run.openSession()));
  const graphs = [1, 2, 3, 4].map(seed => graph(n, seed));
  let t = performance.now();
  for (let i = 0; i < 4; i++) await four[i].run({op: 'import', graph: graphs[i], limit, scene: false});
  out['4 sessions serial'] = Number((performance.now() - t).toFixed(2));
  t = performance.now();
  await Promise.all(four.map((s, i) => s.run({op: 'import', graph: graphs[i], limit, scene: false})));
  out['4 sessions at once'] = Number((performance.now() - t).toFixed(2));
  for (const s of four) await s.close();
  // The peak resident memory of this process (the worker's linear memory included), in bytes.
  out.maxRssBytes = process.resourceUsage().maxRSS * 1024;
  run.end();
  return out;
}

// The peak resident memory of one session's import alone, in a fresh process: `--peak native|wasm N` (WASM here on the
// main thread, without a worker).
if (args[0] === '--peak') {
  const [kind, n] = [args[1], Number(args[2])];
  let run;
  if (kind === 'native') run = (await import(pathToFileURL(join(root, 'packages/node/index.js')).href)).loadNativeEngine({path: addon});
  else {
    const {createEngine} = await import(pathToFileURL(join(root, 'packages/engine-client/wasm-client.js')).href);
    run = await createEngine((await import('node:fs')).readFileSync(wasmFile));
  }
  const base = process.resourceUsage().maxRSS;
  const session = await run.openSession();
  const answer = await session.run({op: 'import', graph: graph(n), limit: Math.min(n + Math.ceil(n / 10), 10000), scene: false});
  console.log(JSON.stringify({peak: kind, nodes: n, ok: answer.ok, baseRssBytes: base * 1024, maxRssBytes: process.resourceUsage().maxRSS * 1024}));
  process.exit(0);
}

if (args[0] === '--child') {
  console.log(JSON.stringify(await child(args[1], Number(args[2]))));
  process.exit(0);
}

const sizes = option('--sizes', '100,1000,4000').split(',').map(Number);
const sh = (cmd, a) => { try { return execFileSync(cmd, a, {encoding: 'utf8', timeout: 10000}).trim(); } catch { return 'unknown'; } };
const meta = {
  revision: sh('git', ['-C', root, 'rev-parse', '--short', 'HEAD']),
  machine: process.platform === 'darwin' ? `${sh('sysctl', ['-n', 'hw.model'])}, ${sh('sysctl', ['-n', 'machdep.cpu.brand_string'])}, ${sh('sysctl', ['-n', 'hw.ncpu'])} cores, ${Math.round(Number(sh('sysctl', ['-n', 'hw.memsize'])) / 2 ** 30)} GB` : `${process.platform}-${process.arch}`,
  node: process.version,
  build: `cargo --release (lto thin): arqlet-node cdylib and arqlet-wasm wasm32-unknown-unknown${process.env.ARQLET_FEATURES ? `, features ${process.env.ARQLET_FEATURES}` : ''}`,
  threadpool: process.env.UV_THREADPOOL_SIZE ?? '4 (default)',
  repeat,
  loadBefore: sh('uptime', []),
  addonSha: sh('shasum', ['-a', '256', addon]).slice(0, 16),
  wasmSha: sh('shasum', ['-a', '256', wasmFile]).slice(0, 16),
};
console.log(JSON.stringify({meta}));
for (const n of sizes) {
  for (const kind of ['native', 'wasm-worker']) {
    // A time limit per child, and one retry: freshly started processes have hung on the measuring machine.
    let line = null;
    for (let attempt = 0; attempt < 2 && !line; attempt++) {
      try {
        line = execFileSync(process.execPath, [import.meta.filename, '--child', kind, String(n), '--repeat', String(repeat)],
          {encoding: 'utf8', timeout: 2400000, maxBuffer: 1 << 28}).trim();
      } catch (e) { console.error(`${kind} ${n}: attempt ${attempt + 1} failed: ${e.message.split('\n')[0]}`); }
    }
    console.log(line ?? JSON.stringify({kind, nodes: n, failed: true}));
  }
}
for (const n of sizes) {
  for (const kind of ['native', 'wasm']) {
    let line = null;
    for (let attempt = 0; attempt < 2 && !line; attempt++) {
      try {
        line = execFileSync(process.execPath, [import.meta.filename, '--peak', kind, String(n)], {encoding: 'utf8', timeout: 900000}).trim();
      } catch (e) { console.error(`peak ${kind} ${n}: attempt ${attempt + 1} failed: ${e.message.split('\n')[0]}`); }
    }
    console.log(line ?? JSON.stringify({peak: kind, nodes: n, failed: true}));
  }
}
console.log(JSON.stringify({loadAfter: sh('uptime', [])}));
