# Algorithm Improvements: Routing, Ports and Placement

See [Recursive, score-driven optimization](recursive-optimization.md) for the proposed regional strategy portfolio, shared quality model, routing/port/label catalogue and migration plan. This is a target architecture; existing acceptance behavior remains unchanged until the documented migration.

See [Layout engine improvement plan](layout-improvement-plan.md) for the step-by-step performance work (measured hotspots, exact speedups) and the planned algorithm replacements per layout family (layered phases, organic stress and overlap removal, tidy trees).

Updated: 2026-10-22. Status: implementation specification, not a claim that all described capabilities exist. English translation of the original German specification, extended with topology-aware free placement (A4b).

Complements the [product specification](specification.md) and [project status](project-status.md). Package identifiers A0–A6 remain stable. A4a denotes the existing local placement work; A4b adds structural rearrangement. Existing links to this file remain valid.

## 1. Purpose and scope

First identify and reduce unintended shared lines and routes through nodes. Then improve border clearance, port arrangement and node placement. Every published intermediate result must be complete and drawable. Global optimization must neither block local edits nor replace newer results with stale ones.

This phase focuses on orthogonal routes. Rounded rendering uses the same corridors; corner radii must not intersect obstacles. Arbitrary curves, radial routes, sequence diagrams and timing layouts are not implemented here, but interfaces must allow their later geometry-specific validation. Respect local constraints in mixed diagrams instead of forcing a single global hierarchical layout.

**Free placement means more than removing a grid or sliding nodes within fixed layers.** Where constraints permit it, the engine must be able to change the structural arrangement: node ordering, cycle arrangement, edge order around nodes, face/corridor choices and inside/outside routing. A4b specifies the algorithmic support for this meaning. Layer-free placement and grid snapping are independent choices.

Non-goals: globally optimal crossing minimization, crossing-free drawings of arbitrary graphs under arbitrary constraints, a complete ELK port, replacing A* with an unmeasured shortest-path algorithm, or a new rendering framework.

## 2. Historical starting point and status interpretation

The original specification recorded this baseline before subsequent implementation iterations:

- `crates/routing/src/channel.rs`: orthogonal grid, A*, four congestion rounds, last-round output rather than best-round selection. Node interiors blocked, clearance bands penalized. Missing grid paths could produce a conflicting direct fallback.
- `crates/layout/src/ports.rs`: explicit sides/offsets preserved; automatic sides based on relative node positions and automatic order based on peer-node positions. Actual routing corridors and explicit/automatic slot occupancy were not jointly optimized.
- `crates/layout/src/lib.rs`: bounded local moves and swaps with rerouting; optimizer scores and external metrics implemented separately.
- `crates/scene/src/metrics.rs`: metric version 2, including a fixed 20-unit shared-stub exemption and broad container exemptions.
- `crates/runtime/src/lib.rs`: candidate selection, revision protection and cooperative cancellation between candidates; no fine-grained interruption inside long routing searches or persistent resumable state.

This is historical context, **not the current implementation inventory**. Later iterations introduced metrics v3, published-key acceptance and additional routing work. Use current code and measured reports for completion status; the requirements below remain acceptance targets.

## 3. Mandatory rules

### 3.1 Hard geometry constraints

A candidate published as valid MUST have:

1. Complete finite geometry, unique identities and an acyclic containment hierarchy.
2. No overlapping independent node/container bodies. Intended parent–child containment is allowed; headers, padding and fixed comment boxes act as obstacles according to their role.
3. No route through a foreign node body or its own endpoint body. Boundary contact at the assigned port is allowed.
4. A continuous route, or an explicit valid bundle-route reference, for every resolved visible edge. Preserve logical source/target direction, including feedback edges and cycles.
5. Unchanged explicit ports, node pins and hard local layout boundaries unless the user explicitly changes those constraints.
6. Assigned boundary transitions for edges entering/leaving containers. Foreign containers are not general transit corridors; their headers remain blocked.

If constraints cannot be satisfied, retain the last complete result and report concrete conflicts. A conflicting preview may be shown but must not be marked valid. Missing routes must never improve a score by disappearing. A direct fallback is explicit conflict geometry, not a routing success.

### 3.2 Soft objectives and acceptance

From a valid baseline, accept only valid candidates. For an invalid baseline, first reduce a repair vector of missing routes, node collisions and node traversals lexicographically; reject structurally unusable input before search. Repair must not violate explicit constraints.

Hard constraints for a published result (ADR 0002, ADR 0005): no unintended shared edge segments outside a declared bundle or bus, and, when a required clearance is set, no edge within that clearance of any node, its own nodes included, beyond the attachment at its port. Exact overlap is not traded against any soft objective.

Default soft-objective order within valid candidates:

1. Border-hugging edge/obstacle pairs without a required clearance, then length inside clearance bands.
3. True crossings, including crossings of edges with common endpoint nodes.
4. Bends.
5. Length, movement from the reference and area growth.

This is an initial profile, not a universal aesthetic. Profiles may reorder soft objectives, but cannot disable hard constraints. Apply movement/area limits so a tiny clearance gain cannot expand the drawing without bound. Initial limits are experimental job parameters, not performance guarantees.

Equal quality retains the incumbent. Stable IDs break search-order ties. Document absolute/relative tolerances for floating-point comparisons; count discrete conflicts exactly. Sorting keys must remain transitive.

**Search state and published state are separate.** Search may visit equal or worse intermediate states, including alternative topological arrangements, within its budget. Only a complete validated candidate may replace the best published state. A lexicographic improvement can worsen a lower-priority metric: it does not require every metric to improve independently. Report these tradeoffs.

The same finalization, validator and configured objective order must be used for competing final candidates. Local improvements do not prove a better global search outcome. During algorithm migration, retain an explicitly computed baseline candidate where needed to make non-regression claims. Do not claim a guarantee against a baseline that was not evaluated.

## 4. A0 — Shared conflict definitions and reliable measurement

**Goal:** the optimizer and evaluation pipeline agree on what constitutes improvement.

### 4.1 Geometry definitions

- Normalize duplicate and forward-collinear intermediate points before measurement; preserve reversals.
- Shared lines are positive collinear overlap beyond geometric tolerance. A point touch is not overlap. Count each edge pair once; merge intervals before summing length. Triple occupancy is pairwise and reported as such.
- Near-parallel routes closer than `trackSpacing` form a separate spacing conflict, not exact overlap or crossing.
- Allowed sharing requires semantic identity: the same physical port or bundle **and** an identified shared section. Equal coordinates or the same endpoint node alone are insufficient. Port bundling permits a shared stem; identical complete paths require full-edge bundling.
- Derive stub exemptions from explicit port/stub sections, ending at their declared branching point, not a universal 20-unit constant. The first normalized straight run is not automatically proof that its entire length is an intended shared stem.
- Border hugging measures route length inside an obstacle's open clearance band. Exact requested clearance is permitted. Exempt only the assigned attachment segment at the endpoint; long routes along the endpoint node's own border remain measurable. Separate node and container clearance.
- A crossing is a true traversal, not a touch or overlap. Edges sharing an endpoint can cross later. Distinguish crossing points from crossing pairs.
- Labels reserve their measured rectangles; port labels receive no blanket exemption. Host text measurement does not alter canonical graph semantics.

### 4.2 Implementation

Share geometry primitives and conflict classification below the optimizer/report boundary. Initially, full validation of a locally changed complete snapshot is acceptable; it is not the same operation as globally rerouting that snapshot. Optimize measurement incrementally only when profiling justifies it. Preserve conflicts outside the changed region in the total score.

Version semantic metric changes and document migration. The original A0 transition targeted v3. Remeasure old raw snapshots into new records rather than overwrite historical values or compare incompatible versions. Keep global measurement thresholds identical within a comparison; report profile-specific constraint violations separately.

**Acceptance:** segmentation-invariant scores; intended stubs exempted but excess sharing detected; shared-endpoint crossings and foreign-container traversal recognized; unknown/cyclic parents and missing paths rejected rather than hanging or receiving zero scores. Measurement errors reject candidates, never produce a default all-zero score.

