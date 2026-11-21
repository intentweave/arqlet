# Lane changes in the constructive search: sweep instead of scan (2026-11-21)

Pure speedup: the lane changes inside a corridor of `constructive::search_on` were found by scanning outwards from every
reached lane (quadratic in the lanes); they are now found in two sweeps with a running minimum (linear), with near-tied
candidates decided by the exact cost sum and the old tie order. The output is byte-identical.

- Machine: Apple M3, macOS, heavily loaded (load average 8–14 during the runs; times are wall-clock inside the engine).
- Build: `cargo build --release -p arqlet-runtime --bin layout-command`; before = `9b72ad1`, after = `9b72ad1` plus this
  change. Both binaries run alternately, 5 times each; the table gives medians.
- Measured with the engine's own phase timings (`timings: true`, `elkLayout`).
- Inputs: two anonymized 500-node evaluation graphs (ELK JSON with the explorer's options; private, not in the
  repository): "dense" (500 nodes, 3 138 edges, almost one layer) and "deep" (500 nodes, 1 505 edges, edges spanning up
  to 28 layers). `POLYLINE` sets `elk.edgeRouting: POLYLINE`.

| Input | Layout before | Layout after | Routing before | Routing after |
| --- | ---: | ---: | ---: | ---: |
| deep, `POLYLINE` | 3 505 ms | **1 883 ms** | 3 143 ms | **1 520 ms** |
| dense, `POLYLINE` | 1 482 ms | 1 485 ms | 924 ms | 928 ms |
| deep, `ORTHOGONAL` (default single pass, no constructive search) | 526 ms | 530 ms | 266 ms | 267 ms |

Work on "deep, `POLYLINE`": 462 searches, lane-change transitions 544 383 254 → 4 748 027.

Output identity: the snapshots of both binaries are byte-identical on every benchmark dataset with `defaultRouting`
`polyline` and `orthogonal` (41 datasets × 2) and on the four inputs above (86 cases). The 15 refinement fingerprint
views are identical. A unit test compares sweep and scan bit for bit on 4 000 generated corridors (ties, blocked
pieces, crossing costs of 10^6).
