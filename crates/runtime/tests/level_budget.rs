//! Provisional (scaling step 2): a first view within a budget of nodes and edges (`LevelOfDetail::nodes`, `edges`).
use arqlet_model::{Graph, LayoutOptions};
use arqlet_runtime::import_graph_with;
use serde_json::json;
use std::collections::BTreeSet;

/// Four groups of six leaves, a ring of edges in each group and one edge from every group to the next.
fn grouped() -> Graph {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for g in 0..4 {
        nodes.push(json!({"id": format!("g{g}"), "kind": "group", "label": format!("g{g}")}));
        for i in 0..6 {
            nodes.push(json!({"id": format!("n{g}-{i}"), "kind": "node", "label": "n", "parentId": format!("g{g}")}));
            edges.push(json!({"id": format!("r{g}-{i}"), "kind": "flow", "source": format!("n{g}-{i}"), "target": format!("n{g}-{}", (i + 1) % 6)}));
        }
        edges.push(json!({"id": format!("x{g}"), "kind": "flow", "source": format!("n{g}-0"), "target": format!("n{}-0", (g + 1) % 4)}));
    }
    serde_json::from_value(json!({"defaultDirection": "lr", "nodes": nodes, "edges": edges}))
        .unwrap()
}

fn options(level: serde_json::Value) -> LayoutOptions {
    serde_json::from_value(json!({"levelOfDetail": level})).unwrap()
}

fn units(snapshot: &arqlet_model::Snapshot) -> usize {
    let frames: BTreeSet<&str> = snapshot
        .graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    snapshot
        .graph
        .nodes
        .iter()
        .filter(|n| !frames.contains(n.id.as_str()))
        .count()
}

#[test]
fn the_first_view_stays_within_its_node_and_edge_budget() {
    // Closed: 4 groups and the 4 edges between them. Each group opened adds 5 nodes and 6 ring edges.
    let closed = import_graph_with(grouped(), 100, &options(json!({"nodes": 4}))).unwrap();
    assert_eq!(units(&closed), 4);
    assert_eq!(closed.collapsed_ids.len(), 4);
    let two = import_graph_with(grouped(), 100, &options(json!({"nodes": 14}))).unwrap();
    assert_eq!(units(&two), 14);
    assert_eq!(two.collapsed_ids, ["g2", "g3"]);
    // The edge budget: one group open draws 4 + 6 edges, two draw 16 or more.
    let edges =
        import_graph_with(grouped(), 100, &options(json!({"nodes": 100, "edges": 12}))).unwrap();
    assert_eq!(edges.collapsed_ids, ["g1", "g2", "g3"]);
    assert!(edges.graph.edges.len() <= 12);
    // Every edge keeps its logical direction: the view's edges are the source's, ends moved to collapsed containers.
    let source = edges.source_graph.as_ref().unwrap();
    for e in &edges.graph.edges {
        let original = source.edges.iter().find(|s| s.id == e.id).unwrap();
        let shown = |id: &str| {
            if id.starts_with("n1") || id.starts_with("n2") || id.starts_with("n3") {
                format!("g{}", &id[1..2])
            } else {
                id.to_string()
            }
        };
        assert_eq!(
            (
                shown(&original.source),
                shown(original.target.as_deref().unwrap())
            ),
            (e.source.clone(), e.target.clone().unwrap())
        );
    }
}

#[test]
fn a_node_budget_divides_a_flat_graph_into_communities() {
    // 40 leaves in four rings of ten, without containers: the hierarchy alone cannot keep a view of 12.
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for g in 0..4 {
        for i in 0..10 {
            nodes.push(json!({"id": format!("v{g}-{i}"), "kind": "node", "label": "v"}));
            edges.push(json!({"id": format!("e{g}-{i}"), "kind": "flow", "source": format!("v{g}-{i}"), "target": format!("v{g}-{}", (i + 1) % 10)}));
        }
    }
    let graph: Graph =
        serde_json::from_value(json!({"defaultDirection": "lr", "nodes": nodes, "edges": edges}))
            .unwrap();
    let snapshot = import_graph_with(graph, 100, &options(json!({"nodes": 12}))).unwrap();
    assert_eq!(
        snapshot.options.communities, 12,
        "the budget gave the communities"
    );
    assert!(units(&snapshot) <= 12, "{} units", units(&snapshot));
    assert!(!snapshot.collapsed_ids.is_empty());
    // Given communities are kept.
    let own: LayoutOptions =
        serde_json::from_value(json!({"levelOfDetail": {"nodes": 12}, "communities": 30})).unwrap();
    let graph = snapshot.source_graph.clone().unwrap();
    assert_eq!(
        import_graph_with(graph, 100, &own)
            .unwrap()
            .options
            .communities,
        30
    );
}

#[test]
fn the_budget_fields_are_omitted_when_unset() {
    let level = serde_json::to_value(arqlet_model::LevelOfDetail {
        depth: Some(1),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(level, json!({"depth": 1}));
}
