# Stage sweep: import and refinement, 2026-10-30 (all stages)

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release profile, rustc 1.99.0. Revision: 0bcfd56 plus uncommitted changes.
Load average about 5 before the runs (the largest CPU users were under 10 %, so the machine was quiet but not idle). Bench built
with `--features benchmark-timings`; medians of 3 timed runs after a warm-up.

Bench, threads 1, limit 100 (tree-100 and organic sets: 1000). Per dataset in `stage-sweep-20261030.jsonl`. Stage times in ms;
"sum" is the sum of the timed stages. The stages overlap (port resolution is also timed inside repair), so the sum is not a
breakdown of the total; for organic-200 it is larger than the total. Scoring is only in refinement. On the organic sets the
time is in repair and port resolution, not in the organic stress or separation (those are 0.01–0.2 % of the total on organic-200).

| dataset | edges | import total | import: placement | import: organicStress | import: separation | import: corridorGaps | import: portResolution | import: routing | import: repair | import sum | refinement total | ref: placement | ref: organicStress | ref: separation | ref: corridorGaps | ref: portResolution | ref: routing | ref: repair | scoring |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| petri-net | 41 | 8.9 | 0.2 | 0.0 | 0.0 | 0.3 | 0.3 | 7.8 | 0.1 | 8.7 | 138.5 | 1.5 | 0.0 | 0.0 | 0.8 | 0.6 | 120.4 | 0.2 | 5.8 |
| er-diagram | 12 | 1.9 | 0.1 | 0.0 | 0.0 | 0.1 | 0.0 | 1.2 | 0.1 | 1.5 | 22.3 | 0.7 | 0.0 | 0.0 | 0.2 | 0.1 | 14.9 | 0.3 | 2.3 |
| state-order | 14 | 0.2 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 0.1 | 0.0 | 0.1 | 13.7 | 0.0 | 0.0 | 0.0 | 0.0 | 0.0 | 5.3 | 0.0 | 0.5 |
| activity-order | 40 | 3.7 | 0.1 | 0.0 | 0.0 | 0.1 | 0.1 | 3.0 | 0.1 | 3.4 | 119.3 | 1.1 | 0.0 | 0.0 | 0.7 | 0.6 | 103.6 | 0.3 | 5.1 |
| uml-class | 24 | 3.7 | 0.1 | 0.0 | 0.0 | 0.1 | 0.1 | 2.9 | 0.1 | 3.4 | 39.9 | 0.5 | 0.0 | 0.0 | 0.3 | 0.2 | 27.5 | 0.3 | 4.0 |
| bpmn-pools | 24 | 1.4 | 0.0 | 0.0 | 0.0 | 0.0 | 0.1 | 1.0 | 0.0 | 1.2 | 32.4 | 0.3 | 0.0 | 0.0 | 0.2 | 0.2 | 26.1 | 0.2 | 2.1 |
| network-vlan | 47 | 5.1 | 0.1 | 0.0 | 0.0 | 0.2 | 0.2 | 4.4 | 0.1 | 4.9 | 135.2 | 1.0 | 0.0 | 0.0 | 0.9 | 0.6 | 107.8 | 0.3 | 9.3 |
| layerweave-pipeline | 33 | 3.3 | 0.1 | 0.0 | 0.0 | 0.1 | 0.1 | 2.6 | 0.1 | 2.9 | 82.0 | 0.7 | 0.0 | 0.0 | 0.6 | 0.5 | 69.1 | 0.3 | 4.3 |
| cloud-architecture | 32 | 2.8 | 0.1 | 0.0 | 0.0 | 0.1 | 0.1 | 2.1 | 0.1 | 2.4 | 55.3 | 0.3 | 0.0 | 0.0 | 0.3 | 0.3 | 39.8 | 0.3 | 5.5 |
| mixed-layouts | 16 | 16.9 | 0.1 | 0.0 | 0.0 | 0.0 | 10.2 | 0.4 | 16.1 | 26.8 | 75.0 | 0.5 | 0.1 | 0.1 | 0.0 | 41.1 | 7.7 | 64.3 | 1.0 |
| nested-tree-radial | 89 | 4.7 | 0.2 | 0.0 | 0.0 | 0.2 | 0.1 | 0.5 | 0.1 | 1.1 | 146.0 | 1.1 | 0.0 | 0.0 | 0.8 | 0.6 | 116.6 | 0.6 | 6.9 |
| tree-100 | 99 | 6.0 | 0.7 | 0.0 | 0.0 | 0.0 | 0.1 | 0.4 | 0.1 | 1.3 | 150.9 | 6.9 | 0.0 | 0.0 | 0.0 | 1.5 | 89.2 | 0.3 | 28.4 |
| organic-25 | 29 | 8.0 | 1.2 | 0.3 | 0.7 | 0.0 | 3.9 | 0.2 | 5.9 | 12.3 | 43.6 | 4.8 | 1.4 | 2.9 | 0.0 | 15.8 | 10.1 | 23.8 | 2.3 |
| organic-50 | 59 | 62.2 | 3.6 | 1.5 | 1.6 | 0.0 | 33.1 | 0.8 | 56.1 | 96.6 | 285.4 | 14.4 | 5.8 | 6.3 | 0.0 | 131.8 | 30.0 | 223.7 | 8.7 |
| organic-100 | 119 | 130.3 | 10.1 | 5.6 | 3.6 | 0.0 | 53.5 | 3.1 | 111.4 | 187.3 | 649.6 | 40.8 | 22.5 | 14.6 | 0.0 | 217.9 | 94.9 | 447.6 | 33.9 |
| layerweave-code | 184 | 132.8 | 0.9 | 0.0 | 0.0 | 2.2 | 1.3 | 122.5 | 0.7 | 127.5 | 7283.2 | 7.6 | 0.0 | 0.0 | 12.5 | 5.3 | 6823.9 | 2.8 | 161.0 |
| organic-200 | 239 | 5772.3 | 34.4 | 22.0 | 9.0 | 0.0 | 1827.5 | 3.8 | 5692.0 | 7588.8 | 23505.3 | 137.7 | 87.8 | 35.9 | 0.0 | 7322.7 | 361.8 | 22762.0 | 129.7 |
