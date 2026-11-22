# Memory of a resident session per node, 2026-11-21

Question: how much heap does a resident engine session (`EngineSession`, the WASM session behind `createSession` with
`resident`) keep per node of the source graph after an import, how high does the import peak, and what is it made of?
Tool: `benchmarks/session-memory` (a standalone crate with a counting allocator; see its source).

## Machine, build, inputs

- Apple M3, macOS 26.0, rustc 1.99.0 (b940084d7 2026-09-28), profile `release` of the standalone crate (no LTO).
- Revision cd63a10 plus the change of this measurement (one-pass import in the session, graph lists trimmed).
- Heap bytes requested from the allocator, counted exactly (live and peak); not resident memory of the process. The
  counts do not depend on the load of the machine (which was high, 25 to 29).
- Graphs from `benchmarks/generate.py::synthetic` (random three-level hierarchy with cycles, self-loops, parallel and
  unresolved edges): 2k and 20k as in the repository's datasets (seeds 11, 12), 100k with seed 13 (114 196 nodes,
  305 650 edges). Import with `limit` 100 and `levelOfDetail: {depth: 0}`, `scene: false`, sent as one JSON text as the
  browser sends it (`lw_session_run`).

```sh
CARGO_TARGET_DIR=target cargo build --release --manifest-path benchmarks/session-memory/Cargo.toml
target/release/session-memory benchmarks/datasets/synthetic-20k.json 100 '{"levelOfDetail":{"depth":0}}'
SESSION_MEMORY_VALUE=1 target/release/session-memory …        # the way before: the request read as a JSON value first
cargo build --release --manifest-path benchmarks/session-memory/Cargo.toml --features profile   # dhat heap profile
```

## Results

| Graph | First view | Kept before | Kept after | Peak before | Peak after |
|---|---|---|---|---|---|
| 2 286 nodes | 3 boxes, 3 routes | 1.5 MB (672 B/node) | 1.5 MB (672 B/node) | 8.2 MB (3 766 B/node) | 2.4 MB (1 114 B/node) |
| 22 819 nodes | 38 boxes, 701 routes | 20.5 MB (943 B/node) | 16.8 MB (774 B/node) | 130.0 MB (5 972 B/node) | 69.1 MB (3 177 B/node) |
| 114 196 nodes | 100 boxes, 4 160 routes | 95.3 MB (875 B/node) | 89.0 MB (817 B/node) | 1 183.7 MB (10 869 B/node) | 885.8 MB (8 133 B/node) |

"Before" is the request read as a JSON value first (`EngineSession::run` until this change); "after" reads an import in
one pass into its types, as `dispatch` already did, and trims the graph's lists to their length (read from text, a list
grows by doubling). The peak is measured above the request text itself (7.5 MB for 20k). Answers and kept snapshots are
identical both ways (test `a_session_reads_an_import_in_one_pass_with_the_same_answers`).

What the kept memory is (22 819 nodes, heap of a copy of each part): the source graph 10.3 MB (474 B/node), the index of
its structure 3.5 MB (163 B/node; the structure's view shares the source graph when no groups or communities change it),
the bounded view graph 0.6 MB, routes 0.1 MB, the rest of the snapshot 2.4 MB.

## What it means

- A session keeps about **0.8 KB per node** of the source graph, stable from 2k to 100k nodes: the graph itself and one
  index. That is what stays after the import.
- The **peak** is what a browser sees: WASM linear memory grows to the peak and is never returned. Before this change a
  third to a half of the peak was the JSON value of the request (the graph as a `serde_json::Value` takes 2.6 KB per
  node, seven times the typed graph). That part is gone.
- The rest of the peak is the **layout of the first view**, and it grows with that view's routes, not with the graph:
  3 routes cost little, 701 routes about 50 MB, 4 160 routes (100 boxes, the edges of 114k nodes aggregated) about 830
  MB. A dhat profile of the 20k import puts the peak in placement and routing structures (rects, the placement plan,
  range trees, cross plans, the joint coordination's bends), each a few percent. Reducing it is a layout change of its
  own (measured before and after, output byte-identical).

## Where the layout's peak sits (114 196 nodes, first view of 100 boxes and 4 160 routes)

A dhat profile of the import from a typed graph (release with line tables, `--features profile`) peaks at 877 MB in
10.9 million blocks, all in the layout of the first view. The largest sites at the peak:

| MB | Blocks | Site |
|---|---|---|
| 43.7 | 575 898 | `constructive/abstract_realization/completion.rs:145` (called from `layout/src/lib.rs:5006`) |
| 39.2 | 98 | a clone of the abstract plan, `abstract_routing.rs:89`, in `planning.rs:163` |
| 36.2 | 581 222 | `placement_geometry.rs:90` |
| 35.4 | 54 | `abstract_routing.rs:425` in `planning.rs:161` |
| 28.0 + 28.0 | 1 + 1 | `continuity.rs:304`, once through `chains.rs:152` and once through `completion.rs:145` |
| 26.0 + 26.0 | 1 + 1 | `joint.rs:1068`, once through `chains.rs:144` and once through `completion.rs:145` |
| 26.0 | 1 | `continuity.rs:228` |
| 25.7 | 21 366 | `coordination.rs:328` (from `joint.rs:688`) |

Single blocks of 26 to 28 MB point to tables that grow with the square of the segments of a channel, and two of them
are alive twice at the same time (through `chains` and through `completion`); the plan is cloned. These are the
candidates for a layout change that lowers the peak; none of them is changed here.
