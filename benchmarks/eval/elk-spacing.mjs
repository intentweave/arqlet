// Visual and measured comparison: should `elkLayout` take ELK's default spacings when a graph names none?
// The same graph in ELK's JSON form (shared adapter) is laid out three ways: elk-rs, Arqlet's `elkLayout` as it is today
// (Arqlet's own spacings when the graph names none), and Arqlet's `elkLayout` with ELK's defaults named on the root
// (`elk.spacing.nodeNode` 20, `elk.layered.spacing.nodeNodeBetweenLayers` 20, `elk.spacing.edgeNode` 10). All three are read
// back by the same normalization and measured by the same `metrics` operation; each drawing is also written as SVG.
// node benchmarks/eval/elk-spacing.mjs [--limit 150] [--html OUT.html] [--out OUT.json] [DATASET ...]
import {spawnSync} from "node:child_process";
import {readFileSync, readdirSync, writeFileSync} from "node:fs";
import {join} from "node:path";
import ElkRs from "@openedges/elk-layout";
import {ROOT, LAYOUT_COMMAND, metrics} from "./lib.mjs";
import {elkLayout} from "./elk-adapter.mjs";

const args = process.argv.slice(2);
const option = (flag, fallback) => { const i = args.indexOf(flag); return i < 0 ? fallback : args.splice(i, 2)[1]; };
const limit = Number(option("--limit", 150));
const out = option("--out", null);
const html = option("--html", null);
const call = request => {
  const p = spawnSync(LAYOUT_COMMAND, [], {input: JSON.stringify(request), encoding: "utf8", maxBuffer: 1 << 29});
  if (p.status) throw new Error(p.stderr);
  return JSON.parse(p.stdout);
};
// ELK's defaults for the layered algorithm, as `elkLayout` would apply them when a graph names no spacing.
export const ELK_DEFAULTS = {
  "elk.spacing.nodeNode": 20,
  "elk.layered.spacing.nodeNodeBetweenLayers": 20,
  "elk.spacing.edgeNode": 10,
};
const arqletWith = extra => ({
  diagnostics: [],
  async layout(graph) {
    graph.layoutOptions = {...graph.layoutOptions, ...extra};
    const r = call({op: "elkLayout", graph});
    if (!r.ok) throw new Error(r.error);
    this.diagnostics = r.diagnostics;
    return r.graph;
  },
});
const arms = [
  ["elkRs", new ElkRs()],
  ["arqlet", arqletWith({})],
  ["arqletElkSpacing", arqletWith(ELK_DEFAULTS)],
];
const names = args.length ? args : readdirSync(join(ROOT, "benchmarks/datasets"))
  .filter(f => f.endsWith(".json") && !/^synthetic-20k|synthetic-100k|intentweave|overview-/.test(f)).map(f => f.slice(0, -5)).sort();
