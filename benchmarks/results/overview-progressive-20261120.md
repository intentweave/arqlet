# Overview built in steps: rough overview against the whole one (2026-11-20)

The overview page shows a rough overview of the top levels of the containment hierarchy first and lets the whole one
bloom out of it (`apps/playground/progressive.js`). This measures both requests through the native engine.

Machine: Apple M3 (8 cores, 16 GB), macOS (Darwin 25.0.0), **heavily loaded** (load average 31 to 32 during the runs):
the times are indications. Build: `cargo build --release -p arqlet-runtime --bin layout-command`, branch
`overview-housekeeping` on `cbd3c8d`. Command: `node benchmarks/overview/progressive.mjs --sizes 10000 50000 100000 --reps 3`
(graphs of `benchmarks/overview/generate.py --seed 1`, shaped like a code index; the galaxy forces gravity 0.05,
repulsion 0.1, gap 3; 4 threads). Each request is one `layout-command` process, JSON read and write included. Raw rows:
`overview-progressive-20261120.jsonl`.

| nodes | edges | rough graph (nodes, edges, levels) | rough overview | runs | whole overview | runs |
|---|---|---|---|---|---|---|
| 10000 | 15205 | 105, 210, 2 | 0.01 s | 0.60, 0.01, 0.01 | 0.33 s | 0.31, 0.33, 0.35 |
| 50000 | 75856 | 349, 698, 2 | 0.02 s | 0.01, 0.02, 0.02 | 2.66 s | 1.77, 2.66, 197.39 |
| 100000 | 151865 | 519, 1038, 2 | 0.02 s | 0.02, 0.03, 0.02 | 2.68 s | 2.68, 2.93, 2.64 |

The rough overview takes hundredths of a second at every size; the whole overview takes 0.3 s at 10 000 nodes and
about 2.7 s at 50 000 and 100 000 nodes. Two runs are outliers of the loaded machine and are kept as measured: the first
rough request at 10 000 nodes (0.60 s, the first process of the run) and one whole request at 50 000 nodes (197 s, the
machine stalled; the other two took 1.77 and 2.66 s). In the browser the page adds the transfer to the worker and the
drawing; not measured here.
