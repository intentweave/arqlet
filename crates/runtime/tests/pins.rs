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

/// `pinsInSinglePass` (ADR 0016, amendment *Pins in single pass*, step P1): two containers with a chain each and edges
/// between them in both directions, a cycle, a parallel pair and a self-loop; `r` holds a chain of three more leaves.
fn layered_graph() -> Value {
    let node = |id: &str, parent: Option<&str>| json!({"id": id, "kind": "symbol", "parentId": parent, "label": id});
    let edge =
        |id: &str, s: &str, t: &str| json!({"id": id, "kind": "calls", "source": s, "target": t});
    json!({
        "nodes": [
            node("p", None), node("q", None),
            node("p1", Some("p")), node("p2", Some("p")), node("p3", Some("p")), node("p4", Some("p")),
            node("q1", Some("q")), node("q2", Some("q")), node("q3", Some("q")),
        ],
        "edges": [
            edge("e1", "p1", "p2"), edge("e2", "p2", "p3"), edge("e3", "p1", "p4"), edge("e4", "p4", "p3"),
            edge("e5", "p2", "q1"), edge("e6", "p3", "q2"), edge("e7", "p3", "q2"), edge("e8", "q3", "p1"),
            edge("e9", "q1", "q2"), edge("e10", "q2", "q3"), edge("e11", "q2", "q2"),
        ],
    })
}

fn pinnable(options: Value) -> Value {
    let response =
        run(json!({"op": "import", "graph": layered_graph(), "limit": 40, "options": options}));
    assert_eq!(response["ok"], true, "{response}");
    response["snapshot"].clone()
}

fn diagnostics(snapshot: &Value) -> Vec<String> {
    serde_json::from_value(snapshot["layoutDiagnostics"].clone()).unwrap_or_default()
}

fn number(value: &Value) -> f64 {
    value.as_f64().unwrap()
}

