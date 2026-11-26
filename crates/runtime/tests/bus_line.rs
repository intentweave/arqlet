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

/// Absolute top of a box in the view (boxes are relative to their parents).
fn top_of(s: &Snapshot, id: &str) -> f64 {
    let parent: std::collections::BTreeMap<&str, Option<&str>> = s
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let mut at = Some(id);
    let mut y = 0.;
    while let Some(n) = at {
        y += s.boxes[n].y;
        at = parent[n];
    }
    y
}

#[test]
fn a_trunk_without_a_gateway_stands_on_top_with_its_branches_below() {
    // One trunk past four nodes; two of them head a branch bus each. No node is on three buses: the middle of the network
    // is the trunk, so it is the root.
    let node = |id: &str, bus: bool| {
        let mut n = json!({"id": id, "kind": if bus { "bus" } else { "ecu" }, "label": id});
        if bus {
            n["shape"] = json!("busLine");
        }
        n
    };
    let edge = |s: &str, t: &str| json!({"id": format!("{s}-{t}"), "kind": "connects", "source": s, "target": t});
    let g: Graph = serde_json::from_value(json!({
        "defaultDirection": "tb",
        "nodes": [node("head-a", false), node("trunk", true), node("a", true), node("b", true), node("head-b", false),
            node("x", false), node("y", false), node("leaf-a", false), node("leaf-b", false)],
        "edges": [edge("head-a", "trunk"), edge("head-b", "trunk"), edge("x", "trunk"), edge("y", "trunk"),
            edge("head-a", "a"), edge("leaf-a", "a"), edge("head-b", "b"), edge("leaf-b", "b")]
    }))
    .unwrap();
    let s = import_graph_with(g, 100, &LayoutOptions::default()).unwrap();
    let bar = |id: &str| s.bus_lines[id].bar.y;
    for n in ["head-a", "head-b", "x", "y"] {
        assert!(top_of(&s, n) > bar("trunk"), "{n} below the trunk");
    }
    assert!(bar("a") > top_of(&s, "head-a") && top_of(&s, "leaf-a") > bar("a"));
    assert!(bar("b") > top_of(&s, "head-b") && top_of(&s, "leaf-b") > bar("b"));
}

#[test]
fn zones_hang_below_their_backbone_and_a_unit_on_two_zone_buses_below_both() {
    // A backbone with a central unit and two zone controllers; each zone (a container) holds its controller, its bus and an
    // actuator; one unit outside is on both zone buses.
    let node = |id: &str, parent: Option<&str>, bus: bool| {
        let mut n = json!({"id": id, "kind": if bus { "bus" } else { "ecu" }, "label": id, "parentId": parent});
        if bus {
            n["shape"] = json!("busLine");
        }
        n
    };
    let edge = |s: &str, t: &str| json!({"id": format!("{s}-{t}"), "kind": "connects", "source": s, "target": t});
    let g: Graph = serde_json::from_value(json!({
        "defaultDirection": "tb",
        "nodes": [node("central", None, false), node("backbone", None, true),
            node("zone-l", None, false), node("zc-l", Some("zone-l"), false), node("can-l", Some("zone-l"), true),
            node("act-l", Some("zone-l"), false),
            node("zone-r", None, false), node("zc-r", Some("zone-r"), false), node("can-r", Some("zone-r"), true),
            node("act-r", Some("zone-r"), false),
            node("brake", None, false)],
        "edges": [edge("central", "backbone"), edge("zc-l", "backbone"), edge("zc-r", "backbone"), edge("zc-l", "can-l"),
            edge("act-l", "can-l"), edge("zc-r", "can-r"), edge("act-r", "can-r"), edge("brake", "can-l"), edge("brake", "can-r")]
    }))
    .unwrap();
    let s = import_graph_with(g, 100, &LayoutOptions::default()).unwrap();
    assert_eq!(s.bus_lines.len(), 3);
    let bar = |id: &str| s.bus_lines[id].bar.y;
    assert!(top_of(&s, "zone-l") > bar("backbone") && top_of(&s, "zone-r") > bar("backbone"));
    assert!(
        top_of(&s, "brake") > top_of(&s, "zone-l") && top_of(&s, "brake") > top_of(&s, "zone-r")
    );
    // As trunks: the backbone, at the root of the tree, keeps its bar; each zone bus is a line from its controller.
    let trunks = import_graph_with(
        s.source_graph.as_deref().unwrap().clone(),
        100,
        &serde_json::from_value(json!({"busStyle": "trunk"})).unwrap(),
    )
    .unwrap();
    assert_eq!(trunks.bus_lines.keys().collect::<Vec<_>>(), ["backbone"]);
    let zone_bus = trunks.graph.buses.iter().find(|b| b.id == "can-l").unwrap();
    assert!(!zone_bus.edges.is_empty());
    assert!(trunks
        .graph
        .edges
        .iter()
        .filter(|e| zone_bus.edges.contains(&e.id))
        .all(|e| e.source == "zc-l" || e.target.as_deref() == Some("zc-l")));
    let shut =
        arqlet_runtime::toggle_container(&trunks, trunks.revision, trunks.layout_version, "zone-l")
            .unwrap();
    assert!(top_of(&shut, "zone-l") > shut.bus_lines["backbone"].bar.y);
    assert!(top_of(&shut, "brake") > top_of(&shut, "zone-l"));
    // Collapsing a zone keeps the backbone; the zone stands for its bus.
    let closed =
        arqlet_runtime::toggle_container(&s, s.revision, s.layout_version, "zone-l").unwrap();
    assert!(closed.bus_lines.contains_key("backbone") && !closed.bus_lines.contains_key("can-l"));
    // The collapsed zone stands for its bus: below the backbone, and the unit on its bus below it.
    let backbone = closed.bus_lines["backbone"].bar.y;
    assert!(
        top_of(&closed, "zone-l") > backbone,
        "zone below the backbone"
    );
    assert!(
        top_of(&closed, "brake") > top_of(&closed, "zone-l"),
        "unit below the collapsed zone"
    );
}

