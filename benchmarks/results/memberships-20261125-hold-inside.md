# Holding a membership group inside its owner: two ways (ADR 0022, M3b feasibility), 2026-11-25

Machine: Apple M3, macOS. Engine revision: `132a049` (PR #176), release build of `layout-command`. No timing. Inputs and planted
groups as in `memberships-20261125-baseline.md`; only the 19 groups inside one container that the default view does not draw
whole, **each tried alone**. `x`: crossings (with shared endpoints), `b`: bends, as a difference to the default view.

- **Order only** (`orderHints`): 2 of 19 become whole. In 7 the container has no layered plan (a tree area); in 4 no rank
  holds both a member and another node, so there is nothing to reorder.
- **Nested container:** 19 of 19 become whole, no line-node hit. Crossings +0 to +2 on 14 of them, +10 on one
  (`far`) and +48 to +187 on the four of `layerweave-code` (576 before, 8 to 32 percent).

Holding several groups at once was not tried, so costs may add up.

## Order only

```
fixture crosses-containers.json focus local (2) first: nothing to reorder | middle: nothing to reorder | last: nothing to reorder
fixture non-contiguous.json concern g3 (3) first: nothing to reorder | middle: nothing to reorder | last: nothing to reorder
fixture pools-and-lanes.json pool customer (3) first: nothing to reorder | middle: nothing to reorder | last: nothing to reorder
fixture pools-and-lanes.json pool supplier (4) first: nothing to reorder | middle: nothing to reorder | last: nothing to reorder
fixture three-dimensions.json team data (2) first: WHOLE hits 0 x 3 b 0 | middle: WHOLE hits 0 x 2 b 0 | last: WHOLE hits 0 x 2 b 0
near layerweave-code.json near g4 (4) first: no hits 0 x 8 b 6 | middle: no hits 0 x 127 b 8 | last: no hits 0 x 156 b 8
near layerweave-code.json near g5 (4) first: no hits 0 x 1 b -2 | middle: no hits 0 x 5 b 0 | last: no hits 0 x 172 b 10
near layerweave-code.json near g6 (4) first: no hits 0 x 198 b 12 | middle: no hits 0 x 90 b 16 | last: no hits 0 x 160 b 0
near layerweave-code.json near g14 (4) first: no hits 0 x 0 b 0 | middle: no hits 0 x 0 b 0 | last: no hits 0 x 0 b 0
near nested-tree-radial.json near g0 no layered plan in team-1-2-0
near nested-tree-radial.json near g2 no layered plan in team-1-1-1
near nested-tree-radial.json near g3 no layered plan in team-2-1-2
near nested-tree-radial.json near g7 no layered plan in team-1-2-2
near nested-tree-radial.json near g10 no layered plan in team-0-0-1
near nested-tree-radial.json near g11 no layered plan in team-1-0-0
near nested-tree-radial.json near g12 no layered plan in team-2-1-0
near network-vlan.json near g1 (4) first: no hits 0 x 0 b 0 | middle: WHOLE hits 0 x 0 b 0 | last: WHOLE hits 0 x 0 b 0
near state-order.json near g0 (4) first: no hits 0 x 0 b 0 | middle: no hits 0 x 0 b 0 | last: no hits 0 x 4 b 2
far state-tcp.json far g0 (4) first: no hits 0 x 3 b 4 | middle: no hits 0 x 0 b 0 | last: no hits 0 x 0 b 0
{"fixture":{"split":5,"noplan":0,"whole":1,"free":0,"cheap":1},"near":{"split":13,"noplan":7,"whole":1,"free":1,"cheap":0},"far":{"split":1,"noplan":0,"whole":0,"free":0,"cheap":0}}
```

## Nested container

```
fixture crosses-containers.json focus local (2) WHOLE hits 0 x 0+0 b 10-2 area 1.16
fixture non-contiguous.json concern g3 (3) WHOLE hits 0 x 0+0 b 4+0 area 2.85
fixture pools-and-lanes.json pool customer (3) WHOLE hits 0 x 0+0 b 6+8 area 1.06
fixture pools-and-lanes.json pool supplier (4) WHOLE hits 0 x 0+1 b 6+6 area 1.00
fixture three-dimensions.json team data (2) WHOLE hits 0 x 0+2 b 31+0 area 1.21
near layerweave-code.json near g4 (4) WHOLE hits 0 x 576+101 b 796-10 area 0.87
near layerweave-code.json near g5 (4) WHOLE hits 0 x 576+187 b 796+38 area 1.05
near layerweave-code.json near g6 (4) WHOLE hits 0 x 576+51 b 796+22 area 1.18
near layerweave-code.json near g14 (4) WHOLE hits 0 x 576+48 b 796+12 area 0.89
near nested-tree-radial.json near g0 (4) WHOLE hits 0 x 0+0 b 112+6 area 1.01
near nested-tree-radial.json near g2 (4) WHOLE hits 0 x 0+1 b 112+8 area 1.01
near nested-tree-radial.json near g3 (4) WHOLE hits 0 x 0+0 b 112+2 area 1.02
near nested-tree-radial.json near g7 (4) WHOLE hits 0 x 0+0 b 112+2 area 1.01
near nested-tree-radial.json near g10 (4) WHOLE hits 0 x 0+0 b 112+2 area 1.00
near nested-tree-radial.json near g11 (4) WHOLE hits 0 x 0+0 b 112+2 area 1.02
near nested-tree-radial.json near g12 (4) WHOLE hits 0 x 0+1 b 112+6 area 1.01
near network-vlan.json near g1 (4) WHOLE hits 0 x 43+2 b 106+23 area 1.24
near state-order.json near g0 (4) WHOLE hits 0 x 0+0 b 19+4 area 1.19
far state-tcp.json far g0 (4) WHOLE hits 0 x 0+10 b 26+22 area 1.85
{"fixture":{"split":5,"noplan":0,"whole":5,"free":2,"cheap":1},"near":{"split":13,"noplan":0,"whole":13,"free":0,"cheap":5},"far":{"split":1,"noplan":0,"whole":1,"free":0,"cheap":0}}
```
