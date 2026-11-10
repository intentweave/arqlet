//! Ports of a curated reference. A port is the attachment of one route end to one node side. Its position is derived
//! from the node bounds, the side and the offset, where the offset is a fraction of the side length, as the engine
//! places ports. Moving a node therefore moves its ports. A port's offset is the curator's choice unless the port is
//! explicit: explicit ports come from the input configuration and stay locked.
//!
//! Re-routing searches only the interior of a route. The endpoints are set from the port records first, so a
//! re-routed route stays attached to its ports.

use std::collections::{BTreeMap, BTreeSet};

use arqlet_model::{Edge, Graph, Point, PortSide, Rect, ResolvedPort, Snapshot};

use crate::route;

use crate::document::{CuratedScene, PortRecord};

/// Derived and stored positions agree within this distance (scene units).
pub const TOLERANCE: f64 = 1e-6;

/// The position of a port on a node side, from the node bounds and the offset (a fraction of the side length).
pub fn derive_position(rect: &Rect, side: PortSide, offset: f64) -> Point {
    match side {
        PortSide::Left => Point {
            x: rect.x,
            y: rect.y + rect.height * offset,
        },
        PortSide::Right => Point {
            x: rect.x + rect.width,
            y: rect.y + rect.height * offset,
        },
        PortSide::Top => Point {
            x: rect.x + rect.width * offset,
            y: rect.y,
        },
        PortSide::Bottom => Point {
            x: rect.x + rect.width * offset,
            y: rect.y + rect.height,
        },
    }
}

/// The port records of a snapshot, against the curated node bounds.
pub fn from_snapshot(
    resolved: &[ResolvedPort],
    nodes: &BTreeMap<String, Rect>,
) -> Result<BTreeMap<String, PortRecord>, String> {
    resolved
        .iter()
        .map(|port| {
            let rect = nodes
                .get(&port.node_id)
                .ok_or_else(|| format!("Port {} names no node", port.id))?;
            let offset = match port.side {
                PortSide::Left | PortSide::Right => (port.position.y - rect.y) / rect.height,
                PortSide::Top | PortSide::Bottom => (port.position.x - rect.x) / rect.width,
            };
            let record = PortRecord {
                node_id: port.node_id.clone(),
                side: port.side,
                edge_id: port.edge_id.clone(),
                endpoint: port.endpoint.clone(),
                offset,
                position: port.position.clone(),
                explicit: port.explicit,
            };
            Ok((port.id.clone(), record))
        })
        .collect()
}

/// Writes the derived position of every port. Used when an edited draft is refreshed.
pub fn sync(curated: &mut CuratedScene) -> Result<(), String> {
    let CuratedScene { nodes, ports, .. } = curated;
    for (id, port) in ports.iter_mut() {
        let rect = nodes
            .get(&port.node_id)
            .ok_or_else(|| format!("Port {id} names no node"))?;
        port.position = derive_position(rect, port.side, port.offset);
    }
    Ok(())
}

