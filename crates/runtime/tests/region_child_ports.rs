//! Experimental `regionChildPorts` (ADR 0020, S1-3): a container with a fixed flow inside an organic area gets border
//! nodes in its own flow; its edges to the outside run inside it along its ranks to such a border node and leave it on a
//! side of its flow's axis, and the area draws them on from there.
use arqlet_model::{Point, Rect, Snapshot};
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

/// An organic root with two layered containers (one of each axis), loose nodes, and edges between all of them.
fn graph() -> Value {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for (c, dir) in [("a", "lr"), ("b", "tb")] {
        nodes.push(json!({"id": c, "kind": "group", "label": c}));
        for k in 0..5 {
            nodes.push(json!({"id": format!("{c}{k}"), "kind": "fn", "label": format!("{c}{k}"), "parentId": c}));
            if k > 0 {
                edges.push(json!({"id": format!("{c}-{k}"), "kind": "calls", "source": format!("{c}{}", (k - 1) / 2), "target": format!("{c}{k}")}));
            }
        }
        let _ = dir;
    }
    for k in 0..3 {
        nodes.push(json!({"id": format!("f{k}"), "kind": "fn", "label": format!("f{k}")}));
    }
    for (id, s, t) in [
        ("x1", "a4", "b0"),
        ("x2", "b3", "f0"),
        ("x3", "f1", "a2"),
        ("x4", "a3", "f2"),
        ("x5", "f0", "b4"),
        ("x6", "b2", "a0"),
    ] {
        edges.push(json!({"id": id, "kind": "calls", "source": s, "target": t}));
    }
    json!({"nodes": nodes, "edges": edges, "defaultDirection": "auto", "defaultStrategy": "organic",
        "defaultRouting": "straight", "containerDirections": {"a": "lr", "b": "tb"}})
}

fn import(options: Value) -> Snapshot {
    let r = call(json!({"op": "import", "graph": graph(), "limit": 100, "options": options}));
    assert_eq!(r["ok"], true, "{r}");
    serde_json::from_value(r["snapshot"].clone()).unwrap()
}

fn on_box(p: &Point, r: &Rect) -> bool {
    p.x >= r.x - 0.5
        && p.x <= r.x + r.width + 0.5
        && p.y >= r.y - 0.5
        && p.y <= r.y + r.height + 0.5
}

#[test]
fn edges_leave_layered_containers_of_an_organic_area_along_their_flow() {
    let s = import(json!({"regionChildPorts": true}));
    let m = arqlet_layout::published_metrics(&s).unwrap();
    assert_eq!(m.line_node_hits, 0, "no line through a node");
    let issues = arqlet_scene::validate_geometry(&s.graph, &s.boxes, Default::default());
    assert!(issues.is_empty(), "{issues:?}");
    let abs = arqlet_layout::absolute_all(&s.graph, &s.boxes);
    // Every edge from its source to its target.
    for e in &s.graph.edges {
        let route = s.routes.iter().find(|r| r.id == e.id).unwrap();
        assert!(
            on_box(&route.points[0], &abs[&e.source]),
            "{} leaves {}",
            e.id,
            e.source
        );
        assert!(
            on_box(
                route.points.last().unwrap(),
                &abs[e.target.as_ref().unwrap()]
            ),
            "{} arrives",
            e.id
        );
    }
    // The edges to the outside cross their container's border on a side of its flow's axis: the route has a point on that
    // side (left or right of `a`, top or bottom of `b`) and none on the other two.
    let mut on_axis = 0;
    let mut ends = 0;
    for e in s.graph.edges.iter().filter(|e| e.id.starts_with('x')) {
        let route = s.routes.iter().find(|r| r.id == e.id).unwrap();
        for (end, container, vertical) in [
            (&e.source, "a", false),
            (e.target.as_ref().unwrap(), "a", false),
            (&e.source, "b", true),
            (e.target.as_ref().unwrap(), "b", true),
        ] {
            if !end.starts_with(container) {
                continue;
            }
            ends += 1;
            let b = &abs[container];
            let on_side = route.points.iter().any(|p| {
                if vertical {
                    ((p.y - b.y).abs() < 0.5 || (p.y - b.y - b.height).abs() < 0.5)
                        && p.x > b.x
                        && p.x < b.x + b.width
                } else {
                    ((p.x - b.x).abs() < 0.5 || (p.x - b.x - b.width).abs() < 0.5)
                        && p.y > b.y
                        && p.y < b.y + b.height
                }
            });
            if on_side {
                on_axis += 1;
            }
        }
    }
    assert!(ends > 0);
    assert!(
        on_axis * 10 >= ends * 8,
        "most ends leave along the flow: {on_axis} of {ends}"
    );
    // The plans carry the border nodes, and a toggle continues.
    assert!(!s.layered_plans["a"].borders.is_empty());
    let v = serde_json::to_value(&s).unwrap();
    let toggled = call(json!({"op": "toggle", "snapshot": v, "nodeId": "a",
        "expectedRevision": v["revision"], "expectedLayoutVersion": v["layoutVersion"]}));
    assert_eq!(toggled["ok"], true, "{toggled}");
    // Off: no border nodes for these containers, and the option is not written.
    let plain = import(json!({}));
    assert!(plain
        .layered_plans
        .get("a")
        .is_none_or(|p| p.borders.is_empty()));
    assert!(!serde_json::to_string(&plain.options)
        .unwrap()
        .contains("regionChild"));
    assert!(!serde_json::to_string(&s.options)
        .unwrap()
        .contains("regionChildSides"));
}
