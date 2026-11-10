# Curated Diagram References and Visual Editing

Status: proposed implementation specification, 2026-10-21. This document specifies new capabilities; it does not claim that the editor or reference pipeline already exists.

Related: [product requirements](specification.md), [algorithm improvements](algorithm-improvements.md), [scene/viewer boundary](scene-viewers.md), [config sets](config-sets.md).

## 1. Purpose

Turn an existing computed diagram into an editable, annotated, reusable reference. A curator can correct node positions, ports, routes and labels, explain the changes, and save a structured reference document plus a visual SVG export. An optional self-contained SVG can also embed the reference document for single-file exchange.

A curated reference is evidence of a feasible or preferred arrangement, not necessarily a unique optimum. The system must support multiple accepted references for the same input and preserve disagreements rather than inventing one universal best drawing.

Primary uses:

- diagnose concrete algorithm failures;
- create geometric regression fixtures and achievable quality targets;
- compare candidate layouts against human preferences;
- collect reproducible pairwise judgments;
- eventually train and evaluate a candidate-ranking model.

The first release is a constrained orthogonal diagram editor built on the existing viewer and engine. It is not a general-purpose SVG editor, a graph authoring system, or an ML layout generator.

## 2. Core design decision: JSON document, SVG presentation

The canonical editable and evaluable artifact is `*.lwref.json`, containing the versioned document defined in section 6. SVG is a generated visual representation, not the editor's native data model.

Supported outputs:

- **Save reference:** `.lwref.json`, the authoritative portable document. This is the default for editing, corpus storage, validation, version control and batch evaluation.
- **Export SVG:** ordinary standalone vector geometry, suitable for viewing and publication; it does not promise editability in Arqlet.
- **Export portable reference SVG:** `.lwref.svg`, containing both generated vector geometry and the complete JSON document in `<metadata>`. This is optional single-file exchange; reopening extracts the JSON into the same editor model.

An SVG companion is optional for JSON references. A corpus manifest identifies one authoritative artifact per reference revision; JSON and embedded-SVG encodings of the same payload are alternate representations, not independent training examples. When both are distributed, compare their canonical document hashes and report disagreement rather than guessing which is newer.

### Why this combination

| Format | Role | Tradeoff |
|---|---|---|
| Versioned JSON | Native reference and evaluation input | Explicit semantics, straightforward validation and tooling; requires a viewer for visual inspection |
| SVG | Vector presentation and optional portable envelope | Readable in common viewers; arbitrary path edits do not preserve graph semantics |
| ZIP-based package | Deferred option for assets or many revisions | Useful if embedded fonts/images become necessary; extra packaging and import complexity is unnecessary for v1 |
| PNG/PDF | Optional preview/publication derivatives | Useful for review, unsuitable as authoritative editable geometry |
| GraphML/DOT | Possible topology interchange adapters | Would still need a project-specific contract for curation history, constraints, geometry and judgments |

No new binary format or compressed bundle is required initially. JSON contains baseline and curated geometry and may be larger than a display-only SVG; manage corpus size through external immutable artifact storage and derived caches before inventing a second semantic format. Large canonical graphs need not be embedded when the declared projected input and boundary context fully define the evaluation case; missing context must never be silently omitted.

The reference document is authoritative for Arqlet semantics and editable geometry. SVG paths are its presentation, generated from the same validated scene. A screenshot alone, or arbitrary SVG without the document, is not a usable geometric reference.

There are four distinct layers:

1. **Input:** graph semantics, visible projection, dimensions, initial constraints and resolved configuration.
2. **Baseline:** the immutable computed snapshot from which curation started.
3. **Curated scene:** explicit final geometry and curator-introduced constraints.
4. **Evidence:** annotations, comparisons, provenance, validation and review status.

Do not modify canonical edge direction, node IDs or containment to make a layout look better. Any actual topology change starts a new input/reference case.

## 3. User workflow

1. Open a computed snapshot in the existing SVG demo or comparison viewer.
2. Choose **Create curated reference**. Clone the input and baseline into a separate document; preserve the original.
3. Choose the whole visible projection or a local focus area. The default preserves the full projection and marks the focus; it does not silently delete surrounding obstacles or edges.
4. Edit geometry, inspect conflicts and add explanations. Undo/redo is available throughout.
5. Compare baseline and curated result with identical measurement settings and optional matched scale.
6. Validate, then save a draft or explicitly accept it as a reference.
7. Save `.lwref.json`; optionally export plain SVG or portable reference SVG. Reopening either reference encoding restores geometry, annotations and provenance.
8. Add accepted artifacts to a versioned corpus manifest for evaluation.

Entering manual curation mode disables automatic refinement. In this default mode, engine-assisted repair is an explicit action that previews a candidate and only replaces the curated scene after acceptance within the editor. A future opt-in assisted mode may automatically accept compatible local repairs under the policy in section 14. A background response must never overwrite newer manual edits.

## 4. Editing capabilities

### 4.1 Navigation and selection

Retain panning, pointer-centered zoom and fit-to-view. Add multi-selection, keyboard clearing, selection cycling for overlapping objects and a detail inspector. Make edge hit areas screen-space consistent across zoom levels; separate drag gestures from clicks.

