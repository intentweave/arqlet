# Review and next optimization steps — 2026-10-21

The frozen inputs, four versioned profiles and a shared metric evaluator are the right foundation. The existing results remain as the historical baseline.

## Corrected in this review

- On timeout, the runner ends the entire process group, including the layout process.
- Determinism is checked against the full snapshot hash instead of only equal metrics. A single run does not prove repeatability.
- Missing or incomplete visible routes lead to an error instead of artificially good scores. Bundled members are validated through their leader.
- Result metadata contains the hash of the executable; individual profiles and inputs can be selected.
- Metric version 2 normalizes redundant forward-collinear points. Crossings and shared connection segments therefore do not depend on segmentation. Backward movements are kept and count as a bend.
- Identical coordinates alone do not justify a shared connection: the logical endpoint node must also match.
- The channel router uses A* with a Manhattan lower bound. Obstacle, clearance, bend and occupancy costs are retained. A regression test compares the optimal costs with Dijkstra for 256 start/target pairs and one unreachable case.

## Bounded before/after comparison

Frozen `state-tcp`, readable with placement search explicitly disabled (`placementSearch: 0`), three alternating repetitions per variant. Both variants were evaluated with the same metric evaluator version 2. Times include process start; no statement about 100K nodes or the full profile corpus.

| Operation | Before, median | After, median |
| --- | ---: | ---: |
| Import | 5.55 ms | 5.09 ms |
| Configuration | 17.76 ms | 9.65 ms |
| Post-optimization | 81.34 ms | 27.38 ms |

Quality metrics of this example are unchanged: 0 sibling/cross-branch overlaps, 0 node traversals, 0 unintended shared segments, 54 bends. Still 2 crossings of independent edge pairs and 11 crossings of pairs with a shared end node; consider both categories together. Border hugging remains present (10 pairs).

Raw data: `benchmarks/results/routing-astar-review-20261021.json`. Workspace Rust tests and nine Python runner tests passed. No new visual browser check and no full profile benchmark in this review. Historical metric v1 results must be re-evaluated before a direct comparison with v2.

## Order of the next work

1. **Close measurement gaps:** validate drawing inputs for invalid parent references/cycles and missing geometry; measure container traversals in a differentiated way; make shared end-node crossings visible in the report. Add label collisions. No blanket zero scoring of missing routes.
2. **Actually enforce the time budget:** end the placement search after the deadline with the best valid intermediate result; store partial results per phase. Keep abortability and revision checks during the frontend/backend handoff.
3. **Routing quality:** keep the best valid congestion round; specifically re-route colliding edges. Separate shared port stubs/bundles from unintended overlaps. Reduce border hugging only after node collisions and route overlaps.
4. **Improve placement and routing together:** adapt local channel widths and rank spacing to occupancy; move nodes only locally and re-route affected edges. Evaluate hybrid direction with a small candidate budget instead of a combinatorial search.
5. **Compare profiles:** keep the four existing profiles. Use the same inputs, at least three repetitions, fixed seeds/threads and the same metric version. Report layout quality and time separately; geometric errors must not be compensated by shorter edges.
6. **Scale:** afterwards, increase size and density in steps, measure native and WASM runtimes separately, and report the full graph and the visible projection separately. Prioritize spatial indices and local rather than global routing grids only once the measured bottlenecks justify it.

Layout/routing profiles remain separate from visualization profiles. Comments and containers with a different presentation logic remain part of the test corpus; other diagram logics do not have to be implemented before this optimization round.


## Continuation: cooperative budgets

Implemented: `refine` optionally accepts `budgetMs` as a non-negative integer in the native JSON transport. Zero returns the existing complete state after validation. `optimization.interrupted` marks an interruption; old snapshots without this field remain readable. `candidatesEvaluated` now also counts global routing rounds (including the baseline), but not individual local placement attempts.

Through `refine_snapshot_until` and `search_placement_until`, the core receives a stop callback supplied by the host. It is checked between jobs, between local placement candidates and before the final overall routing. Once an abort has been requested, it stays set for the refinement call. Finished candidates are selected under the existing geometry and quality rules; revision checks remain in place. If the starting geometry is invalid and no valid candidate exists, an error is still returned.

**Limits:** a cooperative soft budget, not a hard timeout. A candidate that has already started, validation and quality measurement can overrun. Import/configuration are not yet interrupted. The runner subtracts their measured time from the profile budget and marks results separately as interrupted; the process timeout remains the outer safeguard. Budgeted calls run serially, unlimited calls keep the previous parallelization. Time-budgeted results can vary between repetitions. `budgetPolicy: cooperative-refinement-v1` distinguishes new runs from historical comparison runs.

The WASM core still compiles. The current browser ABI has no host clock yet: `budgetMs` is explicitly rejected there as unsupported; without this field the behavior is unchanged. The next step is a host clock/cancellation adapter plus finer abort points within a routing candidate. Native budgets are therefore not yet a fully integrated browser feature.

Validated: full workspace tests, ten runner tests, WASM target check. Regression tests cover immediate abort, stale requests, unfinished placement search with complete, not worse results, and budget validation. `benchmarks/results/budget-smoke-20261021.json` documents a single 50 ms trial run: even the cold import exceeded the budget, and refinement correctly returned the existing state. This trial run is not a performance comparison and does not prove a hard runtime limit.

### Classification of the SSSP paper

Duan et al., [Breaking the Sorting Barrier for Directed Single-Source Shortest Paths](https://arxiv.org/abs/2504.17033), show deterministic O(m log^(2/3) n) for directed graphs with non-negative real weights in the comparison-addition model. This is a theoretical advance for single-source all-targets, especially sparse graphs; it does not imply a measured practical benefit for this router.

Our channel router, by contrast, mostly answers individual start-target queries over geometric direction states with changing occupancy costs. A* uses an admissible geometric lower bound here. Recommendation: first improve the local search space, reuse and abort granularity; evaluate the new method as a separate benchmark candidate only when profiles show many equally weighted queries from the same source. No foreign code was adopted. In such an experiment, the paper's n and m refer to the routing state graph, not directly to the visible diagram nodes.
