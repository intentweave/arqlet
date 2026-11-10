# Single pass timing after the mixed-axis work, 2026-11-07

Revision `7ce851e`, Apple M3 (8 logical CPUs), plain release `bench` (no `benchmark-timings`). The machine was not idle: load 1.6 at
the start of the elk-rs comparison and 3.6 at its end, 1.7 to 1.9 for the mixed views. The figures are indicative.

## Frozen views against elk-rs

`node benchmarks/eval/single-pass-timing.mjs --repeat 21`, medians of 21 in one process. Raw:
[single-pass-timing-20261107.json](single-pass-timing-20261107.json).

| View | default | singlePass import + refine | elk-rs | elkjs |
|---|---:|---:|---:|---:|
| layerweave-code | 1200 ms | 57 + 13 = **70 ms** | 42 ms | 248 ms |
| workflow-dag | 80 ms | 10 + 1 = **11 ms** | 8.6 ms | 56 ms |
| org-chart | 65 ms | 9 + 1 = **10 ms** | 7.9 ms | 52 ms |

As on 2026-11-06 (72, 11, 10 ms): the side nodes and legs cost nothing on views of one axis. Single pass stays 1.3 to 1.7 times
elk-rs, on the simplified task, and at more crossings on the code view (3784 against 3057, seed mean).

## Mixed views against the default

`bench --graph VIEW --limit 1000 --reps 5 --threads 1 --pipeline-only 1 [--import-options '{"abstractRouting":true,"singlePass":true}']`
on the unmodified datasets cut to 150 nodes, medians of 5. Raw: [single-pass-timing-mixed-20261107.txt](single-pass-timing-mixed-20261107.txt).
elk-rs gives no drawing on these views.

| View | default import + refine | singlePass import + refine |
|---|---:|---:|
| workflow-dag | 11 + 1621 = 1632 ms | 22 + 2 = **24 ms** |
| synthetic-2k (231 nodes) | 413 + 4816 = 5229 ms | 471 + 19 = **490 ms** |
| org-chart | 14 + 143 = 156 ms | 14 + 2 = **16 ms** |
| network-vlan | 6 + 30 = 36 ms | 4 + 0.4 = **4 ms** |
| cloud-architecture | 5 + 26 = 32 ms | 6 + 0.4 = **6 ms** |

The import alone is slower than the default's on workflow-dag (22 against 11 ms) and synthetic-2k (471 against 413 ms). synthetic-2k at
231 nodes takes about half a second to import on either path; it has not been profiled on the mixed task.