Objects are selected by stable semantic IDs, not DOM element order. Highlight selected routes, endpoints, ports and annotated conflicts. Preserve selection across harmless redraws; discard missing selections explicitly after a projection change.

### 4.2 Nodes and containers

- Move one node or a selected set; support free placement, optional grid snapping and axis-constrained movement.
- Resize nodes and containers while respecting minimum measured content dimensions; distinguish an intentional size change from a position-only correction.
- Pin position and/or size independently. A pin is a curation constraint, not a graph property.
- Moving a container translates its descendants and internally contained geometry exactly once. Relative child coordinates remain unchanged; edges crossing its boundary are affected routes.
- Resizing a container does not scale child nodes or stretch routes. Offer explicit boundary-only resize or local re-layout as separate actions.
- Do not allow implicit reparenting. Crossing a container boundary during dragging produces a containment diagnostic.

A resize changes the layout input for future constrained reproduction. Record this distinction so a candidate is not judged against an easier or different geometry input without disclosure.

### 4.3 Ports

Allow selecting automatic ports, choosing an allowed side, moving the offset along that side and changing order within a side. Support minimum spacing, corner exclusion and label reservation.

Explicit input port constraints are locked by default. Unlocking requires an explicit **constraint experiment** action and creates a derived input variant. It cannot remain a reference for the original unchanged constraints.

An edited automatic port becomes a curator constraint in the curated scene. It does not retroactively change the baseline input. Whether that constraint is passed to the engine depends on the evaluation mode in section 9.

A hidden port marker still has a physical attachment point. Incoming/outgoing semantics do not imply a fixed screen side. Shared physical ports and bundle membership must remain distinguishable from coincident coordinates.

### 4.4 Edges

For orthogonal routes, support:

- moving a horizontal or vertical segment along its perpendicular axis;
- adding/removing a dogleg;
- moving a bend with a preview of the resulting segments;
- reconnecting an automatic endpoint to an allowed port without changing its semantic node;
- locking a route or selected segments;
- explicit local re-routing of selected edges while respecting locked geometry.

Maintain endpoint continuity and orthogonality. Normalize redundant points on commit. During dragging an invalid draft may be visible, but final validation must identify node intersections, disconnected endpoints and zero-length artifacts.

Moving a node updates its ports and marks incident routes plus routes affected by the old/new obstacle envelope as dirty. Provide a simple provisional connector for interactive feedback; it is not silently accepted as a valid route. Offer local repair on release, with undo of the complete transaction.

Rounded orthogonal rendering retains the underlying polyline as its geometric route. Corner radius is separate appearance data and must be clamped or diagnosed when the curve would intersect obstacles. Arbitrary Bézier editing is deferred; future schemas must distinguish route geometry types explicitly.

### 4.5 Labels, comments and bundles

Move label boxes independently with an explicit anchor and optional offset. Preserve measured dimensions and distinguish geometry-affecting label visibility from styling-only changes.

Add curation notes in a separate annotation layer. Notes do not become graph nodes or routing obstacles by default. A domain comment box already present in the input retains its configured obstacle behavior.

Existing complete bundles expose leader and member IDs; moving the shared geometry affects all members. The editor must not infer bundling from overlapping paths. Shared-trunk editing is deferred until the engine/scene support explicit shared segments, members and branch points. Do not encode a tree of branches using only one branch-point coordinate.

## 5. Annotation and preference model

Each annotation records:

- stable annotation ID;
- target node, port, edge, route segment, region or object pair;
- category: overlap, node intersection, clearance, port side/order, channel space, crossing, bend, label, hierarchy, stability, readability or other;
- explanatory text and optional severity;
- whether it describes a baseline defect, a correction rationale or an unresolved issue;
- optional desired relation such as `port below node`, `A before B`, `route outside region`.

Segment anchors require stable editor segment IDs or a geometric anchor with explicit resolution rules; a raw array index must not silently attach to a different segment after simplification. Unresolvable annotations remain visible as detached annotations for review.

Pairwise judgments use candidate artifact hashes and a declared task: `A preferred`, `B preferred`, `equivalent`, `both unacceptable`, or `uncertain`. Store the reason and relevant region. Optional curator identity is user-supplied; never infer it from the machine account.

Support tasks such as following a dependency path, finding fan-in, identifying a cycle and understanding container membership. An aesthetic preference and task performance are different evidence types; do not label a preference as a measured usability improvement.

## 6. Document and serialization contract

The following is a logical schema, not a finalized public API:

| Field | Required content |
|---|---|
| `format`, `schemaVersion` | `layerweave-curated-reference`, version 1 |
| `referenceId`, `revision`, `status` | Stable document identity, monotonic edit revision, draft/accepted/retired |
| `input` | Canonical projected graph, view/collapse state, boundary context, dimensions, original hard constraints |
| `inputHash`, `familyId` | Canonical input identity and corpus split grouping |
| `baseline` | Original snapshot, engine/schema versions, resolved layout/routing config, preset identity/version, run policy |
| `curated` | Explicit scene geometry, manual constraints, supported geometry types |
| `appearance` | Rendering profile, font declarations, optional viewport; no hidden influence on layout |
| `annotations`, `judgments` | Structured rationale and preference evidence |
| `provenance` | Creation/update timestamps, optional author, source artifact hash, parent reference hash |
| `validation` | Validator/metric versions, measurement configuration, results, unresolved conflicts and review decision |
| `integrity` | Hashes for canonical payload sections and generated SVG geometry |

