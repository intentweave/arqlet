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
    let navigable = navigable(snapshot);
    let canonical = navigable.as_ref();
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

/// Provisional (buses as data): the snapshot's source graph, with its buses given as members (`Bus::taps`) as nodes of
/// shape `busLine` and their edges (`arqlet_layout::with_bus_nodes`), so that a path or a neighbourhood crosses them as
/// it crosses a `busLine` node.
pub(crate) fn navigable(snapshot: &Snapshot) -> std::borrow::Cow<'_, arqlet_model::Graph> {
    let source = snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph);
    match arqlet_layout::with_bus_nodes(source) {
        Some(graph) => std::borrow::Cow::Owned(graph),
        None => std::borrow::Cow::Borrowed(source),
    }
}

/// Provisional: how the current view shows the source graph: the visible box of every node (itself, or the outermost
/// collapsed container above it) and the drawn line that carries every edge. Built once per request (`trace`,
/// `neighbourhood`, `details`).
pub(crate) struct ViewMap {
    pub(crate) parent: BTreeMap<String, Option<String>>,
    pub(crate) visible: BTreeSet<String>,
    collapsed: BTreeSet<String>,
    /// The ends of every edge of the source graph (with buses as nodes).
    ends: BTreeMap<String, (String, Option<String>)>,
    /// The drawn edges (and taps of drawn bus lines) by id: their visible ends, and their kinds.
    pub(crate) drawn: BTreeMap<String, (String, String)>,
    drawn_kind: BTreeMap<String, String>,
    /// The drawn edges between two visible nodes, in id order (one direction per key).
    pairs: BTreeMap<(String, String), Vec<String>>,
    trunk_taps: BTreeSet<String>,
    hub_refs: BTreeSet<String>,
}

impl ViewMap {
    pub(crate) fn new(
        snapshot: &Snapshot,
        canonical: &arqlet_model::Graph,
    ) -> Result<ViewMap, String> {
        let view = structured(canonical, &snapshot.options)?;
        let parent = view
            .nodes
            .iter()
            .map(|n| (n.id.clone(), n.parent_id.clone()))
            .collect();
        // A drawn bus line (`Bus::taps`) is shown as its bus node is, its taps as the bus's edges.
        let lines: Vec<&arqlet_model::Bus> = snapshot
            .graph
            .buses
            .iter()
            .filter(|b| snapshot.bus_lines.contains_key(&b.id))
            .collect();
        let visible = snapshot
            .graph
            .nodes
            .iter()
            .map(|n| n.id.clone())
            .chain(lines.iter().map(|b| b.id.clone()))
            .collect();
        let ends = canonical
            .edges
            .iter()
            .map(|e| (e.id.clone(), (e.source.clone(), e.target.clone())))
            .collect();
        let taps = || {
            lines.iter().flat_map(|b| {
                b.taps.iter().map(move |t| {
                    let ends = if t.to_bus {
                        (t.node.clone(), b.id.clone())
                    } else {
                        (b.id.clone(), t.node.clone())
                    };
                    (t.edge.clone(), ends, t.kind.clone())
                })
            })
        };
        let drawn: BTreeMap<String, (String, String)> = snapshot
            .graph
            .edges
            .iter()
            .filter_map(|e| Some((e.id.clone(), (e.source.clone(), e.target.clone()?))))
            .chain(taps().map(|(id, ends, _)| (id, ends)))
            .collect();
        let drawn_kind = snapshot
            .graph
            .edges
            .iter()
            .map(|e| (e.id.clone(), e.kind.clone()))
            .chain(taps().map(|(id, _, kind)| (id, kind)))
            .collect();
        let mut pairs: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for (id, (x, y)) in &drawn {
            pairs
                .entry((x.clone(), y.clone()))
                .or_default()
                .push(id.clone());
        }
        // A tap of a bus drawn as a shared line (`busStyle: trunk`) is the drawn edge with its id.
        let trunk_taps = snapshot
            .graph
            .buses
            .iter()
            .filter(|b| !b.taps.is_empty() && !b.edges.is_empty())
            .flat_map(|b| b.edges.iter().cloned())
            .collect();
        // A reference to a hub (`hubRefs`) is drawn as a stub with the edge's id.
        let hub_refs = snapshot.hub_refs.iter().map(|r| r.edge.clone()).collect();
        Ok(ViewMap {
            parent,
            visible,
            collapsed: snapshot.collapsed_ids.iter().cloned().collect(),
            ends,
            drawn,
            drawn_kind,
            pairs,
            trunk_taps,
            hub_refs,
        })
    }

    /// The visible box that shows node `id`: the outermost visible node on its way up, a collapsed one first; `None` when
    /// the view shows none of them.
    pub(crate) fn owner(&self, id: &str) -> Option<String> {
        let mut at = Some(id);
        let mut shown = None;
        while let Some(node) = at {
            if self.visible.contains(node) {
                // The outermost visible one: a node inside an expanded container is visible as itself.
                shown.get_or_insert(node);
                if self.collapsed.contains(node) {
                    shown = Some(node);
                }
            }
            at = self.parent.get(node).and_then(|p| p.as_deref());
        }
        shown.map(String::from)
    }

