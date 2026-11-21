//! Lossless view projection: canonical graph is owned by the runtime snapshot.
use arqlet_model::*;
use std::collections::{BTreeMap, BTreeSet};
pub fn project(source: &Graph, collapsed: &BTreeSet<String>) -> Result<Graph, String> {
    validate(source)?;
    project_validated(source, collapsed)
}

/// `project` on a source validated before (by an earlier `project` of the same graph): the projection is still validated.
pub fn project_validated(source: &Graph, collapsed: &BTreeSet<String>) -> Result<Graph, String> {
    project_indexed(source, &GraphIndex::of(source), collapsed)
}

/// `project_validated` with the source's index (`GraphIndex::of(source)`, kept by a caller that projects the same graph
/// many times): nodes and edges are walked by index, and an edge is copied only when the view keeps it.
pub fn project_indexed(
    source: &Graph,
    index: &GraphIndex,
    collapsed: &BTreeSet<String>,
) -> Result<Graph, String> {
    let n = source.nodes.len();
    let mut closed = vec![false; n];
    for id in collapsed {
        match index.node.get(id.as_str()) {
            Some(&i) if index.container[i] => closed[i] = true,
            _ => return Err(format!("Not a container: {id}")),
        }
    }
    // What shows each node: its outermost collapsed ancestor, else itself; computed once per node from its parent's.
    let parent = &index.parent;
    let mut outer: Vec<Option<Option<usize>>> = vec![None; n];
    for start in 0..n {
        let mut chain = Vec::new();
        let mut at = start;
        while outer[at].is_none() {
            chain.push(at);
            match parent[at] {
                Some(p) => at = p,
                None => break,
            }
        }
        for &i in chain.iter().rev() {
            outer[i] = Some(parent[i].and_then(|p| {
                outer[p]
                    .expect("parent resolved first")
                    .or(closed[p].then_some(p))
            }));
        }
    }
    let shown: Vec<usize> = outer
        .iter()
        .enumerate()
        .map(|(i, o)| o.expect("every node resolved").unwrap_or(i))
        .collect();
    let visible = |i: usize| shown[i] == i;
    // The visible nodes only, in their order.
    let mut graph = Graph {
        default_direction: source.default_direction.clone(),
        container_directions: source.container_directions.clone(),
        default_strategy: source.default_strategy.clone(),
        container_strategies: source.container_strategies.clone(),
        default_routing: source.default_routing.clone(),
        container_routing: source.container_routing.clone(),
        ports: Vec::new(),
        edge_ports: Vec::new(),
        nodes: source
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| visible(*i))
            .map(|(_, node)| node.clone())
            .collect(),
        edges: Vec::new(),
        buses: source.buses.clone(),
        // Carried unchanged: a membership names nodes of the source graph, and a member the view does not show is
        // resolved to the box that stands for it when the hulls are built (ADR 0022 §3).
        memberships: source.memberships.clone(),
    };
    let id = |i: usize| source.nodes[i].id.clone();
    graph.edges = source
        .edges
        .iter()
        .zip(&index.ends)
        .filter_map(|(e, &(s, t))| {
            let s = s?;
            let (owner_s, owner_t) = (shown[s], t.map(|t| shown[t]));
            // Hide connections internal to an aggregate; retain true visible self-loops.
            if owner_t == Some(owner_s) && (owner_s != s || owner_t != t) {
                return None;
            }
            let mut edge = e.clone();
            edge.source = id(owner_s);
            edge.target = owner_t.map(id);
            Some(edge)
        })
        .collect();
    // Provisional (bus lines): a tap at a hidden node attaches to the node that shows it (a collapsed container), once
    // per bus and shown node; a bus's own container, when collapsed, shows the bus as part of it (its taps stay).
    graph.buses.retain(|b| {
        b.taps.is_empty()
            || b.parent_id
                .as_deref()
                .is_none_or(|p| index.node.get(p).is_some_and(|&i| shown[i] == i))
    });
    for bus in graph.buses.iter_mut().filter(|b| !b.taps.is_empty()) {
        let mut seen = std::collections::BTreeSet::new();
        bus.taps.retain_mut(|tap| {
            let Some(&i) = index.node.get(tap.node.as_str()) else {
                return false;
            };
            tap.node = id(shown[i]);
            seen.insert(tap.node.clone())
        });
    }
    graph.ports = source
        .ports
        .iter()
        .filter(|port| {
            index
                .node
                .get(port.node_id.as_str())
                .is_some_and(|&i| visible(i))
        })
        .cloned()
        .collect();
    if !source.edge_ports.is_empty() {
        let (old_edges, new_edges) = (first_by_id(&source.edges), first_by_id(&graph.edges));
        graph.edge_ports = source
            .edge_ports
            .iter()
            .filter_map(|b| {
                let old = *old_edges.get(b.edge_id.as_str())?;
                let new = *new_edges.get(b.edge_id.as_str())?;
                let mut binding = b.clone();
                if old.source != new.source {
                    binding.source_port = None;
                }
                if old.target != new.target {
                    binding.target_port = None;
                }
                Some(binding)
            })
            .collect();
    }
    validate(&graph)?;
    Ok(graph)
}

