//! Place measured labels in free scene space. Never silently overlap a node or route.
use super::*;
fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width + 4.
        && b.x < a.x + a.width + 4.
        && a.y < b.y + b.height + 4.
        && b.y < a.y + a.height + 4.
}
/// The edge–node conflicts of the drawn routes, as (edge, node) in route order: a route that runs through a node other than
/// its own ends. The diagnostics and the port search use this same test; it does not place labels.
pub(super) fn route_node_conflicts(snapshot: &Snapshot) -> Vec<(String, String)> {
    let graph = &snapshot.graph;
    let rects = absolute_all(graph, &snapshot.boxes);
    let shapes: BTreeMap<&str, arqlet_model::NodeShape> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.shape))
        .collect();
    // The same outline test as the router: a circle is hit by its disc only.
    let outlines: Vec<(&String, arqlet_model::outline::Outline)> = rects
        .iter()
        .map(|(id, rect)| {
            let shape = shapes.get(id.as_str()).copied().unwrap_or_default();
            (id, arqlet_model::outline::Outline::of(shape, rect))
        })
        .collect();
    // The first edge of every id, as a search of the edges finds it.
    let mut edges: BTreeMap<&str, &Edge> = BTreeMap::new();
    for e in &graph.edges {
        edges.entry(e.id.as_str()).or_insert(e);
    }
    let exclusions = arqlet_routing::Exclusions::new(graph);
    let drawn: Vec<&Route> = snapshot
        .routes
        .iter()
        .filter(|r| r.bundle_of.is_none())
        .collect();
    let prepared = arqlet_routing::PreparedRoutes::new(&drawn);
    let mut found = Vec::new();
    for (i, route) in drawn.iter().enumerate() {
        if let Some(edge) = edges.get(route.id.as_str()) {
            let skip = exclusions.of(edge);
            for &(id, ref outline) in &outlines {
                if prepared.hits_outline(i, outline)
                    && (!skip.contains(id)
                        || id == &edge.source
                        || edge.target.as_ref() == Some(id))
                {
                    found.push((route.id.clone(), id.clone()));
                }
            }
        }
    }
    found
}

