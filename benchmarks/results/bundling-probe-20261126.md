# ADR 0028 step B0: hierarchical edge bundling, probed outside the engine

Data: `benchmarks/results/bundling-probe-20261126.jsonl`. Machine: Apple M3, 8 logical CPUs, 16 GiB, darwin 25.0.0 arm64.
Revision `10cb560038e7fa7393a00cfcc5ccd8329372d6c2`; `cargo build --release -p arqlet-runtime --bin layout-command --locked`; rustc 1.99.0 (b940084d7 2026-09-28); Node v24.21.0.
Command: `node benchmarks/eval/bundling-probe.mjs --out benchmarks/results/bundling-probe-20261126.jsonl --drawings /tmp/arqlet-bundling-b0/final/drawings`.
No timing is reported: the measures are geometric and do not depend on the load.

## Views

| view | nodes | routes | containers | depth | eligible | not eligible, by first reason | eligible routes through a leaf |
|---|---|---|---|---|---|---|---|
| org-chart | 104 | 82 | 33 | 3 | 74 | detoured 8 | 0 |
| layerweave-code | 148 | 239 | 21 | 2 | 20 | selfLoop 4, detoured 101, sameContainer 114 | 0 |
| mixed-large | 96 | 95 | 20 | 2 | 59 | detoured 29, sameContainer 7 | 0 |
| workflow-dag | 72 | 125 | 8 | 1 | 90 | detoured 35 | 6 |
| phase-width-feedback | 188 | 358 | 38 | 2 | 68 | selfLoop 8, detoured 163, sameContainer 119 | 1 |
| synthetic-2k | 2286 | 5824 | 286 | 3 | 1373 | selfLoop 302, detoured 3231, sameContainer 918 | 628 |

## What the hierarchies offer

A section of a cubic spline is the same for two edges only where they share four consecutive control points. "Companion": another eligible edge between the same two outermost boxes.

| view | eligible | control points per edge (points: edges) | with a companion | with a shared section | containers on paths | centre covered by a child | free place differs from centre |
|---|---|---|---|---|---|---|---|
| org-chart | 74 | 3: 65, 6: 5, 8: 4 | 55 | 2 | 33 | 11 | 11 |
| layerweave-code | 20 | 4: 6, 6: 14 | 14 | 8 | 16 | 7 | 8 |
| mixed-large | 59 | 3: 10, 4: 22, 5: 14, 6: 13 | 35 | 4 | 19 | 2 | 2 |
| workflow-dag | 90 | 4: 90 | 89 | 0 | 8 | 1 | 2 |
| phase-width-feedback | 68 | 4: 29, 6: 39 | 56 | 24 | 30 | 14 | 15 |
| synthetic-2k | 1373 | 4: 712, 6: 204, 8: 457 | 1207 | 591 | 286 | 36 | 44 |

## Gate (strength 0.85, with the guard)

Pass: 50 eligible edges or more, bundled ≥ 60 %, no new entry, ink ≤ 0.85, mean distortion ≤ 1.2. Ink: eligible edges, 2-unit raster. Distortion: all eligible edges.

