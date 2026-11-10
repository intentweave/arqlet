# Consistent layered pipeline: the hot phases on arrays, 2026-11-03

Apple M3, release `bench` (`--reps 5 --threads 1`, median), nine uniform-direction views, limit 150, options
`{"layeredConstruction":true,"hierarchicalOrdering":true}` for import and refinement; elk-rs 0.12.1 on the same view as the
control (median of 5, measured in the same run). Load averages 3 to 9 during the runs. Raw records:
`layered-pipeline-arrays-before-20261103.jsonl`, `layered-pipeline-arrays-after-20261103.jsonl`,
`layered-pipeline-arrays-elk-20261103.jsonl` (before, then after).

| step | import, nine views | refinement, nine views | import, layerweave-code | refinement, layerweave-code | elk-rs, nine views |
|---|---|---|---|---|---|
| 5488bc7 (before) | 1021 ms | 1900 ms | 813 ms | 1502 ms | 60.4 ms |
| 52f6c5f hierarchical sweep on arrays | 735 ms | 1618 ms | 609 ms | 1287 ms | 60.9 ms |
| bd71f57 tracks counted before building | 528 ms | 1498 ms | 398 ms | 1164 ms | 60.7 ms |
| cba2d7f per-area order on arrays | 392 ms | 1424 ms | 268 ms | 1093 ms | 61.3 ms |
| d7d3308 metrics skip far pairs | 392 ms | 1086 ms | 268 ms | 839 ms | 60.7 ms |

Every step keeps the output: the bench fingerprints of the pipeline (16 datasets, nine uniform views) are unchanged by every
step except the count of tracks before building, which changes the length of one dataset (activity-order 18794 -> 18822,
the same crossings and bends); the default and layeredConstruction outputs are unchanged; the scene metrics of all engines
on nine views (88 rows) are unchanged by the metric step.

What remains (profile of the refinement of layerweave-code after the last step): the candidate layouts of the refinement
(73 %; each is a whole run of the pipeline), within them the search for the few routes the construction cannot build
(22 %), the placement's remaining lookups by id, and the published key.
