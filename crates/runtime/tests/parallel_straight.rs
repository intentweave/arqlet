//! Parallel and antiparallel edges under straight routing: each edge is one segment with ports of its own, so the segments lie
//! side by side, and an organic strategy is allowed whatever directions the areas carry.

use arqlet_model::{Graph, Mode, Point, Snapshot};
use arqlet_runtime::{configure_snapshot, import_graph};
use serde_json::json;

fn pair(a_shape: &str, b_shape: &str, extra: serde_json::Value) -> Graph {
    let mut graph = json!({
        "nodes": [
            {"id": "a", "kind": "node", "label": "Request payment", "parentId": null, "shape": a_shape},
            {"id": "b", "kind": "node", "label": "Payment ok?", "parentId": null, "shape": b_shape},
            {"id": "c", "kind": "node", "label": "Timeout", "parentId": null},
        ],
        "edges": [
            {"id": "e1", "kind": "flow", "source": "a", "target": "b"},
            {"id": "e2", "kind": "flow", "source": "a", "target": "b"},
            {"id": "e3", "kind": "flow", "source": "b", "target": "a"},
            {"id": "e4", "kind": "flow", "source": "a", "target": "c"},
        ],
        "defaultDirection": "auto", "defaultStrategy": "organic", "defaultRouting": "straight",
    });
    for (key, value) in extra.as_object().expect("an object") {
        graph[key] = value.clone();
    }
    serde_json::from_value(graph).expect("the graph is valid")
}

fn segment(snapshot: &Snapshot, id: &str) -> (Point, Point) {
    let route = snapshot
        .routes
        .iter()
        .find(|r| r.id == id)
        .unwrap_or_else(|| panic!("{id} has no route"));
    assert_eq!(route.points.len(), 2, "{id} is one segment");
    (route.points[0].clone(), route.points[1].clone())
}

fn cross(o: &Point, a: &Point, b: &Point) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// Whether two segments cross each other properly (not only at a shared end).
fn crosses(p: &(Point, Point), q: &(Point, Point)) -> bool {
    let (d1, d2) = (cross(&q.0, &q.1, &p.0), cross(&q.0, &q.1, &p.1));
    let (d3, d4) = (cross(&p.0, &p.1, &q.0), cross(&p.0, &p.1, &q.1));
    d1 * d2 < 0. && d3 * d4 < 0.
}

