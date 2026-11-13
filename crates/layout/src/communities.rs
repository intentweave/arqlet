//! Provisional: communities as containers of the view (option `communities`). The cluster overview's communities
//! (`overview::with_communities`) become nodes of kind `community` that hold their members, so the structured layout can
//! collapse and open them like any container. The result is a derived graph for the view projection; the source graph
//! the runtime keeps is never changed.
use crate::overview::{hierarchy, with_communities_except, Hubs};
use arqlet_model::{Graph, Node};
use std::collections::BTreeMap;

/// The node kind of the containers this adds.
pub const COMMUNITY_KIND: &str = "community";

/// The node kind of the containers given groups add (`with_given_groups`).
pub const GROUP_KIND: &str = "group";

/// The prefix of their ids: `group:<container>:<path>` (the container `#` for the top level, the path joined by `/`).
pub const GROUP_PREFIX: &str = "group:";

/// The node kind of the containers that gather the children a partly shown container does not show (`with_partial`).
pub const REST_KIND: &str = "rest";

/// The prefix of their ids: `rest:<container>`.
pub const REST_PREFIX: &str = "rest:";

/// `graph` with every container of `partial` shown in part: its children not listed move into a container
/// `rest:<container>` of kind `rest`, labelled `+N more`, after the graph's nodes. A container the graph does not have,
/// or one whose children are all listed, is left as it is.
pub fn with_partial(
    graph: &Graph,
    partial: &BTreeMap<String, Vec<String>>,
) -> Result<Graph, String> {
    if partial.is_empty() {
        return Ok(graph.clone());
    }
    let mut out = graph.clone();
    let mut added = Vec::new();
    for (container, shown) in partial {
        let rest = format!("{REST_PREFIX}{container}");
        if graph.nodes.iter().any(|n| n.id == rest) {
            return Err(format!("A node of the graph has the id of a rest: {rest}"));
        }
        let hidden: Vec<usize> = out
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                n.parent_id.as_deref() == Some(container.as_str()) && !shown.contains(&n.id)
            })
            .map(|(i, _)| i)
            .collect();
        if hidden.is_empty() || !graph.nodes.iter().any(|n| &n.id == container) {
            continue;
        }
        for &i in &hidden {
            out.nodes[i].parent_id = Some(rest.clone());
        }
        added.push(Node {
            id: rest,
            kind: REST_KIND.into(),
            label: format!("+{} more", hidden.len()),
            parent_id: Some(container.clone()),
            shape: Default::default(),
        });
    }
    out.nodes.extend(added);
    Ok(out)
}

/// `graph` with the groups the data gives (node id → path of group names inside the node's container) as containers of
/// kind `group`, nested along each path and labelled with their name, after the graph's nodes. A node keeps its place
/// in the source graph's container when it has no path. Ids the graph does not have are ignored; a group whose id a
/// node of the graph already has is refused.
pub fn with_given_groups(
    graph: &Graph,
    groups: &BTreeMap<String, Vec<String>>,
) -> Result<Graph, String> {
    if groups.is_empty() {
        return Ok(graph.clone());
    }
    let taken: std::collections::BTreeSet<&str> =
        graph.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut out = graph.clone();
    let mut made: BTreeMap<String, ()> = BTreeMap::new();
    let mut added = Vec::new();
    for node in &mut out.nodes {
        let Some(path) = groups.get(&node.id).filter(|p| !p.is_empty()) else {
            continue;
        };
        let container = node.parent_id.clone();
        let base = container.clone().unwrap_or_else(|| "#".into());
        let mut parent = container;
        for depth in 0..path.len() {
            let id = format!("{GROUP_PREFIX}{base}:{}", path[..=depth].join("/"));
            if taken.contains(id.as_str()) {
                return Err(format!("A node of the graph has the id of a group: {id}"));
            }
            if made.insert(id.clone(), ()).is_none() {
                added.push(Node {
                    id: id.clone(),
                    kind: GROUP_KIND.into(),
                    label: path[depth].clone(),
                    parent_id: parent.clone(),
                    shape: Default::default(),
                });
            }
            parent = Some(id);
        }
        node.parent_id = parent;
    }
    out.nodes.extend(added);
    Ok(out)
}

/// `graph` with the communities of every container that has more than `threshold` children (`with_communities`) as
/// containers of kind `community`, after the graph's nodes. Each is labelled after its most strongly connected member
/// (the node below it with the most edges, ties to the earlier node), with the number of the others: `parse +11`; the
/// one that gathers children without links is `unlinked (12)`.
/// `threshold` 0, or a graph without communities, returns the graph as it is.
pub fn with_community_containers(graph: &Graph, threshold: usize) -> Result<Graph, String> {
    with_community_containers_except(graph, threshold, &Hubs::default())
}