    /// The drawn line that carries edge `edge` of the source graph: the edge itself when it is drawn, else the edge the
    /// view draws between the two visible ends (with communities, one per pair for both directions; with `kinds`, a
    /// drawn edge of a followed kind first). `None` when the view does not show it.
    pub(crate) fn route_of(&self, edge: &str, kinds: &Option<BTreeSet<String>>) -> Option<String> {
        if self.hub_refs.contains(edge) {
            return Some(edge.to_string());
        }
        if self.trunk_taps.contains(edge) && self.drawn.contains_key(edge) {
            return Some(edge.to_string());
        }
        let (s, Some(t)) = self.ends.get(edge)? else {
            return None;
        };
        let (a, b) = (self.owner(s)?, self.owner(t)?);
        if a == b {
            return None;
        }
        if self
            .drawn
            .get(edge)
            .is_some_and(|(x, y)| *x == a && *y == b)
        {
            return Some(edge.to_string());
        }
        let none = Vec::new();
        let forth = self.pairs.get(&(a.clone(), b.clone())).unwrap_or(&none);
        let back = self.pairs.get(&(b, a)).unwrap_or(&none);
        let mut between: Vec<&String> = forth.iter().chain(back.iter()).collect();
        between.sort();
        let followed = |id: &str| {
            kinds.as_ref().is_none_or(|k| {
                self.drawn_kind
                    .get(id)
                    .is_some_and(|kind| k.contains(kind.as_str()))
            })
        };
        between
            .iter()
            .find(|id| followed(id))
            .or_else(|| between.first())
            .map(|id| (*id).clone())
    }
}

pub(crate) fn on_view(
    snapshot: &Snapshot,
    path_nodes: &[String],
    path_edges: &[String],
    kinds: &Option<BTreeSet<String>>,
) -> Result<OnView, String> {
    let navigable = navigable(snapshot);
    let map = ViewMap::new(snapshot, navigable.as_ref())?;
    let mut nodes: Vec<String> = path_nodes.iter().filter_map(|n| map.owner(n)).collect();
    nodes.sort();
    nodes.dedup();
    let routes: BTreeSet<String> = path_edges
        .iter()
        .filter_map(|e| map.route_of(e, kinds))
        .collect();
    let (visible, parent, drawn) = (&map.visible, &map.parent, &map.drawn);
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
            at = parent.get(n).and_then(|p| p.as_deref());
        }
    }
    let mut next: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (s, t) in drawn.values() {
        next.entry(s.as_str()).or_default().push(t.as_str());
        next.entry(t.as_str()).or_default().push(s.as_str());
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
    let navigable = navigable(snapshot);
    let canonical = navigable.as_ref();
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

/// Provisional (`overviewTrace`): the paths between `from` and `to` in a graph, without a view (`mode`, `edgeKinds` as for
/// `trace`). The overview's viewer maps them to its discs.
pub fn graph_trace(
    graph: &arqlet_model::Graph,
    v: &Value,
) -> Result<arqlet_model::GraphTrace, String> {
    let from = v["from"].as_str().ok_or("Missing from")?;
    let to = v["to"].as_str().ok_or("Missing to")?;
    let mode = match v["mode"].as_str() {
        None | Some("shortest") => TraceMode::Shortest,
        Some("all") => TraceMode::All,
        Some(other) => return Err(format!("Unknown trace mode: {other}")),
    };
    let kinds = edge_kinds_of(v, "Trace")?;
    let path = trace_along(graph, from, to, mode, |e| {
        kinds.as_ref().is_none_or(|k| k.contains(&e.kind))
    })?;
    Ok(arqlet_model::GraphTrace {
        found: path.length.is_some(),
        reversed: path.reversed,
        length: path.length,
        nodes: path.nodes,
        edges: path.edges,
    })
}

/// Provisional (`overviewNeighbourhood`): the nodes within `hops` of `nodeId` or `nodeIds` in a graph, without a view
/// (`direction`, `edgeKinds` as for `neighbourhood`).
pub fn graph_neighbourhood(
    graph: &arqlet_model::Graph,
    v: &Value,
) -> Result<arqlet_model::GraphNeighbourhood, String> {
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
    let found = neighbourhood(graph, &starts, hops, reach, |e| {
        kinds.as_ref().is_none_or(|k| k.contains(&e.kind))
    })?;
    Ok(arqlet_model::GraphNeighbourhood {
        nodes: found.nodes,
        edges: found.edges,
        hops: found.hops,
    })
}

/// A graph read answer as JSON (`{ok: true, …}`), or the error.
pub(crate) fn graph_read_response(graph: Result<arqlet_model::Graph, String>, v: &Value) -> Value {
    let result = graph.and_then(|g| match v["op"].as_str() {
        Some("overviewTrace") => graph_trace(&g, v).and_then(|a| answered(&a)),
        _ => graph_neighbourhood(&g, v).and_then(|a| answered(&a)),
    });
    result.unwrap_or_else(|e| json!({"ok": false, "error": e}))
}