/// The first edge of each id, as a search from the front finds it.
fn first_by_id(edges: &[Edge]) -> BTreeMap<&str, &Edge> {
    let mut by: BTreeMap<&str, &Edge> = BTreeMap::new();
    for e in edges {
        by.entry(e.id.as_str()).or_insert(e);
    }
    by
}

/// Provisional (global layering, iteration 1): the projection with the edges that a collapsed container aggregates drawn
/// once per visible pair. An edge whose end was moved to a collapsed container is kept only when no earlier edge (in the
/// projection's order) joins the same visible pair in the same direction; edges between nodes that are both visible as
/// themselves are all kept, parallel ones included. The canonical graph is unchanged.
pub fn merge_aggregated(source: &Graph, mut projected: Graph) -> Graph {
    let original: BTreeMap<&str, (&str, Option<&str>)> = source
        .edges
        .iter()
        .map(|e| (e.id.as_str(), (e.source.as_str(), e.target.as_deref())))
        .collect();
    let mut seen: BTreeSet<(String, Option<String>)> = BTreeSet::new();
    projected.edges.retain(|e| {
        let aggregated = original
            .get(e.id.as_str())
            .is_some_and(|(s, t)| *s != e.source || *t != e.target.as_deref());
        !aggregated || seen.insert((e.source.clone(), e.target.clone()))
    });
    let kept: BTreeSet<&str> = projected.edges.iter().map(|e| e.id.as_str()).collect();
    let bindings: Vec<_> = projected
        .edge_ports
        .iter()
        .filter(|b| kept.contains(b.edge_id.as_str()))
        .cloned()
        .collect();
    projected.edge_ports = bindings;
    projected
}

/// Provisional (communities): the projection with every pair of visible ends that a collapsed container joins drawn by one
/// edge, both directions together. The first edge of the pair (in the projection's order) stands for the others; the
/// returned map says, by its id, how many edges it stands for in its own direction and in the other (by `Edge::weight`). Edges between nodes
/// that are both visible as themselves are all kept, parallel and opposite ones included. The canonical graph is unchanged.
pub fn merge_aggregated_pairs(
    source: &Graph,
    mut projected: Graph,
) -> (Graph, BTreeMap<String, AggregateWeight>) {
    let original: BTreeMap<&str, (&str, Option<&str>)> = source
        .edges
        .iter()
        .map(|e| (e.id.as_str(), (e.source.as_str(), e.target.as_deref())))
        .collect();
    let aggregated = |e: &Edge| {
        original
            .get(e.id.as_str())
            .is_some_and(|(s, t)| *s != e.source || *t != e.target.as_deref())
    };
    // The pair of every aggregated edge, unordered; the first edge of a pair represents it.
    let mut representative: BTreeMap<(String, String), String> = BTreeMap::new();
    let mut weights: BTreeMap<String, AggregateWeight> = BTreeMap::new();
    let mut endpoints: BTreeMap<String, (String, String)> = BTreeMap::new();
    projected.edges.retain(|e| {
        let Some(target) = e.target.as_ref() else {
            return true;
        };
        if !aggregated(e) || &e.source == target {
            return true;
        }
        let key = if e.source <= *target {
            (e.source.clone(), target.clone())
        } else {
            (target.clone(), e.source.clone())
        };
        match representative.get(&key) {
            None => {
                representative.insert(key, e.id.clone());
                endpoints.insert(e.id.clone(), (e.source.clone(), target.clone()));
                weights.insert(
                    e.id.clone(),
                    AggregateWeight {
                        forward: e.count(),
                        backward: 0,
                    },
                );
                true
            }
            Some(first) => {
                let same = endpoints[first].0 == e.source;
                let weight = weights.get_mut(first).expect("a weight per representative");
                // By weight (`Edge::weight`); saturating, as `usize` is 32-bit in WASM.
                if same {
                    weight.forward = weight.forward.saturating_add(e.count());
                } else {
                    weight.backward = weight.backward.saturating_add(e.count());
                }
                false
            }
        }
    });
    let kept: BTreeSet<&str> = projected.edges.iter().map(|e| e.id.as_str()).collect();
    let bindings: Vec<_> = projected
        .edge_ports
        .iter()
        .filter(|b| kept.contains(b.edge_id.as_str()))
        .cloned()
        .collect();
    projected.edge_ports = bindings;
    (projected, weights)
}

