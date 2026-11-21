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
        nodes.push(json!({"id": format!("g{g}"), "kind": "team", "label": format!("g{g}")}));
        for i in 0..6 {
            nodes.push(json!({"id": format!("n{g}-{i}"), "kind": "node", "label": "n", "parentId": format!("g{g}")}));
            edges.push(json!({"id": format!("r{g}-{i}"), "kind": "flow", "source": format!("n{g}-{i}"), "target": format!("n{g}-{}", (i + 1) % 6)}));
        }
        edges.push(json!({"id": format!("x{g}"), "kind": "flow", "source": format!("n{g}-0"), "target": format!("n{}-0", (g + 1) % 4)}));
    }
    serde_json::from_value(json!({"defaultDirection": "lr", "nodes": nodes, "edges": edges}))
        .unwrap()
}

/// The first view's budget; edges as they are (not lifted, as they are by default with `levelOfDetail`), so that the tests
/// see each edge's ends.
fn options(level: serde_json::Value) -> LayoutOptions {
    serde_json::from_value(json!({"levelOfDetail": level, "liftEdges": false})).unwrap()
}

/// The boxes of the first view: every node shown, an opened container's frame too (as the budget counts them).
fn boxes(snapshot: &arqlet_model::Snapshot) -> usize {
    snapshot.graph.nodes.len()
}

