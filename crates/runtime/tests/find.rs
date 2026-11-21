//! The `find` operation (provisional): nodes of the source graph by label or id, best first, with the box that shows
//! each, also when the view hides them in a collapsed container.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn graph() -> Value {
    let node = |id: &str, label: &str, kind: &str, parent: Option<&str>| {
        let mut n = json!({"id": id, "kind": kind, "label": label});
        if let Some(p) = parent {
            n["parentId"] = json!(p);
        }
        n
    };
    json!({
        "nodes": [
            node("pkg", "Package", "package", None),
            node("n1", "parse", "fn", Some("pkg")),
            node("n2", "Parser", "fn", Some("pkg")),
            node("n3", "tree_parse", "fn", Some("pkg")),
            node("n4", "sparse", "fn", Some("pkg")),
            node("n5", "Main", "file", None),
            node("parse-id", "Other", "file", None),
        ],
        "edges": [{"id": "e", "kind": "calls", "source": "n5", "target": "n1"}],
    })
}

fn ids(answer: &Value) -> Vec<String> {
    answer["matches"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn find_ranks_matches_and_names_the_box_that_shows_them() {
    let snapshot = call(json!({"op": "import", "graph": graph(), "limit": 100,
        "options": {"levelOfDetail": {"depth": 0}}}))["snapshot"]
        .clone();
    let find = |extra: Value| {
        let mut request = json!({"op": "find", "snapshot": snapshot});
        for (k, v) in extra.as_object().unwrap() {
            request[k] = v.clone();
        }
        call(request)
    };
    // Exact label first, then prefixes (by label, then id), then a word start, then anywhere; case ignored.
    let all = find(json!({"query": "PARSE"}));
    assert_eq!(all["ok"], true, "{all}");
    assert_eq!(ids(&all), ["n1", "parse-id", "n2", "n3", "n4"]);
    assert_eq!(all["total"], 5);
    // Inside the collapsed package: shown by it, with its parents.
    let first = &all["matches"][0];
    assert_eq!(first["shownAs"], "pkg");
    assert_eq!(first["parents"], json!(["pkg"]));
    // Limit and kinds.
    let one = find(json!({"query": "parse", "limit": 1}));
    assert_eq!(
        (ids(&one), one["total"].clone()),
        (vec!["n1".to_string()], json!(5))
    );
    assert_eq!(
        ids(&find(json!({"query": "parse", "nodeKinds": ["file"]}))),
        ["parse-id"]
    );
    assert_eq!(find(json!({"query": "nothing like it"}))["total"], 0);
    // Refusals.
    assert_eq!(find(json!({"query": "  "}))["ok"], false);
    assert_eq!(find(json!({}))["ok"], false);
    // The contract type reads it back; a resident session agrees.
    let mut answer = all.clone();
    answer.as_object_mut().unwrap().remove("ok");
    let typed: arqlet_model::FindAnswer = serde_json::from_value(answer.clone()).unwrap();
    assert_eq!(serde_json::to_value(&typed).unwrap(), answer);
    let mut session = arqlet_runtime::engine_session::EngineSession::new();
    session.run_value(&json!({"op": "load", "snapshot": snapshot}));
    assert_eq!(
        session.run_value(&json!({"op": "find", "query": "PARSE"})),
        all
    );
    // The graph's fingerprint, for bookmarks: the same in a session, another for another graph.
    let digest = call(json!({"op": "digest", "snapshot": snapshot}));
    assert_eq!(digest["ok"], true);
    assert_eq!(session.run_value(&json!({"op": "digest"})), digest);
    let mut changed = graph();
    changed["nodes"][1]["label"] = json!("renamed");
    let other = call(json!({"op": "import", "graph": changed, "limit": 100}))["snapshot"].clone();
    assert_ne!(
        call(json!({"op": "digest", "snapshot": other}))["digest"],
        digest["digest"]
    );
}
