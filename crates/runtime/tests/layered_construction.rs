//! The experimental layered construction (plan section 4a): constructed edges of the import keep their constructed routes,
//! and the general routing works only on the other edges. Each test checks that a constructed route appears in the
//! snapshot exactly as it was constructed, and that the drawing stays valid.
use arqlet_model::{Graph, LayoutOptions, Snapshot};
use arqlet_runtime::import_graph_with;
use serde_json::json;

/// The pipeline before single pass became the default (ADR 0016), which these tests exercise.
fn legacy() -> LayoutOptions {
    LayoutOptions {
        single_pass_auto: false,
        single_pass: false,
        abstract_routing: false,
        ..LayoutOptions::default()
    }
}

fn options(on: bool) -> LayoutOptions {
    LayoutOptions {
        layered_construction: on,
        hierarchical_ordering: false,
        ..legacy()
    }
}

fn import(graph: Graph, on: bool) -> Snapshot {
    import_graph_with(graph, 1000, &options(on)).unwrap()
}

/// The constructed routes of an imported snapshot, computed from its own boxes as the import computes them.
fn constructed_of(snapshot: &Snapshot) -> Vec<arqlet_model::Route> {
    arqlet_layout::layered_constructed_routes(&snapshot.graph, &snapshot.boxes, &snapshot.options)
}

#[test]
fn channel_width_planning_skips_discarded_routing_without_changing_the_frozen_view() {
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../examples/fixtures/phase-width-feedback.json"
    ))
    .unwrap();
    let base = import_graph_with(graph, 1000, &legacy()).unwrap();
    let shadow = |snapshot: &Snapshot| {
        let plans = snapshot
            .layered_plans
            .iter()
            .map(|(area, plan)| {
                (
                    area.clone(),
                    arqlet_layout::layered_plan::AreaPlan::from_stored(plan).unwrap(),
                )
            })
            .collect();
        let before = serde_json::to_vec(snapshot).unwrap();
        let plan = arqlet_layout::abstract_routing::build(&snapshot.graph, &plans).unwrap();
        assert_eq!(plan.summary.edges, snapshot.graph.edges.len());
        assert_eq!(
            plan.summary.edges,
            plan.summary.planned_edges + plan.summary.unsupported_edges
        );
        assert!(plan.summary.planned_edges > 0);
        assert!(!plan.safe_to_prune);
        use arqlet_layout::constructive::abstract_realization::{self, growth};
        let boxes = arqlet_layout::absolute_all(&snapshot.graph, &snapshot.boxes);
        let grown = growth::widen(
            &snapshot.graph,
            &boxes,
            &snapshot.resolved_ports,
            &plans,
            &snapshot.options,
        )
        .unwrap();
        for (area, p) in &plans {
            assert_eq!(grown.plans[area].ranks, p.ranks);
            assert_eq!(grown.plans[area].chains, p.chains);
        }
        for (id, rect) in &boxes {
            assert_eq!(grown.boxes[id].y, rect.y, "keep every cross coordinate");
            assert_eq!(
                grown.boxes[id].height, rect.height,
                "keep every cross extent"
            );
            assert!(grown.boxes[id].width >= rect.width);
        }
        let physical = abstract_realization::realize(
            &snapshot.graph,
            &grown.boxes,
            &grown.ports,
            &grown.plans,
            arqlet_layout::constructive_spacing(&snapshot.options),
        )
        .unwrap();
        assert_eq!(physical.routes.len(), 263);
        assert_eq!(
            physical.routes.len() + physical.delegated.len(),
            snapshot.graph.edges.len()
        );
        assert!(physical.channels.iter().all(|c| c.missing <= 1e-6));
        assert_eq!(physical.work.searches, 0);
        assert_eq!(physical.work.states, 0);
        assert_eq!(physical.work.transitions, 0);
        let coordinated = abstract_realization::realize_coordinated(
            &snapshot.graph,
            &grown.boxes,
            &grown.ports,
            &grown.plans,
            arqlet_layout::constructive_spacing(&snapshot.options),
        )
        .unwrap();
        assert_eq!(coordinated.routes.len(), 308);
        assert_eq!(
            coordinated.routes.len() + coordinated.delegated.len(),
            snapshot.graph.edges.len()
        );
        assert_eq!(coordinated.continuity.as_ref().unwrap().rejected_edges, 41);
        assert!(
            coordinated
                .continuity
                .as_ref()
                .unwrap()
                .cross_area_constraints
                > 0
        );
        assert!(coordinated.channels.iter().all(|c| c.missing <= 1e-6));
        assert_eq!(coordinated.work.searches, 0);
        assert_eq!(coordinated.work.states, 0);
        assert_eq!(coordinated.work.transitions, 0);
        let joint = abstract_realization::realize_joint(
            &snapshot.graph,
            &grown.boxes,
            &grown.ports,
            &grown.plans,
            arqlet_layout::constructive_spacing(&snapshot.options),
        )
        .unwrap();
        assert_eq!(joint.routes.len(), 327);
        assert_eq!(
            joint.routes.len() + joint.delegated.len(),
            snapshot.graph.edges.len()
        );
        assert_eq!(joint.continuity.as_ref().unwrap().rejected_edges, 20);
        assert_eq!(joint.joint.as_ref().unwrap().temporal_constraints, 29);
        assert_eq!(joint.joint.as_ref().unwrap().blocked_edges, 0);
        assert_eq!(joint.joint.as_ref().unwrap().capacity_edges, 2);
        assert!(joint.channels.iter().all(|c| c.missing <= 1e-6));
        assert_eq!(joint.work.searches, 0);
        assert_eq!(joint.work.states, 0);
        assert_eq!(joint.work.transitions, 0);
        let chains = abstract_realization::realize_chains(
            &snapshot.graph,
            &grown.boxes,
            &grown.ports,
            &grown.plans,
            arqlet_layout::constructive_spacing(&snapshot.options),
        )
        .unwrap();
        assert_eq!(chains.routes.len(), 347);
        assert_eq!(
            chains.routes.len() + chains.delegated.len(),
            snapshot.graph.edges.len()
        );
        assert_eq!(chains.work.searches, 0);
        assert_eq!(chains.work.states, 0);
        assert_eq!(chains.work.transitions, 0);
        assert_eq!(chains.continuity.as_ref().unwrap().rejected_edges, 0);
        assert_eq!(chains.joint.as_ref().unwrap().capacity_edges, 2);
        assert!(chains.channels.iter().all(|c| c.missing <= 1e-6));
        let trace = chains.chains.as_ref().unwrap();
        assert_eq!(trace.planning_passes, 2);
        assert_eq!(trace.probe_joint, *joint.joint.as_ref().unwrap());
        assert_eq!(trace.probe_continuity, *joint.continuity.as_ref().unwrap());
        assert_eq!(trace.orders.len(), 20);
        assert_eq!(trace.uncut_witnesses, 0);
        assert_eq!(trace.cuts.len(), 20);
        use abstract_realization::completion::{run, Method};
        let completed = run(snapshot, Method::Chains).unwrap();
        let repaired = run(snapshot, Method::ChainsFixedTracks).unwrap();
        assert_eq!(completed.routes.len(), snapshot.graph.edges.len());
        assert_eq!(before, serde_json::to_vec(snapshot).unwrap());
        (
            plan,
            serde_json::to_vec(&physical.routes).unwrap(),
            physical.delegated,
            physical.channels,
            serde_json::to_vec(&grown.boxes).unwrap(),
            serde_json::to_vec(&coordinated.routes).unwrap(),
            coordinated.delegated,
            coordinated.channels,
            coordinated.continuity,
            (
                serde_json::to_vec(&joint.routes).unwrap(),
                joint.delegated,
                joint.channels,
                joint.continuity,
                joint.joint,
            ),
            (
                serde_json::to_vec(&chains.routes).unwrap(),
                chains.delegated,
                chains.channels,
                chains.continuity,
                chains.joint,
                chains.chains,
            ),
            (
                serde_json::to_vec(&completed.scene).unwrap(),
                completed.key,
                completed.track_conflicts,
                serde_json::to_vec(&repaired.scene).unwrap(),
                repaired.key,
                repaired.track_conflicts,
            ),
        )
    };
    shadow(&base);
    let mut results = Vec::new();
    let mut shadows = Vec::new();
    for threads in [1, 4] {
        let (snapshot, report) = arqlet_runtime::refine_snapshot_job(
            "",
            &base,
            base.revision,
            base.layout_version,
            threads,
            None,
            None,
        )
        .unwrap();
        assert_eq!(report.candidate_work.len(), 5);
        assert_eq!(report.counters.deferred_routing_passes, 3);
        assert_eq!(report.counters.placement_passes, 26);
        assert_eq!(report.counters.layout_passes, 4);
        assert!(report.counters.reused_orderings > 0);
        assert!(report.counters.reused_structural_plans > 0);
        assert!(report.counters.reused_region_realizations > 0);
        assert!(report.counters.reused_cross_plans > 0);
        assert!(report
            .candidate_work
            .iter()
            .any(|c| c.counters.reused_cross_plans > 0));
        assert!(report
            .candidate_work
            .iter()
            .any(|c| c.counters.reused_structural_plans > 0));
        assert!(report
            .candidate_work
            .iter()
            .any(|c| c.counters.reused_region_realizations > 0));
        assert!(report.counters.construction.searches > 0);
        assert!(report.counters.construction.states > 0);
        assert!(report.counters.construction.transitions > 0);
        #[cfg(feature = "benchmark-timings")]
        {
            let c = &report.counters.construction;
            assert_eq!(c.search_details.len(), c.searches);
            assert_eq!(
                c.search_details.iter().filter(|s| s.failed).count(),
                c.failed_searches
            );
            assert_eq!(
                c.search_details.iter().map(|s| s.states).sum::<usize>(),
                c.states
            );
            assert_eq!(
                c.search_details
                    .iter()
                    .map(|s| s.transitions)
                    .sum::<usize>(),
                c.transitions
            );
            assert_eq!(
                c.planned_failures.len(),
                c.planned_obstacle_failures + c.planned_track_failures
            );
            assert!(c.search_details.iter().all(|s| snapshot
                .graph
                .edges
                .iter()
                .any(|e| e.id == s.edge)));
        }
        // Recorded before phase separation, on this checked-in bounded view (refinement-audit-20261104.jsonl).
        let geometry = serde_json::to_vec(&(&snapshot.boxes, &snapshot.routes)).unwrap();
        let fingerprint = geometry
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
            });
        assert_eq!(fingerprint, 0x1940_9ef4_dbf0_0e51);
        assert_eq!(snapshot.routes.len(), base.routes.len());
        shadows.push(shadow(&snapshot));
        results.push(snapshot);
    }
    assert_eq!(
        serde_json::to_vec(&results[0]).unwrap(),
        serde_json::to_vec(&results[1]).unwrap()
    );
    assert_eq!(shadows[0], shadows[1]);
}

