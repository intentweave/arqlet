// Usage: node benchmarks/eval/option-gates.mjs --arms '[["name",{...options}],...]' [--pose uniform|native] [--limit 150]
//        [--refine] [--json OUT] [DATASET ...]; needs target/release/layout-command and `npm ci` in benchmarks/eval (elk-rs).
//
// With `--refine`, every arm (the default arm too) also refines its imported drawing and the refined drawing is scored:
// for options that act in the refinement. The table then has the refinement's strategy and its time.
//
// The quality gates for layout options against the default, in `DEFAULT_OBJECTIVES` order: sibling overlaps, line–node
// hits, crossings, bends, then length and area; plus whether single pass ran and how many routes it delegated to the
// search. Every arm is read back and scored by the same `metrics` operation. A default arm is always measured first.
//   --pose uniform   the elk-rs comparison's task (one direction, no strategies, routing or buses); elk-rs is measured
//   --pose native    the datasets as they are configured (their directions, strategies and buses)
// Views worse than the default on a gate above area are listed by name, so a regression cannot hide in the sums.
import {readFileSync, writeFileSync, existsSync} from 'node:fs';
import {join} from 'node:path';
import ElkRs from '@openedges/elk-layout';
import {arqlet, normalizeArqlet, metrics, ROOT} from './lib.mjs';
import {elkLayout} from './elk-adapter.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : fallback; };
const LIMIT = Number(option('--limit', 150));
const POSE = option('--pose', 'uniform');
const JSON_OUT = option('--json', null);
const REFINE = args.includes('--refine') && Boolean(args.splice(args.indexOf('--refine'), 1));
const ARMS = [['default', {}], ...JSON.parse(option('--arms', '[]'))];
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'can-bus', 'cloud-architecture', 'deployment-cluster',
  'er-diagram', 'layerweave-code', 'mixed-large', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp',
  'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;
const GATES = ['overlaps', 'hits', 'crossings', 'bends'];

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
console.log(`pose ${POSE}, limit ${LIMIT}${REFINE ? ', refined' : ''}\n`);
console.log('| dataset | arm | single pass | delegated | overlaps | hits | crossings | bends | length | width × height | area (k) | ms |' +
  (REFINE ? ' refine ms | strategy |' : ''));
console.log('|---|---|---|---:|---:|---:|---:|---:|---:|---|---:|---:|' + (REFINE ? '---:|---|' : ''));
for (const name of selected) {
  if (!existsSync(join(ROOT, 'benchmarks/datasets', `${name}.json`))) continue;
  const first = arqlet({op: 'import', graph: load(name), limit: LIMIT});
  const view = first.snapshot.graph;
  const row = {name};
  for (const [arm, options] of ARMS) {
    try {
      // The native pose keeps the view's own options and adds the arm's.
      const base = POSE === 'native' ? first.snapshot.options ?? {} : {};
      let r = arqlet({op: 'import', graph: {...view}, limit: view.nodes.length, options: {...base, ...options}});
      let refinement = {};
      if (REFINE) {
        const s = r.snapshot;
        const refined = arqlet({op: 'refine', snapshot: s, expectedRevision: s.revision, expectedLayoutVersion: s.layoutVersion});
        refinement = {refineMs: Math.round(refined.ms * 10) / 10, strategy: refined.snapshot.optimization.strategy};
        r = {snapshot: refined.snapshot, ms: r.ms};
      }
      const sp = (r.snapshot.layoutDiagnostics ?? []).find(d => d.startsWith('Single pass: '));
      row[arm] = {...score(normalizeArqlet(r.snapshot)), singlePass: Boolean(sp),
        delegated: sp ? Number(sp.match(/(\d+) delegated/)?.[1] ?? 0) : null, ms: Math.round(r.ms * 10) / 10, ...refinement};
    } catch (error) { row[arm] = {failed: String(error.message ?? error).split('\n')[0].slice(0, 120)}; }
  }
  if (POSE === 'uniform') {
    try { row.elkRs = score(await elkLayout(structuredClone(view), new ElkRs())); } catch { row.elkRs = null; }
  }
  rows.push(row);
  for (const [arm] of ARMS) {
    const a = row[arm];
    console.log(a.failed ? `| ${name} | ${arm} | failed: ${a.failed} | | | | | | | | | |`
      : `| ${name} | ${arm} | ${a.singlePass ? 'yes' : '**no**'} | ${a.delegated ?? '–'} | ${a.overlaps} | ${a.hits} | ${a.crossings} | ${a.bends} | ${a.length} | ${a.width} × ${a.height} | ${Math.round(a.area / 1000)} | ${a.ms} |` +
        (REFINE ? ` ${a.refineMs} | ${a.strategy} |` : ''));
  }
}
const names = ARMS.map(a => a[0]);
const ok = rows.filter(r => names.every(a => !r[a].failed));
const sum = (arm, k) => ok.reduce((s, r) => s + (r[arm][k] ?? 0), 0);
const median = a => { const s = a.slice().sort((x, y) => x - y); return s[s.length >> 1]; };
const elk = ok.filter(r => r.elkRs);
console.log(`\n**Sums over the ${ok.length} views every arm laid out (pose ${POSE}).**\n`);
console.log('| arm | single pass | delegated | overlaps | hits | crossings | bends | length | area (k) | area / default (median) |' +
  (elk.length ? ' area / elk-rs (median) |' : ''));
console.log('|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|' + (elk.length ? '---:|' : ''));
for (const arm of names) {
  const toDefault = median(ok.map(r => r[arm].area / r.default.area)).toFixed(3);
  const toElk = elk.length ? ` ${median(elk.map(r => r[arm].area / r.elkRs.area)).toFixed(2)} |` : '';
  console.log(`| ${arm} | ${ok.filter(r => r[arm].singlePass).length}/${ok.length} | ${sum(arm, 'delegated')} | ${sum(arm, 'overlaps')} | ${sum(arm, 'hits')} | ${sum(arm, 'crossings')} | ${sum(arm, 'bends')} | ${sum(arm, 'length')} | ${Math.round(sum(arm, 'area') / 1000)} | ${toDefault} |${toElk}`);
}
console.log('\n**Views worse than the default on a gate above area**\n');
for (const arm of names.slice(1)) {
  const worse = ok.filter(r => GATES.some(k => r[arm][k] > r.default[k]))
    .map(r => `${r.name} (${GATES.filter(k => r[arm][k] !== r.default[k]).map(k => `${k} ${r.default[k]}→${r[arm][k]}`).join(', ')})`);
  console.log(`- ${arm}: ${worse.length ? worse.join('; ') : 'none'}`);
}
if (JSON_OUT) writeFileSync(JSON_OUT, JSON.stringify({kind: 'option-gates', pose: POSE, limit: LIMIT, refine: REFINE, arms: ARMS, rows}, null, 1) + '\n');
