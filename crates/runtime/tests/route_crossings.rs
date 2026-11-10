//! The crossing step runs at Optimize time: the import keeps its drawing, the refinement separates the crossing routes of
//! the state machine example.
use arqlet_model::LayoutOptions;
use arqlet_runtime::{import_graph_with, refine_snapshot_job};

fn crossings(snapshot: &arqlet_model::Snapshot) -> usize {
    let routes = &snapshot.routes;
    let mut count = 0;
    for (i, a) in routes.iter().enumerate() {
        for b in &routes[i + 1..] {
            if arqlet_routing::crosses(a, b) {
                count += 1;
            }
        }
    }
    count
}

#[test]
fn optimize_removes_the_state_machine_crossing_and_import_keeps_its_drawing() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/state-order.json"
    ))
    .unwrap();
    let graph = serde_json::from_str(&text).unwrap();
    let imported = import_graph_with(
        graph,
        100,
        &LayoutOptions {
            layered_construction: false,
            hierarchical_ordering: false,
            ..LayoutOptions::default()
        },
    )
    .unwrap();
    assert!(crossings(&imported) > 0, "the import has the crossing");
    let (optimized, _) = refine_snapshot_job(
        "crossings",
        &imported,
        imported.revision,
        imported.layout_version,
        1,
        None,
        None,
    )
    .unwrap();
    assert_eq!(crossings(&optimized), 0);
    assert!(optimized
        .routing_issues
        .iter()
        .all(|i| !i.contains("passes through")));
}
