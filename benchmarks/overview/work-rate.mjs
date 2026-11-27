// Provisional (#211): how fast the overview spends its work units, so that a host can turn its time limit into a
// `workBudget`. On the synthetic code indexes of benchmarks/overview/generate.py, per size: the whole overview (a budget
// larger than any work, so `work.used` is all of it), half of that work as the budget, and no budget at all
// (`workBudget: 0`: everything packed, the part of the time the budget does not bound). Natively through
// `layout-command` (one process per request, JSON read and written) and through the WASM engine in this process (one
// thread, the browser's and a Node worker's path). One warm-up, then `--reps` runs; median, min and max. Each row
// records the load average before it.
//
//   cargo build --release -p arqlet-runtime --bin layout-command --locked
//   cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --locked
//   python3 benchmarks/overview/generate.py --nodes 10000 --seed 1 --out benchmarks/datasets/overview-10000.json  (also 50000, 100000)
//   node benchmarks/overview/work-rate.mjs --sizes 10000 50000 100000 --reps 3 --out benchmarks/results/overview-work-budget-<date>.jsonl
import {spawnSync, execFileSync} from 'node:child_process';
import {appendFileSync, readFileSync} from 'node:fs';
import {cpus, loadavg, platform, release} from 'node:os';
import {createEngine} from '../../packages/engine-client/wasm-client.js';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const at = args.indexOf(name);
  if (at < 0) return fallback;
  const out = [];
  for (let k = at + 1; k < args.length && !args[k].startsWith('--'); k++) out.push(args[k]);
  return out;
};
const sizes = option('--sizes', ['10000', '50000']).map(Number);
const reps = Number(option('--reps', ['3'])[0]);
const out = option('--out', [null])[0];
const binary = option('--binary', ['target/release/layout-command'])[0];
const wasmFile = option('--wasm', ['target/wasm32-unknown-unknown/release/arqlet_wasm.wasm'])[0];
const threads = option('--threads', ['1', '4']).map(Number);
const ALL = 2 ** 53 - 1;

const git = (...a) => {
  try {
    return execFileSync('git', a, {encoding: 'utf8'}).trim();
  } catch {
    return null;
  }
};
const machine = {
  platform: `${platform()} ${release()}`,
  cpu: cpus()[0]?.model,
  cores: cpus().length,
  revision: git('rev-parse', '--short', 'HEAD'),
  dirty: Boolean(git('status', '--porcelain', '--untracked-files=no')),
  build: 'release (lto thin); WASM release',
  node: process.version,
};

// A native request, with a time limit and retries: a freshly started process has hung before running on some machines.
const native = request => {
  const body = JSON.stringify(request);
  for (let attempt = 0; ; attempt++) {
    const started = process.hrtime.bigint();
    const done = spawnSync(binary, [], {input: body, encoding: 'utf8', maxBuffer: 1 << 30, timeout: 300000});
    const seconds = Number(process.hrtime.bigint() - started) / 1e9;
    if (done.error || done.status !== 0) {
      if (attempt < 2) continue;
      throw done.error ?? Error(`layout-command exited ${done.status}`);
    }
    const answer = JSON.parse(done.stdout);
    if (!answer.ok) throw Error(answer.error);
    return {seconds, overview: answer.overview};
  }
};
const engine = await createEngine(readFileSync(wasmFile));
const wasm = request => {
  const started = process.hrtime.bigint();
  const answer = engine(request);
  const seconds = Number(process.hrtime.bigint() - started) / 1e9;
  if (!answer.ok) throw Error(answer.error);
  return {seconds, overview: answer.overview};
};
const median = list => [...list].sort((a, b) => a - b)[Math.floor(list.length / 2)];
const timed = (call, request) => {
  call(request);
  const runs = [];
  let overview;
  for (let k = 0; k < reps; k++) {
    const done = call(request);
    runs.push(done.seconds);
    overview = done.overview;
  }
  return {median: median(runs), min: Math.min(...runs), max: Math.max(...runs), work: overview.work, overlaps: overview.stats.overlaps};
};

const rows = [];
for (const n of sizes) {
  const graph = JSON.parse(readFileSync(`benchmarks/datasets/overview-${n}.json`, 'utf8'));
  const engines = [...threads.map(t => [`native, ${t} thread${t > 1 ? 's' : ''}`, native, t]), ['WASM', wasm, 1]];
  for (const [name, call, t] of engines) {
    const whole = {op: 'overview', graph, threads: t, workBudget: ALL};
    const load = loadavg()[0];
    const all = timed(call, whole);
    const used = all.work.used;
    const half = timed(call, {...whole, workBudget: Math.floor(used / 2)});
    const none = timed(call, {...whole, workBudget: 0});
    // Units per second of the budgeted part: the work over the time the budget bounds (whole less nothing budgeted).
    const rate = used / Math.max(all.median - none.median, 1e-9);
    const row = {
      machine, nodes: n, engine: name, threads: t, loadBefore: load, loadAfter: loadavg()[0], reps,
      used, clustering: all.work.clustering, all, half, none,
      unitsPerSecond: Math.round(rate), unitsPerSecondWhole: Math.round(used / all.median),
    };
    rows.push(row);
    if (out) appendFileSync(out, JSON.stringify(row) + '\n');
  }
}
const ms = s => `${Math.round(s * 1000)} ms`;
const range = t => `${ms(t.median)} (${ms(t.min)}–${ms(t.max)})`;
console.log(`Machine: ${machine.cpu}, ${machine.cores} cores, ${machine.platform}; revision ${machine.revision}${machine.dirty ? ' (dirty)' : ''}; ${machine.build}; Node ${machine.node}.`);
console.log('');
console.log('| nodes | engine | load before/after | work used (of which clustering) | whole | half the work as budget | budget 0 | units/s (budgeted part) | units/s (whole) |');
console.log('|---|---|---|---|---|---|---|---|---|');
for (const r of rows) {
  console.log(`| ${r.nodes} | ${r.engine} | ${r.loadBefore.toFixed(1)}/${r.loadAfter.toFixed(1)} | ${r.used} (${r.clustering}) | ${range(r.all)} | ${range(r.half)} (used ${r.half.work.used}, ${r.half.work.packed.length} packed) | ${range(r.none)} (${r.none.work.packed.length} packed) | ${(r.unitsPerSecond / 1e6).toFixed(1)} M | ${(r.unitsPerSecondWhole / 1e6).toFixed(1)} M |`);
}
