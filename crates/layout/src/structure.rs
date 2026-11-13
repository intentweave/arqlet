//! Structure of a directed graph (P02 of docs/recursive-optimization.md): weakly connected components, strongly
//! connected components, and whether the graph is a forest or a DAG. Pure and deterministic. The result describes the
//! input; it is reported, never used for selection.

/// The structure of a graph on nodes `0..n` with directed edges `(source, target)`. Self-loops and parallel edges are
/// counted as edges, so they make a graph cyclic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Structure {
    /// Weakly connected components, each sorted, ordered by their smallest node.
    pub components: Vec<Vec<usize>>,
    /// Number of strongly connected components. Equals `n` exactly when the graph has no directed cycle.
    pub strong_components: usize,
    /// No directed cycle, including no self-loop.
    pub is_dag: bool,
    /// Undirected cycle space is trivial: no cycle counting parallel edges and self-loops.
    pub is_forest: bool,
}

/// Analyses a graph on nodes `0..n`. Out-of-range edge endpoints are a caller error and panic.
pub fn analyse(n: usize, edges: &[(usize, usize)]) -> Structure {
    let components = weak_components(n, edges);
    let strong_components = strong_component_count(n, edges);
    // A DAG has n trivial strong components; a self-loop is a cycle and makes one component non-trivial.
    let is_dag = strong_components == n && edges.iter().all(|(a, b)| a != b);
    // Cycle space dimension of an undirected multigraph is m - n + c; it is zero exactly for a forest.
    let is_forest = edges.len() + components.len() == n;
    Structure {
        components,
        strong_components,
        is_dag,
        is_forest,
    }
}

fn weak_components(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            parent[x] = parent[parent[x]];
            x = parent[x];
        }
        x
    }
    for &(a, b) in edges {
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            // Keep the smaller root, so the representative is deterministic.
            parent[ra.max(rb)] = ra.min(rb);
        }
    }
    let mut by_root: Vec<Vec<usize>> = Vec::new();
    let mut slot = vec![usize::MAX; n];
    for v in 0..n {
        let root = find(&mut parent, v);
        if slot[root] == usize::MAX {
            slot[root] = by_root.len();
            by_root.push(Vec::new());
        }
        by_root[slot[root]].push(v);
    }
    // Nodes are visited in order, so each component is sorted and components are ordered by their smallest node.
    by_root
}

