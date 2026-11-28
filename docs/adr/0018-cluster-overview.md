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
- **Groups the data gives** (`groups`, node id → path of group names, as an import's `groups`;
  `arqlet_layout::communities::with_given_groups`): containers of kind `group` between a node and its container, nested
  along the path, labelled with their name, with the id `group:<container>:<path>` (`#` for the top level). They are
  nodes of the graph for the overview, so Louvain then divides only what is still larger than `communities` (0: a host's
  partition shown as given), and a continuation by node id treats a changed path as a changed container: a group both
  partitions have keeps its inner arrangement, a node whose path changed is placed anew, groups that are gone are
  reported as ignored ids ([integration guide](../integration-guide.md)).

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

## Addendum (2026-11-26): the overview as a server result (#209)

A host may compute the overview on a server and give the browser only the answer. Both additions are provisional and
change no answer that does not ask for them. Usage: [integration guide](../integration-guide.md).

1. **Edge data on request** (`edgeData`): per resolved edge its kind (index into `edgeKinds`, as nodes have
   `kinds`/`kind`) and its weight, the latter only when an edge counts more than once. The index of the input edge was
   the alternative and was rejected: a client without the graph cannot resolve it, the taps of buses given as data have
   no input edge, and a host with the graph already has the order (`resolvedEdgeIds`). On request, because it makes every
   answer larger and most hosts have the graph.
