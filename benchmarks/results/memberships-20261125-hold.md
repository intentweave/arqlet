# Membership groups held inside their owner (ADR 0022, M3b), 2026-11-25

Machine: Apple M3, macOS. Revision: branch `membership-holding-inside` on `499799e` plus the change this file came with.
Build: `cargo build --release -p arqlet-runtime --bins --locked`. Command:
`node benchmarks/memberships.mjs --hold --out benchmarks/results/memberships-20261125-hold.jsonl`; two runs byte-identical. No
timing. Inputs and planted groups as in `memberships-20261125-baseline.md`. The quality table compares each view with the same
view without holding, and lists only views that hold something.

Budget per group (placeholders until M3d): crossings at most the larger of 2 and 5 percent of the drawing without holding,
bends the larger of 4 and 5 percent, halved with every rank after the second; no new hard violation; no group that was whole is
broken. The budget is per group, so a drawing's total grows with the number of groups held.

## Findings

- Parent first, inside one container, drawn whole: fixtures 10 to 11 of 15, `near` 11 to 15 of 24, `far` 0 of 1. Five of the
  19 groups that were not whole are held. Cost: no crossing more; bends 10 to 8 and 112 to 120.
- Held alone as containers all 19 become whole (`memberships-20261125-hold-inside.md`): the budget, not the method, decides.
- What releases a group, where traced: breaking another group that was whole (`non-contiguous`, `pools-and-lanes`,
  `network-vlan`), crossings (`layerweave-code` 79 to 171 more, `state-order` 3), bends, and rank 3 (`three-dimensions`, two
  crossings where one is allowed).
- A layout of the same group can differ between two names of the holding container: `state-order` cost no crossing in the
  earlier experiment and three here. The cause was not investigated.
- The across group of `crosses-containers` is whole as well (fixtures across 7 to 8 of 12), as a side effect.

## After the browser check (same day)

A holding container has no title row any more where its area is layered (`memberships-20261125-hold-2.jsonl`, same command).
The same groups are held. The area of `crosses-containers` falls from 339264 to 310080 (291480 without holding); `nested-tree-radial`
is unchanged, because a tree area keeps its title row. Edges against the flow are counted: on `crosses-containers` 1 to 2.

## Tables

uml-class.json near: no group of 4 can be planted

| Source | Input | First | Soft | Inside: whole/split/marked | Across: whole/split/marked | Overlapping pairs |
| --- | --- | --- | --- | --- | --- | --- |
| fixture | crosses-containers | parent | focus | 1: 1/0/0 | 1: 1/0/0 | 0 |
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
| near | nested-tree-radial | parent | near | 15: 12/3/0 | 0: 0/0/0 | 0 |
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
| fixture | crosses-containers | parent (1 held) | 0 → 0 | 0 → 0 | 10 → 8 | 1627 → 1249 | 291480 → 339264 |
| near | nested-tree-radial | parent (4 held) | 0 → 0 | 0 → 0 | 112 → 120 | 21051 → 22092 | 41158839 → 42970691 |

Parent first, totals (groups: whole/split/marked):
plain fixture inside 15: 10/5/0
plain fixture across 12: 7/5/0
plain near inside 24: 11/13/0
plain near across 30: 0/22/8
plain far inside 1: 0/1/0
plain far across 55: 0/4/51
held fixture inside 15: 11/4/0
held fixture across 12: 8/4/0
held near inside 24: 15/9/0
held near across 30: 0/22/8
held far inside 1: 0/1/0
held far across 55: 0/4/51
Refused: near nested-tree-radial, near first: the tree arrangement of the root refuses parallel or antiparallel links: the children do not form a tree
Refused: far nested-tree-radial, far first: the tree arrangement of the root refuses parallel or antiparallel links: the children do not form a tree
