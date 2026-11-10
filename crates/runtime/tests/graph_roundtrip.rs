//! A graph that sets a strategy on a container against its default keeps it through the snapshot's JSON round trip: the
//! snapshot goes to the page and back with every operation, and a container's explicit `none` must not read as inherited.

use arqlet_model::{Graph, Snapshot};
use arqlet_runtime::import_graph;

const MIXED: &str = include_str!("../../../examples/fixtures/mixed-layouts.json");

#[test]
fn an_explicit_none_strategy_survives_the_round_trip() {
    let graph: Graph = serde_json::from_str(MIXED).expect("the fixture is a graph");
    assert_ne!(graph.area_mode(Some("pipeline")), graph.area_mode(None));
    let again: Graph = serde_json::from_str(&serde_json::to_string(&graph).unwrap()).unwrap();
    assert_eq!(
        again.area_mode(Some("pipeline")),
        graph.area_mode(Some("pipeline")),
        "the graph lost the strategy of pipeline: {}",
        serde_json::to_string(&graph).unwrap()
    );
    let snapshot = import_graph(graph, 100).expect("imports");
    let text = serde_json::to_string(&snapshot).unwrap();
    let back: Snapshot = serde_json::from_str(&text).unwrap();
    let source = back.source_graph.as_ref().unwrap_or(&back.graph);
    assert_ne!(
        source.area_mode(Some("pipeline")),
        source.area_mode(None),
        "the snapshot lost the strategy of pipeline"
    );
}
