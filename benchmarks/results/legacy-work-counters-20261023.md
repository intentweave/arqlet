# Deterministic work counters for the legacy router (2026-10-23)

## What is counted, and in which unit

- `legacyCandidates` (`WorkCounters.legacy_candidates`, `channel::SearchCounters.legacy_candidates`): candidate routes evaluated by the legacy track search (`best_route`). Each evaluation scans the obstacles and the routes placed before it, plus the early-exit comparisons. Unit: candidate evaluations.
- `legacyGridPops` (`legacy_grid_pops`): heap pops of the legacy grid repair (`grid_route`), including pops of entries superseded later. Unit: grid steps.
- `astarExpansions` (channel router, unchanged): A* expansions of the channel router only.

The units are not comparable across routers. A legacy candidate evaluation and an A* expansion are different operations, so the counters must not be added up or compared as cost across engines. Within one engine and one job they are deterministic work: the same input, the same count.

Not counted: the post-processing `spread_tracks` pass, and the port placement's own improvement (`layout::ports::boundaries`, tests only). Work of an interrupted job is counted up to the stop.

## Checks

- Geometry is unchanged by the instrumentation: the 20 default views are identical to the objective-order reference (17 identical; the 3 `layerweave-code` views have no stable reference because their dataset comes from this repository's source). The required-clearance runs at 12 are identical to `clearance-placement-12-20261023.jsonl` (17 identical, the same 3).
- Reproducible: `workflow-dag 100` run twice gives identical work and geometry fingerprints for all six candidate records (`legacyCandidates` 71,690 for each ordering job; 71,621 for routing-only; 851 grid pops for each ordering job, 0 for routing-only). Orderings 1 to 3 have the same geometry as ordering 0 and identical counts.
- Interruption: `candidate_until` stopped after three checks returns no candidate and still reports a partial, non-zero count, below the full count (test `legacy_work_counters_are_reproducible_and_survive_an_interruption`).

## Findings

1. The legacy routing-only job is not interruptible. `reroute_with` passes `&|| false` to the legacy improvement; this was so before the counters and is unchanged. Its work is complete regardless of a stop request. Left as is, to be decided separately.
2. A huge required clearance makes routes detour. With `requiredClearance` 1e6 the routes are pushed about a million units out, and the clearance is met only by that detour. Absurd values should be refused or bounded; this is not done yet.
3. An unmet required clearance is reported as a routing issue (`required clearance C not met: N near-node pairs remain`). The refinement keeps the input in that case, so the report is the only trace.

## Using the counters for the budget policy: measured, not switched

First-stage job costs (candidate evaluations plus grid pops) differ within a view by a median factor of 1.15 (largest 4.4) and across views by the size of the view. Jobs of one view are close in cost.

Judge views (10), budget k, reaching the best first-stage candidate (`benchmarks/results/allocation-simulation-cost-20261023.txt`):

| Budget | Planned | Win rate (current policy) | Win rate per cost |
| --- | --- | --- | --- |
| 1 | 2 | 3 | 3 |
| 2 | 4 | 6 | 7 |
| 3 | 5 | 9 | 9 |
| 4 | 7 | 9 | 9 |

The cost weighting changes one view, at budget 2. With ten views that is within noise, so the budget policy stays the win-rate order. The counters stay as measurement; a cost-aware policy needs more views before it can be preferred.
