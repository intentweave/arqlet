//! Provisional (path cut-out): the `trace` operation. The paths between two nodes of the source graph
//! (`arqlet_view::trace`), and what of them the current view shows: the visible node that holds each path node (itself, or
//! the collapsed container it is in), the routes that carry the path's edges, and how far every visible node is from the
//! path, so that a renderer can highlight the path and fade the rest. Read only: the snapshot is not changed.
use crate::structured;
use arqlet_model::Snapshot;
use arqlet_view::{trace, TraceMode};
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

/// The trace of the request `v` (`from`, `to`, `mode`) on a snapshot the caller holds (a session's).
pub fn trace_on(snapshot: &Snapshot, v: &Value) -> Result<Value, String> {
    let from = v["from"].as_str().ok_or("Missing from")?;
    let to = v["to"].as_str().ok_or("Missing to")?;
    let mode = match v["mode"].as_str() {
        None | Some("shortest") => TraceMode::Shortest,
        Some("all") => TraceMode::All,
        Some(other) => return Err(format!("Unknown trace mode: {other}")),
    };
    let canonical = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    let path = trace(canonical, from, to, mode)?;
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
    let mut nodes: Vec<String> = path.nodes.iter().filter_map(|n| owner(n)).collect();
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
    let mut routes = BTreeSet::new();
    for edge in &path.edges {
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
        let pair = drawn
            .iter()
            .filter(|(_, &(x, y))| (x == a && y == b) || (x == b && y == a))
            .map(|(id, _)| id.to_string())
            .next();
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
    Ok(json!({
        "ok": true,
        "found": path.length.is_some(),
        "reversed": path.reversed,
        "length": path.length,
        "nodes": path.nodes,
        "edges": path.edges,
        "visible": {"nodes": nodes, "routes": routes},
        "distance": distance,
    }))
}
