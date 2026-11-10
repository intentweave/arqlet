# Layout engine improvement plan: performance and algorithms

Status: plan, 2026-10-27. Provisional; Phases 0 and 1, Phases 2.1–2.2, the exact E portion of Phase 2.3, and the opt-in
Q options of Phases 2.4, 2.5 and 2.6 are implemented. Their before/after performance sweeps remain pending; implementation
notes are in sections 4, 5 and 6.
It complements
[Algorithm improvements](algorithm-improvements.md) (routing quality, packages A0–A6, slices P0–P7) and
[Recursive optimization](recursive-optimization.md) (strategy portfolio, M0–M6). Those documents define *what* a
good drawing is and how candidates are accepted. This one gives an ordered, step-by-step path to make the existing
layouts faster and to replace weak algorithm phases with the established ones (ELK-class layered phases, sparse
stress and overlap removal for organic, tidy trees), one reviewable change at a time.

No performance claim is made here. Section 2 records qualitative source and profile findings only; each step must
produce its own before/after sweep with machine and revision metadata before reporting performance (section 3).

## 1. Summary

| Layout | Main cost today (profiled) | Weakest algorithm phase | First step |
|---|---|---|---|
| Layered, refinement | legacy route improvement (`routing::improve_raw` / `best_route`), repeated by clearance-widening rounds | no long-edge dummies, no crossing counting during ordering, simple packing instead of Brandes–Köpf | 0.1 clearance cost, then 1.1 ids, 3.1 spatial index |
| Organic | dense O(n²) stress with a hard cap of 300; geometric repair still re-resolves ports | dense stress, pairwise separation, brute-force repair moves | 2.3 Q options, then 2.4 overlap constraints |
| Tree / radial | not measured as a bottleneck; `width()` recursion is quadratic on deep trees | fixed subtree slots instead of contour-based tidy trees | 5.1 memoized widths, 5.2 tidy tree |
| Channel router | grid build tests every segment against every node; every A* search allocates `nx·ny·5` states | full global Hanan grid instead of a sparse visibility graph | 3.5 interval index, 1.4 buffer reuse |

The steps are split into two classes:

- **E (exact):** pure speedups. Output must stay byte-identical; the stored refinement fingerprints are the gate.
- **Q (quality-changing):** new or replaced algorithms. They enter behind an option (default off, omitted from
  serialized output), are compared on the corpus with the shared metrics, and are promoted to a default only by a
  separate reviewed decision (see `docs/configuration.md`).

Do the E steps of a layout before its Q steps: they make the Q experiments cheaper to measure, and they keep the
fingerprints meaningful as a regression gate for as long as possible.

## 2. Evidence (starting point)

The earlier exploratory timings and sampling notes were single runs on a loaded machine and were not saved as
benchmark results. They are omitted as performance evidence. Use the instrumented `bench` output and the protocol in
section 3 for reproducible measurements.

### 2.1 Layered refinement

The source trace points to `route_all` → `routing::improve_raw` → `improve_pass` → `best_route` as the repeated cost
when a required clearance widens a corridor. The new benchmark reports per-candidate widening counts and route time;
the checked-in sweep in `benchmarks/results/sweep-routing-20261020T161153Z.jsonl` is the historical comparison
baseline, not a measurement of the current code.

### 2.2 Organic

The source trace points to `repair::greedy`, `RootDrawing::scan_from_cache` and `ports::resolve_nodes`. Trial scans
clone the cached geometry and conflict state, resolve ports and assemble the conflicts again. The new organic ladder
separates stress placement from repair at sizes 25 through 300; no current speed or scaling claim follows from adding
the fixtures.

Why (`crates/layout/src/repair.rs`): `greedy` tries, for every node in the conflict region, 16 directions × up to 12
step sizes. Every trial calls `scan_from_cache`, which clones `boxes` (a `BTreeMap<String, Rect>`), the outlines, the
`n²` pair vector and the `E × n` through-matrix, rebuilds the port maps of the whole graph in
`ports::resolve_nodes`, and re-sums all conflicts in `assemble`. The scan budget is 200 000 per layout.

### 2.3 Initial asymptotics found in the code (not yet hot, relevant for scale)

- `layout::place`, rank assignment: `remaining.iter().find(|id| !links.iter().any(..))` inside a loop, O(V²·E).
- `layout::place`, ordering: the barycenter/median `score` closure scans all links and is called inside `sort_by`.
- `layout::compute_once` and `place`: `graph.nodes.iter().find(|n| n.id == ...)` inside loops.
- `routing::excluded()` builds the full parent map for every edge (called inside per-edge and per-pair loops).
- `routing::improve_pass`: rebuilds `occupied` (all earlier probes) for every edge, O(E²) probes per pass.
- `channel::route_channels_raw`: `blocked_*`, `margin_*`, `containers_at`, `bands_at` test every grid segment against
  every node or container, O(segments × nodes); `Grid::index` is a linear search.
- `channel::shortest_search`: allocates `dist` and `prev` of `nx·ny·5` per search, for every edge in every round.
- `organic.rs`: stress evaluated in full after every iteration; separation O(n²) × up to 200 rounds; the distance
  memo clones the whole `n × n` matrix on a hit.
- `tree.rs`: `width()` is recomputed recursively for every node (no memo).
- `runtime::parallel_map`: static chunking (5 jobs on 4 threads become chunks of 2, 2, 1).

## 3. Rules for every step

1. **One step, one change.** Each step below is sized to be one pull request.
2. **Before/after on the same machine.** Run
   `cargo build --release -p arqlet-runtime --bins --features benchmark-timings --locked`, then the step's
   datasets with `bench --reps 3` before and after, on an idle machine, and store both result files in
   `benchmarks/results/` with a short `.md` note. No speedup statement without both files.
3. **E steps keep fingerprints.** Compare `geometryFingerprint` and `refineFingerprint` (threads 1 and 4) against a
   clean result from the exact parent revision. The checked-in
   `benchmarks/results/fingerprints-20261021T103351Z.jsonl` is historical (revision `248c2f0`), so use it only where
   the parent revision's output still matches. Any difference against the exact parent baseline means the step is not
   exact: fix it or reclassify it as Q.
4. **Q steps are options.** New option in `LayoutOptions`, default off and omitted when default, documented in
   `docs/configuration.md`. Compare with `benchmarks/profile_runner.py` and `benchmarks/eval/compare.mjs` on the
   frozen corpus; report hard validity, the published key and time separately. Default changes are separate.
5. **Determinism.** Same output for every `threads` value (`refinement_is_identical_for_any_thread_count`,
   `smoke.mjs`). No hash-map iteration order in output paths; no randomness without a fixed seed.
6. **Core purity.** No new dependencies in `model`, `view`, `layout`, `routing`, `scene`, `runtime` unless clearly
   needed; spatial indexes, quadtrees and solvers below are small enough to write in-crate.
7. **Third-party code.** Reimplement from the cited papers. Before porting any code, record repository, revision,
   license and notices (section 9).
8. **Checks.** `cargo fmt --all -- --check`, `cargo check --workspace --locked`, `cargo test --workspace --locked`;
   `sh apps/playground/build.sh` when the runtime protocol, WASM or demo is touched. Update `docs/specification.md`
   section 8 when a capability changes.

## 4. Phase 0 — Fix, guard and instrument

### 0.1 Bound the cost of the default required clearance

The code-history bisect identifies `c9184a1` ("Required clearance applies by default") as the point where the default
clearance policy changed. The exploratory before/after runs were not retained as results and are not performance
evidence. The source trace and new per-candidate timers point to `route_all`, not `corridor_gaps` or placement, as the
repeated work when the widening loop is active. The initial hypothesis of nine full-layout candidates was inaccurate:
the other first-stage jobs are routing-only and do not call `compute_once`.

Implementation:

- **Done, diagnostic:** `bench` reports each initial candidate's widening rounds and cumulative stage times, plus
  aggregate deterministic route/search counters. The stage times are outside geometry fingerprints.
- **Done, exact:** if a widening iteration changes no box position or size (bit-for-bit), keep the complete prior route
  set and skip the identical repair/routing work. The counter reports this case. It did not trigger on the 147-node
  probe, and the default refinement fingerprint stayed byte-identical.
- **Done, experimental Q:** `incrementalClearanceWidening` (default off) re-places only the widened areas and their
  ancestors, then tries `local_routes` before a full reroute. It remains off: one non-idle, single-repetition paired
  probe changed the published geometry and key. That diagnostic was not retained as a result, so no performance or
  quality conclusion is drawn from it. The option needs an idle repeated sweep and broader quality evaluation before
  any promotion.
