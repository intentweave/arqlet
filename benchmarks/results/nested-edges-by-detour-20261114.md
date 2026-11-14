# Nested straight edges left to their detours (`nestedEdgesByDetour`, experimental), 2026-11-14

Apple M3, macOS arm64, rustc 1.99.0, Node 24.21.0; release `layout-command` and `bench` (`benchmark-timings`) from the
branch `perf-mixed-large` (main at 346171e plus this change). Command: `node benchmarks/eval/nested-edges-by-detour.mjs
[--json]` (raw rows: `nested-edges-by-detour-20261114.jsonl`); bounded views of 150 nodes, one import each, against the
S1-4 default set explicitly (`regionPorts`, `straightDetours`, `singlePassReversed`). The import times in the table are
single runs; the medians below come from `bench`.

Why: a profile of mixed-large (an organic root over layered containers) put 99.7 % of its 7.6 s import into the geometric
repair of the organic root, which used its whole budget of 200 000 scans for 54 moves (about 3 700 trial scans each,
about 40 movable nodes, 8 directions, 12 steps, about 38 µs per scan, half of it port resolution) and still left 57 of
322 conflicts, all straight edges through nodes. With `straightDetours` those edges are drawn around the nodes in their
way anyway. With the option the repair keeps clearing the lines between children of organic areas, which it moves
directly, and leaves the edges with an end inside a container to their detours.

Cells: lines through nodes / crossings / bends / area (thousands) / geometry issues / ms.

| dataset | identical | default: hits / crossings / bends / area (k) / issues / ms | nestedEdgesByDetour: same |
|---|---|---|---|
| activity-order | yes | 0 / 69 / 79 / 1939 / 0 / 6 | 0 / 69 / 79 / 1939 / 0 / 6 |
| arqlet-pipeline | yes | 0 / 22 / 80 / 3535 / 0 / 5 | 0 / 22 / 80 / 3535 / 0 / 5 |
| bpmn-pools | yes | 0 / 10 / 59 / 1971 / 0 / 5 | 0 / 10 / 59 / 1971 / 0 / 5 |
| can-bus | yes | 0 / 0 / 32 / 1315 / 0 / 7 | 0 / 0 / 32 / 1315 / 0 / 7 |
| cloud-architecture | yes | 0 / 22 / 114 / 4816 / 0 / 7 | 0 / 22 / 114 / 4816 / 0 / 7 |
| deployment-cluster | yes | 0 / 95 / 227 / 5038 / 0 / 10 | 0 / 95 / 227 / 5038 / 0 / 10 |
| er-diagram | yes | 0 / 0 / 23 / 1664 / 0 / 3 | 0 / 0 / 23 / 1664 / 0 / 3 |
| fixed-ports | yes | 0 / 0 / 12 / 148 / 0 / 3 | 0 / 0 / 12 / 148 / 0 / 2 |
| layerweave-code | yes | 0 / 655 / 1127 / 83849 / 0 / 81 | 0 / 655 / 1127 / 83849 / 0 / 80 |
| mixed-large | no | 0 / 811 / 68 / 50888 / 10 / 7578 | 0 / 965 / 242 / 12680 / 0 / 293 |
| mixed-layouts | no | 0 / 5 / 0 / 1330 / 0 / 19 | 0 / 7 / 3 / 1330 / 0 / 18 |
| nested-tree-radial | yes | 0 / 0 / 112 / 42157 / 24 / 8 | 0 / 0 / 112 / 42157 / 24 / 8 |
| network-vlan | yes | 0 / 23 / 106 / 3173 / 0 / 6 | 0 / 23 / 106 / 3173 / 0 / 6 |
| org-chart | yes | 0 / 37 / 337 / 20080 / 0 / 14 | 0 / 37 / 337 / 20080 / 0 / 14 |
| organic-100 | yes | 0 / 1 / 0 / 10903 / 0 / 59 | 0 / 1 / 0 / 10903 / 0 / 59 |
| organic-200 | yes | 0 / 5 / 0 / 12848 / 0 / 281 | 0 / 5 / 0 / 12848 / 0 / 284 |
| organic-25 | yes | 0 / 0 / 0 / 1487 / 0 / 9 | 0 / 0 / 0 / 1487 / 0 / 9 |
| organic-300 | yes | 0 / 2 / 0 / 14274 / 0 / 88 | 0 / 2 / 0 / 14274 / 0 / 88 |
| organic-50 | yes | 0 / 2 / 0 / 3826 / 0 / 41 | 0 / 2 / 0 / 3826 / 0 / 40 |
| petri-net-shapes | yes | 0 / 27 / 0 / 1614 / 0 / 532 | 0 / 27 / 0 / 1614 / 0 / 531 |
| petri-net | yes | 0 / 32 / 84 / 1512 / 0 / 6 | 0 / 32 / 84 / 1512 / 0 / 5 |
| self-loops | no | 0 / 0 / 30 / 255 / 0 / 5 | 0 / 2 / 32 / 321 / 0 / 3 |
| state-order | yes | 0 / 0 / 19 / 470 / 0 / 3 | 0 / 0 / 19 / 470 / 0 / 2 |
| state-tcp | yes | 0 / 0 / 26 / 396 / 0 / 3 | 0 / 0 / 26 / 396 / 0 / 3 |
| synthetic-2k | yes | 0 / 2792 / 1613 / 61421 / 0 / 146 | 0 / 2792 / 1613 / 61421 / 0 / 146 |
| tree-100 | yes | 0 / 0 / 0 / 7754 / 0 / 11 | 0 / 0 / 0 / 7754 / 0 / 11 |
| tree-200 | yes | 0 / 0 / 0 / 13443 / 0 / 19 | 0 / 0 / 0 / 13443 / 0 / 19 |
| tree-25 | yes | 0 / 0 / 0 / 1382 / 0 / 3 | 0 / 0 / 0 / 1382 / 0 / 3 |
| tree-300 | yes | 0 / 0 / 0 / 13443 / 0 / 19 | 0 / 0 / 0 / 13443 / 0 / 20 |
| tree-50 | yes | 0 / 0 / 0 / 3269 / 0 / 5 | 0 / 0 / 0 / 3269 / 0 / 5 |
| uml-class | yes | 0 / 3 / 49 / 3675 / 0 / 6 | 0 / 3 / 49 / 3675 / 0 / 5 |
| workflow-dag | yes | 0 / 433 / 785 / 11827 / 0 / 19 | 0 / 433 / 785 / 11827 / 0 / 20 |

