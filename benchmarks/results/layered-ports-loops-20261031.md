# Layered construction: delegated edges keep their moved ports, loop corners, straight-edge ports, 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision ac137e6 plus
uncommitted changes (all behind `layeredConstruction`; default output unchanged, 16 of 16 refinement fingerprints).
`bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000). Raw rows: `layered-ports-loops-20261031.jsonl`.
Compared with the existing path (`all-datasets-off-20261031.jsonl`, `layered-crossings-20261031.jsonl` for the last three
datasets) and with the state of `layered-crossings-20261031.md` ("before"). layerweave-code is the graph of 0bcfd56 (669 / 1084).
The machine was not idle.

Changes against "before":
1. No repeated construction. The construction runs once with every port slot in the order of the ways; a delegated edge
   whose slot moved is drawn by the existing router from the new slot (its initial route is reattached there with a jog),
   so no two edges share a slot. Before, a failed edge left the reordering, the construction ran again, and an edge that
   had been built in the first round (petri-net e12) could fail and fall back to its old slot on the wrong end of the side.
2. Self-loops take the corner whose two sides carry the fewest other edges (ties: the usual loop sides), and their ports
   lie at that corner: on rectangles at the end of the side, on circles on the arc next to the corner.
3. The straight style resolves ports by the same rule as the routing and the published ports. Before, 47 of the 99
   straight edges of tree-100 ended off their published ports.

| dataset | refinement ms: off / before / now | grid pops | crossings | length | bends | hard |
|---|---|---|---|---|---|---|
| petri-net | 137 / 72 / 63 | 562 / 948 / 667 | 67 / 92 / 92 | 12988 / 13676 / 13616 | 126 / 106 / 106 | 0 / 0 / 0 |
| er-diagram | 21 / 51 / 52 | 1340 / 0 / 0 | 4 / 1 / 1 | 3658 / 4242 / 4242 | 33 / 32 / 32 | 0 / 0 / 0 |
| state-order | 13 / 5 / 5 | 0 / 0 / 0 | 0 / 0 / 0 | 2775 / 2762 / 2762 | 26 / 25 / 25 | 0 / 0 / 0 |
| activity-order | 113 / 104 / 112 | 17156 / 32463 / 32463 | 70 / 70 / 70 | 20757 / 19982 / 19982 | 108 / 102 / 102 | 0 / 0 / 0 |
| uml-class | 38 / 31 / 32 | 0 / 0 / 0 | 8 / 8 / 7 | 9782 / 9791 / 9734 | 49 / 51 / 51 | 0 / 0 / 0 |
| bpmn-pools | 31 / 25 / 26 | 0 / 738 / 738 | 8 / 7 / 7 | 10750 / 9923 / 9923 | 46 / 44 / 44 | 0 / 0 / 0 |
| network-vlan | 130 / 114 / 119 | 16345 / 21891 / 21891 | 71 / 72 / 72 | 28667 / 24832 / 24832 | 136 / 118 / 118 | 0 / 0 / 0 |
| layerweave-pipeline | 82 / 53 / 62 | 11572 / 17700 / 17757 | 20 / 22 / 22 | 14923 / 14090 / 14090 | 80 / 80 / 80 | 0 / 0 / 0 |
| cloud-architecture | 54 / 59 / 60 | 20806 / 20456 / 20456 | 35 / 31 / 31 | 24789 / 26073 / 26073 | 90 / 96 / 96 | 0 / 0 / 0 |
| mixed-layouts | 75 / 83 / 88 | 0 / 0 / 0 | 5 / 5 / 5 | 3715 / 3715 / 3715 | 0 / 0 / 0 | 25 / 25 / 25 |
| nested-tree-radial | 143 / 135 / 136 | 0 / 0 / 0 | 0 / 0 / 0 | 18099 / 18099 / 18099 | 88 / 88 / 88 | 0 / 0 / 0 |
| tree-100 | 146 / 81 / 82 | 0 / 0 / 0 | 0 / 0 / 0 | 24768 / 24768 / 27885 | 0 / 0 / 0 | 196 / 196 / 196 |
| state-tcp | 22 / 7 / 8 | 0 / 0 / 0 | 5 / 0 / 0 | 6101 / 5757 / 5757 | 47 / 38 / 38 | 0 / 0 / 0 |
| deployment-cluster | 157 / 141 / 147 | 49278 / 25170 / 39324 | 176 / 183 / 162 | 35854 / 41981 / 42055 | 200 / 148 / 156 | 0 / 0 / 0 |
| self-loops | 32 / 22 / 22 | 0 / 0 / 0 | 15 / 10 / 0 | 2671 / 2646 / 2321 | 30 / 30 / 30 | 21 / 22 / 23 |
| layerweave-code | 7189 / 2857 / 2095 | 2251396 / 630836 / 638755 | 1327 / 1671 / 1603 | 238956 / 299107 / 300831 | 748 / 545 / 561 | 0 / 0 / 0 |

Totals over the 13 layered datasets (off / before / now): refinement ms 8173 / 3669 / 2932; grid pops 2319177 / 725032 / 732727; crossings 1615 / 1979 / 1910; length 414626 / 471061 / 475784; bends 1530 / 1287 / 1303

Reading:
- Crossings fall by 3.5 % against "before" (1979 → 1910 over the 13 layered datasets); deployment-cluster falls from 183
  to 162, below the existing path (176); self-loops from 10 to 0 (no loop crosses any route, counted at every point,
  shared end nodes included); layerweave-code from 1671 to 1603.
- The refinement is 20 % faster than "before" (3.67 → 2.93 s; layerweave-code 2.86 → 2.10 s), because the construction no
  longer repeats.
- tree-100 is 12.6 % longer: its straight edges now start and end at the flow-side ports that were already published
  (bottom to top), where before they used other ports than the published ones.
- self-loops has one hard violation more (23 against 22, existing path 21). The metric counts every route that ends on a
  round node as a hit on that node (e1 to e6 at busy and wait in the existing path too); the loops of busy and wait now
  end on those circles as well. No route passes through a foreign node.
- The existing path still has fewer crossings in total (1615).
