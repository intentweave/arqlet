// Bounded correctness/diagnostic check, not a timing benchmark or candidate-pruning policy.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = resolve(process.argv[2] ?? join(root, "target/release/bench"));
const physical = process.argv.includes("--realize");
const graph = "examples/fixtures/phase-width-feedback.json";
const graphDocument = JSON.parse(readFileSync(join(root, graph)));
const temporary = mkdtempSync(join(tmpdir(), "arqlet-abstract-routing-"));
const hash = (bytes) => createHash("sha256").update(bytes).digest("hex");
const sources = new Set(
  execFileSync("git", ["ls-files", "crates", "Cargo.toml", "Cargo.lock"], { cwd: root, encoding: "utf8" })
    .trim().split("\n"),
);
sources.add("crates/layout/src/abstract_routing.rs");
sources.add("crates/layout/src/constructive/abstract_realization.rs");
sources.add("crates/layout/src/constructive/abstract_realization/growth.rs");
sources.add("crates/layout/src/constructive/abstract_realization/continuity.rs");
sources.add("crates/layout/src/constructive/abstract_realization/joint.rs");
sources.add("crates/layout/src/constructive/abstract_realization/chains.rs");
sources.add("crates/layout/src/constructive/abstract_realization/completion.rs");
sources.add("crates/layout/src/constructive/abstract_realization/construction.rs");
sources.add("crates/layout/src/constructive/abstract_realization/coordination.rs");
sources.add("crates/layout/src/constructive/abstract_realization/planning.rs");
sources.add("crates/layout/src/constructive/abstract_realization/regional.rs");
sources.add("crates/routing/src/fixed_tracks.rs");
sources.add("crates/routing/src/fixed_tracks/memo.rs");
const sourceHashes = Object.fromEntries([...sources].sort().map((p) => [p, hash(readFileSync(join(root, p)))]));

function run(name, shadow) {
  const dir = join(temporary, name);
  const args = [
    "--graph", graph, "--limit", "1000", "--name", "frozen",
    "--reps", "1", "--threads", "1,4", "--candidate-log", dir,
    ...(shadow ? [physical ? "--abstract-realization" : "--abstract-plan", "1"] : []),
  ];
  const output = JSON.parse(execFileSync(binary, args, {
    cwd: root, encoding: "utf8", maxBuffer: 32 * 1024 * 1024,
  }));
  const rows = readFileSync(join(dir, "frozen-1000.jsonl"), "utf8").trim().split("\n").map(JSON.parse);
  return { output, rows };
}

function withoutTiming(value) {
  if (Array.isArray(value)) return value.map(withoutTiming);
  if (value && typeof value === "object") {
    return Object.fromEntries(Object.entries(value).filter(([k]) =>
      k !== "stageMs" && k !== "geometryFile" && k !== "abstractRouting" && !k.endsWith("Ms"),
    ).map(([k, v]) => [k, withoutTiming(v)]));
  }
  return value;
}

function validate(plan) {
  assert.equal(plan.safeToPrune, false);
  assert.equal(plan.stage, "shadow-from-ordered-ranks");
  assert.equal(plan.summary.edges, 358);
  assert.equal(plan.summary.edges, plan.summary.plannedEdges + plan.summary.unsupportedEdges);
  assert.equal(plan.edges.length, 358);
  assert.equal(new Set(plan.edges.map((e) => e.edge)).size, 358);
  const assignments = new Set();
  for (const c of plan.channels) {
    assert.equal(new Set(c.order).size, c.assignments.length);
    const position = new Map(c.order.map((v, i) => [v, i]));
    for (const [a, b] of c.dependencies) {
      assert(position.get(a) < position.get(b) || c.brokenDependencies.some(([x, y]) => a === x && b === y));
    }
    for (const a of c.assignments) {
      const key = JSON.stringify([a.edge, c.area, c.rank]);
      assert(!assignments.has(key));
      assignments.add(key);
      assert(a.slot >= 0 && a.slot < c.tracks);
    }
  }
  let sections = 0;
  for (const e of plan.edges) {
    if (e.unsupported) {
      assert.equal(e.sections.length, 0);
    } else {
      assert(e.sections.length > 0);
      for (const s of e.sections) {
        assert(assignments.has(JSON.stringify([e.edge, s.area, s.channel])));
        sections++;
      }
    }
  }
  assert.equal(sections, plan.summary.sections);
  assert.equal(assignments.size, sections);
}

