#!/usr/bin/env node
// Throughput baseline in WASM (#249, step 1): the session messages of `throughput.py` through the raw module
// (`lw_session_open`/`lw_session_run`, the path of the engine-client worker), in Node, one thread. Reads the graphs
// `throughput.py` wrote (`benchmarks/datasets/throughput-<n>-s<seed>.json`, run it first for the sizes) and checks their
// SHA-256 against its results file when one is given.
//
// Per repetition a fresh child process (fresh module and linear memory) with a time limit, retried once. Per message:
// wall time around `lw_session_run` (text in and out, no JSON value on the engine side for the import), the module's
// `timings` (clock `performance.now`), the linear memory after it (WASM memory only grows, so this is the high-water
// mark so far) and the Node process's resident memory. Rows are appended to --out (JSONL); a complete repetition there
// is skipped.
//
//   cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --locked
//   node benchmarks/scaling/throughput-wasm.mjs --sizes 100000 300000 1000000 --reps 3 --out benchmarks/results/throughput-1m-DATE-wasm.jsonl
import {readFileSync, appendFileSync, existsSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {loadavg} from 'node:os';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';

const ROOT = fileURLToPath(new URL('../../', import.meta.url));
const FIRST_VIEW = {levelOfDetail: {nodes: 1000, edges: 3000}, liftEdges: true};
const argv = process.argv.slice(2);
const opt = (name, fallback) => {
  const i = argv.indexOf(`--${name}`);
  if (i < 0) return fallback;
  const values = [];
  for (let j = i + 1; j < argv.length && !argv[j].startsWith('--'); j++) values.push(argv[j]);
  return values;
};
const sizes = opt('sizes', ['100000', '300000', '1000000']).map(Number);
const reps = Number(opt('reps', ['3'])[0]);
const seed = Number(opt('seed', ['1'])[0]);
const timeout = Number(opt('timeout', ['1800'])[0]);
const budget = Number(opt('overview-budget', ['1000000000'])[0]);
const maxLoad = Number(opt('max-load', ['4'])[0]);
const maxWait = Number(opt('max-wait', ['1200'])[0]);
const noOverview = argv.includes('--no-overview');
// `--overview-only`: a fresh session that only lays out the overview (first and continued by id); its repetitions are
// recorded as mode `wasm-overview`. At 1M the overview after an import in the same session runs out of the 4 GiB of
// linear memory (see the results file), so the two are measured apart there.
const overviewOnly = argv.includes('--overview-only');
// `--no-import-timed`: leaves out the second import with `timings` (it reads the request as a JSON value, which at 1M
// does not fit into the 4 GiB of linear memory after the session's earlier messages; see the results file).
const noImportTimed = argv.includes('--no-import-timed');
// `--import-again`: only the import, twice in the same instance (mode `wasm-import-again`): tells the first import's
// cost apart from the growth of a fresh instance's linear memory.
const importAgain = argv.includes('--import-again');
const MODE = overviewOnly ? 'wasm-overview' : argv.includes('--import-again') ? 'wasm-import-again' : 'wasm';
const out = opt('out', [null])[0];
const wasmPath = opt('wasm', [join(ROOT, 'target/wasm32-unknown-unknown/release/arqlet_wasm.wasm')])[0];
const load = () => loadavg().map(x => Math.round(x * 100) / 100);
const write = row => {
  appendFileSync(out, JSON.stringify(row) + '\n');
  const {estimate, ...shown} = row;
  console.log(JSON.stringify(shown).slice(0, 400));
};

// The same edit targets as `throughput.py` (`plan`): a file for the small patch, 20 files for the large one.
function plan(graph) {
  const children = new Map();
  for (const n of graph.nodes) if (n.parentId) (children.get(n.parentId) ?? children.set(n.parentId, []).get(n.parentId)).push(n.id);
  const sized = graph.nodes.filter(n => n.kind === 'file' && (children.get(n.id)?.length ?? 0) >= 2).map(n => n.id);
  const patch = (targets, perFile, edgesPerNode, tag) => {
    const nodes = [], edges = [];
    for (const f of targets) {
      const fresh = Array.from({length: perFile}, (_, k) => `${f}/${tag}${k}`);
      nodes.push(...fresh.map(i => ({id: i, kind: 'symbol', label: i.slice(i.lastIndexOf('/') + 1), parentId: f})));
      const old = children.get(f);
      fresh.forEach((i, k) => {
        for (let j = 0; j < edgesPerNode; j++) {
          const target = j === 0 ? fresh[(k + 1) % fresh.length] : old[(k * 7 + j) % old.length];
          edges.push({id: `${tag}-e${edges.length}`, kind: 'call', source: i, target});
        }
      });
    }
    return {nodes, edges};
  };
  const small = patch(sized.slice(0, 1), 5, 2, 'ps');
  small.edges = small.edges.slice(0, 8);
  return [small, patch(sized.slice(1, 21), 100, 2, 'pl')];
}

async function child(n, rep) {
  const file = join(ROOT, `benchmarks/datasets/throughput-${n}-s${seed}.json`);
  const text = readFileSync(file, 'utf8');
  const sha = createHash('sha256').update(text).digest('hex');
  const graph = JSON.parse(text);
  const [small, large] = plan(graph);
  const {instance} = await WebAssembly.instantiate(readFileSync(wasmPath),
    {env: {lw_now: () => performance.now(), lw_publish: () => {}}});
  const api = instance.exports;
  const encoder = new TextEncoder(), decoder = new TextDecoder();
  const sid = api.lw_session_open();
  const message = (name, body) => {
    const before = load();
    const started = performance.now();
    let answer;
    try {
      const encoded = encoder.encode(body);
      const ptr = api.lw_alloc(encoded.length);
      new Uint8Array(api.memory.buffer, ptr, encoded.length).set(encoded);
      const result = api.lw_session_run(sid, ptr, encoded.length);
      const at = Number(result & 0xffffffffn), len = Number(result >> 32n);
      answer = decoder.decode(new Uint8Array(api.memory.buffer, at, len));
      api.lw_free(ptr, encoded.length);
      api.lw_free(at, len);
    } catch (e) {
      const row = {mode: MODE, size: n, rep, message: name, ok: false, error: String(e).slice(0, 300),
        seconds: +((performance.now() - started) / 1000).toFixed(3), load: before,
        linearMemoryBytes: api.memory.buffer.byteLength};
      write(row);
      throw e;
    }
    const seconds = (performance.now() - started) / 1000;
    const value = JSON.parse(answer);
    const row = {mode: MODE, size: n, rep, message: name, seconds: +seconds.toFixed(3), messageBytes: body.length,
      answerBytes: answer.length, ok: value.ok, load: before, linearMemoryBytes: api.memory.buffer.byteLength,
      nodeResidentBytes: process.memoryUsage().rss};
    if (!value.ok) row.error = String(value.error).slice(0, 300);
    if (value.timings) row.timings = value.timings;
    if (name === 'memory') row.estimate = {total: value.total, parts: value.parts};
    if (name.startsWith('overview') && value.ok) row.overview = {stats: value.overview.stats, ids: value.overview.ids.length,
      work: Object.fromEntries(Object.entries(value.overview.work ?? {}).map(([k, v]) => [k, Array.isArray(v) ? v.length : v]))};
    write(row);
    return value;
  };
  const withGraph = request => JSON.stringify(request).slice(0, -1) + ',"graph":' + text + '}';
  const overview = () => {
    const o = message('overview-first', withGraph({op: 'overview', threads: 1, workBudget: budget, timings: true}));
    if (o.ok) {
      const ov = o.overview;
      const nodes = Object.fromEntries(ov.ids.map((id, k) => [id, {x: ov.x[k], y: ov.y[k], r: ov.r[k]}]));
      message('overview-byid', JSON.stringify({op: 'overview', threads: 1, workBudget: budget, start: {nodes}, timings: true}));
    }
  };
  if (overviewOnly) {
    overview();
    write({mode: MODE, size: n, rep, message: 'repDone', graphSha256: sha});
    return;
  }
  const first = message('import', withGraph({op: 'import', limit: 10000, options: FIRST_VIEW}));
  if (importAgain) {
    message('import-again', withGraph({op: 'import', limit: 10000, options: FIRST_VIEW}));
    write({mode: MODE, size: n, rep, message: 'repDone', graphSha256: sha});
    return;
  }
  const shown = new Set((first.scene?.nodes ?? []).map(x => x.id));
  const collapsed = first.collapsedIds ?? [];
  const target = collapsed.find(c => shown.has(c));
  if (target) {
    message('toggle-open', JSON.stringify({op: 'toggle', nodeId: target, timings: true}));
    message('toggle-close', JSON.stringify({op: 'toggle', nodeId: target, scene: false, timings: true}));
  }
  const hidden = new Set(collapsed);
  const parent = new Map(graph.nodes.map(x => [x.id, x.parentId]));
  const underCollapsed = id => { for (let p = parent.get(id); p; p = parent.get(p)) if (hidden.has(p)) return true; return false; };
  const symbol = graph.nodes.find(x => x.kind === 'symbol' && underCollapsed(x.id))?.id;
  if (symbol) message('reveal', JSON.stringify({op: 'reveal', nodeIds: [symbol], timings: true}));
  message('columns', JSON.stringify({op: 'columns', timings: true}));
  message('patch-small', JSON.stringify({op: 'patch', patch: small, timings: true}));
  message('patch-large', JSON.stringify({op: 'patch', patch: large, timings: true}));
  message('memory', JSON.stringify({op: 'memory'}));
  if (!noOverview) overview();
  if (!noImportTimed) message('import-timed', withGraph({op: 'import', limit: 10000, options: FIRST_VIEW, timings: true}));
  write({mode: MODE, size: n, rep, message: 'repDone', graphSha256: sha, target, revealed: symbol,
    smallPatch: [small.nodes.length, small.edges.length], largePatch: [large.nodes.length, large.edges.length]});
}

if (argv[0] === '--child') {
  await child(Number(argv[1]), Number(argv[2]));
} else {
  if (!out) { console.error('--out FILE is required'); process.exit(2); }
  if (!existsSync(wasmPath)) { console.error(`Missing ${wasmPath}; build it first (see the header).`); process.exit(2); }
  const done = new Set();
  if (existsSync(out)) for (const line of readFileSync(out, 'utf8').split('\n').filter(Boolean)) {
    const r = JSON.parse(line);
    if (r.message === 'repDone' && r.mode === MODE) done.add(`${r.size}|${r.rep}`);
  }
  const git = (...a) => spawnSync('git', ['-C', ROOT, ...a], {encoding: 'utf8'}).stdout.trim();
  write({machine: {cpu: spawnSync('sysctl', ['-n', 'machdep.cpu.brand_string'], {encoding: 'utf8'}).stdout.trim(),
    node: process.version, revision: git('rev-parse', '--short', 'HEAD'), dirty: Boolean(git('status', '--porcelain', '--untracked-files=no')),
    build: 'wasm32-unknown-unknown release', load: load(), utc: new Date().toISOString()}, wasm: wasmPath.replace(ROOT, ''),
    firstView: FIRST_VIEW, seed, overviewBudget: budget, argv});
  for (const n of sizes) for (let rep = 1; rep <= reps; rep++) {
    if (done.has(`${n}|${rep}`)) continue;
    for (const attempt of [1, 2]) {
      let waited = 0;
      while (loadavg()[0] >= maxLoad && waited < maxWait) { spawnSync('sleep', ['15']); waited += 15; }
      write({mode: MODE, size: n, rep, message: 'repStart', attempt, loadBefore: load(), waitedSeconds: waited});
      const r = spawnSync(process.execPath, ['--max-old-space-size=12000', fileURLToPath(import.meta.url), '--child', String(n), String(rep), ...argv],
        {stdio: ['ignore', 'inherit', 'inherit'], timeout: timeout * 1000, killSignal: 'SIGKILL'});
      const finished = readFileSync(out, 'utf8').split('\n').filter(Boolean).map(l => JSON.parse(l))
        .some(x => x.message === 'repDone' && x.mode === MODE && x.size === n && x.rep === rep);
      if (finished) break;
      write({mode: MODE, size: n, rep, message: 'repFailed', attempt, status: r.status, signal: r.signal});
    }
  }
  console.log(`Wrote ${out}`);
}