#[test]
fn a_trunk_bus_is_a_shared_line_from_its_head_without_a_rank() {
    // A gateway on a bus with three nodes, drawn as a trunk: no bar, no bus box; one edge from the gateway to each node
    // with the tap's id, in the tap's own direction; the bus lists them.
    let node = |id: &str, bus: bool| {
        let mut n = json!({"id": id, "kind": if bus { "bus" } else { "ecu" }, "label": id});
        if bus {
            n["shape"] = json!("busLine");
        }
        n
    };
    let edge = |s: &str, t: &str| json!({"id": format!("{s}-{t}"), "kind": "connects", "source": s, "target": t});
    let g: Graph = serde_json::from_value(json!({
        "defaultDirection": "tb",
        "nodes": [node("gw", false), node("can", true), node("a", false), node("b", false), node("c", false),
            node("lin", true), node("d", false)],
        "edges": [edge("gw", "can"), edge("a", "can"), edge("b", "can"), edge("can", "c"), edge("gw", "lin"), edge("d", "lin")]
    }))
    .unwrap();
    let options: LayoutOptions = serde_json::from_value(json!({"busStyle": "trunk"})).unwrap();
    let s = import_graph_with(g, 100, &options).unwrap();
    assert!(s.bus_lines.is_empty() && s.bus_boxes.is_empty());
    let bus = s.graph.buses.iter().find(|b| b.id == "can").unwrap();
    let mut members = bus.edges.clone();
    members.sort();
    assert_eq!(members, ["a-can", "b-can", "can-c"]);
    let ends = |id: &str| {
        let e = s.graph.edges.iter().find(|e| e.id == id).unwrap();
        (e.source.clone(), e.target.clone().unwrap())
    };
    // Own direction: a → bus is a → head; bus → c is head → c.
    assert_eq!(ends("a-can"), ("a".to_string(), "gw".to_string()));
    assert_eq!(ends("can-c"), ("gw".to_string(), "c".to_string()));
    // The head above its members, all members in one rank (no rank for the bus).
    let top = |id: &str| s.boxes[id].y;
    assert!(
        top("a") > top("gw")
            && top("a") == top("b")
            && top("b") == top("c")
            && top("d") == top("a")
    );
    // The route of a member's tap runs in its own direction: it starts at the member.
    let route = s.routes.iter().find(|r| r.id == "a-can").unwrap();
    let first = route.points.first().unwrap();
    assert!(inside(first, &s.boxes["a"]), "{:?}", route.points);
    // Refinement keeps it.
    let run = |v: serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&arqlet_runtime::dispatch(&v.to_string())).unwrap()
    };
    let refined = run(
        json!({"op": "refine", "threads": 1, "snapshot": s, "expectedRevision": s.revision,
        "expectedLayoutVersion": s.layout_version}),
    );
    assert_eq!(refined["ok"], true, "{refined}");
    let r: Snapshot = serde_json::from_value(refined["snapshot"].clone()).unwrap();
    let e = r.graph.edges.iter().find(|e| e.id == "a-can").unwrap();
    assert_eq!((e.source.as_str(), e.target.as_deref()), ("a", Some("gw")));
    // A trace from a to c runs over the trunk's two edges.
    let answer =
        arqlet_runtime::trace::trace_answer_on(&s, &json!({"from": "a", "to": "c"})).unwrap();
    assert_eq!(answer.visible.routes, ["a-can", "can-c"]);
}

#[test]
fn the_bus_style_switches_both_ways() {
    let run = |v: serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&arqlet_runtime::dispatch(&v.to_string())).unwrap()
    };
    let configure = |s: &serde_json::Value, style: &str| {
        let answer = run(
            json!({"op": "configure", "snapshot": s, "options": {"busStyle": style},
            "expectedRevision": s["revision"], "expectedLayoutVersion": s["layoutVersion"]}),
        );
        assert_eq!(answer["ok"], true, "{answer}");
        serde_json::from_value::<Snapshot>(answer["snapshot"].clone()).unwrap()
    };
    // A single bus is the root of its tree: it keeps its bar as a trunk too.
    let alone = import_graph_with(graph(3, None), 100, &LayoutOptions::default()).unwrap();
    let alone = configure(&serde_json::to_value(&alone).unwrap(), "trunk");
    check(&alone, 3);
    // A gateway on two buses heads both.
    let mut g = graph(3, None);
    let more: Graph = serde_json::from_value(json!({
        "nodes": [{"id": "gw", "kind": "ecu", "label": "gw"}, {"id": "lin", "kind": "bus", "label": "LIN", "shape": "busLine"},
            {"id": "leaf", "kind": "ecu", "label": "leaf"}],
        "edges": [{"id": "gw-can", "kind": "connects", "source": "gw", "target": "can"},
            {"id": "gw-lin", "kind": "connects", "source": "gw", "target": "lin"},
            {"id": "leaf-lin", "kind": "connects", "source": "leaf", "target": "lin"}]
    }))
    .unwrap();
    g.nodes.extend(more.nodes);
    g.edges.extend(more.edges);
    let bar = import_graph_with(g, 100, &LayoutOptions::default()).unwrap();
    assert_eq!(bar.bus_lines.len(), 2);
    let trunk = configure(&serde_json::to_value(&bar).unwrap(), "trunk");
    assert!(
        trunk.bus_lines.is_empty(),
        "{:?}",
        trunk.bus_lines.keys().collect::<Vec<_>>()
    );
    assert_eq!(
        trunk.graph.edges.len(),
        5,
        "one edge from the head to each other member"
    );
    let back = configure(&serde_json::to_value(&trunk).unwrap(), "bar");
    assert_eq!(back.bus_lines.len(), 2);
    assert!(back.graph.edges.is_empty());
}

