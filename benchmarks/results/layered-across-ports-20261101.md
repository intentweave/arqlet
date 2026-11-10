# Layered construction: ends in containers of the other flow axis on their own flow side (tried, reverted), 2026-11-01

Machine: Apple M3, release, one bench run each (`--reps 1 --threads 1`, limit 100), revision 7d85608 plus the patch
`layered-across-ports-20261101.patch`. The machine was not idle.

Why: in activity-order and layerweave-pipeline (root flows top to bottom, the containers inside left to right) most
delegated edges run between containers of different axes. Their port at a node inside a container lies on the side of the
common flow (top), but the nodes of a rank inside such a container stand one under the other, so the top of a deeper node
is reached only between its neighbours, and the route from the plan's sections runs through them (activity-order e17,
e21, e25, e29, e33 into pick-1..5).

Tried: such an end takes the side of its own area's flow (the target is entered on the left), and the constructor meets
the port at its height on a track beside the node in the container's channel (deeper targets on farther tracks), joined by
a stub.

| dataset | built from sections | delegated | crossings | bends |
|---|---|---|---|---|
| activity-order | 22 → 26 | 15 → 8 | 54 → 127 | 70 → 108 |
| layerweave-pipeline | 21 → 26 | 10 → 5 | 38 → 52 | 74 → 92 |
| network-vlan | | | 62 → 91 | 106 → 123 |
| bpmn-pools | | | 6 → 13 | 46 → 50 |
| deployment-cluster | 9 → 8 | 4 → 0 | 107 → 109 | 136 → 137 |
| layerweave-code | 103 → 103 | 4 → 4 | 900 → 900 | 493 → 493 |

Reverted. The run down the channel to a deeper node's left side crosses the container's own edges into the nodes above it
(check-1 → pick-1 and check-2 → pick-2 for pick-3); the searched routes over the top, between the nodes, cross fewer. The
side alone does not help: what would is an order inside the container that takes its edges from outside into account (the
negotiation of border order between a container and its parent, still open).

## Where the crossings arise (for the crossing nodes, plan step 2 of the concept)

Same revision (7d85608, unchanged code), import and refinement with the option. "Planned": the bilayer crossings of the
order in all area plans (`AreaPlan::crossings`, including the chains to border nodes). "Routed": crossings of the drawn
routes, split by whether both edges have their ends in one area.

| dataset | planned | routed, both edges in one area | routed, other |
|---|---:|---:|---:|
| petri-net | 56 | 59 | 0 |
| activity-order | 0 | 0 | 52 |
| uml-class | 0 | 0 | 5 |
| bpmn-pools | 0 | 1 | 5 |
| network-vlan | 28 | 0 | 57 |
| layerweave-pipeline | 3 | 5 | 31 |
| cloud-architecture | 1 | 0 | 29 |
| state-tcp | 0 | 0 | 0 |
| deployment-cluster | 9 | 0 | 105 |
| layerweave-code | 216 | 83 | 745 |

Inside an area the routes already keep to the order (petri-net 59 routed against 56 planned; the others fewer). Almost
all crossings are between edges of different areas or edges that leave their area (layerweave-code 745 of 828), and most
of them are not in any plan's order. Crossing nodes inside an area would save a few (petri-net 3, layerweave-pipeline 2);
the lever is the border order between a container and its parent, including containers of the other axis.
