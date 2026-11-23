// Usage: node benchmarks/eval/border-channel.mjs [--limit 150] [--pose uniform|native] [--gaps 0,6,12,18] [--json OUT]
//        [DATASET ...]; needs target/release/layout-command and `npm ci` in benchmarks/eval (elk-rs, uniform pose only).
//
// Needs the prototype patch applied first: `git apply benchmarks/experiments/border-channel-gap-20261123.patch` (the option was
// measured not to pay and is not part of the engine; see `placement-hierarchy-20261123.md`, fifth step).
//
// Issue #119, lever 1: the experimental option `borderChannelGap` lets a channel beside a border rank in which no edge
// changes lane narrow below the floor every channel keeps (two margins and a track, 30). This runs the quality gates for
// the default and for each gap, in `DEFAULT_OBJECTIVES` order: sibling overlaps, line–node hits, crossings, bends, then
// length and area; plus the routes single pass delegated to the search and whether single pass ran at all.
//   --pose uniform   the elk-rs comparison's task (one direction, no strategies, routing or buses); elk-rs is measured
//   --pose native    the datasets as they are configured (their directions, strategies and buses): the drawings users get
import {readFileSync, writeFileSync, existsSync} from 'node:fs';
import {join} from 'node:path';
import ElkRs from '@openedges/elk-layout';
import {arqlet, normalizeArqlet, metrics, ROOT} from './lib.mjs';
import {elkLayout} from './elk-adapter.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : fallback; };
const LIMIT = Number(option('--limit', 150));
const POSE = option('--pose', 'uniform');
const GAPS = option('--gaps', '0,6,12,18').split(',').map(Number);
const JSON_OUT = option('--json', null);
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'can-bus', 'cloud-architecture', 'deployment-cluster',
  'er-diagram', 'layerweave-code', 'mixed-large', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp',
  'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;
const ARMS = [['default', {}], ...GAPS.map(g => [`gap ${g}`, {borderChannelGap: g}])];

const extent = d => {
  const xs = d.nodes.flatMap(n => [n.x, n.x + n.w]), ys = d.nodes.flatMap(n => [n.y, n.y + n.h]);
  return {w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys)};
};
const score = d => {
  const m = metrics(d), e = extent(d);
  return {overlaps: m.siblingOverlaps, hits: m.lineNodeHits, crossings: m.crossings + m.sharedEndpointCrossings,
    bends: m.bends, length: Math.round(m.length), width: Math.round(e.w), height: Math.round(e.h), area: Math.round(e.w * e.h)};
};
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

const rows = [];
console.log(`pose ${POSE}, limit ${LIMIT}\n`);
console.log('| dataset | arm | single pass | delegated | overlaps | hits | crossings | bends | length | width × height | area (k) | ms |');
console.log('|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|---:|');
for (const name of selected) {
  if (!existsSync(join(ROOT, 'benchmarks/datasets', `${name}.json`))) continue;
  const graph = load(name);
  const first = arqlet({op: 'import', graph, limit: LIMIT});
  const view = first.snapshot.graph;
  const row = {name};
  for (const [arm, options] of ARMS) {
    try {
      // The native pose keeps the view's own options (directions, strategies, routing) and adds the arm's.
      const base = POSE === 'native' ? first.snapshot.options ?? {} : {};
      const r = arqlet({op: 'import', graph: {...view}, limit: view.nodes.length, options: {...base, ...options}});
      const diag = r.snapshot.layoutDiagnostics ?? [];
      const sp = diag.find(d => d.startsWith('Single pass: '));
      const delegated = sp ? Number(sp.match(/(\d+) delegated/)?.[1] ?? 0) : null;
      row[arm] = {...score(normalizeArqlet(r.snapshot)), singlePass: Boolean(sp), delegated, ms: Math.round(r.ms * 10) / 10};
    } catch (error) { row[arm] = {failed: String(error.message ?? error).split('\n')[0].slice(0, 120)}; }
  }
  if (POSE === 'uniform') {
    try { row.elkRs = score(await elkLayout(structuredClone(view), new ElkRs())); } catch { row.elkRs = null; }
  }
  rows.push(row);
  for (const [arm] of ARMS) {
    const a = row[arm];
    console.log(a.failed ? `| ${name} | ${arm} | failed: ${a.failed} | | | | | | | | | |`
      : `| ${name} | ${arm} | ${a.singlePass ? 'yes' : '**no**'} | ${a.delegated ?? '–'} | ${a.overlaps} | ${a.hits} | ${a.crossings} | ${a.bends} | ${a.length} | ${a.width} × ${a.height} | ${Math.round(a.area / 1000)} | ${a.ms} |`);
  }
}
const names = ARMS.map(a => a[0]);
const ok = rows.filter(r => names.every(a => !r[a].failed));
const sum = (arm, k) => ok.reduce((s, r) => s + (r[arm][k] ?? 0), 0);
const median = a => { const s = a.slice().sort((x, y) => x - y); return s[s.length >> 1]; };
console.log(`\n**Sums over the ${ok.length} views every arm laid out (pose ${POSE}).**\n`);
console.log('| arm | single pass | delegated | overlaps | hits | crossings | bends | length | area (k) | area / default (median) |' +
  (POSE === 'uniform' ? ' area / elk-rs (median) |' : ''));
console.log('|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|' + (POSE === 'uniform' ? '---:|' : ''));
for (const arm of names) {
  const sp = ok.filter(r => r[arm].singlePass).length;
  const toDefault = median(ok.map(r => r[arm].area / r.default.area)).toFixed(3);
  const elk = ok.filter(r => r.elkRs);
  const toElk = POSE === 'uniform' ? ` ${median(elk.map(r => r[arm].area / r.elkRs.area)).toFixed(2)} |` : '';
  console.log(`| ${arm} | ${sp}/${ok.length} | ${sum(arm, 'delegated')} | ${sum(arm, 'overlaps')} | ${sum(arm, 'hits')} | ${sum(arm, 'crossings')} | ${sum(arm, 'bends')} | ${sum(arm, 'length')} | ${Math.round(sum(arm, 'area') / 1000)} | ${toDefault} |${toElk}`);
}
// Views where an arm is worse than the default on a gate above area (in objective order), so a regression cannot hide in
// the sums.
console.log('\n**Views worse than the default on a gate above area**\n');
for (const arm of names.slice(1)) {
  const worse = ok.filter(r => ['overlaps', 'hits', 'crossings', 'bends'].some(k => r[arm][k] > r.default[k]))
    .map(r => `${r.name} (${['overlaps', 'hits', 'crossings', 'bends'].filter(k => r[arm][k] !== r.default[k]).map(k => `${k} ${r.default[k]}→${r[arm][k]}`).join(', ')})`);
  console.log(`- ${arm}: ${worse.length ? worse.join('; ') : 'none'}`);
}
if (JSON_OUT) writeFileSync(JSON_OUT, JSON.stringify({kind: 'border-channel', pose: POSE, limit: LIMIT, rows}, null, 1) + '\n');
