//! Experimental `organicForces` (forces in the manner of d3-force after the stress) and `repairCompaction` (the nodes the
//! repair pushed out are pulled back): smaller organic drawings with valid geometry and no more crossings or bends.
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

fn check(dataset: &str, options: Value, smaller: f64) {
    let graph: Value = serde_json::from_str(
        &std::fs::read_to_string(format!(
            "{}/../../benchmarks/datasets/{dataset}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap();
    let plain = import(&graph, json!({}));
    let variant = import(&graph, options.clone());
    let (a, b) = (
        arqlet_layout::published_metrics(&plain).unwrap(),
        arqlet_layout::published_metrics(&variant).unwrap(),
    );
    assert!(
        b.area < a.area * smaller,
        "{dataset} {options}: {} against {}",
        b.area,
        a.area
    );
    assert_eq!(b.line_node_hits, 0);
    assert!(b.crossings <= a.crossings, "{dataset}: {b:?} against {a:?}");
    assert!(b.bends <= a.bends, "{dataset}: {b:?} against {a:?}");
    let issues =
        arqlet_scene::validate_geometry(&variant.graph, &variant.boxes, Default::default());
    assert!(issues.is_empty(), "{issues:?}");
    // Off by default, written only when on.
    let text = serde_json::to_string(&variant.options).unwrap();
    for (key, _) in options.as_object().unwrap() {
        assert!(text.contains(key.as_str()));
        assert!(!serde_json::to_string(&plain.options)
            .unwrap()
            .contains(key.as_str()));
    }
}

#[test]
fn forces_draw_the_organic_examples_smaller() {
    // petri-net-shapes: 1614k to 946k; mixed-layouts: 5 crossings to 0, 1330k to 991k (measured 2026-11-14).
    check("petri-net-shapes", json!({"organicForces": true}), 0.7);
    check("mixed-layouts", json!({"organicForces": true}), 0.8);
}

#[test]
fn forces_take_the_ends_inside_containers_and_room_for_loops() {
    // self-loops: an edge into a container pulls toward the node it ends at inside (not the container's centre), and the
    // loops take a corner where no neighbour lies; 255k to 245k with the same bends (measured 2026-11-14).
    check("self-loops", json!({"organicForces": true}), 1.0);
}

#[test]
fn the_compaction_pulls_back_what_the_repair_pushed_out() {
    // petri-net-shapes: 1614k to 1247k (measured 2026-11-14).
    check("petri-net-shapes", json!({"repairCompaction": true}), 0.85);
}
