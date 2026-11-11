# ADR 0018: A cluster overview of the whole graph as a separate, light layout path

Date: 2026-11-11.
Status: proposed; implemented: operation `overview`, WebGL renderer, overview page, the three representations with user decisions, rules and the zoom policy, structured clusters in place with border ports.

## Context

The box pipeline (placement, ports, routing, repair, scene) lays out a bounded view: an import selects at most
`MAX_VIEW_LIMIT` (10 000) nodes, refinement is bounded to 1 000 nodes and 2 000 edges, and an organic area takes at most
2 000 children (`scalableStress`). Real code indexes have far more nodes. The roadmap after the single-pass default
(step 4) asks for a scalable, clustered force view of the whole graph, in the manner of Cosmograph or d3-force, that the user
enters by drill-down, level by level, until a classic layered layout appears.

SVG cannot hold 100 000 elements interactively, and the box pipeline's per-node work (ports, routes, repair) is what
limits it. The overview needs neither.

## Decision

1. **A separate operation.** `overview` goes through `dispatch` like every operation, so it runs in all three deployment
   modes. It takes a whole graph and returns points: per node (input order) its parent index, centre and radius, the
   resolved edges as index pairs (logical direction kept) and, per container, the aggregated links between its children.
   No boxes, ports, routes, labels or repair. Unresolved edges are left out and counted. The result is compact (parallel
   arrays, coordinates rounded to hundredths).
2. **Clusters are the containment hierarchy** (`parentId`). Community detection (Louvain, Leiden) is left for flat graphs
   and for large flat containers, as a later step.
3. **Layout per container, from the leaves up** (`arqlet_layout::overview`). Every node is a disc; a container's children
   are arranged in its own frame by a force simulation in the manner of d3-force (springs on the aggregated links, a weak
   Barnes–Hut repulsion weighted by area, a gravity to the centre, a collision of the discs), then a separation that ends
   with no overlap, then the container's disc is the (nearly) smallest circle that holds them. An edge counts in the
   container that is the lowest common ancestor of its ends. The containers of one height are independent jobs; the
   runtime runs them with `parallel_map`, and the result is identical for any thread count.
4. **Rendering is a small own WebGL 1 renderer** in `@arqlet/viewer` (`overview-gl.js`): discs as quads with a circle test
   in the fragment shader (point sprites are limited to 64 pixels on some GPUs), edges as lines. No dependency. Picking and
   the drill-down excerpt are pure functions (`overview.js`) and tested in Node.
5. **The result is final.** The engine computes to the end and returns one deterministic result; the browser does not
   run or animate a simulation. It animates the change between two results by interpolation (fifth step below).
6. **Drill-down** (first slice): a click zooms into a cluster; a cluster of at most `DRILL_LIMIT` (300) nodes opens as a
   layered layout of its excerpt (the `import` and `scene` operations). Edges that leave the excerpt are counted, not drawn.

## Consequences

- The overview scales with the containment hierarchy: the work is a sum over containers of their children's simulation.
  A large flat container (or a flat graph) is one simulation of all its children; that is the case community detection
  must split later.
- The overview and the box pipeline are separate layouts: a structured cluster's scene is placed into its disc, and
  positions are not shared between them.
- The leaf radius is uniform; sizes by degree or by content are open.
- Timings need a results file from `benchmarks/overview/measure.py` on an idle machine before any figure is stated.

## Addendum (2026-11-11): one representation per container

Every container is shown in one of three **representations**:

| Representation | Meaning | Existing mechanism |
|---|---|---|
| collapsed | one node; its edges aggregated | `collapsed` of the box pipeline (view projection) |
| organic | children as discs, force layout | the overview (this ADR) |
| structured | children as boxes: layered, tree or radial | the box pipeline and its strategy per container |

The variants under discussion are combinations of these: structured at the top with collapsed containers below (a flat
graph needs community detection first, so that it has containers), organic at the top with deep levels collapsed (semantic
zoom), and switching a container from organic to structured in place.

Which representation a container has is decided in this order, so the question of configuration against dynamic control
need not be settled now:

1. **The user's decision** for that container. It always holds and is stored like the other user decisions (ADR 0015).
2. **Rules of the data**, from the config set: for example by node kind (`file` structured, `directory` organic).
3. **The automatic policy**, by budgets: structured only when the expanded content fits the box pipeline (the drill-down
   limit), organic for larger content, collapsed when the cluster is too small on screen to read.

The first step implements the switch from organic to structured in place (only the user's decision drives it so far;
rules and the automatic policy follow):

- The layered layout of a cluster (`import` and `scene` on its excerpt) is drawn inside its disc, in the same WebGL view:
  boxes, routes with an arrowhead at the target (logical direction), and node names once they are large enough to read.
  Its edges to the rest of the graph run from their boxes to the discs outside.
- The disc grows to hold the layout at a fixed scale (`SCENE_UNIT` on the page: a default box becomes about 36 × 11 beside
  leaf discs 8 across), and the neighbours make room. The `overview` operation takes `radii` (node id → radius) for this,
  and `start` (`x`, `y`, `r` of the previous response) to continue: a container none of whose children changed radius
  keeps its arrangement exactly; the containers on the way from the changed cluster to the top start from their previous
  places, cooler. Switching back to organic shrinks the disc to its arrangement again.

The second step adds the collapsed representation, the rules and the automatic policy, and border ports:

- **Policy** (`representationsOf` in `@arqlet/viewer`): top-down, per container, the user's decision, then a rule by node
  kind (`kinds`/`kind` in the `overview` response), then the zoom: collapsed below 14 pixels in radius on screen;
  structured when it holds at most the drill-down limit, is at least 35 % of the viewport's smaller side and holds the
  focus (the point the user zooms at), so that only the cluster zoomed into switches; a structured cluster stays so
  down to half that size (hysteresis). A rule's `structured` applies from 80 pixels. The page runs the policy when the
  view has rested, one switch at a time.
- **Collapsed** clusters are one filled disc; their edges are drawn once per pair and direction, stronger the more they
  stand for.
- **Border ports** (ADR 0017): the excerpt of a structured cluster keeps the cluster as a container and adds a stand-in
  per cluster outside (`structuredExcerpt`); a leaving edge keeps its id and direction and runs to or from its stand-in.
  The layered layout routes it out of the cluster's box on the side its flow uses; the disc shows only the box, the route
  is cut where it crosses the box's border (`clipToFrame`), and the outer leg runs from that port to the disc outside,
  with an arrowhead there for an edge that leaves.
- When a cluster grows, the view keeps its place and size on screen; when it shrinks, the view keeps the user's zoom.

The third step closes the open points of the second:

- **Engine values** (ADR 0015): `Representation`, `OverviewIntent` (the user's decisions by node id) and `OverviewConfig`
  (rules by node kind, `auto`) are model types, generated for TypeScript; a `ConfigSet` may carry `overview`. The
  `overview` operation takes `intent` and `configSet`, refuses a decision for an unknown node and answers the checked
  values (`representation`, with rules for kinds the graph does not have listed as `unknownKinds`). The page takes them
  over after a new layout, keeps the undo stack of the decisions (Ctrl+Z, Ctrl+Shift+Z) and loads a dataset's config set
  (the synthetic code index: `file` structured).
- **Border side by the neighbours:** `flowDirectionOf` picks `lr`, `rl`, `tb` or `bt` for a structured cluster from the
  edges that leave it (towards their targets) and enter it (from their sources); the excerpt carries it as its default
  direction, so the border ports face the neighbours.
- **Labels** do not overlap: the larger is placed first, a later one that would cover it is left out.
- **Communities** (`with_communities`): a container with more than `communities` children (default 120; 0 never; the
  root of a flat graph included) is split by Louvain on the links between its children, deterministic (index order, ties
  to the smaller community). Every Louvain level is a level of clusters (`community:<container>:<level>.<k>`, kind
  `community`, after the graph's nodes); communities under 8 nodes are no clusters, children without links are gathered.
  A structured excerpt keeps the overview's nesting, so communities become containers there.

The fourth step keeps the drawing still and makes the choices visible:

- **Only a decision of the user makes room.** A cluster that the zoom or a rule structures is fitted into the disc it
  has (no new overview, so zooming never makes the drawing jump); it becomes readable as the user zooms further. A
  cluster the user structures grows to its layout and the view zooms to it.
- **Making room moves as little as it can:** a continuation (`start`) runs no forces in a container whose children
  changed radius; it only removes overlaps (`make_room`, the larger disc moves less) and does not centre the container
  again. A growth that touches nobody moves nothing (test). A cluster that shrinks leaves its neighbours where they are.
- **A panel** shows the selected cluster (click selects, double-click zooms, shift-click collapses), how it is shown and
  why (your choice, a rule, the zoom), and buttons for collapse, open, structured and automatic, plus its flow. Rules by
  node kind are a table; the zoom thresholds and the layout options are fields (kept in the browser). Undo and redo
  buttons.
- The SVG playground had the same jump: opening, closing or setting a container fitted the whole drawing again. Such an
  edit now keeps that container where it is on screen (`anchor` of `edit` in `apps/playground/app.js`).

The fifth step animates the changes; decision 5 still holds (the engine returns a final, deterministic result, and no
simulation runs in the browser), the browser only interpolates:

- **Morph:** a continuation is drawn by moving every vertex from its old to its new place in the vertex shader (two sets
  of positions and a uniform `t`, eased, about half a second), with the view moving along; a cluster the user structures
  grows and is zoomed to in one move. Two states of a drawing have the same vertices (arrowheads are always three
  vertices); otherwise the new drawing is shown at once.
- **Blend:** a change of what is collapsed or structured blends the old drawing out and the new one in.
- **Settle:** since the move is visible, a cluster that gives room back may let its neighbours settle into it:
  `start.settle` runs a short, cool force simulation in the changed containers and centres them again, which closes
  the gap (test). Growing still only makes room.
- **SVG playground:** an edit moves every node and container from its old to its new place on screen (FLIP: CSS
  transforms, about 0.4 s) and fades in routes and labels; off when the user asks for reduced motion, and never in the
  way of the change (a minimal DOM, as in the regression test, just skips it).

## Real data (2026-11-11)

On two real exports (Intentweave, 8 874 nodes; Backstage, 31 758 nodes; `benchmarks/results/overview-real-20261111.md`)
the overview lays out without overlaps and reads as the repositories' structure. Colours come from the clusters left
after opening, repeatedly, the largest one holding more than a quarter of the graph; a container of mostly unlinked
children is not split into communities.

## Not done

- Louvain does not guarantee connected communities (Leiden would); a community can hold parts without links between them.
- The decisions are values of a request, not of a stored overview snapshot; saving a session of the overview is open.
- Level-of-detail rendering of edges beyond the bundling of collapsed clusters.
- A grid packing of unlinked nodes in the box pipeline: a file without internal edges, structured, is one long row.
- Quality comparison against d3-force on the same graphs; incremental updates of the overview after an edit.
- The backend's process timeout (15 s) and its debug binary limit the backend mode to smaller graphs.
