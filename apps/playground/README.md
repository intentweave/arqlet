# Native → WASM prototype

Prerequisites: Rust, `rustup target add wasm32-unknown-unknown`, Node 24+, Python 3.
From the repository root:

```sh
sh apps/playground/build.sh
python3 apps/playground/server.py
```

Open http://127.0.0.1:8765. Select a leaf node and add a connected node. Computation
runs in a module Worker using the same Rust core as the native snapshot command.
A reset invalidates any pending response. Exported snapshots can be retained.
The build script also checks native/WASM parity and stale-revision rejection.
Pass a graph.json as the first build argument to use another dataset.

The view selects up to 40 connected nodes (undirected BFS from highest degree),
plus their ancestors. Unresolved targets are excluded before selection. The full
export remains unchanged. A graph with no resolved edges falls back to one node.

Local edits invalidate their parent and ancestor layouts; unchanged subtree
geometry is reused. Ancestor placement may move neighboring containers. Routes
are currently all recomputed for the bounded view. Parent boxes use local coords.
The local first-pass router is simple; backend refinement tries bounded obstacle
detours. Explicit ports are not yet exposed.

A loopback-only prototype backend now handles full-view refinement. Native CLI
precomputation and browser continuation work;
claims overlays, animations, arbitrary edit operations and large-scene rendering
are subsequent milestones. JSON full snapshots are a prototype transport.

## Examples

The **Beispiel** selector switches between the default graph (the Intentweave export, or the
graph passed to the build) and fourteen generated example graphs: cloud architecture, activity
diagram, deployment, two state machines, workflow DAG, Arqlet's own pipeline and code graph,
org chart, UML class diagram, ER diagram, BPMN with pools, VLAN network, and a Petri net.
The examples come from `benchmarks/generate.py` and are copied to `generated/examples/` by the
build. Each example is imported with a view limit of 150 nodes through the current mode's import
lane: the WASM worker in split and browser-only modes, `/api/op` in backend-only mode. Only the
connected part of a graph is shown, as in the rest of the prototype, so isolated attribute nodes
(UML and ER diagrams) are not visible. The default graph in split mode still uses the native
precomputation.

## Execution modes

The mode selector chooses where each operation runs. All targets execute the same Rust
dispatcher, so results are identical across modes:

- **Geteilt** (default): the native CLI precomputes the view; edits run in the interactive
  worker; refinement runs on the loopback backend.
- **Nur Browser**: the WASM worker imports `generated/graph.json` and runs every operation,
  including refinement in a separate background worker. The backend is not contacted.
- **Nur Backend**: every operation is sent to `POST /api/op`.

Edits and refinement use separate workers, so a long refinement never blocks a local edit.
The `threads` field requests parallel candidate evaluation on native builds; WASM runs serially.
Results do not depend on the thread count.

## Background refinement

On initial load (when enabled) and after each local edit, a debounced request sends the current bounded snapshot to
`POST /api/op` (operation `refine`). A native Rust process compares the existing layout with four candidates:
forward/alternating barycenter sweeps and forward/alternating median sweeps with
vertically centered columns. Rank assignment itself is not searched yet.
Sibling overlap count takes precedence, then edge-node penetrations, strictly
interior routed crossing pairs, shared collinear route pairs, projected crossings,
and finally a cost
of routeLength + 0.15 * movement + 0.001 * area. These are provisional weights.
The crossing indicator uses straight node-center connections, deduplicates
parallel/reverse relationships and excludes shared endpoints. Actual orthogonal-route crossings, shared segments and edge-node penetrations
are now measured separately.
Graph revision is preserved and layoutVersion increases independently.

Local edits remain available during the request. The browser aborts obsolete
fetches and checks graph revision, layout version, reset epoch and pending edits
before adoption. Aborting a fetch does not yet cancel native CPU work; the server
limits work to two jobs with a 15-second subprocess timeout. Size caps are 1000
nodes, 2000 edges and a 4 MB request. This is a local prototype, not a production
multi-user API. No snapshots are persisted by the server.

The quality pass currently uses pairwise comparisons and is intentionally limited
to small views; it is not the final scalable crossing counter. UI diagnostics
report before/after indicators and the number of evaluated arrangements.

## Obstacle-aware refinement

Six candidates now include rerouting without moving any node. Each route tests
up to 24 horizontal detour rails near obstacle boundaries, preserves endpoint
positions/directions, and greedily considers earlier routes. Source/target nodes
and their containing ancestors are excluded from foreign-obstacle counts.
Crossings count route pairs with at least one strict interior perpendicular
intersection; endpoint and bend touches are excluded. Shared positive-length
collinear segments are a separate pair count. These overlaps are not bundles.
The algorithm is deliberately incomplete: no visibility-grid search, clearance
guarantee, container-boundary ports, or guarantee that all collisions disappear.
Quality evaluation is pairwise and remains intended for bounded views.

## Hard node geometry validation

Every computed snapshot contains `geometryIssues`, with a kind, affected IDs and
message. Rules currently require a 12-unit gap between unrelated rectangles,
12-unit container padding and a 36-unit header. Ancestor/descendant overlap is
intentional; overlap between unrelated branches is not. Missing, nonfinite,
nonpositive or overflowing rectangles are diagnosed explicitly.

Refinement recomputes validation rather than trusting incoming diagnostics. Invalid
placement candidates cannot win on quality; a valid candidate repairs an invalid
input even if its route cost is higher. If no valid candidate exists, refinement
fails explicitly. This validates node rectangles, not labels, port clearance or
edge obstacle-freedom. The UI shows affected IDs and marks affected rectangles.

## Endpoint ports and container crossings

Graph JSON optionally accepts `ports` and `edgePorts`:

