# All datasets: existing path against the layered construction (steps 1–8), 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release profile, rustc 1.99.0, bench with `--features benchmark-timings`.
Revision 0f9dcf4. Load average 1.8–3.5 at the start (quiet, not idle). `bench --reps 3 --threads 1`, medians; limit 100 (tree-100 and the
organic sets 1000). "off" = existing path, "on" = `layeredConstruction` on the import. Raw rows: `all-datasets-off-20261031.jsonl`,
`all-datasets-on-20261031.jsonl`. Counters are deterministic; times are medians of 3.

| dataset | import ms | refinement ms | grid pops | legacy candidates | hard | crossings | length | bends |
|---|---|---|---|---|---|---|---|---|
| petri-net | 6 → 15 | 137 → 129 | 562 → 3216 | 431738 → 176661 | 0 → 0 | 67 → 80 | 12988 → 12786 | 126 → 110 |
| er-diagram | 2 → 2 | 21 → 22 | 1340 → 1340 | 76308 → 76308 | 0 → 0 | 4 → 4 | 3658 → 3658 | 33 → 33 |
| state-order | 0 → 1 | 13 → 68 | 0 → 0 | 35904 → 16876 | 0 → 0 | 0 → 0 | 2775 → 2775 | 26 → 24 |
| activity-order | 4 → 9 | 113 → 110 | 17156 → 27694 | 317258 → 253623 | 0 → 0 | 70 → 73 | 20757 → 20529 | 108 → 106 |
| uml-class | 4 → 10 | 38 → 43 | 0 → 0 | 124905 → 92112 | 0 → 0 | 8 → 8 | 9782 → 9824 | 49 → 49 |
| bpmn-pools | 1 → 6 | 31 → 39 | 0 → 0 | 132807 → 77284 | 0 → 0 | 8 → 8 | 10750 → 10828 | 46 → 48 |
| network-vlan | 5 → 10 | 130 → 139 | 16345 → 24310 | 378313 → 241395 | 0 → 0 | 71 → 79 | 28667 → 24782 | 136 → 116 |
| layerweave-pipeline | 3 → 9 | 82 → 77 | 11572 → 11066 | 259287 → 164890 | 0 → 0 | 20 → 22 | 14923 → 15132 | 80 → 84 |
| cloud-architecture | 3 → 4 | 54 → 51 | 20806 → 18126 | 147190 → 116752 | 0 → 0 | 35 → 37 | 24789 → 24807 | 90 → 92 |
| mixed-layouts | 16 → 19 | 75 → 81 | 0 → 0 | 46400 → 34800 | 25 → 25 | 5 → 5 | 3715 → 3715 | 0 → 0 |
| nested-tree-radial | 5 → 6 | 143 → 134 | 0 → 0 | 309720 → 261000 | 0 → 0 | 0 → 0 | 18099 → 18099 | 88 → 88 |
| tree-100 | 6 → 45 | 146 → 215 | 0 → 0 | 217332 → 0 | 196 → 196 | 0 → 0 | 24768 → 24768 | 0 → 0 |
| organic-25 | 8 → 8 | 43 → 46 | 0 → 0 | 67280 → 67280 | 58 → 58 | 0 → 0 | 2751 → 2751 | 0 → 0 |
| organic-50 | 88 → 65 | 280 → 295 | 0 → 0 | 136880 → 136880 | 118 → 118 | 2 → 2 | 5882 → 5882 | 0 → 0 |
| organic-100 | 127 → 132 | 636 → 669 | 0 → 0 | 276080 → 276080 | 238 → 238 | 1 → 1 | 12773 → 12773 | 0 → 0 |
| layerweave-code | 132 → 492 | 7189 → 2255 | 2251396 → 690073 | 2134400 → 630460 | 0 → 0 | 1327 → 1892 | 238956 → 287425 | 748 → 569 |
| organic-200 | 5818 → 5886 | 23889 → 24044 | 5588 → 5588 | 554480 → 554480 | 474 → 474 | 10 → 10 | 29550 → 29550 | 0 → 0 |

Totals over 17 datasets: import 6228 → 6720 ms (+8 %); refinement 33.0 → 28.4 s (-14 %); grid pops 2324765 → 781413 (-66 %); legacy candidates -44 %; crossings 1628 → 2221 (+36 %); length +10 %; bends -14 %. Hard violations unchanged on every dataset.

Reading:
- The gain is concentrated on layerweave-code, the only large hierarchical view: refinement 7.2 s → 2.3 s (−69 %), grid pops −69 %.
- On the small layered views the refinement time stays within about ±10 %, except state-order (13 → 68 ms) and tree-100 (146 → 215 ms).
  On state-order the timed stages are unchanged, so the extra time lies in a part the stage counters do not cover; the cause is not
  found. On tree-100 every edge is constructed (legacy candidates 217 k → 0), and the routing stage rises from 87 to 155 ms: the
  construction costs more than the search it replaces there.
- Grid pops rise on petri-net (562 → 3216), activity-order and network-vlan.
- The import is slower on most layered views (a few ms on the small ones, 132 → 492 ms on layerweave-code).
- The organic views are unchanged in work; their times are within noise. The bench run of organic-200 with the option took more than
  ten minutes in total, against about two without; the refinement median is equal (24.0 s against 23.9 s), so the extra time lies in the
  bench's other measurements (edits, interaction). Not investigated.
- Quality: crossings rise on most layered views (layerweave-code +43 %), bends fall (layerweave-code −24 %, network-vlan −15 %), length is
  mixed.