| view | control points | eligible | full | weakened | kept | bundled | new entries (probe / engine) | ink | distortion mean / median / max | verdict |
|---|---|---|---|---|---|---|---|---|---|---|
| org-chart | centre | 74 | 51 | 13 | 10 | 86 % | 0 / 0 | 1.031 | 1.113 / 1.014 / 2.40 | fail: ink |
| org-chart | free | 74 | 53 | 11 | 10 | 86 % | 0 / 0 | 1.028 | 1.113 / 1.012 / 2.40 | fail: ink |
| org-chart | gate | 74 | 62 | 5 | 7 | 91 % | 0 / 0 | 1.004 | 1.002 / 1.000 / 1.11 | fail: ink |
| layerweave-code | centre | 20 | 1 | 11 | 8 | 60 % | 0 / 0 | 1.016 | 1.039 / 1.000 / 1.35 | fail: fewer than 50 eligible, ink |
| layerweave-code | free | 20 | 0 | 14 | 6 | 70 % | 0 / 0 | 1.015 | 1.055 / 1.002 / 1.42 | fail: fewer than 50 eligible, ink |
| layerweave-code | gate | 20 | 11 | 3 | 6 | 70 % | 0 / 0 | 1.033 | 1.038 / 1.000 / 1.36 | fail: fewer than 50 eligible, ink |
| mixed-large | centre | 59 | 29 | 22 | 8 | 86 % | 0 / 0 | 1.041 | 1.135 / 1.012 / 3.88 | fail: ink |
| mixed-large | free | 59 | 29 | 23 | 7 | 88 % | 0 / 0 | 1.041 | 1.137 / 1.012 / 3.82 | fail: ink |
| mixed-large | gate | 59 | 32 | 16 | 11 | 81 % | 0 / 0 | 1.013 | 1.022 / 1.003 / 1.26 | fail: ink |
| workflow-dag | centre | 90 | 19 | 37 | 34 | 62 % | 0 / 0 | 1.051 | 1.135 / 1.002 / 2.63 | fail: ink |
| workflow-dag | free | 90 | 19 | 37 | 34 | 62 % | 0 / 0 | 1.048 | 1.135 / 1.001 / 2.63 | fail: ink |
| workflow-dag | gate | 90 | 26 | 30 | 34 | 62 % | 0 / 0 | 1.023 | 1.078 / 1.002 / 2.45 | fail: ink |
| phase-width-feedback | centre | 68 | 20 | 38 | 10 | 85 % | 0 / 0 | 1.017 | 1.040 / 1.012 / 1.35 | fail: ink |
| phase-width-feedback | free | 68 | 23 | 36 | 9 | 87 % | 0 / 0 | 1.021 | 1.042 / 1.012 / 1.34 | fail: ink |
| phase-width-feedback | gate | 68 | 42 | 18 | 8 | 88 % | 0 / 0 | 1.011 | 1.032 / 1.024 / 1.21 | fail: ink |
| synthetic-2k | centre | 1373 | 240 | 328 | 805 | 41 % | 0 / 0 | 1.003 | 1.052 / 1.000 / 3.96 | fail: bundled share, ink |
| synthetic-2k | free | 1373 | 238 | 333 | 802 | 42 % | 0 / 0 | 1.003 | 1.052 / 1.000 / 3.96 | fail: bundled share, ink |
| synthetic-2k | gate | 1373 | 358 | 239 | 776 | 43 % | 0 / 0 | 1.001 | 1.021 / 1.000 / 2.99 | fail: bundled share, ink |

| control points | gate views passing | go |
|---|---|---|
| centre | 0 of 5 with 50+ eligible | **no-go** |
| free | 0 of 5 with 50+ eligible | **no-go** |
| gate | 0 of 5 with 50+ eligible | **no-go** |

## Other readings of the gate measures (they decide nothing)