function validatePhysical(p) {
  assert.equal(p.applied, false);
  assert.equal(p.work.searches, 0);
  assert.equal(p.work.states, 0);
  assert.equal(p.work.transitions, 0);
  const all = [...p.routes.map((r) => r.id), ...p.delegated.map((e) => e.edge)];
  assert.deepEqual([...all].sort(), graphDocument.edges.map((e) => e.id).sort());
  assert.equal(new Set(all).size, all.length);
  const parents = new Map(graphDocument.nodes.map((n) => [n.id, n.parentId]));
  const edges = new Map(graphDocument.edges.map((e) => [e.id, e]));
  const occupancy = [];
  for (const route of p.routes) {
    const edge = edges.get(route.id);
    const source = p.resolvedPorts.find((port) => port.edgeId === route.id && port.endpoint === "source");
    const target = p.resolvedPorts.find((port) => port.edgeId === route.id && port.endpoint === "target");
    assert.deepEqual(route.points[0], source.position);
    assert.deepEqual(route.points.at(-1), target.position);
    const excluded = new Set();
    for (let id of [edge.source, edge.target]) {
      while (id) { excluded.add(id); id = parents.get(id); }
    }
    for (let i = 1; i < route.points.length; i++) {
      const a = route.points[i - 1], b = route.points[i];
      assert([a.x, a.y, b.x, b.y].every(Number.isFinite));
      const horizontal = Math.abs(a.y - b.y) < 1e-6;
      assert(horizontal || Math.abs(a.x - b.x) < 1e-6);
      for (const [id, r] of Object.entries(p.boxes)) {
        if (excluded.has(id)) continue;
        const m = p.spacing.margin;
        const enters = Math.max(a.x, b.x) > r.x - m + 1e-6
          && Math.min(a.x, b.x) < r.x + r.width + m - 1e-6
          && Math.max(a.y, b.y) > r.y - m + 1e-6
          && Math.min(a.y, b.y) < r.y + r.height + m - 1e-6;
        assert(!enters, `Trial ${route.id} violates clearance of ${id}`);
      }
      const coord = horizontal ? a.y : a.x;
      const lo = horizontal ? Math.min(a.x, b.x) : Math.min(a.y, b.y);
      const hi = horizontal ? Math.max(a.x, b.x) : Math.max(a.y, b.y);
      if (hi - lo <= 1e-6) continue;
      for (const s of occupancy) {
        if (s.edge === route.id) continue;
        assert(!(s.horizontal === horizontal && Math.abs(s.coord - coord) < p.spacing.track - 1e-6
          && s.lo < hi - 1e-6 && lo < s.hi - 1e-6), `Trials ${s.edge}/${route.id} violate track spacing`);
      }
      occupancy.push({ edge: route.id, horizontal, coord, lo, hi });
    }
  }
  for (const c of p.channels) {
    assert(Math.abs(c.required - (2 * p.spacing.margin + (c.tracks - 1) * p.spacing.track)) < 1e-6);
    assert(Math.abs(c.missing - Math.max(0, c.required - c.available)) < 1e-6);
    const positions = new Map();
    for (const a of c.assignments) {
      assert(a.slot >= 0 && a.slot < c.tracks);
      if (positions.has(a.slot)) assert.equal(a.coordinate, positions.get(a.slot));
      positions.set(a.slot, a.coordinate);
    }
    const sorted = [...positions.entries()].sort((a, b) => a[0] - b[0]);
    for (let i = 1; i < sorted.length; i++) {
      assert(Math.abs((sorted[i][1] - sorted[i - 1][1]) -
        (sorted[i][0] - sorted[i - 1][0]) * p.spacing.track) < 1e-6, "Never compress missing track space");
    }
  }
  if (p.continuity) {
    const c = p.continuity;
    assert.equal(c.reservations, c.lanes.length);
    assert.equal(c.constraints, c.separationConstraints.length);
    const rejected = new Set(c.lanes.filter((r) => r.coordinate === null).map((r) => r.edge));
    assert.equal(rejected.size, c.rejectedEdges);
    for (const r of c.lanes) {
      assert([...r.main, ...r.bounds, r.preferred].every(Number.isFinite));
      if (r.coordinate === null) {
        assert(!p.routes.some((route) => route.id === r.edge));
        continue;
      }
      assert(Number.isFinite(r.coordinate));
      assert(r.coordinate >= r.bounds[0] - 1e-6 && r.coordinate <= r.bounds[1] + 1e-6);
      const route = p.routes.find((route) => route.id === r.edge);
      if (!route || r.main[1] - r.main[0] <= 1e-6) continue;
      const along = p.vertical ? "y" : "x", cross = p.vertical ? "x" : "y";
      assert(route.points.slice(1).some((b, i) => {
        const a = route.points[i];
        return Math.abs(a[cross] - r.coordinate) <= 1e-6
          && Math.abs(b[cross] - r.coordinate) <= 1e-6
          && Math.min(a[along], b[along]) <= r.main[0] + 1e-6
          && Math.max(a[along], b[along]) >= r.main[1] - 1e-6;
      }), `Reservation ${r.edge}/${r.junction} is not realized by its route`);
    }
    let crossArea = 0;
    for (const [a, b] of c.separationConstraints) {
      assert(a >= 0 && a < b && b < c.lanes.length, "Acyclic total lane order");
      const from = c.lanes[a], to = c.lanes[b];
      crossArea += from.area !== to.area ? 1 : 0;
      if (from.coordinate !== null && to.coordinate !== null) {
        assert(to.coordinate - from.coordinate >= p.spacing.track - 1e-6);
      }
    }
    assert.equal(crossArea, c.crossAreaConstraints);
    validateWitnesses(c, p.spacing.track);
  }
  if (p.joint) {
    const j = p.joint;
    assert.equal(j.temporalOrders.length, j.temporalConstraints);
    const blocked = new Set(p.delegated.filter((d) => d.failure.kind === "jointOrder").map((d) => d.edge));
    const capacity = new Set(p.delegated.filter((d) => d.failure.kind === "jointCapacity").map((d) => d.edge));
    assert.equal(j.blockedEdges, blocked.size);
    assert.equal(j.capacityEdges, capacity.size);
    for (const b of j.bends) {
      assert([...b.bounds, b.preferred].every(Number.isFinite));
      if (b.coordinate === null) {
        assert(blocked.has(b.edge) || capacity.has(b.edge));
        continue;
      }
      assert(!blocked.has(b.edge) && !capacity.has(b.edge));
      assert(Number.isFinite(b.coordinate));
      assert(b.coordinate >= b.bounds[0] - 1e-6 && b.coordinate <= b.bounds[1] + 1e-6);
      const channel = p.channels.find((c) => c.area === b.area && c.rank === b.rank);
      const a = channel.assignments.find((a) => a.edge === b.edge);
      assert.equal(a.coordinate, b.coordinate);
    }
    for (const [a, b, distance] of j.precedences) {
      assert(a >= 0 && a < j.bends.length && b >= 0 && b < j.bends.length);
      assert(Number.isFinite(distance) && distance >= 0);
      const from = j.bends[a], to = j.bends[b];
      if (from.coordinate !== null && to.coordinate !== null) {
        assert(to.coordinate - from.coordinate >= distance - 1e-6, `Joint bend precedence ${a}/${b}`);
      }
    }
    for (const [firstEdge, firstJunction, secondEdge, secondJunction] of j.temporalOrders) {
      const first = p.continuity.lanes.find((r) => r.edge === firstEdge && r.junction === firstJunction);
      const second = p.continuity.lanes.find((r) => r.edge === secondEdge && r.junction === secondJunction);
      if (first && second) assert(first.main[1] <= second.main[0] + 1e-6, "Selected temporal order is realized");
    }
  }
}

