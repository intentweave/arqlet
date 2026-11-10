//! A drag in "move only" mode ends with `release`: the drop leaves no pin. In an organic area the node stays where it was
//! dropped (the continuation keeps it as the start position and arranges the others around it); in a layered area the
//! layout places it again by its own rules, because nothing holds the position.

use arqlet_model::{MovePhase, MoveStep, Point, Snapshot};
use arqlet_runtime::{import_graph, move_node};

const PETRI: &str = include_str!("../../../examples/fixtures/petri-net-shapes.json");
const LAYERED: &str = include_str!("../../../examples/fixtures/small.json");

fn drag(snapshot: &Snapshot, node: &str, dx: f64, dy: f64, release: bool) -> Snapshot {
    let r = snapshot.boxes[node].clone();
    let at = |x: f64, y: f64| Some(Point { x, y });
    let steps = [
        (1, MovePhase::Begin, at(r.x, r.y), false),
        (2, MovePhase::Update, at(r.x + dx, r.y + dy), false),
        (3, MovePhase::Finish, None, release),
    ];
    let mut current = snapshot.clone();
    for (seq, phase, position, release) in steps {
        let step = MoveStep {
            gesture_id: format!("g-{node}"),
            seq,
            phase,
            node: node.into(),
            position,
            release,
            others: Default::default(),
        };
        current = move_node(&current, current.revision, current.layout_version, &step)
            .expect("the step is accepted");
    }
    current
}

fn distance(a: &Snapshot, b: &Snapshot, node: &str) -> f64 {
    let (p, q) = (&a.boxes[node], &b.boxes[node]);
    (p.x - q.x).hypot(p.y - q.y)
}

#[test]
fn a_released_drop_leaves_no_pin_and_the_node_stays_in_an_organic_area() {
    let start = import_graph(serde_json::from_str(PETRI).unwrap(), 100).expect("imports");
    let pinned = drag(&start, "lock", 40., 0., false);
    let moved = drag(&start, "lock", 40., 0., true);
    assert!(
        pinned.options.pins.contains_key("lock"),
        "pin mode keeps the pin"
    );
    assert!(moved.options.pins.is_empty(), "move mode leaves no pin");
    // The node stays near the drop: nothing pulls it back. The repair may nudge it for a conflict, nothing more.
    assert!(
        distance(&start, &moved, "lock") > 20.,
        "the node went back: moved {} units",
        distance(&start, &moved, "lock")
    );
    assert!(
        moved.geometry_issues.is_empty(),
        "{:?}",
        moved.geometry_issues
    );
}

#[test]
fn a_released_drop_in_a_layered_area_is_placed_by_the_layout_again() {
    let start = import_graph(serde_json::from_str(LAYERED).unwrap(), 100).expect("imports");
    let node = start.graph.nodes[0].id.clone();
    let moved = drag(&start, &node, 120., 80., true);
    assert!(moved.options.pins.is_empty());
    assert!(
        distance(&start, &moved, &node) < 1.,
        "the layout is authoritative without a pin: {} units from its place",
        distance(&start, &moved, &node)
    );
}