/// Provisional (`LayoutOptions::lift_edges`): every edge between two nodes that are not siblings moves its ends up to the
/// outermost containers in which they differ (the children of their lowest common container on either side), so that
/// `merge_aggregated_pairs` then draws one weighted edge for every such pair of containers. Edges between siblings, and
/// edges between a container and a node inside it, keep their ends; directions are kept (source stays source). A moved end
/// loses its explicit port. Pure: the canonical graph is unchanged.
pub fn lift_edges(mut projected: Graph) -> Graph {
    let parent: BTreeMap<&str, Option<&str>> = projected
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    // The path from the top level down to a node.
    let path = |id: &str| -> Vec<String> {
        let mut out = vec![id.to_string()];
        let mut at = parent.get(id).copied().flatten();
        while let Some(p) = at {
            out.push(p.to_string());
            at = parent.get(p).copied().flatten();
        }
        out.reverse();
        out
    };
    let mut moved: BTreeMap<String, (bool, bool)> = BTreeMap::new();
    for edge in &mut projected.edges {
        let Some(target) = edge.target.clone() else {
            continue;
        };
        if parent.get(edge.source.as_str()) == parent.get(target.as_str()) {
            continue;
        }
        let (a, b) = (path(&edge.source), path(&target));
        let common = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
        // One end inside the other: kept.
        if common == a.len() || common == b.len() {
            continue;
        }
        let (source, target_lifted) = (a[common].clone(), b[common].clone());
        let changed = (source != edge.source, target_lifted != target);
        edge.source = source;
        edge.target = Some(target_lifted);
        moved.insert(edge.id.clone(), changed);
    }
    for binding in &mut projected.edge_ports {
        if let Some(&(source, target)) = moved.get(&binding.edge_id) {
            if source {
                binding.source_port = None;
            }
            if target {
                binding.target_port = None;
            }
        }
    }
    projected
}

/// Provisional (path cut-out): which paths between two nodes `trace` follows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceMode {
    /// The nodes and edges on the shortest directed paths (by edge count).
    Shortest,
    /// The nodes and edges on any directed walk: reachable from the start and reaching the end.
    All,
}

/// The nodes and edges between two nodes (`trace`), in the graph's order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trace {
    pub nodes: Vec<String>,
    pub edges: Vec<String>,
    /// The number of edges of a shortest path; `None` when there is none.
    pub length: Option<usize>,
    /// True when no path runs from `from` to `to` and the result is the one from `to` to `from`.
    pub reversed: bool,
}

/// Provisional (path cut-out): the nodes and edges on the directed paths from `from` to `to` in `graph` (edges keep their
/// direction; unresolved ones are left out; cycles, self-loops and parallel edges are allowed). Without such a path, the
/// paths from `to` to `from`, marked `reversed`; without either, an empty trace. Unknown ids are an error.
pub fn trace(graph: &Graph, from: &str, to: &str, mode: TraceMode) -> Result<Trace, String> {
    trace_along(graph, from, to, mode, |_| true)
}

/// Provisional: `trace` along the edges `follow` accepts only (for example the edges of some kinds); the others are not
/// part of any path.
pub fn trace_along(
    graph: &Graph,
    from: &str,
    to: &str,
    mode: TraceMode,
    follow: impl Fn(&arqlet_model::Edge) -> bool,
) -> Result<Trace, String> {
    for id in [from, to] {
        if !graph.nodes.iter().any(|n| n.id == id) {
            return Err(format!("Unknown node: {id}"));
        }
    }
    let forward = trace_directed(graph, from, to, mode, &follow);
    if forward.length.is_some() {
        return Ok(forward);
    }
    let mut back = trace_directed(graph, to, from, mode, &follow);
    if back.length.is_some() {
        back.reversed = true;
        return Ok(back);
    }
    Ok(Trace::default())
}