#[test]
fn a_refined_trunk_view_with_a_collapsed_zone_keeps_its_tree() {
    // The zones of the zonal test graph as trunks, one zone collapsed, then refined: the tree stands as before.
    let node = |id: &str, parent: Option<&str>, bus: bool| {
        let mut n = json!({"id": id, "kind": if bus { "bus" } else { "ecu" }, "label": id, "parentId": parent});
        if bus {
            n["shape"] = json!("busLine");
        }
        n
    };
    let edge = |s: &str, t: &str| json!({"id": format!("{s}-{t}"), "kind": "connects", "source": s, "target": t});
    let g: Graph = serde_json::from_value(json!({
        "defaultDirection": "tb",
        "nodes": [node("central", None, false), node("backbone", None, true),
            node("zone-l", None, false), node("zc-l", Some("zone-l"), false), node("can-l", Some("zone-l"), true),
            node("act-l", Some("zone-l"), false),
            node("zone-r", None, false), node("zc-r", Some("zone-r"), false), node("can-r", Some("zone-r"), true),
            node("act-r", Some("zone-r"), false),
            node("brake", None, false)],
        "edges": [edge("central", "backbone"), edge("zc-l", "backbone"), edge("zc-r", "backbone"), edge("zc-l", "can-l"),
            edge("act-l", "can-l"), edge("zc-r", "can-r"), edge("act-r", "can-r"), edge("brake", "can-l"), edge("brake", "can-r")]
    }))
    .unwrap();
    let s = import_graph_with(
        g,
        100,
        &serde_json::from_value(json!({"busStyle": "trunk"})).unwrap(),
    )
    .unwrap();
    let shut =
        arqlet_runtime::toggle_container(&s, s.revision, s.layout_version, "zone-l").unwrap();
    let run = |v: serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&arqlet_runtime::dispatch(&v.to_string())).unwrap()
    };
    let refined = run(
        json!({"op": "refine", "threads": 1, "snapshot": shut, "expectedRevision": shut.revision,
        "expectedLayoutVersion": shut.layout_version}),
    );
    assert_eq!(refined["ok"], true, "{refined}");
    let r: Snapshot = serde_json::from_value(refined["snapshot"].clone()).unwrap();
    assert!(top_of(&r, "zone-l") > r.bus_lines["backbone"].bar.y);
    assert!(top_of(&r, "brake") > top_of(&r, "zone-l"));
    // The joining edge keeps its own direction.
    let e = r
        .graph
        .edges
        .iter()
        .find(|e| e.id == "brake-can-l")
        .unwrap();
    assert_eq!(
        (e.source.as_str(), e.target.as_deref()),
        ("brake", Some("zone-l"))
    );
}

#[test]
fn hub_refs_take_the_edges_of_hubs_out_of_the_layout_and_draw_them_as_stubs() {
    // A hub with six edges and a chain a → b: with `hubRefs: 5` the hub's edges are references, the chain stays routed.
    let mut nodes = vec![
        json!({"id": "hub", "kind": "ecu", "label": "Gateway"}),
        json!({"id": "a", "kind": "ecu", "label": "a"}),
        json!({"id": "b", "kind": "ecu", "label": "b"}),
    ];
    let mut edges = vec![json!({"id": "ab", "kind": "k", "source": "a", "target": "b"})];
    for i in 0..6 {
        nodes.push(json!({"id": format!("n{i}"), "kind": "ecu", "label": format!("n{i}")}));
        let (s, t) = if i % 2 == 0 {
            (format!("n{i}"), "hub".to_string())
        } else {
            ("hub".to_string(), format!("n{i}"))
        };
        edges.push(json!({"id": format!("h{i}"), "kind": "k", "source": s, "target": t}));
    }
    let g: Graph = serde_json::from_value(json!({"nodes": nodes, "edges": edges})).unwrap();
    let options: LayoutOptions = serde_json::from_value(json!({"hubRefs": 5})).unwrap();
    let s = import_graph_with(g.clone(), 100, &options).unwrap();
    assert_eq!(s.hub_refs.len(), 6);
    assert!(s.hub_refs.iter().all(|r| r.hub == "hub" && r.node != "hub"));
    assert!(s.hub_refs.iter().any(|r| r.outgoing) && s.hub_refs.iter().any(|r| !r.outgoing));
    assert_eq!(
        s.routes.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        ["ab"]
    );
    // The source graph keeps every edge.
    assert_eq!(s.source_graph.as_deref().unwrap().edges.len(), 7);
    // The scene draws a stub per reference, beside its node, named after the hub.
    let scene = arqlet_scene::build_scene(&s).unwrap();
    assert_eq!(scene.hub_refs.len(), 6);
    let r0 = scene.hub_refs.iter().find(|r| r.edge == "h0").unwrap();
    assert_eq!(r0.text, "Gateway");
    let n0 = &s.boxes["n0"];
    assert_eq!(r0.points[0].x, n0.x + n0.width, "out of its right side");
    // Without the option nothing changes.
    let plain = import_graph_with(g, 100, &LayoutOptions::default()).unwrap();
    assert!(plain.hub_refs.is_empty() && plain.routes.len() == 7);
    // A trace through the hub marks the stubs.
    let answer =
        arqlet_runtime::trace::trace_answer_on(&s, &json!({"from": "n0", "to": "n1"})).unwrap();
    let mut routes = answer.visible.routes.clone();
    routes.sort();
    assert_eq!(routes, ["h0", "h1"]);
}

