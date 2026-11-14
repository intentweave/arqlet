# Where the extent across and along the flow goes, 2026-11-14

Apple M3, macOS arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from the branch `area-analysis` with
`channelsByDemand` on (the default). Commands: `node benchmarks/eval/rank-breakdown.mjs` and `node
benchmarks/eval/along-breakdown.mjs`. The 15 layered datasets, the default's views with every area in the view's default
direction; read from the stored layered plans (ranks with their long-edge dummies) and the boxes. Nested areas count in
each area, so the shares describe the areas, not one drawing. Positions are not snapped to a grid (`placementGrid` is 0).

## Across the flow (per area: its extent against the tightest packing of its widest rank)

| dataset | areas | extent across | nodes | dummies | gaps node–node | gaps node–dummy | gaps dummy–dummy | alignment slack |
|---|---|---|---|---|---|---|---|---|
| activity-order | 5 | 5105 | 4266 (84 %) | 48 (1 %) | 120 (2 %) | 312 (6 %) | 0 (0 %) | 359 (7 %) |
| arqlet-pipeline | 7 | 4926 | 3759 (76 %) | 78 (2 %) | 144 (3 %) | 168 (3 %) | 30 (1 %) | 747 (15 %) |
| bpmn-pools | 6 | 1668 | 1209 (72 %) | 42 (3 %) | 72 (4 %) | 168 (10 %) | 6 (0 %) | 171 (10 %) |
| cloud-architecture | 15 | 4586 | 4059 (89 %) | 78 (2 %) | 24 (1 %) | 264 (6 %) | 24 (1 %) | 137 (3 %) |
| deployment-cluster | 10 | 8131 | 6684 (82 %) | 378 (5 %) | 144 (2 %) | 120 (1 %) | 120 (1 %) | 685 (8 %) |
| er-diagram | 13 | 3710 | 2982 (80 %) | 6 (0 %) | 552 (15 %) | 48 (1 %) | 0 (0 %) | 122 (3 %) |
| layerweave-code | 52 | 36120 | 20462 (57 %) | 6089 (17 %) | 1344 (4 %) | 1224 (3 %) | 1476 (4 %) | 5525 (15 %) |
| network-vlan | 6 | 12126 | 11214 (92 %) | 42 (0 %) | 432 (4 %) | 144 (1 %) | 18 (0 %) | 276 (2 %) |
| org-chart | 34 | 38762 | 33123 (85 %) | 1278 (3 %) | 1128 (3 %) | 168 (0 %) | 18 (0 %) | 3047 (8 %) |
| petri-net | 1 | 801 | 378 (47 %) | 0 (0 %) | 144 (18 %) | 0 (0 %) | 0 (0 %) | 279 (35 %) |
| state-order | 2 | 366 | 312 (85 %) | 6 (2 %) | 24 (7 %) | 24 (7 %) | 0 (0 %) | 0 (0 %) |
| state-tcp | 1 | 269 | 162 (60 %) | 6 (2 %) | 48 (18 %) | 24 (9 %) | 0 (0 %) | 29 (11 %) |
| synthetic-2k | 82 | 36591 | 25759 (70 %) | 2034 (6 %) | 1296 (4 %) | 1272 (3 %) | 666 (2 %) | 5564 (15 %) |
| uml-class | 21 | 8385 | 7874 (94 %) | 6 (0 %) | 264 (3 %) | 48 (1 %) | 0 (0 %) | 194 (2 %) |
| workflow-dag | 9 | 6038 | 4284 (71 %) | 84 (1 %) | 1344 (22 %) | 24 (0 %) | 6 (0 %) | 296 (5 %) |
| **sum** | | 167581 | 126527 (76 %) | 10175 (6 %) | 7080 (4 %) | 4008 (2 %) | 2364 (1 %) | 17428 (10 %) |

The block placement (Brandes and Köpf, as in ELK) keeps a node gap of 24 (`nodeSpacing`) between two nodes and between a
node and a long-edge dummy, and a track (6) between two dummies; ELK keeps 20 and 10. The gaps are 7 % of the extent, the
alignment slack (blocks that keep edges straight, not packed tight) 10 %, the dummies 6 %; the rest is the nodes and the
nested containers themselves.

## Along the flow (per area)

| dataset | length along | rank bands | channels between ranks | border ranks and their gaps | margins (header, padding, ends) |
|---|---|---|---|---|---|
| activity-order | 2050 | 1336 (65 %) | 336 (16 %) | 192 (9 %) | 186 (9 %) |
| arqlet-pipeline | 3686 | 2560 (69 %) | 504 (14 %) | 360 (10 %) | 262 (7 %) |
| bpmn-pools | 7686 | 6924 (90 %) | 408 (5 %) | 216 (3 %) | 138 (2 %) |
| cloud-architecture | 15240 | 13450 (88 %) | 586 (4 %) | 906 (6 %) | 298 (2 %) |
| deployment-cluster | 6780 | 5124 (76 %) | 682 (10 %) | 564 (8 %) | 410 (6 %) |
| er-diagram | 4216 | 3186 (76 %) | 120 (3 %) | 504 (12 %) | 406 (10 %) |
| layerweave-code | 34566 | 28524 (83 %) | 2746 (8 %) | 2412 (7 %) | 884 (3 %) |
| network-vlan | 3144 | 2142 (68 %) | 432 (14 %) | 312 (10 %) | 258 (8 %) |
| org-chart | 13072 | 9124 (70 %) | 968 (7 %) | 1692 (13 %) | 1288 (10 %) |
| petri-net | 1734 | 1440 (83 %) | 294 (17 %) | 0 (0 %) | 0 (0 %) |
| state-order | 2260 | 1928 (85 %) | 246 (11 %) | 72 (3 %) | 14 (1 %) |
| state-tcp | 1698 | 1440 (85 %) | 258 (15 %) | 0 (0 %) | 0 (0 %) |
| synthetic-2k | 73326 | 63286 (86 %) | 3902 (5 %) | 4392 (6 %) | 1746 (2 %) |
| uml-class | 6440 | 4126 (64 %) | 336 (5 %) | 1116 (17 %) | 862 (13 %) |
| workflow-dag | 4906 | 3590 (73 %) | 606 (12 %) | 564 (11 %) | 146 (3 %) |
| **sum** | 180804 | 148180 (82 %) | 12424 (7 %) | 13302 (7 %) | 6898 (4 %) |

## Reading and levers

- The gaps within ranks are a small share. `nodeSpacing` 16 is the cheap lever (10 % less area in sum at 24 fewer
  crossings and 6 more bends, `docs/configuration.md`); a separate smaller gap between a node and a long-edge dummy
  (margin and a track) saved 2 % at 12 more bends alone and nothing on top of `nodeSpacing` 16 (a minimum of margin and
  track holds there), so it was not kept.
- The alignment slack keeps edges straight; packing it tighter trades bends for area, as ELK's averaging of its four
  alignments does too.
- Along the flow, after `channelsByDemand`, channels and ranks of border nodes are about 7 % each and margins 4 %; the
  remaining difference to elk-rs is structural: every container takes a header (36) and padding, a rank of border nodes
  per side where edges cross it (36), and a rank band as long as its longest member.
