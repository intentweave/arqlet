// Organic (force/stress) layout comparison: Arqlet's organic placement against d3-force and Cytoscape CoSE on the same
// flat graphs. Every engine places nodes of 180 × 54; every drawing is measured with straight centre-to-centre edges, by the
// shared `metrics` operation (node overlaps, crossings, line–node hits) and by the organic measures computed here:
//   - edge length CV: standard deviation / mean of the edge lengths (0 = all edges equally long),
//   - stress: normalized stress against graph distances, at the scale that fits best (0 = distances match),
//   - angular resolution: mean over nodes of (smallest angle between incident edges) / (360° / degree),
//   - closest pair: smallest gap between two node boxes, over the mean edge length,
//   - area per node: bounding box / nodes, in units of one node box.
// Engine settings follow compare.mjs --flat-force (d3-force 300 ticks, seed 42; CoSE 1000 iterations). Arqlet runs as one
// `layout-command` process per call, so its times include process start and JSON transport.
// node benchmarks/eval/organic.mjs [--out benchmarks/results/NAME.json] [--sizes 500,1000,2000] [DATASET ...]
import {readFileSync, writeFileSync} from "node:fs";
import {join} from "node:path";
import {cpus, loadavg} from "node:os";
import {execFileSync} from "node:child_process";
import cytoscape from "cytoscape";
import {forceCenter, forceCollide, forceLink, forceManyBody, forceSimulation} from "d3-force";
import {ROOT, LEAF, arqlet, metrics} from "./lib.mjs";

const args = process.argv.slice(2);
const option = (flag, fallback) => { const i = args.indexOf(flag); return i < 0 ? fallback : args.splice(i, 2)[1]; };
const out = option("--out", null);
const sizes = option("--sizes", "500,1000,2000").split(",").filter(Boolean).map(Number);
const names = args.length ? args : ["organic-25", "organic-50", "organic-100", "organic-200", "organic-300"];
const D3_TICKS = 300, COSE_ITERATIONS = 1000, SEED = 42;

function seeded(seed) {
  let s = seed >>> 0;
  return () => ((s = (s * 1664525 + 1013904223) >>> 0) / 4294967296);
}
// A connected sparse graph like the generator's organic ladder: a random tree plus chords (about 1.3 edges per node).
function synthetic(n) {
  const random = seeded(20261109 + n);
  const nodes = [...Array(n)].map((_, i) => ({id: `n${i}`, kind: "node", label: `n${i}`, parentId: null}));
  const edges = [];
  for (let i = 1; i < n; i++) edges.push({id: `t${i}`, kind: "link", source: `n${Math.floor(random() * i)}`, target: `n${i}`});
  for (let k = 0; k < Math.round(n * 0.3); k++) {
    const a = Math.floor(random() * n), b = Math.floor(random() * n);
    if (a !== b) edges.push({id: `c${k}`, kind: "link", source: `n${a}`, target: `n${b}`});
  }
  return {defaultDirection: "auto", defaultStrategy: "organic", defaultRouting: "straight", containerDirections: {}, nodes, edges};
}
function initial(index, count) {
  const angle = 2 * Math.PI * index / Math.max(count, 1), radius = 120 + 18 * Math.sqrt(count);
  return {x: radius * Math.cos(angle), y: radius * Math.sin(angle)};
}

// ---------- Engines: each returns centres by node id and its time ----------
function arqletLayout(graph, options) {
  const started = performance.now();
  const response = arqlet({op: "import", graph, limit: graph.nodes.length, options});
  const ms = performance.now() - started;
  const s = response.snapshot;
  const centres = new Map(Object.entries(s.boxes).map(([id, r]) => [id, {x: r.x + r.width / 2, y: r.y + r.height / 2}]));
  return {centres, ms, issues: (s.geometryIssues ?? []).length};
}
function d3Layout(graph) {
  const nodes = graph.nodes.map((n, i) => ({id: n.id, ...initial(i, graph.nodes.length)}));
  const links = graph.edges.filter(e => e.target && e.target !== e.source).map(e => ({source: e.source, target: e.target}));
  const started = performance.now();
  forceSimulation(nodes)
    .randomSource(seeded(SEED))
    .force("link", forceLink(links).id(n => n.id).distance(200).strength(0.15))
    .force("charge", forceManyBody().strength(-900))
    .force("collide", forceCollide(Math.hypot(LEAF.width, LEAF.height) / 2 + 12).iterations(2))
    .force("center", forceCenter(0, 0))
    .stop()
    .tick(D3_TICKS);
  return {centres: new Map(nodes.map(n => [n.id, {x: n.x, y: n.y}])), ms: performance.now() - started};
}
function coseLayout(graph) {
  const cy = cytoscape({
    headless: true, styleEnabled: true,
    elements: [
      ...graph.nodes.map((n, i) => ({data: {id: n.id}, position: initial(i, graph.nodes.length)})),
      ...graph.edges.filter(e => e.target && e.target !== e.source).map(e => ({data: {id: e.id, source: e.source, target: e.target}})),
    ],
    style: [{selector: "node", style: {width: LEAF.width, height: LEAF.height}}],
  });
  const started = performance.now();
  cy.layout({name: "cose", animate: false, fit: false, randomize: false, numIter: COSE_ITERATIONS,
    nodeRepulsion: 4500, idealEdgeLength: 200, edgeElasticity: 100, gravity: 0.25}).run();
  const ms = performance.now() - started;
  const centres = new Map(cy.nodes().map(n => [n.id(), {...n.position()}]));
  cy.destroy();
  return {centres, ms};
}

