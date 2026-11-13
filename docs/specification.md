# Arqlet – Product Specification

Status: 20.10.2026. Name: Arqlet (formerly the working name Layerweave). Repository: Intentweave organization.
Project license: MIT OR Apache-2.0.

This document collects the requirements gathered so far. It describes the target product;
a requirement is not automatically implemented. Architecture details are in
[architecture.md](architecture.md), and the development history is in [roadmap.md](roadmap.md).
New decisions should be recorded here. Concrete performance budgets are still to be measured
and set.

## 1. Goal and scope

A hierarchical layout engine in Rust for graphs of 2 to over 100,000 nodes. Nesting,
interactive changes, and the handoff between backend precomputation and frontend computation
are core requirements. A quick, usable intermediate result takes priority over a long-running
complete optimization. The view then improves step by step.

The first use case is a code graph from Intentweave. Later, knowledge-graph information, or
initially Intentweave claims, will be overlaid. The graph model must therefore not be bound to
code graphs. The presentation of PR Lens serves as a design reference, not as a binding
technical dependency.

## 2. Graph and data import

- Stable identities for nodes, edges, containers and ports.
- Arbitrarily nested containers; the containment relation stays acyclic.
- Logical edges may contain cycles, back edges, self-loops, parallel edges, and different
  semantic kinds or special edges.
- The logical edge direction is preserved by every layout transformation.
- A standalone export package first reads Intentweave data. A direct API connection is possible
  later, but it is not a prerequisite for the first version.
- Unresolvable references are, for now, excluded from layout calculation; their number is
  reported. The original data set remains unchanged.
- Claims and other overlays are attached through stable references. Presentation, filters and
  influence on the layout should be configurable separately.

## 3. Hierarchical placement and directions

### 3.1 Placement

Fixed grids and flexible placement must both be possible. Node sizes, container padding, header
areas and labels are taken into account. Unrelated nodes or containers must not overlap.
Intentional containment is not a collision. Unsolvable conditions must be reported as
comprehensible diagnostics.

The optimization considers rank assignment, order within the layers, arrangement, spacing and
direction; it is not limited to edge routes.

### 3.2 Direction policy

The following modes are planned for the whole view and for individual layout areas or containers:

| Mode | Meaning |
| --- | --- |
| LR | left → right |
| RL | right → left |
| TB | top → bottom |
| BT | bottom → top |
| Hybrid | automatic choice of a suitable direction per area |
| Inherit | the next explicit setting of an ancestor, otherwise the global setting |

A manual setting overrides the inherited policy. Removing the setting restores inheritance. With
Hybrid, the policy is inherited, not necessarily the concrete direction chosen. Different nested
containers may use different directions. Text and node geometry are not rotated.

Hybrid should weigh compactness, crossings, edge length and movement against the existing
layout. Whether Hybrid also automatically searches RL/BT is still to be decided; explicit RL/BT
is required regardless. A separate override for individual rank layers within the same container
still needs to be specified; the first configuration unit is the container.

## 4. Ports and edge routing

### 4.1 Endpoints

- Input and output ports on all four sides, either automatic or set explicitly.
- Automatic ports follow the sensible actual arrival and departure: a connection arriving from
  below ends at the bottom port, and likewise for top, left and right. Layout preference alone
  must not force a wrong side.
- Port selection and routing should be optimized together. A detour around an obstacle may make
  a different automatic port side sensible.
- Explicit sides, positions and other port conditions remain binding; conflicts are diagnosed,
  not silently overwritten.
- Port order and sufficient spacing reduce crossings and overlaps. At high edge density, resizing,
  aggregation or bundling is necessary.
- Container boundaries get coordinated transitions. A connection is continued outside and inside
  an opened aggregate up to the inner target.

### 4.2 Geometry and quality

Initially orthogonal lines with rounded corners. The interface must later also allow straight
connections without bends and freely curved curves. Arrowheads must clearly show the endpoint and
the logical direction.

Routing should take into account foreign nodes, container header areas, labels and defined
spacing zones. Permitted container passages must be explicitly coordinated. Source and target
bodies must also not be crossed by their own route outside their intended connection. Self-loops
need their own outer routing. For unsolvable conditions, an error or a clearly marked intermediate
state is required.

Soft goals are few crossings and bends, short understandable paths, separated parallel lines,
and little change to geometry that is already visible. Related or matching connections should
optionally be bundleable. Bundles keep their members and their direction; lines that happen to
lie on top of each other are not a bundle.

### 4.3 Concrete routing regression case from the demo

The user screenshot of 20.10.2026 shows the open area `src`, including `queries`, `types.ts`,
`facade.ts` and `incremental.ts`. Visible problems are long detours around the outside, densely
stacked routes, crowded ports and arrows, and lines that appear to continue behind nodes.

From this follow testable criteria for a corresponding test graph:

1. No path crosses a disallowed node body; the check includes source and target bodies outside
   their connection.
2. The arrival segment and the port side match, including after post-optimization.
3. A free short corridor is preferred over an otherwise equivalent large outer detour.
4. Parallel routes have visible spacing or are shown as a bundle.
5. Port and arrow density is handled by spacing, resizing or aggregation; overlapping arrows do
   not count as a finished solution.
6. The criteria apply to LR/RL/TB/BT and to mixed nested directions.

The screenshot is a visual reference. A reproducible synthetic test graph should represent these
cases; private Intentweave data stays local.

## 5. Aggregation and exploration

- Open and close containers, including nested ones; internal states are kept when an ancestor is
  closed.
- LOD reduces presentation details; LOA reduces the graph actually laid out through aggregation.
  The two mechanisms are to be handled separately.
- Drill-down reveals the inner structure; external connections stay understandable.
- Cut-out highlights a path and hides or visually fades out the hierarchy and neighbors. The
  canonical graph is preserved.
- When opening, the original edge endpoints and port bindings are restored.
- Focus, selection and orientation should be preserved for local changes. Position-stable
  expansion and a compact rearrangement are different modes.

## 6. Changes and the backend/frontend handoff

A backend snapshot must be continuable in the browser. A change is first followed by a quick
local repair. Then, if needed, a larger dependent area is recalculated: the neighborhood,
containers, ancestors or other affected areas. This work can run in the frontend or the backend.

- A shared Rust core for native computation and browser WASM in the worker.
- Backend native or via WASM; a Node connection through a napi-rs or comparable adapter is
  planned as an integration target.
- Portable, versioned state; correctness must not depend on a cache that exists only on one host.
- Graph changes, view changes and layout versions are distinguished.
- Outdated results must not overwrite newer changes.
- Related geometry changes are adopted consistently; the renderer may animate the transition.
  Unfinished intermediate geometry must not be output as a finished, collision-free solution.
- Tasks are merged, prioritized and cancelled as soon as they are outdated, where possible.
  Repeated optimization should not oscillate between views.
- Automatic re-optimization also works on first load, if already enabled; manual starting remains
  possible.
- The engine stays independent of React or any particular renderer.

## 7. Scale and evidence

The target size is 2 to over 100,000 nodes in the data set. This does not mean that all nodes are
shown completely at the same time. Aggregation, visibility filters, caching and bounded
calculation areas are part of the design.

Benchmarks must report node count, edge count, density, nesting, cycles, cross-container edges,
hardware and runtime environment. What must be measured: import/precomputation, time to the first
usable image, local change time, post-optimization, memory use and quality. Small demo results do
not prove scaling to 100K. Numeric budgets remain open until measurements exist.

## 8. Implementation status

Hierarchical shadow planning (provisional): `layout::abstract_routing` builds
geometry-free per-edge rank/border paths, ordinal port lanes and channel
ordering/slot estimates. Every edge is accounted for; self-loops, constrained
ports and unsupported structure remain explicit, not silently omitted.
`bench --abstract-plan 1` exposes initial/final plans and, with
`--candidate-log DIR`, candidate plans outside timed engine calls. Benchmark-only
DP events identify logical edges and search phases. The router, candidate set,
selection rules and snapshots are unchanged. This is diagnostic groundwork,
not search-free routing, an infeasibility proof or a performance result.

