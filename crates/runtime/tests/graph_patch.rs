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
        json!({"op": "import", "graph": packages(), "limit": 100, "options": {"levelOfDetail": {"depth": 0}, "liftEdges": false}}),
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
        json!({"op": "import", "graph": graph, "limit": 100, "options": {"levelOfDetail": {"depth": 0}, "liftEdges": false}}),
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

/// A bus view with buses as data, as `benchmarks/experiments/bus-mesh.mjs UNITS BUSES STRATEGY STYLE CHILDREN EDGES` builds
/// it (seed 7): `units` units on 1 to 3 of `buses` buses each, and with `drill` the children under up to 11 members of the
/// first bus and their edges to the other members. Returns the graph and the options of the import.
fn bus_mesh(
    units: usize,
    buses: usize,
    organic: bool,
    style: &str,
    drill: Option<(usize, usize)>,
) -> (Value, Value) {
    let mut seed: u64 = 7;
    let mut rnd = || {
        seed = (seed * 1_103_515_245 + 12_345) % 2_147_483_648;
        seed as f64 / 2_147_483_648.
    };
    let mut nodes: Vec<Value> = (0..units)
        .map(|i| json!({"id": format!("u{i}"), "kind": "unit", "label": format!("U{i}")}))
        .collect();
    let mut taps: Vec<Vec<String>> = vec![Vec::new(); buses];
    for i in 0..units {
        let k = 1 + (rnd() * 3.) as usize;
        for _ in 0..k {
            let b = (rnd() * buses as f64) as usize;
            let node = format!("u{i}");
            if !taps[b].contains(&node) {
                taps[b].push(node);
            }
        }
    }
    let bus_list: Vec<Value> = taps
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.is_empty())
        .map(|(b, t)| {
            let taps: Vec<Value> = t
                .iter()
                .map(|n| json!({"edge": format!("tbus{b}-{}", &n[1..]), "node": n, "toBus": true, "kind": "on"}))
                .collect();
            json!({"id": format!("bus{b}"), "kind": "bus", "label": format!("Bus {b}"), "taps": taps})
        })
        .collect();
    let mut edges = Vec::new();
    if let Some((children, count)) = drill {
        let first = taps.iter().find(|t| !t.is_empty()).unwrap();
        let members: Vec<&String> = first.iter().take(11).collect();
        for k in 0..children {
            nodes.push(
                json!({"id": format!("f{k}"), "kind": "frame", "label": format!("F{k}"),
                "parentId": members[k % members.len()]}),
            );
        }
        for k in 0..count {
            let f = k % children;
            let to = members[(f + 1 + (k % (members.len() - 1))) % members.len()];
            edges.push(json!({"id": format!("rx{k}"), "kind": "rx", "source": format!("f{f}"), "target": to}));
        }
    }
    let mut graph = json!({"nodes": nodes, "edges": edges, "buses": bus_list});
    if organic {
        graph["defaultStrategy"] = json!("organic");
        graph["defaultDirection"] = json!("auto");
    }
    (graph, json!({"busStyle": style}))
}

fn geometry(s: &Value) -> Value {
    json!([s["boxes"], s["routes"], s["busLines"]])
}

/// #228 (ADR 0029 C): a patch into a view with buses is laid out as an import of the patched graph. Before, a layered view
/// with bus lines and an organic view with bus bars started from the earlier drawing and drew far worse than an import.
#[test]
fn a_patch_into_a_bus_view_draws_what_an_import_draws() {
    for (organic, style) in [(false, "lines"), (true, "bar"), (true, "lines")] {
        let (overview, options) = bus_mesh(20, 5, organic, style, None);
        let (full, _) = bus_mesh(20, 5, organic, style, Some((20, 60)));
        let before = ok(call(
            json!({"op": "import", "graph": overview, "limit": 10000, "options": options}),
        ));
        let added: Vec<Value> = full["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|n| n.get("parentId").is_some())
            .cloned()
            .collect();
        let patched = ok(edit(
            &before,
            json!({"op": "patch", "patch": {"nodes": added, "edges": full["edges"]}}),
        ));
        let fresh = ok(call(
            json!({"op": "import", "graph": full, "limit": 10000, "options": options}),
        ));
        assert!(
            geometry(&patched) == geometry(&fresh),
            "organic {organic}, {style}: the patch draws another drawing than the import"
        );
        // Still a graph change on the drawing it continues: revision and layout version advance.
        assert_eq!(
            patched["revision"],
            before["revision"].as_u64().unwrap() + 1
        );
        assert_eq!(patched["baseRevision"], before["revision"]);
        assert_eq!(
            patched["layoutVersion"],
            before["layoutVersion"].as_u64().unwrap() + 1
        );
    }
}

/// A view without buses and without abstraction still continues its earlier drawing (#228 changes only views with buses):
/// an edge added between nodes that are there leaves every node where it was, where an import draws another arrangement.
#[test]
fn a_patch_into_a_plain_view_still_starts_from_the_earlier_drawing() {
    let nodes: Vec<Value> = (0..8)
        .map(|i| json!({"id": format!("n{i}"), "kind": "n", "label": format!("n{i}")}))
        .collect();
    let chain: Vec<Value> = (0..7)
        .map(|i| json!({"id": format!("e{i}"), "kind": "e", "source": format!("n{i}"), "target": format!("n{}", i + 1)}))
        .collect();
    let graph = json!({"nodes": nodes, "edges": chain, "defaultStrategy": "organic"});
    let before = ok(call(json!({"op": "import", "graph": graph, "limit": 100})));
    let added = json!({"id": "back", "kind": "e", "source": "n7", "target": "n0"});
    let patched = ok(edit(
        &before,
        json!({"op": "patch", "patch": {"edges": [added.clone()]}}),
    ));
    // Held to rounding (the warm start passes the places through the frame of the area).
    for (id, b) in boxes(&before) {
        let a = &boxes(&patched)[&id];
        for k in ["x", "y", "width", "height"] {
            let (p, q) = (a[k].as_f64().unwrap(), b[k].as_f64().unwrap());
            assert!((p - q).abs() < 1e-6, "{id}.{k}: {q} -> {p}");
        }
    }
    let mut closed = graph.clone();
    closed["edges"].as_array_mut().unwrap().push(added);
    let fresh = ok(call(json!({"op": "import", "graph": closed, "limit": 100})));
    assert_ne!(
        boxes(&fresh),
        boxes(&before),
        "the import of the ring should be arranged anew"
    );
}