- **Deferred:** there is no useful corridor-gap cache for the routing-only jobs; those jobs do not call
  `compute_once`. A cache across ordering candidates is worthwhile only if measurements show identical placements
  and route inputs.
- **Done, guard:** the runtime test bounds legacy route-candidate work and widening-round counts on a fixed small
  view. The bound is deterministic; stage times are not.

The remaining route hotspot is addressed by 3.1/3.2. The incremental option can be compared on the same imported
snapshot with `bench --refine-options '{"incrementalClearanceWidening":true}'`; ordinary `--options` recomputes the
imported layout and is not a paired refinement-only comparison.

Remaining acceptance: before/after sweep on the five `@100` configurations of `benchmarks/README.md` under the
default and `requiredClearance: null`, on an idle machine with at least three repetitions. The exact no-op part must
keep fingerprints; the experimental option is judged by validity, published key and time separately.

### 0.2 Refinement must keep the incumbent when no candidate is valid

`mixed-large` and `nested-tree-radial` previously ended with "No valid placement candidate; geometry constraints
could not be satisfied". Section 3.1 of [Algorithm improvements](algorithm-improvements.md) requires the last complete
result to be retained with its concrete conflicts.

**Implemented:** when the input has complete boxes but every candidate is ineligible (including the no-candidate
work-budget case), refinement returns that input, keeps the typed `geometryIssues`, adds an explicit `routingIssues`
diagnostic, and reports `keep-existing`; it no longer returns an error that discards the drawing. Regression tests
use the actual `mixed-large` and `nested-tree-radial` graph inputs with deterministic conflicting geometry. A full
run of the slow `mixed-large` import/refinement is still a separate measurement task.

### 0.3 Stage timers and a size ladder

- **Implemented:** bench output includes per-candidate and aggregate stage times for placement, organic stress,
  separation, repair, port resolution, routing, corridor-gap work and scoring when built with
  `benchmark-timings`. Timings are execution metadata and do not enter fingerprints; nested stages overlap and
  parallel candidate totals can exceed wall time. Default core and WASM builds do not read a host clock.
- **Implemented:** deterministic organic and binary-tree datasets are generated at sizes 25, 50, 100, 200 and 300,
  with seeds and hashes in `benchmarks/manifest.json`.
- **Implemented:** `benchmarks/README.md` records macOS `sample` and Linux `perf` recipes.

## 5. Phase 1 — Shared exact speedups (E)

### 1.1 Interned ids: an internal index for one layout pass

Build once per `compute_once` an internal `GraphIndex` in `crates/layout` (not in the canonical model): a stable
ID-to-index lookup, parent and child indices, root children, adjacency derived from indexed edge endpoints, edge lookup,
and per-edge excluded ancestry as sorted index vectors. Port shapes, explicit bindings and peer/family lookups live in
the separate `PortContext`.

Use the index to replace repeated node scans in placement and repair. In the routing crate, build `NodeInfo` once per
pass and pass it to `excluded_with` rather than rebuilding the parent map per edge. Keep public APIs and output order
unchanged. Further integer-index conversion in route scoring or repair trial storage belongs in the relevant later
steps; do not broaden this exact change without measurement.

### 1.2 Prepared port context

`ports::resolve_nodes` rebuilt `explicit`, `bindings`, `absolute_all`, `shapes`, `parents`, the sorted edge list and
the parallel-edge families on every call. Cache graph-derived data in `PortContext::new(graph)` once per layout;
resolve each set of boxes with the same context, optionally touching only requested nodes. Absolute coordinates still
depend on each candidate's boxes, but reuse the cached parent links. Required by 2.1.

### 1.3 Dynamic scheduling in `parallel_map`

Replace static chunks by an atomic next-job index; workers return each result with its job index, and the collector
fills result slots that are read in job order. Output is identical by construction; check with the thread-count test.
Measure threads 1, 4 and 8 on the `@100` configurations.

### 1.4 A* buffer reuse and grid lookup

Keep `dist`, `prev` and the heap in a scratch struct reused across searches of one routing pass, reset by a
generation stamp (or by the list of touched states) instead of reallocation. Replace `Grid::index` (linear
`position`) with a binary search over the sorted `xs` / `ys`. Exact.

Implementation status:

- **Implemented:** 1.1–1.4. `GraphIndex` reuses indexed parents, children, adjacency and edge exclusions during placement
  and repair; `PortContext` caches graph-derived port data and reuses parent links when deriving absolute boxes;
  routing passes share their parent map; `parallel_map` dynamically schedules jobs while preserving result order; and
  channel searches reuse A* scratch with binary grid lookup.
- **Correctness checked:** workspace formatting, check/tests and the playground WASM build pass, including the
  thread-count determinism test. Import and refinement fingerprints were identical to a clean pre-Phase-1 `12d12ff`
  build for `state-tcp`, `workflow-dag`, `org-chart`, `uml-class` and `synthetic-2k@50`.
- **Performance pending:** no before/after sweep was run for Phase 1; no speedup is claimed. Run the required repeated,
  idle-machine measurements before reporting performance.

## 6. Phase 2 — Organic layout

Current pipeline (`crates/layout/src/organic.rs`, `repair.rs`): pivot MDS (Brandes–Pich) → dense SMACOF stress
majorization (Gansner–Koren–North), O(n²) per iteration, up to 300 iterations → pairwise separation, O(n²) × up to 200
rounds → greedy geometric repair (overlaps, straight edges through nodes, walls) → ports. The initialization and the
stress model are sound choices; the cost and the quality problems are in the stages after stress.

### 2.1 Repair trials without allocation (E)

