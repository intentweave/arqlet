// ELK-format comparison: the same graph in ELK's JSON form (built by the shared adapter) laid out by elk-rs and by Arqlet's
// `elkLayout` operation, both read back by the same normalization and measured by the same `metrics` operation. Uniform task
// (one direction, layered, orthogonal), as in single-pass.mjs --corpus.
// node benchmarks/eval/elk-compat.mjs [--limit 150] [--wasm] [--out benchmarks/results/NAME.json] [DATASET ...]
// `--wasm` runs Arqlet in this process as the WASM module that apps/playground/build.sh writes (as elk-rs runs here), so
// the times compare; without it Arqlet runs as one `layout-command` process per call, whose times include the start and the
// JSON transport.
import {spawnSync} from "node:child_process";
import {readFileSync, readdirSync, writeFileSync} from "node:fs";
import {join} from "node:path";
import ElkRs from "@openedges/elk-layout";
import {ROOT, LAYOUT_COMMAND, metrics} from "./lib.mjs";
import {elkLayout} from "./elk-adapter.mjs";
import {createEngine} from "../../packages/engine-client/wasm-client.js";

const args = process.argv.slice(2);
const option = (flag, fallback) => { const i = args.indexOf(flag); return i < 0 ? fallback : args.splice(i, 2)[1]; };
const limit = Number(option("--limit", 150));
const out = option("--out", null);
const wasmAt = args.indexOf("--wasm");
const wasm = wasmAt >= 0 ? (args.splice(wasmAt, 1), await createEngine(readFileSync(join(ROOT, "apps/playground/generated/arqlet.wasm")))) : null;
const call = request => {
  const p = spawnSync(LAYOUT_COMMAND, [], {input: JSON.stringify(request), encoding: "utf8", maxBuffer: 1 << 29});
  if (p.status) throw new Error(p.stderr);
  return JSON.parse(p.stdout);
};
// Arqlet behind the same `layout(graph)` call the adapter makes on elk-rs.
const arqlet = {
  diagnostics: [],
  async layout(graph) {
    const request = {op: "elkLayout", graph};
    const r = wasm ? wasm(request) : call(request);
    if (!r.ok) throw new Error(r.error);
    this.diagnostics = r.diagnostics;
    return r.graph;
  },
};
const elkRs = new ElkRs();
const names = args.length ? args : readdirSync(join(ROOT, "benchmarks/datasets"))
  .filter(f => f.endsWith(".json") && !/^synthetic-20k|intentweave/.test(f)).map(f => f.slice(0, -5)).sort();
const measure = d => { const m = metrics(d); return {crossings: m.crossings + m.sharedEndpointCrossings, bends: m.bends, hits: m.lineNodeHits, shared: m.sharedLinePairs, overlaps: m.siblingOverlaps, hug: m.hugPairs}; };
const rows = [];
for (const name of names) {
  const graph = JSON.parse(readFileSync(join(ROOT, "benchmarks/datasets", `${name}.json`), "utf8"));
  graph.containerDirections = {}; graph.containerStrategies = {}; graph.defaultStrategy = "none";
  if (!["lr", "tb"].includes(graph.defaultDirection)) graph.defaultDirection = "lr";
  delete graph.defaultRouting; delete graph.containerRouting;
  const view = call({op: "import", graph, limit}).snapshot.graph;
  const row = {name, nodes: view.nodes.length, edges: view.edges.filter(e => e.target !== null).length};
  for (const [arm, engine] of [["elkRs", elkRs], ["arqlet", arqlet]]) {
    try {
      const laid = await elkLayout(view, engine);
      row[arm] = {...measure({nodes: laid.nodes, edges: laid.edges}), ms: Math.round(laid.ms * 10) / 10};
      if (arm === "arqlet") row.diagnostics = engine.diagnostics;
    } catch (error) {
      row[arm] = {failed: String(error.message ?? error).split("\n")[0].slice(0, 200)};
    }
  }
  rows.push(row);
  const f = a => a.failed ? `FAILED ${a.failed}` : `cr ${a.crossings} b ${a.bends} hits ${a.hits} sh ${a.shared} ov ${a.overlaps} ${a.ms} ms`;
  console.log(name.padEnd(22), "elk-rs", f(row.elkRs).padEnd(46), "arqlet", f(row.arqlet));
}
const sum = (arm, k) => rows.reduce((s, r) => s + (r[arm]?.[k] ?? 0), 0);
console.log("sum crossings", sum("elkRs", "crossings"), sum("arqlet", "crossings"), "bends", sum("elkRs", "bends"), sum("arqlet", "bends"),
  "hits", sum("elkRs", "hits"), sum("arqlet", "hits"), "failed", rows.filter(r => r.elkRs.failed).length, rows.filter(r => r.arqlet.failed).length);
if (out) writeFileSync(out, JSON.stringify({kind: "elk-format-comparison", limit, arqlet: wasm ? "wasm in process" : "layout-command per call", rows}, null, 1) + "\n");
