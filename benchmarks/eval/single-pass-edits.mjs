// Which edits run on a single-pass drawing, and how far they move the other nodes, against the default.
// node benchmarks/eval/single-pass-edits.mjs [--binary PATH] [--out benchmarks/results/NAME.json]
// A rejected operation is recorded with its error, not hidden. Displacement: absolute box positions of the nodes that are
// neither the edited node nor one of its ancestors or descendants, before against after the operation. Whole-drawing shifts
// are not removed. Not covered: `port`.
import assert from "node:assert/strict";
import {execFileSync, spawnSync} from "node:child_process";
import {readFileSync, writeFileSync} from "node:fs";
import {arch, cpus, loadavg, platform} from "node:os";
import {join} from "node:path";
import {ROOT} from "./lib.mjs";

const args = process.argv.slice(2);
const option = (flag, fallback) => {
  const i = args.indexOf(flag);
  return i < 0 ? fallback : args.splice(i, 2)[1];
};
const binary = option("--binary", join(ROOT, "target/release/layout-command"));
const out = option("--out", null);
assert.equal(args.length, 0);

const command = request => {
  const p = spawnSync(binary, [], {input: JSON.stringify(request), encoding: "utf8", maxBuffer: 256 * 1024 * 1024, timeout: 300_000});
  if (p.error) throw p.error;
  return JSON.parse(p.stdout);
};
const dataset = name => JSON.parse(readFileSync(join(ROOT, "benchmarks/datasets", `${name}.json`), "utf8"));
const uniform = graph => {
  graph.containerDirections = {}; graph.containerStrategies = {}; graph.defaultStrategy = "none";
  if (!["lr", "tb"].includes(graph.defaultDirection)) graph.defaultDirection = "lr";
  delete graph.defaultRouting; delete graph.containerRouting;
  return graph;
};
const view = (graph, limit) => command({op: "import", graph, limit}).snapshot.graph;

const absolute = snapshot => {
  const parents = new Map(snapshot.graph.nodes.map(n => [n.id, n.parentId ?? null]));
  const at = new Map();
  for (const id of parents.keys()) {
    if (!snapshot.boxes[id]) continue;
    let {x, y} = snapshot.boxes[id];
    for (let p = parents.get(id); p; p = parents.get(p)) {
      x += snapshot.boxes[p]?.x ?? 0; y += snapshot.boxes[p]?.y ?? 0;
    }
    at.set(id, {x, y});
  }
  return {at, parents};
};
function displacement(before, after, edited) {
  const a = absolute(before), b = absolute(after);
  const related = id => {
    if (edited === null) return false;
    for (let p = id; p; p = a.parents.get(p)) if (p === edited) return true;
    for (let p = edited; p; p = a.parents.get(p)) if (p === id) return true;
    return false;
  };
  const moves = [];
  for (const [id, p] of a.at) {
    const q = b.at.get(id);
    if (q && !related(id)) moves.push(Math.hypot(p.x - q.x, p.y - q.y));
  }
  moves.sort((x, y) => x - y);
  return {nodes: moves.length, moved: moves.filter(d => d > 0.5).length,
    median: Math.round(moves[moves.length >> 1] ?? 0), max: Math.round(moves.at(-1) ?? 0)};
}

// "default" is the pipeline before single pass (the default until engine 0.2, ADR 0016).
const arms = {default: {singlePassAuto: false, singlePass: false, abstractRouting: false},
  singlePass: {singlePassAuto: false, abstractRouting: true, singlePass: true}, auto: {singlePassAuto: true}};
const cases = [
  ["layerweave-code", view(dataset("layerweave-code"), 60)],
  ["petri-net", view(dataset("petri-net"), 100)],
  ["state-tcp", view(dataset("state-tcp"), 100)],
  ["workflow-dag, uniform", view(uniform(dataset("workflow-dag")), 100)],
  ["org-chart, uniform", view(uniform(dataset("org-chart")), 100)],
];
const report = {kind: "single-pass-edits", generatedAt: new Date().toISOString(),
  environment: {cpu: cpus()[0].model, logicalCpus: cpus().length, os: platform(), arch: arch(), node: process.version,
    revision: execFileSync("git", ["rev-parse", "HEAD"], {cwd: ROOT, encoding: "utf8"}).trim(), load: loadavg()},
  note: "One import per view and arm; each operation starts from that import.", cases: []};

