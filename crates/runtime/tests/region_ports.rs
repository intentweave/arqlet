//! Experimental `regionPorts` (ADR 0020, S1-2): an edge that leaves a region of another strategy in a layered parent is
//! routed by the parent to a port on the region's border, on the side the flow uses, and drawn inside the region from
//! there to its node.
use arqlet_model::{Point, Rect, Snapshot};
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

/// Stages `a` and `b` flowing left to right around region `r` of `strategy`, its children wired as a chain with a few
/// chords; every stage node has an edge into the region or out of it, and one edge runs back into it.
fn graph(strategy: &str) -> Value {
    let mut nodes = vec![json!({"id": "r", "kind": "group", "label": "r"})];
    let mut edges = Vec::new();
    for k in 0..7 {
        nodes.push(json!({"id": format!("r.{k}"), "kind": "fn", "label": format!("r{k}"), "parentId": "r"}));
        if k > 0 {
            edges.push(json!({"id": format!("i{k}"), "kind": "calls", "source": format!("r.{}", (k - 1) / 2), "target": format!("r.{k}")}));
        }
    }
    for j in 0..3 {
        nodes.push(json!({"id": format!("a{j}"), "kind": "svc", "label": format!("a{j}")}));
        nodes.push(json!({"id": format!("b{j}"), "kind": "svc", "label": format!("b{j}")}));
        edges.push(json!({"id": format!("in{j}"), "kind": "calls", "source": format!("a{j}"), "target": format!("r.{}", 2 * j + 1)}));
        edges.push(json!({"id": format!("out{j}"), "kind": "calls", "source": format!("r.{}", 6 - 2 * j), "target": format!("b{j}")}));
    }
    edges.push(json!({"id": "back", "kind": "calls", "source": "b0", "target": "r.3"}));
    let mut g = json!({"nodes": nodes, "edges": edges, "defaultDirection": "lr",
        "containerStrategies": {"r": strategy}});
    if strategy != "tree" {
        g["containerDirections"] = json!({"r": "auto"});
        g["containerRouting"] = json!({"r": "straight"});
    }
    g
}

fn import(graph: &Value, options: Value) -> Snapshot {
    let r = call(json!({"op": "import", "graph": graph, "limit": 100, "options": options}));
    assert_eq!(r["ok"], true, "{r}");
    serde_json::from_value(r["snapshot"].clone()).unwrap()
}

fn on_box(p: &Point, r: &Rect) -> bool {
    p.x >= r.x - 0.5
        && p.x <= r.x + r.width + 0.5
        && p.y >= r.y - 0.5
        && p.y <= r.y + r.height + 0.5
}

#[test]
fn edges_leave_a_region_through_ports_on_the_sides_of_the_flow() {
    for strategy in ["organic", "tree", "radial"] {
        let g = graph(strategy);
        for base in [json!({}), json!({"singlePassMixed": true})] {
            let mut options = base.clone();
            options["regionPorts"] = json!(true);
            let s = import(&g, options);
            let m = arqlet_layout::published_metrics(&s).unwrap();
            assert_eq!(
                m.line_node_hits, 0,
                "{strategy} {base}: no line through a node"
            );
            let issues = arqlet_scene::validate_geometry(&s.graph, &s.boxes, Default::default());
            assert!(issues.is_empty(), "{strategy}: {issues:?}");
            let abs = arqlet_layout::absolute_all(&s.graph, &s.boxes);
            let region = &abs["r"];
            assert_eq!(s.routes.len(), s.graph.edges.len(), "every edge is routed");
            for e in &s.graph.edges {
                let route = s.routes.iter().find(|r| r.id == e.id).unwrap();
                let target = e.target.as_ref().unwrap();
                // In logical direction: from the source's box to the target's box.
                assert!(
                    on_box(&route.points[0], &abs[&e.source]),
                    "{} leaves {}",
                    e.id,
                    e.source
                );
                assert!(
                    on_box(route.points.last().unwrap(), &abs[target]),
                    "{} reaches {target}",
                    e.id
                );
                // An edge that leaves the region passes the side that faces its other end along the flow: the left side
                // when that end lies left of the region, the right side when it lies right of it.
                let outer = match (e.source.starts_with("r."), target.starts_with("r.")) {
                    (true, false) => target,
                    (false, true) => &e.source,
                    _ => continue,
                };
                let o = &abs[outer];
                let side = |x: f64| {
                    route.points.iter().any(|p| {
                        (p.x - x).abs() < 0.5 && p.y > region.y && p.y < region.y + region.height
                    })
                };
                if o.x + o.width <= region.x {
                    assert!(side(region.x), "{strategy}: {} passes the left side", e.id);
                } else if o.x >= region.x + region.width {
                    assert!(
                        side(region.x + region.width),
                        "{strategy}: {} passes the right side",
                        e.id
                    );
                }
            }
            // The snapshot keeps the given graph, and a toggle continues from it.
            assert_eq!(
                s.graph
                    .edges
                    .iter()
                    .find(|e| e.id == "in0")
                    .unwrap()
                    .target
                    .as_deref(),
                Some("r.1")
            );
            assert!(s.graph.ports.is_empty() && s.graph.edge_ports.is_empty());
            let v = serde_json::to_value(&s).unwrap();
            let toggled = call(json!({"op": "toggle", "snapshot": v, "nodeId": "r",
                "expectedRevision": v["revision"], "expectedLayoutVersion": v["layoutVersion"]}));
            assert_eq!(toggled["ok"], true, "{toggled}");
        }
    }
    // Off: nothing changes in the serialized options.
    let plain = import(&graph("organic"), json!({}));
    assert!(!serde_json::to_string(&plain.options)
        .unwrap()
        .contains("regionPorts"));
}