/// `with_community_containers`, leaving the `hubs` out of Louvain (`with_communities_except`): a hub stays a direct
/// child of its container instead of pulling the others into one community.
pub fn with_community_containers_except(
    graph: &Graph,
    threshold: usize,
    hubs: &Hubs,
) -> Result<Graph, String> {
    if threshold == 0 {
        return Ok(graph.clone());
    }
    let h = with_communities_except(hierarchy(graph), threshold, hubs)?;
    if h.ids.len() == h.real {
        return Ok(graph.clone());
    }
    // Edges per graph node (by weight): self-loops count once, unresolved edges not at all.
    let index: BTreeMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut degree = vec![0usize; h.real];
    for edge in &graph.edges {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        if let (Some(&a), Some(&b)) = (index.get(edge.source.as_str()), index.get(target)) {
            // By weight: an edge standing for several counts as many (saturating: 32-bit in WASM).
            degree[a] = degree[a].saturating_add(edge.count());
            if b != a {
                degree[b] = degree[b].saturating_add(edge.count());
            }
        }
    }
    // The graph nodes below every added cluster, and the best connected of them.
    let mut below: Vec<usize> = vec![0; h.ids.len()];
    let mut best: Vec<Option<usize>> = vec![None; h.ids.len()];
    for node in 0..h.real {
        let mut up = h.parent[node];
        while let Some(cluster) = up {
            if cluster >= h.real {
                below[cluster] += 1;
                if best[cluster].is_none_or(|b| degree[node] > degree[b]) {
                    best[cluster] = Some(node);
                }
            }
            up = h.parent[cluster];
        }
    }
    let mut out = graph.clone();
    for (i, node) in out.nodes.iter_mut().enumerate() {
        node.parent_id = h.parent[i].map(|p| h.ids[p].clone());
    }
    for cluster in h.real..h.ids.len() {
        let name = best[cluster].map_or_else(
            || "community".to_string(),
            |b| {
                let node = &graph.nodes[b];
                if node.label.is_empty() {
                    node.id.clone()
                } else {
                    node.label.clone()
                }
            },
        );
        // The gathering of the children without links is named for what it is.
        let label = if h.ids[cluster].ends_with(":unlinked") {
            format!("unlinked ({})", below[cluster])
        } else if below[cluster] > 1 {
            format!("{name} +{}", below[cluster] - 1)
        } else {
            name
        };
        out.nodes.push(Node {
            id: h.ids[cluster].clone(),
            kind: COMMUNITY_KIND.into(),
            label,
            parent_id: h.parent[cluster].map(|p| h.ids[p].clone()),
            shape: Default::default(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overview::COMMUNITY_PREFIX;

    /// A file of 40 functions in four groups of ten: each group a ring with chords, one edge between neighbouring groups.
    fn grouped() -> Graph {
        let mut nodes = vec![
            serde_json::json!({"id": "file", "kind": "file", "label": "file", "parentId": null}),
        ];
        let mut edges = Vec::new();
        for g in 0..4 {
            for k in 0..10 {
                let id = format!("g{g}n{k}");
                nodes.push(serde_json::json!({"id": id, "kind": "function", "label": format!("f{g}_{k}"), "parentId": "file"}));
                edges.push(serde_json::json!({"id": format!("r{g}_{k}"), "kind": "calls", "source": id, "target": format!("g{g}n{}", (k + 1) % 10)}));
                if k % 2 == 0 {
                    edges.push(serde_json::json!({"id": format!("c{g}_{k}"), "kind": "calls", "source": id, "target": format!("g{g}n{}", (k + 5) % 10)}));
                }
            }
            // The hub of each group: three more edges inside it.
            for k in [2, 4, 6] {
                edges.push(serde_json::json!({"id": format!("h{g}_{k}"), "kind": "calls", "source": format!("g{g}n0"), "target": format!("g{g}n{k}")}));
            }
            edges.push(serde_json::json!({"id": format!("x{g}"), "kind": "calls", "source": format!("g{g}n3"), "target": format!("g{}n3", (g + 1) % 4)}));
        }
        serde_json::from_value(serde_json::json!({"nodes": nodes, "edges": edges})).unwrap()
    }

    #[test]
    fn communities_become_containers_named_after_their_best_connected_member() {
        let graph = grouped();
        let out = with_community_containers(&graph, 20).unwrap();
        arqlet_model::validate(&out).unwrap();
        let added: Vec<&Node> = out
            .nodes
            .iter()
            .filter(|n| n.kind == COMMUNITY_KIND)
            .collect();
        assert_eq!(
            added.len(),
            4,
            "{:?}",
            added.iter().map(|n| &n.id).collect::<Vec<_>>()
        );
        for community in &added {
            assert!(community.id.starts_with(COMMUNITY_PREFIX));
            assert_eq!(community.parent_id.as_deref(), Some("file"));
            let members: Vec<&Node> = out
                .nodes
                .iter()
                .filter(|n| n.parent_id.as_deref() == Some(community.id.as_str()))
                .collect();
            assert_eq!(members.len(), 10);
            let group = &members[0].id[..2];
            assert!(
                members.iter().all(|n| n.id.starts_with(group)),
                "one group per community"
            );
            // The hub n0 has the most edges.
            assert_eq!(community.label, format!("f{}_0 +9", &group[1..]));
        }
        // Edges and the source's other nodes are unchanged; only parents moved.
        assert_eq!(out.edges.len(), graph.edges.len());
        assert_eq!(out.nodes.len(), graph.nodes.len() + 4);
    }

    #[test]
    fn given_groups_become_nested_containers_and_louvain_divides_only_what_is_still_large() {
        let graph = grouped();
        // Groups 0 and 1 are one domain ("core"), split into their two files; group 2 is "io"; group 3 has no path.
        let mut groups = BTreeMap::new();
        for g in 0..3 {
            for k in 0..10 {
                let path = match g {
                    0 => vec!["core".to_string(), "a.rs".to_string()],
                    1 => vec!["core".to_string(), "b.rs".to_string()],
                    _ => vec!["io".to_string()],
                };
                groups.insert(format!("g{g}n{k}"), path);
            }
        }
        groups.insert("unknown".into(), vec!["x".into()]);
        let out = with_given_groups(&graph, &groups).unwrap();
        arqlet_model::validate(&out).unwrap();
        let group = |id: &str| out.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(group("group:file:core").parent_id.as_deref(), Some("file"));
        assert_eq!(
            group("group:file:core/a.rs").parent_id.as_deref(),
            Some("group:file:core")
        );
        assert_eq!(group("group:file:core/a.rs").label, "a.rs");
        assert_eq!(
            group("g0n3").parent_id.as_deref(),
            Some("group:file:core/a.rs")
        );
        assert_eq!(group("g2n3").parent_id.as_deref(), Some("group:file:io"));
        assert_eq!(
            group("g3n3").parent_id.as_deref(),
            Some("file"),
            "no path: stays where it was"
        );
        assert_eq!(out.nodes.iter().filter(|n| n.kind == GROUP_KIND).count(), 4);
        // Louvain afterwards: "file" now has 3 groups and 10 functions (13 children), below 20; nothing more is added.
        let both = with_community_containers(&out, 20).unwrap();
        assert_eq!(both.nodes.len(), out.nodes.len());
    }

    #[test]
    fn a_partly_shown_container_gathers_the_others_in_a_rest() {
        let graph = grouped();
        let partial = BTreeMap::from([(
            "file".to_string(),
            vec!["g0n0".to_string(), "g2n5".to_string()],
        )]);
        let out = with_partial(&graph, &partial).unwrap();
        arqlet_model::validate(&out).unwrap();
        let node = |id: &str| out.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(node("g0n0").parent_id.as_deref(), Some("file"));
        assert_eq!(node("g2n5").parent_id.as_deref(), Some("file"));
        assert_eq!(node("g1n1").parent_id.as_deref(), Some("rest:file"));
        assert_eq!(node("rest:file").label, "+38 more");
        assert_eq!(node("rest:file").kind, REST_KIND);
        // Every child shown, or an unknown container: nothing to gather.
        let all: Vec<String> = graph
            .nodes
            .iter()
            .filter(|n| n.parent_id.is_some())
            .map(|n| n.id.clone())
            .collect();
        let none = with_partial(&graph, &BTreeMap::from([("file".to_string(), all)])).unwrap();
        assert_eq!(none.nodes.len(), graph.nodes.len());
        let unknown =
            with_partial(&graph, &BTreeMap::from([("nope".to_string(), vec![])])).unwrap();
        assert_eq!(unknown.nodes.len(), graph.nodes.len());
    }

    #[test]
    fn a_group_id_that_a_node_has_is_refused() {
        let mut graph = grouped();
        graph.nodes.push(Node {
            id: "group:file:core".into(),
            kind: "x".into(),
            label: "x".into(),
            parent_id: None,
            shape: Default::default(),
        });
        let groups = BTreeMap::from([("g0n0".to_string(), vec!["core".to_string()])]);
        assert!(with_given_groups(&graph, &groups).is_err());
    }

    #[test]
    fn below_the_threshold_or_off_the_graph_is_returned_as_it_is() {
        let graph = grouped();
        let json = |g: &Graph| serde_json::to_string(g).unwrap();
        assert_eq!(
            json(&with_community_containers(&graph, 0).unwrap()),
            json(&graph)
        );
        assert_eq!(
            json(&with_community_containers(&graph, 40).unwrap()),
            json(&graph)
        );
    }
}
