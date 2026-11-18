# The import on the index, and a snapshot without its source (2026-11-18)

Machine: Apple M3 (8 cores, 16 GB), macOS (Darwin 25.0.0), load average 4 to 15 (single runs, before and after right
after each other; indications). Build: `cargo build --release -p arqlet-runtime --bin layout-command`; before: branch
`import-prepare` (#37), after: branch `import-index`. Request: the 1 000 000-node graph of
`benchmarks/overview/generate.py` (seed 1, 1.6M edges, 200 MB of JSON), `{op: "import", limit: 10000, options:
{levelOfDetail: {nodes: 1000, edges: 3000}, liftEdges: true}}`, one `layout-command` process.

## The import on the index (byte-identical)

The first view's selection (`select_visible`), its collapsed containers (`initially_collapsed`) and the budget
(`level_budget_collapse`) walk the structured graph by its index (`GraphIndex`, built once and kept with the structure)
instead of maps by id and walks to the root per edge end. The order of ids still decides where it did before (seeds
among equal degrees, neighbours in a breadth-first search): a node's rank in id order stands for its id.

| | import at 1M |
|---|---|
| before (#37) | 16.5 s |
| after | 10.6 s |

Byte-identical: imports of every benchmark dataset and fixture, session imports, toggles, an export and a one-shot toggle
at 20K and 100K with three sets of options, and the 1M import above, compared with #37; 15 refinement fingerprint views.

## `omitSource`: the answer without the source graph (provisional)

An import with `omitSource: true` leaves the source graph out of its snapshot and sets `sourceDigest` (FNV-1a over the
graph's JSON). A request on such a snapshot brings the graph as `source`, as the host sent it to the import (unresolved
edges included); the engine resolves it as the import did, checks the fingerprint, and answers without it again. A
missing or different source is refused. An answer whose source changed (an added node, a patch) carries the new source
whole. An import that kept only part of the graph (a limit without an abstracting view) answers whole.

| request at 1M | time | peak memory | answer |
|---|---|---|---|
| import | 11.1 s | 3.3 GiB | 187.3 MB |
| import, `omitSource` | 8.9 s | 2.5 GiB | 7.3 MB |
| toggle on the whole snapshot (one-shot) | 8.8 s | 3.7 GiB | 188.6 MB |
| toggle on the lean snapshot with `source` | 7.8 s | 3.8 GiB | 8.6 MB |

The view is the same (snapshots compared without `sourceGraph` and `sourceDigest`). The request of a one-shot toggle still
carries the graph (209 MB); a resident session (ADR 0019 step 3) carries neither.
