# Constructive layered routes, step 2 (plan section 4a), 2026-10-30

Step 2 adds monotone paths over the corridors between neighbouring ranks: a route changes lane at most once per
corridor, and the cheapest such path is found by dynamic programming. Step 1 (a single lane change) is the special case.
Measured with `bench --graph <dataset> --limit <L> --reps 1 --threads 1 --constructive 1` (import only). One JSON line per
dataset in `constructive-step2-20261030.jsonl`; step 1 is `constructive-step1-20261030.jsonl`.

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release profile, rustc 1.99.0. Revision: 0bcfd56 plus uncommitted changes.
Load average about 5 (not idle); reps 1.

Not done: placement reservation. Sections do not take part in ordering or placement (no dummy nodes in `place`), so the
boxes are the same as step 1 and the corridors are whatever the placement leaves.

## Per dataset

| dataset | edges | step 1 constructed | step 2 constructed | lane changes | step 2 delegated (reasons) | hard violations import → combined | crossings combined | length combined |
|---|---:|---:|---:|---:|---|---|---:|---:|
| petri-net | 41 | 13 | 14 | 13 | 27 (PortSide 22, NoFreeTrack 5) | 0 → 0 | 96 | 14551 |
| er-diagram | 12 | 0 | 0 | 0 | 12 (ContainerEndpoint 11, SelfLoop 1) | 0 → 0 | 3 | 4873 |
| state-order | 14 | 7 | 7 | 5 | 7 (ContainerEndpoint 3, CrossesArea 1, NoFreeTrack 1, PortSide 1, SelfLoop 1) | 0 → 0 | 3 | 2741 |
| activity-order | 40 | 11 | 11 | 9 | 29 (CrossesArea 20, PortSide 9) | 0 → 0 | 116 | 19362 |
| uml-class | 24 | 0 | 0 | 0 | 24 (ContainerEndpoint 23, SelfLoop 1) | 0 → 0 | 39 | 14529 |
| bpmn-pools | 24 | 13 | 13 | 9 | 11 (CrossesArea 8, PortSide 3) | 0 → 0 | 18 | 10694 |
| network-vlan | 47 | 10 | 10 | 9 | 37 (CrossesArea 21, PortSide 16) | 0 → 0 | 66 | 26719 |
| layerweave-pipeline | 33 | 9 | 10 | 8 | 23 (CrossesArea 13, PortSide 8, NoFreeTrack 2) | 0 → 0 | 53 | 15911 |
| cloud-architecture | 32 | 6 | 7 | 7 | 25 (CrossesArea 24, PortSide 1) | 0 → 0 | 38 | 24813 |
| mixed-layouts | 16 | 5 | 5 | 2 | 11 (CrossesArea 6, NoFixedFlow 5) | 26 → 23 | 5 | 3814 |
| nested-tree-radial | 89 | 15 | 15 | 8 | 74 (NoFixedFlow 59, ContainerEndpoint 15) | 0 → 0 | 0 | 18099 |
| tree-100 | 99 | 76 | 76 | 75 | 23 (PortSide 23) | 196 → 46 | 0 | 28068 |
| organic-25 | 29 | 0 | 0 | 0 | 29 (NoFixedFlow 29) | 58 → 58 | 0 | 2751 |
| organic-50 | 59 | 0 | 0 | 0 | 59 (NoFixedFlow 59) | 118 → 118 | 2 | 5882 |
| organic-100 | 119 | 0 | 0 | 0 | 119 (NoFixedFlow 119) | 238 → 238 | 1 | 12773 |
| organic-200 | 239 | 0 | 0 | 0 | 239 (NoFixedFlow 239) | 474 → 474 | 10 | 29550 |
| layerweave-code | 184 | 26 | 23 | 22 | 161 (CrossesArea 71, PortSide 61, ContainerEndpoint 15, NoFreeTrack 11, SelfLoop 3) | 0 → 0 | 1216 | 265185 |

Total: 191 of 1101 edges constructed (step 1: 191). Lane changes: 167.

## Reading

- The gain is nil: the total stays at 191 edges. Petri-net, layerweave-pipeline and cloud-architecture gain one edge each,
  and layerweave-code loses three. Long edges are rare among the constructed ones: most routes change lane once.
- layerweave-code loses three edges because the conflict check now covers every segment, the start and end segments
  included. The cheapest path ignores the conflicts, so a conflicting path is not replaced by another one. NoFreeTrack
  rises from 8 to 11 there.
- The limit is still before routing: port sides (PortSide), endpoints in different areas (CrossesArea), container
  endpoints, and automatic flow. The corridor search cannot reach those edges. Step 3 (port and track order) and
  the area rule for cross-area edges are the next levers.
- Time: the construction stays in the sub-millisecond to 0.3 ms range per dataset (see the jsonl).
