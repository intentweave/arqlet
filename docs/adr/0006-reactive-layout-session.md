# ADR 0006: Reactive layout sessions, delta-driven repair and continued optimization

Date: 2026-10-24.
Status: accepted architectural direction; implementation is partial. API names below are provisional.

Extends [ADR 0001](0001-incremental-first.md), [the recursive optimization specification](../recursive-optimization.md), [ADR 0003](0003-container-direction-constraints.md) and [ADR 0005](0005-routing-constraints.md). It does not claim that the current runtime already implements persistent incremental solver sessions.

The regional computation plan is governed by [ADR 0007](0007-recursive-strategy-orchestration.md); browser/client/rendering ownership is governed by [ADR 0008](0008-engine-client-viewer-boundaries.md). The session drives the regional plan and delivers geometry to independent consumers.

Browser product scope and migration from the separate curation mode are specified in [ADR 0009](0009-unified-interactive-playground.md), including drag reliability, configuration, resizing and container interaction.

## 1. Context

Arqlet supplies layout information progressively. A consumer can continuously submit graph changes, configuration, actual measured dimensions, requested sizes, positions and pins. Rendering may change text wrapping, font metrics, node content or container dimensions; dragging and resizing are continuous input streams. Browser-local computation and backend precomputation must operate on the same logical state.

Repeatedly invoking a full layout and comparing full scenes is insufficient as the primary architecture. Performance depends on detecting meaningful changes, invalidating the right dependencies, retaining compatible state and recomputing only the necessary scope. Locality is a performance goal, not a guarantee that arbitrary edits have only local consequences.

The existing runtime has progressive jobs, work counters, input identities and a host-side publication Session. Its current edit policy cancels the active job. Local layout/routing transactions and hierarchical composition provide reusable building blocks. This ADR adds a persistent computation model around those parts; it does not replace stale-result protection or require a UI framework.

## 2. Decision

Provide a logical reactive layout session:

`consumer updates -> accepted input state -> delta classification -> dependency invalidation -> bounded repair/refinement -> versioned layout publications -> consumer rendering/measurement`

Continuous means the session can accept updates and continue improving results. It does not require a busy loop or permanent computation when nothing changed. Scheduling, transport and clocks belong to host adapters; pure geometry and solver steps remain in the shared Rust core. A synchronous step-based core can back workers, native processes and asynchronous APIs.

The session owns requested state, accepted measurements, current complete layout, reusable derived state and pending work. The consumer owns rendering, measurement and immediate gesture preview. Display interpolation is not authoritative layout input.

## 3. State and authority

Keep these concepts separate even if the first implementation shares storage:

| State | Examples | Authority |
| --- | --- | --- |
| Semantic graph and view | Nodes/edges, containment, collapse/filter state | Consumer-authored input |
| Authored requirements | Direction, strategy, routing, port constraints, pins, size policies | Consumer-authored input |
| Measured geometry | Intrinsic content bounds, text bounds, realized outline | Consumer observations with measurement context |
| Interaction intent | Drag position, resize request, temporary axis locks | Active consumer gesture |
| Derived layout | Positions, selected auto strategies, ports, routes, labels, container extents | Engine result |
| Display state | Animation positions, zoom, selection, hover | Consumer presentation |
| Solver state | Embeddings, distance matrices, rank order, forces/stress iterates, indexes | Optional versioned engine state |

A consumer resize request is not just a measurement. A rendered engine-assigned width echoed by a ResizeObserver is not automatically a new authored fixed width. Every geometry update declares its role.

The effective geometry satisfies authored requirements and the active interaction while accounting for content requirements. Contradictory requests receive scoped diagnostics rather than silent priority-based relaxation. For example, a fixed user size smaller than required content needs an explicit clipping/overflow policy or a conflict; the engine must not silently reinterpret the size as a minimum.

## 4. Update protocol

A conceptual update batch contains session ID, client/update ID, expected input version, update origin, and typed operations. Apply a batch atomically after validation. Return acceptance/rejection, new input version and diagnostics separately from later computed geometry. Duplicate update IDs are idempotent. A rejected batch leaves its requested state unchanged.

