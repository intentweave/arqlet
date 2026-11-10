// Same-binary opt-in comparison; no detached timings stand in for public calls.
// node benchmarks/eval/abstract-pipeline.mjs --repeat 5 --out benchmarks/results/NAME.jsonl
import assert from "node:assert/strict";
import {execFileSync, spawnSync} from "node:child_process";
import {createHash} from "node:crypto";
import {mkdtempSync, readFileSync, writeFileSync, rmSync, rmdirSync} from "node:fs";
import {cpus, loadavg, arch, platform, tmpdir} from "node:os";
import {join} from "node:path";
import {ROOT, normalizeArqlet, metrics} from "./lib.mjs";

const args = process.argv.slice(2);
const option = (flag, fallback) => {
  const i = args.indexOf(flag);
  if (i < 0) return fallback;
  assert(i + 1 < args.length, `Missing ${flag}`);
  return args.splice(i, 2)[1];
};
const reps = Number(option("--repeat", 5));
const out = option("--out", null);
const replay = option("--replay", join(ROOT, "benchmarks/results/focused-pipeline-20261105.jsonl"));
assert(out && args.length === 0 && Number.isInteger(reps) && reps > 0);
const read = p => JSON.parse(readFileSync(p, "utf8"));
const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const binary = join(ROOT, "target/release/layout-command");
const benchBinary = join(ROOT, "target/release/bench");
const frozen = read(replay);
const inputs = frozen.cases.map(c => ({name: c.name, graph: c.view, sha256: hash(JSON.stringify(c.view))}));
const stats = values => {
  const sorted = values.toSorted((a, b) => a - b), mid = Math.floor(values.length / 2);
  return {samplesMs: values, medianMs: values.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2,
    minMs: sorted[0], maxMs: sorted.at(-1)};
};
const command = request => {
  const start = performance.now();
  const p = spawnSync(binary, [], {input: JSON.stringify(request), encoding: "utf8", maxBuffer: 128 * 1024 * 1024, timeout: 120_000});
  if (p.error) throw p.error;
  assert.equal(p.status, 0, p.stderr);
  const response = JSON.parse(p.stdout);
  assert.equal(response.ok, true, response.error);
  return {snapshot: response.snapshot, ms: performance.now() - start};
};
const withoutClocks = value => {
  if (Array.isArray(value)) return value.map(withoutClocks);
  if (value && typeof value === "object") return Object.fromEntries(Object.entries(value)
    .filter(([k]) => !k.endsWith("Ms") && !k.endsWith("Nanos") && !k.endsWith("nanos"))
    .map(([k, v]) => [k, withoutClocks(v)]));
  return value;
};
const signature = s => hash(JSON.stringify([s.graph, s.boxes, s.routes, s.resolvedPorts]));
function quality(snapshot) {
  const drawing = normalizeArqlet(snapshot);
  assert.equal(new Set(drawing.edges.map(e => e.id)).size, snapshot.graph.edges.length);
  for (const edge of snapshot.graph.edges) {
    const route = drawing.edges.find(r => r.id === edge.id);
    assert(route);
    assert.equal(route.source, edge.source); assert.equal(route.target, edge.target);
    assert(route.points.length >= 2 && route.points.every(p => Number.isFinite(p.x) && Number.isFinite(p.y)));
    assert(route.points.slice(1).every((p, i) => Math.abs(p.x-route.points[i].x) < 1e-6 || Math.abs(p.y-route.points[i].y) < 1e-6));
    for (const [endpoint, p] of [["source", route.points[0]], ["target", route.points.at(-1)]]) {
      const port = snapshot.resolvedPorts.find(v => v.edgeId === edge.id && v.endpoint === endpoint);
      assert(port);
      assert(Math.abs(port.position.x-p.x) < 1e-6 && Math.abs(port.position.y-p.y) < 1e-6);
    }
  }
  return {metrics: metrics(drawing), geometryIssues: snapshot.geometryIssues,
    routingIssues: snapshot.routingIssues, diagnostics: snapshot.layoutDiagnostics ?? [],
    optimization: snapshot.optimization, geometrySha256: signature(snapshot)};
}
const paths = ["Cargo.toml", "Cargo.lock", ...execFileSync("git",
  ["ls-files", "--cached", "--others", "--exclude-standard", "crates"],
  {cwd: ROOT, encoding: "utf8"}).trim().split("\n").filter(p => /\.(rs|toml)$/.test(p))];
