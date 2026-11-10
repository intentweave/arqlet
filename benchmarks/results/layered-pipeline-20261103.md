# Consistent layered pipeline against elk-rs, 2026-11-03

Machine: Apple M3, Node v24.21.0, Layerweave release build (`layout-command`), revision 37bdb10 plus uncommitted
changes; elk-rs `@openedges/elk-layout` 0.12.1. `node compare.mjs --uniform-direction --repeat 3 --limit 150`. The machine was
heavily loaded during the run (load average 40 to 70 on 8 cores), so no times are reported here; see
`layered-pipeline-20261102.md` for times on a less loaded machine. Raw rows: `layered-pipeline-20261103.jsonl`. Default output
unchanged (16 of 16 refinement fingerprints, `bench`); workspace tests pass (582).

Each cell: crossings / bends / line-node hits. "pipeline" is `layeredConstruction` with `hierarchicalOrdering`.

| dataset | layered, refined | pipeline, import | pipeline, refined | elk-rs |
|---|---|---|---|---|
| layerweave-code | 2040 / 907 / 0 | 1430 / 1873 / 0 | 1430 / 1873 / 0 | 1586 / 1150 / 2 |
| deployment-cluster | 78 / 140 / 0 | 81 / 314 / 0 | 77 / 284 / 0 | 68 / 162 / 0 |
| workflow-dag | 465 / 268 / 0 | 218 / 632 / 0 | 218 / 632 / 0 | 249 / 434 / 0 |
| org-chart | 44 / 194 / 0 | 18 / 256 / 0 | 18 / 256 / 0 | 22 / 148 / 0 |
| cloud-architecture | 21 / 80 / 0 | 16 / 134 / 0 | 16 / 134 / 0 | 15 / 106 / 0 |
| network-vlan | 37 / 106 / 0 | 25 / 142 / 0 | 25 / 142 / 0 | 26 / 94 / 0 |
| petri-net | 60 / 92 / 0 | 49 / 106 / 0 | 49 / 106 / 0 | 58 / 66 / 0 |
| uml-class | 4 / 55 / 0 | 0 / 69 / 0 | 0 / 67 / 0 | 2 / 38 / 1 |
| layerweave-pipeline | 17 / 80 / 0 | 5 / 100 / 0 | 5 / 100 / 0 | 5 / 56 / 0 |
| **sum** | 2766 / 1922 / 0 | 1842 / 3626 / 0 | 1838 / 3594 / 0 | 2031 / 2254 / 3 |

Changes against 2026-11-02: in the hierarchical sweep the border rank at a container's far end follows the order inside
(workflow-dag 419 → 218 crossings); eight ordering runs instead of three; the lanes of every rank are placed at once
(weighted least squares under order and gaps), border nodes follow their edges and are placed last; the two routes
through nodes on layerweave-code are gone, so the refinement keeps the import.
