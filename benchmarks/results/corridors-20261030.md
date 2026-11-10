# Corridor capacity of finished layouts, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, revision 0bcfd56 plus uncommitted changes, load about 5 (not idle).
Views: `--limit 100`, refinement with one thread. Script: `corridor-capacity.mjs`. Raw rows: `corridors-20261030.jsonl`.

A corridor is the free interval along the flow between two neighbouring ranks of one fixed-flow area. Its demand is the number of
tracks (segments across the flow, clustered at the track spacing 6) inside it; its capacity is the free width divided by 6.

| dataset | corridors | corridors short of tracks | shortage (tracks) | demand / capacity (all corridors) |
|---|---:|---:|---:|---:|
| petri-net | 3 | 0 | 0 | 16 / 30 |
| activity-order | 5 | 0 | 0 | 20 / 50 |
| network-vlan | 6 | 0 | 0 | 73 / 331 |
| layerweave-pipeline | 9 | 0 | 0 | 39 / 90 |
| cloud-architecture | 6 | 0 | 0 | 52 / 305 |
| bpmn-pools | 10 | 0 | 0 | 24 / 100 |
| state-order | 6 | 0 | 0 | 21 / 143 |
| layerweave-code | 17 | 0 | 0 | 75 / 170 |

Reading: in the finished layouts no corridor is short of tracks as a whole. The width the layout leaves is enough. The tracks
that the constructive router cannot place (NoFreeTrack, see `tracks-20261030.md`) are therefore missing at a particular place
(boxes block the lane there), not for lack of total width. A reservation must hold a lane at the place where it is needed, not
only a width for the corridor as a whole.