fn assert_constructed_routes_kept(snapshot: &Snapshot) -> usize {
    let constructed = constructed_of(snapshot);
    // An edge drawn straight by the graph's routing style keeps that style instead.
    for route in constructed
        .iter()
        .filter(|r| !snapshot.straight_edges.contains(&r.id))
    {
        let kept = snapshot
            .routes
            .iter()
            .find(|r| r.id == route.id)
            .unwrap_or_else(|| panic!("{} is missing from the snapshot", route.id));
        // Equal up to rounding: the test derives the absolute boxes on another path than the import.
        let same = kept.points.len() == route.points.len()
            && kept
                .points
                .iter()
                .zip(&route.points)
                .all(|(a, b)| (a.x - b.x).abs() < 1e-6 && (a.y - b.y).abs() < 1e-6);
        assert!(
            same,
            "{} was changed by the general routing: {:?} against {:?}",
            route.id, kept.points, route.points
        );
    }
    constructed.len()
}

#[test]
fn completed_chain_trial_keeps_fixed_routes_and_reports_full_quality() {
    use arqlet_layout::constructive::abstract_realization::completion::{run, Method};
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../examples/fixtures/phase-width-feedback.json"
    ))
    .unwrap();
    let base = import_graph_with(graph, 1000, &legacy()).unwrap();
    let before = serde_json::to_vec(&base).unwrap();
    let chains = run(&base, Method::Chains).unwrap();
    let existing = run(&base, Method::Existing).unwrap();
    let repaired = run(&base, Method::ChainsFixedTracks).unwrap();
    assert_eq!(
        serde_json::to_value(&repaired.fixed).unwrap(),
        serde_json::to_value(&chains.fixed).unwrap()
    );
    assert_eq!(repaired.delegated, chains.delegated);
    assert!(repaired.track_conflicts.is_empty());
    assert_eq!(repaired.hard_violations, 0);
    assert!(repaired.geometry_issues.is_empty());
    assert!(repaired.routing_issues.is_empty());
    assert_eq!(
        serde_json::to_value(&repaired.scene.ports).unwrap(),
        serde_json::to_value(&chains.scene.ports).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&repaired.scene.nodes).unwrap(),
        serde_json::to_value(&chains.scene.nodes).unwrap()
    );
    let work = repaired.track_repair.as_ref().unwrap();
    assert_eq!(
        (work.initial_pairs, work.remaining_pairs, work.moves),
        (4, 0, 2)
    );
    let changed: Vec<_> = repaired
        .routes
        .iter()
        .zip(&chains.routes)
        .filter(|(a, b)| a.points != b.points)
        .map(|(r, _)| r.id.as_str())
        .collect();
    assert_eq!(changed, vec!["e449", "e471"]);
    assert_eq!(repaired.metrics.bends, 2422);
    assert_eq!(repaired.metrics.crossings, 4437);
    assert_eq!(chains.fixed.len(), 347);
    assert_eq!(chains.delegated.len(), 11);
    assert_eq!(chains.search.construction.searches, 0);
    assert!(chains.search.legacy_candidates > 0);
    assert!(existing.search.construction.searches > 0);
    assert_eq!(chains.metrics.edges, 358);
    assert_eq!(chains.hard_violations, 0);
    assert!(chains.geometry_issues.is_empty());
    assert!(chains.routing_issues.is_empty());
    assert_eq!(
        chains.track_conflicts,
        [
            ("e302", "e449"),
            ("e449", "e655"),
            ("e449", "e661"),
            ("e471", "e486")
        ]
        .map(|(a, b)| (a.to_string(), b.to_string()))
    );
    // Both constructors see exactly the same grown node geometry.
    assert_eq!(
        serde_json::to_vec(&chains.scene.nodes).unwrap(),
        serde_json::to_vec(&existing.scene.nodes).unwrap()
    );
    assert!(
        existing.hard_violations > 0,
        "do not hide baseline quality failures"
    );
    for completed in [&chains, &existing, &repaired] {
        assert_eq!(completed.routes.len(), base.graph.edges.len());
        for fixed in &completed.fixed {
            assert_eq!(
                serde_json::to_value(completed.routes.iter().find(|r| r.id == fixed.id).unwrap())
                    .unwrap(),
                serde_json::to_value(fixed).unwrap()
            );
        }
        for edge in &base.graph.edges {
            let route = completed.routes.iter().find(|r| r.id == edge.id).unwrap();
            for (endpoint, node_id, position) in [
                (
                    "source",
                    edge.source.as_str(),
                    route.points.first().unwrap(),
                ),
                (
                    "target",
                    edge.target.as_deref().unwrap(),
                    route.points.last().unwrap(),
                ),
            ] {
                let port = completed
                    .scene
                    .ports
                    .iter()
                    .find(|p| p.edge_id == edge.id && p.endpoint == endpoint)
                    .unwrap();
                assert_eq!(port.node_id, node_id);
                assert_eq!(&port.position, position);
            }
        }
    }
    let again = run(&base, Method::Chains).unwrap();
    assert_eq!(
        serde_json::to_vec(&chains.scene).unwrap(),
        serde_json::to_vec(&again.scene).unwrap()
    );
    assert_eq!(chains.metrics, again.metrics);
    assert_eq!(chains.track_conflicts, again.track_conflicts);
    assert_eq!(before, serde_json::to_vec(&base).unwrap());
}

