# Layout configuration: option catalog and strategy analysis

Status: historical option catalogue and measurements. Current regional configuration semantics are defined by [ADR 0003](adr/0003-container-direction-constraints.md) and [ADR 0005](adr/0005-routing-constraints.md). The old hybrid/shortest rows below describe the measured historical implementation, not current authoring choices. Requirements: [specification.md](specification.md) section 10.

## Option catalog

The historical execution options below live in `LayoutOptions`. Authored regional constraints are separate: an explicit value equal to the engine default must retain its presence. In particular, explicitly authored root routing `orthogonal` cannot be treated as absent. Legacy `edgeRouting` is only a compatibility fallback; see ADR 0005.

| option | values | default | effect |
|---|---|---|---|
| `candidates` | ordering modes 1–4 | `[1,2,3,4]` | which ordering strategies refinement evaluates |
| `routingCandidate` | bool | `true` | whether the routing-only candidate is evaluated |
| `abstractRouting` | bool | `true` (omitted; since engine 0.2, ADR 0016) | complete abstract routing after placement, in import and every ordering candidate; placement derives channel tracks from actual cross-axis lane positions and span occupancy, widens rank gaps for clearance, then retains the geometric width check and growth safety net. Replaces the old constructor/probe loop, not the refinement strategy list. See restrictions below. |
| `singlePass` | bool | `true` (omitted; since engine 0.2, ADR 0016) | requires `abstractRouting`: construct once without later correction (no fallback search or track repair, no refinement candidates or rerouting rounds); the reversed links are known before the first placement, which only orders; the hierarchical order (five sweep runs, each ending at its first sweep without improvement) comes next, and one placement computes geometry, keeping the border ranks and placing across the flow by Brandes–Köpf blocks. Channels are widened for the joint bend compaction, and self-loops on adjacent sides go around the corner, so no route is rerouted afterwards on the measured views. Accepted by quality gates against default and elk-rs, not by fingerprints. See [results](../benchmarks/results/single-pass-blocks-20261106.md), [timing](../benchmarks/results/single-pass-timing-20261106.md) [phase work](../benchmarks/results/single-pass-phase-20261107.md), [no reroute](../benchmarks/results/single-pass-construct-20261107.md) and [levers](../benchmarks/results/single-pass-levers-20261106.md). |
| `singlePassAuto` | bool | `true` (omitted; since engine 0.2, ADR 0016; `false` keeps the pipeline before single pass) | an import or a `configure` with it on turns on `abstractRouting` and `singlePass` (which take orthogonal and `roundedOrthogonal` routing and, since 2026-11-08, containers of mixed `lr`/`tb` axes); a layout that fails with them is computed with the other pipeline and reported as `Single pass was not used: <reason>`. The two options then say which pipeline made the drawing. An edit does not return to single pass. See [results](../benchmarks/results/single-pass-auto-20261107.md) and [mixed axes](../benchmarks/results/single-pass-auto-mixed-20261108.md). |
| `singlePassMixed` | bool | `false` (omitted) | provisional: graphs with areas of no fixed flow (organic, tree, radial) or straight routing stay in single pass instead of falling back; those areas keep their own placement, their edges go to the completing router, and channel growth moves their overlapping children apart. Better on `can-bus`, equal on two views, worse on `mixed-large`; off. See [results](../benchmarks/results/single-pass-mixed-20261109.md). |
| `orderSeed` | integer | `0` (omitted) | provisional, for measurement: the seed of the shuffled starts of the hierarchical order (its runs from the fourth on). 0 keeps the order used before the option existed. `benchmarks/eval/single-pass.mjs --seeds N` averages each metric over seeds 0..N-1, as the gate of ADR 0016 asks. |
| `orderHints` | map node → `{after}` | empty (omitted) | provisional: the node stands after the named sibling in its rank, or first with `after: null`; set by a released drag on a single-pass drawing. See [results](../benchmarks/results/single-pass-drag-20261107.md). |
| `holdMemberships` | boolean | `false` (omitted) | provisional (ADR 0022 M3b): keep soft membership groups inside one container together within a budget; needs `groupingPriority`. See [results](../benchmarks/results/memberships-20261125-hold.md) |
| `pinsInSinglePass` | bool | `false` (omitted) | provisional (ADR 0016 amendment, steps P1 and P2): a pinned node, leaf or container, of a layered area that flows `lr` or `tb` no longer takes the view off single pass or off the hierarchical pipeline it falls back to. The pin chooses the node's rank (the rank whose band holds it along the flow, or the nearest) and its order there, and holds its place across the flow; along the flow the node stands in its rank's band, and `layoutDiagnostics` says how far from the pin (`Pin x: held across the flow, N along it`). A pinned container is one element of its parent's area; its inside is laid out as without the pin. Pins in areas of other strategies or of reversed flows, global layering and a view budget: as without the option, for all pins of the view. A pin set before there is a drawing costs a second layout (the one that gives the bands). Measured: [leaves](../benchmarks/results/pins-single-pass-20261125.md), [containers](../benchmarks/results/pins-containers-20261125.md). |
| `nodeSpacing` | number ≥ 0 | 24 (omitted) | provisional: the base gap between neighbours of a rank, across the flow; label room and the clearance minimum are added as before. The ELK adapter maps `elk.spacing.nodeNode` to it. 16 measured: 10 % less area, fewer crossings, a few more bends on the layered views in one direction, but lines through nodes on views with mixed directions or organic areas (see *Spacing and area* below), so the default stays 24. |
| `layerSpacing` | number ≥ 0 | 64 (omitted) | provisional: the base gap between ranks, along the flow; label room, the clearance minimum and channel widening are added as before. The ELK adapter maps `elk.spacing.nodeNodeBetweenLayers` to it. With `channelsByDemand` (the default) the channels take what their tracks need, and this gap mostly sets the positions the lanes are planned on: smaller values save little area and cost bends (see *Spacing and area* below). |
| `isolatedGrid` | bool | `false` (omitted) | provisional: the children of a layered area without an edge to another child (a file of unrelated functions) stand in a grid of their own ranks after the connected children, instead of all in the first rank (one long column). The grid takes about √(k·pitch) ranks for k children (`pitch`: the gap of two neighbours over that of two ranks), so it comes out about square; the connected children are placed exactly as without them. The cluster overview's structured clusters use it. See [results](../benchmarks/results/isolated-grid-20261111.md). |
| `isolatedGridFrom` | integer | unset (omitted) | provisional: `isolatedGrid` only in the areas with at least this many children that no edge joins to a sibling (a container opened with hundreds of unlinked members stands as one long column otherwise); the other areas are laid out as by default. At 50, every dataset of the benchmark set keeps its metrics; at 20, `layerweave-code` changes (fewer crossings and bends, less area). |
| `singlePassReversed` | bool | `true` (omitted; written `false` when off) | on by default since S1-4 (ADR 0020, S1-0 and S1-0b): a graph whose areas all flow right to left (`rl`) or bottom to top (`bt`) is laid out with the positive flow, where single pass runs, and mirrored area by area (a container keeps its header at its top); logical edge direction is kept, the drawing equals the mirror of `lr` / `tb`. Not for graphs with pins, label places, explicit ports or buses (those keep the earlier pipeline). Measured: [reversed-flows-20261114.md](../benchmarks/results/reversed-flows-20261114.md). |
| `straightDetours` | bool | `true` (omitted; written `false` when off) | on by default since S1-4 (ADR 0020, S1-1b): a straight edge (organic, radial or straight routing) that would pass through a node it does not end at is drawn around it, on its orthogonal route pulled taut around the nodes it does not lie in; edges with a free line stay one segment, the placement does not change, and the `straight-through-node` findings follow the drawn routes. Measured: [straight-detours-20261114.md](../benchmarks/results/straight-detours-20261114.md). |
| `regionPorts` | bool | `true` (omitted; written `false` when off) | on by default since S1-4 (ADR 0020, S1-2): an edge that leaves a region of another strategy (organic, tree, radial) in a layered parent is routed by the parent to a port on the region's border on the side its flow uses, and drawn inside the region from that port to its node (straight, or axis-parallel in a tree, around the region's nodes where needed). Two layouts (automatic ports, ports at the nodes' places in their order) and the better by the published key, so about twice the time; the parent's order sees a region at its ports and keeps that order. Not for graphs with buses or explicit edge ports. Measured: [region-ports-20261114.md](../benchmarks/results/region-ports-20261114.md). |
| `regionChildPorts` | bool | `false` (omitted) | experimental (ADR 0020, S1-3): a container with a fixed flow inside an organic or radial area gets border nodes in its own flow (on the side that faces the other end in the area's abstract placement); its edges to the outside run inside it along its ranks and leave it there, and the area draws them on straight or around the container's corner. Keeps the border contract; more bends (about sevenfold on the measured graphs). Measured: [region-child-ports-20261114.md](../benchmarks/results/region-child-ports-20261114.md). |
| `channelsByDemand` | bool | `true` (omitted; written `false` when off) | on by default (single pass): every channel between two ranks is as wide as its tracks need (at least the margin on both sides and a track) instead of at least the layer gap; the order and the places across the flow are chosen on the layer gap as before; only in areas with a fixed flow along their parent's axis below fixed flows. On the 15 layered views the same crossings and bends with 3 to 17 % less area; on every dataset no change in lines through nodes, crossings or bends. Measured: [channels-by-demand-20261114.md](../benchmarks/results/channels-by-demand-20261114.md). |
| `nestedEdgesByDetour` | bool | `true` (omitted; written `false` when off) | on by default: with `straightDetours`, a repair of an organic area that cannot clear every straight line within a bounded budget clears the lines between children of organic areas first and those into its containers within a bounded budget of their own; their detours draw what remains around the nodes in its way. A repair that finishes within the bounded budget is as without the option. 31 of 32 datasets identical; mixed-large: valid geometry, import 7× and refinement 6× faster, 42 % of the area, more crossings and bends. Measured: [nested-edges-by-detour-20261114.md](../benchmarks/results/nested-edges-by-detour-20261114.md). |
| `directionsFirst` | bool | `false` (omitted) | experimental: free directions (`auto`) are laid out with their orientations fixed under single pass — every free area left to right, and every free area top to bottom — and the drawing with the best published key wins, so single pass builds the edges instead of delegating them and the result is never worse than left to right by that key; the snapshot keeps the free directions and `areaOrientations`. About twice the layout time of a fixed direction. Measured: [free-directions-20261113.md](../benchmarks/results/free-directions-20261113.md). |
| `packComponents` | `{aspect?}` | unset (omitted) | experimental: the weakly connected components of a layered container's children (a lone child is one) keep their own ranks and are packed in rank space towards `aspect` (width over height on the drawing, default 1): components stand across the flow in columns of rank ranges, the deepest first, so the arrangement, ordering and routing stay one layered drawing. Replaces `isolatedGrid` where both are set. Fewer than two components: unchanged. Measured on synthetic shapes: [pack-components-20261113.md](../benchmarks/results/pack-components-20261113.md). |
| `communities` | integer ≥ 0 | `0` (omitted) | provisional: containers with more children than this (the root included) are divided into communities of their children (the cluster overview's Louvain), shown as containers of kind `community` named after their best connected member, collapsed at first and opened with `toggle`. View only: `sourceGraph` is unchanged. See [results](../benchmarks/results/structured-communities-20261111.md). |
| `communityHubs` | `{minLinks?, nodes?}` | empty (omitted) | provisional: children that `communities` leaves out of Louvain, so that one hub does not pull its container into one community named after it: those with at least `minLinks` links in their container (by edge weight) and those named in `nodes`. A hub stays a direct child of its container, shown as itself. The overview takes the same value as option `hubs`. Ids the graph lacks are ignored. |
| `groups` | map node → path of names | empty (omitted) | provisional: groups the data or its domain gives (`["Billing", "Invoices"]` inside the node's container), shown as containers of kind `group`, nested along the path, collapsed at first; Louvain (`communities`) divides only what is still large. View only; ids the graph lacks are ignored. A host can compute them once with the `communities` operation (`arqlet::community_groups`, npm `communityGroups`): it answers the view's groups and communities in this form, so a view given them needs no Louvain. |
| `levelOfDetail` | `{depth?, kinds?, nodes?, edges?}` | unset (omitted) | provisional (named `initialCollapse` before engine 0.4, which is now refused): the containers an import shows collapsed: at `depth` or deeper (0: all; the top level is 0) and of the `kinds` listed. Unset: none, except added groups and communities. With it, edges between collapsed containers are drawn by one weighted route per pair, and an import's `limit` counts what the first view shows (a collapsed container once, with all it holds; a container an edge reaches comes whole and collapsed). Budget (`nodes`, `edges`, provisional, layered view only): every container starts collapsed and is opened level by level, the outermost first, while the first view stays within `nodes` (leaves and collapsed containers; open frames do not count) and `edges` (as drawn: one per pair of collapsed ends, none inside one); those `depth` and `kinds` name stay collapsed. When the top level alone exceeds a budget, nothing opens. With `nodes` and no `communities`, `communities` is set to `nodes`, so a container with more children than the budget (a flat graph) is divided. See [results](../benchmarks/results/first-view-budget-20261118.md). |
| `liftEdges` | bool | `false` (omitted) | provisional: an edge whose ends are not siblings is drawn between the outermost containers its ends differ in, and the edges of one pair of containers as one route with their weights in both directions (as between collapsed containers); edges between siblings and between a container and its own content stay. A first budget counts the edges lifted. On synthetic-20k: no line–node hits instead of 10 000 and more, crossings from millions to about 34 000, import 1 to 2 s instead of 7 to 13 s. See [results](../benchmarks/results/lift-edges-20261118.md). |
| `hidden` | `{nodeKinds?, nodes?, edgeKinds?, compact?}` | empty (omitted) | provisional view state, set by `filter`: what the view hides (a node with everything below it); the layout keeps its room, so the rest keeps its place, and a route for collapsed content counts only the edges still shown; `compact: true` lays the view out without them instead (the rest closes up). |
| `partial` | map container → shown children | empty (omitted) | provisional view state, set by `reveal`: the container shows the children listed; the others wait in a collapsed container `rest:<id>` of kind `rest` (`+N more`), which a `toggle` dissolves. |
| `portSides` | bool | `false` (omitted) | provisional, single pass: an edge end whose explicit port lies across the flow or on the side against it is constructed through a lead around its node instead of the fallback router; the placement aligns the other end with the lead's lane and widens gaps for the leads, and the channel planning keeps its bends a track from them. The ELK adapter turns it on for graphs with explicit ports. See [results](../benchmarks/results/elk-port-sides-20261108.md). |
| `routeSearchWindow` | bool | `false` (omitted) | provisional: a constructed layered route searches the lanes across the flow only near its ends (as far across as the edge reaches along the flow, at least 16 tracks), and over the whole width only when it finds no way there; a search over the whole width is quadratic in the boxes across the drawing. Same metrics on the benchmark datasets, half the import time on large views of synthetic-20k. See [results](../benchmarks/results/route-search-window-20261118.md). |
| `objectives` | priority list | node overlaps, lines through nodes, overlapping lines, border hugging (margin 12), crossings, bends, `cost` (length) last | lexicographic order of quality criteria |
| `bundling` | `none`, `complete` | `none` | edges with the same source and target share one drawn route |
| `hybridBy` | `compact`, `endToEnd` | `compact` | criterion of the Hybrid direction |
| `nodeSizes` | measured leaf sizes | empty | sizes reported by the visualization (default 180 × 54) |
| `vpscOverlapRemoval` | bool | `false` | experimental; replace organic box separation with minimum-displacement constraints and add a constraint pre-pass to geometry repair |
| `nodeEdgeRepulsion` | bool | `false` | experimental; after organic stress, move free nodes off projected edge chords found by the spatial broad phase, with a bounded step. Repair stays authoritative. |
| `organicGap` | number | 24 (omitted) | provisional (#156): the gap between two boxes of an organic area. The router needs its clearance on both sides of a route and a stub at a port, so a wider gap lets a route leave a node that faces a neighbour instead of crossing it. On generated bus views with containers (`bus-mesh.mjs`) 36 or 48 left far fewer routes through boxes and often took much less time, at more area (up to 2.5 times the width); the effect varies with the view, so the default stays 24. Measured: [organic-gap-20261123.md](../benchmarks/results/organic-gap-20261123.md). |
| `sparseRepairGrid` | bool | `false` (omitted) | provisional (#203), default router: when the grid of the whole view is over the repair search's bound of 120 000 cells, first search a sparse grid of the whole view (a line only in the gaps between box edges that offer a way no neighbouring gap offers; 24 units per bend; at most 480 000 cells), then the windows as before. It holds a way whenever the boxes leave one (channels of no width between touching boxes apart). On generated views with containers and bus meshes: far fewer routes through boxes, fewer crossings, bends and shared lines in sum, longer routes; per view shared lines and crossings can rise, and a view that now gets the layout's second routing pass imports several times slower (up to 13.7). Views within the bound are drawn as without it; bus taps are not routed by it. Measured: [router-sparse-grid-20261126.md](../benchmarks/results/router-sparse-grid-20261126.md). |
| `sparseStress` | bool | `false` | experimental; organic stress uses graph neighbours and 50 pivots instead of the full distance matrix, and an area may hold up to 500 children (default bound 300). The repair stage still keeps an n² pair table. |
| `scalableStress` | bool | `false` | experimental; organic placement for large areas: pivot start, sparse stress with pivot terms weighted by their regions, terms for children two steps apart, then a force refinement (springs, Barnes–Hut repulsion, box collision, and in its second half a push of nodes off the edges that cross them). An area may hold up to 2000 children. The repair runs with the local scan budget, the conflicting routes are not routed again as a whole, and when every edge is straight-only no orthogonal routes are computed; remaining conflicts are reported. Takes precedence over `sparseStress`. Measured against d3-force and CoSE in `benchmarks/results/organic-scalable-20261109.md` and `organic-scalable-speed-20261109.md`. |
| `sizeAwareStress` | bool | `false` | experimental; organic stress targets add both children's half-extents to the pair length. Measured on the organic ladder (25–200): larger areas, mixed routed crossings. Not recommended. |
| `octilinearRefine` | bool | `false` | provisional; only for the strategy `octilinear`: after the star placement a local search on its lattice moves stations and branches so that links outside the spanning tree are octilinear too, without line–node hits or crossings, lines bending less (bends before length). On the made-up city of the example *Metro map*: 1 skewed link, 3 crossings, 1 station on a line, 8 bends (45° units) and 22 lattice steps before, 0, 0, 0, 3 and 24 after (`tree::tests::the_octilinear_refinement_schematises_a_metro_map`); a tree already straight (*Vehicle buses as metro lines*) stays as it is. Groups above 400 nodes are left as placed; time not measured beyond the examples. |
| `directedRepair` | bool | `false` | experimental; organic repair tries overlap translations and perpendicular moves away from through-conflicts first, and runs the exhaustive search only when they do not improve the drawing |
| `incrementalClearanceWidening` | bool | `false` | experimental; re-layout only widened areas and re-route affected edges during clearance widening. This can change the candidate geometry; common validation and published-key selection still apply. |
| direction | `lr`, `tb`, `hybrid`, `shortest` | per graph | `shortest` picks, per area, the direction with the smallest endpoint-distance sum |

Presets are defined in `presets/` (Preview, Grid, Readable, Quality); see `docs/config-sets.md`.
`LayoutOptions::default()` is the engine default and is not itself a preset. A preset applies its
`configSet` and names the snapshot with `configId`; any other option change clears the name.

### Provisional abstract-routing execution

Pass `{"abstractRouting":true}` as the `options` of `import` or `configure`.
The option travels in the snapshot through the existing native/WASM dispatch.
An unsupported request is an explicit error, not a silent legacy fallback; `singlePassAuto` is the opt-in for a reported fallback.
Current support: unflattened hierarchical placement with one positive flow axis
(`lr` or `tb`), default orthogonal routing and no bundling. Pins, fixed container
sizes, placement grids/search, extent bounds, view budgets, reversed/mixed axes,
explicit routing overrides, and tree/radial/organic areas are not supported.
Node-size feedback, explicit ports, labels, cycles, self-loops and parallel
edges remain supported.

The path resolves ports on placed geometry, plans demand, grows flow coordinates,
constructs chains with within-drawing coordination reuse, routes delegated edges
with the existing fallback, repairs strict track spacing, and finalizes a complete
snapshot. Grown plans and local boxes are handed off together. It does not first
compute and discard legacy routes. Layout diagnostics expose constructed/delegated
counts and unresolved strict spacing; refinement rejects abstract ordering candidates with
strict track conflicts as well as applying its usual geometry/clearance/key gates.
Initial imports report unresolved conflicts; they do not prove feasibility.
During completion, fixed-track repair reuses each route's segment projection
while checking other routes, refreshing it only after an accepted move. A
conflict-free repair returns unchanged routes after honoring cancellation. This
preserves outputs and does not change candidate or quality rules; the
[phase-2 comparison](../benchmarks/results/abstract-phase2-20261106.md) records
the single-sample timing limits and the still-open crossing-quality gap.

During coordinate placement, each local rank-channel's required tracks are
derived from the actual cross-axis port/lane positions and occupied spans;
non-overlapping spans can share a track. The placement widens the rank gap
from this geometric demand before route completion. This replaces the tested
ordinal slot-count estimate, which over-widened layouts without reducing
placement passes or improving crossings. The geometric count is specific to
the current placement, not a prediction independent of coordinates; final
physical checks and growth remain authoritative. The old ordinal trials are
documented as historical comparisons in the
[capacity-seeding results](../benchmarks/results/abstract-capacity-seeding-20261105.md)
and [exact-slot trial](../benchmarks/results/abstract-capacity-exact-slots-20261105.md).
The current one-sample integrated comparison is recorded in the
[geometry-aware capacity report](../benchmarks/results/abstract-capacity-geometry-aware-20261106.md);
it does not isolate a capacity-only speedup.
For abstract-enabled layered layouts, the backward-edge border correction
and hierarchical-order rebuild are combined into one placement rebuild. This
reduces root placement passes for the four frozen codegraph ordering
candidates from three to two, but changes abstract geometry and has mixed
crossing/bend results; the default path is unchanged, and no timing improvement
is claimed. See the [placement-reuse prototype](../benchmarks/results/abstract-phase1-20261106.md).
Abstract-routing snapshots report per-channel physical track demand, computed
width, post-placement/pre-completion availability, actual later gap shift, and
remaining shortfall/surplus diagnostics where physical realization has a
channel. Channels without a matching measurement are explicitly marked
`seed=unavailable`; the frozen-view audit found complete coverage after adding
records to reused region realizations. See the
[coverage audit](../benchmarks/results/abstract-capacity-diagnostics-audit-20261105.md).

All four configured ordering strategies, the routing-only strategy and global
rounds remain in the portfolio. Routing-only improvements and Optimize crossing
relief still use their existing algorithms and acceptance rules; not every routing call becomes an
abstract construction. The incumbent is retained under the shared key, but the
alternative ordering drawings are not identical to the default portfolio's
drawings. This is a strategy comparison, not an output-equivalent speedup.
Geometry/reuse contexts are local to each drawing, not a persistent or shared
cross-candidate cache. Cancellation is checked between phases and during fallback
and track repair; an interrupted drawing is never published.

For paired measurement, including a common incumbent, run:

```sh
cargo build --release --locked -p arqlet-runtime --bins --features benchmark-timings
node benchmarks/eval/abstract-pipeline.mjs --repeat 5 --out benchmarks/results/abstract-public-YYYYMMDD.jsonl
```

The harness replays the frozen views from the focused comparison and records
source/binary/input hashes, native import/core-refinement/Optimize clocks,
public two-process calls, full quality and actual abstract-pass/search counts.
`bench --pipeline-only 1` omits ancillary interaction probes (such as pins) and
marks their results as `null`; it does not skip candidates or routing rounds.

## Spacing and area (measured 2026-11-14)

The base gaps are options, so a host can trade area against crossings and bends for its own drawings. Measured with
`node benchmarks/eval/area-breakdown.mjs --options JSON` on the 15 layered datasets (the default's views, every area in the
view's default direction, bounded to 150 nodes), with `channelsByDemand` on (the default), in sum over the 15 views; elk-rs
(`@openedges/elk-layout`) on the same views for reference
([breakdown](../benchmarks/results/area-breakdown-20261114.md),
[channels by demand](../benchmarks/results/channels-by-demand-20261114.md)):

| options | lines through nodes | crossings | bends | area (millions) |
|---|---|---|---|---|
| default (`nodeSpacing` 24, `layerSpacing` 64) | 0 | 2730 | 3734 | 223.7 |
| `nodeSpacing` 16 | 0 | 2706 | 3740 | 200.7 (−10 %) |
| `nodeSpacing` 12 | 0 | 2706 | 3740 | 200.7 (a minimum holds below 16) |
| `layerSpacing` 40 | 0 | 2755 | 4144 (+11 %) | 219.9 (−2 %) |
| `layerSpacing` 32 | 0 | 2757 | 4146 (+11 %) | 218.5 (−2 %) |
| `layerSpacing` 32, `nodeSpacing` 16 | 1 | 2707 | 4152 (+11 %) | 196.0 (−12 %) |
| `channelsByDemand: false` | 0 | 2730 | 3734 | 234.1 (+5 %) |
| elk-rs (reference) | 0 | 4621 | 4078 | 146.5 |

Reading:

- **`nodeSpacing`** is the cheap lever on layered views in one direction: at 16 the area drops by a tenth at fewer crossings
  and six more bends in sum. On the default's own views of every dataset (32, per-container directions and organic areas
  included) it also draws lines through nodes (synthetic-2k 8, mixed-large 21, can-bus 1) and more bends on uml-class
  and workflow-dag, so it is not the default: a host whose drawings are layered in one direction can set it.
- **`layerSpacing`** below 64 does not pay since `channelsByDemand`: the channels take what their tracks need anyway, and
  the lanes are planned on the positions this gap gives, so a smaller gap lays border nodes off their ports' lines and
  costs a tenth more bends for 2 % of area.
- What remains of the gap to elk-rs (about 1.4 times its area at the default) is in the gaps within ranks (the minimum
  distances the block placement keeps for long-edge dummies, port leads and corridors between siblings), container margins
  (header 36, padding 12 per side, a rank of border nodes 36 per side) and channels wider than elk-rs' for the same
  tracks; crossings stay well below elk-rs' at every setting above.

## Measurements

`benchmarks/eval/grid.mjs` runs each variant on the same 15 views (up to 150 nodes) through the same
operations the browser uses. Metrics come from the shared harness; bundled members are not drawn and
are excluded. Raw results: `benchmarks/results/grid-*.jsonl`.

| variant | crossings | node hits | length | area | refine ms (total) | better crossings than default | zero-hit views |
|---|---|---|---|---|---|---|---|
| default | 378.6 | 3.4 | 64,801 | 3.81 M | 1,557 | — | 11/15 |
| fast (one ordering) | 388.3 | 3.3 | 66,230 | 3.81 M | 664 | 1/15 | 11/15 |
| no routing candidate | 378.6 | 3.4 | 64,801 | 3.81 M | 1,283 | 0/15 | 11/15 |
| bundling complete | 343.8 | 3.2 | 63,309 | 3.81 M | 1,551 | 3/15 | 11/15 |
| end-to-end as second-order | 378.6 | 3.4 | 64,802 | 3.81 M | 1,569 | 0/15 | 11/15 |
| direction lr (all views) | 370.9 | 3.8 | 71,054 | 4.19 M | 1,541 | 6/15 | 10/15 |
| direction tb (all views) | 381.6 | 3.4 | 56,212 | 3.94 M | 1,613 | 2/15 | 11/15 |
| direction hybrid (all views) | 382.5 | 3.3 | 56,145 | 3.71 M | 1,614 | 2/15 | 11/15 |
| direction shortest (all views) | 382.5 | 3.4 | 56,005 | 3.92 M | 1,610 | 1/15 | 11/15 |
| hybrid, end-to-end criterion | 382.5 | 3.4 | 56,005 | 3.92 M | 1,611 | 1/15 | 11/15 |

## What the numbers show

**Dominance (one option is better and not more expensive):** none found. Every change trades one
metric for another.

**Cost without measured benefit:**
- The routing-only candidate is never selected on these 15 views. Removing it keeps crossings and node
  hits identical and saves about 18% of refinement time.
- The end-to-end criterion as second-order objective changes nothing measurable: the earlier objectives
  already decide almost every comparison.

**Trade-offs that depend on intent (not dominance):**
- Fewer orderings are cheaper but worse on some views. "Fast" is 2.3× cheaper, costs 2.6% more crossings
  on average, and is clearly worse on the code graph (+94 crossings on `layerweave-code`). It is therefore
  a cheap preview, not a free switch.
- Directions trade crossings against edge length and area. LR has the fewest crossings (6 of 15 views
  better than the default). TB, hybrid and shortest give shorter edges and smaller areas. No direction
  is better on every criterion, so the choice belongs to the user.
- Bundling lowers the drawn crossings by design, because identical routes are drawn once. It is a visual
  change, not a better routing.

**Strategy search cost:** the four orderings are the only axis with a measurable quality effect that is
not intent-dependent. Searching across directions, objective orders and bundling would multiply
refinement time (roughly 4× for directions alone) for trade-offs that the user has to judge anyway.

## Decision needed

Two positions are possible. The measurements point towards the first, but the choice is yours.

1. **One default strategy (recommended by the data).** Four orderings, without the routing-only
   candidate. Direction, bundling, and objective order are explicit user options with the defaults
   above. "Fast" stays as an explicit cheaper choice for interactive preview. No automatic search across
   directions or objective orders.
2. **Optimize against all combinations.** Evaluate directions and bundling per view and keep the best
   result under the objective order. This is roughly four times more expensive for refinement, and it
   needs an explicit rule for which trade-off wins, because the measurements show no single winner.

Open questions to settle with the measurements:
- Is the routing-only candidate needed for inputs that the 15 views do not cover (dense multi-container
  graphs, the 100K target)? If not, drop it from the default.
- Should "best" exist at all, or should the default be the only named strategy?

## Limits of this analysis

- 15 views up to 150 nodes, one machine, one build. The 100K target is not covered.
- Crossings and hits are the harness metrics, not a human judgement of readability.
- Direction variants override the view's default direction; containers with explicit directions keep
  them, so "direction lr (all views)" changes only views whose default was different.
- Bundling changes what is drawn. Its crossing reduction is a visual effect, not a routing improvement.

## Measured edge and port labels

`options.edgeLabels` maps edge IDs to `{text, width, height}`; `options.portLabels`
maps resolved port IDs (explicit IDs or `auto:<edge-id>:source/target`) to the same
shape. Dimensions include text padding and must be positive finite values up to
10,000 units. Empty maps disable the corresponding labels. Configuration replaces
these maps; the separate `measure` operation merges node-size updates into existing
measurements.

Example partial `configure` options:

```json
{
  "edgeLabels": {"ab": {"text": "calls", "width": 48, "height": 22}},
  "portLabels": {"auto:ab:target": {"text": "input", "width": 42, "height": 22}}
}
```

Label sizes increase layer and sibling spacing before routing. Each computed
snapshot contains `labels` with absolute bounds and `routingIssues`. Labels are
placed clear of node bodies, container headers, routes and previously placed
labels. If the bounded search finds no space, the label is omitted with an
explicit diagnostic. Labels for hidden edges/ports remain configured and return
when visible. This is a conservative spacing heuristic, not a global joint
label/route optimization. Custom texts are supplied through the options API;
the demo toggles generate edge-kind and input/output captions with measured widths.

Explicitly bound edges are excluded from complete bundling, preserving their
individual port contracts. Default automatic parallel edges may still bundle.

## Placement search: gate and move types (measured)

`placementSearch` is opt-in and off by default. Moves are: a leaf to a position next to a linked node,
and a swap with a nearby sibling. Each candidate is re-routed locally; the final result is re-routed in
full and kept only if better. Results over the 15 datasets (`grid-placement-search-swap-*.jsonl`):

| variant | median Δ crossings | mean Δ crossings | views improved | refine time |
|---|---|---|---|---|
| local moves only, budget 200 | 0% | −7.6% | 7/15 | ×5.0 |
| local + swap, budget 200 | −4.2% | −22.8% | 8/15 | ×5.5 |
| local + swap, budget 600 | −4.9% | −27.3% | 10/15 | ×11.7 |

Gains are concentrated in views of about 11–56 nodes (for example activity-order −57%, petri-net
−29%, arqlet-pipeline −83%). Views above about 70 nodes gain little at these budgets.
One view gets worse in crossings (bpmn-pools 2 → 3): the search optimizes the objectives in priority
order, so it may trade a crossing for fewer shared route segments.

Decision: the search stays off by default. The readable profile enables it (budget 200), since that
profile is aimed at small views. On the exported view it moves the refined result from 71 to 21
crossings, with no node hits, no geometry issues and all boxes on the placement grid.

## Global rounds and route cleanup

Refinement now runs in global rounds after the candidates: the routing pass (and the placement search,
when its budget is set) is repeated on the current best, and a round keeps its single best result only
when the objective list improves. At most three rounds. Routes are also cleaned of spikes, a stub that
doubles back onto its own segment; the cleanup never changes a crossing count.

Measured on the uploaded `state-tcp` view (revision-0 (2)), single thread:

| variant | crossings | hugging | bends | length | refine |
|---|---|---|---|---|---|
| default | 6 | 5 | 43 | 5,890 | 11 ms |
| `placementSearch` 200 | 5 | 0 | 42 | 7,739 | 85 ms |

Over seven views plus this file, `placementSearch` 200 gives fewer crossings in total (1,590 vs 1,655),
fewer hugging segments (739 vs 873), fewer shared pairs (55 vs 85) and shorter total length (420,512
vs 439,116). It costs about 19× the refinement time on these views (0.6 s → 11.6 s total). It is not the
default yet; see `benchmarks/results/quality-search-20261021T063000Z.json`.

## Port placement

`portPlacement` (`symmetric` or `optimized`, default `symmetric`) decides where automatic ports sit along their
node side. `symmetric` spaces them evenly, in the order of their peers. `optimized` moves each port to its peer's
position along the side, so the route can run straight, and keeps the minimum port spacing (`minPortSpacing`, at
least 8). A side without room falls back to symmetric.

`optimized` is a preference, not a rule. In refinement, the symmetric pipeline for the same view (recomputed from the
source graph, then refined the same way) is a candidate, and the shared conflict key picks the result. So an optimized
view is never published worse than the symmetric one under the priority order. The snapshot's `portPlacement` names the
placement actually used. The default is unchanged: default snapshots are byte-identical to before this option.

Measured on the frozen inputs (`benchmarks/results/ports-candidate-20261021T140000Z.json`; default options and Raster):
the optimized view is better in 12 of 16 cases, equal in 4 (where the symmetric pipeline won), and worse in none.
Bends drop in most cases. Plain optimized placement without the candidate, which had been worse in four cases, is no longer
the published result.
