# Connectivity as a placement key, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, revision 0bcfd56 plus uncommitted changes, load about 5 (not idle).
Views: `--limit 100`. Script: `connectivity-probe.mjs`. Raw rows: `connectivity-20261030.jsonl`.

Measured on the rank order of each fixed-flow area (no change to the layout). "Crossings" are bilayer crossings between neighbouring
ranks; "distance" is the sum over the edges of the cross-flow distance of their ends (a proxy for route length). Long edges are not
counted.

| dataset | edges | crossings: current / barycenter / by degree / degree in middle | distance: current / barycenter / by degree / degree in middle |
|---|---:|---|---|
| petri-net | 33 | 30 / 23 / 51 / 64 | 48 / 38 / 61 / 79 |
| activity-order | 20 | 0 / 0 / 3 / 2 | 5 / 5 / 8 / 7 |
| network-vlan | 25 | 0 / 0 / 0 / 0 | 60 / 60 / 61 / 60 |
| layerweave-pipeline | 16 | 0 / 0 / 2 / 2 | 8 / 8 / 15 / 18 |
| cloud-architecture | 8 | 0 / 0 / 0 / 0 | 3 / 3 / 1 / 1 |
| bpmn-pools | 12 | 0 / 0 / 1 / 1 | 4 / 4 / 7 / 6 |
| state-order | 7 | 0 / 0 / 0 / 0 | 1 / 1 / 1 / 1 |
| layerweave-code | 71 | 12 / 10 / 58 / 79 | 272 / 272 / 344 / 417 |

Reading: ordering the nodes by how many edges they have is worse than the current order on every dataset with crossings or distance,
and it is much worse on petri-net and layerweave-code. The barycenter order, which places a node next to the mean position of its
neighbours, is the only key that does not lose. So the connectivity that helps placement is the position of the neighbours, not the
number of edges. The degree itself is not a usable weight for placement.

## Weighted swap (priority to close neighbours, corridors unchanged)

Local search from the barycenter order: adjacent nodes in a rank are swapped while the objective (crossings, then the weighted
cross distance, weight = number of edges between two nodes) improves. Rank membership is fixed, so the edges that cross each
corridor (and the tracks it needs) do not change; the corridor widths stay valid.

| dataset | crossings: barycenter / weighted swap | distance: barycenter / weighted swap |
|---|---|---|
| petri-net | 23 / 23 | 38 / 35 |
| activity-order | 0 / 0 | 5 / 2 |
| network-vlan | 0 / 0 | 60 / 60 |
| layerweave-pipeline | 0 / 0 | 8 / 8 |
| cloud-architecture | 0 / 0 | 3 / 3 |
| bpmn-pools | 0 / 0 | 4 / 2 |
| state-order | 0 / 0 | 1 / 1 |
| layerweave-code | 10 / 10 | 272 / 236 |

Reading: the weighted swap never adds a crossing and shortens the cross distance where the order has room (up to 13 % on
layerweave-code, 50 % on bpmn-pools and activity-order). On most datasets the barycenter order is already at its optimum for
this objective. The gain is a shorter route, not fewer routing searches; that is measured separately.
