// Usage: node benchmarks/eval/hierarchy-mode.mjs [--limit 150] [--json OUT] [DATASET ...];
// needs target/release/layout-command and `npm ci` in benchmarks/eval (elk-rs).
//
// Issue #119 (placement: nodes far out at the edges, tall layered views). How a hierarchical view is layered decides
// where an area's first rank and its long edges go. Arqlet lays every area out on its own and arranges the areas among
// themselves, which is what ELK calls SEPARATE_CHILDREN; the elk-spacing comparison asked elk-rs for INCLUDE_CHILDREN,
// one global layering of all leaves across the hierarchy. This measures Arqlet's two existing levers against that:
//   default          every area on its own, areas ranked among themselves (today);
//   globalLayering   an expanded child container of the same flow axis takes part in its parent's ranks (experimental);
//   packComponents   the weakly connected components of a container's children are packed as boxes (experimental).
// elk-rs (INCLUDE_CHILDREN) is the reference. Every arm is read back and scored by the same `metrics` operation, so the
// quality gates are comparable: sibling overlaps and line–node hits first, then crossings, then bends, then length and
// area (the order of `DEFAULT_OBJECTIVES`).
import {readFileSync, writeFileSync, existsSync} from 'node:fs';
import {join} from 'node:path';
import ElkRs from '@openedges/elk-layout';
import {arqlet, normalizeArqlet, metrics, ROOT} from './lib.mjs';
import {elkLayout} from './elk-adapter.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : fallback; };
const LIMIT = Number(option('--limit', 150));
const JSON_OUT = option('--json', null);
// The layered datasets of benchmarks/datasets, as the other area probes select them.
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'can-bus', 'cloud-architecture', 'deployment-cluster',
  'er-diagram', 'layerweave-code', 'mixed-large', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp',
  'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;

const ARMS = [
  ['default', {}],
  ['globalLayering', {globalLayering: true}],
  ['packComponents', {packComponents: {}}],
];
const extent = d => {
  const xs = d.nodes.flatMap(n => [n.x, n.x + n.w]), ys = d.nodes.flatMap(n => [n.y, n.y + n.h]);
  return {w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys)};
};
// `crossings` is the sum the other engine comparisons report; the two parts are kept apart because they fail
// differently: `crossed` are routes of unrelated edges crossing, `shared` are routes that share an endpoint crossing
// each other (a fan at one node drawn in the wrong order).
const score = d => {
  const m = metrics(d), e = extent(d);
  return {overlaps: m.siblingOverlaps, hits: m.lineNodeHits, crossings: m.crossings + m.sharedEndpointCrossings,
    crossed: m.crossings, shared: m.sharedEndpointCrossings,
    bends: m.bends, length: Math.round(m.length), width: Math.round(e.w), height: Math.round(e.h),
    area: Math.round(e.w * e.h)};
};
// The uniform task of the elk-spacing comparison: one direction for every area, no per-container strategies or routing,
// no declared buses (the ELK form carries none, so no arm gets them).
const pose = name => {
  const graph = JSON.parse(readFileSync(join(ROOT, 'benchmarks/datasets', `${name}.json`), 'utf8'));
  delete graph.buses; delete graph.defaultRouting; delete graph.containerRouting;
  graph.containerDirections = {}; graph.containerStrategies = {}; graph.defaultStrategy = 'none';
  if (!['lr', 'tb'].includes(graph.defaultDirection)) graph.defaultDirection = 'lr';
  return graph;
};

const rows = [];
console.log('| dataset | arm | overlaps | hits | crossings | crossed | shared-endpoint | bends | length | width × height | area (k) | ms |');
console.log('|---|---|---|---|---|---|---|---|---|---|---|---|');
for (const name of selected) {
  if (!existsSync(join(ROOT, 'benchmarks/datasets', `${name}.json`))) continue;
  const graph = pose(name);
  const view = arqlet({op: 'import', graph, limit: LIMIT}).snapshot.graph;
  const row = {name, nodes: view.nodes.length, edges: view.edges.filter(e => e.target !== null).length};
  for (const [arm, options] of ARMS) {
    try {
      const r = arqlet({op: 'import', graph: {...view}, limit: view.nodes.length, options});
      row[arm] = {...score(normalizeArqlet(r.snapshot)), ms: Math.round(r.ms * 10) / 10};
    } catch (error) { row[arm] = {failed: String(error.message ?? error).split('\n')[0].slice(0, 160)}; }
  }
  try {
    const started = performance.now();
    const laid = await elkLayout(structuredClone(view), new ElkRs());
    row.elkRs = {...score(laid), ms: Math.round((performance.now() - started) * 10) / 10};
  } catch (error) { row.elkRs = {failed: String(error.message ?? error).split('\n')[0].slice(0, 160)}; }
  rows.push(row);
  for (const arm of [...ARMS.map(a => a[0]), 'elkRs']) {
    const a = row[arm];
    console.log(a.failed ? `| ${name} | ${arm} | failed: ${a.failed} | | | | | | | | | |`
      : `| ${name} | ${arm} | ${a.overlaps} | ${a.hits} | ${a.crossings} | ${a.crossed} | ${a.shared} | ${a.bends} | ${a.length} | ${a.width} × ${a.height} | ${Math.round(a.area / 1000)} | ${a.ms} |`);
  }
}
const names = [...ARMS.map(a => a[0]), 'elkRs'];
const ok = rows.filter(r => names.every(a => !r[a].failed));
const sum = (arm, k) => ok.reduce((s, r) => s + r[arm][k], 0);
console.log(`\n**Sums over the ${ok.length} views every arm laid out.**\n`);
console.log('| arm | overlaps | hits | crossings | crossed | shared-endpoint | bends | length | area (k) | area / elk-rs (median) |');
console.log('|---|---|---|---|---|---|---|---|---|---|');
for (const arm of names) {
  const rs = ok.map(r => r[arm].area / r.elkRs.area).sort((a, b) => a - b);
  console.log(`| ${arm} | ${sum(arm, 'overlaps')} | ${sum(arm, 'hits')} | ${sum(arm, 'crossings')} | ${sum(arm, 'crossed')} | ${sum(arm, 'shared')} | ${sum(arm, 'bends')} | ${Math.round(sum(arm, 'length'))} | ${Math.round(sum(arm, 'area') / 1000)} | ${arm === 'elkRs' ? 1 : rs[rs.length >> 1].toFixed(2)} |`);
}
if (JSON_OUT) writeFileSync(JSON_OUT, JSON.stringify({kind: 'hierarchy-mode', limit: LIMIT, rows}, null, 1) + '\n');
