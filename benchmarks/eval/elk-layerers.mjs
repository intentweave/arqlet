// Usage: node benchmarks/eval/elk-layerers.mjs [--pose uniform|native] [--limit 150] [--json OUT] [DATASET ...];
// needs target/release/layout-command (views and metrics) and `npm ci` in benchmarks/eval (elk-rs).
//
// ADR 0027, step 2, the cheap upper bound: what do ELK's own width-bounded layerers (`MIN_WIDTH`, `STRETCH_WIDTH`) save
// against ELK's default layering (network simplex), through the pinned elk-rs on the same views and poses? ELK only;
// Arqlet selects the view and scores every drawing (the shared `metrics` operation). The layering option is set on the
// root and on every container. `native` passes the containers' own directions (the adapter ignores strategies and buses).
// Every elk-rs call runs in a process of its own with a time limit (`ELK_TIMEOUT_MS`, default two minutes): a layerer that
// does not finish is reported as failed.
import {readFileSync, writeFileSync, existsSync} from 'node:fs';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import ElkRs from '@openedges/elk-layout';
import {arqlet, metrics, ROOT} from './lib.mjs';
import {elkLayout} from './elk-adapter.mjs';

const args = process.argv.slice(2);
// The child process of one call: `--one` reads the view and the options from standard input and prints the drawing.
if (args[0] === '--one') {
  const {view, options} = JSON.parse(readFileSync(0, 'utf8'));
  process.stdout.write(JSON.stringify(await elkLayout(view, new ElkRs(), options)), () => process.exit(0));
  await new Promise(() => {});
}
const TIMEOUT_MS = Number(process.env.ELK_TIMEOUT_MS ?? 120000);
const layout = (view, options) => {
  const r = spawnSync(process.execPath, [fileURLToPath(import.meta.url), '--one'], {input: JSON.stringify({view, options}),
    encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, timeout: TIMEOUT_MS, killSignal: 'SIGKILL'});
  if (r.error?.code === 'ETIMEDOUT') throw new Error(`did not finish within ${TIMEOUT_MS / 1000} s`);
  if (r.status !== 0) throw new Error((r.stderr || String(r.error)).trim().split('\n').find(l => /Error|Exception/.test(l)) ?? 'failed');
  return JSON.parse(r.stdout);
};
const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : fallback; };
const LIMIT = Number(option('--limit', 150));
const POSE = option('--pose', 'uniform');
const JSON_OUT = option('--json', null);
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'can-bus', 'cloud-architecture', 'deployment-cluster',
  'er-diagram', 'layerweave-code', 'mixed-large', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp',
  'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;
const ARMS = [['default', {}], ['minWidth', {'elk.layered.layering.strategy': 'MIN_WIDTH'}],
  ['stretchWidth', {'elk.layered.layering.strategy': 'STRETCH_WIDTH'}]];
const GATES = ['overlaps', 'hits', 'crossings', 'bends'];

const load = name => {
  const raw = JSON.parse(readFileSync(join(ROOT, 'benchmarks/datasets', `${name}.json`), 'utf8'));
  const graph = raw.graph ?? raw;
  if (POSE === 'uniform') {
    delete graph.buses; delete graph.defaultRouting; delete graph.containerRouting;
    graph.containerDirections = {}; graph.containerStrategies = {}; graph.defaultStrategy = 'none';
    if (!['lr', 'tb'].includes(graph.defaultDirection)) graph.defaultDirection = 'lr';
  }
  return graph;
};
const score = d => {
  const m = metrics(d);
  const xs = d.nodes.flatMap(n => [n.x, n.x + n.w]), ys = d.nodes.flatMap(n => [n.y, n.y + n.h]);
  const width = Math.max(...xs) - Math.min(...xs), height = Math.max(...ys) - Math.min(...ys);
  return {overlaps: m.siblingOverlaps, hits: m.lineNodeHits, crossings: m.crossings + m.sharedEndpointCrossings,
    bends: m.bends, length: Math.round(m.length), width: Math.round(width), height: Math.round(height), area: Math.round(width * height)};
};

const rows = [];
console.log(`elk-rs layerers, pose ${POSE}, limit ${LIMIT}\n`);
console.log('| dataset | layering | overlaps | hits | crossings | bends | length | width × height | area (k) | ms |');
console.log('|---|---|---:|---:|---:|---:|---:|---|---:|---:|');
for (const name of selected) {
  if (!existsSync(join(ROOT, 'benchmarks/datasets', `${name}.json`))) continue;
  const view = arqlet({op: 'import', graph: load(name), limit: LIMIT}).snapshot.graph;
  const row = {name};
  for (const [arm, layoutOptions] of ARMS) {
    try {
      const d = layout(view, {layoutOptions, containerDirections: POSE === 'native'});
      row[arm] = {...score(d), ms: Math.round(d.ms * 10) / 10};
    } catch (error) { row[arm] = {failed: String(error.message ?? error).split('\n')[0].slice(0, 120)}; }
    const a = row[arm];
    console.log(a.failed ? `| ${name} | ${arm} | failed: ${a.failed} | | | | | | | |`
      : `| ${name} | ${arm} | ${a.overlaps} | ${a.hits} | ${a.crossings} | ${a.bends} | ${a.length} | ${a.width} × ${a.height} | ${Math.round(a.area / 1000)} | ${a.ms} |`);
  }
  rows.push(row);
}
// Each layering is compared with the default on the views both of them laid out.
const median = a => { const s = a.slice().sort((x, y) => x - y); return s[s.length >> 1]; };
const noWorse = (a, b) => { for (const k of GATES) if (a[k] !== b[k]) return a[k] < b[k]; return true; };
console.log('\n**Sums per layering over the views it and the default laid out.** `area, choice per view`: the layering\'s drawing');
console.log('where its area is smaller, else the default (an upper bound of the area, whatever it costs above area).\n');
console.log('| layering | views | overlaps | hits | crossings | bends | length | area (k) | default on these views (k) | area / default (median) | views smaller | of them no worse in objective order | area, choice per view | area, choice among those no worse |');
console.log('|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
for (const [arm] of ARMS.slice(1)) {
  const ok = rows.filter(r => !r.default.failed && !r[arm].failed);
  const sum = (a, k) => ok.reduce((s, r) => s + r[a][k], 0);
  const smaller = ok.filter(r => r[arm].area < r.default.area);
  const safe = smaller.filter(r => noWorse(r[arm], r.default));
  const change = area => `${((area / sum('default', 'area') - 1) * 100).toFixed(1)} %`;
  const chosen = ok.reduce((s, r) => s + Math.min(r[arm].area, r.default.area), 0);
  const safely = ok.reduce((s, r) => s + (safe.includes(r) ? r[arm].area : r.default.area), 0);
  const cell = k => `${sum('default', k)} → ${sum(arm, k)}`;
  console.log(`| ${arm} | ${ok.length} | ${cell('overlaps')} | ${cell('hits')} | ${cell('crossings')} | ${cell('bends')} | ${cell('length')} | ${Math.round(sum(arm, 'area') / 1000)} (${change(sum(arm, 'area'))}) | ${Math.round(sum('default', 'area') / 1000)} | ${median(ok.map(r => r[arm].area / r.default.area)).toFixed(3)} | ${smaller.length} | ${safe.length} | ${change(chosen)} | ${change(safely)} |`);
}
if (JSON_OUT) writeFileSync(JSON_OUT, JSON.stringify({kind: 'elk-layerers', pose: POSE, limit: LIMIT, arms: ARMS, rows}, null, 1) + '\n');
