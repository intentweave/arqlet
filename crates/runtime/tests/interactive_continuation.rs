//! Interactive continuation on mixed diagrams: after a move, a resize, a shape change and a second move, the engine continues
//! from the previous state. The invariants below are what the layout path promises; the ignored test measures how far the
//! continuation holds (unrelated nodes that move, conflicts, time) against a layout from scratch.

use arqlet_layout::absolute_all;
use arqlet_model::outline::{collides, within, Outline};
use arqlet_model::{Graph, MovePhase, MoveStep, NodeShape, NodeSize, Point, Snapshot};
use arqlet_runtime::{import_graph, measure_snapshot, move_node, set_shape};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

/// The mixed diagram of the straight-edge repair measurement: every third node a circle, one edge per node pair, an organic
/// root and straight routing. The same fixed generator, so the two measurements describe the same inputs.
fn diagram(seed: u64, nodes: usize, edges: usize) -> Graph {
    let mut state = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let mut next = |bound: usize| {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((state >> 33) as usize) % bound
    };
    let nodes_json: Vec<_> = (0..nodes)
        .map(|i| {
            serde_json::json!({
                "id": format!("n{i}"),
                "kind": "node",
                "label": format!("N{i}"),
                "parentId": null,
                "shape": if i % 3 == 0 { "circle" } else { "rect" },
            })
        })
        .collect();
    let mut edges_json = Vec::new();
    let mut pairs = BTreeSet::new();
    while edges_json.len() < edges {
        let a = next(nodes);
        let b = next(nodes);
        if a != b && pairs.insert((a.min(b), a.max(b))) {
            edges_json.push(serde_json::json!({
                "id": format!("e{}", edges_json.len()),
                "kind": "flow",
                "source": format!("n{a}"),
                "target": format!("n{b}"),
            }));
        }
    }
    serde_json::from_value(serde_json::json!({
        "nodes": nodes_json,
        "edges": edges_json,
        "defaultDirection": "auto",
        "defaultStrategy": "organic",
        "defaultRouting": "straight",
    }))
    .expect("the diagram is valid")
}

fn centres(snapshot: &Snapshot) -> BTreeMap<String, (f64, f64)> {
    absolute_all(&snapshot.graph, &snapshot.boxes)
        .into_iter()
        .map(|(id, r)| (id, (r.x + r.width / 2., r.y + r.height / 2.)))
        .collect()
}

/// Nodes other than `edited` whose centre moved by more than one unit between two snapshots.
fn drift(before: &Snapshot, after: &Snapshot, edited: &str) -> usize {
    let a = centres(before);
    let b = centres(after);
    a.iter()
        .filter(|(id, _)| id.as_str() != edited)
        .filter(|(id, (x, y))| b.get(*id).is_none_or(|(u, v)| (x - u).hypot(y - v) > 1.))
        .count()
}

/// The displacement of every unrelated node between two snapshots, after the common shift of the unrelated nodes (their
/// median) is taken out: a layout that is only translated as a whole has none. What remains is a rearrangement.
fn displacements(before: &Snapshot, after: &Snapshot, edited: &str) -> Vec<f64> {
    let a = centres(before);
    let b = centres(after);
    let shifts: Vec<(f64, f64)> = a
        .iter()
        .filter(|(id, _)| id.as_str() != edited)
        .filter_map(|(id, (x, y))| b.get(id).map(|(u, v)| (u - x, v - y)))
        .collect();
    if shifts.is_empty() {
        return vec![];
    }
    let median = |mut values: Vec<f64>| {
        values.sort_by(f64::total_cmp);
        values[values.len() / 2]
    };
    let dx = median(shifts.iter().map(|s| s.0).collect());
    let dy = median(shifts.iter().map(|s| s.1).collect());
    shifts
        .iter()
        .map(|(sx, sy)| (sx - dx).hypot(sy - dy))
        .collect()
}

/// What is wrong with the drawing: straight edges through foreign nodes, node pairs whose outlines collide, pairs closer than
/// the clearance, and the geometry issues the engine reports for any of these.
#[derive(Debug, Default, Clone, Copy)]
struct Conflicts {
    through: usize,
    overlap: usize,
    close: usize,
    reported: usize,
}

fn conflicts(snapshot: &Snapshot) -> Conflicts {
    let through = snapshot
        .routing_issues
        .iter()
        .filter(|i| i.starts_with("straight edge") && i.contains("passes through node"))
        .count();
    let reported = snapshot
        .geometry_issues
        .iter()
        .filter(|i| {
            matches!(
                i.kind.as_str(),
                "overlap" | "fixed-overlap" | "straight-through-node" | "outside-area"
            )
        })
        .count();
    let absolute = absolute_all(&snapshot.graph, &snapshot.boxes);
    let outlines: Vec<Outline> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| Outline::of(n.shape, &absolute[n.id.as_str()]))
        .collect();
    let (mut overlap, mut close) = (0, 0);
    for i in 0..outlines.len() {
        for j in i + 1..outlines.len() {
            if collides(&outlines[i], &outlines[j]) {
                overlap += 1;
            } else if within(&outlines[i], &outlines[j], 12.) {
                close += 1;
            }
        }
    }
    Conflicts {
        through,
        overlap,
        close,
        reported,
    }
}

