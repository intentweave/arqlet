# ADR 0025: A scene renderer with the GPU from afar (WebGPU, WebGL 2) and SVG up close

Date: 2026-11-22.
Status: accepted, V1, V2 and V4 implemented (provisional, `@arqlet/viewer`: `scene-buffers.js`, `scene-renderer.js`,
`scene-gpu.js`, `scene-gl.js`, `scene-view.js`; the playground's large-view mode; React Flow as the near layer in
`examples/react-flow`); V3 open. Before, the scene was drawn by `renderSvg` (SVG) or
handed to React Flow (`toReactFlow`); the cluster overview has its own WebGL renderer (`overview-gl.js`, ADR 0018).
Amended on the same day: the far layer draws with WebGPU where the browser offers it and with WebGL 2 as the fallback
(section 7).

Related: [ADR 0008](0008-engine-client-viewer-boundaries.md) (the viewer paints accepted geometry and computes none;
"SVG first, replaceable Canvas/GPU backends later"; feature detection and a fallback before a new backend),
[ADR 0018](0018-cluster-overview.md) (the overview's WebGL renderer, its zoom policy and the layered excerpt drawn inside
a disc), [ADR 0019](0019-engine-interface.md) (step 4: the scene as columns with interned ids, zero-copy in the browser),
[ADR 0023](0023-primary-store-replicas-and-change-stream.md) (H1: the scene first, the engine on demand; D1: scene
changes for clients that only render).

## Context