Initial supported operations:

- Add/remove/update graph elements; change containment or view projection.
- Patch authored configuration by scope; resolve inheritance independently per configuration axis.
- Supply/replace measurements for elements or labels.
- Set size policy: intrinsic/auto, minimum, maximum or fixed; declare which dimensions apply.
- Begin/update/end/cancel move or resize interaction.
- Set/remove persistent pins; initially complete positions, with axis pins as a later compatible extension.
- Request bounded refinement, pause computation, resume or close the session.

The operation catalogue is not a requirement to add all operations in one PR. Existing commands can adapt to it. Bulk snapshot import remains supported for bootstrap and compatibility; bulk replacement computes an input delta once at that boundary.

Serialize all accepted updates through a session authority. Stale input patches are rejected initially with a resynchronization response; do not silently reorder topology changes. More advanced rebasing or multiple simultaneous authors requires a separate contract. High-frequency gesture updates use a gesture ID and monotonic sequence; older samples cannot replace newer ones.

## 5. Measurements and coordinate system

All layout geometry is in logical layout units and names its coordinate space. Node positions identify local-parent or absolute coordinates; adapters convert explicitly. Browser zoom, CSS scale, pan, device pixel ratio and animations must not alter intrinsic layout dimensions accidentally.

A measurement includes element/label ID, measurement revision/context, geometry role, dimensions or outline, and dependencies such as content/style/font revision. If wrapping depends on assigned width, include that width and the layout publication that supplied it. Discard measurements for removed elements or obsolete content/assignment contexts. Do not require an unrelated newer layout version to invalidate an intrinsic measurement whose dependencies remain current.

Use host measurements when available; provisional estimates are marked as such. Replacing an estimate invalidates dependent geometry. Headless/backend execution can use the same supplied measurements and returns their provenance; it must not invent browser font equivalence.

For shapes, measured bounds and contour follow ADR 0005's common outline contract. Leafs derive content requirements from labels/content. Containers derive requirements from children, header/padding, ports and authored dimensions. Measured outer size, intrinsic minimum and requested fixed size are distinct values. Do not multiply own coordinates or count children twice.

## 6. Detect deltas before scheduling work

Explicit typed updates are the primary source of deltas. Do not diff full rendered scenes or serialize the entire graph for every pointer move. On receipt:

1. Validate IDs, input versions and coordinate/measurement context.
2. Compare the supplied fields with their current effective values.
3. Resolve inherited configuration only in the affected scope and descendants, stopping where a local override shields them.
4. Emit semantic deltas only for effective changes; retain declaration/provenance changes even when geometry is unaffected.
5. Coalesce compatible pending updates, invalidate dependent artifacts and schedule work.

Use stable IDs, per-element/property revisions and optional cached fingerprints. Hashes are accelerators, not a substitute for correct dependency keys. Do not rehash all elements for every local update. Exact equality is suitable for authored configuration; measurement noise may use an explicit precision policy. Such filtering must not hide an authored resize or weaken required clearance. Keep requested values distinct from layout snapping/tolerances.

Coalescing rules:

- Latest move/resize sample wins within one active gesture; preserve gesture begin/end/cancel and persistent-pin changes.
- Latest compatible measurement replaces older pending observations for the same context.
- Preserve graph operation order; do not compress delete/recreate into an unrelated property change.
- A single batch changing content and measurement must validate their mutual context.
- Selection/color/pan-only updates do not trigger layout; font/wrapping/outline changes may.

## 7. Dependency graph and invalidation

Store derived artifacts with explicit read dependencies. Invalidate phases, not the entire session by default.

