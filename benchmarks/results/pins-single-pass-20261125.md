# Pins in the layered pipelines (`pinsInSinglePass`, ADR 0016 amendment, step P1, #174), 2026-11-25

**Corrected the same day.** The first version of the script passed no node shapes to the metrics, so circles were
measured as rectangles (81 line–node hits on `self-loops` that the drawing does not have, and the one dataset reported
as worse), and it measured the router's routes of `metro-map` instead of the drawn ones. The engine is the same; these
are the numbers of the corrected script.

Machine: Apple M3 (MacBook), macOS 26.0, loaded during the run (load average 3 to 22), so read the times as rough.
Revision: 65b591a (0.9.0 plus P1). Build: `cargo build --release -p
arqlet-runtime --bin layout-command`, rustc 1.99.0. Script: `node benchmarks/eval/pins-single-pass.mjs
target/release/layout-command --json benchmarks/results/pins-single-pass-20261125.jsonl`
([rows](pins-single-pass-20261125.jsonl)). Datasets: `benchmarks/generate.py` (`layerweave-code` is generated from the
source: 1372 nodes, 2698 edges here).

Per dataset up to four leaves are pinned by hand (`configure`), each in three places: 150 further across the flow, in the
band of another rank, and both. 189 cases; `er-diagram`, `uml-class` and `workflow-dag` offer the script no leaf. Four
drawings are compared per case:

- **as imported**: no pin;
- **control**: the same local change without a pin (the node asked one unit taller). A change of pins places only the
  pinned node's area and the areas above it again, so this is what a local edit does by itself;
- **option off**: the pin as before;
- **option on**: the pin with `pinsInSinglePass` set at the import.

| dataset | cases | single pass | previous pipeline | geometryIssues | routingIssues | busIssues | missing | reversed | lineNodeHits | containerTraversals | sharedLinePairs | crossings | bends | area (mean, 1000 units²) | along from the pin with the option, mean / max | across with the option, max |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| can-bus | 12 | 0 / 0 / 0 / 0 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 2 / 0 | 0 / 0 / 47 / 12 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 2 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 38 / 0 | 384 / 384 / 585 / 384 | 1207 / 1207 / 1389 / 1483 | 43 / 244 | 0 |
| activity-order | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 4 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 4 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 268 / 0 | 828 / 828 / 896 / 828 | 948 / 1158 / 1438 / 1304 | 1935 / 1936 / 1754 / 1996 | 5 / 32 | 0 |
| bpmn-pools | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 2 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 2 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 67 / 0 | 120 / 120 / 130 / 119 | 708 / 840 / 758 / 870 | 1940 / 1941 / 1915 / 2024 | 7 / 42 | 0 |
| cloud-architecture | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 1 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 154 / 0 | 264 / 264 / 379 / 264 | 1296 / 1416 / 1452 / 1416 | 4308 / 4308 / 3905 / 4408 | 42 / 244 | 0 |
| deployment-cluster | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 1 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 734 / 0 | 1140 / 1140 / 1900 / 1148 | 2724 / 3066 / 2243 / 3080 | 4711 / 4715 / 4234 / 4834 | 63 / 202 | 0 |
| fixed-ports | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 7 / 5 | 144 / 144 / 213 / 192 | 135 / 136 / 190 / 195 | 32 / 180 | 0 |
| layerweave-code | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 18 / 1399 / 18 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 396 / 0 | 0 / 18 / 1003 / 18 | 12 / 27 / 58443 / 27 | 1924908 / 1852167 / 2381713 / 1852464 | 148884 / 159666 / 193619 / 159826 | 21802309 / 23602302 / 400508 / 23622828 | 44 / 60 | 0 |
| metro-map | 12 | 0 / 0 / 0 / 0 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 60 / 60 / 85 / 62 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 39 / 0 | 12 / 12 / 88 / 15 | 672 / 708 / 889 / 786 | 777 / 778 / 892 / 852 | 33 / 180 | 0 |
| org-chart | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 603 / 0 | 444 / 504 / 765 / 559 | 4020 / 4116 / 4454 / 4106 | 17884 / 17886 / 16434 / 18656 | 8 / 48 | 0 |
| petri-net | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 467 / 0 | 384 / 384 / 904 / 444 | 1008 / 1008 / 1689 / 1100 | 1389 / 1389 / 694 / 1492 | 13 / 40 | 0 |
| self-loops | 9 | 0 / 0 / 0 / 0 | 0 / 0 / 9 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 4 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 8 / 0 | 0 / 0 / 0 / 4 | 270 / 270 / 274 / 271 | 245 / 245 / 855 / 449 | 0 / 0 | 0 |
| state-order | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 11 / 0 | 0 / 0 / 27 / 16 | 228 / 276 / 382 / 298 | 427 / 427 / 578 / 466 | 94 / 210 | 0 |
| state-tcp | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 1 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 1 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 56 / 0 | 0 / 0 / 38 / 11 | 312 / 318 / 662 / 366 | 357 / 368 / 509 / 466 | 43 / 222 | 0 |
| vehicle-buses | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 1191 / 1194 / 1302 / 1094 | 13 / 78 | 0 |
| vehicle-trunk | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 367 / 368 / 485 / 405 | 7 / 18 | 0 |
| vehicle-zonal | 12 | 12 / 12 / 0 / 12 | 0 / 0 / 12 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 963 / 965 / 1049 / 1027 | 2 / 6 | 0 |
| total | 189 | 156 / 156 / 0 / 156 | 0 / 0 / 189 / 0 | 0 / 0 / 2 / 0 | 0 / 18 / 1412 / 18 | 60 / 60 / 132 / 74 | 0 / 0 / 0 / 0 | 0 / 0 / 0 / 0 | 0 / 0 / 403 / 0 | 0 / 18 / 1005 / 18 | 12 / 27 / 60850 / 27 | 1928100 / 1855419 / 2386885 / 1855877 | 161598 / 173370 / 208658 / 173999 | 1386672 / 1500959 / 27713 / 1502385 | 28 / 244 | 0 |