/// Number of strongly connected components, by an iterative Tarjan search (no recursion, so deep graphs are safe).
fn strong_component_count(n: usize, edges: &[(usize, usize)]) -> usize {
    let mut adj = vec![Vec::new(); n];
    for &(a, b) in edges {
        adj[a].push(b);
    }
    const NONE: usize = usize::MAX;
    let mut index = vec![NONE; n];
    let mut low = vec![0; n];
    let mut on_stack = vec![false; n];
    let mut stack = Vec::new();
    let mut next_index = 0;
    let mut count = 0;
    for root in 0..n {
        if index[root] != NONE {
            continue;
        }
        // Each frame is (node, position in its adjacency list).
        let mut frames = vec![(root, 0usize)];
        index[root] = next_index;
        low[root] = next_index;
        next_index += 1;
        stack.push(root);
        on_stack[root] = true;
        while let Some(&(v, i)) = frames.last() {
            if i < adj[v].len() {
                frames.last_mut().unwrap().1 += 1;
                let w = adj[v][i];
                if index[w] == NONE {
                    index[w] = next_index;
                    low[w] = next_index;
                    next_index += 1;
                    stack.push(w);
                    on_stack[w] = true;
                    frames.push((w, 0));
                } else if on_stack[w] {
                    low[v] = low[v].min(index[w]);
                }
            } else {
                frames.pop();
                if let Some(&(parent, _)) = frames.last() {
                    low[parent] = low[parent].min(low[v]);
                }
                if low[v] == index[v] {
                    count += 1;
                    while let Some(w) = stack.pop() {
                        on_stack[w] = false;
                        if w == v {
                            break;
                        }
                    }
                }
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_singleton() {
        let empty = analyse(0, &[]);
        assert!(empty.components.is_empty());
        assert!(empty.is_dag && empty.is_forest);
        assert_eq!(empty.strong_components, 0);
        let single = analyse(1, &[]);
        assert_eq!(single.components, vec![vec![0]]);
        assert!(single.is_dag && single.is_forest);
        assert_eq!(single.strong_components, 1);
    }

    #[test]
    fn disconnected_chains_are_separate_components_in_node_order() {
        let s = analyse(5, &[(3, 4), (0, 1), (1, 2)]);
        assert_eq!(s.components, vec![vec![0, 1, 2], vec![3, 4]]);
        assert!(s.is_dag && s.is_forest);
    }

    #[test]
    fn a_directed_cycle_is_one_strong_component_and_not_a_dag() {
        let s = analyse(3, &[(0, 1), (1, 2), (2, 0)]);
        assert_eq!(s.strong_components, 1);
        assert!(!s.is_dag);
        assert!(!s.is_forest);
    }

    #[test]
    fn a_self_loop_is_a_directed_cycle() {
        let s = analyse(2, &[(0, 0), (0, 1)]);
        assert_eq!(s.strong_components, 2);
        assert!(!s.is_dag, "a self-loop is a directed cycle");
        assert!(!s.is_forest);
    }

    #[test]
    fn parallel_and_antiparallel_edges_make_an_undirected_cycle() {
        let parallel = analyse(2, &[(0, 1), (0, 1)]);
        assert!(!parallel.is_forest);
        assert!(
            parallel.is_dag,
            "parallel edges do not form a directed cycle"
        );
        let antiparallel = analyse(2, &[(0, 1), (1, 0)]);
        assert!(!antiparallel.is_dag);
        assert_eq!(antiparallel.strong_components, 1);
    }

    #[test]
    fn a_tree_with_a_cross_link_is_not_a_forest() {
        // Tree 0-1, 0-2, 1-3 plus the cross-link 2-3.
        let s = analyse(4, &[(0, 1), (0, 2), (1, 3), (2, 3)]);
        assert!(!s.is_forest);
        assert!(s.is_dag);
        let tree = analyse(4, &[(0, 1), (0, 2), (1, 3)]);
        assert!(tree.is_forest);
    }

    #[test]
    fn a_long_chain_does_not_overflow_the_stack() {
        let n = 200_000;
        let edges: Vec<(usize, usize)> = (0..n - 1).map(|i| (i, i + 1)).collect();
        let s = analyse(n, &edges);
        assert_eq!(s.strong_components, n);
        assert!(s.is_dag && s.is_forest);
        assert_eq!(s.components.len(), 1);
    }
}

/// The structure of one parent's children, as `place` sees them: links between children are the graph edges mapped to
/// the children that own their endpoints (an edge between descendants of two children links those children).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParentStructure {
    /// The container, or `None` for the root level.
    pub parent: Option<String>,
    /// The children, sorted by id.
    pub children: Vec<String>,
    pub structure: Structure,
}

/// Per-parent structure of a graph, ordered by parent (root first, then by id). Reports only.
pub fn analyse_graph(graph: &arqlet_model::Graph) -> Vec<ParentStructure> {
    use std::collections::{BTreeMap, BTreeSet};
    let node_parent: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let mut parents: BTreeSet<Option<&str>> = BTreeSet::new();
    for n in &graph.nodes {
        parents.insert(n.parent_id.as_deref());
    }
    let mut out = Vec::new();
    for parent in parents {
        let mut children: Vec<&str> = graph
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == parent)
            .map(|n| n.id.as_str())
            .collect();
        children.sort_unstable();
        let index: BTreeMap<&str, usize> =
            children.iter().enumerate().map(|(i, c)| (*c, i)).collect();
        // The child of this parent that contains `id`, if any.
        let owner = |id: &str| -> Option<usize> {
            let mut current = Some(id);
            while let Some(c) = current {
                if let Some(i) = index.get(c) {
                    return Some(*i);
                }
                current = *node_parent.get(c)?;
            }
            None
        };
        let mut links: BTreeSet<(usize, usize)> = BTreeSet::new();
        for e in &graph.edges {
            if let Some(target) = &e.target {
                if let (Some(a), Some(b)) = (owner(&e.source), owner(target)) {
                    if a != b {
                        links.insert((a, b));
                    }
                }
            }
        }
        let links: Vec<(usize, usize)> = links.into_iter().collect();
        out.push(ParentStructure {
            parent: parent.map(String::from),
            children: children.iter().map(|c| c.to_string()).collect(),
            structure: analyse(children.len(), &links),
        });
    }
    out
}

#[cfg(test)]
mod graph_tests {
    use super::*;
    use arqlet_model::{Edge, Graph, Node};

    fn node(id: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind: "symbol".into(),
            label: id.into(),
            parent_id: parent.map(String::from),
            shape: Default::default(),
        }
    }

    fn edge(id: &str, a: &str, b: &str) -> Edge {
        Edge {
            id: id.into(),
            kind: "flow".into(),
            source: a.into(),
            target: Some(b.into()),
            weight: None,
        }
    }

    #[test]
    fn children_of_a_container_are_analysed_with_descendant_links_mapped_up() {
        // Container c holds x and y; y has a child z. Edge z->x links the children x and y of c.
        let graph = Graph {
            nodes: vec![
                node("c", None),
                node("x", Some("c")),
                node("y", Some("c")),
                node("z", Some("y")),
                node("solo", None),
            ],
            edges: vec![edge("1", "z", "x")],
            default_direction: arqlet_model::DirectionConstraint::Fixed(
                arqlet_model::Orientation::Lr,
            ),
            container_directions: Default::default(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            buses: Vec::new(),
        };
        let summary = analyse_graph(&graph);
        let c = summary
            .iter()
            .find(|p| p.parent.as_deref() == Some("c"))
            .unwrap();
        assert_eq!(c.children, vec!["x", "y"]);
        assert_eq!(c.structure.components, vec![vec![0, 1]]);
        let root = summary.iter().find(|p| p.parent.is_none()).unwrap();
        assert_eq!(root.children, vec!["c", "solo"]);
        assert_eq!(
            root.structure.components.len(),
            2,
            "the root children are not linked"
        );
    }
}
