//! Scene v1: renderer-neutral absolute geometry derived from an engine snapshot.
//!
//! The scene only projects a snapshot. It runs no layout, does not alter graph semantics and does
//! not invent routes. Container coordinates are flattened once: consumers must not apply a parent
//! transform to routes or labels again.
use crate::{geom, realize};
use arqlet_model::math::PortableMath;
use arqlet_model::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub schema_version: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub layout_version: u64,
    pub nodes: Vec<SceneNode>,
    pub routes: Vec<SceneRoute>,
    pub ports: Vec<ResolvedPort>,
    pub labels: Vec<PlacedLabel>,
    pub crossings: Vec<BoundaryCrossing>,
    pub show_ports: bool,
    /// What does not hold in the drawing, with every party of it: overlaps and children that do not fit a container, authored
    /// sizes below their content, routes through nodes. Omitted when there is none, so a clean drawing is unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<SceneConflict>,
    /// Explicit buses (ADR 0005), each as its edges' ids, for a renderer and for the shared conflict queries (members of one
    /// bus may share lines). Omitted when there is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buses: Vec<Vec<String>>,
    /// Where the lines of a bus branch: points at which its drawn routes run off in three or more directions. A renderer marks
    /// them as junction dots. Omitted when there is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub junctions: Vec<Point>,
    /// Extent of everything drawn, with a 30-unit margin. May start at negative coordinates.
    pub bounds: Rect,
}

/// One conflict of the drawing. A renderer marks every node and route it names and shows `reason` for them.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneConflict {
    /// The kind: a geometry issue's kind (`node_overlap`, `container_bounds`, `minimum_gap`, `container-fixed-below-content`, ...)
    /// or `route-through-node`.
    pub kind: String,
    /// The nodes it involves, and the routes (edge IDs).
    pub nodes: Vec<String>,
    pub routes: Vec<String>,
    pub reason: String,
}

/// A row of a node's content, with its absolute bounds (ADR 0011). The viewer draws the content inside it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneRow {
    pub id: String,
    pub label: String,
    pub bounds: Rect,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneNode {
    #[serde(flatten)]
    pub node: Node,
    /// Absolute bounds in scene coordinates.
    pub bounds: Rect,
    pub container: bool,
    pub aggregate: bool,
    pub collapsed: bool,
    pub invalid: bool,
    /// The rows of the node's content, top to bottom (empty for a node without rows).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<SceneRow>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneRoute {
    pub id: String,
    /// The edge's kind: for a relationship, its type (the verb), drawn as the route's label.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    pub source: String,
    pub target: String,
    pub points: Vec<Point>,
    pub weight: usize,
    /// Provisional (communities): the route stands for collapsed content in both directions; this many edges run the
    /// other way (from `target` to `source`), `weight` in its own. Present only on such routes, 0 when every one of them
    /// runs one way; a renderer draws an arrow at the start too when it is above 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_weight: Option<usize>,
    /// True when the engine reported a structured conflict for this route.
    pub invalid: bool,
    /// True when the route is drawn with rounded corners (styles `roundedOrthogonal` and `roundedPolyline`). Its points
    /// already contain the arcs, sampled by the engine (`realize`); a renderer draws them as they are. Omitted otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rounded: bool,
    /// Edges this drawn route stands for (a bundle leader's members). Absent when the route is one edge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
}

/// How far a polyline route keeps from the nodes it passes (layout units).
const POLYLINE_CLEARANCE: f64 = 4.;

/// The same for a rounded polyline: the arcs cut the corners, so the bends keep further from the nodes.
const ROUNDED_POLYLINE_CLEARANCE: f64 = 10.;

