//! Edge routing styles per edge: the requirement of each edge's area (authored, else the legacy option, else orthogonal),
//! the choice where a requirement admits several drawn styles, and the straight segments of the chosen edges.
//! Orthogonal edges keep the routes of the orthogonal router; straight edges are replaced by one segment between their ports.

use super::{absolute_all, ports, published_key};
#[cfg(test)]
use arqlet_model::math::PortableMath;
use arqlet_model::outline::Outline;
use arqlet_model::*;
use std::collections::{BTreeMap, BTreeSet};

/// The style the legacy option names.
fn legacy_style(style: EdgeRouting) -> RoutingStyle {
    match style {
        EdgeRouting::Straight => RoutingStyle::Straight,
        EdgeRouting::Orthogonal => RoutingStyle::Orthogonal,
    }
}

/// The edges that cannot be straight, with the reason. None today: a self-loop has no second end to draw a segment to, so it
/// is never drawn straight, but that is not a refusal: it follows the loop rule (see `edge_choices`). Parallel and
/// antiparallel edges are drawn: each takes its own ports, so the segments lie side by side (see `PortContext::resolve_nodes`).
fn straight_refusals(_graph: &Graph) -> BTreeMap<String, String> {
    BTreeMap::new()
}

/// The drawn styles an edge may take under its area's requirement, the orthogonal baseline first. Refusals name the reason.
fn candidates(
    requirement: &RoutingConstraint,
    refusal: Option<&String>,
    who: &str,
) -> Result<Vec<RoutingStyle>, String> {
    let straight_ok = refusal.is_none();
    match requirement {
        RoutingConstraint::Fixed(style) if !style.supported() => Err(format!(
            "routing style {} is not drawn yet (for {who}); the drawn styles are straight, orthogonal, roundedOrthogonal, polyline and roundedPolyline",
            style.name()
        )),
        RoutingConstraint::Fixed(RoutingStyle::Straight) => match refusal {
            Some(reason) => Err(reason.clone()),
            None => Ok(vec![RoutingStyle::Straight]),
        },
        RoutingConstraint::Fixed(style) => Ok(vec![*style]),
        RoutingConstraint::Auto => Ok(std::iter::once(RoutingStyle::Orthogonal)
            .chain(straight_ok.then_some(RoutingStyle::Straight))
            .collect()),
        RoutingConstraint::Allowed(list) => {
            let drawn: Vec<RoutingStyle> = [
                RoutingStyle::Orthogonal,
                RoutingStyle::RoundedOrthogonal,
                RoutingStyle::Polyline,
                RoutingStyle::RoundedPolyline,
                RoutingStyle::Straight,
            ]
            .into_iter()
                .filter(|s| list.contains(s) && (*s != RoutingStyle::Straight || straight_ok))
                .collect();
            if drawn.is_empty() {
                Err(format!("no drawn routing style is allowed for {who}"))
            } else {
                Ok(drawn)
            }
        }
    }
}

/// Scores an assignment on the whole drawing: the straight segments, the orthogonal routes around them, then the key.
/// A drawing that cannot be keyed ranks after every keyed one.
fn score(snapshot: &Snapshot, straight: &BTreeSet<String>) -> (bool, Vec<i64>) {
    let mut trial = snapshot.clone();
    let routed = apply_straight_edges(&mut trial, straight).and_then(|()| {
        route_orthogonal_around(
            &mut trial,
            straight,
            &|| false,
            &mut arqlet_routing::channel::SearchCounters::default(),
        )
        .map_err(|_| "interrupted".to_string())
    });
    match routed {
        Ok(()) => match published_key(&trial) {
            Ok(key) => (false, key),
            Err(_) => (true, vec![]),
        },
        Err(_) => (true, vec![]),
    }
}

