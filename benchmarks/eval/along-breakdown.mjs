// Usage: node benchmarks/eval/along-breakdown.mjs; needs target/release/layout-command.
// Where the length along the flow goes, area by area, from the stored layered plans and the boxes: the rank bands (the
// longest element of each rank along the flow), the channels between ranks, the ranks of border nodes with their gaps,
// and the rest of the area's length (header, padding, ends). The layered datasets, every area in the view's default
// direction. Nested areas are counted in each area, so the shares describe the areas, not one drawing.
import {readFileSync} from 'node:fs';
import {arqlet, ROOT} from './lib.mjs';
const LAYERED = ['activity-order', 'arqlet-pipeline', 'bpmn-pools', 'cloud-architecture', 'deployment-cluster', 'er-diagram',
  'layerweave-code', 'network-vlan', 'org-chart', 'petri-net', 'state-order', 'state-tcp', 'synthetic-2k', 'uml-class', 'workflow-dag'];
const B = '\u0001dummy:border:';
const T = {length: 0, bands: 0, channels: 0, borderRanks: 0, margins: 0};
console.log('| dataset | length along | rank bands | channels between ranks | border ranks and their gaps | margins (header, padding, ends) |');
console.log('|---|---|---|---|---|---|');
for (const name of LAYERED) {
  const raw = JSON.parse(readFileSync(`${ROOT}benchmarks/datasets/${name}.json`, 'utf8')); const graph = raw.graph ?? raw;
  const first = arqlet({op: 'import', graph, limit: 150}).snapshot.graph;
  const d = ['lr', 'tb'].includes(first.defaultDirection) ? first.defaultDirection : 'lr';
  const ids = new Set(first.nodes.map(n => n.parentId).filter(Boolean));
  const view = {...first, defaultDirection: d, containerDirections: Object.fromEntries([...ids].map(i => [i, d]))};
  const s = arqlet({op: 'import', graph: view, limit: view.nodes.length}).snapshot;
  const S = {length: 0, bands: 0, channels: 0, borderRanks: 0, margins: 0};
  for (const [area, plan] of Object.entries(s.layeredPlans ?? {})) {
    const V = plan.vertical; const rect = id => plan.dummies?.[id] ?? s.boxes[id];
    const along = r => V ? [r.y, r.height] : [r.x, r.width];
    const ranks = plan.ranks.map(rank => { const rs = rank.map(rect).filter(Boolean).map(along); if (!rs.length) return null; return {s: Math.min(...rs.map(x => x[0])), e: Math.max(...rs.map(x => x[0] + x[1])), border: rank.every(id => id.startsWith(B))}; }).filter(Boolean);
    if (!ranks.length) continue;
    const box = area === '<root>' ? null : s.boxes[area];
    const extent = box ? (V ? box.height : box.width) : Math.max(...ranks.map(r => r.e)) - Math.min(...ranks.map(r => r.s));
    let bands = 0, channels = 0, borders = 0;
    for (let i = 0; i < ranks.length; i++) {
      const r = ranks[i];
      if (r.border) borders += r.e - r.s; else bands += r.e - r.s;
      if (i) { const gap = Math.max(0, r.s - ranks[i - 1].e); if (r.border || ranks[i - 1].border) borders += gap; else channels += gap; }
    }
    const margins = Math.max(0, extent - bands - channels - borders);
    S.length += extent; S.bands += bands; S.channels += channels; S.borderRanks += borders; S.margins += margins;
  }
  for (const k of Object.keys(T)) T[k] += S[k];
  const p = v => `${Math.round(v)} (${Math.round(100 * v / S.length)} %)`;
  console.log(`| ${name} | ${Math.round(S.length)} | ${p(S.bands)} | ${p(S.channels)} | ${p(S.borderRanks)} | ${p(S.margins)} |`);
}
const p = v => `${Math.round(v)} (${Math.round(100 * v / T.length)} %)`;
console.log(`| **sum** | ${Math.round(T.length)} | ${p(T.bands)} | ${p(T.channels)} | ${p(T.borderRanks)} | ${p(T.margins)} |`);