/// Checks the port records of a document. The set of ports must be the baseline's, explicit ports must be unchanged,
/// every stored position must be derived from its node, and every route that is not pending must meet its ports.
pub fn check(curated: &CuratedScene, baseline: &Snapshot) -> Result<(), String> {
    let known: BTreeSet<&str> = baseline
        .resolved_ports
        .iter()
        .map(|p| p.id.as_str())
        .collect();
    let present: BTreeSet<&str> = curated.ports.keys().map(String::as_str).collect();
    if known != present {
        return Err("The ports do not match the baseline".into());
    }
    let pending: BTreeSet<&str> = curated.pending_routes.iter().map(String::as_str).collect();
    let baseline_nodes: BTreeMap<String, Rect> = arqlet_scene::build_scene(baseline)?
        .nodes
        .iter()
        .map(|node| (node.node.id.clone(), node.bounds.clone()))
        .collect();
    for (id, port) in &curated.ports {
        if port.endpoint != "source" && port.endpoint != "target" {
            return Err(format!(
                "Port {id} has an unknown endpoint {}",
                port.endpoint
            ));
        }
        if !(-TOLERANCE..=1.0 + TOLERANCE).contains(&port.offset) {
            return Err(format!("Port {id} lies outside its side"));
        }
        let rect = curated
            .nodes
            .get(&port.node_id)
            .ok_or_else(|| format!("Port {id} names no node"))?;
        let derived = derive_position(rect, port.side, port.offset);
        if !same(&derived, &port.position) {
            return Err(format!(
                "Port {id} disagrees with its node: its position is not derived from node, side and offset"
            ));
        }
        if let Some(original) = baseline.resolved_ports.iter().find(|p| p.id == *id) {
            let base_rect = baseline_nodes
                .get(&port.node_id)
                .ok_or_else(|| format!("Port {id} names no baseline node"))?;
            if port.explicit && !same_offset(original, port, base_rect) {
                return Err(format!("Explicit port {id} is locked and cannot change"));
            }
        }
        if !pending.contains(port.edge_id.as_str()) {
            if let Some(route) = curated.routes.get(&port.edge_id) {
                if !route
                    .windows(2)
                    .all(|pair| axis_aligned(&pair[0], &pair[1]))
                {
                    return Err(format!("Route {} has a diagonal segment", port.edge_id));
                }
                let end = if port.endpoint == "source" {
                    route.first()
                } else {
                    route.last()
                };
                if !end.is_some_and(|point| same(point, &port.position)) {
                    return Err(format!(
                        "Route {} does not meet its port {id}",
                        port.edge_id
                    ));
                }
            }
        }
    }
    Ok(())
}

fn horizontal_side(side: PortSide) -> bool {
    matches!(side, PortSide::Left | PortSide::Right)
}

fn axis_aligned(a: &Point, b: &Point) -> bool {
    (a.x - b.x).abs() <= TOLERANCE || (a.y - b.y).abs() <= TOLERANCE
}

/// Makes every segment orthogonal, adding the fewest bends. A route leaves its source port perpendicular to the
/// source side and arrives at its target port perpendicular to the target side, when those sides are known.
/// Repeated and collinear points are removed afterwards, so the result has no zero-length or straight-through bends.
pub fn orthogonalize(
    points: &[Point],
    source: Option<PortSide>,
    target: Option<PortSide>,
) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let last = points.len() - 1;
    let mut out: Vec<Point> = vec![points[0].clone()];
    for (index, point) in points.iter().enumerate().skip(1) {
        let previous = out.last().cloned().expect("non-empty");
        if same(&previous, point) {
            continue;
        }
        if !axis_aligned(&previous, point) {
            let bend = if index == 1 {
                match source {
                    Some(side) if horizontal_side(side) => Point {
                        x: point.x,
                        y: previous.y,
                    },
                    _ => Point {
                        x: previous.x,
                        y: point.y,
                    },
                }
            } else if index == last {
                match target {
                    Some(side) if horizontal_side(side) => Point {
                        x: previous.x,
                        y: point.y,
                    },
                    Some(_) => Point {
                        x: point.x,
                        y: previous.y,
                    },
                    None => Point {
                        x: point.x,
                        y: previous.y,
                    },
                }
            } else {
                Point {
                    x: previous.x,
                    y: point.y,
                }
            };
            out.push(bend);
        }
        out.push(point.clone());
    }
    remove_straight(out)
}

/// Drops interior points that lie on a straight line between their neighbours. The endpoints always stay.
fn remove_straight(points: Vec<Point>) -> Vec<Point> {
    if points.len() < 3 {
        return points;
    }
    let mut result: Vec<Point> = vec![points[0].clone()];
    for index in 1..points.len() - 1 {
        let a = result.last().expect("non-empty");
        let b = &points[index];
        let c = &points[index + 1];
        let straight = ((a.x - b.x).abs() <= TOLERANCE && (b.x - c.x).abs() <= TOLERANCE)
            || ((a.y - b.y).abs() <= TOLERANCE && (b.y - c.y).abs() <= TOLERANCE);
        if !straight && !same(a, b) {
            result.push(b.clone());
        }
    }
    result.push(points[points.len() - 1].clone());
    result
}