| Delta | First affected artifacts | Potential expansion |
| --- | --- | --- |
| Node moved | Bounds/index entries, attachments, incident routes and spatially intersecting routes | Neighbors, region boundary and ancestor extents |
| Node resized / outline changed | Bounds, port capacity/anchors, labels, local overlaps and affected routes | Container growth and exterior routes |
| Text/font measurement changed | Label/body space requirements and owning geometry | Size/route/neighbor repair only if effective geometry changes |
| Port or pin changed | Port assignment or placement constraints and dependent routes | Region feasibility and boundary contract |
| Edge inserted/removed | Endpoint demand, local topology/recognition, graph distances/order | SCC/block changes, eligible strategies, potentially whole region |
| Containment/collapse changed | Projection mapping, ownership, local transforms and interfaces | Parents and cross-boundary connections |
| Direction/strategy/routing changed | Resolved affected area and incompatible strategy artifacts | Descendants that inherit, interfaces and exterior routes |
| Quality/search policy changed | Evaluation and pending search allocation | Retain geometry/caches when compatible; do not relayout solely for metadata |
| Presentation-only changed | Renderer | No layout invalidation |

The affected route set includes incident edges, edges whose ports move indirectly and routes intersecting old/new obstacle or clearance envelopes. A swept gesture envelope may help conservatively discover affected elements, but final geometry is checked at the latest accepted position. Account for labels, diagonal/curved routes and authorized container crossings. An endpoint incidence list alone is insufficient.

Use old and new bounds in spatial broad-phase queries and shared contour predicates in narrow-phase checks. Maintain indexes by changed entries. Moving a fixed-size container can reuse its children's local geometry and transform them, while invalidating exterior routes; resizing its interior or boundary contract may require more work.

Begin with adjacency and regional dependency maps plus a spatial index where measured scans are expensive. Incremental evaluation may be added independently; full rescoring is initially acceptable if bounded and measured. Eventually validate incremental metrics against complete recomputation. No R-tree or specific cache layout is mandated without measurement.

## 8. Repair and progressive refinement

Accept interaction/input changes promptly; immediate acceptance does not promise a complete optimized scene per pointer sample. Consumer preview follows the latest gesture immediately. The engine prioritizes:

1. Update requested geometry, bounds, attachments and affected-set discovery.
2. Construct the fastest complete consistent candidate under current inputs, or report unresolved conflicts if no eligible candidate is available.
3. Repair local node/label conflicts, ports and routes with unchanged boundaries where possible.
4. Enlarge to dependent neighbors, owning container, ancestors or broader strategy alternatives when needed.
5. Continue bounded quality improvement from compatible candidates and solver state.

Expansion follows dependencies and conflict evidence, not only a geometric radius. Include container-boundary feedback and fixed requirements. If a required move introduces an unavoidable local conflict, retain the latest input and show explicit conflict status or preview; never restore the old user position because it had a better score. Constraint satisfaction, termination and optimization progress are separate statuses.

Every published complete candidate is evaluated under the current request. Score monotonicity applies within one fixed evaluation context; it cannot be required across edits. Establish a current-context baseline after each accepted input update. Do not repeatedly change a movement reference during one search and then compare incompatible scores.

## 9. Dragging, resizing and pins

A gesture owns its specified position/size until end or cancel. Temporary interaction locks and persistent authored pins are separate. Other operators may adjust permitted neighbors but cannot displace the active element against its current intent.

- Begin: record a restoration baseline for interaction-owned properties and the current parent coordinate system.
- Update: accept the latest coordinates/dimensions, coalesce work and supersede obsolete publications.
- End: final sample is applied atomically with an explicit release policy: retain a persistent pin or release into a preferred/warm-start position. The API requires that policy; UI may default to release, preserving any pre-existing persistent pin.
- Cancel: restore interaction-owned properties unless superseded by a later authoritative edit. Do not roll back unrelated graph/configuration changes.

A resize anchor determines which point stays stationary; resizing must not unintentionally move the opposite edge/center because different adapters assume different anchors. Define node-local attachment offsets when dimensions change. Detect incompatible fixed-size, content and containment constraints. Container resize may constrain its interior rather than simply scale all children.

Keep these controls separate: temporary interaction lock, persistent position pin, fixed dimensions, fixed strategy, and required direction/routing/ports. A fixed strategy does not fix every coordinate; a position pin does not select an algorithm.

## 10. Continuing force/stress solvers and fixed options

Both requirements are explicitly part of this decision: continuing iterative Force/Stress computation, and continuing optimization under forced (explicit fixed) configuration.

### Iterative continuation