#[test]
fn completed_trials_include_cycles_parallel_edges_loops_and_labels_in_both_axes() {
    use arqlet_layout::constructive::abstract_realization::completion::{run, Method};
    for direction in ["lr", "tb"] {
        let graph: Graph = serde_json::from_value(json!({
            "defaultDirection": direction,
            "nodes": [
                {"id":"a","kind":"n","label":"A"},
                {"id":"b","kind":"n","label":"B"},
                {"id":"c","kind":"n","label":"C"}
            ],
            "edges": [
                {"id":"ab","kind":"e","source":"a","target":"b"},
                {"id":"parallel","kind":"e","source":"a","target":"b"},
                {"id":"bc","kind":"e","source":"b","target":"c"},
                {"id":"ca","kind":"e","source":"c","target":"a"},
                {"id":"loop","kind":"e","source":"b","target":"b"}
            ]
        }))
        .unwrap();
        let options: LayoutOptions = serde_json::from_value(json!({
            "edgeLabels": {"ab": {"text":"calls","width":48,"height":22}}
        }))
        .unwrap();
        let base = import_graph_with(graph, 1000, &options).unwrap();
        for method in [
            Method::Existing,
            Method::Chains,
            Method::ChainsFixedTracks,
            Method::ChainsAligned,
        ] {
            let result = run(&base, method).unwrap();
            assert_eq!(result.routes.len(), 5);
            assert_eq!(result.scene.routes.len(), 5);
            assert_eq!(result.fixed.len() + result.delegated.len(), 5);
            if matches!(
                method,
                Method::Chains | Method::ChainsFixedTracks | Method::ChainsAligned
            ) {
                assert!(result.delegated.iter().any(|(id, _)| id == "loop"));
            }
            assert_eq!(result.scene.labels.len(), 1);
            for edge in &base.graph.edges {
                let route = result
                    .scene
                    .routes
                    .iter()
                    .find(|r| r.id == edge.id)
                    .unwrap();
                assert_eq!(route.source, edge.source);
                assert_eq!(route.target, *edge.target.as_ref().unwrap());
            }
        }
    }
}

#[test]
fn completed_trials_reject_unsupported_or_incomplete_inputs_explicitly() {
    use arqlet_layout::constructive::abstract_realization::completion::{run, Method};
    let base = import_graph_with(
        graph(
            json!([{"id":"a","kind":"n","label":"A"},{"id":"b","kind":"n","label":"B"}]),
            json!([{"id":"ab","kind":"e","source":"a","target":"b"}]),
        ),
        1000,
        &legacy(),
    )
    .unwrap();
    let mut invalid = base.clone();
    invalid.boxes.remove("a");
    assert!(run(&invalid, Method::Chains)
        .err()
        .unwrap()
        .contains("one box per node"));
    invalid = base.clone();
    invalid.options.routing_engine = arqlet_model::RoutingEngine::Channel;
    assert!(run(&invalid, Method::Chains)
        .err()
        .unwrap()
        .contains("legacy router"));
    invalid = base.clone();
    invalid
        .options
        .pins
        .insert("a".into(), arqlet_model::Point { x: 0., y: 0. });
    assert!(run(&invalid, Method::Chains)
        .err()
        .unwrap()
        .contains("pinned nodes"));
    invalid = base.clone();
    invalid.resolved_ports.retain(|p| p.edge_id != "ab");
    assert!(run(&invalid, Method::Chains).is_err());
}

#[test]
fn prepared_alignment_reuses_exact_subplans_without_reusing_geometry_or_accepting_bad_quality() {
    use arqlet_layout::constructive::abstract_realization::{
        completion::{self, Method},
        planning::Session,
    };
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../examples/fixtures/phase-width-feedback.json"
    ))
    .unwrap();
    let base = import_graph_with(graph, 1000, &legacy()).unwrap();
    let before = serde_json::to_vec(&base).unwrap();
    let baseline = completion::run(&base, Method::ChainsFixedTracks).unwrap();
    let mut session = Session::default();
    let cold = completion::run_in_session(&base, &mut session).unwrap();
    assert_eq!(cold.fixed.len(), 346);
    assert_eq!(cold.routes.len(), 358);
    assert_eq!(cold.hard_violations, 0);
    assert!(cold.track_conflicts.is_empty());
    assert!(cold.geometry_issues.is_empty());
    assert!(cold.routing_issues.is_empty());
    assert!(cold.metrics.bends < baseline.metrics.bends);
    assert!(cold.metrics.crossings < baseline.metrics.crossings);
    assert!(cold.metrics.area < baseline.metrics.area);
    assert!(arqlet_layout::improves(&cold.key, &baseline.key));
    let w = cold.planning.as_ref().unwrap();
    assert_eq!((w.ordinal_computations, w.ordinal_reuses), (95, 0));
    assert_eq!((w.lane_computations, w.lane_reuses), (349, 349));
    assert!(w.channel_computations > 0 && w.channel_reuses > 0);
    assert!(w.preferred_changes_after < w.preferred_changes_before);
    let warm = completion::run_in_session(&base, &mut session).unwrap();
    assert_eq!(
        serde_json::to_value(&cold.scene).unwrap(),
        serde_json::to_value(&warm.scene).unwrap()
    );
    let w = warm.planning.as_ref().unwrap();
    assert_eq!(
        (
            w.ordinal_computations,
            w.lane_computations,
            w.channel_computations
        ),
        (0, 0, 0)
    );
    assert_eq!((w.ordinal_reuses, w.lane_reuses), (95, 698));
    assert_eq!(before, serde_json::to_vec(&base).unwrap());

    // Main-axis translation changes every route, but no cross-domain/ordinal
    // planning dependency. No old coordinate or route may escape the handoff.
    let mut translated = base.clone();
    for node in translated
        .graph
        .nodes
        .iter()
        .filter(|n| n.parent_id.is_none())
    {
        translated.boxes.get_mut(&node.id).unwrap().x += 64.;
    }
    for port in &mut translated.resolved_ports {
        port.position.x += 64.;
    }
    for rect in translated
        .layered_plans
        .get_mut("<root>")
        .unwrap()
        .dummies
        .values_mut()
    {
        rect.x += 64.;
    }
    let cached = completion::run_in_session(&translated, &mut session).unwrap();
    let fresh = completion::run(&translated, Method::ChainsAligned).unwrap();
    assert_eq!(
        serde_json::to_value(&cached.scene).unwrap(),
        serde_json::to_value(&fresh.scene).unwrap()
    );
    assert_eq!(cached.planning.as_ref().unwrap().lane_computations, 0);
    for (a, b) in cached.routes.iter().zip(&cold.routes) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.points.len(), b.points.len());
        for (a, b) in a.points.iter().zip(&b.points) {
            assert!((a.x - b.x - 64.).abs() < 1e-6);
            assert!((a.y - b.y).abs() < 1e-6);
        }
    }
    let mut local = base.clone();
    let edge = &cold.fixed[0].id;
    local
        .resolved_ports
        .iter_mut()
        .find(|p| p.edge_id == *edge && p.endpoint == "source")
        .unwrap()
        .position
        .y += 1.;
    let cached = completion::run_in_session(&local, &mut session).unwrap();
    let fresh = completion::run(&local, Method::ChainsAligned).unwrap();
    assert_eq!(
        serde_json::to_value(&cached.scene).unwrap(),
        serde_json::to_value(&fresh.scene).unwrap()
    );
    let work = cached.planning.as_ref().unwrap();
    assert_eq!(work.ordinal_computations, 0);
    assert_eq!(
        work.lane_computations, 1,
        "only the changed endpoint's lane domains invalidate"
    );
    assert!(
        work.channel_computations > 0
            && work.channel_computations < cold.planning.as_ref().unwrap().channel_computations
    );
}

