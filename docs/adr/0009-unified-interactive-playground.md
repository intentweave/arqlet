# ADR 0009: One interactive editing mode and completion of the browser integration

Date: 2026-10-24.
Status: accepted product/architecture direction. Step 1 (interaction reliability) is implemented with a measured root cause; steps 2–4 are pending. See the status section at the end.
Baseline inspected: main `c0edc9e` (PR #79).

Extends [ADR 0006: reactive sessions](0006-reactive-layout-session.md) and [ADR 0008: engine/client/viewer boundaries](0008-engine-client-viewer-boundaries.md). Uses [ADR 0007](0007-recursive-strategy-orchestration.md) for recursive computation and [ADR 0005](0005-routing-constraints.md) for routing/contour semantics. [Curated references](../curated-references.md) remain a supported artifact and evaluation workflow.

## Context and observed milestone

The browser now provides immediate leaf-node drag preview, coalesced engine requests with one request in flight, and protection of the actively dragged node from older engine positions. Releasing retains its pin; Escape requests restoration; refused operations terminate the controller. Circle painting and controller ordering have automated tests.

The user reports that, once acquired, moving nodes and recalculation are fluid on small examples. Acquisition is unreliable and becomes nearly unusable on a model above 500 nodes; some gestures cannot be dropped reliably. Configuration is inconsistent with the current engine: other layouts do not apply reliably, circle shapes are not readily available, and additional generated models are absent from the selection. These are integration defects, not evidence that layout solvers are inherently too slow.

Current code has separate normal and curation paths. Curation holds a draft and blocks ordinary recomputation; it also provides manual geometry edits, undo and reference export/import. `editor/geometry.ts` computes persistent route modifications and geometric repairs independently in the browser. The SVG render/update path and pointer capture need real-browser investigation; a unit-tested gesture controller alone does not validate DOM event routing or scene replacement.

## Decision

Use one interactive mode for viewing, configuration, selection, moving/resizing elements and editing supported constraints. Retire the separate curation mode after its required functionality has migrated. Reference curation becomes a save/import/review workflow over the same interactive session, not a second geometry authority or mutually exclusive editor state.

Do not remove the editor wholesale or discard curated documents, undo/redo and manual intent merely to reduce legacy code. Migrate behavior in vertical slices, then delete superseded paths and persistent browser geometry. Existing tests and reference artifacts must remain usable or have an explicit migration.

The next milestone is a coherent browser editing experience, not a new layout family, score calibration or force/routing heuristic. Keep the standalone layouter and external viewer APIs independent of the playground.

## 1. Reliable pointer acquisition and gesture termination

Node acquisition and release must work independently of engine request latency and background layout. Selecting or initiating a drag must not require a synchronous engine operation or full-scene measurement. Repeated attempts must not be necessary on supported scenes, including the reported >500-node model.

Requirements:

- Hit targets identify nodes by stable ID for their contour/body and labels. Explicit priority distinguishes ports/handles, node bodies, container headers/borders, child nodes, edges and background pan. Decorative labels, overlays or invisible elements must not unintentionally intercept selection.
- Capture the active pointer on a stable interaction surface. Redrawing a node or applying a publication must not remove the capture owner or lose gesture identity. Use stable keyed elements or an interaction overlay; assess the existing whole-SVG update path before assuming capture survives it.
- Express drag threshold/hit slop in screen pixels and convert accepted movement to layout coordinates. Zoom must not make selection impractically precise or change required layout margins.
- Handle `pointerup`, `pointercancel`, lost capture, Escape, view replacement, transport failure and refusal as distinct terminal events. Cancellation must not accidentally commit a pin as if it were a normal release.
- Queue the final pointer position before finish, deliver finish/cancel once and clear all local gesture flags and capture in every terminal path. An in-flight request must not swallow release; end state cannot depend on another pointermove arriving.
- Multiple updates coalesce; topology/configuration commands preserve their ordering. Input stays responsive while the last engine step finishes. Expose finalization state without permanently locking the editor.
- Keep immediate preview nonauthoritative. Engine results may update the environment, while active intent controls the dragged element until finalization. Never restore a stale view over a newer edit to make cancellation appear successful.

Investigate event targeting, render replacement, view fitting/transforms, global pending guards, per-frame scene scans, publication work and long main-thread tasks separately. The current symptom does not establish a particular root cause. Instrument only enough to distinguish acquisition failure, lost termination and slow processing; existing request counters do not measure all three.

Verification uses actual pointer input in a real browser, not synthesized calls only to the controller. Cover a small fixture, the actual >500-node model, zoomed views, labels/shapes, fast release before an engine response, release outside the original node and repeated successive drags. Record event-to-preview and finalization behavior; do not claim a universal latency target from one machine or a controller unit test.

## 2. Configuration matches the current engine

Provide one current configuration path for direction, layout strategy, routing, shape/size and supported port settings. Root and regional constraints retain their existing inheritance/auto/allowed semantics. Missing, explicit defaults and engine choices remain distinguishable. A shape on a container and its default child shape are separate concepts; see the shared geometry rules rather than inheriting a frame shape accidentally.

- Read authored and effective values with provenance; identify selected root, container or leaf scope explicitly. Engine-computed strategy choices are output, not authored fixed settings.
- Apply configuration through the versioned engine/session path in every execution mode. Do not update the selector optimistically and leave the drawing at the old configuration without indicating rejection.
- Validate and display the reason for unsupported combinations. Valid layout choices must actually reach the engine; no stale frontend list, direction assumption or legacy preset translation may silently replace them.
- Expose currently supported rect/square/circle shapes and an end-to-end example. Merely painting circles already present in data does not satisfy usable shape selection. Future shapes remain capability-gated.
- Preserve explicit regional routing and other overrides across preset application, reset, imports and edits unless the user action explicitly changes them. Explain the scope of a preset rather than silently flattening regional choices.
- Keep browser/native/backend request semantics aligned; share adapters/resolvers rather than duplicating policy per control.

Add integration cases that apply a layout and shape and assert the accepted engine configuration plus resulting scene, including inherited and overridden container values. Mark unsupported controls as such instead of presenting them as working configuration.

## 3. Model catalogue

Make the additional generated example models available in the playground. Use a catalogue/manifest that the build and UI both consume, with stable ID, title, node/edge counts and source/availability. Include generated larger graphs and mixed/nested/shape fixtures intentionally selected for interactive testing. Avoid a second hardcoded menu list or full graph loads just to populate the selector.

Distinguish source files, generated served artifacts and cached browser builds when diagnosing a missing model. A build must update the catalogue and referenced files together; stale/missing entries produce an actionable message rather than an empty or silently reduced graph. Model import limits must be visible; truncation is not evidence that the full model works. Changing models cancels/invalidates the old session/gesture safely.

## 4. Interactive feature scope

| Operation | Expected behavior | Engine responsibility |
| --- | --- | --- |
| Move leaf | Immediate preview, current final position/pin policy, undoable | Constrained placement and affected ports/routes/regions |
| Resize leaf | Resize handles, explicit anchor and dimensions, measured-content feedback | Size/shape constraints, minimums, ports and repair |
| Move container | Header/border affordance; child drag does not move parent | Preserve local child geometry when possible; update transform and exterior routes |
| Resize container | Per-axis size policy (content, minimum, fixed), padding/header visible; see [ADR 0010](0010-containment-tree-and-container-sizing.md) | Content/pin/containment checks, bounded internal repair and ancestor propagation |
| Collapse/expand container | Direct control on the visualization, distinct from drag/pan | View projection, preserved logical edge IDs and valid aggregate routes |
| Pin/unpin | Visible status and explicit command | Authored persistent constraints, separate from temporary gesture locks |
| Undo/redo | One gesture is one logical transaction | Compatible versioned restoration/replay, including indirect engine changes |
| Save/open reference | Curated metadata, constraints and geometry retained | Existing reference schema/validation with interactive-session provenance |

A reference is explicit authoring/review output. Do not automatically mark it a golden optimum or erase pending/conflict state on export. Preserve required reference metadata, baseline linkage and geometry according to the curated-reference contract. Imported references use an explicit replay/read-only capability mode when some edits cannot yet be represented; do not silently approximate them.

Initial release retains pin-on-release as the current UI policy and makes pin/unpin accessible. ADR 0006 still allows other explicit release policies. Undo/cancel cannot overwrite unrelated newer accepted changes. Container movement changes coordinates, not semantic parentage; reparenting is a separate future command.

Collapse controls and resize handles have explicit event priority and keyboard-accessible alternatives. The reusable viewer emits intents; the playground composes the engine client and controls. The viewer does not own a hidden independent layout session.

## 5. Port and edge manipulation: staged engine-backed capability

Port movement and manual edge manipulation remain target functionality. They may depend on engine operations not yet implemented. Do not maintain or expand a second permanent routing solver in the browser to provide them.

Distinguish:

- Move/reassign a port: update admissible side/interval/exact anchor, relative order, capacity or pinning under the contour contract.
- Move a route segment/waypoint: author route constraints or a validated manual path associated with the original logical edge.
- Reconnect an edge endpoint: semantic graph mutation, distinct from changing the route geometry.

Define typed engine commands and diagnostics for each supported operation. Preserve fixed routing style and logical direction. Moving a straight segment cannot silently introduce bends. Unsupported edits are explicitly unavailable with a reason; already supported manual/reference data remain importable and inspectable. A cursor preview may translate a handle or show a tentative path, but it cannot claim repaired canonical geometry before the engine validates it.

Engine-side authoring accepts intent or explicit geometry, validates it and returns a complete transaction with affected routes/ports/labels. It need not implement every future route edit before node/container interactions ship. Undo records the author transaction, not every coalesced pointer sample or solver publication.

## 6. Legacy migration and retained integrations

`editor/geometry.ts` is a migration target because it creates persistent geometry outside the shared engine. Browser coordinate conversion, drag overlays, interpolation and hit testing are still legitimate presentation responsibilities under ADR 0008. That ADR does not forbid immediate endpoint preview, nor does all geometry-related TS code need to move to Rust.

Migrate selection, history, reference import/export and supported edit intents into the unified interaction path. Move validation/routing/containment repair into shared engine operations, then remove dead browser solvers and the old curation toggle. Do not retain a curation path that blocks every configuration/optimization action indefinitely. Temporary migration adapters must be explicit and have tests.

Keep the React Flow adapter/example as an optional external-consumer integration. Lack of use by the playground does not make it dead code. It follows the same scene and command contracts and is not required by the default viewer. Comparison pages, revision gates and session classes likewise remain where used; cleanup follows actual dependency removal, not naming or age.

## 7. Delivery order and acceptance

1. **Interaction reliability:** reproduce and repair acquisition/drop with actual browser events, including >500 nodes. Keep current fast preview and coalescing where valid.
2. **Configuration and catalogue:** current layouts/shapes/scope controls work end to end; generated examples are selectable and accurately counted.
3. **Unified node/container editing:** move/resize, direct collapse/expand, pin/unpin and history through one engine/session path. Preserve reference workflows while retiring the separate mode.
4. **Engine-backed port/route authoring:** add commands and migrate existing manual intent incrementally; remove superseded browser geometric repair only after equivalents or explicit supported restrictions exist.

Acceptance is a coherent consumer workflow: load a larger generated model, select and move a node reliably, release/cancel, change its size/shape, move and resize a container, collapse/expand it directly, change a supported strategy, undo and save/reopen a reference. Background computation and backend responses cannot overwrite newer interactions. Unsupported operations communicate why and leave the session usable.

Real-browser tests cover pointer capture, publication during drag, SVG updates, nested hit priority, cancel/error recovery and interactions in all available transport modes. Existing controller and markup tests remain valuable but do not replace those checks. Screenshot/visual review covers labels, ports and route geometry; shape presence in markup alone is insufficient.

This ADR records scope and priorities. It does not claim these defects are fixed, assign new solver quality weights, or infer large-graph scalability from small-model fluidity. Current user observations are the reproduction starting point; measured findings and implementation completion are recorded separately.

## Status

### Step 1: interaction reliability (implemented)

Measured in headless Chrome 155 (macOS, 1400×900) with real mouse input, 20 moves of 179 px in total:

- **Root cause of the stalled gesture and the lost drop:** the viewport's pan handler (`packages/viewer/navigation.js`) called `setPointerCapture` on the viewport after any move over 4 px, even when the press started on a node. The capture moved from the drawing to the viewport, so the drag received only its first move and no release. Before the change, 3 of 3 runs stopped after one step on the 404-node zoomed view and 3 of 3 on the 40-node default view.
- **Fix:** a press that an element has claimed (`defaultPrevented`) no longer starts a pan (`navigation.js`), and the node drag marks its press as claimed once the gesture starts (`app.js`, `startDrag`). The check `apps/playground/browser-checks/drag-capture.mjs` passes on the repository (20/20 moves on the drawing, 179/179 px, no drift after release) and fails on the previous `app.js` (1/20, 9 px).
- **Not the cause:** a full SVG rebuild per move does not release the capture. Measured on the 404-node view before the fix: a synchronous `sceneOf` at press time took 14–29 ms; the first move to the rebuilt SVG took 24–127 ms. These are open performance items, not the cause of the stall.
- **Fit makes nodes unusable:** after Fit, the median node height on the 404-node view was 3.5 px. Zooming to the maximum (2) gives a median of 40 px.
- **Not yet measured:** event-to-finalization latency. A before/after sweep with the same machine and revision is still required before any performance claim.

### Limits found while measuring

- The example import in the playground caps views at 150 nodes (`EXAMPLE_LIMIT` in `app.js`). The runtime accepts at most 500 (`crates/runtime/src/lib.rs`). A model above 500 nodes cannot be shown completely in the demo; the measurements used a 500-node copy outside the repository.
  *Amended 2026-10-27: both limits are gone. `EXAMPLE_LIMIT` is 10000 (`app.js`) and `MAX_VIEW_LIMIT` is 10000 (`crates/runtime/src/lib.rs`). The demo model `layerweave-code` (583 nodes, 913 edges, generated by `benchmarks/generate.py`) is imported completely. The organic placement is bounded to 300 children per area (`organic.rs`). The pan fix of step 1 is in place (`navigation.js`).*
- A backend `/api/op` request returned 504 twice during the 500-node import. The operation was not identified.
- The playground status line showed no `Move:` counters during any measured gesture, although the preview description claims them there. The cause is not investigated.

### Decisions

- **Curation editor (`apps/playground/editor/`, 1,779 lines of TypeScript without tests):** manual route editing is needed. The editor is migrated to engine operations in vertical slices (section 6), and `geometry.ts` is removed only after engine equivalents or explicit restrictions exist.
- **Shapes:** arbitrary shapes are supported later. Outlines are scene data (see [ADR 0008](0008-engine-client-viewer-boundaries.md)).
- **Container size policy:** per axis, content (`auto`), minimum or fixed, with minimum as the default. Decided in [ADR 0010](0010-containment-tree-and-container-sizing.md), which also defines the containment tree.
- **React Flow:** kept as an optional example and adapter (see [ADR 0008](0008-engine-client-viewer-boundaries.md)).
- **Toolchain:** pnpm on Node 24 for now; Bun is deferred ([ADR 0008](0008-engine-client-viewer-boundaries.md)).

## Status of the unification (2026-10-27)

Done: reliable gestures, a history of decisions, move of a selection, resize, pins, collapse, shape, direction, strategy, routing, placing a
port and a label, saving and opening a reference from the direct view, local edits fast enough to follow a drag on a model of 600 nodes
(0.27 s per step, not smooth). Kept as a legacy draft, and listed in the page as not available in the direct view: moving a route segment,
doglegs, route locks, size pins. See [ADR 0015](0015-user-decisions-as-engine-values.md) for the decisions and what is open.
