// Detached routing/quality experiment; does not change the default or measure refinement.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binary = resolve(process.argv[2] ?? join(root, "target/fast/bench"));
const reps = Number(process.argv[3] ?? 5);
const planning = process.argv.includes("--planning");
const repairReuse = process.argv.includes("--repair-reuse");
const constructionReuse = process.argv.includes("--construction-reuse");
const coordinationReuse = process.argv.includes("--coordination-reuse");
assert(Number(repairReuse)+Number(constructionReuse)+Number(coordinationReuse)<=1,"select one reuse experiment");
const regional = process.argv.includes("--regional") || repairReuse || constructionReuse || coordinationReuse;
assert(Number.isInteger(reps) && reps > 0, "reps must be a positive integer");
const graphPath = "examples/fixtures/phase-width-feedback.json";
const graph = JSON.parse(readFileSync(join(root, graphPath)));
const sha256 = (bytes) => createHash("sha256").update(bytes).digest("hex");
const args = [
  "--graph", graphPath, "--limit", "1000", "--reps", String(reps),
  "--threads", "none", coordinationReuse ? "--regional-coordination" : constructionReuse ? "--regional-construction" : repairReuse ? "--regional-repair" : regional ? "--regional-routing" : planning ? "--routing-planning" : "--routing-completion", "1",
];
const result = JSON.parse(execFileSync(binary, args, {
  cwd: root, encoding: "utf8", maxBuffer: 32 * 1024 * 1024,
}));
assert.equal(result.applied, false);
assert.equal(result.inputUnchanged, true);
assert.equal(result.inputGeometryFingerprint, "79e64fa0fe98e5fe");
assert.equal(result.reps, reps);

function segments(route) {
  return route.points.slice(1).flatMap((b, i) => {
    const a = route.points[i];
    assert([a.x, a.y, b.x, b.y].every(Number.isFinite));
    const dx = Math.abs(b.x - a.x), dy = Math.abs(b.y - a.y);
    assert(dx <= 1e-6 || dy <= 1e-6, `nonorthogonal ${route.id}`);
    if (dx <= 1e-6 && dy <= 1e-6) return [];
    return [{
      horizontal: dy < 1e-6,
      coord: dy < 1e-6 ? a.y : a.x,
      low: dy < 1e-6 ? Math.min(a.x, b.x) : Math.min(a.y, b.y),
      high: dy < 1e-6 ? Math.max(a.x, b.x) : Math.max(a.y, b.y),
    }];
  });
}

