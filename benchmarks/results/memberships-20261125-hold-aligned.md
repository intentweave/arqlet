# Aligning a group's pieces across the flow: feasibility (ADR 0022, M3c), 2026-11-25

Machine: Apple M3, macOS. Revision `3ea1641` (engine as on `1a05010`), release build of `layout-command`. No timing. Command:
`node benchmarks/experiments/membership-hold-aligned.mjs`. The 19 groups with two owners of
`memberships-20261125-hold-across.md`, each tried alone, every member pinned (`pinsInSinglePass`), three places across the flow, three
rounds each; the best of all nine counts, which is an upper bound and not a rule.

## Findings

- Whole: 8 of 19, no new line-node hit; 1 in the M3b budget.
- With the helper edges and nested containers of the earlier experiment (9 of these 19), 12 of 19 become whole by at least one
  way. Aligning adds `crosses-containers` hot-path and two planted groups, **none of them in the budget**: in the budget it stays
  4 of 19 by any way.
- What it costs: 2 crossings on the fixture and on `org-chart`, 131 to 368 on `layerweave-code` (576 before), 146 to 191 on
  `workflow-dag` (553 before).
- The pins were honoured: 639 of 675 within one unit across the flow, all 171 layouts by single pass. Along the flow a pin only
  chooses the rank, so a piece can still share its ranks with other nodes.
- A first version of this probe used order hints. It set none, because helper edges leave a piece alone in its rank, and its
  result (no change) was discarded.

Planted groups; what real groupings cost is open.

## Runs

```
fixture crosses-containers.json focus hot-path WHOLE in 2 layouts, best place 1 round 2 hits 0 x 0 to 2, b 10 to 18, area 1.50
fixture three-dimensions.json team platform never whole
near bpmn-pools.json near g0 never whole
near layerweave-code.json near g0 WHOLE in 9 layouts, best place 1 round 1 hits 0 x 576 to 708, b 796 to 826, area 1.02
near layerweave-code.json near g1 never whole
near layerweave-code.json near g8 never whole
near layerweave-code.json near g9 WHOLE in 3 layouts, best place 1 round 2 hits 0 x 576 to 944, b 796 to 848, area 1.08
near layerweave-code.json near g10 never whole
near layerweave-code.json near g11 never whole
near layerweave-code.json near g12 never whole
near layerweave-code.json near g13 WHOLE in 2 layouts, best place 1 round 2 hits 0 x 576 to 717, b 796 to 826, area 1.04
near network-vlan.json near g0 never whole
near network-vlan.json near g2 never whole
near org-chart.json near g5 never whole
near org-chart.json near g6 WHOLE in 7 layouts, best place 0.5 round 1 hits 0 x 48 to 50, b 335 to 339, area 1.01, in budget
near org-chart.json near g7 never whole
near workflow-dag.json near g4 WHOLE in 2 layouts, best place 1 round 2 hits 0 x 553 to 744, b 785 to 785, area 1.03
near workflow-dag.json near g5 WHOLE in 2 layouts, best place 1 round 2 hits 0 x 553 to 727, b 785 to 781, area 1.05
near workflow-dag.json near g6 WHOLE in 1 layouts, best place 1 round 3 hits 0 x 553 to 699, b 785 to 803, area 1.06
{"fixture":{"groups":2,"bound":1,"noNewHits":1,"cheap":0,"layouts":18,"refused":0},"near":{"groups":17,"bound":7,"noNewHits":7,"cheap":1,"layouts":153,"refused":0}}
```
