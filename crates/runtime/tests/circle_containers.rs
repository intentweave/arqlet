//! Circle containers: a container with the circle outline is a square box around a disc. Its children lie inside the disc
//! (less the padding) and below the header, whatever the layout inside, and the engine reports what does not fit.

use arqlet_layout::absolute_all;
use arqlet_model::outline::Outline;
use arqlet_model::{Graph, MovePhase, MoveStep, NodeShape, NodeSize, Point, Snapshot};
use arqlet_runtime::{configure_snapshot, import_graph, measure_snapshot, move_node, set_shape};
use serde_json::json;
use std::collections::BTreeMap;

/// A circle container `c0` with five leafs (every other one a circle), a rectangle container `c1` with three, and two loose
/// leafs; edges inside, across and outside. `inside` holds the graph-level fields that set the layout inside `c0`.
fn diagram(inside: serde_json::Value) -> Graph {
    let mut nodes = vec![
        json!({"id": "c0", "kind": "group", "label": "Circle", "parentId": null, "shape": "circle"}),
        json!({"id": "c1", "kind": "group", "label": "Box", "parentId": null}),
    ];
    for i in 0..10 {
        let parent = match i {
            0..=4 => json!("c0"),
            5..=7 => json!("c1"),
            _ => serde_json::Value::Null,
        };
        nodes.push(json!({
            "id": format!("n{i}"), "kind": "node", "label": format!("N{i}"), "parentId": parent,
            "shape": if i % 2 == 0 { "circle" } else { "rect" },
        }));
    }
    let pairs = [
        (0, 1),
        (1, 2),
        (2, 3),
        (3, 4),
        (0, 4),
        (4, 5),
        (2, 6),
        (5, 6),
        (6, 7),
        (7, 8),
        (8, 9),
        (1, 9),
    ];
    let edges: Vec<_> = pairs
        .iter()
        .enumerate()
        .map(|(k, (a, b))| json!({"id": format!("e{k}"), "kind": "flow", "source": format!("n{a}"), "target": format!("n{b}")}))
        .collect();
    let mut graph = json!({
        "nodes": nodes, "edges": edges,
        "defaultDirection": "auto", "defaultStrategy": "organic", "defaultRouting": "straight",
    });
    for (key, value) in inside.as_object().expect("an object") {
        graph[key] = value.clone();
    }
    serde_json::from_value(graph).expect("the diagram is valid")
}

/// How far the children of `container` reach beyond its disc (less the padding), or above the header.
fn overshoot(snapshot: &Snapshot, container: &str) -> f64 {
    let rules = arqlet_scene::GeometryRules::default();
    let c = &snapshot.boxes[container];
    let radius = c.width.min(c.height) / 2. - rules.padding;
    snapshot
        .graph
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == Some(container))
        .map(|n| {
            let r = &snapshot.boxes[&n.id];
            (rules.header - r.y).max(0.)
                + Outline::of(n.shape, r).overshoot_beyond_disc(c.width / 2., c.height / 2., radius)
        })
        .fold(0., f64::max)
}

fn assert_holds(snapshot: &Snapshot, context: &str) {
    let c = &snapshot.boxes["c0"];
    assert_eq!(c.width, c.height, "{context}: a circle container is square");
    let reached = overshoot(snapshot, "c0");
    let reported = snapshot
        .geometry_issues
        .iter()
        .any(|i| i.kind == "container_bounds" || i.kind == "outside-area");
    assert!(
        reached < 1e-6 || reported,
        "{context}: children reach {reached} beyond the disc and nothing is reported"
    );
    let _ = absolute_all(&snapshot.graph, &snapshot.boxes);
}

fn drag(snapshot: &Snapshot, node: &str, dx: f64, dy: f64) -> Snapshot {
    let r = snapshot.boxes[node].clone();
    let at = |x: f64, y: f64| Some(Point { x, y });
    let mut current = snapshot.clone();
    for (seq, phase, position) in [
        (1, MovePhase::Begin, at(r.x, r.y)),
        (2, MovePhase::Update, at(r.x + dx, r.y + dy)),
        (3, MovePhase::Finish, None),
    ] {
        let step = MoveStep {
            gesture_id: format!("g-{node}"),
            seq,
            phase,
            node: node.into(),
            position,
            release: false,
            others: Default::default(),
        };
        current = move_node(&current, current.revision, current.layout_version, &step)
            .expect("the step is accepted");
    }
    current
}

