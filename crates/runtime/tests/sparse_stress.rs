//! P2.6 options cross the same native/WASM dispatch boundary as the other experimental organic options.
use arqlet_model::Graph;
use arqlet_runtime::{configure_snapshot, import_graph};
use serde_json::json;

#[test]
fn sparse_stress_defaults_off_and_round_trips_through_a_snapshot() {
    let defaults = serde_json::to_value(arqlet_model::LayoutOptions::default()).unwrap();
    assert!(defaults.get("sparseStress").is_none());
    let graph: Graph = serde_json::from_value(json!({
        "defaultStrategy": "organic", "defaultDirection": "auto", "defaultRouting": "straight",
        "nodes": [{"id":"a", "kind":"node", "label":"A"}, {"id":"b", "kind":"node", "label":"B"}, {"id":"c", "kind":"node", "label":"C"}],
        "edges": [{"id":"ab", "kind":"flow", "source":"a", "target":"b"}]
    }))
    .unwrap();
    let snapshot = import_graph(graph, 20).unwrap();
    let options = json!({"sparseStress": true, "pins": {"a": {"x": 24., "y": 48.}}});
    let result = configure_snapshot(
        &snapshot,
        snapshot.revision,
        snapshot.layout_version,
        &options,
    )
    .unwrap();
    assert!(result.options.sparse_stress);
    assert_eq!(result.boxes["a"].x, 24.);
    assert_eq!(result.boxes["a"].y, 48.);
    let encoded = serde_json::to_string(&result).unwrap();
    let decoded: arqlet_model::Snapshot = serde_json::from_str(&encoded).unwrap();
    assert!(decoded.options.sparse_stress);
}

#[test]
fn scalable_stress_defaults_off_and_round_trips_through_a_snapshot() {
    let defaults = serde_json::to_value(arqlet_model::LayoutOptions::default()).unwrap();
    assert!(defaults.get("scalableStress").is_none());
    let graph: Graph = serde_json::from_value(json!({
        "defaultStrategy": "organic", "defaultDirection": "auto", "defaultRouting": "straight",
        "nodes": [{"id":"a", "kind":"node", "label":"A"}, {"id":"b", "kind":"node", "label":"B"}, {"id":"c", "kind":"node", "label":"C"}],
        "edges": [{"id":"ab", "kind":"flow", "source":"a", "target":"b"}, {"id":"bc", "kind":"flow", "source":"b", "target":"c"}]
    }))
    .unwrap();
    let snapshot = import_graph(graph, 20).unwrap();
    let options = json!({"scalableStress": true, "pins": {"a": {"x": 24., "y": 48.}}});
    let result = configure_snapshot(
        &snapshot,
        snapshot.revision,
        snapshot.layout_version,
        &options,
    )
    .unwrap();
    assert!(result.options.scalable_stress);
    assert_eq!(result.boxes["a"].x, 24.);
    assert_eq!(result.boxes["a"].y, 48.);
    let encoded = serde_json::to_string(&result).unwrap();
    let decoded: arqlet_model::Snapshot = serde_json::from_str(&encoded).unwrap();
    assert!(decoded.options.scalable_stress);
}
