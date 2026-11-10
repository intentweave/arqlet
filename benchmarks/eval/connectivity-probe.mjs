#!/usr/bin/env node
// Connectivity as a placement criterion, measured on the rank order of the finished layout (no change to the layout).
//
// The ranks and the edges between neighbouring ranks are those of order-probe.mjs. Each rank is ordered by one of:
//   - current: the order the layout produced;
//   - barycenter: the Sugiyama sweeps (24 rounds), the best order seen (as order-probe.mjs);
//   - degree-sorted: by the number of edges of each node, most connected first;
//   - degree-centred: by the number of edges, the most connected nodes in the middle of the rank.
// Measured per order: bilayer crossings between neighbouring ranks, and the cross distance of the edges (the sum over edges of the
// distance across the flow between their two ends), as a proxy for the length of the routes. Long edges (skipping a rank) are not
// counted, as in order-probe.mjs.
//
// Usage: node connectivity-probe.mjs [--limit 100] [--out FILE.jsonl] [DATASET ...]
import {readFileSync, mkdirSync, writeFileSync} from 'node:fs';
import {ROOT, arqlet} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const LIMIT = Number(option('--limit', 100));
const OUT = option('--out', `${ROOT}benchmarks/results/connectivity-${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`);
const selected = args.length ? args : ['petri-net', 'activity-order', 'network-vlan', 'arqlet-pipeline', 'cloud-architecture', 'bpmn-pools', 'state-order', 'layerweave-code'];
const ROUNDS = 24;

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

// Crossings and cross distance of the whole area for one order (ranks: lists of ids, adjacency: id -> neighbour ids).
function score(ranks, rankOf, adjacency) {
  const pos = new Map();
  ranks.forEach(list => list.forEach((id, i) => pos.set(id, i)));
  let crossings = 0, distance = 0;
  for (let r = 0; r + 1 < ranks.length; r++) {
    const between = [];
    for (const id of ranks[r]) for (const o of adjacency.get(id) ?? []) if (rankOf.get(o) === r + 1) between.push([id, o]);
    crossings += bilayerCrossings(between, pos);
    for (const [a, b] of between) distance += Math.abs(pos.get(a) - pos.get(b));
  }
  return {crossings, distance};
}

function barycenter(ranks, rankOf, adjacency) {
  const order = ranks.map(list => [...list]);
  const pos = new Map();
  const refresh = () => order.forEach(list => list.forEach((id, i) => pos.set(id, i)));
  refresh();
  let best = score(order, rankOf, adjacency);
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
    const now = score(order, rankOf, adjacency);
    if (now.crossings < best.crossings) {
      best = now;
      bestOrder = order.map(list => [...list]);
    }
  }
  return {score: best, order: bestOrder};
}

// Weighted local search from the barycenter order: swap neighbours inside a rank while the objective (crossings, then the
// weighted cross distance, with the number of edges between two nodes as their weight) improves. The rank membership is kept,
// so the edges that cross each corridor, and with them the tracks it needs, do not change.
function weightedSwap(order, rankOf, adjacency) {
  const pos = new Map();
  const refresh = () => order.forEach(list => list.forEach((id, i) => pos.set(id, i)));
  refresh();
  const objective = () => {
    const s = score(order, rankOf, adjacency);
    return [s.crossings, s.distance];
  };
  const better = (a, b) => a[0] < b[0] || (a[0] === b[0] && a[1] < b[1] - 1e-9);
  let current = objective();
  let improved = true;
  while (improved) {
    improved = false;
    for (const list of order) {
      for (let i = 0; i + 1 < list.length; i++) {
        [list[i], list[i + 1]] = [list[i + 1], list[i]];
        refresh();
        const now = objective();
        if (better(now, current)) {
          current = now;
          improved = true;
        } else {
          [list[i], list[i + 1]] = [list[i + 1], list[i]];
          refresh();
        }
      }
    }
  }
  return {crossings: current[0], distance: current[1], order};
}

