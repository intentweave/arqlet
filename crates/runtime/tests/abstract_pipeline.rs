use arqlet_model::{Graph, LayoutOptions, Snapshot};
use arqlet_runtime::{dispatch, import_graph_counted, import_graph_with, refine_snapshot_job};
use serde_json::{json, Value};
use std::collections::BTreeSet;

/// The pipeline before single pass became the default (ADR 0016), which these tests exercise.
fn legacy() -> LayoutOptions {
    LayoutOptions {
        single_pass_auto: false,
        single_pass: false,
        abstract_routing: false,
        ..LayoutOptions::default()
    }
}

fn graph() -> Graph {
    serde_json::from_str(include_str!("../../../examples/fixtures/small.json")).unwrap()
}

fn options() -> LayoutOptions {
    LayoutOptions {
        abstract_routing: true,
        ..legacy()
    }
}

fn check(snapshot: &Snapshot) {
    assert!(
        snapshot.geometry_issues.is_empty(),
        "{:?}",
        snapshot.geometry_issues
    );
    let edges: Vec<_> = snapshot
        .graph
        .edges
        .iter()
        .filter(|e| e.target.is_some())
        .collect();
    assert_eq!(snapshot.routes.len(), edges.len());
    assert_eq!(
        snapshot
            .routes
            .iter()
            .map(|r| &r.id)
            .collect::<BTreeSet<_>>()
            .len(),
        edges.len()
    );
    for edge in edges {
        let route = snapshot.routes.iter().find(|r| r.id == edge.id).unwrap();
        assert!(route.points.len() >= 2);
        assert!(route
            .points
            .iter()
            .all(|p| p.x.is_finite() && p.y.is_finite()));
        assert!(route
            .points
            .windows(2)
            .all(|p| (p[0].x - p[1].x).abs() < 1e-6 || (p[0].y - p[1].y).abs() < 1e-6));
        for (endpoint, node, point) in [
            ("source", &edge.source, route.points.first().unwrap()),
            (
                "target",
                edge.target.as_ref().unwrap(),
                route.points.last().unwrap(),
            ),
        ] {
            let port = snapshot
                .resolved_ports
                .iter()
                .find(|p| p.edge_id == edge.id && p.endpoint == endpoint)
                .unwrap();
            assert_eq!(&port.node_id, node);
            assert!(
                (port.position.x - point.x).abs() < 1e-6
                    && (port.position.y - point.y).abs() < 1e-6
            );
        }
    }
    arqlet_scene::build_scene(snapshot).unwrap();
    arqlet_layout::published_evaluation(snapshot).unwrap();
}

fn check_capacity_seed_coverage(snapshot: &Snapshot) {
    let channels: Vec<_> = snapshot
        .layout_diagnostics
        .iter()
        .filter(|d| d.starts_with("Abstract capacity channel:"))
        .collect();
    assert!(!channels.is_empty());
    assert!(
        channels.iter().all(|d| !d.contains("seed=unavailable")),
        "{:?}",
        channels
            .iter()
            .filter(|d| d.contains("seed=unavailable"))
            .collect::<Vec<_>>()
    );
}

#[test]
fn import_uses_abstract_routing_without_constructor_search_on_both_axes() {
    for direction in ["lr", "tb"] {
        let mut value = serde_json::to_value(graph()).unwrap();
        value["defaultDirection"] = json!(direction);
        let graph: Graph = serde_json::from_value(value).unwrap();
        let canonical = serde_json::to_value(&graph).unwrap();
        let (snapshot, work) = import_graph_counted(graph, 1000, &options()).unwrap();
        check(&snapshot);
        check_capacity_seed_coverage(&snapshot);
        assert_eq!(
            serde_json::to_value(&snapshot.graph).unwrap()["nodes"],
            canonical["nodes"]
        );
        assert_eq!(snapshot.omitted_unresolved_edges, 1);
        assert!(work.abstract_routing_passes > 0);
        assert_eq!(work.construction.searches, 0);
        assert_eq!(work.construction.transitions, 0);
        assert_eq!(work.reused_preparations, 0);
        assert!(snapshot
            .layout_diagnostics
            .iter()
            .any(|d| d.starts_with("Abstract routing:")));
        let capacity_diagnostics: Vec<_> = snapshot
            .layout_diagnostics
            .iter()
            .filter(|d| d.starts_with("Abstract capacity channel:"))
            .collect();
        assert!(!capacity_diagnostics.is_empty());
        assert!(capacity_diagnostics
            .iter()
            .all(|d| !d.contains("NaN") && !d.contains("inf")));
        assert!(capacity_diagnostics.iter().any(|d| {
            d.contains("seeded_width=")
                && d.contains("initial_available=")
                && d.contains("physical_required=")
                && d.contains("late_shift=")
                && d.contains("physical_missing=")
        }));
        let resumed: Snapshot =
            serde_json::from_str(&serde_json::to_string(&snapshot).unwrap()).unwrap();
        let added = arqlet_runtime::add_node(&resumed, resumed.revision, "b", "Added").unwrap();
        check(&added);
        assert_eq!(added.revision, resumed.revision + 1);
    }
}

