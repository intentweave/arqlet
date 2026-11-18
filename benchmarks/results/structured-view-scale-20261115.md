# The structured view on synthetic-20k at growing limits (2026-11-15)

Machine: Apple M3, macOS (Darwin 25.0.0), not idle (other work running; single runs, times are indications). Build:
`cargo build --release -p arqlet-runtime --bins`, branch `packing`. Graph: `benchmarks/datasets/synthetic-20k.json`
(22 819 nodes, 61 082 edges, 2 819 containers, default direction `hybrid`). Each row: one `layout-command` process with
`{op: "import", graph, limit}`, then one with `{op: "scene", snapshot}`; the time includes reading and writing JSON.

| limit | visible nodes / edges | single pass | import: start of the day | after the three fixes below | snapshot | scene |
|---|---|---|---|---|---|---|
| 1 000 | 1 710 / 1 678 | no | no end in 15 min | 8.3 s | 4 MB | 99 ms |
| 2 000 | 3 142 / 3 555 | yes | 21.3 s | 10.4 s | 9 MB | 371 ms |
| 5 000 | 6 991 / 10 079 | no | no end in 15 min | 78 s (198 s without fix 3) | 31 MB | 1.1 s |
| 10 000 | 12 633 / 23 584 | no | no end in 15 min | 912 s (without fix 3) | 83 MB | 4.0 s |

The fixes, each with byte-identical output on the default fingerprint views and on seven organic datasets:

1. `edge_choices` (`d5e392c`): the routing styles looked every parent of every edge up by a linear search of the nodes.
2. Rerouting bounds (`b84d7e8`): the engine choice (both routers on the whole drawing) and the rerouting of every route
   in conflict ran without a size bound; both now run up to 500 routes or conflicts, above that the conflicts are reported.
   This also took synthetic-20k with one level open (`levelOfDetail: {depth: 1}`: 349 visible nodes, 8 072 edges, nearly
   every route in conflict) from more than 10 minutes to 7.4 s.
3. Loop corners (this change): `loopSpace` tested every node with its ancestors for every corner of every node with a
   loop; the nodes within reach of the corners are now found once per node, and the ancestors tested only for them.

Where the time goes now (macOS `sample` of the 5 000 import): 73 % in the constructive routing (`construct_planned_from`),
of which `lane_changes` (a search per edge over the gaps, before the routes are built) 38 %, the route search 64 % in all.
The 10 000 limit was not measured again with fix 3. Snapshot and scene grow with the view: 83 MB and 75 MB at 12 633
visible nodes.
