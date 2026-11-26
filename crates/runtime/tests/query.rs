//! ADR 0021, stage M5b: the operation `query` in a resident session and through the stateless `dispatch`, over the source
//! graph and the content of ADR 0023 R1. Only with the cargo feature `query`.
#![cfg(feature = "query")]
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
        "edges": [
            {"id": "xa", "kind": "calls", "source": "x", "target": "a"},
            {"id": "ab", "kind": "calls", "source": "a", "target": "b"},
            {"id": "lost", "kind": "calls", "source": "x", "target": null},
        ],
    })
}

fn ok(answer: Value) -> Value {
    assert_eq!(answer["ok"], true, "{answer}");
    answer
}

fn session() -> EngineSession {
    let mut s = EngineSession::new();
    ok(s.run_value(&json!({"op": "import", "graph": graph(), "limit": 100, "scene": false})));
    s
}

#[test]
fn a_query_reads_structure_and_the_sessions_content() {
    let mut s = session();
    ok(s.run_value(&json!({"op": "payload", "set": {
        "nodes": {"a": {"loc": 120}, "b": {"loc": 7}, "ghost": {"loc": 1}},
        "edges": {"xa": {"since": 2024}}}})));
    let a = ok(s.run_value(&json!({
        "op": "query",
        "query": "MATCH (x)-[r:calls]->(f:fn) WHERE f.loc > $min RETURN x.label, f.label, r.since ORDER BY f.label",
        "params": {"min": 10},
    })));
    assert_eq!(a["columns"], json!(["x.label", "f.label", "r.since"]));
    assert_eq!(a["rows"], json!([["main", "parse", 2024]]));
    assert_eq!(a["rowCount"], 1);
    // The content's revision and the ids that matched nothing are reported, never silently dropped.
    assert_eq!(a["contentRevision"], 1);
    assert_eq!(a["ignoredContent"], 1);
}

/// The session's content and the payload of the standalone adapter refuse the same keys (ADR 0021, M0 item 1).
#[test]
fn content_and_the_adapters_payload_refuse_the_same_keys() {
    assert_eq!(
        arqlet_runtime::content::VIRTUAL_KEYS,
        arqlet_query_model::VIRTUAL_KEYS
    );
}

#[test]
fn stateless_content_may_not_use_the_models_own_keys() {
    let mut s = session();
    let snapshot = ok(s.run_value(&json!({"op": "export"})))["snapshot"].clone();
    for content in [
        json!({"nodes": {"a": {"label": "other"}}}),
        json!({"edges": {"xa": {"weight": 9}}}),
    ] {
        let answer: Value = serde_json::from_str(&arqlet_runtime::dispatch(
            &json!({"op": "query", "query": "MATCH (n) RETURN n", "snapshot": snapshot, "content": content})
                .to_string(),
        ))
        .unwrap();
        assert_eq!(answer["ok"], false, "{answer}");
        assert!(
            answer["error"]
                .as_str()
                .unwrap()
                .contains("property of the model itself"),
            "{answer}"
        );
    }
}

#[test]
fn unresolved_edges_are_excluded_and_nodes_and_edges_come_back_as_ids() {
    let mut s = session();
    let a = ok(s.run_value(&json!({
        "op": "query", "query": "MATCH (x)-[r]->(y) RETURN x, r, y ORDER BY r.id",
    })));
    // Two edges resolve; `lost` has no target and is not seen.
    assert_eq!(
        a["rows"],
        json!([
            [{"node": "a"}, {"edge": "ab"}, {"node": "b"}],
            [{"node": "x"}, {"edge": "xa"}, {"node": "a"}],
        ])
    );
    assert_eq!(a["nodes"], json!(["a", "b", "x"]));
    assert_eq!(a["edges"], json!(["ab", "xa"]));
}

#[test]
fn the_view_is_built_only_when_asked_and_names_the_box_that_shows_a_match() {
    let mut s = session();
    let plain = ok(s.run_value(&json!({"op": "query", "query": "MATCH (n:fn) RETURN n"})));
    assert!(plain.get("shown").is_none());
    // Collapse the package: its members are shown by it.
    ok(s.run_value(&json!({"op": "toggle", "nodeId": "pkg", "scene": false})));
    let shown = ok(s.run_value(&json!({
        "op": "query", "query": "MATCH (a)-[r]->(b) RETURN r", "view": true,
    })));
    assert_eq!(
        shown["shown"]["edges"].as_object().unwrap().len(),
        2,
        "{shown}"
    );
    let q =
        ok(s.run_value(&json!({"op": "query", "query": "MATCH (n:fn) RETURN n", "view": true})));
    assert_eq!(q["shown"]["nodes"]["a"], json!("pkg"), "{q}");
    assert_eq!(q["shown"]["nodes"]["x"], json!("x"), "{q}");
}

