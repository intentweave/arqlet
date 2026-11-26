# ADR 0020: Single pass for every area: reversed flows, and regions of other strategies with border ports

Date: 2026-11-13.
Status: accepted (2026-11-14, S1-4): `regionPorts`, `straightDetours` and `singlePassReversed` are on by default;
`singlePassMixed` and `regionChildPorts` stay options. Steps S1-0 to S1-4 below, each behind an option and measured before
the next.
Amendment (2026-11-23, proposed): routes planned inside organic regions, see the end.

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
  [results](../../benchmarks/results/reversed-flows-20261114.md)): the mirrored drawing equals `lr`. `bt` (S1-0b, done
  the same day): a container keeps its header at its top in both senses, so every area is mirrored within its own box
  and its content moved back by the difference of its two margins (the shifts of nested areas add up); routes follow the
  area that holds each point, a horizontal segment that passes a container's side changing its height there. The
  mirrored drawing equals `tb` on the same examples.
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
  nested-tree-radial, petri-net-shapes) against the pipeline before single pass and elk-rs. Measured on all datasets
  (2026-11-14, [results](../../benchmarks/results/s1-4-defaults-20261114.md)): the default is unchanged since 0.4.0
  (fingerprints identical). `regionPorts` + `straightDetours` + `singlePassReversed` (without `singlePassMixed`) leaves 30
  of 31 datasets geometrically identical and removes all 44 lines through nodes on mixed-large (crossings 811 against
  721); with `singlePassMixed` 4 remain, with `regionChildPorts` 2 at many more crossings. Time
  ([results](../../benchmarks/results/s1-4-timing-20261114.md)): the three options cost nothing where they change
  nothing and 5 % on mixed-large; directionsFirst 2.5 times on synthetic-2k. Decided (2026-11-14): the three options are
  the default (the fingerprints of the sweep views stay identical to 0.4.0); `singlePassMixed` and `regionChildPorts`
  stay options; directionsFirst stays an option until it combines with them. The automatic choice of a routing style
  (`auto`) compares the straight segment without its detour, as before.

## Consequences

- Single pass becomes the one pipeline for every graph; the older pipeline remains for options single pass does not take.
- A region is a box with ports for its parent: the same contract later serves drill-down (an area seen as a box with ports)
  and other engines as regions.
- Mirroring keeps the drawing's ranks and orders, so a reversed view is the mirror image of the positive one, not a layout
  of its own.

## Amendment: routes planned inside organic regions (2026-11-23)

Status: proposed (#156). Each step comes behind the option `organicCorridors` (off, omitted) and is accepted only by the
quality gate of decision 6. Only C1 is built; its outcome (2026-11-26) is recorded at the end.

### Context

Decision 3 leaves the edges inside a region to the region's routing. For an organic region whose edges are orthogonal
(the default routing) that is still the pipeline before single pass: the region is placed by stress and separation with a
fixed gap of 24 between boxes, the router draws the routes afterwards, and a repair and an overlap resolution correct
them. Measured on generated views (`benchmarks/experiments/bus-mesh.mjs`, #156,
`benchmarks/results/narrow-gap-repair-20261123.md`, `organic-gap-20261123.md`):

- The gap leaves no room for a route. The router keeps its clearance (12) from a foreign box and starts at a port with a
  stub of the own clearance (12): 24, so a route that has to leave a node toward a neighbour touches the neighbour's grown
  box. Until #158 the repair gave up there, and the route went straight through the boxes between (54 of 250 routes on a
  view of 100 nodes with 11 containers; 518 of 1 159 on a bus view with 240 children).
- Repairing afterwards squeezes the routes into the gaps, where they share segments; resolving those took 117 s on 120
  children before #158 made it 3.5 s, and dense views still cost 7 to 14 times the time before the repair.
- A wider fixed gap (`organicGap`, #159) helps on average but guesses: it is not monotone (36 better than 48 on one
  view, worse on another) and costs area everywhere, up to 2.5 times the width, also between boxes no route passes.

Single pass solved the same problem for layered areas by planning before placing: the channels between ranks grow by the
routes that pass them (`channelsByDemand`), and the routes are drawn into the room reserved for them, without a repair.
This amendment carries that principle into organic regions.

### Decision

An organic region with orthogonal routing is laid out in four phases, each deterministic and without a correction pass:

1. **Arrangement.** Stress placement as today, read only as an arrangement: which boxes are neighbours, and on which
   side of each other. Its coordinates are the targets of phase 3, not the result.
