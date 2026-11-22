//! ADR 0023, step R1: the content of a graph (properties per node and edge) kept by a resident session beside the
//! structure, read by `details` and `find`, never by layout and never in a snapshot.
use arqlet_runtime::engine_session::EngineSession;
use serde_json::{json, Value};

fn graph() -> Value {
    json!({
        "nodes": [
            {"id": "pkg", "kind": "package", "label": "pkg"},
            {"id": "a", "kind": "fn", "label": "parse", "parentId": "pkg"},
            {"id": "b", "kind": "fn", "label": "render", "parentId": "pkg"},
            {"id": "x", "kind": "fn", "label": "main"},
        ],
        "edges": [{"id": "xa", "kind": "calls", "source": "x", "target": "a"}],
    })
}

fn ok(answer: Value) -> Value {
    assert_eq!(answer["ok"], true, "{answer}");
    answer
}

#[test]
fn a_session_keeps_content_beside_the_structure() {
    let mut s = EngineSession::new();
    ok(s.run_value(&json!({"op": "import", "graph": graph(), "limit": 100, "scene": false})));
    let before = ok(s.run_value(&json!({"op": "export"})))["snapshot"].to_string();

    // Set: integers stay integers; an id the structure does not have is kept and counted.
    let set = ok(s.run_value(&json!({"op": "payload", "set": {
        "nodes": {"a": {"owner": "Ann", "loc": 120}, "b": {"owner": "Bob", "loc": 7.5}, "ghost": {"owner": "Gil"}},
        "edges": {"xa": {"since": 2024}}}})));
    assert_eq!(set["contentRevision"], 1);
    assert_eq!(
        (
            set["nodes"].clone(),
            set["edges"].clone(),
            set["unknownNodes"].clone()
        ),
        (json!(3), json!(1), json!(1))
    );
    assert_eq!(set["keys"], json!(["loc", "owner", "since"]));

    // The snapshot is unchanged: content never travels with it, and layout never reads it.
    assert_eq!(
        ok(s.run_value(&json!({"op": "export"})))["snapshot"].to_string(),
        before
    );

    // details answers the properties and the content's revision.
    let a = ok(s.run_value(&json!({"op": "details", "nodeId": "a"})));
    assert_eq!(a["node"]["properties"], json!({"loc": 120, "owner": "Ann"}));
    assert_eq!(a["contentRevision"], 1);
    let xa = ok(s.run_value(&json!({"op": "details", "edgeId": "xa"})));
    assert_eq!(xa["edge"]["properties"], json!({"since": 2024}));
    let x = ok(s.run_value(&json!({"op": "details", "nodeId": "x"})));
    assert!(
        x["node"].get("properties").is_none(),
        "omitted when a node has none"
    );

    // find searches a named key; the matched value comes along.
    let found = ok(s.run_value(&json!({"op": "find", "query": "b", "key": "owner"})));
    assert_eq!(found["matches"][0]["id"], "b");
    assert_eq!(found["matches"][0]["value"], "Bob");
    assert_eq!(found["total"], 1);
    assert_eq!(found["contentRevision"], 1);
    let by_number = ok(s.run_value(&json!({"op": "find", "query": "120", "key": "loc"})));
    assert_eq!(by_number["matches"][0]["id"], "a");
    // Without a key, find is as before (labels and ids), without a content revision.
    let plain = ok(s.run_value(&json!({"op": "find", "query": "parse"})));
    assert!(plain.get("contentRevision").is_none());

    // Refused, and nothing changes: a key of the model itself, a stale revision.
    let virtual_key =
        s.run_value(&json!({"op": "payload", "set": {"nodes": {"a": {"label": "other", "x": 1}}}}));
    assert_eq!(virtual_key["ok"], false, "{virtual_key}");
    let stale = s.run_value(
        &json!({"op": "payload", "expectedContentRevision": 0, "set": {"nodes": {"a": {"x": 1}}}}),
    );
    assert_eq!(stale["ok"], false, "{stale}");
    assert!(
        ok(s.run_value(&json!({"op": "details", "nodeId": "a"})))["node"]["properties"]
            .get("x")
            .is_none()
    );

    // Setting the same value again changes nothing (the revision stays); unset and clear do.
    assert_eq!(
        ok(s.run_value(&json!({"op": "payload", "set": {"nodes": {"a": {"owner": "Ann"}}}})))
            ["contentRevision"],
        1
    );
    let unset = ok(s.run_value(&json!({"op": "payload", "expectedContentRevision": 1, "unset": {"nodes": {"a": ["owner", "loc"]}}})));
    assert_eq!(
        (unset["contentRevision"].clone(), unset["nodes"].clone()),
        (json!(2), json!(2))
    );
    assert!(
        ok(s.run_value(&json!({"op": "details", "nodeId": "a"})))["node"]
            .get("properties")
            .is_none()
    );

    // load (a hand-over of the same graph) keeps the content; a new import starts it empty.
    let snapshot: Value = serde_json::from_str(&before).unwrap();
    ok(s.run_value(&json!({"op": "load", "snapshot": snapshot})));
    assert_eq!(
        ok(s.run_value(&json!({"op": "details", "nodeId": "b"})))["node"]["properties"]["owner"],
        "Bob"
    );
    let cleared = ok(s.run_value(&json!({"op": "payload", "clear": true})));
    assert_eq!(
        (cleared["nodes"].clone(), cleared["contentRevision"].clone()),
        (json!(0), json!(3))
    );
    ok(s.run_value(&json!({"op": "payload", "set": {"nodes": {"b": {"owner": "Bob"}}}})));
    ok(s.run_value(&json!({"op": "import", "graph": graph(), "limit": 100, "scene": false})));
    assert!(
        ok(s.run_value(&json!({"op": "details", "nodeId": "b"})))["node"]
            .get("properties")
            .is_none()
    );
    // Through the text path (one-pass import) as well.
    ok(s.run_value(&json!({"op": "payload", "set": {"nodes": {"b": {"owner": "Bob"}}}})));
    let reimport: Value =
        serde_json::from_str(&s.run(
            &json!({"op": "import", "graph": graph(), "limit": 100, "scene": false}).to_string(),
        ))
        .unwrap();
    ok(reimport);
    assert!(
        ok(s.run_value(&json!({"op": "details", "nodeId": "b"})))["node"]
            .get("properties")
            .is_none()
    );

    // find by key without content in the stateless path is refused; payload needs a session.
    let stateless = serde_json::from_str::<Value>(&arqlet_runtime::dispatch(
        &json!({"op": "payload", "set": {"nodes": {"a": {"x": 1}}}}).to_string(),
    ))
    .unwrap();
    assert_eq!(stateless["ok"], false);
    assert!(stateless["error"]
        .as_str()
        .unwrap()
        .contains("resident session"));

    // The typed contract reads the answer back.
    ok(s.run_value(&json!({"op": "payload", "set": {"nodes": {"a": {"owner": "Ann"}}}})));
    let mut details = ok(s.run_value(&json!({"op": "details", "nodeId": "a"})));
    details.as_object_mut().unwrap().remove("ok");
    let typed: arqlet_model::DetailsAnswer = serde_json::from_value(details.clone()).unwrap();
    assert_eq!(serde_json::to_value(&typed).unwrap(), details);
}
