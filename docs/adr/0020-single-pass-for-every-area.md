# ADR 0020: Single pass for every area: reversed flows, and regions of other strategies with border ports

Date: 2026-11-13.
Status: proposed. Steps S1-0 to S1-4 below, each behind an option and measured before the next.

## Context

Single pass (ADR 0016, default since engine 0.2) builds every edge of a layered drawing in one frame per area, and since ADR
0017 also across areas of different axes. It still leaves whole graphs to the pipeline before it in two cases, reported as
`Single pass was not used: …`:

1. **Reversed flows** (`rl`, `bt`): "Fixed-order channel growth requires positive flow directions". Measured on the layered
   playground examples with every area set to one direction (2026-11-13): `rl` and `bt` cross 2–4 times as often as `lr`
   and `tb` and bend more (activity-order 49 against 14 crossings, arqlet-pipeline 28 against 5, bpmn-pools 10 against 1),
   although they are mirror images.
2. **Areas of another strategy** (organic, tree, radial) or straight routing: "Routing completion requires the legacy
   router…" or "Abstract routing requires unflattened hierarchical placement…". One such area anywhere switches single pass
   off for the whole graph. `singlePassMixed` (provisional, 2026-11-09) keeps such graphs in single pass: the other areas
   keep their own placement and their edges go to the router that completes single pass. Measured then and again on
   2026-11-13 (`benchmarks/eval/single-pass-mixed.mjs`): can-bus better (27 against 32 bends), mixed-layouts and self-loops
   equal, mixed-large worse (117 against 90 lines through nodes, at half the area), so it stays off.

Free directions are handled by `directionsFirst` (S2b: candidates fixed under single pass).

Two findings shape the steps:

- Part of the lines through nodes on the mixed examples are straight edges of organic areas that start or end at the
  centre of their own node instead of its border (4 of 10 on mixed-layouts, 4 of 9 on self-loops). That is a geometry
  defect of the organic edges, independent of single pass.
- Edges between a layered area and a region of another strategy are not constructed by single pass but completed by the
  older router, without the border contract ADR 0017 gives areas of another axis.

## Decision

Single pass takes every area. An area that single pass cannot place itself is a **region**: it is placed by its own
strategy, and it offers **border ports** to the area around it, as ADR 0017's areas of another axis do.

1. **Reversed flows.** An area with a reversed flow is laid out with the positive flow of the same axis and mirrored, with
   its edges, on that axis within its box. Logical edge direction is kept: only coordinates are mirrored. A graph whose
   areas are all reversed is the first case (mirror the whole drawing); mixed senses follow in the same way per area once
   areas can be mirrored as units.
2. **Regions.** Organic, tree and radial areas, and areas with straight routing, are regions: their children keep the
   placement of their strategy (as with `singlePassMixed`).
3. **Edges inside a region** are drawn by the region's routing (straight for organic and radial, the tree's own for tree),
   and end on the border of their nodes, never inside them.
4. **Border ports.** Every edge that crosses a region's border gets a port on the region's box, placed by the region after
   its placement: on the side the surrounding flow uses (ADR 0017, rule 1), at the point closest to the inner end, ports of
   one side a track apart. The inner leg is the region's routing from the node to its port; the outer leg is an ordinary
   single-pass chain of the parent to the port. The route is the legs joined in logical direction.
5. **Layered areas inside a region** (a layered child of an organic parent) are boxes in the region's placement; their own
   edges to the outside leave by their border nodes as today, and the region routes from those border points.
6. **Quality gate** for every step, as for every algorithm change (`CLAUDE.md`): valid geometry, every edge in logical
   direction, no line through a node, crossings and bends against the pipeline before single pass and elk-rs on the same
   views, recorded time. Defaults change only when the gate holds on the corpus.

## Steps

- **S1-0: reversed flows by mirroring**, first for graphs whose areas are all reversed. Measure `rl`/`bt` against `lr`/`tb`
  on the layered examples; expected: equal crossings and bends. Done for `rl` (2026-11-14, `singlePassReversed`,
  [results](../../benchmarks/results/reversed-flows-20261114.md)): the mirrored drawing equals `lr`. Not for `bt`: a
  container's header is at its top in both senses, so a mirror on the vertical axis moves children into the header and
  shifts the axis by a header per level of nesting; `bt` needs single pass with a negative flow on the vertical axis, or a
  header-aware transform (step S1-0b).
- **S1-1: organic edges end on their nodes' borders** (straight routes clipped to the boxes). Fixes the own-node part of the
  lines through nodes in every pipeline.
- **S1-2: border ports for regions in a layered parent** (decision 4), behind `singlePassMixed`.
- **S1-3: layered areas inside regions** (decision 5) and an organic root.
- **S1-4: the default decision** for `singlePassMixed` on the mixed corpus (can-bus, mixed-layouts, mixed-large, self-loops,
  nested-tree-radial, petri-net-shapes) against the pipeline before single pass and elk-rs.

## Consequences

- Single pass becomes the one pipeline for every graph; the older pipeline remains for options single pass does not take.
- A region is a box with ports for its parent: the same contract later serves drill-down (an area seen as a box with ports)
  and other engines as regions.
- Mirroring keeps the drawing's ranks and orders, so a reversed view is the mirror image of the positive one, not a layout
  of its own.