- Keep the cached `Base`, but evaluate a trial into reusable scratch buffers: the moved node's outline, the changed
  port ends, the changed pair items (row and column of the moved node) and the changed through items (touched edges
  and the moved node's column).
- `assemble` reads "overlay if changed, else base" in the same order as today, so the conflict list and the depth sum
  are bit-identical. Commit an accepted move into the base in place.
- Use the `PortContext` of 1.2 and the ids of 1.1; no `BTreeMap<String, Rect>` clone per trial.
- Store the pair items as a flat triangular `Vec` and the through items as one flat `Vec` (`e * n + k`).

The existing `#[cfg(test)]` cross-check in `scan_moved` (incremental scan equals full scan) stays and guards this.

Implementation status:

- **Implemented:** trials now reuse the cached box map, moved outline, resolved-port/absolute-rectangle buffers, changed
  port ends, pair row/column and through-item overlays. Base pairs use triangular storage; through items use one flat
  vector. Accepted moves commit their overlay into the cached base, and the following scan reuses it. The center trial
  buffer is also reused instead of cloned per candidate.
- **Correctness checked:** incremental scans and committed bases are cross-checked against full scans for conflict order
  and bit-identical depth sums; focused layout and runtime tests pass.
- **Performance pending:** the after measurement is deferred until an idle runner is available. No speedup is claimed.

### 2.2 Spatial hash for repair and separation (E)

A uniform grid with a cell size of the largest node extent plus the effective clearance gap. Pair items are only
computed for nodes in neighbouring cells; through items only for nodes in the cells a segment's padded bounding box
touches. Items outside
the reach are `None` today as well, so the result is identical if the reach bound is conservative (assert it in
tests against the full scan). Apply the same index to the separation stage in `organic_stages`.

Implementation status:

- **Implemented:** a deterministic uniform spatial hash now limits repair pair and segment candidates. The one-node
  repair path updates the moved node's bucket and restores it after scanning; the organic separation stage updates
  buckets after each move while preserving the original index-pair visit order.
- **Correctness checked:** tests verify that hash queries include every brute-force clearance overlap and segment hit,
  and that spatial separation matches the original nested-loop result exactly.
- **Performance pending:** measurements remain deferred until an idle runner is available; no speedup is claimed.

### 2.3 Cheaper stress stage (E, then Q)

- E: flat `Vec<f64>` for the distance matrix; return the memoized matrix by `Rc`/`Arc` instead of cloning it; hoist
  `fixed.contains_key` out of the inner loop into a `Vec<bool>`.
- Q (option): compute the convergence test from the update size, or the stress only every k iterations; this
  changes the iteration count and therefore output.
- Q (option): size-aware target distances, `L·d_ij` plus half the sizes of i and j, so that stress already keeps
  large nodes apart and the separation and repair stages start with fewer conflicts.

Implementation status:

- **Implemented, exact E:** graph distances are stored as a flat row-major `Vec<f64>`; the per-thread memo shares that
  matrix through `Rc`; and the fixed-node mask is computed once and reused by the stress and relaxation loops.
- **Q option, implemented, off:** size-aware targets (`sizeAwareStress`): a pair's target length adds both children's
  half-extents. Single deterministic runs on the organic ladder (default vs option, threads 1): the area grows at every
  size (25: 1.61 M → 2.53 M; 50: 4.00 M → 5.23 M; 100: 11.25 M → 12.46 M; 200: 18.53 M → 21.05 M), and routed crossings
  go 1 → 3 at 100 and 10 → 8 at 200. Not promoted.
- **Not implemented:** the convergence variant (stress only every k iterations, or a convergence test on the update
  size). It changes the iteration count; it is left for a separate evaluation.
- **Performance pending:** no after measurement was run; no speedup is claimed.

### 2.4 Overlap removal by constraints instead of brute-force moves (Q)

Replace the pairwise separation loop, and most of the overlap part of the repair, by a minimal-displacement overlap
removal:

- **Scan-line constraint generation plus a VPSC solver** (Dwyer, Marriott, Stuckey, "Fast Node Overlap Removal",
  GD 2005): per axis, a sweep generates separation constraints between neighbouring rectangles in O(n log n), and a
  block-based solver finds the least-squares displacement. Pins are fixed variables, container interiors are bound
  constraints, and the result is deterministic. Good fit for boxes with pins and containers.
- **Alternative: PRISM** (Gansner, Hu, "Efficient, Proximity-Preserving Node Overlap Removal", JGAA 2010): a
  proximity graph (Delaunay, or a cheaper k-nearest graph) and a stress model on overlap factors. It keeps relative
  positions better but needs a triangulation; `crates/topology` has one for planar drawings and could be extended.

Pick by measurement on the organic ladder (0.3): displacement from the stress positions, conflicts left, time.

Implementation status:

- **Choice:** implemented the first-listed scan-line/VPSC formulation without a new dependency. PRISM remains an
  unmeasured alternative; this is not a measured winner.
- **Implemented, opt-in:** `vpscOverlapRemoval` generates deterministic scan-line separation constraints for organic
  siblings and solves the variable-placement least-squares problem with bounded Dykstra projections. The same solver is
  used as a pre-pass for real-geometry overlap repair; pins remain fixed, rectangular container bounds are enforced,
  and circle bounds are conservative per-axis limits followed by the existing exact geometry checks.
- **Fallback and compatibility:** the option defaults off and is omitted from snapshots. If projection is infeasible or
  does not converge within its fixed work budget, the existing pairwise separation and bounded repair remain in use.
- **Evaluation pending:** no quality comparison or timing sweep was run; the runner is not idle. The option remains
  experimental and no quality or performance claim is made.

### 2.5 Edges through nodes: a force term and directed moves (Q)

- Add a node–edge repulsion term to the last stress iterations, as in ImPrEd (Simonetto, Archambault, Auber,
  Bourqui, EuroVis 2011), restricted to node–segment pairs found by the spatial hash of 2.2. This prevents most
  "through" conflicts before the repair runs.
- Replace the 16 directions × 12 steps trial grid by directed moves: for an overlap, the minimum translation vector;
  for a through conflict, the perpendicular from the segment; then a short line search along that direction. Keep the
  brute-force loop only as a fallback when directed moves stall. The trial count drops by about an order of magnitude.

Implementation status:

- **Implemented, opt-in:** `nodeEdgeRepulsion` and `directedRepair` (default off, omitted from serialized defaults),
  documented in `docs/configuration.md` and exposed in the playground.
- **Repulsion placement differs from the plan:** the term runs once after the organic stress loop (up to 8 bounded
  rounds over the projected chords, broad phase from the spatial hash of 2.2), not inside the last stress iterations.
  Moving it into the loop would change the stress trajectory and is a separate Q experiment.
- **Directed moves:** overlap translations use the minimum-translation vector; through-conflicts use the perpendicular
  from the segment, with a 0.5 / 1 / 1.5 line search and the free endpoints moved away. The exhaustive region search runs
  only when no directed proposal is accepted. Fixed blockers without a free endpoint, and walls, fall back to it.
- **Correctness checked:** unit tests cover repulsion determinism and pin handling, directed repair clearing a blocker
  with fewer scans, the fallback for walls and fixed conflicts, and option round-tripping through the runtime.
- **Evaluation pending:** the claimed reduction of about an order of magnitude in trials is not measured yet. No quality
  or timing comparison was run; the runner is not idle. No performance or quality claim is made.

### 2.6 Scaling past 300 nodes: sparse stress, then multilevel (Q)

- **Sparse stress** (Ortmann, Klimenta, Brandes, "A Sparse Stress Model", GD 2016 / JGAA 2017): exact terms for graph
  neighbours, plus k pivots (the pivot MDS already chooses them) that represent all remote nodes; O(k·n + m) per
  iteration and O(k·n) memory. Replaces the n² distance matrix; lifts `MAX_NODES`.
- **Beyond a few thousand:** a multilevel scheme (coarsen by matching, lay out the coarsest level, interpolate,
  refine; as in FM³, Hachul and Jünger, GD 2004) with Barnes–Hut repulsion (Barnes and Hut, 1986; the d3-force
  `forceManyBody` implementation shows the quadtree accumulation in about 100 lines).

Implementation status:

- **Implemented, opt-in:** `sparseStress` (default off, omitted when default), documented in `docs/configuration.md`
  and exposed in the playground. Targets are each child's graph neighbours (distance 1) plus its distance to 50
  farthest-point pivots (one breadth-first search per pivot). The pivot MDS initialization and the majorization step
  use the same arithmetic as the dense path, so the dense code is unchanged in output.
- **Bound:** `SPARSE_MAX_NODES` is 500, not 1 000. The stress is O(pivots·n + m) per iteration, but the repair stage
  still keeps a triangular pair table, and the import of 500 children took 82 s in one release run (300 children took
  13.6 s). Areas above 500 are not measured, so they are refused. The import must carry the option
  (`import` operation `options`, or `bench --import-options`); a later configure cannot lift a refusal made at import.
- **Exactness check:** up to 50 children every child is a pivot, so the sparse targets list every pair and the sums run
  in the dense order. A unit test asserts the same positions and stress bits as the dense path for a 9-child ring.
- **Default output unchanged:** refinement fingerprints match the parent `caac19b` on `organic-25/50/100/200`,
  `cloud-architecture`, `mixed-layouts`, `nested-tree-radial` and `tree-100` (`fingerprint_compare.py`, threads 1 and 4).
  Organic geometry also matches with `--options '{}'`.
- **Quality, single deterministic runs (not timings):** with the sparse option, 25 and 50 children give identical
  metrics (all children are pivots). At 100 children the area falls from 11.25 M to 9.42 M while routed crossings rise
  from 1 to 3; at 200 the area rises from 18.5 M to 20.3 M and routed crossings from 10 to 13. Neither direction is a
  clear improvement, so the option stays off.
- **Not done:** the multilevel scheme (coarsening and Barnes–Hut repulsion) is not implemented; it needs a sparse
  repair first. Timings for sparse runs are single unrepeated observations and are not claimed.

### 2.7 Interactive organic (drag) (Q)

The drag path today runs `FORCE_STEPS` damped stress steps over the whole area plus a local repair. Target:

- Relax only a neighbourhood: nodes within k hops of the moved node, plus nodes whose boxes it touches; everything
  else is fixed. Stress of that region with an anchor term pulling each node toward its previous position (mental
  map) — O(region²), independent of the area size.
- Take the cooling schedule from d3-force: `alpha` decays by `1 − alphaMin^(1/300)` per tick, with `alphaTarget > 0`
  while the pointer is down ("reheating") and 0 on release. In Arqlet this is a step-size factor carried in the
  gesture state of the snapshot, so the core stays pure and the result stays deterministic for a given input.
- Repair with `Effort::Local` on the region only, using 2.1/2.2.

Implementation status:

- **Implemented, opt-in under `forceDirected`:** a drag step relaxes only the changed children and their graph neighbours
  within two hops (`drag_region` in `crates/layout/src/lib.rs`). Other children are fixed at their current centres, so
  they keep their positions through the stress and the separation. The drag step does not repair; the drop does (as
  before). Test: `crates/runtime/tests/drag_region.rs`.
- **Measured (`bench --reps 3 --threads none --options '{"forceDirected":true}'`, median of 18 updates):** dragUpdate
  12.3 → 13.2 ms at 100 children (within noise) and 35.3 → 27.4 ms at 200 children; dragBegin 88 → 10 ms at 100 children,
  unchanged at 200 (1.4 s). These are single runs on a machine that was not idle; the stress is not the dominant part of
  a drag update, so the remaining cost is elsewhere (not yet profiled).
- **Not implemented:** the cooling schedule (alpha in the gesture state, reheating while the pointer is down) and an
  explicit anchor term. The current damped step toward the relaxed positions already limits each move to a fraction of
  the way, which is the existing mental-map behaviour; it is not a new term.

Acceptance: drag-update time (`interaction.dragUpdate` in `bench`) on the organic ladder; movement of unaffected
nodes; zero new hard conflicts at release.

## 7. Phase 3 — Routing cost in refinement

Finding before Phase 3 (2026-10-29, `move_node` on one pinned child, default options): on `er-diagram` a pinned
`ent:Customer` leaves routes through nodes (8 conflicts at the first measurement). Analysis (2026-10-29):

- **Overlap, not routing, at some pins.** At (224, 88) the pinned entity overlaps `ent:Address` (a `node_overlap`
  geometry issue). Its routes then run through the overlapped children. The placement does not move the unpinned
  neighbour out of the way. This is a placement defect, not fixed here.
- **Routing failures without overlap.** At (700, 400) and (−120, 300) the boxes are clear, and the legacy router leaves
  `e2`/`e3` (Order → OrderItem, Product → OrderItem) and `e4` through Customer. The channel router routes the same
  boxes with fewer conflicts. The needed change is a port and track order (the corridor above Customer, a port near
  an entity's top edge), which the fixed-box candidates reach only with a search over ports.
- **Fixed in this step (engine choice, `crates/layout/src/engine_choice.rs`):** when a drawing has edge–node conflicts,
  the same boxes are routed by both engines, and the drawing with fewer conflicts (then shorter routes) is kept. The
  boxes never change. Measured conflicts with the pins above: (700, 400) 8 → 2, (−120, 300) 4 → 1, (224, 88) 8 → 4,
  (700, 48) 8 → 4. Default output is unchanged on the 16 checked datasets. The check costs about 25–30 ms on this small
  view, only when conflicts exist.
- **Tried and not kept:** a bounded search over automatic port sides and offsets (fixed boxes, routing with both
  engines, scored after the finishing steps). It reached 0 conflicts at (−120, 300) and 1 at (224, 88), but not at
  (700, 400), and each evaluation costs one full reroute (about 10 ms on this view), so an edit took 1–2 s. The greedy
  order does not find a change that needs two endpoints together. A port search needs the corridor and track order
  (the next step below), not more candidates.
- **Pinned overlap, fixed (2026-10-30):** `repair_once` now resolves a pin that overlaps a free child. The overlap is
  the strict box (or disc, for two circles) overlap in absolute coordinates, not the clearance margin, and containment
  pairs are excluded. Only then do free children become movable and does the repair keep the cheaper of the greedy
  moves; without such an overlap the repair is unchanged. Test: `crates/runtime/tests/pinned_overlap.rs`. Measured on
  `er-diagram`: the overlap at (224, 88) and (700, 48) is gone, the pin stays at its position, and routing conflicts at
  those pins are no longer reported. The moves are larger than minimal (for example, Address moves about 870 px at
  (224, 88) with the greedy repair), so minimal-displacement repair is still open.
- **Track ordering, implemented (2026-10-30):** `crates/layout/src/engine_choice.rs` searches the sides of the automatic
  endpoints of the conflicting edges (at most four endpoints, in edge order). A move sets one endpoint's side, or the
  sides of all searched endpoints of one node. Endpoints that share a node and a side are spread along it, in the order
  of their peers, so two routes that enter one node from one side do not share a spot. Each candidate is a complete
  layout of the derived graph, since the ports also take part in placement, and a candidate must reduce conflicts
  without adding geometry issues. Measured on `er-diagram` with the pins, against the pinned-overlap
  fix: (700, 400) 2 → 0; (224, 88) and (700, 48) stay at 0; (−120, 300) stays at 1 (open). With fixed boxes the same ports do not help:
  OrderItem has to move by 12 px for the second route to fit. One edit with conflicts costs about 50 ms in release on
  this view. Default output is unchanged (no conflicts, no search).
- **Offsets and complete routes (2026-10-30):** the search also tries offsets 0.15/0.5/0.85 along a side. Candidates keep the nodes and re-make the searched edges' routes with `route_all` (routes of other edges stay), or lay out the derived graph completely, whichever is better and keeps every route. A first version rerouted only and dropped routes, and reported zero conflicts for an empty drawing; that is fixed and tested by counting routes. Measured: (−120, 300) 1 → 0, (700, 400) 0, (224, 88) 0, (700, 48) 0. One edit with conflicts costs about 0.2 s in release on this view. The search is bounded: at most 24 candidates per drawing, and only on views of up to 60 nodes. The debug-build test `explicit_constraints` (4 tests) took about 22 s before the search and about 135 s with it.
- **Crossings, Optimize step (2026-10-30):** refinement (the job that Optimize delivers) runs a crossing step after the
  candidates are chosen (`arqlet_layout::relieve_crossings`). Import and edits do not run it. It searches only the
  endpoints where two crossing routes leave one node side (the order of two ports on one side); without such a pair it
  costs nothing. It minimises the published key, the same key refinement gates on, so a crossing is removed only where
  the configured objectives rank it above what the change costs. Budget: 80 candidates, views up to 60 nodes. The
  refinement core (candidate logs, archives, limited runs) does not run the step, so the diagnostics are unchanged.
  Measured in release, `bench --threads 1`, median of 3, machine not idle (parent = before the layout work): state-order
  1 → 0 crossings, refine 7.1 → 14.2 ms; uml-class (8), petri-net (64), cloud-architecture (33), er-diagram (2) and
  layerweave-code (1067) unchanged, refine times within noise of the parent (39, 144, 55, 21 and 7259 ms). Import is
  unchanged on all datasets. Only state-order changes its default refined drawing among the 16 comparison datasets.
  The earlier versions were slower and removed more crossings in other ways: a 60-candidate layout-budget search on all
  crossing pairs cost 35–90 ms on four datasets and removed nothing there; a crossing-only search that ignored the
  published key removed a crossing by creating a shared segment that the key ranks higher, which the gate now refuses.

- **Known limit:** the search is greedy; the start is the drawing's own sides and offsets, and moves are single endpoints or whole nodes. A corridor-based
  router (plan Phase 3) would replace it.
- **Next:** (1) minimal-displacement repair for pins (the constraint pre-pass moved eight nodes at (224, 88), the greedy
  repair jumps one node far); (2) the corridor-based track order and port order from the review notes, for
  the remaining (700, 400) and (−120, 300) conflicts.

### 3.1 Spatial index for `best_route` scoring (E)

`best_route` builds up to 3 stubs × 96 rails ≈ 288 candidate routes per edge and scores each against all obstacles and
all earlier routes. Build a uniform bucket grid (or per-axis interval lists) over obstacle outlines, and insert each
finished route's segments as `improve_pass` proceeds. A candidate (at most five axis-parallel segments) queries only
the cells it touches; the exact predicates (`hits_outline_probe`, `shares_probe`, `crosses_probe`) stay the same.
The counts are integers, so the result is identical. The existing bounding-box prefilter becomes the cell test.

Implementation status (2026-10-30):

- **Measured, not kept.** A uniform grid over obstacle and earlier-route boxes, queried per candidate, gives identical
  fingerprints on the 16 comparison datasets and on layerweave-code (7.3 s), but is slower: petri-net 144 → 170 ms,
  uml-class 40 → 47 ms, layerweave-code 7.6 → 7.9 s. Candidate routes are long and span much of the view, so each query
  touches many cells, and the query overhead exceeds the pruning. The grid was reverted.
- **Segment memo (3.2), measured, not kept (2026-10-30).** Per distinct segment, the obstacles and earlier routes it
  enters, shares a line with, or crosses are stored once per search, and a candidate's counts are the unions over its
  segments. The first version gave different counts on layerweave-code: a segment-level bounds prefilter (strict,
  without the EPS used by the predicates) can disagree with the candidate-level prefilter by floating-point noise. The
  exact version applies each candidate's bounds to the stored lists, and the fingerprints then match on all three
  checked datasets. It is slower: layerweave-code 7.3 → 17.4 s, petri-net 144 → 647 ms, uml-class 40 → 140 ms. The
  candidates' middle segments are almost all distinct, so the stored results are rarely reused, and each segment test
  now runs without the bounds shortcut. Reverted.
- **Incremental `occupied` (3.3), not implemented.** `occupied` is rebuilt once per edge (O(E) work). The scoring is
  O(candidates × E) per edge, so the rebuild is negligible. An incremental version would need probes that borrow from a
  growing result, which needs an index-based redesign in safe Rust.
- **Profile of `best_route` (uml-class, temporary timers, reverted):** rail construction about 15% of the time, the
  candidate scoring loop about 85%. There are 249 k candidates on this view, about 0.17 µs each. Exact speedups of the
  scoring can reach about 1.5× (3.2 shares the segment tests), so the large gain needs fewer candidates, which changes
  output (an algorithmic item, see 3.6).

### 3.2 Segment memo inside `best_route` (E)

All candidates of one stub distance share their first two and last two segments; candidates with the same rail
share the middle one. Compute per segment the set of obstacles it hits and the earlier routes it shares or crosses
(bitsets), and combine them per candidate by union. `hit`/`shared`/`crossed` are "any segment" predicates, so the
union gives exactly today's counts.

### 3.3 Incremental `occupied` (E)

Grow the occupied probe list by one route after each edge instead of rebuilding it per edge; for the
`reserve_later` pass, keep a prefix/suffix split instead of re-collecting.

### 3.4 Reuse across refinement candidates (E, verify first)

`Routing`, `RoutingLast` and `RoutingExtended` work on the same placement. Check which prepared data (obstacles, grid,
port context, first improvement pass) they recompute, and share it through a per-refinement memo keyed by the
boxes and options. Keep the jobs independent for `parallel_map`: share immutable prepared data only.

### 3.5 Channel grid built with interval indexes (E)

For each grid row, the blocking intervals of horizontal segments are the nodes whose margin box spans that y; sort
the boxes once by y-range and sweep. The same for columns, for containers (`containers_at`) and for bands
(`bands_at`). This brings the grid build from O(segments × nodes) to about O(segments + nodes · log nodes) plus
output size. Exact.

### 3.6 Sparse orthogonal visibility graph and local windows (Q)

Replace the full Hanan grid of all node borders by an orthogonal visibility graph (Wybrow, Marriott, Stuckey,
"Orthogonal Connector Routing", GD 2009): only segments that are visible between obstacle corners and port points.
Restrict each search to a window around the bounding box of its ends, widening on failure (as `search_grid` already
does with `WINDOW_REACH`). This is section 13.3 of the routing plan; implement it there and measure it here.

### 3.7 Candidate limit per axis (Q, `routeTrackLimit`): tried, removed

Outcome (2026-10-30): the option was implemented as an opt-in setting, measured on 17 datasets, and removed again. It is
not part of `LayoutOptions`. The experiment stays reproducible: `benchmarks/results/route-track-limit-20261030.patch`
(against 0bcfd56) applies the change, and the sweep files `route-track-limit-{48,24,12,8}-20261030.jsonl` with the summary
`route-track-limit-20261030.md` (machine: Apple M3, load about 5, one run per point).

**Why it failed.** Candidates fall in proportion to the limit (layerweave-code 2.13 M → 0.37 M at 8), but the work moves
to the repair search. The repair grid pops (`legacyGridPops`) rise from 2.25 M to 2.90 M (+29%) on layerweave-code, and
its refine time stays at about 7.2 s. Routed crossings rise on 7–8 datasets at every limit below 48 (for example
arqlet-pipeline 20 → 42 at 8). Small views do speed up (uml-class 39.6 → 26.2 ms at 12), but that does not justify a
public option that makes the output worse on most datasets.

**What it changes in the plan.** The scoring of candidates is not the bottleneck on the large views, so further pruning
of the existing search is closed. The next routing work is a constructive layered path (section 4.2 and 4.6, pulled
forward), which builds routes directly from edge sections and reserved tracks and leaves the grid search for the
remaining hard connections.

## 4a. Hierarchical layered pipeline (default where supported; previous path retained)

On 2026-11-04 the pipeline became the default for supported direction-based areas. Capability selection leaves explicit tree,
radial and organic strategies on their own placement methods, and uses the previous pipeline when no hierarchical area is
available, `placementGrid` is requested, or a leaf is pinned; each fallback reason is included in `layoutDiagnostics`.
`placementSearch` also uses the previous path until its quality is equivalent. Pinned containers stay on the hierarchical
path. The previous path remains selectable with `layeredConstruction: false`.
This changes geometry intentionally and does not add a full refinement pass to import or reduce the configured refinement
candidates.

Direction from the user (2026-10-30): stop pruning the existing search; construct ordinary routes directly on a layered
area, and let the grid search handle only what remains. Sections 4.2 and 4.6 are pulled forward for this. The
prototype does not need Network Simplex or Brandes–Köpf; the existing ranks and placement are enough for a first
comparison.

**Shared representation (the interface to design first).** One internal value, built once per layered area and
consumed by ordering, placement and routing. It is never part of the canonical graph or the view. Its reusable part (the
sections, slots and reserved tracks, with the inputs they depend on) may be kept in an optional, versioned snapshot field
for the experimental strategy, so that a refinement or a handoff can reuse it; a snapshot without it, or with an
incompatible one, rebuilds it. Search tables, heaps and indexes are never serialized (decision of 2026-10-30):
- `EdgeSection`: an edge's part between two adjacent ranks (or a dummy lane), with its logical direction kept.
- `PortSlot`: the slot an edge uses on a node side, with its order.
- `ReservedTrack`: a track in the channel between two ranks, with its width (track spacing plus label and margin).

**Derived layout graph (the next building block, 2026-10-31).** Ordering, placement, port assignment and routing must
consume the same sections, so that the routing no longer has to guess which side of an obstacle or which track was meant.
The layout graph is derived per area from the view and never changes the canonical graph.

- Nodes of an area: its direct children (a child container is a box with border ports) and three kinds of virtual nodes:

| virtual node | purpose |
|---|---|
| long-edge dummy | reserves a passage for an edge through an intermediate rank |
| boundary port | joins the inner and the outer section of an edge at a container border |
| crossing node | stands for a crossing that is admitted on purpose (planarised view); not a connection, the two edges pass through it |

- A first position is logical: area, rank, order in the rank, track. Coordinates follow from it.
- Pipeline: (1) layout graph per area; (2) topological arrangement: ranks or embedding, node order, long-edge sections,
  admitted crossings; (3) ports and tracks ordered together, fixed constraints binding; (4) space reserved from node sizes,
  track spacing, labels, headers and margins; (5) coordinates and constructed routes; (6) search and repair only for what
  remains.
- Container border negotiation: children report size and possible border-port orders; the parent arranges the containers;
  then the border ports are agreed. The loop is bounded and keeps valid intermediate states. Two inner layouts that are free
  of crossings each do not give a drawing free of crossings when their border-port orders disagree.
- Crossing-free construction where it is provably possible; otherwise a bounded optimisation. Fixed ranks, pins or port
  constraints can require crossings even for a planar graph, and exact global crossing minimisation is not cheap.

Status against the pipeline (code at ea487b0; measurements against the existing path in
`benchmarks/results/layered-one-candidate-20261101.md`):

| part of the pipeline | status |
|---|---|
| canonical graph and visible projection | present |
| (1) derived layout graph per area | `layered_plan::AreaPlan`: ranks with long-edge dummies and border nodes, link chains, reversed links, dummy places; read by the order, the ports and the construction. Crossing nodes (admitted crossings) missing |
| (2) ranks | longest path with a deterministic cycle break (no network simplex); backward links reversed |
| (2) node order | four sweeps, then sweeps in both directions with transpositions over ranks and dummies, crossings counted, best order kept; the configured ordering candidates remain enabled in refinement |
| (2) planarity and embedding | separate experiment (`crates/topology`), not integrated |
| (3) port order | from the plan (next chain element), then reordered by the constructed ways for searched edges; not one joint decision |
| (3) track order | lane changes of a channel in a least-crossing order; lanes along the flow are not ordered |
| (4) space | channels as wide as their tracks (lane changes that do not overlap share one) and port slots a track apart (leaves grow to their ports); removing the widening (cce23c3) had made the sections fail on narrow channels, restored 2026-11-01 (`layered-tracks-20261101.md`: crossings −9 %, layerweave-code 103 of 110 section routes built); labels and headers not part of it |
| (5) coordinates | packing plus median sliding; the dummies of every long edge then on one line (2026-11-01: bends −4 %, state-tcp 72 → 36; crossings layerweave-code 984 → 1031, `layered-straight-20261101.md`); no Brandes–Köpf balancing |
| (5) constructed routes | edges inside a planned area built from their sections; others searched by the constructor with a crossing cost; backward edges, self-loops, edges between areas of either flow axis included |
| (6) search for the rest | general router only for delegated edges: grid pops −94 % against the existing path |
| border nodes and negotiation | border nodes for containers of the parent's axis (order follows the inside); for the other axis the edge is built in the common frame; a negotiation with the parent's order, routes through border nodes, and pieces split at border points were tried and not kept |
| crossing-free construction where possible | not addressed explicitly |
| persisted plan, targeted invalidation, reuse in refinement and edits | `Snapshot::layered_plans` (provisional, versioned); an edit keeps the plans of the areas it does not place again; an area placed again starts its order from the stored one (edits: swapped pairs layerweave-code 40 → 11; placement stage of the refinement 143 → 62 ms); invalidation only by dirty areas and the area's children (`layered-reuse-20261101.md`); edits build their affected edges from the plans around the kept routes (overlapping routes after five edits on layerweave-code 822 → 121, import 90), at about 150 ms per edit on layerweave-code once the sections have room (`layered-tracks-20261101.md`; 100 ms with the old repair, which left 822 overlapping routes) |
| outside-in layout with collapsed containers (scaling) | direction noted, not started |

First slice of the layout graph: long-edge dummies and boundary ports for one area at a time, with the node order and the
placement consuming them (dummies take a slot of track width in each rank they pass) and the constructor building routes
from the sections instead of searching gaps. Measured against the existing path and against step 8 of
`benchmarks/results/hierarchical-step1-20261030.md`, on all datasets.

**Scaling direction (user, 2026-10-31).** For many nodes and deep nesting, the layout should go from the outside in: an
area is arranged with its child containers as nodes of a fixed miniature size first; a container's inner layout is computed
only when it is opened (automatically for the upper levels, or on request), and a collapsed container is drawn as a
miniature. The import and the refinement then pay only for what is open. The parent's border order is known before the
container is arranged, so the container orders its inside to it, which also settles the direction of the border-port
negotiation below. Not started; the view projection already has collapsed containers (`crates/view`).

**Global layering and a node budget (decision of the user, 2026-11-01; iterative prototype).** Agreeing the border
orders level by level did not lower the routed crossings (`layered-border-agreement-20261101.md`): the orders of the levels
are not consistent with each other. The remedy is an order over the levels together: the hierarchy as one layered graph
with ranks across all levels, each expanded container a contiguous block in every rank it spans, so that a border crossing
is a position in a common rank. It is built iteratively, so that a first picture comes early and every iteration is measured
against the per-area layout:

1. *First view, outer layer* (done 2026-11-01, `global-layering-iteration1-20261101.md`). A node budget (`visibleNodeBudget`, for example 50) bounds what the first view lays out.
   Strategy 1: containers are collapsed so that the visible nodes stay within it: the outermost containers first, then
   containers are expanded level by level while the budget holds. A collapsed container takes a size estimated from its
   content (inside out: its rank count along its flow times its largest rank across it), so that expanding it later moves
   the outer picture little. Expanding is local since 2026-11-02: the areas around the container keep their places and
   push what lies after it on by its growth (`local-expansion-20261102.md`).
2. *Second level in the common ranks.* An expanded container's children join the ranks of its parent's layout as a
   contiguous block, instead of a separate area layout; compare with the per-area layout (crossings, bends, time). Done
   2026-11-01 (`globalLayering`, `global-layering-iteration2-20261101.md`): worse than the per-area layout (crossings 1269 →
   1833), because one level leaves the containers below atomic and the block order is fixed once.
3. *All levels*, if 2 is better: one layered graph over every expanded level; or level by level, if that measures better. Done
   2026-11-01 (`global-layering-iteration3-20261101.md`): better than one level (crossings 1482 against 1833) and than the
   per-area layout on network-vlan, worse in total (1269 per area); the order of the common ranks is the weak part
   (layerweave-code: 1010 crossings in the order, 1011 in the drawing).
4. *Strategy 2, a viewport:* only the nodes inside a zoomed-in window (up to the budget) are laid out, plus their neighbours
   one hop outside it, so that the edges leaving the window have a direction (the hop may be a guess).

Areas that are not layered or flow along the other axis stay atomic in the common ranks (laid out on their own, inside out).

**A consistent layered pipeline (plan of 2026-11-02).** The parts built so far (the area plans, sections, channels as wide
as their tracks, port slots, border nodes, routes through them, border agreement) were each measured on the finished
drawing while the routing still fell back to the search; a step in the order could not show there, and several were
removed for showing no effect on their own. They are completed together, in this order, and judged when 1 and 2 stand
together (a step is undone earlier only for hard violations):

1. *Hierarchical order in the sweeps.* Every container keeps its own ranks. A sweep over an area that reaches a container
   with border nodes orders the container's border rank by the places of the edges outside, sweeps the container in the
   same direction (recursively), and then counts the container's edges outside at the places of its border nodes. The
   crossings are counted over all areas with these places, and the best order of all areas together is kept. The
   placement then takes these orders as they are.
2. *Routes only from sections in layered areas.* Every edge of a layered area, also between containers of the same axis,
   runs from port to port through the chains of the areas it passes and their border nodes, changing lanes only in
   channels; the slots of a channel are ordered so that no two segments overlap and few cross, and the channel is as wide
   as its slots. No search for these edges.
3. *Several ordering runs* from different starts, and switches of neighbours that look at both neighbouring ranks; kept
   when they lower the routed crossings.
4. *Ranks that minimise the summed edge spans* (network simplex), against the longest path.
5. *A numeric, array-based model for the hot phases* (ordering, routing), which is also the data model of a persistent
   engine.

*Status (2026-11-03, initially an opt-in `hierarchicalOrdering` option).* One pass
without search for layered areas, in this order:
1. Ranks per area (longest path; network simplex with the option `simplexRanks`, see below). An edge that runs against
   the flow where its ends meet is drawn as a forward edge from its target to its source: its border nodes lie on the
   other sides of the containers on its way (the drawing is placed a second time when such an edge crosses containers,
   since the areas above are only known then).
2. One order for all areas (`hierarchy_order.rs`): sweeps that descend into containers with border nodes, sections
   weighted by their edges; in a sweep every rank after the first follows its neighbours, the border rank at the far end
   of a container too (it had been left to the parent, which made the next container see a stale order: workflow-dag
   419 → 233 crossings). Eight runs (along the flow first, against it first, from reversed ranks, from shuffled orders);
   the placement improves each area's order from there (following the sweep's orders as they are drew more crossings).
3. Ports planned with the order: the ports of a leaf's side lie a track apart in the order of the next elements on their
   chains (`plan_port_offsets`), a container's own edges have border nodes too, and a leaf is long enough for its ports.
4. Coordinates align the lanes (`align_lanes`): every rank at once takes the places nearest to where its elements'
   edges meet their neighbours' lanes (weighted least squares under the rank's order and gaps, by pooling adjacent
   violators); border nodes follow and are placed last, in the order of their lanes; a chain's dummies keep the lane
   pattern where it begins (`chain_slots`). Every channel is as wide as its tracks, counted at these lanes; a channel the
   construction still finds too narrow is widened once.
