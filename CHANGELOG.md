# Changelog

Arqlet is a prototype: every release may change the API, the JSON protocol and the snapshot format. Versions follow
`0.x`; a minor version adds or changes capabilities.

## Unreleased

### Changed

- **`createSession` keeps the snapshot in the engine by default** (`resident: 'auto'`) where the engine can
  (`loadEngine`, `workerRun`): requests carry only the change and answers bring the scene. Measured in the browser path:
  a trace 20–30×, a scene 4–6× faster. **`session.snapshot` is then the session's state** (`revision`, `layoutVersion`,
  `collapsedIds`), not the whole snapshot; use `await session.exportSnapshot()` for that, or `createSession(run,
  {resident: false})` for the earlier behaviour.

### Added

- Sessions in the backend (provisional, ADR 0019): `layout-command --sessions` keeps sessions in one long-running
  process (one JSON message per line); the playground backend serves them at `/api/session` with random keys, one
  message at a time, a time limit per message and at most 32 sessions. `httpRun(base)` in `@arqlet/engine-client`
  (re-exported by `arqlet`) is a backend as a `run` with `openSession()`, so `createSession` keeps the snapshot in the
  backend too.
- `session.trace(from, to)` and `session.reveal(nodeIds)` in `@arqlet/engine-client` (provisional).
- Overview (provisional): option `satellites` (a share, off by default) gathers a node with the siblings that send at
  least that share of their edges to it, as a cluster named `<hub> +<n>`. Clusters the overview adds are named after
  their most connected member (`overview.labels`: `≈ <name>` for a community). `representationsOf` takes a level of
  detail `depth`: clusters from that depth on are collapsed until opened. Playground: *Satellites to their hub* and
  *Collapse from level*; a double-click opens a collapsed cluster.
- Overview: an outline per node kind (`circle`, `square`, `octagon`, `diamond`, inscribed in the disc, so the layout
  stays as it is): `overviewBuffers(…, {shapes})`, renderer option `shapes` and `setShapes`. Playground: a shape column
  in *Rules by kind*. A community inside another named after the same member is named after the next one.
- Overview: option `chains` (off by default): a container with a single child takes that child's disc, without padding
  of its own. The viewer draws, names (`A › B`, a repeated name once) and picks such a chain as one (`chainTop` in the
  model, `chainOf`); a rule for any kind of the chain applies to it. Playground: *Fold chains of single children*.
- Playground: `server.py --local-examples <folder>` offers the graphs of a folder outside the repository on both pages
  (served read-only at `/local/`, nothing copied into the repository); an ELK graph (`children`) is laid out through
  the ELK adapter.
- Resident sessions (provisional, ADR 0019 step 3): `arqlet::Session` keeps a view's snapshot inside the engine;
  requests carry only the change and answers bring the scene. In WASM through `lw_session_open/run/close`, in the
  worker by message, and in JavaScript through `createSession` (by default, see above); `exportSnapshot()` and `load`
  hand a view over.
- `arqlet::protocol` (provisional, ADR 0019 step 1): the engine's requests as a Rust type (`Request`, `Response`,
  `handle`); `dispatch` goes through it with the same JSON and the same errors.
- README: section *Large graphs* (flexible layout per container, groups and drill-down, path cut-out, cluster
  overview with lens and breathing).

### Fixed

- ELK adapter: graphs whose nodes are mostly without edges were laid out as one long row (74 nodes: 19 624 × 172); unless
  `separateConnectedComponents` is `false`, such nodes now stand in a grid (2 520 × 1 972). Mixed graphs (several
  components plus a few lone nodes) are unchanged; real component packing is still open.
- ELK adapter: `spacing.edgeNode` and `spacing.edgeNodeBetweenLayers` are read (as `routingClearance`) instead of
  reported as unknown.
- An empty graph no longer reports `Single pass was not used: …`.
- The WASM engine is fetched with the browser's HTTP cache (no `cache: 'no-store'`), and `createEngine` takes a
  `WebAssembly.Module` the host compiled.
- `session.import` defaults its `limit` to at most 10 000 (`MAX_VIEW_LIMIT`) instead of failing on larger graphs.
- Cluster overview: nodes are named by their `label` (`overview.labels`, `nameOf`), not by the last part of the id;
  `structuredExcerpt` returns the options to import the excerpt with (`isolatedGrid`), so a structured cluster is no
  longer one long row in its disc.
- A toggle in a graph whose edges mostly cross container boundaries took up to 53 s: after single pass, the routes
  still in conflict made the layout route the whole drawing again with both routing engines, which found nothing
  better. Under single pass the conflicts are now reported instead (0.1–0.2 s; the same conflicts). Default output
  on the measured views is unchanged.

- Opening a group, community or initially collapsed container laid out only around it, which at 50 000 nodes took
  9.7 s and handed hundreds of routes to the slow completing router; such views are laid out from scratch again
  (1.6 s), and their drawing depends only on the graph and the view.

## 0.3.0 — 2026-11-12

