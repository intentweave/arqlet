// Usage: node benchmarks/eval/isolated-grid.mjs <graph.json> [clusters=12]; needs target/release/layout-command.
// Prints one table row per cluster: size, area (M), diagonal, bends, crossings, lines through nodes; off, then isolatedGrid.
// Structured excerpts of real clusters with and without isolatedGrid: size, bends, crossings, lines through nodes.
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {overviewModel, structuredExcerpt, resolvedEdgeIds} from '../../packages/viewer/overview.js';
const ROOT = new URL('../../', import.meta.url).pathname;
const cmd = ROOT + 'target/release/layout-command';
const run = r => JSON.parse(execFileSync(cmd, {input: JSON.stringify(r), maxBuffer: 1 << 30}).toString());
const file = process.argv[2];
const graph = JSON.parse(readFileSync(file, 'utf8'));
const ov = run({op: 'overview', graph});
const model = overviewModel(ov.overview ?? ov, resolvedEdgeIds?.(graph));
const o = model.overview;
// Clusters of 20..300 nodes below, whose direct children are mostly leaves with few edges among them.
const below = i => { let k = 0, s = [i]; while (s.length) { const c = s.pop(); for (const d of model.children[c]) { k++; s.push(d); } } return k; };
const cands = [];
for (let i = 0; i < o.ids.length; i++) {
  const kids = model.children[i]; if (kids.length < 15) continue;
  const n = below(i); if (n > 300) continue;
  cands.push([i, n]);
}
cands.sort((a, b) => b[1] - a[1]);
const metric = s => {
  const boxes = s.nodes.filter(n => !n.container); let bends = 0, hits = 0;
  for (const r of s.routes) { bends += Math.max(0, r.points.length - 2);
    for (let k = 1; k < r.points.length; k++) { const a = r.points[k - 1], b = r.points[k];
      const [x0, x1] = [Math.min(a.x, b.x), Math.max(a.x, b.x)], [y0, y1] = [Math.min(a.y, b.y), Math.max(a.y, b.y)];
      for (const n of boxes) { if (n.id === r.source || n.id === r.target) continue; const q = n.bounds;
        if (x1 > q.x + .5 && x0 < q.x + q.width - .5 && y1 > q.y + .5 && y0 < q.y + q.height - .5) hits++; } } }
  return {w: s.bounds.width, h: s.bounds.height, bends, crossings: s.crossings.length, hits, routes: s.routes.length};
};
let total = {off: 0, on: 0};
for (const [i, n] of cands.slice(0, Number(process.argv[3] ?? 12))) {
  const {graph: ex} = structuredExcerpt(graph, model, o.ids[i]);
  const row = {};
  for (const [k, options] of [['off', undefined], ['on', {isolatedGrid: true}]]) {
    const im = run({op: 'import', graph: ex, limit: ex.nodes.length, ...(options ? {options} : {})});
    if (!im.ok) { row[k] = im.error; continue; }
    row[k] = metric(run({op: 'scene', snapshot: im.snapshot}).scene);
  }
  const f = m => typeof m === "string" ? m : `${Math.round(m.w)}×${Math.round(m.h)} | ${(m.w * m.h / 1e6).toFixed(2)} | ${Math.round(Math.hypot(m.w, m.h))} | ${m.bends} | ${m.crossings} | ${m.hits}`;
  console.log(`| ${n} | `+"`"+`${o.ids[i].replace("file:", "")}`+"`"+` | ${f(row.off)} | ${f(row.on)} |`);
}
