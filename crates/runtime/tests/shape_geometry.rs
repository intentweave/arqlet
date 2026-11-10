//! Shape geometry in the session: pins are kept, a straight edge that stays blocked is diagnosed, and a measured resize
//! either keeps the drawing clear or reports what it cannot resolve. The publication rule is the geometry issues: a
//! drawing with conflicts is never reported as clear.

use arqlet_model::outline::{collides, Outline};
use arqlet_model::{Graph, NodeSize, Snapshot};
use arqlet_runtime::{configure_snapshot, import_graph, measure_snapshot};
use serde_json::json;
use std::collections::BTreeMap;

/// Root organic with straight edges; `shapes` gives each node's outline.
fn graph(shapes: &[(&str, &str)], edges: &[(&str, &str)]) -> Graph {
    let nodes: Vec<_> = shapes
        .iter()
        .map(|(id, shape)| {
            json!({"id": id, "kind": "node", "label": id, "parentId": null, "shape": shape})
        })
        .collect();
    let edges: Vec<_> = edges
        .iter()
        .enumerate()
        .map(|(i, (s, t))| json!({"id": format!("e{i}"), "kind": "flow", "source": s, "target": t}))
        .collect();
    serde_json::from_value(json!({
        "nodes": nodes,
        "edges": edges,
        "defaultDirection": "auto",
        "defaultStrategy": "organic",
        "defaultRouting": "straight",
    }))
    .expect("the test graph is valid")
}

fn pins_json(pins: &[(&str, f64, f64)]) -> serde_json::Value {
    let map: serde_json::Map<String, serde_json::Value> = pins
        .iter()
        .map(|(id, x, y)| (id.to_string(), json!({"x": x, "y": y})))
        .collect();
    json!({"pins": map})
}

fn pinned(snapshot: &Snapshot, pins: &[(&str, f64, f64)]) -> Snapshot {
    let options = pins_json(pins);
    configure_snapshot(
        snapshot,
        snapshot.revision,
        snapshot.layout_version,
        &options,
    )
    .expect("the pins are accepted")
}

fn straight_through(snapshot: &Snapshot) -> Vec<String> {
    snapshot
        .geometry_issues
        .iter()
        .filter(|i| i.kind == "straight-through-node")
        .map(|i| i.message.clone())
        .collect()
}

#[test]
fn a_fixed_blocker_between_fixed_ends_is_diagnosed_not_hidden() {
    // a and b are fixed at the ends of a straight edge and d is fixed on the segment: nothing movable can clear it.
    let base = import_graph(
        graph(
            &[("a", "rect"), ("b", "rect"), ("d", "rect")],
            &[("a", "b"), ("d", "a")],
        ),
        100,
    )
    .unwrap();
    let pins = [("a", 0., 0.), ("b", 600., 0.), ("d", 300., 0.)];
    let snapshot = pinned(&base, &pins);
    let issues = straight_through(&snapshot);
    assert_eq!(issues.len(), 1, "{:?}", snapshot.geometry_issues);
    assert!(issues[0].contains("node d"), "{issues:?}");
    assert_eq!(
        (snapshot.boxes["d"].x, snapshot.boxes["d"].y),
        (300., 0.),
        "the pin holds"
    );
}

#[test]
fn a_measured_growth_keeps_the_drawing_clear_or_reports_it() {
    let base = import_graph(
        graph(
            &[
                ("a", "rect"),
                ("b", "circle"),
                ("c", "rect"),
                ("d", "circle"),
            ],
            &[("a", "b"), ("b", "c"), ("c", "d")],
        ),
        100,
    )
    .unwrap();
    let pins = [("a", 0., 0.), ("b", 600., 0.)];
    let pinned_snapshot = pinned(&base, &pins);
    // c grows: the drawing may need to move it, and it must say so when it cannot make room.
    let grown = measure_snapshot(
        &pinned_snapshot,
        pinned_snapshot.revision,
        pinned_snapshot.layout_version,
        BTreeMap::from([(
            "c".to_string(),
            NodeSize {
                width: 360.,
                height: 160.,
            },
        )]),
    )
    .unwrap();
    assert_eq!(grown.boxes["c"].width, 360., "the measured size is kept");
    let clear = grown.geometry_issues.is_empty();
    if clear {
        // Publication rule: no conflict may be reported as clear. Overlaps among the root nodes are checked on the outlines.
        let absolute = arqlet_layout::absolute_all(&grown.graph, &grown.boxes);
        let outlines: Vec<Outline> = grown
            .graph
            .nodes
            .iter()
            .map(|n| Outline::of(n.shape, &absolute[n.id.as_str()]))
            .collect();
        for i in 0..outlines.len() {
            for j in i + 1..outlines.len() {
                assert!(
                    !collides(&outlines[i], &outlines[j]),
                    "clear drawing overlaps"
                );
            }
        }
        assert!(
            straight_through(&grown).is_empty(),
            "clear drawing has a blocked edge"
        );
    }
    for (id, x, y) in pins {
        assert_eq!(
            (grown.boxes[id].x, grown.boxes[id].y),
            (x, y),
            "pin of {id}"
        );
    }
}

#[test]
fn a_moved_pin_is_taken_from_the_previous_layout() {
    let base = import_graph(
        graph(
            &[("a", "rect"), ("b", "rect"), ("c", "circle")],
            &[("a", "b"), ("b", "c")],
        ),
        100,
    )
    .unwrap();
    let first = pinned(&base, &[("a", 0., 0.)]);
    let second = pinned(&first, &[("a", 0., 0.), ("c", 400., 300.)]);
    assert_eq!((second.boxes["a"].x, second.boxes["a"].y), (0., 0.));
    assert_eq!((second.boxes["c"].x, second.boxes["c"].y), (400., 300.));
    assert!(second
        .geometry_issues
        .iter()
        .all(|i| !i.kind.contains("fixed")));
}