For curated geometry v1, use absolute world coordinates for nodes, ports, routes and labels, with explicit parent IDs. The existing snapshot remains in its existing coordinate convention. Conversion occurs once at the adapter boundary. Viewport pan/zoom must never alter saved world coordinates. Port records store node/side/offset and their derived absolute position; validation rejects disagreement.

Use canonical JSON serialization with a documented key ordering and number representation before hashing. Reject non-finite values. Derived measurements, timestamps and viewport changes do not change `inputHash`. Finalize the exact hash implementation with cross-runtime fixtures before treating hashes as durable corpus identities.

Do not hash the final SVG file into its own embedded payload. Define separate payload and presentation hashes excluding the integrity fields; compute the final whole-file hash externally in the corpus manifest.

### SVG presentation

- Export actual vector shapes and paths with `viewBox` in world coordinates.
- Include stable `data-lw-*` object references and separate diagram/annotation groups.
- Store JSON as escaped text inside `<metadata id="layerweave-reference">`; do not embed executable JavaScript or rely on fragile raw CDATA insertion.
- Use self-contained styling and local markers. No network fonts or external images are required in v1. Store text measurements and font requirements; rendering text can differ across machines if the exact font is unavailable.
- Default export is the clean diagram. An explicit review overlay can include annotations and conflict highlights without changing geometry.
- Provide a separate **Plain SVG** export. State clearly that removing metadata makes it a presentation export rather than an editable reference.

## 7. Import and round-trip behavior

For `.lwref.json`, parse and validate the document directly. For portable reference SVG, extract the metadata document first. Both paths use the same schema validation and editor initialization, with no SVG-to-layout inference.

Parse SVG as data, validate the supported reference schema, rebuild the scene from the embedded document, and render through Arqlet. Do not insert imported SVG markup directly into the application DOM or execute scripts, event handlers, external references or foreign HTML. Enforce file-size, nesting and object-count limits with clear errors.

Recognize three cases:

1. **Valid Arqlet reference:** reopen the embedded document and restore editing state.
2. **Metadata present but SVG presentation changed externally:** identify a presentation mismatch and let the user regenerate SVG from metadata. Do not pretend to have imported external geometric edits.
3. **Metadata missing, invalid or incompatible:** permit an explicitly separate static preview if safely supported, but do not admit the file to the reference corpus.

General Inkscape/Illustrator round-trip editing is outside v1. Supporting it later requires a restricted shape/transform importer with identity and constraint reconciliation. Arbitrary external path edits cannot be recovered from metadata automatically.

On save/reopen, canonical graph and geometry must be equivalent within the declared numeric tolerance. Derived metric differences must be explained by a metric-version change, not serialization drift. Imported accepted validation is historical evidence; current evaluation validates again.

## 8. Editing architecture and transactions

Add an editor document/controller above the renderer-neutral scene layer. Renderers consume its derived scene; they do not become owners of graph semantics or optimization state.

Each operation is a reversible command: move/resize nodes, move ports, edit routes, move labels, set a pin or add an annotation. A completed gesture is one undo step. Undo restores all affected geometry and derived state atomically; redo is deterministic.

An engine-assisted operation carries document revision, baseline input hash, view identity and constraint hash. Reject results if any of these changed. Requests operate on a copy and return a candidate. Manual mode requires inspection/acceptance; assisted mode follows the explicit transaction policy in section 14. Curation state is not merged into normal automatic refinement implicitly.

Extract reusable geometry validation so explicit curated scenes can be measured without first running layout. The editor and benchmark runner must use the same versioned validator and metric definitions. Do not compute a second, slightly different crossing or clearance metric in the browser UI.

Structural corruption is rejected. Geometrically invalid work can be saved as a draft or an explicitly tagged negative example. Only validated references enter the accepted-positive set. Known unavoidable conflicts require a specific documented waiver and a separate corpus classification; a waiver must not turn a failing metric into zero.

## 9. Using curated references in evaluation

### 9.1 Three evaluation modes

**Unconstrained quality comparison:** run the algorithm on the original input/constraints. Manual placements are targets/evidence, not extra engine inputs. Compare validity and quality vectors, not exact coordinates. A valid reference provides an achievable example, not proof of optimality.

**Constrained reproduction:** pass explicitly selected curator pins, port choices or route constraints to the algorithm. Check that these are honored and the remaining diagram is laid out validly. Report the constraint variant as a different experiment/input hash.

**Incremental repair:** start from a curated layout, apply a recorded edit and measure validity, quality, movement outside the affected region, time to first usable result and refinement cost. Keep initial geometry and mutation sequence fixed across candidates.

Do not mix scores from these modes into one leaderboard.

### 9.2 Comparison rules

- Evaluate reference and candidate with the same metric version, thresholds and dimensions.
- Require matching visible graph structure, boundary context and declared constraints for direct comparisons.
- Report hard violations first, then individual soft metrics and deltas. Do not reduce all results to one weighted score by default.
- A candidate can outperform a reference. Mark it for human review rather than fail because its coordinates differ.
- Multiple accepted references form a set of examples/preferences. Do not assemble an impossible target by taking the independently best value of every metric from different references.
- Pixel/SVG golden tests remain renderer regression tests. Algorithm acceptance uses geometry, constraints and task-specific preferences.
- Translation of the entire scene must not affect quality. Rotation/reflection are not generally equivalent when ports, direction or hierarchy are constrained. Node/edge IDs are never freely permuted for matching.
- Changes in node dimensions, collapse state or obstacle roles require a distinct comparison context.

