# First view within a budget of nodes and edges (2026-11-18)

Machine: Apple M3, macOS (Darwin 25.0.0), heavily loaded (times of the same configurations were about twice those of
runs an hour before; single runs, indications only). Build: `cargo build --release -p arqlet-runtime --bins`, branch
`first-view-budget` on `d9e89bc`. Command: `node benchmarks/eval/first-view-budget.mjs [--flat] GRAPH OPTIONS ...`, one
`layout-command` import per row with `limit` 10 000, reading and writing JSON included. Raw rows:
`first-view-budget-20261118.jsonl`.

Graphs: `synthetic-20k` (22 819 nodes, 61 082 edges, 2 819 containers, three levels) and the same graph flat (`--flat`:
its 20 000 leaves at the top level, the 58 091 edges between them).

*Boxes* are what the view shows (leaves and collapsed containers); *frames* are open containers, which do not count.
*Constructed* and *delegated* are the single pass's own routes and those it left to the fallback router.

## synthetic-20k

| options | boxes | frames | edges | constructed | delegated | time | answer |
|---|---|---|---|---|---|---|---|
| `levelOfDetail: {depth: 0}` | 38 | 0 | 701 | 701 | 0 | 2.7 s | 7.3 MB |
| `levelOfDetail: {depth: 1}` | 311 | 38 | 8 072 | 428 | 7 644 | 13.4 s | 17 MB |
| `levelOfDetail: {nodes: 300}` | 298 | 36 | 7 730 | 406 | 7 324 | 12.9 s | 15.7 MB |
| `levelOfDetail: {nodes: 1000}` | 999 | 135 | 10 463 | 2 371 | 8 092 | 20.7 s | 27.4 MB |
| `levelOfDetail: {nodes: 1000, edges: 2000}` | 94 | 10 | 2 000 | 506 | 1 494 | 4.2 s | 8.7 MB |
| `levelOfDetail: {nodes: 3000, edges: 5000}` | 203 | 24 | 4 998 | 365 | 4 633 | 7.8 s | 12.4 MB |
| `levelOfDetail: {edges: 1000}` | 50 | 2 | 988 | 645 | 343 | 11.0 s | 7.6 MB |
| `visibleNodeBudget: 300` (earlier experiment) | 300 | 41 | 3 931 | – | – | 75.6 s | 9.1 MB |

The older `visibleNodeBudget` runs the earlier pipeline (single pass refuses a view budget) and selects the source by
`limit` (12 633 nodes kept, not the whole graph); `levelOfDetail` keeps the whole source graph and the single pass.

## synthetic-20k, flat

| options | boxes | frames | edges | constructed | delegated | time | answer |
|---|---|---|---|---|---|---|---|
| `levelOfDetail: {nodes: 300}` (communities 300 from the budget) | 300 | 79 | 6 904 | 323 | 6 581 | 41.3 s | 24.3 MB |
| `levelOfDetail: {nodes: 300, edges: 2000}` | 70 | 0 | 2 065 | 2 065 | 0 | 11.6 s | 15.7 MB |
| `levelOfDetail: {nodes: 1000, edges: 3000}` | 91 | 13 | 2 998 | 1 343 | 1 655 | 21.9 s | 16.9 MB |

Without the budget's communities the flat graph has nothing to collapse: `{nodes: 300}` alone laid out 10 000 leaves and
had not finished after 10 minutes (stopped). With `{nodes: 300, edges: 2000}` the top level alone (70 communities) draws
2 065 edges: a budget below the closed top level is not reached, nothing opens.

## Findings

- The node budget keeps the boxes within it; the edge budget keeps the drawn edges within it (exactly as the projection
  merges them). The edge budget is what keeps such a view small: between the groups of synthetic-20k the graph is nearly
  complete (38 top-level groups, 701 of 703 possible pairs), so a node budget alone gives 25 edges per box.
- Dense aggregated views leave most edges to the fallback router (7 324 of 7 730 at `nodes: 300`), which is where their
  time goes; `{edges: 1000}` takes 11 s for 50 boxes for that reason. Bundling of aggregated edges, or drawing only the
  strongest pairs, is the next lever; not done here.
- The answer carries the whole source graph (7 to 8 MB for synthetic-20k whatever the view): the transfer belongs to
  ADR 0019.
