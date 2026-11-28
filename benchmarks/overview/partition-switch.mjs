#!/usr/bin/env node
// How far nodes move when an overview switches between two partitions a host gives (`options.groups`, #239): today's
// continuation by id (`start.nodes`), the same with `start.carry: true`, and a layout without `start`. The graph is the
// symbols of a generated code index, flat (`benchmarks/overview/generate.py` without `--flat`, flattened here so that
// the files and directories stay known); partition A groups them by top-level directory and file. The partitions B:
//
//   shared     A with every tenth file moved to the next top-level directory (90 % of the paths kept)
//   renamed    A's groups under other names: the same members, no path shared
//   regrouped  the files dealt into as many groups as A has directories, by a hash of the file: no path shared, the
//              groups cut across A's (as "by role" against "by feature")
//   merge      A with the two largest directories merged into one group
//   split      A with the largest directory split in two (files by parity)
//
// `communities: 0` (the partition as given) for all, and once `regrouped` with `communities: 40`, so that Louvain
// divides the large groups and those clusters are carried too. A movement is the distance between a symbol's centre in
// A and in B (the same frame), in percent of A's root radius. Every answer is checked: no overlaps, every disc inside
// its parent's. Times are wall clock of one `layout-command` process per request (indicative on a shared machine).
//
//   cargo build --release -p arqlet-runtime --bin layout-command --locked
//   python3 benchmarks/overview/generate.py --nodes 3000 --seed 1 --out benchmarks/datasets/overview-3000.json
//   python3 benchmarks/overview/generate.py --nodes 10000 --seed 1 --out benchmarks/datasets/overview-10000.json
//   node benchmarks/overview/partition-switch.mjs target/release/layout-command \
//     benchmarks/datasets/overview-3000.json benchmarks/datasets/overview-10000.json
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {basename} from 'node:path';

const [bin, ...files] = process.argv.slice(2);
if (!bin || !files.length) { console.error('usage: partition-switch.mjs <layout-command> <graph.json>...'); process.exit(2); }
const run = request => {
  const input = JSON.stringify(request);
  const once = () => {
    const t = process.hrtime.bigint();
    const answer = JSON.parse(execFileSync(bin, {input, maxBuffer: 1 << 30, timeout: 300000, killSignal: 'SIGKILL'}));
    return [answer, Number(process.hrtime.bigint() - t) / 1e6];
  };
  let result;
  try { result = once(); } catch { result = once(); }   // one retry: the machine is shared
  const [answer, ms] = result;
  if (!answer.ok) throw Error(answer.error);
  return [answer.overview, ms];
};
const placesOf = o => Object.fromEntries(o.ids.map((id, i) => [id,
  {x: o.x[i], y: o.y[i], r: o.r[i], ...(o.parent[i] >= 0 ? {parent: o.ids[o.parent[i]]} : {})}]));

// No two discs of one container overlap, every disc lies inside its parent's (hundredths of rounding allowed).
function valid(o) {
  if (o.stats.overlaps !== 0) return false;
  const byParent = new Map();
  for (let i = 0; i < o.ids.length; i++) {
    const p = o.parent[i];
    if (p >= 0 && Math.hypot(o.x[i] - o.x[p], o.y[i] - o.y[p]) + o.r[i] > o.r[p] + 0.05) return false;
    if (!byParent.has(p)) byParent.set(p, []);
    byParent.get(p).push(i);
  }
  for (const list of byParent.values()) {
    const sorted = [...list].sort((a, b) => (o.x[a] - o.r[a]) - (o.x[b] - o.r[b]));
    for (let s = 0; s < sorted.length; s++) {
      const i = sorted[s];
      for (let t = s + 1; t < sorted.length; t++) {
        const j = sorted[t];
        if (o.x[j] - o.r[j] >= o.x[i] + o.r[i]) break;
        if (Math.hypot(o.x[i] - o.x[j], o.y[i] - o.y[j]) < o.r[i] + o.r[j] - 0.05) return false;
      }
    }
  }
  return true;
}

