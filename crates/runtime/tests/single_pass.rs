use arqlet_model::{Graph, LayoutOptions, Snapshot};
use arqlet_runtime::{dispatch, import_graph_counted, import_graph_with, refine_snapshot_job};
use serde_json::{json, Value};

fn options() -> LayoutOptions {
    LayoutOptions {
        abstract_routing: true,
        single_pass: true,
        ..Default::default()
    }
}

fn small() -> Graph {
    serde_json::from_str(include_str!("../../../examples/fixtures/small.json")).unwrap()
}

/// Two containers with edges between them in both directions (one runs backward between the containers), a parallel
/// pair, a cycle inside a container and a self-loop.
fn nested() -> Graph {
    let node = |id: &str, parent: Option<&str>| json!({"id": id, "kind": "symbol", "parentId": parent, "label": id});
    let edge =
        |id: &str, s: &str, t: &str| json!({"id": id, "kind": "calls", "source": s, "target": t});
    serde_json::from_value(json!({
        "nodes": [
            node("p", None), node("q", None),
            node("p1", Some("p")), node("p2", Some("p")), node("p3", Some("p")),
            node("q1", Some("q")), node("q2", Some("q")), node("q3", Some("q")),
        ],
        "edges": [
            edge("e1", "p1", "p2"), edge("e2", "p2", "p3"), edge("e3", "p3", "p1"),
            edge("e4", "p1", "q1"), edge("e5", "p2", "q2"), edge("e6", "p3", "q3"),
            edge("e7", "p3", "q1"), edge("e8", "p3", "q1"),
            edge("e9", "q3", "p1"), edge("e10", "q1", "q2"), edge("e11", "q2", "q2"),
        ],
    }))
    .unwrap()
}

fn check(snapshot: &Snapshot) {
    assert!(
        snapshot.geometry_issues.is_empty(),
        "{:?}",
        snapshot.geometry_issues
    );
    let edges: Vec<_> = snapshot
        .graph
        .edges
        .iter()
        .filter(|e| e.target.is_some())
        .collect();
    assert_eq!(snapshot.routes.len(), edges.len());
    for edge in edges {
        let route = snapshot.routes.iter().find(|r| r.id == edge.id).unwrap();
        assert!(route.points.len() >= 2);
        // The logical direction is kept: the route starts at the source's port and ends at the target's.
        for (endpoint, node, point) in [
            ("source", &edge.source, route.points.first().unwrap()),
            (
                "target",
                edge.target.as_ref().unwrap(),
                route.points.last().unwrap(),
            ),
        ] {
            let port = snapshot
                .resolved_ports
                .iter()
                .find(|p| p.edge_id == edge.id && p.endpoint == endpoint)
                .unwrap();
            assert_eq!(&port.node_id, node);
            assert!(
                (port.position.x - point.x).abs() < 1e-6
                    && (port.position.y - point.y).abs() < 1e-6
            );
        }
    }
    assert!(snapshot
        .layout_diagnostics
        .iter()
        .any(|d| d.starts_with("Single pass:")));
    arqlet_scene::build_scene(snapshot).unwrap();
}

#[test]
fn single_pass_requires_abstract_routing() {
    let result: Value = serde_json::from_str(&dispatch(
        &json!({"op":"import","graph":small(),
            "options":{"singlePass":true,"abstractRouting":false,"singlePassAuto":false},"limit":1000})
            .to_string(),
    ))
    .unwrap();
    assert_eq!(result["ok"], false, "{result}");
    assert!(serde_json::to_value(LayoutOptions::default())
        .unwrap()
        .get("singlePass")
        .is_none());
}

#[test]
fn single_pass_lays_out_cycles_self_loops_parallel_and_nested_edges() {
    for graph in [small(), nested()] {
        for direction in ["lr", "tb"] {
            let mut value = serde_json::to_value(&graph).unwrap();
            value["defaultDirection"] = json!(direction);
            let graph: Graph = serde_json::from_value(value).unwrap();
            let first = import_graph_with(graph.clone(), 1000, &options()).unwrap();
            check(&first);
            let again = import_graph_with(graph, 1000, &options()).unwrap();
            assert_eq!(
                serde_json::to_value(&first.routes).unwrap(),
                serde_json::to_value(&again.routes).unwrap()
            );
        }
    }
}

#[test]
fn single_pass_knows_backward_borders_before_the_first_placement() {
    let (snapshot, work) = import_graph_counted(nested(), 1000, &options()).unwrap();
    check(&snapshot);
    // The backward edge between the containers moves its border nodes. Its reversal is known before the first placement,
    // so the first pass only orders, no pass rebuilds for the backward edge, and the second pass is the only one with
    // geometry.
    assert_eq!(work.placement_pass_reasons.get("initial_order"), Some(&1));
    assert!(!work
        .placement_pass_reasons
        .contains_key("backward_edge_rebuild"));
    assert!(!work
        .placement_pass_reasons
        .contains_key("backward_edge_hierarchical_ordering_rebuild"));
    assert_eq!(work.placement_pass_reasons.values().sum::<usize>(), 2);
}