#[test]
fn bus_style_lines_never_draws_a_bar() {
    // A single bus is the root of its tree: `trunk` keeps its bar, `lines` draws it as a line from a member.
    let options = |style: &str| -> LayoutOptions {
        serde_json::from_value(json!({"busStyle": style})).unwrap()
    };
    let trunk = import_graph_with(graph(3, None), 100, &options("trunk")).unwrap();
    assert!(trunk.bus_lines.contains_key("can"));
    let lines = import_graph_with(graph(3, None), 100, &options("lines")).unwrap();
    assert!(lines.bus_lines.is_empty(), "no bar");
    let bus = lines.graph.buses.iter().find(|b| b.id == "can").unwrap();
    assert_eq!(
        bus.edges.len(),
        3,
        "a line from the head to each other member"
    );
}

#[test]
fn a_tap_at_a_collapsed_member_attaches_to_its_container() {
    // Members in two containers; one container collapsed: its members' taps end at the container, once.
    let mut nodes = vec![
        json!({"id": "can", "kind": "bus", "label": "CAN", "shape": "busLine"}),
        json!({"id": "left", "kind": "zone", "label": "left"}),
        json!({"id": "right", "kind": "zone", "label": "right"}),
    ];
    let mut edges = Vec::new();
    for (i, zone) in [(0, "left"), (1, "left"), (2, "right"), (3, "right")] {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("e{i}"), "parentId": zone}));
        edges.push(json!({"id": format!("t{i}"), "kind": "connects", "source": format!("e{i}"), "target": "can"}));
    }
    let g: Graph = serde_json::from_value(json!({"nodes": nodes, "edges": edges})).unwrap();
    let open = import_graph_with(g, 100, &LayoutOptions::default()).unwrap();
    let closed =
        arqlet_runtime::toggle_container(&open, open.revision, open.layout_version, "left")
            .unwrap();
    let bus = closed.graph.buses.iter().find(|b| b.id == "can").unwrap();
    let mut nodes: Vec<&str> = bus.taps.iter().map(|t| t.node.as_str()).collect();
    nodes.sort();
    assert_eq!(nodes, ["e2", "e3", "left"]);
    assert!(closed.bus_lines.contains_key("can"));
}

#[test]
fn a_bus_given_as_data_is_drawn_as_lines_too() {
    // No bus node, only `taps`: `lines` draws it as a line (no bar), `bar` as a bar.
    let mut nodes = Vec::new();
    let mut taps = Vec::new();
    for i in 0..4 {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("e{i}")}));
        taps.push(json!({"edge": format!("t{i}"), "node": format!("e{i}"), "toBus": true, "kind": "connects"}));
    }
    let g: Graph = serde_json::from_value(json!({"nodes": nodes, "edges": [],
        "buses": [{"id": "can", "edges": [], "label": "CAN", "kind": "bus", "taps": taps}]}))
    .unwrap();
    let style =
        |s: &str| -> LayoutOptions { serde_json::from_value(json!({"busStyle": s})).unwrap() };
    let bar = import_graph_with(g.clone(), 100, &style("bar")).unwrap();
    assert!(bar.bus_lines.contains_key("can"));
    let lines = import_graph_with(g, 100, &style("lines")).unwrap();
    assert!(lines.bus_lines.is_empty());
    assert_eq!(lines.graph.buses[0].edges.len(), 3);
}