function validateWitnesses(c, track) {
  const constraints = new Set(c.separationConstraints.map(([a, b]) => `${a}:${b}`));
  assert.equal(c.capacityWitnesses.length, c.rejectedEdges);
  for (const w of c.capacityWitnesses) {
    assert(w.chain.length > 0);
    const first = c.lanes[w.chain[0]], last = c.lanes[w.chain.at(-1)];
    assert.equal(first.bounds[0], w.lower);
    assert.equal(last.bounds[1], w.upper);
    assert.equal(last.edge, w.edge);
    assert.equal(last.junction, w.junction);
    assert([w.lower, w.upper, w.required, w.deficit].every(Number.isFinite));
    assert(Math.abs(w.required - (w.chain.length - 1) * track) < 1e-6);
    assert(Math.abs(w.lower + w.required - w.upper - w.deficit) < 1e-6);
    assert(w.deficit > 1e-6);
    for (let i = 1; i < w.chain.length; i++) {
      const a = w.chain[i - 1], b = w.chain[i];
      assert(a < b && constraints.has(`${a}:${b}`));
      const from = c.lanes[a], to = c.lanes[b];
      assert(from.main[0] < to.main[1] - 1e-6 && to.main[0] < from.main[1] - 1e-6);
    }
  }
}

function validateChains(before, after) {
  assert.equal(after.stage, "physical-trial-with-capacity-chain-cuts");
  assert.deepEqual(after.boxes, before.boxes);
  assert.deepEqual(after.resolvedPorts, before.resolvedPorts);
  const c = after.chains;
  assert.equal(c.planningPasses, 2);
  assert.deepEqual(c.probeJoint, before.joint);
  assert.deepEqual(c.probeContinuity, before.continuity, "Probe must start from original preferences, not mutated lanes");
  validateWitnesses(c.probeContinuity, after.spacing.track);
  assert.equal(c.cuts.length, c.probeContinuity.capacityWitnesses.length);
  assert.equal(c.uncutWitnesses, c.cuts.filter((cut) => cut.pair === null).length);
  const orders = new Set();
  const edges = new Map(graphDocument.edges.map((e) => [e.id, e]));
  const bound = (r, start, upper) => {
    const last = Math.max(...c.probeContinuity.lanes.filter((a) => a.edge === r.edge).map((a) => a.junction));
    if ((start && r.junction === 0) || (!start && r.junction === last)) {
      const edge = edges.get(r.edge);
      const source = after.resolvedPorts.find((p) => p.edgeId === edge.id && p.endpoint === "source").position;
      const target = after.resolvedPorts.find((p) => p.edgeId === edge.id && p.endpoint === "target").position;
      const axis = after.vertical ? "y" : "x";
      return start ? Math.min(source[axis], target[axis]) : Math.max(source[axis], target[axis]);
    }
    const section = start ? r.junction - 1 : r.junction;
    const b = c.probeJoint.bends.find((b) => b.edge === r.edge && b.section === section);
    assert(b);
    return b.bounds[upper ? 1 : 0];
  };
  for (const cut of c.cuts) {
    const witness = c.probeContinuity.capacityWitnesses[cut.witness];
    assert(witness);
    const candidates = [];
    for (let i = 1; i < witness.chain.length; i++) {
      for (const [a, b] of [[witness.chain[i - 1], witness.chain[i]], [witness.chain[i], witness.chain[i - 1]]]) {
        const first = c.probeContinuity.lanes[a], second = c.probeContinuity.lanes[b];
        if (first.edge === second.edge) continue;
        const slack = bound(second, true, true) - bound(first, false, false);
        if (slack >= -1e-6) candidates.push({
          pair: [a, b], slack, displacement: Math.max(0, first.main[1] - second.main[0]),
        });
      }
    }
    candidates.sort((a, b) => a.displacement - b.displacement || b.slack - a.slack);
    if (cut.pair === null) {
      assert.equal(candidates.length, 0, "No feasible adjacent temporal cut");
      assert.equal(cut.slack, null);
      assert.equal(cut.displacement, null);
      continue;
    }
    assert.deepEqual(cut.pair, candidates[0].pair, "Least displacement, then largest slack, stable ties");
    const [a, b] = cut.pair;
    assert(witness.chain.some((v, i) => i > 0 && (
      (witness.chain[i - 1] === a && v === b) || (witness.chain[i - 1] === b && v === a)
    )), "Cut must remove an actual link from this witness");
    const first = c.probeContinuity.lanes[a], second = c.probeContinuity.lanes[b];
    assert.notEqual(first.edge, second.edge);
    const slack = bound(second, true, true) - bound(first, false, false);
    assert(slack >= -1e-6);
    assert(Math.abs(slack - cut.slack) < 1e-6);
    assert(Math.abs(Math.max(0, first.main[1] - second.main[0]) - cut.displacement) < 1e-6);
    orders.add(JSON.stringify([first.edge, first.junction, second.edge, second.junction]));
  }
  assert.deepEqual([...orders].sort(), c.orders.map((o) => JSON.stringify(o)).sort());
  for (const order of c.orders) {
    assert(after.joint.temporalOrders.some((a) => JSON.stringify(a) === JSON.stringify(order)));
  }
}

