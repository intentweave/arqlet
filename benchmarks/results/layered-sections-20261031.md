# Layered construction: aligned dummies, channel widths and routes from sections (steps 4 and 5), 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision cb403ee plus
uncommitted changes (behind `layeredConstruction`; default output unchanged, 16 of 16 refinement fingerprints).
`bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000); layerweave-code is the graph of 0bcfd56
(669 / 1084). Raw rows: `layered-sections-20261031.jsonl`. "before" is `layered-plan-ports-20261031.jsonl` (cb403ee). The
machine was not idle.

Changes (plan section 4a):
- Step 4, in the layered arrangement: the plan's sections (dummies included) slide towards the median of their neighbours,
  so that the dummies of a long edge come to lie on one line; every channel between two ranks is made as wide as its lane
  changes need (one track each, and the margin from both ranks), and the ranks after it move along the flow.
- Step 5, `constructive::construct_planned`: an edge whose ends are children of one planned area is built from its
  sections: on the lane of its start port up to the first channel, through the places of its dummies (parallel edges share a
  dummy's width), to the lane of its end port, changing lane only in the channels between ranks, on the track of its place in
  the channel's order. These routes are built first; an edge whose route meets an obstacle or a built route is searched as
  before. The layout pass hands its plans to the construction; `layered_plans` returns the plans of a complete first layout.

Edges built from sections in the import (of those with a chain): petri-net 33 of 41, state-tcp 18 of 19, network-vlan 25 of
25, activity-order 20 of 20, bpmn-pools 15 of 15, uml-class 17 of 17, layerweave-code 75 of 110, deployment-cluster 7 of 7.

| dataset | refinement ms: off / before / now | grid pops | crossings | length | bends | hard |
|---|---|---|---|---|---|---|
| petri-net | 137 / 67 / 46 | 562 / 0 / 0 | 67 / 79 / 64 | 12988 / 11620 / 15428 | 126 / 100 / 102 | 0 / 0 / 0 |
| er-diagram | 21 / 21 / 56 | 1340 / 0 / 0 | 4 / 2 / 0 | 3658 / 4141 / 2677 | 33 / 33 / 30 | 0 / 0 / 0 |
| state-order | 13 / 5 / 6 | 0 / 0 / 0 | 0 / 0 / 0 | 2775 / 2762 / 2645 | 26 / 25 / 31 | 0 / 0 / 0 |
| activity-order | 113 / 105 / 145 | 17156 / 32463 / 11599 | 70 / 70 / 82 | 20757 / 19982 / 18137 | 108 / 102 / 84 | 0 / 0 / 0 |
| uml-class | 38 / 33 / 38 | 0 / 0 / 0 | 8 / 7 / 5 | 9782 / 9734 / 11119 | 49 / 51 / 61 | 0 / 0 / 0 |
| bpmn-pools | 31 / 26 / 41 | 0 / 738 / 6150 | 8 / 7 / 5 | 10750 / 9923 / 10144 | 46 / 44 / 48 | 0 / 0 / 0 |
| network-vlan | 130 / 106 / 132 | 16345 / 21814 / 26889 | 71 / 85 / 54 | 28667 / 24944 / 29000 | 136 / 122 / 128 | 0 / 0 / 0 |
| layerweave-pipeline | 82 / 58 / 91 | 11572 / 17646 / 49993 | 20 / 22 / 33 | 14923 / 14090 / 13564 | 80 / 80 / 100 | 0 / 0 / 0 |
| cloud-architecture | 54 / 59 / 216 | 20806 / 22408 / 70906 | 35 / 31 / 30 | 24789 / 26073 / 25093 | 90 / 96 / 99 | 0 / 0 / 0 |
| mixed-layouts | 75 / 83 / 83 | 0 / 0 / 0 | 5 / 5 / 5 | 3715 / 3715 / 3715 | 0 / 0 / 0 | 25 / 25 / 25 |
| nested-tree-radial | 143 / 136 / 136 | 0 / 0 / 0 | 0 / 0 / 0 | 18099 / 18099 / 18099 | 88 / 88 / 88 | 0 / 0 / 0 |
| tree-100 | 146 / 86 / 81 | 0 / 0 / 0 | 0 / 0 / 0 | 24768 / 27885 / 27885 | 0 / 0 / 0 | 196 / 196 / 196 |
| state-tcp | 22 / 8 / 9 | 0 / 0 / 0 | 5 / 0 / 0 | 6101 / 5757 / 5798 | 47 / 38 / 38 | 0 / 0 / 0 |
| deployment-cluster | 157 / 158 / 182 | 49278 / 39324 / 55568 | 176 / 178 / 154 | 35854 / 42137 / 38019 | 200 / 155 / 172 | 0 / 0 / 0 |
| self-loops | 32 / 22 / 22 | 0 / 0 / 0 | 15 / 0 / 0 | 2671 / 2321 / 2553 | 30 / 30 / 30 | 21 / 23 / 23 |
| layerweave-code | 7189 / 2223 / 2178 | 2251396 / 609399 / 812001 | 1327 / 1254 / 1136 | 238956 / 284519 / 270989 | 748 / 583 / 600 | 0 / 0 / 0 |

Totals over the 13 layered datasets (off / before / now): refinement ms 8173 / 3009 / 3249; grid pops 2319177 / 704468 / 977538; crossings 1615 / 1562 / 1414; length 414626 / 457487 / 448495; bends 1530 / 1324 / 1371

Step 4 alone (routes not built from sections), one run each: activity-order 82, layerweave-pipeline 31, cloud-architecture
30 (refinement 220 ms), network-vlan 54, petri-net 68. Most of the change, the gains and the losses, comes from step 4.

Reading:
- Crossings fall by 9.5 % against "before" (1562 → 1414) and are 12 % below the existing path (1615). network-vlan 85 → 54
  (existing path 71), layerweave-code 1254 → 1136 (1327), petri-net 79 → 64 (67), deployment-cluster 178 → 154 (176).
- activity-order (70 → 82) and layerweave-pipeline (22 → 33) rise, and cloud-architecture's refinement takes 216 ms instead of
  59 ms. In these datasets most edges run between areas of different flow axes and are delegated; the alignment and the
  wider channels move the children of an area, and the delegated edges pay for it (grid pops +39 % in total).
- The refinement takes 8 % longer (3.01 → 3.25 s).