/// Builds scene v1 from a snapshot. Bundle members are omitted: they share their leader's route.
/// The junctions of the buses (`Scene::junctions`): the points of their members' routes at which the bus's lines run off in
/// three or more directions, in a fixed order.
fn junctions_of(snapshot: &Snapshot, routes: &[SceneRoute]) -> Vec<Point> {
    const EPS: f64 = 1e-6;
    let mut out: Vec<Point> = Vec::new();
    for bus in &snapshot.graph.buses {
        let lines: Vec<&Vec<Point>> = routes
            .iter()
            .filter(|r| bus.edges.contains(&r.id))
            .map(|r| &r.points)
            .collect();
        let segments: Vec<(&Point, &Point)> = lines
            .iter()
            .flat_map(|p| p.windows(2).map(|w| (&w[0], &w[1])))
            .collect();
        for p in lines.iter().flat_map(|p| p.iter()) {
            // The directions in which the bus's segments leave `p` (a segment through it leaves both ways).
            let mut directions: Vec<(f64, f64)> = Vec::new();
            for &(a, b) in &segments {
                let on = |q: &Point| (q.x - p.x).abs() < EPS && (q.y - p.y).abs() < EPS;
                let toward = |q: &Point| {
                    let (dx, dy) = (q.x - p.x, q.y - p.y);
                    let l = dx.p_hypot(dy).max(EPS);
                    (dx / l, dy / l)
                };
                let inside = {
                    let (x0, x1) = (a.x.min(b.x) - EPS, a.x.max(b.x) + EPS);
                    let (y0, y1) = (a.y.min(b.y) - EPS, a.y.max(b.y) + EPS);
                    let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                    cross.abs() < EPS && p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1
                };
                if !inside {
                    continue;
                }
                for q in [a, b] {
                    if !on(q) {
                        let d = toward(q);
                        if !directions
                            .iter()
                            .any(|e| (e.0 - d.0).abs() < 1e-3 && (e.1 - d.1).abs() < 1e-3)
                        {
                            directions.push(d);
                        }
                    }
                }
            }
            if directions.len() >= 3
                && !out
                    .iter()
                    .any(|q| (q.x - p.x).abs() < EPS && (q.y - p.y).abs() < EPS)
            {
                out.push(p.clone());
            }
        }
    }
    out.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    out
}

pub fn build_scene(snapshot: &Snapshot) -> Result<Scene, String> {
    build_scene_with(snapshot, true)
}

/// The scene with the corner points of every route: pulled taut where the style asks for it, but without the sampled arcs
/// of rounded styles. The metrics measure this geometry (bends are corners, not samples of an arc).
pub fn build_corner_scene(snapshot: &Snapshot) -> Result<Scene, String> {
    build_scene_with(snapshot, false)
}