#[test]
fn prepared_candidate_sequence_matches_cold_and_surfaces_ineligible_orders() {
    use arqlet_layout::constructive::abstract_realization::{
        completion::{self, Method},
        planning::Session,
    };
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../examples/fixtures/phase-width-feedback.json"
    ))
    .unwrap();
    let base = import_graph_with(graph, 1000, &legacy()).unwrap();
    let mut session = Session::default();
    completion::run_in_session(&base, &mut session).unwrap();
    let mut invalid = 0;
    for mode in [1, 2, 3, 4] {
        let candidate = arqlet_layout::candidate(&base.graph, mode, &base.options).unwrap();
        let before = serde_json::to_vec(&candidate).unwrap();
        let cached = completion::run_in_session(&candidate, &mut session).unwrap();
        let cold = completion::run(&candidate, Method::ChainsAligned).unwrap();
        assert_eq!(
            serde_json::to_value(&cached.scene).unwrap(),
            serde_json::to_value(&cold.scene).unwrap()
        );
        assert_eq!(cached.metrics, cold.metrics);
        assert_eq!(cached.track_conflicts, cold.track_conflicts);
        assert_eq!(cached.routes.len(), 358);
        let work = cached.planning.as_ref().unwrap();
        assert!(work.ordinal_reuses > 0 && work.ordinal_computations > 0);
        // All actual cross domains change on these global-order alternatives.
        // Reusing their geometry just because some topology matches is unsafe.
        assert!(work.lane_computations > 0 && work.channel_computations > 0);
        if cached.hard_violations > 0 || !cached.track_conflicts.is_empty() {
            invalid += 1;
        }
        assert_eq!(before, serde_json::to_vec(&candidate).unwrap());
    }
    assert!(
        invalid > 0,
        "quality proxies must not silently mark every candidate eligible"
    );
}

#[test]
fn regional_swaps_preserve_other_subtrees_and_match_cold_complete_routing() {
    use arqlet_layout::constructive::abstract_realization::{completion, planning, regional};
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../examples/fixtures/phase-width-feedback.json"
    ))
    .unwrap();
    let base = import_graph_with(graph, 1000, &legacy()).unwrap();
    let before = serde_json::to_vec(&base).unwrap();
    let moves = regional::swaps(&base).unwrap();
    assert!(!moves.is_empty());
    for swap in &moves {
        let candidate = regional::prepare(&base, swap).unwrap();
        assert_eq!(candidate.placed_areas, [swap.area.clone()]);
        assert_eq!(candidate.work.region_realizations, 1);
        for (area, plan) in candidate.plans() {
            let mut expected = base.layered_plans[area].ranks.clone();
            if area == &swap.area {
                expected[swap.rank].swap(swap.left, swap.left + 1);
            }
            let real = |ranks: &Vec<Vec<String>>| {
                ranks
                    .iter()
                    .map(|rank| {
                        rank.iter()
                            .filter(|id| base.graph.nodes.iter().any(|n| n.id == **id))
                            .cloned()
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(real(&plan.ranks), real(&expected));
        }
    }
    let mut session = planning::Session::default();
    completion::run_in_session(&base, &mut session).unwrap();
    let mut count = 0;
    for swap in moves.iter().filter(|s| s.area != "<root>").take(4) {
        let candidate = regional::prepare(&base, swap).unwrap();
        assert_eq!(candidate.placed_areas[0], swap.area);
        assert!(candidate.work.region_realizations < base.layered_plans.len());
        for area in &candidate.placed_areas {
            assert!(
                area == &swap.area || {
                    let mut at = swap.area.as_str();
                    loop {
                        if at == area {
                            break true;
                        }
                        if at == "<root>" {
                            break false;
                        }
                        at = base
                            .graph
                            .nodes
                            .iter()
                            .find(|n| n.id == at)
                            .unwrap()
                            .parent_id
                            .as_deref()
                            .unwrap_or("<root>");
                    }
                },
                "only the selected frame and size-dependent ancestors may reflow"
            );
        }
        for node in &base.graph.nodes {
            let parent = node.parent_id.as_deref().unwrap_or("<root>");
            if !candidate
                .placed_areas
                .iter()
                .chain(&candidate.propagated_areas)
                .any(|a| a == parent)
                && node.id != swap.area
                && !candidate.placed_areas.contains(&node.id)
                && !candidate.propagated_areas.contains(&node.id)
            {
                assert_eq!(
                    serde_json::to_value(&candidate.boxes()[&node.id]).unwrap(),
                    serde_json::to_value(&base.boxes[&node.id]).unwrap()
                );
            }
        }
        let ranks = &candidate.plans()[&swap.area].ranks;
        let mut expected = base.layered_plans[&swap.area].ranks.clone();
        expected[swap.rank].swap(swap.left, swap.left + 1);
        let real_order = |ranks: &Vec<Vec<String>>| {
            ranks
                .iter()
                .map(|rank| {
                    rank.iter()
                        .filter(|id| base.graph.nodes.iter().any(|n| n.id == **id))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(real_order(ranks), real_order(&expected));
        let cached = candidate.complete(&mut session).unwrap();
        let cold = candidate
            .complete(&mut planning::Session::default())
            .unwrap();
        assert_eq!(
            serde_json::to_value(&cached.scene).unwrap(),
            serde_json::to_value(&cold.scene).unwrap()
        );
        assert_eq!(cached.key, cold.key);
        assert_eq!(cached.track_conflicts, cold.track_conflicts);
        assert!(cached.planning.as_ref().unwrap().ordinal_reuses > 0);
        assert!(cached.planning.as_ref().unwrap().lane_computations < 349);
        assert_eq!(cached.routes.len(), base.graph.edges.len());
        count += 1;
    }
    assert_eq!(count, 4);
    assert_eq!(before, serde_json::to_vec(&base).unwrap());
}

#[test]
fn regional_trials_keep_direction_ports_and_unaffected_frames_in_both_axes() {
    use arqlet_layout::constructive::abstract_realization::{planning, regional};
    for direction in ["lr", "tb"] {
        let graph: Graph = serde_json::from_value(json!({
            "defaultDirection":direction,
            "nodes":[
                {"id":"left","kind":"group","label":"Left"},
                {"id":"right","kind":"group","label":"Right"},
                {"id":"a","kind":"n","label":"A","parentId":"left"},
                {"id":"b","kind":"n","label":"B","parentId":"left"},
                {"id":"z","kind":"n","label":"Z","parentId":"left"},
                {"id":"c","kind":"n","label":"C","parentId":"right"},
                {"id":"d","kind":"n","label":"D","parentId":"right"}
            ],
            "ports":[
                {"id":"authored-source","nodeId":"a","side":if direction == "lr" {"right"} else {"bottom"},"offset":0.3},
                {"id":"authored-target","nodeId":"z","side":if direction == "lr" {"left"} else {"top"},"offset":0.7}
            ],
            "edgePorts":[{"edgeId":"az","sourcePort":"authored-source","targetPort":"authored-target"}],
            "edges":[
                {"id":"az","kind":"e","source":"a","target":"z"},
                {"id":"bz","kind":"e","source":"b","target":"z"},
                {"id":"parallel","kind":"e","source":"a","target":"z"},
                {"id":"cycle","kind":"e","source":"z","target":"a"},
                {"id":"loop","kind":"e","source":"b","target":"b"},
                {"id":"cross","kind":"e","source":"a","target":"c"},
                {"id":"cd","kind":"e","source":"c","target":"d"}
            ]
        }))
        .unwrap();
        let options: LayoutOptions = serde_json::from_value(json!({
            "edgeLabels":{"az":{"text":"calls","width":48,"height":22}},
            "containerSizing":{"left":{"width":{"min":900},"height":{"min":900}}}
        }))
        .unwrap();
        let base = import_graph_with(graph, 1000, &options).unwrap();
        let swaps = regional::swaps(&base).unwrap();
        let swap = swaps.iter().find(|s| s.area == "left").unwrap();
        let trial = regional::prepare(&base, swap).unwrap();
        assert_eq!(trial.placed_areas, ["left"]);
        assert_eq!(
            serde_json::to_value(&trial.boxes()["c"]).unwrap(),
            serde_json::to_value(&base.boxes["c"]).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&trial.boxes()["d"]).unwrap(),
            serde_json::to_value(&base.boxes["d"]).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&trial.plans()["right"]).unwrap(),
            serde_json::to_value(&base.layered_plans["right"]).unwrap()
        );
        for id in ["left", "right"] {
            assert!(trial.boxes()[id].width >= base.boxes[id].width);
            assert!(trial.boxes()[id].height >= base.boxes[id].height);
        }
        let drawing = trial.complete(&mut planning::Session::default()).unwrap();
        for mut shared_session in [
            planning::Session::with_construction_reuse(),
            planning::Session::with_coordination_reuse(),
        ] {
            for _ in 0..2 {
                let cached = trial.complete(&mut shared_session).unwrap();
                assert_eq!(
                    serde_json::to_value(&drawing.scene).unwrap(),
                    serde_json::to_value(&cached.scene).unwrap()
                );
                assert_eq!(drawing.key, cached.key);
                assert_eq!(drawing.track_conflicts, cached.track_conflicts);
            }
        }
        assert_eq!(drawing.routes.len(), 7);
        assert_eq!(drawing.scene.labels.len(), 1);
        for edge in &base.graph.edges {
            let route = drawing
                .scene
                .routes
                .iter()
                .find(|r| r.id == edge.id)
                .unwrap();
            assert_eq!(route.source, edge.source);
            assert_eq!(route.target, *edge.target.as_ref().unwrap());
            for (endpoint, point) in [
                ("source", route.points.first().unwrap()),
                ("target", route.points.last().unwrap()),
            ] {
                let port = drawing
                    .scene
                    .ports
                    .iter()
                    .find(|p| p.edge_id == edge.id && p.endpoint == endpoint)
                    .unwrap();
                assert_eq!(*point, port.position);
            }
        }
        for port in drawing.scene.ports.iter().filter(|p| p.edge_id == "az") {
            assert!(port.explicit);
            let source = port.endpoint == "source";
            let node = drawing
                .scene
                .nodes
                .iter()
                .find(|n| n.node.id == port.node_id)
                .unwrap();
            let (at, start, size, offset) = if direction == "lr" {
                (
                    port.position.y,
                    node.bounds.y,
                    node.bounds.height,
                    if source { 0.3 } else { 0.7 },
                )
            } else {
                (
                    port.position.x,
                    node.bounds.x,
                    node.bounds.width,
                    if source { 0.3 } else { 0.7 },
                )
            };
            assert!((at - start - size * offset).abs() < 1e-6);
        }
    }
}

#[test]
fn regional_reference_fork_recomputes_only_its_dependencies_and_improves_the_full_key() {
    use arqlet_layout::constructive::abstract_realization::{completion, planning, regional};
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../examples/fixtures/phase-width-feedback.json"
    ))
    .unwrap();
    let base = import_graph_with(graph, 1000, &legacy()).unwrap();
    let mut session = planning::Session::default();
    let initial = completion::run_in_session(&base, &mut session).unwrap();
    let trial = regional::prepare(
        &base,
        &regional::swaps(&base)
            .unwrap()
            .into_iter()
            .find(|s| s.area == "crate:reference")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(trial.placed_areas, ["crate:reference"]);
    assert!(trial.propagated_areas.is_empty());
    assert_eq!(trial.changed_boxes.len(), 2);
    let mut branch = session.clone();
    let result = trial.complete(&mut branch).unwrap();
    let work = result.planning.as_ref().unwrap();
    assert_eq!(
        (
            work.ordinal_computations,
            work.lane_computations,
            work.channel_computations
        ),
        (1, 12, 3)
    );
    assert_eq!(result.hard_violations, 0);
    assert!(
        result.track_conflicts.is_empty()
            && result.geometry_issues.is_empty()
            && result.routing_issues.is_empty()
    );
    assert!(arqlet_layout::improves(&result.key, &initial.key));
    let unchanged = completion::run_in_session(&base, &mut session).unwrap();
    assert_eq!(
        unchanged.planning.as_ref().unwrap().lane_computations,
        0,
        "a fork cannot evict the common base entries"
    );
    assert_eq!(
        serde_json::to_value(&unchanged.scene).unwrap(),
        serde_json::to_value(&initial.scene).unwrap()
    );
    assert!(std::ptr::eq(trial.source(), &base));
}

#[test]
fn regional_trials_reject_invalid_moves_and_unsupported_constraints_without_mutation() {
    use arqlet_layout::constructive::abstract_realization::regional::{self, Swap};
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../examples/fixtures/phase-width-feedback.json"
    ))
    .unwrap();
    let base = import_graph_with(graph, 1000, &legacy()).unwrap();
    let before = serde_json::to_vec(&base).unwrap();
    for swap in [
        Swap {
            area: "missing".into(),
            rank: 0,
            left: 0,
        },
        Swap {
            area: "<root>".into(),
            rank: usize::MAX,
            left: 0,
        },
        Swap {
            area: "<root>".into(),
            rank: 0,
            left: usize::MAX,
        },
    ] {
        assert!(regional::prepare(&base, &swap).is_err());
    }
    let swap = regional::swaps(&base).unwrap().remove(0);
    let mut invalid = base.clone();
    invalid.boxes.remove(&base.graph.nodes[0].id);
    assert!(regional::prepare(&invalid, &swap).is_err());
    invalid = base.clone();
    invalid.options.global_layering = true;
    assert!(regional::swaps(&invalid).is_err());
    invalid = base.clone();
    invalid.options.pins.insert(
        base.graph.nodes[0].id.clone(),
        arqlet_model::Point { x: 0., y: 0. },
    );
    assert!(regional::prepare(&invalid, &swap).is_err());
    invalid = base.clone();
    invalid.layered_plans.get_mut("<root>").unwrap().format = u32::MAX;
    assert!(regional::prepare(&invalid, &swap).is_err());
    let plan = &base.layered_plans[&swap.area];
    if let Some((rank, left)) = plan.ranks.iter().enumerate().find_map(|(rank, ids)| {
        ids.windows(2)
            .enumerate()
            .find(|(_, pair)| pair.iter().any(|id| plan.dummies.contains_key(id)))
            .map(|(left, _)| (rank, left))
    }) {
        assert!(regional::prepare(
            &base,
            &Swap {
                area: swap.area,
                rank,
                left
            }
        )
        .is_err());
    }
    assert_eq!(before, serde_json::to_vec(&base).unwrap());
}

