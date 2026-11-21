# Channel rule by range query instead of listed pairs (2026-11-21)

Pure speedup, output byte-identical. Two bends of one channel whose cross ranges come within a track keep a track apart
along the flow (`abstract_realization::joint::reserve_once`). These pairs were listed as constraints, every pair of a
channel tested: 1.49 million on the dense input, built twice per layout and read by the compaction. They are no longer
listed: each channel keeps its bends in order, and the compaction (and the demand pass) asks a segment tree over the
half-open cross ranges for the greatest value among the earlier overlapping bends (least among the later ones for the
upper bounds), taking back the bends of an edge that fails on capacity. A channel with an empty range falls back to the
pair test. Benchmark builds still list the pairs for their diagnostics (`crossFlowConstraints`, `precedences`).

Setup: Apple M3, macOS, heavily loaded (load average 8–17). `cargo build --release -p arqlet-runtime --bin
layout-command`. Three binaries alternately, 5 runs each, medians of the engine's phase timings (`timings: true`,
`elkLayout`): base = `9b72ad1`; dense-channels = base with `lane-sweep` and `dense-channels`
(`benchmarks/results/dense-channels-20261121.md`); now = dense-channels with this change. Inputs: the anonymized
private 500-node graphs of those files.

| Input | Layout base | dense-channels | now | Routing base → now |
| --- | ---: | ---: | ---: | --- |
| dense, `ORTHOGONAL` (default single pass) | 1 386 ms | 767 ms | **601 ms** | 791 → 339 |
| dense, `POLYLINE` | 1 594 ms | 624 ms | 612 ms | 985 → 366 |
| deep, `ORTHOGONAL` | 532 ms | 519 ms | 516 ms | 269 → 251 |
| deep, `POLYLINE` | 3 528 ms | 1 749 ms | 1 750 ms | 3 164 → 1 421 |

Output identity: byte-identical on all 41 benchmark datasets with `defaultRouting` `polyline` and `orthogonal` and on
the four inputs above (86 cases); 15 refinement fingerprint views identical. A unit test compares the compaction with
the channel lists and with every pair listed on 400 generated cases (120 of them with edges failing on capacity),
values bit for bit and the failed edges.

For comparison (same machine, same inputs, `benchmarks/eval` with `@openedges/elk-layout` 0.12.1, the explorer's options
but `nodePlacement.strategy: BRANDES_KOEPF`): elk-rs takes about 318 ms on "dense" and 120 ms on "deep". With the
explorer's `NETWORK_SIMPLEX` placement it takes about 740 ms on "dense" and 2.9–4.2 s on "deep".
