//! The `peek` operation (provisional): a look into a collapsed container without opening it, as a small scene of its
//! contents, with the edges that cross its border counted per child.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

/// A package with a, b and a container c (c1, c2); x outside. a calls b and c2; x calls a; b and c1 call x.
fn graph() -> Value {
    let node = |id: &str, parent: Option<&str>| {
        let mut n = json!({"id": id, "kind": "k", "label": id});
        if let Some(p) = parent {
            n["parentId"] = json!(p);
        }
        n
    };
    let edge =
        |id: &str, s: &str, t: &str| json!({"id": id, "kind": "calls", "source": s, "target": t});
    json!({
        "nodes": [node("pkg", None), node("a", Some("pkg")), node("b", Some("pkg")), node("c", Some("pkg")),
            node("c1", Some("c")), node("c2", Some("c")), node("x", None)],
        "edges": [edge("ab", "a", "b"), edge("ac2", "a", "c2"), edge("xa", "x", "a"), edge("bx", "b", "x"),
            edge("c1x", "c1", "x")],
    })
}

#[test]
fn peek_shows_a_collapsed_container_without_opening_it() {
    let snapshot = call(json!({"op": "import", "graph": graph(), "limit": 100,
        "options": {"levelOfDetail": {"depth": 0}}}))["snapshot"]
        .clone();
    assert!(snapshot["collapsedIds"]
        .as_array()
        .unwrap()
        .contains(&json!("pkg")));
    let look = call(json!({"op": "peek", "snapshot": snapshot, "nodeId": "pkg"}));
    assert_eq!(look["ok"], true, "{look}");
    assert_eq!(
        (
            look["children"].clone(),
            look["shown"].clone(),
            look["omitted"].clone()
        ),
        (json!(3), json!(3), json!(0))
    );
    // The contents laid out on their own: the children, the container inside collapsed, its inner edges drawn.
    let ids: Vec<&str> = look["scene"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    for id in ["a", "b", "c"] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
    assert!(
        !ids.contains(&"x") && !ids.contains(&"pkg"),
        "only the contents: {ids:?}"
    );
    let c = look["scene"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == "c")
        .unwrap();
    assert_eq!(c["collapsed"], true, "{c}");
    let routes: Vec<&str> = look["scene"]["routes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert!(routes.contains(&"ab"), "{routes:?}");
    // The border: x calls a, b and c (through c1) call x.
    assert_eq!(
        look["external"],
        json!({"a": {"incoming": 1, "outgoing": 0}, "b": {"incoming": 0, "outgoing": 1},
        "c": {"incoming": 0, "outgoing": 1}})
    );
    // The view is not changed: a read, no new revision.
    let again = call(json!({"op": "intent", "snapshot": snapshot}));
    assert!(again["intent"]["collapsedIds"]
        .as_array()
        .unwrap()
        .contains(&json!("pkg")));
    // At most `limit` children, in the source order; the rest counted.
    let small = call(json!({"op": "peek", "snapshot": snapshot, "nodeId": "pkg", "limit": 2}));
    assert_eq!(
        (small["shown"].clone(), small["omitted"].clone()),
        (json!(2), json!(1))
    );
    // Refusals.
    for bad in [
        json!({"nodeId": "x"}),
        json!({"nodeId": "nowhere"}),
        json!({}),
    ] {
        let mut request = json!({"op": "peek", "snapshot": snapshot});
        for (k, v) in bad.as_object().unwrap() {
            request[k] = v.clone();
        }
        assert_eq!(call(request)["ok"], false, "{bad}");
    }
    // A resident session gives the same look.
    let mut session = arqlet_runtime::engine_session::EngineSession::new();
    session.run_value(&json!({"op": "load", "snapshot": snapshot}));
    assert_eq!(
        session.run_value(&json!({"op": "peek", "nodeId": "pkg"})),
        look
    );
}

/// A session reads an import in one pass (no JSON value of the graph); its answers and kept snapshots are the same as the
/// general way's, also for refused requests.
#[test]
fn a_session_reads_an_import_in_one_pass_with_the_same_answers() {
    use arqlet_runtime::engine_session::EngineSession;
    let cases = [
        json!({"op": "import", "graph": graph(), "limit": 100}),
        json!({"op": "import", "graph": graph(), "limit": 100, "scene": false, "options": {"levelOfDetail": {"depth": 0}}}),
        json!({"op": "import", "graph": graph(), "limit": 2, "options": null}),
        json!({"op": "import", "graph": graph(), "limit": "many"}),
        json!({"op": "import", "graph": {"nodes": [{"id": "a"}]}, "limit": 10}),
        json!({"op": "import", "graph": graph(), "limit": 100, "options": {"nothingLikeIt": 1}}),
    ];
    for request in cases {
        let mut by_text = EngineSession::new();
        let mut by_value = EngineSession::new();
        let text_answer: Value = serde_json::from_str(&by_text.run(&request.to_string())).unwrap();
        let value_answer = by_value.run_value(&request);
        assert_eq!(text_answer, value_answer, "{request}");
        assert_eq!(
            by_text
                .snapshot()
                .map(|s| serde_json::to_string(s).unwrap()),
            by_value
                .snapshot()
                .map(|s| serde_json::to_string(s).unwrap()),
            "{request}"
        );
    }
}
