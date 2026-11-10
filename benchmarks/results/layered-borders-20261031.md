# Layered construction: border nodes of containers (step 6, border order), 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision 19275ee plus
uncommitted changes (behind `layeredConstruction`; default output unchanged, 16 of 16 refinement fingerprints).
`bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000); layerweave-code is the graph of 0bcfd56
(669 / 1084). Raw rows: `layered-borders-20261031.jsonl`. "before" is `layered-cross-areas-20261031.jsonl` (19275ee). The
machine was not idle.

Change (kept, sub-step 1): in a container whose flow runs along the same axis as its parent's, every edge between a child
and the outside gets a border node, a dummy in an extra rank at the border it leaves (after the last rank) or enters (before
the first). Its link to the child takes part in the container's order and placement, so the container's border order
follows its inside. The port of such an edge's inner end is keyed by the chain to its border node. A border rank is placed
with the margin and a track before or after it instead of the gap between ranks.

| dataset | refinement ms: off / before / now | grid pops | crossings | length | bends | hard |
|---|---|---|---|---|---|---|
| petri-net | 137 / 46 / 46 | 562 / 0 / 0 | 67 / 64 / 64 | 12988 / 15428 / 15428 | 126 / 102 / 102 | 0 / 0 / 0 |
| er-diagram | 21 / 61 / 83 | 1340 / 0 / 0 | 4 / 0 / 0 | 3658 / 2677 / 2677 | 33 / 30 / 30 | 0 / 0 / 0 |
| state-order | 13 / 6 / 7 | 0 / 0 / 0 | 0 / 0 / 0 | 2775 / 2645 / 2645 | 26 / 31 / 31 | 0 / 0 / 0 |
| activity-order | 113 / 107 / 107 | 17156 / 15006 / 15006 | 70 / 72 / 72 | 20757 / 18016 / 18016 | 108 / 82 / 82 | 0 / 0 / 0 |
| uml-class | 38 / 44 / 45 | 0 / 0 / 0 | 8 / 5 / 5 | 9782 / 11182 / 11182 | 49 / 61 / 61 | 0 / 0 / 0 |
| bpmn-pools | 31 / 30 / 31 | 0 / 6138 / 0 | 8 / 7 / 6 | 10750 / 9695 / 10767 | 46 / 60 / 62 | 0 / 0 / 0 |
| network-vlan | 130 / 63 / 55 | 16345 / 0 / 0 | 71 / 55 / 62 | 28667 / 29916 / 29617 | 136 / 106 / 102 | 0 / 0 / 0 |
| layerweave-pipeline | 82 / 87 / 91 | 11572 / 51447 / 51447 | 20 / 33 / 33 | 14923 / 13690 / 13690 | 80 / 94 / 94 | 0 / 0 / 0 |
| cloud-architecture | 54 / 206 / 242 | 20806 / 0 / 0 | 35 / 22 / 23 | 24789 / 25325 / 26349 | 90 / 85 / 83 | 0 / 0 / 0 |
| mixed-layouts | 75 / 83 / 86 | 0 / 0 / 0 | 5 / 5 / 5 | 3715 / 3715 / 3715 | 0 / 0 / 0 | 25 / 25 / 25 |
| nested-tree-radial | 143 / 138 / 148 | 0 / 0 / 0 | 0 / 0 / 0 | 18099 / 18099 / 18099 | 88 / 88 / 88 | 0 / 0 / 0 |
| tree-100 | 146 / 81 / 84 | 0 / 0 / 0 | 0 / 0 / 0 | 24768 / 27885 / 27885 | 0 / 0 / 0 | 196 / 196 / 196 |
| state-tcp | 22 / 9 / 10 | 0 / 0 / 0 | 5 / 0 / 0 | 6101 / 5798 / 5798 | 47 / 38 / 38 | 0 / 0 / 0 |
| deployment-cluster | 157 / 198 / 190 | 49278 / 64098 / 24732 | 176 / 155 / 117 | 35854 / 37754 / 53091 | 200 / 186 / 156 | 0 / 0 / 0 |
| self-loops | 32 / 22 / 23 | 0 / 0 / 0 | 15 / 0 / 0 | 2671 / 2553 / 2553 | 30 / 30 / 30 | 21 / 23 / 23 |
| layerweave-code | 7189 / 2215 / 2693 | 2251396 / 870062 / 604379 | 1327 / 1116 / 941 | 238956 / 272138 / 376339 | 748 / 603 / 555 | 0 / 0 / 0 |

Totals over the 13 layered datasets (off / before / now): refinement ms 8173 / 3167 / 3719; grid pops 2319177 / 942653 / 670832; crossings 1615 / 1379 / 1211; length 414626 / 450411 / 556409; bends 1530 / 1342 / 1290

Extent of the refined drawing (width x height of the root area), committed state against this one:
layerweave-code 6764 x 1770 → 8818 x 2424 (+79 % area), deployment-cluster 2874 x 928 → 3594 x 1252 (+69 %), bpmn-pools
+9 %, network-vlan +2 %, cloud-architecture +18 %, petri-net unchanged.

Sub-steps tried and not kept (patch: `layered-border-routes-20261031.patch`, both together):

| variant | crossings, 13 layered | layerweave-code | deployment-cluster | length layerweave-code |
|---|---:|---:|---:|---:|
| before (19275ee) | 1379 | 1116 | 155 | 272 k |
| 1: border nodes (kept) | 1211 | 941 | 117 | 376 k |
| 1 + 2: sub-positions of a container's sections in the parent's order, from its border order | 1244 | 975 | 117 | 383 k |
| 1 + 2 + 3: routes through the border nodes (up each container's chain to its border node, the chain in the common area, down the same way) | 1365 | 1091 | 146 | 392 k |
| 1 with narrower gaps for dummies and border nodes (track spacing, margin) | 2097 | 1786 (27 hard violations) | 172 | 379 k |
| 1 with two track spacings between neighbouring dummies only | – | 1021 | 115 | 381 k |

Reading:
- The border nodes lower the crossings by 12 % (1379 → 1211, existing path 1615): layerweave-code 1116 → 941, deployment-cluster
  155 → 117. The refinement takes 17 % longer (3.17 → 3.70 s), grid pops fall by 29 %.
- The drawings of nested same-axis containers grow: length +23 % in total, layerweave-code's area +79 %. The extra border
  ranks and their channels (one track per lane change) add along the flow at every level of nesting.
- Routing an edge as one route through the border nodes is worse than searching it; the next step is to route the inner and
  the outer piece separately, joined at the border point (suggestion of the user, 2026-10-31).
