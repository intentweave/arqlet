# Performance review: remaining experiments and comparison with elk-rs

Date: 2026-11-04. Scope: implementation investigation, diagnostic measurements and proposed experiments. This review does not change engine defaults or search policy; separately implemented E1/E2 work is identified below.

Local baseline: `dac8190` plus the existing uncommitted changes on `performance-imp`. These changes include hotspot work, prepared routing state, timing instrumentation and an A* experiment. No unrelated working-tree changes were altered. External reference: elk-rs 0.12.1 at `eb586b6`, matching the recorded comparison. External source was inspected for algorithm structure; no source was copied into Arqlet. The implementation proposals below describe algorithms and independent experiments.

## 1. Updated diagnosis

Read the following records together; older 1.5-second pipeline numbers no longer describe the most recent working state:

- [In-process comparison](../benchmarks/results/elkrs-inprocess-20261103.md).
- [Hotspot work and reuse experiments](../benchmarks/results/layered-pipeline-hotspots-20261103.md).
- [Fallback A* experiment](../benchmarks/results/fallback-e734-astar-20261104.md).

Recorded nine-view sums: LW initial in-process 185.74 ms; additional core refinement 331.95 ms; elk-rs layout 76.17 ms. Thus initial layout is about 2.44 times elk-rs and initial-plus-core-refinement about 6.80 times. These sums of per-view medians are corpus summaries, not one application's latency. The machine was shared. ELK graph construction/conversion are outside its clock; Arqlet import includes view preparation. Do not combine unrelated timing sessions into a new speedup claim.

The initial/refined outputs have 2009/2002 crossing events, 2878/2866 bends and zero edge-node hits; elk-rs has 2031 crossings, 2254 bends and three hits under the shared evaluator. Refinement's aggregate improvement is small on these particular inputs relative to its added time; this does not prove it is unnecessary on other inputs. Preserve per-view results and structural integrity, not just aggregate totals. LW area is about 2.49 times ELK's on this corpus, while length is similar. Geometry size alone does not explain CPU cost; grid/expanded-state size must be measured separately.

### Previously suggested work that is not the next large lever

- Shared GraphIndex/validation/port-context preparation is below 1 percent in the reported profile. Rank analysis over four placement passes was about 0.6 ms. A broad shared-preparation rewrite cannot explain or remove most of the remaining gap.
- The November 3 profile had one ordering candidate. This is no longer the current default: `e966e93` removed that restriction when switching pipelines. Section 9 measures the four-candidate plan and distinguishes this search-budget change from implementation speed.
- Prepared construction from the final track probe is already reused. Do not propose that same change again.
- A* reduces delegated grid pops from 27,021 to 2,781 in the targeted candidate, but stage time only from 7.897 to 6.407 ms. Few-state search is not automatically cheap preparation, and this is a small fraction of the total job.
- A general spatial hash and segment memoization had earlier negative experiments in the legacy track scorer. They do not rule out a specific new index in the current constructor, but index work needs a named scan and measured benefit.

## 2. What elk-rs does differently

The inspected reference has a phased layered engine. Crossing minimization works with its internal graph and dirty crossing counts; orthogonal routing visits adjacent-layer channels, builds segment dependencies, resolves dependency cycles, assigns routing slots and obtains required channel width from the slot count. Node placement defaults to Brandes–Koepf. It is not a magical linear-time implementation: its routing generator also compares segment pairs, and it performs repeated crossing-minimization attempts.

Arqlet already has related structural pieces (sections, channel track assignment, hierarchy ordering and simplex ranks). The experiment should therefore not be described as adding layered routing from scratch. Differences worth investigating:

| Difference | Arqlet observation | Experiment |
| --- | --- | --- |
| Phase lifecycle | `place` builds ranks/order/coordinates together and can run again for cross-boundary backward edges, hierarchy sweeps and channel sizing | Explicit region-plan phases with late coordinate realization |
| Channel ordering | `dependent_order` repeatedly sums all pair costs over the remaining set | Incremental balances, then exact or alternative ordering experiments |
| Track assignment | `track_slots` scans all earlier changes for every change | Range-maximum structure for the exact interval recurrence |
| Route checking | Planned routes scan their obstacle lists and previously assigned segments | Channel-local bounds/indexes and shared immutable obstacles |
| Coordinate choice | Repeated lane alignment, whose border reordering feeds back into order | Profile frequency and convergence; alternative BK coordinate realization |
| Evaluation | Full scene-oriented geometric metrics on several finished candidates | Reuse compatible evaluation and spatially enumerate relevant pairs |
| Search controls | Fixed eight hierarchy starts, bounded sweeps and Optimize-time complete reroutes | Explicit effort control and quality/time curves, not hidden truncation |

An ELK internal hyperedge segment is not permission to merge Arqlet's semantic edges or silently bundle them. Constraints, multiplicity, explicit ports and individual route identity remain binding. Where a reference result violates a hard requirement, its runtime is not a quality-equivalent result.

## 3. Prioritized output-preserving experiments

### E1. Remove cubic repeated balance computation in `dependent_order`

File: `crates/layout/src/constructive.rs`, `dependent_order`.

Current greedy selection computes, for each remaining i, `D_i = sum_j(cost[i][j] - cost[j][i])` from scratch, then removes one element. Summing over all remaining-set sizes gives cubic work in the channel's number of changes, beyond cost-matrix construction. Keep the original corridor order and tie-breaks, compute balances once, and after removing r subtract `cost[i][r] - cost[r][i]` for every remaining i. Selecting the minimum by a stable linear scan makes this greedy part quadratic, with no new heap needed.

A standalone check performed during this review compared both selection formulations on 280 seeded integer cost matrices (sizes 1–64) and found identical orders. This establishes a useful algebraic check, not engine equivalence or an end-to-end speedup. The initial `corridor_order` and the later sifting phase still have their own work; do not label the whole function quadratic solely from this change.

Measure channel-size distribution, number of calls and time before optimizing. Preserve integer overflow behavior/valid ranges and existing equal-score order. Add direct regression cases for ties and repeated lanes, then compare complete fingerprints.

**Implementation follow-up:** the repeated balances are now maintained as removals occur, with a stable scan preserving first-in-order ties. A test-only differential oracle covers 256 seeded matrices (sizes 1–64). Whole-layout fingerprint and runtime validation are still required before treating this as an output-preserving engine change.

### E2. Exact faster track-slot recurrence

`track_slots` assigns a slot one above the maximum slot of earlier overlapping spans. It currently scans the whole earlier prefix (quadratic). Test a coordinate-compressed interval/range-maximum representation that computes the same maximum and writes the interval's new value. Preserve the precise strict overlap and EPS/track-spacing rules, including zero spans and equal endpoints. An interval enumeration index does not avoid quadratic work in a dense overlap case; an aggregate range-max structure is the intended experiment.

Use the existing implementation as a test oracle on generated channel spans. Measure preprocessing as well as queries. Reuse the result between sizing and route construction only when the ordering, spans and spacing match. Do not merely add an index that costs more than these small channels.

**Implementation follow-up:** the quadratic scan remains for channels of at most 512 spans; larger channels use coordinate compression and a two-dimensional Fenwick range-maximum structure for the exact strict overlap recurrence. The test-only oracle compares randomized spans at both sides of that threshold and boundary cases across multiple track spacings. No full performance sweep or corpus channel-size distribution has been recorded yet, so no engine-level speedup is claimed.

### E3. Stop validating each planned route against the whole drawing

`build_planned` scans p.obstacles and every segment in assigned. `prepare_edge` materializes per-edge geometry; `section_routes` then already knows the relevant rank channels. Measure comparisons and allocations, split by same-area sections versus irregular/cross-area routes.

For eligible section routes, share immutable obstacle arrays, query relevant ranks/containers, and keep channel-local segment occupancy. Foreign containers and cross-hierarchy permissions remain part of narrow-phase checks. Preserve input edge order when it affects conflict/slot decisions. Initially keep one complete final validator; do not remove safety checks merely because construction should be correct.

This differs from the earlier failed broad legacy hash experiment: it narrows the problem using structural channel ownership, rather than indexing every candidate's generic route bounds.

### E4. Spatial pair generation and shared evaluation

The current metrics reject distant route pairs after entering an all-pairs loop. Test sweep-and-prune over route bounds, followed by unchanged segment predicates. Emit pair IDs in original (i,j) order where floating-point accumulation order is part of output compatibility. Include containment, clearance and route thickness in conservative bounds.

Also test whether the current per-candidate engine quality and published evaluation repeat geometric work. `published_evaluation` already shares its clearance variants; preserve this improvement. A common lazy Evaluation can serve multiple consumers, keyed by geometry and the relevant evaluation context. Do not compute output-only metrics for every search state unless they affect selection. Keep mandatory validation; a bounds-only or proxy score cannot replace final geometry acceptance.

The reported pair loop is about 56 percent of an 8-percent metric stage in one profile: eliminating it entirely would only save roughly 4.5 percent there. This is a bounded improvement, not the primary architecture lever.

## 4. Larger algorithmic experiments (output may change)

These should be opt-in alternatives measured for quality and work. Fingerprint equality is not their acceptance rule; required constraints still hold and per-view tradeoffs must be visible.

### Q1. Structural plans first, coordinate realization later

Separate area ranks, edge orientation/boundary nodes, ordering, cross-axis lane coordinates, channel slot demand, and final primary-axis positions. Try assigning cross-boundary backward orientation from the complete rank plan rather than discovering it only after a full drawing. Run hierarchy ordering before expensive final coordinate realization. Channel width changes should update primary-axis offsets and parent extents rather than redoing independent ordering/lane work.

This is not a mechanical deletion of current passes. Today `align_lanes` can reorder border ranks from coordinates, and an earlier experiment following hierarchy orders directly worsened crossings. Preserve an explicit bounded feedback step where needed and measure whether it is required by the affected regions, not the whole graph. Prototype on uniform-direction regions first; keep mixed-layout/explicit-port fallbacks.

Measure full coordinate passes and channel/order computations per region. Success is less repeated work at acceptable quality, not just fewer function calls. The earlier profile's 1–4 placement passes were not a permanent bound; the current diagnostic in section 9 records 5–7 per ordering candidate.

### Q2. Channel-dependency routing as a complete alternative

Compare the current greedy/sifting lane-order construction with an independently implemented dependency-graph channel assignment from the published orthogonal-layered algorithm literature. Construct dependencies, handle cycles explicitly and assign slots jointly for a channel. Materialize required channel width directly. Distinguish this from E1/E2, which retain today's order and coordinates.

Keep lanes continuous across long-edge sections to reduce unnecessary lane changes/bends. Aim to address both routing CPU and the bend/area gap. Reserve A* for unsupported constrained/irregular connections; do not replace it by an unbounded repair stage. Report constructed, delegated and searched edge counts separately.

### Q3. Alternative coordinate assignment

Add a Brandes–Koepf-style coordinate candidate (from the paper, with variable node sizes/ports/containment explicitly handled) beside current lane alignment. Compare coordinate-stage time, route bends, area, crossings and needed fallback work. A faster coordinate routine that makes routing harder can lose end to end. Different ranks via existing `simplexRanks` can be part of a later controlled matrix; report expanded sections/dummies and occupied channels, not just input nodes.

Do not rewrite the existing simplex engine merely because ELK uses simplex by default: rank analysis is currently a small measured cost. The relevant hypothesis is that a different ranking reduces downstream problem size.

### Q4. Make search effort an explicit user budget

Initial layout already captures most aggregate quality in the recorded nine-view comparison. Provide first valid result and progressive extra optimization under a stated effort budget. Measure hierarchy starts/sweeps, routing-only passes and Optimize-time relief separately. Do not assume one ordering attempt in each engine implies equal work: ELK's thoroughness also controls multiple attempts.

For `relieve_crossings`, the recorded six-view diagnostic accepted no result and uml-class spent about 29 of 35 ms in this optional stage. First test local repair of affected routes instead of full rerouting per move, or cheap necessary-condition rejection. A smaller attempt budget/early plateau stop is also a valid explicit search-policy experiment, but changes behavior and cannot be advertised as output-identical acceleration. Retain the current effort mode for comparison; do not hardcode dataset-specific skips from this small corpus.

## 5. Compiler and parallelism follow-ups

Only after algorithm measurements, run controlled release-build experiments: current thin LTO versus fat LTO/codegen-unit choices, then profile-guided optimization on representative training graphs with separate held-out families. `target-cpu=native` is local native benchmarking only, not a portable distribution setting or a browser optimization. Reject unsafe floating-point modes that invalidate geometry/ordering assumptions. Count build/maintenance complexity against measured gain.

Parallel candidates or independent components reduce wall time, not total algorithmic work. Record CPU time, peak memory, thread count and serial results. One shared graph's hierarchy levels are not freely independent. Do not use concurrency to hide repeated whole computations or destroy deterministic work-budget selection.

## 6. Test/measurement contract

No new full benchmark sweep was performed in this review. The quantitative baseline comes from the linked local records, which include shared-machine load limitations. Do not treat dirty working states as reproducible release baselines; record a patch/commit identity before further paired runs.

Start with E1 and E2 and detailed constructor scan/allocation counts for E3. Each experiment is separate and reversible. Use frozen inputs independent of Arqlet's changing source graph, plus an explicit frozen copy of the current layerweave-code view. Include the nine views, larger/dense/long-edge inputs and hierarchy depth variants. Keep same layouts/constraints/labels/ports/bundling semantics across compared engines, and measure remaining mapping costs separately.

For exact changes: compare complete geometry/keys/metric fields and deterministic thread behavior. For algorithmic changes: compare each view's geometry validity, crossings, bends, area, length and first-result latency/extra refinement. No new weight calibration is required. Preserve labels, logical direction, edge multiplicity, clearance and container ownership.

Collect exclusive phase time where possible; existing nested placement/sweep and routing/fallback timers overlap and must not be summed blindly. Include allocation count/bytes and expanded objects (dummies, border nodes, sections, channel changes, occupied tracks). Test the suspected exponent against these sizes. A 182-node graph can have far more routing/order elements, so node count is not the complexity variable by itself.

Recommended sequence: E1 -> E2 -> E3; alongside, a bounded Q1 prototype. Then decide Q2/Q3 from the resulting phase profiles and quality/time curves. Do not repeat the shared-preparation or A* recommendation as the next large lever without new evidence.

## 7. External sources inspected (reference only)

Pinned elk-rs reference is the measured 0.12.1 revision `eb586b6`. The sources were read to identify phase and algorithm choices, not copied, translated or vendored. Its release measurements on a different machine are not Arqlet comparative evidence.

