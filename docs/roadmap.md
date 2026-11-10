# Roadmap

## 1. Contracts and handoff

Define stable IDs, edits, measurements, snapshot and patch schemas. Create a small
nested fixture with ports, a cycle, a self-loop and parallel edges. Demonstrate
backend snapshot -> browser edit -> local repair -> backend refinement. Make a
second edit while refinement is pending; stale results must not overwrite it.

## 2. Layout quality

Evaluate reusable hierarchical phases; implement container boundary coordination,
ports and orthogonal routing. Add constraint diagnostics and stable placement.

## 3. Scale and exploration

Compact long-edge representation, aggregation, drill-down, cut-out, cache
invalidation and adaptive region expansion. Benchmark real graphs through 100K+.

## 4. Additional geometry

Grid placement, rounded routes, explicit bundling, straight routes and free curves.

## 5. Score-driven recursive optimization (proposed)

Specified in [recursive-optimization.md](recursive-optimization.md) §13, with the decision framing in [ADR 0002](adr/0002-quality-model.md). Status as of `main` after PR #40:

- Done: M0 shadow evaluator (tiers, `layout::improves` as the single acceptance rule, candidate log, calibration split); M1 slice (capability declarations, bounded archive, budget ordering, legacy work counters, publication v2 with `score`); a required clearance (`requiredClearance`, hard, with placement and routing support, unmet reported in `routingIssues`); the default objective order (crossings before near-node pairs, ADR 0002).
- Open: M0 closure (ADR 0002 acceptance, one evaluator, calibration on the fixed split); M1 score-driven selection (needs an ADR, changes published output); M2 to M6 not started, with the first M2 slice (structure analysis and component packing) planned in `docs/recursive-optimization.md` §12–13.
- The lexicographic key remains the selection rule until a documented decision changes it.

- M0 Evaluation contract: shadow scoring, one evaluator, fixed quantisation.
- M1 Portfolio coordinator: capability declarations, bounded archive, deterministic batch-boundary allocation. First slice done with the current selection: declarations drive the round jobs, and the archive is built and tested. The first-stage allocation is one deterministic function (first pass: planned order under the job budget). A cost- or progress-aware policy waits for measured job costs. The archive is tested under a mid-run stop. Publication fields are still open.
- M2 Independent regions with fixed boundaries and bounded parallel jobs.
- M3 Negotiated interfaces, space requests and transactional local repair.
- M4 Tree/radial-tree and organic strategies, gated by fixtures and measurements.
- M5 Port assignment, integrated labels, curved paths and explicit buses, delivered separately.
- M6 Scale and resume: sparse/local search, multilevel regions, reusable analysis.

Performance work and algorithm replacements per layout family are planned step by step in
[layout-improvement-plan.md](layout-improvement-plan.md).

## Open decisions (not blocking the scaffold)

- Final project name and package availability.
- Decided: license MIT OR Apache-2.0, copyright Benjamin Becker, contributions under the DCO (CONTRIBUTING.md).
- Real sample graphs: edge density, nesting depth and cross-container traffic.
- Desired local-response budget and acceptable movement; measure before promises.
- Browser support, worker packaging, renderer and snapshot encoding.
- Backend execution environment and transport.
- Upstream algorithm selection after source audit and benchmarks.

## Implemented prototype (2026-10-20)

Read-only exporter; graph validation; omission of unresolved edges; bounded
connected-view selection with ancestors; deterministic nested layered placement;
simple orthogonal routes; native snapshot CLI; browser worker WASM continuation;
revision-checked add-node operation; native/WASM parity smoke test.

First real view: 47 nodes, 63 resolved edges. This does not establish full-graph
performance. Automated browser visual verification was blocked by unavailable
browser security-policy verification; native/WASM execution was tested.

Next: explicit ports and boundary routes, incremental routing, constraint-aware
placement, backend refinement protocol, claim overlay importer.

## Background refinement implemented

Loopback HTTP service invokes native Rust for a bounded full-view ordering pass.
Graph revision and layout version are distinct. Automatic debounced refinement
after local edits can be disabled or requested manually. Pending edits, reset
epochs, revisions and layout versions guard result adoption. Native jobs are
time/size/concurrency bounded; HTTP abort does not yet interrupt native jobs.