| view | control points | ink, eligible: 4 / 8 / 16 / 32 / screen cell | ink, all routes: 2 / 4 / 8 / screen | distortion of the bundled only, mean | refused at full strength | points, bundled / unbundled |
|---|---|---|---|---|---|---|
| org-chart | centre | 1.021 / 1.002 / 0.975 / 0.949 / 1.018 (4.7 u) | 1.023 / 1.016 / 1.002 / 1.014 | 1.131 | node 18, end 2, clearance 3 | 2369 / 148 |
| org-chart | free | 1.019 / 0.999 / 0.973 / 0.950 / 1.016 (4.7 u) | 1.021 / 1.014 / 0.997 / 1.009 | 1.131 | node 18, end 2, clearance 1 | 2367 / 148 |
| org-chart | gate | 1.002 / 0.999 / 0.990 / 0.975 / 1.002 (4.7 u) | 1.003 / 1.002 / 0.999 / 1.001 | 1.003 | clearance 3, node 9 | 2388 / 148 |
| layerweave-code | centre | 1.012 / 1.006 / 1.000 / 0.992 / 1.007 (7.4 u) | 1.002 / 1.000 / 0.998 / 0.999 | 1.066 | node 15, clearance 2, end 2 | 560 / 40 |
| layerweave-code | free | 1.003 / 0.995 / 0.985 / 0.965 / 0.997 (7.4 u) | 1.001 / 0.999 / 0.996 / 0.997 | 1.078 | node 16, clearance 2, end 2 | 648 / 40 |
| layerweave-code | gate | 1.028 / 1.021 / 1.017 / 0.999 / 1.024 (7.4 u) | 1.004 / 1.003 / 1.001 / 1.002 | 1.054 | node 8, clearance 1 | 657 / 40 |
| mixed-large | centre | 1.039 / 1.027 / 1.025 / 1.022 / 1.038 (4.1 u) | 1.027 / 1.026 / 1.020 / 1.025 | 1.157 | clearance 2, node 26, end 2 | 2297 / 118 |
| mixed-large | free | 1.037 / 1.025 / 1.020 / 1.012 / 1.037 (4.1 u) | 1.026 / 1.025 / 1.019 / 1.024 | 1.156 | node 27, end 2, clearance 1 | 2344 / 118 |
| mixed-large | gate | 1.013 / 1.002 / 0.996 / 0.986 / 1.012 (4.1 u) | 1.009 / 1.010 / 1.005 / 1.009 | 1.027 | clearance 4, node 23 | 2177 / 118 |
| workflow-dag | centre | 1.036 / 1.011 / 0.983 / 0.960 / 1.036 (4.0 u) | 1.033 / 1.021 / 1.003 / 1.021 | 1.218 | node 64, clearance 5, end 2 | 2342 / 180 |
| workflow-dag | free | 1.031 / 1.003 / 0.976 / 0.957 / 1.032 (4.0 u) | 1.030 / 1.016 / 0.995 / 1.017 | 1.216 | node 66, clearance 3, end 2 | 2336 / 180 |
| workflow-dag | gate | 1.007 / 0.978 / 0.943 / 0.926 / 1.008 (4.0 u) | 1.013 / 1.002 / 0.983 / 1.003 | 1.125 | node 58, clearance 5, end 1 | 2445 / 180 |
| phase-width-feedback | centre | 1.010 / 0.999 / 0.980 / 0.958 / 1.002 (6.6 u) | 1.007 / 1.006 / 1.003 / 1.003 | 1.047 | node 40, end 4, clearance 4 | 2634 / 136 |
| phase-width-feedback | free | 1.014 / 1.005 / 0.993 / 0.969 / 1.007 (6.6 u) | 1.009 / 1.009 / 1.007 / 1.008 | 1.049 | node 37, end 3, clearance 5 | 2686 / 136 |
| phase-width-feedback | gate | 0.996 / 0.962 / 0.908 / 0.866 / 0.974 (6.6 u) | 1.007 / 1.004 / 0.995 / 0.998 | 1.036 | node 21, clearance 5 | 2770 / 136 |
| synthetic-2k | centre | 1.003 / 1.000 / 0.999 / 1.000 / 0.999 (16.5 u) | 1.002 / 1.001 / 1.000 / 0.999 | 1.125 | node 1005, clearance 77, end 51 | 23822 / 2746 |
| synthetic-2k | free | 1.003 / 1.000 / 0.999 / 1.000 / 0.999 (16.5 u) | 1.002 / 1.001 / 1.000 / 0.999 | 1.125 | node 995, clearance 87, end 53 | 23940 / 2746 |
| synthetic-2k | gate | 1.000 / 0.998 / 0.996 / 0.997 / 0.996 (16.5 u) | 1.000 / 1.000 / 0.998 / 0.997 | 1.048 | node 898, clearance 117 | 26965 / 2746 |

## Control: a deep hierarchy on a circle (synthetic, no engine, no guard)

128 leaves on a circle in 4 × 4 groups, 200 relations between groups, centre control points. It is not a gate view; it shows what the ink measure reaches where the method has what it was made for.

| strength | edges with a shared section | ink: 2 / 4 / 8 / 16 / 32 / screen (1.3 u) | distortion, mean |
|---|---|---|---|
| 0.5 | 175 of 200 | 1.047 / 1.027 / 1.004 / 0.983 / 0.989 / 1.056 | 1.173 |
| 0.85 | 175 of 200 | 1.028 / 0.952 / 0.867 / 0.814 / 0.845 / 1.068 | 1.384 |
| 1 | 175 of 200 | 0.492 / 0.451 / 0.444 / 0.510 / 0.639 / 0.522 | 1.485 |

## Strengths, with and without the guard

Without the guard every curve is drawn at the full strength through whatever it meets. It is not the design; it shows what the guard costs and what the method could reach.

