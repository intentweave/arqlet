//! Lossless view projection: canonical graph is owned by the runtime snapshot.
use arqlet_model::*;
use std::collections::{BTreeMap, BTreeSet};
pub fn project(source: &Graph, collapsed: &BTreeSet<String>) -> Result<Graph, String> {
    validate(source)?;
    let nodes: BTreeMap<_, _> = source.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    for id in collapsed {
        if !nodes.contains_key(id.as_str())
            || !source
                .nodes
                .iter()
                .any(|n| n.parent_id.as_ref() == Some(id))
        {
            return Err(format!("Not a container: {id}"));
        }
    }
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
            // Hide connections internal to an aggregate; retain true visible self-loops.
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
    validate(&graph)?;
    Ok(graph)
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
/// returned map says, by its id, how many edges it stands for in its own direction and in the other. Edges between nodes
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
                        forward: 1,
                        backward: 0,
                    },
                );
                true
            }
            Some(first) => {
                let same = endpoints[first].0 == e.source;
                let weight = weights.get_mut(first).expect("a weight per representative");
                if same {
                    weight.forward += 1;
                } else {
                    weight.backward += 1;
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
    let edges: Vec<(usize, usize, &str)> = graph
        .edges
        .iter()
        .filter(|e| follow(e))
        .filter_map(|e| {
            let t = *index.get(e.target.as_deref()?)?;
            Some((*index.get(e.source.as_str())?, t, e.id.as_str()))
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
        edges: edges
            .iter()
            .filter(|&&(a, b, _)| on_edge(a, b))
            .map(|&(_, _, id)| id.to_string())
            .collect(),
        length: Some(length),
        reversed: false,
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