/// Routes the orthogonal edges again, each paying for its crossings with the straight segments, which stay fixed. The
/// straight segments are (re)placed first from the current ports, so they follow moved nodes and ports. Nothing changes
/// when no edge is straight, so an all-orthogonal drawing keeps its routes.
pub(crate) fn route_orthogonal_around(
    snapshot: &mut Snapshot,
    straight: &BTreeSet<String>,
    stop: &dyn Fn() -> bool,
    counters: &mut arqlet_routing::channel::SearchCounters,
) -> Result<(), arqlet_routing::channel::Stopped> {
    if straight.is_empty() {
        return Ok(());
    }
    let port_started = crate::stage_timer_start();
    apply_straight_edges(snapshot, straight).ok();
    counters.port_resolution_nanos = counters
        .port_resolution_nanos
        .saturating_add(crate::elapsed_nanos(port_started));
    let fixed: Vec<Route> = snapshot
        .routes
        .iter()
        .filter(|r| straight.contains(&r.id))
        .cloned()
        .collect();
    let orthogonal: Vec<Route> = snapshot
        .routes
        .iter()
        .filter(|r| !straight.contains(&r.id))
        .cloned()
        .collect();
    let rects = absolute_all(&snapshot.graph, &snapshot.boxes);
    let routing_started = crate::stage_timer_start();
    let improved = arqlet_routing::improve_until_counted_with_fixed(
        &snapshot.graph,
        &rects,
        &orthogonal,
        &fixed,
        super::routing_params(&snapshot.options),
        stop,
        counters,
    );
    counters.routing_nanos = counters
        .routing_nanos
        .saturating_add(crate::elapsed_nanos(routing_started));
    let improved = improved?;
    let points: BTreeMap<String, Vec<Point>> =
        improved.into_iter().map(|r| (r.id, r.points)).collect();
    for route in snapshot.routes.iter_mut() {
        if let Some(points) = points
            .get(&route.id)
            .filter(|_| !straight.contains(&route.id))
        {
            route.points = points.clone();
        }
    }
    let port_started = crate::stage_timer_start();
    apply_straight_edges(snapshot, straight).ok();
    counters.port_resolution_nanos = counters
        .port_resolution_nanos
        .saturating_add(crate::elapsed_nanos(port_started));
    Ok(())
}

/// The straight edges of a drawing, chosen from the requirements of the snapshot's graph and options. Where a requirement
/// admits several drawn styles, the choice is made per area: each style is tried for that area's edges and kept when the
/// conflict key of the whole drawing is better. Ties keep the orthogonal baseline; the order of a list is no preference.
/// The drawn styles each edge may take, and the areas whose edges have a choice. Known from the graph and the options alone.
type Choices = (
    BTreeMap<String, Vec<RoutingStyle>>,
    BTreeMap<String, Vec<String>>,
);

/// Whether every edge of the graph can only be drawn straight (no other style is admitted): its orthogonal routes are then
/// never drawn, except those `route_orthogonal_around` makes for the edges a straight line cannot take.
pub(crate) fn only_straight(graph: &Graph, options: &LayoutOptions) -> bool {
    edge_choices(graph, options).is_ok_and(|(choices, _)| {
        choices
            .values()
            .all(|list| list.iter().all(|s| *s == RoutingStyle::Straight))
    })
}

fn edge_choices(graph: &Graph, options: &LayoutOptions) -> Result<Choices, String> {
    let refusals = straight_refusals(graph);
    let legacy = options.edge_routing.map(legacy_style);
    let mut choices: BTreeMap<String, Vec<RoutingStyle>> = BTreeMap::new();
    let mut areas: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for edge in graph.edges.iter().filter(|e| e.target.is_some()) {
        let target = edge.target.as_deref().unwrap_or_default();
        let area = graph.edge_area(&edge.source, target);
        let who = area.as_deref().unwrap_or("the root");
        let authored = graph.routing_constraint(area.as_deref());
        let requirement = match (&authored, legacy) {
            (Some(c), Some(style)) if !c.admits(style) => {
                return Err(format!(
                    "routing conflict for {who}: the layout option routes {} but the graph requires {}; remove one of them",
                    style.name(),
                    match c {
                        RoutingConstraint::Fixed(s) => s.name().to_string(),
                        RoutingConstraint::Auto => "auto".to_string(),
                        RoutingConstraint::Allowed(list) => format!(
                            "one of [{}]",
                            list.iter().map(|s| s.name()).collect::<Vec<_>>().join(", ")
                        ),
                    }
                ))
            }
            (Some(c), _) => c.clone(),
            (None, Some(style)) => RoutingConstraint::Fixed(style),
            (None, None) => RoutingConstraint::Fixed(RoutingStyle::Orthogonal),
        };
        let mut list = candidates(&requirement, refusals.get(&edge.id), who)?;
        // The loop rule: an edge that returns to its node is never drawn straight, whatever its area asks for. It takes the
        // other styles the area admits, else the orthogonal loop; ordinary edges of the area are drawn as asked.
        if target == edge.source {
            list.retain(|s| *s != RoutingStyle::Straight);
            if list.is_empty() {
                list.push(RoutingStyle::Orthogonal);
            }
        }
        if list.len() > 1 {
            areas
                .entry(area.unwrap_or_default())
                .or_default()
                .push(edge.id.clone());
        }
        choices.insert(edge.id.clone(), list);
    }
    Ok((choices, areas))
}

