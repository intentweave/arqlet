//! Input edge weights (provisional, `Edge::weight`): an edge that stands for several counts as many wherever edges are
//! counted, and a graph without weights is laid out exactly as before.
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

/// Two packages of files; edges between them in both directions, some pre-aggregated (with a weight), plus a parallel
/// pair and a self-loop.
fn graph(weighted: bool) -> Value {
    let w = |n: u32| if weighted { json!(n) } else { Value::Null };
    let mut nodes = Vec::new();
    for p in ["p0", "p1"] {
        nodes.push(json!({"id": p, "kind": "package", "label": p}));
        for f in 0..3 {
            nodes.push(json!({"id": format!("{p}/f{f}"), "kind": "file", "label": format!("f{f}"), "parentId": p}));
        }
    }
    let mut edges = vec![
        json!({"id": "a", "kind": "calls", "source": "p0/f0", "target": "p1/f0", "weight": w(5)}),
        json!({"id": "b", "kind": "calls", "source": "p0/f1", "target": "p1/f1", "weight": w(2)}),
        json!({"id": "c", "kind": "imports", "source": "p1/f2", "target": "p0/f2", "weight": w(6)}),
        json!({"id": "d", "kind": "calls", "source": "p0/f0", "target": "p0/f1"}),
        json!({"id": "e", "kind": "calls", "source": "p0/f0", "target": "p0/f1", "weight": w(3)}),
        json!({"id": "loop", "kind": "calls", "source": "p1/f0", "target": "p1/f0", "weight": w(4)}),
    ];
    for e in &mut edges {
        if e["weight"].is_null() {
            e.as_object_mut().unwrap().remove("weight");
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

fn route<'a>(scene: &'a Value, id: &str) -> &'a Value {
    scene["routes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no route {id}"))
}

#[test]
fn a_route_weighs_the_edges_it_stands_for() {
    // Open: every edge is its own route, as heavy as its weight; a bundle weighs its members.
    let open = ok(call(
        json!({"op": "import", "graph": graph(true), "limit": 100,
        "options": {"bundling": "complete"}}),
    ));
    let s = scene(&open);
    assert_eq!(route(&s, "a")["weight"], 5);
    assert_eq!(route(&s, "loop")["weight"], 4);
    // d (1) and e (3) run between the same files: one bundle of weight 4.
    let leader = route(&s, "d");
    assert_eq!(leader["weight"], 4, "{leader}");
    assert_eq!(leader["members"], json!(["e"]));
    // The weight travels with the snapshot and is kept for later operations.
    assert_eq!(open["graph"]["edges"][0]["weight"], 5);
    // Collapsed: one route between the packages, its weights the sums in both directions.
    let collapsed = ok(call(
        json!({"op": "import", "graph": graph(true), "limit": 100,
        "options": {"levelOfDetail": {"depth": 0}}}),
    ));
    let s = scene(&collapsed);
    let between: Vec<&Value> = s["routes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["source"] != r["target"])
        .collect();
    assert_eq!(between.len(), 1, "{between:?}");
    // p0 → p1: a + b = 7; p1 → p0: c = 6. The route's own direction is its weight, the other its back weight.
    let (forward, back) = if between[0]["source"] == "p0" {
        (7, 6)
    } else {
        (6, 7)
    };
    assert_eq!(between[0]["weight"], forward);
    assert_eq!(between[0]["backWeight"], back);
    // Hiding a kind recounts by weight: without imports (c), only a and b (5 + 2) remain.
    let filtered = ok(call(
        json!({"op": "filter", "snapshot": collapsed, "hide": {"edgeKinds": ["imports"]},
        "expectedRevision": collapsed["revision"], "expectedLayoutVersion": collapsed["layoutVersion"]}),
    ));
    let s = scene(&filtered);
    let r = s["routes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["source"] != r["target"])
        .unwrap();
    let total = r["weight"].as_u64().unwrap() + r["backWeight"].as_u64().unwrap_or(0);
    assert_eq!(total, 7, "{r}");
}

#[test]
fn a_bundle_that_loses_members_to_the_filter_weighs_what_it_still_carries() {
    let mut g = graph(true);
    g["edges"][4]["kind"] = json!("imports");
    let open = ok(call(json!({"op": "import", "graph": g, "limit": 100,
        "options": {"bundling": "complete"}})));
    assert_eq!(route(&scene(&open), "d")["weight"], 4);
    let filtered = ok(call(
        json!({"op": "filter", "snapshot": open, "hide": {"edgeKinds": ["imports"]},
        "expectedRevision": open["revision"], "expectedLayoutVersion": open["layoutVersion"]}),
    ));
    let d = route(&scene(&filtered), "d").clone();
    assert_eq!(d["weight"], 1, "{d}");
    assert!(
        d["members"].as_array().is_none_or(Vec::is_empty),
        "no members left"
    );
}

#[test]
fn without_weights_nothing_changes_and_a_zero_weight_is_refused() {
    // The same graph without weights: every route weighs its edges by count, as always.
    let open = ok(call(
        json!({"op": "import", "graph": graph(false), "limit": 100,
        "options": {"bundling": "complete"}}),
    ));
    let s = scene(&open);
    assert_eq!(route(&s, "a")["weight"], 1);
    assert_eq!(route(&s, "d")["weight"], 2);
    assert!(open["graph"]["edges"][0].get("weight").is_none(), "omitted");
    // Weights do not move anything: the geometry is that of the unweighted graph.
    let weighted = ok(call(
        json!({"op": "import", "graph": graph(true), "limit": 100,
        "options": {"bundling": "complete"}}),
    ));
    let points = |s: &Value| -> Vec<Value> {
        s["routes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["points"].clone())
            .collect()
    };
    assert_eq!(points(&scene(&weighted)), points(&s));
    let mut zero = graph(true);
    zero["edges"][0]["weight"] = json!(0);
    let refused = call(json!({"op": "import", "graph": zero, "limit": 100}));
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("weight must be at least 1"),
        "{refused}"
    );
    let mut negative = graph(true);
    negative["edges"][0]["weight"] = json!(-2);
    assert_eq!(
        call(json!({"op": "import", "graph": negative, "limit": 100}))["ok"],
        false
    );
}

#[test]
fn the_overview_pulls_by_weight() {
    let overview = |g: Value| {
        let response = call(json!({"op": "overview", "graph": g, "options": {"communities": 0}}));
        assert_eq!(response["ok"], true, "{response}");
        response["overview"].clone()
    };
    // The link between the two packages (at the top) counts a + b + c by weight.
    let links = |o: &Value| o["links"].as_array().unwrap().clone();
    let plain = links(&overview(graph(false)));
    let heavy = links(&overview(graph(true)));
    let top = |l: &[Value]| -> f64 {
        l.chunks(3)
            .find(|t| t[0].as_f64() == Some(0.) && t[1].as_f64() == Some(4.))
            .map(|t| t[2].as_f64().unwrap())
            .unwrap()
    };
    assert_eq!(top(&plain), 3.);
    assert_eq!(top(&heavy), 13.);
}
