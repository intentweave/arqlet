//! The typed geometry update: the consumer reports the outline it drew and the revision of the measurement. The engine applies
//! it to the same state as a plain size, refuses an outline that does not fit the node's shape, and refuses a late measurement
//! of an element it has already heard from at a newer revision.

use arqlet_model::{Contour, Graph, Measurement, NodeSize, Snapshot};
use arqlet_runtime::{dispatch, import_graph, measure_contours, measure_snapshot};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn graph() -> Graph {
    serde_json::from_value(json!({
        "nodes": [
            {"id": "a", "kind": "node", "label": "a", "parentId": null, "shape": "rect"},
            {"id": "b", "kind": "node", "label": "b", "parentId": null, "shape": "circle"},
            {"id": "c", "kind": "node", "label": "c", "parentId": null, "shape": "rect"},
        ],
        "edges": [
            {"id": "e0", "kind": "flow", "source": "a", "target": "b"},
            {"id": "e1", "kind": "flow", "source": "b", "target": "c"},
        ],
    }))
    .expect("the test graph is valid")
}

fn imported() -> Snapshot {
    import_graph(graph(), 100).unwrap()
}

fn report(snapshot: &Snapshot, items: &[(&str, Contour, u64)]) -> Result<Snapshot, String> {
    let measurements: BTreeMap<String, Measurement> = items
        .iter()
        .map(|(id, contour, revision)| {
            (
                id.to_string(),
                Measurement {
                    contour: *contour,
                    geometry_revision: *revision,
                },
            )
        })
        .collect();
    measure_contours(
        snapshot,
        snapshot.revision,
        snapshot.layout_version,
        measurements,
    )
}

fn rect(width: f64, height: f64) -> Contour {
    Contour::Rect { width, height }
}

fn circle(diameter: f64) -> Contour {
    Contour::Circle { diameter }
}

#[test]
fn a_reported_outline_sets_the_size_and_the_revision() {
    let snapshot = report(
        &imported(),
        &[("a", rect(260., 80.), 1), ("b", circle(200.), 1)],
    )
    .unwrap();
    assert_eq!(snapshot.boxes["a"].width, 260.);
    assert_eq!(snapshot.boxes["a"].height, 80.);
    assert_eq!(
        snapshot.boxes["b"].width, 200.,
        "a circle takes its diameter"
    );
    assert_eq!(snapshot.boxes["b"].height, 200.);
    assert_eq!(snapshot.options.geometry_revisions["a"], 1);
    assert_eq!(snapshot.options.geometry_revisions["b"], 1);
}

#[test]
fn a_plain_size_and_an_outline_reach_the_same_state() {
    let base = imported();
    let plain = measure_snapshot(
        &base,
        base.revision,
        base.layout_version,
        BTreeMap::from([(
            "a".to_string(),
            NodeSize {
                width: 260.,
                height: 80.,
            },
        )]),
    )
    .unwrap();
    let typed = report(&base, &[("a", rect(260., 80.), 1)]).unwrap();
    assert_eq!(plain.options.node_sizes, typed.options.node_sizes);
    for (id, p) in &plain.boxes {
        let t = &typed.boxes[id];
        assert_eq!(
            (p.x, p.y, p.width, p.height),
            (t.x, t.y, t.width, t.height),
            "{id}"
        );
    }
    assert!(plain.options.geometry_revisions.is_empty());
}

#[test]
fn an_outline_of_another_kind_than_the_shape_is_refused() {
    let base = imported();
    let error = report(&base, &[("b", rect(200., 200.), 1)]).unwrap_err();
    assert!(error.contains("does not match its shape"), "{error}");
    let error = report(&base, &[("a", circle(100.), 1)]).unwrap_err();
    assert!(error.contains("does not match its shape"), "{error}");
}

#[test]
fn an_unknown_element_or_an_invalid_extent_is_refused() {
    let base = imported();
    assert!(report(&base, &[("zz", rect(100., 50.), 1)])
        .unwrap_err()
        .contains("Unknown node"));
    assert!(report(&base, &[("a", rect(0., 50.), 1)])
        .unwrap_err()
        .contains("Invalid measurement"));
    assert!(report(&base, &[("a", rect(f64::NAN, 50.), 1)]).is_err());
}

#[test]
fn a_late_measurement_is_refused_and_the_same_revision_is_a_retry() {
    let first = report(&imported(), &[("a", rect(300., 90.), 5)]).unwrap();
    let late = report(&first, &[("a", rect(200., 60.), 3)]).unwrap_err();
    assert!(late.contains("Stale measurement for a"), "{late}");
    assert!(late.contains("revision 3, current 5"), "{late}");
    let retry = report(&first, &[("a", rect(300., 90.), 5)]).unwrap();
    assert_eq!(retry.boxes["a"].width, 300.);
    let newer = report(&first, &[("a", rect(220., 70.), 6)]).unwrap();
    assert_eq!(newer.boxes["a"].width, 220.);
    assert_eq!(newer.options.geometry_revisions["a"], 6);
}

#[test]
fn one_refused_measurement_applies_none() {
    let first = report(&imported(), &[("a", rect(300., 90.), 5)]).unwrap();
    let error = report(
        &first,
        &[("a", rect(100., 40.), 2), ("c", rect(250., 70.), 1)],
    )
    .unwrap_err();
    assert!(error.contains("Stale measurement"), "{error}");
    assert_eq!(
        first.boxes["c"].width, 180.,
        "c was not changed by the refused request"
    );
}

#[test]
fn the_operation_takes_measurements_and_keeps_plain_sizes_working() {
    let base = imported();
    let run = |request: Value| -> Value {
        serde_json::from_str(&dispatch(&request.to_string())).unwrap()
    };
    let typed = run(json!({
        "op": "measure",
        "snapshot": base,
        "expectedRevision": base.revision,
        "expectedLayoutVersion": base.layout_version,
        "measurements": {"a": {"contour": {"kind": "rect", "width": 280.0, "height": 76.0}, "geometryRevision": 2}},
    }));
    assert_eq!(typed["ok"], true, "{typed}");
    assert_eq!(typed["snapshot"]["options"]["geometryRevisions"]["a"], 2);
    assert_eq!(typed["snapshot"]["boxes"]["a"]["width"], 280.0);
    let plain = run(json!({
        "op": "measure",
        "snapshot": base,
        "expectedRevision": base.revision,
        "expectedLayoutVersion": base.layout_version,
        "sizes": {"a": {"width": 280.0, "height": 76.0}},
    }));
    assert_eq!(plain["ok"], true, "{plain}");
    assert!(
        plain["snapshot"]["options"]
            .get("geometryRevisions")
            .is_none(),
        "default output omits it"
    );
    let stale = run(json!({
        "op": "measure",
        "snapshot": typed["snapshot"],
        "expectedRevision": typed["snapshot"]["revision"],
        "expectedLayoutVersion": typed["snapshot"]["layoutVersion"],
        "measurements": {"a": {"contour": {"kind": "rect", "width": 100.0, "height": 40.0}, "geometryRevision": 1}},
    }));
    assert_eq!(stale["ok"], false, "{stale}");
    assert!(
        stale["error"]
            .as_str()
            .unwrap()
            .contains("Stale measurement"),
        "{stale}"
    );
}