const extent = d => {
  const xs = d.nodes.flatMap(n => [n.x, n.x + n.w]).concat(d.edges.flatMap(e => e.points.map(p => p.x)));
  const ys = d.nodes.flatMap(n => [n.y, n.y + n.h]).concat(d.edges.flatMap(e => e.points.map(p => p.y)));
  return {x: Math.min(...xs), y: Math.min(...ys), w: Math.max(...xs) - Math.min(...xs), h: Math.max(...ys) - Math.min(...ys)};
};
const measure = d => {
  const m = metrics(d);
  const e = extent(d);
  return {crossings: m.crossings + m.sharedEndpointCrossings, bends: m.bends, hits: m.lineNodeHits, overlaps: m.siblingOverlaps,
    width: Math.round(e.w), height: Math.round(e.h), area: Math.round(e.w * e.h)};
};
const svgOf = d => {
  const e = extent(d);
  const pad = 10;
  const boxes = d.nodes.map(n => `<rect x="${n.x}" y="${n.y}" width="${n.w}" height="${n.h}" class="${n.container ? "c" : "n"}"/>`).join("");
  const lines = d.edges.map(r => `<polyline points="${r.points.map(p => `${p.x},${p.y}`).join(" ")}"/>`).join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${e.x - pad} ${e.y - pad} ${e.w + 2 * pad} ${e.h + 2 * pad}">${boxes}<g class="e">${lines}</g></svg>`;
};
const rows = [];
for (const name of names) {
  const graph = JSON.parse(readFileSync(join(ROOT, "benchmarks/datasets", `${name}.json`), "utf8"));
  graph.containerDirections = {}; graph.containerStrategies = {}; graph.defaultStrategy = "none";
  if (!["lr", "tb"].includes(graph.defaultDirection)) graph.defaultDirection = "lr";
  delete graph.defaultRouting; delete graph.containerRouting;
  const view = call({op: "import", graph, limit}).snapshot.graph;
  const row = {name, nodes: view.nodes.length, edges: view.edges.filter(e => e.target !== null).length};
  for (const [arm, engine] of arms) {
    try {
      const laid = await elkLayout(view, engine);
      row[arm] = {...measure(laid), ms: Math.round(laid.ms * 10) / 10, svg: svgOf(laid)};
    } catch (error) {
      row[arm] = {failed: String(error.message ?? error).split("\n")[0].slice(0, 200)};
    }
  }
  rows.push(row);
  const f = a => a.failed ? `FAILED ${a.failed}` : `${a.width}x${a.height} cr ${a.crossings} b ${a.bends} hits ${a.hits}`;
  console.log(name.padEnd(22), "elk-rs", f(row.elkRs).padEnd(34), "arqlet", f(row.arqlet).padEnd(34), "elk spacing", f(row.arqletElkSpacing));
}
const ok = rows.filter(r => arms.every(([a]) => !r[a].failed));
const sum = (arm, k) => ok.reduce((s, r) => s + r[arm][k], 0);
const ratio = (arm) => {
  const rs = ok.map(r => r[arm].area / r.elkRs.area).sort((a, b) => a - b);
  return {median: rs[rs.length >> 1], min: rs[0], max: rs[rs.length - 1]};
};
const summary = Object.fromEntries(arms.map(([a]) => [a, {crossings: sum(a, "crossings"), bends: sum(a, "bends"), hits: sum(a, "hits"),
  overlaps: sum(a, "overlaps"), areaToElkRs: a === "elkRs" ? null : ratio(a)}]));
console.log(JSON.stringify(summary, null, 1));
if (out) writeFileSync(out, JSON.stringify({kind: "elk-spacing-comparison", limit, elkDefaults: ELK_DEFAULTS, summary,
  rows: rows.map(r => Object.fromEntries(Object.entries(r).map(([k, v]) => [k, v && typeof v === "object" ? {...v, svg: undefined} : v])))}, null, 1) + "\n");
if (html) {
  const cell = a => a.failed ? `<td class="f">failed: ${a.failed}</td>` : `<td><div class="d">${a.svg}</div><div class="m">${a.width} × ${a.height} · crossings ${a.crossings} · bends ${a.bends} · line–node hits ${a.hits}</div></td>`;
  writeFileSync(html, `<!doctype html><meta charset="utf-8"><title>ELK spacing comparison</title>
<style>body{font:13px system-ui,sans-serif;margin:16px}table{border-collapse:collapse;width:100%}td,th{border:1px solid #ccc;padding:6px;vertical-align:top;width:33%}
.d svg{width:100%;height:320px;background:#fff}.n{fill:#e8eef8;stroke:#4a6fa5;stroke-width:1}.c{fill:none;stroke:#999;stroke-width:1}.e polyline{fill:none;stroke:#333;stroke-width:1}
.m{color:#444;margin-top:4px}h2{margin:18px 0 6px}</style>
<h1>elkLayout: Arqlet's spacings vs ELK's defaults (limit ${limit})</h1>
<p>ELK defaults named on the root: ${Object.entries(ELK_DEFAULTS).map(([k, v]) => `<code>${k}</code> ${v}`).join(", ")}.</p>
${rows.map(r => `<h2>${r.name} (${r.nodes} nodes, ${r.edges} edges)</h2><table><tr><th>elk-rs</th><th>Arqlet, own spacings (today)</th><th>Arqlet, ELK default spacings</th></tr><tr>${cell(r.elkRs)}${cell(r.arqlet)}${cell(r.arqletElkSpacing)}</tr></table>`).join("\n")}`);
}
