# Where the drawing area goes: Arqlet against elk-rs, 2026-11-14

Apple M3, macOS arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from main (346171e), elk-rs
(`@openedges/elk-layout`, pinned in `benchmarks/eval`). Command: `node benchmarks/eval/area-breakdown.mjs [--json]
[--options JSON]` (raw rows, with the variant `{"layerSpacing":32,"nodeSpacing":16}`: `area-breakdown-20261114.jsonl`).
The 15 layered datasets; the view the default selects (150 nodes), every area set to the view's default direction so
both engines solve the same task (elk-rs takes no per-container directions across a hierarchy). The breakdown reads only
the geometry (definitions in the script).

| dataset | engine | crossings | bends | width × height | area (k) | fill | along gap | across gap | ranks | rank gap | rank pitch | widest rank | containers |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| activity-order | arqlet | 14 | 82 | 1746 × 1080 | 1886 | 14 % | 65 % | 10 % | 7 | 105 | 80 | 7 | 21 % |
| activity-order | elk-rs | 29 | 90 | 1636 × 992 | 1623 | 16 % | 51 % | 7 % | 9 | 60 | 31 | 7 | 7 % |
| arqlet-pipeline | arqlet | 5 | 64 | 1725 × 2328 | 4016 | 6 % | 65 % | 6 % | 15 | 103 | 167 | 5 | 11 % |
| arqlet-pipeline | elk-rs | 5 | 56 | 1404 × 1394 | 1957 | 13 % | 42 % | 5 % | 15 | 40 | 41 | 7 | 5 % |
| bpmn-pools | arqlet | 1 | 30 | 2924 × 519 | 1518 | 13 % | 32 % | 24 % | 11 | 87 | 78 | 3 | 36 % |
| bpmn-pools | elk-rs | 3 | 32 | 2576 × 331 | 853 | 24 % | 16 % | 20 % | 12 | 32 | 31 | 3 | 25 % |
| cloud-architecture | arqlet | 13 | 98 | 3886 × 1170 | 4547 | 4 % | 40 % | 35 % | 13 | 127 | 211 | 4 | 32 % |
| cloud-architecture | elk-rs | 15 | 106 | 3101 × 608 | 1884 | 10 % | 25 % | 31 % | 13 | 62 | 126 | 4 | 26 % |
| deployment-cluster | arqlet | 82 | 212 | 1769 × 2408 | 4260 | 5 % | 65 % | 16 % | 16 | 104 | 51 | 3 | 30 % |
| deployment-cluster | elk-rs | 68 | 162 | 1348 × 1730 | 2332 | 10 % | 55 % | 3 % | 14 | 74 | 64 | 5 | 17 % |
| er-diagram | arqlet | 0 | 15 | 1218 × 1502 | 1829 | 17 % | 41 % | 37 % | 4 | 150 | 86 | 13 | 21 % |
| er-diagram | elk-rs | 1 | 14 | 886 × 1540 | 1364 | 23 % | 19 % | 20 % | 4 | 47 | 41 | 14 | 8 % |
| layerweave-code | arqlet | 655 | 1127 | 8016 × 10460 | 83849 | 2 % | 43 % | 37 % | 25 | 141 | 457 | 25 | 10 % |
| layerweave-code | elk-rs | 726 | 1234 | 7429 × 5956 | 44244 | 3 % | 41 % | 26 % | 26 | 119 | 280 | 20 | 8 % |
| network-vlan | arqlet | 23 | 110 | 3864 × 1224 | 4730 | 7 % | 65 % | 12 % | 8 | 113 | 332 | 16 | 25 % |
| network-vlan | elk-rs | 26 | 94 | 3392 × 939 | 3185 | 10 % | 47 % | 6 % | 9 | 55 | 154 | 14 | 8 % |
| org-chart | arqlet | 21 | 172 | 5820 × 3748 | 21813 | 3 % | 68 % | 8 % | 23 | 108 | 91 | 12 | 17 % |
| org-chart | elk-rs | 22 | 148 | 5660 × 1846 | 10448 | 7 % | 51 % | 7 % | 16 | 61 | 186 | 16 | 9 % |
| petri-net | arqlet | 32 | 84 | 1888 × 801 | 1512 | 15 % | 24 % | 14 % | 8 | 64 | 154 | 7 | 0 % |
| petri-net | elk-rs | 58 | 66 | 940 × 691 | 650 | 36 % | 23 % | 12 % | 4 | 73 | 65 | 9 | 0 % |
| state-order | arqlet | 0 | 19 | 1974 × 225 | 444 | 22 % | 27 % | 21 % | 8 | 76 | 28 | 2 | 11 % |
| state-order | elk-rs | 1 | 16 | 1629 × 170 | 277 | 35 % | 12 % | 7 % | 8 | 27 | 25 | 2 | 5 % |
| state-tcp | arqlet | 0 | 26 | 1888 × 210 | 396 | 27 % | 24 % | 9 % | 8 | 64 | 38 | 3 | 0 % |
| state-tcp | elk-rs | 0 | 30 | 1640 × 189 | 310 | 34 % | 12 % | 6 % | 8 | 29 | 39 | 2 | 0 % |
| synthetic-2k | arqlet | 1686 | 1358 | 15618 × 6232 | 97327 | 2 % | 39 % | 38 % | 52 | 116 | 316 | 12 | 12 % |
| synthetic-2k | elk-rs | 3416 | 1558 | 15816 × 4595 | 72677 | 2 % | 38 % | 23 % | 53 | 115 | 166 | 7 | 13 % |
| uml-class | arqlet | 0 | 37 | 1730 × 1676 | 2899 | 8 % | 77 % | 10 % | 7 | 192 | 185 | 5 | 31 % |
| uml-class | elk-rs | 2 | 38 | 1610 × 626 | 1008 | 22 % | 45 % | 9 % | 6 | 47 | 72 | 7 | 21 % |
| workflow-dag | arqlet | 198 | 300 | 2760 × 1098 | 3030 | 21 % | 48 % | 11 % | 8 | 182 | 24 | 10 | 23 % |
| workflow-dag | elk-rs | 249 | 434 | 3552 × 1028 | 3650 | 17 % | 59 % | 4 % | 8 | 298 | 20 | 10 | 17 % |

Reading:

- On the same task Arqlet crosses less than elk-rs on 12 of 15 views (in sum 2730 against 4621; synthetic-2k 1686 against
  3416) and bends about as much (3734 against 4078), and takes 1.6 times the area in sum (234 against 146 million;
  1.2 to 3 times per view, workflow-dag smaller).
- The largest share is the gap between ranks along the flow: a mean free distance of 100 to 190 between neighbouring ranks
  against 30 to 75 for elk-rs; 40 to 77 % of Arqlet's extent along the flow is gap (elk-rs 12 to 55 %). The base gap is
  64 (`layerSpacing`), channels widen for their tracks and labels, and a rank gap across a container border adds its
  margins.
- Second, the gap between neighbours of a rank: often twice elk-rs' (arqlet-pipeline 167 against 41, network-vlan 332
  against 154, synthetic-2k 316 against 166): the base is 24 (`nodeSpacing`), the rest comes from the placement that
  straightens edges and centres ranks.
- Third, container margins and headers: 20 to 36 % of the area against 5 to 25 %.
- Smaller base gaps alone (`layerSpacing` 32, `nodeSpacing` 16): 17 % less area in sum, but 11 % more bends (workflow-dag
  +150, synthetic-2k +170) and one line through a node, so narrower channels force more changes of lane: not a lever by
  itself (area must not cost bends).
