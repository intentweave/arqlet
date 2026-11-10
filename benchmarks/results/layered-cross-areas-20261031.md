# Layered construction: edges between areas of different flow axes (step 6, first slice), 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision d666a19 plus
uncommitted changes (behind `layeredConstruction`; default output unchanged, 16 of 16 refinement fingerprints).
`bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000); layerweave-code is the graph of 0bcfd56
(669 / 1084). Raw rows of the kept variant: `layered-cross-areas-20261031.jsonl`. "before" is
`layered-sections-20261031.jsonl` (d666a19). The machine was not idle.

Change (kept, variant e):
- Ports: the flow sides of an edge follow the smallest area that holds both ends, also when an end lies in a container of
  the other flow axis; the end then leaves towards the container's border where the edge goes on. A passed area needs a
  fixed flow of either axis.
- Construction: such edges are built in the frame of that area (they were delegated as `CrossesArea`); the children of the
  containers are obstacles. Only an area without a fixed flow on the way still delegates.
- Port order keys come from the chain between the containers in the common area's plan.
- An edge out of a container of the other axis also gets a route from that chain (on its port's lane to the container's
  border, then the chain's dummies and channels). It is built last, as the better of that route and its search by the
  crossings with all built routes, and only when the port reordering did not move its port.

Variants measured (routed crossings after refinement):

| variant | total, 13 layered | activity-order | layerweave-pipeline | cloud-architecture | network-vlan | deployment-cluster | bpmn-pools | layerweave-code |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| before (steps 4 + 5) | 1414 | 82 | 33 | 30 | 54 | 154 | 5 | 1136 |
| a: built in the outer frame by search | 1379 | 72 | 33 | 22 | 55 | 187 | 7 | 1116 |
| b: + routes from the chain through every container | 1529 | 49 | 51 | 24 | 57 | 184 | 7 | 1267 |
| c: chains only through containers of the other axis | 1376 | 49 | 51 | 22 | 57 | 161 | 7 | 1116 |
| d: + those edges last, better of chain route and search | 1374 | 53 | 51 | 22 | 51 | 163 | 7 | 1116 |
| e: + those edges in the port reordering (kept) | 1379 | 72 | 33 | 22 | 55 | 155 | 7 | 1116 |

Full table of the kept variant:

| dataset | refinement ms: off / before / now | grid pops | crossings | length | bends | hard |
|---|---|---|---|---|---|---|
| petri-net | 137 / 46 / 46 | 562 / 0 / 0 | 67 / 64 / 64 | 12988 / 15428 / 15428 | 126 / 102 / 102 | 0 / 0 / 0 |
| er-diagram | 21 / 56 / 61 | 1340 / 0 / 0 | 4 / 0 / 0 | 3658 / 2677 / 2677 | 33 / 30 / 30 | 0 / 0 / 0 |
| state-order | 13 / 6 / 6 | 0 / 0 / 0 | 0 / 0 / 0 | 2775 / 2645 / 2645 | 26 / 31 / 31 | 0 / 0 / 0 |
| activity-order | 113 / 145 / 107 | 17156 / 11599 / 15006 | 70 / 82 / 72 | 20757 / 18137 / 18016 | 108 / 84 / 82 | 0 / 0 / 0 |
| uml-class | 38 / 38 / 44 | 0 / 0 / 0 | 8 / 5 / 5 | 9782 / 11119 / 11182 | 49 / 61 / 61 | 0 / 0 / 0 |
| bpmn-pools | 31 / 41 / 30 | 0 / 6150 / 6138 | 8 / 5 / 7 | 10750 / 10144 / 9695 | 46 / 48 / 60 | 0 / 0 / 0 |
| network-vlan | 130 / 132 / 63 | 16345 / 26889 / 0 | 71 / 54 / 55 | 28667 / 29000 / 29916 | 136 / 128 / 106 | 0 / 0 / 0 |
| layerweave-pipeline | 82 / 91 / 87 | 11572 / 49993 / 51447 | 20 / 33 / 33 | 14923 / 13564 / 13690 | 80 / 100 / 94 | 0 / 0 / 0 |
| cloud-architecture | 54 / 216 / 206 | 20806 / 70906 / 0 | 35 / 30 / 22 | 24789 / 25093 / 25325 | 90 / 99 / 85 | 0 / 0 / 0 |
| mixed-layouts | 75 / 83 / 83 | 0 / 0 / 0 | 5 / 5 / 5 | 3715 / 3715 / 3715 | 0 / 0 / 0 | 25 / 25 / 25 |
| nested-tree-radial | 143 / 136 / 138 | 0 / 0 / 0 | 0 / 0 / 0 | 18099 / 18099 / 18099 | 88 / 88 / 88 | 0 / 0 / 0 |
| tree-100 | 146 / 81 / 81 | 0 / 0 / 0 | 0 / 0 / 0 | 24768 / 27885 / 27885 | 0 / 0 / 0 | 196 / 196 / 196 |
| state-tcp | 22 / 9 / 9 | 0 / 0 / 0 | 5 / 0 / 0 | 6101 / 5798 / 5798 | 47 / 38 / 38 | 0 / 0 / 0 |
| deployment-cluster | 157 / 182 / 198 | 49278 / 55568 / 64098 | 176 / 154 / 155 | 35854 / 38019 / 37754 | 200 / 172 / 186 | 0 / 0 / 0 |
| self-loops | 32 / 22 / 22 | 0 / 0 / 0 | 15 / 0 / 0 | 2671 / 2553 / 2553 | 30 / 30 / 30 | 21 / 23 / 23 |
| layerweave-code | 7189 / 2178 / 2215 | 2251396 / 812001 / 870062 | 1327 / 1136 / 1116 | 238956 / 270989 / 272138 | 748 / 600 / 603 | 0 / 0 / 0 |

Totals over the 13 layered datasets (off / before / now): refinement ms 8173 / 3249 / 3167; grid pops 2319177 / 977538 / 942653; crossings 1615 / 1414 / 1379; length 414626 / 448495 / 450411; bends 1530 / 1371 / 1342

Reading:
- Crossings fall from 1414 to 1379 (existing path 1615); cloud-architecture 30 → 22 and network-vlan are built without any
  grid search (grid pops 0), activity-order 82 → 72, layerweave-code 1136 → 1116. The refinement is slightly faster
  (3.25 → 3.17 s). bpmn-pools rises from 5 to 7.
- The routes from the chain help where the containers flow along the other axis (activity-order down to 49), but their
  straight run from the port to the container's border crosses the container's own edges (layerweave-pipeline 33 → 51,
  layerweave-code 1136 → 1267 when used through every container). The border crossings are not yet nodes of the container's
  plan; ordering them with the container's own order (border negotiation) is the missing part of step 6.