5. Routes only from the sections, the tracks of a channel ordered so that none overlap and few cross. No corridor
   widening rounds by searched routes, no second computation for the port sizes. A route that still runs through a node
   or shares a segment (an edge between areas of different flow axes, which has no sections yet) is routed again alone,
   around all others; only a conflict that remains then goes to the choice of the routing engine.
Measured against elk-rs on nine views (`benchmarks/results/layered-pipeline-20261103.md`): crossings 1842 / 2031, bends
3626 / 2254, no edge through a node. Network simplex ranks (`ranking.rs`, option `simplexRanks`): fewer bends and
crossings on most small views (petri-net 58 → 27 crossings), more crossings on layerweave-code (1453 → 1623); not the
default until measured further. Bends, 2026-11-03 (`benchmarks/results/layered-pipeline-bends-20261103.md`): a chain's
dummies follow one end, an edge keeps its lane through its dummy where the rank leaves the lane free, and planned port
places hold only on sides across the flow; bends 3580 → 3172 on nine views. On the same placement the channel router draws
layerweave-code with 849 bends against 1541: the remaining bends come from the construction's choice of lanes. Lanes chosen over whole stretches between an edge's fixed points
(`benchmarks/results/layered-pipeline-lanes-20261103.md`): bends 3172 → 2866, crossings 2002 (elk-rs 2031, bends 2254). The hot phases on arrays (`layered-pipeline-arrays-20261103.md`):
the hierarchical sweep and the per-area order on numbered elements with Fenwick crossing counts, tracks counted before the
routes are built, far route pairs skipped in the metrics; the import of the nine views 1021 -> 392 ms and their refinement
1900 -> 1086 ms, with the same output. Where it stands (`layered-pipeline-standing-20261103.md`): crossings 2002 against
elk-rs 2031, bends 2866 against 2254, 1.5 s against 70 ms for import and refinement. Side exits for containers of the other
axis are on the branch side-exits-wip (mixed results, not on the main line). Next: bends, the refinement's candidate
layouts (each a whole run), the search for the few routes the construction cannot build.

