# Layered construction: straight long edges, port sides, edit routes from the plans, 2026-11-01

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision dfd5451 plus
uncommitted changes (behind `layeredConstruction`; default output unchanged, 16 of 16 refinement fingerprints).
`bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000); layerweave-code is the graph of 0bcfd56
(669 / 1084). "off" is the existing path, dfd5451 is `layered-reuse-20261101.jsonl`. The machine was not idle.

Order of the objectives (`DEFAULT_OBJECTIVES`, lexicographic): hard terms, routed crossings, near-node segments, projected
crossings, bends, cost (length last). The previous steps reported length and area; bends rank above them and are
reported first here.

Changes (all behind the option):
1. **Straight long edges.** After the median sliding, the dummies of every long edge are put on one line across the flow
   (the line of its dummy farthest along the cross axis); the elements after a moved dummy in its rank move on, keeping
   their distances (`straighten_chains`). Before, each dummy sat where its rank's packing put it, and every offset became a
   step in the channel (state-tcp: one edge with 14 bends).
2. **Port reordering by side.** The constructor reordered the port slots of a node among all edges ending there, also
   between a top side (edges of a vertical flow) and a left side (horizontal flow), and so could give an edge the place of
   a port on another side: a route ending off its node (deployment-cluster e36 ended at y 1396, its node lies at 558–612).
   The sides are now keyed by node, end and flow axis.
3. **Edit routes from the plans.** An edit (local routing) builds the edges it affects from the plans' sections, around
   the routes it carries along (`construct_planned_subset`), also in its widening rounds; the ports follow the plans' order
   as in the full routing. Only edges the construction cannot build go to the search repair.
4. **Search index per edge** holds only the built segments within the edge's span along the flow (same answers: all 16
   refinement fingerprints with the option equal before and after).

## Refinement (all datasets)

| dataset | bends: off / dfd5451 / now | crossings | shared pairs (now) | hard: off / dfd5451 / now | refine ms now |
|---|---|---|---:|---|---:|
| petri-net | 126 / 102 / 96 | 67 / 68 / 56 | 0 | 0 / 0 / 0 | 44 |
| er-diagram | 33 / 29 / 29 | 4 / 0 / 0 | 1 | 0 / 0 / 0 | 10 |
| state-order | 26 / 31 / 31 | 0 / 0 / 0 | 0 | 0 / 0 / 0 | 5 |
| activity-order | 108 / 70 / 70 | 70 / 54 / 54 | 1 | 0 / 0 / 0 | 34 |
| uml-class | 49 / 61 / 61 | 8 / 5 / 5 | 0 | 0 / 0 / 0 | 19 |
| bpmn-pools | 46 / 46 / 46 | 8 / 6 / 6 | 0 | 0 / 0 / 0 | 17 |
| network-vlan | 136 / 102 / 106 | 71 / 64 / 59 | 0 | 0 / 0 / 0 | 49 |
| layerweave-pipeline | 80 / 74 / 74 | 20 / 38 / 38 | 3 | 0 / 0 / 0 | 33 |
| cloud-architecture | 90 / 85 / 83 | 35 / 30 / 30 | 0 | 0 / 0 / 0 | 199 |
| mixed-layouts | 0 / 0 / 0 | 5 / 5 / 5 | 0 | 25 / 25 / 25 | 23 |
| nested-tree-radial | 88 / 88 / 88 | 0 / 0 / 0 | 0 | 0 / 0 / 0 | 72 |
| tree-100 | 0 / 0 / 0 | 0 / 0 / 0 | 0 | 196 / 196 / 196 | 31 |
| state-tcp | 47 / 72 / 36 | 5 / 0 / 0 | 0 | 0 / 0 / 0 | 5 |
| deployment-cluster | 200 / 156 / 146 | 176 / 116 / 114 | 0 | 0 / 0 / 0 | 76 |
| self-loops | 30 / 30 / 30 | 15 / 0 / 0 | 0 | 21 / 23 / 23 | 7 |
| layerweave-code | 748 / 537 / 533 | 1327 / 984 / 1031 | 1 | 0 / 0 / 0 | 1208 |

off: all 16 bends 1807, crossings 1811, length 459252, hard 242, refine ms 8384
dfd5451: all 16 bends 1483, crossings 1370, length 537711, hard 244, refine ms 1714
now: all 16 bends 1429, crossings 1398, length 570720, hard 244, refine ms 1832

Bends fall by 54 (−4 %), and state-tcp, the case of the screenshot, halves (72 → 36; largest per edge 14 → 4). Crossings
rise by 28; that is layerweave-code (984 → 1031, +47), while petri-net (68 → 56), network-vlan and deployment-cluster fall.
Crossings rank above bends in the objectives, so layerweave-code is a regression to work on. Length rises by 6 %.

## Edits (five add edits, as in `layered-reuse-20261101.md`)

Columns dfd5451 / now. Overlapping segment pairs: collinear overlapping segments of two different edges (including edges
with a common end). Reference: an import of the final graph with the option (no refinement).

| dataset | crossings | overlapping segment pairs | edges repaired by search | ms for 5 edits | import of the final graph: crossings / overlaps |
|---|---|---|---|---|---|
| petri-net | 76 / 107 | 62 / 7 | 70 / 7 | 24 / 45 | 135 / 8 |
| network-vlan | 75 / 92 | 54 / 2 | 268 / 21 | 48 / 69 | 111 / 8 |
| layerweave-pipeline | 53 / 38 | 18 / 2 | 176 / 84 | 36 / 44 | 63 / 2 |
| cloud-architecture | 31 / 28 | 13 / 0 | 205 / 0 | 40 / 48 | 30 / 0 |
| deployment-cluster | 184 / 181 | 87 / 11 | 142 / 71 | 44 / 101 | 175 / 2 |
| layerweave-code | 1061 / 1119 | 822 / 121 | 794 / 261 | 507 / 1741 | 1130 / 90 |

The edits now come close to an import of the same graph: overlapping routes fall from 822 to 121 on layerweave-code (the
import has 90) and nearly vanish on the others. The crossings that look lower before were partly hidden in overlapping
routes. The cost: an edit on layerweave-code takes about 350 ms instead of 100 ms (process start and JSON included), about
an import's time. A profile shows the time in the constructor's lane search (`search_on`, a dynamic programme over
corridors and lanes) for the long edges between areas that an edit affects, repeated in each widening round. Open.