// A small string hash (FNV-1a), for the deal of `regrouped`.
const hash = s => { let h = 0x811c9dc5; for (const c of s) h = Math.imul(h ^ c.charCodeAt(0), 0x01000193) >>> 0; return h; };

for (const file of files) {
  const tree = JSON.parse(readFileSync(file, 'utf8'));
  const parentOf = new Map(tree.nodes.map(n => [n.id, n.parentId ?? null]));
  const top = id => { let at = id; while (parentOf.get(at)) at = parentOf.get(at); return at; };
  const symbols = tree.nodes.filter(n => n.kind === 'symbol');
  const keep = new Set(symbols.map(n => n.id));
  const graph = {nodes: symbols.map(n => ({...n, parentId: null})),
    edges: tree.edges.filter(e => keep.has(e.source) && (e.target === null || keep.has(e.target)))};
  const fileOf = s => parentOf.get(s);
  const dirs = [...new Set(symbols.map(s => top(fileOf(s.id))))];
  const size = new Map(dirs.map(d => [d, symbols.filter(s => top(fileOf(s.id)) === d).length]));
  const bySize = [...dirs].sort((a, b) => size.get(b) - size.get(a) || a.localeCompare(b));
  const fileIds = [...new Set(symbols.map(s => fileOf(s.id)))];
  const moved = new Set(fileIds.filter((_, i) => i % 10 === 3));
  const partition = path => Object.fromEntries(symbols.map(s => [s.id, path(s.id, fileOf(s.id), top(fileOf(s.id)))]));
  const a = partition((_, f, d) => [d, f]);
  const next = d => dirs[(dirs.indexOf(d) + 1) % dirs.length];
  const switches = [
    ['shared', partition((_, f, d) => [moved.has(f) ? next(d) : d, f]), 0],
    ['renamed', partition((_, f, d) => [`t-${d}`, `m-${f}`]), 0],
    ['regrouped', partition((_, f) => [`k${hash(f) % dirs.length}`]), 0],
    ['regrouped', partition((_, f) => [`k${hash(f) % dirs.length}`]), 40],
    ['merge', partition((_, f, d) => [d === bySize[0] || d === bySize[1] ? 'merged' : d, f]), 0],
    ['split', partition((_, f, d) => [d === bySize[0] ? `${d}-${fileIds.indexOf(f) % 2}` : d, f]), 0],
  ];
  const [before, beforeMs] = run({op: 'overview', graph, options: {communities: 0, groups: a}});
  console.log(JSON.stringify({file: basename(file), symbols: symbols.length, groups: before.ids.length - symbols.length,
    how: 'A, no start', valid: valid(before), ms: Math.round(beforeMs)}));
  const places = placesOf(before);
  for (const [name, groups, communities] of switches) {
    const options = {communities, groups};
    for (const [how, start] of [['no start', null], ['start.nodes', {nodes: places}], ['start.nodes, carry', {nodes: places, carry: true}]]) {
      const [o, ms] = run({op: 'overview', graph, options, ...(start ? {start} : {})});
      const at = new Map(o.ids.map((id, i) => [id, i]));
      const moves = [];
      before.ids.forEach((id, i) => {
        const j = at.get(id);
        if (keep.has(id) && j !== undefined) moves.push(Math.hypot(o.x[j] - before.x[i], o.y[j] - before.y[i]));
      });
      moves.sort((p, q) => p - q);
      const q = p => +(100 * moves[Math.min(moves.length - 1, Math.floor(p * moves.length))] / before.rootRadius).toFixed(1);
      console.log(JSON.stringify({file: basename(file), switch: name, communities, how, valid: valid(o),
        anew: o.continuation?.anew, carried: o.continuation?.carried, median: q(0.5), p90: q(0.9), max: q(1),
        rootRadius: +(o.rootRadius / before.rootRadius).toFixed(3), movement: o.movement, ms: Math.round(ms)}));
    }
  }
}
