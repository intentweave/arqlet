//! Routing-engine choice for a drawing that still runs routes through nodes (P3, er-diagram). The legacy and the channel
//! router route the same boxes differently, and each can leave an edge through a node where the other does not. When the
//! drawing has such conflicts, both engines route it again with the same boxes, and the one with fewer conflicts (then
//! shorter routes) is kept. The boxes never change, so no node moves.

use arqlet_model::{EdgePorts, Graph, Port, PortSide, RoutingEngine, Snapshot};
use std::collections::BTreeSet;

/// The automatic endpoints that the side search may place (the first ones in edge order); more would make the enumeration
/// of sides too large.
const SEARCHED_ENDPOINTS: usize = 4;

/// Passes of the greedy side search over the endpoints.
const SEARCH_PASSES: usize = 2;

/// The most candidates one side search may evaluate, and the largest view it runs on. Each candidate is a routing or a
/// layout of the view, so the bounds keep an edit with conflicts in a view of this size from taking seconds.
const MAX_EVALUATIONS: usize = 24;
const MAX_SEARCH_NODES: usize = 60;

/// The Optimize-time budget: the crossing step may try more candidates than the layout, because it runs on the delivered
/// drawing only and the published key decides whether a candidate is kept.
const OPTIMIZE_EVALUATIONS: usize = 80;

/// The offsets along a side that one endpoint may take in the search: the middle, and both ends of the side.
const OFFSETS: [f64; 3] = [0.15, 0.5, 0.85];

/// The sides an automatic endpoint may take in the search.
const SIDES: [PortSide; 4] = [
    PortSide::Bottom,
    PortSide::Top,
    PortSide::Left,
    PortSide::Right,
];

/// The id prefix of the ports the search binds. Derived ports exist only in the graph that routes a candidate.
const PREFIX: &str = "track:";

/// Returns the finished snapshot of the other engine's routing when it has fewer edge–node conflicts (then shorter routes)
/// than `snapshot` and than the configured engine's routing of the same boxes; `None` otherwise. `finish` applies the steps
/// that give a complete layout its drawn routes and labels.
pub(crate) fn improve(
    graph: &Graph,
    previous: Option<&Snapshot>,
    snapshot: &Snapshot,
    finish: &dyn Fn(Snapshot) -> Result<Snapshot, String>,
) -> Result<Option<Snapshot>, String> {
    let configured = snapshot.options.routing_engine;
    let alternative = other_engine(configured);
    let own = routed(snapshot, graph, configured, finish)?;
    let other = routed(snapshot, graph, alternative, finish)?;
    let mut best = if key(&other) < key(&own) { other } else { own };
    if !best.route_conflicts.is_empty() {
        best = side_search(
            graph,
            previous,
            best,
            finish,
            false,
            &layout_score,
            MAX_EVALUATIONS,
        )?;
    }
    if key(&best) >= key(snapshot) {
        return Ok(None);
    }
    Ok(Some(best))
}

/// The Optimize-time step (P3): the side search for routes that cross each other. It is not part of the layout, so an
/// import or an edit does not pay for it. Returns the better drawing, or `None` when the search finds nothing better.
pub(crate) fn relieve(
    snapshot: &Snapshot,
    finish: &dyn Fn(Snapshot) -> Result<Snapshot, String>,
) -> Result<Option<Snapshot>, String> {
    if crossings(&snapshot.routes) == 0 {
        return Ok(None);
    }
    // The search minimises the published key, the one refinement uses: a crossing is removed only where the configured
    // objectives rank it above what the change costs.
    let published = |s: &Snapshot| -> Vec<f64> {
        super::published_key(s)
            .map(|k| k.into_iter().map(|v| v as f64).collect())
            .unwrap_or_else(|_| vec![f64::INFINITY])
    };
    let result = side_search(
        &snapshot.graph,
        None,
        snapshot.clone(),
        finish,
        true,
        &published,
        OPTIMIZE_EVALUATIONS,
    )?;
    if published(&result) < published(snapshot) {
        Ok(Some(result))
    } else {
        Ok(None)
    }
}

