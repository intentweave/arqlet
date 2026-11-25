# Holding a membership group across containers: feasibility (ADR 0022, M3c), 2026-11-25

Machine: Apple M3, macOS. Revision `1a05010`, release build of `layout-command`. No timing. Command:
`node benchmarks/experiments/membership-hold-across.mjs`. The 35 groups across containers that the default view does not draw
whole (5 of the fixtures, 30 planted along edges as in `memberships-20261125-baseline.md`), **each tried alone**.

`pieces`: the members inside each owner as a nested container. `border`: helper edges push each piece to the border of its owner
that faces the others. `pull`: a helper edge between the pieces as well. Helpers are taken out before measuring; the room of
their channels stays, so areas are upper bounds.

## Findings

- Whole: `pieces` 1, `border` 7, `pull` 9, by at least one of the three 10 of 35. Nine of the ten have two owners (of 19 such
  groups), one has three.
- Within the M3b budget: 5 of 35 (one fixture, four planted).
- Where `pull` fails (26): the rectangle encloses only foreign leaves of the group's own owners in 14, only leaves of other
  containers in 7, both in 5. So most failures are about placement inside the owners, which helper edges move along the flow
  but not across it; aligning the pieces across the flow is the missing lever and was not tried.
- Crossings as the cost: mostly small on `org-chart` and the fixtures, up to +396 on `layerweave-code` (576 before).

These are planted groups, tried alone; they say what the levers reach, not what real groupings cost.

## Runs

