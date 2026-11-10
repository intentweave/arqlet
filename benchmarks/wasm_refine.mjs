// WASM-side refinement timing for one frozen dataset, to set beside `bench` (native). Serial, single worker.
// Includes the JSON round trip through the ABI, which is part of what a browser pays.
//
//   node benchmarks/wasm_refine.mjs WASM_FILE GRAPH.json [LIMIT=100] [REPS=3]
//
// Prints one JSON object. Timings are only evidence when the machine is otherwise idle; record the load average.
import {readFileSync} from 'node:fs';
import {arch, cpus, loadavg, platform} from 'node:os';
import {createEngine} from '../packages/engine-client/wasm-client.js';

const [wasmPath, graphPath, limitArg = '100', repsArg = '3'] = process.argv.slice(2);
if (!wasmPath || !graphPath) {
  console.error('Usage: node benchmarks/wasm_refine.mjs WASM_FILE GRAPH.json [LIMIT=100] [REPS=3]');
  process.exit(2);
}
const limit = Number(limitArg);
const reps = Number(repsArg);
const run = await createEngine(readFileSync(wasmPath));
const graph = JSON.parse(readFileSync(graphPath, 'utf8'));
const imported = run({op: 'import', graph, limit});
if (!imported.ok) throw new Error(`import failed: ${JSON.stringify(imported)}`);
const snapshot = imported.snapshot;
const request = {
  op: 'refine',
  snapshot,
  expectedRevision: snapshot.revision,
  expectedLayoutVersion: snapshot.layoutVersion ?? 0,
};
// One unmeasured warm-up, then the timed runs.
if (!run(request).ok) throw new Error('warm-up refinement failed');
const samples = [];
let last;
for (let i = 0; i < reps; i++) {
  const start = performance.now();
  last = run(request);
  samples.push(performance.now() - start);
  if (!last.ok) throw new Error(`refinement failed: ${JSON.stringify(last)}`);
}
samples.sort((a, b) => a - b);
const median = samples.length % 2 ? samples[(samples.length - 1) / 2] : (samples[samples.length / 2 - 1] + samples[samples.length / 2]) / 2;
const round = value => Math.round(value * 1000) / 1000;
console.log(JSON.stringify({
  target: 'wasm32-unknown-unknown',
  dataset: graphPath,
  limit,
  reps,
  refineMs: {medianMs: round(median), minMs: round(samples[0]), maxMs: round(samples.at(-1)), samples: samples.length},
  strategy: last.snapshot.optimization?.strategy ?? null,
  candidatesEvaluated: last.snapshot.optimization?.candidatesEvaluated ?? null,
  environment: {
    node: process.version,
    os: platform(),
    arch: arch(),
    cpus: cpus().length,
    cpuModel: cpus()[0]?.model ?? null,
    loadAverage: loadavg().map(value => round(value)),
  },
}));
