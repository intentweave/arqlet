//! Force-directed behaviour of organic areas (`forceDirected`): a layout that continues an earlier one relaxes the stress from
//! the nodes' positions for a few steps instead of holding them. A node that is pinned or being dragged is held, the others
//! follow it like on springs, and a node that is let go settles. Off by default: then nothing but the edited node moves.

use arqlet_layout::absolute_all;
use arqlet_model::{Graph, MovePhase, MoveStep, Point, Snapshot};
use arqlet_runtime::{configure_snapshot, import_graph, move_node};
use serde_json::json;
use std::collections::BTreeSet;

const PETRI: &str = include_str!("../../../examples/fixtures/petri-net-shapes.json");

fn start(force: bool) -> Snapshot {
    let graph: Graph = serde_json::from_str(PETRI).expect("the fixture is a graph");
    let snapshot = import_graph(graph, 100).expect("imports");
    if !force {
        return snapshot;
    }
    configure_snapshot(
        &snapshot,
        snapshot.revision,
        snapshot.layout_version,
        &json!({"forceDirected": true}),
    )
    .expect("the option is accepted")
}

fn centre(snapshot: &Snapshot, id: &str) -> (f64, f64) {
    let r = &absolute_all(&snapshot.graph, &snapshot.boxes)[id];
    (r.x + r.width / 2., r.y + r.height / 2.)
}

/// Where the neighbours of `node` (over the graph's edges) are, as the mean distance to it.
fn neighbour_distance(snapshot: &Snapshot, node: &str) -> f64 {
    let neighbours: BTreeSet<&str> = snapshot
        .graph
        .edges
        .iter()
        .filter_map(|e| {
            let t = e.target.as_deref()?;
            (e.source == node)
                .then_some(t)
                .or((t == node).then_some(e.source.as_str()))
        })
        .collect();
    let me = centre(snapshot, node);
    let total: f64 = neighbours
        .iter()
        .map(|n| {
            let c = centre(snapshot, n);
            (c.0 - me.0).hypot(c.1 - me.1)
        })
        .sum();
    total / neighbours.len() as f64
}

fn step(
    snapshot: &Snapshot,
    node: &str,
    seq: u64,
    phase: MovePhase,
    to: Option<(f64, f64)>,
    release: bool,
) -> Snapshot {
    let s = MoveStep {
        gesture_id: format!("force-{node}"),
        seq,
        phase,
        node: node.into(),
        position: to.map(|(x, y)| Point { x, y }),
        release,
        others: Default::default(),
    };
    move_node(snapshot, snapshot.revision, snapshot.layout_version, &s)
        .expect("the step is accepted")
}

/// A drag of `node` by (dx, dy) in the given number of updates, then the drop: with a pin kept or released.
fn drag(
    snapshot: &Snapshot,
    node: &str,
    dx: f64,
    dy: f64,
    updates: u64,
    release: bool,
) -> (Snapshot, Snapshot) {
    let r = snapshot.boxes[node].clone();
    let mut current = step(snapshot, node, 1, MovePhase::Begin, Some((r.x, r.y)), false);
    for k in 1..=updates {
        let f = k as f64 / updates as f64;
        current = step(
            &current,
            node,
            1 + k,
            MovePhase::Update,
            Some((r.x + dx * f, r.y + dy * f)),
            false,
        );
    }
    let held = current.clone();
    let done = step(
        &current,
        node,
        updates + 2,
        MovePhase::Finish,
        None,
        release,
    );
    (held, done)
}

/// How much the mean distance between `node` and its neighbours grew from `before` to `after`.
fn growth(before: &Snapshot, after: &Snapshot, node: &str) -> f64 {
    neighbour_distance(after, node) - neighbour_distance(before, node)
}

#[test]
fn without_force_directed_the_neighbours_stay_behind() {
    let before = start(false);
    let (held, _) = drag(&before, "lock", 300., 0., 4, false);
    let grown = growth(&before, &held, "lock");
    assert!(
        grown > 40.,
        "the neighbours did not stay behind: {grown:.0}"
    );
}