fn trace_directed(
    graph: &Graph,
    from: &str,
    to: &str,
    mode: TraceMode,
    follow: &impl Fn(&arqlet_model::Edge) -> bool,
) -> Trace {
    let index: BTreeMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let n = graph.nodes.len();
    // A bus drawn as a bus line is a shared medium: its edges are followed both ways, so a path runs node → bus → node.
    let medium: Vec<bool> = graph
        .nodes
        .iter()
        .map(|node| node.shape == NodeShape::BusLine)
        .collect();
    let edges: Vec<(usize, usize, &str)> = graph
        .edges
        .iter()
        .filter(|e| follow(e))
        .filter_map(|e| {
            let t = *index.get(e.target.as_deref()?)?;
            Some((*index.get(e.source.as_str())?, t, e.id.as_str()))
        })
        .flat_map(|(a, b, id)| {
            let both = (medium[a] || medium[b]) && a != b;
            std::iter::once((a, b, id)).chain(both.then_some((b, a, id)))
        })
        .collect();
    let mut out = vec![Vec::new(); n];
    let mut into = vec![Vec::new(); n];
    for &(a, b, _) in &edges {
        out[a].push(b);
        into[b].push(a);
    }
    // Hop distances from the start along the edges, and to the end against them.
    let bfs = |start: usize, next: &Vec<Vec<usize>>| {
        let mut dist = vec![usize::MAX; n];
        dist[start] = 0;
        let mut queue = std::collections::VecDeque::from([start]);
        while let Some(v) = queue.pop_front() {
            for &w in &next[v] {
                if dist[w] == usize::MAX {
                    dist[w] = dist[v] + 1;
                    queue.push_back(w);
                }
            }
        }
        dist
    };
    let (s, t) = (index[from], index[to]);
    let (from_start, to_end) = (bfs(s, &out), bfs(t, &into));
    let Some(length) = (from_start[t] != usize::MAX).then_some(from_start[t]) else {
        return Trace::default();
    };
    let reached = |v: usize| from_start[v] != usize::MAX && to_end[v] != usize::MAX;
    let on_node = |v: usize| match mode {
        TraceMode::Shortest => reached(v) && from_start[v] + to_end[v] == length,
        TraceMode::All => reached(v),
    };
    let on_edge = |a: usize, b: usize| match mode {
        TraceMode::Shortest => reached(a) && reached(b) && from_start[a] + 1 + to_end[b] == length,
        TraceMode::All => reached(a) && reached(b),
    };
    Trace {
        nodes: (0..n)
            .filter(|&v| on_node(v))
            .map(|v| graph.nodes[v].id.clone())
            .collect(),
        edges: {
            let mut seen = BTreeSet::new();
            edges
                .iter()
                .filter(|&&(a, b, _)| on_edge(a, b))
                .filter(|&&(_, _, id)| seen.insert(id))
                .map(|&(_, _, id)| id.to_string())
                .collect()
        },
        length: Some(length),
        reversed: false,
    }
}

/// Provisional: which way a `neighbourhood` follows edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Along the edges (what the start leads to).
    Out,
    /// Against the edges (what leads to the start).
    In,
    /// Either way.
    Both,
}

/// The nodes within some hops of start nodes (`neighbourhood`), in the graph's order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Neighbourhood {
    pub nodes: Vec<String>,
    /// The edges the neighbourhood was reached by: a followed edge that leaves a node closer than the hop limit in an
    /// allowed direction (so edges between two nodes at the limit are not part of it).
    pub edges: Vec<String>,
    /// The hops of every node from the nearest start (0 for a start).
    pub hops: BTreeMap<String, usize>,
}