## 5. A1 — Retain the best routing round

**Goal:** additional congestion rounds cannot displace a better fully evaluated result merely because they ran later.

1. Use complete existing paths as an initial candidate if endpoints and obstacles still match.
2. Each finished round supplies a complete candidate, including stubs, boundary transitions and bundle references.
3. Run the same finalization and label processing used for published output; normalize, validate and score.
4. Replace the incumbent only on improvement. The next search round may start from a different, worse trial.
5. Stop on round/work/time limits or a bounded stagnation policy. Two non-improving rounds is an initial value to evaluate, not an optimality claim.
6. Mid-round cancellation returns the last complete checked result.

```text
best = valid_existing_or_first_complete_candidate
search = initial_routes
for round in bounded_rounds:
    trial = reroute_round(search, occupancy, stop)
    if trial.incomplete: break
    evaluated = finalize_validate_score(trial)
    if improves(evaluated, best): best = evaluated
    search = trial
return best_or_explicit_failure
```

Keep the current search state and best result rather than indefinitely storing every round. Do not introduce a routing-to-layout dependency; use a neutral geometry/evaluation contract. Round zero may use no congestion penalty; subsequent rounds negotiate occupancy.

**Acceptance:** a worse final round does not replace a better earlier one; a single round works; stopping before/during/after a round loses no edges; fixed-input/seed/work-budget execution is reproducible. Use real congested geometries, not only comparator unit tests.

## 6. A2 — Targeted local rerouting

**Goal:** repair conflicts locally and identify insufficient routing space.

- Track segment occupancy with edge and allowed-bundle references; index horizontal and vertical segments separately.
- Group conflicts into local connected areas. Prioritize body traversals, forbidden sharing, spacing violations and border hugging.
- Rip up and reroute affected edges only; remove an edge's old occupancy before searching for its replacement. Initially freeze unrelated routes.
- Try a small deterministic set of orders: strongest conflict first, reverse order and stable IDs. Do not enumerate unrestricted permutations.
- A* uses nonnegative length, bend, proximity and occupancy costs with a valid geometric lower bound. Search penalties are heuristics; final acceptance uses actual metrics.
- Add spaced tracks locally, not full-diagram lines for every coordinate.
- Escalate a failed local region gradually toward its container. Hard obstacles remain forbidden, not merely expensive.

Estimate channel demand from available width after node/label reservations versus usable parallel tracks. Width alone is not a feasibility proof: bends and port fans can still block routing. Emit a `spaceRequest` with region, axis, additional width, affected edges and blockers for diagnosed bottlenecks. A4 controls node movement; the router does not move nodes implicitly.

A first local transaction may move one leaf without resizing containers. Its affected set includes incident routes and routes intersecting old/new obstacle and clearance envelopes. Form a complete snapshot from changed and retained routes, then validate/score it. **A full reroute is a different candidate, not a mandatory correctness check for every local attempt.** Measure affected-set discovery, routing, validation and scoring separately before adding caches.

**Acceptance:** parallel fanouts, feedback edges, loops, narrow/U-shaped corridors and container transitions improve when space exists. Insufficient space produces diagnostics rather than endless search or implicit bundling. A local candidate need not reproduce a full reroute byte-for-byte.

## 7. A3 — Ports as a joint slot-allocation problem

### 7.1 Contracts

Explicit ports remain fixed. Automatic-port data distinguishes allowed sides, minimum spacing, corner reservations and optional ordering/bundle groups. Start with internal data rather than a large public option surface.

- Reserve explicit ports and their label areas first.
- Allocate automatic slots in remaining intervals, never on top of explicit slots.
- Report conflicting explicit constraints instead of silently moving them.
- Explicit relative offsets remain relative when node size changes; absolute pinning requires its own contract.

### 7.2 Optimization

1. Use existing mutually facing sides as the baseline.
2. Improve order using entry/exit positions in the first free routing corridor. Peer-node centers remain an initial heuristic.
3. Search a bounded neighborhood: adjacent swaps, allowed side changes for automatic groups and interval redistribution.
4. Locally reroute every affected edge, including edges whose slots shifted indirectly.
5. Accept ports, routes and labels atomically after final evaluation.

A route arriving from below should use a bottom automatic port when admissible and better. An explicit top port stays at the top; its route must go around the body. Input/output is a semantic role, not a screen side.

Request node growth from A4 if slots do not fit. Bundle ports only under an explicit compatible policy. Stable aggregation-boundary port identities connect outer and inner routes; collapse/expand never rewrites logical endpoints.

**Acceptance:** all four directions and hybrid, bottom fan-in, mixed explicit/automatic slots, shared explicit ports, self-loops, bundles, labels and collapse/expand. Avoid changing unaffected sides. A4b's embedding order can supply additional ordering constraints.

## 8. A4a — Routing-aware local placement

**Goal:** create room without unnecessarily rearranging the whole graph.

Try cheaper interventions first:

1. Slide a node within its rank without changing order.
2. Increase local rank/sibling spacing to open a channel.
3. Grow a node for ports/labels and move neighbors safely.
4. Swap adjacent siblings.
5. Grow the responsible container and propagate size changes upward.
6. Try a small number of alternate container orientations/arrangements.

Respect pins, local layout logic, clearances, labels and movement/area limits. A container with another inner layout is externally a box with ports; changing its interior requires an explicit delegated layout request. Comments without rank influence remain obstacles unless declared nonblocking overlays.

A grid is a constraint independent of topology. Fixed-grid moves land on legal points. Shifting a local grid origin is an explicit later policy, never an implicit change to free placement.

Transaction: propose boxes → check containment → resolve ports → reroute incident and spatially affected edges → place labels → validate the complete candidate. Include all paths that might intersect old/new node envelopes, not just incident edges.

**Acceptance:** bounded channel opening improves bottleneck fixtures; pins and non-overlap hold; unrelated geometry stays unchanged except documented ancestor growth. Do not silently relax impossible constraints.

## 9. A4b — Topology-aware free placement

### 9.1 Meaning and guarantees

Free placement permits changing the structural arrangement, not just coordinates within a fixed layer assignment. Provide an additional **topology → orthogonal shape → coordinates** candidate pipeline, alongside the hierarchical baseline.

Cycle detection is not a crossing-removal algorithm. Directed strongly connected components (SCCs) identify feedback regions; undirected biconnected blocks/articulation points help organize embedding problems. These decompositions serve different purposes and must not be confused. Do not enumerate all simple cycles: their number can grow exponentially.

A planar embedding specifies the cyclic order of incident edge ends around nodes and the faces between them. It does not yet assign rectangular node sizes, port sides, bends or coordinates. A planar abstract graph may fail our additional constraints. Nonplanarity, constraint incompatibility and search-budget exhaustion are three distinct outcomes.

### 9.2 Analysis and identity preservation

1. Analyze the visible projection and its boundary attachments. Preserve canonical graph, logical arrows, edge IDs and container ownership.
2. Build an undirected analysis skeleton for abstract planarity. Map parallel edges to their representative and retain self-loops/antiparallel identities for explicit reinsertion; simplification must not erase port/order requirements.
3. Compute SCCs for semantic grouping and undirected blocks for structural decomposition. SCC membership alone must not impose a hard container or remove inter-component constraints.
4. Run a planarity test capable of producing an embedding or nonplanarity witness, such as Boyer–Myrvold. A diagnostic-only implementation may run before a geometric constructor exists.
5. Report scope: abstract skeleton, constrained region or complete visible projection. Never report the whole diagram crossing-free from a simplified skeleton test alone.

### 9.3 Candidate construction

For an abstractly planar supported region:

