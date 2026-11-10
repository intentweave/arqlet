//! A Petri net with circular places and rectangular transitions under an organic area with straight edges: the first
//! computation and the computation after the consumer reports the drawn label sizes leave no conflict that the engine does
//! not report, and the shapes keep their outlines.

use arqlet_model::{Graph, NodeSize, Snapshot};
use arqlet_runtime::{import_graph, measure_snapshot};
use std::collections::BTreeMap;

const PETRI: &str = include_str!("../../../examples/fixtures/petri-net-shapes.json");

fn graph() -> Graph {
    serde_json::from_str(PETRI).expect("the fixture is a graph")
}

fn assert_clean(snapshot: &Snapshot, context: &str) {
    // The drawing is published as valid only without geometry or routing conflicts; this diagram has room for that.
    assert!(
        snapshot.geometry_issues.is_empty(),
        "{context}: {:?}",
        snapshot.geometry_issues
    );
    assert!(
        snapshot.routing_issues.is_empty(),
        "{context}: {:?}",
        snapshot.routing_issues
    );
    for route in &snapshot.routes {
        assert!(route.points.len() <= 2, "{context}: {} is bent", route.id);
    }
}

#[test]
fn places_are_circles_and_the_first_layout_is_conflict_free() {
    let snapshot = import_graph(graph(), 100).expect("imports");
    for node in &snapshot.graph.nodes {
        let r = &snapshot.boxes[&node.id];
        if node.kind == "place" {
            assert_eq!(
                r.width, r.height,
                "{} is a circle: its box is square",
                node.id
            );
        }
    }
    assert_clean(&snapshot, "first layout");
}

#[test]
fn the_layout_after_label_measurement_is_conflict_free() {
    let snapshot = import_graph(graph(), 100).expect("imports");
    // What a consumer reports for a drawn label: a text box, which a circle takes as its larger side.
    let sizes: BTreeMap<String, NodeSize> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| {
            let width = 8. * n.label.chars().count() as f64 + 24.;
            (n.id.clone(), NodeSize { width, height: 36. })
        })
        .collect();
    let measured = measure_snapshot(&snapshot, snapshot.revision, snapshot.layout_version, sizes)
        .expect("the sizes are accepted");
    assert_clean(&measured, "after measurement");
}

/// Prints the time of each step of a move gesture on the measured Petri net. Run:
/// `cargo test -p arqlet-runtime --test petri_shapes --release -- --ignored --nocapture`.
#[test]
#[ignore = "measurement: prints the time of each gesture step"]
fn gesture_step_times() {
    use arqlet_model::{MovePhase, MoveStep, Point};
    use std::time::Instant;
    let snapshot = import_graph(graph(), 100).expect("imports");
    let sizes: BTreeMap<String, NodeSize> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| {
            let width = 8. * n.label.chars().count() as f64 + 24.;
            (n.id.clone(), NodeSize { width, height: 36. })
        })
        .collect();
    let t = Instant::now();
    let mut current =
        measure_snapshot(&snapshot, snapshot.revision, snapshot.layout_version, sizes)
            .expect("measured");
    println!("measure: {} ms", t.elapsed().as_millis());
    for node in ["p0-start", "lock", "c1-take"] {
        let r = current.boxes[node].clone();
        for (seq, phase, dx) in [
            (1, MovePhase::Begin, 0.),
            (2, MovePhase::Update, 25.),
            (3, MovePhase::Update, 50.),
            (4, MovePhase::Update, 75.),
            (5, MovePhase::Finish, 0.),
        ] {
            let step = MoveStep {
                gesture_id: format!("g-{node}"),
                seq,
                phase,
                node: node.into(),
                position: (!matches!(phase, MovePhase::Finish)).then_some(Point {
                    x: r.x + dx,
                    y: r.y,
                }),
                release: false,
                others: Default::default(),
            };
            let t = Instant::now();
            current = arqlet_runtime::move_node(
                &current,
                current.revision,
                current.layout_version,
                &step,
            )
            .expect("step");
            println!(
                "{node} {phase:?}: {} ms, {} geometry issues",
                t.elapsed().as_millis(),
                current.geometry_issues.len()
            );
        }
    }
}
