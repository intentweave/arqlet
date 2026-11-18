#!/usr/bin/env node
// The first view of an import within a budget (`levelOfDetail.nodes` and `edges`): for each set of options, the boxes the
// view shows (leaves and collapsed containers), its open frames, its drawn edges, how many of them the single pass built
// and how many it left to the fallback router, the time and the size of the answer.
//
// Usage: node benchmarks/eval/first-view-budget.mjs [--flat] [--limit N] [--out FILE.jsonl] GRAPH OPTIONS_JSON ...
//   --flat   drop every container first (the leaves at the top level, the edges between them kept): a flat graph.
import {readFileSync, writeFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {LAYOUT_COMMAND} from './lib.mjs';

const args = process.argv.slice(2);
const flag = name => {
  const i = args.indexOf(name);
  return i >= 0 ? (args.splice(i, 1), true) : false;
};
const value = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const flat = flag('--flat');
const limit = Number(value('--limit', 10000));
const out = value('--out', null);
const [file, ...sets] = args;
const parsed = JSON.parse(readFileSync(file, 'utf8'));
let graph = parsed.graph ?? parsed;
if (flat) {
  const containers = new Set(graph.nodes.map(n => n.parentId).filter(Boolean));
  const nodes = graph.nodes.filter(n => !containers.has(n.id)).map(n => ({...n, parentId: null}));
  const ids = new Set(nodes.map(n => n.id));
  graph = {...graph, nodes, edges: graph.edges.filter(e => ids.has(e.source) && ids.has(e.target)), containerDirections: {}};
}
const rows = [];
for (const set of sets) {
  const options = JSON.parse(set);
  const started = performance.now();
  const result = spawnSync(LAYOUT_COMMAND, [], {input: JSON.stringify({op: 'import', graph, limit, options}), encoding: 'utf8', maxBuffer: 1 << 30});
  const ms = performance.now() - started;
  const response = JSON.parse(result.stdout);
  if (!response.ok) {
    console.log(set, 'error:', response.error);
    continue;
  }
  const s = response.snapshot;
  const frames = new Set(s.graph.nodes.map(n => n.parentId).filter(Boolean));
  const pass = (s.layoutDiagnostics ?? []).map(d => d.match(/Single pass: (\d+) constructed, (\d+) delegated/)).find(Boolean);
  const row = {
    graph: file.split('/').pop().replace(/\.json$/, '') + (flat ? ' (flat)' : ''), graphNodes: graph.nodes.length, graphEdges: graph.edges.length,
    options, boxes: s.graph.nodes.length - frames.size, frames: frames.size, edges: s.graph.edges.length,
    constructed: pass ? Number(pass[1]) : null, delegated: pass ? Number(pass[2]) : null,
    communities: s.options.communities ?? 0, ms: Math.round(ms), megabytes: Math.round(result.stdout.length / 1e5) / 10,
  };
  rows.push(row);
  console.log(`${set}: ${row.boxes} boxes, ${row.frames} frames, ${row.edges} edges (${row.constructed} constructed, ${row.delegated} delegated), ${(ms / 1000).toFixed(1)} s, ${row.megabytes} MB`);
}
if (out) writeFileSync(out, rows.map(r => JSON.stringify(r)).join('\n') + '\n');
