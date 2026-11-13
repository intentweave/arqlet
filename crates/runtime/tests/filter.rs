//! The `filter` operation (provisional): hiding nodes by kind or id and edges by kind keeps the place of everything else.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn ok(response: Value) -> Value {
    assert_eq!(response["ok"], true, "{response}");
    response["snapshot"].clone()
}

fn scene(snapshot: &Value) -> Value {
    let response = call(json!({"op": "scene", "snapshot": snapshot}));
    assert_eq!(response["ok"], true, "{response}");
    response["scene"].clone()
}

fn filter(snapshot: &Value, hide: Value) -> Value {
    call(json!({"op": "filter", "snapshot": snapshot, "hide": hide,
        "expectedRevision": snapshot["revision"], "expectedLayoutVersion": snapshot["layoutVersion"]}))
}

/// Two packages with files; files call and import each other across the packages; a cycle,
/// a self-loop and a parallel edge among the files.
fn catalog() -> Value {
    let mut nodes = Vec::new();
    for package in ["e0", "e1"] {
        nodes.push(json!({"id": package, "kind": "package", "label": package}));
        for f in 0..3 {
            nodes.push(json!({"id": format!("{package}/f{f}"), "kind": "file", "label": format!("f{f}"), "parentId": package}));
        }
        nodes.push(
            json!({"id": format!("{package}/test"), "kind": "test", "label": "test", "parentId": package}),
        );
    }
    let mut edges = Vec::new();
    let mut add = |id: &str, kind: &str, s: &str, t: &str| {
        edges.push(json!({"id": id, "kind": kind, "source": s, "target": t}));
    };
    add("c0", "calls", "e0/f0", "e1/f0");
    add("c1", "calls", "e1/f1", "e0/f1");
    add("c2", "calls", "e0/f2", "e1/f2");
    add("c3", "calls", "e0/f2", "e1/f2");
    add("s0", "imports", "e0/f0", "e1/f1");
    add("s1", "imports", "e1/f2", "e0/f0");
    add("d0", "calls", "e0/test", "e0/f0");
    add("loop", "calls", "e1/f0", "e1/f0");
    add("x0", "calls", "e0/f1", "e0/f2");
    add("x1", "calls", "e0/f2", "e0/f1");
    json!({"nodes": nodes, "edges": edges})
}

