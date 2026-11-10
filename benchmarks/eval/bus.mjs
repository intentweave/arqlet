// Explicit buses on the corpus views: a view's own buses (can-bus), else every node's outgoing edges (two or more, no
// self-loops) form a bus. Each view is laid
// out without and with the buses (default single pass) and measured by the same `metrics` operation, with the members of one
// bus allowed to share lines; "ink" is the length of all lines with shared stretches counted once.
// node benchmarks/eval/bus.mjs [--limit 150] [--out benchmarks/results/NAME.json] [DATASET ...]
import {readFileSync, readdirSync, writeFileSync} from "node:fs";
import {join} from "node:path";
import {ROOT, arqlet, normalizeArqlet, metrics} from "./lib.mjs";

const args = process.argv.slice(2);
const option = (flag, fallback) => { const i = args.indexOf(flag); return i < 0 ? fallback : args.splice(i, 2)[1]; };
const limit = Number(option("--limit", 150));
const out = option("--out", null);
const names = args.length ? args : readdirSync(join(ROOT, "benchmarks/datasets"))
  .filter(f => f.endsWith(".json") && !/^synthetic-20k|intentweave/.test(f)).map(f => f.slice(0, -5)).sort();

// The length of the union of axis-parallel segments (others counted whole).
function ink(edges) {
  const lines = new Map();
  let other = 0;
  for (const e of edges) for (let i = 0; i + 1 < e.points.length; i++) {
    const [a, b] = [e.points[i], e.points[i + 1]];
    const key = Math.abs(a.y - b.y) < 1e-6 ? `h${Math.round(a.y * 1000)}` : Math.abs(a.x - b.x) < 1e-6 ? `v${Math.round(a.x * 1000)}` : null;
    if (!key) { other += Math.hypot(a.x - b.x, a.y - b.y); continue; }
    const [lo, hi] = key[0] === "h" ? [Math.min(a.x, b.x), Math.max(a.x, b.x)] : [Math.min(a.y, b.y), Math.max(a.y, b.y)];
    if (!lines.has(key)) lines.set(key, []);
    lines.get(key).push([lo, hi]);
  }
  let total = other;
  for (const spans of lines.values()) {
    spans.sort((p, q) => p[0] - q[0]);
    let [lo, hi] = spans[0];
    for (const [a, b] of spans.slice(1)) {
      if (a > hi) { total += hi - lo; [lo, hi] = [a, b]; } else hi = Math.max(hi, b);
    }
    total += hi - lo;
  }
  return total;
}

const rows = [];
for (const name of names) {
  const graph = JSON.parse(readFileSync(join(ROOT, "benchmarks/datasets", `${name}.json`), "utf8"));
  const view = arqlet({op: "import", graph, limit}).snapshot.graph;
  // A view's own buses (can-bus), else one bus per fan-out.
  const fans = {};
  for (const e of view.edges) if (e.target && e.target !== e.source) (fans[e.source] ??= []).push(e.id);
  const buses = view.buses?.length ? view.buses
    : Object.entries(fans).filter(([, l]) => l.length >= 2).map(([s, l]) => ({id: `bus:${s}`, edges: l}));
  const row = {name, nodes: view.nodes.length, edges: view.edges.length, buses: buses.length, members: buses.reduce((n, b) => n + b.edges.length, 0)};
  for (const [arm, graphBuses] of [["none", []], ["bus", buses]]) {
    const snapshot = arqlet({op: "import", graph: {...view, buses: graphBuses}, limit: 10000}).snapshot;
    const drawn = normalizeArqlet(snapshot);
    if (arm === "bus") drawn.buses = buses.map(b => b.edges);
    const m = metrics(drawn);
    row[arm] = {crossings: m.crossings + m.sharedEndpointCrossings, hits: m.lineNodeHits, shared: m.sharedLinePairs, overlaps: m.siblingOverlaps,
      bends: m.bends, ink: Math.round(ink(drawn.edges)),
      apart: (snapshot.layoutDiagnostics ?? []).filter(d => d.startsWith("Bus ")).length};
  }
  rows.push(row);
  const f = a => `cr ${a.crossings} hits ${a.hits} sh ${a.shared} b ${a.bends} ink ${a.ink}${a.apart ? ` apart ${a.apart}` : ""}`;
  console.log(name.padEnd(22), String(row.buses).padStart(3), "buses", f(row.none).padEnd(44), "->", f(row.bus));
}
const sum = (arm, k) => rows.reduce((s, r) => s + r[arm][k], 0);
console.log("sum", ["crossings", "hits", "shared", "bends", "ink", "apart"].map(k => `${k} ${sum("none", k)} -> ${sum("bus", k)}`).join(", "));
if (out) writeFileSync(out, JSON.stringify({kind: "bus-routing", limit, rows}, null, 1) + "\n");