#[test]
fn the_children_of_a_circle_container_lie_in_its_disc() {
    let snapshot = import_graph(diagram(json!({})), 100).expect("imports");
    assert_holds(&snapshot, "organic inside");
    assert!(
        snapshot
            .geometry_issues
            .iter()
            .all(|i| i.kind != "container_bounds"),
        "{:?}",
        snapshot.geometry_issues
    );
}

#[test]
fn a_layered_arrangement_inside_a_circle_container_is_centred_in_the_disc() {
    for direction in ["lr", "tb"] {
        let snapshot = import_graph(
            diagram(json!({
                "containerStrategies": {"c0": "none"},
                "containerDirections": {"c0": direction},
            })),
            100,
        )
        .expect("imports");
        assert_holds(&snapshot, &format!("layered {direction} inside"));
        assert!(
            overshoot(&snapshot, "c0") < 1e-6,
            "{direction}: the arrangement does not fit the disc"
        );
    }
}

#[test]
fn moving_and_enlarging_a_child_keeps_it_in_the_disc_or_reports_it() {
    let start = import_graph(diagram(json!({})), 100).expect("imports");
    let moved = drag(&start, "n1", 40., 25.);
    assert_holds(&moved, "after a move");
    let sizes = BTreeMap::from([(
        "n2".to_string(),
        NodeSize {
            width: 140.,
            height: 140.,
        },
    )]);
    let grown = measure_snapshot(&moved, moved.revision, moved.layout_version, sizes)
        .expect("the size is accepted");
    assert_holds(&grown, "after a resize");
    assert!(
        grown.boxes["c0"].width >= moved.boxes["c0"].width - 1e-9,
        "the container does not shrink below its content"
    );
}

#[test]
fn the_shape_of_a_container_changes_between_circle_and_rectangle() {
    let start = import_graph(diagram(json!({})), 100).expect("imports");
    let rect = set_shape(
        &start,
        start.revision,
        start.layout_version,
        "c0",
        NodeShape::Rect,
    )
    .expect("the shape is accepted");
    assert_ne!(
        rect.boxes["c0"].width, rect.boxes["c0"].height,
        "a rectangle follows its content"
    );
    let circle = set_shape(
        &rect,
        rect.revision,
        rect.layout_version,
        "c0",
        NodeShape::Circle,
    )
    .expect("the shape is accepted");
    assert_holds(&circle, "back to a circle");
}

#[test]
fn a_fixed_size_that_cannot_hold_the_children_in_the_disc_is_reported() {
    let imported = import_graph(diagram(json!({})), 100).expect("imports");
    let snapshot = configure_snapshot(
        &imported,
        imported.revision,
        imported.layout_version,
        &json!({"containerSizing": {"c0": {"width": {"fixed": 120.0}, "height": {"fixed": 120.0}}}}),
    )
    .expect("the sizing is accepted");
    assert_eq!(snapshot.boxes["c0"].width, 120.);
    assert!(
        snapshot
            .geometry_issues
            .iter()
            .any(|i| i.kind == "container_bounds" || i.kind == "outside-area"),
        "{:?}",
        snapshot.geometry_issues
    );
}

const MIXED: &str = include_str!("../../../examples/fixtures/mixed-layouts.json");

