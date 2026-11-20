// Provisional: what the overview built in steps gains (apps/playground/progressive.js): the time of the rough overview
// of the top levels (shown at once) against the whole overview, both through the native `layout-command` (one process
// per request, JSON read and write included), on the synthetic code indexes of benchmarks/overview/generate.py.
//
//   cargo build --release -p arqlet-runtime --bin layout-command
//   node benchmarks/overview/progressive.mjs --sizes 10000 50000 --reps 3 [--out rows.jsonl]
//
// Prints a Markdown table (median and runs per request) and, with --out, one JSON line per size.
import {spawnSync} from 'node:child_process';
import {appendFileSync, mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {coarseGraph} from '../../apps/playground/progressive.js';

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
// The overview page's galaxy forces; the page sends them with every request in that style.
const options = {gravity: 0.05, charge: 0.1, gap: 3};

const run = request => {
  const started = process.hrtime.bigint();
  const done = spawnSync(binary, [], {input: JSON.stringify(request), encoding: 'utf8', maxBuffer: 1 << 30});
  const seconds = Number(process.hrtime.bigint() - started) / 1e9;
  const answer = JSON.parse(done.stdout);
  if (!answer.ok) throw Error(answer.error);
  return {seconds, answer};
};
const median = list => [...list].sort((a, b) => a - b)[Math.floor(list.length / 2)];

const dir = mkdtempSync(join(tmpdir(), 'arqlet-progressive-'));
const rows = [];
try {
  for (const n of sizes) {
    const file = join(dir, `code-${n}.json`);
    const made = spawnSync('python3', ['benchmarks/overview/generate.py', '--nodes', String(n), '--seed', '1', '--out', file], {encoding: 'utf8'});
    if (made.status !== 0) throw Error(made.stderr);
    const graph = JSON.parse(readFileSync(file, 'utf8'));
    const cut = coarseGraph(graph, {limit: 600, radius: 4});
    const rough = [], whole = [];
    let roughNodes = 0, wholeNodes = 0;
    for (let k = 0; k < reps; k++) {
      const r = run({op: 'overview', graph: cut.graph, radii: cut.radii, options, threads: 4});
      rough.push(r.seconds);
      roughNodes = r.answer.overview.ids.length;
      const w = run({op: 'overview', graph, options, threads: 4});
      whole.push(w.seconds);
      wholeNodes = w.answer.overview.ids.length;
    }
    const row = {nodes: graph.nodes.length, edges: graph.edges.length, roughGraphNodes: cut.graph.nodes.length,
      roughEdges: cut.graph.edges.length, cut: cut.cut, roughNodes, wholeNodes, rough, whole};
    rows.push(row);
    if (out) appendFileSync(out, JSON.stringify(row) + '\n');
  }
} finally {
  rmSync(dir, {recursive: true, force: true});
}
console.log('| nodes | edges | rough graph (nodes, edges, levels) | rough overview | runs | whole overview | runs |');
console.log('|---|---|---|---|---|---|---|');
for (const r of rows) {
  const s = list => list.map(v => v.toFixed(2)).join(', ');
  console.log(`| ${r.nodes} | ${r.edges} | ${r.roughGraphNodes}, ${r.roughEdges}, ${r.cut + 1} | ${median(r.rough).toFixed(2)} s | ${s(r.rough)} | ${median(r.whole).toFixed(2)} s | ${s(r.whole)} |`);
}
