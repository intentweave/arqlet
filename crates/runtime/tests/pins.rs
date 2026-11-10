//! Node pins: stored and validated as layout options and honoured by placement, for leaves and containers.

use arqlet_runtime::dispatch;
use serde_json::{json, Value};

const CONTAINER_GRAPH: &str =
    include_str!("../../../examples/fixtures/reference/container-boundary.graph.json");

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn imported() -> Value {
    let graph: Value = serde_json::from_str(CONTAINER_GRAPH).unwrap();
    let response = run(json!({"op": "import", "graph": graph, "limit": 40}));
    assert_eq!(response["ok"], true, "{response}");
    response["snapshot"].clone()
}

fn configure(snapshot: &Value, options: Value) -> Value {
    run(json!({
        "op": "configure",
        "snapshot": snapshot,
        "expectedRevision": snapshot["revision"],
        "expectedLayoutVersion": snapshot["layoutVersion"].as_u64().unwrap_or(0),
        "options": options,
    }))
}

#[test]
fn default_options_do_not_serialize_pins() {
    let response = run(json!({"op": "optionDefaults"}));
    assert!(response["options"].get("pins").is_none(), "{response}");
}

#[test]
fn a_pin_on_a_leaf_node_is_accepted_and_stored() {
    let snapshot = imported();
    let response = configure(&snapshot, json!({"pins": {"x": {"x": 10.0, "y": 20.0}}}));
    assert_eq!(response["ok"], true, "{response}");
    assert_eq!(response["snapshot"]["options"]["pins"]["x"]["x"], 10.0);
    assert!(
        response["snapshot"]["layoutDiagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|message| message.as_str().unwrap().contains("pinned leaf placement")),
        "{response}"
    );
}

#[test]
fn a_pin_on_a_container_keeps_it_in_place_through_a_direction_change_and_refinement() {
    let snapshot = imported();
    let configured = configure(
        &snapshot,
        json!({"pins": {"p": {"x": 300.0, "y": 200.0}, "q": {"x": 30.0, "y": 260.0}}}),
    );
    assert_eq!(configured["ok"], true, "{configured}");
    let pinned = &configured["snapshot"];
    assert_eq!(
        (
            pinned["boxes"]["p"]["x"].as_f64(),
            pinned["boxes"]["p"]["y"].as_f64()
        ),
        (Some(300.0), Some(200.0))
    );
    let turned = run(json!({
        "op": "direction",
        "snapshot": pinned,
        "expectedRevision": pinned["revision"],
        "expectedLayoutVersion": pinned["layoutVersion"],
        "nodeId": null,
        "direction": "tb",
    }));
    assert_eq!(turned["ok"], true, "{turned}");
    let turned = &turned["snapshot"];
    assert_eq!(
        (
            turned["boxes"]["p"]["x"].as_f64(),
            turned["boxes"]["p"]["y"].as_f64()
        ),
        (Some(300.0), Some(200.0))
    );
    let refined = run(json!({
        "op": "refine",
        "snapshot": turned,
        "expectedRevision": turned["revision"],
        "expectedLayoutVersion": turned["layoutVersion"],
    }));
    assert_eq!(refined["ok"], true, "{refined}");
    let refined = &refined["snapshot"];
    assert_eq!(
        (
            refined["boxes"]["p"]["x"].as_f64(),
            refined["boxes"]["p"]["y"].as_f64()
        ),
        (Some(300.0), Some(200.0)),
        "refinement keeps the pinned container where it is"
    );
    assert_eq!(
        (
            refined["boxes"]["q"]["x"].as_f64(),
            refined["boxes"]["q"]["y"].as_f64()
        ),
        (Some(30.0), Some(260.0)),
        "a nested container keeps its place in its parent's frame"
    );
    let unpinned = configure(refined, json!({"pins": {}}));
    assert_eq!(unpinned["ok"], true, "{unpinned}");
    assert!(unpinned["snapshot"]["options"]
        .get("pins")
        .map_or(true, |p| p.as_object().is_some_and(|m| m.is_empty())));
}

#[test]
fn a_pin_on_an_unknown_node_is_refused_explicitly() {
    let snapshot = imported();
    let response = configure(&snapshot, json!({"pins": {"nope": {"x": 0.0, "y": 0.0}}}));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"], "Pin names an unknown node: nope");
}

#[test]
fn a_pin_survives_a_direction_change_and_keeps_its_place_in_the_container() {
    let snapshot = imported();
    let configured = configure(&snapshot, json!({"pins": {"x": {"x": 20.0, "y": 40.0}}}));
    assert_eq!(configured["ok"], true, "{configured}");
    let turned = run(json!({
        "op": "direction",
        "snapshot": configured["snapshot"],
        "expectedRevision": configured["snapshot"]["revision"],
        "expectedLayoutVersion": configured["snapshot"]["layoutVersion"],
        "nodeId": null,
        "direction": "tb",
    }));
    assert_eq!(turned["ok"], true, "{turned}");
    let snapshot = &turned["snapshot"];
    assert_eq!(snapshot["options"]["pins"]["x"]["x"], 20.0);
    let child = &snapshot["boxes"]["x"];
    assert_eq!(
        child["x"], 20.0,
        "the pin is relative to the container, whatever the direction"
    );
    assert_eq!(child["y"], 40.0);
}

#[test]
fn pins_survive_a_reference_round_trip() {
    let snapshot = imported();
    let configured = configure(&snapshot, json!({"pins": {"x": {"x": 20.0, "y": 40.0}}}));
    assert_eq!(configured["ok"], true, "{configured}");
    let created = run(
        json!({"op": "referenceCreate", "snapshot": configured["snapshot"], "referenceId": "pinned"}),
    );
    assert_eq!(created["ok"], true, "{created}");
    let reference = &created["reference"];
    assert_eq!(reference["input"]["options"]["pins"]["x"]["x"], 20.0);
    let text = run(json!({"op": "referenceCanonical", "reference": reference}));
    assert_eq!(text["ok"], true, "{text}");
    let reopened: Value = serde_json::from_str(text["canonical"].as_str().unwrap()).unwrap();
    assert_eq!(reopened["input"]["options"]["pins"]["x"]["y"], 40.0);
    let validated = run(json!({"op": "referenceValidate", "reference": reopened}));
    assert_eq!(validated["ok"], true, "{validated}");
}