const sourceHashes = Object.fromEntries(paths.map(p => [p, hash(readFileSync(join(ROOT, p)))]));
const report = {kind: "integrated-abstract-routing", generatedAt: new Date().toISOString(), reps, inputs,
  environment: {cpu: cpus()[0].model, logicalCpus: cpus().length, os: platform(), arch: arch(),
    node: process.version, rustc: execFileSync("rustc", ["--version"], {encoding: "utf8"}).trim(),
    revision: execFileSync("git", ["rev-parse", "HEAD"], {cwd: ROOT, encoding: "utf8"}).trim(),
    dirty: true, loadStart: loadavg()},
  binaries: Object.fromEntries([binary, benchBinary].map(p => [p, hash(readFileSync(p))])),
  generators: Object.fromEntries(["benchmarks/eval/abstract-pipeline.mjs", "benchmarks/eval/lib.mjs"].map(p => [p, hash(readFileSync(join(ROOT, p)))])),
  replay: {path: replay, sha256: hash(readFileSync(replay))}, sourceHashes,
  note: "Same release/thin-LTO/benchmark-timings binary; default and abstractRouting opt-in, unchanged candidate strategies. Full compares imports and their own refinements; common compares refinement of exactly the default geometry with only abstractRouting changed. Incumbent always retained under the shared key; no equivalence to the default refined winner claimed. Native Optimize adds crossing relief; public refine does not. No detached mode. Pipeline-only skips all unrelated edit/interaction probes. One warmup, rotated/reversed six-arm order. Shared-machine diagnostic, not idle sweep. Complete current metrics, native published keys and repeated output/work checks; invalid outcomes are findings, not hidden.",
  cases: []};