#[test]
fn a_bus_given_as_data_is_crossed_by_trace_and_neighbourhood() {
    // No bus node: the bus is given as members (`Graph.buses` with `taps`), as a host holds bus membership.
    let mut nodes = vec![json!({"id": "gw", "kind": "ecu", "label": "gw"})];
    let mut taps = vec![json!({"edge": "t-gw", "node": "gw", "toBus": true, "kind": "connects"})];
    for i in 0..4 {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("e{i}")}));
        taps.push(json!({"edge": format!("t{i}"), "node": format!("e{i}"), "toBus": true, "kind": "connects"}));
    }
    let g: Graph = serde_json::from_value(json!({"nodes": nodes, "edges": [],
        "buses": [{"id": "can", "edges": [], "label": "CAN", "kind": "bus", "taps": taps}]}))
    .unwrap();
    let s = import_graph_with(g, 100, &LayoutOptions::default()).unwrap();
    assert!(s.bus_lines.contains_key("can"), "drawn as a bar");
    // A path from one member to another runs over the bus.
    let path =
        arqlet_runtime::trace::trace_answer_on(&s, &json!({"from": "e0", "to": "e3"})).unwrap();
    assert_eq!(path.length, Some(2));
    assert_eq!(path.nodes.iter().filter(|n| *n == "can").count(), 1);
    let mut routes = path.visible.routes.clone();
    routes.sort();
    assert_eq!(routes, ["t0", "t3"]);
    // The gateway's neighbourhood within two hops: the bus and every member.
    let around =
        arqlet_runtime::trace::neighbourhood_answer_on(&s, &json!({"nodeId": "gw", "hops": 2}))
            .unwrap();
    assert_eq!(around.nodes.len(), 6, "{:?}", around.nodes);
    // A filter hides the bus by its id, or a member with its tap.
    let run = |v: serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&arqlet_runtime::dispatch(&v.to_string())).unwrap()
    };
    let snapshot = serde_json::to_value(&s).unwrap();
    let filtered = |hidden: serde_json::Value| {
        let answer = run(json!({"op": "filter", "snapshot": snapshot, "hide": hidden,
            "expectedRevision": s.revision, "expectedLayoutVersion": s.layout_version}));
        assert_eq!(answer["ok"], true, "{answer}");
        let scene = run(json!({"op": "scene", "snapshot": answer["snapshot"]}));
        scene["scene"]["busLines"].clone()
    };
    let gone = filtered(json!({"nodes": ["can"]}));
    assert!(gone.as_array().map_or(true, |a| a.is_empty()), "{gone}");
    assert_eq!(
        filtered(json!({"nodes": ["e2"]}))[0]["taps"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}

/// #144: a bus given as data is one shared medium for `neighbourhood` in every bus style, as it is for `trace`.
#[test]
fn neighbourhood_crosses_a_bus_given_as_data_in_every_style() {
    let mut nodes = vec![json!({"id": "gw", "kind": "ecu", "label": "gw"})];
    let mut taps = vec![json!({"edge": "t-gw", "node": "gw", "toBus": true, "kind": "connects"})];
    for i in 0..4 {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("e{i}")}));
        taps.push(json!({"edge": format!("t{i}"), "node": format!("e{i}"), "toBus": true, "kind": "connects"}));
    }
    let g: Graph = serde_json::from_value(json!({"nodes": nodes, "edges": [],
        "buses": [{"id": "can", "edges": [], "label": "CAN", "kind": "bus", "taps": taps}]}))
    .unwrap();
    for style in ["bar", "lines", "trunk"] {
        let options: LayoutOptions = serde_json::from_value(json!({"busStyle": style})).unwrap();
        let s = import_graph_with(g.clone(), 100, &options).unwrap();
        let around =
            arqlet_runtime::trace::neighbourhood_answer_on(&s, &json!({"nodeId": "gw", "hops": 2}))
                .unwrap();
        let mut found = around.nodes.clone();
        found.sort();
        assert_eq!(
            found,
            ["can", "e0", "e1", "e2", "e3", "gw"],
            "{style}: {around:?}"
        );
        // One hop: the bus itself is shown, as its bar or as its lines.
        let near =
            arqlet_runtime::trace::neighbourhood_answer_on(&s, &json!({"nodeId": "gw"})).unwrap();
        let shown =
            near.visible.nodes.contains(&"can".to_string()) || !near.visible.routes.is_empty();
        assert!(
            shown,
            "{style}: the bus is shown at one hop: {:?}",
            near.visible
        );
        if style == "lines" {
            let mut lines = s.graph.buses[0].edges.clone();
            lines.sort();
            assert_eq!(near.visible.routes, lines, "the bus's lines");
        }
        let path =
            arqlet_runtime::trace::trace_answer_on(&s, &json!({"from": "e0", "to": "e3"})).unwrap();
        assert_eq!(path.length, Some(2), "{style}");
    }
}

/// #145: a bus given as data needs no `edges`, and its taps make it a shared medium whatever `edges` holds.
#[test]
fn a_bus_with_taps_is_a_shared_medium_whatever_its_edges_hold() {
    let mut nodes = Vec::new();
    let mut taps = Vec::new();
    for i in 0..4 {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("e{i}")}));
        taps.push(json!({"edge": format!("t{i}"), "node": format!("e{i}"), "toBus": true, "kind": "connects"}));
    }
    // Without `edges`, and with the member edges of an older form (removed from the graph) in `edges`.
    for bus in [
        json!({"id": "can", "kind": "bus", "taps": taps}),
        json!({"id": "can", "kind": "bus", "taps": taps, "edges": ["old0", "old1", "old2"]}),
    ] {
        let g: Graph =
            serde_json::from_value(json!({"nodes": nodes, "edges": [], "buses": [bus]})).unwrap();
        for style in ["bar", "lines"] {
            let options: LayoutOptions =
                serde_json::from_value(json!({"busStyle": style})).unwrap();
            let s = import_graph_with(g.clone(), 100, &options).unwrap();
            let path =
                arqlet_runtime::trace::trace_answer_on(&s, &json!({"from": "e0", "to": "e3"}))
                    .unwrap();
            assert_eq!(path.length, Some(2), "{style}: {path:?}");
            let around = arqlet_runtime::trace::neighbourhood_answer_on(
                &s,
                &json!({"nodeId": "e0", "hops": 2}),
            )
            .unwrap();
            assert_eq!(around.nodes.len(), 5, "{style}: {:?}", around.nodes);
        }
    }
}

