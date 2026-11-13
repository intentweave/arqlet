# Changelog

Arqlet is a prototype: every release may change the API, the JSON protocol and the snapshot format. Versions follow
`0.x`; a minor version adds or changes capabilities.

## Unreleased

### Added

- Overview sessions answer only what moves (provisional): a session's `overview` with `positionsOnly` returns
  `OverviewPositions` (positions, root radius, counts, representation) while the clusters stay those of its last
  overview, and the whole overview otherwise. `createOverviewSession` asks for it on the graph the engine keeps and
  merges it with its last overview, so its answers stay whole (on a synthetic 20 000-node graph: 0.37 instead of
  1.46 MB per continuation). Contract type `OverviewPositions`.
- `filter` takes `compact: true` (provisional): the view is laid out again without what is hidden, so the rest closes
  up instead of leaving gaps; a toggle or an edit keeps it, and showing everything lays the view out again. Playground:
  *Close up* in the section *Filter*.
- Hubs left out of communities (provisional): `communityHubs: {minLinks?, nodes?}` (layered view) and the overview
  option `hubs` (same value). Children with at least `minLinks` links in their container (by edge weight) or named in
  `nodes` stay out of Louvain and remain direct children, so one hub no longer pulls the others into one community
  named after it. Omitted when empty; without it the communities are unchanged. Playground: *Hubs from … links* on
  both pages.
- `packComponents: {aspect?}` (experimental, off): the components of a layered container (a lone child is one) are
  packed in rank space towards an aspect ratio (default 1), so a view with many small components or many leaves no
  longer comes out as one very tall column. On synthetic shapes the aspect goes from 0.02–0.23 to 0.91–1.67 at equal or
  smaller area, without crossings or lines through nodes
  ([measurement](benchmarks/results/pack-components-20261113.md)). On larger nested graphs with many edges across
  containers it did not help and raised crossings, so it stays opt-in. Default output unchanged. Playground: *Pack
  components*.
- `neighbourhood` (provisional, read only): the nodes within `hops` (default 1) of `nodeId` or `nodeIds`, following
  edges `out`, `in` or `both` ways (default) and only of `edgeKinds` if given; answered as `trace` (`nodes`, `edges`,
  `visible: {nodes, routes}`, `distance`) plus each node's `hops`, so its nodes can go to `reveal`. In every mode and
  in sessions: `session.neighbourhood(nodeId, {hops, direction, edgeKinds})`; `arqlet_view::neighbourhood` in Rust.
  Playground: *Neighbourhood* around the *From* node.

### Fixed

- With a level of detail, communities or groups, an import's `limit` counted every node below a collapsed container, so
  a first view with many collapsed containers showed only those whose whole content fit (20 containers of 50 nodes,
  `limit: 100`: 2 shown, the rest left out silently). The limit now counts what the first view shows: a collapsed
  container counts once and keeps all it holds, so it can be opened. A container that an edge brings into the view
  while none of its content is selected (an aggregated edge between containers) keeps its content and starts
  collapsed, instead of becoming a plain node that cannot be opened ("Not a container"). Imports without such a view,
  or within the limit, are unchanged byte for byte.

## 0.4.0 — 2026-11-13

Views that change without a new import: a filter that keeps positions, sessions whose state stays in the engine (also
for the cluster overview and in the backend), `trace` along chosen edge kinds, input edge weights, and typed overview
contracts. One vocabulary for the level of detail.

### Upgrading from 0.3

- **Rename `initialCollapse` to `levelOfDetail`** in import options, config sets and stored snapshots; the old name is
  refused with a message that names the new one. Rust: `InitialCollapse` is `LevelOfDetail`,
  `LayoutOptions::initial_collapse` is `level_of_detail`.
- **`createSession` is resident by default** where the engine can keep a session (`loadEngine`, `workerRun`,
  `httpRun`). `session.snapshot` is then only the state (`revision`, `layoutVersion`, `collapsedIds`); call
  `await session.exportSnapshot()` for the whole snapshot, or pass `{resident: false}` for the 0.3 behaviour.
- **Rust: `arqlet_model::Edge` has a field `weight: Option<u32>`.** Struct literals need `weight: None`; JSON input and
  output are unchanged when no edge has a weight.
- **TypeScript:** `@arqlet/viewer`'s `OverviewResult` is now based on `Overview` from `@arqlet/contracts`
  (`schemaVersion` is `number`). The overview's JSON answer is unchanged byte for byte.