fn distance(a: &Point, b: &Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// The three edges between `a` and `b` have ports of their own, lie apart and do not cross, and nothing is reported.
fn assert_side_by_side(snapshot: &Snapshot, label: &str) {
    assert!(
        snapshot.geometry_issues.is_empty(),
        "{label}: {:?}",
        snapshot.geometry_issues
    );
    let edges = ["e1", "e2", "e3"];
    let drawn: Vec<_> = edges.iter().map(|e| segment(snapshot, e)).collect();
    for i in 0..3 {
        for j in i + 1..3 {
            for (p, q) in [
                (&drawn[i].0, &drawn[j].0),
                (&drawn[i].0, &drawn[j].1),
                (&drawn[i].1, &drawn[j].0),
                (&drawn[i].1, &drawn[j].1),
            ] {
                assert!(
                    distance(p, q) > 1e-6,
                    "{label}: {} and {} share a port",
                    edges[i],
                    edges[j]
                );
            }
            assert!(
                !crosses(&drawn[i], &drawn[j]),
                "{label}: {} and {} cross",
                edges[i],
                edges[j]
            );
            let mid = |s: &(Point, Point)| Point {
                x: (s.0.x + s.1.x) / 2.,
                y: (s.0.y + s.1.y) / 2.,
            };
            assert!(
                distance(&mid(&drawn[i]), &mid(&drawn[j])) >= 4.,
                "{label}: {} and {} lie on one another",
                edges[i],
                edges[j]
            );
        }
    }
}

#[test]
fn parallel_and_antiparallel_edges_are_drawn_side_by_side() {
    for (a, b) in [
        ("rect", "rect"),
        ("circle", "circle"),
        ("rect", "circle"),
        ("circle", "rect"),
    ] {
        let start = import_graph(pair(a, b, json!({})), 100).expect("parallel edges are drawn");
        assert_side_by_side(&start, &format!("{a}/{b}"));
        // The partner on every side: right, left, below, above, and across.
        for (dx, dy) in [
            (420., 0.),
            (-420., 0.),
            (0., 320.),
            (0., -320.),
            (300., 200.),
            (-300., -200.),
        ] {
            let pinned = configure_snapshot(
                &start,
                start.revision,
                start.layout_version,
                &json!({"pins": {"a": {"x": 600., "y": 600.}, "b": {"x": 600. + dx, "y": 600. + dy}}}),
            )
            .expect("the pins are accepted");
            assert_side_by_side(&pinned, &format!("{a}/{b} with b at ({dx}, {dy})"));
        }
    }
}

#[test]
fn a_collapsed_container_keeps_its_edges_as_parallel_segments() {
    // Both edges leave the same container: collapsed, they run to one node and are parallel.
    let graph: Graph = serde_json::from_value(json!({
        "nodes": [
            {"id": "g", "kind": "node", "label": "gateway", "parentId": null},
            {"id": "box", "kind": "group", "label": "box", "parentId": null, "shape": "circle"},
            {"id": "x", "kind": "node", "label": "x", "parentId": "box"},
            {"id": "y", "kind": "node", "label": "y", "parentId": "box"},
        ],
        "edges": [
            {"id": "e1", "kind": "flow", "source": "g", "target": "x"},
            {"id": "e2", "kind": "flow", "source": "g", "target": "y"},
        ],
        "defaultDirection": "auto", "defaultStrategy": "organic", "defaultRouting": "straight",
    }))
    .expect("valid");
    let start = import_graph(graph, 100).expect("imports");
    let collapsed =
        arqlet_runtime::toggle_container(&start, start.revision, start.layout_version, "box")
            .expect("the container collapses under straight routing");
    assert!(collapsed.collapsed_ids.contains(&"box".to_string()));
    let (one, two) = (segment(&collapsed, "e1"), segment(&collapsed, "e2"));
    assert!(
        distance(&one.1, &two.1) > 1.,
        "the two edges end at different ports"
    );
    assert!(!crosses(&one, &two));
}

#[test]
fn an_organic_strategy_is_allowed_with_fixed_directions() {
    // The root is organic and carries a fixed direction: the strategy wins, the direction is not used.
    let graph = pair("rect", "rect", json!({"defaultDirection": "tb"}));
    assert_eq!(graph.area_mode(None), Ok(Mode::Organic));
    import_graph(graph, 100).expect("organic with a fixed direction is laid out");
    // An organic strategy that a container only inherits yields to the direction the container declares: the pools stay
    // layered inside an organic view.
    let pools: Graph = serde_json::from_value(json!({
        "nodes": [
            {"id": "p", "kind": "pool", "label": "Sales", "parentId": null},
            {"id": "q", "kind": "pool", "label": "Customer", "parentId": null},
            {"id": "a", "kind": "task", "label": "Join", "parentId": "p"},
            {"id": "b", "kind": "task", "label": "Split", "parentId": "p"},
            {"id": "c", "kind": "task", "label": "Order", "parentId": "q"},
            {"id": "d", "kind": "task", "label": "Pay", "parentId": "q"},
        ],
        "edges": [
            {"id": "e1", "kind": "flow", "source": "a", "target": "b"},
            {"id": "e2", "kind": "flow", "source": "b", "target": "a"},
            {"id": "e3", "kind": "flow", "source": "c", "target": "d"},
            {"id": "e4", "kind": "flow", "source": "b", "target": "c"},
        ],
        "defaultDirection": "auto", "defaultStrategy": "organic", "defaultRouting": "straight",
        "containerDirections": {"p": "lr", "q": "tb"},
    }))
    .expect("valid");
    assert_eq!(pools.area_mode(None), Ok(Mode::Organic));
    assert_ne!(pools.area_mode(Some("p")), Ok(Mode::Organic));
    assert_ne!(pools.area_mode(Some("p")), pools.area_mode(Some("q")));
    let snapshot = import_graph(pools, 100).expect("the pools are laid out");
    assert!(
        snapshot.geometry_issues.is_empty(),
        "{:?}",
        snapshot.geometry_issues
    );
    // Join and Split run left to right inside Sales; Order and Pay run top to bottom inside Customer.
    assert!(
        snapshot.boxes["a"].x != snapshot.boxes["b"].x
            || snapshot.boxes["a"].y != snapshot.boxes["b"].y
    );
    assert!(snapshot.boxes["c"].y < snapshot.boxes["d"].y);
    // An organic strategy set on the container itself wins over its own direction.
    let explicit: Graph = serde_json::from_value(json!({
        "nodes": [{"id": "p", "kind": "pool", "label": "Sales", "parentId": null},
                  {"id": "a", "kind": "task", "label": "Join", "parentId": "p"}],
        "edges": [],
        "containerDirections": {"p": "lr"}, "containerStrategies": {"p": "organic"},
    }))
    .expect("valid");
    assert_eq!(explicit.area_mode(Some("p")), Ok(Mode::Organic));
}