/// The demo diagram: an organic root and circle container, a layered left-to-right container and a top-to-bottom one.
#[test]
fn the_mixed_layout_fixture_is_conflict_free_and_each_area_follows_its_own_rule() {
    let graph: Graph = serde_json::from_str(MIXED).expect("the fixture is a graph");
    // The areas differ: the circle container and the root are organic, the other two are layered in two directions.
    assert_eq!(graph.area_mode(None), graph.area_mode(Some("mesh")));
    assert_ne!(graph.area_mode(None), graph.area_mode(Some("pipeline")));
    assert_ne!(
        graph.area_mode(Some("pipeline")),
        graph.area_mode(Some("ops"))
    );
    let snapshot = import_graph(graph, 100).expect("imports");
    assert!(
        snapshot.geometry_issues.is_empty(),
        "{:?}",
        snapshot.geometry_issues
    );
    assert!(
        snapshot.routing_issues.is_empty(),
        "{:?}",
        snapshot.routing_issues
    );
    let mesh = &snapshot.boxes["mesh"];
    assert_eq!(mesh.width, mesh.height, "the circle container is square");
    assert!(overshoot(&snapshot, "mesh") < 1e-6);
    // Layered left to right: the stages of the pipeline run along x in the order of their links.
    let x = |id: &str| snapshot.boxes[id].x;
    assert!(x("checkout") < x("compile") && x("compile") < x("test") && x("test") < x("package"));
    // Layered top to bottom: the tools of operations run along y.
    let y = |id: &str| snapshot.boxes[id].y;
    assert!(y("alerts") < y("pager") && y("pager") < y("runbook"));
    // A move in the circle container continues from this arrangement and keeps its children in the disc.
    let moved = drag(&snapshot, "orders", 30., 20.);
    assert!(overshoot(&moved, "mesh") < 1e-6 || !moved.geometry_issues.is_empty());
}

/// The snapshot with its pins replaced, as the pin control of the playground does.
fn pins(snapshot: &Snapshot, pins: serde_json::Value) -> Snapshot {
    configure_snapshot(
        snapshot,
        snapshot.revision,
        snapshot.layout_version,
        &json!({ "pins": pins }),
    )
    .expect("the pins are accepted")
}

/// Pinning a child of a circle container: the pin control without a drag changes nothing on the drawing, and a drag of the
/// pin moves that node by the drag only, while the others stay.
#[test]
fn pinning_in_a_circle_container_moves_nothing_but_the_dragged_node() {
    let graph: Graph = serde_json::from_str(MIXED).expect("the fixture is a graph");
    let start = import_graph(graph, 100).expect("imports");
    let at = |s: &Snapshot, n: &str| {
        let r = &absolute_all(&s.graph, &s.boxes)[n];
        (r.x, r.y)
    };
    let children = ["auth", "orders", "billing", "search", "cache"];
    // A press on the pin: the pin is set at the box the node has now.
    for node in children {
        let r = start.boxes[node].clone();
        let pinned = pins(&start, json!({ node: {"x": r.x, "y": r.y} }));
        for other in children {
            let (a, b) = (at(&start, other), at(&pinned, other));
            assert!(
                (a.0 - b.0).hypot(a.1 - b.1) < 2.,
                "pinning {node} moved {other} by {}",
                (a.0 - b.0).hypot(a.1 - b.1)
            );
        }
        // Unpinning it again leaves the others where they are; the node itself may move by the clearance that the pin had
        // excused (the repair keeps nodes 12 units apart).
        let released = pins(&pinned, json!({}));
        for other in children {
            let (a, b) = (at(&start, other), at(&released, other));
            let limit = if other == node { 12. } else { 2. };
            assert!(
                (a.0 - b.0).hypot(a.1 - b.1) < limit,
                "unpinning {node} moved {other} from {a:?} to {b:?}"
            );
        }
    }
    // A drag of the pin: the node ends at the drop, the others stay.
    for (node, dx, dy) in [
        ("cache", 30., 20.),
        ("orders", -12., 18.),
        ("billing", 30., 20.),
    ] {
        let dragged = drag(&start, node, dx, dy);
        let (a, b) = (at(&start, node), at(&dragged, node));
        assert!(
            (b.0 - a.0 - dx).abs() < 2. && (b.1 - a.1 - dy).abs() < 2.,
            "{node} went from {a:?} to {b:?} for a drag of ({dx}, {dy})"
        );
        assert!(dragged.options.pins.contains_key(node));
        // The others stay, except for what the repair has to clear (a node close to the drop): at most one repair step.
        for other in children.iter().filter(|c| **c != node) {
            let (p, q) = (at(&start, other), at(&dragged, other));
            assert!(
                (p.0 - q.0).hypot(p.1 - q.1) < 60.,
                "dragging the pin of {node} moved {other} from {p:?} to {q:?}"
            );
        }
    }
}

