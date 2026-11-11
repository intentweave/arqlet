// Cluster overview (ADR 0018) against d3-force on the same hierarchical graphs (benchmarks/overview/generate.py). Arqlet
// runs in this process through the WASM engine (the browser's path; build it first, see below); d3-force lays out every
// node flat with its edges, as a Cosmograph-like view does without a hierarchy. Both are measured on the leaves with the
// same measures, from their centres only:
//
//   overlaps   pairs of leaf discs of radius 4 that overlap, in the engines' own coordinates: both use that leaf radius
//              (Arqlet's `nodeRadius`, d3's collide radius). An earlier version scaled both drawings to the same area per
//              leaf first; that counted the dense packing inside Arqlet's clusters as overlaps and was wrong.
//   edgeLength mean length of the edges between leaves, in units of sqrt(area per leaf)
//   cohesion   mean distance of a leaf to the centre of its file (its parent), in the same unit: lower is more clustered
//
// Writes one JSON line per (size, engine) to benchmarks/results/overview-d3-<UTC>.jsonl with machine, load and revision.
// Run on an otherwise idle machine:
//
//   cargo build -p arqlet-wasm --target wasm32-unknown-unknown --release --locked
//   cp target/wasm32-unknown-unknown/release/arqlet_wasm.wasm packages/engine-client/arqlet.wasm
//   cd benchmarks/eval && npm ci && node overview-d3.mjs --sizes 10000,50000 --reps 3
//
// Evaluation only: d3-force (ISC) is pinned in package.json and not part of the engine.
import {readFileSync, appendFileSync, mkdtempSync} from "node:fs";
import {join} from "node:path";
import {tmpdir, cpus, loadavg, platform, release} from "node:os";
import {execFileSync} from "node:child_process";
import {forceCenter, forceCollide, forceLink, forceManyBody, forceSimulation} from "d3-force";
import {createEngine} from "../../packages/engine-client/wasm-client.js";

const ROOT = new URL("../../", import.meta.url).pathname;
const args = Object.fromEntries(process.argv.slice(2).map((a, i, all) => a.startsWith("--") ? [a.slice(2), all[i + 1]] : null).filter(Boolean));
const SIZES = (args.sizes ?? "10000").split(",").map(Number);
const REPS = Number(args.reps ?? 3);
const SEED = 42, D3_TICKS = 300, LEAF_RADIUS = 4, GAP = 2;

function seeded(seed) {
  let s = seed >>> 0;
  return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296);
}

function generate(n) {
  const dir = mkdtempSync(join(tmpdir(), "arqlet-overview-"));
  const out = join(dir, `code-${n}.json`);
  execFileSync("python3", [join(ROOT, "benchmarks/overview/generate.py"), "--nodes", String(n), "--seed", "1", "--out", out]);
  return JSON.parse(readFileSync(out, "utf8"));
}

