#!/usr/bin/env node
// Corridor capacity: how many tracks does each corridor between two neighbouring ranks need, and how many does its free width
// hold? The placement can reserve the width of a corridor only if this is known, so the probe measures the demand of the
// finished layout against the space the layout left.
//
// For every area with a fixed flow, the leaves are grouped into ranks (order-probe.mjs). The corridor between rank r and r + 1 is
// the free interval along the flow between the far edge of rank r and the near edge of rank r + 1. The tracks in a corridor are
// the routes' segments that run across the flow inside that interval (for a left-to-right flow: vertical segments), clustered at
// the track spacing. The capacity is the interval's width divided by the track spacing.
//
// Usage: node corridor-capacity.mjs [--limit 100] [--out FILE.jsonl] [DATASET ...]
import {readFileSync, mkdirSync, writeFileSync} from 'node:fs';
import {ROOT, arqlet} from './lib.mjs';

const args = process.argv.slice(2);
const option = (name, fallback) => {
  const i = args.indexOf(name);
  return i >= 0 ? args.splice(i, 2)[1] : fallback;
};
const LIMIT = Number(option('--limit', 100));
const OUT = option('--out', `${ROOT}benchmarks/results/corridors-${new Date().toISOString().replace(/[:.]/g, '-')}.jsonl`);
const selected = args.length ? args : ['petri-net', 'activity-order', 'network-vlan', 'arqlet-pipeline', 'cloud-architecture', 'bpmn-pools', 'state-order', 'layerweave-code'];
const TRACK = 6;

mkdirSync(OUT.slice(0, OUT.lastIndexOf('/')), {recursive: true});
const rows = [];
for (const name of selected) {
  const graph = JSON.parse(readFileSync(`${ROOT}benchmarks/datasets/${name}.json`, 'utf8'));
  const imported = arqlet({op: 'import', graph, limit: LIMIT});
  const refined = arqlet({
    op: 'refine', snapshot: imported.snapshot, expectedRevision: imported.snapshot.revision,
    expectedLayoutVersion: imported.snapshot.layoutVersion ?? 0, threads: 1,
  });
  const snapshot = refined.snapshot;
  const view = snapshot.graph;
  const parentOf = new Map(view.nodes.map(n => [n.id, n.parentId ?? null]));
  // Absolute boxes: boxes are relative to their container.
  const absolute = id => {
    let x = 0, y = 0;
    for (let c = id; c; c = parentOf.get(c) ?? null) {
      x += snapshot.boxes[c]?.x ?? 0;
      y += snapshot.boxes[c]?.y ?? 0;
    }
    const b = snapshot.boxes[id];
    return {x, y, width: b.width, height: b.height};
  };
  const containers = new Set(view.nodes.map(n => n.parentId).filter(Boolean));
  const routes = snapshot.routes.filter(r => r.bundle_of == null);
  const areas = new Map();
  for (const n of view.nodes) {
    if (containers.has(n.id)) continue;
    const key = n.parentId ?? null;
    if (!areas.has(key)) areas.set(key, []);
    areas.get(key).push(n.id);
  }
  let corridors = 0, tight = 0, shortage = 0, maxDemand = 0, demandTotal = 0, capacityTotal = 0;
  for (const [area, ids] of areas) {
    const direction = area === null ? graph.defaultDirection : (graph.containerDirections?.[area] ?? graph.defaultDirection);
    if (!['lr', 'rl', 'tb', 'bt'].includes(direction)) continue;
    const horizontal = direction === 'lr' || direction === 'rl';
    // Ranks: leaves with the same start along the flow.
    const main = id => {
      const b = absolute(id);
      return horizontal ? b.x : b.y;
    };
    const extentEnd = id => {
      const b = absolute(id);
      return horizontal ? b.x + b.width : b.y + b.height;
    };
    const starts = [...new Set(ids.map(id => Math.round(main(id))))].sort((a, b) => a - b);
    const ranks = [];
    let last = -Infinity;
    for (const s of starts) {
      if (s - last > 2) ranks.push([]);
      last = s;
      for (const id of ids) if (Math.round(main(id)) === s) ranks[ranks.length - 1].push(id);
    }
    for (let r = 0; r + 1 < ranks.length; r++) {
      const far = Math.max(...ranks[r].map(extentEnd));
      const near = Math.min(...ranks[r + 1].map(main));
      const width = near - far;
      corridors++;
      // Tracks: the segments that run across the flow inside the corridor, each cluster of nearby ones counted once.
      const across = [];
      for (const route of routes) {
        for (let i = 0; i + 1 < route.points.length; i++) {
          const a = route.points[i], b = route.points[i + 1];
          const acrossFlow = horizontal ? Math.abs(a.x - b.x) < 1e-6 : Math.abs(a.y - b.y) < 1e-6;
          if (!acrossFlow) continue;
          const c = horizontal ? a.x : a.y;
          if (c > far + 1e-6 && c < near - 1e-6) across.push(c);
        }
      }
      across.sort((p, q) => p - q);
      let tracks = 0, previous = -Infinity;
      for (const c of across) {
        if (c - previous >= TRACK - 1e-6) tracks++;
        previous = c;
      }
      const capacity = width > 0 ? Math.floor(width / TRACK) : 0;
      demandTotal += tracks;
      capacityTotal += capacity;
      maxDemand = Math.max(maxDemand, tracks);
      if (tracks > capacity) {
        tight++;
        shortage += tracks - capacity;
      }
    }
  }
  const row = {dataset: name, corridors, tightCorridors: tight, shortageTracks: shortage, demandTracks: demandTotal, capacityTracks: capacityTotal, maxDemandPerCorridor: maxDemand};
  rows.push(row);
  console.log(`${name.padEnd(22)} corridors ${String(corridors).padStart(3)}  tight ${String(tight).padStart(3)}  short by ${String(shortage).padStart(4)} tracks  demand ${demandTotal} / capacity ${capacityTotal}`);
}
writeFileSync(OUT, rows.map(r => JSON.stringify(r)).join('\n') + '\n');
console.log(`Results: ${OUT}`);
