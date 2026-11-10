//! Curated reference operations through the dispatcher, on the synthetic C0 fixtures. The same JSON path serves the
//! native command, the WASM adapter and the backend.

use arqlet_runtime::dispatch;
use serde_json::{json, Value};

const FIXTURES: [(&str, &str); 3] = [
    (
        "ports-fanout",
        include_str!("../../../examples/fixtures/reference/ports-fanout.graph.json"),
    ),
    (
        "cycle-selfloop",
        include_str!("../../../examples/fixtures/reference/cycle-selfloop.graph.json"),
    ),
    (
        "container-boundary",
        include_str!("../../../examples/fixtures/reference/container-boundary.graph.json"),
    ),
];

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn snapshot_of(graph: &str) -> Value {
    let response = run(
        json!({"op": "import", "graph": serde_json::from_str::<Value>(graph).unwrap(), "limit": 40}),
    );
    assert_eq!(response["ok"], true, "{response}");
    response["snapshot"].clone()
}

fn reference_of(graph: &str, id: &str) -> Value {
    let response =
        run(json!({"op": "referenceCreate", "snapshot": snapshot_of(graph), "referenceId": id}));
    assert_eq!(response["ok"], true, "{response}");
    response["reference"].clone()
}

#[test]
fn every_fixture_creates_a_sealed_reference_that_validates() {
    for (name, graph) in FIXTURES {
        let reference = reference_of(graph, name);
        let response = run(json!({"op": "referenceValidate", "reference": reference}));
        assert_eq!(response["ok"], true, "{name}: {response}");
        let report = &response["report"];
        assert_eq!(report["currentValidation"], true, "{name}");
        assert_eq!(
            report["payloadHash"], reference["integrity"]["payloadHash"],
            "{name}"
        );
        assert!(report["hardViolations"].is_u64(), "{name}");
    }
}

#[test]
fn canonical_text_is_a_fixed_point_for_every_fixture() {
    for (name, graph) in FIXTURES {
        let reference = reference_of(graph, name);
        let canonical = run(json!({"op": "referenceCanonical", "reference": reference}));
        assert_eq!(canonical["ok"], true, "{name}: {canonical}");
        let text = canonical["canonical"].as_str().unwrap();
        let reparsed: Value = serde_json::from_str(text).unwrap();
        let again = run(json!({"op": "referenceCanonical", "reference": reparsed}));
        assert_eq!(again["canonical"], canonical["canonical"], "{name}");
        let report = run(json!({"op": "referenceValidate", "reference": reparsed}));
        assert_eq!(
            report["report"]["payloadHash"], reference["integrity"]["payloadHash"],
            "{name}"
        );
    }
}

#[test]
fn creating_twice_from_one_snapshot_gives_identical_bytes() {
    for (name, graph) in FIXTURES {
        let first =
            run(json!({"op": "referenceCanonical", "reference": reference_of(graph, name)}));
        let second =
            run(json!({"op": "referenceCanonical", "reference": reference_of(graph, name)}));
        assert_eq!(first["canonical"], second["canonical"], "{name}");
    }
}

#[test]
fn unresolved_edges_are_omitted_before_selection_and_recorded() {
    let snapshot = snapshot_of(FIXTURES[2].1);
    assert_eq!(snapshot["omittedUnresolvedEdges"], 1);
    let reference = reference_of(FIXTURES[2].1, "container-boundary");
    let edges = reference["input"]["graph"]["edges"].as_array().unwrap();
    assert!(edges.iter().all(|edge| edge["id"] != "x-unresolved"));
    assert!(reference["curated"]["routes"]
        .as_object()
        .unwrap()
        .keys()
        .all(|id| id != "x-unresolved"));
}

#[test]
fn a_changed_document_is_rejected_with_an_explicit_error() {
    let mut reference = reference_of(FIXTURES[0].1, "ports-fanout");
    reference["referenceId"] = json!("renamed");
    let response = run(json!({"op": "referenceValidate", "reference": reference}));
    assert_eq!(response["ok"], false);
    assert!(
        response["error"]
            .as_str()
            .unwrap()
            .starts_with("Integrity mismatch"),
        "{response}"
    );
}

#[test]
fn an_unsupported_schema_version_is_rejected_with_an_explicit_error() {
    let mut reference = reference_of(FIXTURES[1].1, "cycle-selfloop");
    reference["schemaVersion"] = json!(2);
    let response = run(json!({"op": "referenceCanonical", "reference": reference}));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"], "Unsupported reference schema version 2");
}

#[test]
fn a_missing_reference_id_is_an_error_not_a_document() {
    let response = run(json!({"op": "referenceCreate", "snapshot": snapshot_of(FIXTURES[0].1)}));
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"], "referenceId must not be empty");
}