/// Re-routes the pending routes. Each route is searched against the routes that stay in place, the nodes are
/// obstacles, and its endpoints are its ports. A first pass routes them one after another; a second pass routes each
/// again against the final layout, so earlier routes do not keep their first detour. Clears the pending list.
pub fn reroute(curated: &mut CuratedScene, graph: &Graph) -> Result<(), String> {
    let pending: BTreeSet<String> = curated.pending_routes.iter().cloned().collect();
    for id in &pending {
        if !curated.routes.contains_key(id) {
            return Err(format!("Pending route {id} has no route"));
        }
        for port in curated.ports.values() {
            if port.edge_id != *id {
                continue;
            }
            let route = curated.routes.get_mut(id).expect("checked above");
            let end = if port.endpoint == "source" {
                route.first_mut()
            } else {
                route.last_mut()
            };
            if let Some(end) = end {
                *end = port.position.clone();
            }
        }
    }
    let edges: BTreeMap<&str, &Edge> = graph
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect();
    // Port positions and every current route vertex are grid lines, so the search can run beside them.
    let mut tracks: Vec<Point> = curated
        .ports
        .values()
        .map(|port| port.position.clone())
        .collect();
    for points in curated.routes.values() {
        tracks.extend(points.iter().cloned());
    }
    let mut settled: BTreeSet<String> = BTreeSet::new();
    // A route that cannot be found stays pending and is reported; the other routes are still re-routed.
    let mut unroutable: BTreeSet<String> = BTreeSet::new();
    for pass in 0..route::PASSES {
        for id in &pending {
            let edge = edges
                .get(id.as_str())
                .ok_or_else(|| format!("Pending route {id} names no edge"))?;
            let (start, start_side) = endpoint(curated, id, "source");
            let (end, end_side) = endpoint(curated, id, "target");
            let (start, end) = (
                start.ok_or_else(|| format!("Route {id} has no source port"))?,
                end.ok_or_else(|| format!("Route {id} has no target port"))?,
            );
            // The endpoint nodes and their ancestors are not obstacles: the route starts and ends at their sides.
            // Only ancestor containers are passable. The endpoint nodes are obstacles: a route touches them
            // at their ports and must not run through their interiors.
            let mut passable = ancestors(graph, &edge.source);
            if let Some(target) = edge.target.as_deref() {
                passable.extend(ancestors(graph, target));
            }
            passable.remove(edge.source.as_str());
            if let Some(target) = edge.target.as_deref() {
                passable.remove(target);
            }
            let obstacles: Vec<Rect> = curated
                .nodes
                .iter()
                .filter(|(node, _)| !passable.contains(node.as_str()))
                .map(|(_, rect)| rect.clone())
                .collect();
            let fixed: Vec<Vec<Point>> = curated
                .routes
                .iter()
                .filter(|(other, _)| other.as_str() != id.as_str())
                .filter(|(other, _)| {
                    pass >= 1
                        || !pending.contains(other.as_str())
                        || settled.contains(other.as_str())
                })
                .map(|(_, points)| points.clone())
                .collect();
            let grid_rects: Vec<Rect> = curated.nodes.values().cloned().collect();
            let request = route::Request {
                start: start.clone(),
                start_side,
                end: end.clone(),
                end_side,
                obstacles: &obstacles,
                grid_rects: &grid_rects,
                fixed: &fixed,
                tracks: &tracks,
            };
            let Some(routed) = route::route(&request) else {
                unroutable.insert(id.clone());
                continue;
            };
            unroutable.remove(id);
            let points = orthogonalize(&routed.points, start_side, end_side);
            curated.routes.insert(id.clone(), points);
            settled.insert(id.clone());
        }
    }
    // Only the routes that were routed leave the pending list; an unroutable route stays pending, so the draft is not
    // valid and the curator sees exactly which route is left.
    curated.pending_routes = unroutable.into_iter().collect();
    Ok(())
}

