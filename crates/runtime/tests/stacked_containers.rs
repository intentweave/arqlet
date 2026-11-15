//! `sharedBorderRanks` and `alignStacked` (on by default): containers across their parent's flow lose the rank that held
//! only a long-edge dummy, and a stack of containers (the subnets of a zone) stands so that its edges run straight.
use arqlet_model::Snapshot;
use arqlet_runtime::dispatch;
use serde_json::{json, Value};

fn import(graph: &Value, options: Value) -> Snapshot {
    let r: Value = serde_json::from_str(&dispatch(
        &json!({"op": "import", "graph": graph, "limit": 150, "options": options}).to_string(),
    ))
    .unwrap();
    assert_eq!(r["ok"], true, "{r}");
    serde_json::from_value(r["snapshot"].clone()).unwrap()
}

#[test]
fn the_subnets_of_a_zone_stand_flush_and_the_drawing_is_smaller() {
    let graph: Value = serde_json::from_str(include_str!(
        "../../../benchmarks/datasets/cloud-architecture.json"
    ))
    .unwrap();
    let before = import(
        &graph,
        json!({"sharedBorderRanks": false, "alignStacked": false}),
    );
    let after = import(&graph, json!({}));
    let (a, b) = (
        arqlet_layout::published_metrics(&before).unwrap(),
        arqlet_layout::published_metrics(&after).unwrap(),
    );
    // 114 bends to 108 and 4637k to 4308k at 22 crossings (measured 2026-11-15).
    assert!(
        b.crossings <= a.crossings && b.bends < a.bends,
        "{b:?} against {a:?}"
    );
    assert!(b.area < a.area * 0.95, "{} against {}", b.area, a.area);
    assert_eq!(b.line_node_hits, 0);
    assert!(
        after.geometry_issues.is_empty(),
        "{:?}",
        after.geometry_issues
    );
    // web-a → app-a runs straight: the public and the private subnet of zone a start at the same place across the zone's flow.
    let x = |id: &str| after.boxes[id].x;
    assert!(
        (x("public-a") - x("private-a")).abs() < 0.5,
        "{} against {}",
        x("public-a"),
        x("private-a")
    );
    // No rank of one dummy before app-a: the private subnet narrowed.
    assert!(after.boxes["private-a"].width < before.boxes["private-a"].width);
    // On by default, written only when off.
    assert!(after.options.shared_border_ranks && after.options.align_stacked);
    let text = serde_json::to_string(&after.options).unwrap();
    assert!(!text.contains("sharedBorderRanks") && !text.contains("alignStacked"));
    assert!(serde_json::to_string(&before.options)
        .unwrap()
        .contains("\"alignStacked\":false"));
}