Acceptance currently minimizes total route length only. Explicit ports, obstacle
avoidance, movement-aware quality, true patch transport and claim overlays remain
next work. Five Rust tests, five result-adoption tests, three HTTP integration
tests and native/WASM parity passed locally. Visual automation remains blocked
by browser policy verification.

## Ordering and placement candidate search

The optimizer now compares the current view with four deterministic full-view
variants: barycenter or median ordering, forward or alternating sweeps, and
optional centered columns. A lexicographic quality policy prioritizes sibling
overlap and straight-projection crossings, then weights route length, movement
and area. Rank reassignment and actual routed crossing/obstacle detection remain
future work. Pairwise quality evaluation is limited to 1000 nodes/2000 edges.

On the initial Intentweave view, projected crossings changed from 136 to 91,
route length from 53552 to 51248, with zero sibling overlaps. These are prototype
quality measurements, not a full-graph performance claim.

## Routed quality and detours

Added strict interior perpendicular crossing-pair counts, shared-segment pair
counts, and foreign-node penetrations. Endpoint ancestors are excluded from
obstacle checks. Backend candidates use bounded horizontal detours and include
a routing-only alternative. No controlled bundling or collision-free guarantee.

Initial view measurement: foreign-node penetrations 195 -> 11; crossing pairs
138 -> 23; shared-segment pairs 841 -> 891; route length 53552 -> 43070. The
increase in shared segments is an explicit remaining tradeoff. Ten Rust tests,
five result-adoption tests, three HTTP tests and native/WASM parity passed.

## Node geometry validator implemented

The scene module now validates unrelated rectangles across containment branches,
minimum gap, container padding/header, finite positive dimensions and coordinate
overflow. Diagnostics identify affected nodes. Backend candidate selection treats
these constraints as mandatory and repairs geometrically invalid input when a
valid candidate is available. Test coverage includes cross-branch overlaps,
intentional ancestor containment, exact clearance, invalid dimensions, header
intrusion and stale/empty input diagnostics.

Next: explicit port model and container-boundary routing, then a more complete
corridor search. Existing bounded detours do not guarantee obstacle-free edges.

## Endpoint ports implemented

Added optional graph ports with side and relative offset, edge-to-port bindings,
validation and deterministic automatic endpoints. The router uses these endpoints
and preserves their departure/arrival directions through detours. Scene output
includes resolved ports and derived per-container crossings. The demo displays
both. Tests verify explicit endpoint geometry, invalid references and a route
leaving one container and entering another.

Fixed boundary portals are not implemented yet: existing crossing markers are
derived, not constraints. Next: mandatory boundary waypoints and segment-wise
routing, then a corridor search with explicit failure diagnostics.

## Aggregation and drill-down foundation

Implemented canonical/view separation with collapse-state projection, visible
external endpoints and lossless reopening. Browser controls operate through WASM;
backend refinement retains view state. Tests cover close/refine/reopen, external
edge identity, edits while closed and native/WASM view parity. Current expansion
recomputes the selected view; coordinate caching and fixed boundary portals remain
next work. The retained source graph is the selected excerpt, not the full index.

## Direction policies and UI regressions

Added default and inherited per-container LR/TB/hybrid policies, removable manual
overrides, and matching automatic ports. Hybrid selects locally by compactness;
global direction optimization remains future work. Added initial automatic
refinement scheduling, explicit container selection, fitting after view changes,
and uncached demo assets. Actual WASM runs in a simulated DOM regression covering
initial refinement, collapse/reopen and direction changes.

## Geometry-aware automatic ports

Automatic endpoints now face their neighbor using absolute node bounds, with
layout direction as a tie-breaker. Reverse vertical edges arrive at bottom ports;
reverse horizontal edges arrive at right ports. Incoming and outgoing endpoints
share per-side spacing. Initial routing supports both forward and reverse
doglegs. Regression tests cover all four directions under LR/TB/hybrid, endpoint
rectangle avoidance and approach direction before/after refinement. Explicit
ports remain authoritative; this is not a global joint port/routing search.
