# Reversed flows under single pass (`singlePassReversed`, experimental), 2026-11-14

Apple M3, Darwin 25.0.0 arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from the branch
`single-pass-regions` (ADR 0020, steps S1-0 and S1-0b). Command: `node benchmarks/eval/reversed-flows.mjs [--json]` (raw
rows: `reversed-flows-20261114.jsonl`). The layered playground examples up to 200 nodes, every area set to one direction;
single runs. Metrics: the shared `metrics` operation.

| dataset | variant | crossings | bends | line–node hits | width × height | single pass |
|---|---|---|---|---|---|---|
| activity-order | lr | 14 | 82 | 0 | 1890 × 825 | yes |
| activity-order | rl | 49 | 106 | 0 | 1788 × 594 | no |
| activity-order | rl + singlePassReversed | 14 | 82 | 0 | 1890 × 825 | yes |
| activity-order | tb | 14 | 82 | 0 | 1746 × 1080 | yes |
| activity-order | bt | 45 | 88 | 0 | 1452 × 978 | no |
| activity-order | bt + singlePassReversed | 14 | 82 | 0 | 1746 × 1080 | yes |
| arqlet-pipeline | lr | 5 | 62 | 0 | 4074 × 752 | yes |
| arqlet-pipeline | rl | 28 | 96 | 0 | 3884 × 620 | no |
| arqlet-pipeline | rl + singlePassReversed | 5 | 62 | 0 | 4074 × 752 | yes |
| arqlet-pipeline | tb | 5 | 64 | 0 | 1725 × 2328 | yes |
| arqlet-pipeline | bt | 22 | 82 | 0 | 1444 × 2138 | no |
| arqlet-pipeline | bt + singlePassReversed | 5 | 64 | 0 | 1725 × 2328 | yes |
| bpmn-pools | lr | 1 | 30 | 0 | 2924 × 519 | yes |
| bpmn-pools | rl | 10 | 54 | 0 | 2812 × 431 | no |
| bpmn-pools | rl + singlePassReversed | 1 | 30 | 0 | 2924 × 519 | yes |
| bpmn-pools | tb | 1 | 30 | 0 | 1038 × 1634 | yes |
| bpmn-pools | bt | 9 | 70 | 0 | 857 × 1522 | no |
| bpmn-pools | bt + singlePassReversed | 1 | 30 | 0 | 1038 × 1634 | yes |
| cloud-architecture | lr | 13 | 98 | 0 | 3886 × 1170 | yes |
| cloud-architecture | rl | 31 | 126 | 0 | 3542 × 903 | no |
| cloud-architecture | rl + singlePassReversed | 13 | 98 | 0 | 3886 × 1170 | yes |
| cloud-architecture | tb | 13 | 96 | 0 | 1734 × 2464 | yes |
| cloud-architecture | bt | 30 | 164 | 0 | 1215 × 2120 | no |
| cloud-architecture | bt + singlePassReversed | 13 | 96 | 0 | 1734 × 2464 | yes |
| deployment-cluster | lr | 82 | 202 | 0 | 3860 × 1326 | yes |
| deployment-cluster | rl | 112 | 244 | 0 | 3520 × 978 | no |
| deployment-cluster | rl + singlePassReversed | 82 | 202 | 0 | 3860 × 1326 | yes |
| deployment-cluster | tb | 82 | 212 | 0 | 1769 × 2408 | yes |
| deployment-cluster | bt | 91 | 264 | 0 | 1829 × 2082 | no |
| deployment-cluster | bt + singlePassReversed | 82 | 212 | 0 | 1769 × 2408 | yes |
| er-diagram | lr | 0 | 15 | 0 | 1218 × 1502 | yes |
| er-diagram | rl | 0 | 29 | 0 | 1104 × 1561 | no |
| er-diagram | rl + singlePassReversed | 0 | 15 | 0 | 1218 × 1502 | yes |
| er-diagram | tb | 0 | 15 | 0 | 3209 × 810 | yes |
| er-diagram | bt | 0 | 27 | 0 | 3334 × 696 | no |
| er-diagram | bt + singlePassReversed | 0 | 15 | 0 | 3209 × 810 | yes |
| fixed-ports | lr | 0 | 12 | 0 | 912 × 162 | yes |
| fixed-ports | rl | 0 | 18 | 0 | 912 × 162 | no |
| fixed-ports | rl + singlePassReversed | 0 | 18 | 0 | 912 × 162 | no |
| fixed-ports | tb | 1 | 14 | 0 | 414 × 408 | yes |
| fixed-ports | bt | 0 | 24 | 0 | 414 × 408 | no |
| fixed-ports | bt + singlePassReversed | 0 | 24 | 0 | 414 × 408 | no |
| network-vlan | lr | 23 | 110 | 0 | 2160 × 1752 | yes |
| network-vlan | rl | 58 | 140 | 0 | 2082 × 2064 | no |
| network-vlan | rl + singlePassReversed | 23 | 110 | 0 | 2160 × 1752 | yes |
| network-vlan | tb | 23 | 110 | 0 | 3864 × 1224 | yes |
| network-vlan | bt | 46 | 150 | 0 | 4740 × 1146 | no |
| network-vlan | bt + singlePassReversed | 23 | 110 | 0 | 3864 × 1224 | yes |
| org-chart | lr | 21 | 170 | 0 | 5608 × 2905 | yes |
| org-chart | rl | 43 | 296 | 0 | 4900 × 3244 | no |
| org-chart | rl + singlePassReversed | 21 | 170 | 0 | 5608 × 2905 | yes |
| org-chart | tb | 21 | 172 | 0 | 5820 × 3748 | yes |
| org-chart | bt | 38 | 318 | 0 | 6564 × 3166 | no |
| org-chart | bt + singlePassReversed | 21 | 172 | 0 | 5820 × 3748 | yes |
| petri-net | lr | 32 | 84 | 0 | 1888 × 801 | yes |
| petri-net | rl | 43 | 124 | 0 | 934 × 991 | no |
| petri-net | rl + singlePassReversed | 32 | 84 | 0 | 1888 × 801 | yes |
| petri-net | tb | 32 | 84 | 0 | 1778 × 880 | yes |
| petri-net | bt | 39 | 142 | 0 | 2291 × 430 | no |
| petri-net | bt + singlePassReversed | 32 | 84 | 0 | 1778 × 880 | yes |
| state-order | lr | 0 | 19 | 0 | 1974 × 225 | yes |
| state-order | rl | 2 | 25 | 0 | 1936 × 204 | no |
| state-order | rl + singlePassReversed | 0 | 19 | 0 | 1974 × 225 | yes |
| state-order | tb | 0 | 19 | 0 | 500 × 990 | yes |
| state-order | bt | 2 | 27 | 0 | 466 × 952 | no |
| state-order | bt + singlePassReversed | 0 | 19 | 0 | 500 × 990 | yes |
| state-tcp | lr | 0 | 26 | 0 | 1888 × 210 | yes |
| state-tcp | rl | 3 | 52 | 0 | 1888 × 246 | no |
| state-tcp | rl + singlePassReversed | 0 | 26 | 0 | 1888 × 210 | yes |
| state-tcp | tb | 0 | 26 | 0 | 588 × 880 | yes |
| state-tcp | bt | 1 | 72 | 0 | 643 × 880 | no |
| state-tcp | bt + singlePassReversed | 0 | 26 | 0 | 588 × 880 | yes |
| uml-class | lr | 0 | 39 | 0 | 2342 × 1053 | yes |
| uml-class | rl | 10 | 61 | 0 | 1784 × 1569 | no |
| uml-class | rl + singlePassReversed | 0 | 39 | 0 | 2342 × 1053 | yes |
| uml-class | tb | 0 | 37 | 0 | 1730 × 1676 | yes |
| uml-class | bt | 7 | 65 | 0 | 2681 × 1220 | no |
| uml-class | bt + singlePassReversed | 0 | 37 | 0 | 1730 × 1676 | yes |
| workflow-dag | lr | 198 | 300 | 0 | 2760 × 1098 | yes |
| workflow-dag | rl | 358 | 394 | 0 | 2358 × 848 | no |
| workflow-dag | rl + singlePassReversed | 198 | 300 | 0 | 2760 × 1098 | yes |
| workflow-dag | tb | 200 | 304 | 0 | 2886 × 1950 | yes |
| workflow-dag | bt | 289 | 430 | 0 | 2106 × 1538 | no |
| workflow-dag | bt + singlePassReversed | 200 | 304 | 0 | 2886 × 1950 | yes |

Sums (crossings / bends / lines through nodes): lr 389 / 1249 / 0, rl 747 / 1765 / 0, rl + singlePassReversed 389 / 1255 / 0;
tb 392 / 1265 / 0, bt 619 / 1923 / 0, bt + singlePassReversed 391 / 1275 / 0.
The mirrored drawing equals the positive one in crossings, bends and size on 13 of 14 datasets
for `rl` and on 13 of 14 for `bt`.

Reading:

- `rl` and `bt` today switch single pass off ("Fixed-order channel growth requires positive flow directions") and cross
  and bend more than `lr` and `tb` although they are their mirror images.
- With `singlePassReversed` a graph that flows against its axis everywhere is laid out with the positive flow and
  mirrored: the same metrics as `lr` / `tb`, every edge still from its source to its target. The exception is the graph
  with explicit ports (`fixed-ports`), which the option leaves to the earlier pipeline, as it does pins, label places and
  buses.
- `bt` (S1-0b): a container keeps its header at its top in both senses, so every area is mirrored within its own box and
  its content moved back by the difference of its two margins; a horizontal segment that passes a container's side
  changes its height there. The geometry is valid and every container's children start below its header, as top to
  bottom (tests with nested containers).