fn build_scene_with(snapshot: &Snapshot, arcs: bool) -> Result<Scene, String> {
    if snapshot.schema_version != 1 {
        return Err("Unsupported snapshot schema".into());
    }
    let graph = &snapshot.graph;
    let by_id: BTreeMap<&str, &Node> = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let source = snapshot.source_graph.as_ref().unwrap_or(graph);
    let aggregates: BTreeSet<&str> = source
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let closed: BTreeSet<&str> = snapshot.collapsed_ids.iter().map(String::as_str).collect();
    let invalid: BTreeSet<&str> = snapshot
        .geometry_issues
        .iter()
        .flat_map(|issue| issue.node_ids.iter().map(String::as_str))
        .collect();
    let conflicted: BTreeSet<&str> = snapshot
        .route_conflicts
        .iter()
        .map(|c| c.edge_id.as_str())
        .collect();

    let mut positions: BTreeMap<&str, Rect> = BTreeMap::new();
    let mut nodes = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let bounds = absolute(
            &node.id,
            &by_id,
            snapshot,
            &mut positions,
            &mut BTreeSet::new(),
        )?;
        let rows = node_rows(snapshot, &node.id, &bounds);
        nodes.push(SceneNode {
            node: node.clone(),
            bounds,
            container: containers.contains(node.id.as_str()),
            aggregate: aggregates.contains(node.id.as_str()),
            collapsed: closed.contains(node.id.as_str()),
            invalid: invalid.contains(node.id.as_str()),
            rows,
        });
    }

    let straight: BTreeSet<&str> = snapshot.straight_edges.iter().map(String::as_str).collect();
    let edges: BTreeMap<&str, &Edge> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut routes = Vec::new();
    let weighted = snapshot.weighted_routes();
    let mut pulled_ids: BTreeSet<String> = BTreeSet::new();
    for route in snapshot.routes.iter().filter(|r| r.bundle_of.is_none()) {
        let edge = edges
            .get(route.id.as_str())
            .ok_or_else(|| format!("Missing route endpoints: {}", route.id))?;
        let target = edge
            .target
            .as_deref()
            .filter(|t| by_id.contains_key(edge.source.as_str()) && by_id.contains_key(t))
            .ok_or_else(|| format!("Missing route endpoints: {}", route.id))?;
        if route
            .points
            .iter()
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err(format!("Invalid route: {}", route.id));
        }
        let constraint = (!straight.contains(route.id.as_str()))
            .then(|| graph.routing_constraint(graph.edge_area(&edge.source, target).as_deref()))
            .flatten();
        let pulled = |c: &RoutingConstraint| {
            [RoutingStyle::Polyline, RoutingStyle::RoundedPolyline]
                .into_iter()
                .find(|s| c.picks(*s))
        };
        let style = constraint.as_ref().and_then(pulled);
        let rounds = constraint.as_ref().is_some_and(|c| c.rounds());
        // A container that holds an endpoint is traversable for the edge; every other node is an obstacle, except the two
        // endpoints, which the route may leave and reach along their border. A pulled route keeps a distance; a rounded
        // orthogonal route only must not run into a node.
        let obstacles = |clearance: f64| -> Vec<geom::Rect> {
            let holding: BTreeSet<&str> = [edge.source.as_str(), target]
                .into_iter()
                .flat_map(|id| {
                    std::iter::successors(by_id.get(id).copied(), |n| {
                        n.parent_id.as_deref().and_then(|p| by_id.get(p).copied())
                    })
                })
                .map(|n| n.id.as_str())
                .collect();
            positions
                .iter()
                .filter(|(id, _)| !holding.contains(**id) || **id == edge.source || **id == target)
                .map(|(id, r)| {
                    let rect = geom::Rect {
                        x0: r.x,
                        y0: r.y,
                        x1: r.x + r.width,
                        y1: r.y + r.height,
                    };
                    if *id == edge.source || *id == target {
                        rect
                    } else {
                        rect.inflate(clearance)
                    }
                })
                .collect()
        };
        let clearance = match style {
            Some(RoutingStyle::RoundedPolyline) => ROUNDED_POLYLINE_CLEARANCE,
            Some(_) => POLYLINE_CLEARANCE,
            None => 0.,
        };
        let mut points = route.points.clone();
        if style.is_some() || rounds {
            let blocks = obstacles(clearance);
            if style.is_some() {
                pulled_ids.insert(route.id.clone());
                points = geom::pull_taut(&points, &blocks);
            }
            if rounds && arcs {
                points = realize::round_corners(&points, realize::CORNER_RADIUS, &blocks);
            }
        }
        routes.push(SceneRoute {
            id: route.id.clone(),
            kind: edge.kind.clone(),
            source: edge.source.clone(),
            target: target.to_string(),
            points,
            // An edge that stands for collapsed content: its weights come from the view (`aggregate_weights`), whichever
            // path computed the route.
            weight: match &weighted {
                // With input weights: the edges it stands for (`Snapshot::weighted_routes`).
                Some(w) => w.get(&route.id).copied().unwrap_or(route.weight),
                None => snapshot
                    .options
                    .aggregate_weights
                    .get(&route.id)
                    .map_or(route.weight, |w| w.forward.max(1)),
            },
            back_weight: snapshot
                .options
                .aggregate_weights
                .get(&route.id)
                .map(|w| w.backward),
            invalid: conflicted.contains(route.id.as_str()),
            rounded: rounds,
            members: Vec::new(),
        });
    }
    // Bundle members are not drawn; their leader's route names them so a selection can show what it carries.
    for member in snapshot
        .routes
        .iter()
        .filter_map(|r| r.bundle_of.as_ref().map(|leader| (leader, &r.id)))
    {
        if let Some(leader) = routes.iter_mut().find(|r| &r.id == member.0) {
            leader.members.push(member.1.clone());
        }
    }

    let labels = snapshot.labels.clone();
    let mut left: f64 = 0.;
    let mut top: f64 = 0.;
    let mut right: f64 = 1.;
    let mut bottom: f64 = 1.;
    let mut include = |r: &Rect| {
        left = left.min(r.x);
        top = top.min(r.y);
        right = right.max(r.x + r.width);
        bottom = bottom.max(r.y + r.height);
    };
    for node in &nodes {
        include(&node.bounds);
    }
    for label in &labels {
        include(&label.bounds);
    }
    for route in &routes {
        for p in &route.points {
            include(&Rect {
                x: p.x,
                y: p.y,
                width: 0.,
                height: 0.,
            });
        }
    }

    // The crossings of the snapshot are those of its orthogonal routes. A pulled route crosses the borders elsewhere, so
    // its crossings are found again on the drawn points.
    let mut crossings: Vec<BoundaryCrossing> = snapshot
        .boundary_crossings
        .iter()
        .filter(|c| !pulled_ids.contains(&c.edge_id))
        .cloned()
        .collect();
    let mut container_ids: Vec<&str> = containers.iter().copied().collect();
    container_ids.sort_unstable();
    for route in routes.iter().filter(|r| pulled_ids.contains(&r.id)) {
        for id in &container_ids {
            let Some(r) = positions.get(id) else { continue };
            let border = geom::Rect {
                x0: r.x,
                y0: r.y,
                x1: r.x + r.width,
                y1: r.y + r.height,
            };
            for (ordinal, (side, position)) in geom::border_crossings(&route.points, &border)
                .into_iter()
                .enumerate()
            {
                crossings.push(BoundaryCrossing {
                    edge_id: route.id.clone(),
                    container_id: id.to_string(),
                    ordinal,
                    side,
                    position,
                    derived: true,
                });
            }
        }
    }

    let junctions = junctions_of(snapshot, &routes);
    Ok(Scene {
        schema_version: 1,
        revision: snapshot.revision,
        layout_version: snapshot.layout_version,
        nodes,
        routes,
        ports: snapshot.resolved_ports.clone(),
        labels,
        crossings,
        show_ports: snapshot.options.appearance.show_ports,
        conflicts: conflicts_of(snapshot),
        buses: snapshot
            .graph
            .buses
            .iter()
            .map(|b| b.edges.clone())
            .collect(),
        junctions,
        bounds: Rect {
            x: left - 30.,
            y: top - 30.,
            width: right - left + 60.,
            height: bottom - top + 60.,
        },
    })
}

