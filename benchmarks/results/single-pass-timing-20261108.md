# Single pass timing at the default switch, 2026-11-08

Revision `d874982`, Apple M3 (8 logical CPUs), release build of `bench` without the `benchmark-timings` feature
(`CARGO_TARGET_DIR=… cargo build --release --locked -p layerweave-runtime --bins`). Load average 2.0–3.3 during the mixed runs and
3.3–7.5 at the start of the frozen run (the build had just finished); not an idle machine, so differences under about 10 % are not
results.

## Frozen views against elk-rs and elkjs

`node benchmarks/eval/single-pass-timing.mjs --repeat 21 --bench …/bench`, medians of 21 in one process. Raw:
[single-pass-timing-20261108.json](single-pass-timing-20261108.json).

| View | default | singlePass import + refine | elk-rs | elkjs |
|---|---:|---:|---:|---:|
| layerweave-code | 1231 ms | 57.3 + 12.9 = **70 ms** | 42.7 ms | 249 ms |
| workflow-dag | 79.5 ms | 10.8 + 1.3 = **12 ms** | 8.4 ms | 59.5 ms |
| org-chart | 64.9 ms | 9.0 + 1.2 = **10 ms** | 6.9 ms | 43.1 ms |

Unchanged against 2026-11-07 (70 / 11 / 10 ms) although the hierarchical sweep runs four idle sweeps again: single pass stays
1.4–1.6 times elk-rs on these views, and 3.5–6 times faster than elkjs.

## Mixed views against the default

`bench --graph VIEW --limit 1000 --reps 5 --threads 1 --pipeline-only 1 [--import-options '{"abstractRouting":true,"singlePass":true}']`
on the unmodified datasets cut to 150 nodes by a default import, medians of 5. Raw:
[single-pass-timing-mixed-20261108.txt](single-pass-timing-mixed-20261108.txt). elk-rs gives no drawing on these views.

| View | default import + refine | singlePass import + refine |
|---|---:|---:|
| workflow-dag | 10.9 + 1625.7 = 1637 ms | 14.9 + 2.3 = **17 ms** |
| synthetic-2k (231 nodes) | 400.0 + 4773.4 = 5173 ms | 124.6 + 15.6 = **140 ms** |
| org-chart | 14.3 + 154.3 = 169 ms | 9.8 + 1.5 = **11 ms** |
| network-vlan | 5.7 + 31.2 = 37 ms | 3.1 + 0.4 = **3.5 ms** |
| cloud-architecture | 5.5 + 27.9 = 33 ms | 4.7 + 0.4 = **5.1 ms** |
| deployment-cluster | 9.5 + 46.2 = 56 ms | 6.9 + 0.6 = **7.5 ms** |
| uml-class | 3.3 + 15.0 = 18 ms | 3.0 + 0.3 = **3.3 ms** |

synthetic-2k imports in 125 ms against 471 ms on 2026-11-07 (and 400 ms for the default's import alone). On every mixed view
single pass is faster than the default's import alone except workflow-dag (14.9 against 10.9 ms); with the refinement the default
needs, single pass is 5 to 100 times faster here.
