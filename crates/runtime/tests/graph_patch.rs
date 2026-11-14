//! The `patch` operation (provisional): the graph of a view changes without a new import, and the view keeps its state.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn ok(response: Value) -> Value {
    assert_eq!(response["ok"], true, "{response}");
    response["snapshot"].clone()
}

fn edit(snapshot: &Value, request: Value) -> Value {
    let mut r = request;
    r["snapshot"] = snapshot.clone();
    r["expectedRevision"] = snapshot["revision"].clone();
    r["expectedLayoutVersion"] = snapshot["layoutVersion"].clone();
    call(r)
}

/// Ten packages of six files, collapsed; between the packages only aggregated edges with a weight (as a host feeds them
/// first), each standing for the member edges `member_edges` gives.
fn packages() -> Value {
    let mut nodes = Vec::new();
    for p in 0..10 {
        nodes.push(json!({"id": format!("p{p}"), "kind": "package", "label": format!("p{p}")}));
        for f in 0..6 {
            nodes.push(json!({"id": format!("p{p}/f{f}"), "kind": "file", "label": format!("f{f}"), "parentId": format!("p{p}")}));
        }
    }
    let mut edges = Vec::new();
    for p in 0..10 {
        for q in [(p + 1) % 10, (p + 3) % 10] {
            edges.push(
                json!({"id": format!("agg:p{p}-p{q}"), "kind": "calls", "source": format!("p{p}"),
                "target": format!("p{q}"), "weight": 6}),
            );
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

/// The member edges of package `p`: each file calls a file of the packages it has aggregated edges to.
fn member_edges(p: usize) -> Vec<Value> {
    let mut out = Vec::new();
    for q in [(p + 1) % 10, (p + 3) % 10] {
        for f in 0..6 {
            out.push(json!({"id": format!("m:p{p}/f{f}-p{q}"), "kind": "calls", "source": format!("p{p}/f{f}"),
                "target": format!("p{q}/f{}", (f + 1) % 6)}));
        }
    }
    out
}

fn boxes(s: &Value) -> serde_json::Map<String, Value> {
    s["boxes"].as_object().unwrap().clone()
}

#[test]
fn opening_a_package_and_loading_its_member_edges_keeps_the_view() {
    let first = ok(call(
        json!({"op": "import", "graph": packages(), "limit": 100, "options": {"levelOfDetail": {"depth": 0}}}),
    ));
    assert_eq!(first["collapsedIds"].as_array().unwrap().len(), 10);
    let opened = ok(edit(&first, json!({"op": "toggle", "nodeId": "p0"})));
    // Load p0's member edges and drop its aggregated ones.
    let patched = ok(edit(
        &opened,
        json!({"op": "patch", "patch": {"edges": member_edges(0), "removeEdges": ["agg:p0-p1", "agg:p0-p3"]}}),
    ));
    assert_eq!(
        patched["revision"],
        opened["revision"].as_u64().unwrap() + 1
    );
    assert_eq!(
        patched["layoutVersion"],
        opened["layoutVersion"].as_u64().unwrap() + 1
    );
    // The view state stays: p0 open, the others collapsed.
    let collapsed: Vec<&str> = patched["collapsedIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(collapsed.len(), 9);
    assert!(!collapsed.contains(&"p0"));
    // The new edges are in the source graph, the removed ones are not.
    let source: Vec<&str> = patched["sourceGraph"]["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["id"].as_str().unwrap())
        .collect();
    assert!(source.contains(&"m:p0/f0-p1") && !source.contains(&"agg:p0-p1"));
    // The scene draws them from p0's files to the collapsed packages.
    let scene = call(json!({"op": "scene", "snapshot": patched}));
    let routes = scene["scene"]["routes"].as_array().unwrap();
    assert!(routes.iter().any(|r| r["source"] == "p0/f0"), "{routes:?}");
    // An abstracting view is laid out from scratch, as a toggle lays it out: the drawing depends only on the graph and the
    // view, so it is the one an import of the patched graph gets with the same container opened.
    let mut graph = packages();
    let edges = graph["edges"].as_array_mut().unwrap();
    edges.retain(|e| e["id"] != "agg:p0-p1" && e["id"] != "agg:p0-p3");
    edges.extend(member_edges(0));
    let fresh = ok(call(
        json!({"op": "import", "graph": graph, "limit": 100, "options": {"levelOfDetail": {"depth": 0}}}),
    ));
    let fresh = ok(edit(&fresh, json!({"op": "toggle", "nodeId": "p0"})));
    assert_eq!(
        boxes(&patched),
        boxes(&fresh),
        "the same drawing as a fresh import"
    );
}

#[test]
fn removals_take_what_they_hold_and_bad_patches_are_refused() {
    let s = ok(call(
        json!({"op": "import", "graph": packages(), "limit": 100}),
    ));
    // Removing a package takes its files and every edge at them.
    let less = ok(edit(
        &s,
        json!({"op": "patch", "patch": {"removeNodes": ["p9"]}}),
    ));
    let ids: Vec<&str> = less["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap())
        .collect();
    assert!(!ids.iter().any(|id| id.starts_with("p9")));
    assert!(less["graph"]["edges"]
        .as_array()
        .unwrap()
        .iter()
        .all(|e| e["source"] != "p9" && e["target"] != "p9"));
    // Adding a node into a package and an edge to it; an edge without a target is counted, not drawn.
    let more = ok(edit(
        &s,
        json!({"op": "patch", "patch": {
            "nodes": [{"id": "p1/new", "kind": "file", "label": "new", "parentId": "p1"}],
            "edges": [{"id": "n1", "kind": "calls", "source": "p1/f0", "target": "p1/new"},
                      {"id": "n2", "kind": "calls", "source": "p1/f0", "target": null}]}}),
    ));
    assert!(more["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["id"] == "p1/new"));
    assert_eq!(
        more["omittedUnresolvedEdges"].as_u64().unwrap_or(0),
        s["omittedUnresolvedEdges"].as_u64().unwrap_or(0) + 1
    );
    for (bad, says) in [
        (json!({"removeNodes": ["nowhere"]}), "Unknown node"),
        (json!({"removeEdges": ["nowhere"]}), "Unknown edge"),
        (
            json!({"nodes": [{"id": "p1", "kind": "x", "label": "x"}]}),
            "Node exists",
        ),
        (
            json!({"edges": [{"id": "agg:p0-p1", "kind": "x", "source": "p0", "target": "p1"}]}),
            "Edge exists",
        ),
        (
            json!({"edges": [{"id": "z", "kind": "x", "source": "p0", "target": "ghost"}]}),
            "",
        ),
        (json!({"colour": "red"}), "patch must be"),
    ] {
        let r = edit(&s, json!({"op": "patch", "patch": bad.clone()}));
        assert_eq!(r["ok"], false, "{bad}");
        assert!(r["error"].as_str().unwrap().contains(says), "{bad}: {r}");
    }
    // A stale patch is refused; an empty one changes nothing.
    let mut stale = json!({"op": "patch", "snapshot": s, "patch": {"removeNodes": ["p9"]}, "expectedRevision": 99,
        "expectedLayoutVersion": s["layoutVersion"]});
    assert_eq!(call(stale.take())["ok"], false);
    let same = ok(edit(&s, json!({"op": "patch", "patch": {}})));
    assert_eq!(same["revision"], s["revision"]);
}

#[test]
fn a_resident_session_patches_too() {
    let mut session = arqlet_runtime::engine_session::EngineSession::new();
    session.run_value(&json!({"op": "import", "graph": packages(), "limit": 100, "scene": false}));
    let answer = session
        .run_value(&json!({"op": "patch", "patch": {"edges": member_edges(2)}, "scene": true}));
    assert_eq!(answer["ok"], true, "{answer}");
    assert!(answer["scene"]["routes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == "m:p2/f0-p3"));
}