A scene of a few thousand boxes and routes is too much for SVG to stay fluid while panning and zooming: every element is
a DOM node, and a transform of the whole drawing still re-rasterizes all of them. Engines that handle large drawings
interactively (yFiles' WebGL mode is the example that prompted this) draw from afar with the GPU and switch to SVG or DOM
when the user zooms in, where text, ports and interaction need the DOM and only few elements are in view.

Most of the parts exist:

- The engine computes all geometry and delivers it as a scene with stable ids, also as **columns** (`session.columns()`:
  node bounds, parents, flags, route offsets and points, ports, labels, conflicts as typed arrays, ids and texts in
  tables), zero-copy in the browser.
- `@arqlet/viewer` has the SVG renderer, the React Flow mapping, navigation (wheel zoom, panning) and a WebGL 1 renderer
  without dependencies for the overview (quads for shapes, a program for lines, picking, a text canvas over it, morphing
  between two layouts in the vertex shader).

What is missing is a GPU renderer for the ordinary scene (boxes, nested containers, orthogonal routes, ports, hulls) and
a rule for when which layer draws.

## Decision

### 1. Two layers over one scene

- **Far: GPU (WebGPU, WebGL 2 as the fallback).** One canvas draws everything in the scene: container and node boxes (instanced quads with the outline
  in the fragment shader, as the overview's shapes), routes as line strips with a width in pixels, arrowheads, membership
  hulls (triangulated polygons), bus bars. Text only above a size on screen, on the overlaid 2D canvas, as the overview
  does.
- **Near: SVG (or the host's DOM).** When the elements in view fall below a budget, an SVG layer above the canvas draws
  exactly those elements with everything a reader and the DOM need: labels, ports, rows, selection, focus and
  accessible names. A host that draws its own node content (React Flow, ADR 0008's consumer-defined DOM content) takes
  that layer's place.
- Both draw the **same geometry**, so the switch moves nothing: the viewer computes no positions, it only chooses which
  layer shows an element.

### 2. The switch by zoom and view, with hysteresis

- A policy per frame, pure and tested in Node like `representationsOf` of the overview: the number of elements in the
  view and their size on screen decide. The near layer takes over below a budget of elements in view (a starting value
  set by measurement, see below) and when a node's box is at least a size in pixels; it hands back above 1.25 times the
  budget, so that a zoom at the threshold does not flicker.
- The switch fades over a few frames (the near layer fades in over the canvas, then the canvas leaves out what the near
  layer draws); a reduced-motion preference switches without fading.
- The near layer holds only the elements in view plus a margin; panning adds and drops elements in the near layer and
  never rebuilds it whole.

### 3. Culling by a spatial index

The viewer keeps a static grid (or an R-tree) over the scene's bounds, built once per scene in the columns' order: the
elements in view are a query, both for the near layer and for picking. The far layer draws all and leaves culling to
the GPU until a measurement asks for more.

### 4. Stable indices for buffers and changes

The columns of ADR 0019 use dense indices, but per scene: a new scene numbers again. For GPU buffers that are patched
in place, and for scene changes (D1), an element must keep its index:

- **Indices per session, stable.** The session keeps one id table per kind (nodes, routes, ports, labels); an id keeps
  its index for the session's life, a removed id leaves a hole (its slot is marked unused, never reused within the
  session), and a new id appends. The columns are written at those indices.
- **Scene changes by index.** A D1 change names the indices it adds, moves or removes, with the layout versions before
  and after; the renderer writes only those slots of its buffers (`bufferSubData`), and the near layer updates only
  those elements.
- **Compaction is a new scene.** When holes outweigh live slots (a ratio set by measurement), the session numbers anew
  and answers with a whole scene; a client never patches across a renumbering (it carries a new numbering version).

This is where the "id table" helps most: in the data path and the renderer. The canonical model keeps string ids (an
interned model was weighed and deferred: its gain at the peak was measured small, see
`benchmarks/results/session-memory-20261121.md`).

### 5. Picking and interaction

- Picking from afar reads an id buffer (each element drawn once more with its index as colour into an offscreen target,
  read back under the pointer), or queries the spatial index; the overview does the latter.
- Selection, hover, focus and keyboard navigation stay in the view controller (`@arqlet/engine-client`); both layers
  draw its state. Accessible names and focus need the near layer; from afar the controller's `find`, `focus` and keyboard
  navigation lead to an element, and the view zooms to it, where the near layer takes over.

### 6. Animation

A new layout of the same view (an edit, a refinement) moves from the old to the new positions: from afar in the vertex
shader with two sets of positions (as the overview's `morph`), near by interpolating the SVG attributes over the same
frames. Elements added or removed fade. Stale results are not drawn (the controller's acceptance rule, #82).

### 7. Fallback and boundaries

- **Backends.** WebGPU first, WebGL 2 as the fallback, SVG alone without either (feature detection,
  `sceneBackend`). Both GPU backends are written here, without a library: they draw the same buffers
  (`sceneBuffers`: instanced boxes and segments, triangles) with the same shaders in WGSL and GLSL ES 3.00, behind one
  shell (`rendererShell`: view, texts, picking, frames), so a backend is about 200 lines. WebGPU is the choice for new
  GPU work in browsers (compute, less driver overhead) but is not yet everywhere (older Android devices, Linux desktops
  without current drivers, managed machines with older browsers), so WebGL 2 stays. A library was weighed: a Rust
  renderer with `wgpu` would cover both from one code base, but needs `wasm-bindgen` and `web-sys` (the engine's ABI
  has neither), adds megabytes of WASM and an asynchronous start, and the texts and the SVG near layer stay in
  JavaScript anyway; JavaScript libraries (PixiJS, luma.gl) add a dependency of several hundred kilobytes to a viewer
  without dependencies for drawing that is three kinds of instanced quads. To be weighed again if the far layer needs
  more than that (text atlases, meshes, compute).
- Optional: the far layer in a worker with `OffscreenCanvas` where available; nothing may require it, and no
  `SharedArrayBuffer` is needed (ADR 0019).
- The renderer stays in `@arqlet/viewer`, without dependencies, and computes no layout: no routing, no contour, no
  placement (ADR 0008).

## Steps

- **V1 — GPU scene renderer from columns (done):** boxes, containers, routes, arrowheads, hulls, picking; same geometry as
  `renderSvg` on the golden fixtures (pixel comparison within a tolerance at the same transform). *Exit:* the golden
  scenes drawn by both renderers agree; a scene of the reference set pans and zooms with a measured frame time recorded
  against SVG on the same machine.
- **V2 — near layer and switch (done):** the policy (pure, tested), the SVG layer for the elements in view, fading, hysteresis.
  *Exit:* zooming in and out across the threshold moves no element (positions of both layers compared), and the near
  layer never holds more than the budget plus the margin.
- **V3 — stable indices and scene changes:** stable per-session indices in `SceneColumns`, D1 changes by index, buffer
  patches. *Exit:* applying a change to the buffers gives the same buffers as drawing the later scene whole (byte for
  byte); a change across a renumbering is refused.
- **V4 — playground and React Flow (done):** the playground draws large views with the far layer; a React Flow host takes the
  near layer's place. *Exit:* `ui-regression.mjs` covers the switch; the React Flow example keeps working.

### What V1 and V2 built

- `sceneBuffers` turns a scene (or its columns) into per-layer instances in `renderSvg`'s order, colours, widths and
  outlines (hulls, containers, routes with arrowheads, bus bars, hub stubs, junctions, leaves, ports, crossings, edge
  labels) and the texts for the 2D canvas; a translucent route blends once at its corners through one depth per line.
  Rows, the entity rule, rests (dashed) and the selection colours are drawn; the controls (pins, toggles, resize) are
  the near layer's only.
- `sceneIndex` (a uniform grid) answers the elements in a rectangle and picking in `renderSvg`'s hit order.
- `createSceneView` holds both layers. The near layer is `renderSvg` on `sceneExcerpt` of the view plus a margin of
  half its size on every side; panning only moves it until the view leaves that rectangle; a change of scale draws it
  again. `nearPolicy` (budget 400 elements, median leaf 24 px, hysteresis 1.25: provisional values) decides per view;
  from afar it decides without a query (the median leaf of the scene is too small). The switch fades over 150 ms
  (none under reduced motion), then the canvas is hidden.
- Checks: Node tests of the pure part (`scene-buffers.test.mjs`); in Chrome (`browser-checks/scene-gl.mjs`) the golden
  scenes drawn by both backends differ from `renderSvg` in 0.01 % of the pixels without text, and over a zoom sweep in
  and out the near layer's boxes stand exactly where the view puts them, the far layer's within a pixel, with the
  hysteresis visible. Frame times: `benchmarks/results/scene-frames-20261122.md`.

### What V4 built

- The near layer is replaceable: `createSceneView(host, scene, {near})` takes `{element, draw(excerpt, frame),
  move(frame), clear()}`; the default is `svgNearLayer` (renderSvg). `reactFlowViewport(view, width, height)` gives the
  viewport under which React Flow shows exactly the view's geometry.
- Playground: a large-view mode (button *GPU*, key G) draws the scene with the scene view; it is on by itself from
  1 500 elements (nodes and routes) unless the user chose. Selection (picking from afar, the near layer's handlers),
  route selection, collapse and expand, pins, Fit and zoom work in it; the editing gestures (drags, resize, ports,
  labels) and a curation draft use the plain SVG view. Tested in `ui-regression.mjs` (a stand-in scene view: the
  switch, selection from both layers, redraws, Fit) and in Chrome (`browser-checks/large-view.mjs`: a large example
  opens far, real wheel zoom hands over to the near layer and back, real clicks select in both).
- React Flow example: a checkbox *GPU far layer* (on from 1 500 elements): from afar the GPU, up close React Flow with
  the excerpt and a viewport set from the view; React Flow's own panning and zooming are off, the scene view navigates.
  Checked in Chrome (`browser-checks/react-flow-near.mjs`).
- Found on the way: with only lines in view (no leaf), the switch takes the scene's typical leaf as the size on
  screen, else a zoom onto long edges would never hand over.

### Completeness of the far layer

- Every kind of drawing is checked against `renderSvg` in both backends (`browser-checks/scene-gl.mjs`, pixels without
  text, under 1 % of the pixels off): the golden scenes and the playground's examples as it lays them out (tree and
  radial, mixed strategies with circle containers, ER rows, UML, Petri nets with shapes, bus lines in layered and organic
  views, a zonal vehicle, membership hulls as polygons, hub references, self-loops), each strategy (organic, none) and
  routing style (straight, orthogonal, rounded orthogonal, polyline, rounded polyline) of one view, bundling, and the
  states: a trace with fading, pins with a selection, a selected route. The engine refuses the routing styles
  `octilinear`, `curve` and `bus` for now ("not drawn yet"); junction dots of explicit buses are drawn but no example
  produces them yet.
- Found on the way: lines and outlines thinner than a pixel were drawn one pixel wide at full opacity, much stronger
  than SVG from afar (4 % of the pixels off on a view of 96 nodes); they now keep one pixel of width with their coverage
  as opacity, as SVG rasterises them.
- States drawn from afar as up close: the trace (amber path, the rest receding, with fading by distance), pin dots, the
  selection, and the hover outline (its own small layer, so a hover never rebuilds the scene's buffers).
- React Flow as the near layer draws bus bars, taps, junctions, hub references and membership hulls through
  `toReactFlow(scene, {overlayNode: true})` (hulls are new among its overlays).

## Measurements before defaults

The budget of the near layer, the size threshold, the hysteresis and the compaction ratio are set by measurement on the
reference views and the synthetic graphs (frame times while panning and zooming, time to switch), with hardware,
browser and build recorded in `benchmarks/results/`. No frame rate or "fluid at N elements" is claimed without such a
file.

## Consequences

- Large views stay interactive from afar; text, ports and accessibility stay where the user reads, up close.
- Two drawings of one geometry must agree; the golden fixtures and a pixel comparison guard that.
- Stable indices make the session's columns slightly larger (holes) and give D1 and the renderer in-place updates.
- The overview's renderer and this one share shaders and helpers where they can; they stay separate modules.

## Not decided here

- Text from afar: a glyph atlas (signed distance fields) on the GPU, or the overlaid 2D canvas as now.
- Whether hulls and bus bars draw near in SVG or stay on the canvas under the SVG layer.
- WebGPU and the worker mode as defaults.
