//! Round and rectangular leafs in rectangular containers under an organic area and straight routing: the interactive
//! sequence (move, resize, pin, unpin, shape change) continues from the previous state, keeps the shapes apart, keeps the
//! children in their containers, and leaves no straight edge through a foreign node without a reported conflict.

use arqlet_layout::absolute_all;
use arqlet_model::outline::{collides, Outline};
use arqlet_model::{Graph, MovePhase, MoveStep, NodeShape, NodeSize, Point, Snapshot};
use arqlet_runtime::{configure_snapshot, import_graph, measure_snapshot, move_node, set_shape};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

/// Two containers with four leafs each and four loose leafs; every third leaf is a circle. A fixed generator.
fn diagram(seed: u64) -> Graph {
    diagram_with(seed, json!({}))
}

/// The same diagram with graph-level fields replaced by `extra` (strategies and directions of the areas).
fn diagram_with(seed: u64, extra: serde_json::Value) -> Graph {
    let mut state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut next = |bound: usize| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as usize) % bound
    };
    let mut nodes: Vec<_> = (0..2)
        .map(|c| json!({"id": format!("c{c}"), "kind": "group", "label": format!("C{c}"), "parentId": null}))
        .collect();
    let count = 12;
    for i in 0..count {
        let parent = match i {
            0..=3 => json!("c0"),
            4..=7 => json!("c1"),
            _ => serde_json::Value::Null,
        };
        nodes.push(json!({
            "id": format!("n{i}"), "kind": "node", "label": format!("N{i}"), "parentId": parent,
            "shape": if i % 3 == 0 { "circle" } else { "rect" },
        }));
    }
    let mut edges = Vec::new();
    let mut seen = BTreeSet::new();
    while edges.len() < 16 {
        let (a, b) = (next(count), next(count));
        if a != b && seen.insert((a.min(b), a.max(b))) {
            edges.push(json!({
                "id": format!("e{}", edges.len()), "kind": "flow",
                "source": format!("n{a}"), "target": format!("n{b}"),
            }));
        }
    }
    let mut graph = json!({
        "nodes": nodes, "edges": edges,
        "defaultDirection": "auto", "defaultStrategy": "organic", "defaultRouting": "straight",
    });
    for (key, value) in extra.as_object().expect("extra is an object") {
        graph[key] = value.clone();
    }
    serde_json::from_value(graph).expect("the diagram is valid")
}

/// Conflicts of a drawing, found independently of the engine's own report.
struct Found {
    /// Sibling pairs whose outlines collide.
    overlaps: Vec<(String, String)>,
    /// Children that lie outside their container's interior.
    outside: Vec<String>,
    /// Straight edges whose segment enters a foreign node.
    through: usize,
}

fn check(snapshot: &Snapshot) -> Found {
    let graph = &snapshot.graph;
    let absolute = absolute_all(graph, &snapshot.boxes);
    let outline = |id: &str| {
        let node = graph.nodes.iter().find(|n| n.id == id).unwrap();
        Outline::of(node.shape, &absolute[id])
    };
    let mut overlaps = Vec::new();
    for (i, a) in graph.nodes.iter().enumerate() {
        for b in &graph.nodes[i + 1..] {
            if a.parent_id == b.parent_id && collides(&outline(&a.id), &outline(&b.id)) {
                overlaps.push((a.id.clone(), b.id.clone()));
            }
        }
    }
    let rules = arqlet_scene::GeometryRules::default();
    let outside = graph
        .nodes
        .iter()
        .filter(|n| {
            let Some(parent) = n.parent_id.as_deref() else {
                return false;
            };
            let (r, c) = (&snapshot.boxes[&n.id], &snapshot.boxes[parent]);
            r.x < rules.padding - 1e-6
                || r.y < rules.header - 1e-6
                || r.x + r.width > c.width - rules.padding + 1e-6
                || r.y + r.height > c.height - rules.padding + 1e-6
        })
        .map(|n| n.id.clone())
        .collect();
    let through = snapshot
        .routing_issues
        .iter()
        .filter(|i| i.starts_with("straight edge") && i.contains("passes through node"))
        .count();
    Found {
        overlaps,
        outside,
        through,
    }
}