/// Four units in two containers of `strategy` on a layered top level, and one bus line that taps three of them (#204).
fn bus_graph(strategy: &str) -> Value {
    let mut g = json!({
        "nodes": [
            {"id": "c1", "kind": "area", "label": "C1"}, {"id": "c2", "kind": "area", "label": "C2"},
            {"id": "a", "kind": "unit", "label": "a", "parentId": "c1"},
            {"id": "b", "kind": "unit", "label": "b", "parentId": "c1"},
            {"id": "c", "kind": "unit", "label": "c", "parentId": "c2"},
            {"id": "d", "kind": "unit", "label": "d", "parentId": "c2"}],
        "edges": [],
        "buses": [{"id": "x", "kind": "bus", "label": "X", "taps": [
            {"edge": "t-a", "node": "a", "toBus": true, "kind": "on"},
            {"edge": "t-c", "node": "c", "toBus": true, "kind": "on"},
            {"edge": "t-d", "node": "d", "toBus": true, "kind": "on"}]}],
        "containerStrategies": {"c1": strategy, "c2": strategy}});
    if strategy != "tree" {
        g["containerDirections"] = json!({"c1": "auto", "c2": "auto"});
    }
    g
}

/// The bar of bus `x` with a tap from every node of `taps` to it, and none of them through a node.
fn check_bar(s: &Snapshot, taps: &[&str], context: &str) {
    let abs = arqlet_layout::absolute_all(&s.graph, &s.boxes);
    let line = s
        .bus_lines
        .get("x")
        .unwrap_or_else(|| panic!("{context}: a bar"));
    assert_eq!(line.taps.len(), taps.len(), "{context}");
    for node in taps {
        let points = &line.taps[&format!("t-{node}")];
        assert!(on_box(&points[0], &abs[*node]), "{context}: leaves {node}");
        assert!(
            on_box(points.last().unwrap(), &line.bar),
            "{context}: {node} reaches the bar"
        );
    }
    let metrics = arqlet_layout::published_metrics(s).unwrap();
    assert_eq!(metrics.line_node_hits, 0, "{context}");
    let issues = arqlet_scene::validate_geometry(&s.graph, &s.boxes, Default::default());
    assert!(issues.is_empty(), "{context}: {issues:?}");
}

/// `request` on `s`: the next snapshot.
fn next(s: &Snapshot, mut request: Value) -> Snapshot {
    request["snapshot"] = serde_json::to_value(s).unwrap();
    request["expectedRevision"] = json!(s.revision);
    request["expectedLayoutVersion"] = json!(s.layout_version);
    let r = call(request.clone());
    assert_eq!(r["ok"], true, "{}: {}", request["op"], r["error"]);
    serde_json::from_value(r["snapshot"].clone()).unwrap()
}

