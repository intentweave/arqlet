# Container pins in the layered pipelines (`pinsInSinglePass`, ADR 0016 amendment, step P2, #174), 2026-11-26

Machine: Apple M3 (MacBook), macOS 26.0, heavily loaded for most of the run (load average 6 to 58, and asleep for one
night in between), so read the times as rough. Revision: 65b591a plus the P2 change (the commit that adds this file).
Build: `cargo build --release -p arqlet-runtime --bin layout-command`, rustc 1.99.0. Script:
`node benchmarks/eval/pins-single-pass.mjs target/release/layout-command --containers --json
benchmarks/results/pins-containers-20261125.jsonl` ([rows](pins-containers-20261125.jsonl)), run in parts with
`--resume`. Datasets: `benchmarks/generate.py`.

Per dataset up to four containers that stand in a layered area are pinned by hand, each in three places (further across
the flow, in the band of another rank, both): 177 cases on 16 datasets. `fixed-ports`, `metro-map`, `petri-net`,
`self-loops`, `state-tcp`, `vehicle-buses`, `vehicle-trunk` and `mixed-layouts` offer none. The four drawings per case
are those of [the leaf measurement](pins-single-pass-20261125.md); the **control** here is an order hint that names the
place the container has, which places the same area again without changing the order.

On `layerweave-code` the pin without the option did not answer within 900 s in four cases, and was not run for the three
cases after the second such one (the rows say which). The table counts the 170 cases where all four drawings exist.

| dataset | cases | single pass | previous pipeline | geometryIssues | routingIssues | busIssues | missing | reversed | lineNodeHits | containerTraversals | sharedLinePairs | crossings | bends | area (mean, 1000 units²) | along from the pin with the option, mean / max | across with the option, max |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| can-bus | 9 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 15 / 10 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 5 / 24 | 288 / 288 / 348 / 350 | 1207 / 1207 / 1454 / 1538 | 86 / 224 | 0 |
| activity-order | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 277 / 0 | 828 / 828 / 800 / 1018 | 948 / 1218 / 1388 / 1282 | 1935 / 1935 / 2223 / 2445 | 130 / 552 | 0 |
| bpmn-pools | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 30 / 0 | 120 / 120 / 145 / 126 | 708 / 852 / 656 / 844 | 1940 / 1986 / 2229 / 2253 | 158 / 669 | 0 |
| cloud-architecture | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 13 / 0 | 264 / 297 / 333 / 272 | 1296 / 1392 / 984 / 1404 | 4308 / 4516 / 4565 / 4383 | 186 / 689 | 0 |
| deployment-cluster | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 120 / 2 | 1140 / 1212 / 1506 / 1321 | 2724 / 3030 / 2654 / 3076 | 4711 / 4763 / 5706 / 4883 | 452 / 1196 | 0 |
| er-diagram | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 2 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 2 / 0 | 0 / 0 / 7 / 0 | 0 / 3 / 16 / 14 | 276 / 276 / 411 / 310 | 1664 / 1662 / 1975 / 1817 | 172 / 1147 | 0 |
| layerweave-code | 5 | 5 / 5 / 0 / 5 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 6 / 186 / 2 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 80 / 0 | 0 / 6 / 106 / 2 | 5 / 12 / 216 / 20 | 802045 / 762207 / 678657 / 768849 | 62035 / 66607 / 73813 / 66773 | 21802309 / 24505539 / 13382858 / 22985087 | 154 / 432 | 0 |
| org-chart | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 395 / 0 | 444 / 504 / 820 / 499 | 4020 / 4104 / 3459 / 4114 | 17884 / 17882 / 17605 / 18133 | 91 / 544 | 0 |
| state-order | 3 | 3 / 3 / 0 / 3 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 2 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 2 / 0 | 0 / 0 / 0 / 0 | 0 / 6 / 2 / 5 | 57 / 63 / 86 / 67 | 427 / 617 / 773 / 650 | 164 / 246 | 0 |
| uml-class | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 21 / 0 | 36 / 36 / 72 / 63 | 588 / 588 / 757 / 626 | 3627 / 3627 / 4031 / 3954 | 50 / 294 | 0 |
| vehicle-zonal | 9 | 9 / 9 / 0 / 9 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 963 / 963 / 2430 / 1316 | 3 / 6 | 0 |
| workflow-dag | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 12 / 6 / 8437 / 8 | 5196 / 5529 / 7414 / 5337 | 9420 / 9501 / 6118 / 9447 | 11827 / 13875 / 9770 / 11778 | 1193 / 2016 | 0 |
| arqlet-pipeline | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 63 / 0 | 264 / 309 / 389 / 405 | 936 / 1080 / 1128 / 1078 | 3390 / 3436 / 2890 / 3447 | 145 / 385 | 0 |
| mixed-large | 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 48 / 36 / 38 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 60 / 60 / 67 / 79 | 11712 / 11454 / 11573 / 11462 | 1800 / 1593 / 1634 / 1654 | 21452 / 24612 / 23655 / 25001 | 81 / 244 | 0 |
| network-vlan | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 3 / 0 | 276 / 294 / 421 / 262 | 1272 / 1314 / 1342 / 1332 | 3043 / 3236 / 3772 / 3361 | 306 / 1604 | 0 |
| synthetic-2k | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 67896 / 70065 / 46737 / 69361 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 34548 / 36045 / 21641 / 35635 | 33348 / 34020 / 25096 / 33726 | 7836 / 7242 / 157814 / 7512 | 2618616 / 2636775 / 2625729 / 2634640 | 345456 / 377829 / 319538 / 377847 | 12101353 / 12096379 / 9841280 / 12099450 | 320 / 1266 | 0 |
| total | 170 | 149 / 149 / 0 / 149 | 0 / 0 / 0 / 0 | 0 / 48 / 36 / 38 | 67896 / 70071 / 46927 / 69363 | 0 / 0 / 15 / 10 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 34548 / 36045 / 21721 / 35635 | 33348 / 34026 / 25206 / 33728 | 7913 / 7320 / 167463 / 7621 | 3440941 / 3419574 / 3327882 / 3424297 | 431824 / 469735 / 414316 / 470204 | 1500929 / 1580494 / 1094047 / 1536023 | 244 / 2016 | 0 |

