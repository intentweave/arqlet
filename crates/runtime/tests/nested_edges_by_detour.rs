//! Experimental `nestedEdgesByDetour`: with `straightDetours`, the repair of an organic area does not move nodes to clear
//! straight edges whose ends lie inside its containers; their detours draw them around the nodes in their way.
use arqlet_model::Snapshot;
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn import(dataset: &str, options: Value) -> Snapshot {
    let graph: Value = serde_json::from_str(dataset).unwrap();
    let limit = graph["nodes"].as_array().unwrap().len();
    let r: Value = serde_json::from_str(&dispatch(
        &json!({"op": "import", "graph": graph, "limit": limit, "options": options}).to_string(),
    ))
    .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    serde_json::from_value(r["snapshot"].clone()).unwrap()
}

#[test]
fn nested_lines_are_left_to_their_detours_and_flat_areas_keep_their_repair() {
    // An organic root over layered containers: no line through a node, valid geometry.
    let large = import(
        include_str!("../../../benchmarks/datasets/mixed-large.json"),
        json!({"nestedEdgesByDetour": true, "straightDetours": true}),
    );
    let m = arqlet_layout::published_metrics(&large).unwrap();
    assert_eq!(m.line_node_hits, 0);
    let issues = arqlet_scene::validate_geometry(&large.graph, &large.boxes, Default::default());
    assert!(issues.is_empty(), "{issues:?}");
    assert!(large.options.nested_edges_by_detour);
    // A flat organic area (no containers): the repair still clears its lines, so the drawing is the same as without.
    let flat = include_str!("../../../benchmarks/datasets/petri-net-shapes.json");
    let on = import(
        flat,
        json!({"nestedEdgesByDetour": true, "straightDetours": true}),
    );
    let off = import(flat, json!({"straightDetours": true}));
    assert_eq!(
        serde_json::to_value(&on.boxes).unwrap(),
        serde_json::to_value(&off.boxes).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&on.routes).unwrap(),
        serde_json::to_value(&off.routes).unwrap()
    );
    // Off by default and not written.
    assert!(!serde_json::to_string(&off.options)
        .unwrap()
        .contains("nestedEdgesByDetour"));
}