#[test]
fn a_bus_line_taps_nodes_inside_regions() {
    for strategy in ["organic", "tree", "radial", "octilinear"] {
        for style in ["bar", "trunk"] {
            let context = format!("{strategy} {style}");
            let s = import(&bus_graph(strategy), json!({"busStyle": style}));
            check_bar(&s, &["a", "c", "d"], &context);
            // Configure, direction, move and toggle continue from the snapshot.
            let s = next(
                &s,
                json!({"op": "configure", "options": {"busStyle": style, "nodeSpacing": 40}}),
            );
            check_bar(&s, &["a", "c", "d"], &context);
            let s = next(
                &s,
                json!({"op": "direction", "nodeId": null, "direction": "tb"}),
            );
            check_bar(&s, &["a", "c", "d"], &context);
            let at = s.boxes["c1"].clone();
            let mut s = s;
            for (seq, phase) in [(1, "begin"), (2, "update"), (3, "finish")] {
                s = next(
                    &s,
                    json!({"op": "move", "gestureId": "g", "seq": seq, "phase": phase, "node": "c1",
                        "position": {"x": at.x + 60., "y": at.y + 40.}}),
                );
            }
            check_bar(&s, &["a", "c", "d"], &context);
            let closed = next(&s, json!({"op": "toggle", "nodeId": "c1"}));
            assert!(closed.bus_lines.contains_key("x"), "{context}");
            let open = next(&closed, json!({"op": "toggle", "nodeId": "c1"}));
            check_bar(&open, &["a", "c", "d"], &context);
        }
    }
}

#[test]
fn a_bus_line_inside_one_region_does_not_stop_the_edges_between_regions() {
    let mut g = bus_graph("organic");
    g["buses"][0] = json!({"id": "x", "kind": "bus", "label": "X", "parentId": "c1", "taps": [
        {"edge": "t-a", "node": "a", "toBus": true, "kind": "on"},
        {"edge": "t-b", "node": "b", "toBus": true, "kind": "on"}]});
    g["edges"] = json!([{"id": "e", "kind": "uses", "source": "a", "target": "c"}]);
    let s = import(&g, json!({}));
    check_bar(&s, &["a", "b"], "bus inside c1");
    assert_eq!(s.routes.len(), 1);
    let abs = arqlet_layout::absolute_all(&s.graph, &s.boxes);
    assert!(on_box(&s.routes[0].points[0], &abs["a"]));
    assert!(on_box(s.routes[0].points.last().unwrap(), &abs["c"]));
}

#[test]
fn a_bus_line_in_a_session_survives_an_edit() {
    let mut store = arqlet_runtime::session_store::SessionStore::with_limit(None);
    let mut ask = |m: Value| -> Value { serde_json::from_str(&store.handle(&m)).unwrap() };
    let id = ask(json!({"session": "open"}))["session"].clone();
    let imported = ask(json!({"session": id, "request":
        {"op": "import", "graph": bus_graph("organic"), "limit": 100, "options": {"busStyle": "bar"}}}));
    assert_eq!(imported["ok"], true, "{imported}");
    let toggled = ask(
        json!({"session": id, "request": {"op": "toggle", "nodeId": "c1",
        "expectedRevision": imported["snapshot"]["revision"],
        "expectedLayoutVersion": imported["snapshot"]["layoutVersion"]}}),
    );
    assert_eq!(toggled["ok"], true, "{toggled}");
}

/// A region's box is no hub: its edges are those of the nodes inside.
#[test]
fn hub_references_are_taken_from_the_nodes_not_from_a_region() {
    let mut nodes = vec![json!({"id": "r", "kind": "group", "label": "r"})];
    let mut edges = Vec::new();
    for k in 0..3 {
        nodes.push(json!({"id": format!("r.{k}"), "kind": "fn", "label": format!("r{k}"), "parentId": "r"}));
        nodes.push(json!({"id": format!("o{k}"), "kind": "fn", "label": format!("o{k}")}));
        edges.push(json!({"id": format!("e{k}"), "kind": "calls", "source": format!("r.{k}"), "target": format!("o{k}")}));
    }
    let plain = json!({"nodes": nodes, "edges": edges, "containerStrategies": {"r": "organic"},
        "containerDirections": {"r": "auto"}});
    let s = import(&plain, json!({"hubRefs": 3}));
    assert!(s.hub_refs.is_empty(), "{:?}", s.hub_refs);
    assert_eq!(s.routes.len(), 3, "every edge is routed");
    // With a hub elsewhere the room for the stubs stays, also though an edge leaves the region.
    let mut with_hub = plain;
    with_hub["nodes"]
        .as_array_mut()
        .unwrap()
        .push(json!({"id": "hub", "kind": "fn", "label": "a hub with a long name"}));
    for k in 0..3 {
        with_hub["edges"].as_array_mut().unwrap().push(
            json!({"id": format!("h{k}"), "kind": "calls", "source": format!("o{k}"), "target": "hub"}),
        );
    }
    let s = import(&with_hub, json!({"hubRefs": 3}));
    assert_eq!(s.hub_refs.len(), 3);
    assert!(s.hub_refs.iter().all(|r| r.hub == "hub"));
    assert_eq!(s.routes.len(), 3);
    assert!(s.options.hub_ref_room > 0.);
}
