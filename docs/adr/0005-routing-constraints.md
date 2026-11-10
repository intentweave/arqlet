# ADR 0005: Regional routing constraints and contour-aware geometry

Status: target decision amended 2026-10-24 after review of step A. Implementation is partial. This amendment supersedes the earlier rules that treated explicit root `orthogonal` as absence and refused an entire drawing merely because it used mixed styles. It does not claim those corrections are implemented.

## Context and relationship to the other configuration axes

Direction, layout strategy, routing geometry, port placement and admissible port sides are independent regional requirements. Position grids and margins are separate constraints; `shortest` is an optimization criterion. Root and containers use the same resolution semantics. See [ADR 0003](0003-container-direction-constraints.md) and the [recursive optimization specification](../recursive-optimization.md).

`defaultRouting` and `containerRouting` are existing serialization fields for root requirements and container overrides. They do not establish a different configuration model. Renaming fields or introducing a new nested schema is not required for this change. Resolve them into the same conceptual per-area configuration used by other axes, retaining the source of each value.

## 1. Authored constraints, inheritance and defaults

- A fixed style is binding, including an explicitly authored `orthogonal`.
- `auto` delegates selection to the engine among eligible implementations. It is not an alias for `orthogonal`. An implementation may start with a deterministic orthogonal baseline, but must report the chosen style and preserve `auto` as the authored requirement.
- An allowed list restricts selection; list order is not a priority or quality criterion. A singleton is fixed. Empty lists, duplicates and unknown names are invalid.
- A missing container entry inherits from its nearest ancestor on this axis. A local declaration, including `auto`, replaces the inherited value. Removing it restores inheritance.
- A missing root entry uses the compatibility fallback, then the engine default (`orthogonal`). Root supports fixed, auto and allowed values just like a container.
- Missing, explicit default and auto remain distinguishable through parsing, editing and serialization. An explicit root `orthogonal` must not disappear because it equals the engine default. Keep declared and resolved configurations separate; do not serialize a chosen auto result as an authored fixed requirement.

## 2. Legacy option compatibility

`LayoutOptions.edgeRouting` is a legacy fallback, not a second competing source of constraints. The resolution order is: nearest authored regional declaration (including root), otherwise explicitly supplied legacy option, otherwise engine default. A container declaration remains effective when only other areas use the legacy fallback.

An old caller-supplied option that disagrees with an applicable declaration must never silently replace it. During migration, return a scoped conflict diagnostic identifying both sources and require removal or an explicit configuration update. An implicit default in an options struct is not a caller declaration and must not create a conflict. Preserve presence information at the request boundary; inferring it by comparison with a default enum value is insufficient.

A consumer may intentionally change the requirement through a configuration patch. That changes the configuration revision and invalidates older jobs; it is not an optimizer override. Presets and the UI use this same operation. New callers should use the regional contract rather than the legacy option.

Resolution examples:

| Root | Container | Explicit legacy fallback | Effective requirement |
| --- | --- | --- | --- |
| absent | absent | absent | engine default orthogonal |
| absent | absent | straight | straight via compatibility fallback |
| orthogonal | absent | absent | authored fixed orthogonal |
| orthogonal | straight | absent | orthogonal outside, straight inside |
| auto | absent | absent | inherited free choice, not a fixed default |
| [straight, orthogonal] | orthogonal | absent | allowed outside, fixed orthogonal inside |
| orthogonal | absent | straight | conflicting declarations diagnosed; never silently straight |

## 3. Per-edge ownership and mixed drawings

For the initial whole-edge policy, use the effective routing requirement of the lowest common containing area of both endpoint nodes. Treat a container endpoint as an object in its parent area: an edge from that container object to its descendant is owned by the container's parent. An edge between children of one container belongs to that container; an edge across sibling containers belongs to their common containing area. Root is a real configuration scope even when it has no visible frame.

Use one shared ownership resolver in standard layout, composition, repair and evaluation. Report ownership and requirement provenance. A future explicit per-edge override or per-section routing style needs a separate extension; neither is silently introduced here.

Mixed straight and orthogonal edges are valid target behavior, not an unsupported graph class. Partition routing work by selected style while composing one complete result. A straight edge remains a segment during route-only optimization. If a node or port moves, recompute its endpoints and revalidate it. Orthogonal routing must account for fixed straight segments in occupancy/crossing costs and final validation, including diagonal segments; it cannot assume every retained edge is axis-aligned. Both styles use the same node/label obstacles and required clearances.

A conflict may trigger placement repair, another allowed style, or a diagnostic according to the request. Never silently bend a fixed straight route or convert another region's style. Current inability to process mixed styles is an implementation gap, not a permanent contract restriction.