- Keep a rotation system, stable edge-end identities, face boundaries and a selected outer face as derived layout state.
- Try a bounded number of admissible outer faces, block flips and arrangements. A user-highlighted cycle can suggest a boundary candidate; a simple cycle is not necessarily a face of the whole graph.
- Translate cyclic edge order into compatible port order/side choices. Reserve fixed slots and check all explicit constraints before accepting an embedding. A3 handles physical spacing.
- Compute an orthogonal shape, then compact into coordinates with node dimensions, clearances, labels and channel capacity. Revalidate the embedding after geometric construction.
- Nodes are rectangles with potentially more than four ports; a point-vertex orthogonal algorithm restricted to degree four is insufficient. Use a box/port construction that preserves incident order, or explicitly reject unsupported cases in the first prototype.
- Keep parallel edges and self-loops distinct unless bundling was requested. Reinsertion must preserve the chosen embedding and reserve enough space.
- Return a complete candidate through the common finalization, label, validation and published-key pipeline. Keep the hierarchical result as an alternative, not an overwritten starting state.

For nonplanar regions, a later planarization heuristic may select a planar subgraph and insert remaining edges with controlled crossings. Any temporary crossing vertices are internal representation only: they never become semantic junctions. This does not promise a minimum crossing number.

### 9.4 Constraints, nesting and mixed layouts

Hard port sides/order, pins, exact ranks, containment and boundary attachment order can rule out an otherwise valid embedding. A soft preferred direction may influence candidate selection; a hard layer constraint must not be silently dropped to enable free placement. Cyclic directed graphs cannot have every edge monotonically advancing along one strict flow axis.

Start with a flat, bounded region with movable nodes and automatic ports. Then support one container with fixed boundary attachments. Nested embedding and constrained-planarity support are later stages, not implied by an ordinary planarity test.

A local rearrangement preserves exterior attachment identities and constraints. Treat neighboring containers as boxes; reuse their inner layouts. Do not flatten mixed diagram types into one planarization problem. After growth or port-order changes, validate exterior routes as well as the rearranged interior.

### 9.5 Incremental execution and diagnostic state

Cache embeddings only under matching topology, constraint and view identities. A pure coordinate change may preserve an embedding; topology/port-order changes may invalidate it. Limit alternative embeddings by deterministic work budget and honor A5 cancellation boundaries. Search may traverse worse arrangements; publication retains the best complete admissible result.

Diagnostics should distinguish: `abstractPlanar`, `abstractNonPlanar` (with witness when available), `unsupportedConstraints`, `candidateConstraintFailure`, and `budgetExhausted`. A single failed candidate does not prove constrained infeasibility. Measure analysis, embedding, orthogonalization and compaction separately.

### 9.5a Recursive, per-container pipeline

Planarity is applied locally, per container, not to the whole diagram. For each visible container:

1. Treat its direct children as nodes; sub-containers appear as boxes with ports.
2. Analyze that local graph: find planar regions and their embeddings.
3. Coordinate boundary transitions: the order and position of connections between the inside and the outside layout.
4. Compute the inner layout and report the size required outside.
5. Adjust the outer layout and reconcile the interfaces in a bounded number of iterations.

Two individually crossing-free sub-drawings do not combine into a crossing-free whole when their boundary ports are ordered inconsistently. Boundary ports and their order are therefore part of the optimization. "Layer" has three meanings and must not be conflated: container levels (recursive layout with agreed interfaces), hierarchical ranks (extra arrangement constraints that can exclude planar embeddings), and physical routing layers (crossover-capable layers, as on circuit boards). Only the first is in scope here.

A nonplanar whole graph remains a valid target: it can still contain planar regions that are worth laying out locally. Measurements must match the drawn projection: the full input graph, the visible projection, and individual containers are measured separately, each with its hash and with the treatment of containers, self-loops and parallel edges stated.

### 9.6 First experiment and acceptance

**Stage 1 (state-tcp, first real input):** determine planarity of its actual visible edges with a planarity test that returns an embedding or a nonplanarity result. state-tcp has no containers, so it is a single local region. Record the rotation system and face count. The planarity result must be reproduced inside the repository; external tools are used only as cross-checks in tests.

**Stage 4 (straight-line drawing):** for a planar graph, produce a planar straight-line grid drawing. The method is Chrobak–Payne, applied after triangulation. The drawing is a construction aid for the experiment, not a final orthogonal layout. Its validity (no crossings, no vertex on an edge) is checked by the same geometry code as the metrics.

**Status (recorded with the `arqlet-topology` crate):** Stage 1 on state-tcp: biconnected, planar, 8 faces (Euler check holds). Stage 4: the straight-line drawing is valid by the independent checker, on the Chrobak–Payne grid `(2N − 4) × (N − 2)` for `N` triangulated vertices (Boost documents the same bound for this algorithm). The triangulation adds diagonals where possible; for state-tcp no dummy vertex is needed, and the construction grid is 18 × 9. Planarity uses Demoucron–Malgrange–Pertuiset, not the left-right test, and is checked against networkx on 303 biconnected graphs (148 planar, 155 nonplanar) including relabelled copies. Nonplanarity is reported without a witness. Every planar corpus drawing realises the rotation system of its triangulation at every vertex (one global orientation), so the drawn edge order around a vertex is the embedding's. Flat-region candidate (A4b-4, experimental, option `flatRegion`, off by default): node centres from the drawing grid, scaled by a gap, then the usual channel routing through `assemble`; it joins the candidate pool. On state-tcp under lesbar (same metric, same code): gap 10 gives no change (hugging 2, crossings 5, length 6195); gap 40 is chosen with hugging 0, crossings 8, length 24417; gap 80 is chosen with hugging 0, crossings 8, length 30941. The win under the priority order comes from the spacing, not from a better drawing, and it is not robust, so the candidate stays experimental. With angular port placement (`PortPlacement::Angular`: each side's ports ordered by the direction of their peers), the flat candidate at gap 40 has hugging 0 and crossings 1 (baseline: 2 and 5); at gap 80 crossings return to 8. The port order around every node is checked against the drawn rotation (one global orientation, parallel edges adjacent). Rectangular port slots: each side's automatic ports are checked against slots `pitch` apart (pitch = minimum port spacing, at least 8), with half a pitch reserved at each corner; a flat candidate with an overflowing side is refused rather than squeezed. Ports stay evenly spread on their side: centring them in pitch-spaced slots was measured and rejected (gap 40: crossings back to 8), because packing the ports drives the routes together. Corridors (A4b-4, corridor step): the grid's empty columns and rows are removed, and each corridor shrinks toward its demand minimum (margin plus one track per spanning edge), but a shrink is kept only if every node still sees its neighbours in the same cyclic order (checked against the starting drawing). Corridor widths alone did not remove crossings (margins 24, 60, 120 gave 8 crossings). The crossings came from one route (CLOSED to SYN_SENT) that the router sent through the middle, crossing seven others. The channel router now has a crossing cost (a step onto a point another route already passes), used only by the flat-region option at 200; cost 50 and 800 give the same result. state-tcp under lesbar, same metric: baseline hugging 2, crossings 5, length 6195; flat candidate hugging 0, crossings 0, length 16931. The curated reference (`state-tcp-r0.lwref`, status draft) has node hits 5, hugging 1, crossings 4 (one pair plus three shared-endpoint pairs), length 6213; it is not crossing-free under the metric. The flat candidate is still longer; the length is the lowest priority, so under the priority order it is chosen, and it stays off by default. Length breakdown (`benchmarks/flat_length.py`, results file in `benchmarks/results/`): in the flat candidate every route has exactly its port-to-port Manhattan distance, so its length (16931) comes from the placement spread, not from detours; the baseline has one route with a threefold detour. Changing the starting corridor unit barely changes the length (60: 16531, 240: 16931). Rotating the drawing by 90 degrees shortens it (5070) but breaks the rotation-consistent port order at one node under the current side rule; a diagonal side rule (sectors by the box diagonals) restores the order but changes the outcome, so neither is kept. The published result depends on which candidate wins, so these variants are measured, not tuned. Constrained compaction (implemented): each corridor shrinks toward its demand minimum, and a shrink is kept only if the complete candidate (routes, ports, labels, validation) is geometrically valid, keeps every node's port order equal to the grid drawing's rotation (checked by `port_rotation_ok`, one global orientation), and has a strictly smaller published key. On state-tcp under lesbar the flat candidate keeps zero node hits, zero shared lines, zero hugging and zero crossings, with length 16973 (about the same as before compaction, 16931). Of 45 rejected shrinks, 22 did not reduce the key (length did not drop), 11 created crossings, and 12 changed a node's port order: compaction moves the box centres, so the geometric rotation can change even when the grid's rotation is kept. Using the diagonal side rule removes port-side errors but not the rotation changes, and gives the published result hugging 1, crossings 5, length 5070; it is not kept. Compaction therefore stops where the rotation or the crossings would change; the next lever is routing that keeps crossings at zero in narrower corridors, or a compaction that moves ports with their boxes.

