# @arqlet/viewer

Rendering helpers for Arqlet scenes. The engine computes all geometry; these modules only draw or map it.

- `svg.js`: `renderSvg(svgElement, scene, options)`;
- `react-flow.js`: `toReactFlow(scene, {overlayNode?})`, nodes and edges for React Flow (no React dependency); bus bars
  and taps, junction dots and hub-reference stubs come back as overlays in flow coordinates (`busLines`, `junctions`,
  `hubRefs`), or, with `overlayNode`, as one inert node of type `arqletOverlay` that a host's node type draws;
- `navigation.js`: wheel zoom at the pointer and drag panning;
- `scene.js`, `routes.js`: schema check, path data and route details;
- `scene-view.js`: a scene for large views (provisional, ADR 0025): `await openSceneView(element, scene, options)` draws
  it on the GPU from afar (WebGPU, else WebGL 2: `scene-gpu.js`, `scene-gl.js`) and with `renderSvg` up close, switching
  by the elements in view and their size on screen; `bindSceneNavigation(element, view)` adds wheel zoom and panning;
  `scene-buffers.js` is its pure part (buffers, index, picking, the switch rule); a host may replace the SVG near layer
  with its own (`near`; `reactFlowViewport` for React Flow, see `examples/react-flow`);
- `overview.js`, `overview-gl.js`: the cluster overview of a whole graph (engine operation `overview`), drawn with WebGL, with picking and the excerpt for a drill-down.

> **Status: prototype.** The API, the JSON protocol and the snapshot format are
> provisional and may change in any release.

Part of [Arqlet](https://github.com/intentweave/arqlet), an incremental graph-layout
engine in Rust. See the repository for documentation and the current status.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