## 4. Style catalogue and capabilities

Geometry styles are `straight`, `orthogonal`, `roundedOrthogonal`, `polyline`, `octilinear`, and `curve`. `roundedOrthogonal` is implemented as presentation of the orthogonal route: the layout keeps the route's axis-parallel points and the scene marks it `rounded`, so routing, conflicts and ports are those of `orthogonal`. `polyline` is implemented as the orthogonal route pulled taut in the scene (`geom::pull_taut`): bends are dropped where the direct segment keeps 4 units clear of every other node, so segments run at any angle; the snapshot keeps the orthogonal route. `roundedPolyline` is that route with rounded corners (the scene marks it `rounded`) and keeps 10 units clear of the nodes, so the arcs do not cut into them. A list is drawn in its plainest non-straight member (orthogonal, rounded orthogonal, polyline, rounded polyline). `bus` may remain a recognized compatibility/request name, but the target separates bus grouping/topology from path geometry. Bus membership is explicit; auto must not invent semantic junctions or silently bundle unrelated edges.

Unsupported fixed requests receive a capability diagnostic. Allowed lists may proceed with supported, compatible members and report exclusions; fail if none remain. Auto considers only compatible implemented variants. No silent style substitution. Naming a future style does not imply its implementation.

## 5. Shared geometry and routing algorithms

### Common outline contract

All routers, repair passes, port placement, labeling, validators and evaluation use the same geometry operations. The following is an operation contract, not a frozen Rust API:

| Operation | Required result |
| --- | --- |
| Point containment | Classify a point as inside, outside or on the boundary, under a shared tolerance policy. |
| Segment versus obstacle | Detect intersection/interior entry into the actual contour and its required-clearance region, including segments whose endpoints are both outside. |
| Boundary attachment | Find the contour intersection for a connection and return attachment geometry, including the local normal/tangent where defined. |
| Admissible attachment | Generate or validate anchor positions and exit directions against port constraints; a geometric intersection alone does not authorize a port. |
| Broad-phase bounds | Return a conservative bounding box including the requested clearance and approximation error. |

### Clearance to nodes: foreign and own, at every exit

A required clearance applies to every node, whether it is a foreign node or one of the edge's own endpoints. A route keeps the clearance from each of them along its whole length, with one exception: the attachment at its own port, which is where the route leaves or enters the node. The exception covers the attachment only, never the rest of the node's side or the sides of other nodes.

- The leaving segment of a port is part of the route like any other. Where it must turn to keep the clearance from its own node, the turn is part of the route; the router adds the bend or the extra segment needed for it. Such an additional segment is not a reason to break the clearance, and it is not a reason to drop the route.
- A route that cannot keep the clearance with the ports it has is not a valid result for the requirement. The router may use another admissible port position (see the port order and position below), another path or another arrangement; otherwise it reports a conflict. It never silently accepts a route that runs closer than the requirement.
- Routes measured for the requirement use the same rule everywhere: the search, the acceptance of a candidate, and the result's diagnostics.

Separate shape geometry from obstacle ownership. A leaf body is an obstacle; a container's interior must remain traversable for its own contents. Authorized boundary crossings and source/target attachment exceptions are scoped to the relevant boundary/attachment portions, never the entire endpoint body. Leaf and container contours share the geometry interface without sharing identical traversability rules.

### Exact predicates, approximations and clearance

For circles, use minimum distance from the center to the full segment against radius plus required clearance. Handle zero-length segments as points. For polygons, use segment/boundary intersection and containment, including concave outlines; clearance requires a distance/offset-region test, not only intersection with the original polygon. Rectangles remain a supported specialization.

Complex outlines may initially use conservative outer approximations with a declared maximum error in layout units. The approximation must contain the actual obstacle and its required-clearance region; an inscribed polygon is not a safe circle obstacle. Include the approximation error in bounds and diagnostics so a conservative refusal is distinguishable from an exact collision. Define boundary contact and numerical tolerance once across all consumers. Tolerance must not silently reduce required clearance. Rendering and final validation must agree on the realized contour.

Bounding boxes and a spatial index (for example an R-tree) provide broad-phase candidate selection. The narrow phase tests the actual or conservatively approximated contour. Update or invalidate bounds and index entries when nodes move, resize or change shape; queries must use the same geometry revision as the candidate being evaluated.

Grid-cell center masks are insufficient: validate every connecting segment. Add contour-aware candidate generation so the search can discover legal passages through empty corners of a circle's bounding box. Correct validation alone does not make the existing candidate set complete. Keep the grid as one search representation, not as the shape model.