/// Whether every edge can only be drawn straight (its requirement allows no other style). Then no route runs in a corridor.
pub(crate) fn all_edges_straight(graph: &Graph, options: &LayoutOptions) -> bool {
    edge_choices(graph, options).is_ok_and(|(choices, _)| {
        !choices.is_empty()
            && choices
                .values()
                .all(|list| list.iter().all(|style| *style == RoutingStyle::Straight))
    })
}

/// The edges that may be drawn straight, from the graph and the options alone. A placement takes them into account before
/// the routes exist. An edge that could not be given a style is left out here; `plan_straight_edges` reports it.
pub(crate) fn straight_candidates(graph: &Graph, options: &LayoutOptions) -> BTreeSet<String> {
    edge_choices(graph, options)
        .map(|(choices, _)| {
            choices
                .into_iter()
                .filter(|(_, list)| list.contains(&RoutingStyle::Straight))
                .map(|(id, _)| id)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn plan_straight_edges(snapshot: &Snapshot) -> Result<BTreeSet<String>, String> {
    let (choices, areas) = edge_choices(&snapshot.graph, &snapshot.options)?;
    // Start from each edge's first candidate: the baseline where it is allowed, else its only drawn style.
    let mut current: BTreeSet<String> = choices
        .iter()
        .filter(|(_, list)| list[0] == RoutingStyle::Straight)
        .map(|(id, _)| id.clone())
        .collect();
    // Only a choice needs a key: without one the baseline stands and nothing is scored.
    let mut best = (!areas.is_empty()).then(|| score(snapshot, &current));
    for members in areas.values() {
        for style in [RoutingStyle::Orthogonal, RoutingStyle::Straight] {
            let mut trial = current.clone();
            for id in members {
                let admitted = match style {
                    RoutingStyle::Orthogonal => choices[id]
                        .iter()
                        .any(|s| !matches!(s, RoutingStyle::Straight)),
                    _ => choices[id].contains(&style),
                };
                if admitted && style == RoutingStyle::Straight {
                    trial.insert(id.clone());
                } else if admitted {
                    trial.remove(id);
                }
            }
            if trial == current {
                continue;
            }
            let trial_score = score(snapshot, &trial);
            if best.as_ref().is_none_or(|b| trial_score < *b) {
                best = Some(trial_score);
                current = trial;
            }
        }
    }
    Ok(current)
}

/// What the channel router cannot draw in this snapshot, as diagnostics: it routes around rectangles only and does not see
/// straight segments. The diagnostics are part of the result, so a refinement or a repeated decoration reports them too.
pub(crate) fn capability_diagnostics(snapshot: &Snapshot) -> Vec<String> {
    if snapshot.options.routing_engine != RoutingEngine::Channel {
        return vec![];
    }
    let orthogonal = snapshot
        .graph
        .edges
        .iter()
        .filter(|e| e.target.is_some())
        .any(|e| !snapshot.straight_edges.contains(&e.id));
    if !orthogonal {
        return vec![];
    }
    let mut out = Vec::new();
    if snapshot
        .graph
        .nodes
        .iter()
        .any(|n| n.shape == NodeShape::Circle)
    {
        out.push("channel router: circle outlines are not routed around; the orthogonal edges ignore them, use the legacy engine".to_string());
    }
    if !snapshot.straight_edges.is_empty() {
        out.push("channel router: the orthogonal edges do not see the straight edges; use the legacy engine for mixed routing".to_string());
    }
    out
}

/// The start of the routing issues that `apply_straight_edges` reports about the drawn segments.
pub(crate) const STRAIGHT_ISSUE: &str = "straight edge ";

/// Replaces the routes of the straight edges by one segment between their ports, in edge order. Orthogonal routes are kept.
/// Foreign nodes that a segment enters are routing issues. Nothing changes when no edge is straight.
/// The detour of a straight edge (`straightDetours`): its current route in the snapshot (the orthogonal one on the first
/// application, the detour itself on a later one) pulled taut around the boxes of every node it does not lie in, its own
/// ends included (it leaves and reaches them along their border, as the scene's pulled routes do), if that route has a
/// bend and the result enters none of the other nodes' outlines.
fn detour(
    snapshot: &Snapshot,
    edge: &str,
    ends: (&str, &str),
    holding: &BTreeSet<&str>,
    absolute: &BTreeMap<String, Rect>,
    outlines: &BTreeMap<&str, Outline>,
) -> Option<Vec<Point>> {
    let route = snapshot
        .routes
        .iter()
        .find(|r| r.id == edge && r.bundle_of.is_none())?;
    if route.points.len() < 3 {
        return None;
    }
    let obstacles: Vec<arqlet_scene::geom::Rect> = absolute
        .iter()
        .filter(|(id, _)| {
            !holding.contains(id.as_str()) || id.as_str() == ends.0 || id.as_str() == ends.1
        })
        .map(|(_, r)| arqlet_scene::geom::Rect {
            x0: r.x,
            y0: r.y,
            x1: r.x + r.width,
            y1: r.y + r.height,
        })
        .collect();
    let points = arqlet_scene::geom::pull_taut(&route.points, &obstacles);
    let clear = points.windows(2).all(|w| {
        outlines.iter().all(|(node, outline)| {
            holding.contains(node) || !outline.segment_enters(w[0].x, w[0].y, w[1].x, w[1].y, 0.)
        })
    });
    clear.then_some(points)
}

pub fn apply_straight_edges(
    snapshot: &mut Snapshot,
    straight: &BTreeSet<String>,
) -> Result<(), String> {
    if straight.is_empty() {
        return Ok(());
    }
    let graph = &snapshot.graph;
    // The same port rule as the routing and the published ports (flow sides with the experimental layered construction).
    let resolved = ports::PortContext::new(graph)
        .with_flow_sides(snapshot.options.layered_construction)
        .resolve_with(
            &snapshot.boxes,
            snapshot.options.port_placement,
            snapshot.options.min_port_spacing,
            &snapshot.options.node_rows,
            snapshot.options.row_port_order,
        );
    let absolute = absolute_all(graph, &snapshot.boxes);
    // Each node's real outline: a circle is its disc, so a segment through the empty corner of its box is not an entry.
    let outlines: BTreeMap<&str, Outline> = graph
        .nodes
        .iter()
        .map(|n| {
            (
                n.id.as_str(),
                Outline::of(n.shape, &absolute[n.id.as_str()]),
            )
        })
        .collect();
    let mut routes = Vec::new();
    let mut issues = Vec::new();
    for e in graph.edges.iter().filter(|e| e.target.is_some()) {
        let target = e.target.as_deref().unwrap_or_default();
        if straight.contains(&e.id) {
            // A container that holds an endpoint is traversable for that endpoint's edge: its interior is not an obstacle.
            let holding = [e.source.as_str(), target]
                .into_iter()
                .flat_map(|id| {
                    std::iter::successors(Some(id), |id| {
                        graph
                            .nodes
                            .iter()
                            .find(|n| n.id == *id)
                            .and_then(|n| n.parent_id.as_deref())
                    })
                })
                .collect::<BTreeSet<&str>>();
            if e.source == target {
                return Err(format!(
                    "straight routing does not draw the self-loop {}",
                    e.id
                ));
            }
            let position = |endpoint: &str| {
                resolved
                    .iter()
                    .find(|p| p.edge_id == e.id && p.endpoint == endpoint)
                    .map(|p| p.position.clone())
            };
            let (Some(start), Some(end)) = (position("source"), position("target")) else {
                continue;
            };
            let blocked: Vec<&str> = outlines
                .iter()
                .filter(|(node, outline)| {
                    !holding.contains(*node)
                        && outline.segment_enters(start.x, start.y, end.x, end.y, 0.)
                })
                .map(|(node, _)| *node)
                .collect();
            // Experimental (`straightDetours`, ADR 0020 S1-1b): around the blockers, on the edge's earlier route pulled taut
            // around every node the edge does not end at or lie in; kept only when that route is clear of them.
            let detour = (snapshot.options.straight_detours && !blocked.is_empty())
                .then(|| {
                    detour(
                        snapshot,
                        &e.id,
                        (e.source.as_str(), target),
                        &holding,
                        &absolute,
                        &outlines,
                    )
                })
                .flatten();
            if detour.is_none() {
                for node in blocked {
                    issues.push(format!("straight edge {} passes through node {node}", e.id));
                }
            }
            routes.push(Route {
                id: e.id.clone(),
                points: detour.unwrap_or_else(|| vec![start, end]),
                weight: 1,
                bundle_of: None,
            });
        } else if let Some(route) = snapshot.routes.iter().find(|r| r.id == e.id) {
            if route.bundle_of.is_some()
                || snapshot
                    .routes
                    .iter()
                    .any(|r| r.bundle_of.as_deref() == Some(e.id.as_str()))
            {
                return Err(format!(
                    "bundled orthogonal edge {} cannot be mixed with straight edges yet",
                    e.id
                ));
            }
            routes.push(route.clone());
        }
    }
    // With `straightDetours` the findings for the straight edges follow their drawn routes: the placement's finding for a
    // pair of a straight edge is replaced by one per node that edge's route actually enters, so a detour withdraws it and a
    // later application without one (the route drawn straight again) reports it again. Findings for other pairs stay.
    if snapshot.options.straight_detours {
        let pairs: BTreeSet<(&str, &str)> = graph
            .edges
            .iter()
            .filter(|e| straight.contains(&e.id))
            .filter_map(|e| e.target.as_deref().map(|t| (e.source.as_str(), t)))
            .flat_map(|(a, b)| [(a, b), (b, a)])
            .collect();
        let mut derived: Vec<GeometryIssue> = Vec::new();
        for e in graph.edges.iter().filter(|e| straight.contains(&e.id)) {
            let (Some(target), Some(route)) =
                (e.target.as_deref(), routes.iter().find(|r| r.id == e.id))
            else {
                continue;
            };
            let holding: BTreeSet<&str> = [e.source.as_str(), target]
                .into_iter()
                .flat_map(|id| {
                    std::iter::successors(Some(id), |id| {
                        graph
                            .nodes
                            .iter()
                            .find(|n| n.id == *id)
                            .and_then(|n| n.parent_id.as_deref())
                    })
                })
                .collect();
            for (node, outline) in &outlines {
                let entered = !holding.contains(node)
                    && route
                        .points
                        .windows(2)
                        .any(|w| outline.segment_enters(w[0].x, w[0].y, w[1].x, w[1].y, 0.));
                let issue = GeometryIssue {
                    kind: "straight-through-node".into(),
                    node_ids: vec![e.source.clone(), target.to_string(), node.to_string()],
                    message: format!(
                        "straight edge between {} and {target} passes through node {node}",
                        e.source
                    ),
                };
                if entered && !derived.iter().any(|d| d.node_ids == issue.node_ids) {
                    derived.push(issue);
                }
            }
        }
        snapshot.geometry_issues.retain(|issue| {
            issue.kind != "straight-through-node"
                || !matches!(issue.node_ids.as_slice(), [a, b, _] if pairs.contains(&(a.as_str(), b.as_str())))
        });
        snapshot.geometry_issues.extend(derived);
    }
    snapshot.routes = routes;
    snapshot.straight_edges = straight.iter().cloned().collect();
    // Where the drawn routes meet a container's border follows the routes: derived again from the segments just placed.
    let drawn: Vec<Route> = snapshot
        .routes
        .iter()
        .filter(|r| r.bundle_of.is_none())
        .cloned()
        .collect();
    snapshot.boundary_crossings =
        ports::boundaries(&snapshot.graph, &snapshot.boxes, &drawn, straight);
    // The findings of an earlier application describe earlier segments: they are replaced, not accumulated.
    snapshot
        .routing_issues
        .retain(|issue| !issue.starts_with(STRAIGHT_ISSUE));
    for issue in issues {
        if !snapshot.routing_issues.contains(&issue) {
            snapshot.routing_issues.push(issue);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A graph and its snapshot, with the boxes given (relative to their parents) instead of the laid-out ones.
    fn with_boxes(json: &str, boxes: &[(&str, f64, f64, f64, f64)], straight: &[&str]) -> Snapshot {
        let graph: Graph = serde_json::from_str(json).unwrap();
        let mut snapshot =
            crate::compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        for &(id, x, y, width, height) in boxes {
            snapshot.boxes.insert(
                id.into(),
                Rect {
                    x,
                    y,
                    width,
                    height,
                },
            );
        }
        let straight: BTreeSet<String> = straight.iter().map(|s| s.to_string()).collect();
        apply_straight_edges(&mut snapshot, &straight).unwrap();
        snapshot
    }

    /// The edge `cd` from `c` (inside container `g`) to `d` at the root. Its own container may be crossed: the boundary
    /// of an edge's endpoint container is not an obstacle to that edge.
    #[test]
    fn a_straight_edge_may_leave_its_own_container() {
        let json = r#"{"nodes":[{"id":"g","kind":"group","label":"G","parentId":null},{"id":"c","kind":"node","label":"C","parentId":"g"},{"id":"d","kind":"node","label":"D","parentId":null}],"edges":[{"id":"cd","kind":"flow","source":"c","target":"d"}]}"#;
        let s = with_boxes(
            json,
            &[
                ("g", 0., 0., 200., 300.),
                ("c", 40., 60., 50., 50.),
                ("d", 420., 100., 50., 50.),
            ],
            &["cd"],
        );
        assert!(s.routing_issues.is_empty(), "{:?}", s.routing_issues);
    }

    /// A sibling container that the edge crosses is protected: the edge is an issue, not a silent pass through it.
    #[test]
    fn a_straight_edge_may_not_cross_a_sibling_container() {
        let json = r#"{"nodes":[{"id":"g","kind":"group","label":"G","parentId":null},{"id":"c","kind":"node","label":"C","parentId":"g"},{"id":"h","kind":"group","label":"H","parentId":null},{"id":"k","kind":"node","label":"K","parentId":"h"},{"id":"d","kind":"node","label":"D","parentId":null}],"edges":[{"id":"cd","kind":"flow","source":"c","target":"d"}]}"#;
        let s = with_boxes(
            json,
            &[
                ("g", 0., 0., 200., 300.),
                ("c", 40., 60., 50., 50.),
                ("h", 250., 0., 100., 300.),
                ("k", 20., 60., 40., 40.),
                ("d", 420., 100., 50., 50.),
            ],
            &["cd"],
        );
        assert!(
            s.routing_issues
                .iter()
                .any(|i| i == "straight edge cd passes through node h"),
            "{:?}",
            s.routing_issues
        );
    }

    /// Nested containers: the edge may leave both of its endpoint's containers, but a sibling of the inner one, inside the
    /// outer one, stays protected. The issue names that sibling only.
    #[test]
    fn nested_boundaries_are_crossed_only_where_authorized() {
        let json = r#"{"nodes":[{"id":"g1","kind":"group","label":"G1","parentId":null},{"id":"g2","kind":"group","label":"G2","parentId":"g1"},{"id":"c","kind":"node","label":"C","parentId":"g2"},{"id":"h1","kind":"group","label":"H1","parentId":"g1"},{"id":"k1","kind":"node","label":"K1","parentId":"h1"},{"id":"d","kind":"node","label":"D","parentId":null}],"edges":[{"id":"cd","kind":"flow","source":"c","target":"d"}]}"#;
        let s = with_boxes(
            json,
            &[
                ("g1", 0., 0., 400., 300.),
                ("g2", 40., 40., 120., 200.),
                ("c", 20., 60., 40., 40.),
                ("h1", 200., 0., 100., 300.),
                ("k1", 20., 60., 40., 40.),
                ("d", 500., 100., 50., 50.),
            ],
            &["cd"],
        );
        assert!(
            s.routing_issues
                .iter()
                .any(|i| i == "straight edge cd passes through node h1"),
            "{:?}",
            s.routing_issues
        );
        assert!(
            s.routing_issues
                .iter()
                .all(|i| !i.contains("g1") && !i.contains("g2")),
            "{:?}",
            s.routing_issues
        );
    }

    /// The channel router does not see straight segments: a mixed drawing is refused for it at the plan, and the legacy
    /// engine draws the same graph. An all-orthogonal or an all-straight drawing is fine with the channel router.
    #[test]
    fn the_channel_router_refuses_mixed_drawings_and_no_others() {
        let mixed = r#"{"nodes":[{"id":"g","kind":"group","label":"G","parentId":null},{"id":"a","kind":"node","label":"A","parentId":"g"},{"id":"b","kind":"node","label":"B","parentId":"g"},{"id":"c","kind":"node","label":"C","parentId":null}],"edges":[{"id":"ab","kind":"flow","source":"a","target":"b"},{"id":"ac","kind":"flow","source":"a","target":"c"}],"defaultRouting":"straight","containerRouting":{"g":"orthogonal"}}"#;
        let graph: Graph = serde_json::from_str(mixed).unwrap();
        let channel = LayoutOptions {
            routing_engine: RoutingEngine::Channel,
            ..LayoutOptions::default()
        };
        // The result is produced; the refusal is a diagnostic in it, and a refinement keeps it.
        let mixed_result = crate::compute(&graph, None, &BTreeSet::new(), &channel).unwrap();
        assert!(
            mixed_result
                .routing_issues
                .iter()
                .any(|i| i.contains("do not see the straight edges")),
            "{:?}",
            mixed_result.routing_issues
        );
        let refined = crate::reroute(&mixed_result);
        assert!(
            refined
                .routing_issues
                .iter()
                .any(|i| i.contains("do not see the straight edges")),
            "{:?}",
            refined.routing_issues
        );
        assert!(
            crate::compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default())
                .unwrap()
                .routing_issues
                .iter()
                .all(|i| !i.contains("channel router"))
        );

        let nodes = r#""nodes":[{"id":"g","kind":"group","label":"G","parentId":null},{"id":"a","kind":"node","label":"A","parentId":"g"},{"id":"b","kind":"node","label":"B","parentId":"g"},{"id":"c","kind":"node","label":"C","parentId":null}],"edges":[{"id":"ab","kind":"flow","source":"a","target":"b"},{"id":"ac","kind":"flow","source":"a","target":"c"}]"#;
        let all_straight: Graph =
            serde_json::from_str(&format!("{{{nodes},\"defaultRouting\":\"straight\"}}")).unwrap();
        let straight_result =
            crate::compute(&all_straight, None, &BTreeSet::new(), &channel).unwrap();
        assert!(straight_result
            .routing_issues
            .iter()
            .all(|i| !i.contains("channel router")));
        let orthogonal: Graph =
            serde_json::from_str(&format!("{{{nodes},\"defaultRouting\":\"orthogonal\"}}"))
                .unwrap();
        let orthogonal_result =
            crate::compute(&orthogonal, None, &BTreeSet::new(), &channel).unwrap();
        assert!(orthogonal_result
            .routing_issues
            .iter()
            .all(|i| !i.contains("channel router")));
    }
}

#[cfg(test)]
mod outline_tests {
    use super::*;

    /// Straight segments are checked against the real outline: a segment through the empty corner of a circle's box is not
    /// an entry into the circle, the same segment through a rectangle of that box is one.
    #[test]
    fn a_segment_through_a_circle_box_corner_is_not_a_hit() {
        let json = |shape: &str| {
            format!(
                r#"{{"nodes":[{{"id":"c","kind":"node","label":"C","parentId":null}},{{"id":"d","kind":"node","label":"D","parentId":null}},{{"id":"x","kind":"node","label":"X","parentId":null,"shape":"{shape}"}}],"edges":[{{"id":"cd","kind":"flow","source":"c","target":"d"}}]}}"#
            )
        };
        let boxes = [
            ("c", 0., 0., 50., 50.),
            ("d", 400., 0., 50., 50.),
            ("x", 180., 10., 60., 200.),
        ];
        let run = |shape: &str| {
            let graph: Graph = serde_json::from_str(&json(shape)).unwrap();
            let mut snapshot =
                crate::compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
            for (id, x, y, width, height) in boxes {
                snapshot.boxes.insert(
                    id.into(),
                    Rect {
                        x,
                        y,
                        width,
                        height,
                    },
                );
            }
            apply_straight_edges(&mut snapshot, &BTreeSet::from(["cd".to_string()])).unwrap();
            snapshot.routing_issues
        };
        assert!(run("circle").is_empty(), "{:?}", run("circle"));
        assert!(
            run("rect")
                .iter()
                .any(|i| i.contains("passes through node x")),
            "{:?}",
            run("rect")
        );
    }
}

#[cfg(test)]
mod contour_port_tests {
    use super::*;

    /// A configured port on a circle leaves the disc on its side: the exit is never tangential, so the first segment of an
    /// orthogonal route can leave in the side's direction. The offset 0 and 1 of a side stay within 45 degrees of its normal.
    #[test]
    fn a_configured_circle_port_exits_on_its_side() {
        let json = r#"{"nodes":[{"id":"x","kind":"node","label":"X","parentId":null,"shape":"circle"},{"id":"b","kind":"node","label":"B","parentId":null}],"edges":[{"id":"xb","kind":"flow","source":"x","target":"b"}],"ports":[{"id":"east-low","nodeId":"x","side":"right","offset":0.0}],"edgePorts":[{"edgeId":"xb","sourcePort":"east-low","targetPort":null}]}"#;
        let graph: Graph = serde_json::from_str(json).unwrap();
        let s = crate::compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        let port = s
            .resolved_ports
            .iter()
            .find(|p| p.edge_id == "xb" && p.endpoint == "source")
            .unwrap();
        let x = &s.boxes["x"];
        let (cx, cy) = (x.x + x.width / 2., x.y + x.height / 2.);
        let r = x.width.min(x.height) / 2.;
        let (dx, dy) = (port.position.x - cx, port.position.y - cy);
        assert!(((dx.p_hypot(dy)) - r).abs() < 1e-6, "on the outline");
        assert_eq!(port.side, PortSide::Right);
        assert!(
            dx > 0. && dx / r >= (std::f64::consts::FRAC_PI_4).p_cos() - 1e-9,
            "exit toward the right: {dx}, {dy}"
        );
    }

    /// The channel router does not route around circles: an orthogonal edge in a drawing with a circle is refused for it.
    #[test]
    fn the_channel_router_refuses_circles_for_orthogonal_edges() {
        let json = r#"{"nodes":[{"id":"x","kind":"node","label":"X","parentId":null,"shape":"circle"},{"id":"b","kind":"node","label":"B","parentId":null}],"edges":[{"id":"xb","kind":"flow","source":"x","target":"b"}]}"#;
        let graph: Graph = serde_json::from_str(json).unwrap();
        let channel = LayoutOptions {
            routing_engine: RoutingEngine::Channel,
            ..LayoutOptions::default()
        };
        let result = crate::compute(&graph, None, &BTreeSet::new(), &channel).unwrap();
        assert!(
            result
                .routing_issues
                .iter()
                .any(|i| i.contains("circle outlines")),
            "{:?}",
            result.routing_issues
        );
        assert!(crate::compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).is_ok());
    }
}