/// #149 (first part): a patch keeps a bus given as data, and the taps of the nodes that stay.
#[test]
fn a_patch_keeps_a_bus_given_as_data() {
    let mut nodes = Vec::new();
    let mut taps = Vec::new();
    for i in 0..4 {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("e{i}")}));
        taps.push(json!({"edge": format!("t{i}"), "node": format!("e{i}"), "toBus": true, "kind": "connects"}));
    }
    let g: Graph = serde_json::from_value(
        json!({"nodes": nodes, "edges": [], "buses": [{"id": "can", "kind": "bus", "taps": taps}]}),
    )
    .unwrap();
    for style in ["bar", "lines"] {
        let options: LayoutOptions = serde_json::from_value(json!({"busStyle": style})).unwrap();
        let s = import_graph_with(g.clone(), 100, &options).unwrap();
        let patch: arqlet_model::GraphPatch = serde_json::from_value(json!({
            "nodes": [{"id": "x", "kind": "ecu", "label": "x"}],
            "removeNodes": ["e3"]
        }))
        .unwrap();
        let p = arqlet_runtime::patch::patch_snapshot(&s, s.revision, s.layout_version, &patch)
            .unwrap();
        let bus = p.graph.buses.iter().find(|b| b.id == "can").expect(style);
        let mut members: Vec<&str> = bus.taps.iter().map(|t| t.node.as_str()).collect();
        members.sort();
        assert_eq!(members, ["e0", "e1", "e2"], "{style}");
        let scene = arqlet_scene::build_scene(&p).unwrap();
        if style == "bar" {
            assert!(p.bus_lines.contains_key("can"), "the bar stays");
        } else {
            assert!(
                !scene.junctions.is_empty(),
                "the lines and their junctions stay"
            );
        }
    }
}

#[test]
fn reveal_takes_the_id_of_a_bus_given_as_data() {
    let run = |v: serde_json::Value| -> serde_json::Value {
        serde_json::from_str(&arqlet_runtime::dispatch(&v.to_string())).unwrap()
    };
    // A zone with four ECUs on a bus given as data (no bus node), collapsed at the start; `reveal` by the bus's id opens
    // the zone, so that the line is drawn.
    let mut nodes = vec![
        json!({"id": "zone", "kind": "zone", "label": "zone"}),
        json!({"id": "x", "kind": "ecu", "label": "x"}),
    ];
    let mut taps = Vec::new();
    for i in 0..4 {
        nodes.push(json!({"id": format!("e{i}"), "kind": "ecu", "label": format!("e{i}"), "parentId": "zone"}));
        taps.push(json!({"edge": format!("t{i}"), "node": format!("e{i}"), "toBus": true, "kind": "connects"}));
    }
    let graph = json!({"nodes": nodes, "edges": [],
        "buses": [{"id": "can", "edges": [], "label": "CAN", "kind": "bus", "parentId": "zone", "taps": taps}]});
    let imported = run(
        json!({"op": "import", "graph": graph, "limit": 100, "options": {"levelOfDetail": {"depth": 0}}}),
    );
    assert_eq!(imported["ok"], true, "{imported}");
    let s = imported["snapshot"].clone();
    assert!(
        s["collapsedIds"]
            .as_array()
            .unwrap()
            .contains(&json!("zone")),
        "{}",
        s["collapsedIds"]
    );
    let lines = |snapshot: &serde_json::Value| {
        let scene = run(json!({"op": "scene", "snapshot": snapshot}));
        scene["scene"]["busLines"].as_array().map_or(0, |a| a.len())
    };
    for partial in [false, true] {
        let revealed = run(
            json!({"op": "reveal", "snapshot": s, "nodeIds": ["can"], "partial": partial,
            "expectedRevision": s["revision"], "expectedLayoutVersion": s["layoutVersion"]}),
        );
        assert_eq!(revealed["ok"], true, "partial {partial}: {revealed}");
        let after = &revealed["snapshot"];
        assert!(
            !after["collapsedIds"]
                .as_array()
                .unwrap()
                .contains(&json!("zone")),
            "partial {partial}: zone opened"
        );
        assert_eq!(lines(after), 1, "partial {partial}: the bus line is drawn");
    }
    // An id that is neither a node nor a bus is still refused.
    let unknown = run(
        json!({"op": "reveal", "snapshot": s, "nodeIds": ["nowhere"],
        "expectedRevision": s["revision"], "expectedLayoutVersion": s["layoutVersion"]}),
    );
    assert_eq!(unknown["ok"], false);
}