2. **Continuation by node id** (`start.nodes`, `id → {x, y, r, parent?}`; excluding `x`, `y`, `r`): read onto the graph as it
   is now (`arqlet_layout::overview::continue_by_id`), then laid out as any continuation. A node without a usable place
   (none, a new container, or an entry outside its container's earlier disc) is put next to the nodes it is linked to,
   and its container makes room or settles. A container that lost a node counts as changed, as does one whose children
   no longer fit together at their places (a node that moved up out of a container). The earlier disc of a container
   judges its members only when it holds at least half of them or they gather elsewhere, so that a disc the host set
   smaller than its content (`radii`) does not send them all anew. An entry's `parent` (added 2026-11-26), where given,
   says what held the node instead of the discs, for the container and the cluster alike: discs cannot tell the members
   of a cluster set smaller than its content. A parent that names nothing, and cluster parents that are not of the same
   container or go round in a circle, count as left out ([table](../integration-guide.md)).
3. **The added clusters continue; the changed graph is not clustered anew** (`clusters_by_id`). Discs of one container
   do not overlap, so the earlier cluster discs say which nodes each cluster held, whatever the graph would be clustered
   into now. Each keeps its id; a new node joins the cluster of the nodes it is linked to most; a cluster with fewer
   than two children is dissolved; containers without earlier clusters are clustered as a new graph's. An unchanged
   graph therefore continues exactly as by node index.

   Rejected: clustering the changed graph anew and matching its clusters to the earlier discs by their members (built
   first, commit `78a5bbd`). Louvain regroups far more than what changed, so many nodes were placed anew and the new
   clusters' discs overlapped; removing the overlaps without forces grew the root disc up to 31 times on a flat generated
   graph with 5 % of the nodes changed, and 1.6 times on a hierarchical one with 0.1 %. Kept clusters grow it by at most
   1.2 times on the same changes ([movements](../../benchmarks/results/overview-continuation-20261126.md)).

   The price: the clusters of a continuation drift from those of a fresh layout as changes add up (a node whose links
   moved to another community stays where it was). The host decides when to lay out without `start`.

4. **The drill-down without the input graph** (part 2, added 2026-11-27; viewer only, no engine change):
   `structuredExcerptOf(model, id)` builds a cluster's excerpt from the answer by the rule of `structuredExcerpt`, with
   kinds and weights from 1. Where both run it gives the same graph, apart from edge names (`edge:k`, the edge's index in
   the answer), node fields the answer does not carry (only `id`, `kind`, `label`, `parentId`), and kind `''` and
   weight 1 without edge data. Leaving edges stay one per overview edge by default, as in `structuredExcerpt`, so that
   the two agree; `bundle: true` merges them per inner node, stand-in, kind and direction into weighted edges.

Open, both later: edge ids on request (another request field), so that `overviewTrace` answers map onto the excerpt's
edges; a session continuing a new graph from its own places on `start: {}`.

## Addendum (2026-11-27): a work budget (#211)

A host that bounds the time of an `overview` request (a worker thread with a timeout on a server) has to terminate the
thread and load the engine again when the limit strikes. It wants a bound that keeps the engine usable. A wall-clock
limit inside the core is ruled out: the core has no clock, and the answer would depend on the machine and its load. The
overview gets a **deterministic work budget**, as refinement has (`workBudget`, counted in jobs there). Provisional; a
request without it is answered exactly as before (byte-identical, the default path is not touched).

1. **Request.** `workBudget` (an integer ≥ 0, units of work) and `overBudget`: `"answer"` (the default) or `"refuse"`.
   Every request with `workBudget` gets a report `work` in its answer: `{budget, used, clustering, limited,
   unclustered, packed}`; the last two list containers by node index, -1 for the top level (as `parent` does). Two
   optional fields report hints left out (item 8): `startIgnored` and `radiiIgnored`.
2. **Units.** One unit is roughly one pass of an inner loop over a node or a link; counted in `u64` (WASM's `usize` is
   32 bits), the same on every target and for any thread count. Louvain: per sweep the nodes plus the links visited,
   per level the links aggregated. Arranging a container of `k` children with `l` links: the start (`k + l`), per force
   tick `l + k·(b + 2)`, `b` the bit length of `k` (`⌊log₂ k⌋ + 1`), plus the pairs the collision examines, per
   separation or make-room round `k` plus the pairs examined, per round of the enclosing circle `2k`. A container kept
   as it was costs its start, `k + l`.
3. **Budgeted phases and safe points.** Only the iterative phases are budgeted: **clustering** (Louvain, per container
   that is split, checked after every sweep and level) and the **arrangement** of every container (the start, every
   tick, every separation round, the enclosing circle; settling, making room and the drag of a continuation are paths
   of the arrangement and count the same). The rest is linear in the graph (reading it, the hierarchy, reading a
   continuation by id, satellites, labels, assembling, the answer) and is not budgeted: a host bounds it with the size
   of the graph it accepts. Running out of budget is noticed at the next safe point, so the work done exceeds the
   budget by at most one step (one tick, round or sweep of one container).
4. **The first piece that does not fit ends the budgeted work.** Clustering runs container after container, the
   arrangement level after level and, within a level, in index order. A piece (a container's Louvain, a container's
   arrangement) whose work does not fit into what is left is discarded whole, and from there on nothing budgeted runs:
   the containers not yet split stay unsplit (`unclustered`), and every container not yet arranged takes the
   **fallback**. `used` counts the work the answer holds; it never exceeds `budget`. Rejected: going on with smaller
   later pieces (first fit). It uses the budget better, but the pieces discarded on the way are work too, and with
   many of them the time would no longer follow the budget.
5. **Fallback.** A container with an unchanged arrangement of a continuation keeps it, as without a budget (not
   listed), with the children the user moved (`start.moved`) put where they were put if they overlap nothing there
   (the container is not centred again, so they keep their absolute places; a container that grows by it makes its
   parent changed); a moved child that would overlap a sibling packs its container instead (listed). Any other with two or more children is **packed** (`packed`): its children in rows, in the order of the breadth-first
   walk over their links that the spiral start uses (linked children stay side by side), each in a square of its
   diameter and the gap, the row as high as its largest disc, the rows about as wide as the squares' total area is
   square; the container's disc is centred on the rows and holds them. No forces, `O(k log k)`, not budgeted. The answer
   stays valid: no overlaps (the squares do not overlap), every child inside its parent's disc, logical edge direction,
   cycles and self-loops as always. Nothing is pruned: every node has its place. Rejected: a partly simulated container
   (cut after some ticks) and a separation from there. The separation's own work is unbounded in advance, and a
   container that is either settled or packed is a report a host can act on.
6. **Threads.** The answer depends only on each piece's own deterministic work, never on the schedule: with threads, a
   level's containers are arranged in parallel, each with the level's remaining budget as its limit and a shared
   counter that stops all of them once the level's work together exceeds it; then the containers are taken in index
   order, and one that was stopped by the shared counter while all before it fit is arranged again, alone, with what
   is left. Without threads (and in WASM) every container gets exactly what is left. Both give the same answer; the
   work a thread spends on a container that is then discarded is not counted in `used`, and with threads a level may
   take up to about twice its remaining budget in time.
7. **Answer or refuse.** `"answer"` (the default): what is ready, the rest by the fallback, with `work.limited: true`
   and the lists above. `"refuse"`: an error `Overview work budget exhausted: …` (a stable prefix), naming the phase
   and the units, at the first piece that does not fit; nothing of the fallback is computed. Answering is the default
   because the answer is valid and gives the user something to see, and a host that wants all or nothing can refuse
   on `limited` itself; refusing exists because a refused request changes nothing in a session (its last places and
   hierarchy stay those of the last accepted answer) and saves the fallback's work.
8. **Sessions, columns, positions only.** A session that reuses its hierarchy charges the clustering work recorded when
   it was built (so that its answer equals that of `dispatch`), reuses it only if that clustering was complete and fits
   the budget, and never keeps a hierarchy built with unsplit containers for later requests. `work` is part of
   `OverviewPositions` and of the columnar answer's metadata. An answer with containers left unsplit has other clusters
   than the last one, so a `positionsOnly` request gets the whole overview then. Such a hierarchy may also lack what a
   request's hints name: a `start` by node index made for the complete clustering (more values than nodes; also a
   session's `start: {}`), and `radii` of added clusters (`community:…`) it does not have. With `"answer"` they are left
   out, the overview is laid out without that start (its budget is spent anyway, so its containers are packed), and
   the answer says so (`startIgnored: true`, `radiiIgnored`); with `"refuse"` the refusal comes first, before the hints
   are read. Rejected: charging the complete clustering whatever the budget (`used` could exceed `budget`) and
   refusing the start (no answer, and no stable prefix). Without a cut clustering a start that does not fit is refused
   as before. A continuation (`start`) keeps an
   unchanged packed container as it is, as it keeps any unchanged arrangement: the host lays out without `start` to
   have it arranged.
9. **How a host picks a budget.** Lay out graphs of the sizes it serves with `workBudget` larger than any work (for
   example `2^53 − 1`, the largest integer a JavaScript number holds exactly): `work.used` is the work of the whole
   overview. Time those requests on the host's machine, and take units per second from them; the budget is the time
   limit times that rate, less a margin for the linear part (the time of a request with `workBudget: 0`, which packs
   everything). Measured units per second on generated graphs: [results](../../benchmarks/results/overview-work-budget-20261127.md).

## Addendum (2026-11-27): filling given containers later (#235)

A host that draws a large overview from a server wants to show the top level first and arrange the clusters one by
one (the largest last), or when one is opened, without anything else moving. A work budget (item 5 above) leaves
containers packed in rows, and a continuation keeps them packed. Provisional; a request without the new fields is
answered exactly as before (byte-identical).

1. **Request.** A continuation names the containers to fill: `start.fill` (node ids, added clusters included) and
   `start.fillTop: true` for the top level. It needs earlier places as `moved` does: `start.x`, `y`, `r`, `start.nodes`,
   or a session's last overview (`start: {fill: […]}`). Ids that name no container (no node, or one without children)
   are left out and listed (`fill.ignored`), as `continuation.ignored` lists entries: a host continuing by id may name
   a cluster that was dissolved or a container that lost its children. A start left out because the budget cut the
   clustering (`work.startIgnored`) takes its fill along: every id it names is listed in `fill.ignored`.
2. **What a fill is.** The container's children are arranged as a layout without `start` arranges them (the spiral,
   the forces, the separation, the enclosing circle), with the radii they have now; the result is placed on the
   container's centre. Its own disc keeps its radius, so its parent sees no change and keeps its arrangement, and so do
   all others: every place outside the filled containers is answered as it was given, bit for bit. To make that hold,
   a fill request keeps the earlier radius of every container kept as it was whenever that radius still holds its
   children up to rounding (`Previous::keep_radii`). Without `fill` it is measured again as before: up to a hundredth
   off on some containers, and a container filled earlier, which has free room, shrinks to its content (its neighbours
   stay; the gap stays open unless `settle`). That is why it is not the default; see open points.
   The top level has no disc to keep: it takes the radius its new arrangement needs, and its children move with what
   they hold.
