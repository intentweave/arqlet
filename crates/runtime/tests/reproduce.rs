//! Constrained reproduction through the dispatcher: a curated reference's pins are honoured by a fresh layout.

use arqlet_runtime::dispatch;
use serde_json::{json, Value};

const CONTAINER_GRAPH: &str =
    include_str!("../../../examples/fixtures/reference/container-boundary.graph.json");

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

/// A reference whose input carries one curator pin on leaf `x`.
fn pinned_reference() -> Value {
    let graph: Value = serde_json::from_str(CONTAINER_GRAPH).unwrap();
    let imported = run(json!({"op": "import", "graph": graph, "limit": 40}));
    assert_eq!(imported["ok"], true, "{imported}");
    let snapshot = imported["snapshot"].clone();
    let configured = run(json!({
        "op": "configure",
        "snapshot": snapshot,
        "expectedRevision": snapshot["revision"],
        "expectedLayoutVersion": snapshot["layoutVersion"].as_u64().unwrap_or(0),
        "options": {"pins": {"x": {"x": 20.0, "y": 40.0}}},
    }));
    assert_eq!(configured["ok"], true, "{configured}");
    let created = run(json!({
        "op": "referenceCreate",
        "snapshot": configured["snapshot"],
        "referenceId": "pinned",
    }));
    assert_eq!(created["ok"], true, "{created}");
    created["reference"].clone()
}

#[test]
fn the_pin_of_the_reference_is_honoured_by_the_reproduction() {
    let reproduced = run(json!({"op": "referenceReproduce", "reference": pinned_reference()}));
    assert_eq!(reproduced["ok"], true, "{reproduced}");
    assert_eq!(reproduced["variant"], "constrained");
    assert_eq!(reproduced["pins"]["requested"], 1);
    assert_eq!(reproduced["pins"]["honoured"], json!(["x"]));
    assert_eq!(reproduced["pins"]["broken"], json!([]));
}

#[test]
fn the_reproduction_reports_the_input_hash_and_both_measurements() {
    let reference = pinned_reference();
    let reproduced = run(json!({"op": "referenceReproduce", "reference": reference}));
    assert_eq!(reproduced["inputHash"], reference["inputHash"]);
    assert!(reproduced["curated"]["metrics"].is_object(), "{reproduced}");
    assert!(
        reproduced["candidate"]["metrics"].is_object(),
        "{reproduced}"
    );
    assert_eq!(
        reproduced["candidateReference"]["referenceId"],
        "pinned-candidate"
    );
}

#[test]
fn a_tampered_reference_is_an_error_not_a_reproduction() {
    let mut reference = pinned_reference();
    reference["input"]["options"]["pins"]["x"]["x"] = json!(99.0);
    let reproduced = run(json!({"op": "referenceReproduce", "reference": reference}));
    assert_eq!(reproduced["ok"], false, "{reproduced}");
}
