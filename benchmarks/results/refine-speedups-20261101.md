# Shorter refinement for the layered construction: first measures, 2026-11-01

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision d974441 plus
uncommitted changes (behind `layeredConstruction`; default output unchanged, 16 of 16 refinement fingerprints).
`bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000); layerweave-code is the graph of 0bcfd56
(669 / 1084). Baseline: `layered-borders-20261031.jsonl` (f5675c1). Raw rows: `refine-speedups-20261101.jsonl` (points 1 and
2), `refine-candidates-20261101.jsonl` (ordering candidates, with a `candidates` field). The machine was not idle.

Candidates from `refine-time-breakdown-20261031.md`:

| point | change | refinement, 16 datasets | geometry | crossings, 13 layered |
|---|---|---:|---|---:|
| baseline | | 3941 ms | | 1211 |
| 1 + 2 (kept) | no second search only to name a failed edge's reason; one obstacle index per edge for all its searches | 3828 ms (−2.9 %) | identical on 16 of 16 | 1211 |
| + 3 (not kept) | no way over the gaps' centres for edges built from sections (they leave the corridor order) | 3760 ms (−4.6 %) | changed on 5 | 1240 |

The constructor gets a different input in every one of its calls (16 on layerweave-code, 10 on deployment-cluster: boxes, ports
and plans hashed per call), so caching whole constructions saves nothing.

Ordering candidates of the refinement (option `candidates`; the default is all four), 13 layered datasets:

| candidates | refinement | crossings | length | layerweave-code | activity-order | layerweave-pipeline |
|---|---:|---:|---:|---:|---:|---:|
| 1, 2, 3, 4 (default) | 3622 ms | 1211 | 556 k | 2672 ms, 941 | 72 | 33 |
| 1 | 1649 ms | 1244 | 567 k | 1138 ms, 953 | 86 | 40 |
| 1, 2 | 2050 ms | 1478 | 592 k | 1381 ms, 1199 | 86 | 28 |
| 1, 3 | 2334 ms | 1290 | 565 k | 1694 ms, 953 | 111 | 61 |

Reading:
- Points 1 and 2 are exact and cheap but small (−2.9 %); point 3 costs crossings for −1.8 % more.
- The time is in the number of searches: every candidate places, constructs and routes again. One candidate halves the
  refinement (−54 %) for 2.7 % more crossings. More candidates are not monotone: [1, 2] has more crossings than [1] alone
  (layerweave-code 1199 against 953). Before the crossings the objectives rank only node overlaps, edge-node hits and shared
  routes, and the measured drawings have no hard violations, so why a candidate with more crossings is selected is open (the
  selection may compare candidates before their final routing).
- Choosing the candidate set is a default for the experimental option and is left to a decision.
