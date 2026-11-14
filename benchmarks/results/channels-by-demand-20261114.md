# Channels as wide as their tracks (`channelsByDemand`, experimental), 2026-11-14

Apple M3, macOS arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from the branch `area-analysis` (main at
346171e plus this change). Command: `node benchmarks/eval/area-breakdown.mjs --options '{"channelsByDemand":true}'
[--json]` (raw rows: `channels-by-demand-20261114.jsonl`). The 15 layered datasets, the default's views with every area in
the view's default direction, against the default and elk-rs on the same views; single runs (the import times did not
change measurably: 3 to 156 ms either way).

| dataset | engine | crossings | bends | width × height | area (k) | fill | along gap | across gap | ranks | rank gap | rank pitch | widest rank | containers |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| activity-order | arqlet | 14 | 82 | 1746 × 1080 | 1886 | 14 % | 65 % | 10 % | 7 | 105 | 80 | 7 | 21 % |
| activity-order | arqlet {"channelsByDemand":true} | 14 | 82 | 1746 × 952 | 1662 | 16 % | 60 % | 10 % | 7 | 84 | 80 | 7 | 24 % |
| activity-order | elk-rs | 29 | 90 | 1636 × 992 | 1623 | 16 % | 51 % | 7 % | 9 | 60 | 31 | 7 | 7 % |
| arqlet-pipeline | arqlet | 5 | 64 | 1725 × 2328 | 4016 | 6 % | 65 % | 6 % | 15 | 103 | 167 | 5 | 11 % |
| arqlet-pipeline | arqlet {"channelsByDemand":true} | 5 | 64 | 1725 × 1882 | 3246 | 8 % | 57 % | 6 % | 15 | 71 | 167 | 5 | 13 % |
| arqlet-pipeline | elk-rs | 5 | 56 | 1404 × 1394 | 1957 | 13 % | 42 % | 5 % | 15 | 40 | 41 | 7 | 5 % |
| bpmn-pools | arqlet | 1 | 30 | 2924 × 519 | 1518 | 13 % | 32 % | 24 % | 11 | 87 | 78 | 3 | 36 % |
| bpmn-pools | arqlet {"channelsByDemand":true} | 1 | 30 | 2584 × 519 | 1341 | 15 % | 23 % | 24 % | 11 | 53 | 78 | 3 | 37 % |
| bpmn-pools | elk-rs | 3 | 32 | 2576 × 331 | 853 | 24 % | 16 % | 20 % | 12 | 32 | 31 | 3 | 25 % |
| cloud-architecture | arqlet | 13 | 98 | 3886 × 1170 | 4547 | 4 % | 40 % | 35 % | 13 | 127 | 211 | 4 | 32 % |
| cloud-architecture | arqlet {"channelsByDemand":true} | 13 | 98 | 3562 × 1170 | 4168 | 5 % | 34 % | 35 % | 13 | 100 | 211 | 4 | 33 % |
| cloud-architecture | elk-rs | 15 | 106 | 3101 × 608 | 1884 | 10 % | 25 % | 31 % | 13 | 62 | 126 | 4 | 26 % |
| deployment-cluster | arqlet | 82 | 212 | 1769 × 2408 | 4260 | 5 % | 65 % | 16 % | 16 | 104 | 51 | 3 | 30 % |
| deployment-cluster | arqlet {"channelsByDemand":true} | 82 | 212 | 1769 × 2214 | 3917 | 6 % | 62 % | 16 % | 16 | 92 | 51 | 3 | 32 % |
| deployment-cluster | elk-rs | 68 | 162 | 1348 × 1730 | 2332 | 10 % | 55 % | 3 % | 14 | 74 | 64 | 5 | 17 % |
| er-diagram | arqlet | 0 | 15 | 1218 × 1502 | 1829 | 17 % | 41 % | 37 % | 4 | 150 | 86 | 13 | 21 % |
| er-diagram | arqlet {"channelsByDemand":true} | 0 | 15 | 1122 × 1502 | 1685 | 18 % | 36 % | 37 % | 4 | 118 | 86 | 13 | 23 % |
| er-diagram | elk-rs | 1 | 14 | 886 × 1540 | 1364 | 23 % | 19 % | 20 % | 4 | 47 | 41 | 14 | 8 % |
| layerweave-code | arqlet | 655 | 1127 | 8016 × 10460 | 83849 | 2 % | 43 % | 37 % | 25 | 141 | 457 | 25 | 10 % |
| layerweave-code | arqlet {"channelsByDemand":true} | 658 | 1129 | 7648 × 10460 | 80000 | 2 % | 40 % | 37 % | 25 | 122 | 457 | 25 | 10 % |
| layerweave-code | elk-rs | 726 | 1234 | 7429 × 5956 | 44244 | 3 % | 41 % | 26 % | 26 | 119 | 280 | 20 | 8 % |
| network-vlan | arqlet | 23 | 110 | 3864 × 1224 | 4730 | 7 % | 65 % | 12 % | 8 | 113 | 332 | 16 | 25 % |
| network-vlan | arqlet {"channelsByDemand":true} | 23 | 110 | 3864 × 1096 | 4235 | 7 % | 59 % | 12 % | 8 | 92 | 332 | 16 | 27 % |
| network-vlan | elk-rs | 26 | 94 | 3392 × 939 | 3185 | 10 % | 47 % | 6 % | 9 | 55 | 154 | 14 | 8 % |
| org-chart | arqlet | 21 | 172 | 5820 × 3748 | 21813 | 3 % | 68 % | 8 % | 23 | 108 | 91 | 12 | 17 % |
| org-chart | arqlet {"channelsByDemand":true} | 21 | 172 | 5820 × 3320 | 19322 | 4 % | 63 % | 8 % | 23 | 88 | 91 | 12 | 19 % |
| org-chart | elk-rs | 22 | 148 | 5660 × 1846 | 10448 | 7 % | 51 % | 7 % | 16 | 61 | 186 | 16 | 9 % |
| petri-net | arqlet | 32 | 84 | 1888 × 801 | 1512 | 15 % | 24 % | 14 % | 8 | 64 | 154 | 7 | 0 % |
| petri-net | arqlet {"channelsByDemand":true} | 32 | 84 | 1698 × 801 | 1360 | 17 % | 15 % | 14 % | 8 | 37 | 154 | 7 | 0 % |
| petri-net | elk-rs | 58 | 66 | 940 × 691 | 650 | 36 % | 23 % | 12 % | 4 | 73 | 65 | 9 | 0 % |
| state-order | arqlet | 0 | 19 | 1974 × 225 | 444 | 22 % | 27 % | 21 % | 8 | 76 | 28 | 2 | 11 % |
| state-order | arqlet {"channelsByDemand":true} | 0 | 19 | 1742 × 225 | 392 | 25 % | 17 % | 21 % | 8 | 43 | 28 | 2 | 12 % |
| state-order | elk-rs | 1 | 16 | 1629 × 170 | 277 | 35 % | 12 % | 7 % | 8 | 27 | 25 | 2 | 5 % |
| state-tcp | arqlet | 0 | 26 | 1888 × 210 | 396 | 27 % | 24 % | 9 % | 8 | 64 | 38 | 3 | 0 % |
| state-tcp | arqlet {"channelsByDemand":true} | 0 | 26 | 1656 × 210 | 348 | 31 % | 13 % | 9 % | 8 | 31 | 38 | 3 | 0 % |
| state-tcp | elk-rs | 0 | 30 | 1640 × 189 | 310 | 34 % | 12 % | 6 % | 8 | 29 | 39 | 2 | 0 % |
| synthetic-2k | arqlet | 1686 | 1358 | 15618 × 6232 | 97327 | 2 % | 39 % | 38 % | 52 | 116 | 316 | 12 | 12 % |
| synthetic-2k | arqlet {"channelsByDemand":true} | 1686 | 1358 | 14840 × 6232 | 92479 | 2 % | 35 % | 38 % | 52 | 100 | 316 | 12 | 12 % |
| synthetic-2k | elk-rs | 3416 | 1558 | 15816 × 4595 | 72677 | 2 % | 38 % | 23 % | 53 | 115 | 166 | 7 | 13 % |
| uml-class | arqlet | 0 | 37 | 1730 × 1676 | 2899 | 8 % | 77 % | 10 % | 7 | 192 | 185 | 5 | 31 % |
| uml-class | arqlet {"channelsByDemand":true} | 0 | 37 | 1730 × 1490 | 2577 | 9 % | 74 % | 10 % | 7 | 160 | 185 | 5 | 34 % |
| uml-class | elk-rs | 2 | 38 | 1610 × 626 | 1008 | 22 % | 45 % | 9 % | 6 | 47 | 72 | 7 | 21 % |
| workflow-dag | arqlet | 198 | 300 | 2760 × 1098 | 3030 | 21 % | 48 % | 11 % | 8 | 182 | 24 | 10 | 23 % |
| workflow-dag | arqlet {"channelsByDemand":true} | 198 | 300 | 2750 × 1098 | 3020 | 21 % | 48 % | 11 % | 8 | 180 | 24 | 10 | 23 % |
| workflow-dag | elk-rs | 249 | 434 | 3552 × 1028 | 3650 | 17 % | 59 % | 4 % | 8 | 298 | 20 | 10 | 17 % |

