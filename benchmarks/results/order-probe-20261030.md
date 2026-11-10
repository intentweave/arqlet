# Layer order measurement, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, revision 0bcfd56 plus uncommitted changes, load about 5 (not idle).
Views: `--limit 100`, refinement with one thread. Script: `order-probe.mjs`.
Bilayer crossings count pairs of edges between two neighbouring ranks of one fixed-flow area that are in opposite order on the two
ranks. Ranks are the groups of leaves with the same start along the flow. Edges that skip a rank (long edges) are not counted: they
would need dummy nodes, which the probe does not model. The barycenter order keeps the rank membership of the layout.
Routed crossings are the crossings of the finished layout (all edges), as in the comparison.

| dataset | adjacent edges | long edges | bilayer crossings, current order | after barycenter | routed crossings (of which at shared endpoints) |
|---|---:|---:|---:|---:|---:|
| petri-net | 33 | 8 | 30 | 23 | 52 (15) |
| activity-order | 20 | 0 | 0 | 0 | 35 (35) |
| network-vlan | 25 | 0 | 0 | 0 | 31 (40) |
| layerweave-pipeline | 16 | 4 | 0 | 0 | 16 (4) |
| cloud-architecture | 8 | 0 | 0 | 0 | 29 (6) |
| bpmn-pools | 12 | 3 | 0 | 0 | 7 (1) |
| uml-class | 0 | 0 | 0 | 0 | 6 (2) |
| state-order | 7 | 2 | 0 | 0 | 0 (1) |
| er-diagram | 0 | 0 | 0 | 0 | 1 (3) |
| layerweave-code | 71 | 24 | 12 | 10 | 1058 (269) |

Reading: the current order accounts for most routed crossings only on petri-net (30 of 52) and to a small extent on
layerweave-code (12 of 1058). On the other datasets the order has no bilayer crossings, so their crossings come from routing
(port sides, track choice). A barycenter order would remove 7 of the 52 petri-net crossings.