function median(values) {
  const s = [...values].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

async function arqletLayout(run, graph) {
  const started = performance.now();
  const response = run({op: "overview", graph});
  const ms = performance.now() - started;
  if (!response.ok) throw Error(response.error);
  const o = response.overview, centres = new Map();
  for (let i = 0; i < graph.nodes.length; i++) centres.set(o.ids[i], {x: o.x[i], y: o.y[i]});
  return {centres, ms};
}

function d3Layout(graph) {
  const random = seeded(SEED);
  const nodes = graph.nodes.map(n => ({id: n.id, x: (random() - 0.5) * 1000, y: (random() - 0.5) * 1000}));
  const links = graph.edges.filter(e => e.target && e.target !== e.source).map(e => ({source: e.source, target: e.target}));
  const started = performance.now();
  forceSimulation(nodes)
    .randomSource(seeded(SEED))
    .force("link", forceLink(links).id(n => n.id))
    .force("charge", forceManyBody())
    .force("collide", forceCollide(LEAF_RADIUS + GAP / 2))
    .force("center", forceCenter(0, 0))
    .stop()
    .tick(D3_TICKS);
  return {centres: new Map(nodes.map(n => [n.id, {x: n.x, y: n.y}])), ms: performance.now() - started};
}

function measure(graph, centres) {
  const parent = new Map(graph.nodes.map(n => [n.id, n.parentId]));
  const hasChildren = new Set(graph.nodes.map(n => n.parentId).filter(Boolean));
  const leaves = graph.nodes.filter(n => !hasChildren.has(n.id)).map(n => n.id);
  // Scale both drawings to the same area per leaf: the bounding box of the leaves holds `leaves * unit²`.
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const id of leaves) { const c = centres.get(id); x0 = Math.min(x0, c.x); y0 = Math.min(y0, c.y); x1 = Math.max(x1, c.x); y1 = Math.max(y1, c.y); }
  const unit = Math.sqrt(Math.max((x1 - x0) * (y1 - y0), 1e-9) / leaves.length);
  // The leaf disc in the common scale: Arqlet's leaf radius against its own unit is the reference.
  const at = id => { const c = centres.get(id); return [(c.x - x0) / unit, (c.y - y0) / unit]; };
  // Overlaps in the engines' own coordinates: a grid of cells one disc across, neighbours only.
  const cell = new Map(), size = 2 * LEAF_RADIUS, key = (i, j) => `${i},${j}`;
  for (const id of leaves) {
    const c = centres.get(id), k = key(Math.floor(c.x / size), Math.floor(c.y / size));
    if (!cell.has(k)) cell.set(k, []);
    cell.get(k).push(c);
  }
  let overlaps = 0;
  for (const [k, points] of cell) {
    const [i, j] = k.split(",").map(Number);
    for (let di = -1; di <= 1; di++) for (let dj = -1; dj <= 1; dj++) {
      const other = cell.get(key(i + di, j + dj));
      if (!other) continue;
      for (const p of points) for (const q of other) {
        if (p !== q && (p.x < q.x || (p.x === q.x && p.y < q.y)) && Math.hypot(p.x - q.x, p.y - q.y) < size - 1e-6) overlaps++;
      }
    }
  }
  const leafSet = new Set(leaves);
  let length = 0, count = 0;
  for (const e of graph.edges) {
    if (!e.target || e.source === e.target || !leafSet.has(e.source) || !leafSet.has(e.target)) continue;
    const [ax, ay] = at(e.source), [bx, by] = at(e.target);
    length += Math.hypot(ax - bx, ay - by); count++;
  }
  const files = new Map();
  for (const id of leaves) { const p = parent.get(id); if (!p) continue; if (!files.has(p)) files.set(p, []); files.get(p).push(at(id)); }
  let spread = 0, members = 0;
  for (const points of files.values()) {
    const cx = points.reduce((s, p) => s + p[0], 0) / points.length, cy = points.reduce((s, p) => s + p[1], 0) / points.length;
    for (const p of points) { spread += Math.hypot(p[0] - cx, p[1] - cy); members++; }
  }
  return {overlaps, edgeLength: +(length / Math.max(count, 1)).toFixed(3), cohesion: +(spread / Math.max(members, 1)).toFixed(3), leaves: leaves.length};
}

function machine() {
  const sh = (...cmd) => { try { return execFileSync(cmd[0], cmd.slice(1), {encoding: "utf8"}).trim(); } catch { return null; } };
  return {platform: `${platform()} ${release()}`, cpu: cpus()[0]?.model, cores: cpus().length, node: process.version,
    revision: sh("git", "-C", ROOT, "rev-parse", "--short", "HEAD"), dirty: !!sh("git", "-C", ROOT, "status", "--porcelain")};
}

const run = await createEngine(readFileSync(join(ROOT, "packages/engine-client/arqlet.wasm")));
const stamp = new Date().toISOString().replace(/[-:]/g, "").replace(/\..*/, "Z");
const out = join(ROOT, `benchmarks/results/overview-d3-${stamp}.jsonl`);
const info = machine();
for (const n of SIZES) {
  const graph = generate(n);
  for (const [engine, layout] of [["arqlet-overview (wasm, 1 thread)", g => arqletLayout(run, g)], ["d3-force (flat, 300 ticks)", g => d3Layout(g)]]) {
    const times = [];
    let result = null;
    // One unmeasured warm-up, then the measured runs.
    for (let rep = 0; rep <= REPS; rep++) {
      result = await layout(graph);
      if (rep) times.push(result.ms);
    }
    const row = {dataset: `overview-code-${n}`, seed: 1, engine, reps: REPS, medianMs: Math.round(median(times)),
      minMs: Math.round(Math.min(...times)), maxMs: Math.round(Math.max(...times)), loadAverage: loadavg(),
      ...measure(graph, result.centres), ...info};
    console.log(JSON.stringify(row));
    appendFileSync(out, JSON.stringify(row) + "\n");
  }
}
console.log(`Wrote ${out}`);
