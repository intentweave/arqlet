# Changelog

Arqlet is a prototype: every release may change the API, the JSON protocol and the snapshot format. Versions follow
`0.x`; a minor version adds or changes capabilities.

## 0.2.0 — unreleased

### Added

- **Cluster overview** ([ADR 0018](docs/adr/0018-cluster-overview.md)): the operation `overview` lays out a whole graph
  as nested discs in its containment hierarchy, container by container (force simulation with Barnes–Hut repulsion,
  no overlaps), with the containers of one level in parallel and the same result for any thread count.
  - Large flat containers and flat graphs are split into nested communities (deterministic Louvain; option
    `communities`).
  - Continuations: `radii` (the room a node needs) and `start` (a previous result) make room or, with `settle`, settle
    again; containers that did not change keep their arrangement exactly.
  - Representations per container (collapsed, organic, structured), decided by the user, then by rules of the data
    (`ConfigSet.overview`), then by the zoom; the user's decisions are engine values (`OverviewIntent`), checked and
    answered by the operation.
- `@arqlet/viewer`: `overview.js` (model, buffers, picking, representation policy, structured excerpts with border
  ports facing the neighbours, labels without overlaps) and `overview-gl.js` (a WebGL 1 renderer without dependencies,
  with animated morphs between layouts and blends between representations). Re-exported by `arqlet`.
- Model types `Representation`, `OverviewConfig`, `OverviewIntent` (and their TypeScript contracts).
- Playground: the cluster overview page (browser and backend modes, panel for the selected cluster, rules, zoom and
  layout settings, undo); the SVG playground keeps an opened or closed container in place and animates edits.
- Checked on real exports (Intentweave, Backstage): see `benchmarks/results/overview-real-20261111.md`.
- Benchmarks: a generator of code-index-like graphs (`benchmarks/overview`), a timing script and a comparison with
  d3-force (`benchmarks/eval/overview-d3.mjs`).

### Changed

- The organic layout's Barnes–Hut quadtree takes masses; with mass 1 it is bit-equal to before (test).
- `ConfigSet` has an optional `overview` part; config sets without it serialize as before.

## 0.1.0 — 2026-11-11

First release on crates.io (`arqlet` and the engine crates) and npm (`arqlet`, `@arqlet/engine-client`,
`@arqlet/viewer`, `@arqlet/contracts`): incremental hierarchical layout of nested graphs with ports, routing, the JSON
dispatcher for native, WASM and backend hosts, and an ELK-JSON adapter.