/// The edges of routes that cross another route.
fn crossing_edges(routes: &[arqlet_model::Route]) -> BTreeSet<String> {
    let drawn: Vec<&arqlet_model::Route> =
        routes.iter().filter(|r| r.bundle_of.is_none()).collect();
    let mut edges = BTreeSet::new();
    for (i, a) in drawn.iter().enumerate() {
        for b in &drawn[i + 1..] {
            if arqlet_routing::crosses(a, b) {
                edges.insert(a.id.clone());
                edges.insert(b.id.clone());
            }
        }
    }
    edges
}

/// Greedy search over the sides of the automatic endpoints of the conflicting edges (P3, track ordering). The start is the
/// drawing's own sides; in each pass every searched endpoint tries the other sides, and a change is kept when the routes
/// get fewer conflicts (then shorter). Endpoints that share a node and a side are spread along it in the order of their
/// peers, so two routes that must leave one side do not share one spot. The boxes never move.
fn side_search(
    graph: &Graph,
    previous: Option<&Snapshot>,
    best: Snapshot,
    finish: &dyn Fn(Snapshot) -> Result<Snapshot, String>,
    crossings_too: bool,
    score: &dyn Fn(&Snapshot) -> Vec<f64>,
    budget: usize,
) -> Result<Snapshot, String> {
    let mut conflicted: BTreeSet<String> = best
        .route_conflicts
        .iter()
        .map(|c| c.edge_id.clone())
        .collect();
    if crossings_too {
        conflicted.extend(crossing_edges(&best.routes));
    }
    let mut endpoints = endpoints(graph, &conflicted);
    if crossings_too {
        // The crossings this step targets are the ones where two routes leave one node side in the wrong order: only those
        // endpoints are searched. Without such a pair there is nothing to reorder, and the step costs nothing.
        endpoints = shared_side_endpoints(graph, &best, &endpoints);
    }
    if endpoints.is_empty() || graph.nodes.len() > MAX_SEARCH_NODES {
        return Ok(best);
    }
    let base = best.clone();
    // A state is a side and an offset along it per endpoint. The start keeps the drawing's side at the middle.
    let mut sides: Vec<(PortSide, f64)> = endpoints
        .iter()
        .map(|(edge, is_source)| {
            let endpoint = if *is_source { "source" } else { "target" };
            let side = best
                .resolved_ports
                .iter()
                .find(|p| p.edge_id == *edge && p.endpoint == endpoint)
                .map(|p| p.side)
                .unwrap_or(PortSide::Bottom);
            (side, 0.5)
        })
        .collect();
    // A candidate is a complete layout of the derived graph: the ports take part in the placement too, so the boxes may
    // change (a port that needs room moves its neighbour). The fewest conflicts win, and never more geometry issues.
    let options = base.options.clone();
    let everything: BTreeSet<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    let engine = best.options.routing_engine;
    // A candidate is judged on its drawing. Two kinds are tried: the routes of the searched edges re-made with the boxes kept
    // (the nodes stay where they are), and a complete layout of the derived graph, where the ports take part in the placement
    // and the boxes may move. A candidate that loses a route is not a drawing and is skipped.
    let complete = |snapshot: &Snapshot| snapshot.routes.len() >= base.routes.len();
    let evaluations = std::cell::Cell::new(0usize);
    let evaluate = |sides: &[(PortSide, f64)]| -> Result<Option<Snapshot>, String> {
        evaluations.set(evaluations.get() + 1);
        let derived = with_ports(graph, &endpoints, sides, &base);
        // Every route is made again from this candidate's ports, with the boxes kept.
        let port_context = crate::ports::PortContext::new(&derived);
        let routes = match super::route_all(
            &derived,
            &base.boxes,
            0,
            &options,
            engine == RoutingEngine::Channel,
            &mut super::Run::new(&|| false),
            &port_context,
        ) {
            Ok(routes) => routes,
            Err(super::Abort::Failed(message)) => return Err(message),
            Err(super::Abort::Stopped) => return Err("the side search was interrupted".into()),
        };
        // The searched edges take their new routes; every other edge keeps the route of the drawing being improved.
        let searched = |id: &str| endpoints.iter().any(|(edge, _)| edge == id);
        let mut kept: Vec<arqlet_model::Route> = base
            .routes
            .iter()
            .filter(|r| !searched(&r.id))
            .cloned()
            .collect();
        kept.extend(routes.into_iter().filter(|r| searched(&r.id)));
        let mut routed = base.clone();
        routed.graph = derived.clone();
        routed.routes = kept;
        crate::labels::decorate(&mut routed);
        let routed = finish(routed)?;
        // The complete layout is only needed when the routes with the boxes kept still run through nodes.
        if routed.route_conflicts.is_empty() && complete(&routed) {
            return Ok(Some(routed));
        }
        let laid = super::uninterrupted(super::compute_mode(
            &derived,
            previous,
            &everything,
            0,
            &options,
            &mut super::Run::new(&|| false),
        ))?;
        let laid = finish(laid)?;
        Ok(match (complete(&routed), complete(&laid)) {
            (true, true) => Some(if key(&laid) < key(&routed) {
                laid
            } else {
                routed
            }),
            (true, false) => Some(routed),
            (false, true) => Some(laid),
            (false, false) => None,
        })
    };
    // The start is the drawing the search was given: a candidate has to beat it, never a fresh layout of the same sides.
    let mut current = best.clone();
    let mut current_key = score(&current);
    // A move sets the side of one endpoint, or of all the searched endpoints of one node (two routes that enter one node must
    // both change side for the spread to matter).
    let node_of = |edge_id: &str, is_source: bool| -> Option<String> {
        let edge = graph.edges.iter().find(|e| e.id == edge_id)?;
        if is_source {
            Some(edge.source.clone())
        } else {
            edge.target.clone()
        }
    };
    let mut moves: Vec<Vec<usize>> = (0..endpoints.len()).map(|i| vec![i]).collect();
    for i in 0..endpoints.len() {
        let group: Vec<usize> = (0..endpoints.len())
            .filter(|&j| {
                node_of(&endpoints[j].0, endpoints[j].1) == node_of(&endpoints[i].0, endpoints[i].1)
            })
            .collect();
        if group.len() > 1 && !moves.contains(&group) {
            moves.push(group);
        }
    }
    for _ in 0..SEARCH_PASSES {
        if evaluations.get() >= budget {
            break;
        }
        let mut changed = false;
        for group in &moves {
            // One endpoint may take any side at any searched offset; a node's group sets the side (its ports are spread).
            let choices: Vec<(PortSide, f64)> = if group.len() == 1 {
                SIDES
                    .iter()
                    .flat_map(|&side| OFFSETS.iter().map(move |&offset| (side, offset)))
                    .collect()
            } else {
                SIDES.iter().map(|&side| (side, 0.5)).collect()
            };
            for choice in choices {
                if group.iter().all(|&i| sides[i] == choice) {
                    continue;
                }
                let mut trial = sides.clone();
                for &i in group {
                    trial[i] = choice;
                }
                if evaluations.get() >= budget {
                    break;
                }
                let Some(candidate) = evaluate(&trial)? else {
                    continue;
                };
                if score(&candidate) < current_key
                    && candidate.geometry_issues.len() <= base.geometry_issues.len()
                {
                    current_key = score(&candidate);
                    current = candidate;
                    sides = trial;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok(finalised(current, graph, &base))
}

/// The drawing with the canonical graph and the configured options.
fn finalised(mut result: Snapshot, graph: &Graph, base: &Snapshot) -> Snapshot {
    result.graph = graph.clone();
    result.options = base.options.clone();
    for port in &mut result.resolved_ports {
        if port.id.starts_with(PREFIX) {
            port.explicit = false;
        }
    }
    result
}

/// The endpoints of `endpoints` whose node side is shared with another automatic endpoint of the same node in the drawing:
/// two routes that leave one side of one node.
fn shared_side_endpoints(
    graph: &Graph,
    drawing: &Snapshot,
    endpoints: &[(String, bool)],
) -> Vec<(String, bool)> {
    let node_of = |edge_id: &str, is_source: bool| -> Option<String> {
        let edge = graph.edges.iter().find(|e| e.id == edge_id)?;
        if is_source {
            Some(edge.source.clone())
        } else {
            edge.target.clone()
        }
    };
    let side_of = |edge_id: &str, is_source: bool| -> Option<arqlet_model::PortSide> {
        let endpoint = if is_source { "source" } else { "target" };
        drawing
            .resolved_ports
            .iter()
            .find(|p| p.edge_id == edge_id && p.endpoint == endpoint)
            .map(|p| p.side)
    };
    let keys: Vec<Option<(String, arqlet_model::PortSide)>> = endpoints
        .iter()
        .map(|(e, s)| Some((node_of(e, *s)?, side_of(e, *s)?)))
        .collect();
    // Only pairs whose routes actually cross count: a shared side with routes that do not cross has nothing to reorder.
    let crossing = |a: &str, b: &str| {
        let route = |id: &str| drawing.routes.iter().find(|r| r.id == id);
        match (route(a), route(b)) {
            (Some(x), Some(y)) => arqlet_routing::crosses(x, y),
            _ => false,
        }
    };
    endpoints
        .iter()
        .enumerate()
        .filter(|(i, (edge, _))| {
            keys[*i].as_ref().is_some_and(|k| {
                endpoints.iter().enumerate().any(|(j, (other, _))| {
                    j != *i && keys[j].as_ref() == Some(k) && crossing(edge, other)
                })
            })
        })
        .map(|(_, e)| e.clone())
        .collect()
}

/// The automatic endpoints (source, then target per edge) of the conflicting edges, in edge order, at most SEARCHED_ENDPOINTS.
fn endpoints(graph: &Graph, conflicted: &BTreeSet<String>) -> Vec<(String, bool)> {
    let configured: BTreeSet<(&str, bool)> = graph
        .edge_ports
        .iter()
        .flat_map(|b| {
            [
                b.source_port.as_ref().map(|_| (b.edge_id.as_str(), true)),
                b.target_port.as_ref().map(|_| (b.edge_id.as_str(), false)),
            ]
        })
        .flatten()
        .collect();
    let mut list = Vec::new();
    for edge in &graph.edges {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        if !conflicted.contains(&edge.id) || edge.source == target {
            continue;
        }
        for is_source in [true, false] {
            if !configured.contains(&(edge.id.as_str(), is_source)) {
                list.push((edge.id.clone(), is_source));
            }
        }
    }
    list.truncate(SEARCHED_ENDPOINTS);
    list
}

/// The graph with each listed endpoint bound to a derived port on its side. Endpoints that share a node and a side are spread
/// evenly along it, in the order of their peers' positions along that side.
fn with_ports(
    graph: &Graph,
    endpoints: &[(String, bool)],
    sides: &[(PortSide, f64)],
    base: &Snapshot,
) -> Graph {
    let absolute = super::absolute_all(graph, &base.boxes);
    let peer_of = |edge_id: &str, is_source: bool| -> Option<String> {
        let edge = graph.edges.iter().find(|e| e.id == edge_id)?;
        if is_source {
            edge.target.clone()
        } else {
            Some(edge.source.clone())
        }
    };
    let mut derived = graph.clone();
    // Offsets: per index, from its group (node, side) of searched endpoints.
    let node_of = |edge_id: &str, is_source: bool| -> Option<String> {
        let edge = graph.edges.iter().find(|e| e.id == edge_id)?;
        if is_source {
            Some(edge.source.clone())
        } else {
            edge.target.clone()
        }
    };
    let mut offsets: Vec<f64> = sides.iter().map(|&(_, offset)| offset).collect();
    for (i, (edge_id, is_source)) in endpoints.iter().enumerate() {
        let Some(node) = node_of(edge_id, *is_source) else {
            continue;
        };
        let mut group: Vec<(f64, usize)> = endpoints
            .iter()
            .enumerate()
            .filter(|(j, (e, s))| {
                sides[*j].0 == sides[i].0 && node_of(e, *s).as_deref() == Some(node.as_str())
            })
            .map(|(j, (e, s))| {
                let peer = peer_of(e, *s)
                    .and_then(|p| absolute.get(&p))
                    .map(|r| match sides[i].0 {
                        PortSide::Top | PortSide::Bottom => r.x + r.width / 2.,
                        PortSide::Left | PortSide::Right => r.y + r.height / 2.,
                    })
                    .unwrap_or(0.);
                (peer, j)
            })
            .collect();
        if group.len() > 1 {
            group.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let rank = group.iter().position(|(_, j)| *j == i).unwrap_or(0);
            let rank = group.len() - 1 - rank;
            offsets[i] = (rank + 1) as f64 / (group.len() + 1) as f64;
        }
    }
    for (i, (edge_id, is_source)) in endpoints.iter().enumerate() {
        let Some(edge) = graph.edges.iter().find(|e| &e.id == edge_id) else {
            continue;
        };
        let port_id = format!(
            "{PREFIX}{edge_id}:{}",
            if *is_source { "source" } else { "target" }
        );
        let node_id = if *is_source {
            edge.source.clone()
        } else {
            edge.target.clone().unwrap_or_default()
        };
        derived.ports.push(Port {
            id: port_id.clone(),
            node_id,
            side: sides[i].0,
            offset: offsets[i],
            row: None,
        });
        let index = match derived
            .edge_ports
            .iter()
            .position(|b| &b.edge_id == edge_id)
        {
            Some(index) => index,
            None => {
                derived.edge_ports.push(EdgePorts {
                    edge_id: edge_id.clone(),
                    source_port: None,
                    target_port: None,
                });
                derived.edge_ports.len() - 1
            }
        };
        let binding = &mut derived.edge_ports[index];
        if *is_source {
            binding.source_port = Some(port_id);
        } else {
            binding.target_port = Some(port_id);
        }
    }
    derived
}

/// The drawing of `snapshot` routed again with `engine` for its own graph, the boxes kept, and finished. The snapshot keeps the
/// configured options; only the routes follow the engine.
fn routed(
    snapshot: &Snapshot,
    graph: &Graph,
    engine: RoutingEngine,
    finish: &dyn Fn(Snapshot) -> Result<Snapshot, String>,
) -> Result<Snapshot, String> {
    let mut draft = snapshot.clone();
    draft.graph = graph.clone();
    draft.options.routing_engine = engine;
    let mut routed = super::reroute(&draft);
    crate::labels::decorate(&mut routed);
    let mut result = finish(routed)?;
    crate::labels::decorate(&mut result);
    result.options = snapshot.options.clone();
    Ok(result)
}

/// Edge–node conflicts first, then routed crossings, then the total length of the routes. Smaller is better.
/// The layout's own score: conflicts, crossings, then the length of the routes. Smaller is better.
fn layout_score(snapshot: &Snapshot) -> Vec<f64> {
    let (conflicts, crossings, length) = key(snapshot);
    vec![conflicts as f64, crossings as f64, length]
}

fn key(snapshot: &Snapshot) -> (usize, usize, f64) {
    let length = snapshot
        .routes
        .iter()
        .flat_map(|r| r.points.windows(2))
        .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
        .sum();
    (
        snapshot.route_conflicts.len(),
        crossings(&snapshot.routes),
        length,
    )
}

/// Pairs of drawn routes that cross (bundle members are not drawn and do not count).
pub(crate) fn crossings(routes: &[arqlet_model::Route]) -> usize {
    let drawn: Vec<&arqlet_model::Route> =
        routes.iter().filter(|r| r.bundle_of.is_none()).collect();
    let mut count = 0;
    for (i, a) in drawn.iter().enumerate() {
        for b in &drawn[i + 1..] {
            if arqlet_routing::crosses(a, b) {
                count += 1;
            }
        }
    }
    count
}

fn other_engine(engine: RoutingEngine) -> RoutingEngine {
    match engine {
        RoutingEngine::Legacy => RoutingEngine::Channel,
        RoutingEngine::Channel => RoutingEngine::Legacy,
    }
}
