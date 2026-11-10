# First-stage allocation under a job budget (2026-10-23)

Script: `benchmarks/allocation_simulation.py`; output: `allocation-simulation-20261023.txt`. Inputs: candidate logs of the 20 views (`bench --candidate-log`, current default options). The split is `calibration-split.json`: the learned ranks come from the tune views only, and the result is read on the judge views.

Result (judge views, 10 in total; a view counts when the first k jobs reach the best first-stage key that all jobs reach):

| Budget (jobs) | Planned order (current) | Learned order (new) |
| --- | --- | --- |
| 1 | 2 of 10 | 3 of 10 |
| 2 | 4 of 10 | 6 of 10 |
| 3 | 5 of 10 | 9 of 10 |
| 4 | 7 of 10 | 9 of 10 |

Under a budget the refinement result is the best candidate among the jobs it runs (global rounds need budget left), so these counts are the first-stage outcome of each budget.

Learned ranks (win rates on the tune views): routing 0.6; orderings 2 and 4 0.2; ordering 3 0.1; ordering 1 0.0. They are implemented as `budget_rank` and apply only when a budget cuts the plan. An unbudgeted run keeps the planned order, and the 17 views with a stable reference are byte-identical.

Limits:
- Ten judge views: the gain is measured, not established. The learned order was not tuned on the judge views, but it is a single table from ten tune views.
- No cost-aware policy yet. The work counters are zero for every first-stage job under the default engine (the legacy router counts no A* expansions, and the ordering jobs run no placement trials), so no deterministic cost signal exists. A cost-per-gain policy needs cost counters for the legacy path first.
- `layerweave-code` views have no stable reference: their dataset is generated from this repository's source and changes with it.