| view | control points | strength | bundled, guard | ink, guard (2 / screen) | distortion, guard | ink, no guard (2 / screen) | distortion, no guard | no-guard curves the guard would refuse (hit or clearance) |
|---|---|---|---|---|---|---|---|---|
| org-chart | centre | 0.5 | 92 % | 1.010 / 1.009 | 1.031 | 1.026 / 1.024 | 1.046 | 16 of 74 |
| org-chart | centre | 0.85 | 86 % | 1.031 / 1.018 | 1.113 | 1.146 / 1.123 | 1.195 | 23 of 74 |
| org-chart | centre | 1 | 85 % | 1.032 / 1.013 | 1.140 | 1.163 / 1.116 | 1.300 | 27 of 74 |
| org-chart | free | 0.5 | 92 % | 1.009 / 1.009 | 1.029 | 1.028 / 1.026 | 1.045 | 16 of 74 |
| org-chart | free | 0.85 | 86 % | 1.028 / 1.016 | 1.113 | 1.149 / 1.123 | 1.197 | 21 of 74 |
| org-chart | free | 1 | 85 % | 1.049 / 1.029 | 1.156 | 1.160 / 1.113 | 1.272 | 22 of 74 |
| org-chart | gate | 0.5 | 95 % | 0.993 / 0.993 | 0.993 | 0.995 / 0.996 | 0.994 | 11 of 74 |
| org-chart | gate | 0.85 | 91 % | 1.004 / 1.002 | 1.002 | 1.011 / 1.008 | 1.010 | 12 of 74 |
| org-chart | gate | 1 | 89 % | 1.001 / 0.997 | 1.008 | 1.000 / 0.992 | 1.021 | 13 of 74 |
| layerweave-code | centre | 0.5 | 80 % | 1.023 / 1.007 | 1.047 | 1.065 / 1.054 | 1.074 | 9 of 20 |
| layerweave-code | centre | 0.85 | 60 % | 1.016 / 1.007 | 1.039 | 1.194 / 1.157 | 1.258 | 19 of 20 |
| layerweave-code | centre | 1 | 60 % | 1.022 / 1.005 | 1.048 | 0.975 / 0.941 | 1.354 | 19 of 20 |
| layerweave-code | free | 0.5 | 75 % | 1.043 / 1.041 | 1.061 | 1.130 / 1.115 | 1.137 | 12 of 20 |
| layerweave-code | free | 0.85 | 70 % | 1.015 / 0.997 | 1.055 | 1.325 / 1.269 | 1.394 | 20 of 20 |
| layerweave-code | free | 1 | 60 % | 1.041 / 1.038 | 1.062 | 1.078 / 1.032 | 1.527 | 19 of 20 |
| layerweave-code | gate | 0.5 | 70 % | 1.009 / 1.002 | 1.018 | 1.050 / 1.045 | 1.032 | 9 of 20 |
| layerweave-code | gate | 0.85 | 70 % | 1.033 / 1.024 | 1.038 | 1.097 / 1.066 | 1.089 | 9 of 20 |
| layerweave-code | gate | 1 | 65 % | 0.984 / 0.971 | 1.048 | 0.953 / 0.939 | 1.120 | 10 of 20 |
| mixed-large | centre | 0.5 | 85 % | 1.026 / 1.023 | 1.053 | 1.055 / 1.049 | 1.127 | 26 of 59 |
| mixed-large | centre | 0.85 | 86 % | 1.041 / 1.038 | 1.135 | 1.154 / 1.139 | 1.327 | 30 of 59 |
| mixed-large | centre | 1 | 78 % | 1.044 / 1.037 | 1.189 | 1.140 / 1.116 | 1.446 | 37 of 59 |
| mixed-large | free | 0.5 | 85 % | 1.026 / 1.023 | 1.053 | 1.055 / 1.049 | 1.127 | 26 of 59 |
| mixed-large | free | 0.85 | 88 % | 1.041 / 1.037 | 1.137 | 1.153 / 1.138 | 1.325 | 30 of 59 |
| mixed-large | free | 1 | 78 % | 1.045 / 1.037 | 1.188 | 1.140 / 1.117 | 1.444 | 37 of 59 |
| mixed-large | gate | 0.5 | 85 % | 1.005 / 1.003 | 1.012 | 1.037 / 1.034 | 1.070 | 24 of 59 |
| mixed-large | gate | 0.85 | 81 % | 1.013 / 1.012 | 1.022 | 1.100 / 1.091 | 1.172 | 27 of 59 |
| mixed-large | gate | 1 | 81 % | 1.012 / 1.007 | 1.035 | 1.074 / 1.059 | 1.223 | 24 of 59 |
| workflow-dag | centre | 0.5 | 71 % | 1.028 / 1.013 | 1.099 | 1.095 / 1.073 | 1.227 | 58 of 90 |
| workflow-dag | centre | 0.85 | 62 % | 1.051 / 1.036 | 1.135 | 1.200 / 1.122 | 1.513 | 71 of 90 |
| workflow-dag | centre | 1 | 58 % | 1.038 / 1.021 | 1.121 | 1.139 / 1.039 | 1.651 | 75 of 90 |
| workflow-dag | free | 0.5 | 72 % | 1.028 / 1.013 | 1.099 | 1.094 / 1.072 | 1.226 | 58 of 90 |
| workflow-dag | free | 0.85 | 62 % | 1.048 / 1.032 | 1.135 | 1.198 / 1.122 | 1.513 | 71 of 90 |
| workflow-dag | free | 1 | 58 % | 1.036 / 1.022 | 1.118 | 1.139 / 1.040 | 1.651 | 77 of 90 |
| workflow-dag | gate | 0.5 | 72 % | 1.013 / 1.000 | 1.072 | 1.040 / 1.017 | 1.133 | 48 of 90 |
| workflow-dag | gate | 0.85 | 62 % | 1.023 / 1.008 | 1.078 | 1.083 / 1.015 | 1.277 | 64 of 90 |
| workflow-dag | gate | 1 | 61 % | 0.997 / 0.976 | 1.084 | 0.973 / 0.894 | 1.349 | 65 of 90 |
| phase-width-feedback | centre | 0.5 | 91 % | 1.008 / 0.996 | 1.022 | 1.014 / 1.004 | 1.029 | 17 of 68 |
| phase-width-feedback | centre | 0.85 | 85 % | 1.017 / 1.002 | 1.040 | 1.084 / 1.001 | 1.131 | 48 of 68 |
| phase-width-feedback | centre | 1 | 85 % | 1.018 / 1.006 | 1.047 | 0.638 / 0.571 | 1.223 | 52 of 68 |
| phase-width-feedback | free | 0.5 | 93 % | 1.008 / 0.995 | 1.020 | 1.013 / 1.002 | 1.027 | 15 of 68 |
| phase-width-feedback | free | 0.85 | 87 % | 1.021 / 1.007 | 1.042 | 1.076 / 0.995 | 1.128 | 45 of 68 |
| phase-width-feedback | free | 1 | 87 % | 1.015 / 1.001 | 1.048 | 0.637 / 0.570 | 1.219 | 52 of 68 |
| phase-width-feedback | gate | 0.5 | 91 % | 1.001 / 0.989 | 1.010 | 1.006 / 0.996 | 1.012 | 15 of 68 |
| phase-width-feedback | gate | 0.85 | 88 % | 1.011 / 0.974 | 1.032 | 1.007 / 0.921 | 1.044 | 26 of 68 |
| phase-width-feedback | gate | 1 | 87 % | 0.910 / 0.890 | 1.036 | 0.553 / 0.512 | 1.063 | 36 of 68 |
| synthetic-2k | centre | 0.5 | 46 % | 1.002 / 0.998 | 1.026 | 1.021 / 0.941 | 1.082 | 981 of 1373 |
| synthetic-2k | centre | 0.85 | 41 % | 1.003 / 0.999 | 1.052 | 1.062 / 0.801 | 1.234 | 1133 of 1373 |
| synthetic-2k | centre | 1 | 40 % | 1.004 / 1.001 | 1.054 | 0.520 / 0.369 | 1.333 | 1182 of 1373 |
| synthetic-2k | free | 0.5 | 46 % | 1.002 / 0.997 | 1.027 | 1.021 / 0.940 | 1.083 | 983 of 1373 |
| synthetic-2k | free | 0.85 | 42 % | 1.003 / 0.999 | 1.052 | 1.065 / 0.807 | 1.237 | 1135 of 1373 |
| synthetic-2k | free | 1 | 40 % | 1.004 / 1.001 | 1.055 | 0.525 / 0.373 | 1.336 | 1183 of 1373 |
| synthetic-2k | gate | 0.5 | 46 % | 1.000 / 0.997 | 1.011 | 0.998 / 0.946 | 1.023 | 910 of 1373 |
| synthetic-2k | gate | 0.85 | 43 % | 1.001 / 0.996 | 1.021 | 0.982 / 0.848 | 1.059 | 1015 of 1373 |
| synthetic-2k | gate | 1 | 42 % | 1.001 / 0.997 | 1.025 | 0.556 / 0.480 | 1.079 | 1046 of 1373 |
