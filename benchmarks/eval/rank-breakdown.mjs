// Usage: node benchmarks/eval/rank-breakdown.mjs [--limit 150] [--options JSON] [DATASET ...]; needs target/release/layout-command.
// Where the extent across the flow goes, area by area, from the stored layered plans (ranks with their long-edge dummies)
// and the boxes: per area, its extent across the flow of its ranks against the tightest packing of its widest rank, which
// is broken down into the sizes of its nodes and dummies and the least gaps the block placement keeps between neighbours
// (node–node and node–dummy: `nodeSpacing`, 24; dummy–dummy: a track, 6). The rest of the extent is alignment slack: the
// blocks that keep edges straight are not packed tight. The layered datasets, every area in the view's default direction.
import {readFileSync, existsSync} from 'node:fs';
import {arqlet, ROOT} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : fallback; };
const LIMIT = Number(option('--limit', 150));
const OPTIONS = JSON.parse(option('--options', '{}'));
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'cloud-architecture', 'deployment-cluster', 'er-diagram',
  'layerweave-code', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp', 'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;
const DUMMY = '\u0001dummy:';
const total = {extent: 0, nodes: 0, dummies: 0, nodeGaps: 0, mixedGaps: 0, dummyGaps: 0, slack: 0};
console.log('| dataset | areas | extent across | nodes | dummies | gaps node–node | gaps node–dummy | gaps dummy–dummy | alignment slack |');
console.log('|---|---|---|---|---|---|---|---|---|');
for (const name of selected) {
  const file = `${ROOT}benchmarks/datasets/${name}.json`;
  if (!existsSync(file)) continue;
  const raw = JSON.parse(readFileSync(file, 'utf8'));
  const graph = raw.graph ?? raw;
  const first = arqlet({op: 'import', graph, limit: LIMIT}).snapshot.graph;
  const d = ['lr', 'tb'].includes(first.defaultDirection) ? first.defaultDirection : 'lr';
  const ids = new Set(first.nodes.map(n => n.parentId).filter(Boolean));
  const view = {...first, defaultDirection: d, containerDirections: Object.fromEntries([...ids].map(i => [i, d]))};
  const s = arqlet({op: 'import', graph: view, limit: view.nodes.length, options: OPTIONS}).snapshot;
  const sum = {extent: 0, nodes: 0, dummies: 0, nodeGaps: 0, mixedGaps: 0, dummyGaps: 0, slack: 0};
  let areas = 0;
  for (const [area, plan] of Object.entries(s.layeredPlans ?? {})) {
    const vertical = plan.vertical;
    const rect = id => plan.dummies?.[id] ?? s.boxes[id];
    const across = r => vertical ? [r.x, r.width] : [r.y, r.height];
    let lo = Infinity, hi = -Infinity, widest = null;
    for (const rank of plan.ranks) {
      const items = rank.map(id => ({id, r: rect(id), dummy: id.startsWith(DUMMY)})).filter(x => x.r);
      if (!items.length) continue;
      let nodes = 0, dummies = 0, nn = 0, nd = 0, dd = 0;
      for (const x of items) { const [a, l] = across(x.r); lo = Math.min(lo, a); hi = Math.max(hi, a + l); if (x.dummy) dummies += l; else nodes += l; }
      for (let i = 1; i < items.length; i++) {
        const a = items[i - 1].dummy, b = items[i].dummy;
        if (a && b) dd += 6; else if (a || b) nd += 24; else nn += 24;
      }
      const packed = nodes + dummies + nn + nd + dd;
      if (!widest || packed > widest.packed) widest = {packed, nodes, dummies, nn, nd, dd};
    }
    if (!widest) continue;
    areas++;
    const extent = hi - lo;
    sum.extent += extent; sum.nodes += widest.nodes; sum.dummies += widest.dummies; sum.nodeGaps += widest.nn;
    sum.mixedGaps += widest.nd; sum.dummyGaps += widest.dd; sum.slack += Math.max(0, extent - widest.packed);
  }
  for (const k of Object.keys(total)) total[k] += sum[k];
  const p = v => `${Math.round(v)} (${Math.round(100 * v / sum.extent)} %)`;
  console.log(`| ${name} | ${areas} | ${Math.round(sum.extent)} | ${p(sum.nodes)} | ${p(sum.dummies)} | ${p(sum.nodeGaps)} | ${p(sum.mixedGaps)} | ${p(sum.dummyGaps)} | ${p(sum.slack)} |`);
}
const p = v => `${Math.round(v)} (${Math.round(100 * v / total.extent)} %)`;
console.log(`| **sum** | | ${Math.round(total.extent)} | ${p(total.nodes)} | ${p(total.dummies)} | ${p(total.nodeGaps)} | ${p(total.mixedGaps)} | ${p(total.dummyGaps)} | ${p(total.slack)} |`);
