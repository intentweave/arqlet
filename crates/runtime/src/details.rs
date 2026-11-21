//! Provisional (view details): the `details` operation. What the engine knows about a node or an edge of the current
//! view and the host cannot see in the scene: the containers above a node, what a collapsed box holds, the box that shows
//! a hidden node, and for a drawn line the edges of the source graph it stands for in either direction. Domain attributes
//! stay with the host, which looks them up by id. Read only: the snapshot is not changed.
use crate::trace::{answered, navigable, ViewMap};
use arqlet_model::math::PortableMath;
use arqlet_model::{DetailsAnswer, EdgeDetails, LineDetails, NodeDetails, Rect, Snapshot};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// How many edge ids a line's details list in each direction unless the request says otherwise (`limit`).
const DEFAULT_LIMIT: usize = 50;

pub fn details_response(v: &Value) -> Value {
    match details_answer(v).and_then(|answer| answered(&answer)) {
        Ok(result) => result,
        Err(error) => json!({"ok": false, "error": error}),
    }
}

/// The details of the request `v` (`nodeId` or `edgeId`, optional `limit`) on a snapshot the caller holds (a session's).
pub fn details_on(snapshot: &Snapshot, v: &Value) -> Result<Value, String> {
    details_answer_on(snapshot, v).and_then(|answer| answered(&answer))
}

/// The answer of a `details` request that carries its snapshot.
pub fn details_answer(v: &Value) -> Result<DetailsAnswer, String> {
    let snapshot: Snapshot =
        serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
    details_answer_on(&snapshot, v)
}

/// The typed answer of `details_on`. `edgeId` may name a drawn line (`route`) or an edge of the source graph (`edge`, with
/// the line that carries it as `route`), or both when the edge is drawn as itself.
pub fn details_answer_on(snapshot: &Snapshot, v: &Value) -> Result<DetailsAnswer, String> {
    let node_id = v["nodeId"].as_str();
    let edge_id = v["edgeId"].as_str();
    if node_id.is_some() == edge_id.is_some() {
        return Err("Details need either nodeId or edgeId".into());
    }
    let limit = match v.get("limit") {
        None | Some(Value::Null) => DEFAULT_LIMIT,
        Some(l) => l
            .as_u64()
            .ok_or("Details limit must be a whole number")?
            .min(usize::MAX as u64) as usize,
    };
    let navigable = navigable(snapshot);
    let canonical = navigable.as_ref();
    let map = ViewMap::new(snapshot, canonical)?;
    let mut answer = DetailsAnswer::default();
    if let Some(id) = node_id {
        answer.node = Some(node_details(snapshot, canonical, &map, id)?);
    }
    if let Some(id) = edge_id {
        let edge = canonical.edges.iter().find(|e| e.id == id);
        let carried = edge.and_then(|_| map.route_of(id, &None));
        answer.edge = edge.map(|e| EdgeDetails {
            id: e.id.clone(),
            kind: e.kind.clone(),
            source: e.source.clone(),
            target: e.target.clone(),
            route: carried.clone(),
        });
        // The line asked for, else the one that carries the edge.
        let line = if map.drawn.contains_key(id) || snapshot.routes.iter().any(|r| r.id == id) {
            Some(id.to_string())
        } else {
            carried
        };
        answer.route = match line {
            Some(line) => route_details(snapshot, canonical, &map, &line, limit),
            None => None,
        };
        if answer.edge.is_none() && answer.route.is_none() {
            return Err(format!("Unknown edge: {id}"));
        }
    }
    Ok(answer)
}

