# @arqlet/viewer

Rendering helpers for Arqlet scenes. The engine computes all geometry; these modules only draw or map it.

- `svg.js`: `renderSvg(svgElement, scene, options)`;
- `react-flow.js`: `toReactFlow(scene)`, nodes and edges for React Flow (no React dependency);
- `navigation.js`: wheel zoom at the pointer and drag panning;
- `scene.js`, `routes.js`: schema check, path data and route details.

> **Status: prototype.** The API, the JSON protocol and the snapshot format are
> provisional and may change in any release.

Part of [Arqlet](https://github.com/intentweave/arqlet), an incremental graph-layout
engine in Rust. See the repository for documentation and the current status.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