for (const [name, graph] of cases) {
  const row = {name, nodes: graph.nodes.length, edges: graph.edges.length, arms: {}};
  for (const [arm, options] of Object.entries(arms)) {
    const imported = command({op: "import", graph, limit: 1000, options});
    if (!imported.ok) {
      row.arms[arm] = {importError: imported.error};
      continue;
    }
    const s = imported.snapshot;
    const parents = new Set(s.graph.nodes.map(n => n.parentId).filter(Boolean));
    const container = [...parents][0] ?? null;
    const leaf = s.graph.nodes.find(n => !parents.has(n.id)).id;
    const box = s.boxes[leaf];
    const base = {snapshot: s, expectedRevision: s.revision, expectedLayoutVersion: s.layoutVersion ?? 0};
    const gesture = {gestureId: "probe", node: leaf, release: true};
    const next = {x: box.x + 80, y: box.y + 40};
    // A released drag of the first node of a rank with two nodes beyond the last one, across the flow.
    const graphNodes = new Set(s.graph.nodes.map(n => n.id));
    const rankOf = Object.entries(s.layeredPlans ?? {}).flatMap(([area, plan]) => plan.ranks.map(rank =>
      ({area, vertical: plan.vertical, ids: rank.filter(id => graphNodes.has(id))}))).find(r => r.ids.length >= 2);
    const dragged = () => {
      if (!rankOf) return {ok: false, error: "no rank with two nodes"};
      const [first, last] = [rankOf.ids[0], rankOf.ids.at(-1)];
      const at = s.boxes[first], beyond = s.boxes[last];
      const position = rankOf.vertical ? {x: beyond.x + 40, y: at.y} : {x: at.x, y: beyond.y + 40};
      let state = s;
      for (const [seq, phase] of [[1, "begin"], [2, "update"], [3, "finish"]]) {
        const r = command({op: "move", snapshot: state, expectedRevision: state.revision,
          expectedLayoutVersion: state.layoutVersion ?? 0, gestureId: "drag", node: first, release: true, seq, phase, position});
        if (!r.ok) return r;
        state = r.snapshot;
      }
      const order = state.layeredPlans?.[rankOf.area]?.ranks.find(rank => rank.includes(first)) ?? [];
      return {ok: true, snapshot: state, held: order.indexOf(first) > order.indexOf(last)};
    };
    const moved = () => {
      let at = s;
      for (const [seq, phase] of [[1, "begin"], [2, "update"], [3, "finish"]]) {
        const r = command({op: "move", snapshot: at, expectedRevision: at.revision, expectedLayoutVersion: at.layoutVersion ?? 0,
          ...gesture, seq, phase, position: next});
        if (!r.ok) return r;
        at = r.snapshot;
      }
      return {ok: true, snapshot: at};
    };
    const ops = {
      add: [{op: "add", sourceId: leaf, label: "Added"}, leaf],
      shape: [{op: "shape", nodeId: leaf, shape: "circle"}, leaf],
      measure: [{op: "measure", sizes: {[leaf]: {width: 260, height: 90}}}, leaf],
      move: [null, leaf],
      dragRelease: ["drag", null],
      pin: [{op: "configure", options: {pins: {[leaf]: {x: box.x, y: box.y}}}}, leaf],
      settle: [{op: "settle"}, null],
      refine: [{op: "refine", threads: 1}, null],
      structuredRefine: [{op: "structuredRefine"}, null],
      restore: ["restore", null],
      ...(container ? {
        toggle: [{op: "toggle", nodeId: container}, container],
        direction: [{op: "direction", nodeId: container, direction: s.graph.defaultDirection === "tb" ? "lr" : "tb"}, container],
        routingStraight: [{op: "routing", nodeId: container, routing: "straight"}, container],
        strategyTree: [{op: "strategy", nodeId: container, strategy: "tree"}, container],
      } : {}),
    };
    const results = {};
    for (const [op, [request, edited]] of Object.entries(ops)) {
      let r;
      if (request === null) r = moved();
      else if (request === "drag") r = dragged();
      else if (request === "restore") {
        const intent = command({op: "intent", snapshot: s});
        r = intent.ok ? command({...base, op: "restore", intent: intent.intent}) : intent;
      } else r = command({...base, ...request});
      results[op] = r.ok
        ? (r.snapshot ? {...displacement(s, r.snapshot, edited), singlePass: r.snapshot.options?.singlePass !== false,
          ...(r.held === undefined ? {} : {orderHeld: r.held})} : {ok: true})
        : {error: String(r.error).split("\n")[0].slice(0, 160)};
    }
    row.arms[arm] = results;
  }
  report.cases.push(row);
  console.log(name, row.nodes);
  for (const [arm, results] of Object.entries(row.arms))
    for (const [op, r] of Object.entries(results))
      console.log(`  ${arm.padEnd(11)}${op.padEnd(18)}${r.error ? "ERROR " + r.error : JSON.stringify(r)}`);
}
if (out) writeFileSync(out, JSON.stringify(report, null, 1) + "\n");