/// #149: the engine choice does not route a drawing again with both engines when the estimated work is too large (a
/// meshed bus view, organic); it says so, and the conflicts stay reported. The graph is `benchmarks/experiments/bus-mesh.mjs
/// 50 15`.
#[test]
fn the_engine_choice_skips_a_drawing_too_large_to_route_twice() {
    let (units, count) = (50usize, 15usize);
    // The generator's arithmetic in doubles, as JavaScript does it (IEEE, so the same graph).
    let mut seed: f64 = 7.;
    let mut rnd = || {
        seed = (seed * 1103515245. + 12345.) % 2147483648.;
        seed / 2147483648.
    };
    let mut taps: Vec<Vec<serde_json::Value>> = vec![Vec::new(); count];
    let mut nodes = Vec::new();
    for i in 0..units {
        nodes.push(json!({"id": format!("u{i}"), "kind": "unit", "label": format!("U{i}")}));
    }
    for i in 0..units {
        let k = 1 + (rnd() * 3.).floor() as usize;
        for _ in 0..k {
            let b = (rnd() * count as f64).floor() as usize;
            let node = format!("u{i}");
            if !taps[b].iter().any(|t| t["node"] == json!(node)) {
                taps[b].push(json!({"edge": format!("tbus{b}-{i}"), "node": node, "toBus": true, "kind": "on"}));
            }
        }
    }
    let buses: Vec<serde_json::Value> = taps
        .into_iter()
        .enumerate()
        .filter(|(_, t)| !t.is_empty())
        .map(|(b, t)| json!({"id": format!("bus{b}"), "kind": "bus", "label": format!("Bus {b}"), "taps": t}))
        .collect();
    let g: Graph = serde_json::from_value(json!({"nodes": nodes, "edges": [], "buses": buses,
        "defaultStrategy": "organic", "defaultDirection": "auto"}))
    .unwrap();
    let options: LayoutOptions = serde_json::from_value(json!({"busStyle": "lines"})).unwrap();
    let s = import_graph_with(g, 10_000, &options).unwrap();
    assert!(
        s.layout_diagnostics
            .iter()
            .any(|d| d.starts_with("Engine choice not tried")),
        "{:?}",
        s.layout_diagnostics
    );
}

/// #156: `organicGap` widens the gap between the boxes of an organic area (default 24, omitted when unset).
#[test]
fn organic_gap_keeps_boxes_apart_and_is_omitted_by_default() {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for i in 0..12 {
        nodes.push(json!({"id": format!("n{i}"), "kind": "k", "label": format!("N{i}")}));
        if i > 0 {
            edges.push(json!({"id": format!("e{i}"), "kind": "k", "source": format!("n{}", i / 2), "target": format!("n{i}")}));
        }
    }
    let g: Graph = serde_json::from_value(json!({"nodes": nodes, "edges": edges,
        "defaultStrategy": "organic", "defaultDirection": "auto"}))
    .unwrap();
    let gap_of = |s: &Snapshot| {
        let b: Vec<&Rect> = s.boxes.values().collect();
        let mut min = f64::INFINITY;
        for i in 0..b.len() {
            for j in i + 1..b.len() {
                let dx = (b[j].x - (b[i].x + b[i].width)).max(b[i].x - (b[j].x + b[j].width));
                let dy = (b[j].y - (b[i].y + b[i].height)).max(b[i].y - (b[j].y + b[j].height));
                min = min.min(dx.max(dy));
            }
        }
        min
    };
    let plain = import_graph_with(g.clone(), 100, &LayoutOptions::default()).unwrap();
    assert!(!serde_json::to_string(&plain.options)
        .unwrap()
        .contains("organicGap"));
    let wide: LayoutOptions = serde_json::from_value(json!({"organicGap": 60})).unwrap();
    let s = import_graph_with(g, 100, &wide).unwrap();
    assert!(
        gap_of(&s) >= 60. - 1e-6,
        "every two boxes at least 60 apart: {}",
        gap_of(&s)
    );
    assert!(
        gap_of(&plain) < 60.,
        "the default packs tighter: {}",
        gap_of(&plain)
    );
}

/// Areas of `units` units on buses given as data, each area on one bus and every `gateway`-th on two (the generator of
/// `benchmarks/experiments/bus-collapse.mjs`), organic.
fn areas(containers: usize, units: usize, buses: usize, gateway: usize) -> Graph {
    // In floating point, as the script's numbers are.
    let mut seed = 7_f64;
    let mut rnd = |n: usize| {
        seed = (seed * 1_103_515_245. + 12_345.) % 2_147_483_648.;
        (seed / 2_147_483_648. * n as f64) as usize
    };
    let mut nodes = Vec::new();
    let mut taps = vec![Vec::new(); buses];
    let tap = |b: usize, node: String| json!({"edge": format!("t{b}-{node}"), "node": node, "toBus": true, "kind": "on"});
    for c in 0..containers {
        let home = rnd(buses);
        let two = c % gateway == 0;
        nodes.push(
            json!({"id": format!("c{c}"), "kind": if two { "gateway" } else { "single" },
            "label": format!("Area {c}")}),
        );
        for u in 0..units {
            nodes.push(
                json!({"id": format!("c{c}u{u}"), "kind": "unit", "label": format!("U{c}.{u}"),
                "parentId": format!("c{c}")}),
            );
            taps[home].push(tap(home, format!("c{c}u{u}")));
        }
        if two {
            let other = (home + 1 + rnd(buses - 1)) % buses;
            taps[other].push(tap(other, format!("c{c}u0")));
        }
    }
    let buses: Vec<serde_json::Value> = taps
        .into_iter()
        .enumerate()
        .filter(|(_, t)| !t.is_empty())
        .map(|(b, t)| json!({"id": format!("bus{b}"), "kind": "bus", "label": format!("Bus {b}"), "taps": t}))
        .collect();
    serde_json::from_value(json!({"nodes": nodes, "edges": [], "buses": buses,
        "defaultStrategy": "organic", "defaultDirection": "auto"}))
    .unwrap()
}