Abstraction for large graphs in the structured view: groups (automatic communities or given by the domain), collapsed
first, edges between groups drawn by weight, and a path cut-out between two nodes; for the cluster overview a fisheye
lens and idle breathing. Every new option is provisional and off by default: default output is unchanged.

### Added

- `isolatedGrid` (provisional, off by default): the children of a layered area that have no edge to another child stand
  in a grid after the connected children instead of one long column. The cluster overview's structured clusters use it.
- `createSession(run)` (provisional, `@arqlet/engine-client/session.js`, re-exported by `arqlet`): a layout session for
  drill-down. It keeps the current snapshot, sends collapse, expand and other changes in order with the expected
  revision, and reports changes overtaken by a new import as stale. `workerRun(worker)` turns the engine worker into
  an asynchronous `run`.
- README: a React sketch for the WebGL cluster overview.
- Cluster overview: a fisheye lens (`renderer.setLens`, or `lens` in `bindOverviewNavigation` to follow the pointer).
  Inside its radius the drawing is enlarged towards the middle (Sarkar–Brown), nothing outside moves, labels of what it
  enlarges appear, and picking sees through it (`lensAt`, `lensInverse`). In the playground: panel section Lens, key L.
- Cluster overview: breathing when idle (`renderer.breathe`, or `idle` in `bindOverviewNavigation`). After a while
  without input the discs grow and shrink by a few per cent in a slow wave from the middle; the next input fades it out.
  Drawing only; off under reduced motion. In the playground: panel section Idle.
- `communities` (provisional, off by default): drill-down with automatic communities in the structured view. Large
  containers are divided into communities (the cluster overview's Louvain), shown as collapsed containers named after
  their best connected member and opened with `toggle`. Only the view has them; the source graph is unchanged.
- With communities, the edges between two collapsed communities are drawn by one route for both directions; the scene
  reports what it stands for as `weight` and `backWeight`, and the SVG viewer draws it wider with more edges, with an
  arrow at each end when edges run both ways and a `4 ⇄ 7` label. On a flat call graph of 1 866 nodes: 168 routes
  instead of 236, 1 164 crossing route pairs instead of 2 711.
- `groups` (provisional): groups given by the data (a path of group names per node) instead of, or before, Louvain;
  shown as collapsed containers of kind `group`, view only.
- `initialCollapse` (provisional): an import shows containers collapsed from a depth on or by kind, like the cluster
  overview's first view; edges between them are drawn by one weighted route per pair.
- Path cut-out (provisional): the operation `trace` (read only) gives the nodes and edges on the shortest (or all)
  directed paths between two nodes of the source graph, falling back to the other direction, and what of them the
  current view shows: the boxes that hold them (collapsed groups included), the routes that carry them, and each visible
  node's distance from the path. `reveal` opens exactly the collapsed containers that hold given nodes. `renderSvg`
  takes `trace` to draw the path in amber and fade the rest by distance. Playground: section *Path*.
- Partial reveal (provisional): `reveal` opens the containers above the nodes asked for in part by default: they show
  only the children on the way, the others wait in a collapsed `+N more` box (`rest:<container>`, kind `rest`, dashed in
  the SVG viewer) that a toggle dissolves; `partial: false` opens whole. Stored as `LayoutOptions::partial`, view only.
  Playground: *Only the nodes asked for* and *Show a node*. The 5-step path of the flat example shows 17 nodes instead
  of 56.
- Playground: *Domain groups* and *Collapse first*; the flat call graph example brings the directories and files it was
  flattened from as its groups.

## 0.2.1 — 2026-11-11

Not published on its own: its changes were first released in 0.3.0.

### Fixed

- **The same results on every platform.** Organic layouts and the cluster overview differed between native builds and
  the browser (WASM): an offset of the organic start positions was computed in `usize`, which overflows on 32-bit WASM,
  and the transcendental functions came from each platform's math library. The offset is computed in `u64`, and the core
  calls `sin`, `cos`, `atan2`, `pow` and `hypot` from the `libm` crate everywhere. Native default output is unchanged (the
  stored refinement fingerprints hold); WASM organic layouts change to the native result.
- ELK adapter: the layered spacings under their official names (`elk.layered.spacing.nodeNodeBetweenLayers`,
  `…edgeEdgeBetweenLayers`) were ignored and reported as unknown; they are read now.
- ELK adapter: `elk.separateConnectedComponents` is read (`true` turns on `componentPacking`); it is no longer reported
  as unknown.

### Changed

- The WASM engine on npm is built for size: 4.8 MB (1.2 MB gzip) instead of 9.2 MB (2.1 MB gzip), with the same results
  and somewhat slower (about 15 % in single observations).
- CI checks Linux (x64, arm64), macOS (arm64) and Windows (x64): tests, and native against WASM (`tools/platform-parity.mjs`).
- Documentation: one-shot ELK layout against a session that keeps positions; what `refine` does.

## 0.2.0 — 2026-11-11

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
