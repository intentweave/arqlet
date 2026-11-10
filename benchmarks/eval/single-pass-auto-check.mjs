// Checks `singlePassAuto` (the default since engine 0.2, ADR 0016) on the unmodified datasets: every view imports and refines,
// and a view on which single pass does not run is the drawing of the pipeline before single pass, before and after refinement,
// apart from the options and the report of the fallback.
// node benchmarks/eval/single-pass-auto-check.mjs [--limit 150]. Exits with 1 on a problem.
import {spawnSync} from "node:child_process";
import {readFileSync, readdirSync} from "node:fs";
import {join} from "node:path";
import {ROOT} from "./lib.mjs";

const args = process.argv.slice(2);
const limit = args[0] === "--limit" ? Number(args[1]) : 150;
const binary = join(ROOT, "target/release/layout-command");
const NOTE = "Single pass was not used:";
const call = request => {
  const p = spawnSync(binary, [], {input: JSON.stringify(request), encoding: "utf8", maxBuffer: 1 << 29, timeout: 1_800_000});
  if (p.error) throw p.error;
  return JSON.parse(p.stdout);
};
const refine = s => call({op: "refine", snapshot: s, expectedRevision: s.revision, expectedLayoutVersion: s.layoutVersion ?? 0, threads: 1});
const drawing = s => {
  const copy = structuredClone(s);
  delete copy.options;
  copy.layoutDiagnostics = (copy.layoutDiagnostics ?? []).filter(d => !d.startsWith(NOTE));
  if (!copy.layoutDiagnostics.length) delete copy.layoutDiagnostics;
  return JSON.stringify(copy);
};
const problems = [], reasons = {};
const names = readdirSync(join(ROOT, "benchmarks/datasets")).filter(f => f.endsWith(".json") && !/^synthetic-20k|intentweave/.test(f)).sort();
let singlePass = 0, fallback = 0;
for (const file of names) {
  const graph = JSON.parse(readFileSync(join(ROOT, "benchmarks/datasets", file), "utf8"));
  const view = call({op: "import", graph, limit}).snapshot.graph;
  const tried = call({op: "import", graph: view, limit: 1000, options: {singlePassAuto: true}});
  if (!tried.ok) {
    problems.push(`${file}: import: ${tried.error}`);
    continue;
  }
  const refined = refine(tried.snapshot);
  if (!refined.ok) problems.push(`${file}: refine: ${refined.error}`);
  // On by default and then omitted; a fallback writes `false`.
  if (tried.snapshot.options?.singlePass !== false) {
    singlePass++;
    continue;
  }
  fallback++;
  const note = (tried.snapshot.layoutDiagnostics ?? []).find(d => d.startsWith(NOTE));
  if (note) reasons[note] = (reasons[note] ?? 0) + 1;
  else problems.push(`${file}: no report of the fallback`);
  const plain = call({op: "import", graph: view, limit: 1000, options: {singlePassAuto: false, singlePass: false, abstractRouting: false}}).snapshot;
  if (drawing(tried.snapshot) !== drawing(plain)) problems.push(`${file}: import differs from the default`);
  if (refined.ok && drawing(refined.snapshot) !== drawing(refine(plain).snapshot)) problems.push(`${file}: refinement differs from the default`);
}
console.log(`single pass ${singlePass}, fallback ${fallback}, problems ${problems.length}`);
console.log(reasons);
for (const p of problems) console.log(p);
process.exit(problems.length ? 1 : 0);