Correction after tracing the trials: the binding limit was the corridor minimum, not the rotation. The demand-based minimum (clearance plus one track per spanning edge) was too tight: at 54 units the router starts to cross routes. Dropping the rotation check changed nothing, and the rotation check protects the embedding in any case. With the corridor minimum set to the clearance (zero) and eight passes, the key check alone keeps the flat candidate crossing-free and non-hugging, and the length falls to 11213 (baseline 6195). The minimum is an input to the search, not a claim about routing capacity; the key guarantees that no shrink adds crossings or hugging. Diagonal sides were not needed for this. Where the remaining length comes from (traced): after compaction nearly every node has its own grid column, so the drawing has about ten columns of 180 units, and the rows are nearly closed. The corridors that still stop the compaction are rows: a zero-width row makes neighbouring nodes overlap (geometry), a row shrink moves centres enough to change the port order (rotation), and a row shrink at about 12 to 43 units creates a crossing. Two further experiments were not kept: (a) the rotated drawing (axes swapped; expected to be shorter, since the nodes are wider than they are tall) starts with one crossing at every starting scale from 120 to 240 and compacts to a longer layout (26271); (b) larger starting scales (360, 480) shorten the direct layout only to 11052 and cost about 70 seconds. The remaining lever is per-node compaction: moving individual node positions along a column instead of whole corridors, under the same port, rotation and crossing checks. Containers, loops, parallel edges and non-biconnected graphs are not handled yet.

Required fixtures for the first experiment:

Use the frozen TCP state graph as the first real diagnostic input, alongside a simple cycle and a small planar graph drawn with avoidable crossings. Determine TCP planarity from its actual edges; the screenshot is not a proof. The cycle CLOSED → SYN_SENT → ESTABLISHED → CLOSE_WAIT → LAST_ACK → CLOSED is a candidate structural hint, not a prescribed global embedding.

Required fixtures:

- state-tcp: planar, drawn straight-line, validated; routed crossings compared with the hierarchical baseline per input;
- simple cycle and planar chorded cycle: crossing-free construction when constraints permit;
- multiple overlapping cycles: do not optimize cycles independently at the expense of the full graph;
- nonplanar K5/K3,3: report nonplanarity, never claim every crossing is removable;
- explicit port-order/pin conflict on an abstractly planar graph: preserve constraints and distinguish candidate failure from proof;
- high-degree box, parallel/antiparallel edges and self-loop: preserve all endpoints and semantics;
- constrained container boundary: retain exterior attachments;
- cancellation: retain a complete prior result;
- alternate embedding that is better only after combined node/port/route changes: permit non-improving internal transitions.

Compare with hierarchical and local-only candidates on identical inputs, constraints and measurement settings. Report crossing reduction, other quality deltas, movement, area and computation cost. No guarantee of zero crossings under arbitrary constraints or 100K-node scalability follows from this experiment.

## 10. A5 — Budgets, progress and frontend/backend handoff

- Check cancellation at candidate start, between edges, after a bounded number of A* expansions and between placement attempts. Measure actual interruption latency.
- Never publish an unfinished path. Retain an old path only if valid against the new geometry; otherwise roll back the affected transaction.
- The host supplies a monotonic clock/cancellation mechanism; the core imports no browser or renderer API. Ordinary Worker messages cannot interrupt synchronous WASM by themselves.
- Separate deterministic work budgets from interactive soft time budgets. Report parallel execution and reproducibility effects explicitly.
- Report completion/interruption state, finished candidates/rounds, work consumed, affected region and remaining conflicts. Host supplies timing; validity is separate from termination status.
- The existing early-stop reason contract may represent successful completion by an absent reason. If adding explicit `completed`, migrate the schema deliberately. `infeasible` requires proof or a definitive constraint contradiction; budget exhaustion is not infeasibility.
- Results carry graph revision, layout version, configuration and view/collapse identity. Reject stale responses; newer local edits win over older backend results.
- Initially resume by restarting from the best snapshot. Add persistent search state later, separately versioned and invalidated by incompatible geometry/configuration.

**Acceptance:** every instrumented interruption yields a complete result or explicit error; revision protection remains intact. Promise no hard millisecond bound before measuring maximum overrun and uninterruptible operations.

## 11. A6 — Scaling after correctness baselines

Report total graph size, visible nodes/edges and routing-state size separately. A 100K-node source with a small projection does not establish routing 100K visible nodes.

Priorities: local grids/container corridors, spatial obstacle indexes, unchanged inner-layout caches, occupancy reuse and incremental metrics. Avoid a global Cartesian product of all port/node coordinates for large views. Boundary ports connect local problems; bound top-down space allocation and bottom-up size propagation.

Measure small diagnostic fixtures, the frozen profile corpus, then progressively larger views across density, depth and port count. Separate native/WASM and warm/cold runs. Record time to usable result, refinement, cancellation latency, memory, search expansions, grid size and quality vector. Cheap local validation belongs to A2 and need not wait for a complete A6 architecture.

## 12. Delivery packages and release gates

| Package | Content | Dependency | Acceptance |
|---|---|---|---|
| A0 | Shared conflicts, validation, metric migration | None | Semantic fixtures and historical snapshot remeasurement |
| A1 | Best round and complete fallback | A0 | Earlier better result retained; cancellation fixtures |
| A2 | Local rerouting, tracks, bottlenecks | A1 | Solvable local conflicts improve; full local-result validation |
| A3 | Fixed/free slots and port order/sides | A0, A2 | Port contracts and fanout fixtures |
| A4a | Routing-aware local placement | A2, A3 | Channel opening without hard geometry regression |
| A4b-1 | Local planarity analysis per visible region and container; embeddings | Stable graph/view identities | Fixtures (K5, K3,3, chorded cycle, state-tcp) match expected outcomes |
| A4b-2 | Chrobak–Payne straight-line grid drawing of planar graphs | A4b-1 | Valid drawings on all planar fixtures; grid size recorded |
| A4b-3 | Container boundary contract: port order derived from the embedding, conflicts reported | A4b-1, explicit port contract | Inconsistent boundary orders reported, never silently reordered |
| A4b-4 | Flat-region candidate: embedding → drawing → edge-order-aware router → pool | A4b-2, A0/A1 acceptance | state-tcp compared with baseline per input; defaults unchanged |
| A4b-5 | One-container candidate with fixed boundary attachments and bounded outer iteration | A4b-3, A4b-4 | Complete candidate compared with the hierarchical baseline |
| A5a | Fine-grained stopping and host clock | Alongside A1–A4 | Complete return at checkpoints |
| A5b | Progress and restart/resume | A5a | Revision-safe handoff and restart |
| A6 | Large views and caches | Correctness/quality baseline | Reproducible size ladder |

For each package, isolate the comparison: same input, profile version, thresholds and deterministic work budget; at least three timing repetitions. Preserve individual values. Compare hard validity, quality vector and resource cost separately. A lower-priority gain cannot compensate for a higher-priority regression under the chosen lexicographic policy; a higher-priority improvement may legitimately worsen later metrics.

Required corpus: fanout/fanin, explicit/automatic ports, narrow corridors, nested containers, cycles/loops, comments/labels, pins, four directions/hybrid, collapse/expand, invalid inputs, interruptions, real Intentweave cases, plus A4b planarity fixtures. Known isolated conflicts have explicit expected outcomes. Arbitrary constraints receive no blanket zero-conflict guarantee.

