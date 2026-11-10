//! Moved label positions and curation notes are part of the reference document. They are validated and resealed by the
//! engine; neither changes the geometry of the routes or the nodes.

use arqlet_runtime::dispatch;
use serde_json::{json, Value};

const CONTAINER_GRAPH: &str =
    include_str!("../../../examples/fixtures/reference/container-boundary.graph.json");

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

/// A reference whose baseline has one edge label, so there is a label to move.
fn labelled() -> Value {
    let graph: Value = serde_json::from_str(CONTAINER_GRAPH).unwrap();
    let imported = run(json!({"op": "import", "graph": graph, "limit": 40}));
    assert_eq!(imported["ok"], true, "{imported}");
    let snapshot = imported["snapshot"].clone();
    let edge = graph["edges"][0]["id"].clone();
    let configured = run(json!({
        "op": "configure",
        "snapshot": snapshot,
        "expectedRevision": snapshot["revision"],
        "expectedLayoutVersion": snapshot["layoutVersion"].as_u64().unwrap_or(0),
        "options": {"edgeLabels": {edge.as_str().unwrap(): {"text": "calls", "width": 48, "height": 22}}},
    }));
    assert_eq!(configured["ok"], true, "{configured}");
    let created = run(json!({
        "op": "referenceCreate",
        "snapshot": configured["snapshot"],
        "referenceId": "labels",
    }));
    assert_eq!(created["ok"], true, "{created}");
    created["reference"].clone()
}

fn edited(mut reference: Value, edit: impl FnOnce(&mut Value)) -> Value {
    edit(&mut reference);
    run(json!({"op": "referenceRefresh", "reference": reference}))
}

#[test]
fn a_moved_label_is_saved_and_leaves_the_geometry_alone() {
    let reference = labelled();
    let label = reference["baseline"]["labels"][0]["id"].clone();
    assert!(
        label.is_string(),
        "the fixture has an edge label: {reference}"
    );
    let nodes_before = reference["curated"]["nodes"].clone();
    let moved = edited(reference, |doc| {
        doc["curated"]["labels"] = json!({label.as_str().unwrap(): {"x": 9.0, "y": 9.0}});
    });
    assert_eq!(moved["ok"], true, "{moved}");
    assert_eq!(moved["reference"]["curated"]["nodes"], nodes_before);
    assert_eq!(
        moved["reference"]["curated"]["labels"][label.as_str().unwrap()]["x"],
        9.0
    );
}

#[test]
fn a_label_position_for_an_unknown_label_is_refused() {
    let refused = edited(labelled(), |doc| {
        doc["curated"]["labels"] = json!({"no-such-label": {"x": 1.0, "y": 1.0}});
    });
    assert_eq!(refused["ok"], false);
    assert!(
        refused["error"].as_str().unwrap().contains("no-such-label"),
        "{refused}"
    );
}

#[test]
fn a_note_must_have_text_and_a_known_target() {
    let reference = labelled();
    let node = reference["curated"]["nodes"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let noted = edited(reference.clone(), |doc| {
        doc["annotations"] = json!([{"id": "n1", "target": node, "text": "check the port"}]);
    });
    assert_eq!(noted["ok"], true, "{noted}");
    let empty = edited(reference.clone(), |doc| {
        doc["annotations"] = json!([{"id": "n1", "target": node, "text": "   "}]);
    });
    assert_eq!(empty["ok"], false);
    assert!(
        empty["error"].as_str().unwrap().contains("no text"),
        "{empty}"
    );
    let stray = edited(reference, |doc| {
        doc["annotations"] = json!([{"id": "n1", "target": "nowhere", "text": "x"}]);
    });
    assert_eq!(stray["ok"], false);
    assert!(
        stray["error"].as_str().unwrap().contains("nowhere"),
        "{stray}"
    );
}

#[test]
fn a_note_may_name_a_category_and_a_severity_but_not_an_unknown_one() {
    let reference = labelled();
    let node = reference["curated"]["nodes"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let categorised = edited(reference.clone(), |doc| {
        doc["annotations"] = json!([{"id": "n1", "target": node, "text": "port too close", "category": "port-side-or-order", "severity": "warning"}]);
    });
    assert_eq!(categorised["ok"], true, "{categorised}");
    assert_eq!(
        categorised["reference"]["annotations"][0]["category"],
        "port-side-or-order"
    );
    let unknown = edited(reference, |doc| {
        doc["annotations"] =
            json!([{"id": "n1", "target": node, "text": "x", "category": "vibes"}]);
    });
    assert_eq!(unknown["ok"], false);
    assert!(
        unknown["error"].as_str().unwrap().contains("vibes"),
        "{unknown}"
    );
}