/// Absolute rectangle of a node, computed once per node. Cycles and invalid rectangles are errors.
fn absolute<'a>(
    id: &'a str,
    by_id: &BTreeMap<&'a str, &'a Node>,
    snapshot: &'a Snapshot,
    positions: &mut BTreeMap<&'a str, Rect>,
    visiting: &mut BTreeSet<&'a str>,
) -> Result<Rect, String> {
    if let Some(known) = positions.get(id) {
        return Ok(known.clone());
    }
    if !visiting.insert(id) {
        return Err(format!("Cyclic scene containment: {id}"));
    }
    let node = by_id
        .get(id)
        .ok_or_else(|| format!("Missing scene geometry: {id}"))?;
    let mut rect = snapshot
        .boxes
        .get(id)
        .ok_or_else(|| format!("Missing scene geometry: {id}"))?
        .clone();
    if let Some(parent) = node.parent_id.as_deref() {
        let p = absolute(parent, by_id, snapshot, positions, visiting)?;
        rect.x += p.x;
        rect.y += p.y;
    }
    if ![rect.x, rect.y, rect.width, rect.height]
        .iter()
        .all(|v| v.is_finite())
        || rect.width <= 0.
        || rect.height <= 0.
    {
        return Err(format!("Invalid rectangle: {id}"));
    }
    visiting.remove(id);
    positions.insert(id, rect.clone());
    Ok(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(overrides: serde_json::Value) -> Snapshot {
        let mut value = json!({
            "schemaVersion": 1,
            "engineVersion": arqlet_model::ENGINE_VERSION,
            "revision": 3,
            "layoutVersion": 5,
            "graph": {
                "nodes": [
                    {"id": "group", "kind": "container", "label": "Group", "parentId": null},
                    {"id": "child", "kind": "symbol", "label": "Child", "parentId": "group"},
                    {"id": "other", "kind": "symbol", "label": "Other", "parentId": null}
                ],
                "edges": [{"id": "e", "kind": "calls", "source": "child", "target": "other"}]
            },
            "boxes": {
                "group": {"x": 100, "y": 100, "width": 300, "height": 200},
                "child": {"x": 20, "y": 50, "width": 100, "height": 50},
                "other": {"x": 450, "y": 150, "width": 160, "height": 70}
            },
            "routes": [{"id": "e", "points": [{"x": 220, "y": 175}, {"x": 450, "y": 175}]}],
            "recomputedContainers": [],
            "omittedUnresolvedEdges": 0
        });
        for (key, extra) in overrides.as_object().unwrap() {
            value[key] = extra.clone();
        }
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn nested_geometry_is_flattened_once_and_bounds_include_labels() {
        let mut s = snapshot(json!({"labels": [{
            "id": "e", "kind": "edge", "text": "calls", "bounds": {"x": -40, "y": -60, "width": 80, "height": 22}
        }]}));
        s.options.appearance.show_ports = false;
        let scene = build_scene(&s).unwrap();
        let child = scene.nodes.iter().find(|n| n.node.id == "child").unwrap();
        assert_eq!((child.bounds.x, child.bounds.y), (120., 150.));
        assert!(
            scene
                .nodes
                .iter()
                .find(|n| n.node.id == "group")
                .unwrap()
                .container
        );
        assert!(!scene.show_ports);
        assert_eq!(scene.routes.len(), 1);
        assert_eq!(scene.routes[0].source, "child");
        assert_eq!(scene.routes[0].target, "other");
        // Label reaches x = -40; bounds add the 30-unit margin on each side.
        assert_eq!(scene.bounds.x, -70.);
        assert_eq!(scene.bounds.y, -90.);
        assert_eq!(scene.revision, 3);
        assert_eq!(scene.layout_version, 5);
    }

    #[test]
    fn a_rounded_route_carries_its_arcs_and_the_style_stays() {
        let bend = json!([{"x": 220, "y": 175}, {"x": 335, "y": 175}, {"x": 335, "y": 185}, {"x": 450, "y": 185}]);
        let mut s = snapshot(json!({"routes": [{"id": "e", "points": bend}]}));
        s.graph.default_routing = Some(RoutingConstraint::Fixed(RoutingStyle::RoundedOrthogonal));
        let scene = build_scene(&s).unwrap();
        let route = &scene.routes[0];
        assert!(route.rounded);
        // Two corners of a 10-unit step: each is rounded with a radius limited by the step, so the route has the arc points.
        assert!(route.points.len() > 4, "{:?}", route.points);
        assert_eq!(route.points.first().map(|p| p.x), Some(220.));
        assert_eq!(route.points.last().map(|p| p.x), Some(450.));
        // The style is not given up where there is room: the same route with the orthogonal style keeps its corners.
        s.graph.default_routing = Some(RoutingConstraint::Fixed(RoutingStyle::Orthogonal));
        let plain = build_scene(&s).unwrap();
        assert!(!plain.routes[0].rounded);
        assert_eq!(plain.routes[0].points.len(), 4);
    }

    #[test]
    fn the_scene_names_every_party_of_each_conflict() {
        let clean = build_scene(&snapshot(json!({}))).unwrap();
        assert!(clean.conflicts.is_empty());
        assert!(
            serde_json::to_value(&clean)
                .unwrap()
                .get("conflicts")
                .is_none(),
            "a clean drawing carries no conflicts field"
        );
        let s = snapshot(json!({
            "geometryIssues": [{"kind": "node_overlap", "nodeIds": ["child", "other"], "message": "Unrelated node rectangles overlap"}],
            "routeConflicts": [{"edgeId": "e", "nodeId": "other"}]
        }));
        let scene = build_scene(&s).unwrap();
        assert_eq!(scene.conflicts.len(), 2);
        let overlap = &scene.conflicts[0];
        assert_eq!(overlap.kind, "node_overlap");
        assert_eq!(overlap.nodes, ["child", "other"]);
        assert!(overlap.routes.is_empty());
        let through = &scene.conflicts[1];
        assert_eq!(through.kind, "route-through-node");
        assert_eq!(
            (through.routes.as_slice(), through.nodes.as_slice()),
            (&["e".to_string()][..], &["other".to_string()][..])
        );
        assert!(through.reason.contains("passes through"));
    }

    #[test]
    fn bundle_members_are_omitted_and_conflicts_are_flagged() {
        let s = snapshot(json!({
            "routes": [
                {"id": "e", "points": [{"x": 220, "y": 175}, {"x": 450, "y": 175}], "weight": 2},
                {"id": "f", "points": [{"x": 220, "y": 175}, {"x": 450, "y": 175}], "bundle_of": "e"}
            ],
            "routeConflicts": [{"edgeId": "e", "nodeId": "other"}]
        }));
        let scene = build_scene(&s).unwrap();
        assert_eq!(scene.routes.len(), 1);
        assert_eq!(scene.routes[0].weight, 2);
        assert!(scene.routes[0].invalid);
    }

    #[test]
    fn containment_cycles_and_invalid_rectangles_fail_explicitly() {
        let mut cyclic = snapshot(json!({}));
        cyclic.graph.nodes[0].parent_id = Some("child".into());
        assert!(build_scene(&cyclic)
            .unwrap_err()
            .starts_with("Cyclic scene containment"));

        let flat = snapshot(json!({"boxes": {
            "group": {"x": 100, "y": 100, "width": 0, "height": 200},
            "child": {"x": 20, "y": 50, "width": 100, "height": 50},
            "other": {"x": 450, "y": 150, "width": 160, "height": 70}
        }}));
        assert!(build_scene(&flat)
            .unwrap_err()
            .starts_with("Invalid rectangle: group"));

        let mut missing_target = snapshot(json!({}));
        missing_target.graph.edges[0].target = Some("nowhere".into());
        assert!(build_scene(&missing_target)
            .unwrap_err()
            .starts_with("Missing route endpoints"));

        let schema = snapshot(json!({"schemaVersion": 2}));
        assert!(build_scene(&schema).is_err());
    }
}

/// The rows of a node, stacked from the top inside its padding (ADR 0011), as the model places them.
fn node_rows(snapshot: &Snapshot, id: &str, bounds: &Rect) -> Vec<SceneRow> {
    let Some(rows) = snapshot.options.node_rows.get(id) else {
        return Vec::new();
    };
    arqlet_model::row_bounds(bounds, rows)
        .into_iter()
        .zip(rows)
        .map(|((_, bounds), row)| SceneRow {
            id: row.id.clone(),
            label: row.label.clone(),
            bounds,
        })
        .collect()
}

/// The conflicts of a snapshot as the scene reports them: the geometry issues, then the routes through nodes. Order follows the
/// snapshot, so the same snapshot gives the same list.
fn conflicts_of(snapshot: &Snapshot) -> Vec<SceneConflict> {
    let issues = snapshot.geometry_issues.iter().map(|issue| SceneConflict {
        kind: issue.kind.clone(),
        nodes: issue.node_ids.clone(),
        routes: Vec::new(),
        reason: issue.message.clone(),
    });
    let routes = snapshot.route_conflicts.iter().map(|c| SceneConflict {
        kind: "route-through-node".into(),
        nodes: vec![c.node_id.clone()],
        routes: vec![c.edge_id.clone()],
        reason: format!("Edge {} passes through node {}", c.edge_id, c.node_id),
    });
    issues.chain(routes).collect()
}
