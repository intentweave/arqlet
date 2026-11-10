//! Route locks and size pins are part of the reference document: they are validated, and a document with them is
//! resealed by the engine like any other edit.

use arqlet_runtime::dispatch;
use serde_json::{json, Value};

const CONTAINER_GRAPH: &str =
    include_str!("../../../examples/fixtures/reference/container-boundary.graph.json");

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn created() -> Value {
    let graph: Value = serde_json::from_str(CONTAINER_GRAPH).unwrap();
    let imported = run(json!({"op": "import", "graph": graph, "limit": 40}));
    assert_eq!(imported["ok"], true, "{imported}");
    let created = run(json!({
        "op": "referenceCreate",
        "snapshot": imported["snapshot"],
        "referenceId": "locks",
    }));
    assert_eq!(created["ok"], true, "{created}");
    created["reference"].clone()
}

/// Edits the curated scene and reseals it through the engine, as the editor does after a change.
fn edited(mut reference: Value, edit: impl FnOnce(&mut Value)) -> Value {
    edit(&mut reference["curated"]);
    let refreshed = run(json!({"op": "referenceRefresh", "reference": reference}));
    refreshed
}

#[test]
fn a_locked_route_survives_a_refresh_and_is_saved() {
    let reference = created();
    let route = reference["curated"]["routes"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let locked = edited(reference, |curated| {
        curated["lockedRoutes"] = json!([route]);
    });
    assert_eq!(locked["ok"], true, "{locked}");
    assert_eq!(
        locked["reference"]["curated"]["lockedRoutes"],
        json!([route])
    );
    let validated = run(json!({"op": "referenceValidate", "reference": locked["reference"]}));
    assert_eq!(validated["ok"], true, "{validated}");
}

#[test]
fn a_lock_on_an_unknown_route_is_refused_explicitly() {
    let reference = created();
    let refused = edited(reference, |curated| {
        curated["lockedRoutes"] = json!(["no-such-route"]);
    });
    assert_eq!(refused["ok"], false);
    assert!(
        refused["error"].as_str().unwrap().contains("no-such-route"),
        "{refused}"
    );
}

#[test]
fn a_size_pin_must_agree_with_its_node() {
    let reference = created();
    let node = reference["curated"]["nodes"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let size = reference["curated"]["nodes"][&node].clone();
    let agreeing = edited(reference.clone(), |curated| {
        curated["sizePins"] = json!({&node: {"width": size["width"], "height": size["height"]}});
    });
    assert_eq!(agreeing["ok"], true, "{agreeing}");
    let disagreeing = edited(reference, |curated| {
        curated["sizePins"] = json!({&node: {"width": 1.0, "height": 1.0}});
    });
    assert_eq!(disagreeing["ok"], false);
    assert!(
        disagreeing["error"].as_str().unwrap().contains("disagrees"),
        "{disagreeing}"
    );
}

#[test]
fn documents_without_locks_or_pins_keep_their_bytes() {
    let reference = created();
    assert!(reference["curated"].get("lockedRoutes").is_none());
    assert!(reference["curated"].get("sizePins").is_none());
}
