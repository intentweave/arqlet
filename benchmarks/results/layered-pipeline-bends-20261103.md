# Consistent layered pipeline: bends, before and after, 2026-11-03

Machine: Apple M3, 8 logical CPUs, macOS, release build (`bench`, `--reps 5 --threads 1`, median), revision 37bdb10 plus
uncommitted changes. Nine views of the uniform-direction variants of the datasets (`containerDirections`, `containerStrategies`
cleared, `defaultStrategy` none), limit 150, options `{"layeredConstruction":true,"hierarchicalOrdering":true}` for import and
refinement. elk-rs (`@openedges/elk-layout` 0.12.1) on the same view, median of 5. Load averages are in the table headers
(the machine was not idle; elk-rs, measured in the same runs, is the control: 60.2 and 60.4 ms). Raw records:
`layered-pipeline-bends-before-20261103.jsonl`, `layered-pipeline-bends-after-20261103.jsonl`,
`layered-pipeline-bends-elk-20261103.jsonl` (before, then after). Crossings and bends here are `bench`'s shadow terms.

```
time-before.jsonl | 17:18  up 82 days,  5:59, 6 users, load averages: 8,14 11,40 22,22
  layerweave-code      import    712.4 ms  refine   1410.5 ms  elk-rs   28.4 ms | cross 1499 bends 1873 hard 0
  deployment-cluster   import     36.6 ms  refine     63.3 ms  elk-rs    4.8 ms | cross 94 bends 284 hard 0
  workflow-dag         import     76.0 ms  refine    145.1 ms  elk-rs    8.6 ms | cross 248 bends 632 hard 0
  org-chart            import     47.7 ms  refine     89.6 ms  elk-rs    7.0 ms | cross 30 bends 256 hard 0
  cloud-architecture   import     14.0 ms  refine     23.6 ms  elk-rs    3.6 ms | cross 20 bends 134 hard 0
  network-vlan         import     11.2 ms  refine     19.5 ms  elk-rs    2.4 ms | cross 28 bends 142 hard 0
  petri-net            import      5.4 ms  refine     10.0 ms  elk-rs    1.2 ms | cross 51 bends 106 hard 0
  uml-class            import     20.8 ms  refine     10.9 ms  elk-rs    2.0 ms | cross 5 bends 53 hard 0
  layerweave-pipeline  import     10.7 ms  refine     14.0 ms  elk-rs    2.2 ms | cross 5 bends 100 hard 0
  SUM                  import    934.9 ms  refine   1786.3 ms  elk-rs   60.2 ms | cross 1980 bends 3580
time-after2.jsonl | 17:35  up 82 days,  6:16, 6 users, load averages: 5,54 9,24 13,27
  layerweave-code      import    804.2 ms  refine   1119.8 ms  elk-rs   28.8 ms | cross 1631 bends 1541 hard 0
  deployment-cluster   import     37.2 ms  refine     54.9 ms  elk-rs    4.7 ms | cross 93 bends 272 hard 0
  workflow-dag         import     76.1 ms  refine    144.7 ms  elk-rs    8.6 ms | cross 252 bends 626 hard 0
  org-chart            import     46.9 ms  refine     85.6 ms  elk-rs    6.9 ms | cross 31 bends 214 hard 0
  cloud-architecture   import     14.1 ms  refine     21.7 ms  elk-rs    3.7 ms | cross 20 bends 128 hard 0
  network-vlan         import     11.2 ms  refine     20.0 ms  elk-rs    2.3 ms | cross 28 bends 146 hard 0
  petri-net            import      5.3 ms  refine      9.8 ms  elk-rs    1.2 ms | cross 47 bends 104 hard 0
  uml-class            import      6.6 ms  refine     45.5 ms  elk-rs    2.0 ms | cross 1 bends 53 hard 0
  layerweave-pipeline  import     10.7 ms  refine     13.6 ms  elk-rs    2.2 ms | cross 7 bends 88 hard 0
  SUM                  import   1012.2 ms  refine   1515.5 ms  elk-rs   60.4 ms | cross 2110 bends 3172
```

What changed between the two runs (only with `hierarchicalOrdering`):
- A planned port place holds only on a side across the flow it was planned for (a port on another side had taken a
  coordinate of the wrong axis; on layerweave-code two routes ran through a container and the whole drawing was routed
  again by the choice of the routing engine).
- The dummies of a chain follow one end of it (its start, or its end when the start is a border node), so that the chain
  does not interpolate between its ends and change lane in every channel.
- An edge keeps its lane through a dummy of its chain when nothing else in that rank covers the lane.

Result: bends 3580 → 3172 (−11 %); crossings 1980 → 2110 (`bench`; the scene metric of `compare.mjs` counts 1950 against
elk-rs 2031); refinement 1786 → 1516 ms; import 935 → 1012 ms (layerweave-code 712 → 804 ms: within the spread seen between
runs on this machine, 566 to 804 ms for the same code). On the 16 datasets with their own directions: hard violations
246 (layered alone 244), crossings 961 (1269), bends 1798 (1375).

Measured and not kept: dummies a track apart where the start pattern spreads far (more crossings, 2165 against 1849, for few
fewer bends); a higher placement weight for dummies (no effect); keeping a lane only when no element lies between it and
the slot (same crossings, fewer bends saved). A finding for the next step: on the same placement, the channel router drew
layerweave-code with 849 bends against 1541 here; the remaining bends come from the construction's choice of lanes, not
from the placement.
