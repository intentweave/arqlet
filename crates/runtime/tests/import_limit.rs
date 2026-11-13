//! The first view's `limit` when the view abstracts (provisional): it counts what the view shows, and a container keeps
//! its content so that it can be opened.
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

fn ok(response: Value) -> Value {
    assert_eq!(response["ok"], true, "{response}");
    response["snapshot"].clone()
}

/// `boxes` containers of `size` files each; the given edges.
fn packages(boxes: usize, size: usize, edges: Vec<Value>) -> Value {
    let mut nodes = Vec::new();
    for c in 0..boxes {
        nodes.push(json!({"id": format!("c{c}"), "kind": "package", "label": format!("c{c}")}));
        for k in 0..size {
            nodes.push(json!({"id": format!("c{c}/n{k}"), "kind": "file", "label": format!("n{k}"), "parentId": format!("c{c}")}));
        }
    }
    json!({"nodes": nodes, "edges": edges})
}

fn ids(snapshot: &Value) -> Vec<String> {
    snapshot["graph"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["id"].as_str().unwrap().to_string())
        .collect()
}

fn toggle(snapshot: &Value, id: &str) -> Value {
    call(json!({"op": "toggle", "snapshot": snapshot, "nodeId": id,
        "expectedRevision": snapshot["revision"], "expectedLayoutVersion": snapshot["layoutVersion"]}))
}

#[test]
fn the_limit_counts_collapsed_containers_once_and_keeps_their_content() {
    let lod = json!({"levelOfDetail": {"depth": 0}});
    // 20 boxes of 50: all 20 fit a limit of 100, each whole in the source graph.
    let s = ok(call(
        json!({"op": "import", "graph": packages(20, 50, vec![]), "limit": 100, "options": lod}),
    ));
    assert_eq!(ids(&s).len(), 20);
    assert_eq!(s["collapsedIds"].as_array().unwrap().len(), 20);
    assert_eq!(s["sourceGraph"]["nodes"].as_array().unwrap().len(), 1020);
    let opened = ok(toggle(&s, "c13"));
    assert_eq!(ids(&opened).len(), 20 + 50);
    // 200 boxes and a limit of 100: 100 boxes, in input order, each whole.
    let s = ok(call(
        json!({"op": "import", "graph": packages(200, 3, vec![]), "limit": 100, "options": lod}),
    ));
    assert_eq!(ids(&s).len(), 100);
    assert!(ids(&s)
        .iter()
        .all(|id| id[1..].parse::<usize>().unwrap() < 100));
    assert_eq!(s["sourceGraph"]["nodes"].as_array().unwrap().len(), 400);
}

#[test]
fn a_container_an_edge_reaches_can_be_opened() {
    let lod = json!({"levelOfDetail": {"depth": 0}});
    // An aggregated edge between two collapsed containers, with a weight.
    let edges =
        vec![json!({"id": "agg", "kind": "calls", "source": "c0", "target": "c7", "weight": 12})];
    let s = ok(call(
        json!({"op": "import", "graph": packages(20, 50, edges), "limit": 100, "options": lod}),
    ));
    let opened = ok(toggle(&s, "c7"));
    assert!(ids(&opened).contains(&"c7/n49".to_string()));
    // Collapsing only below the top: the top containers are open, and one an edge reaches without any of its content
    // selected comes whole and collapsed, instead of as a plain node.
    let mut nodes = Vec::new();
    for t in 0..6 {
        nodes.push(json!({"id": format!("t{t}"), "kind": "area", "label": format!("t{t}")}));
        for m in 0..4 {
            let mid = format!("t{t}/m{m}");
            nodes.push(
                json!({"id": mid, "kind": "package", "label": mid, "parentId": format!("t{t}")}),
            );
            for k in 0..10 {
                nodes.push(json!({"id": format!("{mid}/n{k}"), "kind": "file", "label": "n", "parentId": mid}));
            }
        }
    }
    let mut edges =
        vec![json!({"id": "up", "kind": "calls", "source": "t0/m0/n0", "target": "t5"})];
    for m in 1..4 {
        edges.push(json!({"id": format!("in{m}"), "kind": "calls", "source": "t0/m0/n1", "target": format!("t0/m{m}/n0")}));
    }
    let s = ok(call(
        json!({"op": "import", "graph": {"nodes": nodes, "edges": edges}, "limit": 5,
        "options": {"levelOfDetail": {"depth": 1}}}),
    ));
    let collapsed: Vec<&str> = s["collapsedIds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(collapsed.contains(&"t5"), "{collapsed:?}");
    let opened = ok(toggle(&s, "t5"));
    assert!(ids(&opened).iter().any(|id| id.starts_with("t5/m")));
}

#[test]
fn without_abstraction_or_within_the_limit_the_selection_is_as_before() {
    // No level of detail: the limit counts graph nodes, as before.
    let s = ok(call(
        json!({"op": "import", "graph": packages(20, 50, vec![]), "limit": 100}),
    ));
    let plain = ids(&s);
    assert!(plain.len() <= 102, "{}", plain.len());
    // Within the limit: the same view either way.
    let lod = json!({"levelOfDetail": {"depth": 0}});
    let all = ok(call(
        json!({"op": "import", "graph": packages(5, 4, vec![]), "limit": 25, "options": lod}),
    ));
    assert_eq!(ids(&all).len(), 5);
    assert_eq!(all["sourceGraph"]["nodes"].as_array().unwrap().len(), 25);
}

#[test]
fn communities_count_as_boxes_too() {
    // A flat graph of 300 nodes in rings: communities collapse at first; a limit of 30 shows boxes, not 30 nodes.
    let nodes: Vec<Value> = (0..300)
        .map(|i| json!({"id": format!("n{i:03}"), "kind": "fn", "label": format!("n{i}")}))
        .collect();
    let edges: Vec<Value> = (0..300)
        .map(|i| {
            let ring = i / 15;
            json!({"id": format!("e{i}"), "kind": "calls", "source": format!("n{i:03}"),
                "target": format!("n{:03}", ring * 15 + (i + 1) % 15)})
        })
        .collect();
    let s = ok(call(
        json!({"op": "import", "graph": {"nodes": nodes, "edges": edges}, "limit": 30,
        "options": {"communities": 40}}),
    ));
    let collapsed = s["collapsedIds"].as_array().unwrap().len();
    assert!(collapsed > 1, "communities collapsed: {s}");
    // Every node of a shown community is in the source graph, so opening one shows all of it.
    let first = s["collapsedIds"][0].as_str().unwrap().to_string();
    assert_eq!(toggle(&s, &first)["ok"], true);
}
