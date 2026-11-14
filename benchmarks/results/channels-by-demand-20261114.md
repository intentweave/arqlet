# Channels as wide as their tracks (`channelsByDemand`, experimental), 2026-11-14

Apple M3, macOS arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from the branch `area-analysis` (main at
346171e plus this change). Commands: `node benchmarks/eval/area-breakdown.mjs --options '{"channelsByDemand":true}'
[--json]` (raw rows: `channels-by-demand-20261114.jsonl`) on the 15 layered datasets with every area in the view's default
direction, against the default and elk-rs; and a check of every dataset (32) on the default's own views with
per-container directions. Single runs; the import times did not change measurably.

## The 15 layered views, one direction

| dataset | engine | crossings | bends | width × height | area (k) | fill | along gap | across gap | ranks | rank gap | rank pitch | widest rank | containers |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| activity-order | arqlet | 14 | 82 | 1746 × 1080 | 1886 | 14 % | 65 % | 10 % | 7 | 105 | 80 | 7 | 21 % |
| activity-order | arqlet {"channelsByDemand":true} | 14 | 82 | 1746 × 982 | 1715 | 15 % | 62 % | 10 % | 7 | 89 | 80 | 7 | 23 % |
| activity-order | elk-rs | 29 | 90 | 1636 × 992 | 1623 | 16 % | 51 % | 7 % | 9 | 60 | 31 | 7 | 7 % |
| arqlet-pipeline | arqlet | 5 | 64 | 1725 × 2328 | 4016 | 6 % | 65 % | 6 % | 15 | 103 | 167 | 5 | 11 % |
| arqlet-pipeline | arqlet {"channelsByDemand":true} | 5 | 64 | 1725 × 1936 | 3340 | 8 % | 58 % | 6 % | 15 | 75 | 167 | 5 | 13 % |
| arqlet-pipeline | elk-rs | 5 | 56 | 1404 × 1394 | 1957 | 13 % | 42 % | 5 % | 15 | 40 | 41 | 7 | 5 % |
| bpmn-pools | arqlet | 1 | 30 | 2924 × 519 | 1518 | 13 % | 32 % | 24 % | 11 | 87 | 78 | 3 | 36 % |
| bpmn-pools | arqlet {"channelsByDemand":true} | 1 | 30 | 2626 × 519 | 1363 | 15 % | 25 % | 24 % | 11 | 57 | 78 | 3 | 37 % |
| bpmn-pools | elk-rs | 3 | 32 | 2576 × 331 | 853 | 24 % | 16 % | 20 % | 12 | 32 | 31 | 3 | 25 % |
| cloud-architecture | arqlet | 13 | 98 | 3886 × 1170 | 4547 | 4 % | 40 % | 35 % | 13 | 127 | 211 | 4 | 32 % |
| cloud-architecture | arqlet {"channelsByDemand":true} | 13 | 98 | 3632 × 1170 | 4249 | 5 % | 36 % | 35 % | 13 | 106 | 211 | 4 | 33 % |
| cloud-architecture | elk-rs | 15 | 106 | 3101 × 608 | 1884 | 10 % | 25 % | 31 % | 13 | 62 | 126 | 4 | 26 % |
| deployment-cluster | arqlet | 82 | 212 | 1769 × 2408 | 4260 | 5 % | 65 % | 16 % | 16 | 104 | 51 | 3 | 30 % |
| deployment-cluster | arqlet {"channelsByDemand":true} | 82 | 212 | 1769 × 2248 | 3977 | 6 % | 63 % | 16 % | 16 | 94 | 51 | 3 | 31 % |
| deployment-cluster | elk-rs | 68 | 162 | 1348 × 1730 | 2332 | 10 % | 55 % | 3 % | 14 | 74 | 64 | 5 | 17 % |
| er-diagram | arqlet | 0 | 15 | 1218 × 1502 | 1829 | 17 % | 41 % | 37 % | 4 | 150 | 86 | 13 | 21 % |
| er-diagram | arqlet {"channelsByDemand":true} | 0 | 15 | 1146 × 1502 | 1721 | 18 % | 37 % | 37 % | 4 | 126 | 86 | 13 | 23 % |
| er-diagram | elk-rs | 1 | 14 | 886 × 1540 | 1364 | 23 % | 19 % | 20 % | 4 | 47 | 41 | 14 | 8 % |
| layerweave-code | arqlet | 655 | 1127 | 8016 × 10460 | 83849 | 2 % | 43 % | 37 % | 25 | 141 | 457 | 25 | 10 % |
| layerweave-code | arqlet {"channelsByDemand":true} | 655 | 1127 | 7778 × 10460 | 81360 | 2 % | 40 % | 37 % | 25 | 127 | 457 | 25 | 10 % |
| layerweave-code | elk-rs | 726 | 1234 | 7429 × 5956 | 44244 | 3 % | 41 % | 26 % | 26 | 119 | 280 | 20 | 8 % |
| network-vlan | arqlet | 23 | 110 | 3864 × 1224 | 4730 | 7 % | 65 % | 12 % | 8 | 113 | 332 | 16 | 25 % |
| network-vlan | arqlet {"channelsByDemand":true} | 23 | 110 | 3864 × 1120 | 4328 | 7 % | 60 % | 12 % | 8 | 96 | 332 | 16 | 27 % |
| network-vlan | elk-rs | 26 | 94 | 3392 × 939 | 3185 | 10 % | 47 % | 6 % | 9 | 55 | 154 | 14 | 8 % |
| org-chart | arqlet | 21 | 172 | 5820 × 3748 | 21813 | 3 % | 68 % | 8 % | 23 | 108 | 91 | 12 | 17 % |
| org-chart | arqlet {"channelsByDemand":true} | 21 | 172 | 5820 × 3418 | 19893 | 3 % | 64 % | 8 % | 23 | 92 | 91 | 12 | 18 % |
| org-chart | elk-rs | 22 | 148 | 5660 × 1846 | 10448 | 7 % | 51 % | 7 % | 16 | 61 | 186 | 16 | 9 % |
| petri-net | arqlet | 32 | 84 | 1888 × 801 | 1512 | 15 % | 24 % | 14 % | 8 | 64 | 154 | 7 | 0 % |
| petri-net | arqlet {"channelsByDemand":true} | 32 | 84 | 1734 × 801 | 1389 | 17 % | 17 % | 14 % | 8 | 42 | 154 | 7 | 0 % |
| petri-net | elk-rs | 58 | 66 | 940 × 691 | 650 | 36 % | 23 % | 12 % | 4 | 73 | 65 | 9 | 0 % |
| state-order | arqlet | 0 | 19 | 1974 × 225 | 444 | 22 % | 27 % | 21 % | 8 | 76 | 28 | 2 | 11 % |
| state-order | arqlet {"channelsByDemand":true} | 0 | 19 | 1772 × 225 | 399 | 24 % | 19 % | 21 % | 8 | 47 | 28 | 2 | 12 % |
| state-order | elk-rs | 1 | 16 | 1629 × 170 | 277 | 35 % | 12 % | 7 % | 8 | 27 | 25 | 2 | 5 % |
| state-tcp | arqlet | 0 | 26 | 1888 × 210 | 396 | 27 % | 24 % | 9 % | 8 | 64 | 38 | 3 | 0 % |
| state-tcp | arqlet {"channelsByDemand":true} | 0 | 26 | 1698 × 210 | 357 | 30 % | 15 % | 9 % | 8 | 37 | 38 | 3 | 0 % |
| state-tcp | elk-rs | 0 | 30 | 1640 × 189 | 310 | 34 % | 12 % | 6 % | 8 | 29 | 39 | 2 | 0 % |
| synthetic-2k | arqlet | 1686 | 1358 | 15618 × 6232 | 97327 | 2 % | 39 % | 38 % | 52 | 116 | 316 | 12 | 12 % |
| synthetic-2k | arqlet {"channelsByDemand":true} | 1686 | 1358 | 15082 × 6232 | 93987 | 2 % | 36 % | 38 % | 52 | 104 | 316 | 12 | 12 % |
| synthetic-2k | elk-rs | 3416 | 1558 | 15816 × 4595 | 72677 | 2 % | 38 % | 23 % | 53 | 115 | 166 | 7 | 13 % |
| uml-class | arqlet | 0 | 37 | 1730 × 1676 | 2899 | 8 % | 77 % | 10 % | 7 | 192 | 185 | 5 | 31 % |
| uml-class | arqlet {"channelsByDemand":true} | 0 | 37 | 1730 × 1520 | 2629 | 9 % | 74 % | 10 % | 7 | 163 | 185 | 5 | 33 % |
| uml-class | elk-rs | 2 | 38 | 1610 × 626 | 1008 | 22 % | 45 % | 9 % | 6 | 47 | 72 | 7 | 21 % |
| workflow-dag | arqlet | 198 | 300 | 2760 × 1098 | 3030 | 21 % | 48 % | 11 % | 8 | 182 | 24 | 10 | 23 % |
| workflow-dag | arqlet {"channelsByDemand":true} | 198 | 300 | 2756 × 1098 | 3026 | 21 % | 48 % | 11 % | 8 | 181 | 24 | 10 | 23 % |
| workflow-dag | elk-rs | 249 | 434 | 3552 × 1028 | 3650 | 17 % | 59 % | 4 % | 8 | 298 | 20 | 10 | 17 % |

