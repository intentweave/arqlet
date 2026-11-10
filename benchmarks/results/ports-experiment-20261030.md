# Port placement experiment, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, revision 0bcfd56 plus uncommitted changes, load about 5 (not idle).
Views: `--limit 100`, refinement with one thread. Metrics from the shared metric code (`compare.mjs`). Script: `port-experiment.mjs`.
"At shared endpoints": crossings between two routes that share an endpoint node (reported separately by the metric).

| dataset | symmetric (default): crossings / at shared endpoints | angular | optimized |
|---|---:|---:|---:|
| network-vlan | 31 / 40 | 35 / 21 | 31 / 40 |
| activity-order | 35 / 35 | 30 / 21 | 24 / 26 |
| petri-net | 52 / 15 | 61 / 17 | 49 / 16 |
| layerweave-pipeline | 16 / 4 | 15 / 5 | 16 / 4 |
| cloud-architecture | 29 / 6 | 27 / 7 | 29 / 6 |
| bpmn-pools | 7 / 1 | 7 / 1 | 7 / 1 |
| uml-class | 6 / 2 | 6 / 2 | 5 / 2 |

Reading: no placement removes the shared-endpoint crossings everywhere. Angular lowers them on network-vlan (40 → 21) but
raises the other crossings there; optimized lowers both on activity-order (35 → 24 total). The defaults are not changed by this measurement.
