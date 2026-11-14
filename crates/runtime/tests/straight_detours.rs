//! Experimental `straightDetours` (ADR 0020, S1-1b): a straight edge of an organic area that would pass through a node is
//! drawn around it. On mixed-large (an organic root holding layered containers) the straight edges between nodes nested in
//! different containers ran through their siblings; with the option no route enters a node it does not end at.
use arqlet_model::Snapshot;
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn import(options: Value) -> Snapshot {
    let graph: Value = serde_json::from_str(include_str!(
        "../../../benchmarks/datasets/mixed-large.json"
    ))
    .unwrap();
    let limit = graph["nodes"].as_array().unwrap().len();
    let r: Value = serde_json::from_str(&dispatch(
        &json!({"op": "import", "graph": graph, "limit": limit, "options": options}).to_string(),
    ))
    .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    serde_json::from_value(r["snapshot"].clone()).unwrap()
}

fn straight_through(s: &Snapshot) -> usize {
    s.geometry_issues
        .iter()
        .filter(|i| i.kind == "straight-through-node")
        .count()
}

#[test]
fn straight_edges_of_an_organic_root_go_around_the_nodes_in_their_way() {
    let plain = import(json!({}));
    let detours = import(json!({"straightDetours": true}));
    // Without the option: straight edges through nodes, reported.
    let before = arqlet_layout::published_metrics(&plain).unwrap();
    assert!(before.line_node_hits > 0 && straight_through(&plain) > 0);
    // With it: the same placement, no line through a node, nothing reported.
    assert_eq!(
        serde_json::to_value(&plain.boxes).unwrap(),
        serde_json::to_value(&detours.boxes).unwrap(),
        "the placement does not change"
    );
    let after = arqlet_layout::published_metrics(&detours).unwrap();
    assert_eq!(after.line_node_hits, 0);
    assert_eq!(straight_through(&detours), 0);
    assert!(!detours
        .routing_issues
        .iter()
        .any(|i| i.starts_with("straight edge")));
    // Every edge is routed from its source to its target; edges with a free line stay one segment.
    let abs = arqlet_layout::absolute_all(&detours.graph, &detours.boxes);
    let near = |p: &arqlet_model::Point, r: &arqlet_model::Rect| {
        p.x >= r.x - 0.5
            && p.x <= r.x + r.width + 0.5
            && p.y >= r.y - 0.5
            && p.y <= r.y + r.height + 0.5
    };
    let mut straight_segments = 0;
    for id in &detours.straight_edges {
        let e = detours.graph.edges.iter().find(|e| &e.id == id).unwrap();
        let route = detours.routes.iter().find(|r| &r.id == id).unwrap();
        assert!(
            near(&route.points[0], &abs[&e.source]),
            "{id} leaves its source"
        );
        assert!(
            near(
                route.points.last().unwrap(),
                &abs[e.target.as_ref().unwrap()]
            ),
            "{id} reaches its target"
        );
        if route.points.len() == 2 {
            straight_segments += 1;
        }
    }
    assert!(
        straight_segments > 0,
        "edges with a free line stay straight"
    );
    assert!(detours.options.straight_detours);
    assert!(!serde_json::to_string(&plain.options)
        .unwrap()
        .contains("straightDetours"));
}
