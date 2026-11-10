//! P2.5 options cross the same native/WASM dispatch boundary as existing layout options.
use arqlet_model::Graph;
use arqlet_runtime::{configure_snapshot, import_graph};
use serde_json::json;

#[test]
fn experimental_options_default_off_and_roundtrip_without_changing_pins() {
    let defaults = serde_json::to_value(arqlet_model::LayoutOptions::default()).unwrap();
    assert!(defaults.get("directedRepair").is_none());
    assert!(defaults.get("nodeEdgeRepulsion").is_none());
    let graph: Graph = serde_json::from_value(json!({
        "defaultStrategy": "organic", "defaultDirection": "auto", "defaultRouting": "straight",
        "nodes": [{"id":"a", "kind":"node", "label":"A"}, {"id":"b", "kind":"node", "label":"B"}, {"id":"c", "kind":"node", "label":"C"}],
        "edges": [{"id":"ab", "kind":"flow", "source":"a", "target":"b"}]
    }))
    .unwrap();
    let snapshot = import_graph(graph, 20).unwrap();
    let options = json!({"directedRepair": true, "nodeEdgeRepulsion": true,
        "pins": {"a": {"x": 24., "y": 48.}}});
    let result = configure_snapshot(
        &snapshot,
        snapshot.revision,
        snapshot.layout_version,
        &options,
    )
    .unwrap();
    assert!(result.options.directed_repair && result.options.node_edge_repulsion);
    assert_eq!(result.boxes["a"].x, 24.);
    assert_eq!(result.boxes["a"].y, 48.);
    let encoded = serde_json::to_string(&result).unwrap();
    let decoded: arqlet_model::Snapshot = serde_json::from_str(&encoded).unwrap();
    assert!(decoded.options.directed_repair && decoded.options.node_edge_repulsion);
}