/// The position and side of the port that serves one end of an edge.
fn endpoint(curated: &CuratedScene, edge: &str, end: &str) -> (Option<Point>, Option<PortSide>) {
    curated
        .ports
        .values()
        .find(|port| port.edge_id == edge && port.endpoint == end)
        .map(|port| (Some(port.position.clone()), Some(port.side)))
        .unwrap_or((None, None))
}

/// The node itself and every ancestor container.
fn ancestors(graph: &Graph, node: &str) -> BTreeSet<String> {
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|item| (item.id.as_str(), item.parent_id.as_deref()))
        .collect();
    let mut result = BTreeSet::new();
    let mut current = Some(node);
    while let Some(id) = current {
        if !result.insert(id.to_string()) {
            break;
        }
        current = parents.get(id).copied().flatten();
    }
    result
}

fn same(a: &Point, b: &Point) -> bool {
    (a.x - b.x).abs() <= TOLERANCE && (a.y - b.y).abs() <= TOLERANCE
}

/// An explicit port is unchanged when its offset equals the baseline offset on the baseline node bounds.
fn same_offset(original: &ResolvedPort, port: &PortRecord, base: &Rect) -> bool {
    let baseline_offset = match original.side {
        PortSide::Left | PortSide::Right => (original.position.y - base.y) / base.height,
        PortSide::Top | PortSide::Bottom => (original.position.x - base.x) / base.width,
    };
    (baseline_offset - port.offset).abs() <= TOLERANCE
}

#[cfg(test)]
mod tests {
    use crate::document::{create, refresh, reroute, validate, ReferenceDocument};
    use arqlet_model::Snapshot;
    use serde_json::{json, Value};

    fn snapshot() -> Snapshot {
        serde_json::from_str(include_str!(
            "../../../packages/viewer/fixtures/state-tcp.snapshot.json"
        ))
        .unwrap()
    }

    /// A node with a route, moved sideways. Every route that touches it is pending, as the editor marks them.
    /// Returns the document and one such route.
    fn moved_node() -> (Value, String) {
        let mut document: ReferenceDocument = create(&snapshot(), "state-tcp").unwrap();
        let (node, route) = document
            .input
            .graph
            .edges
            .iter()
            .filter(|edge| edge.target.is_some() && document.curated.routes.contains_key(&edge.id))
            .map(|edge| (edge.source.clone(), edge.id.clone()))
            .next()
            .unwrap();
        document.curated.nodes.get_mut(&node).unwrap().x += 40.0;
        let touching: Vec<String> = document
            .input
            .graph
            .edges
            .iter()
            .filter(|edge| edge.source == node || edge.target.as_deref() == Some(node.as_str()))
            .map(|edge| edge.id.clone())
            .filter(|id| document.curated.routes.contains_key(id))
            .collect();
        document.curated.pending_routes = touching;
        (serde_json::to_value(&document).unwrap(), route)
    }

    #[test]
    fn a_moved_node_refreshes_with_pending_routes_and_its_ports_follow() {
        let (value, route) = moved_node();
        let refreshed = refresh(&value).unwrap();
        assert!(refreshed.curated.pending_routes.contains(&route));
        let report = validate(&serde_json::to_value(&refreshed).unwrap()).unwrap();
        assert!(report.pending_routes > 0);
        assert!(!report.valid);
    }

    #[test]
    fn reroute_clears_pending_routes_and_keeps_every_route_on_its_ports() {
        let (value, _) = moved_node();
        let refreshed = serde_json::to_value(refresh(&value).unwrap()).unwrap();
        let rerouted = serde_json::to_value(reroute(&refreshed).unwrap()).unwrap();
        let report = validate(&rerouted).unwrap();
        assert_eq!(report.pending_routes, 0);
        assert!(report.current_validation);
    }

    #[test]
    fn reroute_without_pending_routes_is_refused() {
        let value = serde_json::to_value(create(&snapshot(), "state-tcp").unwrap()).unwrap();
        assert_eq!(
            reroute(&value).unwrap_err(),
            "There are no pending routes to re-route"
        );
    }