**Steps.**
1. Layered areas only, with a fixed direction and automatic ports. Anything else (containers with other rules, straight
   edges, explicit pins, bundles, self-loops) is delegated to the existing router, explicitly, with a reason recorded.
2. Long edges split into sections between adjacent ranks; sections take part in ordering and placement reservation.
3. Port and track order decided together; ordinary edges get separate tracks; bundling only under an explicit rule.
4. Channel width computed from the tracks, the clearance from the nodes and the label sizes.
5. Routes constructed directly from the sections. The grid search (`grid_route`) takes the connections that the
   construction cannot route, and the repair stage keeps its role.
6. Validation against the existing path on the same datasets, with the acceptance questions below.

**Step 1 status (2026-10-30): prototype built and measured.** `crates/layout/src/constructive.rs` constructs the
flow-facing routes of leaf-to-leaf edges in one Lr or Tb area; every other edge is delegated with a recorded reason.
Measured with `bench --constructive 1` (`benchmarks/results/constructive-step1-20261030.md`, machine and revision there).
191 of 1101 edges are constructed without a grid search. The construction takes at most 0.31 ms per dataset. No dataset
gains a hard violation, and two lose some (tree-100 196 → 46, mixed-layouts 26 → 23). Length rises on tree-100 (+13%).
The limit is structural, not the track search: automatic ports put the rest on the wrong side (22 of 41 petri-net
edges), endpoints are containers or lie in different areas, or the flow is `auto`. Step 2 (long-edge sections) and step 3
(joint port and track order) address these. Steps 4–6 are not started.