#[test]
fn refinement_keeps_common_incumbent_and_portfolio_and_is_thread_identical() {
    let base = import_graph_with(graph(), 1000, &legacy()).unwrap();
    let mut input = base.clone();
    input.options.abstract_routing = true;
    let key = arqlet_layout::published_key(&input).unwrap();
    let (one, report) = refine_snapshot_job(
        "one",
        &input,
        input.revision,
        input.layout_version,
        1,
        None,
        None,
    )
    .unwrap();
    check(&one);
    assert!(arqlet_layout::published_key(&one).unwrap() <= key);
    assert!(report.counters.abstract_routing_passes > 0);
    assert_eq!(report.counters.construction.searches, 0);
    assert_eq!(
        report
            .completed
            .iter()
            .filter(|id| id.strategy == "ordering")
            .count(),
        4
    );
    let ordering_work: Vec<_> = report
        .candidate_work
        .iter()
        .filter(|work| work.candidate.strategy == "ordering")
        .collect();
    assert_eq!(ordering_work.len(), 4);
    for candidate in ordering_work {
        assert!(candidate.counters.abstract_routing_passes > 0);
        assert_eq!(
            candidate
                .counters
                .placement_pass_reasons
                .values()
                .sum::<usize>(),
            candidate.counters.placement_passes,
            "{:?}: {:?}",
            candidate.candidate,
            candidate.counters.placement_pass_reasons
        );
        assert!(!candidate
            .counters
            .placement_pass_reasons
            .contains_key("unclassified"));
    }
    let (four, other) = refine_snapshot_job(
        "four",
        &input,
        input.revision,
        input.layout_version,
        4,
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&one).unwrap(),
        serde_json::to_value(&four).unwrap()
    );
    assert_eq!(report.completed, other.completed);
    assert_eq!(one.revision, input.revision);
    assert_eq!(one.base_revision, Some(input.revision));
    assert_eq!(one.layout_version, input.layout_version + 1);
    assert!(
        refine_snapshot_job("", &one, one.revision, input.layout_version, 1, None, None)
            .unwrap_err()
            .contains("Stale")
    );
}

#[test]
fn unsupported_abstract_requests_are_explicit_errors() {
    for patch in [
        json!({"layeredConstruction":false}),
        json!({"hierarchicalOrdering":false}),
        json!({"globalLayering":true}),
        json!({"visibleNodeBudget":2}),
        json!({"routingEngine":"channel"}),
        json!({"bundling":"complete"}),
        json!({"edgeRouting":"straight"}),
        json!({"pins":{"a":{"x":0,"y":0}}}),
        json!({"placementGrid":10}),
        json!({"maxExtent":100}),
        json!({"containerSizing":{"root":{"width":{"fixed":100}}}}),
    ] {
        let mut patch = patch;
        patch["abstractRouting"] = json!(true);
        // Explicitly, without the automatic choice (the default, which falls back instead, ADR 0016).
        patch["singlePassAuto"] = json!(false);
        patch["singlePass"] = json!(false);
        let result: Value = serde_json::from_str(&dispatch(
            &json!({"op":"import","graph":graph(),"options":patch,"limit":1000}).to_string(),
        ))
        .unwrap();
        assert_eq!(result["ok"], false, "{patch}: {result}");
        assert!(result["error"].as_str().is_some_and(|s| !s.is_empty()));
    }
    for direction in ["rl", "bt"] {
        let mut graph = serde_json::to_value(graph()).unwrap();
        graph["defaultDirection"] = json!(direction);
        let result: Value = serde_json::from_str(&dispatch(
            &json!({"op":"import","graph":graph,"options":{"abstractRouting":true}}).to_string(),
        ))
        .unwrap();
        assert_eq!(result["ok"], false, "{result}");
    }
}