for (const variant of Object.values(result.variants)) {
  assert.equal(variant.totalMs.samples, reps);
  assert.equal(variant.samplesMs.length, reps);
  assert.equal(variant.phaseSamplesMs.length, reps);
  for (const [i, phases] of variant.phaseSamplesMs.entries()) {
    assert.equal(phases.length, 8);
    assert(phases.every((p,j) => j === 0 || j === 5 ? p >= 0 : p > 0), "instrumented phase times must be present");
    assert.equal(phases[5] > 0, variant.work.trackRepair !== null);
    assert(phases.reduce((a, b) => a + b, 0) <= variant.samplesMs[i] + 0.01);
  }
  const expected = graph.edges.map((e) => e.id).sort();
  const routes = new Map(variant.routes.map((r) => [r.id, r]));
  assert.equal(routes.size, graph.edges.length);
  assert.deepEqual([...routes.keys()].sort(), expected);
  assert.deepEqual([
    ...variant.fixedRoutes.map((r) => r.id),
    ...variant.delegated.map(([id]) => id),
  ].sort(), expected);
  assert.equal(variant.constructed, variant.fixedRoutes.length);
  for (const fixed of variant.fixedRoutes) assert.deepEqual(routes.get(fixed.id), fixed);
  const sceneRoutes = new Map(variant.scene.routes.map((r) => [r.id, r]));
  assert.equal(sceneRoutes.size, graph.edges.length);
  const nodes = new Map(variant.scene.nodes.map((n) => [n.id, n.bounds]));
  assert.equal(nodes.size, graph.nodes.length);
  for (const edge of graph.edges) {
    const route = routes.get(edge.id), drawn = sceneRoutes.get(edge.id);
    assert.equal(drawn.source, edge.source);
    assert.equal(drawn.target, edge.target);
    assert.deepEqual(drawn.points, route.points);
    for (const [endpoint, nodeId, point] of [
      ["source", edge.source, route.points[0]], ["target", edge.target, route.points.at(-1)],
    ]) {
      const ports = variant.scene.ports.filter((p) => p.edgeId === edge.id && p.endpoint === endpoint);
      assert.equal(ports.length, 1);
      const port = ports[0], box = nodes.get(nodeId);
      assert.equal(port.nodeId, nodeId);
      assert.deepEqual(port.position, point);
      assert(point.x >= box.x - 1e-6 && point.x <= box.x + box.width + 1e-6);
      assert(point.y >= box.y - 1e-6 && point.y <= box.y + box.height + 1e-6);
      assert([
        Math.abs(point.x - box.x), Math.abs(point.x - box.x - box.width),
        Math.abs(point.y - box.y), Math.abs(point.y - box.y - box.height),
      ].some((d) => d < 1e-6), `port not on boundary ${edge.id}`);
    }
  }
  // Independently enumerate every pair with the constructor's strict segment rule.
  const allSegments = variant.routes.map(segments), conflicts = [];
  for (let i = 0; i < allSegments.length; i++) {
    for (let j = i + 1; j < allSegments.length; j++) {
      if (allSegments[i].some((a) => allSegments[j].some((b) =>
        a.horizontal === b.horizontal && Math.abs(a.coord - b.coord) < variant.trackSpacing - 1e-6
        && a.low < b.high && b.low < a.high,
      ))) conflicts.push([variant.routes[i].id, variant.routes[j].id]);
    }
  }
  assert.deepEqual(conflicts, variant.trackConflicts);
  const fixedIds = new Set(variant.fixedRoutes.map((r) => r.id));
  variant.trackConflictCounts = { fixedFixed: 0, fixedFallback: 0, fallbackFallback: 0 };
  for (const [a, b] of conflicts) {
    const count = Number(fixedIds.has(a)) + Number(fixedIds.has(b));
    variant.trackConflictCounts[["fallbackFallback", "fixedFallback", "fixedFixed"][count]]++;
  }
  const m = variant.metrics;
  assert.equal(m.edges, graph.edges.length);
  assert.equal(m.nodes, graph.nodes.length);
  assert.equal(variant.hardViolations,
    m.siblingOverlaps + m.crossBranchOverlaps + m.lineNodeHits + m.containerTraversals + m.sharedLinePairs,
  );
  assert.equal(variant.hardMetricValid, variant.hardViolations === 0);
  assert.equal(variant.strictTrackValid, variant.trackConflicts.length === 0);
  assert.equal(variant.eligible,variant.hardMetricValid && variant.strictTrackValid
    && variant.geometryIssues.length === 0 && variant.routingIssues.length === 0);
}
const improves = (a,b) => {
  const i = a.findIndex((v,i) => v !== b[i]);
  return i !== -1 && a[i] < b[i];
};
let changed = [], endpointDifferences = [];
if (!regional) {
const { chains, existing, chainsFixedTracks: repaired } = result.variants;
assert.deepEqual(chains.scene.nodes, existing.scene.nodes, "identical comparison geometry");
assert.equal(chains.constructed, 347);
assert.equal(chains.delegated.length, 11);
assert.equal(chains.work.construction.searches, 0);
assert.equal(chains.work.construction.states, 0);
assert.equal(chains.work.construction.transitions, 0);
assert(chains.work.legacyCandidates > 0);
assert(existing.work.construction.searches > 0);
assert.equal(chains.trackConflictCounts.fixedFixed, 0);
assert.equal(chains.hardViolations, 0);
assert.equal(chains.geometryIssues.length, 0);
assert.equal(chains.routingIssues.length, 0);
assert(existing.hardViolations > 0, "keep baseline failures visible");
assert.deepEqual(repaired.scene.nodes,chains.scene.nodes);
assert.deepEqual(repaired.scene.ports,chains.scene.ports);
assert.deepEqual(repaired.fixedRoutes,chains.fixedRoutes);
assert.deepEqual(repaired.delegated,chains.delegated);
assert.deepEqual(repaired.trackConflicts,[]);
assert.equal(repaired.strictTrackValid,true);
assert.equal(repaired.hardViolations,0);
assert.deepEqual(repaired.geometryIssues,[]);
assert.deepEqual(repaired.routingIssues,[]);
assert.equal(repaired.work.trackRepair.initialPairs,4);
assert.equal(repaired.work.trackRepair.remainingPairs,0);
assert.equal(repaired.work.trackRepair.moves,2);
assert.equal(repaired.work.construction.searches,0);
assert.equal(repaired.work.legacyCandidates,chains.work.legacyCandidates);
assert.equal(repaired.work.legacyGridPops,chains.work.legacyGridPops);
changed = repaired.routes.filter((r,i) => JSON.stringify(r) !== JSON.stringify(chains.routes[i])).map((r) => r.id);
assert.deepEqual(changed,["e449","e471"]);
const parents = new Map(graph.nodes.map((n) => [n.id,n.parentId]));
const margin = Math.max(result.inputOptions.routingClearance,
  result.inputOptions.requiredClearance === undefined ? 0 : result.inputOptions.requiredClearance + repaired.trackSpacing);
for (const id of changed) {
  const edge = graph.edges.find((e) => e.id === id);
  const excluded = new Set([edge.source,edge.target]);
  for (let node of [edge.source,edge.target]) {
    while ((node = parents.get(node))) excluded.add(node);
  }
  const route = repaired.routes.find((r) => r.id === id);
  for (const n of repaired.scene.nodes) {
    if (excluded.has(n.id)) continue;
    const b = n.bounds;
    assert(!segments(route).some((s) => s.horizontal
      ? s.coord > b.y - margin + 1e-6 && s.coord < b.y + b.height + margin - 1e-6
        && s.low < b.x + b.width + margin - 1e-6 && s.high > b.x - margin + 1e-6
      : s.coord > b.x - margin + 1e-6 && s.coord < b.x + b.width + margin - 1e-6
        && s.low < b.y + b.height + margin - 1e-6 && s.high > b.y - margin + 1e-6,
    ), `new obstacle hit ${id}/${n.id}`);
  }
}
endpointDifferences = existing.scene.ports.filter((port) => {
  const other = chains.scene.ports.find((p) => p.id === port.id);
  assert(other, `missing comparison port ${port.id}`);
  return port.position.x !== other.position.x || port.position.y !== other.position.y;
}).map((port) => port.id);

if (planning) {
  const { chainsAligned: aligned, chainsAlignedCached: cached } = result.variants;
  assert.deepEqual(aligned.scene,cached.scene);
  assert.deepEqual(aligned.key,cached.key);
  assert.deepEqual(aligned.trackConflicts,cached.trackConflicts);
  assert.equal(aligned.constructed,346);
  assert.equal(aligned.eligible,true);
  assert.equal(aligned.work.construction.searches,0);
  assert(aligned.metrics.bends < repaired.metrics.bends);
  assert(aligned.metrics.crossings < repaired.metrics.crossings);
  assert(aligned.metrics.area < repaired.metrics.area);
  assert.equal(aligned.work.planning.ordinalComputations,95);
  assert.equal(aligned.work.planning.laneComputations,349);
  assert(aligned.work.planning.channelComputations > 0);
  assert(aligned.work.planning.preferredChangesAfter < aligned.work.planning.preferredChangesBefore);
  for (const phase of ["ordinal","lane","channel"]) {
    assert.equal(cached.work.planning[`${phase}Computations`],0);
    assert(cached.work.planning[`${phase}Reuses`] > 0);
  }
  const rows = result.planningCandidateSequence.rows;
  assert.deepEqual(rows.map((r) => r.mode),[0,1,2,3,4,0]);
  assert.equal(rows[0].sceneFingerprint,aligned.sceneFingerprint);
  assert.equal(rows.at(-1).sceneFingerprint,aligned.sceneFingerprint);
  assert(rows.some((r) => !r.eligible), "keep ineligible aligned candidates visible");
  assert(rows.slice(1).every((r) => r.planning.ordinalReuses > 0));
  assert(rows.slice(1).every((r) => r.planning.laneComputations > 0), "global changes invalidate cross domains");
  for (const row of rows) {
    assert.equal(row.coldCachedIdentical,true);
    assert(row.hardViolations === 0 || !row.eligible);
    assert(row.trackConflicts === 0 || !row.eligible);
    const selected = row.eligible && (!row.baseline.eligible || improves(row.key,row.baseline.key))
      ? "aligned" : row.baseline.eligible ? "baseline" : null;
    assert.equal(row.diagnosticSelection,selected);
  }
  } else {
    const {baseline} = result.variants;
    assert.equal(result.repairReuse,repairReuse);
    assert.equal(result.constructionReuse,constructionReuse);
    assert.equal(result.coordinationReuse,coordinationReuse);
    assert.equal(baseline.eligible,true);
    assert.equal(baseline.sceneFingerprint,"bb7b0374007882a5");
    assert(result.evaluatedSwaps > 0 && result.evaluatedSwaps <= result.diagnosticBudget);
    assert.equal(Object.keys(result.variants).length,result.evaluatedSwaps+1);
    const parents = new Map(graph.nodes.map((n) => [n.id,n.parentId ?? "<root>"]));
    const ancestors = (id) => {
      const out = [];
      while (id !== "<root>") {out.push(id);id = parents.get(id);}
      return [...out,"<root>"];
    };
    const absolute = (boxes,id) => {
      const r = {...boxes[id]};
      for (const ancestor of ancestors(parents.get(id)).filter((id) => id !== "<root>")) {
        r.x += boxes[ancestor].x;r.y += boxes[ancestor].y;
      }
      return r;
    };
    const same = (a,b) => JSON.stringify(a) === JSON.stringify(b);
    const changes = (before,after) => Object.keys(after).filter((id) => !same(before[id],after[id])).sort();
    for (const [name,v] of Object.entries(result.variants)) {
      assert.equal(v.coldCachedIdentical,true);
      assert.equal(v.coldTotalMs.samples,reps);
      assert.equal(v.diagnosticImprovement,v.eligible && (!baseline.eligible || improves(v.key,baseline.key)));
      if (constructionReuse || coordinationReuse) {
        const w=v.work.constructionReuse,c=v.work.construction;
        assert(w.obstaclePreparationsAvoided > c.preparedEdges);
        assert.equal(w.sharedIndexes,1);
        assert.equal(w.geometriesComputed+w.geometriesReused,c.plannedAttempts);
        assert.equal(w.obstacleDecisionsComputed+w.obstacleDecisionsReused,c.plannedAttempts);
        assert.equal(w.occupancyChecks,c.plannedAttempts-c.plannedObstacleFailures);
        assert.equal(v.work.trackRepair.decisionsReused,0,"isolate construction from final repair reuse");
        assert.equal(v.coldSharedIdentical,true);
        assert.equal(v.coldSharedTotalMs.samples,reps);
        assert.equal(v.coldSharedConstructionMs.samples,reps);
        assert.equal(v.coldConstructionMs.samples,reps);
        assert.equal(v.coldSharedWork.geometriesReused,0);
        assert.equal(v.coldSharedWork.obstacleDecisionsReused,0);
        assert.equal(v.coldSharedWork.geometriesComputed,c.plannedAttempts);
        assert.equal(v.coldSharedWork.obstacleDecisionsComputed,c.plannedAttempts);
        for (const [label,totals] of [["cold",v.coldConstructionSamplesMs],["forked",v.phaseSamplesMs.map((p) => p[2])],["coldShared",v.coldSharedConstructionSamplesMs]]) {
          const samples=v[`${label}ConstructionDetailSamplesMs`],phases=v[`${label}ConstructionDetailMs`];
          assert.equal(samples.length,reps);
          assert(samples.every((s) => s.length===4 && s.every((t) => t>0)));
          assert(Object.values(phases).every((p) => p.samples===reps && p.medianMs>0));
          assert.equal(totals.length,reps);
          assert(samples.every((s,i) => s.reduce((a,b) => a+b,0)<=totals[i]+0.02));
        }
        if (name === "baseline") {
          assert.equal(w.geometriesComputed,0);
          assert.equal(w.obstacleDecisionsComputed,0);
        } else if (v.placement.swap.area==="crate:reference") {
          assert.equal(w.geometriesReused,334);
          assert.equal(w.geometriesComputed,12);
          assert.equal(w.obstacleDecisionsReused,334);
        }
        if (coordinationReuse) {
          const cold=v.coldCoordination,w=v.work.coordination;
          assert.equal(cold.spanReuses,0);
          assert.equal(cold.domainReuses,0);
          assert.equal(cold.spanComputations,w.spanComputations+w.spanReuses);
          assert.equal(cold.domainComputations,w.domainComputations+w.domainReuses);
          assert(w.spanReuses>0 && w.domainReuses>0);
          assert.deepEqual(w,v.coldSharedCoordination);
          assert.equal(v.coordinationSamples.length,reps);
          for (const sample of v.coordinationSamples) assert.deepEqual(sample,w);
        }
      }
      if (repairReuse) {
        const cold = v.coldTrackRepair, cached = v.work.trackRepair;
        assert.equal(cold.decisionsReused,0);
        assert.equal(cold.candidates,cached.candidates+cached.candidatesAvoided);
        assert.equal(cold.decisionsComputed,cached.decisionsComputed+cached.decisionsReused);
        for (const field of ["moves","passes","initialPairs","remainingPairs"]) {
          assert.equal(cold[field],cached[field],`repair parity ${name}/${field}`);
        }
        if (name === "baseline") {
          assert.equal(cached.decisionsComputed,0);
          assert(cached.decisionsReused > 0 && cached.candidatesAvoided > 0);
        } else if (v.placement.swap.area === "crate:reference") {
          assert.equal(cached.decisionsReused,1);
          assert.equal(cached.candidatesAvoided,206);
          assert.equal(cached.candidates,1703);
        }
      }
      if (name === "baseline") continue;
      const p = v.placement, source = result.sourceBoxes;
      const touched = [...p.placedAreas,...p.propagatedAreas];
      assert.equal(p.placedAreas[0],p.swap.area);
      assert.deepEqual(p.placedAreas,[p.swap.area]);
      assert(p.placedAreas.every((id) => ancestors(p.swap.area).includes(id)));
      assert.deepEqual(p.propagatedAreas,ancestors(p.swap.area).slice(1,1+p.propagatedAreas.length));
      assert(p.regionRealizations < Object.keys(result.sourcePlans).length);
      assert.deepEqual(p.changedBoxes,changes(source,p.boxes));
      assert.deepEqual(p.changedPlans,changes(result.sourcePlans,p.plans));
      assert(p.changedPlans.every((id) => touched.includes(id)));
      assert.deepEqual(p.movedAbsolute,Object.keys(source).filter((id) =>
        !same(absolute(source,id),absolute(p.boxes,id))).sort());
      const sourcePorts = Object.fromEntries(result.sourcePorts.map((p) => [p.id,p]));
      assert.deepEqual(p.changedPorts,p.ports.filter((p) => !same(p,sourcePorts[p.id])).map((p) => p.id));
      for (const node of graph.nodes) {
        if (!touched.includes(node.id) && !touched.includes(parents.get(node.id))) {
          assert.deepEqual(p.boxes[node.id],source[node.id],"unaffected local frame preserved");
        }
      }
      const expected = structuredClone(result.sourcePlans[p.swap.area].ranks);
      const rank = expected[p.swap.rank];
      [rank[p.swap.left],rank[p.swap.left+1]] = [rank[p.swap.left+1],rank[p.swap.left]];
      const realRanks = (ranks) => ranks.map((r) => r.filter((id) => parents.has(id)));
      assert.deepEqual(realRanks(p.plans[p.swap.area].ranks),realRanks(expected));
      for (const [area,plan] of Object.entries(p.plans)) {
        if (area !== p.swap.area) {
          assert.deepEqual(realRanks(plan.ranks),realRanks(result.sourcePlans[area].ranks),
            "ancestor real-node order remains fixed");
        }
      }
      assert.equal(v.planningSamples.length,reps);
      assert(v.planningSamples.every((s) => s.ordinalReuses > 0 && s.laneReuses > 0));
      for (const sample of v.planningSamples) assert.deepEqual(sample,v.work.planning,
        "independent forks have identical dependencies regardless of trial order");
      assert(v.work.planning.laneComputations < 349,"local candidates keep some cross domains");
      assert(v.work.planning.channelComputations < 58,"local candidates keep some physical channels");
      assert.equal(v.work.construction.searches,0);
    }
  }
}