2. **Corridor plan.** The free space between neighbouring boxes is a graph: one corridor per pair of neighbours (found as
   the separation finds overlaps, `vpsc::scanline_pairs`, with a search radius instead of the gap), one junction where
   corridors meet. Every edge of the region gets a port side on each end (the side facing the other end in the
   arrangement, as border nodes choose theirs, S1-3) and a path through the corridor graph: the shortest by length, then
   by turns, with a cost per route already in a corridor, so that routes spread over parallel corridors. Edges leave in
   a fixed order (by edge id), so the plan is the same on every platform and for any thread count. The **demand** of a
   corridor is the number of routes it carries.
3. **Separation by demand.** Every pair of neighbours gets as its minimum distance what passes between them: the
   router's clearance on both sides, one track per route, and the stub where a port of either box faces the corridor. A
   pair without routes between them keeps the gap of 24. `vpsc::solve` takes these distances as they are: it accepts one
   `Separation` per pair; today every pair gets `GAP`. The boxes move by the least displacement that satisfies all pairs,
   so the arrangement of phase 1 is kept.
4. **Routes in their corridors.** Each route is drawn along its planned path: one track per route in each corridor, in
   the order that avoids crossings inside the corridor (the order of the routes' other ends, as single pass orders a
   channel), stubs on the planned port sides. The room exists by construction, so the repair has nothing to do, and the
   overlap resolution only sees routes that share a corridor by plan, which it does not move.

The contract of the region to its parent (border ports, decision 4) is unchanged: the region's box grows by what phase 3
adds, and its border ports are placed after phase 4 as today.