/// Provisional: the nodes and edges within `hops` edges of the `starts` in `graph`, following only edges `follow` accepts,
/// in the direction `reach` allows. Unresolved edges are left out; cycles, self-loops and parallel edges are allowed.
/// Unknown start ids are an error.
pub fn neighbourhood(
    graph: &Graph,
    starts: &[&str],
    hops: usize,
    reach: Reach,
    follow: impl Fn(&Edge) -> bool,
) -> Result<Neighbourhood, String> {
    let index: BTreeMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut start = Vec::new();
    for id in starts {
        start.push(*index.get(id).ok_or_else(|| format!("Unknown node: {id}"))?);
    }
    let n = graph.nodes.len();
    let edges: Vec<(usize, usize, &str)> = graph
        .edges
        .iter()
        .filter(|e| follow(e))
        .filter_map(|e| {
            let t = *index.get(e.target.as_deref()?)?;
            Some((*index.get(e.source.as_str())?, t, e.id.as_str()))
        })
        .collect();
    let mut next = vec![Vec::new(); n];
    for &(a, b, _) in &edges {
        if matches!(reach, Reach::Out | Reach::Both) {
            next[a].push(b);
        }
        if matches!(reach, Reach::In | Reach::Both) {
            next[b].push(a);
        }
    }
    let mut dist = vec![usize::MAX; n];
    let mut queue = std::collections::VecDeque::new();
    for &s in &start {
        if dist[s] == usize::MAX {
            dist[s] = 0;
            queue.push_back(s);
        }
    }
    while let Some(v) = queue.pop_front() {
        if dist[v] >= hops {
            continue;
        }
        for &w in &next[v] {
            if dist[w] == usize::MAX {
                dist[w] = dist[v] + 1;
                queue.push_back(w);
            }
        }
    }
    let inside = |v: usize| dist[v] < hops;
    let reached = |v: usize| dist[v] != usize::MAX;
    let used = |a: usize, b: usize| {
        (matches!(reach, Reach::Out | Reach::Both) && inside(a) && reached(b))
            || (matches!(reach, Reach::In | Reach::Both) && inside(b) && reached(a))
    };
    Ok(Neighbourhood {
        nodes: (0..n)
            .filter(|&v| reached(v))
            .map(|v| graph.nodes[v].id.clone())
            .collect(),
        edges: edges
            .iter()
            .filter(|&&(a, b, _)| used(a, b))
            .map(|&(_, _, id)| id.to_string())
            .collect(),
        hops: (0..n)
            .filter(|&v| reached(v))
            .map(|v| (graph.nodes[v].id.clone(), dist[v]))
            .collect(),
    })
}

#[cfg(test)]
mod neighbourhood_tests {
    use super::*;