const sourcePaths = execFileSync("git", [
  "ls-files", "--cached", "--others", "--exclude-standard", "crates", "Cargo.toml", "Cargo.lock",
], { cwd: root, encoding: "utf8" }).trim().split("\n").sort();
const report = {
  ...result,
  generatedAt: new Date().toISOString(),
  input: { path: graphPath, sha256: sha256(readFileSync(join(root, graphPath))) },
  binary: { path: binary, sha256: sha256(readFileSync(binary)), profile: basename(dirname(binary)) },
  generator: { path: "benchmarks/routing-completion-check.mjs", sha256: sha256(readFileSync(fileURLToPath(import.meta.url))) },
  sourceHashes: Object.fromEntries(sourcePaths.map((p) => [p, sha256(readFileSync(join(root, p)))])),
  command: [binary, ...args],
  automaticPortDifferences: endpointDifferences,
  repairedFallbackEdges: changed,
  checks: {
    completeEdgePartition: true, logicalDirection: true, fixedRoutesUnchanged: true,
    endpointsOnResolvedPorts: true, finiteOrthogonalRoutes: true,
    strictTrackConflictsEnumerated: true,
    repeatedSceneFingerprintStable: true,
    ...(!regional ? { identicalGrownNodes: true, fixedAwareTrackSpacingSatisfied: true,
      changedFallbackObstacleClearanceChecked: true } : {
      regionalLocalFramesChecked: true, ancestorOnlyPlacement: true,
      exactRegionalSwapChecked: true, coldCachedIdentity: true, eligibilityAndSelectionChecked: true,
      ...(repairReuse ? { exactRepairWorkParity:true,repairCandidatesAvoidedChecked:true } : {}),
      ...(constructionReuse || coordinationReuse ? {exactConstructionWorkParity:true,constructionGeometryAndObstacleReuseChecked:true,
        coldSharedControlIdentity:true,trackAcceptanceAlwaysFresh:true} : {}),
      ...(coordinationReuse ? {exactCoordinationProjectionWork:true} : {}),
    }),
    ...(planning ? { alignedColdCachedIdentity: true, candidateEligibilityAndSelectionChecked: true,
      changedCrossDomainsInvalidate: true, warmPlanningAllReused: true } : {}),
  },
};
console.log(JSON.stringify(report));