### Reuse across routing variants

| Variant | Search and realization | Shared or additional work |
| --- | --- | --- |
| Straight | Direct segment between admissible contour attachments. | Shared contour and clearance validation; report conflicts or repair placement without silently bending the edge. |
| Orthogonal | Reuse current channel/grid search with contour predicates; orthogonal visibility graphs are an additional representation. | Axis-aligned candidates, port exits and obstacle tests; preserve existing search infrastructure where suitable. |
| Rounded orthogonal | Start from an orthogonal route, choose feasible corner radii and realize arcs. | Check the resulting arcs against contours and clearance. Reduce radii or find another route; rounding is not solely a renderer effect. |
| Polyline | Add a visibility graph around conservative obstacle boundaries. | Reuse shortest-path/A* infrastructure, costs and validation; support diagonal segments throughout repair and evaluation. |
| Octilinear | Restrict segments to horizontal, vertical and 45-degree directions. | Extend candidate directions and transition rules; reuse suitable search and geometry components. |
| Curves/splines | Fit curves inside a previously found collision-free corridor where possible. | Validate the realized curve, endpoint tangents and clearance, using exact tests or conservative subdivision with bounded error. A safe control polygon alone is not a universal guarantee. |
| Bus-style | Plan explicit group backbones and branches jointly. | Reuse an appropriate geometry router for trunks and branches, preserving edge identities and authorized junctions. |

A* is a search method, not a routing style. Search representations may share traversal, budgets, spatial queries and cost accounting without forcing every style into the same grid. Corridor/free-space decomposition is another future representation. This contract does not promise that all variants require or receive a separate search engine.

Bus grouping is an independent topology decision, for example an explicit bus group with orthogonal geometry. Regional `auto` or an allowed geometry list must never introduce membership or semantic junctions between unrelated edges. Root/container inheritance applies to routing geometry as defined above; bus membership remains explicit. Preserve this separation even if `bus` remains a legacy request name.