#[test]
fn default_option_is_omitted_and_cancellation_does_not_publish_partial_routes() {
    assert!(serde_json::to_value(LayoutOptions::default())
        .unwrap()
        .get("abstractRouting")
        .is_none());
    let input = import_graph_with(graph(), 1000, &options()).unwrap();
    let (candidate, _) = arqlet_layout::candidate_from(
        &input.graph,
        1,
        &input.options,
        &|| true,
        &input.layered_plans,
    )
    .unwrap();
    assert!(candidate.is_none());
    let calls = std::cell::Cell::new(0);
    let (candidate, _) = arqlet_layout::candidate_from(
        &input.graph,
        1,
        &input.options,
        &|| {
            calls.set(calls.get() + 1);
            calls.get() >= 2
        },
        &input.layered_plans,
    )
    .unwrap();
    assert!(calls.get() >= 2);
    assert!(candidate.is_none());
    let (result, report) = refine_snapshot_job(
        "",
        &input,
        input.revision,
        input.layout_version,
        1,
        Some(&|| true),
        None,
    )
    .unwrap();
    assert_eq!(report.counters.abstract_routing_passes, 0);
    assert_eq!(
        serde_json::to_value(&result.routes).unwrap(),
        serde_json::to_value(&input.routes).unwrap()
    );
    let mut invalid = input;
    invalid.options.routing_engine = arqlet_model::RoutingEngine::Channel;
    assert!(arqlet_runtime::refine_snapshot_limited(
        &invalid,
        invalid.revision,
        invalid.layout_version,
        1,
        None,
        Some(0),
    )
    .unwrap_err()
    .contains("legacy router"));
}

#[test]
fn complete_abstract_pipeline_covers_frozen_graph_families() {
    for text in [
        include_str!("../../../examples/fixtures/phase-width-feedback.json"),
        include_str!("../../../benchmarks/datasets/workflow-dag.json"),
        include_str!("../../../benchmarks/datasets/org-chart.json"),
        include_str!("../../../benchmarks/datasets/petri-net.json"),
    ] {
        let mut graph: Graph = serde_json::from_str(text).unwrap();
        graph.container_directions.clear();
        graph.container_strategies.clear();
        graph.default_strategy =
            arqlet_model::StrategyConstraint::Fixed(arqlet_model::LayoutStrategy::None);
        let (snapshot, work) = import_graph_counted(graph, 1000, &options()).unwrap();
        check(&snapshot);
        assert!(work.abstract_routing_passes > 0);
        assert_eq!(work.construction.transitions, 0);
        let (result, report) = refine_snapshot_job(
            "",
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            1,
            None,
            None,
        )
        .unwrap();
        check(&result);
        check_capacity_seed_coverage(&result);
        assert!(report.counters.abstract_routing_passes > 0);
        assert_eq!(report.counters.construction.transitions, 0);
        assert!(
            arqlet_layout::published_key(&result).unwrap()
                <= arqlet_layout::published_key(&snapshot).unwrap()
        );
    }
}

#[test]
fn explicit_ports_labels_and_configuration_resume_through_dispatch() {
    let input = import_graph_with(graph(), 1000, &options()).unwrap();
    let with_port = arqlet_runtime::port_edit::set_port(
        &input,
        input.revision,
        input.layout_version,
        "ab",
        "source",
        Some((arqlet_model::PortSide::Top, 0.25)),
    )
    .unwrap();
    check(&with_port);
    let labelled = arqlet_runtime::configure_snapshot(
        &with_port,
        with_port.revision,
        with_port.layout_version,
        &json!({"edgeLabels":{"ab":{"text":"calls","width":40,"height":18}}}),
    )
    .unwrap();
    check(&labelled);
    let explicit = labelled
        .resolved_ports
        .iter()
        .find(|p| p.edge_id == "ab" && p.endpoint == "source")
        .unwrap();
    assert!(explicit.explicit);
    assert_eq!(explicit.side, arqlet_model::PortSide::Top);
    assert!(labelled.labels.iter().any(|label| label.text == "calls"));
    let request = json!({"op":"refine","snapshot":labelled,"expectedRevision":labelled.revision,"expectedLayoutVersion":labelled.layout_version,"threads":4});
    let result: Value = serde_json::from_str(&dispatch(&request.to_string())).unwrap();
    assert_eq!(result["ok"], true, "{result}");
    check(&serde_json::from_value(result["snapshot"].clone()).unwrap());
}

#[test]
fn existing_routing_jobs_keep_their_acceptance_rules_with_the_opt_in() {
    let mut graph: Graph =
        serde_json::from_str(include_str!("../../../benchmarks/datasets/org-chart.json")).unwrap();
    graph.container_directions.clear();
    let mut baseline = import_graph_with(graph, 1000, &legacy()).unwrap();
    baseline.options.candidates.clear();
    let mut alternative = baseline.clone();
    alternative.options.abstract_routing = true;
    let refine = |input: &Snapshot| {
        arqlet_runtime::refine_snapshot_limited(
            input,
            input.revision,
            input.layout_version,
            1,
            None,
            None,
        )
        .unwrap()
    };
    let (a, b) = (refine(&baseline), refine(&alternative));
    assert_eq!(
        serde_json::to_value((&a.boxes, &a.routes)).unwrap(),
        serde_json::to_value((&b.boxes, &b.routes)).unwrap()
    );
    assert_eq!(
        arqlet_layout::published_key(&a).unwrap(),
        arqlet_layout::published_key(&b).unwrap()
    );
}