// ---------- Measures ----------
function measure(graph, centres) {
  const ids = graph.nodes.map(n => n.id);
  const edges = graph.edges.filter(e => e.target && e.target !== e.source && centres.has(e.source) && centres.has(e.target));
  const drawing = {
    nodes: ids.map(id => { const c = centres.get(id); return {id, parentId: null, container: false, x: c.x - LEAF.width / 2, y: c.y - LEAF.height / 2, w: LEAF.width, h: LEAF.height}; }),
    // The straight line between the two boxes' borders (from centre to centre, cut where it leaves each box).
    edges: edges.map(e => {
      const a = centres.get(e.source), b = centres.get(e.target);
      const dx = b.x - a.x, dy = b.y - a.y;
      const cut = Math.min(LEAF.width / 2 / Math.max(Math.abs(dx), 1e-9), LEAF.height / 2 / Math.max(Math.abs(dy), 1e-9), 0.5);
      // Boxes that overlap leave nothing between their borders: the line from centre to centre then.
      const c = cut < 0.5 - 1e-6 ? cut : 0;
      return {id: e.id, source: e.source, target: e.target,
        points: [{x: a.x + dx * c, y: a.y + dy * c}, {x: b.x - dx * c, y: b.y - dy * c}]};
    }),
  };
  const m = metrics(drawing);
  const lengths = edges.map(e => { const a = centres.get(e.source), b = centres.get(e.target); return Math.hypot(a.x - b.x, a.y - b.y); });
  const mean = lengths.reduce((s, v) => s + v, 0) / Math.max(lengths.length, 1);
  const cv = Math.sqrt(lengths.reduce((s, v) => s + (v - mean) ** 2, 0) / Math.max(lengths.length, 1)) / mean;
  // Graph distances (BFS) for stress, on the undirected graph.
  const index = new Map(ids.map((id, i) => [id, i]));
  const adjacency = ids.map(() => []);
  for (const e of edges) { const a = index.get(e.source), b = index.get(e.target); adjacency[a].push(b); adjacency[b].push(a); }
  const pos = ids.map(id => centres.get(id));
  let num = 0, den = 0, pairs = [];
  // At most 600 sources (every source on smaller graphs), so the measure stays affordable on 2000 nodes.
  const step = Math.max(1, Math.floor(ids.length / 600));
  for (let s = 0; s < ids.length; s += step) {
    const dist = new Array(ids.length).fill(-1); dist[s] = 0; const queue = [s];
    for (let q = 0; q < queue.length; q++) for (const v of adjacency[queue[q]]) if (dist[v] < 0) { dist[v] = dist[queue[q]] + 1; queue.push(v); }
    for (let t = 0; t < ids.length; t++) if (t !== s && dist[t] > 0) {
      const d = Math.hypot(pos[s].x - pos[t].x, pos[s].y - pos[t].y);
      pairs.push([d, dist[t]]);
      num += d / dist[t]; den += (d * d) / (dist[t] * dist[t]);
    }
  }
  // Scale that minimizes Σ (s·d_ij − g_ij)² / g_ij² over the pairs: s = Σ(d/g) / Σ(d²/g²) after dividing by g.
  const scale = num / den;
  const stress = pairs.reduce((sum, [d, g]) => sum + ((scale * d - g) / g) ** 2, 0) / Math.max(pairs.length, 1);
  // Angular resolution.
  let angular = 0, counted = 0;
  for (let i = 0; i < ids.length; i++) {
    const neighbours = [...new Set(adjacency[i])];
    if (neighbours.length < 2) continue;
    const angles = neighbours.map(v => Math.atan2(pos[v].y - pos[i].y, pos[v].x - pos[i].x)).sort((a, b) => a - b);
    let smallest = 2 * Math.PI - (angles.at(-1) - angles[0]);
    for (let k = 1; k < angles.length; k++) smallest = Math.min(smallest, angles[k] - angles[k - 1]);
    angular += smallest / (2 * Math.PI / neighbours.length); counted++;
  }
  // Closest pair of boxes (gap; negative when they overlap), over the mean edge length; a grid keeps it near linear.
  let closest = Infinity;
  const cell = 400, grid = new Map();
  for (let i = 0; i < ids.length; i++) { const k = `${Math.floor(pos[i].x / cell)},${Math.floor(pos[i].y / cell)}`; (grid.get(k) ?? grid.set(k, []).get(k)).push(i); }
  for (let i = 0; i < ids.length; i++) {
    const cx = Math.floor(pos[i].x / cell), cy = Math.floor(pos[i].y / cell);
    for (let dx = -1; dx <= 1; dx++) for (let dy = -1; dy <= 1; dy++) for (const j of grid.get(`${cx + dx},${cy + dy}`) ?? []) {
      if (j <= i) continue;
      const gx = Math.abs(pos[i].x - pos[j].x) - LEAF.width, gy = Math.abs(pos[i].y - pos[j].y) - LEAF.height;
      closest = Math.min(closest, Math.max(gx, gy));
    }
  }
  const xs = pos.map(p => p.x), ys = pos.map(p => p.y);
  const width = Math.max(...xs) - Math.min(...xs) + LEAF.width, height = Math.max(...ys) - Math.min(...ys) + LEAF.height;
  const round = (v, d = 3) => Math.round(v * 10 ** d) / 10 ** d;
  return {
    overlaps: m.siblingOverlaps + m.crossBranchOverlaps, crossings: m.crossings + m.sharedEndpointCrossings, hits: m.lineNodeHits,
    edgeLengthCv: round(cv), stress: round(stress, 4), angularResolution: round(angular / Math.max(counted, 1)),
    closestPair: round(closest / mean), areaPerNode: round(width * height / ids.length / (LEAF.width * LEAF.height), 2),
  };
}

