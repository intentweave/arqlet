# Layered construction: room for the tracks and the port slots, 2026-11-01

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision cba91fe plus
uncommitted changes (behind `layeredConstruction`; default output unchanged, 16 of 16 refinement fingerprints).
`bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000). layerweave-code is the graph of 0bcfd56
(sha256 a7636dbb…, 669 / 1084 nodes and edges, view 126 / 184 in every result of the day): the regression of cba91fe on
it is not a changed dataset. The machine was not idle.

## Why the sections gave way to the search

A count of the construction after the import (temporary test, removed): of the edges with a route from the plan's
sections, many were not built from it (layerweave-code 71 of 110, petri-net 22 of 41) and went to the search, where most
of the TrackTaken delegations arise. Counted over the constructions of an import, the section routes failed on a route
already built 571 times (428 of them on a lane change across the flow, 168 and 97 on the first and last piece at a port)
and on an obstacle 190 times. 174 channels had more lane changes than room for their tracks at the track spacing (2254 of
4537 lane changes), so `ordered_tracks` put them closer than the spacing. And one node side (dispatch, 25 ports on 54
units) had its ports 2 units apart.

## Changes (behind the option)

1. **Channels as wide as their tracks** (plan step 4, again): after the straightening, every channel between two ranks
   gets a track for each lane change (changes that do not overlap share one, `track_slots`) and the margin from both
   ranks. It had been removed in cce23c3 for its length; length ranks last, and without it the sections cannot be built.
2. **Port slots a track apart:** with the option, ports of a node side keep at least the track spacing; a leaf grows to the
   ports it has (the existing port sizing, with the track spacing as the least spacing). An edit sizes from the ports of
   the drawing it continues, in one local pass. Not reported as an unmet requirement unless `minPortSpacing` is set.

After both, the same count: 2254 → 718 lane changes in channels that are too narrow (76 channels; the estimate uses node
centres, not ports), conflicts on lane changes 428 → 27, and:

| dataset | with a section route | built from it: before → now | delegated: before → now |
|---|---:|---|---|
| petri-net | 41 | 22 → 34 | 5 → 0 |
| network-vlan | 43 | 27 → 28 | 5 → 1 |
| deployment-cluster | 18 | 9 → 9 | 10 → 4 |
| layerweave-code | 110 | 71 → 103 | 26 → 4 |

activity-order (22 of 40, 15 TrackTaken) and layerweave-pipeline (21 of 33, 10) are unchanged: their failures lie
elsewhere (open).

## Refinement, all datasets (existing path / cba91fe / now)

| dataset | hard | crossings | bends | length | refine ms |
|---|---|---|---|---|---|
| petri-net | 0 / 0 / 0 | 67 / 56 / 62 | 126 / 96 / 92 | 13.0k / 13.0k / 15.4k | 137 / 44 / 21 |
| er-diagram | 0 / 0 / 0 | 4 / 0 / 0 | 33 / 29 / 29 | 3.7k / 3.1k / 3.1k | 21 / 11 / 11 |
| state-order | 0 / 0 / 0 | 0 / 0 / 0 | 26 / 31 / 31 | 2.8k / 2.8k / 2.8k | 13 / 3 / 3 |
| activity-order | 0 / 0 / 0 | 70 / 54 / 54 | 108 / 70 / 70 | 20.8k / 19.0k / 19.0k | 113 / 36 / 40 |
| uml-class | 0 / 0 / 0 | 8 / 5 / 5 | 49 / 61 / 61 | 9.8k / 12.2k / 12.2k | 38 / 19 / 19 |
| bpmn-pools | 0 / 0 / 0 | 8 / 6 / 6 | 46 / 46 / 46 | 10.8k / 11.3k / 11.3k | 31 / 17 / 19 |
| network-vlan | 0 / 0 / 0 | 71 / 59 / 62 | 136 / 106 / 106 | 28.7k / 28.5k / 29.3k | 130 / 47 / 32 |
| layerweave-pipeline | 0 / 0 / 0 | 20 / 38 / 38 | 80 / 74 / 74 | 14.9k / 15.8k / 15.8k | 82 / 32 / 33 |
| cloud-architecture | 0 / 0 / 0 | 35 / 30 / 30 | 90 / 83 / 83 | 24.8k / 26.6k / 26.7k | 54 / 194 / 196 |
| mixed-layouts | 25 / 25 / 25 | 5 / 5 / 5 | 0 / 0 / 0 | 3.7k / 3.7k / 3.7k | 75 / 23 / 23 |
| nested-tree-radial | 0 / 0 / 0 | 0 / 0 / 0 | 88 / 88 / 88 | 18.1k / 18.1k / 18.1k | 143 / 72 / 72 |
| tree-100 | 196 / 196 / 196 | 0 / 0 / 0 | 0 / 0 / 0 | 24.8k / 27.9k / 27.9k | 146 / 31 / 31 |
| state-tcp | 0 / 0 / 0 | 5 / 0 / 0 | 47 / 36 / 36 | 6.1k / 5.9k / 5.9k | 22 / 5 / 5 |
| deployment-cluster | 0 / 0 / 0 | 176 / 114 / 107 | 200 / 146 / 136 | 35.9k / 54.6k / 59.9k | 157 / 75 / 87 |
| self-loops | 21 / 23 / 23 | 15 / 0 / 0 | 30 / 30 / 30 | 2.7k / 2.6k / 2.6k | 32 / 7 / 7 |
| layerweave-code | 0 / 0 / 0 | 1327 / 1031 / 900 | 748 / 533 / 493 | 239.0k / 325.8k / 379.8k | 7189 / 1208 / 1317 |

- off: hard 242, crossings 1811, bends 1807, length 459252, refinement 8384 ms
- cba91fe: hard 244, crossings 1398, bends 1429, length 570720, refinement 1824 ms
- now: hard 244, crossings 1269, bends 1375, length 633336, refinement 1915 ms

Against cba91fe: crossings −9 % (layerweave-code 1031 → 900, deployment-cluster 114 → 107), bends −4 %, hard violations
unchanged, length +11 %. petri-net (56 → 62) and network-vlan (59 → 62) gain crossings. Also measured: the port slots
estimated from each node's edge count (all edges) gave layerweave-code 943, and only edges between siblings 981; the
ports a node actually has give 900.

## Edits (five add edits, as in `layered-straight-20261101.md`; cba91fe / now)

| dataset | crossings | overlapping segment pairs | edges repaired by search | ms for 5 edits | import of the final graph: crossings / overlaps |
|---|---|---|---|---|---|
| petri-net | 107 / 124 | 7 / 0 | 7 / 3 | 42 / 48 | 109 / 0 |
| network-vlan | 92 / 101 | 2 / 2 | 21 / 12 | 68 / 54 | 97 / 0 |
| layerweave-pipeline | 38 / 37 | 2 / 7 | 84 / 89 | 46 / 46 | 72 / 1 |
| cloud-architecture | 28 / 28 | 0 / 0 | 0 / 0 | 51 / 48 | 30 / 0 |
| deployment-cluster | 181 / 141 | 11 / 2 | 71 / 15 | 102 / 83 | 143 / 2 |
| layerweave-code | 1119 / 1068 | 121 / 4 | 261 / 23 | 1735 / 734 | 995 / 1 |

With room for the sections, an edit builds almost every affected edge from them: on layerweave-code 23 instead of 261
edges go to the search, overlapping routes fall from 121 to 4, and five edits take 734 instead of 1735 ms (about 150 ms
each; the import takes about 375 ms). petri-net and network-vlan end with more crossings than before (124 and 101; their
imports have 109 and 97).
