// Quality-gated comparison of the provisional single-pass mode against default, abstract and elk-rs.
// node benchmarks/eval/single-pass.mjs [--repeat 3] [--seeds 1] [--out benchmarks/results/NAME.json] [--dump DIR]
//   [--arms default,abstract,singlePass] [--no-elk] [--corpus [--limit 150] [--raw] [DATASET ...]] [--binary PATH]
// Without --corpus: the three frozen views of the focused replay. With --corpus: every dataset in benchmarks/datasets except
// synthetic-20k and intentweave-export (or the named ones), with uniform direction, cut to a view of --limit nodes by a
// default import. An arm that rejects a view (unsupported options) is recorded as a failure, not hidden. With --raw the
// datasets keep their own directions, strategies and routing: the real task. elk-rs then gets the containers' own directions
// and lays out mixed ones separately (see the adapter); its organic, tree and straight regions are not mapped.
// --seeds N runs every Arqlet arm with the option `orderSeed` 0..N-1 (the shuffled starts of the hierarchical order) and
// reports the mean of each metric over the seeds (`perSeed` keeps them); times come from seed 0. The quality gate of ADR 0016
// compares views by these means.
import assert from "node:assert/strict";
import {execFileSync, spawnSync} from "node:child_process";
import {createHash} from "node:crypto";
import {readFileSync, writeFileSync, readdirSync} from "node:fs";
import {cpus, loadavg, arch, platform} from "node:os";
import {join} from "node:path";
import ElkRs from "@openedges/elk-layout";
import {ROOT, normalizeArqlet, metrics} from "./lib.mjs";
import {elkLayout} from "./elk-adapter.mjs";

const args = process.argv.slice(2);
const option = (flag, fallback) => {
  const i = args.indexOf(flag);
  return i < 0 ? fallback : args.splice(i, 2)[1];
};
const reps = Number(option("--repeat", 3));
const seeds = Number(option("--seeds", 1));
const out = option("--out", null);
const dump = option("--dump", null);
const replay = option("--replay", join(ROOT, "benchmarks/results/focused-pipeline-20261105.jsonl"));
const corpus = args.includes("--corpus") && (args.splice(args.indexOf("--corpus"), 1), true);
const limit = Number(option("--limit", 150));
const armNames = option("--arms", "default,abstract,singlePass").split(",");
const raw = args.includes("--raw") && (args.splice(args.indexOf("--raw"), 1), true);
const elk = !(args.includes("--no-elk") && (args.splice(args.indexOf("--no-elk"), 1), true));
const binary = option("--binary", join(ROOT, "target/release/layout-command"));
assert((corpus || args.length === 0) && (corpus || !raw) && Number.isInteger(reps) && reps > 0 && Number.isInteger(seeds) && seeds > 0);

