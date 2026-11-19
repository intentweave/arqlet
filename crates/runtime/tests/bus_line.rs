//! Provisional (ADR 0019 view model): a node of shape `busLine` is a bus line in the view. The source graph keeps the bus as a
//! node with its edges; the view leaves them out and keeps a `Bus` with one tap per edge; the snapshot draws it as a bar past
//! the nodes the taps attach (`Snapshot::bus_lines`).
use arqlet_model::{Graph, LayoutOptions, Point, Rect, Snapshot};
use arqlet_runtime::import_graph_with;
use serde_json::json;

/// One bus and `ecus` nodes attached to it (edges ECU → bus), and one edge bus → `display` (the other direction).
fn graph(ecus: usize, strategy: Option<&str>) -> Graph {
    let mut nodes = vec![
        json!({"id": "can", "kind": "bus", "label": "CAN", "shape": "busLine"}),
        json!({"id": "display", "kind": "ecu", "label": "Display"}),
    ];
    let mut edges =
        vec![json!({"id": "out", "kind": "connects", "source": "can", "target": "display"})];
    for i in 0..ecus {
        nodes.push(json!({"id": format!("ecu{i}"), "kind": "ecu", "label": format!("ECU {i}")}));
        edges.push(json!({"id": format!("in{i}"), "kind": "connects", "source": format!("ecu{i}"), "target": "can"}));
    }
    let mut g = json!({"defaultDirection": "tb", "nodes": nodes, "edges": edges});
    if let Some(s) = strategy {
        g["defaultStrategy"] = json!(s);
    }
    serde_json::from_value(g).unwrap()
}

fn inside(p: &Point, r: &Rect) -> bool {
    p.x >= r.x - 1e-6
        && p.x <= r.x + r.width + 1e-6
        && p.y >= r.y - 1e-6
        && p.y <= r.y + r.height + 1e-6
}

/// Absolute boxes of the view (boxes are relative to their parents; these graphs have none).
fn check(snapshot: &Snapshot, ecus: usize) {
    // The view: no bus node, no bus edge; one bus line with a tap per edge, in each edge's direction.
    assert!(snapshot.graph.nodes.iter().all(|n| n.id != "can"));
    assert!(snapshot.graph.edges.is_empty());
    let bus = snapshot.graph.buses.iter().find(|b| b.id == "can").unwrap();
    assert_eq!(bus.taps.len(), ecus + 1);
    assert!(bus
        .taps
        .iter()
        .any(|t| t.edge == "out" && t.node == "display" && !t.to_bus));
    assert!(bus.taps.iter().all(|t| t.edge == "out" || t.to_bus));
    // The source graph keeps the bus as a node with its edges.
    let source = snapshot.source_graph.as_deref().unwrap();
    assert!(source.nodes.iter().any(|n| n.id == "can"));
    assert_eq!(source.edges.len(), ecus + 1);
    // The bar: thin; the taps square, from their node to the bar (or back for the edge out of the bus).
    let line = &snapshot.bus_lines["can"];
    assert!(
        line.bar.width.min(line.bar.height) <= 8. + 1e-6,
        "a thin bar: {:?}",
        line.bar
    );
    assert!(
        snapshot.routes.is_empty(),
        "the taps are not routes of the view"
    );
    for tap in &bus.taps {
        let points = &line.taps[&tap.edge];
        let (at_node, on_bar) = if tap.to_bus {
            (points.first().unwrap(), points.last().unwrap())
        } else {
            (points.last().unwrap(), points.first().unwrap())
        };
        assert!(inside(on_bar, &line.bar), "{} ends on the bar", tap.edge);
        assert!(
            inside(at_node, &snapshot.boxes[&tap.node]),
            "{} starts at its node",
            tap.edge
        );
        assert!(points.len() <= 3, "{}: {points:?}", tap.edge);
    }
    // The bus's box stays for the next layout.
    assert!(snapshot.bus_boxes.contains_key("can"));
    assert!(!snapshot.boxes.contains_key("can"));
}

#[test]
fn a_bus_line_is_a_bar_past_its_nodes_with_square_taps() {
    for strategy in [None, Some("organic")] {
        let snapshot =
            import_graph_with(graph(5, strategy), 100, &LayoutOptions::default()).unwrap();
        check(&snapshot, 5);
    }
}

#[test]
fn drawing_the_bars_again_changes_nothing_and_a_graph_without_them_has_none() {
    let mut snapshot = import_graph_with(graph(4, None), 100, &LayoutOptions::default()).unwrap();
    let before = serde_json::to_string(&snapshot).unwrap();
    arqlet_layout::apply_bus_lines(&mut snapshot);
    assert_eq!(serde_json::to_string(&snapshot).unwrap(), before);
    // A snapshot read back draws the same.
    let mut read: Snapshot = serde_json::from_str(&before).unwrap();
    arqlet_layout::apply_bus_lines(&mut read);
    assert_eq!(serde_json::to_string(&read).unwrap(), before);
    let mut plain = graph(4, None);
    plain.nodes[0].shape = Default::default();
    let without = import_graph_with(plain, 100, &LayoutOptions::default()).unwrap();
    assert!(without.bus_lines.is_empty());
    assert!(!serde_json::to_string(&without)
        .unwrap()
        .contains("busLines"));
}