### 9.3 Corpus and runner extensions

Store artifacts under `benchmarks/references/<case-id>/` with an index manifest containing artifact hash, input hash, family ID, reference status, tags, evaluation mode and split. Keep the authoritative reference artifact immutable once accepted; corrections create a new revision linked to the parent. Generated SVG companions can be regenerated from the same document with their renderer version recorded.

Extend the runner with explicit operations to validate references, extract/cache payloads, remeasure them and compare candidate runs. Cache keys include artifact hash, validator version and measurement configuration. Preserve original measurements and append new-version results rather than overwrite history.

Reports include per-case validity, metric deltas, relevant annotations, configuration/run identities, runtime/memory from actual runs and links to side-by-side views. Never infer computation time from a curated SVG.

Extend the comparison UI to load a reference and a candidate, synchronize scale optionally, highlight changed nodes/ports/segments and inspect annotations. Add blind A/B mode with randomized sides and explicit preference capture. Geometry correspondence uses stable IDs; topology differences are shown before judgment.

## 10. Future ML use

The initial artifact format should collect useful data without requiring ML infrastructure.

First candidate: a ranker that selects among already valid layout candidates. Training examples consist of the same input, two candidate geometries, measurement settings, task context and a human judgment. Retain ties and uncertainty. Do not automatically convert all curated references into preferred labels against every generated candidate.

Split by source graph family/repository, not by screenshot or layout variant. Crops, perturbations and repeated layouts of the same graph stay in the same split. Keep a held-out set untouched by manual tuning and parameter search. Track label provenance and disagreement.

A later model may recommend local actions such as port reordering or channel expansion. Its suggestions remain subject to deterministic geometry validation. ML must not relax explicit ports, hierarchy constraints or stale-result protection.

A few dozen curated cases are useful diagnostics, not evidence that a generalizable model can be trained. Introduce training only after label consistency, dataset coverage and baseline heuristics have been measured.

## 11. Delivery plan

| Phase | Scope | Exit criteria |
|---|---|---|
| C0: artifact contract | Explicit curated scene, validator adapter, JSON save/load, SVG export and optional embedded import, corpus manifest | Unedited snapshot round-trips through both reference encodings; mismatches and unsafe inputs rejected |
| C1: useful local editor | Node movement/resize, port edits, orthogonal segment edits, undo/redo, pins, draft validation | Real routing defects can be corrected without changing graph semantics |
| C2: evidence and comparison | Annotations, baseline/curated diff, accept/revise workflow, blind A/B judgments | A curator can explain and save why a result is better or unacceptable |
| C3: eval integration | Three evaluation modes, versioned remeasurement, reports and corpus checks | CI evaluates accepted cases without browser automation |
| C4: extensions | Container-intensive editing, richer bundles, external SVG subset importer, ML experiments | Separate designs and evidence before enabling each feature |

C1 must handle existing containers safely even though advanced nested editing is deferred: locked containment, translation semantics and boundary-edge diagnostics cannot wait until C4. The first release may limit editable regions to one container and report that limit explicitly.

Initial corpus recommendation: 15–25 small diagnostic cases plus 5–10 real local problem areas. Select these by failure diversity, not to make the current algorithm look good. Preserve full boundary context for real excerpts.

## 12. Required acceptance tests

1. Snapshot → `.lwref.json` → reopened document preserves graph, dimensions, ports, geometry and constraints. Portable reference SVG extracts an equivalent canonical document. Alternate encodings are not counted twice in the corpus.
2. SVG text containing `<`, `&`, quotes or non-ASCII labels round-trips correctly; scripts/external resources are never executed.
3. External presentation edits are detected; unsupported schemas and corrupt metadata cannot silently become accepted references.
4. Move/resize/port/route edits followed by undo restore the complete prior scene; redo reproduces it.
5. Container translation does not double-translate descendants or internal routes; boundary edges become dirty and are repaired or diagnosed.
6. Port edits preserve logical endpoints; explicit input constraints require a derived experiment to change.
7. Edge edits remain orthogonal and attached; stale engine repair results are discarded after any intervening edit/reset.
8. Actual and near-overlapping segments, common allowed stubs, node intersections, labels and foreign-container crossings use the same metrics as batch evaluation.
9. Drafts can preserve known problems; positive acceptance cannot silently bypass hard failures.
10. Geometry metrics are invariant under whole-scene translation and redundant forward-collinear points.
11. Unconstrained evaluation does not receive the reference's manual positions; constrained evaluation receives only explicitly selected constraints.
12. Input mismatches, changed measurements and differing view contexts are surfaced before comparison.
13. Multiple valid references are supported; a genuinely better candidate is not rejected for differing coordinates.
14. Negative examples, preferences and metric thresholds are kept distinct; train/test family isolation is enforced by the manifest validator.
15. Golden SVG rendering tests, geometric tests and measured performance runs remain separate reports.

## 13. First implementation slice

