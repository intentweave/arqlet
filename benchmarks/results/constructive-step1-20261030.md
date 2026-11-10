# Constructive layered routes, step 1 (plan section 4a), 2026-10-30

Measured with `bench --graph <dataset> --limit <L> --reps 1 --threads 1 --constructive 1` on the import drawing (no
refinement). One JSON line per dataset in `constructive-step1-20261030.jsonl`. The construction is in
`crates/layout/src/constructive.rs` (uncommitted at the time of the run).

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release profile, rustc 1.99.0. Revision: 0bcfd56 plus uncommitted changes
(the construction and the bench mode). Load average about 5 (not idle); reps 1.

Scope: an edge is constructed when both endpoints are leaves of one area with a fixed Lr or Tb flow, the target lies
after the source, both ports are on the flow-facing sides (right and left for Lr, bottom and top for Tb), and a track
exists in the corridors that keeps the margin (required clearance plus one track) from every other box and the track
spacing from the tracks already assigned. Every other edge keeps its import route and is counted as delegated.
`edges` is the imported view (layerweave-code: 184 of its 1077 graph edges, at limit 100).


## Acceptance numbers

| dataset | edges | constructed | delegated (reasons) | import ms | construct ms | hard violations import → combined |
|---|---:|---:|---|---:|---:|---:|
| petri-net | 41 | 13 | 28 (PortSide 22, NoFreeTrack 6) | 5.7 | 0.05 | 0 → 0 |
| er-diagram | 12 | 0 | 12 (ContainerEndpoint 11, SelfLoop 1) | 2.0 | 0.01 | 0 → 0 |
| state-order | 14 | 7 | 7 (ContainerEndpoint 3, CrossesArea 1, NoFreeTrack 1, PortSide 1, SelfLoop 1) | 0.2 | 0.02 | 0 → 0 |
| activity-order | 40 | 11 | 29 (CrossesArea 20, PortSide 9) | 3.9 | 0.04 | 0 → 0 |
| uml-class | 24 | 0 | 24 (ContainerEndpoint 23, SelfLoop 1) | 4.0 | 0.01 | 0 → 0 |
| bpmn-pools | 24 | 13 | 11 (CrossesArea 8, PortSide 3) | 1.5 | 0.04 | 0 → 0 |
| network-vlan | 47 | 10 | 37 (CrossesArea 21, PortSide 16) | 5.4 | 0.04 | 0 → 0 |
| layerweave-pipeline | 33 | 9 | 24 (CrossesArea 13, PortSide 8, NoFreeTrack 3) | 3.4 | 0.04 | 0 → 0 |
| cloud-architecture | 32 | 6 | 26 (CrossesArea 24, NoFreeTrack 1, PortSide 1) | 3.0 | 0.06 | 0 → 0 |
| mixed-layouts | 16 | 5 | 11 (CrossesArea 6, NoFixedFlow 5) | 16.8 | 0.01 | 26 → 23 |
| nested-tree-radial | 89 | 15 | 74 (NoFixedFlow 59, ContainerEndpoint 15) | 5.1 | 0.10 | 0 → 0 |
| tree-100 | 99 | 76 | 23 (PortSide 23) | 6.2 | 0.31 | 196 → 46 |
| organic-25 | 29 | 0 | 29 (NoFixedFlow 29) | 8.1 | 0.01 | 58 → 58 |
| organic-50 | 59 | 0 | 59 (NoFixedFlow 59) | 62.6 | 0.02 | 118 → 118 |
| organic-100 | 119 | 0 | 119 (NoFixedFlow 119) | 131.0 | 0.03 | 238 → 238 |
| organic-200 | 239 | 0 | 239 (NoFixedFlow 239) | 5831.5 | 0.07 | 474 → 474 |
| layerweave-code | 184 | 26 | 158 (CrossesArea 71, PortSide 61, ContainerEndpoint 15, NoFreeTrack 8, SelfLoop 3) | 133.8 | 0.27 | 0 → 0 |

Total: 191 of 1101 edges constructed without the grid search.

## Quality of the combined drawing (constructed routes in place, delegated routes unchanged)

| dataset | crossings import → combined | length import → combined | shared pairs import → combined |
|---|---:|---:|---:|
| petri-net | 94 → 94 | 14545 → 14545 | 48 → 48 |
| er-diagram | 3 → 3 | 4873 → 4873 | 8 → 8 |
| state-order | 3 → 3 | 2741 → 2741 | 1 → 1 |
| activity-order | 116 → 116 | 19362 → 19362 | 50 → 50 |
| uml-class | 39 → 39 | 14529 → 14529 | 6 → 6 |
| bpmn-pools | 18 → 18 | 10694 → 10694 | 5 → 4 |
| network-vlan | 66 → 66 | 26719 → 26719 | 56 → 56 |
| layerweave-pipeline | 45 → 45 | 15773 → 15773 | 32 → 32 |
| cloud-architecture | 35 → 35 | 24777 → 24777 | 19 → 19 |
| mixed-layouts | 5 → 5 | 3776 → 3814 | 0 → 0 |
| nested-tree-radial | 0 → 0 | 18099 → 18099 | 0 → 0 |
| tree-100 | 0 → 0 | 24768 → 28068 | 0 → 0 |
| organic-25 | 0 → 0 | 2751 → 2751 | 0 → 0 |
| organic-50 | 2 → 2 | 5882 → 5882 | 0 → 0 |
| organic-100 | 1 → 1 | 12773 → 12773 | 0 → 0 |
| organic-200 | 10 → 10 | 29550 → 29550 | 0 → 0 |
| layerweave-code | 1187 → 1222 | 265149 → 265149 | 978 → 953 |

## Reading

- The construction costs at most 0.31 ms per dataset, against 1–5 ms of import on the layered sets. It is not the
  bottleneck.
- No dataset gains a hard violation. Two lose some: tree-100 (196 → 46) and mixed-layouts (26 → 23).
- Length rises on tree-100 (+13%) and mixed-layouts (+1%): the first construction takes the lane nearest to the
  middle, not the shortest one. This is for step 3 and 4 to address.
- Crossings rise on layerweave-code (1187 → 1222) and fall in shared-line pairs (978 → 953).
- The main limit is not the track search. Most delegations are structural: an endpoint is a container (er-diagram,
  uml-class), the endpoints lie in different areas (activity-order, network-vlan, cloud-architecture), or the area's
  flow is `auto` (organic, nested-tree-radial). The automatic ports put 22 of 41 petri-net edges on the top or bottom,
  so they are `PortSide` delegations. Step 3 (joint port and track order) addresses that.
- Not measured here: refinement, and the repair pops of the delegated edges (the import stage does not report them in
  this mode). The acceptance question "how many repair searches remain" is therefore answered only by the delegation
  count.
