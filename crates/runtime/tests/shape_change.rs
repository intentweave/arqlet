//! A shape change in the session: the node is drawn in its new outline, pins hold, a repeated shape changes nothing, and an
//! unknown node is refused.

use arqlet_model::{Graph, NodeShape, Snapshot};
use arqlet_runtime::{configure_snapshot, import_graph, set_shape};
use serde_json::json;

fn graph() -> Graph {
    serde_json::from_value(json!({
        "nodes": [
            {"id": "a", "kind": "node", "label": "a", "parentId": null, "shape": "rect"},
            {"id": "b", "kind": "node", "label": "b", "parentId": null, "shape": "rect"},
            {"id": "c", "kind": "node", "label": "c", "parentId": null, "shape": "rect"},
        ],
        "edges": [
            {"id": "e0", "kind": "flow", "source": "a", "target": "b"},
            {"id": "e1", "kind": "flow", "source": "b", "target": "c"},
        ],
        "defaultDirection": "auto",
        "defaultStrategy": "organic",
        "defaultRouting": "straight",
    }))
    .expect("the test graph is valid")
}

#[test]
fn a_shape_change_redraws_the_node_and_keeps_the_pins() {
    let base = import_graph(graph(), 100).unwrap();
    let pinned: Snapshot = configure_snapshot(
        &base,
        base.revision,
        base.layout_version,
        &json!({"pins": {"a": {"x": 0.0, "y": 0.0}}}),
    )
    .unwrap();
    let circle = set_shape(
        &pinned,
        pinned.revision,
        pinned.layout_version,
        "b",
        NodeShape::Circle,
    )
    .unwrap();
    let b = &circle.boxes["b"];
    assert_eq!(b.width, b.height, "a circle is drawn in a square box");
    assert_eq!(
        (circle.boxes["a"].x, circle.boxes["a"].y),
        (0., 0.),
        "the pin holds"
    );
    let same = set_shape(
        &circle,
        circle.revision,
        circle.layout_version,
        "b",
        NodeShape::Circle,
    )
    .unwrap();
    assert_eq!(
        same.layout_version, circle.layout_version,
        "the same shape changes nothing"
    );
    assert!(set_shape(
        &circle,
        circle.revision,
        circle.layout_version,
        "zz",
        NodeShape::Square
    )
    .is_err());
}
