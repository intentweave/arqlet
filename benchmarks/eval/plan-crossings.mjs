// Usage: node benchmarks/eval/plan-crossings.mjs [--limit 150] [DATASET ...]; needs target/release/layout-command.
//
// Issue #119, second step. Under `globalLayering` the drawings carry many times the default's crossings, and nearly all
// of them are counted as shared-endpoint crossings (routes that share a node crossing each other). This probe separates
// the two places such a crossing can come from:
//
//   plan crossings    the crossings the stored rank order already implies, before any geometry: for every section of
//                     every chain between neighbouring ranks, the pairs whose ends are in opposite order (sections that
//                     share an end do not count, as `layered_plan::order_crossings` has it). This is what the ordering
//                     phase is responsible for.
//   drawn crossings   what `metrics` counts on the finished routes, and how far from the nodes they lie. A fan drawn in
//                     the wrong order at a node crosses next to it; long chains that change lane cross anywhere.
//
// Reading both together says whether a crossing regression belongs to the ordering phase or to the ports and routing.
import {readFileSync, existsSync} from 'node:fs';
import {join} from 'node:path';
import {arqlet, normalizeArqlet, metrics, ROOT} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => { const i = args.indexOf(name); return i >= 0 ? args.splice(i, 2)[1] : fallback; };
const LIMIT = Number(option('--limit', 150));
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'can-bus', 'cloud-architecture', 'deployment-cluster',
  'er-diagram', 'layerweave-code', 'mixed-large', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp',
  'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;
const ARMS = [['default', {}], ['globalLayering', {globalLayering: true}]];
const EPS = 1e-9;

// The crossings the plans' rank order implies, summed over the areas.
const planCrossings = snapshot => {
  let total = 0, areas = 0, sections = 0;
  for (const plan of Object.values(snapshot.layeredPlans ?? {})) {
    areas++;
    const place = new Map();
    plan.ranks.forEach((r, ri) => r.forEach((id, ci) => place.set(id, [ri, ci])));
    const byRank = new Map();
    for (const chain of plan.chains ?? []) {
      const path = [chain.source, ...(chain.dummies ?? []), chain.target];
      for (let i = 0; i + 1 < path.length; i++) {
        sections++;
        const a = place.get(path[i]), b = place.get(path[i + 1]);
        if (!a || !b || b[0] !== a[0] + 1) continue;
        if (!byRank.has(a[0])) byRank.set(a[0], []);
        byRank.get(a[0]).push([a[1], b[1], path[i], path[i + 1]]);
      }
    }
    for (const list of byRank.values()) {
      for (let i = 0; i < list.length; i++) {
        for (let j = i + 1; j < list.length; j++) {
          const [u1, l1, a1, b1] = list[i], [u2, l2, a2, b2] = list[j];
          if (a1 === a2 || b1 === b2) continue;
          if ((u1 - u2) * (l1 - l2) < 0) total++;
        }
      }
    }
  }
  return {total, areas, sections};
};

// Where the crossings of routes that share a node lie, as a distance from the shared node's box.
const sharedCrossingSpread = drawing => {
  const N = Object.fromEntries(drawing.nodes.map(n => [n.id, n]));
  const segs = e => e.points.slice(1).map((q, i) => [e.points[i], q]);
  const meet = (a, b, c, d) => {
    const d1 = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    const d2 = (b.x - a.x) * (d.y - a.y) - (b.y - a.y) * (d.x - a.x);
    const d3 = (d.x - c.x) * (a.y - c.y) - (d.y - c.y) * (a.x - c.x);
    const d4 = (d.x - c.x) * (b.y - c.y) - (d.y - c.y) * (b.x - c.x);
    if (!(d1 * d2 < -EPS && d3 * d4 < -EPS)) return null;
    const t = d3 / (d3 - d4);
    return {x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t};
  };
  const shared = (p, q) => [p.source, p.target].find(id => id === q.source || id === q.target);
  const away = [];
  for (let i = 0; i < drawing.edges.length; i++) {
    for (let j = i + 1; j < drawing.edges.length; j++) {
      const p = drawing.edges[i], q = drawing.edges[j];
      const id = shared(p, q);
      if (!id) continue;
      const n = N[id];
      for (const [a, b] of segs(p)) {
        for (const [c, d] of segs(q)) {
          const at = meet(a, b, c, d);
          if (!at || !n) continue;
          away.push(Math.max(Math.max(n.x - at.x, at.x - (n.x + n.w), 0),
            Math.max(n.y - at.y, at.y - (n.y + n.h), 0)));
        }
      }
    }
  }
  away.sort((a, b) => a - b);
  const q = f => Math.round(away[Math.min(away.length - 1, Math.floor(f * away.length))] ?? 0);
  return {count: away.length, median: q(0.5), p90: q(0.9), near: away.filter(v => v <= 50).length};
};

console.log('| dataset | arm | areas | sections | plan crossings | drawn crossed | drawn shared-endpoint | shared: median distance | p90 | within 50 |');
console.log('|---|---|---|---|---|---|---|---|---|---|');
for (const name of selected) {
  const file = join(ROOT, 'benchmarks/datasets', `${name}.json`);
  if (!existsSync(file)) continue;
  const graph = JSON.parse(readFileSync(file, 'utf8'));
  delete graph.buses; delete graph.defaultRouting; delete graph.containerRouting;
  graph.containerDirections = {}; graph.containerStrategies = {}; graph.defaultStrategy = 'none';
  if (!['lr', 'tb'].includes(graph.defaultDirection)) graph.defaultDirection = 'lr';
  const view = arqlet({op: 'import', graph, limit: LIMIT}).snapshot.graph;
  for (const [arm, options] of ARMS) {
    try {
      const snapshot = arqlet({op: 'import', graph: {...view}, limit: view.nodes.length, options}).snapshot;
      const p = planCrossings(snapshot);
      const drawing = normalizeArqlet(snapshot);
      const m = metrics(drawing);
      const s = sharedCrossingSpread(drawing);
      console.log(`| ${name} | ${arm} | ${p.areas} | ${p.sections} | ${p.total} | ${m.crossings} | ${m.sharedEndpointCrossings} | ${s.median} | ${s.p90} | ${s.near} |`);
    } catch (error) {
      console.log(`| ${name} | ${arm} | failed: ${String(error.message ?? error).split('\n')[0].slice(0, 90)} | | | | | | | |`);
    }
  }
}