#[test]
fn regional_repair_certificates_preserve_complete_results_and_independent_forks() {
    regional_certificates_preserve_complete_results_and_independent_forks(false, false);
}

#[test]
fn regional_construction_certificates_preserve_complete_results_and_independent_forks() {
    regional_certificates_preserve_complete_results_and_independent_forks(true, false);
}

#[test]
fn regional_coordination_projections_preserve_complete_results_and_independent_forks() {
    regional_certificates_preserve_complete_results_and_independent_forks(true, true);
}

fn regional_certificates_preserve_complete_results_and_independent_forks(
    construction: bool,
    coordination: bool,
) {
    use arqlet_layout::constructive::abstract_realization::{completion, planning, regional};
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../examples/fixtures/phase-width-feedback.json"
    ))
    .unwrap();
    let base = import_graph_with(graph, 1000, &legacy()).unwrap();
    let original = serde_json::to_vec(&base).unwrap();
    let mut session = if coordination {
        planning::Session::with_coordination_reuse()
    } else if construction {
        planning::Session::with_construction_reuse()
    } else {
        planning::Session::with_repair_reuse()
    };
    let cold_base = completion::run_in_session(&base, &mut planning::Session::default()).unwrap();
    let primed = completion::run_in_session(&base, &mut session).unwrap();
    assert_eq!(
        serde_json::to_vec(&cold_base.scene).unwrap(),
        serde_json::to_vec(&primed.scene).unwrap()
    );
    let mut areas = std::collections::BTreeSet::new();
    let swaps = regional::swaps(&base).unwrap();
    let selected: Vec<_> = swaps
        .iter()
        .filter(|s| s.area != "<root>" && areas.insert(s.area.clone()))
        .take(8)
        .collect();
    let mut reference_hits = 0;
    for swap in selected {
        let candidate = regional::prepare(&base, swap).unwrap();
        let cold = candidate
            .complete(&mut planning::Session::default())
            .unwrap();
        let mut branch = session.clone();
        let cached = candidate.complete(&mut branch).unwrap();
        let repeat = candidate.complete(&mut branch).unwrap();
        for drawing in [&cached, &repeat] {
            assert_eq!(
                serde_json::to_vec(&cold.scene).unwrap(),
                serde_json::to_vec(&drawing.scene).unwrap()
            );
            assert_eq!(cold.key, drawing.key);
            assert_eq!(cold.track_conflicts, drawing.track_conflicts);
            assert_eq!(cold.hard_violations, drawing.hard_violations);
            assert_eq!(
                serde_json::to_value(&cold.fixed).unwrap(),
                serde_json::to_value(&drawing.fixed).unwrap()
            );
            assert_eq!(cold.delegated, drawing.delegated);
            if construction {
                let mut fresh = cold.search.construction.clone();
                let mut used = drawing.search.construction.clone();
                fresh.preparation_nanos = 0;
                used.preparation_nanos = 0;
                assert_eq!(fresh, used);
                let work = drawing.construction_reuse.as_ref().unwrap();
                assert_eq!(
                    work.geometries_computed + work.geometries_reused,
                    used.planned_attempts
                );
                assert_eq!(
                    work.obstacle_decisions_computed + work.obstacle_decisions_reused,
                    used.planned_attempts
                );
                assert_eq!(
                    work.occupancy_checks,
                    used.planned_attempts - used.planned_obstacle_failures
                );
                let old = cold.coordination_work.as_ref().unwrap();
                let new = drawing.coordination_work.as_ref().unwrap();
                assert_eq!(
                    old.span_computations,
                    new.span_computations + new.span_reuses
                );
                assert_eq!(
                    old.domain_computations,
                    new.domain_computations + new.domain_reuses
                );
                if coordination {
                    assert!(new.span_reuses > 0 && new.domain_reuses > 0);
                    assert_eq!(new.span_computations, 95);
                } else {
                    assert_eq!(new.span_reuses + new.domain_reuses, 0);
                }
            }
            assert_eq!(
                serde_json::to_value(&cold.metrics).unwrap(),
                serde_json::to_value(&drawing.metrics).unwrap()
            );
            let fresh = cold.track_repair.as_ref().unwrap();
            let work = drawing.track_repair.as_ref().unwrap();
            assert_eq!(fresh.candidates, work.candidates + work.candidates_avoided);
            assert_eq!(
                fresh.decisions_computed,
                work.decisions_computed + work.decisions_reused
            );
            assert_eq!(
                (
                    fresh.moves,
                    fresh.passes,
                    fresh.initial_pairs,
                    fresh.remaining_pairs
                ),
                (
                    work.moves,
                    work.passes,
                    work.initial_pairs,
                    work.remaining_pairs
                )
            );
        }
        if construction {
            let work = repeat.construction_reuse.as_ref().unwrap();
            assert_eq!(work.geometries_computed, 0);
            assert_eq!(work.obstacle_decisions_computed, 0);
            assert_eq!(cached.coordination_work, repeat.coordination_work);
        } else {
            assert_eq!(repeat.track_repair.as_ref().unwrap().decisions_computed, 0);
        }
        if swap.area == "crate:reference" {
            if construction {
                let work = cached.construction_reuse.as_ref().unwrap();
                reference_hits = work.geometries_reused;
                assert_eq!(work.geometries_computed, 12);
                assert_eq!(work.obstacle_decisions_reused, 334);
            } else {
                let work = cached.track_repair.as_ref().unwrap();
                reference_hits = work.decisions_reused;
                assert_eq!(work.candidates_avoided, 206);
                assert_eq!(work.candidates, 1703);
            }
        }
    }
    assert_eq!(reference_hits, if construction { 334 } else { 1 });
    let warm = completion::run_in_session(&base, &mut session).unwrap();
    assert_eq!(
        serde_json::to_vec(&warm.scene).unwrap(),
        serde_json::to_vec(&cold_base.scene).unwrap()
    );
    if construction {
        let work = warm.construction_reuse.as_ref().unwrap();
        assert_eq!(
            (
                work.geometries_computed,
                work.geometries_reused,
                work.obstacle_decisions_computed,
                work.obstacle_decisions_reused
            ),
            (0, 346, 0, 346)
        );
    } else {
        let work = warm.track_repair.as_ref().unwrap();
        assert_eq!(
            (
                work.decisions_computed,
                work.decisions_reused,
                work.candidates,
                work.candidates_avoided
            ),
            (0, 2, 0, 1909)
        );
    }
    assert_eq!(original, serde_json::to_vec(&base).unwrap());
}

