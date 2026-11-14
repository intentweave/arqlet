//! Experimental `regionPorts` (ADR 0020, S1-2): an edge that leaves a region of another strategy in a layered parent is
//! routed by the parent to a port on the region's border, on the side the flow uses, and drawn inside the region from
//! there to its node.
use arqlet_model::{Point, Rect, Snapshot};
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn call(request: Value) -> Value {
    serde_json::from_str(&dispatch(&request.to_string())).unwrap()
}

/// Stages `a` and `b` flowing left to right around region `r` of `strategy`, its children wired as a chain with a few
/// chords; every stage node has an edge into the region or out of it, and one edge runs back into it.
fn graph(strategy: &str) -> Value {
    let mut nodes = vec![json!({"id": "r", "kind": "group", "label": "r"})];
    let mut edges = Vec::new();
    for k in 0..7 {
        nodes.push(json!({"id": format!("r.{k}"), "kind": "fn", "label": format!("r{k}"), "parentId": "r"}));
        if k > 0 {
            edges.push(json!({"id": format!("i{k}"), "kind": "calls", "source": format!("r.{}", (k - 1) / 2), "target": format!("r.{k}")}));
        }
    }
    for j in 0..3 {
        nodes.push(json!({"id": format!("a{j}"), "kind": "svc", "label": format!("a{j}")}));
        nodes.push(json!({"id": format!("b{j}"), "kind": "svc", "label": format!("b{j}")}));
        edges.push(json!({"id": format!("in{j}"), "kind": "calls", "source": format!("a{j}"), "target": format!("r.{}", 2 * j + 1)}));
        edges.push(json!({"id": format!("out{j}"), "kind": "calls", "source": format!("r.{}", 6 - 2 * j), "target": format!("b{j}")}));
    }
    edges.push(json!({"id": "back", "kind": "calls", "source": "b0", "target": "r.3"}));
    let mut g = json!({"nodes": nodes, "edges": edges, "defaultDirection": "lr",
        "containerStrategies": {"r": strategy}});
    if strategy != "tree" {
        g["containerDirections"] = json!({"r": "auto"});
        g["containerRouting"] = json!({"r": "straight"});
    }
    g
}

fn import(graph: &Value, options: Value) -> Snapshot {
    let r = call(json!({"op": "import", "graph": graph, "limit": 100, "options": options}));
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
fn edges_leave_a_region_through_ports_on_the_sides_of_the_flow() {
    for strategy in ["organic", "tree", "radial"] {
        let g = graph(strategy);
        for base in [json!({}), json!({"singlePassMixed": true})] {
            let mut options = base.clone();
            options["regionPorts"] = json!(true);
            let s = import(&g, options);
            let m = arqlet_layout::published_metrics(&s).unwrap();
            assert_eq!(
                m.line_node_hits, 0,
                "{strategy} {base}: no line through a node"
            );
            let issues = arqlet_scene::validate_geometry(&s.graph, &s.boxes, Default::default());
            assert!(issues.is_empty(), "{strategy}: {issues:?}");
            let abs = arqlet_layout::absolute_all(&s.graph, &s.boxes);
            let region = &abs["r"];
            assert_eq!(s.routes.len(), s.graph.edges.len(), "every edge is routed");
            for e in &s.graph.edges {
                let route = s.routes.iter().find(|r| r.id == e.id).unwrap();
                let target = e.target.as_ref().unwrap();
                // In logical direction: from the source's box to the target's box.
                assert!(
                    on_box(&route.points[0], &abs[&e.source]),
                    "{} leaves {}",
                    e.id,
                    e.source
                );
                assert!(
                    on_box(route.points.last().unwrap(), &abs[target]),
                    "{} reaches {target}",
                    e.id
                );
                // An edge that leaves the region passes the side that faces its other end along the flow: the left side
                // when that end lies left of the region, the right side when it lies right of it.
                let outer = match (e.source.starts_with("r."), target.starts_with("r.")) {
                    (true, false) => target,
                    (false, true) => &e.source,
                    _ => continue,
                };
                let o = &abs[outer];
                let side = |x: f64| {
                    route.points.iter().any(|p| {
                        (p.x - x).abs() < 0.5 && p.y > region.y && p.y < region.y + region.height
                    })
                };
                if o.x + o.width <= region.x {
                    assert!(side(region.x), "{strategy}: {} passes the left side", e.id);
                } else if o.x >= region.x + region.width {
                    assert!(
                        side(region.x + region.width),
                        "{strategy}: {} passes the right side",
                        e.id
                    );
                }
            }
            // The snapshot keeps the given graph, and a toggle continues from it.
            assert_eq!(
                s.graph
                    .edges
                    .iter()
                    .find(|e| e.id == "in0")
                    .unwrap()
                    .target
                    .as_deref(),
                Some("r.1")
            );
            assert!(s.graph.ports.is_empty() && s.graph.edge_ports.is_empty());
            let v = serde_json::to_value(&s).unwrap();
            let toggled = call(json!({"op": "toggle", "snapshot": v, "nodeId": "r",
                "expectedRevision": v["revision"], "expectedLayoutVersion": v["layoutVersion"]}));
            assert_eq!(toggled["ok"], true, "{toggled}");
        }
    }
    // Off: nothing changes in the serialized options.
    let plain = import(&graph("organic"), json!({}));
    assert!(!serde_json::to_string(&plain.options)
        .unwrap()
        .contains("regionPorts"));
}
