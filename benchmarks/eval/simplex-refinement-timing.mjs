// Usage: node benchmarks/eval/simplex-refinement-timing.mjs --out FILE.jsonl [--reps 9] [--bench PATH] [DATASET ...];
// needs target/release/layout-command (the views) and the release `bench` built with `--features benchmark-timings`.
//
// ADR 0027, step 1: refinement times with and without `simplexRanksCandidate`, per view of both poses (the views
// `option-gates.mjs` lays out at limit 150), one thread, the two arms of a view one after the other. One JSON line per
// view with `bench`'s import and refinement times and the 1-minute load average before each arm. A run that is stopped
// continues where it was: views already in the file are not measured again. The configured `mixed-large` runs the
// pipeline before single pass (no candidate, seconds per refinement): it is measured once.
import {readFileSync, writeFileSync, appendFileSync, existsSync, mkdtempSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {loadavg, tmpdir} from 'node:os';
import {join} from 'node:path';
import {arqlet, ROOT} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : fallback; };
const OUT = option('--out', null);
const REPS = option('--reps', '9');
const BENCH = option('--bench', join(ROOT, 'target/release/bench'));
const LIMIT = 150;
if (!OUT || !existsSync(BENCH)) throw new Error('--out FILE and a built bench are needed');
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'can-bus', 'cloud-architecture', 'deployment-cluster',
  'er-diagram', 'layerweave-code', 'mixed-large', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp',
  'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;

const load = (pose, name) => {
  const raw = JSON.parse(readFileSync(join(ROOT, 'benchmarks/datasets', `${name}.json`), 'utf8'));
  const graph = raw.graph ?? raw;
  if (pose === 'uniform') {
    delete graph.buses; delete graph.defaultRouting; delete graph.containerRouting;
    graph.containerDirections = {}; graph.containerStrategies = {}; graph.defaultStrategy = 'none';
    if (!['lr', 'tb'].includes(graph.defaultDirection)) graph.defaultDirection = 'lr';
  }
  return graph;
};

const views = mkdtempSync(join(tmpdir(), 'arqlet-views-'));
const done = new Set((existsSync(OUT) ? readFileSync(OUT, 'utf8').split('\n').filter(Boolean) : [])
  .map(line => { const r = JSON.parse(line); return `${r.pose} ${r.view}`; }));
for (const pose of ['uniform', 'configured']) {
  for (const name of selected) {
    if (done.has(`${pose} ${name}`)) continue;
    const first = arqlet({op: 'import', graph: load(pose, name), limit: LIMIT}).snapshot;
    const base = pose === 'configured' ? first.options ?? {} : {};
    const file = join(views, `${pose}-${name}.json`);
    writeFileSync(file, JSON.stringify(first.graph));
    const nodes = first.graph.nodes.length;
    const reps = pose === 'configured' && name === 'mixed-large' ? '1' : REPS;
    const row = {pose, view: name, nodes, reps: Number(reps)};
    for (const [arm, extra] of [['off', []], ['on', ['--refine-options', '{"simplexRanksCandidate":true}']]]) {
      const before = loadavg()[0];
      const r = spawnSync(BENCH, ['--graph', file, '--limit', String(nodes), '--reps', reps, '--threads', '1', '--name', name,
        '--import-options', JSON.stringify(base), ...extra], {encoding: 'utf8', timeout: 300000, killSignal: 'SIGKILL', maxBuffer: 1 << 28});
      if (r.status !== 0) { row[arm] = {error: (r.stderr || String(r.error)).trim().split('\n').pop().slice(0, 160)}; continue; }
      const j = JSON.parse(r.stdout.trim().split('\n').pop());
      row[arm] = {importMs: j.importMs, refineMs: j.refineMs['1'], strategy: j.optimization.strategy, loadBefore: Math.round(before * 10) / 10};
    }
    appendFileSync(OUT, JSON.stringify(row) + '\n');
    console.log(JSON.stringify(row));
  }
}