// Each rank in the order of `key`: `most` nodes first (degree-sorted) or the most connected in the middle (degree-centred).
function byDegree(ranks, degree, centred) {
  return ranks.map(list => {
    const sorted = [...list].sort((a, b) => degree.get(b) - degree.get(a) || (a < b ? -1 : 1));
    if (!centred) return sorted;
    // Alternate the nodes to the two ends of the rank so that the most connected end up in the middle.
    const left = [], right = [];
    sorted.forEach((id, i) => (i % 2 === 0 ? left : right).push(id));
    return [...left, ...right.reverse()];
  });
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
  const parentOf = new Map(view.nodes.map(n => [n.id, n.parentId ?? null]));
  const rawBoxes = refined.snapshot.boxes;
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
  // Degree: the number of edges of the node in the view (both directions, parallel edges counted).
  const degree = new Map();
  for (const e of viewEdges) {
    degree.set(e.source, (degree.get(e.source) ?? 0) + 1);
    degree.set(e.target, (degree.get(e.target) ?? 0) + 1);
  }
  const areas = new Map();
  for (const n of view.nodes) {
    if (containers.has(n.id)) continue;
    const key = n.parentId ?? null;
    if (!areas.has(key)) areas.set(key, []);
    areas.get(key).push(n.id);
  }
  const totals = {current: {crossings: 0, distance: 0}, barycenter: {crossings: 0, distance: 0}, weightedSwap: {crossings: 0, distance: 0}, degreeSorted: {crossings: 0, distance: 0}, degreeCentred: {crossings: 0, distance: 0}};
  let adjacentEdges = 0, areasUsed = 0;
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
    const adjacency = new Map();
    for (const e of viewEdges) {
      if (!rankOf.has(e.source) || !rankOf.has(e.target)) continue;
      if (Math.abs(rankOf.get(e.source) - rankOf.get(e.target)) !== 1) continue;
      adjacentEdges++;
      for (const [a, b] of [[e.source, e.target], [e.target, e.source]]) {
        if (!adjacency.has(a)) adjacency.set(a, []);
        adjacency.get(a).push(b);
      }
    }
    if (adjacency.size === 0) continue;
    areasUsed++;
    const sorted = ranks.map(list => [...list].sort((a, b) => across(a) - across(b)));
    const current = score(sorted, rankOf, adjacency);
    const bary = barycenter(sorted, rankOf, adjacency);
    const swapped = weightedSwap(bary.order.map(list => [...list]), rankOf, adjacency);
    const variants = {
      current,
      barycenter: bary.score,
      weightedSwap: {crossings: swapped.crossings, distance: swapped.distance},
      degreeSorted: score(byDegree(sorted, degree, false), rankOf, adjacency),
      degreeCentred: score(byDegree(sorted, degree, true), rankOf, adjacency),
    };
    for (const k of Object.keys(totals)) {
      totals[k].crossings += variants[k].crossings;
      totals[k].distance += variants[k].distance;
    }
  }
  const row = {dataset: name, areas: areasUsed, adjacentEdges, ...Object.fromEntries(Object.entries(totals).flatMap(([k, v]) => [[`${k}Crossings`, v.crossings], [`${k}Distance`, Math.round(v.distance)]]))};
  rows.push(row);
  console.log(`${name.padEnd(22)} edges ${String(adjacentEdges).padStart(4)}  crossings current ${totals.current.crossings} bary ${totals.barycenter.crossings} weighted-swap ${totals.weightedSwap.crossings} deg-sorted ${totals.degreeSorted.crossings}  distance ${Math.round(totals.current.distance)} / ${Math.round(totals.barycenter.distance)} / ${Math.round(totals.weightedSwap.distance)} / ${Math.round(totals.degreeSorted.distance)}`);
}
writeFileSync(OUT, rows.map(r => JSON.stringify(r)).join('\n') + '\n');
console.log(`Results: ${OUT}`);
