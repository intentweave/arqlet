# Layout graph, first slice: long-edge dummies in order and placement, 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, rustc 1.99.0, bench with `--features benchmark-timings`. Revision 098b518 plus
uncommitted changes (the dummies). `bench --reps 3 --threads 1`, medians; limit 100 (tree-100: 1000). Compared with
`all-datasets-off-20261031.jsonl` (existing path) and `all-datasets-on-20261031.jsonl` (step 8, same conditions). Raw rows:
`layout-graph-dummies-20261031.jsonl`.

Change (behind `layeredConstruction`, layered areas only; default output unchanged, 16 of 16 fingerprints): a link between two children
of an area that skips ranks gets a dummy in each rank it passes, as wide as the tracks of its edges (track spacing × number of edges).
The order sweeps use the chain through the dummies instead of the long link, the placement gives each dummy its place in its rank, and
the dummies are removed from the result. The routing does not use their positions yet; the reserved place is found as a free gap.

| dataset | refinement ms: off / step 8 / dummies | grid pops | crossings | length | bends |
|---|---|---|---|---|---|
| petri-net | 137 / 129 / 88 | 562 / 3216 / 6099 | 67 / 80 / 95 | 12988 / 12786 / 11444 | 126 / 110 / 104 |
| er-diagram | 21 / 22 / 52 | 1340 / 1340 / 0 | 4 / 4 / 1 | 3658 / 3658 / 4254 | 33 / 33 / 32 |
| state-order | 13 / 68 / 7 | 0 / 0 / 0 | 0 / 0 / 0 | 2775 / 2775 / 2774 | 26 / 24 / 25 |
| network-vlan | 130 / 139 / 121 | 16345 / 24310 / 19934 | 71 / 79 / 79 | 28667 / 24782 / 24782 | 136 / 116 / 116 |
| layerweave-pipeline | 82 / 77 / 65 | 11572 / 11066 / 18110 | 20 / 22 / 21 | 14923 / 15132 / 14536 | 80 / 84 / 82 |
| layerweave-code | 7189 / 2255 / 2181 | 2251396 / 690073 / 743329 | 1327 / 1892 / 1883 | 238956 / 287425 / 286863 | 748 / 569 / 567 |
| other layered (activity-order, uml-class, bpmn-pools, cloud-architecture, mixed-layouts, nested-tree-radial, tree-100) | within ±7 ms of step 8 | equal or lower | equal | equal within 0.3 % | within ±2 |

Totals over the 13 layered datasets (off / step 8 / dummies): refinement 8.17 / 3.36 / 3.20 s; grid pops 2.32 M / 0.78 M / 0.83 M;
crossings 1615 / 2208 / 2210; length 415 k / 459 k / 457 k; bends 1530 / 1319 / 1311. Hard violations unchanged everywhere.

Reading: reserving the passage alone changes little. It helps where a long edge had no passage (er-diagram: no repair search, crossings
4 → 1) and costs where the new order pulls edges into conflicts (petri-net: grid pops ×2, crossings 80 → 95). On layerweave-code the
effect is within a few percent. The routing still searches its way and does not know the dummies; the second part of this slice is to
hand the dummies' places to the constructor, so that a long edge is built through them instead of searched.
