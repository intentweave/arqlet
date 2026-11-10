//! P3 finding (er-diagram): a pinned entity can leave routes through nodes. The routing-engine choice routes the same boxes
//! with the other engine and keeps the drawing with fewer conflicts. These are the measured counts; they are a regression
//! guard, not the goal (the remaining conflicts need port and track ordering, see the plan).
use arqlet_runtime::{configure_snapshot, import_view};
use serde_json::json;

fn configured_at(x: f64, y: f64) -> arqlet_model::Snapshot {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/er-diagram.json"
    ))
    .unwrap();
    let imported = import_view(&text, 1000).unwrap();
    configure_snapshot(
        &imported,
        imported.revision,
        imported.layout_version,
        &json!({"pins": {"ent:Customer": {"x": x, "y": y}}}),
    )
    .unwrap()
}

#[test]
fn pinned_entities_keep_their_conflict_counts_at_most_at_the_measured_level() {
    // (x, y) of the pin, and the edge–node conflicts measured on 2026-10-30 with the side search (offsets and complete routes).
    for (x, y, measured) in [
        (700., 400., 2),
        (-120., 300., 1),
        (224., 88., 4),
        (700., 48., 4),
    ] {
        let snapshot = configured_at(x, y);
        assert!(
            snapshot.route_conflicts.len() <= measured,
            "pin at ({x}, {y}): {} conflicts, measured {measured}: {:?}",
            snapshot.route_conflicts.len(),
            snapshot.routing_issues
        );
    }
}

#[test]
fn the_engine_choice_keeps_the_canonical_graph_and_the_configured_options() {
    let snapshot = configured_at(-120., 300.);
    assert!(snapshot.graph.ports.is_empty() && snapshot.graph.edge_ports.is_empty());
    assert_eq!(
        snapshot.options.routing_engine,
        arqlet_model::RoutingEngine::Legacy
    );
}
