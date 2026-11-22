# Dense channels: lane-change order and channel pairs without copies (2026-11-21)

Pure speedups, output byte-identical:

- `constructive::dependent_order` (the order of the lane changes of one channel) kept an n × n `i64` cost matrix read
  along columns, and its sifting copied the order and summed the costs again for every change. It now keeps the
  difference matrix as `i32`, read along rows, and finds the best place and the cost where the change stands in one pass,
  copying only on an improvement.
- `abstract_realization::joint::reserve_once` inserted the cross-flow pairs of a channel into a `BTreeMap` one by one
  (over a million on a dense input); they are collected, sorted and built at once, each keeping the larger gap.

Setup: Apple M3, macOS, heavily loaded (load average 8–11). `cargo build --release -p arqlet-runtime --bin
layout-command`. Before = `9b72ad1`; after = `9b72ad1` with `lane-sweep` (`benchmarks/results/lane-sweep-20261121.md`)
and this change. Both binaries alternately, 5 runs each, medians of the engine's phase timings (`timings: true`,
`elkLayout`). Measured on two private evaluation graphs (not in the repository); their figures are not published. On a dense flat graph the remaining time is mostly `reserve_once`
and the solving over its channel pairs: one track per edge in a channel is the cause, not an implementation detail;
request 27 (shared tracks) addresses it.

Output identity: byte-identical on all 41 benchmark datasets with `defaultRouting` `polyline` and `orthogonal` and on
the two private inputs; 15 refinement fingerprint views identical; the existing test against the reference
`dependent_order` passes.