// ---------- Run ----------
const graphs = [
  ...names.map(name => [name, JSON.parse(readFileSync(join(ROOT, "benchmarks/datasets", `${name}.json`), "utf8"))]),
  ...sizes.map(n => [`synthetic-organic-${n}`, synthetic(n)]),
];
const rows = [];
for (const [name, graph] of graphs) {
  const row = {name, nodes: graph.nodes.length, edges: graph.edges.length, engines: {}};
  const arms = [
    ["arqlet", () => arqletLayout(graph, {})],
    // sparseStress takes long from 500 nodes (measured: 26 s at 500 after the repair speedups); larger graphs leave it out.
    ...(graph.nodes.length <= 500 ? [["arqlet sparseStress", () => arqletLayout(graph, {sparseStress: true})]] : []),
    ["arqlet scalableStress", () => arqletLayout(graph, {scalableStress: true})],
    ["d3-force", () => d3Layout(graph)],
    ["cytoscape cose", () => coseLayout(graph)],
  ];
  for (const [engine, run] of arms) {
    try {
      const result = run();
      row.engines[engine] = {...measure(graph, result.centres), ms: Math.round(result.ms), ...(result.issues !== undefined ? {geometryIssues: result.issues} : {})};
    } catch (error) {
      row.engines[engine] = {failed: String(error.message ?? error).split("\n")[0].slice(0, 160)};
    }
    const e = row.engines[engine];
    console.log(name.padEnd(24), engine.padEnd(24), e.failed ? `FAILED ${e.failed}` :
      `ov ${e.overlaps} cr ${e.crossings} hits ${e.hits} cv ${e.edgeLengthCv} stress ${e.stress} ang ${e.angularResolution} gap ${e.closestPair} area ${e.areaPerNode} ${e.ms} ms`);
  }
  rows.push(row);
}
if (out) {
  const environment = {cpu: cpus()[0]?.model, cpus: cpus().length, load: loadavg(), node: process.version,
    git: execFileSync("git", ["rev-parse", "--short", "HEAD"], {cwd: ROOT, encoding: "utf8"}).trim()};
  writeFileSync(out, JSON.stringify({kind: "organic-comparison", environment, settings: {d3Ticks: D3_TICKS, coseIterations: COSE_ITERATIONS, seed: SEED}, rows}, null, 1) + "\n");
}
