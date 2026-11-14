# Right to left under single pass (`singlePassReversed`, experimental), 2026-11-14

Apple M3, Darwin 25.0.0 arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` from the branch
`single-pass-regions` (ADR 0020, step S1-0). Command: `node benchmarks/eval/reversed-flows.mjs [--json]` (raw rows:
`reversed-flows-20261114.jsonl`). The layered playground examples up to 200 nodes, every area set to one direction;
single runs. Metrics: the shared `metrics` operation.

| dataset | variant | crossings | bends | line–node hits | width × height | single pass |
|---|---|---|---|---|---|---|
| activity-order | lr | 14 | 82 | 0 | 1890 × 825 | yes |
| activity-order | rl | 49 | 106 | 0 | 1788 × 594 | no |
| activity-order | rl + singlePassReversed | 14 | 82 | 0 | 1890 × 825 | yes |
| arqlet-pipeline | lr | 5 | 62 | 0 | 4074 × 752 | yes |
| arqlet-pipeline | rl | 28 | 96 | 0 | 3884 × 620 | no |
| arqlet-pipeline | rl + singlePassReversed | 5 | 62 | 0 | 4074 × 752 | yes |
| bpmn-pools | lr | 1 | 30 | 0 | 2924 × 519 | yes |
| bpmn-pools | rl | 10 | 54 | 0 | 2812 × 431 | no |
| bpmn-pools | rl + singlePassReversed | 1 | 30 | 0 | 2924 × 519 | yes |
| cloud-architecture | lr | 13 | 98 | 0 | 3886 × 1170 | yes |
| cloud-architecture | rl | 31 | 126 | 0 | 3542 × 903 | no |
| cloud-architecture | rl + singlePassReversed | 13 | 98 | 0 | 3886 × 1170 | yes |
| deployment-cluster | lr | 82 | 202 | 0 | 3860 × 1326 | yes |
| deployment-cluster | rl | 112 | 244 | 0 | 3520 × 978 | no |
| deployment-cluster | rl + singlePassReversed | 82 | 202 | 0 | 3860 × 1326 | yes |
| er-diagram | lr | 0 | 15 | 0 | 1218 × 1502 | yes |
| er-diagram | rl | 0 | 29 | 0 | 1104 × 1561 | no |
| er-diagram | rl + singlePassReversed | 0 | 15 | 0 | 1218 × 1502 | yes |
| fixed-ports | lr | 0 | 12 | 0 | 912 × 162 | yes |
| fixed-ports | rl | 0 | 18 | 0 | 912 × 162 | no |
| fixed-ports | rl + singlePassReversed | 0 | 18 | 0 | 912 × 162 | no |
| network-vlan | lr | 23 | 110 | 0 | 2160 × 1752 | yes |
| network-vlan | rl | 58 | 140 | 0 | 2082 × 2064 | no |
| network-vlan | rl + singlePassReversed | 23 | 110 | 0 | 2160 × 1752 | yes |
| org-chart | lr | 21 | 170 | 0 | 5608 × 2905 | yes |
| org-chart | rl | 43 | 296 | 0 | 4900 × 3244 | no |
| org-chart | rl + singlePassReversed | 21 | 170 | 0 | 5608 × 2905 | yes |
| petri-net | lr | 32 | 84 | 0 | 1888 × 801 | yes |
| petri-net | rl | 43 | 124 | 0 | 934 × 991 | no |
| petri-net | rl + singlePassReversed | 32 | 84 | 0 | 1888 × 801 | yes |
| state-order | lr | 0 | 19 | 0 | 1974 × 225 | yes |
| state-order | rl | 2 | 25 | 0 | 1936 × 204 | no |
| state-order | rl + singlePassReversed | 0 | 19 | 0 | 1974 × 225 | yes |
| state-tcp | lr | 0 | 26 | 0 | 1888 × 210 | yes |
| state-tcp | rl | 3 | 52 | 0 | 1888 × 246 | no |
| state-tcp | rl + singlePassReversed | 0 | 26 | 0 | 1888 × 210 | yes |
| uml-class | lr | 0 | 39 | 0 | 2342 × 1053 | yes |
| uml-class | rl | 10 | 61 | 0 | 1784 × 1569 | no |
| uml-class | rl + singlePassReversed | 0 | 39 | 0 | 2342 × 1053 | yes |
| workflow-dag | lr | 198 | 300 | 0 | 2760 × 1098 | yes |
| workflow-dag | rl | 358 | 394 | 0 | 2358 × 848 | no |
| workflow-dag | rl + singlePassReversed | 198 | 300 | 0 | 2760 × 1098 | yes |

Sums (crossings / bends / lines through nodes): lr 389 / 1249 / 0, rl 747 / 1765 / 0,
rl + singlePassReversed 389 / 1255 / 0.
The mirrored drawing equals `lr` in crossings, bends and size on 13 of 14 datasets.

Reading:

- `rl` today switches single pass off ("Fixed-order channel growth requires positive flow directions") and crosses and
  bends more than `lr` although it is its mirror image.
- With `singlePassReversed` a graph that flows right to left everywhere is laid out left to right and mirrored: the same
  metrics as `lr`, every edge still from its source to its target, now right to left. The exception is the graph with
  explicit ports (`fixed-ports`), which the option leaves to the earlier pipeline, as it does pins, label places and
  buses.
- `bt` is not mirrored: a container's header is at its top in both senses, so a mirror on the vertical axis moves
  children into it (the geometry check refuses that); it needs a layout of its own.
