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
  to the smaller community number). Every Louvain level is a level of clusters (`community:<container>:<level>.<k>`, kind
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

Open, both later: the drill-down from an `OverviewModel` without the input graph (part 2 of #209) builds on 1 and may
want edge ids, which would be another request field; a session continuing a new graph from its own places on `start: {}`.

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

## Addendum (2026-11-28): Leiden beside Louvain, chosen by an option (#237)

Louvain can leave a community badly connected, or not connected at all: a node that held two parts together moves to
another community, and nothing later splits what it left behind (the first item of *Not done* below). Leiden (V. A.
Traag, L. Waltman, N. J. van Eck, "From Louvain to Leiden: guaranteeing well-connected communities", *Scientific
Reports* 9:5233, 2019) adds a refinement phase that guarantees connected communities. It is implemented from the paper's
description (its Algorithm A.1–A.3 and section "Leiden algorithm"); no code of any implementation was read or copied.
Provisional; a request or snapshot without the new options is computed exactly as before (byte-identical).

1. **Options.** `communityAlgorithm`: `"louvain"` (the default, omitted) or `"leiden"`; `communityResolution`: a number
   \> 0 (the default 1, omitted), the resolution γ of modularity, `Q = Σ_c (e_c / m − γ (K_c / 2m)²)`: above 1 smaller
   communities, below 1 larger ones. Both algorithms take it; Louvain's gain `w − γ·tot·k / 2m` multiplies by exactly
   1.0 by default, so its results do not change. The same two names in both places that find communities: the overview's
   `options` and the layout's `LayoutOptions` (with `communities`; also the `communities` operation, which hands the view's communities to a host as groups). Two option
   sets, not one shared value, because the overview request and the snapshot's options are separate already, as
   `communities` and `hubs`/`communityHubs` are. A session reuses its overview hierarchy only for the same two values.
2. **The algorithm** (per container that is split, on the links between its children, as Louvain):
   - *Fast local moving*: a queue of all nodes in index order (the paper: random order); a node taken from it moves to
     the community with the largest gain, the empty community included (gain 0), only for a real gain (> 1e-12, as
     Louvain), ties to the smaller community number; the neighbours it left in other communities join the end of the queue.
   - *Refinement*: within every community `S`, each node starts alone. In index order, a node still alone that is well
     connected to `S` (`E(v, S − v) ≥ γ·k_v·(K_S − k_v) / 2m`) joins the refined community `C ⊆ S` of a neighbour that
     is well connected (`E(C, S − C) ≥ γ·K_C·(K_S − K_C) / 2m`) with the largest gain `E(v, C) − γ·k_v·K_C / 2m`, if
     that gain is real. A refined community is therefore connected.
   - *Aggregation*: a node per refined community; its initial community is the community of the moving phase. Repeat
     until the moving phase leaves every community as one node.
   - **Levels.** The refined partitions, one per iteration, nest (each groups the nodes of the one before), and when the
     loop ends the last one is the final partition. They are the levels of clusters, as Louvain's levels are, with the
     same ids (`community:<container>:<level>.<k>`, communities numbered by their first node). Every level's communities
     are connected in the container's links (test). The loop ends after 32 levels at most (the levels then end at
     the last refined partition, connected as every one). When a refinement merges nothing (aggregation would not shrink
     the graph), the last level is the moving phase's partition split into its connected parts, which only raises
     modularity (no link between the parts) and keeps the communities connected, though not necessarily γ-connected
     as a refined partition's are.
3. **Determinism: a greedy refinement, no seed.** The paper picks the refined community at random, with probability
   `∝ exp(ΔH / θ)` (θ = 0.01), so that the refinement explores more partitions; greedy is its limit θ → 0, which the
   paper names. The guarantees (γ-connected refined communities, hence connected communities; the fallback level of
   item 2, the moving partition split into connected parts, is connected but not necessarily γ-connected) do not
   depend on the randomness, only on merging nodes still alone into well-connected subsets with a non-negative gain. Greedy was chosen
   over a fixed seed and a deterministic PRNG because: (a) it follows the rule Louvain here follows already (index order,
   ties to the smaller community number), so the comparison measures the algorithm and nothing else; (b) with a random stream
   every choice depends on how many numbers were drawn before it, so one changed node reshuffles the rest of its
   container's refinement — the clusters of an incremental engine would change for reasons unrelated to the edit (the
   continuation by id keeps clusters for the same reason, addendum of 2026-11-26); (c) `exp` is not among the portable
   functions (`PortableMath`), and a seed would be one more option to keep with snapshots. The price is less
   exploration, which the measurement below includes. A randomised refinement can be added later behind a seed if
   measurements ask for it.
4. **Unchanged around it.** Hubs are left out before (they stay direct children, their links count for no community);
   satellites are gathered after; groups the data gives come first, and the algorithm divides only what is still
   larger than `communities`; a container of mostly unlinked children is not split; communities of fewer than 8 nodes
   and communities holding one community of the level below are no clusters; children without links are gathered.
   Labels are made the same way (overview: `≈ <most connected member>`; layout: `<best connected member> +N`). A
   continuation by id keeps the earlier clusters whatever algorithm made them, and clusters a container without
   earlier clusters with the request's algorithm.
5. **Work budget** (addendum of 2026-11-27): the same units as Louvain, counted at the same kind of safe points. Per
   level the nodes plus the links (adjacency and aggregation). The moving phase: per node taken from the queue, 1 plus
   its links, so a pass over every node costs what a Louvain sweep costs; at most 64 × nodes takes per level (Louvain:
   at most 64 sweeps). The refinement: two passes (well-connectedness, merging), each the nodes plus the links. The
   split into connected parts, when it runs: the nodes plus the links. A container's Leiden is one piece, discarded
   whole when it does not fit, as a container's Louvain.
6. **Default.** Louvain stays the default; the owner decides. Measured
   ([results](../../benchmarks/results/communities-20261128.md), generated code index of 10 000 to 100 000 nodes, its
   flat variant of 2 000 to 50 000, and the reference views): Louvain left no community in parts on any of them (0 of
   2 991; a search over 20 000 small generated graphs found one case, now a test); modularity is equal within 1 %
   (Leiden 0.5–0.9 % lower on the flat graphs, slightly higher on two reference views); Leiden's clustering work is
   7–10 % lower on the hierarchical graphs and 2.4–7× lower on the flat ones, and it makes deeper hierarchies with more
   clusters (+10 % on the code index, up to +64 % on flat graphs). Recommendation: keep Louvain the default until real
   exports and the readability of the deeper hierarchy say otherwise; Leiden for large flat graphs and for hosts that
   need the guarantee.

## Real data (2026-11-11)

On two real exports (Intentweave, 8 874 nodes; Backstage, 31 758 nodes; `benchmarks/results/overview-real-20261111.md`)
the overview lays out without overlaps and reads as the repositories' structure. Colours come from the clusters left
after opening, repeatedly, the largest one holding more than a quarter of the graph; a container of mostly unlinked
children is not split into communities.

## Not done

- Louvain does not guarantee connected communities; a community can hold parts without links between them. Leiden,
  which does, is available behind `communityAlgorithm: "leiden"` (addendum of 2026-11-28); Louvain is still the default.
- The decisions are values of a request, not of a stored overview snapshot; saving a session of the overview is open.
- Level-of-detail rendering of edges beyond the bundling of collapsed clusters.
- A grid packing of unlinked nodes in the box pipeline: a file without internal edges, structured, is one long row.
- Quality comparison against d3-force on the same graphs; incremental updates of the overview after an edit.
- The backend's process timeout (15 s) and its debug binary limit the backend mode to smaller graphs.