#[test]
fn coordination_contexts_are_rebuilt_for_changed_global_candidate_geometry() {
    use arqlet_layout::constructive::abstract_realization::{completion, planning};
    let graph: Graph = serde_json::from_str(include_str!(
        "../../../examples/fixtures/phase-width-feedback.json"
    ))
    .unwrap();
    let base = import_graph_with(graph, 1000, &legacy()).unwrap();
    let mut session = planning::Session::with_coordination_reuse();
    let mut first = None;
    for mode in [0, 1, 2, 3, 4, 0] {
        let candidate = if mode == 0 {
            base.clone()
        } else {
            arqlet_layout::candidate(&base.graph, mode, &base.options).unwrap()
        };
        let input = serde_json::to_vec(&candidate).unwrap();
        let cold = completion::run_in_session(
            &candidate,
            &mut planning::Session::with_construction_reuse(),
        )
        .unwrap();
        let shared = completion::run_in_session(&candidate, &mut session).unwrap();
        let scene = serde_json::to_vec(&shared.scene).unwrap();
        assert_eq!(scene, serde_json::to_vec(&cold.scene).unwrap());
        assert_eq!(cold.key, shared.key);
        assert_eq!(cold.track_conflicts, shared.track_conflicts);
        assert_eq!(cold.hard_violations, shared.hard_violations);
        assert_eq!(cold.delegated, shared.delegated);
        let before = cold.coordination_work.unwrap();
        let after = shared.coordination_work.unwrap();
        assert_eq!(
            before.span_computations,
            after.span_computations + after.span_reuses
        );
        assert_eq!(
            before.domain_computations,
            after.domain_computations + after.domain_reuses
        );
        assert!(
            after.span_computations > 0 && after.domain_computations > 0,
            "new geometry always gets a new context"
        );
        if mode == 0 {
            if let Some(first) = &first {
                assert_eq!(&scene, first);
            } else {
                first = Some(scene);
            }
        }
        assert_eq!(input, serde_json::to_vec(&candidate).unwrap());
    }
}

fn graph(nodes: serde_json::Value, edges: serde_json::Value) -> Graph {
    serde_json::from_value(json!({
        "defaultDirection": "lr",
        "containerDirections": {},
        "defaultStrategy": "none",
        "containerStrategies": {},
        "nodes": nodes,
        "edges": edges,
    }))
    .unwrap()
}

#[test]
fn petri_net_keeps_its_constructed_routes_and_stays_valid() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/petri-net.json"
    ))
    .unwrap();
    let graph: Graph = serde_json::from_str(&text).unwrap();
    let snapshot = import(graph, true);
    let constructed = assert_constructed_routes_kept(&snapshot);
    assert!(
        constructed > 0,
        "the petri-net fixture constructs some edges"
    );
    assert_eq!(
        snapshot.routes.len(),
        snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.target.is_some())
            .count()
    );
    let hard = arqlet_layout::shadow_evaluation(&snapshot)
        .unwrap()
        .hard_violations;
    assert_eq!(
        hard, 0,
        "constructed routes and routed edges must not hit bodies"
    );
}

/// The construction reorders the port slots of the edges it builds. An edge that is reordered and then fails would be
/// drawn by the general routing from its own slot, which another edge holds by then; no two edges may end in one point.
#[test]
fn deployment_cluster_gives_every_edge_its_own_end_points() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/deployment-cluster.json"
    ))
    .unwrap();
    let graph: Graph = serde_json::from_str(&text).unwrap();
    let snapshot = import(graph, true);
    assert_constructed_routes_kept(&snapshot);
    let mut ends = std::collections::BTreeMap::new();
    for route in snapshot.routes.iter().filter(|r| r.bundle_of.is_none()) {
        let edge = snapshot
            .graph
            .edges
            .iter()
            .find(|e| e.id == route.id)
            .unwrap();
        let target = edge.target.as_deref().unwrap();
        if edge.source == target {
            continue;
        }
        let (first, last) = (route.points.first().unwrap(), route.points.last().unwrap());
        for (node, point) in [(edge.source.as_str(), first), (target, last)] {
            let key = (
                node.to_string(),
                (point.x * 1000.).round() as i64,
                (point.y * 1000.).round() as i64,
            );
            if let Some(other) = ends.insert(key, route.id.clone()) {
                panic!("{} and {} end in the same point at {node}", other, route.id);
            }
        }
    }
}

