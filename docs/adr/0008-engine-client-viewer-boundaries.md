# ADR 0008: Rust engine, TypeScript client and replaceable visualization

Date: 2026-10-24.
Status: accepted target boundary; existing packages implement parts of it.

Related: [ADR 0006](0006-reactive-layout-session.md), [ADR 0007](0007-recursive-strategy-orchestration.md), [scene viewers](../scene-viewers.md).

The next browser integration milestone is [ADR 0009](0009-unified-interactive-playground.md): unify interactive and curation workflows while preserving legitimate browser previews and moving authoritative geometric repair into the engine.

## Context

Arqlet must work as a standalone layouter and support its own interactive visualization as well as external viewers. Browser measurements feed back into layout; the renderer must remain responsive while computation continues. Rust/WASM can access browser APIs, but using Rust for presentation alone establishes neither a rendering speed advantage nor renderer independence.

## Decision and ownership

Use Rust for the layout/geometry engine and TypeScript/JavaScript for the default browser client, interaction controller and visualization. React wrappers are optional adapters, not a core dependency. The wire/scene contract is language-neutral and versioned.

| Layer | Responsibility | Existing home / target evolution |
| --- | --- | --- |
| Rust semantic/geometry core | Graph/view projection, resolved constraints, contour predicates, regional algorithms, ports, routing, labels, evaluation | Existing model/view/layout/routing/topology/scene crates |
| Rust optimization runtime | Regional plan, solver lifecycle/checkpoints, dependencies, accepted engine input state, complete-candidate selection | Runtime; logical reactive session grows incrementally |
| Host engine client | Worker/native/backend transport, input batching, measurement context, cancellation requests, revision-gated delivery | `packages/engine-client`; TS-facing API over existing JS |
| Browser interaction controller | DOM pointer/keyboard input, pan/zoom, selection, drag/resize intent, immediate preview, measurement acquisition | Framework-independent TS/JS; consumers may provide their own |
| Scene renderer | Paint accepted scene, apply display patches, interpolate, cull visible objects, maintain picking/accessibility | `packages/viewer`; SVG first, replaceable Canvas/GPU backends later |
| Framework adapters | Connect lifecycle and callbacks to React or another UI framework | Optional packages/examples; no required framework in engine/client/viewer |

The host client is a proxy/controller, not a second geometry authority. Rust and host-side revision gates may both reject stale responses; they enforce the same identities at different boundaries. The client never independently resolves layout constraints or chooses new canonical routes.

## Data and feedback flow

`consumer -> typed input deltas -> engine client -> Rust session/region plan -> snapshot/scene publication -> client revision gate -> renderer`

`renderer/measurement provider -> context-tagged content measurements and interaction intent -> same input path`

The Rust scene builder produces realized node contours, route/port/label geometry and stable IDs. A thin browser adapter maps this to SVG, Canvas or another viewer. Do not rerun edge routing or derive a second set of canonical contour attachments in an adapter. Logical paths may be approximated for display using declared tolerances; layout validation remains authoritative.

Consumer-defined DOM node content is permitted. The consumer reports intrinsic bounds and contour data in layout units under ADR 0006, and places content into the accepted geometry. CSS zoom, animation coordinates or a ResizeObserver echo of an assigned width are not new authored dimensions. Text editing, focus and accessibility stay available even if a future renderer uses a canvas.

A consumer can use the engine without the default viewer, the scene renderer without a running engine, or the client with a custom renderer. No mandatory round trip through Rust for pan, hover, selection or each animation frame. Persistent edits are sent to the engine; preview remains an explicitly nonauthoritative presentation layer.

## Rendering and update performance

Two incremental mechanisms are required and independent:

1. Engine delta processing: reuse unchanged analysis and solver/geometry artifacts; repair affected regions.
2. Viewer delta application: retain scene objects by stable ID; update changed geometry/style; avoid full DOM or scene reconstruction per publication.

Snapshots bootstrap and resynchronize. Initially derive keyed renderer changes between accepted snapshots if necessary; this display diff is not the primary engine input-delta detector. Versioned atomic patches can later avoid full-scene transfer and repeated scene processing. Route/label/port/container updates belonging to one transaction appear together. Rejected/out-of-order patches request resynchronization rather than partially applying.

Separate the engine publication cadence from the display frame loop. Batch bridge/worker messages and measurements; do not call WASM once per vertex on each frame. Measure transport, serialization/copying, layout, scene construction, picking and painting separately. Start with existing plain-data contracts; introduce typed/transferable buffers for measured bottlenecks rather than assuming shared memory is necessary.

