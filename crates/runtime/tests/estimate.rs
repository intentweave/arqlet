//! ADR 0024, step C3: the estimate of a request's peak memory (`estimate`). It finds the view an operation lays out the way
//! the operation finds it, without the layout; it is deterministic, answers the same through `dispatch` and in a session,
//! and changes nothing. Its calibration against the counted heap is in `benchmarks/results/memory-estimate-20261127.md`.
use arqlet_runtime::engine_session::EngineSession;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&arqlet_runtime::dispatch(&request.to_string())).unwrap()
}

fn ok(answer: Value) -> Value {
    assert_eq!(answer["ok"], true, "{answer}");
    answer
}

/// Six packages of eight functions, edges within and across packages, a cycle, a self-loop and a parallel edge.
fn graph() -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for p in 0..6 {
        nodes.push(json!({"id": format!("p{p}"), "kind": "package", "label": format!("p{p}")}));
        for f in 0..8 {
            nodes.push(json!({"id": format!("p{p}f{f}"), "kind": "fn", "label": format!("f{f}"), "parentId": format!("p{p}")}));
        }
    }
    let mut add = |s: String, t: String| {
        let id = format!("e{}", edges.len());
        edges.push(json!({"id": id, "kind": "calls", "source": s, "target": t}));
    };
    for p in 0..6 {
        for f in 0..7 {
            add(format!("p{p}f{f}"), format!("p{p}f{}", f + 1));
        }
        add(format!("p{p}f0"), format!("p{}f3", (p + 1) % 6));
    }
    add("p0f7".into(), "p0f0".into());
    add("p1f1".into(), "p1f1".into());
    add("p2f2".into(), "p3f5".into());
    add("p2f2".into(), "p3f5".into());
    json!({"nodes": nodes, "edges": edges})
}

fn import() -> Value {
    json!({"op": "import", "graph": graph(), "limit": 100, "options": {"levelOfDetail": {"depth": 0}}})
}

fn view_of(snapshot: &Value) -> (usize, usize) {
    (
        snapshot["graph"]["nodes"].as_array().unwrap().len(),
        snapshot["graph"]["edges"].as_array().unwrap().len(),
    )
}

fn estimated_view(answer: &Value) -> (usize, usize) {
    let view = &answer["estimate"]["view"];
    (
        view["nodes"].as_u64().unwrap() as usize,
        view["edges"].as_u64().unwrap() as usize,
    )
}

#[test]
fn an_import_is_estimated_with_the_view_it_lays_out() {
    let estimate = ok(call(json!({"op": "estimate", "request": import()})));
    let snapshot = ok(call(import()))["snapshot"].clone();
    assert_eq!(estimated_view(&estimate), view_of(&snapshot));
    let e = &estimate["estimate"];
    assert_eq!(e["op"], "import");
    assert_eq!(e["source"]["nodes"], 54);
    assert_eq!(e["source"]["edges"], 52);
    let parts: u64 = e["parts"]
        .as_object()
        .unwrap()
        .values()
        .map(|v| v.as_u64().unwrap())
        .sum();
    assert_eq!(e["bytes"].as_u64().unwrap(), parts);
    assert!(parts > 0);
    assert_eq!(
        e["target"],
        if cfg!(target_pointer_width = "64") {
            "native64"
        } else {
            "wasm32"
        }
    );
    // Deterministic: the same request, the same answer.
    assert_eq!(
        call(json!({"op": "estimate", "request": import()})),
        estimate
    );
}

#[test]
fn opening_a_container_is_estimated_with_the_view_after_it() {
    let snapshot = ok(call(import()))["snapshot"].clone();
    let toggle = json!({"op": "toggle", "nodeId": "p2", "snapshot": snapshot,
        "expectedRevision": snapshot["revision"], "expectedLayoutVersion": snapshot["layoutVersion"]});
    let estimate = ok(call(json!({"op": "estimate", "request": toggle})));
    let opened = ok(call(toggle))["snapshot"].clone();
    assert_eq!(estimated_view(&estimate), view_of(&opened));
    assert!(view_of(&opened).0 > view_of(&snapshot).0);
    // An edit that keeps the view is estimated on the view as it is, and a refinement holds a layout per job.
    let direction = json!({"op": "direction", "direction": "lr", "snapshot": snapshot,
        "expectedRevision": snapshot["revision"], "expectedLayoutVersion": snapshot["layoutVersion"]});
    let kept = ok(call(json!({"op": "estimate", "request": direction})));
    assert_eq!(estimated_view(&kept), view_of(&snapshot));
    let refine = |threads: u64| {
        ok(call(
            json!({"op": "estimate", "request": {"op": "refine", "threads": threads, "workBudget": 4,
            "snapshot": snapshot, "expectedRevision": snapshot["revision"],
            "expectedLayoutVersion": snapshot["layoutVersion"]}}),
        ))
    };
    let (one, four) = (refine(1), refine(4));
    assert_eq!(one["estimate"]["jobs"], 1);
    assert_eq!(four["estimate"]["jobs"], 4);
    assert!(
        four["estimate"]["parts"]["layout"].as_u64() > one["estimate"]["parts"]["layout"].as_u64()
    );
}

#[test]
fn a_session_estimates_on_its_view_and_changes_nothing() {
    let mut session = EngineSession::new();
    let mut imported = import();
    imported["scene"] = json!(false);
    let in_session = ok(session.run_value(&json!({"op": "estimate", "request": imported})));
    assert_eq!(
        estimated_view(&in_session),
        view_of(&ok(call(import()))["snapshot"])
    );
    ok(session.run_value(&imported));
    let before = serde_json::to_string(session.snapshot().unwrap()).unwrap();
    let toggle = json!({"op": "toggle", "nodeId": "p2", "scene": false});
    // The text form (one pass) and the value form answer alike.
    let text = session.run(&json!({"op": "estimate", "request": toggle}).to_string());
    let estimate = ok(serde_json::from_str(&text).unwrap());
    assert_eq!(
        estimate,
        ok(session.run_value(&json!({"op": "estimate", "request": toggle})))
    );
    assert_eq!(
        serde_json::to_string(session.snapshot().unwrap()).unwrap(),
        before
    );
    ok(session.run_value(&toggle));
    let after = serde_json::to_value(session.snapshot().unwrap()).unwrap();
    assert_eq!(estimated_view(&estimate), view_of(&after));
}

#[test]
fn an_overview_is_estimated_from_its_graph() {
    let estimate = ok(call(
        json!({"op": "estimate", "request": {"op": "overview", "graph": graph()}}),
    ));
    assert_eq!(estimate["estimate"]["source"]["nodes"], 54);
    assert!(estimate["estimate"]["bytes"].as_u64().unwrap() > 0);
}

#[test]
fn what_the_estimate_does_not_know_is_refused() {
    let answer = call(json!({"op": "estimate", "request": {"op": "scene"}}));
    assert_eq!(answer["ok"], false);
    assert!(
        answer["error"].as_str().unwrap().contains("not scene"),
        "{answer}"
    );
    assert_eq!(call(json!({"op": "estimate"}))["ok"], false);
    let edit = call(json!({"op": "estimate", "request": {"op": "toggle", "nodeId": "p2"}}));
    assert_eq!(edit["ok"], false);
}
