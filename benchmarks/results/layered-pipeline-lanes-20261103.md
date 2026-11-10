# Consistent layered pipeline: lanes chosen over whole stretches, 2026-11-03

Same setup as `layered-pipeline-bends-20261103.md` (Apple M3, release `bench`, `--reps 5 --threads 1`, median; nine
uniform-direction views, limit 150; elk-rs 0.12.1 as the control). "Before" is the state of that file
(`layered-pipeline-bends-after-20261103.jsonl`), "after" this step (`layered-pipeline-lanes-20261103.jsonl`,
`layered-pipeline-lanes-elk-20261103.jsonl`). The scene-metric comparison with elk-rs: `layered-pipeline-lanes-eval-20261103.jsonl`.

```
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
time-after3.jsonl | 17:53  up 82 days,  6:33, 6 users, load averages: 5,30 4,59 6,55
  layerweave-code      import    812.6 ms  refine   1501.7 ms  elk-rs   28.8 ms | cross 1737 bends 1285 hard 0
  deployment-cluster   import     37.2 ms  refine     60.2 ms  elk-rs    4.8 ms | cross 94 bends 254 hard 0
  workflow-dag         import     76.3 ms  refine    143.3 ms  elk-rs    8.5 ms | cross 256 bends 618 hard 0
  org-chart            import     46.8 ms  refine     85.0 ms  elk-rs    6.9 ms | cross 36 bends 206 hard 0
  cloud-architecture   import     13.9 ms  refine     21.5 ms  elk-rs    3.6 ms | cross 15 bends 124 hard 0
  network-vlan         import     11.2 ms  refine     19.9 ms  elk-rs    2.3 ms | cross 32 bends 142 hard 0
  petri-net            import      5.4 ms  refine      9.7 ms  elk-rs    1.2 ms | cross 49 bends 98 hard 0
  uml-class            import      6.6 ms  refine     45.5 ms  elk-rs    2.0 ms | cross 1 bends 51 hard 0
  layerweave-pipeline  import     10.7 ms  refine     13.6 ms  elk-rs    2.3 ms | cross 7 bends 88 hard 0
  SUM                  import   1020.8 ms  refine   1900.4 ms  elk-rs   60.4 ms | cross 2227 bends 2866
```

The change (only with `hierarchicalOrdering`): between two fixed points of an edge (its ports, lanes at elements of other
areas), the lanes at the dummies of one area are chosen over the whole stretch: the edge keeps its lane while nothing else
in the rank covers it, and where it must change it takes the one of its own slots that stays free longest.

Scene metric against elk-rs (nine views): crossings 2002 / 2031, bends 2866 / 2254 (before this step: 1950 / 3172);
layerweave-code bends 1541 → 1285. Measured and not kept: offering the stretch's end lane as a candidate as well (bends 2506,
crossings 2208, above elk-rs). Time: import 1012 → 1021 ms, refinement 1516 → 1900 ms (layerweave-code 1120 → 1502 ms; the
refinement evaluates its candidates with the same construction). On the 16 datasets with their own directions: hard
violations 246 (layered alone 244), crossings 1052 (1269), bends 1660 (1375). Default output unchanged (16 of 16).