## What it shows

- **The pipeline is kept.** All 156 cases that are single pass without a pin stay single pass; without the option none
  does (a container pin falls back to the hierarchical pipeline: "Fixed-order channel growth does not support pinned
  nodes").
- **Pins hold across the flow** in all 177 cases. **Along the flow the difference is much larger than for leaves:**
  exact in 68 cases, 12 units in the median, 237 on average, 2016 at most (leaves: 28 on average, 244 at most). A
  container's rank can be a wide band.
- **Hard criteria:** no missing route and none against its logical direction. Geometry issues with the option appear only
  on `mixed-large` (38 over its 12 cases; option off 36, control 48): that view has none as imported, and any re-layout of
  it, without a pin too (an option change that is not a pin), leaves 4 (a child of an organic container outside it).
- **Without the two large graphs** (153 cases):

  | | as imported | control | option off | option on |
  |---|---|---|---|---|
  | geometry issues | 0 | 48 | 36 | 38 |
  | line–node hits | 0 | 0 | 0 | 0 |
  | shared line pairs | 72 | 66 | 9433 | 89 |
  | crossings | 20280 | 20592 | 23496 | 20808 |
  | bends | 24333 | 25299 | 20965 | 25584 |
  | routing issues | 0 | 0 | 4 | 0 |
  | bus notes | 0 | 0 | 15 | 10 |

  The option draws far fewer shared lines and fewer crossings, and **more bends** than the pipeline behind it; the bends
  are those of the single-pass drawing itself (control).
- **The two large graphs** (17 cases with all four drawings: `synthetic-2k` 12, `layerweave-code` 5):

  | | as imported | control | option off | option on |
  |---|---|---|---|---|
  | line–node hits | 34548 | 36045 | 21721 | 35635 |
  | shared line pairs | 7841 | 7254 | 158030 | 7532 |
  | crossings | 3420661 | 3398982 | 3304386 | 3403489 |
  | bends | 407491 | 444436 | 393351 | 444620 |
  | routing issues | 67896 | 70071 | 46923 | 69363 |

  On `synthetic-2k` the single-pass drawing itself, without any pin, has more line–node hits than the pipeline the pin
  falls back to; the option keeps that drawing. A pin takes 1.4 s with the option and 92 s without (median); on
  `layerweave-code` 2.1 s against 160 s for the cases that answered.
- **Quality order, per dataset** (geometry issues, line–node hits, shared line pairs, crossings, bends; the first that
  differs, over the cases with all four): the option is better on 11 datasets, equal on 1, and worse on 4:
  - `synthetic-2k`: line–node hits 21641 → 35635 (control 36045): the single-pass drawing itself.
  - `mixed-large`: geometry issues 36 → 38 (control 48): the re-layout itself.
  - `state-order`: crossings 2 → 5 (control 6, 3 cases).
  - `can-bus`: crossings 5 → 24 (control 0): from the pins; bus notes 15 → 10.
- **What the pins themselves cost** (option against control): more crossings on `activity-order` (828 → 1018),
  `arqlet-pipeline` (309 → 405), `deployment-cluster` (1212 → 1321), `uml-class` (36 → 63), `er-diagram` (3 → 14) and
  `can-bus` (0 → 24); fewer on `workflow-dag` (5529 → 5337), `network-vlan` (294 → 262), `cloud-architecture`
  (297 → 272) and `org-chart` (504 → 499).

Not measured: several containers pinned at once, a leaf and a container together (tests cover it), drags, and the second
layout an import with pins needs.

## Time of one request, milliseconds (median per dataset)

On `layerweave-code` the column without the option is the median of the cases that answered.

| dataset | import | control | configure with the pin, option off | option on |
|---|---|---|---|---|
| can-bus | 15 | 17 | 19 | 17 |
| activity-order | 13 | 13 | 14 | 13 |
| bpmn-pools | 13 | 12 | 12 | 12 |
| cloud-architecture | 19 | 15 | 19 | 16 |
| deployment-cluster | 22 | 19 | 25 | 20 |
| er-diagram | 11 | 11 | 12 | 12 |
| layerweave-code | 2235 | 1635 | 160371 | 2086 |
| org-chart | 28 | 23 | 41 | 24 |
| state-order | 5 | 5 | 21 | 5 |
| uml-class | 10 | 9 | 10 | 9 |
| vehicle-zonal | 6 | 6 | 7 | 7 |
| workflow-dag | 36 | 29 | 27 | 30 |
| arqlet-pipeline | 12 | 9 | 10 | 10 |
| mixed-large | 1514 | 493 | 488 | 485 |
| network-vlan | 11 | 9 | 16 | 10 |
| synthetic-2k | 1843 | 1437 | 92112 | 1388 |