fn bounds(scene: &Value) -> std::collections::BTreeMap<String, Value> {
    scene["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| (n["id"].as_str().unwrap().to_string(), n["bounds"].clone()))
        .collect()
}

fn route_ids(scene: &Value) -> Vec<String> {
    scene["routes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn hiding_keeps_the_place_of_everything_else_and_showing_brings_it_back() {
    let snapshot = ok(call(
        json!({"op": "import", "graph": catalog(), "limit": 100}),
    ));
    let before = scene(&snapshot);
    let hidden = ok(filter(
        &snapshot,
        json!({"nodeKinds": ["test"], "edgeKinds": ["imports"]}),
    ));
    assert_eq!(
        hidden["layoutVersion"],
        snapshot["layoutVersion"].as_u64().unwrap() + 1
    );
    assert_eq!(
        hidden["options"]["hidden"],
        json!({"nodeKinds": ["test"], "edgeKinds": ["imports"]})
    );
    let after = scene(&hidden);
    // What stays has exactly the bounds it had; the hidden nodes and their edges are gone.
    let (all, left) = (bounds(&before), bounds(&after));
    assert!(!left.contains_key("e0/test") && !left.contains_key("e1/test"));
    assert_eq!(left.len(), all.len() - 2);
    for (id, b) in &left {
        assert_eq!(b, &all[id], "{id} keeps its place");
    }
    let routes = route_ids(&after);
    for gone in ["s0", "s1", "d0"] {
        assert!(!routes.contains(&gone.to_string()), "{gone} is hidden");
    }
    assert!(routes.contains(&"c0".to_string()) && routes.contains(&"loop".to_string()));
    assert_eq!(
        after["bounds"], before["bounds"],
        "the drawing does not jump"
    );
    // An empty filter shows everything again, as it was.
    let shown = ok(filter(&hidden, json!({})));
    let again = scene(&shown);
    assert_eq!(bounds(&again), all);
    assert_eq!(route_ids(&again), route_ids(&before));
    assert!(
        shown["options"].get("hidden").is_none(),
        "omitted when empty"
    );
}

#[test]
fn a_hidden_container_hides_what_it_holds_and_an_edit_keeps_the_filter() {
    let snapshot = ok(call(
        json!({"op": "import", "graph": catalog(), "limit": 100}),
    ));
    let hidden = ok(filter(&snapshot, json!({"nodes": ["e1"]})));
    let s = scene(&hidden);
    assert!(
        bounds(&s).keys().all(|id| !id.starts_with("e1")),
        "e1 and its files are hidden"
    );
    assert!(
        route_ids(&s)
            .iter()
            .all(|r| ["d0", "x0", "x1"].contains(&r.as_str())),
        "only edges inside e0 stay"
    );
    // Collapsing e0 keeps the filter: the view state travels with the snapshot.
    let toggled = ok(call(
        json!({"op": "toggle", "snapshot": hidden, "nodeId": "e0",
        "expectedRevision": hidden["revision"], "expectedLayoutVersion": hidden["layoutVersion"]}),
    ));
    assert_eq!(toggled["options"]["hidden"], json!({"nodes": ["e1"]}));
    assert!(bounds(&scene(&toggled))
        .keys()
        .all(|id| !id.starts_with("e1")));
}

#[test]
fn a_route_for_collapsed_content_counts_only_the_edges_still_shown() {
    let snapshot = ok(call(
        json!({"op": "import", "graph": catalog(), "limit": 100,
        "options": {"levelOfDetail": {"depth": 0}}}),
    ));
    let total = |s: &Value| -> u64 {
        s["routes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["weight"].as_u64().unwrap() + r["backWeight"].as_u64().unwrap_or(0))
            .sum()
    };
    // Between the two packages: c0, c1, c2, c3 (calls) and s0, s1 (imports).
    assert_eq!(total(&scene(&snapshot)), 6);
    let calls = ok(filter(&snapshot, json!({"edgeKinds": ["imports"]})));
    assert_eq!(total(&scene(&calls)), 4);
    let none = ok(filter(
        &snapshot,
        json!({"edgeKinds": ["imports", "calls"]}),
    ));
    assert!(
        route_ids(&scene(&none)).is_empty(),
        "nothing left between them: no route"
    );
}

#[test]
fn unknown_nodes_and_stale_requests_are_refused() {
    let snapshot = ok(call(
        json!({"op": "import", "graph": catalog(), "limit": 100}),
    ));
    let unknown = filter(&snapshot, json!({"nodes": ["nowhere"]}));
    assert_eq!(unknown["ok"], false);
    assert!(unknown["error"].as_str().unwrap().contains("Unknown node"));
    let stale = call(
        json!({"op": "filter", "snapshot": snapshot, "hide": {"nodeKinds": ["test"]},
        "expectedRevision": snapshot["revision"], "expectedLayoutVersion": 99}),
    );
    assert_eq!(stale["ok"], false);
    let wrong = filter(&snapshot, json!({"nodeKinds": "test"}));
    assert_eq!(wrong["ok"], false);
    // A kind the graph does not have hides nothing (a filter may name kinds of other views).
    let other = ok(filter(&snapshot, json!({"nodeKinds": ["module"]})));
    assert_eq!(bounds(&scene(&other)), bounds(&scene(&snapshot)));
}

#[test]
fn a_resident_session_filters_too() {
    let mut session = arqlet_runtime::engine_session::EngineSession::new();
    let opened = session.run_value(&json!({"op": "import", "graph": catalog(), "limit": 100}));
    assert_eq!(opened["ok"], true, "{opened}");
    let filtered =
        session.run_value(&json!({"op": "filter", "hide": {"nodeKinds": ["test"]}, "scene": true}));
    assert_eq!(filtered["ok"], true, "{filtered}");
    let ids: Vec<&str> = filtered["scene"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert!(!ids.iter().any(|id| id.ends_with("/test")));
}

#[test]
fn a_trace_follows_only_the_edge_kinds_asked_for() {
    let snapshot = ok(call(
        json!({"op": "import", "graph": catalog(), "limit": 100}),
    ));
    let trace = |from: &str, to: &str, kinds: Value| {
        let mut request = json!({"op": "trace", "snapshot": snapshot, "from": from, "to": to});
        if !kinds.is_null() {
            request["edgeKinds"] = kinds;
        }
        call(request)
    };
    // e0/f2 calls e1/f2 (twice), which imports e0/f0.
    let any = trace("e0/f2", "e0/f0", Value::Null);
    assert_eq!(any["found"], true, "{any}");
    assert_eq!(any["length"], 2);
    assert_eq!(any["edges"], json!(["c2", "c3", "s1"]));
    let both = trace("e0/f2", "e0/f0", json!(["calls", "imports"]));
    assert_eq!(both["edges"], any["edges"]);
    // Along calls only there is no way, in either direction.
    let calls = trace("e0/f2", "e0/f0", json!(["calls"]));
    assert_eq!(calls["ok"], true, "{calls}");
    assert_eq!(calls["found"], false);
    assert_eq!(calls["visible"]["routes"], json!([]));
    assert_eq!(
        trace("e0/f1", "e0/f2", json!(["calls"]))["edges"],
        json!(["x0"])
    );
    assert_eq!(trace("e0/f1", "e0/f2", json!(["imports"]))["found"], false);
    // Bad kinds are refused; a kind the graph lacks just finds nothing.
    for bad in [json!([]), json!("calls")] {
        assert_eq!(trace("e0/f1", "e0/f2", bad)["ok"], false);
    }
    assert_eq!(trace("e0/f1", "e0/f2", json!(["events"]))["found"], false);
    // In a session the same.
    let mut session = arqlet_runtime::engine_session::EngineSession::new();
    session.run_value(&json!({"op": "import", "graph": catalog(), "limit": 100, "scene": false}));
    let resident = session
        .run_value(&json!({"op": "trace", "from": "e0/f1", "to": "e0/f2", "edgeKinds": ["calls"]}));
    assert_eq!(resident["edges"], json!(["x0"]));
}
