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

- The lines through nodes on the small mixed examples are a measurement artefact: the `metrics` drawing carries no node
  shapes, so an edge that ends correctly on a circle's outline runs inside the circle's box and counts as a hit. With
  every node a box, mixed-layouts and self-loops have none (10 and 9 with circles); mixed-large keeps 62 of its 90 in the
  pipeline before single pass, which are real.
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
   and end on the outline of their nodes.
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
- **S1-1: the metrics measure node shapes** (a circle as its disc, as the straight router already does), so that the gate
  counts real lines through nodes; then the real hits of mixed-large are analysed. Done (2026-11-14, [results](../../benchmarks/results/single-pass-mixed-20261114.md)):
  mixed-layouts and self-loops have none; every hit left on mixed-large is a straight one-segment edge of the organic root
  through another node (`straight-through-node`), 44 in the pipeline before single pass and 67 with `singlePassMixed`.
- **S1-1b: straight edges inside a region avoid nodes**: a straight edge that would pass through a node is drawn around it
  (bent at the node's corner) or the region's placement moves the node off the line; measured on mixed-large. Done as
  `straightDetours` (2026-11-14, [results](../../benchmarks/results/straight-detours-20261114.md)): the blockers were
  siblings of the ends inside their layered containers, which the root's placement cannot move, so the edge takes its
  orthogonal route pulled taut around the nodes it does not lie in: 44 lines through nodes become 0, for 68 bends and 90
  crossings more, at the same placement. Under `singlePassMixed` 4 remain (edges that arrive without an orthogonal
  route); that is part of S1-2, whose border ports replace these routes for edges that leave a container.
- **S1-2: border ports for regions in a layered parent** (decision 4), behind `singlePassMixed`. Done as `regionPorts`
  (2026-11-14, [results](../../benchmarks/results/region-ports-20261114.md)): an edge that leaves a region is laid out
  with the region's box as its end, so the parent's routing builds its outer leg to a port on the flow side; a second
  layout puts each port at the height of its node (beside the middle where the region routes axis-parallel, so the leg
  does not share the tree's own line), and the better of the two by the published key is kept. The inner leg is one
  segment where it is free, else the shortest polyline around the region's nodes. On synthetic regions every border end
  is on the flow side and no line runs through a node. Port order: the ports follow the nodes inside, and the parent's
  hierarchical order sees the region at these ports (fixed sub-places, as for a container's border nodes) and is kept
  by the placement for the areas holding such a region; against `singlePassMixed` alone that crosses less on trees and
  radial regions and slightly more on organic ones and the mixed chain.
- **S1-3: layered areas inside regions** (decision 5) and an organic root. Done as `regionChildPorts` (2026-11-14,
  [results](../../benchmarks/results/region-child-ports-20261114.md)): a container with a fixed flow in an organic or
  radial area gets border nodes in its own flow, on the side facing the other end in the area's abstract placement (pivot
  MDS of the area's children, where the organic placement starts); the inner leg follows the plan to the border node, the
  outer leg runs straight or around the container's corner. The border contract holds (5 of 124 synthetic ends off the
  axis, 81 without), crossings drop on the synthetic graphs and rise on mixed-large, and the bends rise about sevenfold,
  most of them inherent to leaving along the flow into an area without one; off by default.
- **S1-4: the default decision** for `singlePassMixed` on the mixed corpus (can-bus, mixed-layouts, mixed-large, self-loops,
  nested-tree-radial, petri-net-shapes) against the pipeline before single pass and elk-rs.

## Consequences

- Single pass becomes the one pipeline for every graph; the older pipeline remains for options single pass does not take.
- A region is a box with ports for its parent: the same contract later serves drill-down (an area seen as a box with ports)
  and other engines as regions.
- Mirroring keeps the drawing's ranks and orders, so a reversed view is the mirror image of the positive one, not a layout
  of its own.