const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const median = values => {
  const s = values.toSorted((a, b) => a - b), m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const command = request => {
  const started = performance.now();
  const p = spawnSync(binary, [], {input: JSON.stringify(request), encoding: "utf8", maxBuffer: 256 * 1024 * 1024, timeout: 300_000});
  if (p.error) throw p.error;
  assert.equal(p.status, 0, p.stderr);
  const response = JSON.parse(p.stdout);
  assert.equal(response.ok, true, response.error);
  return {snapshot: response.snapshot, ms: performance.now() - started};
};
const pipeline = (graph, options) => {
  const started = performance.now();
  const initial = command({op: "import", graph, limit: 1000, options});
  const refined = command({op: "refine", snapshot: initial.snapshot, expectedRevision: initial.snapshot.revision,
    expectedLayoutVersion: initial.snapshot.layoutVersion ?? 0, threads: 1});
  return {initial: initial.snapshot, refined: refined.snapshot, importMs: initial.ms, pipelineMs: performance.now() - started};
};
// Hard gates shared by every arm: complete edges in logical direction, finite orthogonal routes.
function validate(view, drawing) {
  const edges = view.edges.filter(e => e.target !== null);
  assert.equal(drawing.edges.length, edges.length, "complete edge partition");
  const byId = new Map(drawing.edges.map(e => [e.id, e]));
  for (const edge of edges) {
    const route = byId.get(edge.id);
    assert(route, `route ${edge.id}`);
    assert.equal(route.source, edge.source); assert.equal(route.target, edge.target);
    assert(route.points.length >= 2 && route.points.every(p => Number.isFinite(p.x) && Number.isFinite(p.y)));
  }
  const m = metrics(drawing);
  return {totalCrossings: m.crossings + m.sharedEndpointCrossings, crossings: m.crossings,
    sharedEndpointCrossings: m.sharedEndpointCrossings, bends: m.bends, lineNodeHits: m.lineNodeHits,
    siblingOverlaps: m.siblingOverlaps, sharedLinePairs: m.sharedLinePairs, hugPairs: m.hugPairs,
    area: Math.round(m.area), length: Math.round(m.length)};
}
const delegated = snapshot => {
  const line = (snapshot.layoutDiagnostics ?? []).find(d => d.startsWith("Single pass:") || d.startsWith("Abstract routing:"));
  return line ?? null;
};

// The metrics of one run of an arm.
const measured = (view, result) => ({initial: validate(view, normalizeArqlet(result.initial)),
  refined: validate(view, normalizeArqlet(result.refined)), diagnostic: delegated(result.refined),
  geometryIssues: result.refined.geometryIssues?.length ?? 0, routingIssues: result.refined.routingIssues?.length ?? 0});
// The mean of every metric over the seeds (one seed: the metrics as they are).
const meanOf = list => list.length === 1 ? list[0]
  : Object.fromEntries(Object.keys(list[0]).map(k => [k, list.reduce((sum, m) => sum + m[k], 0) / list.length]));

// "default" is the pipeline before single pass (the default until engine 0.2, ADR 0016), so that the reports stay comparable.
const legacy = {singlePassAuto: false, singlePass: false, abstractRouting: false};
const allArms = {default: legacy, abstract: {...legacy, abstractRouting: true},
  singlePass: {singlePassAuto: false, abstractRouting: true, singlePass: true}};
assert(armNames.every(name => name in allArms), `--arms takes ${Object.keys(allArms).join(",")}`);
const arms = Object.fromEntries(armNames.map(name => [name, allArms[name]]));
const elkRs = new ElkRs();
function corpusCases() {
  const names = args.length ? args : readdirSync(join(ROOT, "benchmarks/datasets"))
    .filter(f => f.endsWith(".json") && !/^synthetic-20k|intentweave/.test(f)).map(f => f.slice(0, -5)).sort();
  return names.map(name => {
    const graph = JSON.parse(readFileSync(join(ROOT, "benchmarks/datasets", `${name}.json`), "utf8"));
    // One fixed flow axis and orthogonal routing everywhere: the task elk-rs solves (its adapter maps auto to RIGHT).
    if (!raw) {
      graph.containerDirections = {}; graph.containerStrategies = {}; graph.defaultStrategy = "none";
      if (!["lr", "tb"].includes(graph.defaultDirection)) graph.defaultDirection = "lr";
      delete graph.defaultRouting; delete graph.containerRouting;
    }
    return {name, view: command({op: "import", graph, limit}).snapshot.graph};
  });
}
const frozen = corpus ? {cases: corpusCases()} : JSON.parse(readFileSync(replay, "utf8"));
const report = {kind: "single-pass-comparison", generatedAt: new Date().toISOString(), reps,
  environment: {cpu: cpus()[0].model, logicalCpus: cpus().length, os: platform(), arch: arch(), node: process.version,
    revision: execFileSync("git", ["rev-parse", "HEAD"], {cwd: ROOT, encoding: "utf8"}).trim(), dirty: true, loadStart: loadavg()},
  binary: hash(readFileSync(binary)),
  input: corpus ? (raw ? {corpus: true, limit, raw: true} : {corpus: true, limit, uniformDirection: true, orthogonal: true, autoDirection: "lr"}) : {replay: {path: replay, sha256: hash(readFileSync(replay))}},
  note: "Public arms: import plus refine through two layout-command processes with JSON transport. elk-rs: layout() only, in-process. Shared-machine diagnostic unless stated otherwise.",
  cases: []};
for (const c of frozen.cases) {
  const view = c.view;
  const row = {name: c.name, nodes: view.nodes.length, edges: view.edges.length, arms: {}};
  for (const [name, options] of Object.entries(arms)) {
    const samples = [];
    let result;
    const perSeed = [];
    try {
      for (let i = 0; i <= reps; i++) {
        result = pipeline(view, options);
        if (i) samples.push({importMs: result.importMs, pipelineMs: result.pipelineMs});
      }
      perSeed.push(measured(view, result));
      if (dump) writeFileSync(join(dump, `${c.name}-${name}.json`), JSON.stringify(result.refined));
      for (let seed = 1; seed < seeds; seed++) perSeed.push(measured(view, pipeline(view, {...options, orderSeed: seed})));
    } catch (error) {
      row.arms[name] = {failed: String(error.message ?? error).split("\n")[0].slice(0, 300)};
      continue;
    }
    row.arms[name] = {importMs: median(samples.map(s => s.importMs)), pipelineMs: median(samples.map(s => s.pipelineMs)),
      initial: meanOf(perSeed.map(m => m.initial)), refined: meanOf(perSeed.map(m => m.refined)),
      diagnostic: perSeed[0].diagnostic, geometryIssues: Math.max(...perSeed.map(m => m.geometryIssues)),
      routingIssues: Math.max(...perSeed.map(m => m.routingIssues)),
      ...(seeds > 1 ? {perSeed} : {})};
  }
  const times = [];
  let laid;
  if (elk) try {
    for (let i = 0; i <= reps; i++) {
      laid = await elkLayout(view, elkRs, {containerDirections: raw});
      if (i) times.push(laid.ms);
    }
    if (dump) writeFileSync(join(dump, `${c.name}-elkRs.json`), JSON.stringify({nodes: laid.nodes, edges: laid.edges}));
    row.arms.elkRs = {layoutMs: median(times), refined: validate(view, {nodes: laid.nodes, edges: laid.edges})};
  } catch (error) {
    row.arms.elkRs = {failed: String(error.message ?? error).split("\n")[0].slice(0, 300)};
  }
  report.cases.push(row);
  const fmt = a => `${a.refined.totalCrossings} cr / ${a.refined.bends} b / ${a.refined.lineNodeHits} hits`;
  console.log(c.name);
  for (const [name, a] of Object.entries(row.arms))
    console.log(a.failed ? `  ${name.padEnd(10)} FAILED ${a.failed}`
      : `  ${name.padEnd(10)} ${fmt(a)}  ${(a.pipelineMs ?? a.layoutMs).toFixed(1)} ms${a.diagnostic ? "  " + a.diagnostic : ""}`);
}
report.environment.loadEnd = loadavg();
if (out) writeFileSync(out, JSON.stringify(report, null, 1) + "\n");
