//! The `neighbourhood` operation (provisional): the nodes within some hops of a node, by direction and edge kind, and
//! what of them the view shows, as for `trace`, so that `reveal` can open them.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn ok(response: Value) -> Value {
    assert_eq!(response["ok"], true, "{response}");
    response["snapshot"].clone()
}

/// Two packages with files: p0/f0 calls p0/f1 and p1/f0; p1/f0 imports p1/f1; p1/f2 calls p0/f0.
fn graph() -> Value {
    let mut nodes = Vec::new();
    for p in ["p0", "p1"] {
        nodes.push(json!({"id": p, "kind": "package", "label": p}));
        for f in 0..3 {
            nodes.push(json!({"id": format!("{p}/f{f}"), "kind": "file", "label": format!("f{f}"), "parentId": p}));
        }
    }
    let edges = vec![
        json!({"id": "a", "kind": "calls", "source": "p0/f0", "target": "p0/f1"}),
        json!({"id": "b", "kind": "calls", "source": "p0/f0", "target": "p1/f0"}),
        json!({"id": "c", "kind": "imports", "source": "p1/f0", "target": "p1/f1"}),
        json!({"id": "d", "kind": "calls", "source": "p1/f2", "target": "p0/f0"}),
    ];
    json!({"nodes": nodes, "edges": edges})
}

fn around(snapshot: &Value, extra: Value) -> Value {
    let mut request = json!({"op": "neighbourhood", "snapshot": snapshot, "nodeId": "p0/f0"});
    for (k, v) in extra.as_object().unwrap() {
        request[k] = v.clone();
    }
    call(request)
}

#[test]
fn the_neighbourhood_follows_hops_direction_and_kinds() {
    let open = ok(call(
        json!({"op": "import", "graph": graph(), "limit": 100}),
    ));
    let one = around(&open, json!({}));
    assert_eq!(one["ok"], true, "{one}");
    assert_eq!(one["nodes"], json!(["p0/f0", "p0/f1", "p1/f0", "p1/f2"]));
    assert_eq!(one["edges"], json!(["a", "b", "d"]));
    assert_eq!(one["hops"]["p1/f2"], 1);
    // Out only: what p0/f0 leads to; two hops reach p1/f1 through the import.
    let out = around(&open, json!({"direction": "out", "hops": 2}));
    assert_eq!(out["nodes"], json!(["p0/f0", "p0/f1", "p1/f0", "p1/f1"]));
    assert_eq!(out["hops"]["p1/f1"], 2);
    // Calls only: the import is not followed.
    let calls = around(
        &open,
        json!({"direction": "out", "hops": 2, "edgeKinds": ["calls"]}),
    );
    assert_eq!(calls["nodes"], json!(["p0/f0", "p0/f1", "p1/f0"]));
    // In only: who calls p0/f0.
    let into = around(&open, json!({"direction": "in"}));
    assert_eq!(into["nodes"], json!(["p0/f0", "p1/f2"]));
    // Open view: every node is visible as itself, every edge is its own route.
    assert_eq!(one["visible"]["nodes"], one["nodes"]);
    assert_eq!(one["visible"]["routes"], json!(["a", "b", "d"]));
}

#[test]
fn a_collapsed_view_shows_the_neighbourhood_in_its_boxes_and_reveal_opens_them() {
    let collapsed = ok(call(json!({"op": "import", "graph": graph(), "limit": 100,
        "options": {"levelOfDetail": {"depth": 0}}})));
    let n = around(&collapsed, json!({"direction": "out"}));
    assert_eq!(
        n["visible"]["nodes"],
        json!(["p0", "p1"]),
        "the packages hold them"
    );
    assert_eq!(
        n["visible"]["routes"].as_array().unwrap().len(),
        1,
        "one route between the packages: {n}"
    );
    assert_eq!(n["distance"]["p0"], 0);
    // The nodes go to reveal as they are: the packages open as far as needed.
    let revealed = ok(call(
        json!({"op": "reveal", "snapshot": collapsed, "nodeIds": n["nodes"],
        "expectedRevision": collapsed["revision"], "expectedLayoutVersion": collapsed["layoutVersion"]}),
    ));
    let again = around(&revealed, json!({"direction": "out"}));
    assert_eq!(again["visible"]["nodes"], again["nodes"]);
}

