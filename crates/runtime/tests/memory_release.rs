//! ADR 0023, R7: what a session holds (`memory`) and giving parts back (`release`), without changing the view.
use arqlet_runtime::engine_session::EngineSession;
use serde_json::{json, Value};

fn ok(answer: Value) -> Value {
    assert_eq!(answer["ok"], true, "{answer}");
    answer
}

fn graph() -> Value {
    let mut nodes = vec![json!({"id": "pkg", "kind": "package", "label": "pkg"})];
    let mut edges = Vec::new();
    for i in 0..40 {
        nodes.push(json!({"id": format!("n{i}"), "kind": "fn", "label": format!("function {i}"), "parentId": "pkg"}));
        if i > 0 {
            edges.push(json!({"id": format!("e{i}"), "kind": "calls", "source": format!("n{}", i - 1), "target": format!("n{i}")}));
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

#[test]
fn memory_names_the_parts_and_release_gives_them_back() {
    let mut s = EngineSession::new();
    let empty = ok(s.run_value(&json!({"op": "memory"})));
    assert_eq!(
        (empty["nodes"].clone(), empty["estimated"].clone()),
        (json!(0), json!(true))
    );
    // An abstract view (as large graphs start): the snapshot keeps the structure with its index.
    ok(s.run_value(
        &json!({"op": "import", "graph": graph(), "limit": 100, "scene": false,
        "options": {"levelOfDetail": {"depth": 0}}}),
    ));
    let mut properties = serde_json::Map::new();
    for i in 0..40 {
        properties.insert(
            format!("n{i}"),
            json!({"owner": format!("team {}", i % 3), "loc": i * 10}),
        );
    }
    ok(s.run_value(&json!({"op": "payload", "set": {"nodes": properties}})));
    ok(s.run_value(&json!({"op": "columns"})));
    let held = ok(s.run_value(&json!({"op": "memory"})));
    let part = |m: &Value, name: &str| m["parts"][name].as_u64().unwrap_or(0);
    for name in [
        "sourceGraph",
        "structureIndex",
        "boxes",
        "content",
        "changeLog",
        "columns",
    ] {
        assert!(part(&held, name) > 0, "{name}: {held}");
    }
    assert_eq!(held["nodes"], 41);
    let total: u64 = held["parts"]
        .as_object()
        .unwrap()
        .values()
        .filter_map(Value::as_u64)
        .sum();
    assert_eq!(held["total"].as_u64(), Some(total));

    // Releasing never changes the view.
    let before = ok(s.run_value(&json!({"op": "export"})))["snapshot"].to_string();
    // Content by key: the key is forgotten everywhere, as a change in the log.
    let at = ok(s.run_value(&json!({"op": "changesSince", "sequence": 0})))["sequence"]
        .as_u64()
        .unwrap();
    let by_key = ok(s.run_value(&json!({"op": "release", "parts": ["content"], "keys": ["loc"]})));
    // The content shrinks (the log grows by the change that says so).
    assert!(
        part(&by_key, "content") < part(&held, "content"),
        "{by_key}"
    );
    let n3 = ok(s.run_value(&json!({"op": "details", "nodeId": "n3"})));
    assert_eq!(n3["node"]["properties"], json!({"owner": "team 0"}));
    let log = ok(s.run_value(&json!({"op": "changesSince", "sequence": at})));
    assert_eq!(log["changes"][0]["payload"], json!({"dropKeys": ["loc"]}));
    // The columns and the log: the log's groups are gone, so a replica behind them resyncs.
    let freed = ok(s.run_value(&json!({"op": "release", "parts": ["columns", "changeLog"]})));
    assert_eq!((part(&freed, "columns"), part(&freed, "changeLog")), (0, 0));
    assert_eq!(
        ok(s.run_value(&json!({"op": "changesSince", "sequence": at})))["resync"],
        true
    );
    // All the content.
    let all = ok(s.run_value(&json!({"op": "release", "parts": ["content", "overview"]})));
    assert_eq!(part(&all, "content"), 0);
    assert!(
        ok(s.run_value(&json!({"op": "details", "nodeId": "n3"})))["node"]
            .get("properties")
            .is_none()
    );
    assert_eq!(
        ok(s.run_value(&json!({"op": "export"})))["snapshot"].to_string(),
        before,
        "the view is unchanged"
    );

    // Refusals.
    assert_eq!(
        s.run_value(&json!({"op": "release", "parts": ["snapshot"]}))["ok"],
        false
    );
    assert_eq!(s.run_value(&json!({"op": "release"}))["ok"], false);
    let stateless: Value = serde_json::from_str(&arqlet_runtime::dispatch(
        &json!({"op": "memory"}).to_string(),
    ))
    .unwrap();
    assert!(stateless["error"]
        .as_str()
        .unwrap()
        .contains("resident session"));
}

/// A recycled engine: `load` with the content and the change log's number, read in one pass from text, gives the same
/// session as the general way.
#[test]
fn load_takes_the_content_and_the_number_and_reads_in_one_pass() {
    let mut a = EngineSession::new();
    ok(a.run_value(&json!({"op": "import", "graph": graph(), "limit": 100, "scene": false})));
    ok(a.run_value(&json!({"op": "payload", "set": {"nodes": {"n1": {"owner": "Ann"}}}})));
    ok(a.run_value(&json!({"op": "payload", "set": {"nodes": {"n1": {"owner": "Bob"}}}})));
    let snapshot = ok(a.run_value(&json!({"op": "export"})))["snapshot"].clone();
    let content = ok(a.run_value(&json!({"op": "content"})));
    let request = json!({"op": "load", "snapshot": snapshot, "sequence": content["sequence"],
        "content": {"contentRevision": content["contentRevision"], "nodes": content["nodes"], "edges": content["edges"]}});
    let mut by_text = EngineSession::new();
    let mut by_value = EngineSession::new();
    let text: Value = serde_json::from_str(&by_text.run(&request.to_string())).unwrap();
    assert_eq!(ok(text), ok(by_value.run_value(&request)));
    for s in [&mut by_text, &mut by_value] {
        assert_eq!(
            ok(s.run_value(&json!({"op": "export"})))["snapshot"],
            snapshot
        );
        let n1 = ok(s.run_value(&json!({"op": "details", "nodeId": "n1"})));
        assert_eq!(
            (
                n1["node"]["properties"]["owner"].clone(),
                n1["contentRevision"].clone()
            ),
            (json!("Bob"), json!(2))
        );
        let log = ok(s.run_value(&json!({"op": "changesSince", "sequence": 1})));
        assert_eq!(
            (log["resync"].clone(), log["sequence"].clone()),
            (json!(true), json!(2))
        );
        assert_eq!(
            ok(s.run_value(&json!({"op": "payload", "set": {"nodes": {"n2": {"x": 1}}}})))
                ["sequence"],
            3
        );
    }
}