Support bounded solver steps with explicit checkpoints. Distinguish:

- **Warmstart:** restart a valid solver from current positions. Widely portable and the first implementation target.
- **Exact resume:** retain compatible internal state such as iteration counters, convergence history, step size, velocities for force solvers, sparse/dense distance structures or stress solver auxiliaries. Requires strategy/version and dependency compatibility.

Do not restart Pivot-MDS or equivalent initialization on every drag/measurement. Reuse topology-dependent graph distances when only positions or sizes change; invalidate size-dependent targets when their scale changes. Reuse unaffected topology/region analysis. After a pin/position edit, overwrite the relevant iterate and reset incompatible velocities/momentum/convergence state according to the solver's rules. Solver changes must honor pins, required margins and current contours, not just apply them after convergence.

An edit can affect a stress solution beyond immediate neighbors. Freeze an external boundary as a bounded local approximation, report that approximation, and expand/unfreeze as needed. Never claim exact equivalence to a global optimum from local continuation. Disconnected components use existing component decomposition rather than artificial cross-component pairwise forces when independent computation suffices.

A converged session idles until an update or explicit further-search request. More budget or another initialization is an explicit continuation policy, not endless CPU use. The internal stress/energy guides a solver; the common evaluator still selects complete candidates across strategy families.

### Fixed configuration continuation

Keep authored fixed strategy, directions, routing geometry, ports and shape/size constraints during every repair and resumed job. Continue varying only the remaining degrees of freedom: positions, permitted order, spacing, labels and routes. Auto/allowed choices can be reconsidered when their dependencies change; report the choice without serializing it as a fixed author requirement.

An unsupported forced combination gets a capability diagnostic, not silent fallback. Search exhaustion may request broader scope or more budget but does not authorize relaxing fixed options. `forceRecompute`/cache bypass, if introduced later, is a separate execution control and does not mean ignoring constraints.

## 11. Publications and stale-result protection

Publish snapshots initially; atomic geometry patches may follow when their dependency contract is implemented. A publication carries session epoch, accepted input/evaluation identity, job ID and sequence, base layout version, measurement/configuration/view context and completeness/conflict status. A patch also identifies additions/removals and all geometry it updates, including indirectly moved ports, routes, labels and ancestor bounds.

Display revision and solver checkpoint sequence are not semantic graph revisions. Layout-only changes do not increment graph revision. New consumer input advances its appropriate revisions and the session input version. Showing an improvement does not recursively create new authored input or invalidate the running job's immutable base.

Initial policy: reject every publication for an obsolete accepted input version. Preserve compatible internal caches, but compute a new complete candidate under current input before publication. A disconnected unchanged region's old work may be reused internally; accepting old patches directly requires later read/write dependency checks. Cancellation alone is not a substitute for the revision gate.

Never publish half a transaction (new node with old ports/routes, or changed parent with stale absolute child geometry). A consumer may render an explicitly marked provisional gesture overlay independently. Publication frequency is throttled independently from input acceptance and computation. Drop superseded pending display updates while retaining the best compatible checkpoint.

## 12. Backend/browser handoff and persistence

The logical protocol is transport independent. One authority orders input updates; browser and backend do not mutate shared memory. Browser-local repair may provide early results while a backend computes wider improvements. Backend results based on old measurements, constraints or gestures are rejected by the same gate as local results.

A portable snapshot includes graph/view, declarations, accepted measurements, pins, layout geometry and identity. Optional strategy checkpoints/cache state carry version/dependency keys and work progress. Missing or incompatible caches fall back to a warmstart. Exact solver resume is not implied by snapshot portability. Different runtime numerics require documented tolerances; machine-independent bitwise continuation is not promised.

Do not implement simultaneous browser/backend refinement as a prerequisite for the first reactive session. Establish one-authority update and publication semantics first, then reuse them for handoff.

## 13. Performance and observability

Measure separately: delta acceptance/no-op detection, inherited-value resolution, affected-set discovery, analysis/solver reuse, placement, port resolution, routing, validation, evaluation, serialization and time to first complete current-input result. Record affected nodes/edges/regions, invalidated/reused artifacts, work counters, escalation and publication sizes. Reuse existing counters; do not require a new profiling framework.