3. **Content that does not fit.** First the container's own padding gives way: the arrangement is taken when its
   children lie inside the disc, even closer to its border than the padding, never beyond it (whatever the padding;
   a tolerance on the radius let a child stick out by a few thousandths with padding 0, found in review). This matters: forces do not always arrange
   tighter than rows. On the generated 10 000-node graph (`overview-10000.json`, budget 20 000 000, 474 packed
   containers) 93 containers, mostly files of 6 to 16 equal leaves that rows fill well, needed 1 to 15 % more radius
   with their padding; with the padding giving way all fit (measured once on an Apple M3, `tools/platform-parity.mjs`
   inputs). Only content that reaches beyond the disc (a host may have set it smaller with `radii`) is unfit: nothing
   else is moved, the container is arranged as without `fill` and listed in `fill.unfit` with the radius it needs with
   its padding (`fill.needed`). A host that wants it to grow gives that radius in `radii` together with `fill`: the
   disc grows and its neighbours make room, as for any container whose radius changed (`settle` applies there as
   always). Rejected: shrinking the gap between the children (an option of the whole overview; a smaller one inside one
   container reads as another rule), growing on its own (the request asked for nothing else to move), and packing such
   a container again (it is what the host asked to replace).
4. **Not recursive.** A container inside a filled one is arranged anew only when it is named too, and then it keeps
   its own disc as well, so one request naming several nested containers gives what single requests one after another
   give (inside out). Rejected: a nested filled container taking the radius it needs. It would be tighter, but with a
   work budget the outer container may not fit and keep its old arrangement, whose child would then have changed size
   and pack it; the rule "the disc stays" holds in every case.