/// The self-loops of a node are nested at the corner whose sides carry the fewest other edges, and their ends lie at that
/// corner: no loop crosses another route, straight ones included.
#[test]
fn self_loops_are_nested_at_a_free_corner_and_cross_no_route() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/self-loops.json"
    ))
    .unwrap();
    let graph: Graph = serde_json::from_str(&text).unwrap();
    let snapshot = import(graph, true);
    assert_constructed_routes_kept(&snapshot);
    type Segment = ((f64, f64), (f64, f64));
    let segments = |id: &str| -> Vec<Segment> {
        let route = snapshot.routes.iter().find(|r| r.id == id).unwrap();
        route
            .points
            .windows(2)
            .map(|w| ((w[0].x, w[0].y), (w[1].x, w[1].y)))
            .collect()
    };
    // A proper crossing of two segments of any direction.
    let crosses = |(p, q): Segment, (u, v): Segment| {
        let side = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
            (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
        };
        let (d1, d2) = (side(u, v, p), side(u, v, q));
        let (d3, d4) = (side(p, q, u), side(p, q, v));
        d1 * d2 < -1e-9 && d3 * d4 < -1e-9
    };
    let loops: Vec<&str> = snapshot
        .graph
        .edges
        .iter()
        .filter(|e| e.target.as_deref() == Some(e.source.as_str()))
        .map(|e| e.id.as_str())
        .collect();
    for &id in &loops {
        for other in snapshot
            .routes
            .iter()
            .filter(|r| r.id != id && r.bundle_of.is_none())
        {
            for &s in &segments(id) {
                for &t in &segments(&other.id) {
                    assert!(!crosses(s, t), "self-loop {id} crosses {}", other.id);
                }
            }
        }
    }
}

/// The placement keeps a plan of every layered area: its ranks with the long-edge dummies, a chain for every link between
/// neighbouring or distant ranks, and the dummies' places; the improved order does not raise the crossings of the plan.
#[test]
fn petri_net_has_a_layered_plan_with_chains_and_an_order() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/petri-net.json"
    ))
    .unwrap();
    let graph: Graph = serde_json::from_str(&text).unwrap();
    let plans = arqlet_layout::layered_plans(&graph, &options(true));
    let plan = plans.get("<root>").expect("the root area is layered");
    let in_ranks: std::collections::BTreeSet<&str> =
        plan.ranks.iter().flatten().map(String::as_str).collect();
    for ((a, b), dummies) in &plan.chains {
        assert!(in_ranks.contains(a.as_str()) && in_ranks.contains(b.as_str()));
        for d in dummies {
            assert!(in_ranks.contains(d.as_str()), "dummy {d} is in a rank");
            assert!(plan.dummies.contains_key(d), "dummy {d} has a place");
        }
    }
    assert!(
        plan.chains.values().any(|d| !d.is_empty()),
        "petri-net has long links"
    );
    assert!(arqlet_layout::layered_plans(&graph, &options(false)).is_empty());
    eprintln!(
        "petri-net plan: {} ranks, {} chains, {} crossings",
        plan.ranks.len(),
        plan.chains.len(),
        plan.crossings()
    );
}

#[test]
fn a_fan_out_keeps_every_constructed_route() {
    let g = graph(
        json!([
            {"id": "a", "kind": "symbol", "label": "a"},
            {"id": "b", "kind": "symbol", "label": "b"},
            {"id": "c", "kind": "symbol", "label": "c"},
            {"id": "d", "kind": "symbol", "label": "d"}
        ]),
        json!([
            {"id": "e1", "kind": "calls", "source": "a", "target": "b"},
            {"id": "e2", "kind": "calls", "source": "a", "target": "c"},
            {"id": "e3", "kind": "calls", "source": "a", "target": "d"}
        ]),
    );
    let snapshot = import(g, true);
    assert!(
        assert_constructed_routes_kept(&snapshot) > 0,
        "the fan-out constructs edges"
    );
    assert_eq!(snapshot.routes.len(), 3);
    assert_eq!(
        arqlet_layout::shadow_evaluation(&snapshot)
            .unwrap()
            .hard_violations,
        0
    );
}

#[test]
fn a_long_edge_past_an_intermediate_node_is_routed_clear_of_it() {
    let g = graph(
        json!([
            {"id": "a", "kind": "symbol", "label": "a"},
            {"id": "m", "kind": "symbol", "label": "m"},
            {"id": "b", "kind": "symbol", "label": "b"}
        ]),
        json!([
            {"id": "long", "kind": "calls", "source": "a", "target": "b"},
            {"id": "short", "kind": "calls", "source": "a", "target": "m"},
            {"id": "tail", "kind": "calls", "source": "m", "target": "b"}
        ]),
    );
    let snapshot = import(g, true);
    assert!(
        assert_constructed_routes_kept(&snapshot) > 0,
        "the long edge fixture constructs edges"
    );
    assert_eq!(snapshot.routes.len(), 3);
    assert_eq!(
        arqlet_layout::shadow_evaluation(&snapshot)
            .unwrap()
            .hard_violations,
        0
    );
}

#[test]
fn the_default_import_has_no_constructed_routes_applied() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/petri-net.json"
    ))
    .unwrap();
    let graph: Graph = serde_json::from_str(&text).unwrap();
    let on = import(graph.clone(), true);
    let off = import(graph, false);
    assert_eq!(
        serde_json::to_value(options(false)).unwrap()["layeredConstruction"],
        false
    );
    assert_ne!(
        serde_json::to_value(&on.routes).unwrap(),
        serde_json::to_value(&off.routes).unwrap(),
        "the option changes the routes it constructs"
    );
}

#[test]
fn default_import_uses_the_hierarchical_pipeline_on_supported_areas() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/petri-net.json"
    ))
    .unwrap();
    let graph: Graph = serde_json::from_str(&text).unwrap();
    let automatic = import_graph_with(graph.clone(), 1000, &legacy()).unwrap();
    let explicit = import_graph_with(
        graph.clone(),
        1000,
        &LayoutOptions {
            layered_construction: true,
            hierarchical_ordering: true,
            ..legacy()
        },
    )
    .unwrap();

    assert!(automatic.options.layered_construction);
    assert!(automatic.options.hierarchical_ordering);
    assert!(automatic.layout_diagnostics.is_empty());
    assert!(!automatic.layered_plans.is_empty());
    let expected_routes = graph
        .edges
        .iter()
        .filter(|edge| edge.target.is_some())
        .count();
    assert_eq!(automatic.routes.len(), expected_routes);
    assert_eq!(
        arqlet_layout::shadow_evaluation(&automatic)
            .unwrap()
            .hard_violations,
        0
    );
    assert_eq!(
        serde_json::to_value((&automatic.boxes, &automatic.routes)).unwrap(),
        serde_json::to_value((&explicit.boxes, &explicit.routes)).unwrap()
    );
    assert_eq!(
        serde_json::to_value(legacy()).unwrap()["layeredConstruction"],
        serde_json::Value::Null,
        "default options omit the true pipeline flags"
    );
}