/// The pairs of nodes whose outlines collide.
fn colliding_pairs(snapshot: &Snapshot) -> Vec<(String, String)> {
    let absolute = absolute_all(&snapshot.graph, &snapshot.boxes);
    let nodes = &snapshot.graph.nodes;
    let outline = |i: usize| Outline::of(nodes[i].shape, &absolute[nodes[i].id.as_str()]);
    let mut pairs = Vec::new();
    for i in 0..nodes.len() {
        for j in i + 1..nodes.len() {
            if collides(&outline(i), &outline(j)) {
                pairs.push((nodes[i].id.clone(), nodes[j].id.clone()));
            }
        }
    }
    pairs
}

fn step(
    gesture: &str,
    seq: u64,
    phase: MovePhase,
    node: &str,
    position: Option<Point>,
) -> MoveStep {
    MoveStep {
        gesture_id: gesture.into(),
        seq,
        phase,
        node: node.into(),
        position,
        release: false,
        others: Default::default(),
    }
}

/// Moves a root node by (dx, dy) as one gesture: begin, update, finish keeping the pin.
fn move_by(snapshot: &Snapshot, node: &str, dx: f64, dy: f64, gesture: &str) -> Snapshot {
    let box_ = snapshot.boxes[node].clone();
    let target = Point {
        x: box_.x + dx,
        y: box_.y + dy,
    };
    let mut current = snapshot.clone();
    for (seq, phase, position) in [
        (
            1,
            MovePhase::Begin,
            Some(Point {
                x: box_.x,
                y: box_.y,
            }),
        ),
        (2, MovePhase::Update, Some(target)),
        (3, MovePhase::Finish, None),
    ] {
        current = move_node(
            &current,
            current.revision,
            current.layout_version,
            &step(gesture, seq, phase, node, position),
        )
        .expect("the move step is accepted");
    }
    current
}

fn resized(snapshot: &Snapshot, node: &str, width: f64, height: f64) -> Snapshot {
    measure_snapshot(
        snapshot,
        snapshot.revision,
        snapshot.layout_version,
        BTreeMap::from([(node.to_string(), NodeSize { width, height })]),
    )
    .expect("the size is accepted")
}

fn reshaped(snapshot: &Snapshot, node: &str, shape: NodeShape) -> Snapshot {
    set_shape(
        snapshot,
        snapshot.revision,
        snapshot.layout_version,
        node,
        shape,
    )
    .expect("the shape is accepted")
}

/// The node an edit applies to: the preferred one when the view has it, else the view's node at the same rank. The import keeps
/// nodes that belong to the view only, so a diagram with isolated nodes has fewer than the generator made.
fn pick(snapshot: &Snapshot, preferred: &str, rank: usize) -> String {
    if snapshot.boxes.contains_key(preferred) {
        return preferred.to_string();
    }
    let ids: Vec<&String> = snapshot.graph.nodes.iter().map(|n| &n.id).collect();
    ids[rank % ids.len()].clone()
}

/// A layout of the same graph and options from scratch, for the drift comparison.
fn from_scratch(snapshot: &Snapshot) -> Snapshot {
    arqlet_layout::compute(&snapshot.graph, None, &BTreeSet::new(), &snapshot.options)
        .expect("the graph lays out from scratch")
}

/// The sequence of edits that one measurement and one invariant check both run: a move, a resize, a shape change, a second
/// move. Returns the snapshot before each step and after it, with the edited node.
fn sequence(seed: u64) -> Vec<(&'static str, String, Snapshot, Snapshot)> {
    let start = import_graph(diagram(seed, 14, 18), 500).expect("the diagram imports");
    let mut steps = Vec::new();
    let mut current = start;
    let edits: [(&str, &str); 4] = [
        ("move", "n1"),
        ("resize", "n3"),
        ("shape", "n1"),
        ("move-2", "n5"),
    ];
    for (rank, (kind, preferred)) in edits.into_iter().enumerate() {
        let before = current.clone();
        let node = pick(&before, preferred, rank);
        let node = node.as_str();
        let after = match kind {
            "move" => move_by(&before, node, 90., 40., "g1"),
            "resize" => resized(&before, node, 150., 150.),
            "shape" => reshaped(&before, node, NodeShape::Circle),
            _ => move_by(&before, node, -60., 30., "g2"),
        };
        steps.push((kind, node.to_string(), before, after.clone()));
        current = after;
    }
    steps
}

