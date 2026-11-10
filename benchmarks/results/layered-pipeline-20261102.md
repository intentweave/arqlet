# Consistent layered pipeline against elk-rs, 2026-11-02

Machine: Apple M3, Node v24.21.0, Layerweave release build (`layout-command`), revision 37bdb10 plus uncommitted
changes; elk-rs `@openedges/elk-layout` 0.12.1. `node compare.mjs --uniform-direction --repeat 3 --limit 150` (every engine
lays out the same view, every container left to right and layered). Times are medians of 3 runs and include import and, for
the refined rows, refinement through `layout-command`; the machine was not idle. Raw rows: `layered-pipeline-20261102.jsonl`.
Default output unchanged (16 of 16 refinement fingerprints, `bench`); the workspace tests of this state were not run to the end.

Each cell: crossings / bends / line-node hits / time. "pipeline" is `layeredConstruction` with `hierarchicalOrdering`.

| dataset | layered, refined | pipeline, import | pipeline, refined | elk-rs |
|---|---|---|---|---|
| layerweave-code | 2040 / 907 / 0 / 22815 ms | 1581 / 2041 / 2 / 457 ms | 1845 / 1941 / 0 / 3345 ms | 1586 / 1150 / 2 / 115 ms |
| deployment-cluster | 78 / 140 / 0 / 124 ms | 95 / 316 / 0 / 32 ms | 95 / 316 / 0 / 85 ms | 68 / 162 / 0 / 6 ms |
| workflow-dag | 465 / 268 / 0 / 310 ms | 419 / 554 / 0 / 73 ms | 419 / 554 / 0 / 257 ms | 249 / 434 / 0 / 17 ms |
| org-chart | 44 / 194 / 0 / 287 ms | 25 / 202 / 0 / 65 ms | 25 / 202 / 0 / 196 ms | 22 / 148 / 0 / 15 ms |
| cloud-architecture | 21 / 80 / 0 / 60 ms | 18 / 138 / 0 / 22 ms | 18 / 138 / 0 / 58 ms | 15 / 106 / 0 / 8 ms |
| network-vlan | 37 / 106 / 0 / 96 ms | 30 / 146 / 0 / 22 ms | 30 / 146 / 0 / 56 ms | 26 / 94 / 0 / 5 ms |
| petri-net | 60 / 92 / 0 / 56 ms | 58 / 88 / 0 / 10 ms | 58 / 88 / 0 / 31 ms | 58 / 66 / 0 / 3 ms |
| uml-class | 4 / 55 / 0 / 39 ms | 3 / 49 / 0 / 16 ms | 3 / 49 / 0 / 38 ms | 2 / 38 / 1 / 5 ms |
| layerweave-pipeline | 17 / 80 / 0 / 73 ms | 9 / 74 / 0 / 20 ms | 9 / 74 / 0 / 44 ms | 5 / 56 / 0 / 6 ms |
| **sum** | 2766 / 1922 / 0 / 23859 ms | 2238 / 3608 / 2 / 716 ms | 2502 / 3508 / 0 / 4109 ms | 2031 / 2254 / 3 / 180 ms |

Findings:
- The pipeline's import has fewer crossings than the layered construction on every view and the fewest of all on
  layerweave-code (1581, elk-rs 1586); in sum 2238 against elk-rs 2031.
- Bends are the gap: 3608 against 2254. Bundles of one chain run through tight dummies, so edges from spread ports or
  border nodes change lanes where they join.
- Time: layerweave-code imports in 457 ms (the layered construction with refinement: 22.8 s; elk-rs 115 ms).
- Refinement makes the pipeline's layerweave-code worse (1581 → 1845 crossings): two routes touch a node, a hard term, and
  the re-routing that removes them gives up the routes from the sections.