fn node_details(
    snapshot: &Snapshot,
    canonical: &arqlet_model::Graph,
    map: &ViewMap,
    id: &str,
) -> Result<NodeDetails, String> {
    let node = canonical
        .nodes
        .iter()
        .find(|n| n.id == id)
        .ok_or_else(|| format!("Unknown node: {id}"))?;
    let parent_of: BTreeMap<&str, &str> = canonical
        .nodes
        .iter()
        .filter_map(|n| Some((n.id.as_str(), n.parent_id.as_deref()?)))
        .collect();
    let mut parents = Vec::new();
    let mut at = parent_of.get(id).copied();
    // Bounded by the node count, so that a malformed parent cycle cannot loop.
    while let Some(p) = at {
        if parents.len() > canonical.nodes.len() {
            break;
        }
        parents.push(p.to_string());
        at = parent_of.get(p).copied();
    }
    parents.reverse();
    let mut children_of: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (child, parent) in &parent_of {
        children_of.entry(parent).or_default().push(child);
    }
    let children = children_of.get(id).map_or(0, Vec::len);
    let mut descendants = 0usize;
    let mut stack: Vec<&str> = children_of.get(id).cloned().unwrap_or_default();
    while let Some(n) = stack.pop() {
        descendants += 1;
        if descendants > canonical.nodes.len() {
            break;
        }
        stack.extend(children_of.get(n).map(Vec::as_slice).unwrap_or(&[]));
    }
    let incoming = canonical
        .edges
        .iter()
        .filter(|e| e.target.as_deref() == Some(id))
        .count();
    let outgoing = canonical.edges.iter().filter(|e| e.source == id).count();
    let shown_as = map.owner(id);
    let bounds = shown_as.as_deref().and_then(|s| absolute(snapshot, s));
    Ok(NodeDetails {
        id: node.id.clone(),
        kind: node.kind.clone(),
        label: node.label.clone(),
        parents,
        shown_as,
        bounds,
        collapsed: snapshot.collapsed_ids.iter().any(|c| c == id),
        children,
        descendants,
        incoming,
        outgoing,
    })
}

/// The bounds of visible node `id` in the scene's coordinates: its box plus the boxes of the nodes above it (as the scene
/// computes them).
fn absolute(snapshot: &Snapshot, id: &str) -> Option<Rect> {
    let parent_of: BTreeMap<&str, Option<&str>> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let mut rect = snapshot.boxes.get(id)?.clone();
    let mut at = parent_of.get(id).copied().flatten();
    let mut steps = 0usize;
    while let Some(p) = at {
        steps += 1;
        if steps > parent_of.len() {
            break;
        }
        let b = snapshot.boxes.get(p)?;
        rect.x += b.x;
        rect.y += b.y;
        at = parent_of.get(p).copied().flatten();
    }
    Some(rect)
}

fn route_details(
    snapshot: &Snapshot,
    canonical: &arqlet_model::Graph,
    map: &ViewMap,
    id: &str,
    limit: usize,
) -> Option<LineDetails> {
    let route = snapshot.routes.iter().find(|r| r.id == id);
    let (source, target) = map.drawn.get(id).cloned().or_else(|| {
        // A line without a drawn edge of its id (a hub stub): the ends of the source edge as the view shows them.
        let e = canonical.edges.iter().find(|e| e.id == id)?;
        Some((map.owner(&e.source)?, map.owner(e.target.as_deref()?)?))
    })?;
    let kind = snapshot
        .graph
        .edges
        .iter()
        .find(|e| e.id == id)
        .or_else(|| canonical.edges.iter().find(|e| e.id == id))
        .map(|e| e.kind.clone())
        .unwrap_or_default();
    let points = route.map(|r| r.points.as_slice()).unwrap_or(&[]);
    let length: f64 = points
        .windows(2)
        .map(|w| (w[1].x - w[0].x).p_hypot(w[1].y - w[0].y))
        .sum();
    // A bend: an inner point where the line changes direction.
    let bends = points
        .windows(3)
        .filter(|w| {
            let (ax, ay) = (w[1].x - w[0].x, w[1].y - w[0].y);
            let (bx, by) = (w[2].x - w[1].x, w[2].y - w[1].y);
            ax * by - ay * bx != 0.0 && (ax != 0.0 || ay != 0.0) && (bx != 0.0 || by != 0.0)
        })
        .count();
    let bundled = snapshot
        .routes
        .iter()
        .filter(|r| r.bundle_of.as_deref() == Some(id))
        .map(|r| r.id.clone())
        .collect();
    // The edges of the source graph it carries, by the direction of their visible ends.
    let (mut edges, mut opposite) = (Vec::new(), Vec::new());
    let (mut edge_count, mut opposite_count) = (0usize, 0usize);
    for e in &canonical.edges {
        if map.route_of(&e.id, &None).as_deref() != Some(id) {
            continue;
        }
        let forth = map.owner(&e.source).as_deref() == Some(source.as_str());
        let (list, count) = if forth {
            (&mut edges, &mut edge_count)
        } else {
            (&mut opposite, &mut opposite_count)
        };
        *count += 1;
        if list.len() < limit {
            list.push(e.id.clone());
        }
    }
    Some(LineDetails {
        id: id.to_string(),
        kind,
        source,
        target,
        length,
        bends,
        weight: route.map_or(1, |r| r.weight),
        bundle_of: route.and_then(|r| r.bundle_of.clone()),
        bundled,
        edges,
        edge_count,
        opposite,
        opposite_count,
    })
}