#[test]
fn a_moved_node_stays_where_it_was_put_and_no_conflict_is_silent() {
    for seed in 1..=8u64 {
        for (kind, node, before, after) in sequence(seed) {
            let now = conflicts(&after);
            // Shapes are kept apart: the outlines of two nodes do not collide. The one exception is a contradiction the
            // user made: two pinned nodes dragged onto each other cannot both keep their pins, so the engine keeps them
            // and reports the conflict (`fixed-overlap`) instead of moving a pin.
            let collisions = colliding_pairs(&after);
            for (a, b) in &collisions {
                assert!(
                    after.options.pins.contains_key(a) && after.options.pins.contains_key(b),
                    "seed {seed}, {kind} of {node}: {a} and {b} collide and one of them is not pinned; \
                     before=({:?}, {:?}), after=({:?}, {:?}), issues={:?}",
                    before.boxes.get(a),
                    before.boxes.get(b),
                    after.boxes.get(a),
                    after.boxes.get(b),
                    after.geometry_issues
                );
                assert!(
                    after.geometry_issues.iter().any(|i| i.kind == "fixed-overlap"
                        && i.node_ids.contains(a)
                        && i.node_ids.contains(b)),
                    "seed {seed}, {kind} of {node}: the pinned overlap of {a} and {b} is not reported"
                );
            }
            // A straight edge through a foreign node is reported as a geometry issue: nothing is published as valid.
            assert!(
                now.through == 0 || now.reported > 0,
                "seed {seed}, {kind} of {node}: {} straight edges pass through nodes, none reported",
                now.through
            );
        }
    }
}

#[test]
fn a_pinned_node_is_placed_at_its_pin_through_the_whole_sequence() {
    for seed in 1..=8u64 {
        let steps = sequence(seed);
        let (_, _, _, last) = steps.last().expect("steps");
        for (id, pin) in &last.options.pins {
            let placed = &last.boxes[id];
            assert!(
                (placed.x - pin.x).abs() < 1e-6 && (placed.y - pin.y).abs() < 1e-6,
                "seed {seed}: {id} is at ({}, {}), its pin is ({}, {})",
                placed.x,
                placed.y,
                pin.x,
                pin.y
            );
        }
    }
}

/// Prints, per step kind: the unrelated nodes that move with the warm start and from scratch, the conflicts, and the time of
/// the engine step. Run: `cargo test -p arqlet-runtime --test interactive_continuation --release -- --ignored --nocapture`.
#[test]
#[ignore = "measurement: prints per-step statistics"]
fn continuation_on_mixed_diagrams() {
    #[derive(Default)]
    struct Total {
        warm: Vec<f64>,
        cold: Vec<f64>,
        through: usize,
        overlap: usize,
        close: usize,
        reported: usize,
        milliseconds: f64,
        count: usize,
    }
    let mut totals: BTreeMap<&str, Total> = BTreeMap::new();
    for seed in 1..=20u64 {
        // Rebuild the sequence step by step so each edit can be timed.
        let mut current = import_graph(diagram(seed, 14, 18), 500).expect("imports");
        let edits: [(&'static str, &str); 4] = [
            ("move", "n1"),
            ("resize", "n3"),
            ("shape", "n1"),
            ("move-2", "n5"),
        ];
        for (rank, (kind, preferred)) in edits.into_iter().enumerate() {
            let before = current.clone();
            let node = pick(&before, preferred, rank);
            let node = node.as_str();
            let started = Instant::now();
            let after = match kind {
                "move" => move_by(&before, node, 90., 40., "g1"),
                "resize" => resized(&before, node, 150., 150.),
                "shape" => reshaped(&before, node, NodeShape::Circle),
                _ => move_by(&before, node, -60., 30., "g2"),
            };
            let elapsed = started.elapsed().as_secs_f64() * 1000.;
            let cold = from_scratch(&after);
            let now = conflicts(&after);
            let total = totals.entry(kind).or_default();
            total.warm.extend(displacements(&before, &after, node));
            total.cold.extend(displacements(&before, &cold, node));
            total.through += now.through;
            total.overlap += now.overlap;
            total.close += now.close;
            total.reported += now.reported;
            total.milliseconds += elapsed;
            total.count += 1;
            current = after;
        }
    }
    let summary = |values: &[f64]| {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        let share = |limit: f64| {
            100. * sorted.iter().filter(|v| **v > limit).count() as f64 / sorted.len().max(1) as f64
        };
        let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0.);
        format!(
            "median {median:6.1}  >10: {:3.0}%  >40: {:3.0}%",
            share(10.),
            share(40.)
        )
    };
    println!("unrelated-node displacement after removing the common shift (layout units)");
    for (kind, t) in &totals {
        println!(
            "{kind:8} warm {}   cold {}",
            summary(&t.warm),
            summary(&t.cold)
        );
    }
    println!("step      through overlap close reported  ms/step");
    for (kind, t) in &totals {
        println!(
            "{kind:8} {:7} {:7} {:5} {:8} {:8.2}",
            t.through,
            t.overlap,
            t.close,
            t.reported,
            t.milliseconds / t.count as f64
        );
    }
}