- [elk-rs 0.12.1 README](https://github.com/openedges/elk-rs/blob/eb586b6/README.md): API and version boundary.
- [Orthogonal routing phase](https://github.com/openedges/elk-rs/blob/eb586b6/plugins/org.eclipse.elk.alg.layered/src/org/eclipse/elk/alg/layered/p5edges/orthogonal_edge_router.rs): adjacent-layer processing and channel width from slot demand.
- [Orthogonal channel generator](https://github.com/openedges/elk-rs/blob/eb586b6/plugins/org.eclipse.elk.alg.layered/src/org/eclipse/elk/alg/layered/p5edges/orthogonal/orthogonal_routing_generator.rs): segment dependencies, cycle handling and slot assignment.
- [Crossing minimization](https://github.com/openedges/elk-rs/blob/eb586b6/plugins/org.eclipse.elk.alg.layered/src/org/eclipse/elk/alg/layered/p3order/layer_sweep_crossing_minimizer.rs): internal order state, repeated attempts and dirty crossing counts.
- [ELK phased overview](https://eclipse.dev/elk/blog/posts/2025/25-08-21-layered.html).
- [ELK node placement options](https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-nodePlacement-strategy.html): Brandes–Koepf default.
- [ELK thoroughness](https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-thoroughness.html): effort control, default 7 in the current official reference; verify metadata at the pinned binary for controlled comparisons.
- Existing papers in [layout-improvement-plan.md](layout-improvement-plan.md), especially Brandes–Koepf coordinate assignment and Sander's orthogonal hyperedge/channel approach, are the implementation starting points for independent algorithm work.

## 8. Matched elkjs/elk-rs measurement follow-up

The evaluation harness now has an `--inprocess-core` mode. It first obtains the bounded view, then
passes the exact same serialized graph to Arqlet's native `compute_counted` benchmark and to
elkjs/elk-rs. The raw rows include an input SHA-256 per engine and view; all three engines matched.
ELK graph-object construction and output normalization are timed separately from `layout()`.

On nine views, the sum of per-view medians was 281.76 ms for Arqlet core, 513.99 ms for elkjs
core, and 89.93 ms for elk-rs core. Thus the current Arqlet core was about 1.82× faster than
elkjs and 3.13× slower than elk-rs on this shared machine. These figures supersede the earlier
public-process comparison only for core layout time. They exclude the Arqlet process/JSON
boundary and exclude refinement. Methodology and caveats are in
[the matched comparison report](../benchmarks/results/inprocess-core-elk-20261104.md); raw rows are
[here](../benchmarks/results/inprocess-core-elk-20261104.jsonl).

## 9. Current refinement audit: repeated whole layouts and changed search budget

### Measurement and reproducibility

New diagnostic runs on Apple M3, eight logical CPUs, macOS/aarch64, Rust
1.99.0, native release with `benchmark-timings`, one refinement thread.
Revision `e966e93`, dirty tree including the existing E1/E2 changes. The raw
rows record the exact benchmark binary SHA-256 and the SHA-256 of the existing
layout/benchmark source diff. This is not a clean committed release baseline.
Sequential runs on a shared machine, one warmup and two timed repetitions per
operation; small differences are not statistically established speedups.

Inputs were frozen before running: `layerweave-code`, `workflow-dag` and
`org-chart`, limit 150. As in the uniform-direction comparison, replace
`containerDirections` and `containerStrategies` with empty objects and
`defaultStrategy` with `"none"` before importing. Each variant retains the
same initial geometry; only `--refine-options` changes. Input hashes and the
three matching initial geometry fingerprints are recorded in
[raw runs](../benchmarks/results/refinement-audit-20261104.jsonl).

Build and run each frozen input with:

```sh
cargo build --release --locked -p arqlet-runtime --bin bench --features benchmark-timings
target/release/bench --graph /path/to/frozen-uniform.json --limit 150 --reps 2 --threads 1 --name DATASET
# Repeat with only refinement options changed; these do not recompute the initial drawing:
target/release/bench --graph /path/to/frozen-uniform.json --limit 150 --reps 2 --threads 1 --name DATASET --refine-options '{"candidates":[1]}'
target/release/bench --graph /path/to/frozen-uniform.json --limit 150 --reps 2 --threads 1 --name DATASET --refine-options '{"candidates":[]}'
```

The benchmark runs Optimize and core refinement separately. The following
table is **additional in-process core refinement**, excluding import,
process/JSON transport and the Optimize-only crossing step:

| View | Current: four ordering candidates | One ordering candidate | Routing only |
| --- | ---: | ---: | ---: |
| layerweave-code (188 nodes, 358 edges) | 1776.50 ms | 592.62 ms | 118.32 ms |
| workflow-dag | 84.13 ms | 27.42 ms | 8.19 ms |
| org-chart | 74.83 ms | 27.26 ms | 12.34 ms |

These are **budget experiments, not output-preserving implementation
optimizations**. The delivered geometry fingerprint and published key agree
across all three variants for layerweave-code and workflow-dag. For org-chart,
the full portfolio selects `forward-median-centered`: key
`[0,0,2,35,0,46,208,8416387]`, versus
`[0,0,2,36,0,46,206,8205459]` for the two reduced budgets. The full portfolio
therefore provides a real lexicographic improvement there, despite greater
area/length/movement. Full snapshot fingerprints differ between budgets
because options and optimization metadata are part of the snapshot.
Do not silently remove candidates on the strength of the first two views.

### What the work actually does

1. **The default switch also expanded search.** The runtime diff in `e966e93`
   removes the `layered_construction` first-candidate slice in
   `plan_first_stage`, including its routed-candidate equivalent. The default
   `[1,2,3,4]` is now evaluated in full. This explains a substantial difference
   from the earlier one-candidate pipeline reports; changing input graph
   sizes also prevents a direct historical regression ratio.
2. **Ordering is a full layout, not a local refinement.**
   `candidate_from` passes earlier plans as ordering seeds, but calls
   `compute_mode` with `previous=None`. Unchanged geometry is not reused.
   The separate
   [candidate log](../benchmarks/results/refinement-audit-candidates-20261104.jsonl)
   records 7, 7, 7 and 5 full-view placement passes, 26 total. Channel probes
   and late widening can rebuild geometry; the first three candidates each
   construct/route the whole 358-edge view twice. Across the four jobs:
   2431 constructed-route outcomes plus 75 delegated outcomes, or seven
   complete route sets. These are counts of work, not distinct edges.
   The four job timers sum to 1660.25 ms in the extra logged run, with
   78.24 ms of candidate scoring. Timers overlap; do not sum nested stages or
   compare this logging run as an exact partition of the unlogged median.
   The eventual winner is routing of the original geometry, not an ordering
   layout. The expensive alternatives were genuine distinct geometries, not
   byte-identical layouts that a final fingerprint cache could have skipped.
3. **"Constructed" does not mean "never searched".**
   `construct_with` first tries `build_planned`; failed planned routes can
   enter `build_edge` and its `monotone_path`/`search_on` calls and still count
   as constructed. `lane_changes` also searches inside construction.
   `delegated_nanos` starts only after construction. Its 167.36 ms across the
   four ordering jobs does not measure all geometric search. A separate
   five-second macOS sample of the default benchmark repeatedly reaches
   these constructor search stacks. It includes surrounding benchmark work,
   not an isolated core-phase measurement. Instrument constructor attempts,
   failed planned checks, search calls/states and preparation separately
   before treating delegated time or grid-pop counts as total search cost.
4. **Evaluation is still repeated, though not in every previously suspected
   place.** `published_evaluation` already shares its clearance variants.
   Nevertheless, `evaluate_job` then calls `quality_with`, which independently
   scans route pairs and node hits and computes projected measures.
   Publication calls `shadow_evaluation`, measuring the drawing again even
   for a no-op publication callback. `Archive::observe` fingerprints geometry
   and clones entries regardless of archive refinement being enabled.
   Their semantics differ: share compatible raw observations, not one
   substituted score. The last no-op routing round already reuses evaluation,
   but its equality test serializes both complete snapshots.
5. **Optimize has separate avoidable work, not the main explanation.**
   On this code view its crossing-relief timer is about 30 ms, not seconds.
   The `MAX_SEARCH_NODES=60` guard is inside `side_search`, after crossing and
   endpoint discovery; `relieve` also evaluates published keys afterwards.
   Move the proven size-based no-op rejection ahead of this work as a
   separate output-equivalent experiment. For eligible small views, the
   search still calls `route_all` before retaining only searched edges.
   Earlier local-routing experiments had quality regressions: this requires
   a fixed-occupancy equivalence proof or an explicit algorithm alternative.

### Revised next experiments

| Priority | Experiment | Status / output contract |
| --- | --- | --- |
| 1 | Separate ordering/lane planning from width-only coordinate realization; invalidate only dependent regions after channel growth | Still open. Main structural lever; current coordinate-to-border feedback prevents simply deleting passes |
| 2 | Expose and reduce constructor-internal search, not just delegated fallback: failure reasons, repeated topology work, unchanged immutable obstacles and per-attempt buffers | Some search/index micro-optimizations already retained. Exact reuse needs identical relevant inputs and occupancy; whole-construction caching previously missed |
| 3 | Share compatible evaluation observations across key, quality and publication; spatially generate relevant route pairs | Still open beyond the already merged key/clearance sharing. Preserve scene-versus-engine metric semantics and deterministic pair order |
| 4 | Materialize generic seed routes only for delegated edges and resolve ports once per geometry | Newly identified concrete waste: `legacy_routes` seeds every edge before construction, then discards most seeds; keep full-graph port allocation and edge order unchanged |
| 5 | Channel-local obstacle/assigned-segment checks and the early Optimize size guard | Open exact experiments; the sampled planned-check cost is not evidence that these alone dominate |
| Separate policy | Explicit routing-first / ordering-budget modes and quality-time curves | Large measured opportunity on two views, but org-chart is a counterexample to output equivalence. Retain the full portfolio |

E1/E2 are already implemented and tested locally; no paired engine-wide
before/after measurement of them exists yet. General preparation below one
percent, fewer delegated A* pops, and the earlier unsuccessful generic
spatial hash are not new large-lever proposals. BK coordinates and
dependency-channel routing remain untried algorithm alternatives, not exact
speedups. No engine behavior, candidate set or defaults were changed by this
audit.

## 10. First phase separation and constructor instrumentation

The first implementation step keeps all four ordering candidates and the
existing acceptance, quality and publication rules:

- The coordinate-free regional ordering phase is extracted from placement.
  A memo local to one complete layout reuses an ordering only for identical
  incoming ranks, links, components, mode and earlier-plan ranks/fixed flag.
  Different border ordering invalidates it. No geometry cache or cross-job
  mutable cache is introduced; global flattening keeps its previous path.
- After the bounded channel probes, a final preparation can already prove
  that the existing late widening will discard the current routes. In that
  case, routing waits until after that widening instead of constructing and
  repairing a complete route set which cannot survive. The full final
  routing and validation still run. Local edits and the channel engine are
  excluded.
- Aggregate, initial-stage and per-candidate diagnostics now expose
  constructor preparations, planned attempts, obstacle/occupied-track
  failures, edge/lane search causes, internal search calls/failures,
  corridor/lane cells and clear lane transitions. These are separate from
  delegated fallback work. Cells are allocated DP cells, **not A* expansions
  or heap pops**. Preparation includes section planning; internal search has
  its own native benchmark timer.

### Frozen-input paired diagnostic

[Raw before/after results](../benchmarks/results/phase-split-20261104.jsonl)
record Apple M3, eight logical CPUs, macOS/aarch64, Rust 1.99.0, release,
revision `e966e93` with dirty worktree, binary/input/source hashes and load.
The preserved baseline already includes E1/E2 and the benchmark changes;
this is not a comparison against clean `e966e93`. One warmup and three timed
repetitions per binary, on identical frozen inputs, with threads 1 and 4:

| View, limit 150 | Core refinement before, 1 thread | After, 1 thread | Before, 4 threads | After, 4 threads |
| --- | ---: | ---: | ---: | ---: |
| Code (188 visible nodes, 358 edges) | 1822.898 ms | 1425.165 ms | 664.065 ms | 517.286 ms |
| Workflow | 91.256 ms | 88.883 ms | 34.137 ms | 33.525 ms |
| Org chart | 78.316 ms | 76.033 ms | 34.477 ms | 34.422 ms |

This was a shared-machine diagnostic, with recorded load reaching roughly
8–10, not an idle-machine sweep. The code view shows about 22% less observed
core-refinement wall time; do not generalize this as a controlled speedup or
combine it with earlier ELK timings. Initial import on the code view was
216.660 → 219.313 ms: no initial-layout improvement is established here.

Full initial/refined snapshot fingerprints, geometry and published keys
match before/after on all three views; threads 1 and 4 produce identical
snapshots. All seven code-view candidate occurrences additionally match in
ID, geometry, key, acceptance, selection and publication against the earlier
[candidate log](../benchmarks/results/refinement-audit-candidates-20261104.jsonl);
the [new candidate log](../benchmarks/results/phase-split-candidates-20261104.jsonl)
contains the new per-candidate counters.

For the four code-view ordering jobs:

- Full route sets fall from seven to four: three discarded complete routings
  are deferred. Constructed/delegated outcomes become 1388/44 instead of
  2431/75. They are repeated work counts, not unique edges.
- Regional ordering runs 375 times and is reused 639 times.
- The **26 full placement passes remain**. This is the first separation,
  not completion of the larger rank/lane-plan versus coordinate architecture.
- Construction still performs 435 internal searches, including 128 failures,
  allocating 1,388,434 corridor/lane cells and evaluating 55,055,505 clear
  transitions. The diagnostic records 334.260 ms inside constructor search
  and 147.672 ms in preparation at one thread. Those timers overlap enclosing
  stages and cannot be summed as an independent time partition.
- There are 1396 planned attempts, with 105 obstacle failures and 40 occupied
  track failures. Search causes include 149 edge and four lane searches; one
  cause can try multiple internal search graphs.

The final routing after late widening is now included in `routing` timing;
the previous timer omitted that final call. Before/after stage-time columns
therefore do not have identical coverage, although end-to-end core timing
does.

The regression test freezes the bounded graph in
[phase-width-feedback.json](../examples/fixtures/phase-width-feedback.json).
It must not import the generated `layerweave-code` dataset: the playground
build regenerates that graph from the current source, changing its topology.
The fixed fixture checks three deferrals, 26 placements, the pre-change
geometry fingerprint `19409ef4dbf00e51` and full thread-identical snapshots.
Additional cached/uncached comparisons cover four fixtures, five ordering
modes and global layering on/off.

Next: separate width-only coordinate realization from structural/rank
planning more fully, then address the still-visible constructor transitions
with exact input/occupancy invalidation. Do not remove candidates to disguise
this remaining work.

## 11. Cross-flow planning separated from channel-width realization

The next step separates the two coordinate axes for non-flattened
hierarchical regions:

1. **Cross-flow plan:** free sliding, dummy widths and slots, lane alignment,
   border-rank reordering and the required track count for each channel.
2. **Flow-axis realization:** current child extents, initial rank positions,
   requested channel widths and the same cumulative shifts of later ranks.

The cross-flow plan has no dependency on flow coordinates, flow-axis child
sizes or requested channel growth. Its memo is local to one complete layout,
separate for each region and axis. Reuse requires identical levels, links,
chains, edge membership, incoming child/leaf lane offsets, initial cross-axis
positions/sizes, extent, spacing and free-sliding mode. Floating inputs match
bit-for-bit, including signed zero. Changed child-border lanes or border
ordering invalidate reuse; the final flow coordinates and container extents
are always recomputed. It is not a cache across revisions or candidates.
Global flattening and non-hierarchical strategies keep their prior path.

For the frozen code view, the four ordering jobs perform **329 cross-flow
plans and 685 exact reuses**, instead of recomputing all 1014 regional
cross-flow plans. Workflow records 72/36 computations/reuses, org chart
144/196. **The 26 code-view hierarchy placement entries still remain**:
child traversal, graph projection, rank construction and initial coordinate
loops have not yet been replaced by a dependency-only scheduler.

### Paired release diagnostic

[Raw results](../benchmarks/results/coordinate-split-20261104.jsonl) record
Apple M3, eight logical CPUs, macOS/aarch64, Rust 1.99.0, release, revision
`e966e93` with dirty worktree and binary/input/source hashes. The preserved
baseline includes E1/E2 and the first phase split in section 10. Inputs are
the same three frozen graphs; each binary has one warmup and three timed
repetitions with threads 1 and 4.

| View, limit 150 | Core refinement before, 1 thread | After, 1 thread | Before, 4 threads | After, 4 threads |
| --- | ---: | ---: | ---: | ---: |
| Code | 1409.920 ms | 1342.226 ms | 525.084 ms | 503.123 ms |
| Workflow | 91.338 ms | 85.070 ms | 34.416 ms | 32.656 ms |
| Org chart | 76.142 ms | 73.349 ms | 33.523 ms | 33.228 ms |

This is a **heavily shared-machine diagnostic**, not a controlled speedup:
recorded load averages range from roughly 8.5 to 12.5. The code-view median
is about 4.8% lower at one thread and 4.2% lower at four threads; that is a
modest observation, not the larger overall improvement still sought.
Initial import is 218.065 → 213.901 ms on this view. Do not mix these
measurements with historical ELK runs as a new matched engine comparison.

All three views retain initial geometry, full refined snapshot/core
fingerprints, published keys and routing issues. Threads 1/4 still produce
identical snapshots. All seven code-view candidate occurrences retain their
IDs, metrics, keys, outcome, geometry, duplicate/archive ranks and
accepted/selected/published flags; the
[new candidate log](../benchmarks/results/coordinate-split-candidates-20261104.jsonl)
includes the cross-plan counters. Constructor search calls/cells/transitions
and constructed/delegated route counts are unchanged. This step reduces
placement work, not the still-substantial routing search.

Instrumentation also now includes the previously untimed placement calls
after hierarchical channel probes and late widening in `placement_nanos`.
The code-view after value therefore covers more work than the before value;
do not interpret its higher cumulative placement timer as a regression.
End-to-end core timing has the same measurement boundary.

Validation includes all five cross-plan tests (input invalidation,
region/axis isolation, signed zero, projection/application and full drawing
comparisons): the frozen feedback graph at all five ordering modes; six
fixed/free/allowed direction settings with fractional node sizes, clearance
and free placement on/off; and the earlier four-fixture cached/uncached
matrix with global layering on/off. Workspace fast-profile tests, workspace
check, formatting, native timing-feature runtime regression and the
playground/WASM build pass. The previously reported unrelated timing-only
organic unit-test compilation error remains outside this change.

The remaining larger work is explicit: reuse structural region/rank
planning, then schedule coordinate realization by changed child extents and
channel dependencies rather than traverse every region again. Constructor
search and compatible evaluation reuse are separate remaining costs.

## 12. Structural/rank reuse and selective regional realization

The next step removes repeated regional structure work and skips unchanged
local coordinate realizations, without removing candidates or changing the
selection/quality rules.

- Projection to children, deterministic cycle breaking and ranks, optional
  simplex ranking, long-edge chains, border nodes and component grouping are
  now a size-independent structural plan. Reuse is local to one complete
  layout on its immutable graph/options. Ordered children, structural options
  and relevant ancestor reversal decisions form the key. An ancestor's
  changed reversal invalidates border construction; coordinates, channel
  gaps and earlier rank order do not belong in this structural key.
- After children and ordering are updated, a full-view hierarchical region
  compares its current sizes, local gaps, ordered ranks, structural inputs,
  child border ranks/geometry, fixed-order flag and ordering mode. An exact
  match restores its local boxes, complete area plan and chosen orientation.
  Changed child extents or borders invalidate its parent. Floating geometry
  inputs are compared by bits, not approximate equality.
- Child interiors are never replaced by a parent's cached boxes. A channel
  growth in another independent region does not invalidate this region.
  Partial/warm-start and flattened placement do not use realization reuse;
  global-flattening scratch layouts have separate memos.

A targeted test stabilizes two independent containers, then grows a channel
in the left one. **All three structural plans are reused; only the left
container and the root realize coordinates again. The right container is
reused.** Boxes, extents and full internal area plans match an uncached run.

For the four ordering jobs of the frozen code view:

| Regional work | Before this step | After this step |
| --- | ---: | ---: |
| Structural projections/rank constructions | 1014 | 156 computations + 858 reuses |
| Local coordinate realizations | 1014 | 367 computations + 647 reuses |
| Cross-flow plans | 329 computations + 685 reuses | 329 computations + 38 reuses |
| Top-level placement entries | 26 | 26 |

The lower cross-flow reuse count is intentional: 647 regions now skip that
phase entirely by reusing their complete local realization. It does not mean
their lanes are recalculated. Workflow records 36 structural computations,
72 structural reuses and 36 coordinate reuses; org chart records 136/204/196.
The hierarchy is still traversed for child extents and dependency checks.
This is not yet a persistent scheduler which skips whole subtrees.

### Frozen-input paired release diagnostic

[Raw results](../benchmarks/results/structural-split-20261104.jsonl) record
Apple M3, eight logical CPUs, macOS/aarch64, Rust 1.99.0, release, revision
`e966e93` with dirty worktree, binary/input/source hashes. The preserved
baseline includes E1/E2 and the phase separation of sections 10–11. Each
binary has one warmup and three timed repetitions on identical frozen
inputs, with threads 1 and 4.

| View, limit 150 | Core refinement before, 1 thread | After, 1 thread | Before, 4 threads | After, 4 threads |
| --- | ---: | ---: | ---: | ---: |
| Code | 1339.270 ms | 1293.024 ms | 494.792 ms | 482.670 ms |
| Workflow | 85.623 ms | 83.914 ms | 32.906 ms | 32.778 ms |
| Org chart | 73.119 ms | 71.206 ms | 32.679 ms | 31.841 ms |

Recorded one-minute machine load is roughly **23–35 on eight logical CPUs**.
These are diagnostic observations, not a controlled speedup claim. The code
median is about 3.5% lower at one thread and 2.4% lower at four threads;
candidate placement time, with the same timing boundary, is 382.731 →
353.261 ms. Initial import is **207.459 → 212.482 ms**, so this run does not
establish an initial-layout gain. Cache/key construction also has a cost.
No new matched ELK comparison or idle-machine sweep was run.

All three views keep initial geometry, full refined/core fingerprints,
published keys and routing issues. Threads 1/4 still produce identical
snapshots. Constructor searches/cells/transitions and constructed/delegated
route counts are unchanged. All seven code-view candidate occurrences keep
their IDs, input identity, outcome, geometry, metrics, keys, duplicate/archive
ranks and accepted/selected/published flags; the
[candidate log](../benchmarks/results/structural-split-candidates-20261104.jsonl)
includes the new work counters.

Validation covers ancestor-reversal invalidation, unrelated-gap isolation,
child-border and size invalidation, selective channel-growth propagation,
all five ordering modes on the frozen feedback graph, and a bounded
fixed/free-direction matrix with fractional sizes, component packing and
simplex ranking. Existing global-layering/cached-versus-uncached tests and
the frozen runtime fingerprint/thread checks pass. Workspace fast-profile
tests, workspace check and the playground/WASM build pass. The unrelated
timing-only organic unit-test compile issue remains as previously reported.

The deterministic work reduction is substantial in the reused regions,
but the total wall-time observation is modest. The constructor still does
435 searches and roughly 55 million clear lane transitions on the code view;
evaluation and routing are not reduced by this change. Those costs should
be measured separately before extending geometry caching further.

## 13. Geometry-free routing shadow and per-edge search attribution

Implemented on `feature/abstract-routing-plan`, based on `d14db45`.
This is the first diagnostic contract for abstract routing, **not** replacement
routing or early candidate pruning. No third-party code was adopted.

`layout::abstract_routing::build` consumes the graph and ordered regional plans.
It reads containment, ranks, long-edge chains, cycle-reversal decisions and
border identities, but no boxes, dummy rectangles or physical port/lane
offsets. The hierarchical section traversal is shared with the existing
constructor. Logical edge direction remains separate from construction
direction; parallel edges have distinct discrete incident lanes. Every edge
has either a complete sequence of channel sections/boundary connections or an
explicit unsupported reason. Self-loops and explicit/row-bound ports are
reported as unsupported by this first planner, not dropped from the engine.
The existing router still handles them.

Channels report deterministic order preferences, broken preferences from cycle
removal, ordinal crossings and conservative interval slots. These quantities
are **not physical widths, geometric lower bounds or infeasibility proofs**:
element positions on different ranks need not align, and sizes, labels,
clearance and fixed port positions are not modeled yet. Long-edge dummies
preserve their parallel lane order. Nested boundary crossings are explicit;
their geometric alignment remains a realization requirement.

`bench --abstract-plan 1` exports initial/final plans and, with
`--candidate-log DIR`, plans for materialized candidates. Analysis happens after
the timed engine calls. The ranks currently come from completed placement:
planning has not yet been moved ahead of coordinate assignment. Neither
candidate admission nor acceptance uses these estimates (`safeToPrune: false`).

Benchmark-only constructor events now identify each internal DP call by
logical edge and phase (`lane`, `topology`, `boundTracks`, `freeTracks`,
`alternateTopology`), with failure/cell/transition counts. Planned-route
obstacle and occupied-track failures also carry edge IDs. Event recording
does add overhead inside benchmark engine clocks and is absent from ordinary
and WASM builds. Events accumulate across routing passes; a candidate's shadow
describes its final ordered plans, not a reconstruction of every earlier pass.

### Frozen-view diagnostic, not a speedup measurement

Reproduce the checked-in
[diagnostic result](../benchmarks/results/abstract-routing-shadow-20261104.jsonl):

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/abstract-routing-check.mjs target/fast/bench
```

The script uses the checked-in 188-node/358-edge
[feedback fixture](../examples/fixtures/phase-width-feedback.json), records
input, binary and Rust source hashes, and compares shadow on/off plus all seven
candidate occurrences against the committed structural-split baseline.
Apple M3, eight logical CPUs, Rust 1.99.0, `fast` profile (the older benchmark
environment labels optimized builds `release`; the result's `binary.profile`
records `fast`). No wall-time comparison or new ELK sweep is claimed.

Initial and final topology: **349/358** edges structurally planned, **1,781**
channel sections in **95** channels; eight self-loops and `e217` (missing chain)
explicitly unsupported. The model reports 1,175 interval slots in total, at
most 108 in one channel, and 168 broken ordering preferences. These are
diagnostic model counts, not required physical tracks.

| Ordering candidate | Distinct searched edges | Structurally planned among them | DP calls | Planned obstacle failures | Planned track failures |
|---|---:|---:|---:|---:|---:|
| Forward barycenter | 39 | 38 | 114 | 25 | 13 |
| Alternating barycenter | 41 | 40 | 125 | 27 | 13 |
| Forward median centered | 37 | 36 | 107 | 25 | 11 |
| Alternating median centered | 32 | 31 | 89 | 28 | 3 |

Across these candidates: the same **435** DP calls, **128** unsuccessful calls,
**1,388,434** cells and **55,055,505** transitions as before. Every constructor
search edge except `e217` already has a structural path. Most remaining search
therefore does not need discovery of connectivity: it follows **105**
obstacle failures and **40** occupied-track failures of geometric planned
routes. This motivates realizing consistent lanes/slots and adequate clearance
from the abstract plan, rather than adding more generic path searches.
It does not prove that the shadow's current slots eliminate those failures.

The estimates alone are unsafe pruning criteria: alternating barycenter has
only six broken preferences, against forward barycenter's 156, but a worse
published geometric key. The routing-only winner keeps the initial abstract
plan. The full candidate portfolio is retained.

Validation: seven new planner tests cover geometry independence, all four
axis/mirror combinations, nested cycle reversal and multiple boundaries,
flattened membership, parallel IDs/lanes, self-loops, unresolved edges,
explicit ports, missing/invalid structure, order preferences and interval-slot
separation. Frozen runtime tests check non-mutation, deterministic shadows at
threads 1/4 and exact per-edge counter aggregation. Initial/final fingerprints
and all seven candidate identities, geometries, metrics and selection/
publication decisions match the baseline; final geometry remains
`19409ef4dbf00e51`, snapshot `9fc9f367016cb818`.
Workspace fast-profile tests, workspace check, formatting and the complete
small-fixture playground/WASM build pass. The focused runtime test also
passes with `benchmark-timings`; the previously reported unrelated organic
feature-unit-test compile issue has not been changed.

Next: realize abstract port/lane continuity and slot demand under actual node,
border, label and clearance constraints, compare per-edge failures, then replace
the corresponding searches. Pre-coordinate scheduling and any early heuristic
strategy rejection are separate later changes.

## 14. Physical lanes and fixed-order channel-width realization

The next prototype realizes abstract sections on real geometry, without
calling the constructor DP or generic path search. It is an opt-in diagnostic
(`bench --abstract-realization 1`), **not the production routing path**.
No new dependency, copied third-party code or default option was added.

### What is implemented

- Resolved logical endpoints remain fixed. A long-edge dummy keeps one
  parallel-lane order through both sides, and a nested border junction uses
  one absolute lane across the two regional coordinate frames.
- Discrete channel order is retained, but ordinal slots are **not** mistaken
  for physical overlap: the existing exact `track_slots` recurrence runs on
  actual lane intervals with real track spacing.
- Required channel space is `2 * margin + (tracks - 1) * track_spacing`.
  Insufficient space and insufficient dummy lane capacity are explicit.
  The trial never compresses tracks to fit.
- Before accepting a route into the trial's occupancy, it checks orthogonal
  monotonicity, constructor-inflated obstacles and spacing to accepted
  parallel segments. Logical cycle direction and resolved endpoints are kept.
  Every edge has a trial route or a named reason for delegation.
- A separate `growth::widen` trial recomputes demand from its exact input,
  realizes the flow-axis rank gaps in one bottom-up traversal, and propagates
  child growth to parent outlines. Container moves translate all descendants;
  dummy rectangles stay local to their area. Cross coordinates, cross
  extents, rank order and at least the existing gaps are kept. It invokes
  neither complete placement nor path search.
- The growth trial conservatively refuses pins, fixed container dimensions,
  placement grids, maximum extents, flattened membership and mixed/reversed flow. Structural self-loops,
  explicit/row-bound ports and missing chains retain their unsupported status.
  Neither labels nor a complete candidate quality key are evaluated.

Both original-geometry and widened-geometry trials are exported for the
initial/final snapshots and materialized candidates. They never replace
snapshot geometry, publish a result or reject a candidate. The existing
router still supplies the complete public result.

### Reproducible bounded result

[Raw diagnostic result](../benchmarks/results/abstract-routing-realization-20261104.jsonl):

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/abstract-routing-check.mjs target/fast/bench --realize
```

Same checked-in 188-node/358-edge feedback fixture, SHA256
`d1a5fbb559bde520b41236bd603e1dee0954ad458da29eddc223ed827964a322`.
Apple M3, eight logical CPUs, Rust 1.99.0, `fast` profile; the raw record
includes revision/dirty state, input/binary/source/generator hashes and
the machine line. This is a correctness/coverage experiment, **not a
speedup measurement or new ELK comparison**.

| Input geometry | Search-free trial routes before growth | After growth | Narrow channels before | Sum of missing flow space before | Longitudinal conflicts after |
|---|---:|---:|---:|---:|---:|
| Initial / final | 112 | 263 | 9 | 290 | 86 |
| Forward barycenter | 122 | 265 | 9 | 442 | 84 |
| Alternating barycenter | 135 | 269 | 6 | 302 | 80 |
| Forward median centered | 125 | 264 | 9 | 478 | 85 |
| Alternating median centered | 145 | 267 | 7 | 314 | 82 |

After **one** bottom-up width realization, all these trials have zero
remaining channel-width deficits and zero accepted-route obstacle failures.
On the initial/final geometry, 263 routes are accepted and 95 are delegated:
86 longitudinal segment conflicts, eight self-loops and the missing `e217`
chain. The overall node-outline bounding area grows by about 2.50% there;
the four ordering-candidate trials grow by roughly 2.49–4.08%.
These areas are trial node extents, not a fully labeled/routed scene metric.

Of the old constructor's distinct searched edges, 24/26/22/18 are accepted
without search in the four widened trials. **That is not a net search-saving
claim:** the new trial also refuses other edges the old constructor builds.
The existing constructor, including its DP searches, constructs
349/347/347/345 edges in those candidates; the new search-free subset is still
smaller. Replacing the default now would shift work to fallback and may lose
quality. No such replacement was made.

The residual diagnosis is more specific than “not enough channel space”:
all remaining geometric rejections in this view involve **flow-parallel /
longitudinal** segments, not the cross-flow channel tracks. Each rejection
records the accepted blocking edge, direction, separation and overlap.
Independent local dummy slots/channel ordering do not yet coordinate
longitudinal lane occupancy across neighbouring channels and container
transitions. More rank gap or another generic search is not the first fix;
that lane continuity/occupancy constraint belongs in the shared abstract plan.

### Checks and remaining boundary

Eight focused tests cover exact physical gap thresholds (including equality),
parallel dummy lanes/capacity, nested border origins, logical cycle direction,
obstacle versus occupancy failures, axis transposition, unsupported growth
constraints, numeric/input errors, and bottom-up child growth with translated
neighbour subtrees. Frozen runtime tests check unchanged ranks/cross geometry,
complete edge partition, zero search work and identical physical trials at
threads 1/4 without mutating the input.

The diagnostic script independently checks every emitted route's endpoints,
orthogonality, foreign-node clearance and inter-route parallel spacing. It
also verifies exact channel demand, no compressed tracks, container
containment, no new sibling overlaps, fixed cross geometry and zero deficits
after growth. Shadow on/off and all seven candidate occurrences retain the
committed baseline's geometry, metrics and selection/publication decisions.
Initial/final geometry fingerprints stay `79e64fa0fe98e5fe` /
`19409ef4dbf00e51`, snapshot `9fc9f367016cb818`.

The next step is a shared constraint for longitudinal lanes spanning channel
boundaries, followed by full-layout validation/quality and an explicit
experimental router integration. Pre-coordinate planning, explicit
port/row/label handling, unsupported-edge fallback and safe early candidate
rejection are not completed by these physical trials.

## 15. Global longitudinal lane reservations on fixed channel tracks

The next opt-in physical trial now coordinates longitudinal occupancy across
all regions, rather than assigning each dummy/border lane independently.
It is exposed as `coordinatedRealization` beside the two existing trials under
`bench --abstract-realization 1`. None is applied to a snapshot.

### Construction, not path search

1. Retain the widened drawing from section 14 and freeze its original
   cross-flow channel-track coordinates. No additional node movement or growth.
2. Reserve each longitudinal interval from the preceding bend to the following
   bend, including source/target stubs. A boundary junction uses the same
   absolute cross coordinate on both sides of the container.
3. Keep endpoints fixed. Interior lanes have the actual dummy/border rectangle
   as their cross-coordinate domain. No off-rectangle offset candidates.
4. Sweep the longitudinal intervals across **all** areas. Potentially colliding
   domains receive a track-separation constraint, ordered by preferred cross
   coordinate, edge identity and junction index. This total order makes a DAG.
5. Propagate minimum values forward. An edge that exceeds its domain is
   explicitly delegated as `longitudinalCapacity`; all its reservations are
   excluded from final assignment. Propagate upper bounds backwards, then
   select values near the original preference in one final forward pass.
   There is no retry/reordering to maximize the retained edge set.
6. Materialize routes once with these lanes and the frozen bends. A section
   that was straight can use an existing channel track, or the centre of an
   otherwise straight channel. Missing width is still explicit. Final
   monotonicity, inflated-obstacle and accepted-segment checks remain intact.

The first formulation reserved whole channel windows before fixing bend
coordinates. It was too conservative and reduced coverage, so it was replaced
by reservations of the actual fixed-track intervals. This is still a physical
prototype consuming completed ranks/geometry, **not** the independent
pre-placement abstract scheduler.

Capacity failures are relative to the selected order and drawing. They are
neither graph infeasibility proofs nor safe reasons to prune refinement
candidates. A failure at a fixed port does not mean that authored port
constraints should be relaxed. The complete production constructor remains
the public router.

### Reproducible bounded coverage

[Raw result](../benchmarks/results/abstract-routing-continuity-20261104.jsonl):

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/abstract-routing-check.mjs target/fast/bench --realize
```

Same 188-node/358-edge frozen fixture and SHA256 as section 14, Apple M3,
eight logical CPUs, Rust 1.99.0, `fast` profile. The raw record identifies the
revision/dirty state and exact input, source, generator and binary hashes.
Trial analysis is outside the engine clocks. **No runtime speedup, new ELK
comparison or complete-scene quality improvement is claimed.**

| Input geometry | Uncoordinated widened routes | Coordinated routes | Order-specific capacity failures | Final cross-flow conflicts | Gained / lost route IDs |
|---|---:|---:|---:|---:|---:|
| Initial / final | 263 | 308 | 41 | 0 | 66 / 21 |
| Forward barycenter | 265 | 312 | 37 | 0 | 64 / 17 |
| Alternating barycenter | 269 | 303 | 45 | 1 | 64 / 30 |
| Forward median centered | 264 | 303 | 44 | 2 | 60 / 21 |
| Alternating median centered | 267 | 300 | 48 | 1 | 61 / 28 |

All trials retain zero channel-width deficits, no final longitudinal conflicts
and no accepted-route clearance violations. The same nine structural
delegations remain (eight self-loops and missing chain `e217`). On the initial
geometry there are 2,130 reservations and 2,520 separation constraints,
including 1,370 between different owning areas; 95 lane coordinates move.
Of the 41 selected-order capacity failures, 13 occur at a source reservation,
14 at a target and 14 at an interior junction.

The four coordinated ordering trials accept 32/33/26/27 of the distinct edges
that the existing constructor searched. This still does not establish a net
fallback or CPU reduction: the old constructor builds 349/347/347/345 edges,
more than these incomplete search-free trials. The table also explicitly
shows losses: the coordinated result is **not a superset**, and combining the
two trial route sets without revalidating their joint occupancy is invalid.

### Verification and next boundary

Focused tests exercise exact spacing and fixed endpoint bounds, look-back
upper-bound propagation, 512 generated ordered systems (including several
reservations owned by the same edge), two independently placed junctions
sharing longitudinal space, LR/TB transposition, nested absolute border
origins and reversed logical direction. Frozen runtime tests compare complete
coordinated diagnostics/routes at threads 1/4 and verify input immutability.

The independent JavaScript check validates every route, every emitted lane
domain, each retained separation constraint and correspondence between
reservations and the actual route segments. It also checks fixed channel
tracks, unchanged node boxes/resolved ports, complete edge partition and the
committed seven-candidate geometry/metric/selection baseline. Existing public
fingerprints remain `79e64fa0fe98e5fe` / `19409ef4dbf00e51`, snapshot
`9fc9f367016cb818`.

The next algorithmic step is **joint endpoint/lane/channel-order planning
before tracks are frozen**, not blanket channel widening. Endpoint failures
and the few remaining cross-flow collisions show where the current separate
decisions are incompatible. Full labels/scene quality, unsupported-edge
fallback, early proven pruning and production integration remain open.

## 16. Endpoint-aware joint bend order and lane compaction

The next separate trial releases the frozen channel-track decision from
section 15. It derives bend precedence from actual endpoint and virtual lane
domains **before** compacting the bends and longitudinal lanes. This is
`jointRealization` under `bench --abstract-realization 1`; the prior
`coordinatedRealization` remains available for a direct coverage comparison.
No trial is applied to public geometry, candidate selection or snapshots.

### One selected separation plan

Every section receives a bend variable bounded by its existing rank gap,
clearance margins and resolved endpoint flow coordinates. Its preferred
position comes from the previous channel assignment, or the channel centre
for an originally straight section. Every junction retains the same cross
domain as section 15; endpoints stay fixed.

The planner sweeps nominal longitudinal intervals. If two overlapping
intervals cannot fit the **preferred lane order** at the required track
distance within their domains, it selects an alternative: one interval must
end before the other starts along flow. That becomes a bend precedence, or
a bend bound tightened against a fixed endpoint. When both temporal orders
are locally feasible, the earlier nominal interval wins deterministically.
When neither is feasible, the pair stays for explicit lane-capacity diagnosis.
This is selected-order reasoning, not a disjunctive feasibility solver.

Along-edge nondecreasing bend order and these temporal decisions form a
directed graph. A stable topological order uses original bend positions and
identity as tie-breaks. Nominal overlapping cross-flow spans in each channel
receive a track-distance constraint in that order. Cycles, including their
downstream blocked edges, are explicitly delegated as `jointOrder`, without
cycle-breaking search. These are selected-order failures, not evidence that
the graph cannot be routed.

One forward minimum pass detects `jointCapacity` failures; backward upper
bounds and a final forward pass compact the retained bends near their
preferences. Each channel has an exact track grid rooted at its first
clearance-safe coordinate. Spacing is never compressed. The resulting bend
coordinates feed the existing global bounded lane compactor. No node is
moved or resized, no port is reassigned, no additional placement/path search
or iterative retry occurs.

All materialized routes still pass the full monotonicity, inflated-obstacle
and accepted-segment checks. Nominal cross spans can change when lanes move,
so the bend constraints alone are not a full clearance/occupancy proof.
Neither labels nor full-scene quality are evaluated.

### Reproducible bounded result

[Raw result](../benchmarks/results/abstract-routing-joint-20261104.jsonl):

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/abstract-routing-check.mjs target/fast/bench --realize
```

Same frozen 188-node/358-edge input and SHA256 as sections 14/15. Apple M3,
eight logical CPUs, Rust 1.99.0, `fast` profile. Revision/dirty state and exact
source, input, generator and binary hashes are recorded. The independently
verified trials run outside engine clocks. **This is route coverage, not a
runtime speedup, new ELK comparison or complete-layout quality result.**

| Input geometry | Fixed-track coordinated routes | Joint-order routes | Lane-capacity failures | Bend-capacity failures | Gained / lost route IDs |
|---|---:|---:|---:|---:|---:|
| Initial / final | 308 | 327 | 20 | 2 | 22 / 3 |
| Forward barycenter | 312 | 339 | 9 | 1 | 34 / 7 |
| Alternating barycenter | 303 | 339 | 10 | 0 | 40 / 4 |
| Forward median centered | 303 | 337 | 12 | 0 | 37 / 3 |
| Alternating median centered | 300 | 343 | 5 | 1 | 46 / 3 |

These trials have zero remaining channel-width deficits, no cycle-blocked
edges and no final obstacle, longitudinal or cross-flow conflict rejections.
The nine structural delegations remain: eight self-loops and missing chain
`e217`. On initial/final geometry, 29 selected temporal orders precede
7,559 nominal cross-flow separation constraints; 1,136 bend coordinates move
relative to the old preferences. Of the 20 remaining lane-capacity failures,
nine occur at source reservations, ten at targets and one at an interior
junction. The four ordering candidates use 41/42/37/53 temporal orders.

The result is still **not a superset** of the fixed-track trial, as the table
records. The four joint ordering trials accept 35/38/32/31 of the distinct
edges that the old constructor searched, but the complete old constructor
still builds 349/347/347/345 edges, more than the joint subsets. Fallback
integration, changed geometry/quality and the joint planner's own CPU cost
must be measured together before any optimization claim or router replacement.

### Verification and next boundary

Focused tests include an adverse initial channel order where the fixed-track
trial admits one edge but endpoint-aware bend order admits both, exact grid
capacity/equality, nonintegral grid origins, backward bound propagation,
cycle/downstream blockage, LR/TB transposition and reversed logical direction
through nested absolute border geometry. Another 512 generated grid DAGs,
including multiple reservations per edge and differing grid origins, verify
every retained difference constraint and domain. Frozen runtime tests enforce
327 routes, complete edge partition, zero search and identical complete joint
diagnostics at threads 1/4 without mutating the input.

The independent JavaScript check validates all emitted routes, bend domains,
retained bend precedences, selected temporal orders, lane domains and
separations. It verifies unchanged node boxes/resolved ports, exact channel
spacing, no compressed deficits and the committed seven-candidate geometry,
metric and selection/publication baseline. Existing public fingerprints stay
`79e64fa0fe98e5fe` / `19409ef4dbf00e51`, snapshot `9fc9f367016cb818`.

Remaining capacity failures need **full conflict-chain reasoning and order
selection**, not blanket channel widening. This prototype resolves local
pair incompatibilities, but does not reconsider lane order or lift an entire
overfull dependency chain into alternative temporal constraints. It still
consumes already placed ranks and rectangles, not an independent
pre-coordinate scheduler. Those limitations, complete scene/label quality
and unsupported-edge fallback remain explicit before production integration.

Validation completed: workspace formatting, `cargo check --workspace --locked`
with and without `arqlet-runtime/benchmark-timings`,
`cargo test --workspace --locked --profile fast`, the focused instrumented
runtime regression, the independent diagnostic script and
`sh apps/playground/build.sh examples/fixtures/small.json` (WASM/smoke,
revision, backend and UI regressions).

## 17. Propagated capacity witnesses and one chain-cut replan

The selected-order lane failures now include the **whole propagated
predecessor path**, not just the final junction that exceeds its domain.
The new separate `chainRealization` uses those paths to derive temporal
orders and performs one bounded replan. The default router and all earlier
trial stages remain unchanged.

### Why pairwise feasibility missed these failures

For each propagated lane minimum, the compactor retains the predecessor that
last strictly increased it. At a failure it traces that path back to a
domain lower bound and records:

- logical edge/junction identities and reservation indices for the path;
- first lower bound, last upper bound and required track distance;
- the positive deficit and every path link in the separation DAG.

The initial joint trial's 20 lane failures have path lengths **2 once,
3 eighteen times and 6 once**. Thus 19 of the 20 involve longer propagation,
not just a directly incompatible pair. Their maximum deficit is six units.
A three-reservation test demonstrates the distinction: each adjacent pair
individually fits six-unit separation, but the complete chain needs twelve
units between endpoints with only six available.

These are **historical propagation witnesses**. Earlier failed-edge removal
can already have relaxed part of a path without recomputing propagated
minima. The certificates prove the recorded selected bounds/path deficit,
not infeasibility of the graph or the currently retained edge subset. They
are diagnostic input, never a rule for pruning refinement candidates.

### Exactly two planning passes, not a new route search

1. Probe the joint bend/lane compactor on private clones of the original
   trial state. No probe route is built or admitted to final occupancy.
2. For every capacity witness, inspect its adjacent path links in both
   temporal directions. A cut is locally admissible if the first interval's
   earliest possible end is no later than the other's latest possible start,
   using the probe bend bounds and unchanged fixed endpoints.
3. Select at most one cut per witness: least nominal flow displacement,
   then most bound slack; ties retain stable path/direction order.
   Deduplicate identical temporal orders. A witness without such a link is
   explicitly recorded as uncut.
4. Replan **once** from the original lane preferences and channel assignments,
   adding these temporal orders before the joint planner's topological
   compaction. Then compact longitudinal lanes and materialize/check the
   final routes once.

There is no retry loop, best-of-two selection, port reassignment, extra
node/container growth or geometric path search. Local cut feasibility does
not imply simultaneous feasibility: cuts can introduce cycles or move a
capacity failure elsewhere. All such failures remain explicit; the earlier
joint trial is preserved to expose gains **and losses**.

### Reproducible bounded result

[Raw result](../benchmarks/results/abstract-routing-chains-20261104.jsonl):

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/abstract-routing-check.mjs target/fast/bench --realize
```

Same frozen 188-node/358-edge input and SHA256 as sections 14-16. Apple M3,
eight logical CPUs, Rust 1.99.0, `fast` profile. Revision/dirty state and exact
source, input, generator and binary hashes accompany the machine line.
All trial work is outside engine clocks. **No runtime speedup, new ELK
comparison or complete-scene quality improvement is claimed.** In
particular, the two planning passes have a cost that is not removed by
reporting zero path searches.

| Input geometry | Joint routes | Chain-cut routes | Lane-capacity failures | Bend-capacity failures | Cycle/downstream-blocked edges | Gained / lost route IDs |
|---|---:|---:|---:|---:|---:|---:|
| Initial / final | 327 | 347 | 0 | 2 | 0 | 21 / 1 |
| Forward barycenter | 339 | 344 | 2 | 1 | 2 | 7 / 2 |
| Alternating barycenter | 339 | 347 | 1 | 1 | 0 | 9 / 1 |
| Forward median centered | 337 | 344 | 3 | 0 | 2 | 10 / 3 |
| Alternating median centered | 343 | 346 | 2 | 1 | 0 | 4 / 1 |

The probe contributes 20/9/10/12/5 distinct temporal cuts in these five
inputs; none of those probe witnesses is uncut. All final trials retain
zero channel-width deficits and no final obstacle, longitudinal or cross-flow
conflict rejections. Node boxes, resolved ports and total node extents are
identical to the existing widened trial. Nine structural delegations remain
(eight self-loops and missing chain `e217`).

The four ordering trials accept 37/40/34/31 distinct edges that the old
constructor searched. Their complete constructor counts remain
349/347/347/345, against these incomplete search-free subsets
344/347/344/346. Even an equal or larger route count does not prove a
superset, unchanged quality or a net reduction of fallback work. It also
does not route the structural exceptions. Full routing and CPU comparisons
must include fallback, final validation, labels and both planning passes.

### Checks and remaining boundary

The focused tests cover a transitive deficit whose adjacent pairs fit,
generated witness path/deficit invariants, impossible fixed-endpoint cuts,
LR/TB transposition, nested borders and reversed logical direction.
The frozen runtime regression checks 347 routes, zero remaining lane
failures, exact probe equivalence to the original joint trial, input
immutability and complete diagnostic/route determinism at threads 1/4.

The independent JavaScript check validates every witness link, domain
bound, required distance and deficit; every chosen cut's membership in its
witness, local temporal feasibility, displacement/slack and deterministic
selection among adjacent alternatives; and its inclusion in final bend
constraints. Probe diagnostics must exactly equal the separate joint
trial, proving original rather than mutated lane inputs are used. Final
route endpoints, orthogonality, clearance, occupancy, lane/bend constraints,
node/port immutability and edge partition remain independently checked.
The committed seven-candidate geometry/metric/selection baseline and public
fingerprints remain unchanged (`79e64fa0fe98e5fe` / `19409ef4dbf00e51`,
snapshot `9fc9f367016cb818`).

The useful next boundary is **complete experimental routing with explicit
fallback and full quality/cost measurement**, while diagnosing remaining
bend-capacity failures and simultaneous cut cycles. Adding an unbounded
chain-retry loop would simply hide repeated planning work. The current
prototype still consumes placed geometry, not an independent pre-coordinate
scheduler, and must not silently replace the production router.

Validation completed: `cargo fmt --all -- --check`, workspace checks with
and without `arqlet-runtime/benchmark-timings`,
`cargo test --workspace --locked --profile fast`, the focused instrumented
runtime regression, diagnostic script syntax and independent geometry/cut
verification, and `sh apps/playground/build.sh examples/fixtures/small.json`
(WASM/smoke, revision, backend and UI regressions).

## 18. Detached complete routing, fallback and full quality

The chain trial can now be completed with explicit fallback through a
separate provisional `abstract_realization::completion::run` API and
`bench --routing-completion 1`. It never installs its geometry in a snapshot,
changes defaults or participates in candidate acceptance.

### Matched boundary and reproduction

Both alternatives start from the same frozen imported view, run the same
fresh channel-growth pass, and receive identical grown boxes and input
ports/plans. One uses the existing constructor, the other the two-pass chain
planner. Each completes its delegated edges with the existing legacy router,
preserving all constructed routes, and rebuilds boundary crossings, labels,
geometry issues and the scene before evaluating the shared metrics/key.
The existing constructor may change automatic slots; these changes are
propagated to its seeds/ports, not silently discarded. On this fixture the
resulting endpoint positions happen to match between alternatives.

Raw evidence:
[`abstract-routing-completion-20261104.jsonl`](../benchmarks/results/abstract-routing-completion-20261104.jsonl).
Machine/revision: Apple M3, eight logical CPUs, macOS/aarch64, Rust
1.99.0, `d14db45` with uncommitted changes. Binary profile is **fast**, without
LTO; the older environment helper labels optimized builds generically
`release`, so the separate `binary.profile` is authoritative. Load average
was `{ 5.80 6.40 7.42 }`: shared-machine diagnostic, not an idle-machine
speedup claim. Source, binary, generator and input hashes are in the result.

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/routing-completion-check.mjs target/fast/bench 5
```

One unmeasured warmup per alternative, then five paired measurements with
alternating execution order. Total wall time includes validation/plan decode,
growth, construction, seeds, fallback, cloning, labels, scene construction,
shared scoring and strict track validation. Import and prior placement,
process/JSON transport, refinement and diagnostic JSON export are excluded.
Per-edge benchmark instrumentation remains enabled in both alternatives.
Phase medians need not sum to the total median.

### Observed cost and work

| Phase, median ms | Existing constructor | Chain planner |
| --- | ---: | ---: |
| Common fresh growth | 14.836 | 14.816 |
| Construction / two-pass plan and realization | 168.473 | 46.232 |
| Fallback seeds | 0.444 | 0.446 |
| Delegated routing | 50.735 | 14.019 |
| Finalization, labels and scene | 6.595 | 6.616 |
| Shared evaluation and strict track validation | 11.045 | 16.255 |
| **Complete detached experiment** | **252.993** | **99.038** |

These are different algorithms and outputs, not output-identical optimization.
They do not measure an integrated replacement pipeline, the four-candidate
refinement loop or an updated ELK comparison.

The existing constructor accepts 334 edges on this exact grown geometry and
delegates 24; it performs 137 internal searches, 571,310 cells and 33,072,243
clear lane transitions. The chain planner accepts 347 and delegates 11, with
zero internal path searches, states or transitions. Legacy fallback still
evaluates 6,380 candidate routes (versus 13,920); it is not search-free even
though its grid-pop count is zero here (versus 34,920).

### Complete routes do not mean complete correctness or equal quality

Every one of 358 logical edges is present exactly once in both scenes.
Endpoints, logical direction, self-loops and parallel edges are retained;
constructed routes are unchanged by fallback. Both drawings have zero
geometry issues, route-through-node diagnostics and node/container hits.

| Shared metric / diagnostic | Existing constructor | Chain planner |
| --- | ---: | ---: |
| Metric hard violations | 2 | 0 |
| Shared-line pairs | 2 | 0 |
| Strict constructor track-conflict pairs | 73 | 4 |
| Of those: fixed/fallback | 71 | 4 |
| Of those: fallback/fallback | 2 | 0 |
| Of those: fixed/fixed | 0 | 0 |
| Near-parallel pairs (shared metric) | 88 | 21 |
| Bends | 1,752 | 2,416 |
| Crossings | 3,183 | 4,435 |
| Area | 91,470,046.38 | 91,470,046.38 |

The legacy fallback's improvement passes receive fixed occupancy, but its
final repair/separation passes do not include it. Remaining conflicts are
therefore explicit, not treated as a successful spacing guarantee. The four
chain-trial pairs are `e302/e449`, `e449/e655`, `e449/e661` and `e471/e486`.
The shared hard key does not encode every strict parallel track distance;
`hardMetricValid` and `strictTrackValid` are separate reported booleans.
No result is automatically accepted.

The new plan removes internal path search but increases bends by about 38%
and crossings by about 39% against the matched existing constructor. Its
evaluation phase also costs more. Its area is about 2.5% above the original
import because both alternatives deliberately share the widened trial
geometry. This is a useful cost experiment, not evidence for a default switch.

### Checks and next boundary

The independent JavaScript check validates complete fixed/fallback partitions,
fixed-route identity, logical source/target, exact resolved endpoint positions,
ports on node boundaries, finite orthogonal routes, identical grown nodes,
and independently enumerates every strict track-conflict pair. Phase samples
and repeated scene fingerprints are checked. Runtime tests cover LR/TB,
cycles, loops, parallel edges, measured labels, explicit unsupported-input
errors and unchanged input snapshots. The existing frozen regression also
compares complete diagnostic scenes after refinement at threads 1/4; the
ordinary seven-candidate/public-output baseline remains unchanged.

The next substantive boundary is to include fixed routes in fallback
clearance/spacing decisions without modifying accepted chain routes, and to
choose abstract orders for fewer bends/crossings rather than coverage alone.
Growth still materializes an earlier trial and the chain planner still
prepares from placed geometry: this is **not** pre-coordinate scheduling.
Sharing geometry-independent planning and avoiding redundant trial materialization
remain experiments to measure, not claimed speedups.

Validation completed: workspace formatting and checks with/without
`benchmark-timings`; full workspace tests in the fast profile; all 17
instrumented layered-construction integration tests; independent completion
and shadow/candidate geometry checks; and the playground build on the small
fixture (WASM/smoke, revisions, backend and 55 UI regressions). The ordinary
initial/final geometry fingerprints and seven-candidate baseline are unchanged.
All 136 recorded Rust source/manifest hashes, the generator and the measured
binary were verified against the persisted result after validation.

## 19. Fixed-occupancy final compaction with explicit quality cost

The separate completion experiment now has a third variant,
`chainsFixedTracks`: the same chain plan and legacy fallback as section 18,
followed by provisional final compaction that sees both fixed and moving
occupancy. It is not enabled in the ordinary engine, candidate portfolio,
repair-only paths or refinement acceptance. The old two variants remain in
the measurement, with unchanged scene fingerprints.

### Why simple shifts were insufficient

The four remaining pairs involve only two delegated edges, `e449` and `e471`.
Moving an interior bend alone can transfer a conflict to another constructed
route. In particular, shortening `e471`'s long departure leg lengthens its
arrival leg into another occupied track. Relaxing the no-new-partner rule
would hide this transfer, not solve it.

The final repair instead uses exact geometric events: parallel segment
coordinates plus/minus the track distance, interval endpoints, and corridor
bounds from the existing obstacle rules. It can shift an interior segment or
insert a local orthogonal bypass around the occupied span. Bypass ends come
from the blocking intervals and track distance, not a list of arbitrary
offsets. Each route partner's blocked span and their joint span are tried.
The bypass uses its own local corridor span, not an unnecessarily restrictive
bound from a longer unmodified segment.

Every accepted move must remove at least one conflicting partner and add none,
preserve exact endpoints and first/last segment direction, remain finite and
orthogonal, and introduce no node/clearance hit. Equal conflict counts prefer
fewer points, then smaller displacement; route, segment and coordinate order
provide stable ties. Thus the number of successful moves is bounded by the
initial conflict-pair count. This is bounded geometric candidate enumeration,
**not search-free work** or another grid/path search. Candidate evaluations,
accepted moves and passes are exported. Unrepairable pairs remain explicit;
fixed/fixed conflicts are diagnosed separately and never repaired by moving
the constructed routes.

### Matched cost and quality

Raw evidence:
[`abstract-routing-fixed-tracks-20261104.jsonl`](../benchmarks/results/abstract-routing-fixed-tracks-20261104.jsonl).
Machine/revision: Apple M3, eight logical CPUs, macOS/aarch64, Rust
1.99.0, `d14db45` with uncommitted changes, optimized **fast** profile
without LTO and with benchmark event recording. Load average was
`{ 4.29 3.90 5.15 }`: shared-machine diagnostic, not a controlled production
speedup. The result records source, input, generator and binary hashes.

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/routing-completion-check.mjs target/fast/bench 6
```

One warmup per variant, then six rounds of rotating/reversed variant order,
covering all six permutations. Every variant includes the common fresh growth
and the complete detached routing/labels/scene/metric boundary from section 18;
the new variant additionally includes final track repair. Import, previous
placement, process/JSON transport and refinement remain excluded. Compare
these variants within this result, not milliseconds across historical runs.

| Median ms / quality | Existing constructor | Chain baseline | Chain + fixed-track repair |
| --- | ---: | ---: | ---: |
| Construction | 177.303 | 47.728 | 47.760 |
| Legacy fallback | 52.335 | 14.490 | 14.504 |
| Additional track repair | 0 | 0 | 16.208 |
| **Complete detached experiment** | **263.357** | **100.775** | **117.093** |
| Constructed routes kept fixed | 334 | 347 | 347 |
| Delegated routes | 24 | 11 | 11 |
| Metric hard violations | 2 | 0 | 0 |
| Strict track-conflict pairs | 73 | 4 | **0** |
| Bends | 1,752 | 2,416 | 2,422 |
| Crossings | 3,183 | 4,435 | 4,437 |
| Near-parallel pairs (shared metric) | 88 | 21 | 17 |

All variants use identical grown node geometry; their area remains
91,470,046.38. The new variant evaluates 1,934 geometric candidates and
accepts two moves in two passes (the second verifies no further move).
Only `e449` and `e471` change. All 347 constructed routes, all node boxes and
resolved ports remain unchanged. The new phase adds no constructor/grid
searches: the chain constructor still has zero, and legacy fallback retains
6,380 candidate evaluations and zero grid pops. Every one of 358 edges remains
present, with no geometry issues, route-through-node diagnostics or shared-line
pairs. The scene fingerprint is `99f893fa9227106f`; the unchanged chain and
existing-constructor fingerprints are `9d8533f1a5bceba1` and
`a600e679fd5a46ea`.

The important limitation is **quality, not just cost**: fixing strict spacing
adds six bends and two crossings against the chain baseline, and its
published key is worse (the crossing component rises from 7,023 to 7,033,
including shared-endpoint crossings). The published key is not a complete
encoding of strict track spacing. This result is therefore neither silently
accepted nor offered as an output-equivalent optimization or default switch.
The substantial bend/crossing gap to the old constructor remains.

### Verification and next boundary

The independent completion checker enumerates all segment pairs, verifies
zero remaining strict conflicts, unchanged fixed routes/nodes/ports and exact
changed-edge identities, and checks changed fallback routes against foreign
node/clearance rectangles. It still validates complete edge partitions,
logical directions, endpoint attachment, orthogonality and repeated scene
fingerprints. The old two scene fingerprints remain stable.

Seven focused routing tests cover LR/TB transposition, exact spacing tolerance
and range-touch boundaries, perpendicular crossings, explicit unrepairable
shared departures, obstacle rejection, required own clearance, moving/moving
occupancy, invalid inputs/cancellation and 64 deterministic/idempotent
generated cases. Runtime tests cover the frozen zero-conflict result,
preservation of fixed routes and authored ports, loops/cycles/parallel edges
and labels; the full frozen regression also compares repaired scenes at
threads 1/4.

This closes the frozen view's fallback-spacing gap without changing the
accepted abstract plan. It does **not** solve general routing feasibility or
move these decisions before placement. The next larger step remains
quality-aware abstract order/strategy selection: prevent avoidable detours,
bends and crossings while keeping the explicit fixed-route spacing contract,
rather than accepting faster but worse plans or adding unbounded retries.

Validation completed: workspace formatting; workspace checks with and without
`benchmark-timings`; full workspace tests in the fast profile; all 17
instrumented layered-construction integration tests; independent completion,
shadow-plan and seven-candidate parity checks; and the small-fixture
playground build (WASM/smoke, revisions, backend and 55 UI regressions).
Initial/final public geometry and snapshot fingerprints remain
`79e64fa0fe98e5fe`, `19409ef4dbf00e51` and `9fc9f367016cb818`, with
thread-count parity at 1/4. The new result's 137 source/manifest hashes,
generator and measured binary were verified against the working tree.

## 20. Quality-aware lane planning, demand handoff and exact subplan reuse

The next opt-in diagnostic, `bench --routing-planning 1`, separates three
decisions without changing the engine default, candidate portfolio or runtime
acceptance. The complete detached drawing remains the unit of evaluation;
an ordinal estimate never proves infeasibility or permits early pruning.

### Planning and dependency contract

1. **Quality-aware plan.** Two fixed adjacent-swap sweeps improve a pairwise
   ordinal crossing proxy only on strict improvement, then recompute slots
   and explicitly broken dependencies. For each edge, ordered cross domains
   are partitioned into maximal runs with a common intersection. A centered
   constant lane per run minimizes lane changes for those supplied intervals,
   not global bends/crossings after inter-edge coordination. Endpoint domains
   are singletons and stay fixed. Capacity-chain and fixed-track repair still
   run, and final metrics remain authoritative.
2. **Demand before final flow coordinates.** Bottom-up growth now consumes
   the selected demand plan without constructing throwaway routes, joint
   plans or continuity repairs. Final materialization follows flow placement.
   This also preserves the old trial geometry/fingerprints. The plan still
   requires cross coordinates, node sizes and endpoint domains: this is not
   a fully geometry-free placement scheduler.
3. **Run-local reuse.** `planning::Session` keeps one replaceable entry per
   active edge/channel and prunes removed entries. Exact dependencies are the
   complete ordered ordinal assignments; every cross-domain bound; or ordered
   edge/section cross spans plus spacing. Float keys use exact bits, not
   tolerances or collision-prone geometry hashes. Topology/ranks are validated
   afresh; main-axis placement is independent of these cached values. Routes,
   obstacle decisions, final coordinates and scores are never cached.

This is **subplan reuse**, not incremental full-routing/placement/scoring or
integration into production refinement. It exposes the boundary for that work
rather than claiming it complete.

### Reproduction and bounded result

[Raw result](../benchmarks/results/abstract-routing-planning-20261104.jsonl):

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/routing-completion-check.mjs target/fast/bench 10 --planning
```

Same frozen 188-node/358-edge input as sections 18/19; Apple M3, eight logical
CPUs, macOS, Rust 1.99.0, dirty `d14db45` on `feature/abstract-routing-plan`.
The machine was shared (load `{ 11.96 8.38 6.56 }`), not idle. The result
records input/source/generator/binary hashes; `binary.profile: fast` is
authoritative, despite the generic environment helper's `release` label.
Each of five variants has one warmup and ten rotating/reversed repetitions.
Wall totals include preparation/validation, demand planning/growth,
construction, seeds, fallback, final repair, finalization and evaluation.
They exclude previous import/placement, process/JSON transport and ordinary
refinement. No production speedup or new ELK comparison follows from this.

| Median ms / quality | Existing | Chain | Chain + fixed repair | Aligned cold | Aligned warm |
| --- | ---: | ---: | ---: | ---: | ---: |
| Abstract plan | 0 | 0 | 0 | 7.225 | 6.017 |
| Demand / growth | 14.431 | 14.438 | 14.462 | 7.602 | 7.492 |
| Construction | 168.506 | 45.907 | 45.804 | 37.810 | 37.870 |
| Legacy fallback | 50.356 | 13.972 | 13.936 | 14.044 | 14.056 |
| Fixed-track repair | 0 | 0 | 15.647 | 14.244 | 14.291 |
| Full evaluation | 9.674 | 14.394 | 14.449 | 11.852 | 11.856 |
| **Complete detached experiment** | **250.771** | **96.504** | **112.128** | **100.861** | **99.610** |
| Constructed / delegated | 334 / 24 | 347 / 11 | 347 / 11 | 346 / 12 | 346 / 12 |
| Metric hard violations | 2 | 0 | 0 | 0 | 0 |
| Strict track pairs | 73 | 4 | 0 | 0 | 0 |
| Bends | 1,752 | 2,416 | 2,422 | 2,108 | 2,108 |
| Crossings | 3,183 | 4,435 | 4,437 | 4,183 | 4,183 |
| Area | 91,470,046.38 | 91,470,046.38 | 91,470,046.38 | 90,070,387.02 | 90,070,387.02 |

Cold/warm aligned scenes are identical (`bb7b0374007882a5`). Their published
key improves on the fixed-repair baseline; they do not surpass the existing
constructor's bend/crossing counts, and that constructor still has hard/track
violations on this grown geometry. Unlike the first three variants, aligned
planning intentionally changes demand and therefore node geometry. It loses
one fixed route but preserves every edge through explicit fallback. Preferred
lane changes across demand/final preparation fall from 2,288 to 1,940;
these are pre-coordination counts, not a second bend metric.

### What reuse saves, and what it does not

The cold aligned run computes 95 ordinal channels, 349 lane plans and 58
physical track plans. It reuses 349 lane and 58 track plans between demand and
materialization. An unchanged warm run computes none of those plans and
records 95 ordinal, 698 lane and 116 track reuses, with identical scene/key.
Yet its complete wall median is only slightly different: graph traversal,
exact key creation/comparison, validation, materialization, repair and scoring
still run. Cache hit counts are not a large refinement speedup.

A separate public-order sequence `0,1,2,3,4,0` compares cached and fresh
complete outputs at every step. Across different modes, 16-31 ordinal channels
can be reused, but all 349 lane plans and 56-58 physical track inputs change.
Single observations range from 93 to 138 ms and show no reliable cache timing
advantage; they are not repeated production-refinement measurements.

The diagnostic also evaluates the previous fixed-repair baseline for each
mode. An aligned result is eligible only with zero hard/track violations and
no geometry/routing issues. It is selected diagnostically only if eligible
and better by the published key, or if the baseline is ineligible. Otherwise
an eligible baseline is retained, or neither is selected:

| Order | Aligned hard violations | Aligned strict pairs | Diagnostic choice |
| --- | ---: | ---: | --- |
| 0 | 0 | 0 | aligned |
| 1 | 0 | 0 | aligned |
| 2 | 0 | 2 | none |
| 3 | 2 | 3 | none |
| 4 | 1 | 1 | none |
| 0 again | 0 | 0 | aligned |

This table exposes failures; it is not heuristic early rejection, a runtime
candidate-policy change, or a claim that the rejected strategies are
infeasible. A scheduler that changes only dependent regions, rather than
global cross placement, remains the larger next step.

### Verification

Three focused planning tests include 512 generated interval sequences checked
against an independent discrete minimum-change dynamic program, exact
edge/channel/spacing invalidation, and 128 generated ordinal channels checking
non-worsening pair cost, slot feasibility and broken-dependency reports.
A demand-only test verifies identical channel requirements with zero
materialized routes. Runtime tests compare cold/warm complete scenes,
global candidate changes, a main-axis translation of every route, and a
single-port cross change that invalidates exactly one lane plan while reusing
unaffected channels. Cycles, parallel edges, loops, labels and both axes are
covered, and inputs remain immutable.

The independent JavaScript checker verifies complete partitions, logical
directions, port attachment, fixed-route identity and strict conflicts for
all five variants, cold/warm identity and diagnostic selection. The old
three trial scene fingerprints and seven production-candidate fingerprints
remain unchanged. Workspace checks/tests, instrumented integration tests and
the small-fixture WASM/playground build pass. No public snapshot schema,
default option, accepted candidate, or worker handoff changed.

## 21. Regional order trials with anchored size propagation

The next experiment makes candidate placement genuinely regional instead of
changing every area's cross order. It does **not** make full routing or
refinement incremental yet. The initial result is a verified locality
boundary and one small eligible quality improvement, not a large speedup.

### Regional candidate and dependency boundary

`abstract_realization::regional::swaps` enumerates every adjacent pair of
real siblings in an existing rank, in stable area/rank order. Border/dummy
pairs are not moves in this experiment. `prepare` fixes the requested order
and calls the existing placement only for that frame. Other real-node orders
remain fixed; derived border order/geometry may change.

An initial implementation reran ancestor placement for size/border changes.
The probe showed this recreated the same waste at a higher level: root
cross-placement changes moved almost every node and invalidated almost every
lane plan. The retained implementation instead:

- keeps each existing frame's footprint as a minimum, without shrinking it
  and re-centering unrelated siblings;
- propagates positive size changes with the existing `push_grown` helper,
  moving only dependent siblings/dummies, then ancestors as necessary;
- preserves unrelated local child frames and plans exactly;
- refreshes ports for moved node/peer geometry, changed order keys,
  newly planned lanes and changed border geometry;
- recomputes demand, routes, obstacles, fallback, final repair, labels and
  shared quality on the complete resulting drawing.

The candidate owns a private placement-only input and borrows its immutable
source. It is not a public snapshot with stale routes. `complete` returns a
detached fully evaluated scene. Unsupported flattened/budgeted views, pins,
fixed container sizes and the existing growth capability restrictions are
explicit errors, not silent global fallbacks.

Planning reuse now has an independent branch per candidate: clone the common
base `planning::Session`, replace only changed keys in that fork, and never
let one rejected trial evict the common baseline's entries. Fork allocation
and comparison costs are included. Cold and forked complete scene bytes, keys
and strict conflict lists must match. No final route, obstacle decision or
score is cached, and no ordinal estimate authorizes early rejection.

### Reproduction and bounded sample

[Raw result](../benchmarks/results/abstract-routing-regional-20261104.jsonl):

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/routing-completion-check.mjs target/fast/bench 6 --regional
```

Same frozen 188-node/358-edge input; shared Apple M3, eight logical CPUs,
macOS, Rust 1.99.0, dirty `d14db45` on `feature/abstract-routing-plan`, load
`{ 8.79 8.17 7.30 }`. The recorded binary profile is `fast` (optimized without
release LTO); the generic environment helper's `release` label does not
override it. The result records 139 source/manifest hashes, the generator,
binary and input hashes. Historical section-20 results are not overwritten.

There are **54 available adjacent swaps**. The diagnostic explicitly evaluates
at most eight: the first pair in each non-root area in stable area/rank order.
This sample is neither exhaustive nor selected by a quality proxy and does
not replace the production candidate portfolio. Each variant has one warmup
and six measured pairs, rotating/reversed candidate order and alternating
cold/forked execution. All candidates start from the same source, not a chain
of accepted changes.

Wall totals include local placement, size propagation, session cloning,
validation, full growth/construction, fallback/repair and scene evaluation.
They exclude import, process/JSON transport and ordinary refinement.
The initial complete baseline used to populate the common session is recorded
separately (101.872 ms); it is not a free initialization. The paired sequence
median of 2,657.153 ms includes both cold and forked evaluations plus identity
checks and is **not** a production refinement latency.

| Target frame | Local / absolute changed boxes | Recomputed ordinal / lane / track plans | Cold / forked total median ms | Hard / strict conflicts | Full-key improvement |
| --- | ---: | ---: | ---: | ---: | --- |
| unchanged baseline | 0 / 0 | 0 / 0 / 0 | 105.555 / 104.298 | 0 / 0 | no |
| `crate:layout` | 14 / 115 | 1 / 339 / 53 | 239.525 / 238.758 | 1 / 4 | ineligible |
| `crate:reference` | 2 / 9 | 1 / 12 / 3 | 115.571 / 114.300 | 0 / 0 | **yes** |
| `crate:runtime` | 14 / 188 | 1 / 288 / 55 | 175.180 / 174.341 | 0 / 1 | ineligible |
| `crate:scene` | 8 / 177 | 1 / 348 / 53 | 151.700 / 150.781 | 0 / 1 | ineligible |
| `crate:topology` | 2 / 5 | 1 / 20 / 5 | 116.953 / 115.398 | 0 / 0 | no |
| `mod:crates/layout/src/constructive.rs` | 21 / 21 | 2 / 161 / 36 | 140.979 / 139.795 | 0 / 1 | ineligible |
| `mod:crates/layout/src/engine_choice.rs` | 4 / 4 | 1 / 56 / 14 | 117.704 / 117.083 | 0 / 0 | no |
| `mod:crates/layout/src/global.rs` | 5 / 5 | 2 / 231 / 54 | 151.929 / 150.768 | 0 / 1 | ineligible |

Every move places **one frame**; the frozen hierarchy contains 39 plans.
Only `crate:runtime` and `crate:scene` propagate size changes to the root in
this sample; neither reruns root ordering. Nevertheless their absolute
effects are broad. Even five moved boxes in `global.rs` change 231 edge lane
domains: adjacency, border and port dependencies can be much larger than the
placement edit. Some channels become active/inactive, so their counts should
not be read as a fixed common denominator for every candidate.

### Small quality gain, not a speedup conclusion

The `reference` move refreshes 16 ports, computes one ordinal channel, twelve
of 349 edge lane plans and three physical channel plans. The unchanged entries
are reused, including 94 ordinal, 686 lane and 113 track-plan uses across
demand/final materialization. No ancestor footprint grows.

It preserves zero metric hard violations, zero strict track pairs, no
geometry/routing issues, all 358 logical edges and the same area. Its key is:

```text
base:      [0,0,0,6489,0,3582,2108,125991375]
reference: [0,0,0,6483,0,3579,2110,125913765]
```

Proper crossings stay at 4,183; shared-endpoint crossings fall from 2,306 to
2,300, explaining the earlier-priority crossing-key improvement. Projected
crossings fall from 3,582 to 3,579 and route length from 1,259,913.75 to
1,259,137.65. Bends rise from 2,108 to 2,110 and near-parallel pairs from 16 to
20: this is a published-key improvement, not improvement of every metric.
The scene fingerprint is `c9db33e8371676d2`, versus unchanged baseline
`bb7b0374007882a5`. It remains detached, not accepted by production refinement.

The complete forked median is still 114.300 ms, including 10.041 ms local
placement, versus 115.571 ms cold. Construction still costs 39.607 ms,
fallback 14.628 ms, fixed-track repair 14.725 ms and evaluation 12.229 ms,
even though most planning entries survive. Cache reuse alone does not remove
those global passes. These shared-machine observations are not a controlled
speedup claim or an ELK comparison.

Five of eight alternatives are explicitly ineligible. In the expensive
`layout` move, final repair alone reaches 118.054 ms: 20,519 exact-event
candidates, eight accepted moves, four remaining track pairs. Fallback also
reaches 5,401 grid pops, while the baseline has none. Adding generic retries
or accepting a lower crossing count would conceal both the cost and its
remaining hard violation.

### Verification and next boundary

Four regional integration tests check all 54 enumerated moves' exact
real-node orders and single-frame placement; complete cold/forked identity
for several frozen candidates; exact `reference` invalidation counts and
full-key improvement; common-base cache isolation; both axes; cycles, loops,
parallel edges, edge labels and authored port fractions; unchanged unrelated
frames; unsupported constraints/invalid moves; and source immutability.
The independent JavaScript checker reconstructs absolute movement from local
boxes, verifies changed box/plan/port inventories, ancestor-only size
propagation, exact real-node swaps and unchanged other orders. It also checks
complete partitions, endpoint attachment, fixed-route preservation,
orthogonality, all strict segment conflicts and full diagnostic eligibility.
Fork dependencies are identical across measured trial order.

Workspace formatting/checks/tests, 23 instrumented layered-construction
integration tests, prior planning and seven-candidate/public fingerprint
checks, and the small-fixture WASM/playground build (including 55 UI
regressions) pass. No default, accepted candidate, public snapshot format or
worker dispatch changed; no commit or push was made.

The next dependency boundary is **route/repair influence**, not another cache
around ordinal plans. Start with exact unchanged construction dependencies
and occupancy/obstacle validation, explicitly account for changed ports and
shared channel constraints, and retain global cold-output comparisons.
A small placement delta does not prove that unrelated routes can be frozen,
nor that full scoring or fallback search may be skipped.

## 22. Exact fixed-track repair certificates and their limits

The next detached experiment reuses individual final fixed-track repair
decisions, not arbitrary fallback routes. It is explicitly enabled by
`planning::Session::with_repair_reuse()` or `bench --regional-repair 1`.
Ordinary layout, the historical planning/regional diagnostic, candidate
budgets, production acceptance and default serialized snapshots stay unchanged.

### Exact dependency boundary

Every decision still freshly generates all track/corridor events, including
distant projected tracks and obstacle limiters. The key records the **ordered
admissible proposal operations**, not a fuzzy geometry hash or a heuristic
route bounding box. Interior moves and bypass operations retain exact segment
indices, coordinates and order, including duplicates. Together with the
bit-exact input route, parameters and endpoints these determine the candidate
geometry and tie-breaking order. A remote event may change harmlessly only
when it produces exactly the same operations.

The cold run collects current and all geometrically valid proposal segments.
A sorted coordinate index merges overlapping collinear influence intervals;
lookup selects only potentially conflicting occupied segments, retaining
route identity/grouping and order. The obstacle certificate includes every
potentially intersecting outline, its shape and raw box, exclusions, endpoint
identity and required own-clearance geometry. Tiny segments and the accepted
orthogonality tolerance are conservatively covered rather than silently
discarded with the track-conflict segments. Newly entering occupancy or
obstacles invalidate the certificate.

A hit replays either the selected move or the prior unsuccessful bounded
decision. A failed decision is not an infeasibility proof. Current conflict
partners and initial/final pairs are still computed afresh. Different
occupancy states for the same route can occur across passes; all visited
decision inputs are retained. Successful calls retain only that call's states,
while errors/cancellation leave the prior session unchanged. Every regional
candidate forks the common base session independently, including clone costs.
Construction, legacy fallback/repair, labels, scene and scoring remain fresh.

This boundary was tightened after an initial raw-projection/envelope key
reused no regional decisions. A naive segment dependency scan added substantial
cost; the final sorted influence index and early exact-proposal comparison
reduce it. Neither unsuccessful prototype timing is presented as a speedup.

### Reproducible paired result

Raw data:
[`abstract-routing-repair-reuse-20261104.jsonl`](../benchmarks/results/abstract-routing-repair-reuse-20261104.jsonl).

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/routing-completion-check.mjs target/fast/bench 6 --repair-reuse
```

Same frozen 188-node/358-edge input and explicit eight-area sample as section
21, all 54 available swaps still disclosed. One warmup, six measured pairs,
rotating/reversed order and alternating cold/certified execution. Shared Apple
M3, eight logical CPUs, macOS, Rust 1.99.0, dirty revision `d14db45` on
`feature/abstract-routing-plan`, load `{ 5.09 6.46 6.38 }`. The recorded binary
profile is `fast` (optimized without release LTO), not the generic environment
helper's `release` label. The result records 140 source/manifest hashes plus
input, generator and binary hashes. Historical section-21 results are retained.

Total wall time includes placement, propagation, session cloning, certificate
creation/validation and complete routing/evaluation. It excludes import and
process/JSON transport. Baseline cache priming costs 101.949 ms separately.
The paired-sequence median, 2,559.226 ms, includes both cold and forked
evaluations and comparisons; it is **not** production refinement latency.

| Target frame | Reused / cold decisions | Avoided / cold trial count | Cold / certified repair ms | Cold / forked total ms |
| --- | ---: | ---: | ---: | ---: |
| unchanged baseline | 2 / 2 | 1,909 / 1,909 | 14.163 / 3.156 | 101.722 / 89.386 |
| `crate:layout` | 0 / 19 | 0 / 20,519 | 113.958 / 119.939 | 230.771 / 235.737 |
| `crate:reference` | 1 / 2 | 206 / 1,909 | 14.249 / 14.461 | 111.051 / 110.282 |
| `crate:runtime` | 0 / 8 | 0 / 6,640 | 51.603 / 53.358 | 168.958 / 169.657 |
| `crate:scene` | 0 / 6 | 0 / 5,616 | 33.547 / 35.150 | 146.017 / 146.475 |
| `crate:topology` | 0 / 2 | 0 / 4,149 | 15.253 / 16.206 | 112.459 / 111.847 |
| `mod:crates/layout/src/constructive.rs` | 0 / 4 | 0 / 3,720 | 26.246 / 27.353 | 135.301 / 135.258 |
| `mod:crates/layout/src/engine_choice.rs` | 0 / 2 | 0 / 4,170 | 15.275 / 16.278 | 113.604 / 113.437 |
| `mod:crates/layout/src/global.rs` | 0 / 6 | 0 / 6,205 | 32.203 / 33.845 | 146.700 / 146.935 |

These are paired medians on a shared machine, not a controlled optimization
claim. In particular, the `reference` hit avoids actual work (206 of 1,909
candidate trials) but does **not** reduce repair time after certificates.
Its small total-time difference also includes planning reuse and noise; it
must not be attributed to repair reuse. Most regional edits get no hit and
pay extra certificate work. Unchanged-input repeats avoid every candidate
trial, but that is not representative of a changed regional layout.

All nine scene fingerprints, full keys, metrics, strict conflicts, eligibility
and diagnostic improvement flags match the section-21 result exactly. The
`reference` scene remains `c9db33e8371676d2`, baseline
`bb7b0374007882a5`; the same five alternatives remain ineligible. Repair moves,
passes and initial/remaining pairs are identical cold/cached, with
`cold trials = executed trials + avoided trials` and
`cold decisions = computed decisions + reused decisions`.

### Verification and next leverage

Twelve focused fixed-track tests include five certificate/index tests:
successful and unsuccessful decisions, cancellation retention, remote
irrelevant geometry versus newly admissible events/corridor limits, indexed
versus direct interval tests at spacing tolerance boundaries, tiny/near-axis
segments and 128 generated cold/cached/warm cases with both axes, shapes,
clearance, endpoints, exclusions, port geometry and moving-route order.
The new full integration test checks independent forks for all eight sampled
regions, complete cold/fork/repeat scene/key/metric/conflict identity,
actual/avoided-work parity and zero recomputed decisions on identical repeats.
It also guards the unchanged base cache and input snapshot.

Formatting, both workspace checks (with/without benchmark timings), the
workspace tests, all 24 instrumented layered-construction integration tests,
the independent paired checker, prior planning parity and the frozen
seven-candidate public fingerprint check at threads 1/4 pass. The small-fixture
WASM/playground build also passes, including all 55 UI regressions. All 140
recorded source hashes, generator/binary hashes and all nine historical
regional outputs were checked against the persisted result. Existing unrelated
test warnings and ignored tests remain; no full benchmark sweep or controlled
idle-machine measurement was run. No commit, push or default change was made.

The result is a safe, measured dependency boundary, **not the large
refinement speedup still needed**. On `reference`, construction still takes
38.164 ms, fallback 14.091 ms and evaluation 11.917 ms, versus 14.461 ms final
repair. The next larger target is exact reuse of unchanged **construction**
dependencies and shared route evaluation, while retaining fresh occupancy
acceptance and complete cold comparisons. Do not silently relax the
certificates, reduce candidates or add more retries to make a cache look useful.

## 23. Shared fixed-path preparation and certified construction reuse

The next opt-in boundary separates fixed-path construction from preparation
for a possible path search. `planning::Session::with_construction_reuse()` and
`bench --regional-construction 1` enable it separately from final repair reuse.
Ordinary layout, prior diagnostics, candidate budgets, publication, acceptance
and snapshot serialization remain unchanged.

### Shared preparation, exact geometry and fresh acceptance

Previously every planned edge eagerly inflated all non-excluded node boxes
and built its own search index, including during demand-only planning, where
no route is constructed. Joint/chain planning also cloned these prepared
edges even though it only needs endpoints, flow and lane domains.

The new path still validates and prepares flow/endpoints afresh, but omits
those per-edge obstacle arrays/indexes. Final construction builds **one**
shared index over current inflated boxes. In the frozen view this avoids
698 eager obstacle preparations (349 during demand and 349 after growth);
there is no throwaway demand-route materialization or demand obstacle index.
The new cold path includes creating this shared index and its own certificates.

Each current edge has one replaceable geometric entry keyed by exact start/
end coordinates, axis, logical reversal and the complete ordered corner list.
The cache holds original path points, occupancy segments and the normalized,
logically directed route. Current segment-box queries collect all potentially
intersecting inflated boxes, with endpoint/ancestor exclusions refreshed.
Only identical obstacle identities/rectangle bits and identical path geometry
may reuse the clearance decision; a previous obstacle rejection may also
replay. A newly entering obstacle, changed margin, path, endpoint or exclusion
therefore cannot silently preserve an invalid route.

The certificate matches the existing constructor's **rectangular inflated
boxes**, not the fallback router's outline-based rules. Both eager/shared
paths use the same endpoint-exclusion, point construction and straightening
helpers. Tiny and near-orthogonal segments remain in obstacle queries.

**Occupancy acceptance is never cached.** Every candidate checks current
previously accepted segments in graph-input order at the current spacing.
A prior success can now fail because another route moved; a prior track
failure can become accepted. The initial geometric/monotonicity and
coordination failures also remain fresh. Entries not attempted in the current
complete construction are pruned; independent candidate forks cannot evict the
common baseline. Fallback, final fixed-track repair, labels, scene and scoring
remain fresh, with repair-decision reuse deliberately disabled in this test.

The existing constructor counters retain their meaning: `plannedAttempts`
counts fixed-path acceptance attempts, including those whose geometry reuses.
New counters separately expose actual geometry computations/reuses,
clearance computations/reuses, rectangle tests, fresh occupancy checks,
avoided eager obstacle preparations and shared indexes. Full constructor
work/failure reasons still match the original path.

### Three-arm control and persisted measurement

Raw data:
[`abstract-routing-construction-reuse-20261104.jsonl`](../benchmarks/results/abstract-routing-construction-reuse-20261104.jsonl).

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/routing-completion-check.mjs target/fast/bench 6 --construction-reuse
```

Same frozen 188-node/358-edge input, 54 available adjacent swaps and disclosed
eight-area sample. One warmup plus six rounds, rotating/reversing candidates
and all three arm orders:

1. **Eager cold:** original per-edge obstacles/indexes and no reuse.
2. **Shared cold:** new shared preparation, empty planning/construction caches.
3. **Forked:** clone the primed shared session, including clone costs.

Each arm includes fresh regional placement/propagation and complete
growth/routing/repair/evaluation, excluding import and process/JSON transport.
All three must produce identical fixed routes, delegation reasons, full scene,
key, strict conflicts and deterministic constructor counters. The shared-cold
control prevents preparation sharing from being mislabeled as cache savings;
forked totals also include the pre-existing planning reuse.

Machine/revision: shared Apple M3, eight logical CPUs, macOS, Rust 1.99.0,
dirty `d14db45` on `feature/abstract-routing-plan`, load
`{ 4.32 7.95 8.90 }`. The recorded binary profile is `fast` (optimized,
no release LTO), overriding the generic environment helper's `release` label.
Input, generator, binary and 141 source/manifest hashes are recorded.
Cache priming costs 95.906 ms separately. The sequence median, 3,745.381 ms,
includes all three arms and identity checks, **not** production refinement.
These heavily shared-machine observations are not a controlled speedup claim.

| Target frame | Reused / attempted geometries and clearance decisions | Eager / shared-cold / forked total median ms |
| --- | ---: | ---: |
| unchanged baseline | 346 / 346 | 102.275 / 95.906 / 95.211 |
| `crate:layout` | 2 / 338 | 231.392 / 225.246 / 224.359 |
| `crate:reference` | 334 / 346 | 111.745 / 105.458 / 104.367 |
| `crate:runtime` | 0 / 337 | 168.779 / 162.863 / 161.886 |
| `crate:scene` | 0 / 339 | 146.341 / 140.283 / 139.431 |
| `crate:topology` | 105 / 346 | 112.958 / 106.846 / 105.469 |
| `mod:crates/layout/src/constructive.rs` | 146 / 342 | 135.725 / 129.429 / 128.803 |
| `mod:crates/layout/src/engine_choice.rs` | 88 / 346 | 114.153 / 108.009 / 106.923 |
| `mod:crates/layout/src/global.rs` | 45 / 339 | 146.564 / 140.488 / 139.437 |

Geometry and clearance reuse counts coincide in this frozen sample, not by
contract; their keys and counters remain separate. The `reference` move
recomputes twelve path geometries and twelve clearance decisions, keeps
334 of each, performs no detailed rectangle tests and still runs **all 346
occupancy checks**. Some other regional swaps change hundreds of paths despite
small placement deltas: global coordination can spread the influence.
The shared-cold control captures most of the observed total reduction.
Warm cache replay alone does not provide a large additional improvement.

### The remaining constructor cost is now directly measured

New nested clocks cover edge preparation, channel assignment, global
coordination (including chain probe, joint/continuity assignment and final
corner preparation), and materialization/obstacle/occupancy acceptance.
Every arm records individual samples and medians; the independent checker
verifies each nested sample fits its matching construction sample, not a
sum of unrelated medians.

For `crate:reference`:

| Constructor phase | Eager / shared-cold / forked median ms |
| --- | ---: |
| edge preparation | 4.447 / 2.010 / 1.988 |
| channels | 0.556 / 0.560 / 0.545 |
| global coordination | 29.306 / 29.114 / 29.011 |
| materialization and fresh acceptance | 3.924 / 3.358 / 3.283 |
| complete construction | 38.779 / 35.479 / 35.349 |

Per-phase medians need not sum to the complete median. The common preparation
also reduces demand/growth time; forked growth is 4.807 ms. Fallback remains
14.165 ms, final repair 14.187 ms and evaluation 11.828 ms. The roughly 29 ms
coordination pass, not rebuilding a 346-route point list, is the next larger
construction boundary. Exact regional coordination dependencies and shared
route evaluation should be addressed before more geometry-cache micro-work.
No candidates were removed, rejected by proxy or granted fewer repair trials.

### Correctness and limits

Three focused tests compare cached and original eager construction with
identical counters and assigned segments. They cover both axes, logical
reversal, newly entering/moved obstacles, changed endpoints/corners, clearance,
exclusions, successful/failed decisions, cache pruning and changed occupancy
after a cache hit. Generated tests exercise 256 geometry/clearance cases plus
128 indexed boxes and 257 segment queries at four clearances, including tiny
and near-axis segments. Full integration checks all eight regional
cold/fork/repeat results, independent base forks and zero recomputed geometry/
clearance on identical repeats. The authored-port/label/cycle/parallel/self-loop
fixture is also checked with construction reuse in both axes.

All nine scene fingerprints, full keys, metrics, conflicts, eligibility and
diagnostic improvements match section 21 exactly. The baseline remains
`bb7b0374007882a5`, `reference` remains `c9db33e8371676d2`, and the same five
regional alternatives remain explicitly ineligible. A cached clearance result
does not imply whole-layout validity or repair completeness. Broader
graph-family timing, organic/radial-tree coordination, general route reuse and
production integration remain open; the shared preparation/certificate
principle is reusable, not this layered channel scheduler unchanged.

Workspace formatting, checks with/without benchmark timings, workspace tests,
all 25 instrumented layered-construction integration tests, the three-arm
checker, prior planning/repair diagnostics and the frozen seven-candidate
public fingerprint check at threads 1/4 pass. The small-fixture
WASM/playground build passes, including all 55 UI regressions. All 141 source/
manifest hashes, generator/binary hashes and historical regional outputs were
verified against the final persisted result. Existing unrelated test warnings
and ignored tests remain. No full benchmark sweep or controlled idle-machine
measurement, commit, push or default change was made.

## 24. Sharing immutable coordination geometry within one drawing

The chain probe and final joint/continuity passes repeatedly resolve the same
adjacent rank extents and interior junction rectangles. Previously each call
walked the rank elements again, looked up nodes/dummies and projected their
rectangles. This step shares those geometric inputs, not solved regions,
constraints or candidate results.

The opt-in `planning::Session::with_coordination_reuse()` enables construction
reuse plus a new context borrowing exactly one immutable, fully grown drawing.
It lazily memoizes main-span results by area/rank and interior rectangles by
area/element, including explicit failures. Endpoint domains still read current
trial lanes, and axis projection reads the current trial orientation. The
context is local to the constructor, never stored in a session or cloned into
another candidate. Placement, growth or plan changes therefore require a new
context without an approximate invalidation certificate.

Every chain probe, joint/continuity solve, constraint discovery, ordering,
tie-break, chain cut, compaction and witness still executes. Fallback, final
repair, labels and quality also remain fresh. No search candidate or trial
budget was removed, and repair-decision reuse is disabled in this measurement.

### Three-arm control and reproducibility

Raw result:
[`abstract-routing-coordination-reuse-20261104.jsonl`](../benchmarks/results/abstract-routing-coordination-reuse-20261104.jsonl).

```sh
cargo build --locked --profile fast -p arqlet-runtime \
  --features benchmark-timings --bin bench
node benchmarks/routing-completion-check.mjs target/fast/bench 6 \
  --coordination-reuse
```

Six samples follow one warmup, with rotating/reversed candidate order and
three interleaved arms:

1. The previous shared-construction path, with fresh caches and uncached
   coordination projections.
2. The same fresh shared-construction path with within-drawing projections.
3. An independent fork of the primed planning/construction session, but with
   another **new** coordination context.

Unlike section 23, the first arm already has shared obstacle preparation.
The first-to-second comparison isolates projection sharing, not that earlier
optimization or a warm cross-candidate cache. Complete scenes, fixed/delegated
partitions, keys, conflicts and deterministic constructor counters match all
three ways. Per-sample projection counters must match regardless of candidate
order; actual computations plus reused results equal the uncached requests.

The result records Apple M3, eight logical CPUs, macOS/aarch64,
Rust 1.99.0, dirty revision `d14db45`, 142 source/manifest hashes, generator,
binary and input hashes. `binary.profile: fast` is authoritative; the generic
environment helper still labels native optimized builds `release`.
The machine was heavily shared (load average `{ 11.40 12.00 9.35 }`), with
large wall-time outliers. This is **not a controlled speedup measurement**,
and absolute times must not be compared with section 23's run.

### Actual work and observed costs

For baseline and `crate:reference`, rank-span projection work falls from
3,752 computations to 95 computations plus 3,657 within-run reuses.
Interior-domain geometry falls from 5,706 computations to 1,329 computations
plus 4,377 reuses. These counts repeat exactly in all six samples and both new
arms. All eight regional moves have 95 actual span computations; their
interior-domain request totals vary with the unchanged solver work.
Endpoint-lane reads are deliberately not counted as reused geometry.

Observed medians in milliseconds:

| Case / phase | Previous shared-construction cold | New shared cold | Primed fork, new context |
| --- | ---: | ---: | ---: |
| baseline coordination | 76.338 | 56.219 | 53.526 |
| baseline complete construction | 94.434 | 82.448 | 78.694 |
| baseline complete diagnostic | 243.482 | 292.949 | 286.599 |
| `reference` coordination | 67.994 | 32.286 | 31.455 |
| `reference` complete construction | 83.942 | 47.942 | 49.614 |
| `reference` complete diagnostic | 247.077 | 208.385 | 224.134 |

The baseline total becomes worse despite less projection work; the fork does
not consistently beat shared cold. Reference total samples range from
99.828 to 548.024 ms in the control and 84.622 to 453.923 ms in shared cold.
Do not turn those noisy medians into an end-to-end speedup claim.
Initial session priming is 321.250 ms. The full three-arm, nine-drawing sequence
has a 12,503.299 ms median; it is **not refinement latency**.

The reference fork still spends medians of 28.563 ms in fallback, 30.176 ms in
final repair and 26.912 ms in evaluation. Those phases and the remaining
constraint/solver work are larger boundaries than further projection-cache
micro-optimizations. Next work should isolate prepared constraint inputs or
shared route evaluation, with exact dependencies and cold-output parity,
before attempting cross-candidate solver replay. A small placement edit is
still not evidence of small global routing influence.

### Correctness and limits

Three focused context tests cover 128 generated cases, both axes, fresh
endpoint lanes and changed trial orientation, exact successful/error replay,
new contexts after changed geometry, and complete fixed/joint/chain routes,
delegations, channels and solver summaries/witnesses. Integration checks all
eight regional cold/fork/repeat results, independent base forks and unchanged
inputs. A single reusable session also visits all five global order modes and
returns to the original geometry; every result matches a cold complete run.
Both axes with authored ports, labels, cycles, parallel edges and self-loops
remain covered.

All nine scene fingerprints, full keys, metrics, conflicts, eligibility and
diagnostic improvements match section 21 exactly. Baseline remains
`bb7b0374007882a5`, reference remains `c9db33e8371676d2`, and the same five
regional alternatives remain ineligible. All 142 source/manifest hashes,
generator/binary/input hashes and historical outputs were verified against
the persisted result.

This remains a detached layered diagnostic, not production refinement, a
default change or an organic/radial-tree implementation. Sharing immutable
prepared inputs is transferable; the layered rank/channel geometry is not a
generic scheduler. Broader graph-family timing and an idle-machine comparison
remain open.

Workspace formatting, checks with/without benchmark timings, workspace tests,
all 27 instrumented layered-construction integration tests, the three-arm
checker, prior planning/construction/repair diagnostics and the frozen seven-
candidate public fingerprint check at threads 1/4 pass. The small-fixture
WASM/playground build passes, including native/WASM and serial/threaded parity,
stale-result rejection, all three deployment modes and all 55 UI regressions.
Existing unrelated warnings and ignored tests remain. No full benchmark sweep,
controlled idle-machine measurement, commit, push or default change was made.

## 25. Shared segment evaluation and a measured spatial-index limit

The next boundary was shared metric evaluation. Published keys and published
metrics already share `measure_variants`; another whole-evaluation cache would
not remove an accidental duplicate here. Within each route pair, however, the
old evaluator computed the same segment boxes repeatedly, then visited the same
segment pairs a second time for crossings.

The common evaluator now prepares one box per normalized segment and handles
collinear overlap, near-parallel length and crossing classification in one
ordered pass. Shared-endpoint/bundle exemptions are resolved only when there
is a collinear line to exempt. Touch detection, line merging, node interaction,
clearance variants and all metric definitions remain unchanged. Every sum
retains its previous route/segment order. This output-equivalent change is
active in the shared scene metric, including per-route conflict queries,
not only detached routing. No candidate, acceptance rule or budget changed.

### Why the route spatial index stays diagnostic

A provisional balanced bounding-box tree generates potentially relevant route
pairs, queries one route at a time and sorts matching indices into the original
`(i,j)` order. It retains linear index/query storage instead of storing a
quadratic pair table. Bounds use the exact previous reach/tolerance predicate.
No geometric result survives another measurement.

On baseline, the tree reduces source-level route bounds tests from 63,903 to
52,329, but still evaluates the same 13,279 relevant pairs. The frozen routes'
large envelopes limit pruning, and traversal/sorting costs can outweigh saved
tests. The spatial path is therefore exposed only by `benchmark-pairs`; the
production evaluator keeps exhaustive route-pair iteration with the cheaper
shared segment evaluation. It is not enabled by a guessed size threshold.
Sparse controls demonstrate potential, not evidence for a general default.

### Reproducible three-arm measurement

Raw result:
[`metric-pair-evaluation-20261105.jsonl`](../benchmarks/results/metric-pair-evaluation-20261105.jsonl).

```sh
cargo build --locked --profile fast -p arqlet-scene \
  --features benchmark-pairs --example metric-pairs
node benchmarks/metric-pairs-check.mjs target/fast/examples/metric-pairs 12
```

The reference arm retains the former two-pass pair evaluator and does not
prepare segment boxes. The shared arm is the uninstrumented production
evaluator. The spatial arm adds route-pair indexing to that same shared
evaluator. One warmup precedes twelve rotated/reversed three-arm samples per
case. Clocks include complete drawing validation/preparation, all route/node
metrics under `(12, false)` and `(24, true)` clearance variants, and any index
construction/querying/sorting. JSON transport and work-counter collection are
outside the clocks. No warm cache or candidate pruning is involved.

The result records Apple M3, eight logical CPUs, macOS/arm64, Rust 1.99.0,
`fast` profile, dirty revision `d14db4513e7364102032306d560785d3927aef21`,
binary/generator/input hashes and 134 Rust-source/manifest/workspace hashes.
Load averages were approximately 33.39 / 40.31 / 47.36 on the shared machine.
These are paired observations, **not an idle-machine optimization sweep,
full-pipeline speedup or refinement latency**.

Observed complete-metric medians in milliseconds:

| Drawing | Former evaluator | Shared segments (production) | Shared segments + spatial route index |
| --- | ---: | ---: | ---: |
| frozen baseline | 10.604 | 6.987 | 7.183 |
| frozen `reference` | 11.348 | 7.249 | 7.399 |
| sparse 256 edges | 0.063 | 0.069 | 0.059 |
| sparse 1,024 edges | 0.587 | 0.615 | 0.307 |
| dense 256 edges | 17.886 | 17.746 | 18.412 |

Reference samples span 10.558--12.066 ms in the former arm and
6.768--8.044 ms with shared segments. Baseline/reference improve in this
focused comparison; sparse controls pay extra segment-box preparation when
there are no relevant pairs, and the dense one-segment control barely changes.
Those regressions/limits are retained, not hidden behind a general speed claim.
The spatial arm is not uniformly worse or better across the nine frozen cases.

For baseline, overlap/parallel/crossing loop slots fall from 1,993,258 to
996,629, and source-level segment-box computations from 3,986,516 to 2,466.
For reference the corresponding counts are 1,993,712 to 996,856 and
3,987,424 to 2,468. These are source-level loop/box operations, not measured
CPU instructions; touch-helper work is explicitly excluded. Route-pair
counts do not change. The sparse 1,024-edge spatial control performs 16,913
route bounds tests instead of 523,776; the dense control still evaluates all
32,640 pairs and adds tree overhead.

### Input reconstruction and exact correctness boundaries

The focused benchmark reconstructs absolute drawings from the stored section
24 scenes, in graph order, with graph-derived container membership and resolved
port identities. These are **scene-derived drawings**, not the original
runtime evaluation inputs. Their node-hugging values differ from the stored
snapshot measurements: baseline/reference have 12 hug pairs versus zero in
the original snapshots. Every changed `hugPairs`/`hugLength` value is recorded
as `historicalMetricDifferences`; all other historical fields must match.
This experiment does not infer a snapshot key from the reconstructed drawing
or silently substitute these proximity values into runtime results.

On each identical scene-derived input, all complete metric fields and their
serialized values match the former evaluator exactly for both clearances.
Separately, native full completion checks verify all nine original scenes,
full metrics (including proximity), keys, conflicts, eligibility and diagnostic
improvements against section 24 without reconstruction. Baseline remains
`bb7b0374007882a5`, reference remains `c9db33e8371676d2`, and the same five
regional alternatives remain ineligible.

The example's JSON parser explicitly enables `float_roundtrip`, matching the
runtime. Without it, decimal coordinates near exact spacing boundaries can
parse differently and corrupt the historical near-parallel comparison.
This is a dev-dependency feature, not a new core dependency.

Three new focused tests cover 64 generated route sets at seven spacings,
arbitrary/orthogonal segments, redundant points, backtracking, tiny segments,
shared ports/bundles, cycles/self-loops/parallel edges, both coordinate axes,
exact tolerance boundaries, extreme finite coordinates, negative/NaN/infinite
spacing, sparse/dense cases and empty input. Existing diagnostic fixtures
also compare full old/indexed/default metrics; invalid drawings preserve the
same explicit validation errors. The shared evaluator is geometry-based and
therefore also serves organic/tree polylines, but no organic/radial-tree
solver or family-specific performance claim follows.

The next larger work remains prepared coordination constraints or exact
per-route/pair evaluation dependencies across candidates. A segment-level
spatial index may be more selective than whole-route envelopes; it needs the
same exact-order controls and dense-case measurements before adoption.
Full metric calls still recompute node interactions and relevant route pairs.

Validation passes: all 44 scene tests; workspace formatting, checks with and
without benchmark controls and workspace tests; all 27 instrumented
layered-construction integration tests; the frozen seven-candidate public
fingerprint check at threads 1/4; prior planning/coordination diagnostics; and
the small-fixture WASM/playground build with native/WASM and serial/threaded
parity, stale-result rejection, all deployment modes and all 55 UI regressions.
All result source/manifest, generator, binary, graph and frozen-input hashes
were verified. Existing unrelated warnings and ignored tests remain. No full
benchmark sweep, controlled idle-machine run, commit or push was made.

## 26. Focused public pipeline remeasurement before further integration

The [three-view release comparison](../benchmarks/results/focused-pipeline-20261105.md)
now measures clean `d14db45` and the current worktree on the same machine and
embedded frozen inputs, with five rotated/reversed samples. It separates core
layout, native import, public core refinement, Optimize and public two-process
JSON calls; elkjs/elk-rs receive the same preselected views. Source/binary/input
hashes, full quality and unchanged candidate/search aggregates are recorded.

The production codegraph core refinement changes from 1,292.489 to
1,268.643 ms; native import stays approximately 209 ms. Current initial core
layout is 208.098 ms versus elk-rs `layout()` 47.760 ms. Across the three inputs
the sums of initial core medians are 243.902 versus 67.130 ms. These are shared-
machine observations, not a controlled sweep or quality-equivalent algorithms.

The public gain is modest because the larger abstract-plan reuse work is still
detached. The active shared metric reduces codegraph scoring from 123.191 to
108.038 ms, while placement/routing remain 333.045/791.983 ms in cumulative,
nested Optimize work timers. Constructor searches still execute 55,055,505
transitions. No search candidates were removed.

Separately, release detached reference coordination measures
28.560 / 13.113 / 13.100 ms for old shared-construction cold / new shared cold /
primed fork with a new context; complete drawings cost
100.263 / 84.520 / 83.424 ms. All nine historical detached scenes/keys/metrics/
conflicts/eligibility values match, with the same five ineligible alternatives.
These drawings are not the production candidate portfolio or output; their
cost is not a new public refinement latency.

The ELK preflight also found and corrected a comparison-adapter error for
self-loop section coordinates. The adapter now respects explicit container
frames and never treats the self-loop's leaf as its coordinate frame. Three
tests and actual-output endpoint/partition checks pass. Historical ELK quality
rows containing self-loops require recomputation, not retroactive endorsement.

The next bounded milestone is broader detached quality validation, followed
by an opt-in complete abstract-plan candidate with the incumbent preserved as
a control. Measure quality and actual complete-candidate work before deciding
production integration or defaults; do not pursue more metric micro-work or
replace all current strategies based on the single-drawing timing.

## 27. Public abstract-routing integration and complete-call comparison

The provisional `abstractRouting` option now connects the shared complete
abstract router directly after placement, before the legacy probe/construction
loop. Import, all configured ordering candidates and supported edits use it
through native/WASM dispatch. Grown stored plans travel with grown local boxes,
ports, routes, boundaries and labels. Unsupported modes fail explicitly.
The option defaults off and is omitted; existing default outputs are unchanged.
The incumbent and strategy list remain. Strict track conflicts exclude abstract
ordering candidates; legacy-only routing jobs retain their acceptance rules.

The [integrated release comparison](../benchmarks/results/abstract-public-pipeline-20261105.md)
records five paired samples on the same three frozen views, including a common
default incumbent. Codegraph native import changes from 209.685 to 163.208 ms,
core refinement from 1270.312 to 989.674 ms on its own input, and public
import/refinement from 1510.525 to 1192.714 ms. Its constructor transitions drop
from 55,055,505 to zero and placement passes from 26 to 12. Five first-stage
jobs still execute; global rounds stop after one instead of two under the
unchanged no-improvement rule on different geometry. Workflow and organigram
are slightly more expensive, not accelerated.

This is not equivalent-quality speedup: refined total crossings change
4113 -> 6430 (codegraph), 256 -> 400 (workflow), and 35 -> 74 (organigram).
All have zero geometry issues/line-node hits and complete finite orthogonal
logical routes. The organigram still reports two strict track conflicts.
From the common incumbent codegraph refinement costs 913.780 ms, but its winner
removes the input's ten shared-line pairs at the price of 7334 total crossings;
the default refined winner removes those pairs with 4113 crossings. The shared
key therefore improves against the input, not against the old refined result.

Measurements are shared-machine diagnostics (start load 12.89/12.90/11.68,
end 5.23/9.79/10.60), not an idle sweep. Complete raw outputs, native keys,
per-sample work, binary/source/input hashes and the timing boundaries are in
the report. Default frozen geometry/keys remain identical. Seven new public
integration tests, workspace checks/tests, generated contract drift checks
and the full browser/WASM build/test script pass.

Next improve abstract order/port coordination against the complete crossing
key, not just route coverage. The integrated completion also still performs
diagnostic scene/evaluation work before runtime scoring; removing that duplicate
work is a separate follow-up, not a remedy for the quality regression. No
default change or removal of legacy strategies is justified by these timings.

## 28. Pre-placement ordinal channel-capacity seeding

The opt-in abstract path now estimates ordinal interval overlap for each
region's ranked chains before assigning coordinates, then uses that estimate
to seed flow-axis rank gaps. The physical width check and growth remain in
place. The ordinal estimate is not a clearance proof.

The [five-sample public comparison](../benchmarks/results/abstract-capacity-seeding-20261105.md)
uses the same three frozen views and harness as section 27. Against the prior
abstract-path run, public import-plus-refinement medians change from 1192.714
to 1147.824 ms for codegraph, 129.510 to 134.066 ms for workflow, and 101.285
to 104.880 ms for organigram. The result is a mixed shared-machine diagnostic,
not a general speedup; placement-pass counts are unchanged.

On the abstract path's own selected outputs, total crossings change
6430 -> 6579, 400 -> 400, and 74 -> 74. Initial areas increase on all three
views, notably workflow from 2.36M to 3.06M and codegraph from 88.61M to
99.20M. Against default, the new abstract outputs still have more total
crossings in every view. Thus the estimate currently adds space without
demonstrated routing-quality improvement. It remains disabled by default.

The next step is to report seeded width, realized physical demand and remaining
late rank shifts per channel. This is needed to tell useful preallocation from
over-allocation before revising the estimate or making any performance claim.
Raw timings, quality, work and provenance are in the linked result.

## 29. Per-channel seed versus physical capacity diagnostics

The public opt-in snapshot now includes a per-channel comparison of ordinal
tracks/seeded width, available space after placement but before abstract
completion growth, physical realization demand, final availability, actual
late shift and remaining deficit/surplus. A channel without a matching
placement record is explicitly marked rather than treated as zero demand.

Across the frozen codegraph/workflow/org-chart views, 184 physical channels
were reported and 163 matched to a placement record. Exactly one matched
channel widened after placement: codegraph area `crate:layout`, rank 3, from
390 to 450 units, a 60-unit shift. There was no final physical width deficit.
Matched workflow and org-chart channels had no late shifts. Conversely, 68 of
89 matched codegraph channels, 19 of 21 workflow channels, and 25 of 53
org-chart channels already had positive pre-growth surplus. The measured
positive surplus sums were 1,228 / 490 / 286 units respectively; this is
per-channel space, not added diagram area. The remaining 21 physical channels
have no matching seed record and are an explicit coverage gap.

These observations do not establish that the estimate caused all available
slack: base rank spacing and the existing geometric channel preparation also
contribute. They do show that the single measured later widening is isolated
and that broad capacity increases are not justified. Audit unmatched channel
mapping next; then evaluate only targeted reductions while preserving the
physical-width and output-quality gates. See the [diagnostic report](../benchmarks/results/abstract-capacity-diagnostics-20261105.md)
and [raw results](../benchmarks/results/abstract-capacity-diagnostics-20261105.jsonl).

## 30. Complete placement-seed coverage on region-cache hits

The unmatched-channel audit traced the 21 missing seed records to placement
region memo hits: reused coordinates returned before their placement-time
diagnostic capture. Region realizations now retain the seed records alongside
their coordinates, and reuse replays those records rather than recomputing
track counts. The abstract pipeline regression test now requires complete
seed coverage on import and refinement across its frozen graph families.

The repeated frozen views now map all 184 physical channels: codegraph 95/95,
workflow 21/21, and org-chart 68/68. The only post-placement shift remains
codegraph `crate:layout`, rank 3, at 60 units; all channels finish without a
physical width deficit. Default, abstract, and common-incumbent geometry
fingerprints match the earlier diagnostic run exactly.

This was a one-sample coverage audit on a heavily loaded machine, not a timing
comparison. It closes the mapping gap for these views, but it does not
demonstrate a quality or performance benefit from capacity seeding. See the
[audit report](../benchmarks/results/abstract-capacity-diagnostics-audit-20261105.md)
and [raw results](../benchmarks/results/abstract-capacity-diagnostics-audit-20261105.jsonl).

## 31. Bounded ordinal capacity trials

With channel coverage complete, two placement variants were tested against the
full ordinal preallocation: no extra capacity above the existing base rank gap,
and half of the estimated excess above that gap. All variants kept the same
candidate portfolio, selection rules, and physical completion checks.

On the three frozen views, no preallocation produced smaller initial drawings
and no worse crossing counts than the full estimate: codegraph area fell from
99.20M to 88.61M with crossings 4,248 -> 4,144; workflow fell from 3.06M to
2.36M with crossings unchanged at 280; org-chart fell from 33.46M to 32.51M
with crossings unchanged at 18. Bends were unchanged or slightly lower.
However, codegraph's later channel growth increased from one channel / 60
units under the full estimate to six channels / 186 units with no
preallocation. Half-excess was intermediate in area but did not reduce
crossings relative to no preallocation. All three variants ended with zero
physical missing width and no geometry/routing issues in these outputs.

Each arm had one timed sample, collected at materially different high machine
loads. The public timings cannot resolve whether smaller initial geometry
offsets the additional late growth. Keep the current full-estimate opt-in
behavior unchanged; do not choose a halfway factor by intuition. The next
step is a repeated, rotated full-versus-none run on an idle machine, with
candidate work, published keys, quality, and physical missing width checked.
See the [trial report](../benchmarks/results/abstract-capacity-bounded-trials-20261105.md)
and [raw results](../benchmarks/results/abstract-capacity-no-seed-trial-20261105.jsonl)
and [half-excess results](../benchmarks/results/abstract-capacity-half-seed-trial-20261105.jsonl).

## 32. Reuse runtime scoring for abstract candidates

The public abstract completion path used to build a renderer-neutral scene,
decorate labels, and compute a published key/metrics result, then return the
snapshot to runtime, which decorated and scored it again. Production completion
now skips the unused scene and throwaway evaluation; the outer layout path
reuses the already decorated labels. The standalone completion/diagnostic API
retains the scene and evaluation; the runtime still performs its normal
candidate scoring, while route validation, geometry checks, and strict-track
eligibility remain unchanged.

On the three frozen views, five-sample public abstract import-plus-refinement
medians summed to 1,405.630 -> 1,338.228 ms (-4.80%); refinement alone summed
to 1,189.301 -> 1,135.800 ms (-4.50%). Each view's public refinement improved:
codegraph 1,000.212 -> 953.759 ms, workflow 106.785 -> 103.296 ms, and
org-chart 82.304 -> 78.744 ms. These are sums of per-view medians, not one
application's latency. Output fingerprints, metrics, and deterministic
candidate/work identities match exactly.

This is a specific measured gain in the opt-in abstract candidate path, not a
general Arqlet speedup or a quality-equivalent ELK comparison. The
codegraph quality gap remains, and the abstract path remains opt-in. See the
[evaluation-reuse report](../benchmarks/results/abstract-evaluation-reuse-20261105.md),
[baseline samples](../benchmarks/results/abstract-evaluation-baseline-20261105.jsonl),
and [optimized samples](../benchmarks/results/abstract-evaluation-reuse-final-20261105.jsonl).