Deliver C0 and a narrow C1 as one usable milestone, optionally in two sequential reviewable changes: create a draft from a snapshot, move nodes, move an automatic port, shift an orthogonal segment, undo/redo, annotate the reason, validate, save/reopen `.lwref.json`, and generate a matching SVG. Add metadata embedding/extraction to support single-file `.lwref.svg` exchange without creating another document model. Keep all existing explicit constraints locked. Use three diagnostic fixtures and one real excerpt to verify the workflow.

Then connect the artifact to the existing side-by-side viewer and runner before expanding editor tools. This makes every new editing feature produce usable evaluation evidence immediately.


## 14. Implementation architecture: TypeScript editor, Rust geometry services

### 14.1 Decision and scope

Develop the browser editor in **TypeScript**, extending the existing visualization. Keep layout, routing, authoritative geometric validation, metrics and the portable reference contract in **Rust**, exposed through WASM and native/backend adapters.

Do not build a second Rust GUI, replace the existing visualization framework, or make pointer interactions depend on a backend round trip. The present JavaScript viewer can remain in place initially: introduce typed editor/controller modules with a small build integration, then migrate adjacent modules as they are changed. A wholesale JavaScript-to-TypeScript rewrite is not a prerequisite.

The existing SVG demo is the first editable host. Its normal layout view gains an editing mode, rather than launching an unrelated reference-authoring application. The existing comparison view consumes saved documents and candidates. The React Flow example may reuse the editor controller and commands later; it must not become a second implementation of editing semantics.

### 14.2 Ownership and dependencies

| Responsibility | Owner | Notes |
|---|---|---|
| Pointer/keyboard gestures, hit areas, selection, focus, zoom/pan | TypeScript browser host | Immediate interaction; independent of engine scheduling |
| Draft document, command history, undo/redo, edit transactions | TypeScript editor controller | One authoritative in-memory draft; renderer is a projection |
| Provisional node/port/segment movement, snapping, drag feedback | TypeScript geometry helpers | Simple direct manipulation; no competing quality metric or full router |
| Formal document schema, format migration, validation, canonical serialization/hashes | Rust reference service | Same behavior in WASM and native corpus tools |
| Geometric conflict classification and quality metrics | Shared Rust geometry/scene services | Browser only presents returned diagnostics |
| Constraint-aware repair, routing and layout search | Rust engine/runtime | Returns a candidate, never mutates browser state directly |
| File picker/download, XML envelope parsing, network access | Host adapters | No DOM, filesystem or network dependencies in core crates |
| SVG rendering/export | Existing renderer plus shared export contract | No duplicate layout computation; exported scene must match document geometry |

A small `crates/reference` is the preferred location for the portable document model and reference validation orchestration. It may depend on model/scene/shared geometry, but not on browser hosts or runtime transport. Lower-level geometry cannot depend back on the reference document. The runtime dispatch adapter composes reference and layout services.

Types crossing the boundary must have a shared source or schema compatibility tests. Rust serialization is authoritative for durable payloads. TypeScript types do not substitute for runtime validation of imported files. Prefer generating transport types/schema if this fits the existing build; otherwise maintain a narrow typed adapter with round-trip fixtures.

Keep one draft in TypeScript and one immutable input copy per engine request. Do not maintain continuously synchronized, independently mutable Rust and TypeScript editor documents. Pointer movement does not serialize the entire graph into WASM on every event.

### 14.3 Reference serialization and validation

The browser sends the current draft to the Rust reference service for validation and canonical serialization when saving, accepting, hashing or requesting engine repair. Lightweight UI checks may give immediate feedback, but only authoritative validation can mark a document valid or accepted.

SHA-256 is the intended durable identity algorithm. Select a maintained WASM-compatible implementation during implementation; record dependency license/version. Define canonical number serialization in one implementation rather than assuming `JSON.stringify` and Rust emit identical floating-point text. Native and WASM must produce identical canonical bytes for shared fixtures. Node tests consume those bytes or verify a fully specified implementation; a second serializer is not mandatory.

Plain SVG export can reuse the existing renderer/exporter if it preserves stable identities, standalone styling and escaping. A Rust SVG writer is optional for headless export; it is not a reason to move the editor into Rust. Both paths must be checked against the same scene/document contract. XML envelope import belongs in a safe host adapter, using an actual parser and the restrictions in section 7.

Existing JavaScript crossing helpers may remain solely for visual hit testing if clearly named as such. They must not supply authoritative conflict labels, quality values, acceptance decisions or training labels. Replace those uses with Rust diagnostics, including per-object conflict references needed by the inspector.

### 14.4 Editing modes

Expose three distinct modes, with visible state:

1. **View:** display computed Arqlet geometry; ordinary graph exploration and existing layout controls.
2. **Manual curation:** direct edits, immediate previews, explicit validation/repair, no unsolicited layout movement. This is the initial editor release.
3. **Assisted editing (later):** user movement is an explicit local intent; the engine may adjust the surrounding layout under a configured scope and stability policy.

All modes use the same graph identity and scene renderer. Entering curation creates a draft from the displayed snapshot. It does not modify the source snapshot or automatically convert the current draft back into an ordinary engine snapshot.

Local editing remains browser-local in browser-only, split and backend-only execution configurations. These configurations choose the provider for engine services, not the location of mouse handling or undo history. Backend-only mode sends validation/repair requests to native services; it does not send every pointer move. On provider failure, retain a local draft and clearly mark validation as pending; never label an unvalidated draft accepted.