/// The sequence of a session: drops without a pin, a drag of a pin, then a click on the pin of another node. Nothing but the
/// dragged node moves on the drawing; a click on a pin moves nothing.
#[test]
fn a_session_of_drops_and_pins_in_a_circle_container_keeps_the_drawing_still() {
    let graph: Graph = serde_json::from_str(MIXED).expect("the fixture is a graph");
    let mut current = import_graph(graph, 100).expect("imports");
    let at = |s: &Snapshot, n: &str| {
        let r = &absolute_all(&s.graph, &s.boxes)[n];
        (r.x, r.y)
    };
    let children = ["auth", "orders", "billing", "search", "cache"];
    let release = |s: &Snapshot, node: &str, dx: f64, dy: f64| {
        let r = s.boxes[node].clone();
        let mut next = s.clone();
        for (seq, phase, position, release) in [
            (1, MovePhase::Begin, Some(Point { x: r.x, y: r.y }), false),
            (
                2,
                MovePhase::Update,
                Some(Point {
                    x: r.x + dx,
                    y: r.y + dy,
                }),
                false,
            ),
            (3, MovePhase::Finish, None, true),
        ] {
            let step = MoveStep {
                gesture_id: format!("s-{node}-{dx}"),
                seq,
                phase,
                node: node.into(),
                position,
                release,
                others: Default::default(),
            };
            next = move_node(&next, next.revision, next.layout_version, &step).expect("step");
        }
        next
    };
    for (node, dx, dy) in [
        ("billing", 50., 30.),
        ("orders", -35., 35.),
        ("search", 30., 50.),
    ] {
        current = release(&current, node, dx, dy);
    }
    current = drag(&current, "cache", 35., 25.);
    // Click on the pin of auth: pin it where it is.
    let before: Vec<_> = children.iter().map(|n| at(&current, n)).collect();
    let r = current.boxes["auth"].clone();
    let mut all = current.options.pins.clone();
    all.insert("auth".to_string(), Point { x: r.x, y: r.y });
    let pinned = configure_snapshot(
        &current,
        current.revision,
        current.layout_version,
        &json!({ "pins": all }),
    )
    .expect("the pin is accepted");
    for (n, b) in children.iter().zip(&before) {
        let a = at(&pinned, n);
        assert!(
            (a.0 - b.0).hypot(a.1 - b.1) < 2.,
            "the click on the pin of auth moved {n} from {b:?} to {a:?} (mesh {:.0} to {:.0})",
            current.boxes["mesh"].width,
            pinned.boxes["mesh"].width
        );
    }
}

/// A circle container collapses into one node and expands again with its children where they were.
#[test]
fn a_circle_container_collapses_and_expands() {
    let graph: Graph = serde_json::from_str(MIXED).expect("the fixture is a graph");
    let start = import_graph(graph, 100).expect("imports");
    let collapsed =
        arqlet_runtime::toggle_container(&start, start.revision, start.layout_version, "mesh")
            .expect("the container collapses");
    assert!(collapsed.collapsed_ids.contains(&"mesh".to_string()));
    let mesh = &collapsed.boxes["mesh"];
    assert_eq!(
        mesh.width, mesh.height,
        "a collapsed circle container is a circle node"
    );
    assert!(
        !collapsed.boxes.contains_key("orders"),
        "its children are hidden"
    );
    let expanded = arqlet_runtime::toggle_container(
        &collapsed,
        collapsed.revision,
        collapsed.layout_version,
        "mesh",
    )
    .expect("the container expands");
    assert!(expanded.boxes.contains_key("orders"));
    assert!(overshoot(&expanded, "mesh") < 1e-6);
}
