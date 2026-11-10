# Layout plan: routes through the dummies' places (tried, reverted), 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision a61504f plus the change in
`layout-plan-vias-20261031.patch`. `bench --reps 3 --threads 1`, medians; limit 100 (tree-100: 1000). Raw rows:
`layout-plan-vias-20261031.jsonl`. Compared with `all-datasets-off-20261031.jsonl` and `layout-graph-dummies-20261031.jsonl`.

Change tried (behind `layeredConstruction`): a layout plan carried the places of the long-edge dummies from the placement (`place`
records them per area and lifted link, before removing the dummies) through `Run` and `route_all` to the construction. The constructor
required every step along the flow that passes a dummy's column to run inside the dummy's place ("via"), added the place's tracks to
its lanes, and fell back to the search without vias when that failed.

| dataset | refinement ms: off / dummies / vias | grid pops | crossings | length | bends |
|---|---|---|---|---|---|
| layerweave-code | 7189 / 2181 / 7354 | 2251396 / 743329 / 628607 | 1327 / 1883 / 2303 | 238956 / 286863 / 299324 | 748 / 567 / 621 |
| petri-net | 137 / 88 / 95 | 562 / 6099 / 6286 | 67 / 95 / 85 | 12988 / 11444 / 11486 | 126 / 104 / 104 |
| layerweave-pipeline | 82 / 65 / 66 | 11572 / 18110 / 15063 | 20 / 21 / 21 | 14923 / 14536 / 14536 | 80 / 82 / 82 |
| other layered datasets | within ±5 ms of dummies | equal | equal | equal | within ±2 |

On layerweave-code (one import, all widening rounds): 544 constructions with places; 406 succeed through them, 62 only without
them, 76 not at all; ignoring the routes already built, a way through the places always exists. The routing stage of the refinement
grows from 1.8 s to 7.0 s: every edge with places searches with more lanes, and the failures run several full searches.

Reading: the vias lower the repair search (−15 % grid pops) but triple the refinement time and raise the crossings by 22 %. All edges
of one link share one place, while their ports lie at different heights, so they cross around it. Reverted to the dummies step. What it
shows: forcing a fixed passage is not enough; the order of the edges inside a passage has to follow their ports on both sides, and a
failed passage should not cost a full second search.
