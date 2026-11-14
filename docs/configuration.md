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
| `nodeSpacing` | number ≥ 0 | 24 (omitted) | provisional: the base gap between neighbours of a rank, across the flow; label room and the clearance minimum are added as before. The ELK adapter maps `elk.spacing.nodeNode` to it. |
| `layerSpacing` | number ≥ 0 | 64 (omitted) | provisional: the base gap between ranks, along the flow; label room, the clearance minimum and channel widening are added as before. The ELK adapter maps `elk.spacing.nodeNodeBetweenLayers` to it. |
| `isolatedGrid` | bool | `false` (omitted) | provisional: the children of a layered area without an edge to another child (a file of unrelated functions) stand in a grid of their own ranks after the connected children, instead of all in the first rank (one long column). The grid takes about √(k·pitch) ranks for k children (`pitch`: the gap of two neighbours over that of two ranks), so it comes out about square; the connected children are placed exactly as without them. The cluster overview's structured clusters use it. See [results](../benchmarks/results/isolated-grid-20261111.md). |
| `singlePassReversed` | bool | `false` (omitted) | experimental (ADR 0020, S1-0): a graph whose areas all flow right to left (`rl`) is laid out left to right, where single pass runs, and mirrored; logical edge direction is kept, the drawing equals the mirror of `lr`. Not `bt` yet, nor graphs with pins, label places, explicit ports or buses (those keep the earlier pipeline). Measured: [reversed-flows-20261114.md](../benchmarks/results/reversed-flows-20261114.md). |
| `straightDetours` | bool | `false` (omitted) | experimental (ADR 0020, S1-1b): a straight edge (organic, radial or straight routing) that would pass through a node it does not end at is drawn around it, on its orthogonal route pulled taut around the nodes it does not lie in; edges with a free line stay one segment, the placement does not change, and the `straight-through-node` findings follow the drawn routes. Measured: [straight-detours-20261114.md](../benchmarks/results/straight-detours-20261114.md). |
| `packComponents` | `{aspect?}` | unset (omitted) | experimental: the weakly connected components of a layered container's children (a lone child is one) keep their own ranks and are packed in rank space towards `aspect` (width over height on the drawing, default 1): components stand across the flow in columns of rank ranges, the deepest first, so the arrangement, ordering and routing stay one layered drawing. Replaces `isolatedGrid` where both are set. Fewer than two components: unchanged. Measured on synthetic shapes: [pack-components-20261113.md](../benchmarks/results/pack-components-20261113.md). |
| `communities` | integer ≥ 0 | `0` (omitted) | provisional: containers with more children than this (the root included) are divided into communities of their children (the cluster overview's Louvain), shown as containers of kind `community` named after their best connected member, collapsed at first and opened with `toggle`. View only: `sourceGraph` is unchanged. See [results](../benchmarks/results/structured-communities-20261111.md). |
| `communityHubs` | `{minLinks?, nodes?}` | empty (omitted) | provisional: children that `communities` leaves out of Louvain, so that one hub does not pull its container into one community named after it: those with at least `minLinks` links in their container (by edge weight) and those named in `nodes`. A hub stays a direct child of its container, shown as itself. The overview takes the same value as option `hubs`. Ids the graph lacks are ignored. |
| `groups` | map node → path of names | empty (omitted) | provisional: groups the data or its domain gives (`["Billing", "Invoices"]` inside the node's container), shown as containers of kind `group`, nested along the path, collapsed at first; Louvain (`communities`) divides only what is still large. View only; ids the graph lacks are ignored. |
| `levelOfDetail` | `{depth?, kinds?}` | unset (omitted) | provisional (named `initialCollapse` before engine 0.4, which is now refused): the containers an import shows collapsed: at `depth` or deeper (0: all; the top level is 0) and of the `kinds` listed. Unset: none, except added groups and communities. With it, edges between collapsed containers are drawn by one weighted route per pair, and an import's `limit` counts what the first view shows (a collapsed container once, with all it holds; a container an edge reaches comes whole and collapsed). |
| `hidden` | `{nodeKinds?, nodes?, edgeKinds?, compact?}` | empty (omitted) | provisional view state, set by `filter`: what the view hides (a node with everything below it); the layout keeps its room, so the rest keeps its place, and a route for collapsed content counts only the edges still shown; `compact: true` lays the view out without them instead (the rest closes up). |
| `partial` | map container → shown children | empty (omitted) | provisional view state, set by `reveal`: the container shows the children listed; the others wait in a collapsed container `rest:<id>` of kind `rest` (`+N more`), which a `toggle` dissolves. |
| `portSides` | bool | `false` (omitted) | provisional, single pass: an edge end whose explicit port lies across the flow or on the side against it is constructed through a lead around its node instead of the fallback router; the placement aligns the other end with the lead's lane and widens gaps for the leads, and the channel planning keeps its bends a track from them. The ELK adapter turns it on for graphs with explicit ports. See [results](../benchmarks/results/elk-port-sides-20261108.md). |
| `objectives` | priority list | node overlaps, lines through nodes, overlapping lines, border hugging (margin 12), crossings, bends, `cost` (length) last | lexicographic order of quality criteria |
| `bundling` | `none`, `complete` | `none` | edges with the same source and target share one drawn route |
| `hybridBy` | `compact`, `endToEnd` | `compact` | criterion of the Hybrid direction |
| `nodeSizes` | measured leaf sizes | empty | sizes reported by the visualization (default 180 × 54) |
| `vpscOverlapRemoval` | bool | `false` | experimental; replace organic box separation with minimum-displacement constraints and add a constraint pre-pass to geometry repair |
| `nodeEdgeRepulsion` | bool | `false` | experimental; after organic stress, move free nodes off projected edge chords found by the spatial broad phase, with a bounded step. Repair stays authoritative. |
| `sparseStress` | bool | `false` | experimental; organic stress uses graph neighbours and 50 pivots instead of the full distance matrix, and an area may hold up to 500 children (default bound 300). The repair stage still keeps an n² pair table. |
| `scalableStress` | bool | `false` | experimental; organic placement for large areas: pivot start, sparse stress with pivot terms weighted by their regions, terms for children two steps apart, then a force refinement (springs, Barnes–Hut repulsion, box collision, and in its second half a push of nodes off the edges that cross them). An area may hold up to 2000 children. The repair runs with the local scan budget, the conflicting routes are not routed again as a whole, and when every edge is straight-only no orthogonal routes are computed; remaining conflicts are reported. Takes precedence over `sparseStress`. Measured against d3-force and CoSE in `benchmarks/results/organic-scalable-20261109.md` and `organic-scalable-speed-20261109.md`. |
| `sizeAwareStress` | bool | `false` | experimental; organic stress targets add both children's half-extents to the pair length. Measured on the organic ladder (25–200): larger areas, mixed routed crossings. Not recommended. |
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
