//! Provisional: a node of shape `busLine` is drawn as a bar past the nodes its edges attach, the edges as taps onto it.
use arqlet_model::{Graph, LayoutOptions, Rect, Snapshot};
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

fn inside(p: &arqlet_model::Point, r: &Rect) -> bool {
    p.x >= r.x - 1e-6
        && p.x <= r.x + r.width + 1e-6
        && p.y >= r.y - 1e-6
        && p.y <= r.y + r.height + 1e-6
}

fn check(snapshot: &Snapshot) {
    let bar = &snapshot.bus_lines["can"];
    assert!(
        bar.width.min(bar.height) <= 8. + 1e-6,
        "a thin bar: {bar:?}"
    );
    // The box stays as the layout placed it.
    assert!(
        snapshot.boxes["can"]
            .width
            .min(snapshot.boxes["can"].height)
            > 8.
    );
    for route in &snapshot.routes {
        let first = route.points.first().unwrap();
        let last = route.points.last().unwrap();
        // Every tap ends on the bar, at the bus's end of the edge; the other end is on the node's border.
        let (on_bar, at_node, node) = if route.id == "out" {
            (first, last, "display")
        } else {
            (last, first, route.id.trim_start_matches("in"))
        };
        assert!(
            inside(on_bar, bar),
            "{} ends on the bar: {on_bar:?} {bar:?}",
            route.id
        );
        let id = if node == "display" {
            "display".to_string()
        } else {
            format!("ecu{node}")
        };
        assert!(
            inside(at_node, &snapshot.boxes[&id]),
            "{} starts at its node",
            route.id
        );
        // Square: one straight segment, or one bend to the bar's end.
        assert!(route.points.len() <= 3, "{}: {:?}", route.id, route.points);
    }
}

#[test]
fn a_bus_line_is_a_bar_past_its_nodes_with_square_taps() {
    for strategy in [None, Some("organic")] {
        let snapshot =
            import_graph_with(graph(5, strategy), 100, &LayoutOptions::default()).unwrap();
        check(&snapshot);
        // The bar reaches past the nodes on one side of it in a layered area: across the flow, wider than the bus box.
        if strategy.is_none() {
            let bar = &snapshot.bus_lines["can"];
            assert!(bar.width > snapshot.boxes["can"].width, "{bar:?}");
        }
    }
}

#[test]
fn applying_the_bars_again_changes_nothing_and_a_graph_without_them_has_none() {
    let mut snapshot = import_graph_with(graph(4, None), 100, &LayoutOptions::default()).unwrap();
    let before = serde_json::to_string(&snapshot).unwrap();
    arqlet_layout::apply_bus_lines(&mut snapshot);
    assert_eq!(serde_json::to_string(&snapshot).unwrap(), before);
    let mut plain = graph(4, None);
    plain.nodes[0].shape = Default::default();
    let without = import_graph_with(plain, 100, &LayoutOptions::default()).unwrap();
    assert!(without.bus_lines.is_empty());
    assert!(!serde_json::to_string(&without)
        .unwrap()
        .contains("busLines"));
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
        "nodes": [node("gateway", None), node("a", Some("busLine")), node("b", Some("busLine")), node("leaf-a", None), node("leaf-b", None), node("both", None)],
        "edges": [edge("ga", "gateway", "a"), edge("gb", "gateway", "b"), edge("la", "leaf-a", "a"), edge("lb", "leaf-b", "b"),
            edge("xa", "both", "a"), edge("xb", "both", "b")]
    }))
    .unwrap();
    let s = import_graph_with(g, 100, &LayoutOptions::default()).unwrap();
    let top = |id: &str| s.boxes[id].y;
    let bar = |id: &str| s.bus_lines[id].y;
    // The gateway (on the most buses) above both bars, every other node below them: a node on both below both.
    for bus in ["a", "b"] {
        assert!(
            top("gateway") + s.boxes["gateway"].height <= bar(bus),
            "gateway above {bus}"
        );
        for n in ["leaf-a", "leaf-b", "both"] {
            assert!(top(n) >= bar(bus), "{n} below {bus}");
        }
    }
    // Every route keeps its logical direction: it starts at its source's node (the export's edges point to the bus).
    for route in &s.routes {
        let edge = s.graph.edges.iter().find(|e| e.id == route.id).unwrap();
        let start = route.points.first().unwrap();
        let b = &s.boxes[&edge.source];
        assert!(
            start.x >= b.x - 1e-6
                && start.x <= b.x + b.width + 1e-6
                && start.y >= b.y - 1e-6
                && start.y <= b.y + b.height + 1e-6,
            "{} starts at {}",
            route.id,
            edge.source
        );
        assert_eq!(
            edge.target.as_deref().map(|t| t.len() == 1),
            Some(true),
            "the graph keeps its edges: {}",
            edge.id
        );
    }
}
