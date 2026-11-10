//! The move gesture contract: begin, update, finish (keep or release), cancel; sequence and gesture checks; pins honoured.

use arqlet_model::{MovePhase, MoveStep, Point};
use arqlet_runtime::{import_view, move_node};

fn step(
    gesture: &str,
    seq: u64,
    phase: MovePhase,
    node: &str,
    position: Option<Point>,
    release: bool,
) -> MoveStep {
    MoveStep {
        gesture_id: gesture.into(),
        seq,
        phase,
        node: node.into(),
        position,
        release,
        others: Default::default(),
    }
}

fn at(x: f64, y: f64) -> Option<Point> {
    Some(Point { x, y })
}

fn fixture() -> arqlet_model::Snapshot {
    let text = include_str!("../../../examples/fixtures/small.json");
    import_view(text, 100).expect("import")
}

fn apply(
    snapshot: &arqlet_model::Snapshot,
    step: &MoveStep,
) -> Result<arqlet_model::Snapshot, String> {
    move_node(snapshot, snapshot.revision, snapshot.layout_version, step)
}

#[test]
fn a_moved_leaf_is_placed_at_its_pin_and_the_pin_stays_after_finish() {
    let s0 = fixture();
    let s1 = apply(
        &s0,
        &step("g1", 1, MovePhase::Begin, "a", at(40., 60.), false),
    )
    .unwrap();
    let s2 = apply(
        &s1,
        &step("g1", 2, MovePhase::Update, "a", at(120., 90.), false),
    )
    .unwrap();
    let s3 = apply(&s2, &step("g1", 3, MovePhase::Finish, "a", None, false)).unwrap();
    assert_eq!(s3.options.pins.get("a"), Some(&Point { x: 120., y: 90. }));
    let parent = s3.boxes["root"].clone();
    let placed = &s3.boxes["a"];
    // The pin is in the parent's space; the box is at the pin (in the parent's coordinates, the box is relative to its parent).
    assert!(
        (placed.x - 120.).abs() < 1e-6 && (placed.y - 90.).abs() < 1e-6,
        "placed {:?} in {:?}",
        placed,
        parent
    );
    assert!(!s3.gesture.as_ref().unwrap().active);
}

#[test]
fn finish_with_release_removes_the_pin() {
    let s0 = fixture();
    let s1 = apply(
        &s0,
        &step("g1", 1, MovePhase::Begin, "a", at(40., 60.), false),
    )
    .unwrap();
    let s2 = apply(&s1, &step("g1", 2, MovePhase::Finish, "a", None, true)).unwrap();
    assert!(s2.options.pins.get("a").is_none());
}

#[test]
fn cancel_restores_the_pin_the_node_had_before() {
    let s0 = fixture();
    let s1 = apply(
        &s0,
        &step("g1", 1, MovePhase::Begin, "a", at(40., 60.), false),
    )
    .unwrap();
    let s2 = apply(&s1, &step("g1", 2, MovePhase::Finish, "a", None, false)).unwrap();
    let s3 = apply(
        &s2,
        &step("g2", 1, MovePhase::Begin, "a", at(200., 200.), false),
    )
    .unwrap();
    let s4 = apply(&s3, &step("g2", 2, MovePhase::Cancel, "a", None, false)).unwrap();
    assert_eq!(s4.options.pins.get("a"), Some(&Point { x: 40., y: 60. }));
    let s0c = apply(
        &s0,
        &step("g3", 1, MovePhase::Begin, "a", at(9., 9.), false),
    )
    .unwrap();
    let s0x = apply(&s0c, &step("g3", 2, MovePhase::Cancel, "a", None, false)).unwrap();
    assert!(
        s0x.options.pins.get("a").is_none(),
        "no earlier pin: cancel leaves none"
    );
}

#[test]
fn a_stale_step_is_refused_and_so_is_a_second_gesture() {
    let s0 = fixture();
    let s1 = apply(
        &s0,
        &step("g1", 5, MovePhase::Begin, "a", at(40., 60.), false),
    )
    .unwrap();
    assert!(
        apply(
            &s1,
            &step("g1", 5, MovePhase::Update, "a", at(41., 61.), false)
        )
        .is_err(),
        "repeated seq"
    );
    assert!(
        apply(
            &s1,
            &step("g1", 4, MovePhase::Update, "a", at(41., 61.), false)
        )
        .is_err(),
        "older seq"
    );
    assert!(
        apply(
            &s1,
            &step("g2", 1, MovePhase::Begin, "b", at(1., 1.), false)
        )
        .is_err(),
        "a second gesture while one is active"
    );
    assert!(
        apply(
            &s1,
            &step("g1", 6, MovePhase::Update, "b", at(1., 1.), false)
        )
        .is_err(),
        "another node in the gesture"
    );
}