pub(super) fn decorate(snapshot: &mut Snapshot) {
    snapshot.labels.clear();
    // What `apply_straight_edges` found about the drawn segments stays: it is not derived here.
    let straight_findings: Vec<String> = if snapshot.straight_edges.is_empty() {
        Vec::new()
    } else {
        snapshot
            .routing_issues
            .iter()
            .filter(|issue| issue.starts_with(crate::styles::STRAIGHT_ISSUE))
            .cloned()
            .collect()
    };
    snapshot.routing_issues = crate::pin_diagnostics(&snapshot.graph, &snapshot.options);
    snapshot.routing_issues.extend(straight_findings);
    snapshot
        .routing_issues
        .extend(crate::styles::capability_diagnostics(snapshot));
    snapshot.route_conflicts.clear();
    let graph = &snapshot.graph;
    let rects = absolute_all(graph, &snapshot.boxes);
    let containers: BTreeSet<_> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_ref())
        .collect();
    // Circles are checked by their disc, the other nodes by their rectangle (containers with their header height).
    let shapes: BTreeMap<&str, arqlet_model::NodeShape> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.shape))
        .collect();
    let mut discs: Vec<arqlet_model::outline::Outline> = Vec::new();
    let mut occupied: Vec<Rect> = Vec::new();
    for (id, r) in &rects {
        if shapes.get(id.as_str()) == Some(&arqlet_model::NodeShape::Circle) {
            discs.push(arqlet_model::outline::Outline::of(
                arqlet_model::NodeShape::Circle,
                r,
            ));
            continue;
        }
        let mut r = r.clone();
        if containers.contains(id) {
            r.height = 36.;
        }
        occupied.push(r);
    }
    for (edge_id, node_id) in route_node_conflicts(snapshot) {
        snapshot
            .routing_issues
            .push(format!("Edge {edge_id} passes through node {node_id}"));
        snapshot
            .route_conflicts
            .push(RouteConflict { edge_id, node_id });
    }
    let routes: Vec<_> = snapshot
        .routes
        .iter()
        .filter(|r| r.bundle_of.is_none())
        .collect();
    let mut jobs = Vec::new();
    for (id, label) in &snapshot.options.edge_labels {
        // Every labelled member retains its own text even when sharing a route.
        if let Some(route) = snapshot.routes.iter().find(|r| &r.id == id) {
            let mut candidates = Vec::new();
            // The closest free place to the edge comes first: a label sits next to its own edge, not near another one.
            // Positions along each segment are tried at the same distance before the next distance.
            for gap in [8., 24., 48., 72., 96.] {
                for w in route.points.windows(2) {
                    for fraction in [0.5, 0.25, 0.75, 0.1, 0.9, 0.4, 0.6] {
                        let x = w[0].x + (w[1].x - w[0].x) * fraction;
                        let y = w[0].y + (w[1].y - w[0].y) * fraction;
                        if (w[0].y - w[1].y).abs() < 1e-6 {
                            candidates.extend([
                                (x - label.width / 2., y - gap - label.height),
                                (x - label.width / 2., y + gap),
                            ]);
                        } else {
                            candidates.extend([
                                (x - gap - label.width, y - label.height / 2.),
                                (x + gap, y - label.height / 2.),
                            ]);
                        }
                    }
                }
            }
            jobs.push((id.clone(), "edge", label.clone(), candidates));
        }
    }
    // A bus's label (`Bus::label`, its size under `edge_labels["bus:<id>"]`): next to the longest stretches of its members'
    // routes first, which is its trunk, at the same places an edge label tries.
    for bus in &snapshot.graph.buses {
        let (Some(text), Some(size)) = (
            bus.label.as_ref(),
            snapshot.options.edge_labels.get(&format!("bus:{}", bus.id)),
        ) else {
            continue;
        };
        let mut segments: Vec<(&Point, &Point)> = snapshot
            .routes
            .iter()
            .filter(|r| bus.edges.contains(&r.id) && r.bundle_of.is_none())
            .flat_map(|r| r.points.windows(2).map(|w| (&w[0], &w[1])))
            .collect();
        let length = |s: &(&Point, &Point)| (s.0.x - s.1.x).abs() + (s.0.y - s.1.y).abs();
        segments.sort_by(|a, b| length(b).total_cmp(&length(a)));
        let mut candidates = Vec::new();
        for gap in [8., 24., 48.] {
            for (a, b) in &segments {
                for fraction in [0.5, 0.25, 0.75, 0.1, 0.9] {
                    let x = a.x + (b.x - a.x) * fraction;
                    let y = a.y + (b.y - a.y) * fraction;
                    if (a.y - b.y).abs() < 1e-6 {
                        candidates.extend([
                            (x - size.width / 2., y - gap - size.height),
                            (x - size.width / 2., y + gap),
                        ]);
                    } else {
                        candidates.extend([
                            (x - gap - size.width, y - size.height / 2.),
                            (x + gap, y - size.height / 2.),
                        ]);
                    }
                }
            }
        }
        let mut label = size.clone();
        label.text = text.clone();
        jobs.push((bus.id.clone(), "bus", label, candidates));
    }
    for (id, label) in &snapshot.options.port_labels {
        if let Some(port) = snapshot.resolved_ports.iter().find(|p| &p.id == id) {
            let mut candidates = Vec::new();
            let p = &port.position;
            for gap in [8., 24., 48.] {
                for tangent in [8., 24., 48., 96.] {
                    match port.side {
                        PortSide::Left => candidates.extend([
                            (p.x - gap - label.width, p.y + tangent),
                            (p.x - gap - label.width, p.y - tangent - label.height),
                        ]),
                        PortSide::Right => candidates.extend([
                            (p.x + gap, p.y + tangent),
                            (p.x + gap, p.y - tangent - label.height),
                        ]),
                        PortSide::Top => candidates.extend([
                            (p.x + tangent, p.y - gap - label.height),
                            (p.x - tangent - label.width, p.y - gap - label.height),
                        ]),
                        PortSide::Bottom => candidates.extend([
                            (p.x + tangent, p.y + gap),
                            (p.x - tangent - label.width, p.y + gap),
                        ]),
                    }
                }
            }
            jobs.push((id.clone(), "port", label.clone(), candidates));
        }
    }
    for (id, kind, label, mut candidates) in jobs {
        // Where the user put the label comes first; the automatic places follow if it no longer fits.
        let wanted = snapshot
            .options
            .label_positions
            .get(&id)
            .map(|p| (p.x, p.y));
        if let Some(at) = wanted {
            candidates.insert(0, at);
        }
        let found = candidates
            .into_iter()
            .map(|(x, y)| Rect {
                x,
                y,
                width: label.width,
                height: label.height,
            })
            .find(|r| {
                !occupied.iter().any(|o| overlaps(r, o))
                    && !discs.iter().any(|d| {
                        arqlet_model::outline::within(
                            &arqlet_model::outline::Outline::Rect(r.clone()),
                            d,
                            4.,
                        )
                    })
                    && !routes.iter().any(|route| arqlet_routing::hits(route, r))
            });
        if let Some(bounds) = found {
            if wanted
                .is_some_and(|(x, y)| (bounds.x - x).abs() > 1e-6 || (bounds.y - y).abs() > 1e-6)
            {
                snapshot.routing_issues.push(format!(
                    "The {kind} label {id} does not fit where it was put; it is placed again"
                ));
            }
            occupied.push(bounds.clone());
            snapshot.labels.push(PlacedLabel {
                id,
                kind: kind.into(),
                text: label.text,
                bounds,
            });
        } else {
            snapshot
                .routing_issues
                .push(format!("No free space for the {kind} label {id}"));
        }
    }
}