### 14.5 Manual editing transaction

A pointer gesture follows this lifecycle:

```text
begin gesture -> capture pre-edit state and document revision
pointer moves -> update local preview and dirty-object set
release -> commit one reversible manual command
         -> request authoritative validation (asynchronous)
         -> show conflicts or offer explicit repair
```

The dirty set includes moved objects, their ports and labels, descendant geometry affected by container translation, boundary edges and routes intersecting the swept obstacle area. Changes to topology or containment remain outside initial manual editing.

Provisional connectors are visually distinguishable from final routes. A user can save a draft while repair is pending; the saved document records unresolved state. Acceptance/export as a positive corpus reference requires current validation of that exact document revision.

### 14.6 Assisted editing: intent and local recomputation

The future assisted mode extends the same command pipeline, not a new editor:

- During dragging, keep direct local feedback responsive. Initial implementation starts repair after release; continuous drag-time repair is a later optimization.
- Treat the object being moved as temporarily fixed at the requested position for the current repair transaction. This is not automatically a permanent pin. Existing explicit constraints and persistent user locks remain hard constraints.
- Select the smallest affected region, initially incident edges and neighboring obstacles. Keep unrelated geometry fixed or under a strict motion limit.
- Attempt route-only repair first. If infeasible, propose local node/port movement within allowed constraints, then container-size adjustment. Scope expansion must be explicit in the policy and report.
- Do not silently move objects outside the authorized region or change a local diagram's layout logic. If a valid repair needs broader changes, retain the draft and offer an explicit expansion action.
- Apply a compatible complete result atomically: boxes, ports, routes, labels and diagnostics together. Never display a mixture of old boxes and new paths as a validated result.
- Use a stable-reference objective to penalize movement of untouched nodes and route churn. The user's requested change is intentional and must not be penalized back toward its previous position as if it were accidental drift.

Automatic acceptance is allowed only because the user explicitly enabled assisted mode, and only when the result respects the requested position/constraints, stays inside the authorized scope, passes hard validation and satisfies the configured quality/stability acceptance policy. Otherwise display it as a proposal or report why repair failed. Intermediate search states are never accepted merely because a budget expired.

### 14.7 Request identity, cancellation and handoff

Every repair request must include or bind to:

- document/reference identity and edit revision;
- gesture/transaction ID and request generation;
- original input identity, current geometry identity, view/collapse identity;
- resolved configuration and constraint identities;
- affected region and user intent;
- work/time budget and result acceptance policy.

These are logical requirements, not a demand to rehash the full graph on each pointer event. Use revisioned immutable inputs and compute durable hashes at request/save boundaries where needed.

Any later edit, undo, redo, reset, import, mode/configuration change or projection change invalidates incompatible requests. Discard stale results even if graph revision alone still matches. Cancellation is best-effort resource control; identity checks are the correctness guarantee.

Start with one-shot local repair. Later progression can use a fast browser repair followed by a larger backend refinement. The backend must start from an explicitly identified accepted state and return a candidate for that state; it cannot overwrite subsequent gestures. Default to discarding incompatible results rather than attempting an implicit merge. Engine capabilities and format versions must be checked across hosts.

The synchronous WASM call cannot be interrupted by an ordinary message queued to the same worker. Budgeting must use cooperative checks with a host clock/work budget, or worker termination with the last accepted draft preserved. Promise cancellation alone does not cancel computation.

### 14.8 Undo/redo and provenance with automatic changes

A drag plus its accepted local repair is one compound transaction. Undo restores the complete pre-gesture state, including neighbor movements and routes; redo restores the exact accepted result rather than rerunning a nondeterministic time-budgeted search.

A repair can join the gesture's undo entry only while that transaction is still current and no later user command has occurred. Otherwise reject it or present it as a separate explicit proposal. A later accepted backend refinement is a separate, named undoable transaction. Undo/redo invalidates pending repair requests.

Record the origin of changes: manual, engine-assisted, imported or mixed. Store engine/configuration identity and, when applicable, the accepted repair operation. An engine-assisted result becomes a human-accepted reference only after an explicit reference review action; enabling assisted editing is not blanket labeling of all intermediate outputs as preferred examples.

When saving for evaluation, preserve the original baseline, manual intent and final accepted geometry separately. This enables both ordinary quality comparison and incremental-repair replay without leaking manual target geometry into unconstrained algorithm inputs.

### 14.9 Initial modules and implementation sequence

Suggested boundaries, with names provisional:

- `editor/document.ts`: typed draft state and baseline/curated separation;
- `editor/commands.ts`: reversible geometry commands and compound transactions;
- `editor/interaction.ts`: gesture state, selection and preview;
- `editor/engine-adapter.ts`: WASM/backend service calls, capability checks and stale-result guards;
- `editor/reference-io.ts`: host file handling, validated serialization/import and downloads;
- Rust `reference`: document contract and authoritative reference operations;
- existing Rust runtime/layout/routing: explicit repair jobs and constraints.

Do not require all modules or transport extensions before the first useful edit. Deliver:

1. Reference contract and save/load round-trip, using real existing snapshots.
2. Existing SVG view plus manual edit controller: node drag, automatic-port adjustment, orthogonal segment shift, undo/redo and draft save.
3. Validation and explicit local repair, respecting locked geometry and revisions.
4. Reference acceptance, annotations, comparisons and batch evaluation.
5. Opt-in assisted mode on release, with bounded local repair and compound undo.
6. Only then progressive browser/backend refinement and optional drag-time previews.

C0 may be reviewed separately from C1, but the first user-facing milestone is editing and saving a real existing visualization. Do not stop at a serializer-only milestone and describe the editor as delivered.

### 14.10 Additional acceptance criteria

- Existing layout visualization continues working without entering edit mode.
- Pointer previews work without a round trip, and engine latency does not block navigation or further edits.
- Moving a node in manual mode does not invoke global refinement; all provisional route changes are visibly pending.
- Manual edits persist through redraw, save/load and explicit validation without being recomputed away.
- Backend-only service mode still supports local drag and undo.
- Assisted repair preserves the requested user position and explicit pins; infeasible requests remain diagnosed rather than silently undone.
- A repair result arriving after a new drag, undo or config change is rejected.
- Compound undo removes both the gesture and its associated automatic adjustments; redo restores saved geometry exactly.
- Outside-region movement is either prohibited or explicitly approved and reported.
- A saved incremental fixture can replay the same starting geometry and user intent through native and WASM repair paths.
- Native and WASM serialization/validation agree; browser display helpers do not create independent metric labels.
- Automated intermediate improvements are not silently entered into the corpus as human preference judgments.

## 15. Package structure and build resolution

Status: PR 1 and PR 2 implemented: `packages/viewer`, `packages/engine-client`, pnpm workspace, browser import maps, the demo in `apps/playground` and the React Flow consumer in `examples/react-flow`. `contracts` is implemented (generated TypeScript types, typecheck and drift checks). `editor` is still planned. Introduced before C0 (section 11).

### 15.1 Target layout

```text
Cargo.toml, Cargo.lock           Rust workspace (unchanged)
package.json, pnpm-workspace.yaml, pnpm-lock.yaml
crates/                          Rust core, wasm, runtime (reference crate added in C0)
presets/                         versioned preset definitions (data only)
packages/
  viewer/          implemented, PR 1  scene rendering to SVG, navigation, selection display, shared scene helpers
  engine-client/   implemented, PR 1  WASM/worker access, scene requests, preset sequencing
  contracts/       implemented    ts-rs generated contract types from crates/contracts
  editor/          planned        draft, commands, undo/redo, reference I/O (kept inside the playground until a second host needs it)
apps/
  playground/      implemented, PR 2  existing demo, comparison view and later the editor host
examples/
  react-flow/      implemented, PR 1 consumer, moved here in PR 2
```

Apps depend on packages; packages never import from apps or examples. Presets stay as data in `presets/`. Selecting and applying a preset is host or controller work.

### 15.2 Responsibilities

- **viewer:** scene-to-SVG rendering, navigation, selection highlighting, route display, and shared helpers for scene data (including the React Flow mapping, which uses scene helpers but not the SVG renderer).
- **engine-client:** loads WASM, runs the worker protocol, and sends scene requests. It is shared by the playground and the React Flow example, so it is extracted now rather than later.
- Quality metrics and conflict classification stay in Rust. Viewer code only displays the returned diagnostics.
- Framework: none for now. The controller owns the draft; a view library (e.g. Preact) may be added for the inspector later, as long as it renders controller state and holds no second authoritative state.

### 15.3 Build and browser resolution

`tsc` emits JavaScript but does not resolve workspace imports for the browser. The no-bundler setup needs:

- an import map in each HTML entry point mapping `@arqlet/*` to the package directory under a defined public path;
- relative imports inside packages with explicit `.js` extensions;
- the web server serving `packages/` and `presets/` at those paths, without exposing other repository files;
- worker and WASM URLs resolved relative to the module (`new URL(..., import.meta.url)`), with the WASM file still delivered from `generated/`.

PR 1 proves this with tests: the browser entry points load, the worker starts, the WASM engine answers a scene request, and the React Flow example builds and renders against the same packages. Node tests resolve the same bare specifiers through the pnpm workspace links. If the manual delivery becomes more fragile than a build step, a small bundler is acceptable; that decision is made in PR 1 and recorded here.

### 15.4 PR sequence