#[test]
fn a_stale_layout_version_is_refused() {
    let s0 = fixture();
    let s1 = apply(
        &s0,
        &step("g1", 1, MovePhase::Begin, "a", at(40., 60.), false),
    )
    .unwrap();
    // The revision is the graph's and does not change on a move; a caller that holds the earlier layout version is stale.
    let stale = move_node(
        &s1,
        s1.revision,
        s0.layout_version,
        &step("g1", 2, MovePhase::Update, "a", at(1., 1.), false),
    );
    assert!(stale.is_err());
}

#[test]
fn moves_of_unknown_nodes_are_refused() {
    let s0 = fixture();
    assert!(apply(
        &s0,
        &step("g1", 1, MovePhase::Begin, "missing", at(1., 1.), false)
    )
    .is_err());
}

#[test]
fn a_container_moves_as_one_and_its_children_keep_their_frame() {
    // `inner` holds `a`; the children's boxes are in their container's frame, so they move with it and keep their place.
    // The pipeline before single pass (ADR 0016): a pin is not a single-pass input, so with the default the pinned view falls
    // back to that pipeline and the container's inside is laid out again; here the move is checked on one pipeline.
    let graph: arqlet_model::Graph = serde_json::from_str(include_str!(
        "../../../examples/fixtures/structure/nested.json"
    ))
    .unwrap();
    let legacy = arqlet_model::LayoutOptions {
        single_pass_auto: false,
        single_pass: false,
        abstract_routing: false,
        ..Default::default()
    };
    let s0 = arqlet_runtime::import_graph_with(graph, 100, &legacy).expect("import");
    let start = s0.boxes["inner"].clone();
    let child = s0.boxes["a"].clone();
    let s1 = apply(
        &s0,
        &step(
            "g1",
            1,
            MovePhase::Begin,
            "inner",
            at(start.x, start.y),
            false,
        ),
    )
    .unwrap();
    let s2 = apply(
        &s1,
        &step(
            "g1",
            2,
            MovePhase::Update,
            "inner",
            at(start.x + 30., start.y + 20.),
            false,
        ),
    )
    .unwrap();
    let s3 = apply(&s2, &step("g1", 3, MovePhase::Finish, "inner", None, false)).unwrap();
    assert_eq!(
        s3.options.pins.get("inner"),
        Some(&Point {
            x: start.x + 30.,
            y: start.y + 20.
        })
    );
    assert!((s3.boxes["inner"].x - (start.x + 30.)).abs() < 1e-6);
    assert!((s3.boxes["inner"].y - (start.y + 20.)).abs() < 1e-6);
    assert!(
        (s3.boxes["a"].x - child.x).abs() < 1e-6 && (s3.boxes["a"].y - child.y).abs() < 1e-6,
        "the child keeps its place in its container"
    );
}

fn together(mut step: MoveStep, others: &[(&str, f64, f64)]) -> MoveStep {
    step.others = others
        .iter()
        .map(|(id, x, y)| (id.to_string(), Point { x: *x, y: *y }))
        .collect();
    step
}

#[test]
fn a_selection_moves_as_a_whole_and_a_cancel_restores_every_pin() {
    let s0 = fixture();
    let begin = together(
        step("m1", 1, MovePhase::Begin, "a", at(40., 60.), false),
        &[("b", 300., 60.)],
    );
    let s1 = apply(&s0, &begin).unwrap();
    assert_eq!(s1.options.pins.get("b"), Some(&Point { x: 300., y: 60. }));
    let update = together(
        step("m1", 2, MovePhase::Update, "a", at(70., 80.), false),
        &[("b", 330., 80.)],
    );
    let s2 = apply(&s1, &update).unwrap();
    for (id, x, y) in [("a", 70., 80.), ("b", 330., 80.)] {
        let placed = &s2.boxes[id];
        assert!(
            (placed.x - x).abs() < 1e-6 && (placed.y - y).abs() < 1e-6,
            "{id}: {placed:?}"
        );
    }
    // An update that leaves a node out of the set the gesture began with is refused.
    let missing = step("m1", 3, MovePhase::Update, "a", at(80., 80.), false);
    assert!(apply(&s2, &missing).unwrap_err().contains("began with"));
    // A cancel puts every node back to the pin it had before: none here.
    let s3 = apply(&s2, &step("m1", 3, MovePhase::Cancel, "a", None, false)).unwrap();
    assert!(s3.options.pins.is_empty(), "{:?}", s3.options.pins);
    // A finish with release frees every node of the selection; without release both pins stay.
    let kept = apply(&s2, &step("m1", 3, MovePhase::Finish, "a", None, false)).unwrap();
    assert_eq!(kept.options.pins.len(), 2);
    let freed = apply(&s2, &step("m1", 3, MovePhase::Finish, "a", None, true)).unwrap();
    assert!(freed.options.pins.is_empty());
}
