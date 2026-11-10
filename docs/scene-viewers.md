# Scene v1 and independent viewers

Target ownership and language decisions are in [ADR 0008](adr/0008-engine-client-viewer-boundaries.md). This file describes existing adapters; [ADR 0006](adr/0006-reactive-layout-session.md) defines the future measurement/interaction session.

The first renderer-neutral JavaScript boundary lives in
`packages/viewer/scene.js`. It projects an existing Rust snapshot; it does
not run layout, alter graph semantics or invent routes. This is a provisional
small contract, not a general-purpose scene graph or published package.

`snapshotToScene(snapshot)` returns plain serializable data:

- `schemaVersion`, graph `revision` and `layoutVersion`;
- nodes with stable IDs, parent IDs, absolute bounds, title/kind, container,
  aggregate, collapsed and invalid flags;
- drawn routes with source/target IDs, exact absolute points, bundle weight and
  conflict state (member edges remain in the snapshot, not in the drawing list);
- resolved ports, measured label bounds and derived container crossings;
- port-marker visibility and total bounds including negative coordinates, labels
  and outside routes.

Input snapshots are not mutated. Unknown schemas, invalid rectangles, containment
cycles and missing route endpoints are rejected. The adapter expects snapshots
from the engine; this is not a complete untrusted-document validation layer.
Container coordinates are flattened once into scene coordinates. Consumers must
not apply a parent transform to routes or labels a second time.

Geometry is no longer computed in JavaScript. The Rust scene crate builds scene v1
(`arqlet_scene::build_scene`) and the engine exposes it as the operation `scene`
(`{ok, scene}`), in WASM, native and backend alike. `viewer/engine.js` provides `loadEngine` and
`sceneOf`; `viewer/scene.js` only checks the schema and offers path helpers. Viewer fixtures
(`fixtures/*.scene.json`) are engine output, and a Rust test keeps them equal to `build_scene`
for the stored snapshots.

## SVG reference viewer

`renderSvg(svg, scene, {selected, onSelect, scale, document})` paints the scene.
It owns neither snapshots nor workers, backend calls, controls or graph mutations.
Selection emits an ID to its host. SVG marker IDs are isolated per viewer.
The existing demo remains the host and retains edit/version/measurement logic.

## React Flow integration

`toReactFlow(scene)` is a pure adapter without a React dependency. It converts
absolute node bounds back to parent-local React Flow positions, emits parents
before children and exposes exact route endpoints as non-connectable handles.
A custom edge component draws Arqlet's polyline; React Flow routing helpers
are deliberately not invoked. Labels and boundary markers occupy absolute scene
coordinates in `ViewportPortal`. Hidden markers keep endpoint handles alive.

The example in `examples/react-flow` supports pan/zoom, selection, snapshot
file loading and engine-backed collapse/expand. Dragging, resizing, reconnecting
and deletion are disabled until they can emit versioned engine commands. Reset
and file load invalidate pending responses. The engine remains independently
usable, without React or any renderer dependencies in Rust.

The shared synthetic regression includes a nested container, explicitly bound
endpoint, bundle, measured label, negative bounds and a node titled as a comment
box. This tests representation only: annotation anchoring and exclusion from rank
assignment remain future engine work.

## Validation and next steps

Node tests compare both adapters against exact input geometry and exercise
selection, hidden markers and isolated SVG marker IDs. Existing simulated-DOM
app tests still execute real WASM. The React example has a production build check;
these checks do not replace a real-browser visual/interaction test.

No large-view performance claim follows from this work. Next: browser visual
regressions, a versioned measurement/interaction event contract, then measurements
of visible scenes before deciding on a Canvas/GPU renderer. The SVG viewer still
rebuilds its content; incremental rendering and viewport culling are not yet added.

## Historical review notes

The notes below record an earlier review; subsequent changes described above supersede the JavaScript scene-geometry finding. They are not the current implementation inventory.

Done in this round:
- Route conflicts are structured (`routeConflicts`: edge and node IDs). The viewer no longer
  recognises conflicts by matching diagnostic text. The field is omitted from snapshots without conflicts.
- Golden representation tests (`viewer/golden.test.mjs`) render two engine snapshots through the SVG viewer
  and the React Flow mapping, and compare the output with reviewed files in `viewer/golden/`. A deliberate
  change to the painter fails the test. `UPDATE_GOLDEN=1` rewrites the files after review.
- The viewer tests run in `build.sh` and in the React example's `pnpm test`.

Open:
- Resolved since this review: the Rust `scene` operation now owns absolute geometry and scene bounds, as described above; JavaScript adapters map and validate the scene.
- Markup tests do not show rendering. A browser screenshot test needs a browser dependency; this is a
  decision to make, not a silent addition.
- The React build prints a `"use client"` warning from `@xyflow/react`. It is upstream and harmless.
- Config-set validation is strict (`deny_unknown_fields`). Config-set import and export in the UI are not
  implemented yet.

References used for API integration (no upstream implementation copied):
- https://reactflow.dev/learn/customization/custom-edges
- https://reactflow.dev/api-reference/components/handle
- https://reactflow.dev/api-reference/components/viewport-portal

Dependency versions are recorded in the example's package-lock.json; upstream
packages retain their own licenses and notices.
