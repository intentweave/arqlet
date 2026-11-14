// Usage: node benchmarks/eval/area-breakdown.mjs [--limit 150] [--json] [DATASET ...]; needs target/release/layout-command
// and `npm ci` in benchmarks/eval (elk-rs).
// Where the drawing area goes, Arqlet against elk-rs on the same task: the layered datasets of benchmarks/datasets, the
// view the default selects (bounded to `limit` nodes) with every area set to the view's default direction (elk-rs takes no
// per-container directions across a hierarchy), both engines on that view. Each drawing is broken down from its geometry
// alone, so both are measured the same way:
//   fill        the leaves' area over the drawing's (width × height);
//   along gap   the drawing's extent along the flow less the part the leaves cover when projected on that axis
//               (rank gaps and channels), as a share of the extent;
//   across gap  the same across the flow (gaps within ranks);
//   ranks       leaves grouped by their position along the flow (within half a leaf);
//   rank gap    the mean free distance between neighbouring ranks along the flow;
//   rank pitch  the mean free distance between neighbouring leaves of one rank across the flow;
//   containers  the containers' area less the hull of their children, over the drawing's (margins and headers).
import {readFileSync, existsSync} from 'node:fs';
import {arqlet, normalizeArqlet, metrics, ROOT} from './lib.mjs';
import {elkLayout} from './elk-adapter.mjs';
import ElkRs from '@openedges/elk-layout';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const LIMIT = Number(option('--limit', 150));
// `--options JSON`: layout options for Arqlet (for example smaller base gaps), measured beside the default.
const EXTRA = JSON.parse(option('--options', 'null'));
const JSON_OUT = args.includes('--json') && (args.splice(args.indexOf('--json'), 1), true);
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'cloud-architecture', 'deployment-cluster', 'er-diagram',
  'layerweave-code', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp', 'synthetic-2k', 'uml-class', 'workflow-dag'];
const selected = args.length ? args : LAYERED;

function union(intervals) {
  const sorted = intervals.slice().sort((a, b) => a[0] - b[0]);
  let total = 0, end = -Infinity, start = -Infinity;
  for (const [a, b] of sorted) {
    if (a > end) { if (end > start) total += end - start; start = a; end = b; } else end = Math.max(end, b);
  }
  if (end > start) total += end - start;
  return total;
}

