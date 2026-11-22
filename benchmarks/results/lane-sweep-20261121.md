# Lane changes in the constructive search: sweep instead of scan (2026-11-21)

Pure speedup: the lane changes inside a corridor of `constructive::search_on` were found by scanning outwards from every
reached lane (quadratic in the lanes); they are now found in two sweeps with a running minimum (linear), with near-tied
candidates decided by the exact cost sum and the old tie order. The output is byte-identical.

- Machine: Apple M3, macOS, heavily loaded (load average 8–14 during the runs; times are wall-clock inside the engine).
- Build: `cargo build --release -p arqlet-runtime --bin layout-command`; before = `9b72ad1`, after = `9b72ad1` plus this
  change. Both binaries run alternately, 5 times each; the table gives medians.
- Measured with the engine's own phase timings (`timings: true`, `elkLayout`).
- Inputs: two private evaluation graphs (not in the repository; their figures are not published). The speedup is largest on a graph with long edges and `POLYLINE` routing; the
  default single pass does not use this search.

Output identity: the snapshots of both binaries are byte-identical on every benchmark dataset with `defaultRouting`
`polyline` and `orthogonal` (41 datasets × 2) and on the two private inputs. The 15 refinement fingerprint
views are identical. A unit test compares sweep and scan bit for bit on 4 000 generated corridors (ties, blocked
pieces, crossing costs of 10^6).