function validateJoint(before, after) {
  assert.equal(after.stage, "physical-trial-with-joint-bend-order");
  assert.deepEqual(after.boxes, before.boxes);
  assert.deepEqual(after.resolvedPorts, before.resolvedPorts);
  assert(after.channels.every((c) => c.missing <= 1e-6));
}

function validateCoordinated(before, after) {
  assert.equal(after.stage, "physical-trial-with-longitudinal-reservations");
  assert.deepEqual(after.boxes, before.boxes, "Lane packing does not move or grow nodes");
  assert.deepEqual(after.resolvedPorts, before.resolvedPorts, "Resolved endpoints stay fixed");
  assert(after.channels.every((c) => c.missing <= 1e-6));
  assert(!after.delegated.some((d) => d.failure.kind === "trackConflict" && d.failure.alongFlow));
  for (const c of before.channels) {
    const fixed = after.channels.find((a) => a.area === c.area && a.rank === c.rank);
    for (const a of c.assignments) {
      assert.deepEqual(fixed.assignments.find((b) => b.edge === a.edge), a, "Existing channel tracks stay fixed");
    }
  }
}

function physicalSummary(p, work, reference) {
  validatePhysical(p);
  const realized = new Set(p.routes.map((r) => r.id));
  const searched = new Set(work?.construction?.searchDetails.map((s) => s.edge) ?? []);
  const reasons = {};
  const capacityAt = { source: 0, target: 0, junction: 0 };
  for (const { failure: f } of p.delegated) {
    const reason = f.kind === "structural" || f.kind === "preparation" ? `${f.kind}:${f.reason}` :
      f.kind === "trackConflict" ? `${f.kind}:${f.alongFlow ? "alongFlow" : "acrossFlow"}` : f.kind;
    reasons[reason] = (reasons[reason] ?? 0) + 1;
  }
  if (p.continuity) {
    for (const d of p.delegated.filter((d) => d.failure.kind === "longitudinalCapacity")) {
      const last = Math.max(...p.continuity.lanes.filter((r) => r.edge === d.edge).map((r) => r.junction));
      capacityAt[d.failure.junction === 0 ? "source" : d.failure.junction === last ? "target" : "junction"]++;
    }
  }
  const bounds = Object.values(p.boxes);
  const width = Math.max(...bounds.map((b) => b.x + b.width)) - Math.min(...bounds.map((b) => b.x));
  const height = Math.max(...bounds.map((b) => b.y + b.height)) - Math.min(...bounds.map((b) => b.y));
  return {
    realizedEdges: p.realizedEdges, delegatedEdges: p.delegatedEdges, reasons,
    searches: p.work.searches, channels: p.channels.length,
    continuity: continuitySummary(p.continuity),
    joint: p.joint && Object.fromEntries(Object.entries(p.joint)
      .filter(([k]) => k !== "bends" && k !== "precedences" && k !== "temporalOrders")),
    chains: p.chains && {
      planningPasses: p.chains.planningPasses,
      witnesses: p.chains.probeContinuity.capacityWitnesses.length,
      orders: p.chains.orders.length, uncutWitnesses: p.chains.uncutWitnesses,
      selectedOrders: p.chains.orders,
      probeWitnesses: continuitySummary(p.chains.probeContinuity).capacityWitnesses,
      cuts: p.chains.cuts.map((cut) => ({
        ...cut,
        pair: cut.pair?.map((i) => ({
          edge: p.chains.probeContinuity.lanes[i].edge,
          junction: p.chains.probeContinuity.lanes[i].junction,
        })) ?? null,
      })),
    },
    ...(p.continuity ? { capacityAt } : {}),
    ...(reference ? { coverageChange: {
      gainedIds: [...realized].filter((id) => !reference.routes.some((r) => r.id === id)),
      lostIds: reference.routes.filter((r) => !realized.has(r.id)).map((r) => r.id),
    } } : {}),
    narrowChannels: p.channels.filter((c) => c.missing > 1e-6).length,
    missingTotal: p.channels.reduce((n, c) => n + c.missing, 0),
    formerlySearchedRealized: [...searched].filter((e) => realized.has(e)).length,
    width, height, area: width * height,
    geometrySha256: hash(JSON.stringify({ boxes: p.boxes, routes: p.routes, ports: p.resolvedPorts })),
    realizedIds: [...realized],
    delegated: p.delegated,
  };
}

