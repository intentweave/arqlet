//! The `details` operation (provisional): what the engine knows about a node or an edge of the view (the containers above
//! a node, what a collapsed box holds, the edges a drawn line stands for), read only.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn ok(response: Value) -> Value {
    assert_eq!(response["ok"], true, "{response}");
    response["snapshot"].clone()
}

/// A package `p` with files a and b (a calls b), a node x outside: a and b call x, x calls a back.
fn graph() -> Value {
    json!({
        "nodes": [
            {"id": "p", "kind": "package", "label": "p"},
            {"id": "a", "kind": "file", "label": "a", "parentId": "p"},
            {"id": "b", "kind": "file", "label": "b", "parentId": "p"},
            {"id": "x", "kind": "file", "label": "x"},
        ],
        "edges": [
            {"id": "ab", "kind": "calls", "source": "a", "target": "b"},
            {"id": "ax", "kind": "calls", "source": "a", "target": "x"},
            {"id": "bx", "kind": "calls", "source": "b", "target": "x"},
            {"id": "xa", "kind": "calls", "source": "x", "target": "a"},
        ],
    })
}

fn details(snapshot: &Value, what: Value) -> Value {
    let mut request = json!({"op": "details", "snapshot": snapshot});
    for (k, v) in what.as_object().unwrap() {
        request[k] = v.clone();
    }
    let answer = call(request);
    assert_eq!(answer["ok"], true, "{answer}");
    answer
}

#[test]
fn details_name_what_a_node_and_a_line_stand_for() {
    let open = ok(call(
        json!({"op": "import", "graph": graph(), "limit": 100}),
    ));
    // Open: a is visible as itself, inside p; its line to x carries only itself.
    let a = details(&open, json!({"nodeId": "a"}))["node"].clone();
    assert_eq!(a["parents"], json!(["p"]));
    assert_eq!(a["shownAs"], "a");
    assert_eq!(
        (a["incoming"].as_u64(), a["outgoing"].as_u64()),
        (Some(1), Some(2))
    );
    let p_box = details(&open, json!({"nodeId": "p"}))["node"]["bounds"].clone();
    let a_box = a["bounds"].clone();
    assert!(
        a_box["x"].as_f64().unwrap() >= p_box["x"].as_f64().unwrap(),
        "absolute: a lies inside p"
    );
    let line = details(&open, json!({"edgeId": "ax"}));
    assert_eq!(line["edge"]["route"], "ax");
    assert_eq!(line["route"]["edges"], json!(["ax"]));
    assert_eq!(line["route"]["source"], "a");
    // Collapsed: a is shown by p, and one line from p to x stands for both calls; x's call back has its own line.
    let closed = ok(call(
        json!({"op": "toggle", "snapshot": open, "nodeId": "p",
        "expectedRevision": open["revision"], "expectedLayoutVersion": open["layoutVersion"]}),
    ));
    let a = details(&closed, json!({"nodeId": "a"}))["node"].clone();
    assert_eq!(a["shownAs"], "p");
    let p = details(&closed, json!({"nodeId": "p"}))["node"].clone();
    assert_eq!(
        (
            p["collapsed"].as_bool(),
            p["children"].as_u64(),
            p["descendants"].as_u64()
        ),
        (Some(true), Some(2), Some(2))
    );
    // Toggled closed, each call keeps its own line from p.
    let bx = details(&closed, json!({"edgeId": "bx"}));
    assert_eq!(bx["edge"]["route"], "bx");
    assert_eq!(
        (
            bx["route"]["source"].as_str(),
            bx["route"]["edgeCount"].as_u64()
        ),
        (Some("p"), Some(1))
    );
    assert!(bx["route"]["length"].as_f64().unwrap() > 0.0);
    // An abstract view (lifted edges): one line from p to x stands for both calls, and x's call back in the other
    // direction.
    let abstract_view = ok(call(json!({"op": "import", "graph": graph(), "limit": 100,
        "options": {"levelOfDetail": {"depth": 0}}})));
    let bx = details(&abstract_view, json!({"edgeId": "bx"}));
    let carrier = bx["edge"]["route"]
        .as_str()
        .expect("bx is carried by a line")
        .to_string();
    let route = &bx["route"];
    assert_eq!(route["id"], carrier.as_str());
    assert_eq!(
        (route["source"].as_str(), route["target"].as_str()),
        (Some("p"), Some("x"))
    );
    assert_eq!(
        (route["edges"].clone(), route["edgeCount"].clone()),
        (json!(["ax", "bx"]), json!(2)),
        "{route}"
    );
    assert_eq!(
        (route["opposite"].clone(), route["oppositeCount"].clone()),
        (json!(["xa"]), json!(1))
    );
    assert_eq!(route["weight"], 2);
    // An edge inside the box is not drawn.
    let inside = details(&closed, json!({"edgeId": "ab"}));
    assert_eq!(inside["edge"]["route"], Value::Null);
    assert!(inside.get("route").is_none());
    // At most `limit` ids, the count complete.
    let limited = details(&abstract_view, json!({"edgeId": carrier, "limit": 1}));
    assert_eq!(limited["route"]["edges"].as_array().unwrap().len(), 1);
    assert_eq!(limited["route"]["edgeCount"], 2);
    // Refusals.
    for bad in [
        json!({}),
        json!({"nodeId": "a", "edgeId": "ax"}),
        json!({"nodeId": "nowhere"}),
        json!({"edgeId": "nowhere"}),
    ] {
        let mut request = json!({"op": "details", "snapshot": closed});
        for (k, v) in bad.as_object().unwrap() {
            request[k] = v.clone();
        }
        assert_eq!(call(request)["ok"], false, "{bad}");
    }
    // The contract type reads the answer back; a resident session and the typed protocol agree.
    let mut answer = details(&closed, json!({"edgeId": "bx"}));
    answer.as_object_mut().unwrap().remove("ok");
    let typed: arqlet_model::DetailsAnswer = serde_json::from_value(answer.clone()).unwrap();
    assert_eq!(serde_json::to_value(&typed).unwrap(), answer);
    let mut session = arqlet_runtime::engine_session::EngineSession::new();
    session.run_value(&json!({"op": "load", "snapshot": closed}));
    let resident = session.run_value(&json!({"op": "details", "edgeId": "bx"}));
    assert_eq!(resident, details(&closed, json!({"edgeId": "bx"})));
    use arqlet_runtime::protocol::{handle, Request, Response};
    let request =
        Request::from_value(&json!({"op": "details", "snapshot": closed, "nodeId": "a"})).unwrap();
    let Response::Details(typed) = handle(request) else {
        panic!("a typed details answer")
    };
    assert_eq!(typed.node.unwrap().shown_as.as_deref(), Some("p"));
}