**Track-aware search (2026-10-30, `benchmarks/results/constructive-track-aware-20261030.md`).** Segments are kept in
absolute coordinates, and the path search rejects steps that conflict with assigned tracks. The baseline rises from 191 to
198 edges, and NoFreeTrack falls from 19 to 12. The construction still reserves no space in placement and does not change
the boxes or the ports. The integrated vertical slice in the review of the same date (placement reservation, a refinement
that reuses reservations, local-edit fixtures) is the next milestone and is not started.

**Ceiling experiments (2026-10-30, `benchmarks/results/constructive-ceiling-20261030.md`).** Flow-facing ports lift the
construction from 191 to 222 edges and remove the PortSide delegations, but 92 of the released edges fail on tracks.
Orientations of `auto` areas change nothing: the areas that delegate are reversed (`rl`, `bt`) or radial, not free layered
areas. Neither experiment runs in import or refinement; both are bench-only.

**Step 2 status (2026-10-30): built, measured, no net gain.** The construction is now a monotone path over the corridors
between neighbouring ranks (a dynamic programme, at most one lane change per corridor). The total stays at 191 constructed
edges of 1101. Petri-net, arqlet-pipeline and cloud-architecture gain one edge each, and layerweave-code loses three,
because the conflict check now covers every segment (`benchmarks/results/constructive-step2-20261030.md`). The 167 lane
changes show that the sections exist, but most constructed routes change lane once. Sections do not yet take part in
ordering or placement (no dummy nodes), which is the part of step 2 this prototype does not do.

