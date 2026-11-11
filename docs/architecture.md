# Architecture draft

See [Recursive, score-driven optimization](recursive-optimization.md) for the proposed regional strategy portfolio, shared quality model, routing/port/label catalogue and migration plan. This is a target architecture; existing acceptance behavior remains unchanged until the documented migration.

The target decisions are [ADR 0007: recursive strategy orchestration](adr/0007-recursive-strategy-orchestration.md), [ADR 0006: reactive sessions](adr/0006-reactive-layout-session.md), and [ADR 0008: engine/client/viewer boundaries](adr/0008-engine-client-viewer-boundaries.md). They define the target beyond older prototype descriptions below; implementation completion is tracked separately.

[ADR 0009](adr/0009-unified-interactive-playground.md) defines the next browser milestone: reliable pointer interaction, current configuration and generated models, one editing mode, container/resize support and engine-backed port/edge authoring.

## Execution model

Canonical graph + view options -> projected graph -> constraints -> layout phases
-> routing -> scene. A Rust core is shared by browser WASM and native backend.
The renderer and UI framework are independent clients of the scene.

The provisional `LayoutOptions.abstract_routing` opt-in now connects complete
abstract routing at the placement/routing boundary of `compute_once`, before
the legacy channel-count/probe/reroute loop. The shared completion module returns
a snapshot with grown local boxes, grown stored plans, resolved absolute ports,
complete logical routes, boundaries, labels and diagnostics. Native import,
ordering candidates and subsequent supported edits all reach that same code
through dispatch; no host-specific graph semantics or cache is added.
Strict track conflicts make abstract ordering candidates ineligible. Existing
routing-only jobs retain their acceptance rules. The incumbent
and existing strategy list remain; the alternative changes candidate geometry,
not default behavior. Routing-only improvements retain their existing algorithm.
Unsupported capabilities fail explicitly. See
[configuration](configuration.md#provisional-abstract-routing-execution) for scope.

## Graph and views

Containment is a forest; logical edges may be cyclic. IDs remain stable across
snapshots, projection and edits. Ports can constrain side, order and position.
Container boundary ports map to internal endpoints. Routes crossing boundaries
retain the original logical edge identity. Container coordinates are local.

LOD simplifies rendering; LOA reduces the actual layout graph. Drill-down and
cut-out offer position-preserving and compact-relayout modes. View-specific
collapse state never mutates the canonical graph. Bundles retain member IDs.

## Incremental-first pipeline

Validation -> projection -> cycle handling -> rank assignment -> ordering ->
coordinate assignment -> bounded container coordination -> routing and labels.
Grid and flexible placement share earlier phases. Long edges need a compact
representation rather than uncontrolled dummy-node expansion.

The hierarchical prototype separates coordinate-free regional ordering from
coordinate realization. Within one complete layout, each region can reuse its
last ordering only when the incoming ranks, links, components, ordering mode
and earlier-plan ranks/fixed flag match exactly. This ordering memo does not
cache geometry or share mutable state between candidates. Coordinate-dependent border feedback
still invalidates ordering, and placement/repair passes still run.

Region projection, deterministic cycle breaking/ranking, long-edge/border
structure and component grouping have their own run-local memo. Its immutable
graph/options context is one `compute_once`; global-flattening scratch calls
use a separate memo. The key includes ordered children, structural options and,
where border placement depends on them, ancestor reversal decisions. Node
sizes, channel gaps and earlier order are not structural inputs. Port-capacity
growth still reads current leaf sizes after structure reuse.

For non-flattened hierarchical regions, cross-flow placement is a separate
plan: free sliding, dummy sizing/slots, lane alignment, border reordering and
channel track counts. A run-local memo per region and axis compares ranks,
links, chains, edge membership, incoming lane offsets, initial cross geometry,
extent, spacing and sliding mode exactly (including floating-point bits).
Changes along the flow axis alone do not invalidate this plan. The coordinate
realization still reads current child sizes, flow positions and requested
channel gaps, then applies the same cumulative rank shifts as a fresh run.
Child-border changes invalidate incoming lanes; border-order feedback still
invalidates ranks. Global flattening retains the uncached path. Tree, radial
and organic placement are unchanged.

After realizing children and ordering, a full-view non-flattened hierarchical
area can also reuse its complete local realization. Its exact key includes
structure, ordered ranks, current child/virtual sizes, locally read gaps,
child border ranks/geometry, the fixed-order flag and ordering mode. Reuse
restores local boxes, the complete area plan and any chosen orientation,
without replacing the freshly realized child interiors. A grown child
invalidates its parent; changed border geometry also invalidates it even when
the child's outer size is unchanged. Unrelated channel growth does not
invalidate a region. This cache is not used for partial/warm-start placement
or flattened areas. The hierarchy is still traversed to check dependencies:
this is selective coordinate realization, not a persistent graph scheduler.

Channel-width preparation precedes construction. If the last bounded probe
already proves that the existing late-widening step will discard every route,
the complete routing pass is deferred until after widening. This applies only
to full-view hierarchical construction, not local edits or the channel router.
The final routing and validation remain unchanged. Deterministic work counters
separate order reuse, deferred passes and constructor-internal dynamic-programme
search from delegated fallback; optional native benchmark timers are nested,
not additive stage partitions.

A provisional `layout::abstract_routing` shadow planner reads only ordered
ranks, chains, reversal decisions, containment and border identities. Its
hierarchical edge traversal is shared with the constructive router; coordinates,
dummy rectangles and physical port/lane offsets are not inputs. It keeps logical
edge IDs/direction and expands long edges and nested border crossings into
channel sections. Discrete incident-port order, parallel lanes, pairwise
channel-order preferences, deterministic cycle breaking and conservative
ordinal interval slots are reported. Broken preferences are diagnostics, not
proofs of infeasibility. Ordinal positions on different ranks are not aligned
physical coordinates; slot counts are neither physical widths nor lower bounds.

With opt-in `abstractRouting`, placement also measures ordinal interval overlap
for each region's local chains before assigning coordinates. This demand seeds
the flow-axis rank gaps; the subsequent geometry-based width check remains
authoritative and can still shift ranks or trigger growth. This brings a
structural capacity estimate into placement, but does not make the complete
shadow plan or physical routing coordinate-free, nor does it prove clearance.

The shadow API partitions every edge into a complete structural path or an
explicit unsupported reason (including self-loops and constrained ports).
It does not replace the router, change candidate admission or apply geometry.
Benchmark `--abstract-plan 1` analyzes initial, final and materialized candidate
snapshots **after** the timed engine calls. These ordered ranks currently come
from placement: this is not yet a complete pre-coordinate planning pipeline.
Benchmark-only constructor events identify each DP call's logical edge, phase, cells,
transitions and failure, plus planned obstacle/track failures. They can be
joined to the shadow sections by edge ID; cumulative events span all routing
passes, whereas the shadow describes the candidate's final rank order.
Recording those events adds benchmark-only overhead inside the timed engine
calls; it is absent from ordinary and WASM builds.
`constructive::abstract_realization` now provides a separate physical trial:
it preserves resolved endpoint positions, gives a shared dummy/border junction
one absolute lane on both sides, projects abstract channel order onto actual
lane intervals, and uses the existing exact track-spacing recurrence. Missing
lane capacity and channel width are explicit failures; tracks are never
compressed to fit. Monotonicity, inflated obstacles and accepted-route
occupancy are checked before a trial route is admitted. Every graph edge
remains accounted for, including unsupported ones.

A bounded `growth::widen` trial reads demand from those exact inputs, realizes
flow-axis gaps in fixed rank order and propagates child extent changes to
parents bottom-up. Moving a container translates its subtree; dummy geometry
stays in the owning area's frame. Existing gaps, cross coordinates/extents and
rank/port order are preserved. Demand is recomputed from the current drawing,
not reused from a potentially stale report. Pins, fixed container sizes,
placement grids, maximum extents, flattened membership and non-uniform/non-positive flow are explicitly
unsupported by this first growth prototype; authored constraints are never
silently overridden.

`realize_coordinated` adds a separate longitudinal trial on the widened drawing.
Existing channel bend coordinates are frozen first. Each endpoint/dummy/border
junction reserves its full longitudinal interval between those bends; one
absolute cross coordinate is used through a container transition. Endpoints
have fixed bounds, while virtual junctions may move only inside their supplied
rectangles. A global sweep finds overlapping intervals across area boundaries.
A total order by preferred cross coordinate and identity turns separation
constraints into a DAG. Forward minimum values detect order-specific capacity
failures, then backward upper bounds and a final forward pass keep feasible
lanes near their preferred values. Failed edges are explicitly delegated;
there is no reordering, offset probing, path search or candidate pruning.
Failure is not a proof of unroutability, and removing failed reservations does
not trigger a retry to maximize the accepted subset. Newly non-straight
sections use an existing channel track or the centre of an otherwise straight
channel; final monotonicity, obstacle and full accepted-occupancy checks remain
authoritative, including cross-flow conflicts introduced by lane changes.

`realize_joint` adds endpoint-aware bend ordering before lane compaction.
When nominal longitudinal intervals overlap and their cross-coordinate
domains cannot realize the preferred separation, the selected disjunction
orders the first interval's end before the other's start along flow instead.
This adds a bend precedence or tightens a bound against a fixed endpoint.
Along-edge bend order is retained. A stable topological order prioritizes
original bend coordinates; within each channel, overlapping nominal cross
spans receive track-separation constraints in that order. Forward minimums,
backward upper bounds and a final forward pass compact bends on each channel's
track grid, bounded by rank clearance and the unchanged resolved endpoints.
These bends then feed the existing global longitudinal lane compaction.
There is one selected order, no disjunctive search or iterative repair.
Cycles and their downstream blocked edges, and selected-order capacity
failures, are explicitly delegated without claiming graph infeasibility.
Lane reversal, changed port positions, full conflict-chain reasoning and
pre-placement scheduling remain open. Cross spans used for bend constraints
are nominal; final full-route occupancy remains authoritative after lane moves.

Longitudinal compaction records the propagated longest predecessor path when
a selected-order domain overflows: reservation identities, lower/upper
bounds, required track distance and the deficit. This is a historical
propagation witness; prior edge removal may already have relaxed part of
that path. It is not a graph-infeasibility or active-subset-minimum proof.
`realize_chains` performs exactly two planning passes: probe the joint plan on
private clones, then select at most one temporal cut per witness and replan
once from the original lane preferences and channel assignments. Both
directions of every adjacent path link are checked against the probe's bend
bounds and fixed endpoints. Least nominal movement wins, then most slack,
with stable path order for ties. Duplicate temporal orders are deduplicated.
An uncut witness remains explicit. The final pass adds these selected orders
before topological bend compaction and reruns longitudinal compaction; no
routes are materialized during the probe and no best-result selection occurs.
The cuts may introduce cycles, capacity failures or lose routes. They are
locally feasible bound choices, not a simultaneous feasibility proof.
No retry loop, alternative order search, endpoint change or extra widening
is hidden in this trial. Fully coordinated conflict-chain/order selection,
pre-placement scheduling and quality-aware selection remain open.

The separate provisional `abstract_realization::completion::run` diagnostic
completes either the chain trial or the existing constructor on identical
freshly widened node geometry. It returns a detached scene, not a publishable
snapshot or runtime operation. Supplied endpoints stay fixed in the chain
trial; the existing constructor's automatic slot changes are reflected in
its fallback seeds and ports. Every resolved edge appears once, fixed routes
remain byte-identical, and missing routes or changed endpoints are errors.
Both use the same legacy fallback. Its improvement passes receive fixed
occupancy, but its final repair/separation stages do not; strict constructor
track conflicts are therefore enumerated after completion, independently
of shared metric hard violations. Neither zero hard violations nor route
completeness implies that track spacing holds.

The third completion variant, `ChainsFixedTracks`, adds a final provisional
`arqlet_routing::fixed_tracks::repair` after every legacy routing stage.
Only delegated routes can move. Interior segments can shift to exact track,
interval-end or corridor events; a blocked span can receive a local orthogonal
bypass bounded by its occupancy endpoints and track distance. This is not a
new grid/path search or an arbitrary-offset retry policy. Candidate enumeration
and accepted moves are counted explicitly. Every accepted move removes at
least one conflicting partner and adds none, preserves exact route endpoints
and their directions, and adds no obstacle or required-own-clearance hit.
Thus successful moves are bounded by the initial moving/moving plus
moving/fixed conflict-pair count. Ties prefer fewer points, then less
displacement, then deterministic segment/event order. Fixed routes remain
immutable, perpendicular crossings remain allowed, and unresolved pairs are
returned explicitly rather than claimed infeasible. Shared endpoint legs get
no spacing exemption; some such conflicts cannot be repaired without changing
authored ports. This final diagnostic repair does not alter production legacy
routing, repair-only callers or candidate acceptance.

This is not quality-aware abstract order selection: local bypasses can add
bends/crossings and worsen the published key even while repairing strict track
spacing, which that key does not encode completely. The detached experiment
reports both, without automatically accepting either variant.

The opt-in `ChainsAligned` variant adds two fixed ordinal pair-cost sweeps and
maximal common-domain lane runs before channel growth. Demand is planned
without constructing disposable routes; bottom-up growth then fixes flow
coordinates and the existing chain materialization/fallback/repair follows.
Cross geometry and endpoint domains remain inputs, so this is not a complete
pre-coordinate scheduler. Its geometry may differ from the first three
completion variants. Shared metrics and strict spacing are checked in full;
the ordinal heuristic is not an infeasibility proof or a pruning criterion.

`abstract_realization::planning::Session` reuses exact ordinal assignments,
edge lane domains and ordered channel cross spans/spacing within a candidate
sequence. Each current key has one replaceable entry, and removed keys are
pruned. By default no route, obstacle result, final coordinate or score is cached. Graph
validation and fresh realization remain mandatory. A main-axis translation
therefore reuses plans but changes all output coordinates; a single endpoint
cross change invalidates only its edge's domains and dependent channels.
Global ordering candidates still change almost every cross domain. This is
local **planning** reuse, not local routing/placement/scoring or production
refinement integration. `bench --routing-planning 1` measures cold/warm trials
and explicitly reports ineligible full candidates and a non-publishing
diagnostic choice against the prior baseline. See
[planning measurements and limits](performance-review-20261104.md#20-quality-aware-lane-planning-demand-handoff-and-exact-subplan-reuse).

`abstract_realization::regional` now provides detached adjacent real-sibling
rank swaps. Its opaque candidate borrows its immutable source and keeps the
placement-only snapshot private; it cannot be published with stale routes.
Only the selected frame runs placement, with its requested real-node order
fixed. Derived border ordering may change. Existing frame extents are not
shrunk. Positive growth uses the existing `push_grown` helper to move dependent
siblings/dummies and propagate to ancestors, without rerunning ancestor
ordering or cross placement. Unaffected local frames and plans remain exact;
absolute movement may still spread through a resized ancestor. Port refresh
depends on moved node/peer geometry, changed order keys, newly placed lanes
and changed border geometry. By default every route and quality metric is then
computed afresh; a local placement is not evidence that routing influence is local.

The explicit diagnostic `bench --regional-routing 1` samples at most eight
moves (first adjacent pair per non-root area, stable area/rank order). It does
not modify the production candidate portfolio or infer infeasibility from a
crossing proxy. Each candidate forks the common base planning session so one
trial cannot evict another trial's unchanged dependencies; cloning cost is
included in wall totals. Cold/forked complete scenes, keys and strict conflict
reports must match. Full eligibility and the published key decide diagnostic
improvement only, never automatic publication. Unsupported growth constraints,
flattening and visible-node budgets are explicit errors. Regional route
and metric reuse remain separate work; see
[regional findings](performance-review-20261104.md#21-regional-order-trials-with-anchored-size-propagation).

`planning::Session::with_repair_reuse()` additionally opts into provisional
run-local fixed-track **decision** certificates; ordinary layout and the
planning-only diagnostic remain unchanged. `bench --regional-repair 1` measures
it separately. Each decision freshly generates occupancy/corridor events and
compares the exact ordered admissible proposal operations, current route,
parameters and logical endpoints. A sorted segment-influence index covers the
current route and every geometrically valid proposal, including conservative
coverage for tiny and near-orthogonal segments. The certificate compares all
potentially conflicting occupied segments grouped by route and all relevant
outline/clearance geometry, shapes and exclusions. Remote projected tracks and
corridor limiters cannot be ignored merely because they lie outside the old
route: changed admissible proposals invalidate the decision. Unchanged
operations may survive changes to irrelevant raw event sources.

Both a chosen move and an unsuccessful bounded decision may replay, without
claiming repair completeness. Multiple occupancy states of one input route
are retained because earlier moving routes change later passes. Successful
calls replace the session with only visited decisions; errors/cancellation
retain the prior session. Candidate branches fork the immutable base cache.
Graph/input validation, current partners, initial/final conflict checks,
construction, fallback, labels and quality evaluation stay fresh. Work records
computed/reused decisions and actually avoided candidate trials separately;
the cold count must equal executed plus avoided trials. Complete cold/forked
scenes and keys remain mandatory. The frozen regional sample yields one small
cross-candidate hit, not a general refinement speedup; miss-time certificate
overhead is reported rather than hidden. See
[repair certificates and costs](performance-review-20261104.md#22-exact-fixed-track-repair-certificates-and-their-limits).

The separate provisional `planning::Session::with_construction_reuse()` opts
into fixed-path construction reuse instead of repair reuse. Endpoint/flow
preparation remains fresh but omits per-edge search obstacles and indexes,
including during demand planning. Only the final materialization builds one
shared inflated-box index. Its geometric keys contain exact resolved endpoints,
axis, logical reversal and ordered corners. Current segment/box queries
recollect all potentially intersecting obstacles after current endpoint/ancestor
exclusions; their identities and inflated rectangles must match before a
clearance decision (including rejection) can replay. This deliberately preserves
the constructor's rectangular clearance rules, not the fallback router's
separate outline rules. Both paths use the same endpoint-exclusion and point/
straightening helpers.

Only geometry and clearance are cached: current graph-order occupancy acceptance
always reruns, even on a hit. Unattempted entries are pruned after each complete
construction; candidates fork the common base session independently.
Coordination, growth, fallback, final repair, labels and quality stay fresh.
`bench --regional-construction 1` measures original eager preparation, a cold
shared-index session and the primed fork in rotated order. All three must keep
the same complete scene, fixed/delegated partition, key, conflicts and
deterministic constructor counters. The historical `plannedAttempts` counter
still counts acceptance attempts; separate actual geometry/clearance counters
prevent hits from being mistaken for eliminated occupancy checks. Nested
constructor clocks distinguish endpoint preparation, channel assignment,
global coordination and final materialization. The frozen `reference` move
reuses 334 of 346 geometries/clearance decisions, but the measured wall saving
is mostly shared preparation, not cache replay; roughly 29 ms of coordination
still reruns. This is not a general route cache or production default change.
See [construction reuse and its control](performance-review-20261104.md#23-shared-fixed-path-preparation-and-certified-construction-reuse).

The separate provisional `planning::Session::with_coordination_reuse()` also
shares immutable rank-span and interior-junction geometry between the chain
probe and final joint/continuity passes. Its context borrows exactly one grown
drawing, memoizes exact results or explicit failures, and is never retained in
the session or copied into candidate forks. Endpoint domains read current
trial lanes and rectangle projection uses the current trial axis. Placement,
growth or plan changes always get a new context. All constraint discovery,
solvers, cuts, witnesses and occupancy decisions remain fresh.
`bench --regional-coordination 1` compares the previous shared-construction
cold path, a fresh path with within-drawing sharing and an independent primed
fork with a new context. This isolates projection sharing from earlier
obstacle preparation and cross-candidate cache effects. Actual/reused-work
counts and complete outputs must match across orderings and all three arms.
No repair-decision reuse or production-default change is implied. See
[coordination geometry and its limits](performance-review-20261104.md#24-sharing-immutable-coordination-geometry-within-one-drawing).

Labels, boundary crossings, geometry issues, scene conflicts and the shared
published metric/key are rebuilt on the grown drawing. The diagnostic
rejects non-legacy engines, bundling and explicit routing styles, and retains
the growth capability restrictions. Native benchmark-only phase clocks
cover abstract planning, growth, construction, seeds, fallback, fixed-track
repair, finalization and evaluation;
default/WASM builds do not read a host clock. `bench --routing-completion 1`
is a separate opt-in mode, with warmups, alternating repeated pairs and
separate wall totals including all phases. This is not a full pipeline or
refinement timing, acceptance rule or router default change.

The common scene metric now prepares segment boxes once per normalized route
and combines overlap/parallel/crossing work into one ordered pass. This applies
to all engines and per-route conflict queries; floating accumulation order,
touch checks, exemptions and metric definitions remain unchanged. It is not
a cross-candidate score cache. The former evaluator and a conservative spatial
route-pair index remain benchmark controls under `benchmark-pairs`. Production
retains exhaustive route-pair generation: the tree helps sparse controls but
does not consistently help the frozen broad-envelope layouts. See
[shared metric work and the index limit](performance-review-20261104.md#25-shared-segment-evaluation-and-a-measured-spatial-index-limit).

Benchmark `--abstract-realization 1` includes fixed-geometry, independently
widened, coordinated-lane, joint-bend and chain-cut trial geometry/routes for initial, final and materialized candidate
snapshots, outside engine timing. These trials do not alter any snapshot,
router choice, candidate admission or acceptance. Labels and full-layout
quality are not evaluated. Exported lane bounds, coordinates, interval
reservations, bend bounds, temporal orders, capacity witnesses/cuts and separation constraints allow
independent verification. Remaining selected-order capacity failures require
conflict-chain/ordering decisions before this can replace routing search.
Early heuristic pruning remains a separate explicit quality/budget decision.

An edit first repairs a small region with boundary anchors. If infeasible or
insufficient, expand to its container, dependent neighbors, then broader scope.
Expansion follows dependencies, not only spatial distance. Stop when adequate;
a global recomputation is not mandatory. New cycles may force wider invalidation.

Hard constraints are enforced or diagnosed. Soft objectives include crossings,
route length, area and movement from the previous layout. Local repair cannot
guarantee that all existing positions remain fixed.

## Reactive consumer feedback

A logical layout session accepts typed graph/configuration deltas, measured content geometry, resize intent, drag updates and pins. It invalidates dependent phases and regions, retains compatible analysis/solver state, and publishes complete current-input results. Rendering measurements, authored size requirements and derived layout geometry are separate to avoid feedback loops. Force/Stress warmstart and exact solver resume are distinct capabilities; explicit fixed options remain binding during continued optimization. See [ADR 0006](adr/0006-reactive-layout-session.md) for delta detection, interaction ownership, dependency expansion and publication semantics. This is a target contract, not a claim that the current host publication Session already implements persistent incremental computation.

## Snapshots and patches

A portable versioned snapshot includes graph/view revision, stable IDs, ranks,
ordering, geometry, ports, constraints, routes, options and engine version.
Optional caches may accelerate resumption but must not be required for correctness.
Host-supplied node and label measurements ensure consistent geometry.

Patches identify graph revision, base layout version, scope, dependencies and
changed geometry. Initially reject stale patches; later consider dependency-aware
merging. Apply related geometry atomically. Reject incompatible snapshots with
an explicit diagnostic. Transport serialization remains to be selected.

The runtime coalesces edits, cancels obsolete jobs and publishes only consistent
checkpoints. Browser and backend share computation semantics, not mutable memory.
Moving computation to the backend is scheduling, not a change of graph semantics.

## Engine, host client and visualization

The shared Rust core owns graph/view semantics, resolved constraints, geometry, regional layout, ports, routes, labels and evaluation. The runtime owns candidate planning and the target persistent computation session. Its algorithms are shared by native and WASM execution; core geometry has no browser or renderer dependency. The regional plan and reactive lifecycle are one computation model, as defined by ADRs 0007 and 0006.

The default browser integration is a framework-independent TypeScript/JavaScript client and viewer. The client transports updates to a worker or backend, batches measurements and guards publication identities. The viewer owns painting, pan/zoom, selection, accessibility and gesture preview. It emits typed intent and measured geometry through the client instead of recomputing canonical layout. React and other UI frameworks are optional adapters. See ADR 0008 for the responsibility matrix and feedback contract.

Rust builds the renderer-neutral scene. SVG is the reference renderer; Canvas/GPU backends may follow based on measured workloads. Optional Rust/WASM rendering helpers remain possible for heavy geometry work. A browser renderer can use GPU APIs from JS; source language alone is not a performance decision.

## Execution and concurrency

Pure computation can expose bounded synchronous steps while host adapters manage asynchronous scheduling, clocks, transport and worker lifecycle. Existing one-shot JSON dispatch remains an adapter; the reactive target can retain explicit Rust session/solver state. No async runtime is required inside geometry algorithms.

One runtime resource budget covers regional and alternative jobs. The native parallel map and serial WASM path are existing mechanisms, not a restriction on future host-managed workers. Deterministic work-budget mode specifies ordering and tie-breaking; adaptive time-budget execution need not explore identical candidates across machines.

Cooperative stop checks and publication revision gates serve different purposes. Preserve both; host messages alone cannot interrupt synchronous WASM without an interruptible stepping/shared signal mechanism. Backend process termination remains a host capability. Browser/backend handoff transfers versioned logical state and optional compatible checkpoints, not shared mutable authority.

## Routing and rendering

No quadratic or cubic path segment type exists in `crates/scene` or the model yet (see [recursive-optimization.md](recursive-optimization.md) §6 for the required versioned curve representation). First implement orthogonal
routing, then rounded corners, straight connections and free curves. Straight
connections are not always obstacle-free. Preserve port tangents. Route bundles
are shared segments with branches and individually identifiable members.

Animation belongs to the renderer: accepted target geometry is distinct from
interpolated display geometry. Preserve focus and protect actively dragged nodes.
Repeated optimization should not oscillate; acceptance considers movement as well
as geometric quality. Use viewport culling and spatial queries independently of
layout scheduling.

## Selected dependencies of the core

- `serde`, `serde_json`: serialization.
- `libm` 0.2.16 (exact; MIT OR Apache-2.0; https://github.com/rust-lang/libm, a Rust port of musl's libm, no
  dependencies): the transcendental functions (`sin`, `cos`, `atan2`, `pow`, `hypot`), called through
  `arqlet_model::math::PortableMath` on every platform, so that native builds and WASM compute the same. Added
  2026-11-11 for 0.2.1 after organic layouts and the cluster overview were found to differ between a native build and
  the browser. Used as a dependency, no code copied.

## Reuse candidates, not selected dependencies

- https://github.com/paddison/rust-sugiyama
- https://github.com/nadavrot/layout
- https://docs.rs/mermaid-dagre/latest/dagre_rust/
- https://github.com/eclipse-elk/elk (behavior and quality reference)
- https://github.com/coldteadotai/pr-lens (presentation and interaction reference)

Evaluate individual phases behind an adapter before adopting a whole engine.
Audit exact source and license before copying. No 100K scaling claim is inferred
from a candidate's language or project description.

## Regional routing and shared geometry

Direction, layout strategy, routing style and ports resolve independently through root/container inheritance. Authored constraints, resolved values and legacy fallbacks are distinct. Mixed route styles belong to one scene and one validation path; style-specific routing jobs must account for the geometry of retained routes of other styles. See [ADR 0005](adr/0005-routing-constraints.md) for ownership, presence, compatibility and contour-aware routing requirements.

The shared outline contract is specified in ADR 0005 §5: containment, full-segment/clearance tests, contour attachments, admissible anchors and exits, and conservative bounds. Spatial indexes accelerate broad-phase queries; contour predicates decide narrow-phase validity. Grid, visibility and corridor representations may reuse search infrastructure. Curves and rounded corners require validation after realization. Bus membership/topology remains separate from path geometry.