#[test]
fn single_pass_refinement_evaluates_no_candidates_for_any_thread_count() {
    let input = import_graph_with(nested(), 1000, &options()).unwrap();
    let mut results = Vec::new();
    for threads in [1, 4] {
        let (result, report) = refine_snapshot_job(
            "",
            &input,
            input.revision,
            input.layout_version,
            threads,
            None,
            None,
        )
        .unwrap();
        assert!(report.candidate_work.is_empty(), "{:?}", report.completed);
        assert!(report
            .excluded
            .iter()
            .all(|e| e.reason == "singlePass constructs once"));
        // No global routing rounds and no Optimize-time crossing step: the constructed drawing is delivered unchanged.
        assert_eq!(report.counters.routing_rounds, 0);
        assert_eq!(
            serde_json::to_value((&result.boxes, &result.routes)).unwrap(),
            serde_json::to_value((&input.boxes, &input.routes)).unwrap()
        );
        results.push(serde_json::to_value(&result.routes).unwrap());
    }
    assert_eq!(results[0], results[1]);
}

fn run(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn import(graph: &Graph, options: Value) -> Value {
    let reply = run(json!({"op": "import", "graph": graph, "limit": 1000, "options": options}));
    assert_eq!(reply["ok"], true, "{reply}");
    reply["snapshot"].clone()
}

fn on(snapshot: &Value, mut request: Value) -> Value {
    request["snapshot"] = snapshot.clone();
    request["expectedRevision"] = snapshot["revision"].clone();
    request["expectedLayoutVersion"] = json!(snapshot["layoutVersion"].as_u64().unwrap_or(0));
    run(request)
}

/// A snapshot without what the option itself adds: the option, and the report of the single pass that failed.
fn drawing_of(snapshot: &Value) -> Value {
    let mut drawing = snapshot.clone();
    let map = drawing.as_object_mut().unwrap();
    map.remove("options");
    let kept: Vec<_> = map
        .remove("layoutDiagnostics")
        .and_then(|d| d.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|d| !d.as_str().unwrap().starts_with("Single pass was not used:"))
        .collect();
    if !kept.is_empty() {
        map.insert("layoutDiagnostics".into(), Value::Array(kept));
    }
    drawing
}

/// Single pass made the drawing: both options on, which since engine 0.2 is the default and omitted (ADR 0016).
fn on_single_pass(snapshot: &Value) -> bool {
    snapshot["options"]["singlePass"] != false && snapshot["options"]["abstractRouting"] != false
}

fn reported_fallback(snapshot: &Value) -> bool {
    snapshot["layoutDiagnostics"].as_array().is_some_and(|d| {
        d.iter()
            .any(|d| d.as_str().unwrap().starts_with("Single pass was not used:"))
    })
}

/// `nested` with the container p flowing top to bottom in the left-to-right root: an area of the other axis (ADR 0017).
fn mixed_axes() -> Graph {
    let mut mixed = serde_json::to_value(nested()).unwrap();
    mixed["containerDirections"] = json!({"p": "tb"});
    serde_json::from_value(mixed).unwrap()
}

/// Graphs single pass rejects: areas that are not layered.
fn rejected() -> Vec<(&'static str, Graph)> {
    vec![(
        "mixed layouts",
        serde_json::from_str(include_str!(
            "../../../examples/fixtures/mixed-layouts.json"
        ))
        .unwrap(),
    )]
}

/// Mixed axes go to single pass under the automatic choice, as under an explicit single pass (every mixed dataset is better
/// than the refined default over five seeds, benchmarks/results/single-pass-idle-20261108.md).
#[test]
fn auto_takes_mixed_axes() {
    let graph = mixed_axes();
    let snapshot = import(&graph, json!({"singlePassAuto": true}));
    assert!(
        on_single_pass(&snapshot),
        "{}",
        snapshot["layoutDiagnostics"]
    );
    assert!(!reported_fallback(&snapshot));
    let explicit = import(&graph, json!({"abstractRouting": true, "singlePass": true}));
    assert_eq!(drawing_of(&snapshot), drawing_of(&explicit));
}

#[test]
fn auto_uses_single_pass_where_it_works_and_keeps_the_option() {
    let snapshot = import(&nested(), json!({"singlePassAuto": true}));
    assert!(on_single_pass(&snapshot));
    // On by default since engine 0.2, so not written (ADR 0016).
    assert_ne!(snapshot["options"]["singlePassAuto"], false);
    assert!(!reported_fallback(&snapshot));
    let explicit = import(
        &nested(),
        json!({"abstractRouting": true, "singlePass": true}),
    );
    assert_eq!(drawing_of(&snapshot), drawing_of(&explicit));
}

#[test]
fn auto_falls_back_to_exactly_the_default_drawing_and_says_why() {
    for (name, graph) in rejected() {
        let snapshot = import(&graph, json!({"singlePassAuto": true}));
        assert!(!on_single_pass(&snapshot), "{name}");
        assert!(reported_fallback(&snapshot), "{name}");
        assert_eq!(
            drawing_of(&snapshot),
            drawing_of(&import(&graph, json!({}))),
            "{name}"
        );
    }
}

#[test]
fn explicit_single_pass_lays_out_mixed_axes_in_each_areas_own_flow() {
    let graph = mixed_axes();
    let snapshot = import(&graph, json!({"abstractRouting": true, "singlePass": true}));
    let plans = &snapshot["layeredPlans"];
    assert_ne!(plans["<root>"]["vertical"], plans["p"]["vertical"]);
    // The containers do not overlap, although p grew across the flow of the root.
    let (p, q) = (&snapshot["boxes"]["p"], &snapshot["boxes"]["q"]);
    let apart = |a: &Value, b: &Value, x: &str, w: &str| {
        a[x].as_f64().unwrap() + a[w].as_f64().unwrap() <= b[x].as_f64().unwrap() + 1e-6
            || b[x].as_f64().unwrap() + b[w].as_f64().unwrap() <= a[x].as_f64().unwrap() + 1e-6
    };
    assert!(
        apart(p, q, "x", "width") || apart(p, q, "y", "height"),
        "{p} {q}"
    );
    assert!(
        snapshot["geometryIssues"]
            .as_array()
            .is_none_or(|i| i.is_empty()),
        "{}",
        snapshot["geometryIssues"]
    );
}

#[test]
fn without_auto_an_unsupported_request_stays_an_explicit_error() {
    // Mixed axes run under an explicit single pass (`auto_takes_mixed_axes`); the others stay errors.
    for (name, graph) in rejected() {
        let reply = run(json!({"op": "import", "graph": graph, "limit": 1000,
            "options": {"abstractRouting": true, "singlePass": true, "singlePassAuto": false}}));
        assert_eq!(reply["ok"], false, "{name}: {reply}");
    }
}

fn edits(snapshot: &Value) -> Vec<(&'static str, Value)> {
    let intent = on(snapshot, json!({"op": "intent"}));
    assert_eq!(intent["ok"], true, "{intent}");
    vec![
        (
            "add",
            json!({"op": "add", "sourceId": "q3", "label": "Added"}),
        ),
        ("toggle", json!({"op": "toggle", "nodeId": "q"})),
        (
            "direction",
            json!({"op": "direction", "nodeId": "q", "direction": "tb"}),
        ),
        (
            "routing",
            json!({"op": "routing", "nodeId": "q", "routing": "straight"}),
        ),
        (
            "strategy",
            json!({"op": "strategy", "nodeId": "q", "strategy": "organic"}),
        ),
        (
            "shape",
            json!({"op": "shape", "nodeId": "q1", "shape": "circle"}),
        ),
        (
            "measure",
            json!({"op": "measure", "sizes": {"q1": {"width": 260, "height": 90}}}),
        ),
        (
            "port",
            json!({"op": "port", "edgeId": "e4", "endpoint": "source", "side": "right", "offset": 10}),
        ),
        (
            "pin",
            json!({"op": "configure", "options": {"pins": {"q1": {"x": 40, "y": 60}}}}),
        ),
        (
            "grid",
            json!({"op": "configure", "options": {"placementGrid": 8}}),
        ),
        ("settle", json!({"op": "settle"})),
        (
            "restore",
            json!({"op": "restore", "intent": intent["intent"]}),
        ),
        ("refine", json!({"op": "refine", "threads": 1})),
        (
            "refineProgressive",
            json!({"op": "refineProgressive", "threads": 1}),
        ),
        ("structuredRefine", json!({"op": "structuredRefine"})),
    ]
}

#[test]
fn every_edit_that_the_default_accepts_works_on_a_single_pass_drawing() {
    let tried = import(&nested(), json!({"singlePassAuto": true}));
    // The pipeline before single pass, which accepted these edits first.
    let plain = import(
        &nested(),
        json!({"singlePassAuto": false, "singlePass": false, "abstractRouting": false}),
    );
    assert!(on_single_pass(&tried));
    for (name, request) in edits(&tried) {
        let reply = on(&tried, request.clone());
        let baseline = on(&plain, request);
        assert_eq!(
            reply["ok"], baseline["ok"],
            "{name}: {reply} against {baseline}"
        );
        let Some(after) = reply.get("snapshot") else {
            continue;
        };
        // Whichever pipeline drew it, the next edit and the refinement go on from that drawing.
        let next = on(
            after,
            json!({"op": "add", "sourceId": "p2", "label": "Next"}),
        );
        assert_eq!(next["ok"], true, "{name}, then add: {next}");
        let refined = on(after, json!({"op": "refine", "threads": 1}));
        assert_eq!(refined["ok"], true, "{name}, then refine: {refined}");
        assert_ne!(after["options"]["singlePassAuto"], false, "{name}");
    }
}

#[test]
fn an_edit_out_of_single_pass_stays_on_the_other_pipeline_until_configured_again() {
    let snapshot = import(&nested(), json!({"singlePassAuto": true}));
    let pin = json!({"op": "configure", "options": {"pins": {"q1": {"x": 40, "y": 60}}}});
    let pinned = on(&snapshot, pin)["snapshot"].clone();
    assert!(!on_single_pass(&pinned));
    assert!(reported_fallback(&pinned));
    // Without the pin single pass would work again, but an edit does not change the pipeline back.
    let release = json!({"op": "configure", "options": {"pins": {}}});
    let released = on(&pinned, release)["snapshot"].clone();
    assert!(!on_single_pass(&released));
    let again = on(
        &released,
        json!({"op": "configure", "options": {"singlePassAuto": true}}),
    );
    assert_eq!(again["ok"], true, "{again}");
    assert!(on_single_pass(&again["snapshot"]));
}

#[test]
fn a_deliberate_move_and_a_pin_hold_on_a_single_pass_drawing() {
    let snapshot = import(&nested(), json!({"singlePassAuto": true}));
    let target = json!({"x": 300.0, "y": 200.0});
    let mut at = snapshot.clone();
    for (seq, phase) in [(1, "begin"), (2, "update"), (3, "finish")] {
        let reply = on(
            &at,
            json!({"op": "move", "gestureId": "g", "seq": seq, "phase": phase,
            "node": "q1", "position": target, "release": false}),
        );
        assert_eq!(reply["ok"], true, "{phase}: {reply}");
        at = reply["snapshot"].clone();
    }
    assert_eq!(at["boxes"]["q1"]["x"], 300.0, "{}", at["boxes"]["q1"]);
    assert_eq!(at["boxes"]["q1"]["y"], 200.0, "{}", at["boxes"]["q1"]);
    let pin = json!({"op": "configure", "options": {"pins": {"q1": {"x": 40, "y": 60}}}});
    let pinned = on(&snapshot, pin)["snapshot"].clone();
    assert_eq!(
        pinned["boxes"]["q1"]["x"], 40.0,
        "{}",
        pinned["boxes"]["q1"]
    );
    assert_eq!(
        pinned["boxes"]["q1"]["y"], 60.0,
        "{}",
        pinned["boxes"]["q1"]
    );
}

#[test]
fn stale_requests_stay_rejected_and_refinement_is_identical_for_any_thread_count_with_auto() {
    let snapshot = import(&nested(), json!({"singlePassAuto": true}));
    let stale = run(
        json!({"op": "add", "snapshot": snapshot, "sourceId": "q3", "label": "x",
        "expectedRevision": snapshot["revision"].as_u64().unwrap() + 1, "expectedLayoutVersion": 0}),
    );
    assert_eq!(stale["ok"], false, "{stale}");
    for (name, graph) in rejected() {
        let fallen = import(&graph, json!({"singlePassAuto": true}));
        let one = on(&fallen, json!({"op": "refine", "threads": 1}));
        let four = on(&fallen, json!({"op": "refine", "threads": 4}));
        assert_eq!(one["ok"], true, "{name}: {one}");
        assert_eq!(one["snapshot"], four["snapshot"], "{name}");
        // The refinement of a fallback is the default's.
        let plain = on(
            &import(&graph, json!({})),
            json!({"op": "refine", "threads": 1}),
        );
        assert_eq!(
            one["snapshot"]["boxes"], plain["snapshot"]["boxes"],
            "{name}"
        );
        assert_eq!(
            one["snapshot"]["routes"], plain["snapshot"]["routes"],
            "{name}"
        );
    }
}

#[test]
fn the_option_is_omitted_by_default() {
    let snapshot = import_graph_with(small(), 1000, &LayoutOptions::default()).unwrap();
    let value = serde_json::to_value(&snapshot).unwrap();
    assert!(value
        .get("options")
        .is_none_or(|o| o.get("singlePassAuto").is_none()));
}

/// A rank of a stored plan with at least two nodes of the graph: its area, and its nodes in order.
fn rank_with_two_nodes(snapshot: &Value) -> (String, Vec<String>) {
    let nodes: Vec<&str> = snapshot["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    for (area, plan) in snapshot["layeredPlans"].as_object().unwrap() {
        for rank in plan["ranks"].as_array().unwrap() {
            let ids: Vec<String> = rank
                .as_array()
                .unwrap()
                .iter()
                .map(|id| id.as_str().unwrap().to_string())
                .filter(|id| nodes.contains(&id.as_str()))
                .collect();
            if ids.len() >= 2 {
                return (area.clone(), ids);
            }
        }
    }
    panic!("no rank with two nodes")
}

fn order_in(snapshot: &Value, area: &str, node: &str) -> Vec<String> {
    snapshot["layeredPlans"][area]["ranks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|rank| rank.as_array().unwrap())
        .find(|rank| rank.iter().any(|id| id == node))
        .unwrap()
        .iter()
        .map(|id| id.as_str().unwrap().to_string())
        .collect()
}

fn index(order: &[String], id: &str) -> usize {
    order.iter().position(|x| x == id).unwrap()
}

/// Drags the first node of a rank beyond the last one, across the flow, with the given last phase.
fn drag_past(
    snapshot: &Value,
    finish: &str,
    release: bool,
) -> (Vec<Value>, String, String, String) {
    let (area, ids) = rank_with_two_nodes(snapshot);
    let (first, last) = (ids[0].clone(), ids[ids.len() - 1].clone());
    let vertical = snapshot["layeredPlans"][&area]["vertical"] == true;
    let beyond = &snapshot["boxes"][&last];
    let at = &snapshot["boxes"][&first];
    let (x, y) = if vertical {
        (
            beyond["x"].as_f64().unwrap() + 40.,
            at["y"].as_f64().unwrap(),
        )
    } else {
        (
            at["x"].as_f64().unwrap(),
            beyond["y"].as_f64().unwrap() + 40.,
        )
    };
    let mut steps = Vec::new();
    let mut current = snapshot.clone();
    for (seq, phase) in [(1, "begin"), (2, "update"), (3, finish)] {
        let reply = on(
            &current,
            json!({"op": "move", "gestureId": "drag", "seq": seq, "phase": phase,
            "node": first, "position": {"x": x, "y": y}, "release": release}),
        );
        assert_eq!(reply["ok"], true, "{phase}: {reply}");
        current = reply["snapshot"].clone();
        steps.push(current.clone());
    }
    (steps, area, first, last)
}

#[test]
fn a_released_drag_stays_on_single_pass_and_its_drop_orders_the_rank() {
    let snapshot = import(&nested(), json!({"singlePassAuto": true}));
    let (steps, area, first, last) = drag_past(&snapshot, "finish", true);
    for step in &steps {
        assert!(on_single_pass(step), "{}", step["layoutDiagnostics"]);
    }
    let done = steps.last().unwrap();
    assert_eq!(done["options"]["orderHints"][&first]["after"], json!(last));
    assert!(done["options"]["pins"].is_null(), "no pin is left");
    let order = order_in(done, &area, &first);
    assert!(index(&order, &first) > index(&order, &last), "{order:?}");
    // The hint lasts: a later edit keeps the order.
    let next = on(
        done,
        json!({"op": "add", "sourceId": "p2", "label": "Next"}),
    );
    let next = &next["snapshot"];
    let order = order_in(next, &area, &first);
    assert!(index(&order, &first) > index(&order, &last), "{order:?}");
    assert!(on_single_pass(next));
}

#[test]
fn a_cancelled_drag_on_single_pass_puts_the_node_back() {
    let snapshot = import(&nested(), json!({"singlePassAuto": true}));
    let (steps, _, first, _) = drag_past(&snapshot, "cancel", true);
    let done = steps.last().unwrap();
    assert!(on_single_pass(done));
    assert_eq!(done["boxes"][&first], snapshot["boxes"][&first]);
    assert_eq!(done["options"], snapshot["options"]);
    assert_eq!(done["gesture"]["active"], false);
}

#[test]
fn an_order_hint_is_undone_by_its_intent_and_holds_on_the_default_pipeline() {
    let snapshot = import(&nested(), json!({"singlePassAuto": true}));
    let before = on(&snapshot, json!({"op": "intent"}))["intent"].clone();
    let (steps, area, first, last) = drag_past(&snapshot, "finish", true);
    let done = steps.last().unwrap();
    let undone = on(done, json!({"op": "restore", "intent": before}));
    assert_eq!(undone["ok"], true, "{undone}");
    assert!(undone["snapshot"]["options"]["orderHints"].is_null());

    let plain = import(&nested(), json!({}));
    let hinted = on(
        &plain,
        json!({"op": "configure", "options": {"orderHints": {first.clone(): {"after": last.clone()}}}}),
    );
    assert_eq!(hinted["ok"], true, "{hinted}");
    let order = order_in(&hinted["snapshot"], &area, &first);
    assert!(index(&order, &first) > index(&order, &last), "{order:?}");
    for wrong in [
        json!({"nowhere": {"after": null}}),
        json!({first.clone(): {"after": first.clone()}}),
        json!({first.clone(): {"after": "q1"}}),
    ] {
        let reply = on(
            &plain,
            json!({"op": "configure", "options": {"orderHints": wrong}}),
        );
        if area != "q" || wrong.get(&first).is_none_or(|h| h["after"] != "q1") {
            assert_eq!(reply["ok"], false, "{wrong}: {reply}");
        }
    }
}

#[test]
fn a_child_of_another_axis_gets_side_nodes_on_its_parents_flow_sides() {
    let graph = mixed_axes();
    let snapshot = import(&graph, json!({"abstractRouting": true, "singlePass": true}));
    let p = &snapshot["layeredPlans"]["p"];
    let sides = p["sides"]
        .as_object()
        .expect("the tb container p has side nodes");
    // e4..e8 run from p to q, e9 from q into p.
    for edge in ["e4", "e5", "e6", "e7", "e8", "e9"] {
        assert!(sides.contains_key(edge), "{edge}: {sides:?}");
    }
    let ranks: Vec<Vec<String>> = serde_json::from_value(p["ranks"].clone()).unwrap();
    let rank_of = |id: &str| {
        ranks
            .iter()
            .position(|r| r.iter().any(|x| x == id))
            .unwrap()
    };
    let edges: Vec<Value> = serde_json::from_value(graph_edges(&graph)).unwrap();
    for (edge, node) in sides {
        let node = node.as_str().unwrap();
        let e = edges.iter().find(|e| e["id"] == edge.as_str()).unwrap();
        // The end inside p: the source, or the target of a backward edge (e9 runs against the flow).
        let leaf = [&e["source"], &e["target"]]
            .into_iter()
            .map(|v| v.as_str().unwrap())
            .find(|id| ranks.iter().flatten().any(|x| x == id))
            .unwrap();
        let rank = &ranks[rank_of(node)];
        let inner: Vec<&String> = rank.iter().filter(|x| !x.contains("side-")).collect();
        // Beside its leaf (ADR 0017, amendment A): only where the leaf stands outermost on that side.
        let lateral = rank_of(node) == rank_of(leaf);
        if node.contains("side-out:") {
            if lateral {
                assert_eq!(inner.last().map(|x| x.as_str()), Some(leaf), "{edge}");
            } else {
                assert_eq!(rank_of(node), rank_of(leaf) + 1, "{edge}");
            }
            assert!(
                rank.iter()
                    .rev()
                    .take_while(|x| x.contains("side-out:"))
                    .any(|x| x == node),
                "{edge}: last in {rank:?}"
            );
        } else {
            if lateral {
                assert_eq!(inner.first().map(|x| x.as_str()), Some(leaf), "{edge}");
            } else {
                assert_eq!(rank_of(node) + 1, rank_of(leaf), "{edge}");
            }
            assert!(
                rank.iter()
                    .take_while(|x| x.contains("side-in:"))
                    .any(|x| x == node),
                "{edge}: first in {rank:?}"
            );
        }
        assert!(
            p["dummies"].get(node).is_some(),
            "{edge}: the side node is placed"
        );
    }
    // A container of the parent's axis has none.
    assert!(snapshot["layeredPlans"]["q"].get("sides").is_none());
}

fn graph_edges(graph: &Graph) -> Value {
    serde_json::to_value(graph).unwrap()["edges"].clone()
}

#[test]
fn edges_across_an_axis_border_are_built_in_legs_in_logical_direction() {
    let graph = mixed_axes();
    let snapshot = import(&graph, json!({"abstractRouting": true, "singlePass": true}));
    let report = snapshot["layoutDiagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find_map(|d| d.as_str().filter(|d| d.starts_with("Single pass:")))
        .unwrap()
        .to_string();
    let constructed: usize = report.split_whitespace().nth(2).unwrap().parse().unwrap();
    // e4..e9 cross the border of the tb container p; before the legs none of them was constructed.
    assert!(constructed >= 8, "{report}");
    // Every route keeps its logical direction: it starts at its source's box and ends at its target's.
    let absolute = |id: &str| {
        let mut r = snapshot["boxes"][id].clone();
        let mut parent = snapshot["graph"]["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap()["parentId"]
            .clone();
        while let Some(p) = parent.as_str().map(str::to_string) {
            let b = &snapshot["boxes"][&p];
            r["x"] = json!(r["x"].as_f64().unwrap() + b["x"].as_f64().unwrap());
            r["y"] = json!(r["y"].as_f64().unwrap() + b["y"].as_f64().unwrap());
            parent = snapshot["graph"]["nodes"]
                .as_array()
                .unwrap()
                .iter()
                .find(|n| n["id"] == p.as_str())
                .unwrap()["parentId"]
                .clone();
        }
        r
    };
    let on = |point: &Value, rect: &Value| {
        let (x, y) = (point["x"].as_f64().unwrap(), point["y"].as_f64().unwrap());
        let (rx, ry) = (rect["x"].as_f64().unwrap(), rect["y"].as_f64().unwrap());
        let (w, h) = (
            rect["width"].as_f64().unwrap(),
            rect["height"].as_f64().unwrap(),
        );
        x >= rx - 1e-6 && x <= rx + w + 1e-6 && y >= ry - 1e-6 && y <= ry + h + 1e-6
    };
    for edge in ["e4", "e5", "e6", "e7", "e8", "e9"] {
        let route = snapshot["routes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == edge)
            .unwrap();
        let e = snapshot["graph"]["edges"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == edge)
            .unwrap();
        let points = route["points"].as_array().unwrap();
        assert!(
            on(&points[0], &absolute(e["source"].as_str().unwrap())),
            "{edge}: {route}"
        );
        assert!(
            on(
                points.last().unwrap(),
                &absolute(e["target"].as_str().unwrap())
            ),
            "{edge}: {route}"
        );
    }
}

/// Rounded corners are drawn by the scene on the orthogonal route, so single pass takes `roundedOrthogonal` as it takes
/// `orthogonal`: the same routes, marked rounded in the scene, also under the automatic choice.
#[test]
fn rounded_orthogonal_routing_is_drawn_by_single_pass_on_the_same_routes() {
    let mut rounded = serde_json::to_value(nested()).unwrap();
    rounded["defaultRouting"] = json!("roundedOrthogonal");
    rounded["containerRouting"] = json!({"q": ["orthogonal", "roundedOrthogonal"]});
    let rounded: Graph = serde_json::from_value(rounded).unwrap();
    let plain = import(
        &nested(),
        json!({"abstractRouting": true, "singlePass": true}),
    );
    let explicit = import(
        &rounded,
        json!({"abstractRouting": true, "singlePass": true}),
    );
    let auto = import(&rounded, json!({"singlePassAuto": true}));
    assert!(on_single_pass(&auto) && !reported_fallback(&auto), "{auto}");
    for snapshot in [&explicit, &auto] {
        assert_eq!(snapshot["routes"], plain["routes"]);
    }
    let snapshot: Snapshot = serde_json::from_value(explicit).unwrap();
    check(&snapshot);
    let scene = arqlet_scene::build_scene(&snapshot).unwrap();
    // The root rounds; the list in `q` keeps the plain baseline (`RoutingConstraint::picks`).
    assert!(scene.routes.iter().any(|r| r.rounded));
    assert!(scene.routes.iter().any(|r| !r.rounded));
    // A style that is not drawn on the orthogonal route still falls back.
    let mut polyline = serde_json::to_value(nested()).unwrap();
    polyline["defaultRouting"] = json!("polyline");
    let polyline: Graph = serde_json::from_value(polyline).unwrap();
    assert!(reported_fallback(&import(
        &polyline,
        json!({"singlePassAuto": true})
    )));
}

/// ADR 0017, amendment A: where a leaf stands outermost in its rank, its side node stands beside it and the edge leaves the
/// leaf across the area's flow, straight to the side port, then runs on outside.
#[test]
fn a_leaf_outermost_in_its_rank_leaves_straight_through_a_side_node_beside_it() {
    let graph = mixed_axes();
    let snapshot = import(&graph, json!({"abstractRouting": true, "singlePass": true}));
    let typed: Snapshot = serde_json::from_value(snapshot.clone()).unwrap();
    check(&typed);
    let p = &snapshot["layeredPlans"]["p"];
    let ranks: Vec<Vec<String>> = serde_json::from_value(p["ranks"].clone()).unwrap();
    let chains = p["chains"].as_array().unwrap();
    let mut lateral = 0;
    for (edge, node) in p["sides"].as_object().unwrap() {
        let node = node.as_str().unwrap();
        let linked = chains
            .iter()
            .any(|c| c["source"] == node || c["target"] == node);
        if linked {
            continue;
        }
        lateral += 1;
        let e = typed.graph.edges.iter().find(|e| &e.id == edge).unwrap();
        let leaf = [&e.source, e.target.as_ref().unwrap()]
            .into_iter()
            .find(|id| ranks.iter().flatten().any(|x| x == *id))
            .unwrap();
        let route = typed.routes.iter().find(|r| &r.id == edge).unwrap();
        let points = &route.points;
        // The segment at the leaf's end is horizontal: p flows top to bottom, its side lies across.
        let (a, b) = if leaf == &e.source {
            (&points[0], &points[1])
        } else {
            (&points[points.len() - 1], &points[points.len() - 2])
        };
        assert!((a.y - b.y).abs() < 1e-6, "{edge}: {points:?}");
    }
    assert!(lateral > 0, "the fixture has a leaf outermost in its rank");
}

/// ADR 0017, amendment A3: an edge from a leaf in an `lr` area inside a `tb` area inside the `lr` root leaves the inner area
/// by a border node of its own flow, crosses the middle area straight beside it, and runs on in the root.
#[test]
fn an_edge_through_a_middle_area_of_the_other_axis_crosses_it_straight() {
    let node = |id: &str, parent: Option<&str>| json!({"id": id, "kind": "symbol", "parentId": parent, "label": id});
    let edge =
        |id: &str, s: &str, t: &str| json!({"id": id, "kind": "calls", "source": s, "target": t});
    let graph: Graph = serde_json::from_value(json!({
        "defaultDirection": "lr",
        "containerDirections": {"m": "tb", "c": "lr"},
        "nodes": [
            node("start", None), node("end", None), node("m", None),
            node("top", Some("m")), node("c", Some("m")),
            node("leaf", Some("c")), node("pre", Some("c")),
        ],
        "edges": [
            edge("e1", "start", "top"), edge("e2", "top", "pre"), edge("e3", "pre", "leaf"),
            edge("e4", "leaf", "end"),
        ],
    }))
    .unwrap();
    let snapshot = import(&graph, json!({"abstractRouting": true, "singlePass": true}));
    let typed: Snapshot = serde_json::from_value(snapshot.clone()).unwrap();
    check(&typed);
    assert!(
        !snapshot["layoutDiagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d.as_str().unwrap().contains("Single pass:")
                && !d.as_str().unwrap().contains(" 0 delegated")),
        "{}",
        snapshot["layoutDiagnostics"]
    );
    let c = &snapshot["layeredPlans"]["c"];
    assert!(
        c["borders"].get("e4").is_some(),
        "c leaves by a border node: {c}"
    );
    let m = &snapshot["layeredPlans"]["m"];
    let side = m["sides"]["e4"].as_str().expect("m has a side node for e4");
    assert!(
        !m["chains"]
            .as_array()
            .unwrap()
            .iter()
            .any(|ch| ch["source"] == side || ch["target"] == side),
        "the side node stands beside c: {m}"
    );
    // Across m the route is one horizontal segment: from c's right side to beyond m's right side.
    let route = typed.routes.iter().find(|r| r.id == "e4").unwrap();
    let rect = |id: &str| {
        let mut r = typed.boxes[id].clone();
        let mut at = typed
            .graph
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .parent_id
            .clone();
        while let Some(p) = at {
            r.x += typed.boxes[&p].x;
            r.y += typed.boxes[&p].y;
            at = typed
                .graph
                .nodes
                .iter()
                .find(|n| n.id == p)
                .unwrap()
                .parent_id
                .clone();
        }
        r
    };
    let (cr, mr) = (rect("c"), rect("m"));
    let crossing = route.points.windows(2).find(|w| {
        let (lo, hi) = (w[0].x.min(w[1].x), w[0].x.max(w[1].x));
        lo <= cr.x + cr.width + 1e-6 && hi >= mr.x + mr.width - 1e-6
    });
    let w =
        crossing.unwrap_or_else(|| panic!("one segment from c to m's border: {:?}", route.points));
    assert!((w[0].y - w[1].y).abs() < 1e-6, "{:?}", route.points);
}

/// Self-loops are constructed, nested at a free corner of their node as the default nests them, not drawn as seed routes:
/// the report counts no delegated edge, and every loop starts and ends at its node.
#[test]
fn single_pass_constructs_self_loops() {
    let snapshot = import(
        &nested(),
        json!({"abstractRouting": true, "singlePass": true}),
    );
    let report = snapshot["layoutDiagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d.as_str())
        .find(|d| d.starts_with("Single pass:"))
        .unwrap()
        .to_string();
    assert!(report.contains(" 0 delegated"), "{report}");
    let typed: Snapshot = serde_json::from_value(snapshot).unwrap();
    check(&typed);
    let route = typed.routes.iter().find(|r| r.id == "e11").unwrap();
    assert!(
        route.points.len() >= 4,
        "a loop around a corner: {:?}",
        route.points
    );
}

/// An explicit port on the side the flow leaves by is constructed like an automatic one, at its declared place; ports on other
/// sides stay seed routes (`Delegation::PortSide`).
#[test]
fn single_pass_constructs_an_explicit_port_on_its_flow_side() {
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../benchmarks/datasets/fixed-ports.json"
    ))
    .unwrap();
    let snapshot = import(&graph, json!({"abstractRouting": true, "singlePass": true}));
    let typed: Snapshot = serde_json::from_value(snapshot.clone()).unwrap();
    check(&typed);
    let report = snapshot["layoutDiagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|d| d.as_str())
        .find(|d| d.starts_with("Single pass:"))
        .unwrap()
        .to_string();
    // e1 leaves the hub on its right side (hub-east), the flow side of `lr`; e2..e4 use the top, bottom and left sides.
    assert!(report.contains(" 3 delegated"), "{report}");
    let hub = &typed.boxes["hub"];
    let route = typed.routes.iter().find(|r| r.id == "e1").unwrap();
    let start = &route.points[0];
    assert!(
        (start.x - (hub.x + hub.width)).abs() < 1e-6,
        "{start:?} {hub:?}"
    );
    assert!(
        (start.y - (hub.y + 0.25 * hub.height)).abs() < 1e-6,
        "{start:?} {hub:?}"
    );
}
