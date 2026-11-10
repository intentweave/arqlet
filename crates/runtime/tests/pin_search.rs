//! The placement search keeps free nodes clear of a pinned leaf: a candidate with a geometry issue (overlap or a
//! violated minimum gap) is never accepted, so a pin is never crowded by a move.

use std::collections::BTreeSet;

use arqlet_model::{Graph, LayoutOptions, Point};

const CONTAINER_GRAPH: &str =
    include_str!("../../../examples/fixtures/reference/container-boundary.graph.json");

#[test]
fn the_placement_search_never_crowds_a_pin() {
    let graph: Graph = serde_json::from_str(CONTAINER_GRAPH).unwrap();
    let mut options = LayoutOptions::default();
    options.pins.insert("x".into(), Point { x: 20.0, y: 40.0 });
    let snapshot = arqlet_layout::compute(&graph, None, &BTreeSet::new(), &options).unwrap();
    assert!(
        snapshot.geometry_issues.is_empty(),
        "{:?}",
        snapshot.geometry_issues
    );
    let searched = arqlet_layout::search_placement(&snapshot, 200).unwrap();
    let boxes = |s: &arqlet_model::Snapshot| -> Vec<(f64, f64)> {
        s.boxes.values().map(|r| (r.x, r.y)).collect()
    };
    assert_ne!(
        boxes(&searched),
        boxes(&snapshot),
        "the search must move something for this test to mean anything"
    );
    let pinned = searched.boxes["x"].clone();
    assert_eq!((pinned.x, pinned.y), (20.0, 40.0));
    let involving_pin: Vec<_> = searched
        .geometry_issues
        .iter()
        .filter(|issue| issue.node_ids.iter().any(|id| id == "x"))
        .collect();
    assert!(involving_pin.is_empty(), "{involving_pin:?}");
}
