#!/usr/bin/env node
// How far nodes move when the overview of a changed graph continues from places by id (#209), against a layout without
// them. A share of the leaves is removed, as many nodes are added (each beside one it calls) and that share of the
// edges is rewired (seeded, seed 7). A movement is the distance between a node's old and new centre in the shared
// frame (origin: the centre of the root disc), in percent of the earlier root radius. Positions only; no timing.
//
//   cargo build --release -p arqlet-runtime --bin layout-command --locked
//   python3 benchmarks/overview/generate.py --nodes 10000 --seed 1 --out benchmarks/datasets/overview-10000.json
//   python3 benchmarks/overview/generate.py --nodes 6000 --seed 1 --flat --out benchmarks/datasets/overview-flat-6000.json
//   node benchmarks/overview/continuation-drift.mjs target/release/layout-command \
//     benchmarks/datasets/overview-10000.json benchmarks/datasets/overview-flat-6000.json
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {basename} from 'node:path';

const [bin, ...files] = process.argv.slice(2);
if (!bin || !files.length) { console.error('usage: continuation-drift.mjs <layout-command> <graph.json>...'); process.exit(2); }
const run = request => {
  const answer = JSON.parse(execFileSync(bin, {input: JSON.stringify(request), maxBuffer: 1 << 30, timeout: 240000, killSignal: 'SIGKILL'}));
  if (!answer.ok) throw Error(answer.error);
  return answer.overview;
};
// mulberry32: a small seeded generator.
const random = seed => () => {
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const placesOf = o => Object.fromEntries(o.ids.map((id, i) => [id, {x: o.x[i], y: o.y[i], r: o.r[i]}]));

function changedGraph(graph, share) {
  const next = random(7);
  const containers = new Set(graph.nodes.map(n => n.parentId));
  const gone = new Set(graph.nodes.filter(n => !containers.has(n.id) && next() < share).map(n => n.id));
  const kept = graph.nodes.filter(n => !gone.has(n.id));
  const leaves = kept.filter(n => !containers.has(n.id));
  const pick = () => leaves[Math.floor(next() * leaves.length)];
  const added = Array.from({length: Math.round(share * graph.nodes.length)},
    (_, i) => ({id: `added-${i}`, kind: 'symbol', label: `added ${i}`, parentId: pick().parentId}));
  const edges = graph.edges.filter(e => !gone.has(e.source) && !gone.has(e.target))
    .map(e => (next() < share ? {...e, target: pick().id} : e));
  added.forEach((n, i) => edges.push({id: `added-edge-${i}`, kind: 'call', source: n.id,
    target: leaves.find(l => l.parentId === n.parentId).id}));
  return {nodes: [...kept, ...added], edges};
}

for (const file of files) {
  const graph = JSON.parse(readFileSync(file, 'utf8'));
  const before = run({op: 'overview', graph});
  for (const share of [0.001, 0.01, 0.05]) {
    const changed = changedGraph(graph, share);
    const places = placesOf(before);
    for (const [how, start] of [['by id', {nodes: places}], ['by id, settle', {nodes: places, settle: true}], ['no start', null]]) {
      const o = run({op: 'overview', graph: changed, ...(start ? {start} : {})});
      const at = new Map(o.ids.map((id, i) => [id, i]));
      const moves = [];
      before.ids.forEach((id, i) => {
        const j = at.get(id);
        if (!id.startsWith('community:') && j !== undefined) moves.push(Math.hypot(o.x[j] - before.x[i], o.y[j] - before.y[i]));
      });
      moves.sort((a, b) => a - b);
      const q = p => +(100 * moves[Math.min(moves.length - 1, Math.floor(p * moves.length))] / before.rootRadius).toFixed(1);
      console.log(JSON.stringify({file: basename(file), share, how, anew: o.continuation?.anew, overlaps: o.stats.overlaps,
        communities: o.stats.communities, median: q(0.5), p90: q(0.9), max: q(1),
        rootRadius: +(o.rootRadius / before.rootRadius).toFixed(3)}));
    }
  }
}
