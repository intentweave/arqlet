// Mixed views (layered areas with organic, tree or radial areas, or straight routing) laid out by the pipeline before single
// pass and with `singlePassMixed`, measured by the same `metrics` operation (members of one bus may share lines).
// node benchmarks/eval/single-pass-mixed.mjs [--limit 150] [DATASET ...]
import {readFileSync} from "node:fs";
import {join} from "node:path";
import {ROOT, arqlet, normalizeArqlet, metrics} from "./lib.mjs";

const args = process.argv.slice(2);
const at = args.indexOf("--limit");
const limit = at < 0 ? 150 : Number(args.splice(at, 2)[1]);
const names = args.length ? args : ["can-bus", "mixed-large", "mixed-layouts", "self-loops"];
for (const name of names) {
  const graph = JSON.parse(readFileSync(join(ROOT, "benchmarks/datasets", `${name}.json`), "utf8"));
  for (const [arm, options] of [["before", {}], ["mixed", {singlePassMixed: true}]]) {
    const started = performance.now();
    const snapshot = arqlet({op: "import", graph, limit, options}).snapshot;
    const ms = Math.round(performance.now() - started);
    const drawn = normalizeArqlet(snapshot);
    drawn.buses = (snapshot.graph.buses ?? []).map(b => b.edges);
    const m = metrics(drawn);
    const used = (snapshot.layoutDiagnostics ?? []).some(d => d.startsWith("Single pass:"));
    console.log(name.padEnd(14), arm.padEnd(7), JSON.stringify({singlePass: used, overlaps: m.siblingOverlaps + m.crossBranchOverlaps,
      hits: m.lineNodeHits, shared: m.sharedLinePairs, crossings: m.crossings + m.sharedEndpointCrossings, hugging: m.hugPairs,
      bends: m.bends, area: Math.round((m.area ?? 0) / 1000), geometryIssues: (snapshot.geometryIssues ?? []).length, ms}));
  }
}
