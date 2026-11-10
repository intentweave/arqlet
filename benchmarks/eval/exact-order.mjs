#!/usr/bin/env node
// Brute-force reference for the order inside the ranks of small areas.
//
// For every fixed-flow area whose ranks admit few orders (the product of the factorials of the rank sizes is at most
// MAX_ORDERS), the exact minimum of the bilayer crossings is computed by enumerating all orders. The result is compared with
// the barycenter order (24 Sugiyama sweeps, as in order-probe.mjs) on the same ranks. The gap says how far the heuristic is
// from the optimum on the instances where the optimum is known. The rule the heuristic should follow is then derived from the
// instances where it is not optimal, not from the average.
//
// Usage: node exact-order.mjs [--limit 100] [--out FILE.jsonl] [DATASET ...]
import {readFileSync, mkdirSync, writeFileSync} from 'node:fs';
import {ROOT, arqlet} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const LIMIT = Number(option('--limit', 100));
const OUT = option('--out', `${ROOT}benchmarks/results/exact-order-${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`);
const selected = args.length ? args : ['petri-net', 'activity-order', 'network-vlan', 'arqlet-pipeline', 'cloud-architecture', 'bpmn-pools', 'state-order', 'layerweave-code'];
const MAX_ORDERS = 200000;
const ROUNDS = 24;

const factorial = n => (n <= 1 ? 1 : n * factorial(n - 1));

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

// Crossings of the whole area for the given ranks (arrays of ids), using the adjacent-rank edges only.
function crossingsOf(ranks, rankOf, edgesByRank) {
  const pos = new Map();
  ranks.forEach(list => list.forEach((id, i) => pos.set(id, i)));
  let total = 0;
  for (let r = 0; r + 1 < ranks.length; r++) total += bilayerCrossings(edgesByRank[r] ?? [], pos);
  return total;
}

// All permutations of a list (Heap's algorithm would also do; this one is simple and small).
function* permutations(list) {
  if (list.length <= 1) {
    yield list;
    return;
  }
  for (let i = 0; i < list.length; i++) {
    const rest = [...list.slice(0, i), ...list.slice(i + 1)];
    for (const p of permutations(rest)) yield [list[i], ...p];
  }
}

// Exact minimum over all orders of all ranks: the product over ranks of their permutations, enumerated rank by rank.
function exactMinimum(ranks, edgesByRank) {
  let best = Infinity;
  let bestOrder = null;
  const choose = (r, chosen) => {
    if (r === ranks.length) {
      const c = crossingsOf(chosen, null, edgesByRank);
      if (c < best) {
        best = c;
        bestOrder = chosen.map(list => [...list]);
      }
      return;
    }
    for (const p of permutations(ranks[r])) choose(r + 1, [...chosen, p]);
  };
  choose(0, []);
  return {crossings: best, order: bestOrder};
}

// Barycenter sweeps (as order-probe.mjs): the best order seen.
function barycenter(ranks, rankOf, adjacency) {
  const order = ranks.map(list => [...list]);
  const pos = new Map();
  const refresh = () => order.forEach(list => list.forEach((id, i) => pos.set(id, i)));
  refresh();
  const cost = () => {
    let c = 0;
    for (let r = 0; r + 1 < order.length; r++) {
      const between = [];
      for (const id of order[r]) for (const o of adjacency.get(id) ?? []) if (rankOf.get(o) === r + 1) between.push([id, o]);
      c += bilayerCrossings(between, pos);
    }
    return c;
  };
  let best = cost();
  let bestOrder = order.map(list => [...list]);
  for (let round = 0; round < ROUNDS; round++) {
    const forward = round % 2 === 0;
    const sequence = forward ? [...order.keys()].slice(1) : [...order.keys()].slice(0, -1).reverse();
    for (const r of sequence) {
      const fixedRank = forward ? r - 1 : r + 1;
      const bary = new Map();
      for (const id of order[r]) {
        const ps = (adjacency.get(id) ?? []).filter(o => rankOf.get(o) === fixedRank).map(o => pos.get(o));
        bary.set(id, ps.length ? ps.reduce((a, b) => a + b, 0) / ps.length : pos.get(id));
      }
      order[r].sort((a, b) => bary.get(a) - bary.get(b) || pos.get(a) - pos.get(b));
      order[r].forEach((id, i) => pos.set(id, i));
    }
    const now = cost();
    if (now < best) {
      best = now;
      bestOrder = order.map(list => [...list]);
    }
  }
  return {crossings: best, order: bestOrder};
}