**Containers inside the region** (the drill-down case of #156) take part as boxes: their own children are laid out
first, as today. An edge from a child inside a container to a node outside leaves the container by a port on the side
its plan uses, so the container's outer corridors carry it; the container's inner routing ends at that port. Phase 3
grows the corridors around the container, never its inside.

**Straight routing** in an organic region (decision 3, `straightDetours`) is not affected: the corridors are for
orthogonal routes.

### Prior work (read 2026-11-23)

- **libavoid** (Wybrow, Marriott, Stuckey: *Orthogonal Connector Routing*, GD 2009; *Orthogonal Hyperedge Routing*,
  Diagrams 2012; open PDFs, read in full): nodes are fixed ("each i ∈ N has a fixed position, width and height"). Routes
  are searched on an orthogonal visibility graph (O(n²) vertices, A*, length plus bend penalty). Then routes are
  **nudged**: first the order of the connectors in shared segments is chosen so that they cross least (minimal when the
  shared segments form a tree), then per axis the segment positions are variables under separation constraints (keep the
  order, keep the minimum distance, do not pass through objects), projected with VPSC; the desired position is the middle
  of the alley, or the corner a route bends around. A channel too narrow for its connectors is not addressed; the paper
  notes that a compact graph ("many of the nodes in this graph are close together") routes worse and slower.
- **libtopology** (Adaptagrams, the implementation of Dwyer, Marriott, Wybrow: *Topology Preserving Constrained Graph
  Layout*, GD 2008; the paper itself is not openly available, its source is): routes are the input, their bends pinned to
  node corners; a `TriConstraint` keeps a route point on one side of the line through two others, which fixes the side of
  a node a route passes, with the node's half size as the distance; node positions come from stress majorization under
  these and the non-overlap constraints, solved with VPSC. Nothing about room for several parallel routes.
- **ELK and elk-rs** `force` and `stress` draw every edge as one straight segment and give an edge that leaves a
  container no route at all; d3-force has only node collision; Graphviz and yFiles keep fixed margins (`sep`/`esep`; an
  organic edge router that needs nodes at least twice its distance apart). None reserves room for routes.

Neither prior method moves nodes and routes together: libtopology moves nodes around fixed routes, libavoid moves routes
between fixed nodes. That combination is what this amendment needs, and it changes phases 3 and 4 as follows.

### Phases 3 and 4 as one constraint system (refinement)

Instead of counting demand and separating nodes before the routes are drawn, phases 3 and 4 become one projection per
axis, as libavoid's nudging with the nodes free:

- **Variables:** the position of every node, and of every interior route segment across its axis (a horizontal segment's
  y, a vertical segment's x), from the corridor plan of phase 2.
- **Constraints:** node to node, no overlap (the gap of 24, as today); segment to node, the router's clearance, on the
  side the plan fixed (the `TriConstraint` idea of libtopology, here for axis-parallel segments); segment to segment in
  one corridor, one track, in the order chosen as libavoid orders shared segments (fewest crossings).
- **Desired positions:** nodes at their arrangement (phase 1), segments in the middle of their corridor or at the corner
  they bend around.

A corridor then gets exactly its width from the constraints themselves (clearance, tracks, clearance), and pairs of
nodes with no route between them stay at the gap of 24. Separating by counted demand (phase 3 as written) is the
fallback if the joint system turns out too large; C1 measures both sizes.

### What is reused, and what is new

- Reused: the stress placement and its pivot start (phase 1), `vpsc::scanline_pairs` and `vpsc::solve` (phases 2 and 3),
  the port sides of S1-3, the track spacing and clearances of the router, the border ports of S1-2.
- New: the corridor graph over an arrangement, the routing on it with capacity costs, the ordering of shared segments
  (as libavoid's), and the joint projection of nodes and segments (VPSC with segment variables). The shadow routing of the layered single pass (`abstract_routing.rs`) works on ranks and orders, not on an
  arrangement, so it is a model for the cost rules, not code to share.

### Steps

- **C1: corridor plan as a measurement only.** Build phases 1 and 2 behind the option and report per region the
  corridors, their demand and the gap each would need, without changing the layout. Measured against the routes the
  current pipeline draws on the generated views: does the planned demand predict where routes go through boxes today?
- **C2: separation by demand** (phase 3), routes still drawn by the current router. Expected: the repair finds room
  everywhere, so no route goes through a box, with less area than a fixed wider gap. Measured against the default,
  `organicGap` 36 and 48 on the generated views and the organic curated views.
- **C3: routes drawn in their corridors** (phase 4); the repair and the overlap resolution are skipped for the region.
  Measured: time, crossings, bends, area, lines through nodes.
- **C4: containers inside the region**, then the default decision on the mixed corpus as in S1-4.

### Quality gate and measurements

As decision 6: valid geometry, every edge in logical direction, no line through a node, crossings and bends against the
default and elk-rs on the same views, recorded time and area, results files with machine and revision. The default
fingerprints stay identical while the option is off. Views: `bus-mesh.mjs` with and without buses and containers, the
organic playground examples, mixed-large and the other organic views of the corpus.

### Risks and open questions

- Phase 3 moves boxes away from the stress result; how far is part of every measurement (displacement against phase 1).
- The plan can be wrong where the arrangement is: two boxes that are not neighbours in phase 1 can become neighbours
  after phase 3. One more round of phases 2 and 3 on the separated boxes is the bounded answer if C2 shows it matters.
- Capacity cost against length: a cost that spreads routes too eagerly makes long detours; the weight is measured in C1
  and C2, not set by intuition.
- Large regions (above 300 children the scalable stress takes over): the corridor graph is linear in the neighbour pairs,
  but the routing on it is not free; the time is part of every step.

### C1 outcome (2026-11-26)

Built behind `organicCorridors` (a report, `corridorPlans`; the drawing is unchanged) and measured:
[results](../../benchmarks/results/organic-corridor-plan-20261126.md). The numbers and their limits are there.

- **The question of C1** ("does the planned demand predict where routes go through boxes today?"): **no** at the
  default route cost. The ADR names no threshold; the reading was fixed in the measurement script before its first run
  (where, which routes and which boxes, each against what is known without a plan). Two of its three parts fail at the
  default; all three hold only at route cost 0, narrowly, where the demand is that of shortest ways. The straight line
  between an edge's ends, without any plan, finds the boxes hit better than the short corridors do.
- **The cause given under Context does not explain today's failures on the generated views.** Of 461 routes the router
  draws through a box, 437 have a way with the same clearance and stubs that the router does not search: it takes the
  grid of the whole view only up to 120 000 cells. Without that bound (an experiment, not merged) 461 become 24, at up
  to 37 times the CPU time on one view and less on another. Missing room can explain at most the 23 with no way. The bus
  taps through boxes (247) do not change with the bound; what draws them there was not investigated.
- **What it means for C2.** Its expectation, that after separation by demand no route goes through a box, is not
  supported: the separation moves boxes apart, and the router's grid still has its lines beside every box. A wider gap
  did change the counts (`organicGap`, [results](../../benchmarks/results/organic-gap-20261123.md)), so a separation
  may change them too, but not for the reason given here, and this measurement gives no reason to expect the planned
  demand to place that room well. C2 is not started. The nearer step is the router's search on large views, then this
  comparison again.
- **Sizes and widths for a later decision.** About a third of all neighbour pairs would be short (one route already needs
  30 against the gap of 24), and the busiest corridor of a view would need up to 1 638 units with one track per route.
  The joint system has up to 1 605 variables and 17 679 constraints per axis on the generated views, 5 to 43 times the
  constraints of separation by demand; whether `vpsc` solves either was not tried.
- **Route cost**, measured: 1 by default (6 % more planned length for half the missing width); 24 lengthens the planned
  ways by a third.
- **Departures from the text above**: the corridors are cut from the free space directly (a search radius of
  `scanline_pairs` gives pairs, not free space or junctions); the plan is made on the boxes the layout ends with, after
  the geometry repair, which moved a few root boxes by up to 1 200 units after the arrangement; the port is a side, not
  a point on it; the space at an area's outer edge costs a route nothing and is never short.
