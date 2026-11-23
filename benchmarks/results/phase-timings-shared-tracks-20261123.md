# Phase timings for ELK callers, and the `sharedTracks` fallback without a second completion (#152), 2026-11-23

Machine: Apple M3 (MacBook), macOS, heavily loaded during the runs (load average 15 to 40), so take the ratio, not the
absolute times. Revisions: before = `8ca38c8` (origin/main), after = this branch. Build: `cargo build --release -p
arqlet-runtime --bin layout-command --locked`. Input: `node benchmarks/experiments/dense-flat.mjs 12 488 3138
[target]` (500 nodes, 3 138 edges, nearly all nodes in one rank; seeded, public), one import each, 3 runs, wall time.

| `sharedTracks` | Before (s) | After (s) | Boxes and routes |
| --- | --- | --- | --- |
| none | 1.35 0.80 0.80 | 1.16 0.79 0.79 | identical (`e72dc62257`) |
| `target` | 1.92 2.01 1.92 | 1.51 1.49 1.49 | identical (`e72dc62257`) |

On this graph shared tracks make the lane orders cyclic for many edges, so the layout falls back to one without them
(`sharedTracks: shared tracks made the lane orders of many edges cyclic; laid out without them`). Before, the completion
first completed the drawing again without groups and then `compute_counted` laid out the whole view again without shared
tracks, discarding that drawing; the completion now stops at the detection (`Abort::SharedTracksCyclic`) and only the
second layout runs. The result is the one before; about 0.7 s of overhead remains, the first layout with shared tracks up
to the detection. Detecting it before the placement is open.

Fingerprints: the 15 default views (`fingerprints-20261122-default-0.8.0.jsonl`, release bench, threads 1 and 4) are
identical (`layerweave-code` on the same dataset as the previous comparison).