```
fixture crosses-containers.json focus hot-path 3 members, 2 owners: pieces no hits 0 x 0+0 b 10+0 area 1.18 | border no hits 0 x 0+0 b 10+0 area 1.18 | pull no hits 0 x 0+0 b 10+4 area 1.16
fixture three-dimensions.json zone internal 5 members, 3 owners: pieces no hits 0 x 0+0 b 31+0 area 1.29 | border no hits 0 x 0+0 b 31+6 area 1.58 | pull no hits 0 x 0+0 b 31+10 area 1.60
fixture three-dimensions.json team platform 4 members, 2 owners: pieces no hits 0 x 0+0 b 31+0 area 1.12 | border WHOLE hits 0 x 0+0 b 31+0 area 0.94 | pull WHOLE hits 0 x 0+0 b 31+2 area 0.95
fixture three-dimensions.json layer api 3 members, 3 owners: pieces no hits 0 x 0+0 b 31+0 area 1.00 | border no hits 0 x 0+0 b 31+0 area 1.07 | pull no hits 0 x 0+0 b 31+2 area 1.05
fixture three-dimensions.json layer logic 4 members, 3 owners: pieces no hits 0 x 0+0 b 31-2 area 1.13 | border no hits 0 x 0+0 b 31-4 area 1.14 | pull no hits 0 x 0+0 b 31-2 area 1.14
near bpmn-pools.json near g0 4 members, 2 owners: pieces WHOLE hits 0 x 12+0 b 59-8 area 1.28 | border no hits 0 x 12+2 b 59-11 area 1.23 | pull no hits 0 x 12-2 b 59-7 area 1.39
near bpmn-pools.json near g1 4 members, 3 owners: pieces no hits 0 x 12+3 b 59+1 area 1.04 | border no hits 0 x 12+2 b 59+5 area 1.48 | pull no hits 0 x 12+8 b 59+16 area 1.35
near layerweave-code.json near g0 4 members, 2 owners: pieces no hits 0 x 576+96 b 796+0 area 0.87 | border no hits 0 x 576+130 b 796+42 area 1.10 | pull WHOLE hits 0 x 576+182 b 796+10 area 1.07
near layerweave-code.json near g1 4 members, 2 owners: pieces no hits 0 x 576+259 b 796+12 area 1.02 | border no hits 0 x 576+19 b 796+28 area 1.56 | pull no hits 0 x 576+268 b 796+76 area 1.20
near layerweave-code.json near g3 4 members, 4 owners: pieces no hits 0 x 576+0 b 796+0 area 1.00 | border no hits 0 x 576+122 b 796+72 area 1.03 | pull no hits 0 x 576+51 b 796+40 area 1.05
near layerweave-code.json near g7 4 members, 3 owners: pieces no hits 0 x 576+92 b 796+12 area 1.10 | border no hits 0 x 576+182 b 796+38 area 1.20 | pull no hits 0 x 576+43 b 796+20 area 0.83
near layerweave-code.json near g8 4 members, 2 owners: pieces no hits 0 x 576+41 b 796+22 area 0.96 | border no hits 0 x 576+105 b 796+14 area 1.49 | pull no hits 0 x 576+59 b 796+70 area 1.17
near layerweave-code.json near g9 4 members, 2 owners: pieces no hits 0 x 576+156 b 796+50 area 0.97 | border no hits 0 x 576+211 b 796+88 area 1.03 | pull no hits 0 x 576+396 b 796+44 area 1.08
near layerweave-code.json near g10 4 members, 2 owners: pieces no hits 0 x 576+54 b 796+28 area 1.09 | border no hits 0 x 576+229 b 796+48 area 1.47 | pull no hits 0 x 576+191 b 796+18 area 1.46
near layerweave-code.json near g11 4 members, 2 owners: pieces no hits 0 x 576+270 b 796+38 area 0.99 | border no hits 0 x 576+21 b 796+34 area 1.11 | pull no hits 0 x 576+88 b 796-12 area 1.43
near layerweave-code.json near g12 4 members, 2 owners: pieces no hits 0 x 576+63 b 796-30 area 1.16 | border no hits 0 x 576+167 b 796+54 area 1.61 | pull no hits 0 x 576+296 b 796+74 area 1.44
near layerweave-code.json near g13 4 members, 2 owners: pieces no hits 0 x 576-6 b 796+2 area 1.05 | border WHOLE hits 0 x 576+85 b 796+4 area 1.33 | pull WHOLE hits 0 x 576+340 b 796+8 area 1.03
near network-vlan.json near g0 4 members, 2 owners: pieces no hits 0 x 43-4 b 106+12 area 1.27 | border no hits 0 x 43-2 b 106+14 area 1.52 | pull no hits 0 x 43+5 b 106+18 area 1.65
near network-vlan.json near g2 4 members, 2 owners: pieces no hits 0 x 43+3 b 106+27 area 1.09 | border no hits 0 x 43+7 b 106+21 area 1.25 | pull no hits 0 x 43+7 b 106+25 area 1.26
near org-chart.json near g0 4 members, 3 owners: pieces no hits 0 x 48+0 b 335+0 area 1.02 | border WHOLE hits 0 x 48+0 b 335+2 area 0.99 | pull WHOLE hits 0 x 48+0 b 335+2 area 0.99
near org-chart.json near g1 4 members, 4 owners: pieces no hits 0 x 48+0 b 335+0 area 1.00 | border no hits 0 x 48-7 b 335-16 area 1.45 | pull no hits 0 x 48+7 b 335-18 area 1.27
near org-chart.json near g2 4 members, 3 owners: pieces no hits 0 x 48+0 b 335+3 area 1.01 | border no hits 0 x 48+3 b 335+6 area 1.00 | pull no hits 0 x 48+2 b 335+14 area 1.00
near org-chart.json near g3 4 members, 3 owners: pieces no hits 0 x 48+0 b 335+1 area 1.02 | border no hits 0 x 48+0 b 335+1 area 1.02 | pull no hits 0 x 48+0 b 335+3 area 1.02
near org-chart.json near g4 4 members, 3 owners: pieces no hits 0 x 48+1 b 335+3 area 1.01 | border no hits 0 x 48+4 b 335-6 area 1.05 | pull no hits 0 x 48+4 b 335-8 area 1.03
near org-chart.json near g5 4 members, 2 owners: pieces no hits 0 x 48+6 b 335+1 area 1.02 | border no hits 0 x 48+14 b 335-3 area 1.06 | pull WHOLE hits 0 x 48-1 b 335-11 area 0.97
near org-chart.json near g6 4 members, 2 owners: pieces no hits 0 x 48+1 b 335+2 area 1.01 | border WHOLE hits 0 x 48-1 b 335-1 area 0.99 | pull WHOLE hits 0 x 48-1 b 335+1 area 0.99
near org-chart.json near g7 4 members, 2 owners: pieces no hits 0 x 48+1 b 335+2 area 1.01 | border WHOLE hits 0 x 48+4 b 335+3 area 0.98 | pull WHOLE hits 0 x 48+4 b 335+3 area 0.98
near workflow-dag.json near g0 4 members, 3 owners: pieces no hits 0 x 553+73 b 785+43 area 0.77 | border no hits 0 x 553+107 b 785+42 area 0.86 | pull no hits 0 x 553+107 b 785+44 area 0.97
near workflow-dag.json near g1 4 members, 3 owners: pieces no hits 0 x 553+13 b 785-2 area 0.77 | border no hits 0 x 553+68 b 785-15 area 0.81 | pull no hits 0 x 553+30 b 785-47 area 0.93
near workflow-dag.json near g2 4 members, 3 owners: pieces no hits 0 x 553+19 b 785+38 area 0.90 | border no hits 0 x 553+110 b 785+9 area 0.92 | pull no hits 0 x 553+150 b 785-8 area 0.98
near workflow-dag.json near g3 4 members, 4 owners: pieces no hits 0 x 553+0 b 785+0 area 1.00 | border no hits 0 x 553+58 b 785-21 area 0.84 | pull no hits 0 x 553+75 b 785+3 area 0.90
near workflow-dag.json near g4 4 members, 2 owners: pieces no hits 0 x 553-36 b 785+48 area 1.00 | border WHOLE hits 0 x 553+64 b 785+4 area 0.87 | pull WHOLE hits 0 x 553+60 b 785+21 area 0.90
near workflow-dag.json near g5 4 members, 2 owners: pieces no hits 0 x 553+12 b 785+39 area 0.81 | border WHOLE hits 0 x 553+65 b 785+10 area 1.17 | pull WHOLE hits 0 x 553+73 b 785-12 area 0.90
near workflow-dag.json near g6 4 members, 2 owners: pieces no hits 0 x 553+26 b 785+30 area 0.78 | border no hits 0 x 553+90 b 785+1 area 0.90 | pull no hits 0 x 553+90 b 785-12 area 0.95
near workflow-dag.json near g7 4 members, 4 owners: pieces no hits 0 x 553+0 b 785+0 area 1.00 | border no hits 0 x 553+63 b 785+2 area 0.76 | pull no hits 0 x 553+102 b 785-20 area 0.78
{"fixture":{"groups":5,"two":2,"pieces":0,"border":1,"pull":1,"any":1,"cheap":1,"refused":0,"pullFailsOwnOnly":4,"pullFailsOtherOnly":0,"pullFailsBoth":0},"near":{"groups":30,"two":17,"pieces":1,"border":6,"pull":8,"any":9,"cheap":4,"refused":0,"pullFailsOwnOnly":10,"pullFailsOtherOnly":7,"pullFailsBoth":5}}
```