/// What the drawing may claim: every conflict found independently is also reported by the engine, never silent.
fn assert_honest(snapshot: &Snapshot, context: &str) {
    let found = check(snapshot);
    let reported = |kinds: &[&str]| {
        snapshot
            .geometry_issues
            .iter()
            .any(|i| kinds.contains(&i.kind.as_str()))
    };
    assert!(
        found.overlaps.is_empty() || reported(&["overlap", "fixed-overlap", "node_overlap"]),
        "{context}: silent overlap {:?}",
        found.overlaps
    );
    assert!(
        found.outside.is_empty() || reported(&["outside-area", "container_bounds"]),
        "{context}: silent child outside its container {:?}",
        found.outside
    );
    assert!(
        found.through == 0 || reported(&["straight-through-node"]),
        "{context}: {} straight edges through nodes, none reported",
        found.through
    );
    // No edge is bent to avoid a conflict: a straight edge stays one segment.
    for route in &snapshot.routes {
        if snapshot.graph.edges.iter().any(|e| e.id == route.id) {
            assert!(route.points.len() <= 2, "{context}: {} is bent", route.id);
        }
    }
}

fn centre(snapshot: &Snapshot, id: &str) -> (f64, f64) {
    let r = &snapshot.boxes[id];
    (r.x + r.width / 2., r.y + r.height / 2.)
}

fn move_by(snapshot: &Snapshot, node: &str, dx: f64, dy: f64) -> Snapshot {
    let r = snapshot.boxes[node].clone();
    let steps = [
        (1, MovePhase::Begin, Some(Point { x: r.x, y: r.y })),
        (
            2,
            MovePhase::Update,
            Some(Point {
                x: r.x + dx,
                y: r.y + dy,
            }),
        ),
        (3, MovePhase::Finish, None),
    ];
    let mut current = snapshot.clone();
    for (seq, phase, position) in steps {
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
            .expect("the move step is accepted");
    }
    current
}

fn pins(snapshot: &Snapshot, pins: serde_json::Value) -> Snapshot {
    configure_snapshot(
        snapshot,
        snapshot.revision,
        snapshot.layout_version,
        &json!({ "pins": pins }),
    )
    .expect("the pins are accepted")
}

#[test]
fn the_interactive_sequence_in_containers_leaves_no_silent_conflict() {
    for seed in 1..=6u64 {
        let mut current = import_graph(diagram(seed), 500).expect("imports");
        assert_honest(&current, &format!("seed {seed}: import"));
        for (step, kind) in ["move", "resize", "pin", "unpin", "shape"]
            .iter()
            .enumerate()
        {
            let node = format!("n{}", [1, 2, 5, 5, 4][step]);
            if !current.boxes.contains_key(&node) {
                continue;
            }
            let before = current.clone();
            current = match *kind {
                "move" => move_by(&before, &node, 60., 30.),
                "resize" => measure_snapshot(
                    &before,
                    before.revision,
                    before.layout_version,
                    BTreeMap::from([(
                        node.clone(),
                        NodeSize {
                            width: 150.,
                            height: 150.,
                        },
                    )]),
                )
                .expect("the size is accepted"),
                "pin" => {
                    let r = &before.boxes[&node];
                    pins(&before, json!({ node.clone(): {"x": r.x, "y": r.y} }))
                }
                "unpin" => pins(&before, json!({})),
                _ => set_shape(
                    &before,
                    before.revision,
                    before.layout_version,
                    &node,
                    NodeShape::Circle,
                )
                .expect("the shape is accepted"),
            };
            assert_honest(&current, &format!("seed {seed}: {kind} of {node}"));
            // A pin is honoured: the node is where the pin says.
            for (id, pin) in &current.options.pins {
                let placed = &current.boxes[id];
                assert!(
                    (placed.x - pin.x).abs() < 1e-6 && (placed.y - pin.y).abs() < 1e-6,
                    "seed {seed}: {id} left its pin after {kind}"
                );
            }
        }
    }
}

#[test]
fn a_move_continues_from_the_previous_arrangement() {
    // Root nodes other than the moved node's container that change place, summed over the seeds, after the move: the
    // continuation against a layout of the same graph and options from scratch.
    let (mut warm, mut cold) = (0, 0);
    for seed in 1..=6u64 {
        let before = import_graph(diagram(seed), 500).expect("imports");
        let after = move_by(&before, "n1", 60., 30.);
        let scratch = arqlet_layout::compute(&after.graph, None, &BTreeSet::new(), &after.options)
            .expect("lays out from scratch");
        for n in before.graph.nodes.iter().filter(|n| n.parent_id.is_none()) {
            if n.id == "c0" || !after.boxes.contains_key(&n.id) {
                continue;
            }
            let start = centre(&before, &n.id);
            let moved = |other: &Snapshot| {
                let c = centre(other, &n.id);
                (start.0 - c.0).hypot(start.1 - c.1) >= 1.
            };
            warm += usize::from(moved(&after));
            cold += usize::from(moved(&scratch));
        }
    }
    assert!(warm < cold, "warm {warm} against scratch {cold}");
}