**Acceptance (answered per dataset, against the existing path).**
- How many edges are built without the grid search? (Target: all ordinary edges of the layered datasets.)
- How many grid searches and repair pops remain? (Baseline: column "repair pops" below.)
- Do body crossings and unallowed overlaps stay at zero? (Baseline: "body hits" and "shared pairs"; both are already
  nonzero on several non-layered datasets, so those are reported, not fixed here.)
- Do crossings improve, without an unreasonable increase of the area? (Baseline: "crossings", and the area in the
  shadow terms of the bench.)
- How fast does the first usable result appear? (Baseline: "import ms" and "refine ms".)

**Baseline (current path, 0bcfd56, one run each, Apple M3, load about 5; `benchmarks/results/route-track-limit-48-20261030.jsonl`).**

| dataset | edges | import ms | refine ms | repair pops | body hits | issues | shared pairs | crossings | length |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| er-diagram | 12 | 2.2 | 20.9 | 1340 | 0 | 0 | 0 | 4 | 3658 |
| petri-net | 41 | 5.2 | 139.8 | 562 | 0 | 0 | 0 | 67 | 12988 |
| state-order | 14 | 0.2 | 13.7 | 0 | 0 | 0 | 0 | 0 | 2775 |
| activity-order | 40 | 3.7 | 116.4 | 17156 | 0 | 0 | 0 | 70 | 20757 |
| uml-class | 24 | 3.8 | 39.6 | 0 | 0 | 0 | 0 | 8 | 9782 |
| bpmn-pools | 24 | 1.5 | 32.2 | 0 | 0 | 0 | 0 | 8 | 10750 |
| network-vlan | 47 | 5.2 | 134.1 | 16345 | 0 | 0 | 0 | 71 | 28667 |
| arqlet-pipeline | 33 | 3.4 | 80.7 | 11572 | 0 | 0 | 0 | 20 | 14923 |
| cloud-architecture | 32 | 3.0 | 54.9 | 20806 | 0 | 0 | 0 | 35 | 24789 |
| mixed-layouts | 16 | 16.8 | 74.0 | 0 | 25 | 0 | 0 | 5 | 3715 |
| nested-tree-radial | 121 | 4.7 | 145.1 | 0 | 0 | 1 | 0 | 0 | 18099 |
| tree-100 | 99 | 6.0 | 149.0 | 0 | 196 | 1 | 0 | 0 | 24768 |
| organic-25 | 29 | 8.1 | 43.2 | 0 | 58 | 1 | 0 | 0 | 2751 |
| organic-50 | 59 | 60.9 | 281.0 | 0 | 118 | 1 | 0 | 2 | 5882 |
| organic-100 | 119 | 129.1 | 640.8 | 0 | 238 | 1 | 0 | 1 | 12773 |
| organic-200 | 239 | 5770.1 | 23595.1 | 5588 | 474 | 1 | 0 | 10 | 29550 |
| layerweave-code | 1077 | 133.7 | 7220.7 | 2251396 | 0 | 0 | 4 | 1327 | 238956 |

## 8. Phase 4 — Layered: established phases (mostly Q)

Today (`layout::place`): longest-path ranks via a Kahn loop with a deterministic cycle break; 4 sweeps of barycenter
or median ordering over the *projected* links of one container; coordinates by packing each rank left to right,
optional centring and `slide_free`. Long edges are not represented in intermediate ranks, so they are invisible to
ordering and placement and must be repaired by the router (189 edge–node hits on average before refinement in
`docs/comparison.md`, against 0.3 for ELK).

### 4.1 Exact algorithmic fixes (E)

- Kahn with a ready set: keep the ready ids in a `BTreeSet` (same smallest-id choice), decrement in-degrees, fall
  back to the smallest remaining id on a cycle exactly as today: O((V + E) log V), identical ranks.
- Ordering: build adjacency (predecessors, successors with ranks) once; compute every node's score before sorting
  the rank, then sort by the precomputed key. Same keys, same order.

### 4.2 Long-edge dummy nodes (Q, the structural lever)

Insert internal dummy nodes for every rank an edge spans (internal to the layout; never in the canonical graph or the
view; their places belong to the layout plan, which the snapshot may keep, see section 4a). Dummies take part in ordering and coordinate assignment with a small width (track spacing).
Their positions become route hints: the router starts from the polyline through the dummies, so a long edge has a
reserved lane. Self-loops and parallel edges keep their own handling; reversed (cycle) edges keep their logical
direction in the output (rule: preserve logical edge direction).

