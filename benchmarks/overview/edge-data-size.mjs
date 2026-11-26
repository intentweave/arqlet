#!/usr/bin/env node
// Size of the `overview` answer with and without `edgeData` (#209), as written by `layout-command` and gzipped (zlib's
// default level), on generated graphs with their own kind (`call`), with six kinds, and with weights 1 to 20 besides
// (kinds and weights drawn from a seeded generator, seed 1). `columnBytes` is what the two columns take instead.
//
//   cargo build --release -p arqlet-runtime --bin layout-command --locked
//   python3 benchmarks/overview/generate.py --nodes 100000 --seed 1 --out benchmarks/datasets/overview-100000.json
//   node benchmarks/overview/edge-data-size.mjs target/release/layout-command benchmarks/datasets/overview-100000.json
import {readFileSync} from 'node:fs';
import {execFileSync} from 'node:child_process';
import {gzipSync} from 'node:zlib';
import {basename} from 'node:path';

const [bin, ...files] = process.argv.slice(2);
if (!bin || !files.length) { console.error('usage: edge-data-size.mjs <layout-command> <graph.json>...'); process.exit(2); }
const run = request => execFileSync(bin, {input: JSON.stringify(request), maxBuffer: 1 << 30, timeout: 240000, killSignal: 'SIGKILL'});
// mulberry32: a small seeded generator.
const random = seed => () => {
  seed = (seed + 0x6d2b79f5) | 0;
  let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
  t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const kinds = ['call', 'import', 'extends', 'implements', 'reads', 'writes'];
for (const file of files) {
  const graph = JSON.parse(readFileSync(file, 'utf8'));
  const draw = (weights, next = random(1)) => ({...graph, edges: graph.edges.map(e => ({
    ...e, kind: kinds[Math.floor(next() * kinds.length)], ...(weights ? {weight: 1 + Math.floor(next() * 20)} : {})}))});
  for (const [variant, g] of [['one kind', graph], ['six kinds', draw(false)], ['six kinds, weights 1..20', draw(true)]]) {
    const off = run({op: 'overview', graph: g});
    const on = run({op: 'overview', graph: g, edgeData: true});
    const o = JSON.parse(on).overview;
    const [without, with_] = [off, on].map(t => ({bytes: t.length, gzip: gzipSync(t).length}));
    console.log(JSON.stringify({file: basename(file), variant, nodes: g.nodes.length, edges: o.edges.length / 2,
      without, with: with_, addedBytes: with_.bytes - without.bytes, addedGzip: with_.gzip - without.gzip,
      columnBytes: 4 * (o.edgeKind.length + (o.edgeWeight?.length ?? 0))}));
  }
}