export function breakdown(out, vertical) {
  const leaves = out.nodes.filter(n => !n.container);
  const containers = out.nodes.filter(n => n.container);
  if (!leaves.length) return null;
  const lo = (n, ax) => ax ? n.y : n.x, len = (n, ax) => ax ? n.h : n.w;
  const xs = out.nodes.flatMap(n => [n.x, n.x + n.w]), ys = out.nodes.flatMap(n => [n.y, n.y + n.h]);
  const W = Math.max(...xs) - Math.min(...xs), H = Math.max(...ys) - Math.min(...ys);
  const along = vertical, across = !vertical; // axis flags: true = y
  const extentAlong = vertical ? H : W, extentAcross = vertical ? W : H;
  const coverAlong = union(leaves.map(n => [lo(n, along), lo(n, along) + len(n, along)]));
  const coverAcross = union(leaves.map(n => [lo(n, across), lo(n, across) + len(n, across)]));
  // Ranks: leaves whose start along the flow lies within half a mean leaf length of the rank's first.
  const meanAlong = leaves.reduce((s, n) => s + len(n, along), 0) / leaves.length;
  const byStart = leaves.slice().sort((a, b) => lo(a, along) - lo(b, along));
  const ranks = [];
  for (const n of byStart) {
    const r = ranks[ranks.length - 1];
    if (r && lo(n, along) - r.start <= meanAlong / 2) r.nodes.push(n); else ranks.push({start: lo(n, along), nodes: [n]});
  }
  let gapSum = 0, gapCount = 0;
  for (let i = 1; i < ranks.length; i++) {
    const prevEnd = Math.max(...ranks[i - 1].nodes.map(n => lo(n, along) + len(n, along)));
    const start = Math.min(...ranks[i].nodes.map(n => lo(n, along)));
    gapSum += Math.max(0, start - prevEnd); gapCount++;
  }
  let pitchSum = 0, pitchCount = 0;
  for (const r of ranks) {
    const sorted = r.nodes.slice().sort((a, b) => lo(a, across) - lo(b, across));
    for (let i = 1; i < sorted.length; i++) {
      pitchSum += Math.max(0, lo(sorted[i], across) - (lo(sorted[i - 1], across) + len(sorted[i - 1], across))); pitchCount++;
    }
  }
  let containerOverhead = 0;
  for (const c of containers) {
    const kids = out.nodes.filter(n => n.parentId === c.id);
    if (!kids.length) continue;
    const hx0 = Math.min(...kids.map(k => k.x)), hx1 = Math.max(...kids.map(k => k.x + k.w));
    const hy0 = Math.min(...kids.map(k => k.y)), hy1 = Math.max(...kids.map(k => k.y + k.h));
    containerOverhead += c.w * c.h - (hx1 - hx0) * (hy1 - hy0);
  }
  const leafArea = leaves.reduce((s, n) => s + n.w * n.h, 0);
  return {width: Math.round(W), height: Math.round(H), areaK: Math.round(W * H / 1e3), fill: leafArea / (W * H),
    alongGap: 1 - coverAlong / extentAlong, acrossGap: 1 - coverAcross / extentAcross, ranks: ranks.length,
    rankGap: gapCount ? gapSum / gapCount : 0, rankPitch: pitchCount ? pitchSum / pitchCount : 0,
    maxRank: Math.max(...ranks.map(r => r.nodes.length)), containers: containerOverhead / (W * H)};
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const rows = [];
  for (const name of selected) {
    const file = `${ROOT}benchmarks/datasets/${name}.json`;
    if (!existsSync(file)) continue;
    const raw = JSON.parse(readFileSync(file, 'utf8'));
    const graph = raw.graph ?? raw;
    // The view the default selects, then every area in its default direction.
    const first = arqlet({op: 'import', graph, limit: LIMIT}).snapshot.graph;
    const direction = typeof first.defaultDirection === 'string' && ['lr', 'tb'].includes(first.defaultDirection) ? first.defaultDirection : 'lr';
    const containerIds = new Set(first.nodes.map(n => n.parentId).filter(Boolean));
    const view = {...first, defaultDirection: direction, containerDirections: Object.fromEntries([...containerIds].map(id => [id, direction])),
      containerStrategies: {}, defaultStrategy: undefined};
    const vertical = direction === 'tb';
    for (const [engine, options] of [['arqlet', {}], ...(EXTRA ? [[`arqlet ${JSON.stringify(EXTRA)}`, EXTRA]] : [])]) {
      const a = arqlet({op: 'import', graph: view, limit: view.nodes.length, options});
      const ao = normalizeArqlet(a.snapshot);
      const am = metrics(ao);
      rows.push({dataset: name, engine, direction, nodes: view.nodes.length, lineNodeHits: am.lineNodeHits, crossings: am.crossings,
        bends: am.bends, ...breakdown(ao, vertical)});
    }
    const e = await elkLayout(view, new ElkRs());
    const em = metrics(e);
    rows.push({dataset: name, engine: 'elk-rs', direction, nodes: view.nodes.length, lineNodeHits: em.lineNodeHits, crossings: em.crossings,
      bends: em.bends, ...breakdown(e, vertical)});
  }
  if (JSON_OUT) for (const r of rows) console.log(JSON.stringify(r));
  else {
    const p = v => `${Math.round(v * 100)} %`;
    console.log('| dataset | engine | crossings | bends | width × height | area (k) | fill | along gap | across gap | ranks | rank gap | rank pitch | widest rank | containers |');
    console.log('|---|---|---|---|---|---|---|---|---|---|---|---|---|---|');
    for (const r of rows) console.log(`| ${r.dataset} | ${r.engine} | ${r.crossings} | ${r.bends} | ${r.width} × ${r.height} | ${r.areaK} | ${p(r.fill)} | ${p(r.alongGap)} | ${p(r.acrossGap)} | ${r.ranks} | ${Math.round(r.rankGap)} | ${Math.round(r.rankPitch)} | ${r.maxRank} | ${p(r.containers)} |`);
  }
}