Acceptance: edge–node hits before refinement, routed crossings, refinement time (fewer conflicts to repair), on the
eval corpus against ELK.

### 4.3 Crossing counting and layer sweep (Q)

- Count crossings between two adjacent ranks with the accumulator tree of Barth, Jünger, Mutzel ("Simple and
  Efficient Bilayer Cross Counting", JGAA 2004), O(E log V).
- Layer sweep: alternate down/up barycenter or median sweeps, keep the best ordering by total crossings, stop after a
  fixed number of non-improving sweeps (deterministic work budget, like ELK's thoroughness).
- Then a greedy switch / transposition pass of adjacent pairs using the counter.

The earlier negative result (docs/comparison.md: crossing-objective ordering changed one of 15 views) was measured
without dummies, where ordering crossings and routed crossings differ. Re-run that experiment after 4.2.

### 4.4 Brandes–Köpf coordinates (Q)

Brandes, Köpf, "Fast and Simple Horizontal Coordinate Assignment" (GD 2001): four vertical alignments (up/down ×
left/right) with conflict marking for inner segments (dummy-to-dummy), horizontal compaction per alignment, and a
balanced combination. Straightens long edges, centres nodes over their neighbours, O(V + E). Needs the variable
node-size extension (ELK and dagre both use one); the separation is half the sizes plus the gap. Replaces packing,
centring and `slide_free` as an alternative candidate.

### 4.5 Rank assignment by network simplex (Q)

Gansner, Koutsofios, North, Vo, "A Technique for Drawing Directed Graphs" (IEEE TSE 1993): minimize the total
weighted edge span, which reduces the number of dummies of 4.2 and the drawing length. Then optional rank constraints
(first / last rank, same rank) and a model-order tie-break (keep the input order instead of sorting children by id)
as an option for stable drawings, as ELK's `considerModelOrder` does.

### 4.6 Channel routing between ranks (Q)

With proper ranks and dummies, every edge segment runs in the channel between two adjacent ranks or along a dummy
lane. Route each channel by hyperedge segments and slot assignment over a segment-dependency graph (Sander, "Layout
of Directed Hypergraphs with Orthogonal Hyperedges", GD 2003; the approach ELK's orthogonal router uses): O(E log E)
per channel, no global grid. Use it as the fast initial routing for layered areas; the A* channel router remains for
repairs, containers and non-layered areas.

### 4.7 Later: cross-container ordering

Each container is ordered on the links projected to its children. ELK's `INCLUDE_CHILDREN` orders across levels.
Defer this to the boundary contract work (A4b-3, P6) instead of adding a second hierarchy model.

## 9. Phase 5 — Tree and radial

### 5.1 Memoized subtree widths (E)

Compute every subtree's width once, bottom-up, and pass the table to `place`. Same numbers, O(n).

### 5.2 Tidy trees (Q)

Replace fixed subtree slots (a subtree takes the full width of its widest level) by contour-based compaction:
Walker's algorithm in the linear-time form of Buchheim, Jünger, Leipert (GD 2002), with variable node sizes; for
nodes of very different sizes the non-layered variant of van der Ploeg ("Drawing Non-layered Tidy Trees in Linear
Time", Software: Practice and Experience 2014). Subtrees nest into each other's free space; parents stay centred
over their children; order of children is kept.

### 5.3 Radial from the tidy tree (Q)

Allocate angles by the tidy-tree width of each subtree instead of its leaf count, keep each child inside its parent's
wedge (Eades, "Drawing Free Trees", 1992), and size each ring from the node sizes on it, as now.

## 10. Phase 6 — Hosts, browser and scale

- **6.1 Several WASM workers for independent candidates.** Each worker runs one candidate on the same snapshot; the
  page reduces the results in the fixed candidate order. Keeps determinism; needs no shared memory.
- **6.2 Size ladder.** Repeat the sweep up to the refinement cap (1 000 nodes) for layered, and past 300 for organic
  after 2.6. Report the full graph, the visible projection and the routing-state size separately.
- **6.3 Delta publications.** Publications are full snapshots; send changed boxes and routes only, with the base
  identity.

## 11. Order and dependencies

| step | class | depends on | gate |
|---|---|---|---|
| 0.1 clearance cost | E/Q | 0.3 timers | `@100` sweep, default and opt-out; counter guard |
| 0.2 incumbent on failure | fix | – | both datasets return a result with conflicts |
| 0.3 timers, ladder | infra | – | stage times in bench output |
| 1.1 ids | E | 0.3 | fingerprints |
| 1.2 port context | E | 1.1 | fingerprints |
| 1.3 scheduling | E | – | thread-count test |
| 1.4 A* buffers | E | – | fingerprints |
| 2.1 repair trials | E | 1.1, 1.2 | fingerprints, organic ladder |
| 2.2 spatial hash | E | 2.1 | fingerprints |
| 2.3 stress | E, Q | – | fingerprints / corpus |
| 2.4 overlap removal | Q | 2.2 | organic ladder: conflicts, displacement, time |
| 2.5 node–edge term | Q | 2.2 | through conflicts before repair |
| 2.6 sparse stress | Q | 2.4 | ladder past 300 |
| 2.7 interactive | Q | 2.1, 2.2 | drag-update time |
| 3.1 route index | E | 1.1 | fingerprints, `@100` sweep |
| 3.2 segment memo | E | 3.1 | fingerprints |
| 3.3 occupied | E | – | fingerprints |
| 3.4 candidate reuse | E | 3.1 | fingerprints |
| 3.5 grid intervals | E | – | fingerprints (channel engine) |
| 3.6 visibility graph | Q | 3.5 | routing plan 13.3 |
| 4.1 rank/order fixes | E | – | fingerprints |
| 4.2 dummies | Q | 4.1 | eval vs ELK |
| 4.3 crossing sweep | Q | 4.2 | eval vs ELK |
| 4.4 Brandes–Köpf | Q | 4.2 | eval vs ELK |
| 4.5 network simplex | Q | 4.2 | dummy count, length |
| 4.6 channel routing | Q | 4.2, 4.4 | refine time, hits |
| 5.1 tree widths | E | – | fingerprints |
| 5.2 tidy tree | Q | 5.1 | area, tree fixtures |
| 5.3 radial | Q | 5.2 | radial fixtures |
| 6.x hosts/scale | – | per step | per step |

Suggested sequence: 0.3 → 0.1 → 0.2 → 1.1 → 1.2 → 2.1 → 2.2 → 3.1 → 3.2 → 1.3 → 4.1 → 4.2 → 4.4 → 4.3 → 2.4 → 2.5 →
2.6 → 5.x → 4.5 → 4.6 → 3.5/3.6 → 2.7 → 6.x. The E steps of phases 1–3 and 5.1 are independent of each other once
1.1 exists and can be done in any order.

## 12. Reuse from d3-force and ELK

| source | license | what to take | how |
|---|---|---|---|
| d3-force 3.0.0 (`c3e73cf`), d3-quadtree | ISC | Barnes–Hut charge accumulation (`manyBody.js`), quadtree collision with radius pruning (`collide.js`), degree-based link bias and strength computed once (`link.js`), alpha cooling and reheating (`simulation.js`) | Port allowed with the ISC notice; record revision and notice in `docs/architecture.md` before copying. Ideas alone need no notice. |
| ELK Layered (via elkjs 0.13.0, `95f18bf`; algorithm sources not vendored in elkjs) | EPL-2.0 | phase structure and options: network simplex, BJM crossing counter, layer sweep with thoroughness, greedy switch, Brandes–Köpf, orthogonal channel routing, model order, layer constraints | Do not copy code into the Arqlet tree (MIT OR Apache-2.0). Reimplement from the papers cited above. Keep elkjs for evaluation only (`benchmarks/eval`). |
| libavoid (orthogonal visibility graph) | LGPL-2.1 | the sparse visibility-graph idea | Reimplement from the paper. |
| OGDF (FM³, planarization) | GPL | algorithm structure | Reference only. |

Features that ELK or d3-force have and that no step above adds (backlog, not planned here): ELK's spline routing,
edge-label dummy nodes and end-label placement, post-routing compaction, graph wrapping, rectangle packing and
component arrangement by aspect ratio (DisCo); d3's positional forces (`forceX`, `forceY`, `forceRadial`), per-link
distance and strength, and pluggable custom forces.
