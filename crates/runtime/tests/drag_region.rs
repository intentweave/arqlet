//! P2.7: a force-directed drag of one organic child leaves children outside its two-hop region where they were.
use arqlet_model::{MovePhase, MoveStep, Point};
use arqlet_runtime::{configure_snapshot, import_view, move_node};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn a_force_directed_drag_keeps_children_outside_its_region_in_place() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/organic-50.json"
    ))
    .unwrap();
    let imported = import_view(&text, 1000).unwrap();
    let snapshot = configure_snapshot(
        &imported,
        imported.revision,
        imported.layout_version,
        &json!({"forceDirected": true}),
    )
    .unwrap();
    let moved = snapshot
        .graph
        .nodes
        .iter()
        .find(|n| n.parent_id.is_none())
        .unwrap()
        .id
        .clone();
    let start = snapshot.boxes[&moved].clone();
    let step = |seq, phase, at: Option<(f64, f64)>| MoveStep {
        gesture_id: "drag".into(),
        seq,
        phase,
        node: moved.clone(),
        position: at.map(|(x, y)| Point { x, y }),
        release: false,
        others: Default::default(),
    };
    let began = move_node(
        &snapshot,
        snapshot.revision,
        snapshot.layout_version,
        &step(1, MovePhase::Begin, Some((start.x, start.y))),
    )
    .unwrap();
    let after = move_node(
        &began,
        began.revision,
        began.layout_version,
        &step(2, MovePhase::Update, Some((start.x + 60., start.y))),
    )
    .unwrap();

    // Two-hop region of the moved child over the graph's edges.
    let mut adjacent: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for e in &snapshot.graph.edges {
        if let Some(t) = e.target.as_deref() {
            adjacent.entry(e.source.as_str()).or_default().insert(t);
            adjacent.entry(t).or_default().insert(e.source.as_str());
        }
    }
    let mut region: BTreeSet<&str> = BTreeSet::from([moved.as_str()]);
    for _ in 0..2 {
        let next: Vec<&str> = region
            .iter()
            .flat_map(|n| adjacent.get(n).into_iter().flatten().copied())
            .collect();
        region.extend(next);
    }
    let mut checked = 0;
    for node in &snapshot.graph.nodes {
        if region.contains(node.id.as_str()) || node.parent_id.is_some() {
            continue;
        }
        checked += 1;
        let (a, b) = (&after.boxes[&node.id], &snapshot.boxes[&node.id]);
        assert!(
            (a.x, a.y, a.width, a.height) == (b.x, b.y, b.width, b.height),
            "{} is outside the drag region and moved",
            node.id
        );
    }
    assert!(
        checked > 0,
        "the region covers the whole area; the test checks nothing"
    );
}
