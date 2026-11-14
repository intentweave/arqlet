//! `organicForces` (forces in the manner of d3-force after the stress, on by default) and the experimental `repairCompaction`
//! (the nodes the repair pushed out are pulled back): smaller organic drawings with valid geometry and no more crossings or
//! bends than without.
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

/// The drawing with `options` against the one with `base`: smaller by the factor, valid, no more crossings or bends.
fn check(dataset: &str, base: Value, options: Value, smaller: f64) {
    let graph: Value = serde_json::from_str(
        &std::fs::read_to_string(format!(
            "{}/../../benchmarks/datasets/{dataset}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap();
    let plain = import(&graph, base);
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
}

#[test]
fn forces_draw_the_organic_examples_smaller() {
    // petri-net-shapes: 1614k to 946k; mixed-layouts: 5 crossings to 0, 1330k to 991k (measured 2026-11-14).
    let off = json!({"organicForces": false});
    check("petri-net-shapes", off.clone(), json!({}), 0.7);
    check("mixed-layouts", off, json!({}), 0.8);
}

#[test]
fn forces_take_the_ends_inside_containers_and_room_for_loops() {
    // self-loops: an edge into a container pulls toward the node it ends at inside (not the container's centre), and the
    // loops take a corner where no neighbour lies; 255k to 245k with the same bends (measured 2026-11-14).
    check(
        "self-loops",
        json!({"organicForces": false}),
        json!({}),
        1.0,
    );
}

#[test]
fn the_compaction_pulls_back_what_the_repair_pushed_out() {
    // petri-net-shapes: 1614k to 1247k (measured 2026-11-14).
    let off = json!({"organicForces": false});
    check(
        "petri-net-shapes",
        off,
        json!({"organicForces": false, "repairCompaction": true}),
        0.85,
    );
}

#[test]
fn the_forces_are_on_by_default_and_written_only_when_off() {
    assert!(arqlet_model::LayoutOptions::default().organic_forces);
    let off: arqlet_model::LayoutOptions =
        serde_json::from_value(json!({"organicForces": false})).unwrap();
    assert!(!off.organic_forces);
    assert!(serde_json::to_string(&off)
        .unwrap()
        .contains("\"organicForces\":false"));
    assert!(
        !serde_json::to_string(&arqlet_model::LayoutOptions::default())
            .unwrap()
            .contains("organicForces")
    );
}
