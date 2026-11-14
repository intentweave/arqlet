//! Experimental `channelsByDemand`: every channel between two ranks is as wide as its tracks need, narrower than the layer
//! gap where it carries few tracks, with the order and the places across the flow as before.
use arqlet_model::Snapshot;
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn import(graph: &Value, options: Value) -> Snapshot {
    let limit = graph["nodes"].as_array().unwrap().len();
    let r: Value = serde_json::from_str(&dispatch(
        &json!({"op": "import", "graph": graph, "limit": limit, "options": options}).to_string(),
    ))
    .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    serde_json::from_value(r["snapshot"].clone()).unwrap()
}

#[test]
fn channels_with_few_tracks_narrow_and_the_drawing_stays_valid() {
    // Every area in the root's direction: the option narrows areas with a fixed flow along their parent's axis.
    let mut graph: Value = serde_json::from_str(include_str!(
        "../../../benchmarks/datasets/arqlet-pipeline.json"
    ))
    .unwrap();
    let direction = graph["defaultDirection"].clone();
    if let Some(map) = graph["containerDirections"].as_object_mut() {
        for value in map.values_mut() {
            *value = direction.clone();
        }
    }
    let plain = import(&graph, json!({"channelsByDemand": false}));
    let narrow = import(&graph, json!({}));
    let (a, b) = (
        arqlet_layout::published_metrics(&plain).unwrap(),
        arqlet_layout::published_metrics(&narrow).unwrap(),
    );
    assert!(
        b.area < a.area * 0.9,
        "smaller: {} against {}",
        b.area,
        a.area
    );
    assert_eq!(b.line_node_hits, 0);
    assert_eq!(
        (b.crossings, b.bends),
        (a.crossings, a.bends),
        "{b:?} against {a:?}"
    );
    let issues = arqlet_scene::validate_geometry(&narrow.graph, &narrow.boxes, Default::default());
    assert!(issues.is_empty(), "{issues:?}");
    assert!(narrow.route_conflicts.is_empty());
    // The order across the flow is the same: every node keeps its place across the flow relative to its rank neighbours.
    assert_eq!(plain.routes.len(), narrow.routes.len());
    // On by default, written only when turned off.
    assert!(narrow.options.channels_by_demand);
    assert!(!serde_json::to_string(&narrow.options)
        .unwrap()
        .contains("channelsByDemand"));
}