A local update should demonstrate bounded scope on suitable fixtures, not just a faster total runtime. Some updates legitimately invalidate a whole region or graph. Separate these cases in benchmarks. Compare repeated updates against cold recomputation on identical accepted inputs; do not compare obsolete display states. Correctness means current-input validity and contract compliance, not necessarily identical geometry to a cold solver's different local minimum.

No hard latency targets or new score calibration is decided here. Establish measured baselines in WASM and native modes before promising timings. Required resource limits remain enforced.

## 14. Implementation sequence and verification

1. **Input/session adapter:** typed delta batches for measurements, position/size and pins; shared root/container constraint resolution; explicit accepted-input identity and no-op detection. Keep the current publication gate.
2. **Reactive geometry path:** move and resize leaf nodes in a container, discover spatially affected routes, update bounds/ports, compose and publish complete geometry. Preserve unrelated local state and diagnose conflicts.
3. **Iterative warmstart:** connect existing Organic start positions to layout; bounded steps and cancellation; retain compatible analysis. Continue fixed strategies without reinitialization.
4. **Regional reuse:** parent-boundary escalation, container movement/resize and multi-region updates; incremental indexes and metric updates where justified.
5. **Optional exact checkpoints/handoff:** versioned solver continuation and dependency-validated remote execution. No need to finish this before interactive use.

Required verification:

- Repeated identical measurements and zoom-only changes schedule no layout work.
- New content/font or assigned wrapping width invalidates relevant measurements; old feedback is rejected.
- Several gesture samples while a job runs retain the latest intent; old results fail the gate even if cancellation is ineffective.
- Resize updates port positions and routes; nonincident routes hit by old/new envelopes are considered.
- End/cancel and pre-existing pins behave as specified; unrelated edits are not rolled back.
- Shrinking below required content or enclosing pinned children produces a scoped conflict, not silent relaxation.
- Moving a container reuses local child geometry where possible and repairs cross-boundary routes.
- Fixed strategy/direction/routing options survive repair; unsupported combinations get diagnostics.
- Force/Stress warmstart does not reinitialize unaffected positions; exact-resume incompatibility falls back safely.
- Aborting any stage preserves a complete previous publication, while indicating whether it matches the newest input.
- Native, worker and backend adapters share update/revision semantics.

## 15. Consequences and remaining product choices

The session becomes the central consumer-facing interaction model. Existing one-shot APIs remain useful adapters. Persistent state increases memory use and requires dependency bookkeeping, but avoids repeatedly reconstructing unchanged analysis. Correct local invalidation is foundational; a scheduler cannot compensate for stale caches or incomplete affected sets.

No open product choice blocks this architectural decision. Initial implementation uses explicit gesture release policies, intrinsic measurements distinct from authored sizes, strict stale-input rejection, warmstart before exact resume and fixed requirements retained throughout. UI defaults for pin-on-release, content clipping and continuous-versus-release-only neighbor movement can be selected later without changing those contracts. Detailed wire schemas and numerical measurement tolerances are implementation follow-ups, not hardcoded here.

## Implementation status (2026-10-27)

Implemented: gestures that end exactly once and an epoch for replies ([ADR 0015](0015-user-decisions-as-engine-values.md)); a measurement that carries only a
revision, or a size within `MEASURE_TOLERANCE`, lays nothing out again (`recompute_with`); local layout and local routing of an edit
with a contract, a report and a fallback ([ADR 0012](0012-local-edits.md)); the history of decisions as `intent` and `restore`; `settle`
after a drag; a typed `PublicationEnvelope`; the page's and the Rust publication policy run the same cases.

Deferred, and not part of the code: idempotent update IDs; `begin`/`end`/`cancel` for resizes as for moves; per-property revisions; a
solver state kept in the snapshot (the continuation starts from the positions); stop signals and work budgets inside the placement; slim
move requests (measured, see the results file: the engine boundary is a JSON string, so only the structured copy would be saved);
`requested_sizes` apart from measured sizes.