function continuitySummary(c) {
  if (!c) return c;
  return {
    ...Object.fromEntries(Object.entries(c).filter(([k]) =>
      k !== "lanes" && k !== "separationConstraints" && k !== "capacityWitnesses")),
    capacityWitnesses: c.capacityWitnesses.map((w) => ({
      ...w, path: w.chain.map((i) => c.lanes[i]),
    })),
  };
}

function validateGrowth(before, after) {
  const parents = new Map(graphDocument.nodes.map((n) => [n.id, n.parentId]));
  const cross = after.vertical ? "x" : "y";
  const crossSize = after.vertical ? "width" : "height";
  const mainSize = after.vertical ? "height" : "width";
  const overlap = (a, b) => a.x < b.x + b.width - 1e-6 && b.x < a.x + a.width - 1e-6
    && a.y < b.y + b.height - 1e-6 && b.y < a.y + a.height - 1e-6;
  for (const [id, r] of Object.entries(before.boxes)) {
    const grown = after.boxes[id];
    assert.equal(grown[cross], r[cross]);
    assert.equal(grown[crossSize], r[crossSize]);
    assert(grown[mainSize] >= r[mainSize]);
    const parent = parents.get(id);
    if (parent) {
      const container = after.boxes[parent];
      assert(grown.x >= container.x - 1e-6 && grown.y >= container.y - 1e-6
        && grown.x + grown.width <= container.x + container.width + 1e-6
        && grown.y + grown.height <= container.y + container.height + 1e-6, `Grown ${id} leaves ${parent}`);
    }
    for (const [other, b] of Object.entries(before.boxes)) {
      if (id < other && parents.get(id) === parents.get(other) && !overlap(r, b)) {
        assert(!overlap(grown, after.boxes[other]), `Growth introduces sibling overlap ${id}/${other}`);
      }
    }
  }
  assert(after.channels.every((c) => c.missing <= 1e-6), "This fixture must fit after one bottom-up realization");
}