#[test]
fn a_toggle_keeps_the_bus_line() {
    // The bus and its nodes inside a container: closing and opening it again gives the bus line back.
    let mut g = graph(3, None);
    g.nodes.push(
        serde_json::from_value(json!({"id": "net", "kind": "zone", "label": "Net"})).unwrap(),
    );
    for n in &mut g.nodes {
        if n.id != "net" {
            n.parent_id = Some("net".into());
        }
    }
    let first = import_graph_with(g, 100, &LayoutOptions::default()).unwrap();
    assert!(first.bus_lines.contains_key("can"));
    let closed =
        arqlet_runtime::toggle_container(&first, first.revision, first.layout_version, "net")
            .unwrap();
    assert!(
        closed.bus_lines.is_empty(),
        "the bus is inside the closed container"
    );
    let open =
        arqlet_runtime::toggle_container(&closed, closed.revision, closed.layout_version, "net")
            .unwrap();
    assert!(open.bus_lines.contains_key("can"));
    assert_eq!(
        open.graph
            .buses
            .iter()
            .find(|b| b.id == "can")
            .unwrap()
            .taps
            .len(),
        4
    );
}

#[test]
fn a_bus_network_reads_as_a_tree_from_its_gateway() {
    // A gateway on two buses, a node on each, and one node on both buses; every edge points from the node to its bus, as
    // in an export of bus connections. The gateway and the node on both buses tie on buses: the earlier in the graph is
    // the root, so the gateway comes first.
    let node = |id: &str, shape: Option<&str>| {
        let mut n = json!({"id": id, "kind": "ecu", "label": id});
        if let Some(s) = shape {
            n["shape"] = json!(s);
        }
        n
    };
    let edge = |id: &str, s: &str, t: &str| json!({"id": id, "kind": "connects", "source": s, "target": t});
    let g: Graph = serde_json::from_value(json!({
        "defaultDirection": "tb",
        "nodes": [node("gateway", None), node("a", Some("busLine")), node("b", Some("busLine")), node("leaf-a", None),
            node("leaf-b", None), node("both", None)],
        "edges": [edge("ga", "gateway", "a"), edge("gb", "gateway", "b"), edge("la", "leaf-a", "a"), edge("lb", "leaf-b", "b"),
            edge("xa", "both", "a"), edge("xb", "both", "b")]
    }))
    .unwrap();
    let s = import_graph_with(g, 100, &LayoutOptions::default()).unwrap();
    let top = |id: &str| s.boxes[id].y;
    let bar = |id: &str| s.bus_lines[id].bar.y;
    for bus in ["a", "b"] {
        assert!(
            top("gateway") + s.boxes["gateway"].height <= bar(bus),
            "gateway above {bus}"
        );
        for n in ["leaf-a", "leaf-b", "both"] {
            assert!(top(n) >= bar(bus), "{n} below {bus}");
        }
    }
    // Every tap keeps its logical direction: it starts at its node (the export's edges point to the bus).
    for bus in &s.graph.buses {
        for tap in &bus.taps {
            let start = s.bus_lines[&bus.id].taps[&tap.edge]
                .first()
                .unwrap()
                .clone();
            assert!(
                inside(&start, &s.boxes[&tap.node]),
                "{} starts at {}",
                tap.edge,
                tap.node
            );
        }
    }
}

#[test]
fn a_trace_marks_the_bar_and_the_taps_it_runs_over() {
    let s = import_graph_with(graph(3, None), 100, &LayoutOptions::default()).unwrap();
    let answer =
        arqlet_runtime::trace::trace_answer_on(&s, &json!({"from": "ecu0", "to": "display"}))
            .unwrap();
    assert_eq!(answer.length, Some(2));
    let mut edges = answer.edges.clone();
    edges.sort();
    assert_eq!(edges, ["in0", "out"]);
    assert_eq!(answer.visible.nodes, ["can", "display", "ecu0"]);
    assert_eq!(answer.visible.routes, ["in0", "out"]);
}

#[test]
fn every_operation_answers_the_view_with_its_bus_lines() {
    // Refinement, a direction change and a move continue the layout of the graph with the bus node; each answer is the
    // view again: no bus node, one bus line with a tap per edge, the tap layout kept apart.
    let run = |v: serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&arqlet_runtime::dispatch(&v.to_string())).unwrap()
    };
    let first = import_graph_with(graph(4, None), 100, &LayoutOptions::default()).unwrap();
    let first = serde_json::to_value(&first).unwrap();
    let on = |s: &serde_json::Value, op: serde_json::Value| {
        let mut v = op;
        v["snapshot"] = s.clone();
        v["expectedRevision"] = s["revision"].clone();
        v["expectedLayoutVersion"] = s["layoutVersion"].clone();
        let answer = run(v);
        assert_eq!(answer["ok"], true, "{answer}");
        answer["snapshot"].clone()
    };
    let refined = on(&first, json!({"op": "refine", "threads": 1}));
    let turned = on(&refined, json!({"op": "direction", "direction": "lr"}));
    let mut moved = turned.clone();
    for (seq, phase) in [(1, "begin"), (2, "finish")] {
        moved = on(
            &moved,
            json!({"op": "move", "gestureId": "g", "seq": seq, "phase": phase, "node": "display",
                "position": {"x": 900.0, "y": 40.0}, "release": false}),
        );
    }
    for s in [&refined, &turned, &moved] {
        let s: Snapshot = serde_json::from_value(s.clone()).unwrap();
        check(&s, 4);
        assert_eq!(
            s.bus_taps.routes.len(),
            5,
            "the taps' routes are kept apart"
        );
    }
}
