# Changelog

Arqlet is a prototype: every release may change the API, the JSON protocol and the snapshot format. Versions follow
`0.x`; a minor version adds or changes capabilities.

## Unreleased

### Added

- **Inner-edge share and modularity per container on request** (#236, provisional; ADR 0018 addendum of
  2026-11-28): `containerStats: true` adds `containerStats` to the `overview` answer: per container (node index, -1 the
  root) the weight of the edges inside it and of those touching it, their share, and its term of the modularity of the
  partition at its depth, plus the modularity of each depth, so that a host can mark weak containers of its own
  partition (`groups`) or of the communities. The same numbers for any thread count, natively and in WASM; typed
  columns in the columnar answer (`containerStats.*`); not part of `positionsOnly` answers. Without the flag nothing
  changes. New contract type `OverviewContainerStats`. Formula and usage:
  [integration guide](docs/integration-guide.md#marking-weak-containers-provisional).
- **A native Node.js adapter (provisional, unpublished; ADR 0030, #223).** `crates/node` wraps `dispatch` and the engine
  sessions through Node-API (napi-rs 3.16.1); every call that computes runs as an asynchronous task on libuv's thread
  pool, so the event loop stays free during a layout, and sessions run in parallel with each other and one at a time
  each. `@arqlet/node` (`packages/node`, `private` until publishing is decided) loads it with the interface of the WASM
  engine: `loadNativeEngine({sessionBytes})` answers an asynchronous `run` with `openSession()` (`run`, `close`,
  `columns`, `overview`) that `createSession` takes, and `loadEngine({engine})` chooses native, WASM or the first that
  loads (`ARQLET_ENGINE`). The answers are byte-identical to the WASM engine's (`tools/platform-parity.mjs --node`).
  Build it with `node packages/node/build.mjs`. Accepted gaps (ADR 0030): no progressive publications, phase timings
  unreliable under concurrent calls, no musl or Intel macOS binaries; prebuilt binaries and publishing are a separate step.
- **Sessions usable from several threads (provisional; ADR 0024 C2):** `arqlet_runtime::shared_sessions::SharedSessionStore`,
  the store of the native adapter: one short lock for its book, one lock per session, a byte limit that never drops a
  running session.
- **A work budget for the overview** (#211, provisional; ADR 0018 addendum of 2026-11-27): `workBudget` bounds the
  `overview` operation deterministically, so a host can bound a request without terminating its worker. When it runs
  out, the answer is valid with the rest packed in rows and says what was left out (`work`: `used`, `limited`,
  `unclustered`, `packed`); `overBudget: "refuse"` fails cleanly instead. The same answer for any thread count, natively
  and in WASM; without `workBudget` nothing changes. New contract types `OverviewWork` and `OverBudget`. How to choose
  a budget: [integration guide](docs/integration-guide.md#bounding-an-overviews-work-provisional).
- Overview renderer: `pause()` and `resume()` (provisional, #221), for a host that hides the overview (for example under
  a drill-down) without destroying it. While paused the renderer draws no frame and requests no animation frame
  (breathing, morphs, blends, lens and orbit updates), keeps its GPU resources, model, view and state, and takes every
  call that changes them, drawn at `resume`; the animations' clock stands still, so a morph or breathing goes on where
  it stood. `resume()` draws one frame at once. Both are idempotent, `destroy()` works in either state, `paused` tells
  which, and it is the same on WebGPU, WebGL 2 and WebGL 1. A `bindOverviewNavigation` binding takes no input while its
  renderer is paused (a node drag in progress ends at its last point) and holds its idle wait until `resume`. The
  overview page pauses the overview while the scene view of a cluster covers it.
- **Opening a cluster of an overview without the graph** (#209 part 2, provisional): `structuredExcerptOf(model, id,
  {direction, bundle})` in `@arqlet/viewer/overview.js` (also `structuredExcerpt(null, model, id)`, and from `arqlet`)
  builds the excerpt a drill-down lays out from an overview answer alone, for a host that computes the overview on a
  server. The same rule as `structuredExcerpt` and, where both run, the same graph (checked on generated graphs and
  engine answers): the cluster as the one top-level container, its inner edges, and its leaving edges to one stand-in
  per neighbouring cluster, direction kept. Edge kinds and weights come from the answer's `edgeData` (without it: kind
  `''`, weight 1, and `edgeData: false` says so); edges are named `edge:k` after their index in the answer
  (`EXCERPT_EDGE_PREFIX`), so `overviewTrace` answers cannot be mapped onto them without the input edge ids; nodes carry
  only `id`, `kind`, `label` and `parentId`. `bundle: true` merges the leaving edges of a node to a stand-in per kind and
  direction into one weighted edge. Flow: [integration guide](docs/integration-guide.md#the-cluster-overview-as-a-server-result-provisional).
- Integration guide: *Showing your own partition* (overview): a partition the host computed (for example with Leiden)
  as `groups`, flat or nested, with `communities: 0` shown as given; what Louvain does inside large groups, labels, the
  stable group ids, and switching between two partitions with `start.nodes` (a group both have keeps its inner
  arrangement, nodes whose path changed are placed anew, groups that are gone are listed in `continuation.ignored`),
  pinned by a test in `crates/runtime/tests/overview.rs`. Mentioned in the READMEs and ADR 0018.
- Every npm package (`arqlet`, `@arqlet/contracts`, `@arqlet/engine-client`, `@arqlet/viewer`, `@arqlet/query`) ships
  `CHANGELOG.md`, copied by `prepack` (`tools/npm-prepack.mjs`) as the licenses are.

### Fixed

- **A `patch` into a view with buses draws what an import of the patched graph draws** (#228, ADR 0029 C). It started
  from the earlier drawing and drew far worse in layered views with bus lines and organic views with bus bars: on the
  generated bus meshes of #212, 1 043 and 2 091 routes through nodes where an import has none, and 1 374 where it has 36.
  Organic views with bus lines already drew the import's drawing and are unchanged; views without buses still continue
  their earlier drawing. Such a patch now takes an import's time: about the same in layered views, much longer in organic
  views with bus bars where the import runs its second routing pass (the dense view of #212: 2.0 s before, 16.5 s now;
  `benchmarks/results/patch-as-import-20261127.md`; measured before the faster second pass of #229). Revisions and stale
  checks are unchanged.

## 0.10.0 — 2026-11-27

A query and grouping release. The read-only openCypher layer grows by the scalar functions, `UNION`, maps and list
expressions, and gets an npm package of its own, `@arqlet/query`, without the layout engine; several answers it gave
wrongly in 0.8.0 and 0.9.0 are fixed. Memberships (ADR 0022) can be held together inside their container and drawn as
lanes and phases, pinned nodes and containers can stay in single pass (behind an option), a view fits into a URL as a
compact bookmark, the galaxy overview draws on a light background, and bus lines with closed containers import faster.
A cluster overview computed on a server carries the kind and weight of every edge and continues by node id after the
graph changed, and the router can search a sparse grid on large views (behind an option). Two panics on views with bus
lines are fixed, `refine` works on views with a backbone bus, and a long-form bookmark of a view with a changed shape or
direction opens again on a new import.

### Upgrading from 0.9

- **The engine version is `0.10.0`**: edits of a snapshot from an earlier engine are refused, so import the graph again.
  Default layouts of the reference views are unchanged; the reference fingerprints are
  `fingerprints-20261126-default-0.10.0.jsonl` (14 views differ from the 0.9.0 file only in metadata;
  `layerweave-code`, which is generated from the source, has the same geometry as v0.9.0 on the same graph).
- **Some drawings differ:** views where an edge or a tap leaves an organic, radial, tree or octilinear container of a
  layered area next to a bus line, and views with `hubRefs` and region ports (see *Fixed*); membership hulls
  (provisional). With bus lines, `busTaps`, `recomputedContainers` and one line of `layoutDiagnostics` can differ.
- **Views with a backbone bus that single pass did not take:** their snapshot says `singlePass: false` and
  `abstractRouting: false`, and an edit that takes the last backbone bus out of the view stays on that pipeline (see
  *Fixed*).
- **Queries answer differently where 0.9.0 was wrong:** a `WHERE` after `ORDER BY`, `SKIP` or `LIMIT`, a label or
  properties on a repeated node variable, a grouping expression beside an aggregate, and a relationship variable bound
  by an earlier clause.
- **Long-form bookmarks:** a revision of the graph that differs from the bookmarked source only in declared shapes,
  directions, strategies, routing or ports is now the same source (as for compact bookmarks): the bookmark opens, and its
  values replace the declared ones. Before, it was refused as another graph.
- **Queries and content that are refused now:** properties named `id`, `kind`, `label`, `parentId` or `weight` beside
  the graph; an expression that reads a variable without a value; a computed value beyond 16 MiB. A map in content or a
  parameter is a map now, and a map in a JSON answer is `{"map": {…}}`. An `overview` request with an `edgeData` that is
  not a boolean is refused (it was ignored).
- **Overview texts on a light background:** the overview renderer (`createOverviewRenderer`) takes its tone from its
  clear colour (`background`, `setBackground`): above one half relative luminance it is `light` (`renderer.tone`) and
  draws the texts of its `labels` canvas dark itself (cluster and node names, the lens rim; galaxy name tags light with
  dark text). In 0.9 they were always light. A host that inverted the label canvas for a light background (for example
  with a CSS `filter: invert(1)`) must stop, or its texts turn light again.
- **Rust hosts** that build these types field by field have new fields: `Snapshot`, `LayoutOptions`, `Scene`,
  `SceneMark`, `SceneMembershipReport` and `AreaPlan`; `Request` has new variants. In `arqlet-query`: `Value::Map`, the
  `plan::Expr` variants `Call`, `Map`, `Index`, `Slice` and `Comprehension`, `plan::Op::Union` and
  `plan::Expand::rel_bound`. Overview: `Overview`, `OverviewPositions`, `OverviewRequest`, `OverviewStart` and
  `layout::overview::Previous`. In `arqlet-routing`: `Params::sparse_grid` and `SearchCounters::sparse_grids`.

### Added

- The cluster overview as a server result (provisional, #209): with `edgeData: true` the `overview` answer gives every
  edge's kind and weight (`edgeKinds`, `edgeKind`, `edgeWeight`; with `columns`, two more `u32` columns), for a host
  without the graph. `start.nodes` (`id → {x, y, r, parent?}`) continues the overview of a changed graph without a
  session: the clusters the overview added continue with their ids instead of the graph being clustered anew, new and
  moved nodes are placed anew among the others, and removed ids are reported (`continuation`). An entry's optional
  `parent` says what held the node, so that a cluster the host set smaller than its content keeps its members.
  Existing requests are answered as before. The typed `OverviewStart` now has `moved`, which the engine already took. In a session,
  `positionsOnly` answers a whole overview when `edgeData` differs from the last request's. For Rust users: `Overview`,
  `OverviewPositions`, `OverviewRequest` and `OverviewStart` have new fields, `layout::overview::Previous` has `anew`
  and `changed` (and implements `Default`).
- `sparseRepairGrid` (provisional, off, #203): on views over the repair grid's bound the default router first searches a
  sparse grid of the whole view (a line only in the gaps between boxes that matter, a cost per bend). On 24 generated
  views with containers and bus meshes the routes the router draws through boxes fell from 1 091 to 52, with fewer
  crossings, bends and shared lines in sum and 5 % longer routes; some views get more shared lines or import up to 13.7
  times slower (a second routing pass they now get), and line-node hits are not 0, so it stays off
  (`benchmarks/results/router-sparse-grid-20261126.md`). Views within the bound are unchanged. For users of
  `arqlet-routing`: `Params` has the new field `sparse_grid`, `SearchCounters` the field `sparse_grids`.
- **Galaxy style on a light background** (overview, provisional): a light clear colour (`background`, new
  `setBackground` for a host that switches themes, `tone`) draws the galaxy in ink instead of light — colours subtracted
  from the background, so clouds deepen where they overlap, darker stars, a dark selection ring, light name tags and dark
  texts. WebGPU, WebGL 2 and WebGL 1 draw it alike (`browser-checks/overview-backends.mjs`). Playground: *Light background*.
  For a dark drawing in a light page instead, `setEdgeFade(px)` (option `edgeFade`, helper `edgeFadeMask`) fades the canvas
  and its texts out at the edges with an eased CSS mask, so the page shows through. Playground: *Fade into a light page*.
- Lanes inside every container (provisional, ADR 0022 M4d): behind the parent hierarchy, or a dimension drawn as boxes, a second
  criterion `as: lanes` cuts every layered area into bands, the top level included; drawn inside each container. Playground:
  *Second structure as bands* (phases with *As lanes*, lanes inside the containers without).
- Phases (provisional, ADR 0022 M4b): with lanes first, a second criterion `as: lanes` lays its groups out one after the other
  along the flow, so that lanes and phases make a grid; drawn as bands with their names. Playground: *Phases along the flow*.
- Compact bookmarks that fit into a URL (provisional, #151): `controller.compactBookmark()` gives a view as `b1.…`
  (base64url), holding only what differs from the view as imported, every id once. `openBookmark` takes the text; on
  another revision of the graph it restores what still exists and reports `missing` ids and `ignored` decisions. Engine
  operations `compactBookmark` and `readBookmark` (read only). A 6 360-node view with 12 opened modules: 855 characters
  against 26 508 for the long form (`benchmarks/results/bookmarks-20261126.md`). The playground's *Link to this view*
  uses it; old links still open.
- `as: lanes` on the first criterion of `groupingPriority` (provisional, ADR 0022 M4a) lays its groups out as bands across the flow of
  the top level, in their declared order, instead of as containers. The scene draws them as bands
  (`membershipLanes`), the SVG viewer behind everything with the lane's name; playground *As lanes*. The report counts lanes out of
  place.
- `holdMemberships` (provisional, ADR 0022 M3b, off by default): the layout keeps a soft membership group whose shown members
  share a container together there, where that stays within a budget of crossings and bends. Snapshot `heldGroups`, report
  `held`; `holding` says why each other group is not held and what holding costs, edges against the flow included. An edit
  keeps the held groups. It costs a layout per group tried. Playground: *Hold groups together*.
- **`@arqlet/query`** (new npm package, provisional): the query-only WASM module with a loader and types — read-only
  openCypher queries over an Arqlet graph in the browser and Node, without the layout engine (1.16 MB, 0.37 MB gzip;
  `benchmarks/results/query-merged-20261125.md`). ADR 0021, M5c.
- **Query layer: functions and `UNION`** (ADR 0021, provisional): `id`, `elementId`, `type`, `labels`, `size`,
  `coalesce`, `toString`, `toUpper`, `toLower`, `trim`, `ltrim`, `rtrim`, `replace`, `substring`, `left`, `right`,
  `split`, `reverse`, and `UNION [ALL]`. `id` gives the model's string id. The query module grows by 2.2 % (gzip 2.4 %,
  `benchmarks/results/query-functions-20261125.md`). For users of `arqlet-query` as a library: `plan::Expr` has the new
  variant `Call`, `plan::Op` the new variant `Union`.
- **Query layer: maps and list expressions** (ADR 0021, provisional): map literals, list comprehension, `list[i]`,
  `list[from..to]`, `map['key']` and `+` on lists. The query module grows by 4.6 % (gzip 5.7 %,
  `benchmarks/results/query-expressions-20261125.md`). For users of `arqlet-query`: `Value` has the new variant `Map`,
  `plan::Expr` the variants `Map`, `Index`, `Slice` and `Comprehension`.
- Pins in the layered pipelines, behind `pinsInSinglePass` (provisional, off; ADR 0016 amendment, steps P1 and P2, #174):
  a pinned leaf or container of a layered area no longer takes the view off single pass, or off the hierarchical pipeline
  behind it.
  The pin chooses the node's rank and its order there and holds its place across the flow; along the flow the node
  stands in its rank's band, and the layout diagnostics say how far from the pin. Buses stay buses, and a drag that keeps
  the pin keeps the pipeline. A pinned container's inside is laid out as without the pin. Pins in organic, tree, radial
  and reversed areas are unchanged. The playground has a checkbox for it.

### Changed

- **Bus lines, faster:** where every bus of a view is drawn as lines (no bar left), the routes of the taps that run
  through boxes are no longer routed a second time; nothing draws or reads them (#150). A generated view with its
  one-bus containers collapsed imported in up to 5.5 times the time of the open view; it no longer pays for that pass
  (`benchmarks/results/bus-collapse-20261126.md`). The drawing is unchanged. `busTaps` (the routes, ports and border
  crossings kept for the taps) and `recomputedContainers` can differ, and so can one line of `layoutDiagnostics`
  ("Engine choice not tried"), which stays where that pass had left no route through a box. Views with bars are laid out
  as before, and stay slower collapsed than open.
- `arqlet-layout` (Rust): `compute_compared`, the layout for a drawing that is compared with another one.
- `openBookmark(bookmark, {force: true})` of another graph's long-form bookmark restores what the graph has and reports
  the rest (`missing`, `ignored`); before, one unknown id refused the whole restore. `openBookmark` also takes text.
- Membership hulls (provisional, ADR 0022): members that left a group's box are joined to it by a hull only where the hull
  stays compact, else marked (mark `ownBox` names the box). The moved-down parent hierarchy draws each container's
  children only, so nested levels no longer repeat one hull. In the SVG viewer hull names avoid each other and container
  titles, hulls of different dimensions are drawn a step apart, and marks sit at the bottom right, off the pin.
- Playground: the membership controls are a section of their own, *Groupings*, second in the side panel.
- **Query layer:** a map in a payload, in a session's content or in a parameter is a map; before, the payload
  and parameters refused it and the engine's query read it as null (`skippedValues`). In a JSON answer a map is
  `{"map": {…}}`.
- **Query layer:** a computed value (text or list) holds at most 16 MiB; beyond it a query stops with a limit
  error. Before, a short query could double a value until memory ran out.

### Fixed

- **Query layer, wrong answers up to 0.9.0** (#184): the `WHERE` of a `WITH` applied before the `ORDER BY`, `SKIP` and
  `LIMIT` of the same clause; it now filters what they leave.
- **Query layer, wrong answers up to 0.9.0** (#185): a label or property map on a variable written again was dropped when
  a later relationship of the same `MATCH` started at that node.
- `restore` (undo, redo, bookmarks) refused a view with a collapsed or partly shown community or group container
  ("unknown node"): containers that only the view has are accepted.
- `refine` panicked on a view with buses given as data where one bus is drawn as lines and another as a bar (organic,
  `busStyle: "lines"`; #197). An operation now takes back only what the layout had kept for the taps of the bars.
- Import and every later operation panicked (a resident session ended) on a view with a bus line where an edge or a tap
  leaves an organic, radial, tree or octilinear container of a layered area (`regionPorts`, the default; #204): the two
  layouts that place the border ports took the bus nodes out of the graph a second time. With `hubRefs` the same repeated
  step could take a region's box for a hub, so that the edges of its nodes were neither drawn nor listed as references,
  and it dropped the room between the ranks for the stubs whenever an edge left a region; such views are drawn
  differently now.
- **Query layer, wrong answers in 0.8.0 and 0.9.0:** a grouping expression used beside an aggregate could read
  null: `RETURN n.name AS who, [n.name, count(*)]` answered `[null, …]`. Grouping expressions now keep their value
  wherever they stand, and an expression that would read a variable without a value (also a property of a pattern that
  names a variable the pattern binds itself) is refused as unsupported instead of answered with null.
- **Query layer, wrong answers in 0.8.0 and 0.9.0:** a relationship variable bound by an earlier clause did not
  constrain a later `MATCH`: `MATCH (a)-[r]->(b) WITH r, a MATCH (a)-[r]->(c)` matched every edge of `a`. It matches the
  bound edge (or the bound list of a variable-length path) only. For users of `arqlet-query`: `plan::Expand` has the new
  field `rel_bound`.
- **Query layer:** `collect` stops at the value limit like the other ways to build a value, and the
  limit counts every value the same way; a node written before a relationship of its pattern can be read by the
  properties of what follows; a comprehension variable named like a variable outside the comprehension is refused (the
  parser could answer from a result column instead); a property or item of a comprehension variable is read without
  copying it. The playground shows maps in query rows.
- **Query layer:** the keys `id`, `kind`, `label`, `parentId` and `weight` are refused wherever properties come
  in beside the graph, as a session's `payload` already did (ADR 0021, M0): by `Payload::from_json` (and with it the
  query-only module and `arqlet-query`) and by a stateless `query` request with `content`. Before, such a key was
  accepted there: where the model has that property the model's value silently won, elsewhere the given one was read.
- A released drag (`move`, `finish` with `release`) of a pinned node kept the pin on a view whose options ask for single
  pass but whose graph it does not take (`can-bus`): the node stayed pinned where it was dropped. The pin is removed.
- `refine` failed on every view with a backbone bus (an explicit bus with several sources and several targets, as the CAN
  bus example; #205): where single pass did not take the view, because the options still asked for it ("Routing completion
  requires the legacy router …"), and where it did, because the check read the view without the bus's node ("missing
  dummy geometry"). It failed on an empty view too. A progressive refinement as the browser runs it
  (`dispatch_streaming`) read a view with bus lines without their nodes; it now answers as `refine` does. A view that
  fell back is refined as on the pipeline that drew it; as there before, the routing candidates may draw members of a bus
  apart when that removes conflicts.
- Snapshot of a view with backbone buses that single pass did not take: `options.singlePass` and
  `options.abstractRouting` are `false`, as on every other view that falls back (the `options` object is then written in
  full). The drawing of an import is unchanged. Such a view still tries single pass again at every edit that keeps a
  backbone bus in it (while `singlePassAuto` is on), and a released drag still keeps the drop's order. An edit that takes
  the last backbone bus out of the view (collapsing its members' containers) now stays on the pipeline that drew it, as on
  other views; `configure` with `singlePassAuto` tries single pass again. For users of `arqlet-layout`:
  `retries_single_pass`.
- A long-form bookmark (`bookmark`, `encodeBookmark`) of a view with a changed shape, direction, strategy, routing or
  port was refused on a new import of the same source as "The bookmark belongs to another graph" (#195). `openBookmark`
  and `readBookmark` (`sameSource`) now know it as the same source; bookmarks written by 0.8.0 and 0.9.0 included. As for
  compact bookmarks, a revision that differs from the bookmarked source only in declared shapes, directions, strategies,
  routing or ports is therefore no longer refused: the bookmark's values replace the new ones. `digest` is unchanged.

## 0.9.0 — 2026-11-24

A visual release: one appearance for every renderer and a metro-map style, the GPU scene renderer for large views, and
routing styles beyond orthogonal. A style (ADR 0026) colours parts and states from a theme and draws explicit buses as
metro lines with station dots; the layout strategy `octilinear` lays bus systems out as an octilinear star, and the
routing styles `octilinear`, `curve` and `bus` (fans as buses) are drawn, with bridges at crossings on request. The far
layer of large views draws with WebGPU (WebGL 2 as the fallback, ADR 0025), and the cluster overview shares it. Buses
given as data work everywhere (neighbourhood, overview, patches), a step back restores the geometry it left, phase
timings reach ELK callers, and dragging on a single-pass drawing keeps the edges at the node. Routes find their way
through narrow gaps in organic views, the overlap resolution behind it is much faster with the same output, and the
engine choice no longer routes large drawings a second time. Memberships (ADR 0022) are drawn in stages (whole, in
pieces, as marks) and a criterion can consider only some groups.

### Upgrading from 0.8

- **The engine version is `0.9.0`**: edits of a snapshot from an earlier engine are refused, so import the graph again.
  Default layouts are unchanged; the reference fingerprints are `fingerprints-20261124-default-0.9.0.jsonl` (they differ
  from the 0.8.0 file only in metadata; `layerweave-code`, which is generated from the source, was checked on the same
  graph against v0.8.0).
- **A bus given with `taps` ignores its `edges`** (#145): it is one shared medium for drawing, trace and neighbourhood.
  Before, a non-empty `edges` beside the taps silently stopped trace and neighbourhood at the bus. `edges` is optional.
- **The cluster overview shows buses given as data as bus nodes** (#147): their ids are nodes of the overview, as with
  `busLine` input.
- **The view controller's `back` and `forward` restore the geometry they left** (#148) instead of laying the view out
  again; `keepGeometry: false` keeps 0.8's behaviour, and the result says `relaidOut` when it had to lay out again.
- **Routes may differ from 0.8 on views that 0.8 drew with routes through nodes**: the route repair now leaves through
  gaps narrower than its stub (#158), and the engine choice is not tried above a work limit (#157). The reference views
  are unchanged.
- **Membership hulls are unfilled outlines above the container frames**, and a soft group may be drawn in pieces or as
  marks on its members (scene `marks`, hull `piece`/`pieces`, report `keptBox`); drawing only.

### Added

- A membership group that is still a box (a source container that kept some members, or a dimension's container)
  draws only the members that left it, joined to the box where that encloses nothing foreign, instead of falling into
  pieces and marks (provisional, ADR 0022 §3). The report counts them as `keptBox`.
- `consider` on a criterion of `groupingPriority` (provisional, ADR 0022): a dimension works with the listed groups only
  — one document among hundreds. As the first criterion only those groups become boxes and every other node falls to the
  next criterion; held softly only they draw hulls or marks. A member of a nested group belongs to the deepest considered
  group on its path. Refused on `parent` and for a group the dimension does not declare. Playground: *Only group*.
- `organicGap` (#156, provisional, off): the gap between the boxes of an organic area (default 24). On generated bus views
  with containers, 36 or 48 left far fewer routes through boxes and often took much less time, at more area
  (`benchmarks/results/organic-gap-20261123.md`); the default stays until the quality gates accept a change.
- Release workflow (#120): a tag `vX.Y.Z` checks that every version agrees (`tools/release-check.mjs`), runs the tests
  and the dry runs, then publishes the crates and the npm packages with trusted publishing (OIDC, no stored token; npm
  provenance) and creates the GitHub release from the CHANGELOG section. Versions already published are skipped, so a
  failed run can be run again; a manual run is a dry run. One-time setup on crates.io, npm and GitHub:
  `docs/releasing.md`.
- Steps back restore the geometry (#148, provisional): the engine session keeps earlier snapshots under marks (`keep`,
  `recall`; at most 32 by default, counted in `memory`), and the view controller's `back` and `forward` recall them, so
  every box and route comes back where it was instead of being laid out again. An evicted mark or a changed graph falls
  back to laying out the decisions again (`relaidOut: true`); `keepGeometry: false` keeps the old behaviour. Sessions
  without a resident engine keep the snapshots in JS.
- Routing style `bus` (#132, provisional): an area routed `bus` draws the fans of its edges as buses: the edges of one
  source share one port and one way until they branch off, then the edges left with one target (from two edges up).
  Choosing `bus` is the explicit request; `auto` and style lists never pick it, explicit buses come first, and the
  graph keeps no new bus (`Graph::effective_buses`). Drawn by the existing bus mechanism on the finished orthogonal
  routes, so a member whose shared way would enter a node or share a line keeps its own route; one diagnostic line
  (`Bus style: …`) counts them. Selectable in the playground's routing choices.
- Bridges at route crossings (#131, provisional, `appearance.bridges`, off): where two routes cross, the later one
  passes over the other in a small half circle, computed in the scene as points, so every renderer (SVG, both GPU
  backends, React Flow, the overview) draws it alike; members of one explicit bus never hop over each other, and the
  metrics see the routes without bridges. Playground: *Bridges at crossings* in the Display group.
- Metro stations: a style may draw a node as a glyph (`glyph: 'dot'`, `glyphSize`): a station dot in the middle of its
  box, its name outside, right of and below the dot, its lines running on to the dot's middle (straight routes from
  dot to dot) and no port marks; in renderSvg, the GPU far layer (also its labels from afar) and React Flow. The
  `metro` preset uses it for stations and interchanges.
- Layout strategy `octilinear` (provisional): an octilinear star for bus systems and metro maps: the most linked node
  in the middle, branches in the eight compass directions, every line (explicit bus or edge kind) straight through the
  nodes it passes, other branches bending off by 45°. The example *Vehicle buses as metro lines* uses it with straight
  routing: every bus a straight line through the central gateway. Selectable in the containment tree.
- The GPU far layer labels from afar: where a text is too small at its own size, container titles and node names are
  drawn at a fixed screen size where they fit and do not overlap (containers first, a title following the view inside
  a large container, a halo for contrast). Before, the far layer showed almost no text. The playground's *Style* field
  is in the canvas toolbar now.
- Playground example *Metro map (lines as buses)*: a made-up city's five metro lines as explicit buses, stations in
  order, interchanges where lines meet, routed octilinear; with *Style: Metro* it draws as a metro map.
- Playground example *Vehicle buses as metro lines*: the physical buses of a made-up vehicle as explicit buses running
  through their ECUs in the order of the wiring harness, the central gateway and the body controller as interchanges.
- Routing style `octilinear` (#129): the orthogonal route with its corners cut at 45° in the scene, clear of the nodes,
  drawn alike by SVG, both GPU backends, React Flow and the overview; selectable in the containment tree. With the
  metro style it gives a metro-map look.
- Routing style `curve` (#130): the route pulled taut, then a smooth centripetal Catmull-Rom curve through its points,
  sampled in the scene; a piece that would come too near a node stays straight. Drawn alike by every renderer;
  selectable in the containment tree.
- Appearance (ADR 0026, provisional): one style for every renderer in `@arqlet/viewer/style.js`: a theme of named
  colours (also from CSS custom properties, `--arqlet-…`) and rules with selectors over the data (`edge[kind=import]`,
  `leaf`, `#id`; colours as CSS, `$token` or `auto` by kind). `renderSvg`, the GPU far layer and `toReactFlow`
  (`data.look`) draw the same colours; presets `default`, `byKind` and `light`. Without a style the drawing is unchanged.
- Metro map style (preset `metro`): explicit buses as coloured lines without arrowheads, their nodes as stations, a node
  on several buses as an interchange. The style model derives `bus`, `buses` and `interchange` from the scene's buses
  (`bind`) and gains `[key]` selectors, `auto(field)`, the node property `radius` and `arrow: 'none'`.
- The cluster overview reads a style (`overviewBuffers(…, {style})`, `setStyle`): trace, route, line and bus colours from
  its theme, cluster colours from its palette; unchanged without one.
- The cluster overview draws with WebGPU (`createOverviewRenderer(…, {device})`, ADR 0025 O2), else WebGL 2, else WebGL 1,
  with every mode (galaxy, orbit, lens, breathing, morph) as before; the playground's overview page uses WebGPU where
  available.
- The cluster overview can draw structured clusters as the scene view does (`structured: 'scene'`, `setStructured`;
  ADR 0025 O3), with the style and the trace; the overview page has a checkbox for it.
- The GPU far layer morphs to a new scene (`setScene(next, {animate})`: boxes move by id, lines fade over, texts wait;
  ADR 0025 §6, O4); the playground's GPU view morphs on every new layout. The overview page zooms on into the scene view
  of a structured cluster and back without a jump (`sceneViewOf`, `overviewViewOf`; checkbox *Zoom on into the scene
  view*).
- Scene renderer for large views (ADR 0025 V1–V2, provisional) in `@arqlet/viewer`: a far layer on the GPU, WebGPU
  first and WebGL 2 as the fallback (`createSceneGpuRenderer`, `createSceneRenderer`, `sceneBackend`), drawing the
  same boxes, routes, arrowheads, hulls and marks as `renderSvg`, with texts on a 2D canvas and picking; and a scene
  view with both layers (`createSceneView`, `openSceneView`) that switches to SVG up close (`nearPolicy`, with
  hysteresis) without moving anything. Frame times while panning and zooming: `benchmarks/results/scene-frames-20261122.md`.
- The scene view's near layer is replaceable (`near`, `svgNearLayer`, `reactFlowViewport`; ADR 0025 V4). The playground
  has a large-view mode (button *GPU*, key G; on by itself from 1 500 elements) with selection, collapse and pins; the
  React Flow example can draw large views with the GPU from afar and React Flow up close (*GPU far layer*).
- The far layer draws the trace (path in amber, the rest receding), pin dots and a hover outline as `renderSvg` does;
  lines thinner than a pixel keep their coverage as opacity, as in SVG. `toReactFlow` returns membership hulls among its
  overlays (`hulls`), and the React Flow example draws all overlays.

### Changed

- Phase timings (#152): `timings` also reports the single-pass parts of the routing (`plan`, `channelGrowth`,
  `construction`, `fallbackRouting`, `finalization`), `buses`, and for `elkLayout` the translation (`elkTranslation`) and
  a refinement (`refinement`). The ELK drop-in takes `timings: true` and exposes them as `elk.timings` after each call.
- `sharedTracks`: when shared tracks make the lane orders cyclic and the layout falls back to one without them, the
  drawing is no longer completed a second time before that fallback; same result, 1.5 s instead of 1.9–2.0 s on a dense
  flat graph (`benchmarks/results/phase-timings-shared-tracks-20261123.md`).
- A soft membership group is no longer always one hull (provisional, ADR 0022 M2b): it is drawn whole where its hull
  encloses no foreign node, else in pieces — one per container that holds its members, then by distance within one —
  and a group that falls into more than three pieces is marked member by member instead (`Scene.membershipMarks`, a dot
  in its dimension's hue in the SVG viewer). Hulls carry `piece` and `pieces`; the report adds `split`, `marked` and
  `intrudersWhole` (what the group's whole hull would have enclosed). Drawing only: no node moves and no route changes.
  The threshold of three pieces is a placeholder until it is measured (M3).
- The engine choice (routing a drawing that still runs routes through nodes again with both routing engines) is not
  tried when its estimated work (routes × grid cells over the box edges) exceeds 1 million; the conflicts are reported
  with a diagnostic (#149). A meshed bus view of 100 nodes on 30 buses given as data, organic, took 20 to 42 s and now
  0.35 s with the same output (`benchmarks/results/engine-choice-work-20261123.md`); the stored default fingerprints are
  unchanged.
- Routing style `bus`: laid out by single pass like `orthogonal`, so the fans find their shared ways in its channels
  (on six public examples, fans drawn in part from 78 of 105 to 8); the `Bus style` diagnostic counts apart the fans whose
  edges leave to different sides.

### Fixed

- The SVG viewer drew membership hulls before the container frames, whose fill hid every hull inside a container. Hulls
  are now unfilled outlines drawn above the frames and below routes and leaves (provisional, ADR 0022).
- Playground: *Only groups* is a multiple select; several groups, of one dimension or several, set `consider` per
  dimension.
- Dragging on a single-pass drawing: the edges follow the node. A drag step moved only the node's box and kept every
  route, so the edges stayed at the old place until the drop; it now routes again what the move touches (the node's
  edges and the routes into its old or new box) around the drawing as it is (`arqlet_layout::preview_move`), which stays
  a single-pass one. The drop lays the view out as before.
- Route repair: where a route's end faces a neighbour closer than its stub (narrow gaps in organic views with
  containers), the repair tries shorter stubs instead of keeping the route through the boxes (#156). Generated organic
  view, 100 nodes with 11 containers: 54 of 250 routes crossed foreign boxes, now none; with buses and 240 children, 518
  of 1 159 routes, now 18. The overlap resolution became much faster with the same output (segment index by line,
  counts per move, cached moves): 117 s to 3.5 s on 120 children; dense views still cost more than before
  (`benchmarks/results/narrow-gap-repair-20261123.md`). Default fingerprints unchanged.
- Buses given as data (`Graph.buses` with `taps`): `edges` is optional, and a bus with taps is a shared medium whatever
  `edges` holds (#145; before, listing edges beside the taps silently stopped trace and neighbourhood at the bus);
  `neighbourhood` shows a reached bus drawn as lines (`busStyle: 'lines'`) by its lines, as a bar by itself (#144);
  a `patch` keeps such buses (it dropped every bus without edges, part of #149); the cluster overview draws their bus
  lines (#147).
- `reveal` with `more` (the view controller's `showRest`) on a collapsed container shows its first page instead of
  refusing it (#146).

## 0.8.0 — 2026-11-22

A view controller for hosts and less memory. `@arqlet/engine-client` gains a headless view controller (selection,
details, search and focus, bookmarks, keyboard navigation, a look into a collapsed container, refinement on a second
engine), and the playground runs on it. Memberships cross the hierarchy (ADR 0022): groups of nodes in dimensions of
their own, drawn as hulls, and a grouping priority that lets a dimension own the view's structure. A resident session
keeps content beside the layout, logs every change in one sequence, reports and gives back its memory, moves to a new
engine, and a backend limits its sessions by bytes (ADR 0023). The import peak of a large view fell by about 40 %
(byte-identical output). The Cypher query layer (ADR 0021) is published as provisional crates and runs in the
playground; the npm engine is built without it.

### Upgrading from 0.7

- **`membershipViews` is removed** (it was provisional): use `groupingPriority` (`{"team": "overlay"}` is
  `[{"by": "parent"}, {"by": "team"}]`). Options, `configure` requests and stored snapshots that still carry it are
  refused with a message.
- **Membership hulls are rectangles by default**: `membershipHull: "polygon"` gives 0.7's outline. Drawing only.
- **The engine version is `0.8.0`**: edits of a snapshot from an earlier engine are refused, so import the graph again.
  Default layouts are unchanged; the reference fingerprints are
  `fingerprints-20261122-default-0.8.0.jsonl` (they differ from the 0.7.0 file only in metadata).
- **The minimum supported Rust version is 1.94** (was 1.88).
- **New crates on crates.io** (provisional): `arqlet-query`, `arqlet-query-cypher`, `arqlet-query-model`,
  `arqlet-query-json`, reached through the cargo feature `query` of `arqlet-runtime` (off by default).

### Performance

- The import peak of a large view is about 40 % lower with byte-identical output (a synthetic 114k-node graph, first
  view of 100 boxes: 864 → 512 MB; 20k: 68.8 → 49.8 MB; counted heap, `benchmarks/session-memory`): large summaries get
  their room at once, a channel's ordering preferences are shared and stored as `u32` pairs, the placement's memos are
  released before the completion, and the placement works on shared ids (`arqlet_layout::id::Id`) instead of copied
  strings. Measurements: `benchmarks/results/session-memory-20261121.md`.

### Added

- A query field in the playground (provisional, ADR 0021): an openCypher query over the source graph and its content,
  run by the engine's `query` in every mode; the rows are listed, a row with a node focuses it, and "Select all" selects
  the boxes that show the nodes the answer names. The engine in the published npm package is built without `query`.
- A grouping priority (provisional, ADR 0022 M1): `groupingPriority` ranks the structures a view shows — the parent
  hierarchy (`parent`) and membership dimensions. The first criterion that assigns a node owns it, as its container in
  the view (`membership:<dimension>/<group>`); `values` decide which group of a multi-valued dimension owns a node in
  several; every later criterion is drawn as hulls, the parent hierarchy too once it is moved down. An organisation chart
  can thus be read by project instead of by reporting line. Projection only, the canonical graph untouched; the default
  (`parent` alone) is omitted, so default output is unchanged. `details` answers `groupings` per criterion; the scene's
  `membershipReport` has one entry per criterion (owner or soft, nodes owned, decided by the value order, unassigned).
  Playground: *Structure first*. A criterion's `hull` overrides `membershipHull`. Replaces the provisional
  `membershipViews` (see Breaking).
- A byte limit for a backend's sessions (provisional, ADR 0023 step R7): the sessions of a long-running
  `layout-command --sessions` process live in `arqlet_runtime::session_store::SessionStore`; with `--session-bytes N`
  the session that ran a message is weighed again (`EngineSession::heap_estimate`, cheap, no JSON text) and, while the
  sessions together weigh more than `N`, the least recently used one that holds something is dropped (never the one that
  just ran, never an empty one just opened); its next message is answered as an unknown session, and the answer of the
  message that caused it says `evictedSessions`. `{"sessions": "list"}` names the open ones. The playground backend sets
  2 GiB (`ARQLET_SESSION_BYTES`) and forgets the keys of dropped sessions.
- Moving a resident session to a new engine (provisional, ADR 0023 step R7): `session.recycle(newEngine)` exports the
  snapshot and the content from the old engine, loads them into a session of the new one and closes the old, so that
  the host can end the old worker and its memory comes back (WebAssembly never shrinks its linear memory). The view,
  the content with its revision and the change log's number move; the log's groups stay behind, so a replica resyncs.
  `load` takes `content` and `sequence` for it and reads a snapshot in one pass from text. A 3 000-node session laid out
  in full: linear memory 39.9 MB in the old engine, 9.8 MB in the new one, heap 3.9 MB.
- What a session holds, and giving it back (provisional, ADR 0023 step R7): `memory` answers the session's parts in
  estimated heap bytes (source graph, the structure's view and index, the view graph, boxes, routes, layered plans, the
  rest of the snapshot, content, change log, columns, overview), within 15 % below the counted heap; the WASM engine adds
  its live heap (`heapBytes`, counted) and its linear memory (`linearMemoryBytes`, never given back). `release` gives
  content (all or by key, logged as `dropKeys`), the change log, the columns and the overview back; the snapshot stays.
  The import trims the source graph's lists after selecting the view (a 114k graph: a session keeps 67 MB instead of
  89 MB); the change log is bounded by heap and keeps no group larger than its bound (a large payload is no longer held
  twice). JS: `session.memory`, `session.release`. `benchmarks/results/memory-release-20261122.md`.
- A change log in a resident session (provisional, ADR 0023 step R2): every change of the source, structure (`patch`)
  or content (`payload`), is a numbered group in one sequence; the new operation `change` applies a patch and a payload
  together as one group, all of it or nothing (`expectedSequence` refuses a stale one). `changesSince` answers the
  groups after a number, or `resync` when the log (bounded to 1 000 groups and 16 MB) no longer has them; `content`
  answers the whole content for a resync. A new import or `load` clears the log; the number goes on. Replaying the log
  on the snapshot it started from gives the same snapshot byte for byte and the same content (tested over seeded
  sequences). Memberships change through `patch` (ADR 0022 §5b): `memberships` adds or replaces dimensions and groups,
  `removeMemberships` removes a dimension or a group with the groups nested in it (removals first); a patch that changes
  memberships only moves nothing (boxes and routes stay, a new revision) unless a changed dimension owns nodes under the
  grouping priority, which is projected again. JS: `session.change`,
  `session.changesSince`, `session.content`; session answers carry `sequence` and `contentRevision`.
- Content in a resident session (provisional, ADR 0023 step R1): the operation `payload` sets, unsets or clears
  properties of nodes and edges (JSON values; an integer stays an integer) as one change with its own
  `contentRevision` (`expectedContentRevision` refuses a stale change). The content is kept beside the snapshot, never
  in it, and layout never reads it: serialized snapshots are unchanged. Ids the structure does not have are kept and
  counted; keys named like the model's own properties (`id`, `kind`, `label`, `parentId`, `weight`) are refused. `details`
  answers a node's or edge's `properties` and the `contentRevision`; `find` with `key` searches that property and answers
  the matched `value`. A new import starts the content empty, `load` keeps it. `session.payload`, `controller.payload`
  (refreshes `state.details`). About 740 B per node with a few properties
  (`benchmarks/results/content-memory-20261122.md`); typed columns per key are the next step if needed.
- `elkLayout` packs components as ELK does (`separateConnectedComponents`, default true, towards `elk.aspectRatio`,
  default 1.6, now read) when the top level has at least two components with edges and none holds more than half of
  the linked nodes. Flat graphs with many small components no longer stand in one tall column: synthetic flat graphs
  go from up to 4.5 to at most 1.9 times elk-rs's area, with fewer crossings (15 → 3 on one), and the 37 corpus views
  are unchanged (`benchmarks/results/elk-flat-components-20261121.md`).
- A resident session reads an import in one pass into its types, as `dispatch` already did, instead of a JSON value of
  the whole graph first, and trims the graph's lists to their length. The peak of the import's heap drops by a third
  to more than half (synthetic 2k/20k/100k: 8.2/130/1 184 MB to 2.4/69/886 MB), which in WASM is the memory that stays;
  a session keeps about 0.8 KB per node (`benchmarks/results/session-memory-20261121.md`, tool
  `benchmarks/session-memory`). Answers are unchanged. The rest of the peak is the layout of the first view.
- `reveal` takes the id of a bus given as data (`Graph.buses` with `taps` and no node): it opens the containers above
  the bus (`parentId`), so its line is drawn; with `partial` the bus's own container opens in part, its taps at the rest.
  Before, such an id was refused as an unknown node.
- A query-only WASM module (provisional, ADR 0021 stage M5c; `publish = false`): `arqlet-query-wasm` holds one graph with its payload (`load`) and answers queries (`query`) with the JSON the engine's `query` operation gives, without the layout engine: 1.08 MB (gzip 0.34 MB) in the `wasm-size` profile against 7.12 MB (1.84 MB) for the engine with `query` (`benchmarks/results/query-m5c-20261122.md`). The new `arqlet-query-json` holds the request and answer and is shared with the engine. `tools/query-wasm.test.mjs` runs the oracle's 40 supported cases through the WASM file. No npm package for it yet (ADR 0021, *M5c: the query-only module*).
- The operation `query` (provisional, ADR 0021 stage M5b; cargo feature `query` of `arqlet-runtime` and `arqlet-wasm`, off by default): an openCypher query over the source graph and the content of a resident session (ADR 0023, R1), also as a stateless `dispatch` request with `snapshot` and `content`. Answer: `columns`, `rows` (nodes as `{node: id}`, edges as `{edge: id}`), the ids once each, the size of the graph it saw, the content's revision, counts of content it ignored or could not read (a map value reads as null and is counted); `params`, `maxSteps`, `maxRows`, `maxHops` in the request; with `view: true`, `shown` maps each node to the box that shows it and each edge to the line that carries it. Errors read `query <class>: message`. `session.query` in `@arqlet/engine-client`. The playground build and the parity run (`tools/platform-parity.mjs`, `platforms.yml`) switch the feature on; native and WASM answer identically. A query sees the session's source graph, for a bounded import the selection it kept. With the feature the WASM engine is 12 % larger (`benchmarks/results/query-m5b-20261122.md`).
- The Cypher query layer runs queries (ADR 0021, stages M1 to M5a; provisional, `publish = false`, no `dispatch` operation yet): `arqlet-query` gains values, the store seam, the plan IR and a deterministic executor (three-valued logic, patterns with relationships, variable-length paths, `OPTIONAL MATCH`, `WITH`, `UNWIND`, `DISTINCT`, aggregation, `ORDER BY`, `SKIP`, `LIMIT`, checked integer arithmetic, a work budget in steps, a row limit and a hop limit); `arqlet-query-cypher` lowers decypher's HIR into the plan (the column order, which the HIR loses for aggregating queries, is read from the syntax tree); the new `arqlet-query-model` reads an `arqlet-model` graph in place with a payload table (unresolved edges are left out, payload for unknown ids is ignored and counted); the new tool `arqlet-query GRAPH.json 'QUERY' [--payload F] [--param N=JSON] [--json]` runs a query without the layout engine. All 40 supported cases of `examples/cypher/` give the oracle's rows, over a raw-JSON store and over the real model. Not yet: `UNION`, the power operator, a path variable, `RETURN *`.
- The first two crates of the Cypher query layer (ADR 0021; provisional, `publish = false`, no operation yet):
  `arqlet-query` (error classes) and `arqlet-query-cypher`, which checks a query with decypher and rejects, from its
  syntax tree, what decypher drops silently (`shortestPath`, quantified paths, `WHERE` inside a pattern: upstream #132 to
  #134), writes, `CALL`, subqueries, `=~` and unknown functions. A test runs the guard against the fixtures of
  `examples/cypher/`. Both crates are usable without the layout engine; `tools/check-query-isolation.mjs` checks it in
  CI, for the host and for wasm32. (The plan IR and the executor followed; see the entry above.)
- Memberships across the hierarchy, first slice (provisional, ADR 0022): `Graph.memberships` (dimensions of groups,
  besides the containment) and `LayoutOptions.membershipViews`; a dimension shown as `overlay` is drawn as a padded
  convex hull behind the nodes (`Scene.membershipHulls`, with the boxes it encloses and the foreign leaf boxes inside
  it), with a typed report per dimension (`Scene.membershipReport`). Nothing moves: boxes and routes are identical with
  and without hulls. Refused: duplicate dimension or group ids, an unknown or cyclic `parent`, `groups` together with a
  `hierarchy` view; unknown member ids are ignored and counted. `hierarchy` and `lanes` are accepted and reported as not
  yet applied. SVG viewer and playground (*Membership hulls*, demo *Memberships*).
- A look into a collapsed container (provisional): the read-only operation `peek` (`nodeId`, optional `limit`, default
  50) lays out the container's contents on their own as a small scene (the containers inside collapsed, at most `limit`
  direct children in the source order, the rest counted) and counts per child the edges that cross the container's
  border (`external`); the view is not changed. `session.peek`; in the view controller `peek(id)` (`state.peek`, until
  `closePeek` or the next change; not a step for `back`). Playground: *Look inside* (or Space) for a selected closed
  container.
- Refinement on a second engine (provisional): `createViewController(session, {refine})` takes a second engine for
  refinement (`workerRefineRun(worker)` for its own worker, or any `(request, {onPublication, signal}) => answer`).
  `refine({op})` (`refine`, `refineProgressive`, `structuredRefine`) runs beside the interactive steps, shows
  intermediate results throttled (`publicationInterval`), applies a result only while the view is still the one it
  started from (else `state.refined.stale`, counted in `state.discarded`), and is stopped by any change, step back,
  import or new refinement (`state.refined.cancelled`); `state.refining` names it meanwhile; `cancelRefinement()`. Not a
  step for `back`. The playground's whole-view refinement runs through it on the mode's refinement lane.
- Keyboard navigation (provisional): the view controller's `navigate(direction)` moves the selection to the nearest box
  that way (`left`, `right`, `up`, `down`), up to the container or to the first child (`parent`, `child`), between
  siblings in reading order (`next`, `previous`), or along a drawn edge (`out`, `in`), and sets `state.focus` for the
  camera; nothing opens, and it is not a step for `back`. `navigateScene(scene, from, direction)` is the pure,
  deterministic rule. Playground: arrows, Shift+arrows, `[` and `]`, Enter to open or close.
- Search, focus and bookmarks (provisional). The read-only operation `find` (`query`, optional `limit`, `nodeKinds`)
  finds nodes of the source graph by label or id (ignoring case; exact, then prefix, then word start, then anywhere),
  also inside collapsed groups, with the containers above each and the box that shows it (`FindAnswer`); `digest`
  answers the fingerprint of the snapshot's source graph. In the view controller: `find` (`state.search`), `focus(id)`
  (opens what hides the node as one undo step, selects it, `state.focus` with the bounds for the host's camera),
  `bookmark()` (the view as a value: intent, highlight, selection, graph digest) and `openBookmark(bookmark)` (one undo
  step; another graph's bookmark is refused unless forced), `encodeBookmark`/`decodeBookmark` for URLs. Playground:
  *Find* with a list of matches (a click focuses), *Link to this view* (`#view=…` in the address, opened on load).
- View details (provisional): the read-only operation `details` (`nodeId` or `edgeId`, optional `limit`) answers what
  the engine knows about a node or an edge and a host cannot see in the scene: the containers above a node, the box
  that shows it (itself, the collapsed container it is in, or none) with its absolute bounds, what a box holds
  (children, all nodes inside), its edges in and out; for a drawn line its visible ends, length, bends, bundle, and the
  edges of the source graph it stands for in its own direction and in the other (at most `limit` ids each, counts
  complete); for an edge of the source graph the line that carries it. Typed (`DetailsAnswer`), in sessions and in
  the typed protocol. `session.details(target)`; in the view controller `select(target)` keeps `state.details` up to
  date after every change, and `hover(target)` holds what the pointer is over (no engine call). Domain attributes stay
  with the host. The playground shows the details of the selected node or edge.
- A headless view controller in `@arqlet/engine-client` (provisional, request 31): `createViewController(session)` keeps
  what a user does between two scenes in one frozen state object (`scene`, `diff`, `busy`, `error`, `highlight`,
  `hide`, `canBack`, `canForward`) with `subscribe`, so a React binding is `useSyncExternalStore(view.subscribe,
  view.getState)`. Operations: `import`, `open`/`close`/`toggle`, `showRest` (the next page of a `+N more` box),
  `reveal`, `filter`, `trace`/`neighbourhood` kept as the highlight (asked again after every change) until
  `clearHighlight`, `revealHighlight`, `back`/`forward` over the view's decisions (the engine's `intent`/`restore`, one
  step each), `apply` for any other change, and `cancel` (drops the steps not started). Every step carries the
  difference between the scenes before and after it (`sceneDiff`: nodes and routes added, removed, changed). Also from
  `arqlet`. Not yet: refinement on a second worker.
- The view controller takes snapshots a host made itself (`adopt`: a drag, a refinement, a loaded document; with
  `remember` as an undo step, with `forget` for another graph), merges a follow-up into the last step
  (`apply(..., {merge: true})`), applies steps that change no decision without remembering them (`remember: false`),
  names the next steps (`backLabel`, `forwardLabel`) and can leave out the scene (`scenes: false`, for a host that draws
  from the snapshot). An import or a forgetting `adopt` no longer waits for the steps still under way; they resolve as
  cancelled. `session.load(snapshot)` continues a layout session from a snapshot.
- Third-party notices in the engine package: `@arqlet/engine-client` now ships `THIRD_PARTY_NOTICES.md`, the copyright and
  license notices of the Rust crates linked into its WASM (15 crates today: `serde`, `serde_json`, `libm`, `sha2` and their
  dependencies), generated by `tools/npm-prepack.mjs` through `tools/third-party-notices.mjs`. The packages carried
  only the project's own two license files before. A crate without a license file fails
  `node tools/third-party-notices.mjs --check`, which CI runs as a test.

### Breaking (provisional)

- `membershipViews` is removed, replaced by `groupingPriority` (ADR 0022, M1): `{"team": "overlay"}` is now
  `[{"by": "parent"}, {"by": "team"}]`; the modes `hierarchy` and `lanes` became a dimension listed first and `"as":
  "lanes"`. Options, `configure` requests and stored snapshots that still carry `membershipViews` are refused with that
  message, not read as if it were absent.

### Changed

- Decided: `elkLayout` keeps applying only the spacings a graph names. ELK's default spacings as the adapter's own were
  compared on 39 datasets and not adopted (two lines through nodes, crossings +2.9 %, bends +8.6 % for about 9 % less
  area; `benchmarks/eval/elk-spacing.mjs`, `benchmarks/results/elk-spacing-20261122.md`).
- `elkLayout` passes on the diagnostic of `sharedTracks` when the layout was computed again without shared tracks (lane
  orders cyclic for many edges), as it already did for a single pass that was not used.
- Membership hulls are rectangles by default (provisional, ADR 0022 §3): the padded bounding box of a group's boxes,
  instead of the padded convex polygon drawn so far. `LayoutOptions.membershipHull: "polygon"` gives the previous
  outline. A rectangle encloses at least as much as the polygon, so a view with overlays may report more intruders.
  Drawing only: no node moves and no route changes; views without `membershipViews` are unchanged.
- The playground runs its view through the view controller: every change, the undo stack (now also covering the
  three modes through `execute`), drags and refinement results handed over with `adopt`.
- The remembered state (`intent`) keeps the filter (`hidden`), so a step back undoes a filter.
- The minimum supported Rust version is 1.94 (was 1.88). The rule: the minimum follows what the dependencies need and
  stays well behind stable, so that toolchains pinned by companies, distributions and CI images keep working (1.94, not
  1.100: nothing needs 1.95 to 1.100; the parser planned for the Cypher query layer, ADR 0021, needs 1.94). A CI job
  (`msrv`) checks the workspace, and the WASM adapter for `wasm32-unknown-unknown`, with that exact version. Edition
  2021 is unchanged; a change of edition is a separate decision. Crates published earlier keep the version they
  declared.

## 0.7.0 — 2026-11-21

Large views read faster and denser: dense flat graphs and graphs with long edges and polyline routing lay out several
times faster, with the same drawings (pure speedups) or with single pass now also for polyline routing. Views that start abstract lift edges between containers and grid large unlinked areas by default, and
the first-view budget holds. Buses can be given as data (`Graph.buses` with `taps`) and are navigable, drawn as lines
without bars if wanted, clear of foreign boxes, and orthogonal in organic areas. New for hosts: paging through
`+N more` with a step back, child counts on scene nodes, phase timings on request, a typed drag API, and bus lines,
junctions and hub references from `toReactFlow`. Hub references draw the edges of hubs as stubs; shared tracks
(experimental) let the edges into one node share a track. The WebGL overview gains bus lines, dragging, a galaxy style,
orbit, adaptive dimming and names, and builds in steps.

### Upgrading from 0.6

- **The default drawing changes** in three places. Views that start abstract (`levelOfDetail` or `communities` set) lift
  edges between containers (`liftEdges`) and lay out areas of at least 50 unlinked children as a grid
  (`isolatedGridFrom`); graphs with polyline routing are laid out in single pass (`singlePassPolyline`). To keep 0.6's
  drawings, set `liftEdges: false`, `isolatedGridFrom: 0` and `singlePassPolyline: false`. The first-view budget
  (`levelOfDetail.nodes`) now counts an opened container as a box plus its children, so a budget opens fewer
  containers than in 0.6; raise it to keep a first view. The reference fingerprints are
  `fingerprints-20261121-default-0.7.0.jsonl` (orthogonal views without an abstract start are unchanged).
- **The engine version is `0.7.0`**: edits of a snapshot from an earlier engine are refused, so import the graph again.
- **The WASM module imports `env.lw_now`** (the host's clock for `timings: true`). `@arqlet/engine-client`'s loader
  provides it; a host with its own loader must add `lw_now: () => performance.now()` to the `env` imports.
- **Additions to the formats**, all omitted when empty or default: `Snapshot.hubRefs`, `Scene.hubRefs`,
  `SceneNode.childCount`/`hiddenCount` (columns `nodeChildCount`/`nodeHiddenCount`), `Intent.partial`, the options
  `hubRefs`, `sharedTracks`, `singlePassPolyline` (written only as `false`) and `communityHubs.top`, the request fields
  `timings` and `reveal.more`, and the operation `pin`. Rust: `LayoutOptions::lift_edges` is an `Option<bool>`
  (`lifts_edges()` gives the effective value); exhaustive struct literals of `LayoutOptions`, `SceneNode`,
  `CommunityHubs` and `Intent` need the new fields (or `..Default::default()`).

### Changed (default)

- **Polyline routing in single pass** (`singlePassPolyline`, on by default; `false` restores the fallback): `polyline` and
  `roundedPolyline` are laid out orthogonally and pulled taut by the scene, as before, but a graph with them no longer
  falls back to the pipeline before single pass. On 36 benchmark datasets with polyline routing: crossings −16 %, bends
  −2 %, time −29 %, no route through a node; workflow-dag gets its orthogonal single-pass drawing (crossings 136 → 553).
  Orthogonal output unchanged. Measurements: `benchmarks/results/polyline-single-pass-20261121.md`.

### Added (experimental)

- `sharedTracks: "target"` (provisional, off by default; request 27): in the last channel before a node, the edges into
  that node share one track (one bend per group in the coordination, segments of a group may overlap), and the placement
  makes room for the shared tracks only. When the shared bends make the lane orders cyclic for many edges (groups
  interleaved in narrow gaps, as in a dense flat graph), the layout is computed again without groups (diagnostic). 41
  datasets: no error, no route through a node;
  18 change (area 94–100 %, crossings +0.4 % and bends −3.6 % on the smaller ones). Measurements:
  `benchmarks/results/shared-tracks-20261121.md`. Output without the option is byte-identical.

### Performance

- The rule that bends of one channel keep a track apart is answered by a range query instead of listing every pair of the
  channel (1.49 million on a dense graph); the output is byte-identical. Measurements: `benchmarks/results/channel-overlaps-20261121.md`.
- Indexes instead of scans by edge and node id, and a channel's slots by a range query over its lanes; output
  byte-identical (same file).
- Dense channels: the order of a channel's lane changes is found without copying the order or reading a full cost
  matrix along columns, and a channel's cross-flow pairs are built into their map at once; the output is byte-identical.
  Measurements: `benchmarks/results/dense-channels-20261121.md`.
- The constructive search finds the lane changes of a corridor in two sweeps instead of a scan from every lane (linear
  instead of quadratic in the lanes); the output is byte-identical. The default single pass does not use this search and
  is unchanged. Measurements: `benchmarks/results/lane-sweep-20261121.md`.

### Added (phase timings)

- `timings: true` on any request (provisional) adds `timings` to the answer, in ms: `total`, `layout` (every layout in
  the request) and its phases `placement`, `organicStress`, `separation`, `repair`, `portResolution`, `routing`; through
  `dispatch` and resident sessions alike. The core reads no clock of its own: the host installs one
  (`arqlet_model::clock`, re-exported as `arqlet_runtime::clock`); the native binaries use `Instant`, and the WASM module
  imports `env.lw_now` (the loader `wasm-client.js` passes `performance.now`). A host with its own loader must provide
  that import now. The times are only reported: the output is the same with and without (15 fingerprint views
  identical).

### Added (scene counts)

- `SceneNode.childCount` and `hiddenCount` (provisional): a container's children in the view (added communities and
  groups count; a partly shown container counts those in its rest too) and how many of them are not drawn (all of them
  when collapsed, those in the rest when shown in part); omitted for leaves and when every child is drawn. A host can show
  "12" on a collapsed box without a copy of the graph. The columnar scene carries them as `nodeChildCount` and
  `nodeHiddenCount` (`NONE` when absent).
### Added (drag API)

- A typed drag API in the session (provisional): `session.drag(nodeId, position, {others})` returns a handle with
  `move(position)` (positions that arrive while a step is computed replace each other; only the newest is sent),
  `drop({release, settle})` (the node stays pinned where it was dropped, or released; `settle` lets force-directed areas
  settle afterwards) and `cancel()` (back to where it was), plus `active`, `stats` and `begun`; `session.pin(nodeId,
  position?)` and `session.unpin(nodeId)`. Positions are in the parent's frame, as `toReactFlow` gives them. Works with
  resident sessions and with a snapshot sent per request.
- Engine operation `pin` (provisional): `{nodeId, position?}` pins a node (default: where it is), `{nodeId, unpin: true}`
  releases it; through `dispatch`, so in all three deployment modes (backend allowlist updated).
- `toReactFlow(scene, {draggable: true})` marks leaves draggable (containers never), for a host that passes React Flow's
  drag events to `session.drag`.

### Added (React Flow)

- `toReactFlow(scene, {overlayNode})` returns what is neither a node nor an edge between nodes, in flow coordinates:
  `busLines` (bars and their taps), `junctions` (where shared bus lines branch) and `hubRefs` (stubs and labels of hub
  references); before, a React Flow host lost them. With `overlayNode: true` one inert node of type `arqletOverlay` over
  the scene's bounds carries them (`data.origin` turns flow into node coordinates), so a host draws them with a node type
  instead of a viewport portal. Types in `react-flow.d.ts`.

### Fixed (first-view budget)

- `levelOfDetail.nodes` counts the boxes as drawn: an opened container stays a box (its frame) and adds its children;
  before, it was counted as replaced by them, so every opened container added one box over the budget. A first
  view that cannot keep its budget (the top level alone is larger, or its edges are) says so in `layoutDiagnostics`
  ("First view: N boxes, M over the node budget of B: …"). The older `initial_collapse` (import `limit`) is unchanged.
### Fixed (bus lines)

- Bars and taps keep clear of the boxes: a bar that would lie over a box that is neither a bus, a member nor a container
  around it moves to the nearest free lane; a tap that would pass another box (another member of its bus too) takes
  another place along the stretch it shares with the bar, bends around it (an L or a Z), or follows a short orthogonal
  search on a grid of the nearby boxes' edges. In an area that is not layered, `busStyle: trunk`/`lines` lays a bus out
  as a bar and turns it into shared lines along the bar's middle (no diagonal lines any more). Issues found on the taps'
  hidden layout routes, which are never drawn, are dropped. Default output unchanged.

### Added (bus lines)

- `busStyle: "lines"` (provisional): every bus as shared lines with junctions, never a bar; a bus with no member above
  it in its bus tree (a root bus, a backbone) takes the member on the most buses as its head. `trunk` and `lines` now
  also draw buses given as data (`Bus::taps` without a node). Playground: Routing → *Buses*.
- A tap at a member hidden in a collapsed container attaches to that container, once per bus and container (the view
  projection remaps the taps; a bus inside a collapsed container is hidden with it). Before, such buses fell back to a
  bar; now none does in `trunk` or `lines`, with bus nodes or with taps-only input.
### Changed

- **Default:** in views that start abstract (`levelOfDetail` or `communities` set), an area with at least 50 children
  without an edge to a sibling lays them out as a grid (`isolatedGridFrom` unset now means 50 there; `0` turns it off).
  Opening a container of many unlinked children inside a community gave one tall column, now a grid. Views without an abstract start are unchanged (15 fingerprint
  views identical). `LayoutOptions::isolated_grid_threshold()` gives the effective value.
- **Default:** `liftEdges` is on in views that start abstract (`levelOfDetail` or `communities` set, `liftEdges`
  unset): edges between opened containers are drawn between the outermost containers they differ in, one route per
  pair. `liftEdges: false`
  keeps the earlier drawing; views without an abstract start are unchanged (15 fingerprint views identical).
  `LayoutOptions::lift_edges` is an `Option<bool>` now; `lifts_edges()` gives the effective value.

### Added

- Buses as data (provisional): a bus given in `Graph.buses` with `taps` and no node of its own, as a host holds bus
  membership, is crossed by `trace` and `neighbourhood` as a shared medium (as a `busLine` node is), and `filter` hides
  it by its id or hides a member's tap (`arqlet_layout::with_bus_nodes`). Documented on `Bus::taps` as an input.
- Paging through `+N more` (provisional): `reveal` with `more: {container, count}` shows the next `count` children of a
  container shown in part, in the graph's order; on the last page the container is whole again, without its rest. The
  playground shows the next 12 on a click on a `+N more` box (Shift: all at once, as before).
- A step back over a reveal: the remembered state (`intent`) keeps the containers shown in part (`partial`), and
  `restore` takes them and their rests (`rest:<container>`), so undo returns to the page before; before, a restore
  after a partial reveal was refused ("unknown node").
- `communityHubs.top` (provisional, also the overview's `hubs.top`): a relative hub threshold, a share of a container's
  linked children with the most links (`0.01`: the top 1 %, at least one; ties by the children's order), beside the
  absolute `minLinks`, so a threshold scales with the container instead of leaving no community at all.
- `hubRefs` (provisional layout option, unset): an edge of a node with at least this many edges in the view is not laid
  out or routed but drawn as a short stub at its other end, named after the hub (out of the right side for an edge to
  the hub, into the left for one from it), as a wiring diagram refers to another sheet (`Snapshot.hubRefs`, scene
  `hubRefs`; the SVG viewer draws them, a trace marks them). The channels between ranks keep room for the stubs and
  names. Not yet: grouping the
  stubs of one node, a hub's own side of the references.
- Adaptive edge dimming in the WebGL overview (`edgeDimming`, on): the renderer estimates how many edge lines lie over a
  pixel on average in what is visible (their length on the screen over the area) and makes each line fainter the more
  there are, so thousands of overlapping lines no longer add up to a white tangle in the galaxy style; zoomed in, they
  come back.
- Names of nodes when zoomed in (provisional, `labelsOf` options `names`, `details`; `renderer.setNames`): every node
  whose disc is 6 pixels across or more on the screen (zoomed in, or near in the orbit) is named beside it; with details
  also its kind and how many edges it has (`degreesOf`). Overview page: *Names of nodes when zoomed in* (on), *… with
  kind and links*.
- Orbit, a pseudo-3D view of the WebGL overview (provisional, `renderer.setOrbit({yaw, pitch, depth})`): every point
  gets a depth from a smooth field at the scale of the clusters (what lies close together, a galaxy, lies at a similar
  depth) plus a little of its own, the drawing turns around the view's centre and is seen in perspective, so clusters lie
  at different depths and pass each other when it turns; the same in both vertex shaders (lines stay on their stars) and
  in JavaScript for picking and labels (`orbitDepth`, `orbitPoint`). Under the orbit a drag turns the drawing and
  Shift-drag pans. Overview page: *3D orbit*. The layout stays two-dimensional.
- Cosmic motion in the galaxy style (provisional): when idle, instead of breathing, every point turns a little around the
  drawing's centre by a slowly oscillating angle that differs with its distance (the galaxies seem to rotate and shear
  against each other; bounded, it never drifts), the same field for stars and line ends so lines stay on their stars,
  and the stars twinkle on phases of their own; it fades in and out with the idle breath. Galaxy lines are fainter and a
  darker shade of their cluster's colour. In the overview page a collapsed cluster can be dragged directly.
- Overview built in steps (provisional, overview page, *Rough overview first, then bloom*): for a graph with a
  containment hierarchy larger than 600 nodes, a rough overview of its top levels is laid out first (at most 600 nodes; a
  cut container stands for what it holds with a radius for its leaves, and only the strongest lifted edges, two per node)
  and shown at once (natively a few hundredths of a second for the synthetic code indexes of 10 000 to 100 000 nodes,
  whose whole overview takes 0.3 to 2.7 s on a heavily loaded machine, see
  [results](benchmarks/results/overview-progressive-20261120.md));
  when the whole overview is done, every node starts at its rough ancestor's place and moves to its own, so the clusters
  bloom out of the rough picture. `apps/playground/progressive.js` (`coarseGraph`, `bloomStart`), tested.
- Galaxy style of the WebGL overview (provisional, `createOverviewRenderer(..., {galaxy})`, `setGalaxy`): clusters as
  clouds of light (a Gaussian glow, fainter the deeper the cluster, so nested clouds add up towards their cores), a
  collapsed cluster as a brighter glow, every leaf as a star (a bright core in its own glow), every edge as a faint line in
  its source cluster's colour; light is added (additive blending), so where many edges and nodes meet the drawing grows
  brighter. Positions are the overview's as before. Overview page: *Galaxy*. In the galaxy style a cluster is named in
  its middle on a dark tag with a dot of its colour (`labelsOf` option `galaxy`, labels carry `colour`); a single root
  around everything is left unnamed. Turning the style on in the overview page also loosens the forces (gravity 0.05,
  repulsion 0.1, gap 3; fields the user changed are kept), so the clusters fray out into clouds; turning it off gives the
  defaults back.
- Bus lines in the cluster overview (provisional): a node of shape `busLine` is a line instead of a disc
  (`Overview.busLines`, also in `positionsOnly` answers and the columns' description): horizontal or vertical, whichever
  its members in its own cluster spread more along, through their centroid, from the first to the last one's
  projection; every member taps it at its projection (a member in another cluster at the line's end). No roots or
  trigonometry, the same on every platform and for any thread count. The WebGL overview draws thick orthogonal
  connectors instead of the disc and the edges to it: the line, from each member a stub at its rim that runs at a right
  angle to the line (an L to the end for a member beyond it), a square junction at the line; amber on a path through the
  bus, white when selected; a bus is picked by its bar where no disc is
  (`pickNode`, new `busLinesOf`) and named above the middle of its bar. The overview page offers *Vehicle network, bus lines* (371 nodes, 40 buses,
  made up, `benchmarks/generate.py vehicle_network`) and the two vehicle examples.
- `busRows` (provisional overview option, off): after the forces, the members of every bus line that a cluster holds
  stand in one row on the bus's line (across x or y, whichever they spread more along, in the order they lay), the
  bus's own disc beside the row; the separation then makes room for the other children. The same for any thread count.
  Overview page: *Members of a bus in a row on its line*.
- Moving a node in the overview (provisional): `start.moved` (node id → new centre) lays the overview out from the
  previous places with each moved node held where it was put; its cluster settles around it with cool forces (d3's
  fixed positions; the others follow and make room without arranging themselves anew) and is not centred again.
  A moved cluster takes what it holds along. `bindOverviewNavigation` takes `onNodeDrag(i, x, y, phase)` and
  `draggable(i)`: by default a drag that starts on a leaf moves it (`move`, `end`), any other drag pans. The overview page
  moves the selected cluster with all it holds when a drag starts inside it, else a leaf, and previews the move under the
  pointer; it settles at the release.
- Neighbours follow while dragging in the overview (provisional): the overview page sends the latest pointer place
  while dragging, one request at a time, and draws each answer (the next one settles on from there), so the cluster gives
  way during the gesture; *Neighbours follow while dragging* turns it off. A cluster whose child was moved (or that holds
  one) is centred on its children again and its parent moves it as far (`Arrangement::shift`), so the moved node keeps
  its place while a node dragged out of its cluster no longer inflates the cluster's disc around its old centre.

## 0.6.0 — 2026-11-19

Physical buses and large graphs. A node of shape `busLine` (CAN, LIN, FlexRay, Ethernet) becomes a bus line of the view:
a bar past its nodes with square taps, or with `busStyle: "trunk"` a shared line from its head; bus networks read as
trees from their gateway or their middle, stay trees when zones collapse, and traces cross them. Large graphs start
abstract and stay responsive: a first-view budget (`levelOfDetail.nodes`/`edges`), lifted edges between containers,
columnar scenes for sessions, a shared prepared source, and indexed projections (a 1 000 000-node import 22.5 s → 10.6 s,
a toggle in its session 8.5 s → 2.5 s). Hosts can precompute communities and imports as library calls and leave the
source graph out of the answers (`omitSource`). New defaults give stacked containers and loops their room and place
organic areas with forces; an organic area above 300 children is laid out instead of refused.

### Upgrading from 0.5

- **The default drawing changes** where the new defaults act: `sharedBorderRanks`, `alignStacked`, `loopSpace` and
  `organicForces` are on (see *Changed*). To keep 0.5's drawings, set the four options to `false`; they are written into
  a snapshot only when `false`. The reference fingerprints are `fingerprints-20261119-default-0.6.0.jsonl`.
- **The engine version is `0.6.0`**: edits of a snapshot from an earlier engine are refused, so import the graph again
  rather than continuing a 0.5 snapshot.
- **An organic area above 300 children is laid out** (by the scalable stress, up to 2 000) instead of refused, and the
  snapshot records `scalableStress`.
- **Additions to the formats**, all omitted when empty or default, so earlier documents read unchanged: the node shape
  `busLine` (columns code 3); `Bus.taps`, `Bus.kind`, `Bus.parentId`; `Snapshot.busLines`, `Snapshot.busTaps`,
  `Snapshot.sourceDigest`; `Scene.busLines`; the options `busStyle`, `isolatedGridFrom`, `liftEdges`,
  `routeSearchWindow` and `levelOfDetail.nodes`/`edges`. Rust: exhaustive struct literals of `LayoutOptions`, `Bus` and
  `Snapshot` need the new fields (or `..Default::default()`); `Snapshot.source_graph` is an `Arc<Graph>`.

### Added

#### Bus lines

- Node shape `busLine` (provisional): a physical bus (CAN, LIN, FlexRay, …) as a node keeps its box for the layout and is
  drawn as a bar past the nodes its edges attach (`Snapshot.busLines`), with the edges as square taps onto the bar. The
  bar lies across the flow of its area, or along the axis its nodes spread on; it stops short of other boxes and of a node
  on its own line; bars that would overlap take parallel lanes away from their nodes. Applied on every snapshot an
  operation answers, so refinement and edits keep it; the scene, the columns, the SVG viewer and the playground's shape
  menu know it. Examples *Vehicle buses (bus lines)* and *Vehicle buses, organic* (made up).
- A network of `busLine` nodes reads as a tree (provisional): for the layering, the bus edges are turned so that in every
  connected network the node on the most buses (the earliest in the graph on a tie) is the root, and every bus edge
  points from the end a breadth-first search reaches first to the one it reaches later. A gateway stands above its
  buses, a node on one bus below it, a node on two buses below both; the edges keep their logical direction in every
  output. In *Vehicle buses (bus lines)* the central gateway heads its three buses and the body controller its LIN bus.
- `trace` follows the edges of a `busLine` node both ways (a bus is a shared medium), so a path runs node → bus → node;
  other edges keep their direction.
- Bus lines are a view (provisional, ADR 0019 view model): the source graph keeps a `busLine` node with its edges; the
  view (`Snapshot.graph`) leaves them out and keeps a `Bus` with `taps` (one per edge: edge, node, direction, kind) and the
  bus's label, kind and parent. The layout sees the bus as a node again; `Snapshot.busLines` holds each bar and its taps'
  points (absolute, in each edge's direction), `Snapshot.busTaps` the taps' layout state for the next operation, the bus
  box stays in `busBoxes`. Every operation continues the layout with the bus nodes and answers the view
  (`expand_bus_lines`/`apply_bus_lines`). The scene has `busLines` (id, label, kind, bar, taps) instead of a node with a
  bar; the columns carry them, the SVG viewer draws them (selectable, traced by the bus's and the edges' ids), and a trace
  or neighbourhood marks the bar and taps it runs over.
- Root of a bus tree (provisional): a node on three or more buses (a gateway) as before; without one, the middle of the
  network (the member whose farthest member is nearest; above 2 000 members the middle of a longest path), which may be a
  bus. A trunk without gateways stands on top with its branches below; zones hang below their backbone, and a unit on two
  zone buses below both. Examples *Vehicle trunk bus (no gateway)* and *Zonal vehicle (zones, redundant unit)* (made up).
- `busStyle: "trunk"` (provisional option, default `bar`): a bus kept as a `busLine` node is drawn without a bar and
  without a rank of its own, as one shared line from its head (the member nearest the root of its bus tree) to every
  other member, an explicit bus (ADR 0005) with junction dots. Each line carries its tap's id and runs in the tap's own
  direction; refinement, toggles and traces keep it; switching back to `bar` restores the bars. A bus at the root of
  its tree (a trunk without gateways, a backbone) has no head and keeps its bar. Playground: Routing → *Buses*.
- Bus trees keep their roots whatever is collapsed: the roots are found on the whole graph and each is shown by the
  visible node that holds it (`LayoutOptions::bus_roots`, internal; `Snapshot.busTaps.roots` for continuations), and an
  edge between two members of a bus network that joins no bus (as to a collapsed zone that holds a bus) follows the tree.
  A collapsed zone now stands below its backbone instead of at the bottom.
- An organic area above the dense bound of 300 children is placed by the scalable stress (up to 2 000) when no stress was
  chosen, instead of being refused; above 2 000 it is refused with that bound. The snapshot records `scalableStress`.
  Areas within the bound are unchanged. Organic rings of 500, 1 000 and 2 000 nodes imported in 0.8, 1.5 and 3.4 s on a
  heavily loaded machine ([results](benchmarks/results/organic-fallback-20261119.md)).

#### Large graphs and hosts

- `liftEdges` (provisional, off): an edge whose ends are not siblings is drawn between the outermost containers its ends
  differ in, and the edges of one pair of containers as one route with their weights in both directions; edges between
  siblings stay. synthetic-20k with `levelOfDetail: {nodes: 300}`: 1 126 instead of 7 730 drawn edges, no line–node hits
  instead of 10 775, 33 940 crossings instead of 8.4 million, 1.6 s instead of 7.2 s
  ([results](benchmarks/results/lift-edges-20261118.md)).
- `levelOfDetail` takes a budget (provisional): `nodes` and `edges` bound the first view of an import, which opens
  containers level by level while it stays within both; edges are counted as drawn (one per pair of collapsed ends). With
  `nodes` and no `communities`, containers with more children than the budget are divided into communities, so a flat
  graph starts collapsed too. synthetic-20k (22 819 nodes) with `{nodes: 1000, edges: 2000}`: 94 boxes and 2 000 edges in 4.2 s;
  a flat graph of 20 000 nodes with `{nodes: 300}`: 300 boxes in 41 s (loaded machine)
  ([results](benchmarks/results/first-view-budget-20261118.md)).
- `routeSearchWindow` (provisional, off): a constructed layered route searches the lanes across the flow only near its
  ends (as far across as it reaches along the flow, at least 16 tracks) and over the whole width only when it finds no
  way there. A search over the whole width is quadratic in the boxes across the drawing (4 000 lanes in a 5 000-node
  view). The same metrics on every dataset of the benchmark set; on synthetic-20k slightly fewer crossings and bends, and
  half the time (5 000-node view: 47 s → 22 s). `benchmarks/eval/search-window.mjs` compares it with the default
  ([results](benchmarks/results/route-search-window-20261118.md)).
- `isolatedGridFrom` (provisional, unset): `isolatedGrid` only in the areas with at least this many children that no edge
  joins to a sibling, so a container opened with hundreds of unlinked members is a grid instead of one long column while
  every other area keeps its default layout. At 50 every dataset of the benchmark set keeps its metrics; `isolatedGrid`
  everywhere changes five of them (line–node hits on `mixed-large`, more crossings on `workflow-dag`).
- Precomputing for a host (provisional): `communities` (operation; `arqlet::community_groups`; npm `communityGroups`)
  answers the groups and communities a view adds as the `groups` option takes them (node id → path of labels). Computed
  once in a backend and given back as `groups`, the view gets the same members and labels without Louvain (the
  containers are of kind `group`). `arqlet::import` and npm `importGraph` (with `omitSource`) run the import as a
  library call. Tests `community_groups.rs`, `precompute.test.mjs`.
- `omitSource` (provisional, import): the snapshot leaves the source graph out and carries its fingerprint
  (`sourceDigest`); later requests on it bring the graph as `source`, checked against the fingerprint, and their answers
  leave it out again (an answer whose source changed carries the new one). At 1M the import's answer is 7.3 MB instead
  of 187 MB.
- **Columnar scene (ADR 0019 step 4, provisional):** a resident session answers `columns` with the id tables and keeps
  the scene's numbers as columns (`f32` bounds and route points, `u32` indices); `session.columns()` in JavaScript reads
  them as views into WASM memory, as transferred copies from a worker, or builds them from the scene for a backend
  (`columnsOfScene`). WASM export `lw_session_column`. The overview too (opt-in `columns: true`, WASM export
  `lw_session_overview_column`): `x`, `y`, `r`, `parent`, `kind`, `edges` and `links` as typed arrays. Measured against
  the JSON answers ([results](benchmarks/results/columns-20261118.md), load 4 to 2, one run per size): the scene is
  3.3–3.7× smaller and 1.8–8.6× faster to the page, an overview continuation 1.7× faster but larger in bytes. The columns
  cover the whole scene: nodes, routes (kind, two-way weight, bundle members too), ports, boundary crossings, labels, rows,
  conflicts and junctions. The SVG viewer draws columns (`renderSvg(host, columns)`, `sceneOfColumns` in
  `@arqlet/viewer/scene-columns.js`) with the same markup; in Chrome the page's total for a scene is 1.4× (107 boxes) and
  1.07× (10 000 boxes) smaller, all of it the engine call ([results](benchmarks/results/columns-20261118.md), load 18.7).
  The overview page has a "typed arrays" box. Benchmark:
  `benchmarks/protocol/measure-columns.sh`.

#### Playground and tools

- Playground: **Live** (organic areas keep settling, animated): turns on force-directed behaviour and follows every change
  with bounded `settle` calls, one animated frame each, until the engine reports convergence (at most 60 calls). Each
  call goes through `execute()`, so it runs in the split, browser-only and backend-only modes, and a stale answer is
  discarded as for any edit. Each frame is a preview; the converged one is repaired. An import in live mode animates
  from scratch: it keeps the stress placement, then the forces are switched on and the settling draws the way into their
  equilibrium (petri-net-shapes: about 60 frames, 2.7 s in the split mode).
- `tools/readme-examples.mjs` takes `--options`, `--out` and dataset names, for before-and-after images.

### Changed

- **Default:** `sharedBorderRanks` and `alignStacked` are on: free space beside stacked containers. In a container across
  its parent's flow the border nodes share the first or last rank with the side nodes instead of opening one of their
  own, which held only a long-edge dummy; and an edge that is the only one between two containers of adjacent ranks (a
  stack, such as the subnets of a zone) forms a block of the placement across the flow with priority, at the ports the
  containers' plans give, so the stack stands flush and its edges run straight. Decided in the plan, not by moving
  boxes afterwards (a pass that did, `straightenOffsets`, added crossings it could not see and was dropped).
  cloud-architecture 114 → 108 bends and 7.1 % smaller, org-chart 2 bends fewer and 9.8 % smaller, synthetic-2k 12
  crossings and 35 bends fewer at 1.3 % more area, arqlet-pipeline 2 bends fewer at 0.3 % more area; every other dataset
  unchanged ([measurement](benchmarks/results/cross-axis-space-20261115.md)). New reference fingerprints:
  `benchmarks/results/fingerprints-20261115-stacked-default.jsonl` (4 of 15 views changed; with both options `false`
  the geometry equals the 20261114 reference).
- **Default:** `loopSpace` is on: the loops of a node take a corner where no other box lies in their way, in every area
  (before only in organic ones with `organicForces`); they take no part in the placement. synthetic-2k 8 bends fewer, every
  other dataset with the same crossings, bends and area (state-order draws its loops at other corners). New reference
  fingerprints: `benchmarks/results/fingerprints-20261115-loop-space-default.jsonl` (state-order changed; with
  `loopSpace: false` the views equal the stacked-default reference).
- **Default:** `organicForces` is on: a fresh organic placement continues after the stress with forces in the manner of
  d3-force (springs between where the edges end, many-body repulsion, box collision, a push off straight edges, and a pull
  toward the centroid that weakens with the number of children). Taken per area only where the children are connected
  and the straight edges cross no more often and pass through no more boxes, nested ones included; a fresh layout that
  keeps a geometry issue is laid out again without the forces. A node's loops in an organic area take a corner where no
  neighbour lies. A force-directed drag and `settle` relax with the same forces. petri-net-shapes 41 % smaller and 5×
  faster, mixed-layouts 5 crossings to 0 and 22 % smaller, organic-50 40 % smaller, organic-200 5 crossings to 2,
  self-loops 4 % smaller; every other dataset and every fingerprint view unchanged (can-bus was 0.25 % larger until an
  area in a layered parent kept the stress at a tie of crossings and boxes in the way when the forces took more room)
  ([measurement](benchmarks/results/organic-forces-20261114.md)). `organicForces: false` restores the earlier placement.

- The import's selection, its collapsed containers and the budget walk the structured graph by index (`GraphIndex`,
  built once): a 1 000 000-node import 16.5 s → 10.6 s, byte-identical
  ([results](benchmarks/results/import-index-20261118.md)).
- The import of a large graph: an abstracting import reuses the structure (communities) its selection computed, the
  selection and `validate` walk ancestors once (linear), edge-port bindings are found by maps, a typed request field is
  read without a copy, and an `import` is read from its text in one pass. A 1 000 000-node import: 22.5 s → 15.8 s;
  byte-identical ([results](benchmarks/results/import-prepare-20261118.md)).
- A view's projection walks the structured graph by index (`arqlet_model::GraphIndex`, kept with the structure;
  `arqlet_view::project_indexed`) and copies an edge only when the view keeps it. A toggle in a session of a 1 000 000-node
  graph: 3.9 s → 2.5 s on the same machine and load; byte-identical
  ([results](benchmarks/results/sessions-indexed-20261118.md)).
- Resident sessions keep the prepared source (ADR 0019 step 3): the snapshot's source graph is shared (`Arc`), and the
  view's structure (groups, communities) is kept with the snapshot (`Snapshot.structure`, not serialized) and reused by the
  next view of the same source and structure options; a structured view projected once is not validated again. Opening a
  container in a session of a 1 000 000-node graph: 8.5 s → 3.8 s; byte-identical answers
  ([results](benchmarks/results/sessions-prepared-20261118.md)).
- The projection of a view (`arqlet_view::project`) checked for every collapsed container that it has children by a
  search of all nodes, and walked from every node and edge end to the root by id lookups; both are computed once now, and
  two more searches of all nodes per id (pulled containers at import, pins) use a set. A first view of a 300 000-node
  graph: 24 s → 6.4 s, of 1 000 000 nodes: 71 s → 23 s, opening a container there 74 s → 14 s; byte-identical output.
  `benchmarks/scaling/series.py` measures overview, first view and opening a container from 20K to 1M nodes
  ([results](benchmarks/results/scaling-series-20261118.md)).

- With `organicForces`, `settle` takes its steps as previews of the forces (no repair in between, as the steps of a
  force-directed drag) and repairs the step that converges; the settling then counts as converged. Without a held node a
  step keeps the drawing's centroid. Before, the repair and the relaxation worked against each other and a continuation
  shifted the drawing at every step (it is translated when a node passes the margin, never back), so settling did not
  converge (petri-net-shapes swung by 80 units, mixed-layouts crept by 70 a step; with the stress model up to 1300). Now
  petri-net-shapes converges in 5 calls, mixed-layouts in 8, organic-50 in 24, each call 8 to 40 ms.

- Settling cools: each call of one settling takes 0.9 of the step of the call before (the report counts the calls as
  `round`), as a force simulation cools, so that it converges instead of swinging. On the organic examples within 4 to
  43 calls (organic-100 did not converge in 60 before).

- `organicForces` leaves out an area whose child containers hold containers themselves: the edges deep inside them are
  what the choice before the repair cannot foresee, and such a drawing kept an issue and was laid out a second time
  without the forces (mixed-large: import 2.0 s, now 1.1 s as before the forces; every other result unchanged).

- The geometric repair resolves ports faster: the per-edge parts that depend on the graph only (the flow sides' common
  area and axis, the source's area mode, the end sides) are computed once, ends are read by index, and loop corners only
  for the nodes a trial asks for; it keeps the boxes between the trials of a repair and reads only the changed ones again,
  computes the sort keys of a side once and the ids of automatic ports once per edge end. mixed-large's layout
  1147 → 808 ms with byte-identical output ([measurement](benchmarks/results/port-resolution-20261115.md)).

- A step of a drag repairs within its local budget also for the lines into containers (`nestedEdgesByDetour`), whose own
  budget it used to take: drag steps on mixed-large 0.7–0.85 s → 75–154 ms; the finish does the rest
  ([measurement](benchmarks/results/drag-steps-20261115.md)).

- The end of a drag on a nested organic drawing: an organic root moves as a whole off its wall before the repair, the
  moderate repair of the lines between children counts its scans on its own, and the full repair after a drop runs only for
  conflicts the detours do not draw around. mixed-large's gesture 4.9 s → 1.3 s (3.8 s → 2.4 s with force-directed
  behaviour), with fewer crossings and less area, no issue left; without force-directed behaviour more bends until the
  background refinement ([measurement](benchmarks/results/drop-repair-20261115.md)).

- The preview steps of a settling keep their straight edges straight instead of drawing detours that change from frame
  to frame; the converged step is repaired and drawn as before.
- Playground: after a force-directed drop the settling goes on until it converges (it stopped after three calls and left
  an unrepaired preview since the steps became previews).
- Playground test of the live mode (`ui-regression.mjs`): an import animates from the stress, converges, stays valid and
  starts no refinement.

- `directionsFirst` combines with the other options: each candidate with fixed directions goes through the whole
  pipeline (region ports, reversed flows, region child ports), where before the first matching branch ran alone. The
  measured datasets draw as before (er-diagram and synthetic-2k, the only ones with free directions, take none of the other
  branches); `directionsFirst` stays off (synthetic-2k: 306 crossings fewer, but 62 bends more and 67 % more area in 2.9×
  the time).

- The routing styles of the edges (`edge_choices`) look the areas up in a parent table built once, instead of searching the
  node list for every parent of every edge; the repair asks for them again and again, which was quadratic in the size of
  the view (a view of 1000 nodes of synthetic-20k spent most of its first 15 minutes there). Byte-identical output.

- Large views no longer route everything again: the engine choice (both routers on the whole drawing) runs only up to 500
  routes, and routing the routes in conflict again only up to 500 conflicts; above that the conflicts are reported, as
  under single pass. A 1000-node view of synthetic-20k took more than 15 minutes, now 26 s; synthetic-20k with one level
  open (349 visible nodes, 8072 edges) more than 10 minutes, now 7 s.

- `loopSpace` finds the nodes within reach of a node's loop corners once and tests their ancestors only for them; every
  node was tested for every corner of every node with a loop (a 5 000-node view of synthetic-20k: 198 s → 78 s,
  [measurement](benchmarks/results/structured-view-scale-20261115.md)). Byte-identical output.
- Large views are routed faster, with byte-identical output: a route search whose first step (from the port to the first
  corridor) or last step is blocked is no longer run (the way is impossible whatever lies between; more than half the
  searches of a 5 000-node view); an edge's gaps and obstacle index are built at its first search and only once; its
  obstacles are taken from the boxes as ranges between the ids it skips; and the fallback repair of the routes looks the
  boxes' shapes up once and skips by index. A 5 000-node view of synthetic-20k: 77 s → 47 s
  ([measurement](benchmarks/results/route-search-window-20261118.md)).

### Removed

- `repairCompaction` (experimental, never released): with `organicForces` it added nothing.

### Fixed

- Playground: the end of the live animation flickered and the drawing rolled in at other places. A node's pin control
  lies inside the node and took the node's animation offset a second time; in the live mode's frames, which come faster
  than an animation lasts, the offsets doubled from frame to frame. Only the outermost element of an id is moved now,
  and the places after a change are measured without a running animation's offset. The routes are no longer faded in at
  every frame of settling, and the live mode starts no refinement (it laid the drawing out anew after the animation).
- The end of a drag that changes no pin repaired nothing: the engine saw unchanged options and answered without a layout,
  so conflicts the local repair of the last drag step left stood. The end of a gesture now lays out the dragged node
  again when geometry issues remain (not for a force-directed drag, which settles by its own steps).

## 0.5.0 — 2026-11-14

Every area in single pass, smaller drawings and valid geometry on every dataset. Reversed flows (`rl`, `bt`) draw as the
mirrors of their positive ones; regions of another strategy (organic, tree, radial) meet layered parents through ports
on the sides of the flow; straight edges go around the nodes in their way; channels between ranks are as wide as their
tracks need (3 to 17 % less area on the layered views at unchanged crossings and bends); an organic root over layered
containers lays out several times faster with valid geometry. Also new: `patch` (change the graph of a view without a
new import), `neighbourhood`, trace in the cluster overview, packed components, hubs left out of communities, and typed
answers in the Rust protocol.

### Upgrading from 0.4

- **The default drawing changes** where the new defaults act: `regionPorts`, `straightDetours`, `singlePassReversed`,
  `channelsByDemand` and `nestedEdgesByDetour` are on. Layered views get shorter channels; reversed flows, regions in
  layered parents, straight edges through nodes and organic roots over containers are drawn differently. To keep 0.4's
  drawings, set the five options to `false` (with `channelsByDemand: false` the measured fingerprint views equal 0.4.0).
  They are written into a snapshot only when `false`.
- **The engine version is `0.5.0`** (it was `0.2.0-prototype` since 0.2): edits of a snapshot from an earlier engine are
  refused as before for any other version, so import the graph again rather than continuing a 0.4 snapshot.
- **Rust: `arqlet::protocol::Response` has typed answers** for `scene`, `trace`, `neighbourhood`, `overview`, `intent`
  and `metrics` instead of `Response::Encoded`; the JSON is unchanged. Exhaustive struct literals of `LayoutOptions`
  need the new fields (or `..Default::default()`), and `arqlet_scene::metrics::DrawNode` has a field `shape`.
- A vertical tree area now keeps its container's header free (its first rank moved down by 24 and the area grew by as
  much); every other strategy and orientation keeps its geometry apart from the defaults above.

### Changed

- **Default:** `channelsByDemand` is on: channels between ranks are as wide as their tracks need instead of at least the
  layer gap (in areas with a fixed flow along their parent's axis below fixed flows). Every measured dataset keeps its
  lines through nodes, crossings and bends; the layered views take 3 to 17 % less area
  ([measurement](benchmarks/results/channels-by-demand-20261114.md), [area breakdown](benchmarks/results/area-breakdown-20261114.md)).
  New reference fingerprints: `benchmarks/results/fingerprints-20261114-channels-by-demand-default.jsonl`. The spacing
  options and what they cost are measured in `docs/configuration.md` (*Spacing and area*).
- **Default:** `nestedEdgesByDetour` is on: a repair of an organic area that cannot clear every straight line within a
  bounded budget clears the lines between its children first and those into its containers within a bounded budget of
  their own, and the detours draw what remains around the nodes in its way. 31 of 32 datasets draw as before; on an
  organic root over layered containers (mixed-large) the geometry becomes valid, the import takes 1.1 s instead of 7.7 s
  and the refinement 4.9 s instead of 31 s, at more crossings and bends
  ([measurement](benchmarks/results/nested-edges-by-detour-20261114.md)).
- **Default (ADR 0020, S1-4):** `regionPorts`, `straightDetours` and `singlePassReversed` are on by default (written as
  `false` when turned off). Measured on every dataset: 30 of 31 draw exactly as before (the sweep fingerprints are
  identical to 0.4.0), mixed-large draws no line through a node (44 before) at 90 more crossings, and a graph that flows
  right to left or bottom to top everywhere draws as the mirror of its positive flow. Time: unchanged where nothing
  changes, 5 % on mixed-large ([quality](benchmarks/results/s1-4-defaults-20261114.md),
  [time](benchmarks/results/s1-4-timing-20261114.md)). A layout through region ports keeps the options its layout wrote,
  so a drawing that fell back to the earlier pipeline is refined by that pipeline.
- **`arqlet::protocol::Response` has typed answers** (provisional) for `scene`, `trace`, `neighbourhood`, `overview`,
  `intent` and `metrics` (`Response::Scene`, `Trace`, `Neighbourhood`, `Overview`, `Intent`, `Metrics`) instead of
  `Response::Encoded`; the JSON is unchanged byte for byte. A Rust host that matched `Encoded` for these operations
  matches the new variants. Contract types `TraceAnswer`, `NeighbourhoodAnswer`, `VisibleParts`; `session.trace` and
  `session.neighbourhood` return them (`ReadAnswer<T>` in `@arqlet/engine-client`).
- Playground: `build.sh` builds the native binaries with the `fast` profile (optimised like release, without LTO), and
  the backend runs `target/fast/layout-command` (`ARQLET_LAYOUT_COMMAND` overrides it; a debug build is the fallback).
  On a synthetic 20 000-node overview: 0.56 instead of 4.08 s (single run, Apple M3).

### Added

- `singlePassReversed` covers `bt` too (ADR 0020 S1-0b): every area is mirrored within its box and a container's header
  stays at its top; a bottom-to-top graph draws as its top-to-bottom mirror
  ([measurement](benchmarks/results/reversed-flows-20261114.md)).
- `regionChildPorts` (experimental, off; ADR 0020 S1-3): layered containers inside an organic or radial area get border
  nodes in their own flow, so their edges leave them along the flow and the area draws them on from there
  ([measurement](benchmarks/results/region-child-ports-20261114.md)).
- `regionPorts` (on by default in this release, see *Changed*; ADR 0020 S1-2): edges that leave an organic, tree or radial region in a layered parent
  enter and leave it through ports on the sides of the parent's flow, with the inner leg drawn by the region
  ([measurement](benchmarks/results/region-ports-20261114.md)).
- `straightDetours` (on by default in this release, see *Changed*; ADR 0020 S1-1b): a straight edge that would pass through a node is drawn around it
  (its orthogonal route pulled taut), so an organic root over layered containers has no line through a node; the
  placement does not change ([measurement](benchmarks/results/straight-detours-20261114.md)).
- `metrics` measures node shapes (provisional `shape` on a drawing node): a circle is its disc, so an edge that ends on a
  circle's outline no longer counts as a line through the node. Rectangles measure as before
  ([measurement](benchmarks/results/single-pass-mixed-20261114.md)).
- `singlePassReversed` (on by default in this release, see *Changed*; ADR 0020 S1-0): a graph that flows right to left everywhere is laid out left to
  right under single pass and mirrored, so `rl` draws as well as `lr` (today it switches single pass off and crosses and
  bends more). Not yet `bt`, nor graphs with pins, label places, explicit ports or buses
  ([measurement](benchmarks/results/reversed-flows-20261114.md)). ADR 0020 (proposed): single pass for every area.
- ELK adapter: `elk.layered.considerModelOrder.strategy` (other than `NONE`) makes ties follow the input order instead
  of the node ids (provisional), as ELK follows its model order; renaming nodes no longer changes the drawing. The
  answer carries the given ids. Off by default; without it the output is unchanged.
- `patch` (provisional): changes the graph of a view without a new import — `{nodes, edges, removeNodes, removeEdges}`,
  removals first, a removed node with what it holds and its edges. The view keeps its collapsed containers, filter and
  options; an abstracting view is laid out from scratch as a toggle lays it out (the same drawing as a fresh import of
  the changed graph with that view), other views from their earlier positions. A host can start with aggregated edges
  between collapsed containers and load a container's member edges when it is opened. `session.patch(patch)`; contract
  type `GraphPatch`.
- Trace in the cluster overview (provisional): `overviewTrace` and `overviewNeighbourhood` find paths and neighbourhoods
  on the overview's graph (the one an overview session keeps, or the request's), answered as `GraphTrace` /
  `GraphNeighbourhood`; `createOverviewSession` gets `trace` and `neighbourhood`. `@arqlet/viewer`: `pathOf` maps an
  answer to the model, `overviewBuffers` / the renderer's `setState({trace})` draw it in amber over the discs that show
  it (the rest dimmed), `revealPathDecisions` opens the clusters on the way, `viewOfNodes` frames it. Playground: section
  *Path* on the overview page.
- `directionsFirst` (experimental, off): free directions (`auto`) are laid out with every free area left to right and
  with every free area top to bottom, both fixed under single pass, and the drawing with the best published key wins.
  Single pass then builds the edges between areas instead of delegating them: on the layered examples no lines through
  nodes, never worse than a fixed `lr` by the key (better on two), about twice its time, where plain `auto` crossed
  2–3× more and took 53 s on the largest example ([measurement](benchmarks/results/free-directions-20261113.md)).
  Script `benchmarks/eval/free-directions.mjs`.
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

- A tree area that flows top to bottom (or bottom to top) put its first rank into its container's header: the
  arrangement is computed left to right and transposed, which moved the header room to the left. It now keeps the
  same margins as the other orientations; nested-tree-radial had 24 such geometry issues and has none. With this and
  `nestedEdgesByDetour`, no dataset in `benchmarks/datasets` has a geometry issue in the default layout.
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
