//! Provisional: a host computes the view's communities once (`community_groups`) and gives them back as `groups`.
use arqlet_model::{Graph, LayoutOptions};
use arqlet_runtime::{community_groups, structured};
use serde_json::json;
use std::collections::BTreeMap;

/// Rings of different sizes at the top level and inside a container, with a few edges between them.
fn graph() -> Graph {
    let mut nodes = vec![json!({"id": "box", "kind": "dir", "label": "box"})];
    let mut edges = Vec::new();
    let mut ring = |name: &str,
                    size: usize,
                    parent: Option<&str>,
                    nodes: &mut Vec<serde_json::Value>,
                    edges: &mut Vec<serde_json::Value>| {
        for i in 0..size {
            nodes.push(json!({"id": format!("{name}{i}"), "kind": "fn", "label": format!("{name}{i}"), "parentId": parent}));
            edges.push(json!({"id": format!("{name}-{i}"), "kind": "calls", "source": format!("{name}{i}"), "target": format!("{name}{}", (i + 1) % size)}));
        }
    };
    ring("a", 9, None, &mut nodes, &mut edges);
    ring("b", 7, None, &mut nodes, &mut edges);
    ring("c", 11, None, &mut nodes, &mut edges);
    ring("d", 8, Some("box"), &mut nodes, &mut edges);
    ring("e", 9, Some("box"), &mut nodes, &mut edges);
    edges.push(json!({"id": "ab", "kind": "calls", "source": "a0", "target": "b0"}));
    edges.push(json!({"id": "de", "kind": "calls", "source": "d0", "target": "e0"}));
    edges.push(json!({"id": "loose", "kind": "calls", "source": "c3", "target": null}));
    serde_json::from_value(json!({"defaultDirection": "lr", "nodes": nodes, "edges": edges}))
        .unwrap()
}

/// Every node's path of container labels in a structured view, up to the top.
fn paths(view: &Graph) -> BTreeMap<String, Vec<String>> {
    let by_id: BTreeMap<&str, &arqlet_model::Node> =
        view.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    view.nodes
        .iter()
        .map(|n| {
            let mut path = Vec::new();
            let mut at = n.parent_id.as_deref();
            while let Some(p) = at {
                path.push(by_id[p].label.clone());
                at = by_id[p].parent_id.as_deref();
            }
            (n.id.clone(), path)
        })
        .collect()
}

#[test]
fn precomputed_groups_give_the_communities_without_louvain() {
    let g = graph();
    let options: LayoutOptions = serde_json::from_value(json!({"communities": 6})).unwrap();
    let groups = community_groups(&g, &options).unwrap();
    assert!(!groups.is_empty(), "the rings are divided");
    let by_louvain = structured(&g, &options).unwrap().into_owned();
    let given = LayoutOptions {
        groups: groups.clone(),
        ..options.clone()
    };
    let by_groups = structured(&g, &given).unwrap().into_owned();
    // The same members under the same labels, for every node of the graph.
    let (louvain, precomputed) = (paths(&by_louvain), paths(&by_groups));
    for node in &g.nodes {
        assert_eq!(louvain[&node.id], precomputed[&node.id], "{}", node.id);
    }
    // Louvain divides nothing further: computed again on the grouped view, the groups are the same.
    assert_eq!(community_groups(&g, &given).unwrap(), groups);
    // The containers are groups now.
    assert!(by_groups.nodes.iter().all(|n| n.kind != "community"));
}

#[test]
fn without_communities_the_given_groups_come_back() {
    let g = graph();
    let groups = BTreeMap::from([("a0".to_string(), vec!["front".to_string()])]);
    let options = LayoutOptions {
        groups: groups.clone(),
        ..LayoutOptions::default()
    };
    assert_eq!(community_groups(&g, &options).unwrap(), groups);
    assert!(community_groups(&g, &LayoutOptions::default())
        .unwrap()
        .is_empty());
}
