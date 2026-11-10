# Layered construction: port order from the area plan (step 3), 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision 01aaf0b plus
uncommitted changes (behind `layeredConstruction`; default output unchanged). `bench --reps 3 --threads 1`, medians; limit
100 (er-diagram and tree-100: 1000); layerweave-code is the graph of 0bcfd56 (669 / 1084). Raw rows:
`layered-plan-ports-20261031.jsonl`. "before" is `layered-plan-order-20261031.jsonl` (01aaf0b). The machine was not idle.

Change: `layered_plan::port_keys` gives every automatic port of an edge inside a planned area a key, the place of the next
element of the edge's chain in the neighbouring rank (first dummy or other end). The port resolution orders a node side by
these keys when all its ports have one (`PortContext::with_order_keys`); the routing of the layout pass uses them.

| dataset | refinement ms: off / before / now | grid pops | crossings | length | bends | hard |
|---|---|---|---|---|---|---|
| petri-net | 137 / 65 / 67 | 562 / 0 / 0 | 67 / 79 / 79 | 12988 / 11620 / 11620 | 126 / 100 / 100 | 0 / 0 / 0 |
| er-diagram | 21 / 57 / 21 | 1340 / 0 / 0 | 4 / 1 / 2 | 3658 / 4242 / 4141 | 33 / 32 / 33 | 0 / 0 / 0 |
| state-order | 13 / 5 / 5 | 0 / 0 / 0 | 0 / 0 / 0 | 2775 / 2762 / 2762 | 26 / 25 / 25 | 0 / 0 / 0 |
| activity-order | 113 / 104 / 105 | 17156 / 32463 / 32463 | 70 / 70 / 70 | 20757 / 19982 / 19982 | 108 / 102 / 102 | 0 / 0 / 0 |
| uml-class | 38 / 32 / 33 | 0 / 0 / 0 | 8 / 7 / 7 | 9782 / 9734 / 9734 | 49 / 51 / 51 | 0 / 0 / 0 |
| bpmn-pools | 31 / 26 / 26 | 0 / 738 / 738 | 8 / 7 / 7 | 10750 / 9923 / 9923 | 46 / 44 / 44 | 0 / 0 / 0 |
| network-vlan | 130 / 107 / 106 | 16345 / 21814 / 21814 | 71 / 85 / 85 | 28667 / 24944 / 24944 | 136 / 122 / 122 | 0 / 0 / 0 |
| layerweave-pipeline | 82 / 61 / 58 | 11572 / 17646 / 17646 | 20 / 22 / 22 | 14923 / 14090 / 14090 | 80 / 80 / 80 | 0 / 0 / 0 |
| cloud-architecture | 54 / 62 / 59 | 20806 / 22408 / 22408 | 35 / 31 / 31 | 24789 / 26073 / 26073 | 90 / 96 / 96 | 0 / 0 / 0 |
| mixed-layouts | 75 / 83 / 83 | 0 / 0 / 0 | 5 / 5 / 5 | 3715 / 3715 / 3715 | 0 / 0 / 0 | 25 / 25 / 25 |
| nested-tree-radial | 143 / 136 / 136 | 0 / 0 / 0 | 0 / 0 / 0 | 18099 / 18099 / 18099 | 88 / 88 / 88 | 0 / 0 / 0 |
| tree-100 | 146 / 83 / 86 | 0 / 0 / 0 | 0 / 0 / 0 | 24768 / 27885 / 27885 | 0 / 0 / 0 | 196 / 196 / 196 |
| state-tcp | 22 / 8 / 8 | 0 / 0 / 0 | 5 / 0 / 0 | 6101 / 5757 / 5757 | 47 / 38 / 38 | 0 / 0 / 0 |
| deployment-cluster | 157 / 159 / 158 | 49278 / 39324 / 39324 | 176 / 178 / 178 | 35854 / 42137 / 42137 | 200 / 155 / 155 | 0 / 0 / 0 |
| self-loops | 32 / 22 / 22 | 0 / 0 / 0 | 15 / 0 / 0 | 2671 / 2321 / 2321 | 30 / 30 / 30 | 21 / 23 / 23 |
| layerweave-code | 7189 / 2065 / 2223 | 2251396 / 582647 / 609399 | 1327 / 1255 / 1254 | 238956 / 284519 / 284519 | 748 / 583 / 583 | 0 / 0 / 0 |

Totals over the 13 layered datasets (off / before / now): refinement ms 8173 / 2885 / 3009; grid pops 2319177 / 677716 / 704468; crossings 1615 / 1562 / 1562; length 414626 / 457588 / 457487; bends 1530 / 1323 / 1324

Variant measured and not kept: the same with the constructor's own reordering of port slots (by the lanes of its planned
ways) switched off, so that the plan's order is final: 1702 crossings over the 13 layered datasets (before 1562),
layerweave-code 1397 (before 1255), uml-class 7 → 5, bpmn-pools 7 → 6.

Reading: the plan's port order alone is worse than the reordering by the constructed ways, and together with it changes
nothing measurable (1562 → 1562). The ports follow the plan, but the routes do not: the constructor still searches its ways
through the gaps of the geometry, not through the dummies' places. The plan's port order can pay off only when the routes are
built from the plan's sections (step 5) in channels sized for them (step 4).
