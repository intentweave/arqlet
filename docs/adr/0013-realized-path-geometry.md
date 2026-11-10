# ADR 0013: One drawn path: rounded corners are computed in the scene

Date: 2026-10-27.
Status: accepted and implemented for the rounded styles; the labels and the border crossings still use the router's corner geometry.

## Context

The viewer rounded the corners of `roundedOrthogonal` and `roundedPolyline` routes itself (`roundedPoints`), while the metrics, the
conflict key and the reference export used the router's corners. What was drawn, what was judged and what was exported could differ,
and the rounded arc could cut a node.

## Decision

The scene computes the drawn path once (`crates/scene/src/realize.rs`, `round_corners`) and the consumers use it:

- A corner takes the radius that fits the adjacent segments (12 at most). When its arc would enter a node, the radius halves until it
  stays clear; when none does, the corner stays sharp. **The style never changes** where there is room.
- `build_scene` returns the points with the arcs sampled (`SceneRoute.rounded` says they are in the points). The viewer draws the points as
  given and has no rounding function of its own.
- `build_corner_scene` returns the corners (pulled taut for a polyline, no arcs). The published drawing (`published_drawing`: key,
  metrics, quality) and the reference document are built from it, so bends are corners, not samples of an arc. Only a graph that sets a
  routing style is affected; the default measures the router's routes as before.
- The reference SVG export derives the rounded routes from the document's baseline view and draws the same arcs with the same function,
  so the export shows what the viewer shows and still round-trips byte for byte.

## Consequences

- `browser-checks/rounded-routes.mjs`: a rounded style is drawn with the engine's arcs (57 to 231 points on `state-order`) and no drawn
  point lies inside a node.
- Not done: labels and the border crossings of containers are placed from the corner geometry; an arc near a corner can come closer to
  a label than the corner does. The radius is a constant (12), not an option.
