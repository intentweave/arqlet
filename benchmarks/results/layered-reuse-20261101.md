# Layered construction: plan kept with the snapshot and reused, 2026-11-01

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`. Revision cce23c3 plus
uncommitted changes (behind `layeredConstruction`; default output unchanged, 16 of 16 refinement fingerprints).
`bench --reps 3 --threads 1`, medians; limit 100 (er-diagram and tree-100: 1000); layerweave-code is the graph of 0bcfd56
(669 / 1084). "before" is `layered-space-20261101.jsonl` (cce23c3). The machine was not idle (petri-net's 340 ms before
is an outlier; measured again then: 33 ms).

What changed (all behind the option):
- `Snapshot::layered_plans` (provisional, versioned with `STORED_PLAN_FORMAT`, omitted when empty) keeps every layered
  area's plan: ranks with dummies and border nodes, chains, reversed links, dummy places, border nodes.
- An edit keeps the stored plan of every area it does not place again (the area exists, is not dirty, and its plan holds
  exactly its children). Before, those areas had no plan in an edit, so the ports and the construction saw only the
  areas placed again.
- An area that is placed again (in an edit, or by a refinement candidate) starts its order improvement from its stored
  order when that order has no more bilayer crossings than the sweeps' order (`layered_plan::follow_order`). Plans that
  were neither kept nor placed again are dropped after the placement.

## Refinement (import, then refine; stored plans as the start of the order)

Columns: before / now.

| dataset | crossings | length | refinement ms | placement stage ms | hard |
|---|---|---|---|---|---|
| petri-net | 68 / 68 | 11840 / 11840 | 340 / 35 | 8 / 1 | 0 |
| er-diagram | 0 / 0 | 3089 / 3089 | 14 / 10 | 1 / 0 | 0 |
| state-order | 0 / 0 | 2760 / 2760 | 4 / 3 | 0 / 0 | 0 |
| activity-order | 54 / 54 | 18996 / 18996 | 35 / 33 | 1 / 1 | 0 |
| uml-class | 5 / 5 | 12200 / 12200 | 19 / 19 | 1 / 1 | 0 |
| bpmn-pools | 6 / 6 | 11305 / 11305 | 17 / 17 | 1 / 1 | 0 |
| network-vlan | 64 / 64 | 26186 / 26186 | 37 / 36 | 2 / 1 | 0 |
| layerweave-pipeline | 38 / 38 | 15778 / 15778 | 44 / 32 | 2 / 1 | 0 |
| cloud-architecture | 27 / 30 | 26113 / 26617 | 198 / 188 | 2 / 1 | 0 |
| mixed-layouts | 5 / 5 | 3715 / 3715 | 23 / 23 | 0 / 0 | 25 |
| nested-tree-radial | 0 / 0 | 18099 / 18099 | 72 / 72 | 0 / 0 | 0 |
| tree-100 | 0 / 0 | 27885 / 27885 | 31 / 31 | 2 / 2 | 196 |
| state-tcp | 0 / 0 | 6018 / 6018 | 5 / 5 | 0 / 0 | 0 |
| deployment-cluster | 116 / 116 | 43969 / 43969 | 88 / 87 | 7 / 6 | 0 |
| self-loops | 0 / 0 | 2553 / 2553 | 7 / 7 | 0 / 0 | 23 |
| layerweave-code | 984 / 984 | 306700 / 306700 | 1126 / 1115 | 143 / 62 | 0 |

Totals over the 13 layered datasets (before / now): crossings 1251 / 1254; length 484666 / 485171; placement stage 160 / 71 ms.

The placement stage of layerweave-code takes 62 instead of 143 ms: the order improvement starts at the stored order and
stops after its idle sweeps. The refinement as a whole is routing-bound, and its total does not change measurably. One
dataset changes its drawing: cloud-architecture ends with 30 instead of 27 crossings (the stored order is the start of a
different local optimum).

## Edits (five add edits in a row, `layout-command`, import with the option, no refinement)

Sources: the leaves in id order at 10, 30, 50, 70 and 90 % of the list. Pairs of siblings that share a rank (equal centre
along the flow) before and after an edit, and whose order across the flow swapped; nodes whose absolute box moved by more
than 1. Columns: cce23c3 / now. Raw rows: `layered-reuse-edits-20261101.jsonl`.

| dataset | swapped pairs in a rank | pairs compared | nodes moved | node positions compared |
|---|---|---:|---|---:|
| petri-net | 21 / 2 | 375 | 49 / 25 | 130 |
| activity-order | 0 / 0 | 280 | 118 / 118 | 165 |
| uml-class | 0 / 0 | 66 | 84 / 84 | 225 |
| bpmn-pools | 0 / 0 | 54 | 59 / 59 | 140 |
| network-vlan | 0 / 0 | 314 | 123 / 119 | 195 |
| layerweave-pipeline | 8 / 7 | 106 | 85 / 86 | 170 |
| cloud-architecture | 0 / 0 | 31 | 105 / 105 | 180 |
| deployment-cluster | 0 / 0 | 54 | 45 / 44 | 175 |
| layerweave-code | 40 / 11 | 2013 | 291 / 232 | 640 |

Cost: the stored plans are 153 KB of the 380 KB snapshot of layerweave-code (40 %), mostly dummy ids and dummy places. An
add edit on layerweave-code, measured as a whole `layout-command` call with process start and JSON (7 runs, median, two
rounds, alternating binaries), takes 47 instead of 44 ms.

Not done: the routes of an edit still come from the local legacy routing (`local_routes`), which does not use the plans;
the plan is not invalidated by its inputs (only by the edit's dirty areas and a check of the area's children); a handoff
to the browser carries the plans but the WASM path has not been measured with them.