#[test]
fn the_first_view_stays_within_its_node_and_edge_budget() {
    // Closed: 4 groups and the 4 edges between them. Each group opened adds its 6 leaves (its frame stays a box) and 6
    // ring edges.
    let closed = import_graph_with(grouped(), 100, &options(json!({"nodes": 4}))).unwrap();
    assert_eq!(boxes(&closed), 4);
    assert_eq!(closed.collapsed_ids.len(), 4);
    let two = import_graph_with(grouped(), 100, &options(json!({"nodes": 16}))).unwrap();
    assert_eq!(boxes(&two), 16);
    assert_eq!(two.collapsed_ids, ["g2", "g3"]);
    // One box less than two groups need: one group opens.
    let one = import_graph_with(grouped(), 100, &options(json!({"nodes": 15}))).unwrap();
    assert_eq!(boxes(&one), 10);
    // The edge budget: one group open draws 4 + 6 edges, two draw 16 or more.
    let edges =
        import_graph_with(grouped(), 100, &options(json!({"nodes": 100, "edges": 12}))).unwrap();
    assert_eq!(edges.collapsed_ids, ["g1", "g2", "g3"]);
    assert!(edges.graph.edges.len() <= 12);
    // Every edge keeps its logical direction: the view's edges are the source's, ends moved to collapsed containers.
    let source = edges.source_graph.as_deref().unwrap();
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
    assert!(boxes(&snapshot) <= 12, "{} boxes", boxes(&snapshot));
    assert!(!snapshot.collapsed_ids.is_empty());
    // Given communities are kept.
    let own: LayoutOptions =
        serde_json::from_value(json!({"levelOfDetail": {"nodes": 12}, "communities": 30})).unwrap();
    let graph = (*snapshot.source_graph.clone().unwrap()).clone();
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

#[test]
fn lifted_edges_join_open_groups_once_and_stay_lifted_after_a_toggle() {
    let options: LayoutOptions = serde_json::from_value(json!({"liftEdges": true})).unwrap();
    let snapshot = import_graph_with(grouped(), 100, &options).unwrap();
    // Every group open: the rings stay, and the four edges between groups are drawn between the groups.
    assert!(snapshot.collapsed_ids.is_empty());
    let between: Vec<(String, String)> = snapshot
        .graph
        .edges
        .iter()
        .filter(|e| e.id.starts_with('x'))
        .map(|e| (e.source.clone(), e.target.clone().unwrap()))
        .collect();
    assert_eq!(
        between,
        [("g0", "g1"), ("g1", "g2"), ("g2", "g3"), ("g3", "g0")]
            .map(|(a, b)| (a.to_string(), b.to_string()))
    );
    assert_eq!(snapshot.graph.edges.len(), 4 * 6 + 4);
    assert!(snapshot.routes.iter().all(|r| r.points.len() >= 2));
    // Closing a group keeps the others lifted.
    let closed = arqlet_runtime::toggle_container(
        &snapshot,
        snapshot.revision,
        snapshot.layout_version,
        "g1",
    )
    .unwrap();
    assert_eq!(closed.collapsed_ids, ["g1"]);
    assert_eq!(
        closed
            .graph
            .edges
            .iter()
            .filter(|e| e.id.starts_with('x'))
            .count(),
        4
    );
    assert_eq!(closed.graph.edges.len(), 3 * 6 + 4);
}

#[test]
fn a_budget_counts_lifted_edges() {
    // Lifted, a group open adds only its ring: 4 + 6 per open group. Budget 16: one group open... unlifted it would be 10.
    let lifted: LayoutOptions =
        serde_json::from_value(json!({"liftEdges": true, "levelOfDetail": {"edges": 16}})).unwrap();
    let snapshot = import_graph_with(grouped(), 100, &lifted).unwrap();
    assert_eq!(snapshot.collapsed_ids, ["g2", "g3"]);
    assert_eq!(snapshot.graph.edges.len(), 16);
}

#[test]
fn lift_edges_is_on_by_default_in_views_that_start_abstract() {
    // With `levelOfDetail` (or communities) and no `liftEdges`, edges between opened groups are lifted; `false` turns it
    // off; without an abstract start nothing is lifted.
    let opts = |v: serde_json::Value| -> LayoutOptions { serde_json::from_value(v).unwrap() };
    assert!(opts(json!({"levelOfDetail": {"depth": 0}})).lifts_edges());
    assert!(opts(json!({"communities": 30})).lifts_edges());
    assert!(!opts(json!({"levelOfDetail": {"depth": 0}, "liftEdges": false})).lifts_edges());
    assert!(!opts(json!({})).lifts_edges());
    assert!(opts(json!({"liftEdges": true})).lifts_edges());
    let lifted = import_graph_with(
        grouped(),
        100,
        &opts(json!({"levelOfDetail": {"nodes": 100}})),
    )
    .unwrap();
    let plain = import_graph_with(
        grouped(),
        100,
        &opts(json!({"levelOfDetail": {"nodes": 100}, "liftEdges": false})),
    )
    .unwrap();
    // The edge between two open groups ends at the groups when lifted, at its leaves when not.
    let ends = |s: &arqlet_model::Snapshot| {
        let x0 = s.graph.edges.iter().find(|e| e.id == "x0").unwrap();
        (x0.source.clone(), x0.target.clone().unwrap())
    };
    assert_eq!(ends(&lifted), ("g0".to_string(), "g1".to_string()));
    assert_eq!(ends(&plain), ("n0-0".to_string(), "n1-0".to_string()));
    // The default is not written into the snapshot.
    assert!(!serde_json::to_string(&lifted.options)
        .unwrap()
        .contains("liftEdges"));
}

#[test]
fn a_first_view_over_its_budget_says_by_how_much_and_why() {
    // Ten top-level nodes and no containers: a budget of 4 cannot hold the top level.
    let nodes: Vec<serde_json::Value> = (0..10)
        .map(|i| json!({"id": format!("n{i}"), "kind": "k", "label": "n"}))
        .collect();
    let g: Graph = serde_json::from_value(json!({"nodes": nodes, "edges": []})).unwrap();
    let opts: LayoutOptions =
        serde_json::from_value(json!({"levelOfDetail": {"nodes": 4}, "communities": 0})).unwrap();
    let s = import_graph_with(g, 100, &opts).unwrap();
    assert_eq!(s.graph.nodes.len(), 10);
    {
        assert!(
            s.layout_diagnostics
                .iter()
                .any(|d| d.starts_with("First view:") && d.contains("over the node budget of 4")),
            "{:?}",
            s.layout_diagnostics
        );
    }
    // Within its budget nothing is said.
    let within = import_graph_with(grouped(), 100, &options(json!({"nodes": 16}))).unwrap();
    assert!(!within
        .layout_diagnostics
        .iter()
        .any(|d| d.starts_with("First view:")));
}
