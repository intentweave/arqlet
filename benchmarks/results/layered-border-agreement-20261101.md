# Layered construction: border agreement and routes through border nodes (tried, reverted), 2026-11-01

Machine: Apple M3, release, one bench run each (`--reps 1 --threads 1`, limit 100), revision 7d85608 plus the patch
`layered-border-agreement-20261101.patch` (variants switched by environment variables during the measurement). The machine
was not idle. layerweave-code is the graph of 0bcfd56 (a7636dbb).

Why: almost all routed crossings lie between edges of different areas (`layered-across-ports-20261101.md`). A container's
border order follows its inside, its parent arranges it as one node, and the order in which its edges go on outside is not
agreed with the border order. Edges with an end inside a container of the same axis are not built from the plans at all:
the constructor searches them (layerweave-code: 74 of 184 edges searched).

Tried:
1. **Agreement:** after the first placement, the parent's plan gives every border node a key (the place of the next
   element of its edge's chain outside, then its place at the next container's border). Every container is placed again
   with its border ranks fixed in that order (each border node its own group in `improve_order`) and its inside ordered to
   it. The second placement is kept only when the crossings of all plans plus the disagreements at the borders fall.
2. **Routes through border nodes:** an edge with an end inside a container of the same axis is built from steps across
   the areas: up the container's chain to its border node, along the common chain (parallel edges ordered by their border
   nodes), and down the chain of the container it enters (the variant of 2026-10-31, rebuilt on the steps of the current
   `section_routes`).

| variant | layerweave-code crossings | bends | length | deployment-cluster crossings | bends |
|---|---:|---:|---:|---:|---:|
| 7d85608 | 900 | 493 | 380 k | 107 | 136 |
| 1 | 962 | 529 | 377 k | 107 (second placement rejected) | 136 |
| 2 | 1667 | 699 | 451 k | 137 | 172 |
| 1 + 2 | 1627 | 735 | 422 k | 137 | 172 |

network-vlan, bpmn-pools and uml-class are unchanged by all variants; cloud-architecture 30 → 32 with 2.

Findings:
- The agreement lowers the plans' own score on layerweave-code (1239 → 1002; border disagreements 1023 → 492), but the
  routed crossings rise: the routes of edges between containers are searched and do not pass the border nodes.
- On deployment-cluster the second placement disagrees more than the first (62 → 80): ordering a container's inside to
  its parent changes the container, and the parent orders itself anew. One round of agreement does not converge.
- Routed through the border nodes, 172 instead of 110 edges have a route from the plans and 140 are built from it, but
  the built routes cross each other 1022 times (of 1156 in the import). Example: e4 leaves ns-ingress at the bottom of the
  drawing (y 1034) and its border node in ns-app lies at the top of the entering rank (y 435); the lane change between them
  spans 600 units in one channel and crosses everything entering ns-app. The orders of the levels are not consistent with
  each other, and a route that follows all of them pays for every disagreement.

What would make them consistent is an order over the levels together rather than one level at a time: the hierarchy laid
out as one layered graph with ranks across all levels and each container a contiguous block in every rank it spans (the
"include children" approach of hierarchical layered layout), so that a border crossing is a position in a common rank and
cannot disagree. Not started.