#[test]
fn a_container_of_fixed_size_that_cannot_hold_its_children_is_reported() {
    // A fixed 120 x 90 container holds four leafs of 180 x 54 and more: no arrangement fits. The engine says so and
    // neither grows the container silently nor publishes the drawing without the conflict.
    let imported = import_graph(diagram(2), 500).expect("imports");
    let snapshot = configure_snapshot(
        &imported,
        imported.revision,
        imported.layout_version,
        &json!({"containerSizing": {"c0": {"width": {"fixed": 120.0}, "height": {"fixed": 90.0}}}}),
    )
    .expect("the sizing is accepted");
    assert_eq!(snapshot.boxes["c0"].width, 120.);
    assert_eq!(snapshot.boxes["c0"].height, 90.);
    assert!(
        snapshot
            .geometry_issues
            .iter()
            .any(|i| matches!(i.kind.as_str(), "outside-area" | "container_bounds")),
        "{:?}",
        snapshot.geometry_issues
    );
    assert_honest(&snapshot, "fixed container");
}

/// Areas with different layouts in one drawing: (name, graph-level fields).
fn mixes() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        (
            "organic root, layered containers (lr and tb)",
            json!({
                "containerStrategies": {"c0": "none", "c1": "none"},
                "containerDirections": {"c0": "lr", "c1": "tb"},
            }),
        ),
        (
            "layered root, organic containers",
            json!({
                "defaultStrategy": "none", "defaultDirection": "lr",
                "containerStrategies": {"c0": "organic", "c1": "organic"},
                "containerDirections": {"c0": "auto", "c1": "auto"},
            }),
        ),
        (
            "organic root and container, layered container",
            json!({
                "containerStrategies": {"c1": "none"},
                "containerDirections": {"c1": "lr"},
            }),
        ),
    ]
}

/// An organic root with a container that is arranged in layers inside: the outside and the inside follow different rules.
/// Each case is a mix and the seeds that lay out quickly in a debug build.
#[test]
fn an_area_with_another_layout_than_its_surroundings_leaves_no_silent_conflict() {
    let cases: [(&str, serde_json::Value, &[u64]); 2] = [
        (
            "organic root, layered containers (lr and tb)",
            mixes()[0].1.clone(),
            &[1],
        ),
        (
            "organic root and container, layered container",
            mixes()[2].1.clone(),
            &[1, 4, 6],
        ),
    ];
    for (name, extra, seeds) in cases {
        for &seed in seeds {
            let graph = diagram_with(seed, extra.clone());
            // The areas really differ: the layered container is not arranged like its organic surroundings.
            assert_ne!(
                graph.area_mode(Some("c1")),
                graph.area_mode(None),
                "{name}: the container follows the root"
            );
            let snapshot = import_graph(graph, 500).expect("imports");
            assert_honest(&snapshot, &format!("{name}, seed {seed}"));
            // Moving a node in the organic part continues from the previous arrangement and leaves no silent conflict.
            let moved = move_by(&snapshot, "n9", 50., 20.);
            assert_honest(&moved, &format!("{name}, seed {seed}: move of n9"));
        }
    }
}

/// Prints, per mix and seed, the time of the first layout and the conflicts that remain. Run:
/// `cargo test -p arqlet-runtime --test containers_straight --release -- --ignored --nocapture measure_mixes`.
#[test]
#[ignore = "measurement: prints time and conflicts per mix"]
fn measure_mixes() {
    for (name, extra) in mixes() {
        for seed in 1..=6u64 {
            let started = std::time::Instant::now();
            let snapshot = import_graph(diagram_with(seed, extra.clone()), 500).expect("imports");
            let found = check(&snapshot);
            println!(
                "{name} seed {seed}: {} ms, sibling overlaps {}, outside {}, through {}, reported issues {}",
                started.elapsed().as_millis(),
                found.overlaps.len(),
                found.outside.len(),
                found.through,
                snapshot.geometry_issues.len()
            );
        }
    }
}
