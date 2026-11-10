# Where the layered pipeline stands, 2026-11-03

Machine: Apple M3, Node v24.21.0, Layerweave release build of d7d3308 (`layout-command`); elk-rs
`@openedges/elk-layout` 0.12.1, elkjs pinned in `benchmarks/eval`. `node compare.mjs --uniform-direction --repeat 3 --limit 150`:
every engine lays out the same view, every container left to right and layered; one metric implementation (crates/scene)
scores all drawings. Times are medians of 3 runs; Layerweave's include import and refinement through `layout-command`
(process start and JSON included). Load average 2 to 6. Raw rows: `layered-pipeline-standing-20261103.jsonl`.

| engine | crossings | bends | line-node hits | length | time (sum, median of 3) |
|---|---|---|---|---|---|
| Layerweave default | 3117 | 2816 | 0 | 762k | 42.8 s |
| Layerweave layered | 2766 | 1922 | 0 | 1461k | 12.9 s |
| Layerweave pipeline | 2002 | 2866 | 0 | 1158k | 1.5 s |
| elkjs | 2052 | 2384 | 3 | 1233k | 581 ms |
| elk-rs | 2031 | 2254 | 3 | 1153k | 70 ms |

| dataset | pipeline: crossings / bends / time | elk-rs: crossings / bends / time |
|---|---|---|
| layerweave-code | 1591 / 1285 / 1106 ms | 1586 / 1150 / 31 ms |
| deployment-cluster | 77 / 254 / 67 ms | 68 / 162 / 6 ms |
| workflow-dag | 222 / 618 / 125 ms | 249 / 434 / 10 ms |
| org-chart | 18 / 206 / 106 ms | 22 / 148 / 8 ms |
| cloud-architecture | 13 / 124 / 28 ms | 15 / 106 / 4 ms |
| network-vlan | 27 / 142 / 26 ms | 26 / 94 / 3 ms |
| petri-net | 47 / 98 / 13 ms | 58 / 66 / 2 ms |
| uml-class | 0 / 51 / 20 ms | 2 / 38 / 3 ms |
| layerweave-pipeline | 7 / 88 / 19 ms | 5 / 56 / 3 ms |

Against the start of this work (`eval-20261102.md`, layerweave-code 150 nodes: default refined 80 s, layered refined 21 s):
the pipeline draws the nine views with fewer crossings than elk-rs and no edge through a node, in 1.5 s against 43 s for the
default and 12.9 s for the layered construction; elk-rs needs 70 ms. Bends remain the quality gap (2866 against 2254), time
the other (about 20 times elk-rs; the import alone, without refinement, 392 ms in `bench`, see
`layered-pipeline-arrays-20261103.md`).