Sums (lines through nodes / crossings / bends / area in millions): default 0 / 2730 / 3734 / 234.1, `channelsByDemand`
0 / 2730 / 3734 / 223.7, elk-rs 0 / 4621 / 4078 / 146.5. The same crossings and bends on every view, 3 to 17 % less area
(4.4 % in sum).

## Every dataset, the default's views

18 identical (every organic, tree and mixed dataset, and the views whose areas choose their direction), 14 with the same
lines through nodes, crossings and bends and less area; none better or worse by the quality key; sums of crossings and
bends unchanged.

## How it works, and what the first versions got wrong

- With `layerSpacing` 48 or less the area hardly changed and the bends rose by half on workflow-dag: the lanes are planned
  and the border nodes laid on them on the positions the layer gap gives, and a smaller gap laid border nodes off their
  ports' lines (workflow-dag e32: 755 against 770), while the channels widened for their tracks came back to almost the
  same width.
- `channelsByDemand` plans on the layer gap as before and then sets each channel to what its tracks need: the tracks
  counted, a track more for every self-loop of a node beside it (the loops are built later and pass the channels on
  both sides), one track in reserve when the channel has tracks (several edges changing lanes towards one node needed a
  lane more than counted), at least the margin on both sides and a track, and never wider than without the option.
- It narrows only where the area's flow is fixed, its parent's flow runs along the same axis and every area above has a
  fixed flow. Each restriction came from a measured regression: inside organic roots the shrinking containers overlapped
  (mixed-large: 16 node overlaps, lines through nodes); a container across its parent's axis changed its extent across the
  parent's flow and the parent was placed anew (state-order: a crossing more; workflow-dag: bends); and an area that
  chooses its direction compares both by its children's extents, so the choice turned (synthetic-2k: 17 crossings and 24
  bends more).
- Container margins (header 36, padding 12 each side, a border rank 36 per side) and the gaps within ranks are untouched:
  the next levers. The base gaps stay configurable as before (`layerSpacing`, default 64; `nodeSpacing`, default 24).
