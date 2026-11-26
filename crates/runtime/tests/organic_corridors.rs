//! Provisional (`organicCorridors`, #156, step C1): the corridor plan of every organic area is reported beside the drawing,
//! which stays as it is without the option.
use arqlet_model::{CorridorAxis, Snapshot};
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

/// An organic view: eight units in a chain with chords, two of them containers whose children send edges to other units,
/// and one bus given as data, drawn as lines.
fn request(options: Value) -> Value {
    let mut nodes: Vec<Value> = (0..8)
        .map(|i| json!({"id": format!("u{i}"), "kind": "unit", "label": format!("U{i}")}))
        .collect();
    let mut edges = Vec::new();
    for i in 1..8 {
        edges.push(json!({"id": format!("e{i}"), "kind": "k", "source": format!("u{}", i / 2), "target": format!("u{i}")}));
    }
    for i in 0..6 {
        let parent = format!("u{}", i % 2);
        nodes.push(json!({"id": format!("f{i}"), "kind": "frame", "label": format!("F{i}"), "parentId": parent}));
        edges.push(json!({"id": format!("r{i}"), "kind": "rx", "source": format!("f{i}"), "target": format!("u{}", 2 + i)}));
    }
    edges.push(json!({"id": "loop", "kind": "k", "source": "u3", "target": "u3"}));
    let taps: Vec<Value> = [2, 4, 6]
        .iter()
        .map(
            |i| json!({"edge": format!("t{i}"), "node": format!("u{i}"), "toBus": true, "kind": "on"}),
        )
        .collect();
    json!({"op": "import", "limit": 1000, "options": options, "graph": {
        "nodes": nodes, "edges": edges, "buses": [{"id": "bus", "kind": "bus", "label": "Bus", "taps": taps}],
        "defaultStrategy": "organic", "defaultDirection": "auto"}})
}

fn answer(request: &Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn snapshot(value: &Value) -> Snapshot {
    assert_eq!(value["ok"], true, "{value}");
    serde_json::from_value(value["snapshot"].clone()).unwrap()
}

/// Every corridor of a root plan starts at the side of the box before it and ends at the side of the box after it, on the
/// boxes of the snapshot (a bus drawn as lines keeps its box in `bus_boxes`).
fn describes_the_boxes(s: &Snapshot) {
    let root = s
        .corridor_plans
        .iter()
        .find(|p| p.area == "<root>")
        .unwrap();
    let find = |id: &String| s.boxes.get(id).or_else(|| s.bus_boxes.get(id)).unwrap();
    let near = |a: f64, b: f64| (a - b).abs() < 0.01;
    for c in &root.corridors {
        let x = c.axis == CorridorAxis::X;
        if let Some(b) = &c.before {
            let r = find(b);
            let (side, start) = if x {
                (r.x + r.width, c.rect.x)
            } else {
                (r.y + r.height, c.rect.y)
            };
            assert!(near(side, start), "{b}: {c:?} {r:?}");
        }
        if let Some(a) = &c.after {
            let r = find(a);
            let (side, end) = if x {
                (r.x, c.rect.x + c.rect.width)
            } else {
                (r.y, c.rect.y + c.rect.height)
            };
            assert!(near(side, end), "{a}: {c:?} {r:?}");
        }
    }
}

#[test]
fn the_report_leaves_the_drawing_as_it_is_and_is_omitted_without_the_option() {
    let plain = answer(&request(json!({"busStyle": "lines"})));
    assert!(!plain.to_string().contains("corridorPlans"));
    assert!(!plain.to_string().contains("organicCorridors"));
    let with = answer(&request(
        json!({"busStyle": "lines", "organicCorridors": {}}),
    ));
    let mut stripped = with.clone();
    let report = stripped["snapshot"]
        .as_object_mut()
        .unwrap()
        .remove("corridorPlans")
        .expect("a report");
    stripped["snapshot"]["options"]
        .as_object_mut()
        .unwrap()
        .remove("organicCorridors");
    assert_eq!(stripped, plain);

    let s = snapshot(&with);
    describes_the_boxes(&s);
    let areas: Vec<&str> = s.corridor_plans.iter().map(|p| p.area.as_str()).collect();
    assert_eq!(areas, ["<root>", "u0", "u1"], "{report}");
    let root = &s.corridor_plans[0];
    // The eight units and the bus, which the router sees as a box; seven chain edges, six from children and three taps.
    assert_eq!((root.boxes, root.routes.len()), (9, 16));
    assert_eq!(root.skipped.self_loops, 1);
    assert_eq!(root.skipped.unrouted, 0);
    assert_eq!(
        (
            root.clearance,
            root.track,
            root.stub,
            root.gap,
            root.route_cost
        ),
        (12., 6., 12., 24., 1.)
    );
    // The children's edges leave their containers.
    assert_eq!(s.corridor_plans[1].skipped.leaving, 3);
    assert!(root.corridors.iter().any(|c| c.demand > 0));
}

#[test]
fn an_edit_reports_the_boxes_it_laid_out_and_turning_the_option_off_removes_the_report() {
    let first = answer(&request(
        json!({"busStyle": "lines", "organicCorridors": {"routeCost": 6}}),
    ));
    let before = snapshot(&first);
    let toggled = answer(
        &json!({"op": "toggle", "nodeId": "u0", "snapshot": first["snapshot"],
        "expectedRevision": before.revision, "expectedLayoutVersion": before.layout_version}),
    );
    let after = snapshot(&toggled);
    assert_ne!(before.boxes, after.boxes);
    describes_the_boxes(&after);
    assert_eq!(after.corridor_plans[0].route_cost, 6.);
    let off = answer(
        &json!({"op": "configure", "options": {"organicCorridors": null}, "snapshot": toggled["snapshot"],
        "expectedRevision": after.revision, "expectedLayoutVersion": after.layout_version}),
    );
    assert!(snapshot(&off).corridor_plans.is_empty());
}

#[test]
fn a_route_cost_that_is_not_a_distance_is_refused() {
    let refused = answer(&request(json!({"organicCorridors": {"routeCost": -1}})));
    assert_eq!(refused["ok"], false);
    assert!(refused["error"]
        .as_str()
        .unwrap()
        .contains("organicCorridors.routeCost"));
}