#[test]
fn an_explicit_port_cannot_be_moved_by_curation() {
    let reference = reference_of(FIXTURES[0].1, "ports-fanout");
    let mut document: arqlet_reference::ReferenceDocument =
        serde_json::from_value(reference).unwrap();
    let explicit = document
        .curated
        .ports
        .iter_mut()
        .find(|(_, port)| port.explicit)
        .expect("the fixture has an explicit port");
    explicit.1.offset = 0.9;
    arqlet_reference::seal(&mut document).unwrap();
    let response = run(json!({"op": "referenceRefresh", "reference": document}));
    assert_eq!(response["ok"], false, "{response}");
    assert!(
        response["error"].as_str().unwrap().contains("is locked"),
        "{response}"
    );
}

#[test]
fn reroute_without_pending_routes_is_an_explicit_error() {
    let response = run(
        json!({"op": "referenceReroute", "reference": reference_of(FIXTURES[1].1, "cycle-selfloop")}),
    );
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"], "There are no pending routes to re-route");
}

/// C0 exit criterion: an unedited snapshot round-trips through the SVG encoding and through the JSON encoding, and the
/// SVG round trip gives the same canonical bytes. Every fixture goes through the dispatcher.
#[test]
fn every_fixture_round_trips_through_svg_and_json() {
    for (name, graph) in FIXTURES {
        let reference = reference_of(graph, name);
        let canonical = run(json!({"op": "referenceCanonical", "reference": reference}));
        assert_eq!(canonical["ok"], true, "{canonical}");
        let svg = run(json!({"op": "referenceSvg", "reference": reference}));
        assert_eq!(svg["ok"], true, "{svg}");
        let back = run(json!({"op": "referenceFromSvg", "svg": svg["svg"]}));
        assert_eq!(back["ok"], true, "{back}");
        let again = run(json!({"op": "referenceCanonical", "reference": back["reference"]}));
        assert_eq!(
            again["canonical"], canonical["canonical"],
            "SVG round trip changed the document"
        );
    }
}

/// A visible edit of the SVG is rejected with an explicit error, not imported as a changed document.
#[test]
fn an_edited_svg_is_rejected_by_the_dispatcher() {
    let reference = reference_of(FIXTURES[0].1, "edited");
    let svg = run(json!({"op": "referenceSvg", "reference": reference}))["svg"]
        .as_str()
        .unwrap()
        .to_string();
    let edited = svg.replacen("stroke=\"#333\"", "stroke=\"#f00\"", 1);
    assert_ne!(edited, svg);
    let response = run(json!({"op": "referenceFromSvg", "svg": edited}));
    assert_eq!(response["ok"], false, "{response}");
    assert!(
        response["error"]
            .as_str()
            .unwrap()
            .contains("visible drawing"),
        "{response}"
    );
}

/// A rounded routing style: the reference keeps the corners (so its metrics count bends at corners), the SVG draws the arcs of the
/// scene, and the SVG still round-trips byte for byte.
#[test]
fn a_rounded_style_is_drawn_with_arcs_in_the_svg_and_kept_as_corners_in_the_document() {
    let graph = |style: Option<&str>| {
        let mut value: Value = serde_json::from_str(FIXTURES[0].1).unwrap();
        if let Some(style) = style {
            value["defaultRouting"] = json!(style);
        }
        value.to_string()
    };
    let plain = reference_of(&graph(None), "plain");
    let rounded = reference_of(&graph(Some("roundedOrthogonal")), "rounded");
    let count = |reference: &Value| -> usize {
        reference["curated"]["routes"]
            .as_object()
            .unwrap()
            .values()
            .map(|points| points.as_array().unwrap().len())
            .sum()
    };
    assert_eq!(
        count(&plain),
        count(&rounded),
        "the document keeps the corners of both"
    );
    assert_eq!(
        plain["validation"]["metrics"]["bends"],
        rounded["validation"]["metrics"]["bends"]
    );
    let svg_points = |reference: &Value| -> usize {
        run(json!({"op": "referenceSvg", "reference": reference}))["svg"]
            .as_str()
            .unwrap()
            .lines()
            .filter(|line| line.starts_with("<polyline"))
            .map(|line| {
                line.split("points=\"")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap()
                    .split(' ')
                    .count()
            })
            .sum()
    };
    assert!(
        svg_points(&rounded) > svg_points(&plain),
        "the rounded export has the points of its arcs"
    );
    let svg = run(json!({"op": "referenceSvg", "reference": rounded}));
    let back = run(json!({"op": "referenceFromSvg", "svg": svg["svg"]}));
    assert_eq!(back["ok"], true, "{back}");
    let canonical =
        |r: &Value| run(json!({"op": "referenceCanonical", "reference": r}))["canonical"].clone();
    assert_eq!(canonical(&back["reference"]), canonical(&rounded));
}
