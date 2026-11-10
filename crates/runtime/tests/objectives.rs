//! The published key and the quality comparison agree on the projected objectives (`ProjectedCrossings`, `EndToEnd`).
//! Before this, the key reported 0 for both, so the placement search could not see them.

use std::collections::BTreeSet;

use arqlet_model::{Graph, LayoutOptions, Objective};

const CONTAINER_GRAPH: &str =
    include_str!("../../../examples/fixtures/reference/container-boundary.graph.json");

#[test]
fn the_published_key_matches_the_quality_comparison_for_the_projected_objectives() {
    let graph: Graph = serde_json::from_str(CONTAINER_GRAPH).unwrap();
    let objectives = vec![Objective::ProjectedCrossings, Objective::EndToEnd];
    let options = LayoutOptions {
        objectives: objectives.clone(),
        ..LayoutOptions::default()
    };
    let snapshot = arqlet_layout::compute(&graph, None, &BTreeSet::new(), &options).unwrap();
    let key = arqlet_layout::published_key(&snapshot).unwrap();
    let quality = arqlet_layout::quality_with(&snapshot, &snapshot, &objectives);
    assert!(
        quality.end_to_end > 0.0,
        "the fixture has edges between separated nodes"
    );
    assert_eq!(
        key[0], quality.projected_crossings as i64,
        "projected crossings"
    );
    assert_eq!(
        key[1],
        (quality.end_to_end * 100.).round() as i64,
        "end to end, in hundredths"
    );
}