Use side-by-side inspection and curated references for selected outputs. Bind timing/memory to actual run metadata. Demo/eval profiles share a versioned source; visualization profiles remain separate. Changed measured node/label dimensions are changed layout input.

## 13. Current execution plan (2026-10-22)

This section supersedes the previous next-slice assignment. A0/A1 foundations and the flat planar experiment already exist; do not restart them. A2 repair is active work again. The package IDs above describe capabilities; the steps below define delivery order. All new work must use the asynchronous execution contract in section 15 from the beginning, even when initially executed by one worker.

### 13.1 Current baseline and remaining gaps

- Present: shared metrics/published-key evaluation, channel A*, congestion rounds, bounded placement search, experimental planar embedding/straight-line construction and flat-candidate compaction, editor drag preview and background route repair.
- Partial: local repair selects only some conflicts; channel placement trials and planar compaction still perform expensive complete reroutes. The flat candidate checks port rotation but does not construct orthogonal routes directly from embedding faces.
- Missing: spatially bounded repair transactions, shared corridor track ordering, actionable space requests, targeted corridor widening, and recursive planar candidates with coordinated boundary contracts.
- Parallel candidate evaluation and stale-result checks already exist in runtime. Supplying a stop callback currently selects serial evaluation. Fine-grained cancellation, resumable work and safely coordinated parallel local transactions are not complete.
- No new speedup is asserted by this plan. Record a reproducible baseline before implementation; retain the current hierarchical candidate and keep the planar candidate experimental.

### 13.2 Ordered delivery slices

| Step | Packages | Deliverable | Depends on | Exit evidence |
|---|---|---|---|---|
| P0 | A0, A5, A6 | Freeze corpus/profile baselines; execution identities, work units and stage counters; cancellable job shell | Existing implementation | Native/WASM timings separated; stale and cancelled jobs cannot overwrite edits; deterministic single-worker reference |
| P1 | A0, A1, A2 | Complete conflict selection: crossings, repeated crossings, hugging, near-parallel spacing and sharing | P0 integration, independently prepared fixtures/classifier | Crossing-only routes selected; current metric/key unchanged; experimental strategy compared against frozen baseline; complete incumbent survives interruption |
| P2 | A2, A5, A6 | Spatial index and bounded local repair transaction; reuse unchanged geometry and prepared obstacles | Index prototype independent; repair integration after P1 | One-leaf edit reroutes incident and spatially affected edges without mandatory full rerouting; unchanged exterior preserved; measured discovery/search/validation cost |
| P3 | A2, A3 | Joint corridor track order and automatic port-slot order; small conflict-group repair | P2 | Repeated-crossing fixtures improve where feasible; separate tracks remain separate edges; explicit ports and fixed orders preserved |
| P4 | A2, A4a | Diagnosed space requests and local corridor widening/node displacement | P2, P3 | Narrow-channel fixtures repaired by bounded movement; fixed-grid and free placement covered; pins/containment preserved |
| P5 | A4b | Embedding-guided corridor/face routing and box-aware compaction | P2–P4 | Flat planar corpus compared with baseline; complete routes respect constraints; no reliance on rotation checks alone as proof of crossing freedom |
| P6 | A4b, A4a, A6 | One-container boundary contract, then recursive mixed-container layout | P4, P5 | Inner/outer port-order reconciliation, bounded ancestor growth, cancellation and invalidation fixtures |
| P7 | A5, A6 | Expand parallel scheduling, persistent reuse and size ladder using measured bottlenecks | P2 onward, each capability independently | Quality/resource comparisons across worker counts, bounded memory, edit-to-result latency and cancellation overrun |

P7 is not permission to defer execution design: identities, isolated candidate state, checkpoints and resource accounting are P0 requirements. Enable concurrency incrementally once each job type has a correct serial implementation. P5/P6 must not block the general-purpose routing repairs P1–P4.

### 13.2-status: state after P0 (2026-10-22, with evidence)