#[test]
fn with_force_directed_the_neighbours_follow_the_dragged_node() {
    let (plain_start, force_start) = (start(false), start(true));
    let (plain_held, _) = drag(&plain_start, "lock", 300., 0., 4, false);
    let t = std::time::Instant::now();
    let (force_held, _) = drag(&force_start, "lock", 300., 0., 4, false);
    let milliseconds = t.elapsed().as_millis();
    let (plain, force) = (
        growth(&plain_start, &plain_held, "lock"),
        growth(&force_start, &force_held, "lock"),
    );
    assert!(
        force < plain - 20.,
        "the neighbours stayed away: they are {force:.0} farther with the force, {plain:.0} without"
    );
    // The dragged node is held at the pointer, in both.
    for (s, h) in [(&plain_start, &plain_held), (&force_start, &force_held)] {
        let (a, b) = (centre(s, "lock"), centre(h, "lock"));
        assert!(
            ((b.0 - a.0) - 300.).abs() < 3. && (b.1 - a.1).abs() < 3.,
            "the held node is not at the pointer: {a:?} to {b:?}"
        );
    }
    eprintln!("{milliseconds} ms for six steps");
}

#[test]
fn a_node_that_is_let_go_settles_toward_its_neighbours() {
    let before = start(true);
    let (held, released) = drag(&before, "lock", 300., 0., 4, true);
    assert!(released.options.pins.is_empty(), "no pin is left");
    let (at_drop, settled) = (
        neighbour_distance(&held, "lock"),
        neighbour_distance(&released, "lock"),
    );
    assert!(
        settled < at_drop,
        "the node did not settle: {settled:.0} after, {at_drop:.0} at the drop"
    );
    // A node dragged by its pin stays: the pin holds it.
    let (_, pinned) = drag(&before, "lock", 300., 0., 4, false);
    let held_at = centre(&held, "lock");
    let pinned_at = centre(&pinned, "lock");
    assert!((held_at.0 - pinned_at.0).hypot(held_at.1 - pinned_at.1) < 3.);
}

#[test]
fn force_directed_steps_are_deterministic_and_leave_no_silent_conflict() {
    let run = || drag(&start(true), "p1-start", -200., 120., 5, true).1;
    let (a, b) = (run(), run());
    assert_eq!(format!("{:?}", a.boxes), format!("{:?}", b.boxes));
    // Every straight edge through a foreign node, and every pair of outlines that collide, is reported.
    let through = a
        .routing_issues
        .iter()
        .filter(|i| i.starts_with("straight edge") && i.contains("passes through node"))
        .count();
    assert!(
        through == 0
            || a.geometry_issues
                .iter()
                .any(|i| i.kind == "straight-through-node"),
        "{through} straight edges through nodes, none reported"
    );
}

#[test]
fn the_option_is_off_by_default_and_omitted_from_the_options() {
    let off = start(false);
    let text = serde_json::to_string(&off.options).unwrap();
    assert!(!text.contains("forceDirected"), "{text}");
    let on = start(true);
    assert!(on.options.force_directed);
    assert!(serde_json::to_string(&on.options)
        .unwrap()
        .contains("\"forceDirected\":true"));
}

