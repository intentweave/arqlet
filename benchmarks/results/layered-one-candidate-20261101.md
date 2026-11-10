# Layered construction: one ordering candidate in the refinement, 2026-11-01

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision of the commit that adds this file
plus this change; `bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000); layerweave-code is the
graph of 0bcfd56 (669 / 1084). Raw rows: `layered-one-candidate-20261101.jsonl`. "before" is `layered-borders-20261031.jsonl`
(f5675c1, four candidates); "off" is the existing path (`all-datasets-off-20261031.jsonl`, `layered-crossings-20261031.jsonl`).
Default output unchanged (16 of 16 refinement fingerprints). The machine was not idle.

Change (decision of the user, 2026-11-01): with `layeredConstruction`, the refinement plans one ordering candidate, the first
configured one, instead of all; the improved rank order runs in it. See `refine-speedups-20261101.md` for the comparison of
candidate sets.

| dataset | refinement ms: off / before / now | grid pops | crossings | length | bends | hard |
|---|---|---|---|---|---|---|
| petri-net | 137 / 46 / 32 | 562 / 0 / 0 | 67 / 64 / 64 | 12988 / 15428 / 15428 | 126 / 102 / 102 | 0 / 0 / 0 |
| er-diagram | 21 / 83 / 10 | 1340 / 0 / 0 | 4 / 0 / 0 | 3658 / 2677 / 3051 | 33 / 30 / 27 | 0 / 0 / 0 |
| state-order | 13 / 7 / 4 | 0 / 0 / 0 | 0 / 0 / 0 | 2775 / 2645 / 2760 | 26 / 31 / 31 | 0 / 0 / 0 |
| activity-order | 113 / 107 / 54 | 17156 / 15006 / 4593 | 70 / 72 / 86 | 20757 / 18016 / 18816 | 108 / 82 / 78 | 0 / 0 / 0 |
| uml-class | 38 / 45 / 19 | 0 / 0 / 0 | 8 / 5 / 5 | 9782 / 11182 / 12200 | 49 / 61 / 61 | 0 / 0 / 0 |
| bpmn-pools | 31 / 31 / 17 | 0 / 0 / 0 | 8 / 6 / 6 | 10750 / 10767 / 11335 | 46 / 62 / 46 | 0 / 0 / 0 |
| network-vlan | 130 / 55 / 31 | 16345 / 0 / 0 | 71 / 62 / 62 | 28667 / 29617 / 29617 | 136 / 102 / 102 | 0 / 0 / 0 |
| layerweave-pipeline | 82 / 91 / 29 | 11572 / 51447 / 5520 | 20 / 33 / 40 | 14923 / 13690 / 15840 | 80 / 94 / 82 | 0 / 0 / 0 |
| cloud-architecture | 54 / 242 / 191 | 20806 / 0 / 0 | 35 / 23 / 23 | 24789 / 26349 / 26349 | 90 / 83 / 83 | 0 / 0 / 0 |
| mixed-layouts | 75 / 86 / 23 | 0 / 0 / 0 | 5 / 5 / 5 | 3715 / 3715 / 3715 | 0 / 0 / 0 | 25 / 25 / 25 |
| nested-tree-radial | 143 / 148 / 72 | 0 / 0 / 0 | 0 / 0 / 0 | 18099 / 18099 / 18099 | 88 / 88 / 88 | 0 / 0 / 0 |
| tree-100 | 146 / 84 / 32 | 0 / 0 / 0 | 0 / 0 / 0 | 24768 / 27885 / 27885 | 0 / 0 / 0 | 196 / 196 / 196 |
| state-tcp | 22 / 10 / 5 | 0 / 0 / 0 | 5 / 0 / 0 | 6101 / 5798 / 6018 | 47 / 38 / 72 | 0 / 0 / 0 |
| deployment-cluster | 157 / 190 / 76 | 49278 / 24732 / 6698 | 176 / 117 / 117 | 35854 / 53091 / 53091 | 200 / 156 / 156 | 0 / 0 / 0 |
| self-loops | 32 / 23 / 7 | 0 / 0 / 0 | 15 / 0 / 0 | 2671 / 2553 / 2553 | 30 / 30 / 30 | 21 / 23 / 23 |
| layerweave-code | 7189 / 2693 / 1141 | 2251396 / 604379 / 131060 | 1327 / 941 / 953 | 238956 / 376339 / 381764 | 748 / 555 / 531 | 0 / 0 / 0 |

Totals over the 13 layered datasets (off / before / now): refinement ms 8173 / 3719 / 1654; grid pops 2319177 / 670832 / 141173; crossings 1615 / 1211 / 1244; length 414626 / 556409 / 566857; bends 1530 / 1290 / 1231

Against the existing path, over the 13 layered datasets: refinement 8.17 s → 1.65 s (−80 %), grid pops 2.32 M → 0.14 M
(−94 %), crossings 1615 → 1244 (−23 %), length 415 k → 567 k (+37 %), bends 1530 → 1231 (−20 %), hard violations unchanged.
