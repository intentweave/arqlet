# Layered construction: area plan and improved rank order (steps 1 and 2), 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision ad8115f plus
uncommitted changes (behind `layeredConstruction`; default output unchanged, 16 of 16 refinement fingerprints).
`bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000); layerweave-code is the graph of 0bcfd56
(669 / 1084). Raw rows: `layered-plan-order-20261031.jsonl`. "before" is `layered-ports-loops-20261031.jsonl` (e09206c, the
same construction code). The machine was not idle.

Changes (plan section 4a, steps 1 and 2 of the order agreed on 2026-10-31):
1. `crates/layout/src/layered_plan.rs` (provisional): the derived layout graph of a layered area (`AreaPlan`): the ranks in
   flow order with children and long-edge dummies, the chain of dummies of every link, the links the cycle break reversed,
   and the dummies' places. The placement fills it instead of dropping the dummies; it travels with the layout pass (`Run`)
   for the steps after the placement. `layered_plans(graph, options)` returns the plans of a first placement (tests,
   measurements). The constructor does not read it yet (step 3 and 5).
2. The order of the ranks, dummies included, is improved after the existing sweeps: sweeps down and up alternate, each
   followed by transpositions of neighbours that lower the crossings with both neighbouring ranks; the crossings are counted
   and the best order is kept (`layered_plan::improve_order`).

Order crossings of the plans of a first placement (import view, sections between neighbouring ranks), input order against
the improved order: 522 → 118 over 16 datasets; layerweave-code 314 → 32, petri-net 116 → 56, network-vlan 51 → 27,
state-tcp 18 → 0, the others to 0 or 3.

After the refinement:

| dataset | refinement ms: off / before / now | grid pops | crossings | length | bends | hard |
|---|---|---|---|---|---|---|
| petri-net | 137 / 63 / 65 | 562 / 667 / 0 | 67 / 92 / 79 | 12988 / 13616 / 11620 | 126 / 106 / 100 | 0 / 0 / 0 |
| er-diagram | 21 / 52 / 57 | 1340 / 0 / 0 | 4 / 1 / 1 | 3658 / 4242 / 4242 | 33 / 32 / 32 | 0 / 0 / 0 |
| state-order | 13 / 5 / 5 | 0 / 0 / 0 | 0 / 0 / 0 | 2775 / 2762 / 2762 | 26 / 25 / 25 | 0 / 0 / 0 |
| activity-order | 113 / 112 / 104 | 17156 / 32463 / 32463 | 70 / 70 / 70 | 20757 / 19982 / 19982 | 108 / 102 / 102 | 0 / 0 / 0 |
| uml-class | 38 / 32 / 32 | 0 / 0 / 0 | 8 / 7 / 7 | 9782 / 9734 / 9734 | 49 / 51 / 51 | 0 / 0 / 0 |
| bpmn-pools | 31 / 26 / 26 | 0 / 738 / 738 | 8 / 7 / 7 | 10750 / 9923 / 9923 | 46 / 44 / 44 | 0 / 0 / 0 |
| network-vlan | 130 / 119 / 107 | 16345 / 21891 / 21814 | 71 / 72 / 85 | 28667 / 24832 / 24944 | 136 / 118 / 122 | 0 / 0 / 0 |
| layerweave-pipeline | 82 / 62 / 61 | 11572 / 17757 / 17646 | 20 / 22 / 22 | 14923 / 14090 / 14090 | 80 / 80 / 80 | 0 / 0 / 0 |
| cloud-architecture | 54 / 60 / 62 | 20806 / 20456 / 22408 | 35 / 31 / 31 | 24789 / 26073 / 26073 | 90 / 96 / 96 | 0 / 0 / 0 |
| mixed-layouts | 75 / 88 / 83 | 0 / 0 / 0 | 5 / 5 / 5 | 3715 / 3715 / 3715 | 0 / 0 / 0 | 25 / 25 / 25 |
| nested-tree-radial | 143 / 136 / 136 | 0 / 0 / 0 | 0 / 0 / 0 | 18099 / 18099 / 18099 | 88 / 88 / 88 | 0 / 0 / 0 |
| tree-100 | 146 / 82 / 83 | 0 / 0 / 0 | 0 / 0 / 0 | 24768 / 27885 / 27885 | 0 / 0 / 0 | 196 / 196 / 196 |
| state-tcp | 22 / 8 / 8 | 0 / 0 / 0 | 5 / 0 / 0 | 6101 / 5757 / 5757 | 47 / 38 / 38 | 0 / 0 / 0 |
| deployment-cluster | 157 / 147 / 159 | 49278 / 39324 / 39324 | 176 / 162 / 178 | 35854 / 42055 / 42137 | 200 / 156 / 155 | 0 / 0 / 0 |
| self-loops | 32 / 22 / 22 | 0 / 0 / 0 | 15 / 0 / 0 | 2671 / 2321 / 2321 | 30 / 30 / 30 | 21 / 23 / 23 |
| layerweave-code | 7189 / 2095 / 2065 | 2251396 / 638755 / 582647 | 1327 / 1603 / 1255 | 238956 / 300831 / 284519 | 748 / 561 / 583 | 0 / 0 / 0 |

Totals over the 13 layered datasets (off / before / now): refinement ms 8173 / 2932 / 2885; grid pops 2319177 / 732727 / 677716; crossings 1615 / 1910 / 1562; length 414626 / 475784 / 457588; bends 1530 / 1303 / 1323

Reading:
- For the first time the layered construction has fewer crossings than the existing path over the 13 layered datasets
  (1562 against 1615; "before" 1910). layerweave-code 1603 → 1255 (existing path 1327), petri-net 92 → 79. Length −3.8 %,
  grid pops −7.5 %; the refinement time is unchanged (2.93 → 2.89 s).
- network-vlan rises from 72 to 85 and deployment-cluster from 162 to 178. In both, most edges cross between areas of
  different flow axes and are delegated (boundary ports, step 6); the order of an area does not see them.