/// A single-pass drawing with valid geometry and every edge drawn from its source to its target.
fn assert_single_pass(snapshot: &Value) {
    let said = diagnostics(snapshot);
    assert!(
        said.iter().any(|d| d.starts_with("Single pass:")),
        "{said:?}"
    );
    assert!(
        !said
            .iter()
            .any(|d| d.starts_with("Single pass was not used")
                || d.contains("pinned leaf placement")),
        "{said:?}"
    );
    assert_eq!(snapshot["geometryIssues"], json!([]));
    for edge in snapshot["graph"]["edges"].as_array().unwrap() {
        let route = snapshot["routes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == edge["id"])
            .unwrap_or_else(|| panic!("no route for {}", edge["id"]));
        let points = route["points"].as_array().unwrap();
        assert!(points.len() >= 2, "{route}");
        for (end, node, point) in [
            ("source", &edge["source"], &points[0]),
            ("target", &edge["target"], points.last().unwrap()),
        ] {
            let port = snapshot["resolvedPorts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|p| p["edgeId"] == edge["id"] && p["endpoint"] == end)
                .unwrap();
            assert_eq!(&port["nodeId"], node);
            assert!((number(&port["position"]["x"]) - number(&point["x"])).abs() < 1e-6);
            assert!((number(&port["position"]["y"]) - number(&point["y"])).abs() < 1e-6);
        }
    }
}

#[test]
fn a_pinned_leaf_stays_on_single_pass_and_holds_its_place_across_the_flow() {
    let snapshot = pinnable(json!({"pinsInSinglePass": true}));
    let before = snapshot["boxes"]["p2"].clone();
    let pin = json!({"x": number(&before["x"]), "y": number(&before["y"]) + 150.});
    let response = configure(&snapshot, json!({"pins": {"p2": pin}}));
    assert_eq!(response["ok"], true, "{response}");
    let pinned = &response["snapshot"];
    assert_single_pass(pinned);
    assert_eq!(pinned["boxes"]["p2"]["y"], pin["y"]);
    assert!(
        diagnostics(pinned)
            .iter()
            .any(|d| d.starts_with("Pin p2: held across the flow")),
        "{:?}",
        diagnostics(pinned)
    );
    // The import with the pin, which has no drawing to start from, holds it too.
    let imported = pinnable(json!({"pinsInSinglePass": true, "pins": {"p2": pin}}));
    assert_single_pass(&imported);
    assert_eq!(imported["boxes"]["p2"]["y"], pin["y"]);
    // A later option change lays the view out again and keeps the pin.
    let again = configure(pinned, json!({"nodeSpacing": 40.0}));
    assert_eq!(again["ok"], true, "{again}");
    assert_single_pass(&again["snapshot"]);
    assert_eq!(again["snapshot"]["boxes"]["p2"]["y"], pin["y"]);
    // Refinement takes the drawing as a single-pass drawing.
    let refined = run(json!({
        "op": "refine",
        "snapshot": pinned,
        "expectedRevision": pinned["revision"],
        "expectedLayoutVersion": pinned["layoutVersion"],
    }));
    assert_eq!(refined["ok"], true, "{refined}");
    assert_eq!(refined["snapshot"]["boxes"]["p2"]["y"], pin["y"]);
}

#[test]
fn a_pin_in_the_band_of_another_rank_moves_the_node_into_that_rank() {
    let snapshot = pinnable(json!({"pinsInSinglePass": true}));
    // p4 stands beside p2, one rank after p1: pinned where p3 stands, it takes the rank of p3.
    let (target, own) = (&snapshot["boxes"]["p3"], &snapshot["boxes"]["p4"]);
    let pin = json!({"x": number(&target["x"]), "y": number(&own["y"]) + 100.});
    let response = configure(&snapshot, json!({"pins": {"p4": pin}}));
    assert_eq!(response["ok"], true, "{response}");
    let pinned = &response["snapshot"];
    assert_single_pass(pinned);
    assert_eq!(pinned["boxes"]["p4"]["y"], pin["y"]);
    let ranks: Vec<Vec<String>> =
        serde_json::from_value(pinned["layeredPlans"]["p"]["ranks"].clone()).unwrap();
    let rank = |id: &str| ranks.iter().position(|r| r.iter().any(|x| x == id));
    assert!(rank("p4") > rank("p2"), "{ranks:?}");
    // The same pins on the same drawing give the same drawing, and so does laying it out again.
    let same = configure(&snapshot, json!({"pins": {"p4": pin}}));
    assert_eq!(same["snapshot"], *pinned);
}

#[test]
fn a_drag_that_keeps_the_pin_stays_on_single_pass_and_a_release_removes_it() {
    let snapshot = pinnable(json!({"pinsInSinglePass": true}));
    let step =
        |snapshot: &Value, gesture: &str, seq: u64, phase: &str, at: &Value, release: bool| {
            let response = run(json!({
                "op": "move",
                "snapshot": snapshot,
                "expectedRevision": snapshot["revision"],
                "expectedLayoutVersion": snapshot["layoutVersion"].as_u64().unwrap_or(0),
                "gestureId": gesture, "seq": seq, "phase": phase, "node": "p2",
                "position": at, "release": release,
            }));
            assert_eq!(response["ok"], true, "{response}");
            response["snapshot"].clone()
        };
    let at = &snapshot["boxes"]["p2"];
    let to = json!({"x": number(&at["x"]), "y": number(&at["y"]) + 150.});
    let begun = step(&snapshot, "g", 1, "begin", &to, false);
    let pinned = step(&begun, "g", 2, "finish", &to, false);
    assert_single_pass(&pinned);
    assert_eq!(pinned["options"]["pins"]["p2"], to);
    assert_eq!(pinned["boxes"]["p2"]["y"], to["y"]);
    // Closing and opening the other container keeps the pin and single pass.
    let toggle = |snapshot: &Value| {
        let response = run(json!({
            "op": "toggle",
            "snapshot": snapshot,
            "expectedRevision": snapshot["revision"],
            "expectedLayoutVersion": snapshot["layoutVersion"],
            "nodeId": "q",
        }));
        assert_eq!(response["ok"], true, "{response}");
        response["snapshot"].clone()
    };
    let closed = toggle(&pinned);
    assert_single_pass(&closed);
    assert_eq!(closed["boxes"]["p2"]["y"], to["y"]);
    let open = toggle(&closed);
    assert_single_pass(&open);
    assert_eq!(open["boxes"]["p2"]["y"], to["y"]);
    // Dragged again and released: the pin is gone, the drop is an order.
    let back = json!({"x": number(&at["x"]), "y": number(&at["y"])});
    let begun = step(&open, "h", 1, "begin", &back, false);
    let released = step(&begun, "h", 2, "finish", &back, true);
    assert_single_pass(&released);
    assert!(
        released["options"]
            .get("pins")
            .is_none_or(|p| p == &json!({})),
        "{}",
        released["options"]
    );
}

#[test]
fn pins_the_option_does_not_take_are_handled_as_without_it() {
    for pins in [
        // A container (step P2).
        json!({"p": {"x": 300.0, "y": 200.0}}),
        // A leaf together with a container.
        json!({"p": {"x": 300.0, "y": 200.0}, "q1": {"x": 30.0, "y": 260.0}}),
    ] {
        let with = configure(
            &pinnable(json!({"pinsInSinglePass": true})),
            json!({"pins": pins}),
        );
        let without = configure(&pinnable(json!({})), json!({"pins": pins}));
        assert_eq!(with["ok"], true, "{with}");
        assert_eq!(with["snapshot"]["boxes"], without["snapshot"]["boxes"]);
        assert_eq!(with["snapshot"]["routes"], without["snapshot"]["routes"]);
        assert_eq!(
            with["snapshot"]["layoutDiagnostics"],
            without["snapshot"]["layoutDiagnostics"]
        );
    }
}

/// The vehicle bus view is drawn by the hierarchical pipeline (single pass does not take its buses): a leaf pin keeps it
/// there, and the buses stay buses.
#[test]
fn a_pinned_leaf_of_a_bus_view_stays_on_the_hierarchical_pipeline_and_the_buses_stay_buses() {
    let graph: Value =
        serde_json::from_str(include_str!("../../../benchmarks/datasets/can-bus.json")).unwrap();
    let imported = run(
        json!({"op": "import", "graph": graph, "limit": 100, "options": {"pinsInSinglePass": true}}),
    );
    assert_eq!(imported["ok"], true, "{imported}");
    let snapshot = &imported["snapshot"];
    let at = &snapshot["boxes"]["abs"];
    // The root flows top to bottom: x is across the flow.
    let pin = json!({"x": number(&at["x"]) + 400., "y": number(&at["y"])});
    let response = configure(snapshot, json!({"pins": {"abs": pin}}));
    assert_eq!(response["ok"], true, "{response}");
    let pinned = &response["snapshot"];
    assert_eq!(pinned["boxes"]["abs"]["x"], pin["x"]);
    assert_eq!(pinned["geometryIssues"], json!([]));
    let said = diagnostics(pinned);
    assert!(
        !said
            .iter()
            .any(|d| d.contains("pinned leaf placement") || d.starts_with("Bus ")),
        "{said:?}"
    );
    assert!(said
        .iter()
        .any(|d| d.starts_with("Pin abs: held across the flow")));
    assert_eq!(pinned["routingIssues"], json!([]));
    // A released drag of the pinned node removes the pin, with the option and without it.
    for option in [true, false] {
        let mut from = pinned.clone();
        from["options"]["pinsInSinglePass"] = json!(option);
        let step = |snapshot: &Value, seq: u64, phase: &str| {
            let response = run(json!({
                "op": "move",
                "snapshot": snapshot,
                "expectedRevision": snapshot["revision"],
                "expectedLayoutVersion": snapshot["layoutVersion"],
                "gestureId": "g", "seq": seq, "phase": phase, "node": "abs",
                "position": pin, "release": phase == "finish",
            }));
            assert_eq!(response["ok"], true, "{response}");
            response["snapshot"].clone()
        };
        let released = step(&step(&from, 1, "begin"), 2, "finish");
        assert!(
            released["options"]
                .get("pins")
                .is_none_or(|p| p == &json!({})),
            "{}",
            released["options"]["pins"]
        );
    }
    // Without the option the same pin selects the previous pipeline, as before.
    let mut off = snapshot.clone();
    off["options"]["pinsInSinglePass"] = json!(false);
    let without = configure(&off, json!({"pins": {"abs": pin}}));
    assert!(diagnostics(&without["snapshot"])
        .iter()
        .any(|d| d.contains("pinned leaf placement")));
}
