# `liftEdges`: one weighted route per pair of groups (2026-11-18)

Machine: Apple M3, macOS (Darwin 25.0.0), loaded (single runs, times are indications). Build:
`cargo build --release -p arqlet-runtime --bins`, branch `bundle-aggregates` on `9961130`. Each row: one
`layout-command` import with `limit` 10 000, scored with the shared metrics (`benchmarks/eval/lib.mjs`, `metrics`).
Raw rows: `lift-edges-20261118.jsonl`. Graphs as in [first-view-budget-20261118.md](first-view-budget-20261118.md):
synthetic-20k (22 819 nodes, 61 082 edges, three levels of containers) and the same graph flat (20 000 leaves, 58 091
edges; with `nodes`, the budget divides it into communities).

`liftEdges` draws an edge whose ends are not siblings between the outermost containers in which they differ; the edges of
one pair of containers become one route with their weights in both directions (`aggregateWeights`, as between collapsed
containers). Edges between siblings stay one by one.

| graph | first view | lift | boxes | edges | delegated | line–node hits | crossings | bends | area | time |
|---|---|---|---|---|---|---|---|---|---|---|
| synthetic-20k | nodes 300 | no | 298 | 7730 | 7324 | 10775 | 8 410 185 | 15328 | 4 834 M | 7.2 s |
| synthetic-20k | nodes 300 | yes | 298 | 1126 | 2 | 0 | 33 940 | 3412 | 708 M | 1.6 s |
| synthetic-20k | nodes 1000 | no | 999 | 10463 | 8092 | 17237 | 10 374 118 | 24747 | 18 621 M | 11.2 s |
| synthetic-20k | nodes 1000 | yes | 999 | 2727 | 3 | 0 | 34 150 | 6708 | 3 547 M | 1.5 s |
| synthetic-20k | depth 1 | no | 311 | 8072 | 7644 | 14108 | 9 200 547 | 16014 | 4 896 M | 7.5 s |
| synthetic-20k | depth 1 | yes | 311 | 1147 | 2 | 0 | 33 944 | 3448 | 749 M | 1.0 s |
| synthetic-20k | depth 2 | no | 2470 | 15063 | 8599 | 26455 | 9 980 333 | 41923 | 54 137 M | 13.5 s |
| synthetic-20k | depth 2 | yes | 2470 | 6027 | 1 | 0 | 34 623 | 13476 | 19 102 M | 2.3 s |
| synthetic-20k (flat) | nodes 300 | no | 300 | 6904 | 6581 | 7326 | 6 583 782 | 13668 | 6 810 M | 21.5 s |
| synthetic-20k (flat) | nodes 300 | yes | 300 | 2409 | 478 | 329 | 378 501 | 7620 | 1 443 M | 3.7 s |
| synthetic-20k (flat) | nodes 1000 | no | 1000 | 10375 | 9004 | 20550 | 11 454 987 | 20655 | 15 194 M | 31.3 s |
| synthetic-20k (flat) | nodes 1000 | yes | 1000 | 3605 | 194 | 33 | 338 375 | 10927 | 5 097 M | 6.3 s |

Findings:

- On synthetic-20k every view drops from 10 000 or more line–node hits to none, from millions of crossings to about
  34 000, and to a sixth to a third of the area; the single pass builds nearly every route itself (1 to 3 delegated
  instead of thousands), so the import takes 1.0 to 2.3 s instead of 7 to 13 s.
- The remaining crossings are those between the 38 top-level groups, whose traffic is nearly complete (701 of 703 pairs);
  they are the same in every view.
- The flat graph keeps some hits and delegated routes (329 and 478 at `nodes: 300`): its communities come from Louvain on
  random edges, and the sibling edges between communities and their members' neighbours stay one by one.
- What lifting gives up: which member of a group an edge starts or ends at, until the group's members are siblings of the
  other end (an opened group, or edges inside it). Bundled routes with taps to the members (a trunk per pair of groups)
  would keep that; they belong to bus routing.