/// Prints, for each step of a drag, the time and how far the nodes other than the dragged one moved. Run:
/// `cargo test -p arqlet-runtime --test force_directed --release -- --ignored --nocapture profile_steps`.
#[test]
#[ignore = "measurement"]
fn profile_steps() {
    for force in [false, true] {
        let mut current = start(force);
        let ids: Vec<String> = current.graph.nodes.iter().map(|n| n.id.clone()).collect();
        let r = current.boxes["lock"].clone();
        println!("force {force}");
        current = step(
            &current,
            "lock",
            1,
            MovePhase::Begin,
            Some((r.x, r.y)),
            false,
        );
        for k in 1..=10u64 {
            let before: Vec<(f64, f64)> = ids.iter().map(|n| centre(&current, n)).collect();
            let t = std::time::Instant::now();
            current = step(
                &current,
                "lock",
                1 + k,
                MovePhase::Update,
                Some((r.x + 40. * k as f64, r.y + 10. * k as f64)),
                false,
            );
            let ms = t.elapsed().as_millis();
            let moved: Vec<f64> = ids
                .iter()
                .zip(&before)
                .filter(|(n, _)| n.as_str() != "lock")
                .map(|(n, b)| {
                    let a = centre(&current, n);
                    (a.0 - b.0).hypot(a.1 - b.1)
                })
                .collect();
            let max = moved.iter().cloned().fold(0., f64::max);
            let mut sorted = moved.clone();
            sorted.sort_by(f64::total_cmp);
            let median = sorted[sorted.len() / 2];
            let mean = moved.iter().sum::<f64>() / moved.len() as f64;
            println!(
                "  step {k}: {ms} ms, others moved mean {mean:.0} median {median:.0} max {max:.0}, issues {}",
                current.geometry_issues.len()
            );
        }
        // The pointer stops: the same position again and again. The drawing must come to rest.
        for k in 11..=20u64 {
            let before: Vec<(f64, f64)> = ids.iter().map(|n| centre(&current, n)).collect();
            current = step(
                &current,
                "lock",
                1 + k,
                MovePhase::Update,
                Some((r.x + 400., r.y + 100.)),
                false,
            );
            let moved: Vec<f64> = ids
                .iter()
                .zip(&before)
                .filter(|(n, _)| n.as_str() != "lock")
                .map(|(n, b)| {
                    let a = centre(&current, n);
                    (a.0 - b.0).hypot(a.1 - b.1)
                })
                .collect();
            println!(
                "  rest {k}: mean {:.1} max {:.0}",
                moved.iter().sum::<f64>() / moved.len() as f64,
                moved.iter().cloned().fold(0., f64::max)
            );
        }
        let t = std::time::Instant::now();
        let done = step(&current, "lock", 22, MovePhase::Finish, None, true);
        let ms = t.elapsed().as_millis();
        let moved: Vec<f64> = ids
            .iter()
            .filter(|n| n.as_str() != "lock")
            .map(|n| {
                let (a, b) = (centre(&current, n), centre(&done, n));
                (a.0 - b.0).hypot(a.1 - b.1)
            })
            .collect();
        println!(
            "  drop: {ms} ms, mean {:.1} max {:.0}, issues {}",
            moved.iter().sum::<f64>() / moved.len() as f64,
            moved.iter().cloned().fold(0., f64::max),
            done.geometry_issues.len()
        );
    }
}

/// Steps of a drag on a diagram with orthogonal routes, from the file named by `LW_GRAPH`. Run:
/// `LW_GRAPH=benchmarks/datasets/bpmn-pools.json LW_NODE=s-request cargo test -p arqlet-runtime --test force_directed --release -- --ignored --nocapture profile_orthogonal`.
#[test]
#[ignore = "measurement"]
fn profile_orthogonal() {
    let path = std::env::var("LW_GRAPH").expect("LW_GRAPH names a graph file");
    let node = std::env::var("LW_NODE").expect("LW_NODE names the node to drag");
    let graph: Graph = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let mut current = import_graph(graph, 200).expect("imports");
    let r = current.boxes[&node].clone();
    current = step(
        &current,
        &node,
        1,
        MovePhase::Begin,
        Some((r.x, r.y)),
        false,
    );
    for k in 1..=8u64 {
        let t = std::time::Instant::now();
        current = step(
            &current,
            &node,
            1 + k,
            MovePhase::Update,
            Some((r.x + 25. * k as f64, r.y)),
            false,
        );
        println!("step {k}: {} ms", t.elapsed().as_millis());
    }
}
