# Membership groups when nothing holds them (ADR 0022, M3a), 2026-11-25

Machine: Apple M3, macOS. Engine revision: `132a049` (PR #176). Build: `cargo build --release -p arqlet-runtime --bins
--locked`. Command: `node benchmarks/memberships.mjs --out benchmarks/results/memberships-20261125-baseline.jsonl`; two runs
byte-identical. No timing. Imports at limit 200, no refinement. Seed 22, planted groups of four leaves over about half the
leaves: `near` grows a group along edges, `far` deals shuffled leaves. **Planted groups are synthetic:** they show how the
stages behave, not what real groupings look like. Inputs: the seven membership fixtures and nine graphs of
`benchmarks/inputs` (all but `synthetic-2k`).

`Inside`: all shown members have one container in the view (what holding within an owner, M3b, can improve). `Across`:
several containers (needs regions across owners, M3c). The second table compares a dimension taken first, as real
containers, with the default view of the same graph. It is **not** the cost of holding: there the containment stays first
and holding may be broken. It is the cost of the strictest alternative.

## Findings

- Parent first, groups drawn whole: fixtures 10 of 15 inside and 7 of 12 across; `near` 11 of 24 inside and **0 of 30
  across**; `far` 0 of 1 inside and 0 of 55 across. Marked: 8 of the `near` groups and 51 of the `far` groups.
- For groups that follow the edges, M3b can improve at most 13 groups and M3c at most 30. M3c has the larger reach; M3b is
  the smaller step and the one a piece needs first. Scattered groups (`far`) end as marks and should: holding them is
  what a budget must refuse.
- `near` is drawn with no overlapping pairs on any input, so overlap does not need a budget of its own there.
- As containers, crossings rise on most corpus inputs (`layerweave-code` 576 to 1708 `near`, 2633 `far`) but fall on
  `workflow-dag` (553 to 430) and `network-vlan` (43 to 23) for `near`: crossings alone do not predict the cost.
- No line-node hits in any view.
- **Refused:** `nested-tree-radial` with a dimension first. The root's tree arrangement no longer finds a tree. A projection
  can break what an arrangement requires; today the import is an error. Open.
- `uml-class` has no four leaves along edges, so it has no `near` row.

## What this does not measure

Real groupings. The real Intentweave export carries memberships but no resolved call edge, so
it cannot show what holding costs in crossings and bends.

## Tables

| Source | Input | First | Soft | Inside: whole/split/marked | Across: whole/split/marked | Overlapping pairs |
| --- | --- | --- | --- | --- | --- | --- |
| fixture | crosses-containers | parent | focus | 1: 0/1/0 | 1: 0/1/0 | 0 |
| fixture | non-contiguous | parent | concern | 3: 2/1/0 | 0: 0/0/0 | 3 |
| fixture | one-group-per-node | parent | zone | 2: 2/0/0 | 0: 0/0/0 | 1 |
| fixture | org-chart-projects | parent | project | 1: 1/0/0 | 2: 2/0/0 | 1 |
| fixture | org-chart-projects | parent | site | 0: 0/0/0 | 2: 2/0/0 | 0 |
| fixture | org-chart-projects | project | site | 0: 0/0/0 | 2: 1/1/0 | 0 |
| fixture | org-chart-projects | site | project | 2: 2/0/0 | 1: 1/0/0 | 1 |
| fixture | overlapping-boxes | parent | team | 0: 0/0/0 | 1: 1/0/0 | 0 |
| fixture | pools-and-lanes | parent | pool | 2: 0/2/0 | 0: 0/0/0 | 0 |
| fixture | pools-and-lanes | parent | phase | 3: 3/0/0 | 0: 0/0/0 | 1 |
| fixture | pools-and-lanes | pool | phase | 0: 0/0/0 | 3: 3/0/0 | 2 |
| fixture | pools-and-lanes | phase | pool | 0: 0/0/0 | 2: 0/2/0 | 0 |
| fixture | three-dimensions | parent | zone | 1: 1/0/0 | 2: 1/1/0 | 2 |
| fixture | three-dimensions | parent | team | 2: 1/1/0 | 1: 0/1/0 | 0 |
| fixture | three-dimensions | parent | layer | 0: 0/0/0 | 3: 1/2/0 | 1 |
| fixture | three-dimensions | zone | team | 0: 0/0/0 | 3: 0/3/0 | 0 |
| fixture | three-dimensions | zone | layer | 1: 1/0/0 | 2: 0/2/0 | 0 |
| fixture | three-dimensions | team | zone | 1: 1/0/0 | 2: 1/0/1 | 0 |
| fixture | three-dimensions | team | layer | 0: 0/0/0 | 3: 1/2/0 | 2 |
| fixture | three-dimensions | layer | zone | 1: 1/0/0 | 2: 1/1/0 | 2 |
| fixture | three-dimensions | layer | team | 0: 0/0/0 | 3: 0/3/0 | 1 |
| near | bpmn-pools | parent | near | 0: 0/0/0 | 2: 0/2/0 | 0 |
| far | bpmn-pools | parent | far | 0: 0/0/0 | 2: 0/1/1 | 0 |
| near | layerweave-code | parent | near | 5: 1/4/0 | 10: 0/8/2 | 0 |
| far | layerweave-code | parent | far | 0: 0/0/0 | 15: 0/2/13 | 0 |
| near | nested-tree-radial | parent | near | 15: 8/7/0 | 0: 0/0/0 | 0 |
| far | nested-tree-radial | parent | far | 0: 0/0/0 | 15: 0/0/15 | 0 |
| near | network-vlan | parent | near | 2: 1/1/0 | 2: 0/2/0 | 0 |
| far | network-vlan | parent | far | 0: 0/0/0 | 4: 0/0/4 | 0 |
| near | org-chart | parent | near | 0: 0/0/0 | 8: 0/7/1 | 0 |
| far | org-chart | parent | far | 0: 0/0/0 | 8: 0/0/8 | 0 |
| near | state-order | parent | near | 1: 0/1/0 | 0: 0/0/0 | 0 |
| far | state-order | parent | far | 0: 0/0/0 | 1: 0/0/1 | 0 |
| near | state-tcp | parent | near | 1: 1/0/0 | 0: 0/0/0 | 0 |
| far | state-tcp | parent | far | 1: 0/1/0 | 0: 0/0/0 | 0 |
| far | uml-class | parent | far | 0: 0/0/0 | 2: 0/0/2 | 0 |
| near | workflow-dag | parent | near | 0: 0/0/0 | 8: 0/3/5 | 0 |
| far | workflow-dag | parent | far | 0: 0/0/0 | 8: 0/1/7 | 0 |

| Source | Input | First | Line-node hits | Crossings | Bends | Length | Area |
| --- | --- | --- | --- | --- | --- | --- | --- |
| fixture | crosses-containers | focus | 0 → 0 | 0 → 2 | 10 → 12 | 1627 → 2208 | 291480 → 265440 |
| fixture | non-contiguous | concern | 0 → 0 | 0 → 0 | 4 → 8 | 1140 → 1544 | 70794 → 229008 |
| fixture | one-group-per-node | zone | 0 → 0 | 0 → 0 | 4 → 8 | 660 → 1179 | 89856 → 258336 |
| fixture | org-chart-projects | project | 0 → 0 | 0 → 0 | 15 → 15 | 1490 → 1798 | 355080 → 383940 |
| fixture | org-chart-projects | site | 0 → 0 | 0 → 0 | 15 → 13 | 1490 → 1099 | 355080 → 330597 |
| fixture | overlapping-boxes | team | 0 → 0 | 1 → 1 | 14 → 10 | 1545 → 1323 | 178503 → 200186 |
| fixture | pools-and-lanes | pool | 0 → 0 | 0 → 0 | 6 → 14 | 611 → 1606 | 178020 → 230112 |
| fixture | pools-and-lanes | phase | 0 → 0 | 0 → 0 | 6 → 10 | 611 → 1383 | 178020 → 381282 |
| fixture | three-dimensions | zone | 0 → 0 | 0 → 0 | 31 → 17 | 5049 → 2047 | 620004 → 563550 |
| fixture | three-dimensions | team | 0 → 0 | 0 → 0 | 31 → 29 | 5049 → 4854 | 620004 → 604170 |
| fixture | three-dimensions | layer | 0 → 0 | 0 → 1 | 31 → 23 | 5049 → 3050 | 620004 → 512316 |
| near | bpmn-pools | near | 0 → 0 | 12 → 14 | 59 → 77 | 14449 → 20455 | 1939941 → 2191460 |
| far | bpmn-pools | far | 0 → 0 | 12 → 20 | 59 → 74 | 14449 → 18100 | 1939941 → 2317402 |
| near | layerweave-code | near | 0 → 0 | 576 → 1708 | 796 → 990 | 508817 → 856910 | 58356612 → 70654059 |
| far | layerweave-code | far | 0 → 0 | 576 → 2633 | 796 → 1062 | 508817 → 1366084 | 58356612 → 116886000 |
| near | network-vlan | near | 0 → 0 | 43 → 23 | 106 → 193 | 31751 → 44536 | 3043047 → 3954054 |
| far | network-vlan | far | 0 → 0 | 43 → 92 | 106 → 201 | 31751 → 74920 | 3043047 → 7385433 |
| near | org-chart | near | 0 → 0 | 48 → 117 | 335 → 435 | 88170 → 164650 | 17883992 → 22380299 |
| far | org-chart | far | 0 → 0 | 48 → 340 | 335 → 364 | 88170 → 358852 | 17883992 → 49838290 |
| near | state-order | near | 0 → 0 | 0 → 3 | 19 → 19 | 2374 → 3700 | 427488 → 535612 |
| far | state-order | far | 0 → 0 | 0 → 3 | 19 → 27 | 2374 → 4772 | 427488 → 549978 |
| near | state-tcp | near | 0 → 0 | 0 → 1 | 26 → 46 | 5286 → 6267 | 356580 → 574560 |
| far | state-tcp | far | 0 → 0 | 0 → 10 | 26 → 48 | 5286 → 10463 | 356580 → 660144 |
| far | uml-class | far | 0 → 0 | 6 → 4 | 49 → 46 | 13873 → 12615 | 3626910 → 4821504 |
| near | workflow-dag | near | 0 → 0 | 553 → 430 | 785 → 594 | 305473 → 305218 | 11826879 → 29278158 |
| far | workflow-dag | far | 0 → 0 | 553 → 742 | 785 → 674 | 305473 → 580205 | 11826879 → 33754268 |

Parent first, totals (groups: whole/split/marked):
fixture inside 15: 10/5/0
fixture across 12: 7/5/0
near inside 24: 11/13/0
near across 30: 0/22/8
far inside 1: 0/1/0
far across 55: 0/4/51
Refused: near nested-tree-radial, near first: the tree arrangement of the root refuses parallel or antiparallel links: the children do not form a tree
Refused: far nested-tree-radial, far first: the tree arrangement of the root refuses parallel or antiparallel links: the children do not form a tree
