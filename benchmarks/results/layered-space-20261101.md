# Layered construction: space (channel widening removed), 2026-11-01

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision 4c0e3e1 plus
uncommitted changes (behind `layeredConstruction`; default output unchanged, 16 of 16 refinement fingerprints).
`bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000); layerweave-code is the graph of 0bcfd56
(669 / 1084). Area: width x height of the root area of the refined drawing (`layout-command` import and refine, one thread).
Raw rows: `layered-space-20261101.jsonl`; areas of all variants: `layered-space-20261101-areas.json`. "before" is
`layered-one-candidate-20261101.jsonl` (ea487b0); "off" is the existing path. The machine was not idle (one refinement time of
petri-net, 340 ms, was an outlier; measured again: 33 ms).

Changes (kept):
- The channel widening of step 4 is removed: it gave every lane change of a channel between two ranks its own track and moved
  the later ranks on. Without it, layerweave-code is 20 % narrower and the crossings stay as they are.
- Lane changes of a channel whose spans across the flow do not overlap share a track (`constructive::track_slots`), in the
  construction from sections and in the corridor order of searched edges.

Measured on the way (layerweave-code, deployment-cluster; one run each unless stated):

| variant | area layerweave-code | crossings layerweave-code | area deployment-cluster | crossings deployment-cluster |
|---|---:|---:|---:|---:|
| before (ea487b0) | 21.2 M | 953 | 4.50 M | 117 |
| border nodes two tracks apart only | 22.0 M | 915 | 4.54 M | 117 |
| border ranks without space (all datasets measured: crossings 1296, area 79.4 M) | 18.8 M | 1003 | 3.68 M | 109 |
| + chain dummies one track wide (all datasets: crossings 1335, area 76.0 M) | 15.7 M | 1043 | 3.05 M | 140 |
| border chain dummies without space | 13.6 M | 1231 | 2.81 M | 162 |
| tracks shared by non-overlapping changes, widening kept | 20.8 M | 964 | 4.27 M | 118 |
| **widening removed (kept)** | **16.7 M** | **984** | **3.83 M** | **116** |
| + chain gaps of one track | 14.1 M | 1143 | 3.16 M | 128 |
| existing path | 11.7 M | 1327 | 2.37 M | 176 |

Full comparison of the kept variant:

| dataset | area k: off / before / now | crossings | length | refinement ms | hard |
|---|---|---|---|---|---|
| petri-net | 536 / 808 / 673 | 67 / 64 / 68 | 12988 / 15428 / 11840 | 137 / 32 / 340 | 0 / 0 / 0 |
| er-diagram | 1516 / 1566 / 1563 | 4 / 0 / 0 | 3658 / 3051 / 3089 | 21 / 10 / 14 | 0 / 0 / 0 |
| state-order | 462 / 462 / 462 | 0 / 0 / 0 | 2775 / 2760 / 2760 | 13 / 4 / 4 | 0 / 0 / 0 |
| activity-order | 1383 / 1378 / 1384 | 70 / 86 / 54 | 20757 / 18816 / 18996 | 113 / 54 / 35 | 0 / 0 / 0 |
| uml-class | 3194 / 3221 / 3221 | 8 / 5 / 5 | 9782 / 12200 / 12200 | 38 / 19 / 19 | 0 / 0 / 0 |
| bpmn-pools | 1725 / 1893 / 1887 | 8 / 6 / 6 | 10750 / 11335 / 11305 | 31 / 17 / 17 | 0 / 0 / 0 |
| network-vlan | 2715 / 3152 / 3019 | 71 / 62 / 64 | 28667 / 29617 / 26186 | 130 / 31 / 37 | 0 / 0 / 0 |
| layerweave-pipeline | 1916 / 2054 / 2046 | 20 / 40 / 38 | 14923 / 15840 / 15778 | 82 / 29 / 44 | 0 / 0 / 0 |
| cloud-architecture | 2570 / 3065 / 3023 | 35 / 23 / 27 | 24789 / 26349 / 26113 | 54 / 191 / 198 | 0 / 0 / 0 |
| mixed-layouts | 1330 / 1330 / 1330 | 5 / 5 / 5 | 3715 / 3715 / 3715 | 75 / 23 / 23 | 25 / 25 / 25 |
| nested-tree-radial | 34160 / 34160 / 34160 | 0 / 0 / 0 | 18099 / 18099 / 18099 | 143 / 72 / 72 | 0 / 0 / 0 |
| tree-100 | 7754 / 7754 / 7754 | 0 / 0 / 0 | 24768 / 27885 / 27885 | 146 / 32 / 31 | 196 / 196 / 196 |
| state-tcp | 396 / 396 / 396 | 5 / 0 / 0 | 6101 / 6018 / 6018 | 22 / 5 / 5 | 0 / 0 / 0 |
| deployment-cluster | 2368 / 4499 / 3831 | 176 / 117 / 116 | 35854 / 53091 / 43969 | 157 / 76 / 88 | 0 / 0 / 0 |
| self-loops | 327 / 327 / 327 | 15 / 0 / 0 | 2671 / 2553 / 2553 | 32 / 7 / 7 | 21 / 23 / 23 |
| layerweave-code | 11737 / 21206 / 16715 | 1327 / 953 / 984 | 238956 / 381764 / 306700 | 7189 / 1141 / 1126 | 0 / 0 / 0 |

Totals over the 13 layered datasets (off / before / now): area k 71003 / 82053 / 77243 (without nested-tree-radial: 36843 / 47893 / 43083); crossings 1615 / 1244 / 1251; length 414626 / 566857 / 484666; refinement ms 8173 / 1654 / 1961

Reading:
- Removing the widening lowers the area by 10 % (without nested-tree-radial, which is unchanged: 47.9 → 43.1 M) and the length
  by 15 % (567 k → 485 k), with the crossings unchanged (1244 → 1251; activity-order 86 → 54, layerweave-code 953 → 984).
- Against the existing path the area is still 17 % larger and the length 17 % longer. The rest of the growth comes from the
  dummies of the chains to border nodes (layerweave-code is 2400 high instead of 1770); every way of making them narrower cost
  crossings (layerweave-code 984 → 1043..1231).
