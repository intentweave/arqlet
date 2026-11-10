#!/usr/bin/env node
// Measurement step for the layer order: are the crossings already fixed by the order of the ranks?
//
// For every area with a fixed flow (lr, rl, tb, bt), its leaves are grouped into ranks along the flow, and the edges between
// two neighbouring ranks are counted as bilayer crossings. Edges that skip a rank are reported separately (they need dummy
// nodes, which this probe does not model). Two orders are compared with the same rank membership:
//   - current: the order the layout produced (positions across the flow);
//   - barycenter: sweeps that move each node to the mean position of its neighbours on the adjacent rank, as in the
//     Sugiyama method (forward and backward sweeps, 24 rounds, the best order seen is kept).
// The routed crossings of the layout are reported beside them: if the current order has few bilayer crossings but the routes
// have many, the crossings come from the routing; if the order has many, they come from the order.
//
// Usage: node order-probe.mjs [--limit 100] [--out FILE.jsonl] [DATASET ...]
import {readFileSync, mkdirSync, writeFileSync} from 'node:fs';
import {ROOT, arqlet, normalizeArqlet, metrics} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const LIMIT = Number(option('--limit', 100));
const OUT = option('--out', `${ROOT}benchmarks/results/order-${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`);
const selected = args.length ? args : ['petri-net', 'activity-order', 'network-vlan', 'arqlet-pipeline', 'cloud-architecture', 'bpmn-pools', 'uml-class', 'state-order', 'er-diagram', 'layerweave-code'];
const ROUNDS = 24;

// Bilayer crossings between two adjacent ranks: pairs of edges whose ends are in the opposite order on the two ranks.
function bilayerCrossings(edges, pos) {
  let count = 0;
  for (let i = 0; i < edges.length; i++) {
    for (let j = i + 1; j < edges.length; j++) {
      const [a1, b1] = edges[i], [a2, b2] = edges[j];
      if (a1 === a2 || b1 === b2) continue;
      if ((pos.get(a1) - pos.get(a2)) * (pos.get(b1) - pos.get(b2)) < 0) count++;
    }
  }
  return count;
}

// Barycenter sweeps over the ranks; `order` is the list of node lists per rank and is changed in place.
function barycenterOrder(order, adjacency) {
  const pos = new Map();
  const refresh = () => order.forEach(list => list.forEach((id, i) => pos.set(id, i)));
  refresh();
  const total = () => {
    let c = 0;
    for (let r = 0; r + 1 < order.length; r++) c += bilayerCrossings(edgesBetween(r, r + 1), pos);
    return c;
  };
  const edgesBetween = (r, s) => {
    const out = [];
    for (const id of order[r]) for (const other of adjacency.get(id) ?? []) if (order[s].includes(other)) out.push(r < s ? [id, other] : [other, id]);
    return out;
  };
  let best = total();
  let bestOrder = order.map(list => [...list]);
  for (let round = 0; round < ROUNDS; round++) {
    const forward = round % 2 === 0;
    const sequence = forward ? [...order.keys()].slice(1) : [...order.keys()].slice(0, -1).reverse();
    for (const r of sequence) {
      const fixedRank = forward ? r - 1 : r + 1;
      const bary = new Map();
      for (const id of order[r]) {
        const ps = (adjacency.get(id) ?? []).filter(o => order[fixedRank].includes(o)).map(o => pos.get(o));
        bary.set(id, ps.length ? ps.reduce((a, b) => a + b, 0) / ps.length : pos.get(id));
      }
      order[r].sort((a, b) => bary.get(a) - bary.get(b) || pos.get(a) - pos.get(b));
      order[r].forEach((id, i) => pos.set(id, i));
    }
    const now = total();
    if (now < best) {
      best = now;
      bestOrder = order.map(list => [...list]);
    }
  }
  return {crossings: best, order: bestOrder};
}

// The areas of the input graph with a fixed flow, and the leaves of each.
function areas(graph) {
  const containers = new Set(graph.nodes.map(n => n.parentId).filter(Boolean));
  const flowOf = area => {
    const d = area === null ? graph.defaultDirection : (graph.containerDirections?.[area] ?? graph.defaultDirection);
    return ['lr', 'rl', 'tb', 'bt'].includes(d) ? (d === 'lr' || d === 'rl' ? 'x' : 'y') : null;
  };
  const byArea = new Map();
  for (const n of graph.nodes) {
    if (containers.has(n.id)) continue;
    const key = n.parentId ?? null;
    if (!byArea.has(key)) byArea.set(key, []);
    byArea.get(key).push(n.id);
  }
  return [...byArea.entries()].map(([area, ids]) => ({area, ids, axis: flowOf(area)})).filter(a => a.axis);
}

