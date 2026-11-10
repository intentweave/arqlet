# Joint order of lane changes with bound tracks, 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision 4c464fb plus uncommitted
changes. `bench --reps 3 --threads 1`, medians; limit 100 (tree-100: 1000). Raw rows: `joint-order-20261031.jsonl`. Compared with
`all-datasets-off-20261031.jsonl` (existing path) and `layout-graph-dummies-20261031.jsonl` (dummies, a61504f).

Change (behind `layeredConstruction`; default output unchanged, 16 of 16 fingerprints): the constructor first checks every edge and plans
its way over the gaps' centres. The lane changes of all edges in one corridor are ordered so that they cross least (pairwise crossing
count of the lanes before and after the change, neighbours swapped while that lowers it). Each edge is then bound to the track of its
place in that order (tracks at the track spacing, centred in the gap); only when that track is not free does it choose freely. The
earlier attempt (`corridor-order-20261030.patch`) only rewarded the place with a cost; here the place is binding.

| dataset | refinement ms: off / dummies / joint | grid pops | crossings | length | bends |
|---|---|---|---|---|---|
| petri-net | 137 / 88 / 78 | 562 / 6099 / 4158 | 67 / 95 / 105 | 12988 / 11444 / 13842 | 126 / 104 / 114 |
| activity-order | 113 / 112 / 110 | 17156 / 27694 / 37391 | 70 / 73 / 104 | 20757 / 20529 / 19534 | 108 / 106 / 110 |
| network-vlan | 130 / 121 / 120 | 16345 / 19934 / 28124 | 71 / 79 / 56 | 28667 / 24782 / 27992 | 136 / 116 / 133 |
| uml-class | 38 / 44 / 44 | 0 / 0 / 0 | 8 / 8 / 7 | 9782 / 9797 / 11170 | 49 / 51 / 55 |
| bpmn-pools | 31 / 39 / 26 | 0 / 0 / 0 | 8 / 8 / 8 | 10750 / 10828 / 10750 | 46 / 48 / 46 |
| tree-100 | 146 / 222 / 80 | 0 / 0 / 0 | 0 / 0 / 0 | 24768 / 24768 / 24768 | 0 / 0 / 0 |
| layerweave-code | 7189 / 2181 / 2944 | 2251396 / 743329 / 909875 | 1327 / 1883 / 1706 | 238956 / 286863 / 286554 | 748 / 567 / 551 |
| other layered (er-diagram, state-order, layerweave-pipeline, cloud-architecture, mixed-layouts, nested-tree-radial) | within ±3 ms | equal or +3 % | equal or +1 | within 3.5 % | within ±2 |

Totals over the 13 layered datasets (off / dummies / joint): refinement 8.17 / 3.20 / 3.80 s; grid pops 2.32 M / 0.83 M / 1.02 M;
crossings 1615 / 2210 / 2051; length 415 k / 457 k / 462 k; bends 1530 / 1311 / 1326. Hard violations unchanged everywhere.

Reading: binding the order lowers the crossings by 7 % against the dummies step (layerweave-code −9 %, network-vlan 79 → 56, below the
existing path's 71), but raises them on activity-order (73 → 104) and petri-net (95 → 105). The refinement is 19 % slower than the
dummies step (layerweave-code 2.2 s → 2.9 s) and the grid pops rise by 22 %: bound tracks fail more often, and the edges then take the
free search or are routed by the general router. The order covers only the lane changes across the flow; the lanes along the flow and the
port lanes are still free.