Viewport culling and render LOD reduce drawing work without changing the authoritative graph or layout. Layout aggregation/collapse changes the view and uses the session update protocol. Approximate hover queries may use a renderer-local spatial index; editing decisions must refer to current IDs/geometry, and exact geometry helpers may be shared from Rust where needed. Avoid round trips for every pointer sample by batching or using cached display geometry.

## Where Rust/WASM rendering may help later

The default viewer remains TS/JS. Optional Rust/WASM modules may implement measured CPU-heavy work such as tessellation, batched geometric picking, large geometry-buffer construction or export. A later renderer can use Rust with a GPU backend while preserving the scene and interaction contracts. This does not migrate DOM measurement, focus or browser lifecycle into the layout core.

Canvas/WebGL/WebGPU rendering is a backend choice independent of source language. GPU acceleration does not require Rust; Rust/WASM does not automatically accelerate DOM/SVG updates. OffscreenCanvas can move supported drawing work into a worker, but worker rendering and layout remain independently scheduled. Keep input and immediate preview responsive even if a renderer worker or layout worker is busy. Feature detection and a supported fallback are required before adopting a new rendering backend.

Adopt an optional accelerator only after measuring end-to-end benefit including bridge cost, data ownership and memory. Preserve headless native engine use and external viewer compatibility. A native Rust renderer is a separate adapter, not a browser dependency of the core.

## Toolchain and package boundaries

- The TypeScript workspace uses pnpm on Node 24, as CI does now. Bun is deferred: no package may depend on Bun-specific APIs, and every package must run on Node. Revisit when a measured need appears (install time, script runtime), with its own decision.
- Target packages: `contracts` (versioned scene and protocol types), `engine-client` (transport and revision gates), `interaction` (pointer and keyboard controller, gesture state machine), `viewer` (renderer with stable IDs, theme tokens and a shape registry), `containment-tree` (framework-independent tree view that emits intents, see [ADR 0010](0010-containment-tree-and-container-sizing.md)), `react-flow` (optional adapter, moved out of `viewer`), and `apps/playground` as a consumer only.
- Core packages depend downward on contracts only; no core package imports an app or a framework. Adapters depend on their core package, never the reverse.
- Node and container outlines are data in the scene contract (a path or polygon with a stable ID), not an enum name. Renderers, picking and routing tests draw and hit-test from that data. A built-in shape such as a circle is one outline, not a special case.

## Alternatives considered

- **Everything in Rust/WASM:** technically possible, but would expand browser bindings and UI responsibilities without evidence that it addresses the current bottleneck. Retain as a possible renderer backend, not the default product architecture.
- **Everything in TS/JS:** convenient browser integration, but duplicates the established Rust computation and weakens native/WASM algorithm parity. Keep the shared Rust engine.
- **Require a React diagram framework:** useful as an optional integration, but couples consumers and risks rerouting/repositioning already computed geometry. Use adapters with exact engine paths.

## Delivery and verification

First establish the measurement/interaction contract and reusable TS-facing client over the current worker/backend adapters. Keep the SVG reference viewer and add stable-ID updates. Validate drag/resize while computation runs, stale-result rejection and feedback-loop suppression. Use mixed shapes, nested containers and external DOM labels in integration fixtures.

Only then select a larger-scene rendering backend using measured visible-scene workloads, memory, interaction latency and accessibility requirements. Do not tie engine node-count limits or a 100K-layout claim to renderer language selection. Reactivity and correctness tests must cover custom/external viewers as well as the default demo.

No renderer rewrite, framework dependency or new GPU library is authorized merely by this ADR. Current implementation status remains in the project status/specification documents.

## Sources checked 2026-10-24

- [wasm-bindgen DOM example](https://rustwasm.github.io/docs/wasm-bindgen/examples/dom.html): browser DOM access from Rust is possible through bindings; this is capability evidence, not a speed claim.
- [MDN OffscreenCanvas](https://developer.mozilla.org/en-US/docs/Web/API/OffscreenCanvas): transferable offscreen rendering and worker use; check individual backend compatibility.
- [MDN transferControlToOffscreen](https://developer.mozilla.org/en-US/docs/Web/API/HTMLCanvasElement/transferControlToOffscreen): transfer a canvas for worker rendering.
- [MDN ResizeObserver](https://developer.mozilla.org/en-US/docs/Web/API/ResizeObserver): browser element-size observation; measurement interpretation and provenance remain Arqlet's contract.

## Update (2026-10-27)

The viewer draws the points of the scene as given; the rounding of corners moved into the scene ([ADR 0013](0013-realized-path-geometry.md)).
A comparison of the viewer's drawing with the host element by element exists as an option (`renderSvg(..., {keyed: true})`) but is not used:
on 548 nodes it costs about eight times a complete drawing, because it serialises every element. Stable event targets would need a key from the
scene's data before the elements are made. The streamed publication is a typed `PublicationEnvelope` in the generated contracts.