## What it shows

- **The pipeline is kept.** With the option every case that is single pass without a pin stays single pass (156 of 156),
  and no case takes the previous layout pipeline; without it all 189 do.
- **Pins hold across the flow** in all 189 cases. Along the flow the node stands in its rank's band: exact in 103 cases,
  28 units from the pin on average, 244 at most.
- **Hard criteria:** no geometry issue (off: 2), no missing route, no route against its logical direction.
- **Against the option off**, summed over the 177 cases without `layerweave-code`:

  | | as imported | control | option off | option on |
  |---|---|---|---|---|
  | line–node hits | 0 | 0 | 7 | 0 |
  | shared line pairs | 0 | 0 | 2407 | 0 |
  | crossings | 3192 | 3252 | 5172 | 3413 |
  | bends | 12714 | 13704 | 15039 | 14173 |
  | routing issues | 0 | 0 | 13 | 0 |

- **`layerweave-code`:** option off draws 396 line–node hits, 1003 container traversals and 58443 shared line pairs; with
  the option 0, 18 and 27, the same as the control. A pin costs 1.1 s instead of 80 s (median, one process per request).
- **The cost of the local edit.** Most of the distance to the imported drawing is the local placement's, not the pin's:
  bends 12714 → 13704 (control) → 14173 (pin), and the 18 routing issues of `layerweave-code` appear in the control too.
- **Quality order, per dataset** (geometry issues, line–node hits, shared line pairs, crossings, bends; the first that
  differs): the option is better than without it on 13 datasets and equal on 3; it is worse on none.
- **Buses.** `can-bus` has no bus note without a pin; with the option 12, in 4 of its 12 cases: a pin into the band of
  another rank leaves some edges of a bus running across the flow, and they are drawn apart (option off: 47 notes, all
  12 cases; in two of the four cases the option has more than off, 4 against 2). `metro-map`: 62 notes against 60 without a pin (off: 85).
- **Area** with the option follows the imported drawing, and is larger than with the option off on 9 of 16
  datasets; on `layerweave-code` by far (23.6 million units² against 0.4 million; 21.8 million without a pin).

Not measured: several pins at once, drags (the engine path is the same `configure` of pins; tests cover the gesture),
and the second layout an import with pins needs to find the bands.

Found on the way, present in 0.9.0 and not changed here: a full re-layout of an existing drawing of `layerweave-code`
(any option change that is not a pin, a size or an order hint) comes out much taller than its import (325835 → 544625;
width unchanged).

## Time of one request, milliseconds (median per dataset)

| dataset | import | control | configure with the pin, option off | option on |
|---|---|---|---|---|
| can-bus | 17 | 16 | 15 | 17 |
| activity-order | 16 | 15 | 14 | 15 |
| bpmn-pools | 13 | 12 | 11 | 12 |
| cloud-architecture | 16 | 15 | 14 | 16 |
| deployment-cluster | 21 | 21 | 18 | 20 |
| fixed-ports | 8 | 9 | 8 | 9 |
| layerweave-code | 4155 | 1938 | 80027 | 1102 |
| metro-map | 5 | 3 | 3 | 3 |
| org-chart | 14 | 13 | 12 | 13 |
| petri-net | 6 | 5 | 4 | 5 |
| self-loops | 5 | 3 | 5 | 4 |
| state-order | 3 | 3 | 2 | 3 |
| state-tcp | 3 | 3 | 3 | 3 |
| vehicle-buses | 4 | 3 | 3 | 3 |
| vehicle-trunk | 3 | 3 | 3 | 3 |
| vehicle-zonal | 4 | 3 | 3 | 3 |
