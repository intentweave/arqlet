// ADR 0019, step 2: where the time of a request goes when the engine runs as WASM (in Node, as in a browser worker).
// The same requests as crates/runtime/examples/protocol_timing.rs (import collapsed from depth 1, a toggle, the scene, the
// overview). Per request, the medians of `reps` runs after one warm-up of: JSON.stringify of the request, encoding to
// UTF-8 and copying into WASM memory, lw_run (parse, handle and write inside WASM), and decoding and JSON.parse of the
// answer; and, separately, a round trip of the request object and of the answer object to a worker thread
// (structured clone, as between a page and the engine's worker).
//
//   node benchmarks/protocol/measure-wasm.mjs <arqlet_wasm.wasm> <graph.json> [reps]
import {readFileSync} from 'node:fs';
import {Worker} from 'node:worker_threads';
import {performance} from 'node:perf_hooks';

const [wasmPath, graphPath, repsArg] = process.argv.slice(2);
const reps = Number(repsArg ?? 5);
const graph = JSON.parse(readFileSync(graphPath, 'utf8'));
const nodes = graph.nodes.length;
const {instance} = await WebAssembly.instantiate(readFileSync(wasmPath), {env: {lw_publish: () => {}}});
const api = instance.exports;
const encoder = new TextEncoder(), decoder = new TextDecoder();
const median = v => [...v].sort((a, b) => a - b)[Math.floor(v.length / 2)];

function once(request) {
  let t = performance.now();
  const text = JSON.stringify(request);
  const stringify = performance.now() - t;
  t = performance.now();
  const encoded = encoder.encode(text);
  const ptr = api.lw_alloc(encoded.length);
  new Uint8Array(api.memory.buffer, ptr, encoded.length).set(encoded);
  const copyIn = performance.now() - t;
  t = performance.now();
  const result = api.lw_run(ptr, encoded.length);
  const run = performance.now() - t;
  const out = Number(result & 0xffffffffn), len = Number(result >> 32n);
  t = performance.now();
  const answer = JSON.parse(decoder.decode(new Uint8Array(api.memory.buffer, out, len)));
  const parse = performance.now() - t;
  api.lw_free(ptr, encoded.length);
  api.lw_free(out, len);
  return {ms: {stringify, copyIn, lwRun: run, decodeParse: parse}, answer, bytes: [encoded.length, len]};
}

// A worker that sends every message back: the round trip is two structured clones.
const echo = new Worker('const {parentPort}=require("node:worker_threads");parentPort.on("message",m=>parentPort.postMessage(m));', {eval: true});
const roundTrip = value => new Promise(resolve => {
  const t = performance.now();
  echo.once('message', () => resolve(performance.now() - t));
  echo.postMessage(value);
});

async function measure(name, request) {
  const first = once(request);
  const parts = {};
  const clone = {request: [], answer: []};
  for (let i = 0; i < reps; i++) {
    const {ms} = once(request);
    for (const [k, v] of Object.entries(ms)) (parts[k] ??= []).push(v);
    clone.request.push(await roundTrip(request));
    clone.answer.push(await roundTrip(first.answer));
  }
  const ms = Object.fromEntries(Object.entries(parts).map(([k, v]) => [k, median(v)]));
  ms.total = Object.values(ms).reduce((a, b) => a + b, 0);
  ms.workerRoundTripRequest = median(clone.request);
  ms.workerRoundTripAnswer = median(clone.answer);
  console.log(JSON.stringify({engine: 'wasm-node', request: name, nodes, reps, requestBytes: first.bytes[0], answerBytes: first.bytes[1], ms}));
  return first.answer;
}

const limit = Math.min(nodes, 10000);
const imported = await measure('import', {op: 'import', graph, limit, options: {levelOfDetail: {depth: 1}}});
const snapshot = imported.snapshot;
const visible = new Set(snapshot.graph.nodes.map(n => n.id));
const container = snapshot.collapsedIds.find(c => visible.has(c));
await measure('toggle', {op: 'toggle', snapshot, nodeId: container, expectedRevision: snapshot.revision, expectedLayoutVersion: snapshot.layoutVersion});
await measure('intent', {op: 'intent', snapshot});
await measure('scene', {op: 'scene', snapshot});
await measure('overview', {op: 'overview', graph});
await echo.terminate();
