// In-process timing of the frozen views: Arqlet through the native `bench` binary (import and refine inside one
// process, no JSON transport between them), elk-rs (native Node addon) and elkjs through `layout()`.
// node benchmarks/eval/single-pass-timing.mjs [--repeat 11] [--bench PATH] [--out benchmarks/results/NAME.json]
// Build `bench` without the `benchmark-timings` feature, so no phase counters run inside the measured span:
//   CARGO_TARGET_DIR=/tmp/lw-plain cargo build --release --locked -p arqlet-runtime --bins
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {createHash} from "node:crypto";
import {mkdtempSync, readFileSync, rmSync, writeFileSync} from "node:fs";
import {arch, cpus, loadavg, platform, tmpdir} from "node:os";
import {join} from "node:path";
import ElkRs from "@openedges/elk-layout";
import ELK from "elkjs";
import {ROOT} from "./lib.mjs";
import {elkLayout} from "./elk-adapter.mjs";

const args = process.argv.slice(2);
const option = (flag, fallback) => {
  const i = args.indexOf(flag);
  return i < 0 ? fallback : args.splice(i, 2)[1];
};
const reps = Number(option("--repeat", 11));
const bench = option("--bench", "/tmp/lw-plain/release/bench");
const out = option("--out", null);
const replay = option("--replay", join(ROOT, "benchmarks/results/focused-pipeline-20261105.jsonl"));
assert(args.length === 0 && Number.isInteger(reps) && reps > 0);

const hash = bytes => createHash("sha256").update(bytes).digest("hex");
const median = values => {
  const s = values.toSorted((a, b) => a - b), m = Math.floor(s.length / 2);
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
// "default" is the pipeline before single pass (the default until engine 0.2, ADR 0016).
const legacy = {singlePassAuto: false, singlePass: false, abstractRouting: false};
const arms = {default: legacy, abstract: {...legacy, abstractRouting: true},
  singlePass: {singlePassAuto: false, abstractRouting: true, singlePass: true}};
const engines = [["elkRs", new ElkRs()], ["elkjs", new ELK()]];
const dir = mkdtempSync(join(tmpdir(), "lw-timing-"));
const report = {kind: "single-pass-timing", generatedAt: new Date().toISOString(), reps,
  environment: {cpu: cpus()[0].model, logicalCpus: cpus().length, os: platform(), arch: arch(), node: process.version,
    revision: execFileSync("git", ["rev-parse", "HEAD"], {cwd: ROOT, encoding: "utf8"}).trim(), dirty: true, loadStart: loadavg()},
  binary: hash(readFileSync(bench)),
  input: {replay: {path: replay, sha256: hash(readFileSync(replay))}},
  note: "Arqlet: bench import and refine (threads 1) in one native process, median of reps after one warm-up. elk-rs and elkjs: layout() in this process, median of reps after one warm-up. No process start or JSON transport in any arm.",
  cases: []};
try {
  for (const c of JSON.parse(readFileSync(replay, "utf8")).cases) {
    const file = join(dir, `${c.name}.json`);
    writeFileSync(file, JSON.stringify(c.view));
    const row = {name: c.name, nodes: c.view.nodes.length, edges: c.view.edges.length, arms: {}};
    for (const [name, options] of Object.entries(arms)) {
      const r = JSON.parse(execFileSync(bench, ["--graph", file, "--limit", "1000", "--reps", String(reps), "--threads", "1",
        "--pipeline-only", "1", "--import-options", JSON.stringify(options)],
        {encoding: "utf8", stdio: ["ignore", "pipe", "ignore"], maxBuffer: 1 << 28}));
      const importMs = r.importMs.medianMs, refineMs = r.refineMs["1"].medianMs;
      row.arms[name] = {importMs, refineMs, totalMs: importMs + refineMs, refineFingerprint: r.refineFingerprint};
    }
    for (const [name, engine] of engines) {
      const times = [];
      for (let i = 0; i <= reps; i++) {
        const laid = await elkLayout(c.view, engine);
        if (i) times.push(laid.ms);
      }
      row.arms[name] = {layoutMs: median(times)};
    }
    report.cases.push(row);
    const a = row.arms;
    console.log(`${c.name.padEnd(16)} default ${a.default.totalMs.toFixed(1)}  abstract ${a.abstract.totalMs.toFixed(1)}  `
      + `singlePass ${a.singlePass.importMs.toFixed(1)}+${a.singlePass.refineMs.toFixed(1)}  `
      + `elk-rs ${a.elkRs.layoutMs.toFixed(1)}  elkjs ${a.elkjs.layoutMs.toFixed(1)} ms`);
  }
} finally {
  rmSync(dir, {recursive: true, force: true});
}
report.environment.loadEnd = loadavg();
if (out) writeFileSync(out, JSON.stringify(report, null, 1) + "\n");
console.log("load", report.environment.loadStart.map(v => v.toFixed(1)), report.environment.loadEnd.map(v => v.toFixed(1)));