    #[test]
    fn a_port_position_that_disagrees_with_its_node_is_rejected() {
        let mut document = create(&snapshot(), "state-tcp").unwrap();
        let port = document.curated.ports.values_mut().next().unwrap();
        port.position.x += 3.0;
        crate::document::seal(&mut document).unwrap();
        let error = validate(&serde_json::to_value(&document).unwrap()).unwrap_err();
        assert!(error.contains("disagrees with its node"), "{error}");
    }
}

#[cfg(test)]
mod sweep_tests {
    use crate::document::{create, refresh, reroute, validate, ReferenceDocument};
    use arqlet_model::Snapshot;

    /// Every movable port of the TCP fixture, moved both ways and re-routed: each result must keep all routes
    /// orthogonal and attached to their ports, and the document must validate.
    #[test]
    fn every_port_move_reroutes_to_an_orthogonal_route_on_its_ports() {
        let snapshot: Snapshot = serde_json::from_str(include_str!(
            "../../../packages/viewer/fixtures/state-tcp.snapshot.json"
        ))
        .unwrap();
        let base: ReferenceDocument = create(&snapshot, "state-tcp").unwrap();
        let movable: Vec<(String, String)> = base
            .curated
            .ports
            .iter()
            .filter(|(_, port)| !port.explicit && base.curated.routes.contains_key(&port.edge_id))
            .map(|(id, port)| (id.clone(), port.edge_id.clone()))
            .collect();
        assert!(!movable.is_empty());
        for (port_id, edge_id) in movable {
            for step in [0.2, -0.2] {
                let mut edited = base.clone();
                let port = edited.curated.ports.get_mut(&port_id).unwrap();
                let offset = port.offset + step;
                if !(0.0..=1.0).contains(&offset) {
                    continue;
                }
                port.offset = offset;
                edited.curated.pending_routes = vec![edge_id.clone()];
                edited.revision += 1;
                let value = serde_json::to_value(&edited).unwrap();
                let refreshed = serde_json::to_value(refresh(&value).unwrap()).unwrap();
                let rerouted = reroute(&refreshed).unwrap();
                for (id, points) in &rerouted.curated.routes {
                    for pair in points.windows(2) {
                        let straight = (pair[0].x - pair[1].x).abs() < 1e-6
                            || (pair[0].y - pair[1].y).abs() < 1e-6;
                        assert!(
                            straight,
                            "{port_id} {step}: route {id} has a diagonal segment"
                        );
                    }
                }
                let report = validate(&serde_json::to_value(&rerouted).unwrap()).unwrap();
                assert_eq!(report.pending_routes, 0, "{port_id} {step}");
                assert!(report.current_validation, "{port_id} {step}");
            }
        }
    }
}

#[cfg(test)]
mod moved_tests {
    use crate::document::{refresh, reroute, validate, ReferenceDocument};
    use serde_json::Value;

    /// A saved draft of the TCP fixture after moving CLOSE_WAIT and LAST_ACK, with five routes pending. Re-routing
    /// must give a valid draft: no node hits and no crossings. Before the grid search it had crossings and node hits.
    #[test]
    fn the_moved_tcp_draft_re_routes_without_crossings_or_node_hits() {
        let value: Value = serde_json::from_str(include_str!(
            "../../../examples/fixtures/reference/state-tcp-moved.lwref.json"
        ))
        .unwrap();
        let before: ReferenceDocument = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(before.curated.pending_routes.len(), 5);
        let refreshed = serde_json::to_value(refresh(&value).unwrap()).unwrap();
        let rerouted = serde_json::to_value(reroute(&refreshed).unwrap()).unwrap();
        let report = validate(&rerouted).unwrap();
        assert!(report.valid, "{:?}", report.metrics);
        assert_eq!(report.hard_violations, 0);
        assert_eq!(report.metrics.crossings, 0);
        assert_eq!(report.metrics.line_node_hits, 0);
        assert_eq!(report.pending_routes, 0);
    }
}