mkdirSync(OUT.slice(0, OUT.lastIndexOf('/')), {recursive: true});
const rows = [];
for (const name of selected) {
  const graph = JSON.parse(readFileSync(`${ROOT}benchmarks/datasets/${name}.json`, 'utf8'));
  const imported = arqlet({op: 'import', graph, limit: LIMIT});
  const refined = arqlet({
    op: 'refine', snapshot: imported.snapshot, expectedRevision: imported.snapshot.revision,
    expectedLayoutVersion: imported.snapshot.layoutVersion ?? 0, threads: 1,
  });
  const view = imported.snapshot.graph;
  // Boxes are stored relative to their container; the probe works in absolute coordinates.
  const parentOf = new Map(view.nodes.map(n => [n.id, n.parentId ?? null]));
  const rawBoxes = refined.snapshot.boxes;
  const boxes = {};
  for (const id of Object.keys(rawBoxes)) {
    let x = 0, y = 0;
    for (let c = id; c; c = parentOf.get(c) ?? null) {
      x += rawBoxes[c]?.x ?? 0;
      y += rawBoxes[c]?.y ?? 0;
      if (!parentOf.get(c)) break;
    }
    boxes[id] = {x, y, width: rawBoxes[id].width, height: rawBoxes[id].height};
  }
  const routed = metrics(normalizeArqlet(refined.snapshot));
  const viewIds = new Set(view.nodes.map(n => n.id));
  const viewEdges = view.edges.filter(e => e.target !== null && viewIds.has(e.source) && viewIds.has(e.target));
  let adjacentEdges = 0, longEdges = 0, current = 0, optimized = 0, areaCount = 0;
  for (const {ids, axis} of areas({...view, defaultDirection: view.defaultDirection, containerDirections: view.containerDirections ?? {}})) {
    const inArea = new Set(ids.filter(id => viewIds.has(id)));
    if (inArea.size < 2) continue;
    const mainOf = id => {
      const b = boxes[id];
      if (!b) return null;
      return axis === 'x' ? b.x : b.y;
    };
    const crossOf = id => {
      const b = boxes[id];
      return axis === 'x' ? b.y + b.height / 2 : b.x + b.width / 2;
    };
    const members = [...inArea].filter(id => boxes[id] && mainOf(id) !== null);
    // Ranks: neighbouring leaves with (nearly) the same main-axis start belong to one rank.
    const starts = [...new Set(members.map(id => Math.round(mainOf(id))))].sort((a, b) => a - b);
    const rankOf = new Map();
    let rank = -1, last = -Infinity;
    for (const s of starts) {
      if (s - last > 2) rank++;
      last = s;
      for (const id of members) if (Math.round(mainOf(id)) === s) rankOf.set(id, rank);
    }
    const ranks = Array.from({length: rank + 1}, () => []);
    for (const id of members) ranks[rankOf.get(id)].push(id);
    for (const list of ranks) list.sort((a, b) => crossOf(a) - crossOf(b));
    const adjacency = new Map();
    for (const e of viewEdges) {
      if (!rankOf.has(e.source) || !rankOf.has(e.target)) continue;
      const d = Math.abs(rankOf.get(e.source) - rankOf.get(e.target));
      if (d === 0) continue;
      if (d > 1) { longEdges++; continue; }
      adjacentEdges++;
      if (!adjacency.has(e.source)) adjacency.set(e.source, []);
      if (!adjacency.has(e.target)) adjacency.set(e.target, []);
      adjacency.get(e.source).push(e.target);
      adjacency.get(e.target).push(e.source);
    }
    // Current order.
    const pos = new Map();
    ranks.forEach(list => list.forEach((id, i) => pos.set(id, i)));
    for (let r = 0; r + 1 < ranks.length; r++) {
      const between = [];
      for (const id of ranks[r]) for (const o of adjacency.get(id) ?? []) if (rankOf.get(o) === r + 1) between.push([id, o]);
      current += bilayerCrossings(between, pos);
    }
    const result = barycenterOrder(ranks.map(list => [...list]), adjacency);
    optimized += result.crossings;
    areaCount++;
  }
  const rowData = {
    dataset: name, areas: areaCount, adjacentEdges, longEdges,
    bilayerCrossingsCurrentOrder: current, bilayerCrossingsBarycenter: optimized,
    routedCrossings: routed.crossings, routedAtSharedEndpoints: routed.sharedEndpointCrossings,
  };
  rows.push(rowData);
  console.log(`${name.padEnd(22)} adjacent ${String(adjacentEdges).padStart(4)}  long ${String(longEdges).padStart(3)}  order: current ${String(current).padStart(4)} -> barycenter ${String(optimized).padStart(4)}  routed ${routed.crossings} (+${routed.sharedEndpointCrossings} at shared endpoints)`);
}
writeFileSync(OUT, rows.map(r => JSON.stringify(r)).join('\n') + '\n');
console.log(`Results: ${OUT}`);