#[test]
fn non_hierarchical_strategies_keep_their_placement_and_report_fallback() {
    for (strategy, direction) in [("tree", "lr"), ("radial", "auto"), ("organic", "auto")] {
        let graph: Graph = serde_json::from_value(json!({
            "defaultDirection": direction,
            "defaultStrategy": strategy,
            "nodes": [
                {"id": "a", "kind": "n", "label": "a"},
                {"id": "b", "kind": "n", "label": "b"},
                {"id": "c", "kind": "n", "label": "c"}
            ],
            "edges": [
                {"id": "ab", "kind": "e", "source": "a", "target": "b"},
                {"id": "bc", "kind": "e", "source": "b", "target": "c"}
            ]
        }))
        .unwrap();
        let automatic = import_graph_with(graph.clone(), 1000, &legacy())
            .unwrap_or_else(|error| panic!("{strategy}: {error}"));
        let previous = import_graph_with(
            graph,
            1000,
            &LayoutOptions {
                layered_construction: false,
                ..legacy()
            },
        )
        .unwrap_or_else(|error| panic!("{strategy}: {error}"));

        assert!(
            automatic
                .layout_diagnostics
                .iter()
                .any(|message| message.contains("previous layout pipeline")),
            "{strategy}: {:?}",
            automatic.layout_diagnostics
        );
        assert!(automatic.layered_plans.is_empty(), "{strategy}");
        assert_eq!(
            serde_json::to_value((&automatic.boxes, &automatic.routes)).unwrap(),
            serde_json::to_value((&previous.boxes, &previous.routes)).unwrap(),
            "{strategy} placement or routing changed"
        );
    }
}

#[test]
fn an_edit_keeps_the_plans_of_the_areas_it_does_not_place_again() {
    let node = |id: &str, parent: Option<&str>| json!({"id": id, "kind": "symbol", "label": id, "parentId": parent});
    let edge =
        |id: &str, s: &str, t: &str| json!({"id": id, "kind": "calls", "source": s, "target": t});
    let g = graph(
        json!([
            node("A", None),
            node("B", None),
            node("a1", Some("A")),
            node("a2", Some("A")),
            node("a3", Some("A")),
            node("b1", Some("B")),
            node("b2", Some("B")),
        ]),
        json!([
            edge("e1", "a1", "a2"),
            edge("e2", "a2", "a3"),
            edge("e3", "a1", "a3"),
            edge("e4", "b1", "b2"),
            edge("e5", "a3", "b1"),
        ]),
    );
    let base = import(g, true);
    for area in ["<root>", "A", "B"] {
        assert!(
            base.layered_plans.contains_key(area),
            "the import keeps the plan of {area}"
        );
    }
    let added = arqlet_runtime::add_node(&base, base.revision, "b2", "Added").unwrap();
    // A is not placed again: its plan is the one of the import.
    assert_eq!(
        serde_json::to_value(&added.layered_plans["A"]).unwrap(),
        serde_json::to_value(&base.layered_plans["A"]).unwrap()
    );
    // B is placed again, with the new node in its plan.
    let members: Vec<&String> = added.layered_plans["B"].ranks.iter().flatten().collect();
    assert!(
        members.iter().any(|id| id.starts_with("local:")),
        "B's plan holds the added node: {members:?}"
    );
    assert!(added.layered_plans.contains_key("<root>"));
    // The stored plans survive the trip through JSON.
    let back: Snapshot = serde_json::from_str(&serde_json::to_string(&added).unwrap()).unwrap();
    assert_eq!(back.layered_plans.len(), added.layered_plans.len());
}

#[test]
fn the_legacy_opt_out_stores_no_plans() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../benchmarks/datasets/petri-net.json"
    ))
    .unwrap();
    let graph: Graph = serde_json::from_str(&text).unwrap();
    let snapshot = import(graph, false);
    assert!(snapshot.layered_plans.is_empty());
    assert!(!serde_json::to_string(&snapshot)
        .unwrap()
        .contains("layeredPlans"));
}

#[test]
fn global_layering_lays_out_containers_of_the_same_axis_in_common_ranks() {
    let node = |id: &str, parent: Option<&str>| json!({"id": id, "kind": "symbol", "label": id, "parentId": parent});
    let edge =
        |id: &str, s: &str, t: &str| json!({"id": id, "kind": "calls", "source": s, "target": t});
    let g = graph(
        json!([
            node("A", None),
            node("B", None),
            node("x", None),
            node("a1", Some("A")),
            node("a2", Some("A")),
            node("a3", Some("A")),
            node("b1", Some("B")),
            node("b2", Some("B")),
        ]),
        json!([
            edge("e1", "a1", "a2"),
            edge("e2", "a2", "a3"),
            edge("e3", "a1", "b1"),
            edge("e4", "b1", "b2"),
            edge("e5", "x", "a1"),
            edge("e6", "b2", "a3"),
        ]),
    );
    let options = LayoutOptions {
        layered_construction: true,
        global_layering: true,
        ..legacy()
    };
    let s = import_graph_with(g, 1000, &options).unwrap();
    assert!(s.geometry_issues.is_empty(), "{:?}", s.geometry_issues);
    let (a, b) = (&s.boxes["A"], &s.boxes["B"]);
    let apart = a.x + a.width <= b.x
        || b.x + b.width <= a.x
        || a.y + a.height <= b.y
        || b.y + b.height <= a.y;
    assert!(apart, "A {a:?} and B {b:?} overlap");
    // Children lie in their container (their boxes are in its frame).
    for (child, container) in [
        ("a1", "A"),
        ("a2", "A"),
        ("a3", "A"),
        ("b1", "B"),
        ("b2", "B"),
    ] {
        let (r, c) = (&s.boxes[child], &s.boxes[container]);
        assert!(
            r.x >= 0. && r.y >= 0. && r.x + r.width <= c.width && r.y + r.height <= c.height,
            "{child} {r:?} in {c:?}"
        );
    }
    // a2 and b1 follow a1 in the common ranks (left to right): they start at the same x.
    let abs = |child: &str, container: &str| s.boxes[container].x + s.boxes[child].x;
    assert!(
        (abs("a2", "A") - abs("b1", "B")).abs() < 1e-6,
        "a2 and b1 share a rank"
    );
    assert!(s.routing_issues.is_empty(), "{:?}", s.routing_issues);
}

#[test]
fn global_layering_lays_out_nested_containers_of_the_same_axis_in_common_ranks() {
    let node = |id: &str, parent: Option<&str>| json!({"id": id, "kind": "symbol", "label": id, "parentId": parent});
    let edge =
        |id: &str, s: &str, t: &str| json!({"id": id, "kind": "calls", "source": s, "target": t});
    let g = graph(
        json!([
            node("A", None),
            node("B", None),
            node("A1", Some("A")),
            node("a1", Some("A")),
            node("x", Some("A1")),
            node("y", Some("A1")),
            node("b1", Some("B")),
            node("b2", Some("B")),
        ]),
        json!([
            edge("e1", "a1", "x"),
            edge("e2", "x", "y"),
            edge("e3", "y", "b2"),
            edge("e4", "a1", "b1"),
            edge("e5", "b1", "b2"),
        ]),
    );
    let options = LayoutOptions {
        layered_construction: true,
        global_layering: true,
        ..legacy()
    };
    let s = import_graph_with(g, 1000, &options).unwrap();
    assert!(s.geometry_issues.is_empty(), "{:?}", s.geometry_issues);
    let inside = |child: &str, container: &str| {
        let (r, c) = (&s.boxes[child], &s.boxes[container]);
        r.x >= 0.
            && r.y >= 0.
            && r.x + r.width <= c.width + 1e-6
            && r.y + r.height <= c.height + 1e-6
    };
    for (child, container) in [
        ("A1", "A"),
        ("a1", "A"),
        ("x", "A1"),
        ("y", "A1"),
        ("b1", "B"),
        ("b2", "B"),
    ] {
        assert!(
            inside(child, container),
            "{child} {:?} in {container} {:?}",
            s.boxes[child],
            s.boxes[container]
        );
    }
    // x (inside A1 inside A) follows a1 in the common ranks, as b1 (inside B) does: they start at the same x.
    let abs_x = |id: &str| {
        let mut x = 0.;
        let mut at = Some(id.to_string());
        while let Some(n) = at {
            x += s.boxes[&n].x;
            at = s
                .graph
                .nodes
                .iter()
                .find(|m| m.id == n)
                .and_then(|m| m.parent_id.clone());
        }
        x
    };
    assert!(
        (abs_x("x") - abs_x("b1")).abs() < 1e-6,
        "x {} b1 {}",
        abs_x("x"),
        abs_x("b1")
    );
    assert!(s.routing_issues.is_empty(), "{:?}", s.routing_issues);
}