`bench` (median of 3, one refinement thread, load average about 1.6 to 3.4): mixed-large import 7571 → 290 ms, refinement
31263 → 2917 ms.

Reading:

- 28 of 31 datasets are identical, among them every flat organic area (organic-25 to organic-300, petri-net-shapes) and
  every layered, tree and radial one.
- mixed-large: the geometry becomes valid (10 issues before: container bounds and areas left), import 26 times and
  refinement 11 times faster, a quarter of the area; 154 more crossings and 174 more bends (the detours of lines that the
  repair cleared before by moving whole containers apart, which is where the area went).
- mixed-layouts and self-loops (small organic roots over containers): 2 more crossings each, 3 and 2 more bends,
  self-loops 26 % more area.
- Without a measured gain in crossings and bends it stays an option; it is the lever for the time of organic roots over
  containers and part of their area.

## Default (later the same day)

The first version moved the lines into containers out of the repair altogether: mixed-layouts and self-loops then crossed
twice more. Now, with `straightDetours`:

- when the graph has no straight edge into a container of an organic area, the repair is as without the option;
- otherwise a probe repairs every line within a bounded budget (8000 scans); a probe that ends within it is the repair
  without the option (the search is deterministic and does not end on its budget then), and its result is kept;
- only a repair that would spend more goes in two phases: the lines between children of organic areas first, then the
  lines into containers within a bounded budget of their own; the detours draw what remains around the nodes in its way.

Every dataset (32), the default's views, against `nestedEdgesByDetour: false`: 31 identical (the same import times
within a few milliseconds), mixed-large changed: valid geometry (10 issues before: container bounds and areas left),
import 7.7 s → 1.1 s and refinement 31 s → 4.9 s (`bench`, median of 3), 42 % of the area, crossings 976 against 811,
bends 150 against 68. Valid geometry comes first in the quality order, so it is the default; the fingerprint views are
identical.