try {
  const before = run("plain", false);
  const after = run("shadow", true);
  assert.equal(after.output.inputGeometryFingerprint, "79e64fa0fe98e5fe");
  assert.equal(after.output.geometryFingerprint, "19409ef4dbf00e51");
  assert.equal(after.output.refineFingerprint, "9fc9f367016cb818");
  assert.equal(after.output.threadsIdentical, true);
  for (const key of ["refineFingerprint", "refineCoreFingerprint", "geometryFingerprint",
    "inputGeometryFingerprint", "publishedKey", "inputPublishedKey", "routingIssues", "optimization"]) {
    assert.deepEqual(after.output[key], before.output[key], key);
  }
  assert.equal(before.output.abstractRouting, undefined);
  assert.equal(before.rows.length, 7);
  assert.deepEqual(withoutTiming(after.rows), withoutTiming(before.rows));
  const baselinePath = "benchmarks/results/structural-split-candidates-20261104.jsonl";
  const baseline = readFileSync(join(root, baselinePath), "utf8").trim().split("\n").map(JSON.parse);
  const decisions = (rows) => rows.map(({ work, ...row }) => withoutTiming(row));
  assert.deepEqual(decisions(after.rows), decisions(baseline), "Committed pre-feature candidate geometry/metrics/decisions");
  validate(after.output.inputAbstractRouting);
  validate(after.output.abstractRouting);
  const candidates = after.rows.map((r) => {
    validate(r.abstractRouting);
    if (physical) {
      validateGrowth(r.abstractRouting.realization, r.abstractRouting.grownRealization);
      validateCoordinated(r.abstractRouting.grownRealization, r.abstractRouting.coordinatedRealization);
      validateJoint(r.abstractRouting.coordinatedRealization, r.abstractRouting.jointRealization);
      validateChains(r.abstractRouting.jointRealization, r.abstractRouting.chainRealization);
    }
    const c = r.work.construction;
    assert.equal(c.searchDetailsRecorded, true, "Build bench with --features benchmark-timings");
    assert.equal(c.searchDetails.length, c.searches);
    assert.equal(c.searchDetails.filter((s) => s.failed).length, c.failedSearches);
    assert.equal(c.searchDetails.reduce((n, s) => n + s.states, 0), c.states);
    assert.equal(c.searchDetails.reduce((n, s) => n + s.transitions, 0), c.transitions);
    assert.equal(c.plannedFailures.length, c.plannedObstacleFailures + c.plannedTrackFailures);
    const planned = new Set(r.abstractRouting.edges.filter((e) => !e.unsupported).map((e) => e.edge));
    const searched = new Set(c.searchDetails.map((s) => s.edge));
    const phases = {};
    for (const s of c.searchDetails) phases[s.phase] = (phases[s.phase] ?? 0) + 1;
    return {
      index: r.index, strategy: r.strategy, geometryFingerprint: r.geometryFingerprint,
      key: r.key, accepted: r.accepted, selected: r.selected, published: r.published,
      abstract: r.abstractRouting.summary,
      searchedEdges: searched.size, structurallyPlannedSearchedEdges: [...searched].filter((e) => planned.has(e)).length,
      unplannedSearchedEdges: [...searched].filter((e) => !planned.has(e)),
      searches: c.searches, failedSearches: c.failedSearches, states: c.states, transitions: c.transitions, phases,
      plannedObstacleFailures: c.plannedObstacleFailures, plannedTrackFailures: c.plannedTrackFailures,
      ...(physical ? {
        physical: physicalSummary(r.abstractRouting.realization, r.work),
        grown: physicalSummary(r.abstractRouting.grownRealization, r.work),
        coordinated: physicalSummary(r.abstractRouting.coordinatedRealization, r.work, r.abstractRouting.grownRealization),
        joint: physicalSummary(r.abstractRouting.jointRealization, r.work, r.abstractRouting.coordinatedRealization),
        chain: physicalSummary(r.abstractRouting.chainRealization, r.work, r.abstractRouting.jointRealization),
      } : {}),
    };
  });
  if (physical) {
    validateGrowth(after.output.inputAbstractRouting.realization, after.output.inputAbstractRouting.grownRealization);
    validateGrowth(after.output.abstractRouting.realization, after.output.abstractRouting.grownRealization);
    for (const p of [after.output.inputAbstractRouting, after.output.abstractRouting]) {
      validateCoordinated(p.grownRealization, p.coordinatedRealization);
      validateJoint(p.coordinatedRealization, p.jointRealization);
      validateChains(p.jointRealization, p.chainRealization);
      assert.equal(p.coordinatedRealization.realizedEdges, 308);
      assert.equal(p.coordinatedRealization.continuity.rejectedEdges, 41);
      assert.equal(p.jointRealization.realizedEdges, 327);
      assert.equal(p.jointRealization.continuity.rejectedEdges, 20);
      assert.equal(p.chainRealization.realizedEdges, 347);
      assert.equal(p.chainRealization.continuity.rejectedEdges, 0);
    }
  }
  console.log(JSON.stringify({
    kind: physical ? "abstract-routing-capacity-chain-correctness" : "abstract-routing-shadow-correctness",
    note: physical
      ? "No speedup measurement. Independent physical/growth/coordinated-lane/joint-bend/chain-cut trials run outside engine clocks and are not applied. Chain trial performs exactly two planning passes and may lose edges; no best-result selection. Selected-order witnesses/cuts/failures are not candidate pruning or unroutability proofs. Per-edge event recording is benchmark-only. Export full geometry and witnesses with bench --abstract-realization 1 --candidate-log DIR."
      : "No speedup measurement. Shadow analysis runs outside engine clocks; per-edge event recording is benchmark-only. Full plan/candidate log exports are available through bench --abstract-plan 1 --candidate-log DIR.",
    environment: after.output.environment,
    binary: { path: binary, sha256: hash(readFileSync(binary)), profile: binary.includes("/fast/") ? "fast" : "caller-supplied" },
    sources: sourceHashes,
    input: { path: graph, sha256: hash(readFileSync(join(root, graph))) },
    generator: { path: "benchmarks/abstract-routing-check.mjs", sha256: hash(readFileSync(fileURLToPath(import.meta.url))) },
    baseline: { path: baselinePath, sha256: hash(readFileSync(join(root, baselinePath))) },
    threads: [1, 4], onOffIdentical: true, geometryFingerprint: after.output.geometryFingerprint,
    inputGeometryFingerprint: after.output.inputGeometryFingerprint,
    snapshotFingerprint: after.output.refineFingerprint,
    initial: after.output.inputAbstractRouting.summary,
    final: after.output.abstractRouting.summary,
    ...(physical ? {
      physicalInitial: physicalSummary(after.output.inputAbstractRouting.realization),
      grownInitial: physicalSummary(after.output.inputAbstractRouting.grownRealization),
      coordinatedInitial: physicalSummary(after.output.inputAbstractRouting.coordinatedRealization, undefined, after.output.inputAbstractRouting.grownRealization),
      jointInitial: physicalSummary(after.output.inputAbstractRouting.jointRealization, undefined, after.output.inputAbstractRouting.coordinatedRealization),
      chainInitial: physicalSummary(after.output.inputAbstractRouting.chainRealization, undefined, after.output.inputAbstractRouting.jointRealization),
      physicalFinal: physicalSummary(after.output.abstractRouting.realization),
      grownFinal: physicalSummary(after.output.abstractRouting.grownRealization),
      coordinatedFinal: physicalSummary(after.output.abstractRouting.coordinatedRealization, undefined, after.output.abstractRouting.grownRealization),
      jointFinal: physicalSummary(after.output.abstractRouting.jointRealization, undefined, after.output.abstractRouting.coordinatedRealization),
      chainFinal: physicalSummary(after.output.abstractRouting.chainRealization, undefined, after.output.abstractRouting.jointRealization),
    } : {}),
    unsupported: after.output.abstractRouting.edges.filter((e) => e.unsupported).map((e) => ({ edge: e.edge, reason: e.unsupported })),
    candidates,
  }));
} finally {
  rmSync(temporary, { recursive: true, force: true });
}
