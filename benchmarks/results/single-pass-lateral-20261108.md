# Single pass: side nodes beside their leaf (ADR 0017, amendment A, steps A1–A2), 2026-11-08

Revision `53039dc` against `53039dc` plus lateral side nodes, Apple M3, release builds. The 11 mixed datasets, unmodified (raw
task), cut to 150 nodes, five seeds (`--seeds 5`), metric means over the seeds. "Before" is the same code at `e12c88f`/`53039dc`
([gate report](single-pass-gate-mixed-20261107.md), follow-up); "default" is the refined default of the gate report. Load was not
controlled; no time is claimed.

Raw: [single-pass-lateral-20261108.json](single-pass-lateral-20261108.json). Command:
`node benchmarks/eval/single-pass.mjs --corpus --raw --repeat 1 --seeds 5 --arms singlePass --no-elk DATASETS…`.

## The change

- **A1, placement.** In the placement that makes the geometry (not in the order-only one, so the hierarchical order is chosen on
  the side nodes after their leaves), a side node moves into its leaf's rank, beside it, without a link, when the leaf stands
  outermost on that side among the rank's elements other than side nodes (`lateral_sides`).
- **A2, construction.** The leaf's port faces the side (`end_sides`) at the side port's place (`port_lanes`), and the leg from
  the leaf to the side port is one straight segment, built without lanes or tracks (`lateral_leg`). The join turns a two-point
  leg to fit, since the realization gives the other legs of a backward edge in logical direction.
- Two fixes found on the way, both needed for the result: a container's port order along its side now counts the side nodes'
  ranks (the place within a rank alone mixed the ports of side nodes in different ranks: org-chart 98 → 70 crossings); and the
  side nodes of one child are ordered by the places of their edges outside (in the parent's plan of the earlier pass), so that the
  straight legs of one leaf do not cross outside (synthetic-2k 4134 → 3694, org-chart 70 → 55).

## Result

| Five-seed means | before | after | default |
|---|---:|---:|---:|
| sum of crossings | 4844.6 | **4622.6** | 5135.4 |
| sum of bends | 4329 | **3741** | 3110 |
| org-chart crossings / bends | 68 / 424 | 55 / 373 | 122 / 308 |
| synthetic-2k crossings / bends | 3927.8 / 2051 | 3693.8 / 1841 | 4064.4 / 1520 |
| cloud-architecture crossings / bends | 36.2 / 220 | 36.8 / 141 | 31 / 84 |
| activity-order crossings / bends | 72.4 / 148 | 86.4 / 79 | 86 / 88 |
| network-vlan crossings / bends | 35.8 / 162 | 43.6 / 107 | 45 / 104 |

Against the default: 9 better, 2 worse (cloud-architecture by crossings, uml-class 8.6 / 8), as before. Against the state before,
per view lexicographically: 3 better (org-chart, state-order, synthetic-2k), 1 equal (deployment-cluster), 7 worse: activity-order
+14 crossings, network-vlan +7.8, bpmn-pools +2.8, layerweave-pipeline +0.8, cloud-architecture +0.6, uml-class +0.4, workflow-dag
by 0.6 shared line pairs (crossings −1.4). Bends fall on every view but deployment-cluster (no lateral exits; uml-class by only 1).
Activity-order (79 / 88) and layerweave-pipeline (78 / 80) now draw fewer bends than the default, network-vlan nearly as few
(107 / 104).

Default, uniform and frozen views: byte-identical (the default on all 31 datasets at threads 1 and 4; single pass on every uniform
and frozen view, which have no side nodes).

## Intermediate trials

- First version: 8 edges on activity-order and cloud-architecture, 4 on org-chart, 50 on synthetic-2k delegated as `NonMonotone`,
  because the straight leg of a backward edge came out in construction direction; fixed in the join.
- Converting side nodes in the order-only placement too: crossings 5117.8 against 5115.6; the hierarchical order then saw side
  nodes without links. Kept to the geometry placement.

## Open

- A3, middle areas (an end that is itself a child of the parent's axis, db-a's data-a inside az-a on cloud-architecture): not
  done; cloud-architecture keeps its gap.
- A5, an order that prefers lateral ends at the rank's end: not done.
- The 7 small regressions against the state before, mostly crossings outside the area where a lateral leg's port now lies one
  rank earlier along the side.