/// #150: where every bus becomes lines, nothing draws or reads the routes of the taps, and `compute` does not route the
/// ones in conflict a second time. `compute_compared` leaves nothing out, so the two show what the step changes: the
/// routes kept for the taps, and no part of the drawing. Wherever those routes could reach the result, the two are equal.
#[test]
fn taps_of_bus_lines_are_not_routed_twice_and_nothing_drawn_changes() {
    let both = |graph: Graph, options: serde_json::Value| {
        let options: LayoutOptions = serde_json::from_value(options).unwrap();
        let view = import_graph_with(graph, 10_000, &options).unwrap();
        let none = std::collections::BTreeSet::new();
        let plain = arqlet_layout::compute(&view.graph, None, &none, &view.options).unwrap();
        let complete =
            arqlet_layout::compute_compared(&view.graph, None, &none, &view.options).unwrap();
        (plain, complete)
    };
    let collapsed = json!({"busStyle": "lines", "levelOfDetail": {"kinds": ["single"]}});
    let large = || areas(24, 4, 3, 2);
    let (plain, complete) = both(large(), collapsed.clone());
    assert!(plain.bus_lines.is_empty(), "every bus is drawn as lines");
    // This view has routes in conflict that routing again improves: the kept tap routes differ.
    let routes = |s: &Snapshot| serde_json::to_value(&s.bus_taps.routes).unwrap();
    assert_ne!(routes(&plain), routes(&complete));
    assert_eq!(plain.bus_taps.routes.len(), complete.bus_taps.routes.len());
    assert_eq!(plain.bus_taps.roots, complete.bus_taps.roots);
    // Everything else is the same, but for the report of the containers routed again.
    let rest = |s: &Snapshot| {
        let mut v = serde_json::to_value(s).unwrap();
        let map = v.as_object_mut().unwrap();
        map.remove("busTaps");
        map.remove("recomputedContainers");
        v
    };
    assert_eq!(rest(&plain), rest(&complete));

    let same = |graph: Graph, options: serde_json::Value, why: &str| {
        let (plain, complete) = both(graph, options);
        assert_eq!(
            serde_json::to_value(&plain).unwrap(),
            serde_json::to_value(&complete).unwrap(),
            "{why}"
        );
    };
    // A bar's tap routes are restored for later operations.
    let mut bar = collapsed.clone();
    bar["busStyle"] = json!("bar");
    same(large(), bar, "bars");
    // Few boxes: the engine choice may run, and it judges the drawing with the tap routes.
    same(areas(16, 3, 3, 2), collapsed.clone(), "engine choice");
    // A label beside a tap's route is placed along it.
    let mut labelled = collapsed;
    let options: LayoutOptions = serde_json::from_value(labelled.clone()).unwrap();
    let first = import_graph_with(large(), 10_000, &options).unwrap();
    let tap = first.graph.edges[0].id.clone();
    labelled["edgeLabels"] = json!({tap: {"text": "tap", "width": 20.0, "height": 10.0}});
    same(large(), labelled, "an edge label");
}

/// A bus of an organic area becomes lines of its own, while a bus with one tap stays a bar. What the layout had kept for
/// the taps of the first must not come back with the second's: the head's tap has no edge (`refine` panicked on it).
#[test]
fn expanding_a_view_restores_only_the_taps_of_its_bars() {
    let tap = |bus: &str, node: &str| json!({"edge": format!("{bus}-{node}"), "node": node, "toBus": true, "kind": "on"});
    let nodes: Vec<_> = ["a", "b", "c", "d"]
        .iter()
        .map(|id| json!({"id": id, "kind": "unit", "label": id}))
        .collect();
    let wide: Vec<_> = ["a", "b", "c"].iter().map(|n| tap("wide", n)).collect();
    let lone = vec![tap("lone", "d")];
    let graph: Graph = serde_json::from_value(json!({
        "defaultStrategy": "organic",
        "defaultDirection": "auto",
        "nodes": nodes,
        "edges": [],
        "buses": [
            {"id": "wide", "kind": "bus", "label": "Wide", "taps": wide},
            {"id": "lone", "kind": "bus", "label": "Lone", "taps": lone},
        ],
    }))
    .unwrap();
    let options = LayoutOptions {
        bus_style: arqlet_model::BusStyle::Lines,
        ..Default::default()
    };
    let snapshot = import_graph_with(graph, 100, &options).unwrap();
    // The case at hand: one bus drawn as lines (edges of the view), one as a bar.
    assert!(!snapshot.graph.edges.is_empty());
    assert!(snapshot.bus_lines.contains_key("lone") && !snapshot.bus_lines.contains_key("wide"));

    let mut expanded = snapshot.clone();
    assert!(arqlet_layout::expand_bus_lines(&mut expanded));
    let edges: std::collections::BTreeSet<&str> =
        expanded.graph.edges.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(edges.len(), expanded.graph.edges.len());
    let mut routed = std::collections::BTreeSet::new();
    for route in &expanded.routes {
        assert!(
            edges.contains(route.id.as_str()),
            "no edge for {}",
            route.id
        );
        assert!(routed.insert(route.id.as_str()), "twice: {}", route.id);
    }
    assert_eq!(routed, edges);
    for port in &expanded.resolved_ports {
        assert!(
            edges.contains(port.edge_id.as_str()),
            "port of {}",
            port.edge_id
        );
    }
}