Reference: [yFiles routing documentation](https://docs.yworks.com/yfiles-html/dguide/automatic-layouts-main-chapter/polyline_router.html) describes routing styles and buses with common backbone segments. It is a behavior reference, not an implementation dependency or a claim that every variant above is provided by that router.

## 6. Delivery and acceptance

1. Correct presence/default handling and shared resolution; implement mixed straight/orthogonal dispatch and expose the same fixed/auto/allowed/inherit choices in the demo.
2. Use shared contour predicates throughout routing, improvement and validation; add contour-aware search candidates. Preserve existing rectangle behavior where requirements are unchanged.
3. Introduce polyline visibility routing, then rounded/curved realization and explicit bus planning as separate capabilities.

Required checks: explicit orthogonal survives a round trip; omission remains omission; legacy fallback cannot override a declaration; per-axis inheritance and local auto; list order does not encode preference; root/container mixed styles; nested and container-to-descendant edge ownership; diagonal fixed segments respected by orthogonal repair; free cell endpoints whose connecting segment crosses a circle; routes through empty circle-box corners; required clearance; endpoint clipping; stale results after a routing configuration change.

Record current gaps without describing them as intended semantics. Existing CI and unchanged default fixtures do not establish support for these cases.

Geometry acceptance fixtures must additionally cover circle tangency and clearance, concave polygons, conservative approximation error, constrained port exits, container boundary permissions, index invalidation after resize/move, and a rounded corner whose arc intersects an obstacle although its original orthogonal segments do not. A curve or bus result passes the same final scene validation as other styles. Test that `auto` does not create bus membership.

## Amendment: overlaps and corridor capacity (2026-10-25)

Unintended shared edge segments are hard for a published result: outside a declared bundle or bus, two edges never share a segment (exact overlap beyond tolerance). Points where edges cross are a separate measure, and a shared endpoint does not allow a shared segment. Near-parallel routes closer than the track spacing form their own spacing conflict. Both rules are checked on the composed result, after every post-processing step (track separation, label placement, re-routing).

The repair that makes these rules hold follows the plan in `docs/algorithm-improvements.md` (A2 space requests, A4a local repair): find the overlap and the free corridor capacity, assign tracks jointly, request space where a corridor is short, widen the corridor within pins and size limits, re-route the affected edges and labels, and validate the composed result. When a result cannot satisfy the rules within the budget, it is published as an unresolved conflict with its diagnostics, not as an optimized result. Stopping early proves nothing about feasibility.

The combination of track separation and a required clearance is a regression requirement: each option must keep the guarantee of the other.

## Amendment: the clearance always applies (2026-10-25)

The required clearance is on by default: `LayoutOptions.requiredClearance` defaults to `DEFAULT_REQUIRED_CLEARANCE` (6, the track spacing), so every layout keeps it from nodes, the edge's own nodes beyond the stub at their port included. A request changes the value. An explicit `null` is the only opt-out: proximity is then a weighted quality term with the fixed margin of 12, as before. The default value is omitted from serialized options; the opt-out is serialized as `null`.

With the clearance on, three things follow, all implemented in the legacy router and the placement: track separation always runs (shared segments are hard, see above), corridors that are too narrow for the routes drawn through them are widened in the placement, and a candidate with remaining proximity is published only when it lowers the sum of hard violations of its input, and then as an open conflict.

This changes default output. The stored refinement fingerprints (`benchmarks/results/fingerprints-20261021T103351Z.jsonl`) and the benchmark results no longer describe the default; they describe the opt-out. They have to be regenerated on an idle machine before any measurement is quoted for the default. The channel router does not keep the clearance as a hard constraint (it routes through nodes when no strict path exists) and is not used with a required clearance.

## Amendment: explicit buses, first step (2026-11-08)

Status: provisional. `Graph::buses` lists explicit buses (`{id, edges}`; membership is topology, so it lives in the graph); membership is never inferred, and `auto` or a
geometry list never creates one. A bus whose edges share an end (one source, or one target) is drawn on the finished
orthogonal routes (`crates/layout/src/bus.rs`): its members leave (enter) that node by one port, share the stub and one way
across the first channel (the trunk), and branch off onto their own lanes. A bus whose edges have several sources and several targets is drawn as a backbone: one
line across a channel every member passes (after the last source, before the first target), with one stub per node; when no
channel is common to all, the members are split into the fewest groups that each share one, each with its own line. A member
whose end lies inside a container (a software component in an ECU) keeps its way inside: the bus runs between the points where
it leaves the outermost container of its source that does not hold its target and enters the corresponding container of its
target, and the members at one such component share its way in or out. A member that leaves or enters across the flow keeps
its own route. Members of
one bus may share segments and meet; the shared metrics count no shared line, near-parallel run or crossing between two members
of one bus (`Drawing::buses`), the scene carries the buses and their junctions (points where the bus's lines run off in three or
more directions) for the viewer, and a member whose shared way would enter a
node or share a line with an edge outside the bus keeps its own route. Everything not drawn as asked (members without a shared
end, routes that are not axis-parallel, members that must keep their routes) is reported as `Bus …` in the layout
diagnostics. The routing itself is unchanged, so buses do not move nodes or other edges. Open: buses in the other geometry
styles (a star through a junction for straight edges was measured and not kept), and the trunk in the routing's own channel
planning. Measured in `benchmarks/results/bus-20261108.md`.

### Amendment B: a backbone bus as a layout element (2026-11-08)

Status: provisional. A backbone bus (several sources, several targets, no node on both sides) takes part in the layout
(`crates/layout/src/bus_layout.rs`): before the layout it becomes a thin virtual node in the deepest area that holds all its
members' ends (40 × 8 across that area's flow), and every member `s → t` becomes `s → bus` and `bus → t`. Ordering, placement and
routing then see the bus like a node: it gets a rank of its own between its senders and receivers, and they are ordered around
it. After the layout the canonical graph and options come back unchanged; the two parts of every member are joined, the bus
node's box is kept in `Snapshot::bus_boxes` (relative to its parent, like `boxes`), and the drawing step draws the members on one
line through that box (`bus::draw_buses` with the box's middle as the trunk). A later layout of the snapshot expands it again and
continues from the kept box. The canonical graph never holds the virtual node. Fans (one shared end) and buses with a node on
both sides (a gateway) stay with the drawing step alone. Rationale against reserving the trunk in the channel planning only:
the ordering and placement would not know the bus, and a bus as an element can later carry its own label and be selected,
collapsed and expanded in a drill-down.

### Amendment C: bus labels and collapsed ECUs (2026-11-08)

Status: provisional. A bus may carry a `label` ("CAN 500 kbit/s"). Like an edge label it is placed by the engine, collision
free, beside the longest stretches of its members' routes (its trunk first), when the options give its measured size under
`edgeLabels["bus:<id>"]`; the playground's edge-label switch measures and passes it. Collapsing a container (an ECU) needs no
bus rule of its own: the view moves the members' ends onto the collapsed container and keeps the edge ids, so the bus is laid out
and drawn with the visible ends; a bus that becomes a fan this way (all members from one collapsed ECU) is drawn as a fan. Fans
as layout elements of their own were measured and not kept (three times the crossings on the corpus).