- Additive and omitted when unused, so 0.3 snapshots without `initialCollapse` load unchanged: `LayoutOptions.hidden`
  (filter), `Edge.weight`.

### Changed

- **`initialCollapse` is now `levelOfDetail`** (`{depth?, kinds?}`, the same value for the layered view and the
  cluster overview); `initialCollapse` is refused with a message that names the new option. The Rust type
  `InitialCollapse` is `LevelOfDetail`, the field `LayoutOptions::level_of_detail`. Rename the option in requests and
  in stored snapshots.
- **`createSession` keeps the snapshot in the engine by default** (`resident: 'auto'`) where the engine can
  (`loadEngine`, `workerRun`): requests carry only the change and answers bring the scene. Measured in the browser path:
  a trace 20–30×, a scene 4–6× faster. **`session.snapshot` is then the session's state** (`revision`, `layoutVersion`,
  `collapsedIds`), not the whole snapshot; use `await session.exportSnapshot()` for that, or `createSession(run,
  {resident: false})` for the earlier behaviour.

### Added

- Input edge weights (provisional): `Edge.weight` (a whole number of at least 1, omitted when 1) says how many edges an
  input edge stands for (pre-aggregated relations). A bundle, a route for collapsed content (`weight`, `backWeight`)
  and the filter's recount count it that many times, so the drawn width follows it; the overview's links,
  communities, satellites and cluster names pull by it. Weights do not move anything; without them the output is
  byte-identical. A bundle that loses members to the filter now weighs what it still carries.
- `trace` takes `edgeKinds` (provisional): the paths follow only edges of those kinds; `session.trace(from, to,
  {edgeKinds})`; the playground can follow the edge kinds its filter shows. `arqlet_view::trace_along` takes any edge
  predicate.
- Overview types in `@arqlet/contracts` (provisional): `Overview` (the answer, with `OverviewStats` and
  `OverviewRepresentation`) and `OverviewRequest` (with `OverviewLayoutOptions`, `OverviewStart`), generated from
  `arqlet_model`; the engine writes its answer through them (byte-identical output). `@arqlet/viewer`'s `OverviewResult`
  is now based on `Overview`; `createOverviewSession` is typed with them.
- Resident overview sessions (provisional, ADR 0019): in a session, `overview` keeps the graph, its cluster hierarchy
  and the last positions; later requests leave the graph out, and a `start` without positions (`{settle}`) continues
  from the last overview (refused when `communities`, `satellites` or `groups` changed). The hierarchy is built again
  only when those options change. `createOverviewSession(run)` in `@arqlet/engine-client` (re-exported by `arqlet`;
  without a resident session it sends the graph and positions itself, with the same answers); the overview page uses
  it in the worker and the backend.
- `filter` (provisional, view state `hidden`): hides nodes by kind or id (each with what it holds) and edges by kind
  without a new import; what stays keeps its place, the scene leaves the hidden out, and a route for collapsed content
  counts only the edges still shown. `session.filter(hide)` in `@arqlet/engine-client`; playground section *Filter*.
- Sessions in the backend (provisional, ADR 0019): `layout-command --sessions` keeps sessions in one long-running
  process (one JSON message per line); the playground backend serves them at `/api/session` with random keys, one
  message at a time, a time limit per message and at most 32 sessions. `httpRun(base)` in `@arqlet/engine-client`
  (re-exported by `arqlet`) is a backend as a `run` with `openSession()`, so `createSession` keeps the snapshot in the
  backend too.
- `session.trace(from, to)` and `session.reveal(nodeIds)` in `@arqlet/engine-client` (provisional).
- Overview (provisional): option `satellites` (a share, off by default) gathers a node with the siblings that send at
  least that share of their edges to it, as a cluster named `<hub> +<n>`. Clusters the overview adds are named after
  their most connected member (`overview.labels`: `≈ <name>` for a community). `representationsOf` takes the level of
  detail (`levelOfDetail: {depth, kinds}`), and a config set's `overview.levelOfDetail` carries it to the engine:
  clusters from that depth on, and of those kinds, are collapsed until opened. Overview option `groups` takes the
  domain's groups as an import does. Playground: *Satellites to their hub* and
  *Level of detail*; a double-click opens a collapsed cluster.
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

- `@arqlet/contracts`: `Graph` no longer requires `defaultDirection`, `containerDirections`, `ports` and `edgePorts`;
  the engine reads a graph without them, and `session.import({nodes, edges})` now typechecks.
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
