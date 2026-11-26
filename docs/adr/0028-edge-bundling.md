# ADR 0028: Edge bundling: hierarchical bundles, drawn by the scene

Date: 2026-11-26.
Status: proposed (#133, milestone 0.10). Nothing is built into the engine. The owner accepted the recommendations on
2026-11-26 (decisions below). The probe B0 ran the same day: **no-go** for all three kinds of control points
([B0 result](#b0-result-2026-11-26)).

Related: [ADR 0004](0004-organic-placement-and-straight-routing.md) (organic placement, straight edges),
[ADR 0005](0005-routing-constraints.md) (routing requirements, buses, shared segments),
[ADR 0013](0013-realized-path-geometry.md) (one drawn path), [ADR 0002](0002-quality-model.md) (proposed: node hits are
never traded), [ADR 0020](0020-single-pass-for-every-area.md) (regions, organic corridors),
[ADR 0025](0025-scene-renderer-webgl-far-svg-near.md) (renderers),
[ADR 0026](0026-appearance-one-style-for-every-renderer.md) (appearance).

## Context

The issue asks for edge bundling in organic, radial and circular views (Holten's hierarchical edge bundling, or
force-directed bundling), as d3's `curveBundle` and yFiles' edge bundling, with the routes as sampled curves in the
scene, so that every renderer draws them alike.

### What draws routes today (checked in the code at `70d744b`)

- **Parallel edges** (`bundling: complete`, `bundle_complete` in `crates/layout`): edges with the same source and target
  share the route of the lowest id; the others are not drawn and are listed as its `members`. Antiparallel edges,
  self-loops and edges with explicit ports are left out. Straight edges stay single lines: `apply_straight_edges` writes
  each of them anew after the layout (read in the code, not run).
- **Buses** (ADR 0005): explicit buses, and the fans of areas routed `bus`, share lines on orthogonal routes.
- **Straight edges** (`apply_straight_edges`): one segment between ports, parallel edges side by side on ports of their
  own, foreign nodes entered are reported. `straightDetours` (on by default) replaces a blocked segment by a pulled
  route around the blockers where it finds one.
- **Scene styles** (`crates/scene`): `polyline` and `curve` pull the orthogonal route taut; `curve` then samples a
  centripetal Catmull-Rom spline (at most 16 points per piece, one per 8 units; a piece that would enter a node stays
  straight). Arcs, 45° cuts, curves and bridges exist only in `build_scene`; `build_corner_scene` leaves them out, and
  the published drawing (key, metrics) is built from it when the graph sets a routing style, else from the snapshot's
  routes.
- **Renderers** draw `SceneRoute.points` as they are: an SVG polyline (`svg.js`), one segment instance per pair of points
  (`scene-buffers.js`), the same points in React Flow's edge data.

### Two facts that decide where bundling can do anything

1. **Today's radial areas leave next to nothing to bundle.** A `tree` or `radial` area is refused unless the links
   between its children form a forest (`check_tree_areas`): no cycle, no second parent, no parallel or antiparallel pair,
   and an edge between nodes inside two children counts as a link between them. So two children of a radial area are
   joined by at most one edge; at most, edges from one child to several others could share their start. (Edges that leave
   a radial container are not restricted by it.) There is no `circular` strategy (`LayoutStrategy`: none, shortest, tree, radial, organic, octilinear;
   `recursive-optimization.md` names circular as a target addition). The benchmark input `nested-tree-radial` (160 nodes,
   121 edges) has no edge between different containers. Holten's well-known picture (the hierarchy on rings or a circle,
   many relations across it) needs an arrangement that places the **containment** hierarchy and ignores the relations;
   `radial` does the opposite.
2. **Bundling by hierarchy needs containment.** Nested organic views have it, and so do the containers a view adds
   (`communities`, `groups`). A flat view has none, and the method below leaves its edges as they are. Organic areas also
   route orthogonally by default (ADR 0004); the straight edges that bundling replaces must be asked for.

## Methods considered

| Method | Input | An edit changes | Nodes in the way | Direction | Verdict |
| --- | --- | --- | --- | --- | --- |
| Hierarchical (HEB, Holten 2006) | containment | the routes of the changed subtree | not considered | kept, per edge | **proposed** |
| Force-directed (FDEB, Holten and van Wijk 2009) | straight edges | any route | not considered | mixed | not now |
| Divided bundling (Selassie et al. 2011), FDEB with lanes per direction | straight edges | any route | not read | separated | not now |
| Edge-path bundling (Wallinger et al. 2021) | graph and positions | any route | control points are nodes | by directed paths | candidate for flat views |
| Ordered bundles routed around obstacles (Pupyrev, Nachmanson et al.) | positions, free space | any route | routed around | kept | candidate beside ADR 0020's corridors |
| Confluent drawings (Bach et al. 2017) | graph; own layout | the layout | by construction of its layout | supported | a layout, not a route style |
| Image or density based (KDEEB, CUBu, SBEB) | edge positions | any route | not read | not read | not read; not proposed |

**HEB.** The control polygon of an edge is the path in the hierarchy from its source over the least common ancestor (LCA)
to its target; the LCA is removed when the polygon has more than three points. With `N` control points and strength
`b` in `[0, 1]`, each point is straightened, `P'i = b·Pi + (1 − b)·(P0 + i/(N−1)·(PN−1 − P0))`, and a cubic B-spline with
an open uniform knot vector (degree 2 or 1 for 3 or 2 points) is drawn through the result. The paper names bundles lying on each
other in layouts with many collinear nodes as its biggest problem, uses a colour gradient instead of arrows for
direction, and draws short curves on top with long ones fainter.

**FDEB.** Every edge is cut into subdivision points; springs along the edge and attraction between corresponding points of
compatible edges (angle, length, distance and visibility compatibility) move them over 6 cycles of 50, 33, 22, 15, 9 and
7 iterations with 1 to 32 points. The paper gives `O(N·M²·K)` (iterations, edges, points per edge) and names it as the
main limitation.

**Edge-path bundling** bundles an edge along a short path of other edges (length bounded by 2 times the straight
distance), so a bundle follows connections that exist; its authors report poor results for edges in both directions
between the same nodes, and a worst case quadratic in the edges. In Arqlet its control points are node centres of boxes,
so curves would be drawn towards and through nodes (our inference, not measured).

**What libraries do.** d3's `curveBundle` straightens the control points exactly as above (default `beta` 0.85) and draws
a basis spline; the caller supplies the hierarchy path. yFiles for HTML bundles in its circular and radial layouts and in
a tree reduction stage (no multi-parent structures), with `bundlingStrength` in `[0, 1]`, and offers a
`BundledEdgeRouter` on the result of any layout, whose guide warns that routes may overlap nodes; it can bundle incoming
and outgoing edges apart. ELK's option reference lists no bundling option: ELK Layered merges ports (`mergeEdges`,
`mergeHierarchyEdges`), and Graphviz' `concentrate` (`dot`) merges multi-edges and shares parts of paths.

## Decision (proposed)

### 1. Hierarchical bundling only, over the containment of the view

HEB fits the project's rules without exceptions:

- **Local and incremental.** A curve depends on its edge's ends, the boxes of their containers, what it passes
  (section 5), the number of edges with the same two ends and the options, never on the shape of another curve. An edit
  moves only the curves it touches; hiding an edge changes no other curve.
- **Deterministic.** A closed formula, evaluated once per route in route order with `+ - * /`, `sqrt` and `p_hypot`: no
  iteration, no seed, nothing parallel, so threads cannot matter, and native and WASM compute the same.
- **No new dependency and no copied code.** Written from the paper's formulas. d3 is a behaviour reference; if any code
  were taken from a library, repository, revision, license and notices are recorded first.
- **Logical direction.** Points run from source to target; arrowheads stay.

FDEB fails the first point (every route depends on every other, so each edit would redraw all bundles and every scene
change would list all routes) and costs far more (below). It would have to be a runtime job whose result the snapshot
keeps, not a step of the scene; that is a separate decision if flat views need bundling.

### 2. It belongs to the scene

A pure module `crates/scene/src/bundle.rs`, called by `build_scene` after the routes are collected and before border
crossings, bridges and bounds, like the curves. The snapshot's routes, ports and labels, the placement, the router and the
published key stay as they are.

- Not in **layout or routing**: bundled points in `snapshot.routes` would enter the repair, port, label and track steps,
  which would treat the shared trunks as conflicts to remove, and `Route::points` would no longer be the route an edit
  continues from.
- Not in the **renderers**: three renderers would compute geometry three times (against ADR 0008 and ADR 0013).
- `build_corner_scene` does not bundle. So the engine's metrics, key and refinement judge the **unbundled** drawing; what
  differs in the drawn one is reported by the scene (section 6). This narrows ADR 0013's "what is drawn is what is
  judged" for this option (decision 5).

### 3. One option, off by default

`appearance.edgeBundling: { "method": "hierarchical", "strength": 0.85 }`, omitted when absent, so default output stays
byte-identical. `strength` is finite, in `[0, 1]`, default 0.85; at 0 nothing is bundled. Like `bridges`, a change of
`appearance` advances the layout version without a new layout (`recompute_with`), so it works in all three deployment
modes through `configure`, and stale results are refused as today.

Not a value of `bundling`: that option replaces members by a leader's route in the layout, and code paths test for
`!= none`. The two are independent. Not a routing style: ADR 0005 keeps grouping apart from path
geometry, and `auto` or a list must never bundle. One strength for the whole view; per area only on request.

### 4. Which edges are bundled

An edge is **eligible** when all hold; every other edge is drawn as today and counted by reason:

- it is drawn straight (`snapshot.straightEdges`) with two points (not detoured);
- source and target differ and neither contains the other;
- it has no explicit port, is in no effective bus and is no bus tap or hub stub;
- no label is requested for it (labels are placed beside the unbundled route);
- at least one container lies between an end and the LCA of the two ends.

The control polygon is: source centre, the centres of the source's containers below the LCA (inner first), those of the
target's (outer first), target centre. The LCA is never a control point. This is Holten's rule except for two ends in the
same container, which stay straight: his three-point curve bends to the parent's centre, and in a nested drawing that
centre is usually occupied by another child. A flat view therefore changes nothing.

ADR 0005 says never to bend a fixed straight route silently. Here the bend is not silent: the option is an explicit
request and the scene marks every route it bent. ADR 0005's rule that edges share no segment outside a declared bundle or
bus gets one amendment: the strands of these bundles are declared by the option. `auto`, routing lists and defaults never
turn it on.

### 5. Geometry and nodes in the way

- The spline is sampled by the control polygon's length: one segment per 8 units, at least 8 and at most 48 segments
  (Holten finds about 50 needed for a smooth curve; the cap is open to measurement).
- Edges with the same two ends (either direction) share a polygon. The k-th of n, by edge id, is translated by
  `(k − (n−1)/2)·s` along the normal of the chord from the smaller end id to the larger, with `s` the track spacing,
  reduced so that all stay within the smaller end node. They remain separate lines and antiparallel edges lie on opposite
  sides.
- The curve is cut at the outlines of its two nodes; the first and last point lie on the borders. The scene's ports of a
  bundled route move to these points (the snapshot's do not), so port marks and React Flow handles meet the line.
- **Guard: bundling adds no node hit.** A curve may enter a node only where its unbundled route does: every node except
  its two ends and the containers holding them is an obstacle (the straight router's rule), the curve may not re-enter an
  end, and placed labels are obstacles too. With a required clearance, it also stays that far from every node its
  unbundled route stays that far from. A curve that fails is tried at `strength/2`, then `strength/4`, then drawn
  unbundled. This is the practice of rounded corners and curves in the scene, decided per route. The obstacles are found
  through a spatial grid.

The guard may take much of the bundling away: a container's centre is the control point, and children often lie on it. B0
measures that before anything is built into the engine. The fallback, if needed, is a better control point per container
(a free place near the centre, or a gate on the border towards the other end), compared in B0 and built as B4.

### 6. What the scene says

- `SceneRoute.bundled: true` (omitted otherwise) and `bundleVia`, the container ids of the control polygon, for hosts to
  highlight a bundle and for measurements.
- `Scene.bundling` (omitted without the option): routes bundled at full strength, weakened, kept by the guard, and not
  eligible by reason.
- For bundled routes, `conflicts` and `invalid` are derived from the drawn points (a curve may avoid a node its straight
  segment enters), border crossings are found again as for pulled routes, and no bridges are drawn on or over them.

## Interaction with other features

| Feature | Behaviour |
| --- | --- |
| Routing requirements | Only straight edges, also those `auto` chose. Orthogonal, rounded, polyline, curve and octilinear areas are unchanged. |
| Buses, bus lines, junctions | Excluded; unchanged. |
| `bundling: complete` | No effect on straight edges today, so none on what is bundled here. |
| Aggregated routes (collapsed content) | Eligible; weights and both-way arrows stay. |
| Containment from the view | `communities`, `groups` and other containers of the view graph count. Membership hulls, marks and lanes are no hierarchy and no obstacles. |
| Organic region in a layered parent | Straight edges inside the region are eligible. An edge that leaves it belongs to an outer area (ADR 0005) and is eligible only where it too is drawn straight, in two points. |
| Hit testing | Strands coincide in a trunk. `sceneIndex.pick` takes the first of equally near segments, SVG the later element; B2 makes both take the last drawn. |
| Trace, selection, hover | By edge id, unchanged; B2 draws traced and selected routes last, so a strand is not hidden in its bundle. |
| Route details | `routes.js` counts bends on scene points and `details` measures the snapshot route. B2 shows no bend count for a bundled route and states which route a length is of; its conflicts are queried on the drawn points. |
| Hidden elements | The scene is built, then filtered: a hidden node still counts for the guard (conservative). Compacting lays out again. |
| Exports | ELK-form answers write snapshot routes, and the reference SVG draws curated corners: neither shows bundles. Stated in the docs, not changed here. |
| Drag, settle, morph | Curves are recomputed with each scene; scene diffs list a curve that changed. Cost is measured in B3. |

## Readability and the quality gate

Bundling trades the ability to follow one edge for a view of the whole. Two strands in a trunk look connected to each
other's ends; Holten names this ambiguity and offers the strength against it, and Wallinger et al. find that force-directed
bundling gathers unrelated edges. Bach et al. report that confluent drawings, which are unambiguous by construction,
helped participants read connectivity better than common bundling. So it stays an option and is never chosen by the
engine.

The engine's metrics do not reward it: strands count as near-parallel runs, shared lines and crossings, and bends count
samples. They must not gate it, and sampled curves do not enter the published key. Measured offline
(`benchmarks/eval`), bundled against unbundled on the same boxes:

| Measure | Definition | Role |
| --- | --- | --- |
| New node and label entries | entered by the drawn curve, not by the unbundled route | **hard: 0** |
| Completeness | every edge drawn once, source to target, ends on outlines, finite | **hard** |
| Share bundled | of eligible edges: full, weakened, kept | gate |
| Ink ratio | covered cells of a fixed raster (2 units), bundled / unbundled | gate |
| Distortion | drawn length / unbundled length per edge: mean, median, maximum | gate |
| Ambiguity | Wallinger et al.: false neighbours among those a strand seems to reach (angle below 7.5°); the distances their text does not give are fixed by us and recorded | reported |
| Crossings | between routes that share no `bundleVia` container | reported |
| Scene time, payload | `build_scene`, columns and JSON size | recorded |

No number stands for readability: the owner looks at a fixed set of drawings at strengths 0, 0.5, 0.85 and 1.

Gate views, all in the repository, with root and containers organic and routing straight (counts are edges whose two ends
have different parents, an upper bound of the eligible ones): `org-chart` (82), `layerweave-code` (83), `mixed-large`
(87), `workflow-dag` (125), `phase-width-feedback` (201), `synthetic-2k` (4 590).

## Cost

Estimates from the formulas and data layouts; **nothing here was run or timed**.

- **HEB.** Per eligible edge, at most `2D + 2` control points (hierarchy depth `D`) and 49 samples of six interpolations
  each, times at most three strengths. For `E` edges at most about `3·48·E` segment queries against the grid (1.4 million
  at `E = 10 000`), each costing the nodes near it. Linear in the edges; testing every segment against every node instead
  (as the pulled styles gather obstacles per route) would not be.
- **Size, at the cap** (columns store `f32` pairs; a GPU segment is 13 floats):

| Bundled routes | Points, unbundled / bundled | Column points | GPU segments |
| --- | --- | --- | --- |
| 1 000 | 2 000 / 49 000 | 0.016 / 0.39 MB | 0.05 / 2.5 MB |
| 4 590 (`synthetic-2k`, all) | 9 180 / 225 000 | 0.07 / 1.8 MB | 0.24 / 11.5 MB |
| 10 000 | 20 000 / 490 000 | 0.16 / 3.9 MB | 0.52 / 25 MB |

  A JSON scene grows by about 20 MB at 10 000 routes if a point takes about 40 characters (an assumption).
- Work that compares segments of route pairs grows with the square of the samples: 2 304 segment pairs instead of one for
  two bundled routes. `routeDetails` for one selected route among 10 000 would test about 23 million.
- **FDEB**, from the paper's schedule: points times iterations sum to 692 per edge, each against every compatible edge, so
  about `692·c·M²` point forces (`c` = share of interacting pairs, which the paper puts at 0.25 to 0.5 with a threshold):
  1.7 to 3.5·10⁸ at 1 000 edges, 1.7 to 3.5·10¹⁰ at 10 000, besides `M²/2` compatibility tests. The authors report 19 s
  for 2 101 edges and 80 s for 9 780 on their desktop of 2009 (their figures, not ours).

## Steps

Each is accepted on its own; none changes a default.

| Step | Content | Gate |
| --- | --- | --- |
| **B0 probe** (ran: no-go, below) | A script in `benchmarks/eval` computes the curves of sections 4 and 5 from the engine's scene, with centre, free-place and gate control points. No engine change. | Results file with machine and revision, drawings for the owner. Go (proposed thresholds, not derived from a measurement): on three gate views with 50 eligible edges or more, at strength 0.85, 60 % or more drawn bundled with no new entry, ink ratio 0.85 or less, mean distortion 1.2 or less. Else B4 first, or stop. |
| **B1 engine** | Option, `bundle.rs`, eligibility, offsets, ports, guard with grid, scene fields, contracts. | Default fingerprints and golden scenes unchanged; tests for direction, parallel and antiparallel edges, self-loops, nesting, circles, exclusions, strength 0; `platform-parity.mjs` with a bundled case identical native and WASM; both hard measures on every gate view; B0's numbers reproduced or the difference explained. |
| **B2 viewers** | Picking and draw order, route details and conflicts, a playground control, ADR 0005 and docs amended. | `scene-gl.mjs`: SVG and both GPU backends within 1 % of the pixels; Node tests for pick and highlight; `ui-regression.mjs`; the React Flow example. |
| **B3 acceptance** | Full sweep of the table above, including scene time while dragging. | Results file; the owner accepts the option and its default strength from numbers and drawings. |
| **B4** (if B0 or B3 asks) | Better control points. | B1's gate; share bundled and ink ratio against B1 on the same views. |
| **Later, own ADR** | A circular or hierarchy-radial arrangement; bundling for flat views (edge-path or ordered bundles, as a runtime job). | Decided there. |

## B0 result (2026-11-26)

**No-go.** No gate view passes with any kind of control points; the gate asks for three. Numbers, machine, revision,
build and command: [summary](../../benchmarks/results/bundling-probe-20261126.md) and
[data](../../benchmarks/results/bundling-probe-20261126.jsonl). Script: `benchmarks/eval/bundling-probe.mjs` (curves
and guard in `bundling.mjs`, tests beside it). The drawings are not in the repository; the command writes them again.

At strength 0.85 with the guard; each cell is centre / free place / gate:

| View | Eligible of routes | Drawn bundled | Ink ratio | Mean distortion | Fails |
| --- | --- | --- | --- | --- | --- |
| `org-chart` | 74 of 82 | 86 / 86 / 91 % | 1.03 / 1.03 / 1.00 | 1.11 / 1.11 / 1.00 | ink |
| `layerweave-code` | 20 of 239 | 60 / 70 / 70 % | 1.02 / 1.02 / 1.03 | 1.04 / 1.06 / 1.04 | fewer than 50 eligible, ink |
| `mixed-large` | 59 of 95 | 86 / 88 / 81 % | 1.04 / 1.04 / 1.01 | 1.14 / 1.14 / 1.02 | ink |
| `workflow-dag` | 90 of 125 | 62 / 62 / 62 % | 1.05 / 1.05 / 1.02 | 1.14 / 1.14 / 1.08 | ink |
| `phase-width-feedback` | 68 of 358 | 85 / 87 / 88 % | 1.02 / 1.02 / 1.01 | 1.04 / 1.04 / 1.03 | ink |
| `synthetic-2k` | 1 373 of 5 824 | 41 / 42 / 43 % | 1.00 / 1.00 / 1.00 | 1.05 / 1.05 / 1.02 | bundled share, ink |

**What holds.** No new node entry in any guarded drawing: the probe's test against every node, and the engine's
`routeConflicts` on the 2 467 bundled curves at strength 0.85. Every eligible edge is drawn once, from border to border,
in its direction. The trial runs before the recorded one gave the same counts, distortion and ink on all 108 rows.

**Why so little is bundled.**

1. **Shallow nesting.** A section of a cubic spline is the same for two edges only where they share four consecutive
   control points, so only where they share four containers on their way. That holds for 2 of 74 eligible edges in
   `org-chart`, 4 of 59 in `mixed-large`, none of 90 in `workflow-dag`, 8 of 20 in `layerweave-code`, 24 of 68 in
   `phase-width-feedback` and 591 of 1 373 in `synthetic-2k`. In `org-chart`, 65 of 74 edges have three control points:
   each is a curve of its own.
2. **The guard removes the bundles where the structure has them.** Without it, at strength 1, centre control points
   reach an ink ratio of 0.64 on `phase-width-feedback` and 0.52 on `synthetic-2k`, with visible trunks; the guard would
   refuse 52 of 68 and 1 182 of 1 373 of those curves. With it the ratios are 1.02 and 1.00. In `synthetic-2k` the guard
   keeps 776 to 805 edges unbundled at strength 0.85. Most refusals are a node that the curve enters and the straight
   segment does not.
3. **Many edges are not eligible.** A blocked straight edge is drawn as a detour (`straightDetours`, on by default) and
   left out: 3 231 routes in `synthetic-2k`, 163 in `phase-width-feedback`, 101 in `layerweave-code`. Edges within one
   container are another 918, 119 and 114.
4. **Better control points do not change the verdict.** The gate points give the least distortion on every view and the
   largest bundled share on `org-chart`, `phase-width-feedback` and `synthetic-2k`, but no ink ratio below 1.00. The free
   place differs from the centre on 11 to 50 % of the containers on the paths and gives nearly the same numbers.

**What the drawings show** (every view at 0.85, not every variant of each; the large view also in a crop):

- `org-chart`, `mixed-large`, `workflow-dag`: no trunks. Single curves, some with hooks (a leaf behind its container's
  centre leaves away from its target and turns back) or S-shaped kinks where control points lie close together. This
  reads worse than the straight lines. A weakened or kept edge beside bent ones makes the picture uneven.
- `phase-width-feedback`, gate points: the one useful case. The edges from the left containers towards the central node
  close up into bands and free some space; without the guard the same drawing has clear trunks.
- `layerweave-code`: 20 curves among 239 routes; nothing to see.
- `synthetic-2k`: at a glance and in the crop, guarded and unbundled drawings cannot be told apart. Without the guard the
  drawing has strong, readable trunks between the three large groups (with node hits).

**The thresholds were badly chosen in one respect, and stay as they were.** The ink ratio on a 2-unit raster counts only
strands that lie on each other. At strength 0.85 each strand keeps part of its own chord, so strands of one bundle run
side by side, not on each other. A synthetic control (128 leaves of a deep hierarchy on a circle, 200 relations, no
engine, no guard), which shows clear bundles to the eye, reaches 1.03 at 2 units, 0.87 at 8 and 0.81 at 16, and 0.49
only at strength 1; its mean distortion at 0.85 is 1.38, above the limit of 1.2 as well. So the picture the method was
made for fails this gate.

The verdict does not rest on the raster alone: with the guard at strength 0.85, the ratio does not fall below 0.86 on
any view with cells up to 32 units, and not below 0.92 except for the gate points of `phase-width-feedback`;
`synthetic-2k` also fails the bundled share, and the drawings agree. But without an ink criterion at all, four views
(`org-chart`, `mixed-large`, `workflow-dag`, `phase-width-feedback`) would pass the other criteria and the probe would
read go. Share and distortion say that curves were drawn, not that anything was bundled.

**Definitions the probe had to fix** (written into the script before its first run, except the last item):

- Views: authored directions, strategies and routing removed; root and containers organic, routing straight; every node
  in the view; all other options default (`straightDetours`, a required clearance of 6).
- Free place: the point of a 4-unit grid in the container's content area (36 below the top, 12 from the other sides)
  nearest to its centre with 12 units, else 6, else any room from every child; else the centre.
- Gate: on the container's border towards the centre of the outermost box on the other side.
- Segments by the length of the straightened polygon. Strands of the same two ends: spacing
  `min(6, 2·(h − 1)/(n − 1))`, `h` half the smallest side of the two nodes.
- Ink over the eligible edges; mean distortion over all eligible edges, kept ones counting 1.
- Added after the first trial, beside the gate and deciding nothing: raster cells of 16 and 32 units, the counts of
  shared sections and covered centres, and the control.

Not measured: ambiguity, crossings, scene time and payload bytes (B3).

## Decisions (owner, 2026-11-26)

The recommendations of the proposal are accepted; the ADR stays proposed.

1. **Radial and circular.** Nested organic views are enough for 0.10. No new circular or hierarchy-radial arrangement.
2. **Flat views.** Bundling only along containment, `communities` included.
3. **Guard.** Strict: bundling adds no node hit.
4. **Routing requirements.** The option may bend edges whose area fixes `straight`. Orthogonal areas are not bundled.
5. **Metrics.** Key, quality and refinement describe the unbundled drawing.
6. **Same container.** Edges between two children of one container stay straight.
7. **Excluded edges.** Labelled, port-bound and bus edges stay unbundled in the first version.
8. **Appearance.** Arrowheads only.
9. **B0 thresholds** as proposed in the steps below.

## Recommendation

Before the probe: build hierarchical bundling as an optional step of the scene, and probe it first. After the probe:
**do not build B1.** The strict guard (decision 3) and containment as the only hierarchy (decisions 1 and 2) leave the
method too little to bundle on the gate views, and better control points (B4) were part of the probe. Take #133 out of
0.10 and keep this ADR proposed, with the probe as its record. Should bundling still be wanted, each of these needs its
own design and decision, and none was probed: bundles routed through the free space around nodes (ordered bundles, on
ADR 0020's corridors), which would keep the strict guard's promise by construction; an arrangement of the hierarchy on
a circle, the case the method was made for (decision 1 left it out of 0.10); or a bundled drawing that is allowed to
pass through nodes (against decision 3), probed again with a measure of closeness instead of a 2-unit raster.

## Sources

Read: Holten, *Hierarchical Edge Bundles*, IEEE TVCG 12(5), 2006, and Holten and van Wijk, *Force-Directed Edge Bundling
for Graph Visualization*, Computer Graphics Forum 28(3), 2009 (text extracted from public copies at
[JHU](https://www.cs.jhu.edu/~misha/ReadingSeminar/Papers/Holten06.pdf) and
[WUSTL](https://classes.engineering.wustl.edu/cse557/readings/holten-edgebundling.pdf); figures not seen). Through a
fetch tool's extracts: Wallinger et al., *Edge-Path Bundling* ([arXiv 2108.05467](https://arxiv.org/abs/2108.05467));
[d3 curveBundle](https://d3js.org/d3-shape/curve#curveBundle) and its source; yFiles for HTML
[guide](https://docs.yworks.com/yfiles-html/dguide/layout-edge_bundling/) and
[`EdgeBundling`](https://docs.yworks.com/yfiles-html/api/EdgeBundling/); the
[ELK options reference](https://eclipse.dev/elk/reference/options.html) and
[`mergeEdges`](https://eclipse.dev/elk/reference/options/org-eclipse-elk-layered-mergeEdges.html); Graphviz
[`concentrate`](https://graphviz.org/docs/attrs/concentrate/). Abstracts or search summaries only: Pupyrev, Nachmanson et
al., *Edge Routing with Ordered Bundles* ([arXiv 1209.4227](https://arxiv.org/abs/1209.4227)); Bach et al., *Towards
Unambiguous Edge Bundling*, IEEE TVCG 23(1), 2017; Selassie et al., *Divided Edge Bundling*, IEEE TVCG 17(12), 2011.

Not read: McGee and Dingliana's user study of bundling (AVI 2012); the survey by Lhuillier, Hurter and Telea (2017) and
the skeleton-based method (SBEB), both too large for the fetch tool; the papers on KDEEB and CUBu.
