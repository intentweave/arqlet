//! Provisional (peek): the `peek` operation. A look into a collapsed container without opening it: its contents laid out
//! on their own as a small scene (the containers inside it collapsed, at most `limit` direct children in the source
//! order), and for every child shown how many edges cross the container's border from its subtree, so that a host can
//! draw the look as a popover, a magnifier or a panel. Read only: the snapshot and the view are not changed, and the look
//! is not a step for `back`.
use arqlet_model::{Graph, LayoutOptions, LevelOfDetail, Snapshot};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// How many direct children a look shows unless the request says otherwise (`limit`), and at most.
const DEFAULT_LIMIT: usize = 50;
const MAX_LIMIT: usize = 500;

pub fn peek_response(v: &Value) -> Value {
    let result = serde_json::from_value::<Snapshot>(v["snapshot"].clone())
        .map_err(|e| e.to_string())
        .and_then(|snapshot| peek_on(&snapshot, v));
    match result {
        Ok(answer) => answer,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

/// The look of the request `v` (`nodeId`, optional `limit`) on a snapshot the caller holds (a session's).
pub fn peek_on(snapshot: &Snapshot, v: &Value) -> Result<Value, String> {
    let id = v["nodeId"].as_str().ok_or("Missing nodeId")?;
    let limit = match v.get("limit") {
        None | Some(Value::Null) => DEFAULT_LIMIT,
        Some(l) => l
            .as_u64()
            .ok_or("Peek limit must be a whole number")?
            .clamp(1, MAX_LIMIT as u64) as usize,
    };
    let source = snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph);
    if !source.nodes.iter().any(|n| n.id == id) {
        return Err(format!("Unknown node: {id}"));
    }
    let mut children_of: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for n in &source.nodes {
        if let Some(p) = n.parent_id.as_deref() {
            children_of.entry(p).or_default().push(n.id.as_str());
        }
    }
    let direct = children_of.get(id).cloned().unwrap_or_default();
    if direct.is_empty() {
        return Err(format!("Not a container: {id}"));
    }
    // The children shown (the first `limit` in the source order) and everything inside them, each with the child it is in.
    let shown: Vec<&str> = direct.iter().copied().take(limit).collect();
    let mut top: BTreeMap<&str, &str> = BTreeMap::new();
    for &child in &shown {
        let mut stack = vec![child];
        while let Some(n) = stack.pop() {
            if top.insert(n, child).is_some() {
                continue;
            }
            stack.extend(children_of.get(n).map(Vec::as_slice).unwrap_or(&[]));
        }
    }
    // Everything inside the container (also below the children left out), to tell border edges from inner ones.
    let mut inside: BTreeSet<&str> = BTreeSet::new();
    let mut stack = direct.clone();
    while let Some(n) = stack.pop() {
        if inside.insert(n) {
            stack.extend(children_of.get(n).map(Vec::as_slice).unwrap_or(&[]));
        }
    }
    let nodes = source
        .nodes
        .iter()
        .filter(|n| top.contains_key(n.id.as_str()))
        .map(|n| {
            let mut n = n.clone();
            if n.parent_id.as_deref() == Some(id) {
                n.parent_id = None;
            }
            n
        })
        .collect();
    let mut external: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut edges = Vec::new();
    for e in &source.edges {
        let Some(t) = e.target.as_deref() else {
            continue;
        };
        let (s_top, t_top) = (top.get(e.source.as_str()), top.get(t));
        match (s_top, t_top) {
            (Some(_), Some(_)) => edges.push(e.clone()),
            // An edge across the border: from outside the container (or from the container itself) to a child shown, or
            // the other way.
            (Some(&child), None) if !inside.contains(t) => {
                external.entry(child).or_default().1 += 1
            }
            (None, Some(&child)) if !inside.contains(e.source.as_str()) => {
                external.entry(child).or_default().0 += 1
            }
            _ => {}
        }
    }
    let kept: BTreeSet<&str> = top.keys().copied().collect();
    let edge_ids: BTreeSet<&str> = edges
        .iter()
        .map(|e: &arqlet_model::Edge| e.id.as_str())
        .collect();
    let within = |key: &String| kept.contains(key.as_str());
    // The container's own constraints are those of the look's root.
    let graph = Graph {
        default_direction: source
            .container_directions
            .get(id)
            .cloned()
            .unwrap_or_else(|| source.default_direction.clone()),
        container_directions: source
            .container_directions
            .iter()
            .filter(|(k, _)| within(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        default_strategy: source
            .container_strategies
            .get(id)
            .cloned()
            .unwrap_or_else(|| source.default_strategy.clone()),
        container_strategies: source
            .container_strategies
            .iter()
            .filter(|(k, _)| within(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        default_routing: source
            .container_routing
            .get(id)
            .cloned()
            .or_else(|| source.default_routing.clone()),
        container_routing: source
            .container_routing
            .iter()
            .filter(|(k, _)| within(k))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        ports: source
            .ports
            .iter()
            .filter(|p| kept.contains(p.node_id.as_str()))
            .cloned()
            .collect(),
        edge_ports: source
            .edge_ports
            .iter()
            .filter(|p| edge_ids.contains(p.edge_id.as_str()))
            .cloned()
            .collect(),
        nodes,
        edges,
        buses: Vec::new(),
    };
    // The view's options, with the containers inside collapsed and without what belongs to the view's own geometry.
    let mut options: LayoutOptions = snapshot.options.clone();
    options.level_of_detail = Some(LevelOfDetail {
        depth: Some(0),
        ..LevelOfDetail::default()
    });
    options.pins = Default::default();
    options
        .container_sizing
        .retain(|k, _| kept.contains(k.as_str()));
    let count = kept.len();
    let look = crate::import_graph_with(graph, count, &options)?;
    let scene = crate::filter::scene_of(&look)?;
    let external: BTreeMap<&str, Value> = external
        .into_iter()
        .map(|(child, (incoming, outgoing))| {
            (child, json!({"incoming": incoming, "outgoing": outgoing}))
        })
        .collect();
    Ok(json!({
        "ok": true,
        "nodeId": id,
        "children": direct.len(),
        "shown": shown.len(),
        "omitted": direct.len() - shown.len(),
        "external": external,
        "scene": scene,
    }))
}
