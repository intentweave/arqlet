# Dense channels: lane-change order and channel pairs without copies (2026-11-21)

Pure speedups, output byte-identical:

- `constructive::dependent_order` (the order of the lane changes of one channel) kept an n × n `i64` cost matrix read
  along columns, and its sifting copied the order and summed the costs again for every change. It now keeps the
  difference matrix as `i32`, read along rows, and finds the best place and the cost where the change stands in one pass,
  copying only on an improvement.
- `abstract_realization::joint::reserve_once` inserted the cross-flow pairs of a channel into a `BTreeMap` one by one
  (1.49 million on the dense input); they are collected, sorted and built at once, each keeping the larger gap.

Setup: Apple M3, macOS, heavily loaded (load average 8–11). `cargo build --release -p arqlet-runtime --bin
layout-command`. Before = `9b72ad1`; after = `9b72ad1` with `lane-sweep` (`benchmarks/results/lane-sweep-20261121.md`)
and this change. Both binaries alternately, 5 runs each, medians of the engine's phase timings (`timings: true`,
`elkLayout`). Inputs: the two anonymized private 500-node graphs of the lane-sweep file ("dense": 3 138 edges, almost
one layer; "deep": 1 505 edges, long edges).

| Input | Layout before | Layout after | Placement before → after | Routing before → after |
| --- | ---: | ---: | --- | --- |
| dense, `ORTHOGONAL` (default single pass) | 1 400 ms | **773 ms** | 569 → 229 | 806 → 511 |
| dense, `POLYLINE` | 1 607 ms | **623 ms** | 1 051 → 327 | 1 003 → 379 |
| deep, `POLYLINE` | 3 588 ms | **1 805 ms** | 694 → 599 | 3 204 → 1 468 |
| deep, `ORTHOGONAL` | 533 ms | 517 ms | 231 → 229 | 268 → 256 |

On "dense", the largest channel holds 2 585 lane changes: `dependent_order` took 410 ms for it before (334 ms of it in
the sifting). The remaining time on "dense" is mostly `reserve_once` (the 1.49 million channel pairs and the
11.5 million pairs of its sweep, twice per layout) and the solving over those pairs: one track per edge in a channel is
the cause, not an implementation detail; request 27 (shared tracks) addresses it.

Output identity: byte-identical on all 41 benchmark datasets with `defaultRouting` `polyline` and `orthogonal` and on
the four inputs above (86 cases); 15 refinement fingerprint views identical; the existing test against the reference
`dependent_order` passes.
