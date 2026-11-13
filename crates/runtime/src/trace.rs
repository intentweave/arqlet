//! Provisional (path cut-out): the `trace` operation. The paths between two nodes of the source graph
//! (`arqlet_view::trace`), and what of them the current view shows: the visible node that holds each path node (itself, or
//! the collapsed container it is in), the routes that carry the path's edges, and how far every visible node is from the
//! path, so that a renderer can highlight the path and fade the rest. Read only: the snapshot is not changed.
use crate::structured;
use arqlet_model::{NeighbourhoodAnswer, Snapshot, TraceAnswer, VisibleParts};
use arqlet_view::{neighbourhood, trace_along, Reach, TraceMode};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub fn trace_response(v: &Value) -> Value {
    match trace_of(v) {
        Ok(result) => result,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

fn trace_of(v: &Value) -> Result<Value, String> {
    let snapshot: Snapshot =
        serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
    trace_on(&snapshot, v)
}

/// The trace of the request `v` (`from`, `to`, `mode`, `edgeKinds`) on a snapshot the caller holds (a session's).
/// `edgeKinds` (provisional): the paths follow only edges of these kinds (for example the data flow, not the calls).
pub fn trace_on(snapshot: &Snapshot, v: &Value) -> Result<Value, String> {
    trace_answer_on(snapshot, v).and_then(|answer| answered(&answer))
}

/// The answer of a `trace` request that carries its snapshot (`trace_on` as a typed value).
pub fn trace_answer(v: &Value) -> Result<TraceAnswer, String> {
    let snapshot: Snapshot =
        serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
    trace_answer_on(&snapshot, v)
}

/// The typed answer of `trace_on`.
pub fn trace_answer_on(snapshot: &Snapshot, v: &Value) -> Result<TraceAnswer, String> {
    let from = v["from"].as_str().ok_or("Missing from")?;
    let to = v["to"].as_str().ok_or("Missing to")?;
    let mode = match v["mode"].as_str() {
        None | Some("shortest") => TraceMode::Shortest,
        Some("all") => TraceMode::All,
        Some(other) => return Err(format!("Unknown trace mode: {other}")),
    };
    let kinds = edge_kinds_of(v, "Trace")?;
    let canonical = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    let path = trace_along(canonical, from, to, mode, |e| {
        kinds.as_ref().is_none_or(|k| k.contains(&e.kind))
    })?;
    let (nodes, routes, distance) = on_view(snapshot, &path.nodes, &path.edges, &kinds)?;
    Ok(TraceAnswer {
        found: path.length.is_some(),
        reversed: path.reversed,
        length: path.length,
        nodes: path.nodes,
        edges: path.edges,
        visible: VisibleParts {
            nodes,
            routes: routes.into_iter().collect(),
        },
        distance,
    })
}

/// A typed answer as JSON, with `ok: true` beside its fields (keys in order, as every answer).
pub(crate) fn answered<T: serde::Serialize>(answer: &T) -> Result<Value, String> {
    let mut value = serde_json::to_value(answer).map_err(|e| e.to_string())?;
    value
        .as_object_mut()
        .ok_or("An answer is an object")?
        .insert("ok".into(), json!(true));
    Ok(value)
}

/// The `edgeKinds` of a request (`what` names the operation in errors): `None` follows every edge.
pub(crate) fn edge_kinds_of(v: &Value, what: &str) -> Result<Option<BTreeSet<String>>, String> {
    match v.get("edgeKinds") {
        None | Some(Value::Null) => Ok(None),
        Some(k) => {
            let kinds: BTreeSet<String> = serde_json::from_value(k.clone())
                .map_err(|_| format!("{what} edgeKinds must be a list of edge kinds"))?;
            if kinds.is_empty() {
                return Err(format!(
                    "{what} edgeKinds must name at least one kind (leave it out to follow every edge)"
                ));
            }
            Ok(Some(kinds))
        }
    }
}

/// What of some source-graph nodes and edges the current view shows: the visible boxes that hold the nodes (sorted), the
/// routes that carry the edges (preferring a drawn edge of a followed kind), and the hops of every visible node from
/// those boxes through the drawn edges (for fading).
pub(crate) type OnView = (Vec<String>, BTreeSet<String>, BTreeMap<String, usize>);

pub(crate) fn on_view(
    snapshot: &Snapshot,
    path_nodes: &[String],
    path_edges: &[String],
    kinds: &Option<BTreeSet<String>>,
) -> Result<OnView, String> {
    let canonical = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    // The visible node of every node of the view: itself, or the outermost collapsed container above it.
    let view = structured(canonical, &snapshot.options)?;
    let parent: BTreeMap<&str, Option<&str>> = view
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let visible: BTreeSet<&str> = snapshot.graph.nodes.iter().map(|n| n.id.as_str()).collect();
    let owner = |id: &str| -> Option<String> {
        let mut at = Some(id);
        let mut shown = None;
        while let Some(node) = at {
            if visible.contains(node) {
                // The outermost visible one: a node inside an expanded container is visible as itself.
                shown.get_or_insert(node);
                if snapshot.collapsed_ids.iter().any(|c| c == node) {
                    shown = Some(node);
                }
            }
            at = parent.get(node).copied().flatten();
        }
        shown.map(String::from)
    };
    let mut nodes: Vec<String> = path_nodes.iter().filter_map(|n| owner(n)).collect();
    nodes.sort();
    nodes.dedup();
    // The routes that carry the path's edges: the edge itself when it is drawn, else the edge the view draws between the
    // two visible ends (with communities, one per pair for both directions).
    let ends: BTreeMap<&str, (&str, Option<&str>)> = canonical
        .edges
        .iter()
        .map(|e| (e.id.as_str(), (e.source.as_str(), e.target.as_deref())))
        .collect();
    let drawn: BTreeMap<&str, (&str, &str)> = snapshot
        .graph
        .edges
        .iter()
        .filter_map(|e| Some((e.id.as_str(), (e.source.as_str(), e.target.as_deref()?))))
        .collect();
    let drawn_kind: BTreeMap<&str, &str> = snapshot
        .graph
        .edges
        .iter()
        .map(|e| (e.id.as_str(), e.kind.as_str()))
        .collect();
    let followed = |id: &str| {
        kinds
            .as_ref()
            .is_none_or(|k| drawn_kind.get(id).is_some_and(|kind| k.contains(*kind)))
    };
    let mut routes = BTreeSet::new();
    for edge in path_edges {
        let (s, Some(t)) = ends[edge.as_str()] else {
            continue;
        };
        let (Some(a), Some(b)) = (owner(s), owner(t)) else {
            continue;
        };
        if a == b {
            continue;
        }
        if drawn
            .get(edge.as_str())
            .is_some_and(|&(x, y)| x == a && y == b)
        {
            routes.insert(edge.clone());
            continue;
        }
        // With edge kinds, a drawn edge of a followed kind first.
        let mut between = drawn
            .iter()
            .filter(|(_, &(x, y))| (x == a && y == b) || (x == b && y == a))
            .map(|(id, _)| *id);
        let pair = between
            .clone()
            .find(|id| followed(id))
            .or_else(|| between.next())
            .map(String::from);
        routes.extend(pair);
    }
    // Hops from the path through the drawn edges (both ways), for fading: the path's visible nodes and the containers
    // around them are 0; nodes the path cannot reach are left out.
    let mut distance: BTreeMap<String, usize> = BTreeMap::new();
    let mut queue = VecDeque::new();
    for node in &nodes {
        let mut at = Some(node.as_str());
        while let Some(n) = at {
            if visible.contains(n) && !distance.contains_key(n) {
                distance.insert(n.to_string(), 0);
                queue.push_back(n.to_string());
            }
            at = parent.get(n).copied().flatten();
        }
    }
    let mut next: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for &(s, t) in drawn.values() {
        next.entry(s).or_default().push(t);
        next.entry(t).or_default().push(s);
    }
    while let Some(n) = queue.pop_front() {
        let d = distance[&n];
        for &m in next.get(n.as_str()).map(Vec::as_slice).unwrap_or(&[]) {
            if !distance.contains_key(m) {
                distance.insert(m.to_string(), d + 1);
                queue.push_back(m.to_string());
            }
        }
    }
    Ok((nodes, routes, distance))
}

/// Provisional: the `neighbourhood` operation (read only). The nodes within `hops` edges (default 1) of `nodeId` (or of
/// each of `nodeIds`), following edges `out`, `in` or `both` ways (`direction`, default `both`), only of `edgeKinds` if
/// given; and what of them the current view shows, as for `trace`, so that a host can highlight them or `reveal` them.
pub fn neighbourhood_response(v: &Value) -> Value {
    let result = neighbourhood_answer(v).and_then(|answer| answered(&answer));
    match result {
        Ok(result) => result,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

/// The neighbourhood of the request `v` on a snapshot the caller holds (a session's).
pub fn neighbourhood_on(snapshot: &Snapshot, v: &Value) -> Result<Value, String> {
    neighbourhood_answer_on(snapshot, v).and_then(|answer| answered(&answer))
}

/// The answer of a `neighbourhood` request that carries its snapshot, as a typed value.
pub fn neighbourhood_answer(v: &Value) -> Result<NeighbourhoodAnswer, String> {
    let snapshot: Snapshot =
        serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
    neighbourhood_answer_on(&snapshot, v)
}

/// The typed answer of `neighbourhood_on`.
pub fn neighbourhood_answer_on(
    snapshot: &Snapshot,
    v: &Value,
) -> Result<NeighbourhoodAnswer, String> {
    let starts: Vec<&str> = match (v.get("nodeId"), v.get("nodeIds")) {
        (Some(id), None) => vec![id
            .as_str()
            .ok_or("Neighbourhood nodeId must be a node id")?],
        (None, Some(ids)) => ids
            .as_array()
            .filter(|ids| !ids.is_empty())
            .ok_or("Neighbourhood nodeIds must be a non-empty list of node ids")?
            .iter()
            .map(|id| id.as_str().ok_or("Neighbourhood nodeIds must be node ids"))
            .collect::<Result<_, _>>()?,
        _ => return Err("Neighbourhood needs nodeId or nodeIds (one of them)".into()),
    };
    let hops = match v.get("hops") {
        None => 1,
        Some(h) => h
            .as_u64()
            .filter(|h| *h <= 1_000)
            .ok_or("Neighbourhood hops must be a whole number from 0 to 1000")?
            as usize,
    };
    let reach = match v["direction"].as_str() {
        None | Some("both") => Reach::Both,
        Some("out") => Reach::Out,
        Some("in") => Reach::In,
        Some(other) => {
            return Err(format!(
                "Unknown neighbourhood direction: {other} (out, in or both)"
            ))
        }
    };
    let kinds = edge_kinds_of(v, "Neighbourhood")?;
    let canonical = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    let found = neighbourhood(canonical, &starts, hops, reach, |e| {
        kinds.as_ref().is_none_or(|k| k.contains(&e.kind))
    })?;
    let (nodes, routes, distance) = on_view(snapshot, &found.nodes, &found.edges, &kinds)?;
    Ok(NeighbourhoodAnswer {
        nodes: found.nodes,
        edges: found.edges,
        hops: found.hops,
        visible: VisibleParts {
            nodes,
            routes: routes.into_iter().collect(),
        },
        distance,
    })
}