5. **Work budget.** A fill is the arrangement of its container (#211 item 3): counted the same, in the same order
   (level by level from the leaves up, index order within a level; the top level last). The first that does not fit
   ends the budgeted work as before; a named container left out keeps its places (the fallback keeps an unchanged
   container; one that changed too is packed and in `work.packed`) and is listed in `fill.overBudget`. An unfit
   container's work counts both attempts. In a fill request a container kept as it was costs nothing (#211 charged
   it the start, `k + l`): that work would come back unchanged in every round and eat the budget the fills need.
   **Stop rule:** each round fills at least the first named container in the order whose fill alone fits the budget,
   since nothing before it in a fill request costs anything but other fills. A round that fills nothing changes no
   place, so the same request again gives the same answer: the host stops there or raises the budget. A host
   therefore asks again with `fill.overBudget` while the last round filled something (found in review: listing the
   left-out containers in `work.packed` and charging the kept ones, a container whose fill needed more than the
   budget left after the kept ones was listed again in every round, unchanged, without end).
6. **Other fields.** `moved` naming a node inside a filled container (anywhere below it; every node with `fillTop`) is
   refused: it could not stay where it was put. `moved` elsewhere, `radii`, `settle`, places by id, `edgeData`,
   `positionsOnly` and `columns` work as before; the report `fill` (`filled`, `unfit`, `needed`, `overBudget`, `ignored`; containers
   as node indices, -1 the top level) comes with the whole answer, the positions-only answer and the columnar metadata.
   A container without an earlier place (placed anew) has no disc either and takes the radius it needs. Sessions need
   nothing new: `start: {fill: […]}` continues from the last overview, and the filled answer is the last one for the
   next request. A fill acts on the earlier places after they are read (`Previous`), so a change of how places by id
   are read (switching partitions, #239) does not touch it.

Open: whether every continuation should keep earlier radii that still hold their children (it would change existing
answers by a hundredth on some containers, and stop discs with free room from shrinking); `fill: []` asks for it now; a fill that settles the arrangement from the packed places instead of a
fresh start (it would keep the reading order of the rows, at the price of a less tidy result).

## Addendum (2026-11-28): inner-edge share and modularity per container (#236)

A host that shows its own partition (`groups`) or the overview's communities wants to mark weak containers: those whose
children are not much more connected inside than to the outside. On request (`containerStats: true`) the answer gives
per container how much of its edges it keeps and its term of the modularity, plus the modularity of each depth.
Provisional; without the flag answers are byte-identical. Usage: [integration guide](../integration-guide.md).

1. **What is counted.** The resolved edges of the answer (`edges`; unresolved edges are left out before, as everywhere),
   undirected, each with its weight (`Edge::weight`, 1 when left out), so parallel edges add up. A container's *block*
   is the container node and everything below it, in the final hierarchy (with the given groups, the added communities
   and satellites). `in(B)`: the weight of the edges with both ends in `B`; `deg(B)`: the weight of the edge ends in `B`
   (a self-loop counts once in `in`, twice in `deg`, so that the degrees add up to `2m`). `inner = in(B)`,
   `touching = deg(B) − in(B)` (inside plus leaving), `share = inner / touching`.
2. **Modularity per depth** (Newman and Girvan): the partition at depth `d` has the blocks of the nodes at depth `d`
   (0: top level), and every node above depth `d` is a block of its own, as in a cut through the dendrogram of the
   containment tree. `Q_d = Σ_B [in(B)/m − (deg(B)/2m)²]`, summed in node index order; a container's `modularity` is
   its own term at its depth. The root is the one-block partition (depth -1, term 0). The terms of the containers of a
   depth need not add up to `Q_d`, since leaves at that depth and the nodes above are blocks too; the per-depth sum is
   given so that a host need not add them itself. Rejected: a modularity per parent (the partition of each container's
   subgraph into its children). It is a different number for every container, with its own `m`, and does not compare
   across containers; the per-depth partition is the standard one for a hierarchy of clusters and the issue asks for
   the modularity of each level as a whole. "Depth" is used instead of "level" because `stats.levels` counts the
   heights the arrangement runs in.
3. **Edges of containers.** An edge between a container and a node below it lies inside the container's block (the
   container node belongs to its own block) and not inside the child holding the node; an edge from a container node
   to a node outside it touches it. At deeper depths the container node is a block of its own. This differs from
   `links`, where such an edge counts nowhere: `links` serve the arrangement of children, the stats the partition.
4. **Determinism.** Weights are integers, so `inner`, `touching`, the degrees and `m` are exact (`u64`); every quotient
   and the sums per depth are computed in a fixed order, so the numbers are the same for any thread count and natively
   and in WASM (a platform-parity request). With `m = 0` every share and modularity is 0, and a share is 0 when nothing
   touches the block (`touching` tells this apart). Cost: one walk up from both ends of every edge, `O(E · depth)`,
   plus `O(n · depth)` for the per-depth sums; not budgeted (#211), as the rest of the linear part.
5. **Columns and positions only.** In the columnar answer the arrays are typed columns `containerStats.<field>`
   (`container`, `depth` as `i32`, as `parent`; the rest `f64`) after the others; `weight` stays in the JSON, and the
   engine client nests the columns back into `containerStats`, so the answer is shaped like the JSON one. A
   `positionsOnly` answer does not carry them: they depend only on the clusters and the edges, which such an answer
   assumes the host has. A session therefore answers one only while `containerStats` is the same as in its last answer
   (as for `edgeData`), and the whole overview otherwise.

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
