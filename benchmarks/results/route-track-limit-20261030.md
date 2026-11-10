# Route track limit sweep (`routeTrackLimit`), 2026-10-30

Plan section 3.7. One JSON line per run in `route-track-limit-<N>-20261030.jsonl` (N = 48, 24, 12, 8); the bench
command is `bench --graph benchmarks/datasets/<name>.json --limit <L> --reps 1 --threads 1` with the option set on both
`--import-options` and `--refine-options`.

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release profile, rustc 1.99.0. Revision: 0bcfd56 with uncommitted
changes (`gitDirty: true`; the change adds the option). Load average about 5 (not idle), so timings are noisy: reps 1.

Decision rule (fixed before the run): a lower N may become a default only if no dataset gains hard violations, routing
issues, shared-line pairs or routed crossings, total length rises at most 2%, and layerweave-code refine time falls at
least 30%.

Result: rejected. Crossings rise on 7–8 datasets at every N below 48, and layerweave-code refine time does not fall
(7.2 s at 48, 7.5 s at 8), because its time is in the repair grid (`grid_route`), which the limit does not change.
Default output is unchanged (17 geometry fingerprints equal the baseline, with the option absent and at 48).

## Refine time (ms, median of 1 run) and candidates

| dataset | N=48 ms | N=8 ms | N=48 legacy | N=8 legacy |
|---|---:|---:|---:|---:|
| er-diagram | 20.9 | 14.1 | 76308 | 14400 |
| petri-net | 139.8 | 100.6 | 431738 | 73800 |
| state-order | 13.7 | 10.6 | 35904 | 8400 |
| activity-order | 116.4 | 70.9 | 317258 | 48000 |
| uml-class | 39.6 | 25.3 | 124905 | 21600 |
| bpmn-pools | 32.2 | 20.1 | 132807 | 26400 |
| network-vlan | 134.1 | 94.0 | 378313 | 65800 |
| layerweave-pipeline | 80.7 | 61.7 | 259287 | 49500 |
| cloud-architecture | 54.9 | 41.5 | 147190 | 28800 |
| mixed-layouts | 74.0 | 70.8 | 46400 | 8000 |
| nested-tree-radial | 145.1 | 80.3 | 309720 | 53400 |
| tree-100 | 149.0 | 98.4 | 217332 | 39600 |
| organic-25 | 43.2 | 38.0 | 67280 | 11600 |
| organic-50 | 281.0 | 269.6 | 136880 | 23600 |
| organic-100 | 640.8 | 605.1 | 276080 | 47600 |
| organic-200 | 23595.1 | 23398.4 | 554480 | 95600 |
| layerweave-code | 7220.7 | 7489.6 | 2134400 | 368000 |

## Quality against N = 48 (regressions only)

```
N=24: quality regressions 7; max length increase 12.07%
    petri-net:cross 67.0->82.0
    activity-order:cross 70.0->90.0
    uml-class:cross 8.0->12.0
    network-vlan:cross 71.0->77.0
    layerweave-pipeline:cross 20.0->27.0
    layerweave-code:overlap 4.0->6.0
    layerweave-code:cross 1327.0->2016.0
    layerweave-code ms 7221 -> 7785, legacy 2134400 -> 1074560, cross 1327.0 -> 2016.0, length 238956 -> 267808
N=12: quality regressions 8; max length increase 11.53%
    petri-net:cross 67.0->94.0
    activity-order:cross 70.0->89.0
    uml-class:cross 8.0->12.0
    bpmn-pools:cross 8.0->11.0
    network-vlan:cross 71.0->90.0
    layerweave-pipeline:cross 20.0->38.0
    cloud-architecture:cross 35.0->41.0
    layerweave-code:cross 1327.0->2079.0
    layerweave-code ms 7221 -> 7592, legacy 2134400 -> 544640, cross 1327.0 -> 2079.0, length 238956 -> 266498
N=8: quality regressions 8; max length increase 2.28%
    petri-net:cross 67.0->102.0
    activity-order:cross 70.0->94.0
    uml-class:cross 8.0->12.0
    bpmn-pools:cross 8.0->12.0
    network-vlan:cross 71.0->92.0
    layerweave-pipeline:cross 20.0->42.0
    cloud-architecture:cross 35.0->43.0
    layerweave-code:cross 1327.0->1394.0
    layerweave-code ms 7221 -> 7490, legacy 2134400 -> 368000, cross 1327.0 -> 1394.0, length 238956 -> 237528
```

## Status

The option was removed from the code after this sweep. The change is kept as `route-track-limit-20261030.patch` against
0bcfd56, so the sweep can be reproduced. The per-run files are the raw output of that patch.