const directory = mkdtempSync(join(tmpdir(), "arqlet-integrated-"));
try {
  for (const input of inputs) {
    const path = join(directory, `${input.name}.json`);
    writeFileSync(path, JSON.stringify(input.graph));
    const common = command({op: "import", graph: input.graph, limit: 1000,
      options: {singlePassAuto: false, singlePass: false, abstractRouting: false}}).snapshot;
    const nativeReports = {}, nativeSamples = {}, outputs = {}, identities = {}, samples = {};
    const add = (key, value) => (samples[key] ??= []).push(value);
    for (let round = 0; round <= reps; round++) {
      const arms = ["default-native", "abstract-native", "common-native", "default-public", "abstract-public", "common-public"];
      arms.push(...arms.splice(0, round % arms.length));
      if (round % 2) arms.reverse();
      for (const arm of arms) {
        const [variant, operation] = arm.split("-");
        if (operation === "native") {
          const flags = ["--graph", path, "--limit", "1000", "--name", input.name,
            "--reps", "1", "--threads", "1", "--pipeline-only", "1"];
          // The pipeline before single pass (the default until engine 0.2, ADR 0016), with or without abstract routing.
          const legacy = {singlePassAuto: false, singlePass: false, abstractRouting: false};
          flags.push("--import-options", JSON.stringify(variant === "abstract" ? {...legacy, abstractRouting: true} : legacy));
          if (variant === "common") flags.push("--refine-options", JSON.stringify({...legacy, abstractRouting: true}));
          const p = spawnSync(benchBinary, flags, {cwd: ROOT, encoding: "utf8", timeout: 120_000, maxBuffer: 128 * 1024 * 1024});
          if (p.error) throw p.error;
          assert.equal(p.status, 0, p.stderr);
          const r = JSON.parse(p.stdout);
          const identity = {input: r.inputGeometryFingerprint, final: r.refineFingerprint,
            core: r.refineCoreFingerprint, key: r.publishedKey, coreKey: r.refineCorePublishedKeys,
            work: withoutClocks(r.work), candidates: withoutClocks(r.candidateWork)};
          if (identities[arm]) assert.deepEqual(identity, identities[arm], `${input.name} ${arm}: output/work drift`);
          identities[arm] = identity;
          if (variant !== "default") {
            assert(r.work["1"].abstractRoutingPasses > 0);
            assert.equal(r.work["1"].construction.searches, 0);
            assert.equal(r.work["1"].construction.transitions, 0);
            assert.equal(r.candidateWork["1"].filter(c => c.strategy === "ordering").length, 4);
          } else {
            assert.equal(r.work["1"].abstractRoutingPasses, 0);
            const previous = frozen.cases.find(c => c.name === input.name)?.native?.current;
            if (previous) {
              assert.equal(r.inputGeometryFingerprint, previous.inputGeometryFingerprint);
              assert.equal(r.geometryFingerprint, previous.geometryFingerprint);
              assert.deepEqual(r.publishedKey, previous.publishedKey);
            }
          }
          nativeReports[variant] = r;
          if (round) {
            (nativeSamples[variant] ??= []).push(r);
            add(`${arm}-import`, r.importMs.medianMs);
            add(`${arm}-refineCore`, r.refineCoreMs["1"].medianMs);
            add(`${arm}-optimize`, r.refineMs["1"].medianMs);
          }
        } else {
          const start = performance.now();
          const initial = variant === "common" ? {snapshot: common, ms: null} :
            command({op: "import", graph: input.graph, limit: 1000,
              options: {singlePassAuto: false, singlePass: false, abstractRouting: variant === "abstract"}});
          const snapshot = variant === "common" ? {...initial.snapshot, options: {...initial.snapshot.options, abstractRouting: true}} : initial.snapshot;
          const refined = command({op: "refine", snapshot, expectedRevision: snapshot.revision,
            expectedLayoutVersion: snapshot.layoutVersion ?? 0, threads: 1});
          const total = performance.now() - start;
          const identity = [signature(initial.snapshot), signature(refined.snapshot)];
          if (identities[arm]) assert.deepEqual(identity, identities[arm]);
          identities[arm] = identity;
          outputs[variant] = {initial: initial.snapshot, refined: refined.snapshot};
          if (round) {
            if (initial.ms !== null) {add(`${arm}-import`, initial.ms); add(`${arm}-pipeline`, total);}
            add(`${arm}-refine`, refined.ms);
          }
        }
      }
    }
    assert.equal(identities["common-native"].input, identities["default-native"].input);
    assert.deepEqual(nativeReports.common.inputPublishedKey, nativeReports.default.inputPublishedKey);
    const row = {name: input.name, nodes: input.graph.nodes.length, edges: input.graph.edges.length,
      inputSha256: input.sha256, timings: Object.fromEntries(Object.entries(samples).map(([k, v]) => [k, stats(v)])),
      quality: Object.fromEntries(Object.entries(outputs).map(([name, v]) => [name, {initial: quality(v.initial), refined: quality(v.refined)}])),
      native: nativeReports, nativeSamples, identities, outputs, load: loadavg()};
    report.cases.push(row);
    console.error(`${input.name}: integrated path exercised; default historical geometry preserved`);
    const candidates = nativeReports.abstract.candidateWork["1"];
    if (input.name === "layerweave-code") {
      const ordering = candidates.filter(c => c.strategy === "ordering");
      assert.equal(ordering.length, 4);
      for (const candidate of ordering) {
        assert.equal(candidate.work.placementPasses, 2,
          `${input.name} ${candidate.parameter}: combined rebuild should leave two root placements`);
        assert.equal(candidate.work.placementPassReasons.backward_edge_hierarchical_ordering_rebuild, 1,
          `${input.name} ${candidate.parameter}: expected one combined backward-edge/order rebuild`);
      }
    }
    console.log(`\n${input.name} abstract candidate phases (ms; abstract subphases are nested within routing):`);
    console.log("strategy/parameter | placement passes and reasons | placement | routing | plan | growth | construction | seeds | fallback | track repair | finalization | conflict check | scoring");
    for (const candidate of candidates) {
      const {strategy, parameter} = candidate;
      const work = candidate.work;
      const stages = work.stageTimesMs;
      const phases = work.abstractCompletionMs;
      assert.equal(Object.values(work.placementPassReasons).reduce((sum, count) => sum + count, 0),
        work.placementPasses, `${input.name} ${strategy}/${parameter}: unclassified placement pass`);
      if (work.abstractRoutingPasses > 0) {
        const phaseTimes = Object.values(phases);
        assert(phaseTimes.length === 8 && phaseTimes.every(Number.isFinite));
        const phaseTotal = phaseTimes.reduce((sum, ms) => sum + ms, 0);
        assert(stages.routing > 0 && phaseTotal / stages.routing >= 0.9,
          `${input.name} ${strategy}/${parameter}: abstract phases explain less than 90% of routing`);
      }
      console.log([
        `${strategy}/${parameter}`,
        `${work.placementPasses} ${JSON.stringify(work.placementPassReasons)}`,
        stages.placement, stages.routing, phases.abstractPlan, phases.growth,
        phases.construction, phases.seedPreparation, phases.fallback,
        phases.trackRepair, phases.finalization, phases.trackConflictCheck,
        stages.scoring,
      ].join(" | "));
    }
  }
} finally {
  for (const input of inputs) rmSync(join(directory, `${input.name}.json`), {force: true});
  rmdirSync(directory);
}
for (const [p, h] of Object.entries(sourceHashes)) assert.equal(hash(readFileSync(join(ROOT, p))), h, `Source changed: ${p}`);
report.environment.loadEnd = loadavg();
writeFileSync(out, JSON.stringify(report) + "\n");
console.log(`Saved ${out}`);