#[test]
fn bad_requests_are_refused_and_a_session_answers_the_same() {
    let open = ok(call(
        json!({"op": "import", "graph": graph(), "limit": 100}),
    ));
    for (bad, says) in [
        (json!({"nodeId": "nowhere"}), "Unknown node"),
        (json!({"hops": -1}), "hops"),
        (json!({"hops": 1.5}), "hops"),
        (json!({"direction": "up"}), "direction"),
        (json!({"edgeKinds": []}), "edgeKinds"),
        (json!({"nodeIds": []}), "nodeId"),
    ] {
        let answer = around(&open, bad.clone());
        assert_eq!(answer["ok"], false, "{bad}");
        assert!(
            answer["error"].as_str().unwrap().contains(says),
            "{bad}: {answer}"
        );
    }
    let neither = call(json!({"op": "neighbourhood", "snapshot": open}));
    assert!(neither["error"].as_str().unwrap().contains("nodeId"));
    // Several starts, zero hops: the starts alone.
    let both = call(
        json!({"op": "neighbourhood", "snapshot": open, "nodeIds": ["p0/f1", "p1/f1"], "hops": 0}),
    );
    assert_eq!(both["nodes"], json!(["p0/f1", "p1/f1"]));
    assert_eq!(both["edges"], json!([]));
    // A resident session gives the same answer.
    let mut session = arqlet_runtime::engine_session::EngineSession::new();
    session.run_value(&json!({"op": "import", "graph": graph(), "limit": 100, "scene": false}));
    let resident = session.run_value(
        &json!({"op": "neighbourhood", "nodeId": "p0/f0", "direction": "out", "hops": 2}),
    );
    assert_eq!(
        resident,
        around(&open, json!({"direction": "out", "hops": 2}))
    );
}

#[test]
fn read_answers_are_the_typed_contracts() {
    use arqlet_runtime::protocol::{handle, Request, Response};
    let open = ok(call(
        json!({"op": "import", "graph": graph(), "limit": 100}),
    ));
    // Every JSON answer, without `ok`, reads back as its contract type (unknown fields refused) and writes back the same.
    fn check<T: serde::de::DeserializeOwned + serde::Serialize>(mut answer: Value) {
        assert_eq!(answer["ok"], true, "{answer}");
        answer.as_object_mut().unwrap().remove("ok");
        let typed: T = serde_json::from_value(answer.clone()).unwrap();
        assert_eq!(serde_json::to_value(&typed).unwrap(), answer);
    }
    check::<arqlet_model::TraceAnswer>(call(
        json!({"op": "trace", "snapshot": open, "from": "p0/f0", "to": "p1/f1"}),
    ));
    check::<arqlet_model::TraceAnswer>(call(
        json!({"op": "trace", "snapshot": open, "from": "p1/f1", "to": "p0/f2"}),
    ));
    check::<arqlet_model::NeighbourhoodAnswer>(around(&open, json!({"hops": 2})));
    // The typed protocol hands Rust hosts the values themselves.
    let request =
        Request::from_value(&json!({"op": "neighbourhood", "snapshot": open, "nodeId": "p0/f0"}))
            .unwrap();
    let Response::Neighbourhood(answer) = handle(request) else {
        panic!("a typed neighbourhood answer")
    };
    assert!(answer.nodes.contains(&"p0/f1".to_string()));
    let request = Request::from_value(&json!({"op": "scene", "snapshot": open})).unwrap();
    assert!(matches!(handle(request), Response::Scene(_)));
    // A refused read is an error, as before.
    let request = Request::from_value(
        &json!({"op": "trace", "snapshot": open, "from": "nowhere", "to": "p0/f0"}),
    )
    .unwrap();
    assert!(matches!(handle(request), Response::Error(_)));
}