#[test]
fn a_query_does_not_change_the_session() {
    let mut s = session();
    let before = ok(s.run_value(&json!({"op": "export"})))["snapshot"].to_string();
    ok(s.run_value(&json!({"op": "query", "query": "MATCH (n) RETURN count(*)", "view": true})));
    assert_eq!(
        ok(s.run_value(&json!({"op": "export"})))["snapshot"].to_string(),
        before
    );
}

#[test]
fn errors_name_their_class_and_a_limit_stops_the_query() {
    let mut s = session();
    let syntax = s.run_value(&json!({"op": "query", "query": "MATCH (n RETURN n"}));
    assert_eq!(syntax["ok"], false);
    assert!(
        syntax["error"]
            .as_str()
            .unwrap()
            .starts_with("query syntax:"),
        "{syntax}"
    );
    let write = s.run_value(&json!({"op": "query", "query": "CREATE (n)"}));
    assert!(
        write["error"]
            .as_str()
            .unwrap()
            .starts_with("query unsupported:"),
        "{write}"
    );
    let limit = s.run_value(&json!({
        "op": "query", "query": "MATCH (a), (b), (c) RETURN count(*)", "maxSteps": 10,
    }));
    assert!(
        limit["error"].as_str().unwrap().starts_with("query limit:"),
        "{limit}"
    );
    // No query text, a bad parameter: refused with a reason.
    assert_eq!(s.run_value(&json!({"op": "query"}))["ok"], false);
    let bad = s.run_value(&json!({"op": "query", "query": "RETURN $p", "params": {"p": u64::MAX}}));
    assert!(bad["error"].as_str().unwrap().contains("$p"), "{bad}");
}

#[test]
fn a_map_valued_property_and_a_map_parameter_are_maps() {
    let mut s = session();
    ok(s.run_value(
        &json!({"op": "payload", "set": {"nodes": {"a": {"meta": {"node": "x", "k": [1]}}}}}),
    ));
    let a = ok(s.run_value(&json!({
        "op": "query",
        "query": "MATCH (a) WHERE a.id = 'a' AND a.meta.k[0] = $p.one RETURN a.meta, a",
        "params": {"p": {"one": 1}},
    })));
    // A map is tagged, so its key `node` is not taken for a node; the ids of the answer name nodes only.
    assert_eq!(
        a["rows"],
        json!([[{"map": {"k": [1], "node": "x"}}, {"node": "a"}]])
    );
    assert_eq!(a["nodes"], json!(["a"]));
    assert!(a.get("skippedValues").is_none(), "{a}");
}

#[test]
fn a_property_the_query_layer_has_no_type_for_reads_as_null_and_is_counted() {
    let mut s = session();
    ok(s.run_value(&json!({"op": "payload", "set": {"nodes": {"a": {"big": u64::MAX, "n": 3}}}})));
    let a = ok(s.run_value(&json!({
        "op": "query", "query": "MATCH (a) WHERE a.id = 'a' RETURN a.big, a.n",
    })));
    assert_eq!(a["rows"], json!([[null, 3]]));
    assert_eq!(a["skippedValues"], 1);
}

#[test]
fn stateless_dispatch_answers_the_same_as_the_session() {
    let mut s = session();
    let content = json!({"nodes": {"a": {"loc": 120}}});
    ok(s.run_value(&json!({"op": "payload", "set": content.clone()})));
    let question = "MATCH (n:fn) WHERE n.loc IS NOT NULL RETURN n.label, n.loc";
    let in_session = ok(s.run_value(&json!({"op": "query", "query": question})));
    let snapshot = ok(s.run_value(&json!({"op": "export"})))["snapshot"].clone();
    let stateless: Value = serde_json::from_str(&arqlet_runtime::dispatch(
        &json!({"op": "query", "query": question, "snapshot": snapshot, "content": content})
            .to_string(),
    ))
    .unwrap();
    assert_eq!(
        ok(stateless.clone())["rows"],
        in_session["rows"],
        "{stateless}"
    );
    assert_eq!(in_session["rows"], json!([["parse", 120]]));
}