```json
{
  "ports": [{"id":"out","nodeId":"a","side":"bottom","offset":0.25}],
  "edgePorts": [{"edgeId":"ab","sourcePort":"out","targetPort":null}]
}
```

Sides are left/right/top/bottom; offset is a finite fraction from 0 to 1 along
the side. Port IDs and node ownership are validated. Automatic source/target ports
use mutually facing sides based on absolute node rectangles, including reverse
edges arriving from below or from the right. Layout direction breaks diagonal
ties. Incoming and outgoing automatic endpoints share deterministic per-side
spacing; explicit ports retain their configured position. This is not yet
minimum port spacing or optimized port order. A configured endpoint and its
outward direction are preserved by detour routing.

Snapshots expose `resolvedPorts` and `boundaryCrossings`. Green/blue circles show
source/target endpoints, golden squares show derived container intersections.
Boundary crossings are observations of the current route, not fixed boundary-port
constraints; they are recomputed after routing. Corner/boundary-touch cases are
not full topological crossing analysis yet. Explicit fixed container portals,
inside/outside route sections and port editing UI remain future work.

## Open and close aggregate containers

Select a container from the container dropdown, by its header, or by its closed aggregate card, then
use the open/close button. View projection runs in the same WASM worker as edits.
The snapshot retains `sourceGraph` (the full selected excerpt) and `collapsedIds`;
`graph` is the currently projected view. Internal edges are hidden, external edges
retain their original IDs but use the visible aggregate endpoint. The canonical
endpoints and port bindings are restored when reopened. Parallel edges stay
separate; they are not automatically bundled.

View changes advance layoutVersion, not graph revision, and invalidate pending
backend results. Backend refinement preserves canonical data and collapse state.
Editing a visible leaf while another container is closed retains hidden data.
The demo disables adding from container selections; it supports additions from
visible leaves. Nested collapse flags remain when an ancestor closes.

This is currently a complete relayout of the bounded projected view, not yet
position-preserving expansion or incremental fetch of a full 100K graph. Stored
inner graph structure is restored; prior optimized coordinates are not cached.
Fixed container portals remain the next routing milestone.

## Inherited layout direction

Select “whole view” or a visible container, choose LR, TB, hybrid or inherit, and
apply. The graph stores `defaultDirection` and a `containerDirections` map. An
absent container override inherits the nearest ancestor policy. “Inherit” removes
an override; the whole view requires an explicit policy. Policies survive
collapse/reopen, native refinement and snapshot export.

Hybrid tries LR and TB locally for each container and selects the smaller cost
`width * height + 0.25 * max(width, height)^2`. Children inherit the policy, not
the parent's chosen orientation. This is a compactness heuristic, not a global
crossing-minimization search. Nodes retain their dimensions in either direction.
Direction changes currently recompute the bounded view and advance layoutVersion.
Explicit endpoint ports retain their configured sides.

Opening/closing and direction changes fit the resulting view. Static assets and
WASM are loaded without caching to avoid mixed demo versions after a rebuild.
The build includes a simulated DOM app regression with real WASM covering initial
automatic refinement, collapse/reopen, and direction overrides. It does not replace
a visual browser test.

## Label controls and route diagnostics

The edge-label and port-caption checkboxes measure default text and send it through
`configure`. The core reserves spacing and returns collision-checked label bounds.
Custom text and sizes can be supplied via `edgeLabels` / `portLabels`; see
[configuration](../../docs/configuration.md). Node measurements now merge rather
than replace earlier sizes. Unplaceable labels and remaining node penetrations
appear in `routingIssues` and the demo status; affected routes are highlighted.
A bounded rectilinear grid repairs penetrations even before background refinement.
The search is limited to 120,000 grid intersections per affected edge. It does not
guarantee zero crossings/shared segments or fixed container boundary portals.

## Reusable scene and second renderer

Drawing now lives in `packages/viewer/svg.js`, fed by the renderer-neutral scene projection
in `packages/viewer/scene.js`. The host app retains commands, workers and snapshot state.
A second client is available in [react-flow](react-flow/README.md). Both consumers
share synthetic geometry/port/label fixtures; see [the contract](../../docs/scene-viewers.md).

## Navigation and snapshot comparison

Drag the diagram to move it; the mouse wheel zooms at the pointer. Fit, zoom in and zoom out are also
available. The advanced JSON field passes layout and routing options to `configure`; e.g.
`{"routingClearance":16,"trackSpacing":12}`.

`/compare.html` compares two local snapshot JSON files (a demo download or files from
`benchmarks/results/*-snapshots/`). Both receive current shared metrics from WASM.
Matching scale is optional; panning stays independent. A different visible graph structure
is reported. Options can be expanded. The page does not compute a layout again and shows no
historical timings; the benchmark reports remain authoritative for those.

## Presets

The selector shows the preset the view is in (`configId` = `<id>@<version>`). An older version
or a manual change shows "Custom". Applying runs through `packages/engine-client/presets.js`: the
direction first, then the config set. If the first step fails or the view has changed, the second
is not sent. The React reference uses the same logic.

## Routes and bundling

- **Route selection:** a click on a route highlights it (above the others, which are dimmed). A
  second click or a click on the empty area clears the selection; Esc does too. A new revision
  drops the selection. The line under the node selection names source, target, length, bends and
  the crossing edges (`packages/viewer/routes.js`). The route's conflicts (passes through a node,
  foreign container, border proximity, shared lines, crossings) come from the engine
  (`routeConflicts`), with the same numbers as the overall metrics. Each conflict partner is a
  button: a route partner is selected itself, a node partner as a node.
- **Bundling:** the "Bundling" selection has the levels "None" and "Bundle parallel edges". A bundled
  route carries a number (the number of edges), and the detail line names the bundled edges. Branches
  (trunks) are not implemented yet.