    fn graph() -> Graph {
        // a → b → c → d, b → b (loop), e → b, c → a (cycle), x unresolved from d, a → b twice (parallel).
        serde_json::from_value(serde_json::json!({
            "nodes": (["a", "b", "c", "d", "e", "f"].iter().map(|id| serde_json::json!({"id": id, "kind": "n", "label": id})).collect::<Vec<_>>()),
            "edges": [
                {"id": "ab", "kind": "calls", "source": "a", "target": "b"},
                {"id": "ab2", "kind": "calls", "source": "a", "target": "b"},
                {"id": "bc", "kind": "calls", "source": "b", "target": "c"},
                {"id": "cd", "kind": "data", "source": "c", "target": "d"},
                {"id": "bb", "kind": "calls", "source": "b", "target": "b"},
                {"id": "eb", "kind": "calls", "source": "e", "target": "b"},
                {"id": "ca", "kind": "calls", "source": "c", "target": "a"},
                {"id": "dx", "kind": "calls", "source": "d", "target": null}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn hops_direction_and_kinds_bound_the_neighbourhood() {
        let g = graph();
        let all = |_: &Edge| true;
        let out1 = neighbourhood(&g, &["b"], 1, Reach::Out, all).unwrap();
        assert_eq!(out1.nodes, ["b", "c"]);
        assert_eq!(out1.edges, ["bc", "bb"]);
        assert_eq!(out1.hops["c"], 1);
        let in1 = neighbourhood(&g, &["b"], 1, Reach::In, all).unwrap();
        assert_eq!(in1.nodes, ["a", "b", "e"]);
        assert_eq!(in1.edges, ["ab", "ab2", "bb", "eb"]);
        let both2 = neighbourhood(&g, &["b"], 2, Reach::Both, all).unwrap();
        assert_eq!(both2.nodes, ["a", "b", "c", "d", "e"]);
        assert_eq!(both2.hops["d"], 2);
        // ca joins two nodes at hop 1: reached from c (hop 1 < 2), so it is part of it.
        assert!(both2.edges.contains(&"ca".to_string()));
        // Only data edges from c: d; the calls are not followed.
        let data = neighbourhood(&g, &["c"], 3, Reach::Both, |e: &Edge| e.kind == "data").unwrap();
        assert_eq!(data.nodes, ["c", "d"]);
        assert_eq!(data.edges, ["cd"]);
        // Zero hops: the starts alone; several starts; an unknown start is an error.
        assert_eq!(
            neighbourhood(&g, &["a", "e"], 0, Reach::Both, all)
                .unwrap()
                .nodes,
            ["a", "e"]
        );
        assert!(neighbourhood(&g, &["nowhere"], 1, Reach::Both, all).is_err());
        // f has no edges: only itself.
        assert_eq!(
            neighbourhood(&g, &["f"], 5, Reach::Both, all)
                .unwrap()
                .nodes,
            ["f"]
        );
    }
}

#[cfg(test)]
mod trace_tests {
    use super::*;

    /// a → b → d, a → c → d (two shortest paths), d → e, a long way a → x → y → z → e, a cycle d → b, a parallel
    /// edge b → d, an unresolved edge, and f without edges.
    fn graph() -> Graph {
        let nodes: Vec<serde_json::Value> = ["a", "b", "c", "d", "e", "x", "y", "z", "f"]
            .iter()
            .map(|id| serde_json::json!({"id": id, "kind": "n", "label": id, "parentId": null}))
            .collect();
        serde_json::from_value(serde_json::json!({
            "nodes": nodes,
            "edges": [
                {"id": "ab", "kind": "k", "source": "a", "target": "b"},
                {"id": "ac", "kind": "k", "source": "a", "target": "c"},
                {"id": "bd", "kind": "k", "source": "b", "target": "d"},
                {"id": "bd2", "kind": "k", "source": "b", "target": "d"},
                {"id": "cd", "kind": "k", "source": "c", "target": "d"},
                {"id": "de", "kind": "k", "source": "d", "target": "e"},
                {"id": "db", "kind": "k", "source": "d", "target": "b"},
                {"id": "ax", "kind": "k", "source": "a", "target": "x"},
                {"id": "xy", "kind": "k", "source": "x", "target": "y"},
                {"id": "yz", "kind": "k", "source": "y", "target": "z"},
                {"id": "ze", "kind": "k", "source": "z", "target": "e"},
                {"id": "open", "kind": "k", "source": "a", "target": null}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn shortest_paths_take_every_shortest_way_and_parallel_edges() {
        let t = trace(&graph(), "a", "e", TraceMode::Shortest).unwrap();
        assert_eq!(t.length, Some(3));
        assert_eq!(t.nodes, ["a", "b", "c", "d", "e"]);
        assert_eq!(t.edges, ["ab", "ac", "bd", "bd2", "cd", "de"]);
        assert!(!t.reversed);
    }

    #[test]
    fn all_paths_add_the_long_way_and_the_cycle() {
        let t = trace(&graph(), "a", "e", TraceMode::All).unwrap();
        assert_eq!(t.nodes, ["a", "b", "c", "d", "e", "x", "y", "z"]);
        assert!(
            t.edges.contains(&"db".to_string()),
            "the cycle d → b lies between a and e"
        );
        assert!(t.edges.contains(&"yz".to_string()));
        assert!(!t.edges.contains(&"open".to_string()));
    }

    #[test]
    fn against_the_edges_the_reverse_trace_is_marked_and_without_a_path_it_is_empty() {
        let t = trace(&graph(), "e", "a", TraceMode::Shortest).unwrap();
        assert!(t.reversed);
        assert_eq!(t.nodes, ["a", "b", "c", "d", "e"]);
        assert_eq!(
            trace(&graph(), "a", "f", TraceMode::Shortest).unwrap(),
            Trace::default()
        );
        assert!(trace(&graph(), "a", "nope", TraceMode::Shortest).is_err());
        let same = trace(&graph(), "b", "b", TraceMode::Shortest).unwrap();
        assert_eq!((same.nodes, same.length), (vec!["b".to_string()], Some(0)));
    }
}

#[cfg(test)]
mod pair_tests {
    use super::*;

    #[test]
    fn both_directions_between_two_collapsed_containers_are_one_edge_with_their_counts() {
        let graph: Graph = serde_json::from_value(serde_json::json!({
            "nodes": [
                {"id": "A", "kind": "c", "label": "A", "parentId": null},
                {"id": "B", "kind": "c", "label": "B", "parentId": null},
                {"id": "a1", "kind": "n", "label": "a1", "parentId": "A"},
                {"id": "a2", "kind": "n", "label": "a2", "parentId": "A"},
                {"id": "b1", "kind": "n", "label": "b1", "parentId": "B"},
                {"id": "x", "kind": "n", "label": "x", "parentId": null},
                {"id": "y", "kind": "n", "label": "y", "parentId": null}
            ],
            "edges": [
                {"id": "e1", "kind": "k", "source": "a1", "target": "b1"},
                {"id": "e2", "kind": "k", "source": "a2", "target": "b1"},
                {"id": "e3", "kind": "k", "source": "b1", "target": "a1"},
                {"id": "e4", "kind": "k", "source": "a1", "target": "a2"},
                {"id": "e5", "kind": "k", "source": "x", "target": "y"},
                {"id": "e6", "kind": "k", "source": "x", "target": "y"},
                {"id": "e7", "kind": "k", "source": "y", "target": "x"}
            ]
        }))
        .unwrap();
        let collapsed = BTreeSet::from(["A".to_string(), "B".to_string()]);
        let (merged, weights) =
            merge_aggregated_pairs(&graph, project(&graph, &collapsed).unwrap());
        let ids: Vec<&str> = merged.edges.iter().map(|e| e.id.as_str()).collect();
        // e1 stands for e1, e2 (A → B) and e3 (B → A); e4 is inside A; x and y keep all three of their edges.
        assert_eq!(ids, ["e1", "e5", "e6", "e7"]);
        assert_eq!(
            weights["e1"],
            AggregateWeight {
                forward: 2,
                backward: 1
            }
        );
        assert_eq!(weights.len(), 1);
        let e1 = &merged.edges[0];
        assert_eq!(
            (e1.source.as_str(), e1.target.as_deref()),
            ("A", Some("B")),
            "the logical direction of its first edge"
        );
    }
}

#[cfg(test)]
mod lift_tests {
    use super::*;

    #[test]
    fn edges_between_containers_are_lifted_and_merged_with_their_weights() {
        // P = {p1, p2, R = {r1}}, Q = {q1, q2}, z at the top level.
        let node = |id: &str, parent: Option<&str>| serde_json::json!({"id": id, "kind": "n", "label": id, "parentId": parent});
        let edge = |id: &str, s: &str, t: &str| serde_json::json!({"id": id, "kind": "k", "source": s, "target": t});
        let g: Graph = serde_json::from_value(serde_json::json!({
            "nodes": [node("P", None), node("p1", Some("P")), node("p2", Some("P")), node("R", Some("P")),
                node("r1", Some("R")), node("Q", None), node("q1", Some("Q")), node("q2", Some("Q")), node("z", None)],
            "edges": [edge("e1", "p1", "q1"), edge("e2", "q2", "p2"), edge("e3", "r1", "q1"), edge("e4", "p1", "p2"),
                edge("e5", "p1", "p2"), edge("e6", "r1", "p2"), edge("e7", "z", "q2"), edge("e8", "P", "p1"), edge("e9", "q1", "q1")]
        }))
        .unwrap();
        let lifted = lift_edges(project(&g, &BTreeSet::new()).unwrap());
        let ends = |id: &str| {
            let e = lifted.edges.iter().find(|e| e.id == id).unwrap();
            (e.source.as_str(), e.target.as_deref().unwrap())
        };
        // Between P and Q in both directions, keeping the direction; from inside R to P's child: R to p2.
        assert_eq!(ends("e1"), ("P", "Q"));
        assert_eq!(ends("e2"), ("Q", "P"));
        assert_eq!(ends("e3"), ("P", "Q"));
        assert_eq!(ends("e6"), ("R", "p2"));
        assert_eq!(ends("e7"), ("z", "Q"));
        // Siblings, a container to its own child and a self-loop keep their ends.
        assert_eq!(ends("e4"), ("p1", "p2"));
        assert_eq!(ends("e8"), ("P", "p1"));
        assert_eq!(ends("e9"), ("q1", "q1"));
        let (merged, weights) = merge_aggregated_pairs(&g, lifted);
        let ids: Vec<&str> = merged.edges.iter().map(|e| e.id.as_str()).collect();
        // One edge for P–Q (e1 stands for e1 and e3 forward, e2 backward); the parallel siblings both stay.
        assert_eq!(ids, ["e1", "e4", "e5", "e6", "e7", "e8", "e9"]);
        assert_eq!((weights["e1"].forward, weights["e1"].backward), (2, 1));
    }
}

#[cfg(test)]
mod indexed_tests {
    use super::*;

    /// The projection as it was first written: a copy of the whole graph, each end's visible node found by a walk to the
    /// root, the hidden dropped afterwards.
    fn reference(source: &Graph, collapsed: &BTreeSet<String>) -> Graph {
        let nodes: BTreeMap<_, _> = source.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        let owner = |id: &str| {
            let mut current = nodes[id].parent_id.as_deref();
            let mut visible = id.to_string();
            while let Some(p) = current {
                if collapsed.contains(p) {
                    visible = p.into();
                }
                current = nodes[p].parent_id.as_deref();
            }
            visible
        };
        let visible: BTreeSet<_> = source
            .nodes
            .iter()
            .filter(|n| owner(&n.id) == n.id)
            .map(|n| n.id.clone())
            .collect();
        let mut graph = source.clone();
        graph.nodes.retain(|n| visible.contains(&n.id));
        graph.edges = source
            .edges
            .iter()
            .filter_map(|e| {
                let mut edge = e.clone();
                edge.source = owner(&e.source);
                edge.target = e.target.as_deref().map(owner);
                if edge.target.as_ref() == Some(&edge.source)
                    && (edge.source != e.source || edge.target != e.target)
                {
                    return None;
                }
                Some(edge)
            })
            .collect();
        graph.ports.retain(|port| visible.contains(&port.node_id));
        graph.edge_ports = source
            .edge_ports
            .iter()
            .filter_map(|b| {
                let old = source.edges.iter().find(|e| e.id == b.edge_id)?;
                let new = graph.edges.iter().find(|e| e.id == b.edge_id)?;
                let mut binding = b.clone();
                if old.source != new.source {
                    binding.source_port = None;
                }
                if old.target != new.target {
                    binding.target_port = None;
                }
                Some(binding)
            })
            .collect();
        graph
    }

    #[test]
    fn the_indexed_projection_equals_the_projection_for_every_collapsed_set() {
        let node = |id: &str, parent: Option<&str>| serde_json::json!({"id": id, "kind": "n", "label": id, "parentId": parent});
        let edge = |id: &str, s: &str, t: Option<&str>| serde_json::json!({"id": id, "kind": "k", "source": s, "target": t});
        // A = {a1, B = {b1, b2}}, C = {c1}, z; ports on b1 and c1, bound to edges; a self-loop, parallel edges, an edge to a
        // container, an unresolved one.
        let g: Graph = serde_json::from_value(serde_json::json!({
            "nodes": [node("A", None), node("a1", Some("A")), node("B", Some("A")), node("b1", Some("B")), node("b2", Some("B")),
                node("C", None), node("c1", Some("C")), node("z", None)],
            "edges": [edge("e1", "b1", Some("c1")), edge("e2", "b1", Some("b2")), edge("e3", "b2", Some("b2")),
                edge("e4", "a1", Some("C")), edge("e5", "z", Some("b1")), edge("e6", "z", Some("b1")), edge("e7", "c1", None),
                edge("e8", "c1", Some("a1"))],
            "ports": [{"id": "p1", "nodeId": "b1", "side": "right", "offset": 0.5}, {"id": "p2", "nodeId": "c1", "side": "left", "offset": 0.5}],
            "edgePorts": [{"edgeId": "e1", "sourcePort": "p1", "targetPort": "p2"}]
        }))
        .unwrap();
        let index = GraphIndex::of(&g);
        let containers = ["A", "B", "C"];
        for mask in 0..8u8 {
            let collapsed: BTreeSet<String> = containers
                .iter()
                .enumerate()
                .filter(|(i, _)| mask & (1 << i) != 0)
                .map(|(_, c)| c.to_string())
                .collect();
            let plain = reference(&g, &collapsed);
            assert_eq!(
                serde_json::to_string(&project(&g, &collapsed).unwrap()).unwrap(),
                serde_json::to_string(&plain).unwrap()
            );
            let indexed = project_indexed(&g, &index, &collapsed).unwrap();
            assert_eq!(
                serde_json::to_string(&indexed).unwrap(),
                serde_json::to_string(&plain).unwrap(),
                "collapsed {collapsed:?}"
            );
        }
        // Not a container: refused alike.
        let leaf = BTreeSet::from(["a1".to_string()]);
        assert_eq!(
            project(&g, &leaf).unwrap_err(),
            project_indexed(&g, &index, &leaf).unwrap_err()
        );
    }
}

#[cfg(test)]
mod bus_trace_tests {
    use super::*;

    #[test]
    fn a_trace_crosses_a_bus_line_from_node_to_node() {
        // Two nodes on one bus, every edge from the node to the bus: no directed path, but the bus is a shared medium.
        let node = |id: &str, shape: &str| serde_json::json!({"id": id, "kind": "k", "label": id, "shape": shape});
        let edge = |id: &str, s: &str, t: &str| serde_json::json!({"id": id, "kind": "connects", "source": s, "target": t});
        let g: Graph = serde_json::from_value(serde_json::json!({
            "nodes": [node("bus", "busLine"), node("a", "rect"), node("b", "rect"), node("c", "rect")],
            "edges": [edge("ab", "a", "bus"), edge("bb", "b", "bus"), edge("ca", "c", "a")]
        }))
        .unwrap();
        let found = trace(&g, "a", "b", TraceMode::Shortest).unwrap();
        assert_eq!(found.length, Some(2));
        assert_eq!(found.edges, ["ab", "bb"]);
        assert!(!found.reversed);
        // An edge between two ordinary nodes keeps its direction: from a to c there is no path, from c to a one.
        let back = trace(&g, "a", "c", TraceMode::Shortest).unwrap();
        assert!(back.reversed);
        assert_eq!(back.edges, ["ca"]);
    }
}