mkdirSync(OUT.slice(0, OUT.lastIndexOf('/')), {recursive: true});
const rows = [];
for (const name of selected) {
  const graph = JSON.parse(readFileSync(`${ROOT}benchmarks/datasets/${name}.json`, 'utf8'));
  const imported = arqlet({op: 'import', graph, limit: LIMIT});
  const view = imported.snapshot.graph;
  const parentOf = new Map(view.nodes.map(n => [n.id, n.parentId ?? null]));
  const rawBoxes = imported.snapshot.boxes;
  const absolute = id => {
    let x = 0, y = 0;
    for (let c = id; c; c = parentOf.get(c) ?? null) {
      x += rawBoxes[c]?.x ?? 0;
      y += rawBoxes[c]?.y ?? 0;
    }
    return {x, y, width: rawBoxes[id].width, height: rawBoxes[id].height};
  };
  const containers = new Set(view.nodes.map(n => n.parentId).filter(Boolean));
  const viewIds = new Set(view.nodes.map(n => n.id));
  const viewEdges = view.edges.filter(e => e.target !== null && viewIds.has(e.source) && viewIds.has(e.target));
  const areas = new Map();
  for (const n of view.nodes) {
    if (containers.has(n.id)) continue;
    const key = n.parentId ?? null;
    if (!areas.has(key)) areas.set(key, []);
    areas.get(key).push(n.id);
  }
  for (const [area, ids] of areas) {
    const direction = area === null ? graph.defaultDirection : (graph.containerDirections?.[area] ?? graph.defaultDirection);
    if (!['lr', 'rl', 'tb', 'bt'].includes(direction)) continue;
    const horizontal = direction === 'lr' || direction === 'rl';
    const main = id => { const b = absolute(id); return horizontal ? b.x : b.y; };
    const across = id => { const b = absolute(id); return horizontal ? b.y + b.height / 2 : b.x + b.width / 2; };
    const starts = [...new Set(ids.map(id => Math.round(main(id))))].sort((a, b) => a - b);
    const ranks = [];
    let last = -Infinity;
    for (const s of starts) {
      if (s - last > 2) ranks.push([]);
      last = s;
      for (const id of ids) if (Math.round(main(id)) === s) ranks[ranks.length - 1].push(id);
    }
    const rankOf = new Map();
    ranks.forEach((list, r) => list.forEach(id => rankOf.set(id, r)));
    const sorted = ranks.map(list => [...list].sort((a, b) => across(a) - across(b)));
    const edgesByRank = Array.from({length: ranks.length}, () => []);
    const adjacency = new Map();
    for (const e of viewEdges) {
      if (!rankOf.has(e.source) || !rankOf.has(e.target)) continue;
      const d = rankOf.get(e.source) - rankOf.get(e.target);
      if (Math.abs(d) !== 1) continue;
      const r = Math.min(rankOf.get(e.source), rankOf.get(e.target));
      edgesByRank[r].push(d < 0 ? [e.source, e.target] : [e.target, e.source]);
      for (const [a, b] of [[e.source, e.target], [e.target, e.source]]) {
        if (!adjacency.has(a)) adjacency.set(a, []);
        adjacency.get(a).push(b);
      }
    }
    if (edgesByRank.every(list => list.length === 0)) continue;
    const orders = ranks.reduce((product, list) => product * factorial(list.length), 1);
    if (orders > MAX_ORDERS) {
      rows.push({dataset: name, area: String(area), ranks: ranks.map(l => l.length), orders, skipped: true});
      continue;
    }
    const current = crossingsOf(sorted, rankOf, edgesByRank);
    const heuristic = barycenter(sorted, rankOf, adjacency);
    const exact = exactMinimum(sorted, edgesByRank);
    rows.push({dataset: name, area: String(area), ranks: ranks.map(l => l.length), orders, current, barycenter: heuristic.crossings, exact: exact.crossings});
    console.log(`${name.padEnd(20)} area ${String(area).padEnd(14)} ranks ${JSON.stringify(ranks.map(l => l.length))} orders ${orders}: current ${current}, barycenter ${heuristic.crossings}, exact ${exact.crossings}`);
  }
}
writeFileSync(OUT, rows.map(r => JSON.stringify(r)).join('\n') + '\n');
console.log(`Results: ${OUT}`);