1. **PR 1, workspace and viewer/engine packages (done):** introduce pnpm workspace, move viewer and engine-client modules, update import maps, server mapping and the React Flow example. No behavior change. Gate: `build.sh` (including the React Flow build), `smoke.mjs`, `ui-regression.mjs`, all Node tests.
2. **PR 2, playground (done):** move the demo from `examples/browser` to `apps/playground` and React Flow to `examples/react-flow` as mechanical moves, with build, server, CI and documentation paths updated. No logic change.
3. **Contracts (done):** `crates/contracts` exports the TypeScript types of `Snapshot`, `Scene` and `ConfigSet` (ts-rs 12.0.1, MIT) into `packages/contracts`, with a `typecheck` script (TypeScript 7.0.2, Apache-2.0). The `ts` feature of `arqlet-model` and `arqlet-scene` is optional; default builds, including the WASM crate, do not compile ts-rs.
4. **C0 (in progress):** `crates/reference` with the document v1 contract, canonical JSON (rules in `canonical.rs`, format 1), SHA-256 payload and input hashes (`sha2` 0.11.1, MIT/Apache-2.0), and validation: structural rejection with explicit errors, recomputed geometry metrics from `scene::metrics`, hard violations reported. Operations `referenceCreate`, `referenceValidate` and `referenceCanonical` go through the shared dispatcher, so they run in browser, split and backend modes (backend allowlist and native/WASM parity in `smoke.mjs`). Synthetic fixtures in `examples/fixtures/reference/`. SVG encoding (`svg.rs`: visible drawing plus the canonical document embedded as hex in `<metadata id="layerweave-reference">`; import accepts only text that export reproduces byte for byte, so visible edits, tampered payloads, non-canonical JSON and unsafe markup are rejected), operations `referenceSvg` and `referenceFromSvg`, and the corpus manifest (`benchmarks/curated/manifest.json`, generated, with file hashes; planned cases are marked planned). Not yet: the frozen real excerpt (no `exports/` input in this checkout) and the planned diagnostic fixtures.
5. **C1 (partial):** editor in `apps/playground/editor` (TypeScript, compiled to `generated/editor`). Implemented: manual curation mode, node drag inside its container (one undo step per gesture), shift of interior orthogonal route segments, port slides along their side (explicit ports are locked), undo/redo over whole sealed states, engine validation after every change, save and reopen of `.lwref.json`. A moved node or a moved port marks its routes as pending: they are drawn as provisional connectors and the draft is never valid until they are re-routed. Re-routing reuses `routing::improve_subset`: the endpoints stay on their ports, only the interiors are searched, and the other routes and the node bounds stay fixed. Not yet: container translation, annotations, SVG export, blind comparison, and port side changes (a port stays on its side). Extraction of `editor` into a package waits for a second host.

Contract decisions, verified by the checks listed below:

- **Generated files:** committed under `packages/contracts/src/generated`, never edited by hand. `cargo test -p arqlet-contracts` fails when they differ from a fresh export (drift test). `pnpm --filter @arqlet/contracts run generate` regenerates them.
- **Counters:** `u64` fields (`revision`, `layoutVersion`, `baseRevision`) are typed as `number`, because JSON carries them as numbers. ts-rs would emit `bigint`. The Rust side keeps `u64`; values beyond 2^53 are not representable in the TypeScript type.
- **Output shape:** the types describe what the engine serializes. Fields with `skip_serializing_if` are optional; `Option` fields without it are `T | null`. Inputs that Rust accepts with defaults (`serde(default)`) may be omitted by hosts; no request type is generated yet.
- **Enums:** every generated string-union value must be accepted by the Rust enum and serialize back unchanged (`generated_enum_values_round_trip_through_rust`). The TypeScript check cannot verify enum values, because JSON imports widen string literals.
- **Compile-time fixture check:** `packages/contracts/src/contract.check.ts` assigns the committed engine fixtures to the generated types. `pnpm --filter @arqlet/contracts run typecheck` runs it in CI. Extra JSON keys are not rejected by it.
- **Versions:** ts-rs is pinned to `=12.0.1` (MIT); TypeScript to `7.0.2` (Apache-2.0). Both were checked on the registries before pinning.

### 15.5 Known issues found during planning

- `viewer/routes.js` computes crossings in JavaScript. That violates section 14.3; the display helper must use Rust diagnostics. This is a follow-up after PR 1, not part of the no-behavior-change move.
- `viewer/presets.js` mixes pure logic with application sequencing. It moves to `engine-client` in PR 1, because both hosts use it.

## 16. Node pins (planned, not implemented)

Status: decided, not implemented. Pins are the next layout-side step after the C1 editor.

- **What is pinned:** leaf nodes only, position only. Containers, collapsed aggregates and sizes are out of the first version. Size changes do not move a pin's anchor, but they can create new conflicts, which are checked again.
- **Coordinate contract:** the top-left corner, relative to the direct parent. For a root node the parent is the world origin. A pinned node moves with its container, and its position inside stays fixed.
- **Where pins act:** in placement, including every optimisation and repair step. A pin is a constraint, not an objective: it is never traded against quality.
- **Preserved by:** initial placement, local search, direction changes, container changes, refinement, save and reload, and the browser and backend handoff. A preset change must not remove pins silently; only an explicit unpin does.
- **Unsatisfiable pins are diagnosed, not hidden:**
  - a pin against a free node: the free node moves;
  - two overlapping pins: a conflict naming both ids, neither is changed silently;
  - a pin against a soft layer preference: the preference gives way;
  - a pin against an explicit hard order or a container border: the constraint conflict is reported.
  The manual editor keeps such a state as an invalid draft. The assisted mode keeps the user's change visible and diagnosed when a repair fails; it does not accept an incomplete automatic correction.
- **Empty pin set:** omitted when serialised, so the default output is byte-identical. The existing fixtures and stored fingerprints check this.
- **Order of work:** editor revision guard and dirty routes (done in this change set), leaf position pins, conflict and round-trip tests, then objective semantics. The two objectives whose published key was constant zero (`ProjectedCrossings`, `EndToEnd`) now use the same values as the quality comparison; the key and the comparison are checked against each other by a test (`objectives`). Offering them as editor controls is a separate UI decision.
