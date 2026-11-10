# Search cost and quality: existing path against the constructed-edge path, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release profile, rustc 1.99.0. Revision 0bcfd56 plus uncommitted changes.
Load average about 5 (not idle). `bench --threads 1 --reps 1`, limit 100 (tree-100 and organic sets 1000, the rest as listed).
"off": the existing path. "on": `layeredConstruction` on the import (constructed edges fixed, the search only for the others).
Raw rows: `cost-off-20261030.jsonl`, `cost-on-20261030.jsonl`. Times are single runs; the counters are deterministic.

Primary: search work and time. Secondary: length and bends of the refined layout (route quality), with hard violations and crossings.

| dataset | refine ms off→on | legacy candidates off→on | grid pops off→on | length off→on | bends off→on | crossings off→on |
|---|---|---|---|---|---|---|
| petri-net | 134→95 | 431738→282751 | 562→5410 | 12988→12960 | 126→126 | 67→93 |
| er-diagram | 21→22 | 76308→76308 | 1340→1340 | 3658→3658 | 33→33 | 4→4 |
| state-order | 13→12 | 35904→23020 | 0→0 | 2775→2775 | 26→26 | 0→0 |
| activity-order | 112→92 | 317258→234138 | 17156→17035 | 20757→21467 | 108→108 | 70→66 |
| uml-class | 37→40 | 124905→124905 | 0→0 | 9782→9782 | 49→49 | 8→8 |
| bpmn-pools | 30→24 | 132807→77035 | 0→0 | 10750→10828 | 46→48 | 8→8 |
| network-vlan | 127→119 | 378313→320574 | 16345→16441 | 28667→28667 | 136→136 | 71→73 |
| layerweave-pipeline | 77→69 | 259287→193693 | 11572→11527 | 14923→15007 | 80→84 | 20→21 |
| cloud-architecture | 52→50 | 147190→123619 | 20806→19432 | 24789→24789 | 90→90 | 35→35 |
| mixed-layouts | 71→72 | 46400→34800 | 0→0 | 3715→3715 | 0→0 | 5→5 |
| nested-tree-radial | 139→166 | 309720→274920 | 0→0 | 18099→18099 | 88→88 | 0→0 |
| tree-100 | 143→87 | 217332→49460 | 0→0 | 24768→24768 | 0→0 | 0→0 |
| organic-25 | 41→44 | 67280→67280 | 0→0 | 2751→2751 | 0→0 | 0→0 |
| organic-50 | 271→277 | 136880→136880 | 0→0 | 5882→5882 | 0→0 | 2→2 |
| organic-100 | 621→638 | 276080→276080 | 0→0 | 12773→12773 | 0→0 | 1→1 |
| layerweave-code | 7221→5891 | 2134400→1608340 | 2251396→2280869 | 238956→268799 | 748→771 | 1327→2030 |
| organic-200 | 23713→23690 | 554480→554480 | 5588→5588 | 29550→29550 | 0→0 | 10→10 |

Totals (17 datasets): refinement time 32.8 s → 31.4 s (-4 %); legacy candidates 5.65 M → 4.46 M (-21 %).
Hard violations: 1109 → 1109 (no change on any dataset).

Reading:
- The constructed edges remove about a fifth of the candidate work, but not the repair search: the grid pops that dominate
  layerweave-code (2.3 M) are unchanged, so the refinement time falls by 18 % there and not more.
- The quality cost is larger than the speed gain on the layered sets with routed crossings: layerweave-code crossings 1327 → 2030
  (+53 %) and length +12 %; petri-net crossings 67 → 93. Where the time goes up (nested-tree-radial, +20 %), no candidate work was saved.
- Conclusion for the goal "less compute": the constructed-edge path, as it is, is not a speed-up for the refinement. The saving has
  to come from the repair search, which the constructed edges do not touch.