- **P0, merged (PR #25):** identities, counters, parallel stop-aware candidates, checkpoints down to routing, ordering, flat-region and placement; progressive publication and host connection (browser). Open: measurements on an idle machine (the sweep ran at load 3 to 4, recorded as not idle); per-candidate limits are removed from the plan by decision. Evidence: `benchmarks/results/20261022T160324Z.jsonl`, `p0-browser-latency-breakdown-20261022.jsonl`.
- **P1, experimental (branch `worktree-p1-conflict-selection`, default off):** `extendedConflictSelection` adds a candidate that re-routes crossing and hugging routes with a crossing cost. Measured on seven datasets, not idle: only `state-order` changes (routed crossings 3 to 2, candidate chosen); the other six keep their result. Still open: a fixture-driven comparison on crossing-only, repeated-crossing and tight-spacing cases; the spacing criterion (near-parallel tracks) is not implemented.
- **P2, partly:** affected set, subset routing around fixed routes, escalation with bounded rounds, explicit fallback. Open: spatial index (not built; first measure the grid build, affected discovery, routing and validation separately), and the cost split.
- **Orthogonal fallback (channel):** an unroutable edge is drawn with one orthogonal corner instead of a diagonal. Its node passes are reported in `routingIssues` (105 on `workflow-dag`, equal to the hits). The cause is container traversal: `workflow-dag` has five stage containers without boundary transitions, which is P6.
- **P3, P4, P5, P6, P7:** not started. P5's compaction work is limited to node-level embedding-aware moves; box sizes are already considered.

### 13.2a P0/P1 implementation decisions

**Job identity (implemented).** Provisional internal types in `crates/runtime/src/execution.rs`: `InputIdentity`, `CandidateId`, `WorkCounters` and `JobReport`. Snapshot `revision` and `layout_version` are reused as `InputIdentity.graph_revision` and `base_layout_version`; no competing counters are added to the snapshot. `refine_snapshot_job` takes a host-supplied opaque `job_id` and returns it in `JobReport` together with the input identity, the completed candidate IDs and the work counters. Job IDs identify invocations, not geometry, and never affect scoring or geometry fingerprints. No `JobContext` or `ExecutionLimits` type exists; per-candidate caps were removed from the plan (see the measured cost below).

`InputIdentity` additionally identifies the projected graph/collapse state, effective layout configuration/objective policy, and geometry/constraints including measured labels. In P0 retain canonical serialized values for exact equality (stable map order, explicit schema version); do not use a process-random hash or preset ID alone. A later compact digest requires a documented deterministic encoding. Core code receives identities/data without host I/O. Rendering-only preferences are excluded unless they change layout input.

`CandidateId` is an ordered structured value: `(strategy, parameter_key, occurrence, region_key)`. Strategy names are fixed (`ordering`, `routing`, `routing-last`, `search`, `flat-region`, `other-port-placement`); parameter keys use stable numeric/enum values, occurrence is the position among identical entries in the configured candidate list, and region keys use sorted stable node IDs (empty for whole-view candidates). Assign IDs before dispatch, never from thread IDs, completion times or a global concurrent counter. Preserve the existing configured candidate reduction order in P0 so equal-quality alternatives do not change output merely because IDs were introduced.

**Work accounting.** There is no single interchangeable unit for an A* expansion and a full placement attempt. Preserve the existing public `workBudget` meaning (maximum dispatched top-level candidates) in P0. Add typed counters: dispatched/completed candidates, routing rounds, placement trials, A* expansions, prepared grid elements, and exact geometry tests. One A* expansion means one non-stale state popped and expanded; stale heap pops are recorded separately if profiled. A placement trial is counted when evaluated, including rejection. Count actual completed operations, not reserved capacity.

Status: implemented counters are dispatched/completed candidates, routing rounds, A* expansions (routing-only, last-round and global-round jobs; other job kinds add zero) and placement trials (evaluated, rejected ones included). Cancellation checkpoints (implemented): the channel router checks the stop signal before every edge and every 256 A* expansions. An interrupted routing search returns `Stopped`, and the router never falls back to direct stubs for it; the candidate is discarded, and so is any candidate that finishes while the stop signal is set. A stopped global round ends the rounds, keeping the last complete candidate. Prepared grid elements and exact geometry tests are open. Ordering candidates checkpoint too: the layout passes the stop signal into placement and the legacy router (`routing::improve_until` checks before every route and between passes), and `candidate_until` returns the channel-router work. Flat-region compaction (per trial), the nested refinement of the alternative port placement, and the placement search's local re-routes (per routing pass) checkpoint too. Still without checkpoints: the source recompute of the alternative port placement, `repair_with` and `spread_tracks` (short passes), and the legacy local re-route path. Interruption latency: `bench --cancel-after-ms` fires the stop signal after a delay and records the overrun (first `true` answer to return) and whether every drawn edge keeps a route. Profiling (`sample` on a running bench) showed that the large overruns came from the legacy router (`improve_pass`, about half of the candidate time) and the metric measurements, not from A* (the default engine is `Legacy`). After the legacy checkpoints, workflow-dag overruns are about 6 ms at every delay (earlier up to 59 ms at 80 ms), and org-chart overruns are 4 ms or less. All probes returned complete routes. The remaining ~6 ms on workflow-dag is fixed post-stop work (input measurement and quality of the baseline), present even when the signal fires before the first candidate. Results: `benchmarks/results/p0-cancel-probe-after-checkpoints-20261022.jsonl` (load about 4 to 6, not idle). Native and WASM refinement medians, same seven datasets, one worker: `benchmarks/results/p0-native-vs-wasm-nonidle-20261022.jsonl` and `...wasm.jsonl` (`benchmarks/wasm_refine.mjs`); WASM took 1.5 to 2.0 times the native time (load about 4, not idle). Quiet-machine runs (load 1.2 to 2.3 on 8 CPUs, checked before and after): `benchmarks/results/p0-idle-native-20261022.jsonl` (seven datasets, seven repetitions; `uml-class` and `er-diagram` re-run with fifteen, medians 12.7 and 12.6 ms after an unstable first pass), `benchmarks/results/p0-idle-wasm-20261022.jsonl`, and `benchmarks/results/p0-idle-cancel-20261022.jsonl`. The cancellation medians on workflow-dag stay at about 5.7 ms; one probe at a 20 ms delay overran by 38 ms, which is not explained yet. The full `benchmarks/run.sh` sweep has now been run (`benchmarks/results/20261022T160324Z.jsonl`, 20 configurations). Its load stayed at 2.9 to 3.7 (one minute) and 5.2 to 5.4 (five minutes) on 8 CPUs, so it is recorded as not idle; the timings are reference values for the same configuration, not a speed claim. Geometry fingerprints equal the stored fingerprints for every dataset except `layerweave-code` (its source changed, so its input changed) and a metadata-only difference on `petri-net` (strategy and candidate count equal, geometry equal; the origin of this difference is not yet traced against the unchanged baseline, as was done for `uml-class`). Baseline records: `benchmarks/results/p0-baseline-nonidle-20261022.jsonl` holds the frozen-corpus fingerprints for seven datasets (three repetitions, threads 1 and 4). Its machine load was about 9 on 8 CPUs at the start, which violates the idle-machine rule, so its timings are not evidence; the fingerprints are. A valid timing baseline needs `benchmarks/run.sh` on an idle machine. Measured cost (`bench --preset presets/lesbar.json`, `benchmarks/results/p0-lesbar-work-20261022.jsonl`, deterministic counts; machine load 2 to 10, so times are not idle evidence): the mean A* search needs 80 to 3 500 expansions and the largest single search 138 000 on org-chart, so a per-search cap would cut little. The cost is the number of searches: a channel-engine placement trial re-routes every edge (`local_candidate` calls `assemble_with`), giving 206 640 searches on workflow-dag (58 s single refinement) and 68 990 on org-chart (26 s). The lever is local re-routing of placement trials (P2), not a limit.

P2 step 1 (affected set, implemented): `affected_edges` gives one definition shared by the local transactions: every edge incident to a moved node, plus every route that touches a moved node's old or new box. The legacy local path uses it unchanged. Measured on the channel engine (one leaf moved by 40 units, up to twelve leaves per dataset, `affected_edges` against the visible edges): median fraction 0.000 (uml-class, er-diagram), 0.024 (org-chart), 0.048 (workflow-dag), 0.125 (cloud-architecture), 0.211 (state-tcp); maximum 0.146 on org-chart, 0.112 on workflow-dag. P2 step 2 (implemented, channel engine): `route_active` routes only the affected edges around fixed routes. Fixed routes keep their geometry; their corners become grid lines, and their segments and points occupy the grid (congestion and crossing costs), but they are not obstacles. Round 0 charges shared segments when fixed routes exist. Non-axis-aligned routes are direct fallbacks (explicit conflicts) and are re-routed rather than kept. Edges whose ports move are included. The composed result is validated and scored with the same key; an invalid or refused composition falls back to a complete re-route, explicitly. Measured (one leaf moved by 12 units, eight leaves per dataset, release, load not idle): median local time 0.5 ms against 2.5 ms (state-tcp), 2.3 against 14.6 ms (cloud-architecture), 24 against 109 ms (org-chart), 1.2 against 4.5 ms (uml-class), 1.4 against 2.7 ms (er-diagram), 0.3 against 0.5 ms (state-order), 9.7 against 243 ms (workflow-dag). Published key against the complete re-route: better, equal and worse counts per dataset are 2/5/1 (state-tcp), 2/4/2 (cloud-architecture), 5/0/3 (org-chart), 0/8/0 (uml-class), 0/8/0 (er-diagram), 1/7/0 (state-order), 3/0/5 (workflow-dag). The subset is therefore faster, but it is not never worse: a worse subset candidate loses in the placement search, so no published result regresses, but the search loses some alternatives. Correction: the speedups of the local transaction are 1.8 to 25 times (state-order 1.85, er-diagram 1.9, org-chart 4.5, uml-class 3.9, state-tcp 4.9, cloud-architecture 6.5, workflow-dag 25); the commit message of that step said 3 to 400, which was wrong. These are local transaction timings, not whole-refinement speedups. Escalation after a failed local composition (add conflicting edges) and the rebuild of the grid per call (still global, not yet measured separately) remain open. The triggers for escalation are remaining conflicts or a failed local repair, not a comparison with a complete re-route, which would cost the same time the local path saves.

Progressive publication (implemented): `refine_snapshot_progressive` publishes each accepted improvement over the input as soon as it is selected, as a `Publication` with its sequence, strategy, key and input identity. Candidates run in batches of `threads` and are reduced in configured order, so the result is identical for every batch size (default fingerprints unchanged). Each publication's key is below the input's, keys strictly decrease, and the last publication is the result (test `published_improvements_strictly_decrease_and_end_at_the_result`). Timeline (`bench --progress 1`, load 14 to 15, not idle): the first publication of org-chart arrives at 34 ms of a 97 ms refinement, and workflow-dag at 39 ms of 101 ms. Host connection (implemented at the boundary, verified in Node, not in a browser): each publication carries the job ID and its input identity. `dispatch_streaming` (operation `refineProgressive`) hands every publication to a callback as JSON while the refinement runs. The WASM module imports `lw_publish` from the host and calls it for each publication before `lw_run` returns; `createEngine` in `packages/engine-client` provides the import and an `onPublish` callback (a call without it still works). `packages/engine-client/wasm-progressive.test.mjs` checks that publications arrive inside the call, in sequence, with decreasing keys, and that the last one is the result. The policy for the host is in `runtime::session` (pure Rust, no I/O): it keeps the base of the active job separate from the shown publication, accepts only the active job's publications with its base and an increasing sequence, drops duplicates and stale results, keeps only the newest pending publication (latest wins), hands over an unshown result on completion, reports a completion without improvement, and on failure drops pending work and keeps the last shown state. A user edit cancels the active job; a job cannot start from an outdated base. Tests cover the cases required for the host: an edit between publications, a late or duplicate publication, several valid intermediate states, completion without improvement, and failure after a shown state. Browser connection (implemented, measured in headless Chrome 155 with `apps/playground/browser-checks/progressive.mjs`, load 5 to 7 on 8 cores, not idle): the playground's background worker forwards each publication while its request runs; the page shows the first one at once and merges later ones with a 120 ms leading and trailing throttle, so the newest result wins. The job's base is kept apart from the displayed snapshot: each publication is checked against the job's base (`acceptPublication` in `revisions.js`, with unit tests), and the final result is checked the same way, so a shown intermediate layout does not make the next one stale. Measured from the click to the displayed text (workflow-dag, three runs: first improvement 93 to 100 ms, final 302 to 309 ms; org-chart, two runs: first 78 ms, final 277 to 279 ms). In WASM the candidates of a batch run one after another, so the batch size is 1 there: the first result arrives after one candidate instead of four (measured before that change: 172 to 185 ms first). Observation: the shared-line key comes first, so on workflow-dag the adopted layout removes 888 shared pairs but raises crossing pairs from 41 to 624; this is the configured objective order at work, not a regression of the key. Closed in the following step: the page keeps the job lifecycle in `apps/playground/session.js`, a port of `runtime::session` with the same tests (`session.test.mjs`, seven cases from `session.rs`); `refine()` starts, offers, shows, finishes and fails jobs through it, and cancelling refinement cancels the job. The page's display limit is the 120 ms throttle, which is the only frame-rate control. Latency is split per shown publication (`window.lwRefineTimeline`, read by `progressive.mjs`): computation from request to emission in the worker, transfer to the page, and display (drawing). Measured in headless Chrome 155 (`benchmarks/results/p0-browser-latency-breakdown-20261022.jsonl`, six runs, load 5 to 6): workflow-dag first result 85 to 95 ms (computation 85 to 94 ms, transfer 0 to 1 ms, display 18 to 19 ms), final 294 to 305 ms; org-chart first 71 to 77 ms, final 276 to 297 ms. Payload: each publication is a full snapshot of 90,694 to 119,875 characters, so transfer is cheap at this size but scales with the view; a delta encoding is not implemented. Escalation is implemented (below). Escalation (implemented, bounded): when the composed local result has a hard geometry problem, the fixed routes whose corridors run through the bounding box of a re-routed edge are released, and the set is routed again, at most `ESCALATION_ROUNDS` (2) times. Without a hard problem nothing is released, so the search does not spend work on satisfied placements. The first version released fixed routes that run through the moved node's new box; that set is already in the affected set, so it never released anything, and it was replaced. Measured for single-leaf moves on the seven datasets: none needed a complete re-route. Test `channel_local_move_onto_another_node_is_refused_not_published`: a placement that overlaps another node is refused, so the caller falls back explicitly. Recovery after a failed local transaction: the only consumer of the local transaction is the placement search, which discards an unusable trial and keeps the incumbent; no user operation runs the local path yet, so there is no user-visible repair to recover. When a user operation uses it, a failed local composition must be re-run in the background and its old geometry kept until then; this is the remaining design point, not an open defect of the current code. Per-candidate caps (`ExecutionLimits`) are removed from the plan. The existing `placementSearch` and `workBudget` budgets cap placement trials and dispatched candidates. Cancellation is enforced by checkpoints: before and after each candidate, before each edge, every 256 A* heap pops, between routing passes, and between placement trials and flat-region trials. These are batch limits, not millisecond guarantees. P0 must list remaining uninstrumented operations and measure their cancellation overrun; full resumable chunking is an incremental A5 delivery, not a claim made by adding counters.

**Parallel cancellation.** P0 removes the automatic serial fallback for native independent candidate jobs: use the existing bounded scoped-thread mechanism, isolated candidate state and a thread-safe stop signal/callback; do not add an async/thread-pool dependency. Reduce results in the existing fixed order. Test no-stop and never-stop callbacks across thread counts for identical geometry. Time cancellation can finish different candidate subsets; retain the incumbent and report interruption and completed IDs. WASM remains serial within a worker until cooperative resumable chunks are implemented; worker termination/restart is an explicit host fallback, not fine-grained cancellation.

**Measurement protocol.** Store a frozen input/preset manifest and raw records under `benchmarks/results/`. Each record includes hardware/CPU/RAM, OS, compiler/tool versions, target, release/debug build and flags, Git revision plus dirty-diff identity, input/view hash, full effective options and preset/objective/metric versions, seed (or explicit deterministic/no-randomness), worker count, budget kind/values, warm/cold mode, repetition and exact invocation. Record stage times, work counters, peak-memory method or unavailable, quality vector and geometry fingerprint. Use at least three measured repetitions on the same otherwise idle machine for paired comparisons; preserve every sample and report median/range. Separate native/WASM and compilation/startup/transfer costs. No speedup claim from an unrecorded single run.

**P1 metric decision.** Keep the current metric version and `published_key` unchanged. Shared-endpoint crossing points already contribute to `Objective::RoutedCrossings`; do not add them again. Repeated-crossing information is derived diagnostic/search-selection data, not a new acceptance-key component. P1 widens the conflict set eligible for repair; it does not change geometric definitions. Any later new scoring dimension or changed definition requires a separate versioned migration and snapshot remeasurement.

An unchanged key does not imply unchanged geometry: additional candidates can win. Therefore expose P1 initially through an explicit experimental routing-strategy option, defaulting to the existing strategy and omitted when default. Keep default frozen geometry fingerprints unchanged. Record experimental fingerprints and per-input quality/time differences separately; do not overwrite baseline fingerprints to make checks pass. Promotion to a default is a separate reviewed strategy/profile-version change. P0 infrastructure changes must preserve default output geometry; volatile execution metadata lives outside geometry fingerprints.

### 13.2b Development concurrency and integration order

Development concurrency is distinct from runtime concurrency. Independent preparation can proceed together: (A) frozen corpus/measurement records, (B) runtime execution shell, (C) conflict classification without scoring changes, (D) pure spatial-index prototype, and (F) diagnostic fixtures. Agree on geometry/conflict interfaces before C/D integration. A topology/face-routing spike (E) is optional and must not delay P0–P2; it needs a routing-facing contract and is not assumed to fit entirely inside the dependency-free topology crate.

Integrate P1 after the execution shell and classifier are ready; integrate the P2 repair transaction after P1 and the index module. P3/P4 may be developed on agreed interfaces, but their combined geometry/occupancy changes must be evaluated atomically. Section 15 restricts unsafe concurrent execution against shared mutable occupancy, not parallel development of separate modules. This document does not itself dispatch development agents.

### 13.3 Local search and spatial indexing (P2)

Use an index over obstacle/label envelopes and route-segment bounds to discover affected geometry and query candidate segments. Exact geometry predicates follow broad-phase queries. Compare an R-tree with simpler axis-specific interval indexes for orthogonal track occupancy; choose by measurements rather than assuming one structure suits all queries.

An index accelerates queries; it does not by itself reduce a Cartesian routing grid. Build a bounded local search region and a sparse corridor/visibility representation, then run detailed A* within selected corridors. Grow the region on failure up to the owning container and an explicit budget. Do not interpret restricted-search failure as infeasibility. Keep a safe fallback with explicit diagnostics.

Reuse immutable prepared obstacles within a transaction. Invalidate by graph, geometry, labels, constraints and configuration dependencies. Discover affected routes using both old and new obstacle envelopes including clearance, not incident edges alone. Initially validate and score the complete combined snapshot; incremental metric caches come only after cross-checking against full evaluation.

### 13.4 Repeated crossings and track allocation (P1/P3)

Count both crossing points and crossing edge pairs, including shared-endpoint pairs. Record repeated-crossing events per pair separately; crossing count changes require explicit metric/objective versioning where semantics change. Later local rounds must not filter solely on shared segment occupancy.

For routes sharing a corridor, coordinate their relative track order across entry, bends and exit, and coordinate automatic ports with that order. Detect order inversions and reroute a bounded group together. Try local uncrossing only when obstacles, fixed ports and boundary transitions permit it; removing two crossings is not universally feasible. Track coordination must not silently create a bundle or swap logical endpoint identities.

### 13.5 Space requests and recursive boundaries (P4/P6)

A space request identifies region, axis, estimated extra width, affected edges, blockers and evidence of a bottleneck. First test whether port/track reordering resolves the conflict. Capacity estimates are not feasibility proofs. The placement stage may open a grid gap, move free nodes, grow a node for slots, or enlarge a container; the router never moves nodes implicitly.

Start with one container whose external attachments are fixed. Then introduce recursive negotiation: children report sizes and admissible boundary orders, parents allocate space, and affected children reconcile interfaces. Bound the number of upward/downward passes and retain the best complete candidate. Sibling interiors may execute concurrently only under fixed interface contracts; a changed boundary order or container size invalidates dependent exterior work. Mixed-layout containers remain opaque boxes outside their own delegated layout.

### 13.6 Quality and performance gates

Do not optimize total edge length in isolation. Report hard validity, sharing, hugging, crossing points/pairs/repeated crossings, bends, route detour, area/aspect ratio, minimum spacing and movement of unaffected elements. Orthogonal detour uses actual endpoint Manhattan distance with an explicit zero-distance policy; report median and tail values. It measures routing efficiency for an arrangement, not arrangement quality: widely separated nodes can still have a low detour ratio. Keep total length as a diagnostic/tie-breaker alongside placement measures.

Keep the current published key as the comparison baseline. Any profile-specific tradeoff policy, movement/area cap or objective-order change must be separately versioned and evaluated, not slipped into an optimization patch. Search may traverse worse states; publication compares complete valid candidates under one policy. An explicit user move establishes new constraints and a new repair baseline; an older better score must not undo the requested move.

For each slice, use diagnostic fixtures plus the frozen profile corpus and at least three timing repetitions. Record stage durations, A* expansions, grid/index size, affected edges, complete reroute count, memory, first usable result, edit-to-repair latency and cancellation overrun. Compare serial and concurrent execution at matching deterministic work budgets; report wall-clock-budget runs separately. Show selected results side by side, including quality regressions and unresolved cases.

## 14. Existing implementations and references

These sources establish precedents, not an automatic recommendation to copy code. Record license, revision and notices before any reuse.

- [OGDF layout examples](https://ogdf.github.io/doc/ogdf/ex-layout.html): orthogonal layout via `PlanarizationLayout`, crossing minimization via `SubgraphPlanarizer`, and orthogonal geometry via `OrthoLayout`. This is a direct architectural reference for A4b.
- [yFiles OrthogonalLayout](https://docs.yworks.com/yfiles-html/api/OrthogonalLayout/): documents topology–shape–metrics phases: planarization, orthogonalization and compaction. Its documentation also lists restrictions on combinations of directed layout and nested groups; do not infer unrestricted feature combinations for Arqlet.
- [ELK Layered overview](https://eclipse.dev/elk/blog/posts/2025/25-08-21-layered.html): cycle breaking, layer assignment, crossing minimization, node placement and routing, including SCC-based cycle-breaking strategies. This uses structural analysis but is not the same as a planar-embedding-based free-placement pipeline.
- [Boost.Graph planar algorithms](https://www.boost.org/doc/libs/latest/libs/graph/doc/html/graph/algorithms/planar/planar_graphs.html): planarity testing and embeddings provide incident-edge cyclic order. This is an analysis building block, not a complete port-aware nested diagram layouter.

Sources checked on 2026-10-21. No external implementation was copied as part of this documentation update.

## 15. Asynchronous and parallel execution contract

This is a cross-cutting requirement for P0–P7, not a later replacement of the core algorithms. Asynchronous scheduling keeps the host responsive; parallelism executes independent work concurrently. Neither makes an unbounded synchronous search interruptible by itself.

### 15.1 Job model and ownership

The host schedules engine work. Core crates remain independent of browser, Node and transport APIs. Use immutable input snapshots plus isolated candidate state. Each job carries a job ID, graph revision, base layout version, view/collapse identity, layout-config/objective version, constraint/geometry identity, region scope and deterministic candidate ID. These are semantic requirements; reuse existing fields instead of duplicating the schema unnecessarily.

A job returns progress or a candidate transaction containing changed boxes/ports/routes/labels, its dependency identities, diagnostics and termination state. A single coordinator owns publication. It rejects stale dependencies, combines a local patch with its matching baseline, finalizes/validates/scores, and commits atomically. Progress events are not partially committed geometry. Throttle/coalesce progress and bound queued results.

Initially serialize commits and reject/recompute stale local patches. Do not blindly merge patches merely because moved node IDs differ: routes, clearance envelopes, labels, boundary ports and ancestor sizes can overlap. Future rebasing requires explicit read/write dependencies and revalidation against the current snapshot.

### 15.2 Scheduling and safe parallel units

Prioritize current interactive repair over speculative refinement. Coalesce superseded edits, cancel obsolete work and cap concurrency, queued jobs and total candidate memory. A newer job must not wait behind an unbounded queue of old candidates. Admission control must account for large route grids, not just thread count.

Safe initial parallel units are independent complete candidates from one baseline and sibling interiors with fixed, independent boundary contracts. Independent conflict regions may follow after dependency detection exists. Within a congestion round, routes share occupancy and order-dependent costs: keep this sequential initially. Parallel alternatives must have isolated occupancy and be compared as complete candidates. Shared mutable occupancy across unsynchronized A* searches is not allowed.

Use stable candidate IDs and deterministic reduction of completed candidates. In deterministic work-budget mode, select the same winner for the same completed candidate set regardless of completion order; resolve equal alternatives by stable IDs while preserving an equal incumbent. Tests requiring identical final outputs must also use the same candidate set and per-job budgets. Time-budget mode may complete different subsets and must report that fact. Parallelism must not multiply a declared total work budget silently.

### 15.3 Checkpoints, cancellation and host adapters

Make expensive loops bounded: A* expansions, grid/index preparation, congestion rounds, placement/compaction trials, validation and topology stages need checkpoints or explicit bounded work chunks. A step returns running/progress, a complete candidate, or termination. Preserve the last complete checked result across cancellation; interrupted private state is never published as valid.

For browser/WASM, begin with a worker and cooperative resumable chunks that return control to the worker event loop. A queued cancel message cannot interrupt synchronous WASM. A worker pool or shared-memory cancellation is a separate optional host capability, not a mandatory deployment requirement. Worker termination is a fallback that discards that worker's private state; the coordinator must retain the last complete result. Measure snapshot transfer/serialization and copying costs.

Native and Node adapters use bounded worker pools and cancellation tokens checked by the same core checkpoints. Budget clock access is supplied by the host. Do not block the UI/Node event loop with layout execution. Backend and frontend exchange versioned snapshots/patches and the same dependency identities; backend results receive no privileged right to overwrite newer local edits.

Initially restart refinement from the best compatible snapshot. Persisting an A* frontier, embedding search or occupancy state requires a separately versioned resume format with dependency invalidation; it is not implied by snapshot handoff. Completion may retain the existing absent-stop-reason convention; cancellation, work/time exhaustion and definitive infeasibility must not be conflated.

### 15.4 Required concurrency acceptance cases

- A second drag, config change or collapse arrives while old work runs: old results never overwrite the new state.
- Cancellation occurs during preparation, A*, compaction and final validation: return or retain a complete result, and record measured overrun.
- Two local repairs overlap through an unchanged route, label or ancestor: serialize/reject/recompute rather than merge unsafely.
- Independent sibling jobs finish in reverse order: fixed-work final selection is stable and boundary contracts remain valid.
- Worker failure/backend disconnect: retain the current scene, release reservations and allow a bounded restart.
- Single-worker fallback and multi-worker execution use identical validity/objective semantics; transferred results are checked before publication.
- Interactive work preempts speculative work at checkpoints; queue/memory limits hold under repeated edits.