Sums (lines through nodes / crossings / bends / area in millions): default 0 / 2730 / 3734 / 234.1, `channelsByDemand`
0 / 2733 / 3736 / 219.8, elk-rs 0 / 4621 / 4078 / 146.5.

Reading:

- The area drops by 6 % in sum and by up to 19 % per view (arqlet-pipeline 4016 → 3246 thousand, org-chart and uml-class
  11 %), with the same crossings and bends on 14 of 15 views (layerweave-code: 3 crossings and 2 bends more), no line
  through a node, no geometry issue and no route conflict on any view.
- Why it works where a smaller layer gap did not: with `layerSpacing` 48 (or less) the area hardly changed and the bends
  rose by half on workflow-dag. The channels are widened for their tracks after the lanes are planned and the border
  nodes laid on them, and that planning reads the positions along the flow the layer gap gives; with a smaller gap it laid
  border nodes off their ports' lines (workflow-dag e32: 755 against 770), each costing a change of lane, while the widened
  channels came back to almost the same width. `channelsByDemand` plans on the layer gap as before and only then sets each
  channel to what its tracks need (at least the margin on both sides and a track, as a border rank keeps).
- Container margins (header 36, padding 12 each side, a border rank 36 per side) and the gaps within ranks are untouched:
  the next levers.