Physical routing trials (provisional): `bench --abstract-realization 1` adds
real endpoint/dummy/border lanes, exact physical channel spacing, explicit
width/capacity deficits, a separate bottom-up fixed-order growth trial, and
global longitudinal lane reservations on fixed channel tracks. The latter can
move virtual lanes within existing dummy/border rectangles, never resolved
endpoints or node geometry. A deterministic separation-constraint DAG reports
order-specific capacity failures explicitly; these are not infeasibility
proofs or a candidate-pruning rule. Final route occupancy checks remain.
A separate `jointRealization` trial orders channel bends when fixed endpoint
or virtual lane domains cannot realize the preferred longitudinal separation,
then compacts bounded lanes. Bends stay inside existing channel gaps; endpoints
and node boxes remain fixed. Selected-order cycle/blockage and bend-capacity
failures are reported, not silently retried or treated as infeasibility proofs.
A separate `chainRealization` traces propagated lane-capacity paths, selects
at most one locally feasible temporal cut per path, then replans once from
the original inputs. Exactly two planning passes, no probe routes, no
best-result selection or retry loop. Uncut witnesses, cycles and capacity
failures stay explicit; historical paths and local cut feasibility are not
proofs about the graph or the final simultaneous constraint set.
Trial routes use no path search and are checked for monotonicity, obstacle
clearance and accepted-route occupancy. The input snapshot and ordinary
pipeline remain unchanged. Growth rejects pins, fixed container sizes,
placement grids, maximum extents, flattened plans and mixed/reversed flow;
no authored constraint is bypassed.
These are incomplete trial route sets, not published layouts: label placement,
full quality comparison and routing of unsupported edges remain with the
existing engine. See [physical realization results](performance-review-20261104.md#14-physical-lanes-and-fixed-order-channel-width-realization).
The [longitudinal trial](performance-review-20261104.md#15-global-longitudinal-lane-reservations-on-fixed-channel-tracks)
raises bounded search-free coverage but is not a superset or a speedup result.
The [joint bend-order trial](performance-review-20261104.md#16-endpoint-aware-joint-bend-order-and-lane-compaction)
further raises that coverage without extra node growth, still outside the
production router and without a speedup or full-quality claim.
The [capacity-chain trial](performance-review-20261104.md#17-propagated-capacity-witnesses-and-one-chain-cut-replan)
further improves bounded coverage while exposing both recovered and lost
routes and the cost of a second planning pass.
The separate [complete routing diagnostic](performance-review-20261104.md#18-detached-complete-routing-fallback-and-full-quality)
now adds explicit fallback, labels, scene/geometry validation and shared
metrics on the widened drawing. It reports strict track conflicts separately
from metric hard violations. The frozen-view comparison has complete routes
but remaining fallback spacing conflicts and more bends/crossings; it is not
a production router, candidate acceptance change or release-quality result.
An additional `chainsFixedTracks` completion variant now repairs fallback
spacing against immutable constructed routes after the legacy passes. It uses
exact occupancy/corridor events and bounded monotone local moves, not a new
path search. On the frozen view it removes all four remaining strict track
conflicts while preserving nodes/ports and all 347 constructed routes; local
bypasses add six bends and two crossings, so the shared key worsens. General
repair completeness, quality-aware abstract order choice and default
integration remain open. See [fixed-track completion](performance-review-20261104.md#19-fixed-occupancy-final-compaction-with-explicit-quality-cost).

The detached `bench --routing-planning 1` variant now aligns lane runs,
improves a bounded ordinal pair proxy, schedules demand without throwaway
routes before final flow coordinates, and reuses exact dependent subplans
inside a run. On the frozen initial view it reduces bends/crossings and area
against fixed-track completion while preserving zero hard/strict conflicts.
Other global-order candidates still have explicit failures. Full cold/cached
scenes match, but global ordering changes almost all cross domains and the
cache has little measured wall-time effect. This is not a fully geometry-free
scheduler, local full-routing refinement, heuristic early pruning or default
integration. See [planning results and limits](performance-review-20261104.md#20-quality-aware-lane-planning-demand-handoff-and-exact-subplan-reuse).

An additional detached `--regional-routing 1` diagnostic now changes one
adjacent real-sibling pair in one frame, preserves unrelated local placement
and uses anchored positive-size propagation instead of arranging ancestors
again. It evaluates a disclosed sample of eight non-root areas, not a new
production search budget. Candidates fork the same base planning session;
their complete cold/forked results must match. Footprint growth can still
move many descendants, and routing/repair/scoring still run globally.
Ineligible candidates remain visible and no result is published automatically.
See [regional findings](performance-review-20261104.md#21-regional-order-trials-with-anchored-size-propagation).

A further opt-in `--regional-repair 1` diagnostic reuses individual fixed-track
repair decisions only when exact admissible proposal operations and their
occupancy/outline dependencies match. Remote projected events are regenerated;
tiny/near-orthogonal segments, shapes, endpoint clearance and changing moving
routes remain dependencies. Every cold/forked scene, key and conflict report
must match. On the frozen eight-area sample one regional decision reuses,
avoiding 206 of 1,909 candidate trials; certificate costs erase the repair-time
saving. Unchanged-input repeats avoid all 1,909 trials, but most regional edits
miss the cache and add overhead. This remains detached, not a production
refinement/default change. See
[repair reuse and limits](performance-review-20261104.md#22-exact-fixed-track-repair-certificates-and-their-limits).

The separate `--regional-construction 1` diagnostic now shares one inflated-box
index instead of preparing search obstacles per edge, and reuses exact fixed
path geometries/clearance decisions. Current graph-order occupancy acceptance
never replays. Original eager, cold shared-index and forked sessions must match
complete outputs and constructor work. On the frozen `reference` move 334 of
346 path geometries and clearance decisions reuse, with all 346 occupancy
checks fresh. Most observed time reduction comes from shared preparation;
global coordination still costs about 29 ms in that measurement and motivates
the next experiment below. No production operation, budget or default changes. See
[construction reuse and control](performance-review-20261104.md#23-shared-fixed-path-preparation-and-certified-construction-reuse).

The separate `--regional-coordination 1` diagnostic now shares immutable
rank-span and junction geometry within each grown drawing, across the chain
probe and final solvers. No context survives into another candidate;
endpoint lanes, axis projections, constraints and solver decisions stay fresh.
On the frozen baseline/reference view, actual span computations fall from
3,752 to 95 and interior-domain computations from 5,706 to 1,329, with exact
complete-output parity. Three-arm timings isolate this from prior construction
sharing; heavy shared-machine noise prevents an end-to-end speedup claim.
No production operation, budget or default changes. See
[coordination geometry and control](performance-review-20261104.md#24-sharing-immutable-coordination-geometry-within-one-drawing).

| Area | Status |
| --- | --- |
| Shared segment evaluation | Output-equivalent common metric optimization: one normalized-segment box per call and one ordered overlap/parallel/crossing pass instead of two. Published keys/metrics and per-route conflict queries use it across engines; no candidate, budget or metric-version change. Exact former/default/indexed parity and focused twelve-sample measurements recorded in [section 25](performance-review-20261104.md#25-shared-segment-evaluation-and-a-measured-spatial-index-limit). Spatial route-pair generation stays benchmark-only because frozen envelopes limit its benefit; sparse preparation overhead and dense-case limits remain explicit. No cross-candidate score cache or full-refinement speedup claim. |
| Within-drawing coordination geometry | Exact rank-span/junction results, including explicit errors, shared across chain probes and final solvers only for one immutable grown geometry. Fresh endpoints/orientation and a new context per drawing; no solver-result or cross-candidate geometric cache. Previous shared-construction cold/new shared cold/primed fork output identity and repeated actual/reused-work counts checked. All solvers, fallback, final repair and scoring still execute. Also used by the provisional public `abstractRouting` path; default execution remains unchanged. |
| Public complete abstract routing | Provisional opt-in `abstractRouting`: uses geometric track demand from placed cross-axis lane positions and occupied spans to widen rank gaps during placement, then runs after placement before legacy routing for import, ordering candidates and supported edits via the same native/WASM dispatch. Snapshots expose per-channel physical track demand/width, post-placement/pre-completion availability, later gap shift and remaining width diagnostics. The earlier ordinal slot-count seed over-widened layouts and is no longer used; final physical checks and growth remain authoritative. The current one-sample integrated comparison does not isolate a capacity-only speedup and still shows mixed quality/time tradeoffs; retain opt-in. The follow-up audit traced 21 missing diagnostic matches to region-cache hits and now maps all 184 physical channels across the three frozen views, with unchanged geometry fingerprints. Phase 1 combines backward-edge and hierarchical-order rebuilds into one abstract-only placement; the codegraph candidates use two root passes instead of three, with changed abstract geometry and mixed crossings/bends, and no isolated timing gain. Default layouts/fingerprints remain unchanged. See [placement-reuse prototype](../benchmarks/results/abstract-phase1-20261106.md), [capacity-seeding results](../benchmarks/results/abstract-capacity-seeding-20261105.md), [channel diagnostics](../benchmarks/results/abstract-capacity-diagnostics-20261105.md), [seed coverage audit](../benchmarks/results/abstract-capacity-diagnostics-audit-20261105.md) and [geometry-aware capacity comparison](../benchmarks/results/abstract-capacity-geometry-aware-20261106.md). Complete grown plans, local boxes, ports, routes, boundaries, labels and diagnostics survive snapshot handoff. Existing strategies and incumbent/key selection remain; new candidates with strict track conflicts are ineligible. Unsupported modes/constraints are explicit errors. Not output-equivalent and not a default switch; no guarantee of matching the default refined winner. Both axes, four graph families, explicit ports/labels, cycles, loops, parallel edges, revision/cancellation and continuation covered. See [configuration](configuration.md#provisional-abstract-routing-execution). |
| Single-pass construction | Provisional opt-in `singlePass` (requires `abstractRouting`): order, channel slots, widths, one placement and routes, without fallback search, track repair, refinement candidates or later rerouting rounds; delegated edges are drawn as seed routes and counted. Channel slots follow the physical span order (also for `abstractRouting`). The backward-edge border correction runs before the hierarchical sweep, and the placement keeps the swept border ranks; coordinates across the flow come from Brandes–Köpf block placement with edge straightening. Three views, one loaded machine: crossings / bends 3291 / 1592, 240 / 460, 30 / 190 against default 4113 / 1844, 256 / 618, 35 / 208 and elk-rs 3057 / 1638, 278 / 434, 23 / 148; 31-view corpus sums 6667 / 6410 against elk-rs 7472 / 5912; code view leaves 11 strict track pairs unresolved. In one process (loaded machine) the code view takes 135 + 21 ms against default 1498, elk-rs 47 and elkjs 270 ms. Output-identical work on the import then removed 20–25 % of its retired instructions (code view 1924 → 1447 M per import). On a quiet machine the code view then takes 106 + 19 ms in one process (elk-rs 50, elkjs 284 ms) and 151 ms on the public path. Phase-level work (2026-11-07): the reversals are computed before the first placement, which only orders; the final placement is the only pass with geometry; five sweep runs instead of eight; the chain probe's track assignment is reused when it derives no orders, and the refinement measures its input once. Retired instructions per import fall by 27–35 % (code view 1435 → 1004 M); seed means over 34 views 10487 / 8545 → 9919 / 8559 crossings / bends, the frozen code view slightly worse (3322 → 3439 crossings). Loaded machine: code view 77 + 15 ms in one process, 122 ms on the public path (elk-rs 50). The conflict reroute after construction still took 21 % of the code-view import; three causes of delegated edges are now handled at construction (channel width for the joint bend compaction, longest-path ranks after the cycle break, self-loops around the corner), so on the three frozen views nothing is rerouted: code view 1004 → 831 M instructions per import, org chart 158 → 124 M, workflow 149 → 155 M; seed means over 33 comparable views 6642 / 7067 → 6644 / 7185 crossings / bends, hits 101 → 96, issues 8 → 7. Not a default; default fingerprints unchanged. See [results](../benchmarks/results/single-pass-blocks-20261106.md), [timing](../benchmarks/results/single-pass-timing-20261106.md) [phase work](../benchmarks/results/single-pass-phase-20261107.md) and [no reroute](../benchmarks/results/single-pass-construct-20261107.md). |
| Fixed-track repair geometry reuse | Production fixed-track repair reuses exact per-route segment/bounds projections across partner checks and candidate evaluations, updating only after an accepted route move; conflict-free repairs return unchanged routes after cancellation is checked. Deterministic projection-build and conflict-pair-check counters are exposed in benchmark candidate work. Workspace tests and browser/WASM build pass; a three-round integrated rerun preserves repeat identities and default historical geometry. The abstract opt-in still has more total crossings on all three frozen views, so no equivalent-quality speedup or default switch is claimed. Port-order and channel-heuristic probes did not improve the selected key; crossing-aware ordering remains a separate follow-up. See [phase-2 completion and crossing findings](../benchmarks/results/abstract-phase2-20261106.md). |
| Fixed-path construction reuse | Detached `--regional-construction 1`: fresh endpoint/flow preparation without eager per-edge search obstacles, one shared inflated-box index, bit-exact path keys and fresh relevant-obstacle certificates. Geometry/clearance may reuse; occupancy acceptance always reruns in graph order. Three-arm original/shared-cold/forked identity and actual-work checks; nested constructor phase clocks. Coordination, fallback, repair and scoring remain fresh. No general route cache or production integration; broader graph-family measurements remain open. |
| Exact repair-decision reuse | Detached `--regional-repair 1`: opt-in run-local certificates for successful and unsuccessful fixed-track decisions, exact ordered proposals plus indexed candidate-segment influence, independent base forks and transactional session replacement. Complete cold/fork/repeat identity and actual/avoided work checked. Only one regional decision reuses in the frozen sample; cache-miss overhead is explicit. Construction, fallback and scoring stay global; no general route cache or production integration. |
| Regional order trials | Detached `--regional-routing 1`: adjacent real-sibling swaps, target-only placement, anchored positive-size propagation and exact independent planning forks; unchanged local frames/order and complete cold/forked scene identity checked. Explicit eight-area diagnostic sample, no production portfolio/acceptance change. Full routing/repair/scoring remain global; regional routing influence and broader graph-family quality remain open. |
| Quality-aware demand planning and reuse | Provisional detached `--routing-planning 1`: bounded ordinal quality sweeps and constant-lane runs, demand before final flow placement, exact run-local edge/channel caches and complete cold/cached quality checks. No route/score cache or production-refinement integration. Actual global order candidates invalidate almost all cross-domain plans; unresolved hard/track failures remain visible. Production regional scheduling, earlier cross placement integration and broader graph-family validation remain open. |
| Order hints from a drag | Provisional `orderHints` (node → the sibling it follows in its rank, or first): applied after the whole ordering of a layered area, validated, part of the undo intent. A released drag of one node on a single-pass drawing previews without leaving single pass and turns the drop into a hint. Pins, rank changes and multi-node drags still use the pin path. See [results](../benchmarks/results/single-pass-drag-20261107.md). |
| ELK-form adapter | Provisional `elkLayout` operation (`crates/elk`, `packages/engine-client/elk.js`): a graph in ELK's JSON form (children, ports, edges with sources and targets, labels) is laid out and returned in that form. Read: `elk.direction`, `algorithm`, `edgeRouting`, the spacings `nodeNode`, `nodeNodeBetweenLayers`, `edgeEdge`, `portPort` (as the provisional `nodeSpacing`/`layerSpacing`, `trackSpacing`, `minPortSpacing`), `portConstraints` (`FREE`, `FIXED_SIDE`/`FIXED_ORDER` with `port.index`, `FIXED_POS`/`FIXED_RATIO`) and `nodeLabels.placement`; options without a counterpart are reported. No ELK code or dependency; not 1:1. Ports on any side are constructed by the provisional `portSides` option (on in the adapter for graphs with explicit ports; on random graphs with ports on all sides every edge is constructed, with no shared lines, fewer crossings and less hugging than elk-rs, but more bends). See [ELK format](../benchmarks/results/elk-format-20261108.md) and [port sides](../benchmarks/results/elk-port-sides-20261108.md). |
| Packaging (crates.io, npm) | Published: 0.1.0, 0.2.0 and 0.2.1 (2026-11-11), 0.3.0 and 0.4.0 (2026-11-13; see CHANGELOG.md, 0.4.0 with an upgrade section). Version 0.1.0 published on 2026-11-11 (tag `v0.1.0`; crates.io's limit on new crates needed the last five to be published one by one). Crates: `arqlet` (facade, re-exports the engine crates and `dispatch`) and `arqlet-model`, `-topology`, `-routing`, `-scene`, `-view`, `-layout`, `-reference`, `-elk`, `-runtime` at 0.1.0, exact internal versions, MSRV 1.88 (checked with `cargo +1.88 check`), `cargo publish --dry-run --workspace` passes; `arqlet-wasm`, `-contracts`, `-node` stay unpublished. npm: `arqlet` (entry point), `@arqlet/engine-client` (ships `arqlet.wasm`; `loadEngine()` reads it by `fetch` in a browser and from disk in Node), `@arqlet/viewer` and `@arqlet/contracts` (one bundled `.d.ts`), with hand-written type declarations; `@arqlet/containment-tree` stays private. Checked: the packed tarballs install into an empty project, lay out an ELK graph in Node, typecheck strictly (`nodenext` and `bundler`), and a Vite build emits the WASM. Steps in [releasing.md](releasing.md). Since 0.2.1 the npm WASM is built for size (`wasm-size` profile: 4.8 MB, 1.2 MB gzip). Not done: a release workflow (tag, trusted publishing, provenance) |
| Cluster overview (ADR 0018, first slice) | Provisional operation `overview`: the whole graph as discs in its containment hierarchy, each container's children arranged by a force simulation (springs on aggregated links, weak Barnes–Hut repulsion by area, gravity, collision, separation to no overlap, nearly smallest enclosing circle), containers of one height in parallel with identical output for any thread count (`crates/layout/src/overview.rs`, `crates/runtime/src/overview.rs`, tests `overview::` in both). WebGL renderer, picking and drill-down excerpt in `@arqlet/viewer` (`overview.js`, `overview-gl.js`, Node tests). Page `apps/playground/overview.html`: browser (WASM worker) and backend modes, zoom into a cluster by click; a cluster of at most 300 nodes switches in place from organic to structured: its layered layout is drawn in its disc (boxes, routes with arrowheads, names when readable), the disc grows to the layout's size and the neighbours make room (`overview` with `radii` and `start`: unchanged containers keep their arrangement exactly, the path to the top continues cooler); a second click switches back. Every cluster is collapsed, organic or structured, decided by the user (click, shift-click), then by rules by node kind, then by the zoom (`representationsOf`: collapsed below 14 px, structured for the cluster under the focus from 35 % of the view, with hysteresis); collapsed clusters bundle their edges per pair; edges leaving a structured cluster cross its box at border ports (`structuredExcerpt` with stand-ins, `clipToFrame`). Decisions and rules are engine values (`OverviewIntent`, `OverviewConfig` in a `ConfigSet`; checked and answered by `overview`), with undo; the border side faces the neighbours (`flowDirectionOf`); labels do not overlap; large flat containers and flat graphs are split into nested communities by deterministic Louvain (`with_communities`, default above 120 children, clusters of at least 8). Only the user's decision makes a structured cluster grow (the zoom and rules fit it into its disc, so zooming does not jump); making room removes overlaps only and moves nothing that is not touched. A panel shows the selected cluster with buttons (collapse, open, structured, automatic), its flow, rules by kind, zoom thresholds and layout options. In the SVG playground, opening, closing or setting a container keeps it where it is on screen instead of fitting the drawing. Changes are animated by interpolation, not simulation: a continuation morphs in the vertex shader (old and new places, eased) with the view; collapsing and structuring blend; a cluster that gives room back lets its neighbours settle (`start.settle`), closing the gap. The SVG playground animates edits (FLIP, about 0.4 s; off with reduced motion). Checked in headless Chrome: 10 000 and 50 000 synthetic nodes (`benchmarks/overview/generate.py`, seed 1) lay out without overlaps and draw; a 169-node cluster switches to structured in place, grows, and switches back. No timing is claimed: `benchmarks/overview/measure.py` records results on an idle machine. Trace and neighbourhood (provisional): `overviewTrace` / `overviewNeighbourhood` on the overview's graph (`GraphTrace`, `GraphNeighbourhood`; in a session on the graph it keeps), drawn by the viewer in amber over the discs that show the path now, the rest dimmed (`pathOf`, `setState({trace})`), opened by `revealPathDecisions` and framed by `viewOfNodes`; page section *Path* (tested in Rust, the viewer and the session tests; checked in Chrome on a large overview). The answer is typed (`arqlet_model::Overview`, request `OverviewRequest`; `Overview` and `OverviewRequest` in `@arqlet/contracts`, checked against engine output in Rust and by the contract typecheck). Not done: Leiden instead of Louvain, edge level of detail beyond bundling, comparison with d3-force |
| Cross-platform determinism | Native builds (Linux x64 and arm64, macOS arm64, Windows x64) and WASM give identical responses: transcendental functions through `arqlet_model::math::PortableMath` (`libm`), no `usize` arithmetic that overflows on 32-bit. Checked by `tools/platform-parity.mjs` (layered, tree, organic, mixed, ELK, overview) on every platform in CI (`platforms.yml`), whose last job compares the response digests across platforms. Found on 2026-11-11: organic layouts and the overview differed between native and WASM before. |
| Isolated children as a grid (`isolatedGrid`, provisional, off) | Children of a layered area without an edge to another child stand in a roughly square grid of their own ranks after the connected children instead of one long column; the connected part is placed exactly as without them. On 14 real file clusters: same bends, crossings and lines through nodes, diagonal 2.2–3.9× smaller, area smaller in 9 of 14 ([results](../benchmarks/results/isolated-grid-20261111.md)). Used by the overview's structured clusters; not the default. |
| Packed components (`packComponents`, experimental, off) | The weakly connected components of a layered area's children (a lone child is one) keep their own ranks and are packed in rank space towards an aspect ratio (`aspect`, default 1; vertical flows turned): columns of rank ranges, components across the flow within a column, the deepest first. The arrangement stays one layered drawing, so ordering, placement and routing are unchanged. On four synthetic shapes ([results](../benchmarks/results/pack-components-20261113.md)) aspect 0.02–0.23 becomes 0.91–1.67 at equal or smaller area, 0 crossings, 0 lines through nodes, equal or fewer bends; a test graph shows +3 % bends. elk-rs with separated components is 27 % smaller in area on the mixed shape (the general area gap). Default output unchanged (25 graphs byte-identical with and without `isolatedGrid`/`componentPacking`). Checked locally on larger evaluation graphs (not published): no gain on large nested views (more crossings), behind the ELK adapter, or for the overview's structured clusters, so neither uses it. Not yet: a structured cluster that is one connected component (its aspect follows its ranks), and a default switch. |
| Layout session for hosts (`createSession`, provisional) | JavaScript helper over any engine `run` (direct or `workerRun` on the engine worker): import, collapse, expand, toggle and any other edit, applied in order to the newest snapshot with the expected revision and layout version; a change overtaken by a new import or reset resolves as stale and is not applied. Tested against the WASM engine and with a delayed fake engine (`packages/engine-client/session.test.mjs`). |
| Overview fisheye lens (renderer, provisional) | Screen-space Sarkar–Brown lens in the WebGL renderer: shapes and lines go through it in the vertex shaders (every corner, so the mapping is the true one and nothing overlaps; discs at its rim become ellipses), labels are placed and sized through it, and `world`/picking use its inverse. Follows the pointer through `bindOverviewNavigation({lens})`. No layout change. Tested: `lensAt`/`lensInverse` (continuity at the rim, monotone, inverse) and labels under the lens; checked by eye on the Intentweave export. |
| Overview idle breathing (renderer, provisional) | After `idle.after` ms without input, the discs of the WebGL overview grow and shrink by `amplitude` in a wave from the canvas centre (vertex shader only; boxes, lines and texts of structured clusters stay); input fades it out over 400 ms, unbinding stops it at once; never under `prefers-reduced-motion`. Tested with a fake renderer (`overview-gl.test.mjs`) and in Chrome on the Intentweave export (60 frames per second while breathing; still after input). |
| Communities in the structured view (`communities`, provisional, off) | Drill-down like the cluster overview's, in the layered engine: large containers get virtual community containers in the view (`arqlet_layout::communities`, the overview's Louvain), collapsed at first, opened with `toggle`; every projection derives them from `sourceGraph` again (`arqlet_runtime::project_view`), so edits work and the source graph never holds them. Named after the member with the most edges. On a flat call graph of 1 866 nodes: first view 33 nodes, 4 620×2 902, instead of 15 340×77 792 ([results](../benchmarks/results/structured-communities-20261111.md)). Edges between collapsed communities: one route per pair for both directions, with `weight`/`backWeight` in the scene (crossing route pairs 2 711 → 1 164 on that graph). In the SVG playground: field *Communities above*, example *Flat call graph*. Domain groups (`groups`: a path per node, containers of kind `group`, Louvain only for what stays large) and `levelOfDetail` (by depth or kind) give a first view like the overview's: the flat call graph with its directories as groups opens as 3 collapsed groups with 3 weighted routes, then level by level. Hubs (`communityHubs`, overview option `hubs`, provisional): children with at least `minLinks` links in their container, or named, are left out of Louvain and stay direct children, so one hub no longer pulls others into one community named after it (tests `community_hubs.rs`). An import's `limit` counts the boxes of such a first view (a collapsed container once, kept whole so it can be opened; a container an edge reaches comes whole and collapsed; tests `import_limit.rs`). Next: details inside a node (ports, rows) hidden at coarse levels, and whether a WebGL scene renderer is needed. |
| Path cut-out (`trace`, `reveal`, provisional) | Steps 1–3 of the path cut-out: `trace` (read only, `arqlet_view::trace`) finds the nodes and edges on the shortest or all directed paths between two source-graph nodes (the other direction if needed; cycles, self-loops and parallel edges allowed; unresolved edges excluded) and maps them to the view: holding boxes, carrying routes, distance of each visible node; `neighbourhood` (provisional, `arqlet_view::neighbourhood`) answers the same way for the nodes within `hops` (default 1, at most 1000) of `nodeId` or `nodeIds`, following edges `out`, `in` or `both` ways and only of `edgeKinds` if given, with each node's `hops`; its nodes go to `reveal` (playground: *Neighbourhood* around *From*); `edgeKinds` restricts the paths to edges of those kinds (`arqlet_view::trace_along`; a carrying route of a followed kind is preferred; the playground's *Only shown edge kinds* follows the filter); `reveal` expands exactly the collapsed containers above given nodes (a view request with revision checks); the SVG viewer draws the path in amber and fades by distance. Partial reveal (default): an opened container shows only the children on the way; the others wait in a collapsed `rest:<container>` (`+N more`, dashed), which a toggle dissolves (`LayoutOptions::partial`, view only). Step 4 (straightening the path) not started. Tests in `structured_communities.rs` and `arqlet_view`. |
| Filter (`filter`, provisional) | Hides nodes by kind or id (with what they hold) and edges by kind as view state (`LayoutOptions::hidden`); the geometry stays, the scene leaves the hidden out, and a route for collapsed content is counted again from the edges still shown (dropped when none is left). In every mode and in resident sessions (`session.filter`); playground section *Filter*. With `compact: true` the view leaves the hidden out (`structured`) and is laid out again, so the rest closes up; a toggle or an edit keeps it, and showing everything lays it out again (tests in `filter.rs`, session test). |
| Typed protocol (ADR 0019 step 1, provisional) | `arqlet::protocol::{Request, Response, handle}`: the import and every operation on a snapshot as typed variants (snapshot, expected revision and layout version in `Edit`), the other operations as variants with a JSON body; `dispatch` parses, handles and writes through them with the same JSON and the same errors (test: 68 requests byte for byte against the dispatcher before; typed requests written back get the same answers). Rust hosts build requests as values. Typed answers (provisional): `Response::Scene`, `Trace` (`TraceAnswer`), `Neighbourhood` (`NeighbourhoodAnswer`), `Overview`, `Intent` and `Metrics`, written to the same JSON (the byte-for-byte test covers them); `TraceAnswer`, `NeighbourhoodAnswer` and `VisibleParts` in `@arqlet/contracts`, and `session.trace` / `session.neighbourhood` typed with them; a test reads every such answer back with unknown fields refused. Not yet: typed request fields of the read-only and tool operations, and typed answers of the tool operations (`elkLayout`, the refinements, references). |
| Resident sessions (ADR 0019 step 3, provisional; the default of `createSession` where the engine offers them) | `arqlet::Session` (`EngineSession`) keeps a view's snapshot in the engine: requests without the snapshot (expected versions default to the session's; a request naming others is refused as stale), answers with revision, layout version, collapsed containers and the scene; `load`/`export` hand a view over between engines. WASM by handle (`lw_session_*`), the worker by message, JavaScript `createSession` (`resident: 'auto'` by default; `false` sends the snapshot). Tested: the same scenes, states, traces and exported snapshot as a session that sends the snapshot (Rust and against the WASM engine). In the backend: `layout-command --sessions` (one long-running process, a message per line) behind the playground's `/api/session` (random keys, serial, time limit per message, at most 32 sessions), and `httpRun` in JavaScript; so sessions keep the snapshot in all three deployment modes. The overview too: a session's `overview` keeps the graph, the cluster hierarchy (rebuilt only when `communities`, `satellites` or `groups` change) and the last positions, so a `start` without positions continues from them; with `positionsOnly` and the same clusters the answer carries only what moves (`OverviewPositions`: positions, root radius, counts, representation; on the synthetic `benchmarks/overview/generate.py --nodes 20000 --seed 1` graph, a single run in Node on an Apple M3: 0.37 instead of 1.46 MB per continuation), which `createOverviewSession` merges with its last overview, so its answers stay whole; `createOverviewSession` in JavaScript (the overview page uses it). Tested: the same answers as `dispatch` given the graph and the positions (Rust), and resident against sending (WASM). |
| Automatic single-pass path choice | `singlePassAuto`: single pass where it works; otherwise the other pipeline and the diagnostic `Single pass was not used: <reason>`. `abstractRouting` and `singlePass` in the result say which pipeline made the drawing; edits and refinement stay on it until the next import or `configure` that sets the option. Graph-level fallback only; no fallback per area. Tested for every edit, in all three deployment modes, and on 31 unmodified datasets. **The default since engine 0.2** (ADR 0016, accepted 2026-11-08; `singlePassAuto: false` keeps the pipeline before). See [results](../benchmarks/results/single-pass-auto-20261107.md) and [gate](../benchmarks/results/single-pass-gate-20261108.md). |
| Abstract-plan routing completion | Detached opt-in `bench --routing-completion 1` experiment. Same grown geometry for existing constructor, two-pass chain planner and chain planner plus final fixed-track repair, explicit fixed-route/fallback partition, complete scene/labels/metric evaluation and separate repeated phase timings. The optional fixed-track repair only changes fallback geometry, removes all four frozen-view spacing conflicts and reports unresolved cases in general. No ordinary engine/default/refinement change. Additional bends/crossings and a worse shared key stay visible, not accepted or hidden. Full pre-coordinate scheduling and production quality-aware order selection remain open. |
| Constraint-based overlap removal | Experimental `vpscOverlapRemoval` option (default off and omitted from serialized defaults): deterministic minimum-displacement separation constraints run during organic separation and as a pre-pass to geometry repair. The existing outline checks and bounded repair remain authoritative; quality and performance evaluation is pending. |
| Force-directed drag region | With `forceDirected`, a drag relaxes only the changed children and their graph neighbours within two hops; other children keep their centres (P2.7). The drag step does not repair; the drop does. Cooling (alpha) and an explicit anchor term are not implemented. |
| Routing-engine choice for conflicts | When a drawing runs routes through nodes, both engines route the same boxes and the drawing with fewer conflicts is kept (`crates/layout/src/engine_choice.rs`). Boxes never change; default output is unchanged where there are no conflicts. Also a greedy side search over the automatic endpoints of conflicting edges (track ordering): endpoints of one node that share a side are spread, and candidates are complete layouts. Offsets along a side are searched too (a route can leave a side near its end). The Optimize step (refinement job) also separates routes that cross each other where two crossing routes leave one node side in the wrong order; it minimises the published key, and import and edits do not run it. The remaining (−120, 300) conflict is fixed by the offset search (plan, Phase 3). |
| Sparse organic stress | Experimental `sparseStress` option (default off and omitted from serialized defaults): stress targets are graph neighbours plus 50 farthest-point pivots, so the stress needs no n² distance matrix, and the organic bound is 500 children instead of 300 (the largest size with a measured single import: 82 s for 500 children on one release run, against 13.6 s for 300). Repair still keeps a triangular pair table; quality and timing evaluation is pending. |
| Scalable organic placement | Experimental `scalableStress` option (default off and omitted from serialized defaults): pivot MDS start, sparse stress whose pivot terms carry the size of their region (Ortmann, Klimenta and Brandes, GD 2016) plus terms for children two steps apart, then a deterministic force refinement in the manner of d3-force (springs, Barnes–Hut repulsion, box collision with the gap, and in its second half a push of nodes off the edges that cross them). Organic bound 2000 children per area. Under the option the repair uses the local scan budget, neither the conflicting routes nor the whole drawing are routed again (that step did not finish in 30 minutes at 2000 children), and no orthogonal routes are computed when every edge is straight-only; remaining straight-through conflicts are reported. Measured on synthetic graphs of 500–2000 nodes against d3-force and Cytoscape CoSE: no overlaps, less than half the crossings, 3.5–5.5 times fewer edges through nodes and lower stress than d3-force, with less area, and faster (0.38 s at 500, 2.3 s at 2000 nodes, against 0.54 and 2.7 s; `benchmarks/results/organic-scalable-speed-20261109.md`). Not yet the default; containers with scalable areas, continuation (drag), browser timing and the clustered force view are open. |
| Edge-aware organic repair | Experimental `nodeEdgeRepulsion` (bounded push of free nodes off projected edge chords after organic stress, found through the spatial hash) and `directedRepair` (overlap translations and perpendicular through-conflict moves tried before the exhaustive search, which remains the fallback). Both default off and omitted from serialized defaults; trial-count and quality evaluation is pending. |
| Rust core, native CLI, browser WASM worker | Prototype available |
| Intentweave export, exclusion of unresolvable edges | Available |
| Nested placement, cycles, self and parallel edges | Basic function available |
| Requirement values | Rules: a non-finite, non-positive or overflowing clearance is refused; large valid clearances are respected, not capped. `maxExtent` (optional) reports a drawing above the limit in `routingIssues`; the search does not enforce it, so the report does not prove infeasibility. The playground hint on the clearance-to-node ratio is open |
| Refinement fallback and work diagnostics | If no candidate is eligible, retain the input snapshot and its geometry issues instead of returning an error without a drawing. Aggregate and per-candidate benchmark work includes regional structure, order, cross-flow lane/track plan and local coordinate realization computations/reuses, deferred routing passes, constructor preparations, planned obstacle/track failures and internal search calls, failures, corridor/lane cells and clear lane transitions, separately from delegated fallback. Cumulative placement, organic, repair, port, routing, corridor-gap, constructor preparation/search and scoring times require the native `benchmark-timings` build; default core and WASM builds do not read a host clock. Timers overlap. Placement timing includes re-placement after hierarchical channel probes and late widening. Experimental `incrementalClearanceWidening` is off by default; it re-places widened areas and reuses unaffected routes, and can change the candidate geometry. |
| Direction and layout strategy constraints for the root and every container (ADR 0003) | Direction: fixed (`lr`, `rl`, `tb`, `bt`), free (`auto`, chosen per area by compactness) or allowed (a list), inherited per container. Layout strategy: `none`, `tree` (a forest along the direction, refused with the reason on cycles, two parents, parallel or antiparallel links; a self-loop is ignored by the tree skeleton and drawn by the router), `radial` (a forest on rings, needs a free direction), `auto` (the best eligible strategy wins, by the shared key) or an allowed list; inherited per container on its own axis. `shortest` is a criterion for an orientation per area, not a strategy, and waits for its own parameter; `hybrid` is removed (it is the free direction). Earlier documents keep their meaning. Not done: the criterion parameter class, drawing-level constraint checks, tree spanning variants |
| Hierarchical layout pipeline | Default for views with at least one supported direction-based area. Area modes are capability-checked; fixed/free/allowed directions remain binding, and explicit tree, radial and organic areas keep their strategy-specific placement. Views with no supported hierarchical area, a nonzero `placementGrid` or `placementSearch`, or pinned leaf nodes use the previous pipeline and report the fallback reason in `layoutDiagnostics`; pinned containers remain on the hierarchical path. Set `layeredConstruction: false` to select the previous path explicitly for comparison. Within one layout, non-flattened regions reuse structural/rank planning with ancestor-reversal invalidation. Hierarchical regions reuse cross-flow lane/track plans; full-view placement also reuses unchanged local coordinate realizations, checking current child sizes, local gaps, border geometry and order. The hierarchy is still traversed to check dependencies; no cache survives a complete layout or replaces partial/warm-start geometry. The first layout remains separate from refinement; the default does not request a full refinement after import or reduce the configured refinement candidates. A default change intentionally changes geometry. Prototype correctness is covered by the layout, routing, port, label, clearance and container checks; this is not a release-readiness claim. |
| Abstract routing and longitudinal lanes | Diagnostic only: coordinate-free rank/border plan, physical channel demand, bottom-up fixed-order growth, global longitudinal reservations, endpoint-aware bend compaction and a bounded two-pass capacity-chain-cut trial with verifiable witnesses. Every edge is either realized or explicitly delegated. Endpoint positions and public snapshots stay unchanged. Selected lane/bend order may reject routable edges; simultaneous conflict-chain/order selection, labels, full-quality acceptance, pre-placement planning and production fallback integration remain open. |
| Organic placement and straight routing (ADR 0004) | `organic` is a layout strategy for an area with a free direction: stress majorization on graph distances, separated by the real node sizes; bounded to 300 children per area; deterministic. `edgeRouting: straight` is an edge option for any arrangement: one segment per edge between the port positions, no bends; an edge through a foreign node is a routing issue; a self-loop has no second end, so it follows the loop rule instead of being refused (amended 2026-10-27): it is never drawn straight, the ordinary edges of the area stay straight, and the loop takes the other styles the area admits, else the orthogonal loop around its node (`styles::edge_choices`); parallel and antiparallel edges are drawn side by side (row `Parallel edges and organic areas with directions`). Repair at an organic root (`crates/layout/src/repair.rs`): after the stress placement, the drawing is checked on its real geometry (the drawn ports of the straight edges, node outlines, clearance). Overlaps and straight edges that enter a foreign outline are resolved by moving movable root leaves in a bounded greedy search with region growth; pinned nodes never move and edges never bend. Conflicts that remain are geometry issues (`layout-conflict` kinds `overlap`, `fixed-overlap`, `straight-through-node`, `outside-area`), which make the drawing ineligible. Measured on 20 mixed diagrams (14 nodes, 18 straight edges, circles and rectangles, seeds 1 to 20, Apple M3, macOS 26.0, revision 8efd358 plus this change; `benchmarks/results/straight-repair-20261025.md`): drawn straight edges through foreign nodes went from 24 to 0 (one overlap or clearance conflict is left and reported); overlaps stayed at 0. The search ranks a move by conflict count, then by the total depth of the conflicts (how far outlines reach into the clearance, how close a segment passes a blocker), then by displacement, and tries jumps up to eight steps. Warm start: a session recomputation (`recompute_with`) takes the previous positions as they are and runs no stress iterations and no pivot initialization again; a change reaches the neighbours through the pins, the separation and the repair, so unchanged nodes do not move (a repeated recomputation on an unchanged drawing gives the same boxes; test `a_warm_start_on_an_unchanged_drawing_does_not_drift`). Measured on 8 mixed diagrams through move, resize, shape change and a second move (`crates/runtime/tests/interactive_continuation.rs`, debug build, Apple M3): the median displacement of unrelated nodes after removing the common shift is 0 for the warm start and 39 to 136 units when the layout is computed from scratch. A cold layout, and a partial start that does not name every child, still run the stress. Default outputs are unchanged: 19 benchmark configurations at 1 and 4 threads are identical to base `8efd358` in refine and geometry fingerprints (`benchmarks/results/straight-repair-20261025.md`). Not done: containers as obstacles are conservative; the playground drag, resize and shape change are not yet wired to these results. Diagnostics order: with straight edges, `compute` derives the labels and the route diagnostics (`Edge X passes through node Y`, `route_conflicts`) again after the straight segments are applied, so they describe the drawn routes; before, they described the orthogonal routes the segments replaced. `decorate` keeps the findings of `apply_straight_edges` (`straight edge X passes through node Y`), and `apply_straight_edges` replaces its earlier findings instead of accumulating them. The rectangle hit test of the router now also judges oblique segments by entering the interior; it ignored them before. Both messages can name the same edge: the first judges the segment against every node including its own endpoints, the second skips an edge's endpoints. Organic in the auto search is not done. First comparison: `benchmarks/results/organic-20261024.md`, drawings for review in `benchmarks/results/organic/` |
| Repair in containers (organic areas inside rectangular containers) | The repair (`repair_areas`) works on every organic area, not only the root: unpinned leafs and containers of an organic area are movable (a moved container carries its children), overlaps are judged among siblings, and a container child keeps to the interior of its container (geometry padding and header). A container whose size follows its content grows to hold its children after each round (`fit_containers`, never shrinks, deepest first) and the repair runs again, up to four rounds, because a grown container can reach a sibling; an authored fixed size is kept, and a child that cannot fit is reported (`outside-area`, plus the validator's `container_bounds`) instead of growing the container silently. A straight edge crosses only the containers that hold one of its endpoints; any other container is an obstacle. The disc test of `Outline::segment_enters` now has the same 1e-9 inset as the rectangle test: a port that lies on a circle up to rounding no longer counts as entering its own node (it was reported as `Edge X passes through node Y` for a circle's own port inside its bounding box). Measured on 10 diagrams (2 containers with 4 leafs each, 4 loose leafs, every third leaf a circle, 16 straight edges, seeds 1 to 10, debug build, Apple M3, macOS 26.0, base b90a5e8 plus this change; `crates/runtime/tests/containers_straight.rs`): drawn straight edges through foreign nodes went from 81 (9 of 10 diagrams) to 0; overlaps between siblings 0 before and after except one diagram (seed 10) that keeps an overlap, which the engine reports. The interactive sequence (move, resize, pin, unpin, shape change) on 6 diagrams leaves no conflict the engine does not report, keeps pins, and moves fewer loose nodes than a layout from scratch. Not done: polygon contours; reserved areas, clearances and port constraints reported by the consumer; labels as obstacles of a straight edge; a label-measure edit that re-checks only the affected area (a measurement recomputes the layout from the current positions as for a resize); a visible marking of the conflicting nodes in the playground during a drag preview (the conflicts are listed as text); a circular container. Default outputs: 19 benchmark configurations at 1 and 4 threads are identical to base b90a5e8 (`benchmarks/results/containers-repair-20261025.md`) |
| Circles in the visualization and the first computation | The viewer draws a circle node as a disc with its label centred (`text-anchor: middle`) and its pin marker and pin control on the disc, not in the corner of the box (`packages/viewer/svg.js`, `shapes.test.mjs`). The playground reports the size the label needs: a rectangle as before (at least 180 x 54), a circle by the diagonal of the label's text box, at least 56 (`labelSize` in `apps/playground/geometry.js`; before, every measured node was at least 180 wide, so a circle was 180 across). In the engine a circle without a measured size is at least 72 across and as large as the diagonal of its label's box (7.5 units a character, 23 characters at most; `DEFAULT_CIRCLE_DIAMETER`, an engineering default), so the label of a collapsed container fits its disc. Demo dataset `petri-net-shapes` (generator `benchmarks/generate.py`, copy in `examples/fixtures/petri-net-shapes.json`): the Petri net with circular places and rectangular transitions, organic strategy and straight edges. Repair speed: the trial scans of the repair are answered from the scan of the arrangement the search stands on (only the moved node's items and the ports of its peers are recomputed; each answer is compared with a full scan in the tests), a repeated repair on identical boxes is answered from a memo, the search uses finer step sizes, and siblings keep the validator's gap. When single moves are stuck (typically after a measurement made the nodes larger), the arrangement is spread out in steps (1.2 to 2.5) and repaired again; the best result wins. Corridor widening is skipped when every area is organic or every edge can only be drawn straight (it re-placed the drawing from scratch each round and dropped the warm start; an organic area reads no gaps): the label measurement of `petri-net-shapes` went from 2.6 s to 74 ms and a move-gesture step from a median of about 3.2 s to 9 ms (`benchmarks/results/containers-repair-20261025.md`). Measured: the first computation of `petri-net-shapes` went from 13.1 s to 0.6 s (release build, Apple M3, details in `benchmarks/results/containers-repair-20261025.md`); with label sizes reported the result has no conflict. Not checked in a real browser. Not done: the repair of a very large organic area (the scan is still linear in the nodes and edges per trial), a visible marking of conflicting nodes during a drag |
| Drag: move or pin | What a drag does depends on where it starts. A drag on the node (or on a container's ring or title) only moves it: the gesture finishes with `release`, so the drop leaves no pin unless the node already had one (then the pin moves with it). A drag on the node's pin control moves the node and keeps the pin at the drop; a press on the pin without movement toggles it (`onNodeDragStart(node, event, {fromPin: true})` in `packages/viewer/svg.js`, `releaseOnDrop(fromPin, wasPinned)` in `apps/playground/drag.js`; the first version had a Drag selector, which was dropped for this rule). The engine needed no change (`MoveStep.release`). Behaviour without a pin: in an organic area the node stays where it is dropped (the continuation takes its box as the start position) and the repair arranges the others around it; in a layered, tree or radial area the layout places it again by its own rules, and the status line says so ("placed again by the layout"). Tests: `crates/runtime/tests/move_only.rs` (both cases), `a_released_drop_in_a_circle_container_stays_where_it_was_dropped`, `apps/playground/drag.test.mjs`, `packages/viewer/shapes.test.mjs` (pin press). Checked in headless Chrome: a drag on the node leaves no pin, a drag on the pin pins, a click on the pin toggles it twice. Not done: a move-only drag that reorders nodes in a layered area (drop between two nodes) |
| Parallel edges and organic areas with directions | Parallel and antiparallel edges are drawn straight: each is one segment with ports of its own, so the segments lie side by side, without a bend or a curve (`ports::resolve_nodes`). A rectangle already spread the ports on one side in the order of their edge ids; a circle moves each port of such an edge along its outline, the width of the minimum port spacing per sibling, to the side along which the facing rectangle spreads its ports (between two circles the longer axis), so that a circle and a rectangle agree on which sibling lies where and the segments do not cross. Self-loops stay refused under straight routing (they have no second end). A collapsed container, whose edges to one outside node become parallel, is drawn straight too (it was refused: `straight routing does not draw parallel edges between X and Y yet`, which also made the collapse of a container fail). Organic is allowed whatever the direction: the strategy an area is given wins over its direction, which stays in the document and applies again when the strategy changes (`Graph::area_mode`); a strategy that a container only inherits yields to a direction the container declares itself, so the pools of the BPMN example stay layered inside an organic view (before: `organic cannot take the fixed tb direction of pool:customer: set the direction to auto`). Tests: `crates/runtime/tests/parallel_straight.rs` (four shape pairs with the partner on six sides: ports of their own, no crossing, nothing reported; collapse; organic with fixed directions), `crates/layout/src/organic.rs`. Measured on the BPMN dataset (26 nodes, release build, Apple M3): organic with orthogonal routing and the two parallel edges `e10` and `e13` is laid out without a conflict; straight routing draws the pair as two segments 45 to 60 units apart. Organic with straight edges draws them too, but a straight edge cannot run around a node inside a layered pool, so crossings remain and are reported (1.5 s). Not done: a visible marking of the siblings (labels at the segments), bundling of parallel straight edges |
| Force-directed behaviour | Option `forceDirected` (default off and omitted, so default outputs are unchanged; checkbox `Force-directed` in the playground's layout settings). In an organic area a layout that continues an earlier one normally holds every node where it is (no stress iterations on a warm start) and moves only what an edit and the repair require. With the option the continuation relaxes the stress for 12 steps from the nodes' positions (`FORCE_STEPS`, `organic_stages(..., relax)`) and holds only the pins: a node that is dragged is held at the pointer (a gesture pins it), the others follow it like on springs, and a node that is let go (a drop without a pin) settles toward its neighbours; a node dragged by its pin stays where it is put. The repair afterwards has a smaller budget per layout (15000 trial scans instead of 200000: every step of a drag recomputes) and reports what it did not resolve. Deterministic. Measured on the Petri net with shapes (24 nodes, `crates/runtime/tests/force_directed.rs`, release build, Apple M3): a drag of `lock` by 300 units in four updates leaves its neighbours about 60 units farther away without the option, and the neighbours follow it with the option (tests `without_force_directed_the_neighbours_stay_behind`, `with_force_directed_the_neighbours_follow_the_dragged_node`); Interactive cost and stability (first version: 340 ms a step, the other nodes moving 100 to 290 units on average and up to 1450 in one step, 1.2 s at the drop with mean moves of 260). Changes: (1) the repair has three efforts (`repair::Effort`): a step of a drag (the earlier snapshot has an active gesture) repairs locally, with moves of one node spacing at most, a budget of 1500 trial scans and no spreading; the step that ends the gesture (`Gesture.ending`, set only on the copy it continues from) repairs moderately, with moves of up to two node spacings and 8000 scans; anything else repairs fully as before. A force-directed step of a drag does not repair at all, only reports the conflicts that stand (the forces decide where the nodes go and the drawing is a preview until the drop), and the repair never spreads a force-directed arrangement. (2) The relaxation is damped: a step moves the nodes 30 per cent of the way to the relaxed positions and at most a quarter of a node spacing (`FORCE_DAMPING`, `FORCE_MAX_STEP`), 8 relaxation steps (`FORCE_STEPS`). (3) A layout that continues an earlier one keeps its frame: it was translated so that the corner node sat at (24, 48) again on every layout, which moved the whole drawing at each drop; now it is translated only when a node would start before the margin (`organic_arrange`, also for the plain organic continuation). Measured with `profile_steps` (ignored test in `force_directed.rs`: ten drag steps of 41 units, ten at rest, the drop; Petri net with shapes, 24 nodes, release build, Apple M3; the machine was under a load average of 60 to 300 from other processes, so the times are indicative): a step 13 to 29 ms with the force (plain: 82 to 143 ms before this change's local repair settled, 33 to 47 ms on a quiet machine), the other nodes moving 21 to 34 units on average per step and at most 90 (they follow the dragged node, which moves 41); with the pointer at rest the drawing does not move; the drop moves the other nodes 63 on average and 254 at most (plain: 7.8 and 123; before: 1248), and what the repair could not clear (11 conflicts) is reported. The plain drag steps no longer jump either (largest move of another node in a step: 123, before 1470, because the spreading of the repair no longer runs during a drag). On a diagram with orthogonal routes (BPMN, 26 nodes) a drag step takes 1 to 3 ms: the routes are not the cost, the repair of organic areas was. Not done: reusing the graph distances and the stress state across steps (each step starts from the positions only and computes the distances again), re-routing only the edges of the moved node (all routes are made again), and a settling of the force-directed drawing over time while the pointer rests. Not done: continuing the solver state across steps (each step starts from the positions only), an animation of the settling between two requests, and the option for areas that are not organic. Not checked in a real browser |
| Circle containers | A container with `shape: circle` is a square box around a disc (`NodeShape` was a leaf property; containers use the same names now). Layout (`fit_disc` in `crates/layout/src/lib.rs`): the children are arranged by the area's own rule, then the smallest diameter (stepping by the placement grid) is found at which every child outline lies inside the disc less the geometry padding and below the header; the block of children is centred in the disc unless a child is pinned (a pin is a position in the container frame, so nothing is shifted and the diameter grows until the children fit). An authored size is honoured and a circle takes its larger side on both axes; children that do not fit are reported (`container_bounds`, `outside-area`; `validate_geometry` tests the disc). Repair: children of a circle container are bound to the disc (`Limits::Disc`), a container that follows its content grows to the smallest diameter that holds them (at most 1.5 times its first size). Ports and straight edges treat the container as a disc through the same outline rules as a circle leaf. Viewer: a disc with a ring-shaped move handle, the title centred at the top, and the pin, collapse and resize controls on the disc (`packages/viewer/svg.js`, `shapes.test.mjs`). The Shape selector of the playground offers a circle for a container. Tests: `crates/runtime/tests/circle_containers.rs` (children in the disc for organic and layered arrangements, move and resize, shape change both ways, fixed size too small is reported, the mixed fixture); `Outline::overshoot_beyond_disc` in `crates/model/src/outline.rs`. Border crossings (the derived ports where a route meets a container's border, `ports::boundaries`): for a circle container they lie on the disc for a segment in any direction, and for straight edges they are derived again from the drawn routes after the straight segments replace the orthogonal ones (they came from the replaced routes, and sat on the square around the disc); a box container gets them for oblique segments of straight edges only, so orthogonal outputs are unchanged. Test `border_crossings_lie_on_the_outline_of_the_container`. In a continuation of an organic area the disc scales about its old centre (`fit_disc_about_centre`): the children keep the places they are drawn at, the box grows or shrinks symmetrically, so a node dropped without a pin stays where it was dropped (0 to 1 unit away in four drops, no other child moves; test `a_released_drop_in_a_circle_container_stays_where_it_was_dropped`). A first layout, an arrangement that is made again, or a pinned child centres the block of children in the disc (`fit_disc`; a pin is a position in the container frame, so it keeps its place in the frame and the box grows to the right and below). The diameter follows the content in both directions: the engine keeps no minimum for it, and the playground does not remember a grown size for a circle container (it does for a rectangle), so the circle grows for a child dropped outward and shrinks when the child comes back (headless Chrome: 661, 1286, 661; engine test `the_circle_grows_and_shrinks_with_its_content`). Checked in headless Chrome: the demo `Mixed layouts` draws the disc, the layered containers and the drag modes. Not done: orthogonal routing and the refinement paths (`hierarchy.rs`, the container composition) were not changed or checked for circle containers; nested circle containers and the hit areas of their controls were not checked; a container that is dropped outward grows by about twice the drop distance (the top-left corner of the box stays) and the diameter shrinks again when the child returns |
| Areas with different layouts in one drawing | Direction and strategy are set per area (ADR 0003), so an organic root can hold a container that is arranged in layers (`lr`, `tb`) and the other way round. Since this change the whole path holds them together: the repair works per organic area; after the repair a container that grew for its children makes the area that holds it arrange again from the repaired boxes (up to two rounds; the last repair may not grow a container); a container grows by at most 1.5 times its first size over the rounds (`MAX_GROWTH`, `Memo::reference`), because a conflict nothing can resolve pushed the children out and the container after them without bound (sizes of 1e16 units); the spread stage is accepted only for strictly fewer conflicts (it compounded in every round); a repair of one layout may make at most 200000 trial scans (`SCAN_BUDGET`) and then returns the conflicts that remain, so an unresolvable drawing does not search for minutes (62 s before). A graph that sets a strategy anywhere keeps an explicit `none` of a container in its JSON (`written_slot`): it was dropped, and the container then inherited the organic root when the snapshot came back from the page (the engine refused every edit with `organic cannot take the fixed lr direction`; test `graph_roundtrip.rs`). Tests: `an_area_with_another_layout_than_its_surroundings_leaves_no_silent_conflict` (containers_straight), `the_mixed_layout_fixture_is_conflict_free_and_each_area_follows_its_own_rule`. Demo dataset `mixed-layouts` (organic root and circle container, a left-to-right and a top-to-bottom container). Measured: `benchmarks/results/containers-repair-20261025.md`. Not done: tree and radial areas inside organic ones (the generator's random edges are no forests); in a layered area a straight edge has no freedom, so an organic container inside a layered root reports the crossings that remain (1 to 13 per drawing on the random diagrams); a visible marking of conflicting nodes in the playground |
| Structured optimization in the demo (`structuredRefine` button) | Runs the common entry on the current view with the refinement's gate: the request carries the view's revision and layout version, and an applied result advances the layout version and keeps the graph revision. A stale or failed result leaves the diagram unchanged. The report names the approach, both candidates' keys (smaller is better), why other approaches did not apply, and each part's strategy. Same operation in split, browser and backend modes. Not automatic: the default refinement is unchanged |
| Common entry (`structuredRefine`) | Reads the structure and picks the path: components for a flat graph with independent components, the container path for one flat container, otherwise the joint layout. Paths that do not apply are reported with their reasons; each path competes with the joint layout under the shared key. Several or nested containers take the hierarchical composition (below); a result where the joint layout wins is reported as `joint`. The per-path operations remain as diagnostics |
| Hierarchical composition of several and nested containers (step 3, `hierarchy.rs`) | Each container's interior is laid out and refined as its own level, from the inside out; each container's size is fixed by its interior; boxes stay parent-relative. The composed drawing is routed by the regular router from the seed and compared with the joint layout under the shared key. Strict check: every route starts and ends on the absolute border of its nodes across all levels. Limited prototype: no negotiation of sizes or attachment sides; no per-container regional strategy choice yet; explicit ports refused |
| Recursive prototype for independent components (`componentRefine`) | Flat graphs: each weakly connected component is refined on its own with the strategies its plan allows, the components are composed side by side, and the composition competes with the joint layout under the shared key; the better one is returned. Refused for containers and explicit ports. |
| Container with a fixed boundary (`containerRefine`) | One container of leaf children: its interior is laid out and refined on its own, which fixes its size; the outer graph uses the container as a leaf of that size, the fixed boxes are composed and every edge, including the border crossings, is routed by the regular router from the seed (no stitched stubs). The boundary contract is stated in the module documentation. The composition and the joint layout compete under the shared key. Limited prototype: refused for nested or several containers and explicit ports; no negotiation of the boundary |
| Required clearance as the default (ADR 0005 amendment) | `requiredClearance` defaults to 6 and applies to foreign and own nodes; track separation runs with it; narrow corridors are widened from the drawn routes; overlaps left by the separation are resolved monotonically. Intentweave export (view of 40, clearance 6, debug build, one run): shared pairs 37 to 0, near-node pairs 10 to 0, crossings 82 to 86 (without widening: 10 shared pairs, 49 crossings). Stored fingerprints and benchmark results describe the opt-out and are not regenerated; the channel router is not used with the clearance. Provisional. |
| Required clearance sweep | Contacts at the preferred margin (12) vanish from clearance 7 upward on the 20 views, at +360 crossings (+3.9 percent) and +116k length for 7; see `benchmarks/results/clearance-sweep-20261023.md` |
| Deterministic work counters (legacy router) | `legacyCandidates` (candidate route evaluations) and `legacyGridPops` (grid-search pops) count the work the legacy router performs. Units are router-specific and not comparable with channel A* expansions. Output-neutral: the 17 stable benchmark views are identical; counts reproduce and survive an interruption (`benchmarks/results/legacy-work-counters-20261023.md`). An unmet required clearance is reported in `routingIssues` |
| Default objective order | Routed crossings are ranked before near-node pairs in the published key (ADR 0002). Measured: 30 fewer routed crossings over 20 views, 6 views change geometry, new fingerprint baseline `benchmarks/results/fingerprints-20261023-objective-order.jsonl` |
| Shadow quality score (M0 of [recursive-optimization.md](recursive-optimization.md) §13) | Prototype: `scene::quality` weighted evaluator, `layout::shadow_evaluation`, reported by `bench` as `shadowQuality`. Tier 1 (overlaps, edge-node hits) reported apart from the weighted total. Not used for selection; acceptance goes through `layout::improves` on the published conflict key. Weights are hypotheses. Candidate log (`bench --candidate-log DIR`, `refine_snapshot_logged`): every complete first-stage and round candidate with key, tier 1, shadow total, parent and work; logging is verified not to change the result, at one and four threads and under work budgets; comparison in `benchmarks/candidate_compare.py`. M1 slice (selection unchanged): strategy capability declarations (`Job::capabilities`) drive the round job list; a bounded archive (`archive_indices`, best K measured candidates, ties by log index) starts with the selected candidate. First-stage allocation under a budget runs the jobs that most often gave the best candidate first (learned on the tune views; judge views: budget 2 reaches the best on 6 of 10 views against 4 for the planned order; see `benchmarks/results/allocation-simulation-20261023.md`). A cost-aware policy waits for deterministic cost counters on the default engine. Unbudgeted selection is unchanged. Experimental, opt-in `routedCandidates` (off by default; mixed on the corpus) re-routes ordering candidates before the key compares them. Streamed publications are version 2: additive `publicationVersion` and `score` (shadow tier-1 violations and tier-2 total of the published result; version 1 publications have neither). The portfolio policy and selection are unchanged. Required clearance (`LayoutOptions.requiredClearance`): near-node proximity within the clearance becomes a hard constraint (candidates with any are not eligible; the shadow score counts them as tier-1 violations). Unset by default, so default output is unchanged. With a clearance, placement keeps gaps of at least 2 x clearance + track spacing and routing keeps routes a track outside it; on the 20 views the clearance is then met and the refinement works (cost about 11 percent more routed crossings, see `benchmarks/results/clearance-placement-20261023.md`) |
| LR/TB and inherited container overrides | Available |
| Hybrid | Local compactness choice between LR/TB; no global optimization |
| RL/BT as layout settings | Available as the mirror image of LR/TB on the rank axis, for the root and for containers (ADR 0003); edges keep their logical direction; back edges are not forbidden |
| Automatic and explicitly set endpoint ports | Basic function available; joint port/routing optimization missing |
| Orthogonal routing | Limited heuristic; no general obstacle avoidance |
| Routing constraints (ADR 0005) | Authored `defaultRouting` and `containerRouting` keep an explicit `orthogonal` (presence kept). Each edge takes its deepest common area's requirement; without one, the legacy option `edgeRouting` (optional, presence kept) applies, then orthogonal. A conflict between an authored requirement and the option is reported by name. Mixed drawings: straight edges are segments between their ports; orthogonal edges are routed around them. The orthogonal search pays for crossings with straight segments, including diagonal ones; straight segments stay fixed, follow moved ports, and are validated against nodes (a container is crossed only by edges that have an endpoint inside it; sibling containers stay protected; nested boundaries are crossed only where authorized, see the counterexamples in `styles::tests`). Choice (`auto`, lists with several drawn styles): one pass over the areas, the root first and then by area id; each area tries orthogonal, then straight, for its edges with the others fixed, and keeps the choice when the conflict key of the whole drawing is better (ties keep orthogonal; list order is no preference). This is a bounded sequential search, not a global optimum. Checked: a fixture where straight wins (`auto-straight-wins.json`), one where it would cross a node (`auto-orthogonal-wins.json`), and a two-area fixture whose automatic result equals the best of the fixed combinations (`auto-two-areas.json`). Playground: the routing control applies fixed, auto, allowed and inherit like direction and strategy. The channel router refuses mixed drawings by name at the planning step, so refinement cannot switch to it silently; all-orthogonal and all-straight drawings are accepted. Not done: the channel router does not route around straight segments; bundled orthogonal edges cannot be mixed with straight ones (refused by name); the outline contract beyond circles (ADR 0005 §5); the other styles are named and refused with the reason. `roundedOrthogonal` is drawn: the routes are the orthogonal ones, unchanged, and the scene marks them `rounded` (a fixed requirement, or a list that has it without `orthogonal`; `auto` and lists with both keep plain orthogonal). The viewer rounds the corners (`roundedPoints`, radius 12, shortened on short segments); a renderer that ignores the flag draws plain orthogonal routes. Not done: the corner radius is not an option, and the rounded corners are not checked against nodes. `polyline` is drawn too: the scene route is the orthogonal route pulled taut (greedy, over the route's own points; obstacles are all nodes except the holding containers, 4 units of clearance, circles as their boxes). The snapshot keeps the orthogonal route, so crossing, conflict and label metrics describe the orthogonal route, not the drawn polyline; the pulled segments are not checked against other edges, and a circle's corner gap is not used. `roundedPolyline` pulls the same way with 10 units of clearance and the viewer's corner arcs; the arcs are not checked against nodes beyond that distance |
| Contour-aware orthogonal routing (ADR 0005 §5) | The legacy search, the track improvement and the grid repair test each segment against the node's outline: a rectangle as before, a circle by its disc (inflated by the clearance, which grows the disc). Free corners of a circle's box are reachable: the search adds tracks just inside the box and the grid adds lines there. Containers: an edge crosses only the containers that hold one of its endpoints; sibling containers stay protected; nested boundaries are crossed only where authorized (routing tests). Automatic and configured circle ports leave the disc on their side (a configured arc spans 90 degrees). Required clearance keeps a route round a circle at the clearance. Growing a circle invalidates a route; the search after the change routes around it. The channel router routes around rectangles only and refuses circle outlines for orthogonal edges by name. Acceptance cases: `contour_acceptance` (routing), `styles::outline_tests`, `contour_port_tests` (layout). Not done: the channel router's outline search; polygons; a spatial index; invalidation beyond the recomputation |
| Move gesture (engine) | `move_node` (operation `move`): begin, update, finish (keep the pin, or release it) and cancel of a leaf node, with a gesture id and a sequence number; a stale step, a second gesture, or another node is refused. Positions are in the parent's coordinate space and pins are binding for the layout; each step is a full recomputation (no preview yet). Cancel restores the previous pin. Measured in `benchmarks/results/interaction-baseline-20261024.md`: every phase costs about one full recomputation; unrelated nodes move during a drag. Not done: the browser preview, coalescing of updates, local repair, spatial index |
| Geometry update (typed measurement) | The existing `measure` operation also takes `measurements`: for each element the realized outline (`Contour`: `rect` with width and height, or `circle` with its diameter, the same description as a node's shape) and a `geometryRevision`. It writes the same state as a plain size (`node_sizes`) and records the latest revision per element (`geometryRevisions`, omitted when empty). The whole request is refused when an element is unknown, an extent is not finite and positive, the outline is of another kind than the node's shape (the shape is a requirement; `shape` changes it first), or the revision is older than the one applied (a late measurement; the same revision again is accepted as a retry). Plain `sizes` still work and carry no revision. Contract types `Contour` and `Measurement` are generated (`packages/contracts`). The playground reports through it (`apps/playground/geometry.js`): a resize and the label measurement send the drawn outline with a rising revision per element. Checked in headless Chrome: two resizes of one leaf (180 -> 260 -> 220) are applied in order. Not done: reserved areas and clearances as part of the report, port constraints reported by the consumer, polygon outlines, and a measurement context that ties a report to the content it measured (the revision is per element only) |
| Shape change and leaf resize (engine and playground) | `shape` operation (`set_shape`): a node's outline (`rect`, `square`, `circle`) changes and the layout is recomputed from the current positions (warm start); a node already in that shape changes nothing, an unknown node is refused. Playground: the Shape selector of a selected leaf sends it; a press in a leaf's bottom-right corner resizes it, the dashed frame shows the size, and the release sends the size as a requirement (`measure`). A user-chosen size is not overwritten by label measurement. Checked in headless Chrome against the worktree server (circle redraw at the new size; resize frame during the gesture). Not done: the resize of a container in the engine mode, and an undo for these operations; the layout tests cover only the engine side (`crates/runtime/tests/shape_change.rs`, `shape_geometry.rs`) |
| Browser move preview | The playground drags a leaf: the node and the ends of its routes follow the pointer at once (a local preview, drawn at the pointer, not at the engine box). The engine gets one move step at a time (`apps/playground/drag.js`, `MoveGesture`): one request in flight, the newest position pending, earlier positions coalesced, steps numbered; a finished gesture keeps or releases the pin; Escape cancels. A refused step ends the gesture and the drag state resets. Counts (sent, coalesced, applied, discarded) are shown in the status line. Circles are drawn as discs. Not verified in a real browser here: the pointer path runs in the page only; the controller and the renderer are tested (`drag.test.mjs`, `shapes.test.mjs`). Not done: the local preview of crossings and labels; the curation editor still computes manual route geometry in the browser (`editor/geometry.ts`), a second geometry authority against ADR 0008 |
| Container transitions | Derived intersection points; binding boundary ports missing |
| Expand/collapse | Available for the selected section; complete view recalculation |
| First view within a node budget (experimental, 2026-11-01) | Option `visibleNodeBudget`: the import collapses containers so that at most that many nodes are visible, the outermost first, then expanding level by level while the budget holds (`crates/runtime/src/budget.rs`). A collapsed container is drawn as a miniature (a quarter) of the size its content would take in its flow (`collapsedSizes`, derived by the runtime); the edges it aggregates are drawn once per visible pair and direction (`view::merge_aggregated`), edges between visible nodes stay as they are. Playground: "Node budget" field (an import). Measured on the whole layerweave-code graph (669 nodes): first view 0.5 s at budget 50 against 117 s for all nodes (`benchmarks/results/global-layering-iteration1-20261101.md`). Expanding a container within a budget is local (2026-11-02): the earlier drawing is continued, the areas around the container keep their children's places and push what lies right of or below it on by its growth, and only the routes whose ends did not move together are routed again (layerweave-code, budget 50: 0.1–0.6 s against 0.1–5 s for a new layout; `benchmarks/results/local-expansion-20261102.md`). The playground marks the edge count of an aggregated edge. Not done: a viewport with one hop beyond it, local collapse |
| Post-optimization and version check | Available for bounded views |
| Conflict metric v3 and placement search | Shared metric for all engines; candidates are compared via the published conflict key; cross-check between engine and metric. Local channel candidates run through `search_placement_counted` (`local_channel_candidate`, `crates/layout/src/lib.rs`) and fall back to full assembly when they cannot be proven; they are not a separate acceptance rule |
| Staged area expansion, patch transport, computation cancellation | Not yet fully implemented |
| Grid, rounding, free curves, bundling | Open |
| Cut-out, claims/KG overlay, large views | Open |
| Execution modes shared, browser-only, backend-only | Available in the demo; all operations run through the same dispatch interface |
| Parallel candidate evaluation | Native threads in the runtime, also with a stop callback (candidates not yet started are skipped; running candidates stop at their checkpoints); result independent of the thread count; candidate IDs and work counters in `runtime::execution`; WASM serial |
| Native Node connection | Placeholder |
| Scene geometry | In the Rust crate `scene` (operation `scene`); viewer adapters are pure mappings |
| Profiles (config sets) in SVG demo and React Flow reference | Three example profiles; selection via `configure` |
| Container borders in the channel grid; strict clearance with corridor widening (experimental) | Container borders are track lines, so routes run around foreign containers through the corridors outside them. Strict clearance (`strictClearance`, default off) forbids a foreign node's clearance band, except for the endpoints of the edge itself. Under strict clearance, each corridor between consecutive siblings is widened to 2 × clearance + (k − 1) × track, where k is the number of unconstrained routes through it, and each widening is reported in the routing issues. On workflow-dag, strict with widening against default: diagonal fallbacks 9 against 0, routing issues 0 against 0 (strict without widening: 135), hugging 98 against 395, crossings 888 against 948, shared pairs 313 against 375, length 245146 against 226465, width 14848 against 13516. Other inputs keep fallbacks: layerweave-code 37 diagonal and 2 issues against 0 and 1 by default; org-chart 38 against 0; network-vlan 14 against 0; uml-class 4 against 0. A reserve track per corridor (k instead of k − 1) removes the workflow-dag fallbacks but adds issues on layerweave-code (7), so it was not adopted. Not done: track separation, and the cause of the remaining fallbacks, which are not all explained by corridors. Only the default output is byte-identical; widened layouts are a different layout
| Node pins (leaf and container positions relative to the parent) | Stored, validated and honoured by placement; a free sibling that crowds a pin moves to the nearest clear spot. A pinned container keeps its place in its parent's frame and its children are still arranged inside it (`crates/runtime/tests/pins.rs`: through a direction change and refinement, nested too); a move gesture on a container is refused (moving a container is ADR 0009, section 4). The playground pins and unpins a node or a container from a pin control on the canvas and from the selection panel; the viewer draws the controls only when the host handles them (`packages/viewer/controls.test.mjs`, real-mouse check `browser-checks/canvas-controls.mjs`). The placement search keeps free nodes clear of pins, because a candidate with a geometry issue is never accepted; pins on nodes outside the visible view are reported only as routing issues |
| Container sizes (ADR 0010, step 1) | Per container and axis: content (default), a minimum or a fixed size, stored only when set; invalid values are rejected by `configure`; a patch changes one container and keeps the others. Minimum grows the container and its ancestors. A fixed size below its content keeps the size and is reported as the geometry issue `container_bounds`. Composition is required when a size is set. Also applied by the plain placement, so the background refinement shows them. A fixed size below its content is named as `container-fixed-below-content` with the size it needs, and kept; the layout search accepts it (`is_ineligible_geometry`), and overlaps that the box causes inside it are authored too. A pinned child that leaves its container enlarges it. Not yet: a fixed box that the interior is laid out into |
| View selection on import | The import takes the connected neighbourhood of the most connected node; when it is exhausted before the limit, it continues with the next most connected node, so a graph of several components fills the view. The limit is at most 10,000 (`MAX_VIEW_LIMIT`), and the playground's examples use 10,000. A graph in one component is selected as before. The stored fingerprints of graphs with several components (er-diagram, uml-class, workflow-dag) no longer match and need a rerun of the benchmarks |
| Containment tree (ADR 0010, step 2) | Read-only tree in the playground sidebar, built from the scene's IDs (`packages/containment-tree`). Rows show size states, pins, conflicts and invalid geometry. A row selects its node and a node selects its row; arrow keys move between rows. Collapse and expand, the size of a container, and its direction, strategy and routing are set from the tree, each with its provenance (ADR 0010, step 3). Each of direction, strategy and routing offers inherit, `auto`, the fixed values and an allowed set (ticks, sent from two on); the whole view has the same form as the tree's "View" row (the first row), where only routing can stay undeclared. Containers and collapsed containers also have collapse and expand controls on the canvas. Dragging a node past its container's border stores the grown extent as the container's minimum (ADR 0010, step 4); a fixed axis stops the drag at its border. A container with an authored size has a reset control per axis on the canvas. Ctrl/Cmd+Z undoes the last container size change. A resize handle at a container's corner sets its minimum. A container can be moved by its header, and its children move with it; the engine accepts container moves (`move` op). Not yet: undo for pins and moves, and a resize that sets a fixed size |
| Curated references: evaluation modes and corpus (C3) | `referenceReproduce` (modes `constrained` and `unconstrained`) and `referenceRepair` (recorded node moves, rerouted, measured with the movement elsewhere). `benchmarks/references/` holds six cases with hashes; `cargo test` checks them without a browser; `evaluate.py` writes a results file with metric deltas per mode. Six cases only; all still draft, none accepted by human review; no versioned remeasurement across metric versions yet |
| Curation editor: containers grow on demand | Opt-in checkbox. A node dragged past its container's right or bottom border grows the container (and ancestors) in one undoable step; the top-left corner stays. Off by default: a node is then clamped inside its container, as before. Routes along the grown borders become pending. Growth that would newly overlap an unrelated node is refused with a message |
| Objectives `ProjectedCrossings` and `EndToEnd` in the published key | Use the same values as the quality comparison (they were constant 0 in the key), and end-to-end values are quantized to hundredths in both. `ProjectedCrossings` is in the default priority, so candidate selection now sees it. Default output is byte-identical on the 19 fingerprint configurations. Refinement time, alternating runs on identical inputs (not idle-controlled, see `benchmarks/results/objectives-timing-20261021.json`): the objectives change alone costs about 2%; with the sweep for projected crossings the working tree is about 1.5% faster than before the change |
| Objectives as playground controls | Settings group "Objectives (priority order)": tick to use an objective, arrows to change its priority, at least one stays active. The UI regression test covers adding and moving one; a real-browser check has not been done |
| Curation editor: container translation and port side change | Dragging a container's header moves it with everything inside: members and routes wholly inside translate once; routes that cross its border or attach to a moved node become pending. Dragging an automatic port clearly past its node moves it to that side; explicit ports stay locked. Derived positions come from the engine's refresh. Alt-click on an interior segment adds a dogleg (a jog of 36 units in the middle third; endpoints and arrival sides stay). Resize by dragging the bottom-right corner: a leaf never goes below 60 x 30 (an engineering minimum, not a measured label size), and its routes become pending; a container changes its boundary only and never cuts into the boxes inside it. Route locks (Shift-click a route): a locked route keeps its geometry; a change that would re-route it is refused with a message, and a pending route cannot be locked. Size pins (Shift-click a box): a pinned size is kept through resizing. Both are stored in the reference (`lockedRoutes`, `sizePins`, omitted when empty, validated structurally). The engine does not honour size pins yet, so constrained reproduction does not check them. Selection and inspector: a click selects a node or a route; Ctrl or Cmd-click adds nodes to a selection, and a drag moves the whole selection in one step; Escape or a click on the background clears it. The inspector lists the facts of the selection (box, container, ports, routes with lock and pending state, size pin) and offers lock and pin buttons. Selected nodes are outlined; selected routes are highlighted. Not yet: cycling through overlapping objects, keyboard navigation of the canvas, conflicts in the inspector. A headless Chrome smoke check (DevTools protocol, run by hand, not part of CI) passed for draft creation, click and Ctrl-click selection, Escape, a note shown in the inspector, the pin button, and no page errors. In the same manner, real mouse input passed for a node drag, a corner resize, and Shift-click lock and unlock of a route; a locked route refused a node move with its message. Also passed in headless Chrome with real mouse and Cmd (Meta) input: Shift-click size pin and unpin, Alt-click dogleg, segment drag, container header drag, port drag, label box drag, and a joint selection drag. Not checked: a port side change to another side (the check confirms only that a port moved), the Ctrl-click path on macOS (Ctrl is a right-click there), and visual appearance. Labels and notes: a label box is dragged to a new position, kept in the reference (`labels`, the top-left corner per baseline label id; the measured size stays). Notes are curation evidence in `annotations`: text on a node, a route or a label, listed by the inspector; they never change geometry. The reference validates label ids and note targets and rejects empty notes. Label boxes are not measured by the metrics yet, so moving one does not change the conflict counts, and it is not checked against nodes. Notes may carry a category (the section 5 list) and a severity (info, warning, error); both are optional and validated. Bends are edited by shifting their adjacent interior segments; a bend point cannot be moved on its own. Re-routing: pending routes are re-routed automatically about 0.4 s after the last change, in the engine worker; the status line shows the run and its time. Edits queue behind a running re-route instead of being refused; an edit computed from an older document is refused, never applied over newer work. A failed re-route is not retried until the curation changes again. The reference route search indexes the fixed segments per grid line and computes each grid edge once; its output is unchanged (fingerprints compared before and after). Measured on the benchmark datasets, same machine, single runs: re-routing all routes of org-chart (82 routes) went from 105 s to 4.4 s; re-routing the 4 pending routes after one move went from 4.5 s to 0.35 s. Live preview while a node is dragged, in two tiers: the browser draws a simple orthogonal shape for each affected route on every pointer move (dashed, not valid); the engine's real routes for the moved position follow, one request at a time, and only the newest position is drawn. A release commits the move, or is refused with its reason. A route that has no path after a move stays pending and is named in the status; the other routes are still re-routed. Not yet: a windowed grid for large diagrams, and a warning while a drag would place a node on top of another. Not yet: pairwise judgments and provenance (section 5), derived experiments for explicit constraints |
| Configuration presets, target priorities | Open; see section 10 |
| Bundling (full/partial), edge weight | Full bundling implemented (`Bundling::Complete`: merges edges with identical source and target via route weight; skipped for edges with explicit port bindings and for local channel repair). Explicit buses (provisional `buses` in the graph, ADR 0005; a backbone takes part in the layout as a virtual node with a rank of its own, kept apart in `busBoxes`, amendment B): edges with one shared end are drawn on one trunk with branches, edges with several sources and targets on one backbone line across a channel they all pass (or one line per channel group when none is common), with one stub per node; members may start or end at components inside containers and keep their way inside, and collapsing an ECU keeps the bus with the ECU as the end; a bus may carry a label placed at its trunk; members may share lines and meet, other edges keep apart, the scene carries junction dots and the viewer draws them, and what cannot be drawn so is reported; orthogonal routes only. Example: `can-bus` in the playground ([results](../benchmarks/results/bus-20261108.md)). Input edge weights (`Edge::weight`, provisional): an edge that stands for several counts as many in a bundle, in a route for collapsed content (both directions), in the filter's recount, in the overview's links (and so its communities, satellites and names) and in the names of communities; without weights the output is byte-identical (fixtures compared), and weights move nothing (tests in `edge_weights.rs`). Partial bundling remains open |
| Size feedback from the visualization | Open |
| Direction strategy with shortest end-to-end path | Open |
| 100K performance evidence | Open |
| Gestures that end badly (2026-10-27) | A drag, a leaf resize and a container resize listen on the window from the press to the end (`apps/playground/gesture.js`): only a release commits; `pointercancel`, a lost capture and Escape abandon the gesture and restore the engine's state, a model change abandons it and ignores its replies, and background refinement does not start during a gesture. `browser-checks/gesture-robustness.mjs` checks a release outside the window, `pointercancel`, Escape, an engine error and a model change in a real Chrome (the `pointercancel` and Escape checks fail against the previous `app.js`). Verified on `workflow-dag`; not on the 583-node model and not zoomed. |
| Measurements that change nothing (2026-10-27) | A measurement that carries only a newer geometry revision, or a size within `MEASURE_TOLERANCE` (4 units) of the size in use, lays nothing out again and does not advance the layout version (`recompute_with`, `measure_contours`). A pin or a requested size lays out only the node's area and the containers above it, also in layered graphs (equal to a complete layout on layered fixtures; the corridor widening re-places everything on large models, see `benchmarks/results/interaction-20261027/README.md`). |
| Warm start, reused analysis, continued solve (2026-10-27) | Three different things, named alike before. **Warm start**: a layout that continues an earlier one begins from the earlier positions (organic areas keep the unedited nodes). **Reused analysis**: the graph distances of an organic area are kept for the next call with the same children and links (`organic::distance_memo_stats`, one entry per thread; a hit gives the value a new search gives), and are not computed at all when a warm start holds the positions. **Continued solve**: further damped stress steps (`forceDirected`, at most `FORCE_STEPS` per call); this is not a separate force simulation and not an exact continuation of an earlier solve (no solver state is kept in the snapshot). |
| Router speed and local edits on large views (2026-10-27) | The repair router's grid search tests only the obstacles near a step, through a uniform grid of their boxes (output unchanged). An edit that lays out only part of the drawing (a pin, a requested size, a drag step, a size report, a new node) also keeps the routes it does not touch, carries the routes of edges whose ends moved together, widens only the corridors it touched from the widths the earlier drawing had (`Snapshot.corridorGaps`), and counts the routes through nodes before and after: if the result has more than before, a step of a drag is reported as a preview (`editReport.preview`, a routing issue says so) and any other edit falls back to the complete layout when that has fewer (`editReport.completeFallback`). `editReport` also counts the routes kept and made again. Contract of this path: deterministic, constraints binding, unaffected routes stable; not equal to a complete layout. On `layerweave-code` (599 nodes) the import went from 65.4 s to 4.1 s, and a drag step, pin, size change or new node from 65 s to 0.27 s (1 to 3 of 60 areas; 0.47 s before the string lookups were taken out of the hot path, [ADR 0012](adr/0012-local-edits.md)), on one machine under load. After a drag with force-directed behaviour, `settle` runs at most three damped steps per call, reports the largest move (`editReport.settle`) and the page calls it at most three times (`browser-checks/settle.mjs`: on `mixed-layouts` it ends refined, not converged, after nine steps). Not done: the stop signal and work budgets inside the placement (the core runs without a clock; a host that needs to interrupt uses a worker) and the same local routing for the channel router. The 120,000-cell limit of the repair search used to be a cliff for this model (a pin at a moved place gave over 3,000 node hits); above it the search now works on a window around the route, and the node hits of the code model are 0. Measurements and findings: `benchmarks/results/interaction-20261027/`. |
| Publications, stale results, keyed drawing (2026-10-27) | A streamed publication is a typed `PublicationEnvelope` (job, sequence, `kind`, input identity, snapshot, score, archive; `publicationVersion` 4) in the generated contracts. The page counts results that arrive for a changed view or a superseded job and shows `Stale results discarded: N`; its publication policy (`session.js`) and the Rust one (`session.rs`) run the same nine cases from `examples/fixtures/session/cases.json`. `renderSvg` can compare its drawing with the host element by element (`keyed: true`), but on 548 nodes that is about eight times slower than drawing everything (480 to 550 ms against 59 to 63 ms to the preview), so the page does not use it. Slim move requests are not built (the engine boundary is a JSON string; see `benchmarks/results/interaction-20261027/README.md`). |
| Drawn route geometry (2026-10-27, partial) | The corners of rounded routes are computed in the scene (`crates/scene/src/realize.rs`), checked against the nodes (the radius shrinks, the style stays), and the viewer draws the points as given. The published drawing (key, metrics) of a view with a routing style is the scene's corner geometry. Not done: the reference export (`reference` SVG) and the browser check of rounded routes. |
| Self-loops (2026-10-27) | A self-loop is no longer refused under straight routing: it follows the loop rule (never drawn straight, drawn as a loop in the other styles its area admits, else orthogonal) while the ordinary edges stay straight. On a circle the two ends lie on different points of the border; the loops of one node are turned against each other and never bundled; tree, organic and radial areas draw several loops. The rule is behaviour, not an option: it has no switch or entry in the options yet. |
| Conflicts, setting origin and catalogue (2026-10-27) | `Scene.conflicts` lists every conflict with its kind, nodes, routes and reason (geometry issues and routes through nodes, `crates/scene`); the viewer marks every named node and route red with the reasons as the hint, the tree row lists each conflict that names the node, and a panel lists the conflicts (30 shown, with `N of M shown`) and selects the parties on click. `Snapshot.areaOrientations` records what a free direction chose per area; `settingsOf` adds the origin (fixed, auto, allowed, inherited, default) and the chosen orientation, and a value the engine refuses is shown at its own field with the engine's reason. Optimisation never overrides explicit requirements: `crates/runtime/tests/explicit_constraints.rs` checks that a pin, a placed port and a fixed container size hold in every publication of a refinement, in its result and in the structured optimisation, and that direction, strategy and routing set through `dispatch` are the accepted configuration. Finding: the container, component and hierarchy prototypes did not read pins or authored sizes (`containerRefine` moved a pinned node); they now refuse such a view, as they refused ports. Catalogue: `mixed-large` (96 nodes, nested, organic and layered, circles; the engine reports 75 remaining conflicts, mostly straight edges through nodes), `self-loops`, `fixed-ports`; `index.json` has `description` and `source`, and the page shows them with the visible and total node counts. Checked in headless Chrome: `conflicts.mjs`, `settings-origin.mjs`, `catalogue.mjs`. Completed afterwards: `Snapshot.areaStrategies` records the strategy the structured optimisation (route `hierarchy`) chose for each area whose strategy is not fixed (`auto` or an allowed set), and the settings line names it ("chose organic"); a plain layout records nothing, so a stale choice cannot outlive its drawing (test `the_strategy_an_auto_area_chose_is_recorded_only_by_the_search`). The shape of the selected node is chosen in its tree row (`select.tree-shape`; a container offers a rectangle and a circle). The example line says when the import is cut by the limit. Not done: the choice of the other optimisation routes (components, container) is not recorded; a plain layout still draws `auto` as `none` |
| Undo and redo (2026-10-27) | The engine defines the user's decisions as an `Intent` (pins, sizes, shapes, collapsed containers, direction, strategy, routing, ports, label positions) and offers `intent` (read) and `restore` (apply in one recomputation, stale revisions and unknown names refused). The page keeps the stack (`apps/playground/history.js`, 50 entries, one per action, cleared when a node is added or a graph is loaded); Ctrl/Cmd+Z and Ctrl/Cmd+Shift+Z or Ctrl/Cmd+Y. `browser-checks/history.mjs`. Not done: `requested_sizes` apart from measured sizes, so a restore also restores measured label sizes. |
| Direct view and the legacy draft (2026-10-27) | Move (also a selection moved as a whole, `others` in the move step), resize, pin, collapse, shape, direction, strategy, routing, placing a port by dragging it along the border (`port` operation), placing a label by dragging it (`labelPositions`), undo and redo, and saving a reference from the view all run through the engine in every mode. Opening a reference without curated parts opens it in the direct view. Not available there, and listed in the page: moving a route segment, doglegs (Alt-click), route locks (Shift-click), size pins; a reference that holds them opens as a legacy draft, which still edits them. The backend allowlist now matches the dispatcher (a test keeps it so). Browser checks: `selection-move.mjs`, `port-drag.mjs`, `label-drag.mjs`. The fixed-size border clamp of a drag applies to the grabbed node only. |

## 9. Next implementation steps

Single pass is the default (2026-11-08, ADR 0016 accepted): `singlePassAuto`, `singlePass`
and `abstractRouting` are on by default, `ENGINE_VERSION` is `0.2.0-prototype`, and the new
reference fingerprints are `fingerprints-20261108-single-pass-default.jsonl`.

Single pass against the default gate (2026-11-08): over five seeds single pass is better
than the refined default on all 16 datasets the automatic choice takes (11 mixed, 5 of one
axis). Explicit ports on their flow side are constructed, and areas that choose their
direction are constructed in the flow they chose. Timing: 70 / 12 / 10 ms on the frozen
views against elk-rs 43 / 8.4 / 6.9 ms; on mixed views 3–140 ms against the default's
18–5173 ms. See [gate](../benchmarks/results/single-pass-gate-20261108.md) and
[timing](../benchmarks/results/single-pass-timing-20261108.md).

Single pass, mixed axes in the automatic choice and self-loops (2026-11-08):
`singlePassAuto` takes containers of mixed `lr`/`tb` axes (16 of 31 datasets on single pass,
15 fallbacks, all with organic, tree or straight areas or a view budget). Self-loops are
nested at their node's freest corner like the default's instead of being drawn as seed
routes. See [auto and self-loops](../benchmarks/results/single-pass-auto-mixed-20261108.md).

Single pass, idle limit of the hierarchical sweep (2026-11-08): four idle sweeps instead of
one. On the 11 mixed datasets over five seeds every view is now better than the refined
default (uml-class 8.6 → 6.6 crossings, default 8); uniform crossings 9093 → 8566. The
time cost is not yet measured on a quiet machine. See
[idle](../benchmarks/results/single-pass-idle-20261108.md).

Single pass, edges through a middle area (2026-11-08, ADR 0017 amendment A3): an
edge from an `lr` area inside a `tb` area inside an `lr` area (or the other way round)
leaves the inner area by a border node of its own flow and crosses the middle area
straight. Mixed datasets over five seeds: crossings 4623 → 3862, bends 3741 → 3422;
10 better and 1 worse than the refined default (uml-class by 0.6 crossings);
cloud-architecture is now better than the default. See
[A3](../benchmarks/results/single-pass-a3-20261108.md).

Single pass, side nodes beside their leaf (2026-11-08, ADR 0017 amendment A, A1–A2):
where a leaf stands outermost in its rank, its side node stands beside it and the edge
leaves the leaf straight across the area's flow to the side port. On the 11 mixed
datasets over five seeds crossings fall from 4845 to 4623 and bends from 4329 to 3741
(default 5135 / 3110); against the default 9 better and 2 worse, as before. Middle
areas (A3) are open. See [lateral](../benchmarks/results/single-pass-lateral-20261108.md).

Single pass, port order along chains (2026-11-07): the edges of one chain left their
leaf in the order of their ids, because the leaf's port looked at the chain's next
element (a dummy) instead of the container at its end; on cloud-architecture they
crossed each other on the chain. Ports now follow the far end, and the placement gets
the side ports' order for children of the other axis. Mixed datasets over five seeds:
9 better and 2 worse than the refined default (cloud-architecture 36.2 / 31 crossings,
uml-class 8.2 / 8); deployment-cluster went from 141 to 113. See
[gate](../benchmarks/results/single-pass-gate-mixed-20261107.md).

Single pass, rounded orthogonal routing (2026-11-07): `roundedOrthogonal` keeps the
orthogonal route and only marks it rounded for the scene, so single pass (explicit and
automatic) now takes it, and lists of only `orthogonal` and `roundedOrthogonal`, like
plain orthogonal routing. Other styles (straight, polyline, auto) still fall back.

Single pass, mixed datasets against the default gate (2026-11-07): with the new
provisional option `orderSeed` (omitted at 0, default unchanged) the evaluation
averages over seeds (`--seeds`). Over five seeds, single pass is better than the
refined default on 8 of the 11 mixed datasets and worse on cloud-architecture,
deployment-cluster and uml-class (by crossings). The automatic choice therefore keeps
mixed axes on the other pipeline. See
[gate](../benchmarks/results/single-pass-gate-mixed-20261107.md).

Single pass, joint order and side port order (2026-11-07): the realization's joint
order no longer blocks every edge on and behind a cycle of orders; it selects again
with a cycle check and fails at most the edge it cannot place. Leaf ports on edges
through side nodes now take the order of the side ports, which removed crossings of
an edge group with itself: mixed crossings 4901 → 4850, org-chart 119 → 70. Giving
the placement the side ports as lanes would save 284 bends but makes workflow-dag
worse; it needs channel planning that keeps such lanes apart. See
[bends](../benchmarks/results/single-pass-bends-20261107.md).

Single pass, border alignment from the top down (2026-11-07): each area is placed
on its own, so an edge through several container borders changed lane in every area
it passed. After the placement, a border node whose edge changes lane on both sides
of the border moves onto one of the two lanes, within its rank's gaps. On the uniform
task bends fall from 6690 to 6240, below elk-rs (6290), at equal crossings; the
frozen workflow-dag goes from 448 to 300 bends (elk-rs 434). Default output is
unchanged. The remaining gap needs the parent to place a container where its border
nodes meet their lanes, and alignment through side nodes. See
[bends](../benchmarks/results/single-pass-bends-20261107.md).

Single pass, port offsets through side nodes (2026-11-07): a leaf's port on an edge
through a side node found no chain and could share its place with another port. With
that fixed, every edge of 8 of the 11 mixed datasets is constructed, the crossings fall
below the default's in sum (4925 against 5185), 9 of 11 views are better than the refined
default, and the synthetic-2k import drops from 483 to 196 ms. Bends remain 44 % above
the default's. See [legs](../benchmarks/results/single-pass-legs-20261107.md).

Single pass, side nodes in their children's order (2026-11-07): crossings over the
11 mixed datasets 7137 → 6004 (default 5185), 6 of 11 better than the refined
default; workflow-dag 1526 → 771 (default 610). The rest is a limit of the port
model: a leg leaves a cross-axis child only after its child's rank. See
[legs](../benchmarks/results/single-pass-legs-20261107.md).

Single pass against elk-rs (2026-11-07): on the uniform task single pass is better
on 21 of 31 views (crossings 4167 / 6584), worse on bends (6124 / 5620) and hits
(95 / 0); on the frozen code view and org chart elk-rs is better. elk-rs fails on all
11 datasets with mixed lr/tb containers. See
[elk-rs](../benchmarks/results/single-pass-elk-20261107.md).

Single pass, legs across the axis border (2026-11-07, ADR 0017): an edge out of a
container of another axis is built in legs, joined at a port on a side node of the
container. On org-chart 76 of 82 edges are now constructed (0 before), on
workflow-dag 87 of 125. Against the refined default 4 of 11 mixed views are better;
shared lines and hugging fall, crossings and bends rise, because the order across
the border is not yet crossing-aware. See
[legs](../benchmarks/results/single-pass-legs-20261107.md).

Single pass, mixed axes, first slice (2026-11-07): each area grows along its own
flow, and an explicit single pass lays out views with mixed `lr`/`tb` containers.
Edges across an axis border are still delegated (on org-chart and workflow-dag all
of them), so it is worse than the default on 8 of 11 such views, and the automatic
choice keeps them on the other pipeline. Next: construct those edges by leaving a
cross-axis child on its parent's flow sides. See
[mixed axes](../benchmarks/results/single-pass-axes-20261107.md).

Single pass, drag as an order hint (2026-11-07): `orderHints` keeps a node after a
chosen sibling in its rank, on the default's hierarchical pipeline and under single
pass. A released drag on a single-pass drawing stays on single pass and turns its
drop into such a hint; a pin still leaves single pass, and a change of rank by a drop
is not done. See [drag](../benchmarks/results/single-pass-drag-20261107.md).

Single pass, automatic path choice (2026-11-07): `singlePassAuto` falls back to
the other pipeline with a diagnostic instead of failing, so every edit the default
accepts works, in all three modes. All 26 rejected datasets are byte-identical to
the default apart from the option and its report. A drag and a pin hold, but they
leave single pass for the rest of the session, because single pass has no pins.
Supporting them in single pass is the next open point for deliberate moves. See
[automatic path choice](../benchmarks/results/single-pass-auto-20261107.md).

Single pass on the real task (2026-11-06): 26 of 31 unmodified datasets are
rejected (11 mixed axes, 12 without a ranked area, 3 with mixed regions). On the
normalized corpus it is better than the refined default on 19 views and worse on
12, 7 of them tree or organic views that do not use this path on real input. A
drag (`move`), pins, a direction change and straight routing fail on a single-pass
drawing, and shape, measure and toggle move other nodes further than the default.
Next: automatic path choice and edit coverage. Gate: [ADR 0016](adr/0016-single-pass-default-gate.md),
results: [real task](../benchmarks/results/single-pass-raw-20261106.md).

Single pass, levers (2026-11-06): each sweep run now ends at its first sweep
without improvement. Over 34 views and five seeds crossings fall 4.3 % and bends
0.7 %, the frozen code view gets worse (+4.3 %; 3830 crossings with the fixed
seed), and instructions per import move by −8 % (workflow), −7 % (org chart) and
+2 % (code). The children walk in the channel growth is output-identical and
saves 3 % on synthetic-2k; a track-line index for the occupancy check of a route
saves 7.2 % there and 1.4 % on the code view, and lookups instead of linear searches
for ports and channels save 18 % there and 3.3 % on the code view; a reach filter in
`ports::boundaries` saves 6.5 % and 1.5 %. String-keyed maps and allocation are about 43 % of
the code-view profile and no listed lever addresses them. In one process the frozen
code view takes 59 + 13 ms (elk-rs 44, load 1.5–2.1), on the simplified task and at
more crossings than elk-rs. See
[levers](../benchmarks/results/single-pass-levers-20261106.md).

Single pass, construction instead of reroute (2026-11-07): the phase work had
missed that the conflict reroute still ran after construction (21 % of the
code-view import). Its conflicts came from delegated edges drawn as seed routes.
The channel growth now measures the joint bend compaction's demand, the cycle
break leaves no link inside a rank, and self-loops on adjacent sides go around
the corner within the free room. Nothing is rerouted on the frozen views; the
code-view import retires 17 % fewer instructions (1004 → 831 M), the 33-view
seed mean keeps its crossings (6642 → 6644) with 1.7 % more bends, fewer hits
and issues; the frozen code view is 3.3 % worse in crossings. Next: skip the
joint demand pass where no channel can be short (workflow +4 %), the remaining
delegated self-loops, then the placement pass and the sweep. See
[no reroute](../benchmarks/results/single-pass-construct-20261107.md).

Single pass (2026-11-07): whole phases now run fewer times. The reversals are
known before the first placement, which only orders; one placement computes
geometry; five sweep runs instead of eight; the chain probe's track assignment
is not repeated when it derives no orders; the refinement measures its input
once. The import of the code view retires 30 % fewer instructions (1435 →
1004 M), the 34-view seed mean improves (10487 / 8545 → 9919 / 8559 crossings /
bends), the frozen code view is slightly worse (3322 → 3439 crossings). (The
claim that nothing was rerouted after construction was wrong; see above.) Next, in the order of the profile: the
geometry placement pass (28 % of the import), the sweep (14 %), the chain
probe (11 %), the channel growth that realizes the plan once more to measure
demand (8.5 %); the gap between model and drawn crossings in the root
channels; the 13 delegated edges and 10 unresolved strict track pairs of the
code view; and a timing repeat on a quiet machine. See the
[phase work](../benchmarks/results/single-pass-phase-20261107.md).

Single pass (2026-11-06): the swept hierarchical order now reaches the
placement unchanged, and coordinates across the flow come from Brandes–Köpf
blocks, which removed most placement-forced lane changes (code view bends
2092 → 1586, elk-rs 1638). All 31 corpus views pass in every arm; single pass
sums 6667 crossings and 6410 bends against elk-rs 7472 and 5912. Next, close
the crossing gap in large containers (code view, deployment-cluster, org
chart) and the bend gap in tree-like views, construct the delegated edges and
the unresolved strict track pairs (11 in the code view) instead of leaving
them, and investigate the line–node hits that every Arqlet arm shows on
shaped nodes and self-loops. Refinement no longer reroutes a single-pass
drawing; in one process the code view took 156 ms on a loaded machine (124 ms
quiet, after the work below) against elk-rs 47–50 ms,
spread over placement (34 %), abstract realization (27 %), ordering (22 %)
and the reroute of edges through nodes (9 %). Output-identical work on lookups,
allocations and the crossing count then removed 20–25 % of the import's retired
instructions; the remaining profile is flat (no source line above 4 %), so the
next steps are fewer executions of whole phases (sweep runs, placement passes,
the conflict reroute), accepted by the quality gates. See the
[results](../benchmarks/results/single-pass-blocks-20261106.md) and the
[timing](../benchmarks/results/single-pass-timing-20261106.md).

Hierarchical performance update (2026-11-04): run-local structural/rank,
ordering and cross-flow plans are reusable, and full-view placement skips
unchanged regional coordinate realizations. Next, measure dependency-check
and key-building overhead before replacing the remaining hierarchy traversal
with subtree scheduling. Constructor-internal search and compatible metric
reuse remain separate open costs; retain the candidate portfolio and verify
output fingerprints. See the [performance review, section 12](performance-review-20261104.md#12-structural-rank-reuse-and-selective-regional-realization).

The abstract rank/border contract now has a physical realization trial and a
bottom-up channel-width trial without repeated placement. Global longitudinal
coordination on fixed tracks now improves bounded coverage without further
node movement. A joint trial now derives bend order from incompatible
endpoint/lane domains before freezing tracks. A bounded two-pass chain trial
now traces propagated capacity paths and lifts one selected link per path into
temporal ordering. Remaining failures require simultaneous cycle/capacity
handling and bend-capacity analysis, not an unbounded retry loop. Next,
evaluate an explicitly experimental complete-router/fallback integration,
including planning cost and full-scene quality, before a default change.
Explicit ports/rows, labels and full-layout quality
acceptance still need integration before replacing the router. Only proven
infeasibility or valid objective lower bounds may reject candidates without
changing the search policy; ordinal crossing/slot estimates alone cannot.
The shadow currently consumes ranks from completed placement, not from an
independent pre-placement scheduler. The aligned demand trial now avoids
throwaway routes before final flow coordinates and reuses unchanged planning
dependencies, but still needs cross geometry. Next, introduce genuinely local
cross-order/region changes so unaffected plans and eventually full routes can
be reused, evaluate hard/track quality on broader graph families, and only then
consider production refinement/default integration. A detached target-only
order trial with anchored ancestor-size propagation now establishes that
local placement boundary and forks exact base plans. Optional exact
fixed-track decision certificates now bound candidate/occupancy/outline
dependencies, but cross-candidate hits are scarce and misses add work.
Fixed-path construction now shares preparation and reuses exact geometry/
clearance certificates, while preserving fresh occupancy acceptance.
The three-arm measurement separates this sharing from warm cache effects;
global lane/bend coordination now shares immutable projections within one
drawing, but still reruns every constraint and solver. Next, isolate prepared
constraint inputs, regional coordination dependencies and shared route evaluation without
assuming that a small placement edit has a small routing influence. Fallback
routes and scores are not reused. Retain full cold-output parity and disclose
invalidation/certificate costs before considering production integration.

Shared metric calls now reuse normalized segment boxes and combine the two
pair passes without changing outputs. The spatial route-pair experiment stays
diagnostic after the frozen comparison; more selective segment indexing and
exact per-route/pair dependencies remain open. No whole-evaluation cache has
been introduced, and relevant pair/node statistics still rerun per candidate.

The [first three-view release comparison of 2026-11-05](../benchmarks/results/focused-pipeline-20261105.md)
measured only the previous public path and the active metric change. The new
`abstractRouting` opt-in now connects the abstract completion before legacy
routing, not as post-processing of a complete old layout. Measure both full
import/refinement and refinement from the same incumbent, with quality and actual
abstract-pass/search counters; the detached single-drawing timing is not that
measurement. The [integrated comparison](../benchmarks/results/abstract-public-pipeline-20261105.md)
now shows cheaper codegraph execution but worse crossing quality, and no gain
on the two smaller views. Improve complete abstract candidate quality before
considering defaults; broader quality coverage, more area modes and cross-candidate
reuse remain open. ELK self-loop comparison frames are corrected; affected historical
external-quality rows need recomputation.

State of 2026-10-27, after the work on gestures, local edits, the shared path geometry, self-loops and the history of decisions:

1. **Done (M8, 2026-10-27):** see the row `Conflicts, setting origin and catalogue`. Open: the choice of the components and container routes of the structured optimisation is not recorded; the prototypes (container, component, hierarchy) refuse pins, authored sizes and ports instead of keeping them.
   mixed model, a loop model and a pinned-ports model.
2. **The repair search:** it works on a window around the route above 120,000 grid cells, where it used to give up (done 2026-10-27; a pin at a moved
   place and the 121 routes through nodes of the import had this one cause). Open: a better search than a grid for views with very many
   distinct coordinates, and whether a second pass over a route improves views within the limit (not measured).
3. **Local edits beyond the legacy router:** the channel router; stop signals and work budgets inside the placement; solver state in the
   snapshot if a measurement shows that the continuation from positions is not enough.
4. **Direct view completeness:** moving a route segment, doglegs, route locks and size pins as engine rules; `requested_sizes` apart from
   measured sizes (so an undo does not restore measured label sizes); the constants for clearance and minimum sizes in one place.
5. Earlier open items stay: routing with automatic port selection and corridors, RL/BT coverage, bound container transitions, joint port
   order and bundling, drill-down with cache invalidation, the quality model of [ADR 0002](adr/0002-quality-model.md), structure
   summaries (`docs/recursive-optimization.md` §12-13).

Third-party code may only be adopted after its concrete revision, license and notice obligations
have been checked. Reuse is welcome; a complete reimplementation of all algorithms is not a
requirement.

## 10. Configuration, bundling and feedback (draft)

This section is a draft from the agreement of 20.10.2026. Decisions in it are not yet implemented.

### 10.1 Configuration and presets

- Every layout option is configurable: direction strategy, target priorities, the extent of
  candidate calculation, bundling, and node sizes.
- It is still open which strategy is the default. With many possible combinations, there are two
  options: optimize all combinations against each other (more expensive, but exhaustive), or offer
  a single "best" strategy based on measurements. The decision follows from measurements of the
  benefit-to-cost ratio of each option on the reference graphs (section 7); until then, the
  presets are placeholders.
- The default setting matches the previous behavior; its results remain byte-identical.
- Options are part of the portable snapshot, so that backend and browser compute identically.

### 10.2 Target priorities

- The order of the goals is configurable. The default follows sections 4.2 and 3.1: node
  overlaps, penetration of foreign nodes, shared line segments, route crossings, projected
  crossings, length, movement, area.
- The end-to-end length (shortest path) can be selected as an additional, lower-priority criterion.

### 10.3 Direction and placement strategies

- Besides LR and TB, the strategy "shortest end-to-end path" is planned: per area, the direction
  with the smallest sum of endpoint distances of the edges.
- Hybrid gets a selectable criterion (compactness or end-to-end length).
- The inheritance rules from section 3.2 apply unchanged to the direction strategy.

### 10.4 Edge bundling and edge weight

- Full bundling (phase 1): edges with the same source and the same target run together. Members
  remain individually preserved; direction and identifier remain unchanged.
- Partial bundling (later): edges that share only one endpoint (source or target) bundle along a
  shared portion.
- The bundle weight (number of members) is calculated and shown on the edge, for example through
  stroke width or a label.
- Route weights are attributes of the view, not of the canonical graph. An input edge may say how
  many edges it stands for (`Edge::weight`, provisional, a whole number of at least 1; for
  pre-aggregated relations); the view counts it that many times wherever it counts edges.

### 10.5 Feedback of node sizes

- The visualization measures labels and shapes and reports the sizes to the engine.
- The engine recalculates with the measured sizes. The same version and staleness rules apply as
  for view changes (section 6).
- Measured values belong to the view, not to the graph. The engine stays independent of the
  renderer; measurement takes place in the visualization.


## Addendum: labels and review corrections (20.10.2026)

Edge labels and port labels must be configurable. Text and measured dimensions belong to the
portable layout state and are already taken into account for spacing and routing. Labels that
cannot be placed need diagnostics. Size feedback for nodes is merged as changes; previous
measurements not included remain in place.

Implemented first version: `edgeLabels`/`portLabels` in the layout options, enlarged layer and
sibling spacing, collision-checked label rectangles, a browser switch and a diagnostic display. A
bounded corridor search repairs node crossings already in the first layout as well as after
post-optimization. The quality check now includes source and target bodies. Unsolvable cases are
displayed. General obstacle avoidance and complete label placement are still not guaranteed.

Resetting or switching examples discards late editing responses by epoch and operation identity.
Bundle members are hidden in the renderer; explicit port bindings are not overwritten by bundling.

## Addendum: small config sets and mixed diagram logic

[Config sets and local exceptions](config-sets.md) specify the separation into layout, routing and
visualization, anchored comments without rank effect, and containers with a different internal
logic. Swimlanes, sequence, timing, free and radial graph layouts are later specialized methods.
They must be combinable with profiles and must not be derived from or forced by the global diagram
type. The first implemented scope comprises only versioned config sets for existing methods and
separate port markings.


## Specification: next algorithm phase

[Algorithm improvements: routing, ports and placement](algorithm-improvements.md) specifies the
next packages A0–A6, shared conflict definitions, hard geometry conditions, local optimization,
budgets and acceptance criteria. The functions described there as planned are not yet implemented.


## Addendum: visual editor and later interactive layout adjustment

The browser editor extends the existing visualization in TypeScript. Rust remains responsible for
layout, routing, binding geometry checks and the portable reference format. First, manual changes
are implemented together with undo/redo and curated saving. A later, explicitly enabled assistant
mode may recalculate the surroundings locally without overriding the user's intent or newer
changes. Transactions, revision protection and the frontend/backend handoff are specified in
[section 14 of the design specification](curated-references.md#14-implementation-architecture-typescript-editor-rust-geometry-services).

Status C0 (reference contract): the reference document v1 with canonical JSON, SHA-256 identities
and validation is implemented in `crates/reference` and reachable through the shared dispatcher
interface (`referenceCreate`, `referenceValidate`, `referenceCanonical`). Not yet implemented: SVG
export with metadata, the corpus manifest and the frozen real section.
