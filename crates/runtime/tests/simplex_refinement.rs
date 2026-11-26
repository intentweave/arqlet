//! `simplexRanksCandidate` (ADR 0027, step 1): the refinement of a single-pass drawing also lays the view out with
//! network-simplex ranks and takes that drawing only when its published key is better. Off by default.
use arqlet_model::{Graph, LayoutOptions, Snapshot};
use arqlet_runtime::execution::{JobReport, Publication};
use arqlet_runtime::{
    add_node, configure_snapshot, import_graph, import_graph_with, refine_snapshot_job,
    refine_snapshot_progressive, toggle_container,
};
use serde_json::json;

/// A view whose published key is better with network-simplex ranks.
fn better() -> Graph {
    serde_json::from_str(include_str!("../../../benchmarks/inputs/bpmn-pools.json")).unwrap()
}

/// A view whose ranks differ with network simplex, without a better published key.
fn not_better() -> Graph {
    serde_json::from_str(include_str!("../../../benchmarks/inputs/uml-class.json")).unwrap()
}

fn candidate() -> LayoutOptions {
    LayoutOptions {
        simplex_ranks_candidate: true,
        ..Default::default()
    }
}

fn ranked() -> LayoutOptions {
    LayoutOptions {
        simplex_ranks: true,
        ..Default::default()
    }
}

fn view(graph: Graph, options: &LayoutOptions) -> Snapshot {
    let snapshot = import_graph_with(graph, 150, options).unwrap();
    assert!(snapshot.options.single_pass, "the view runs single pass");
    snapshot
}

fn refine(snapshot: &Snapshot, threads: usize, work: Option<usize>) -> (Snapshot, JobReport) {
    refine_snapshot_job(
        "job",
        snapshot,
        snapshot.revision,
        snapshot.layout_version,
        threads,
        None,
        work,
    )
    .unwrap()
}

fn geometry(snapshot: &Snapshot) -> serde_json::Value {
    serde_json::to_value((&snapshot.boxes, &snapshot.routes, &snapshot.resolved_ports)).unwrap()
}

fn key(snapshot: &Snapshot) -> Vec<i64> {
    arqlet_layout::published_key(snapshot).unwrap()
}

fn strategy(snapshot: &Snapshot) -> &str {
    &snapshot.optimization.as_ref().unwrap().strategy
}

fn reason(report: &JobReport) -> Option<&str> {
    report
        .excluded
        .iter()
        .find(|e| e.strategy == "simplex-ranks")
        .map(|e| e.reason.as_str())
}

#[test]
fn without_the_option_nothing_is_planned_reported_or_serialized() {
    let input = view(better(), &LayoutOptions::default());
    assert!(!serde_json::to_string(&input)
        .unwrap()
        .contains("simplexRanks"));
    let (result, report) = refine(&input, 1, None);
    assert_eq!(strategy(&result), "keep-existing");
    assert!(report.completed.is_empty() && report.candidate_work.is_empty());
    assert_eq!(reason(&report), None);
    assert_eq!(geometry(&result), geometry(&input));
    assert!(!serde_json::to_string(&result)
        .unwrap()
        .contains("simplexRanks"));
}

#[test]
fn a_better_key_exchanges_the_drawing_and_its_ranking() {
    let input = view(better(), &candidate());
    // The option changes no layout.
    assert_eq!(
        geometry(&input),
        geometry(&view(better(), &LayoutOptions::default()))
    );
    let mut publications: Vec<Publication> = Vec::new();
    let (result, report) = refine_snapshot_progressive(
        "job",
        &input,
        input.revision,
        input.layout_version,
        1,
        None,
        None,
        &mut |p| publications.push(p.clone()),
    )
    .unwrap();
    assert_eq!(strategy(&result), "simplex-ranks");
    assert!(key(&result) < key(&input));
    assert_eq!(report.completed.len(), 1);
    assert_eq!(report.completed[0].strategy, "simplex-ranks");
    assert_eq!(
        (
            report.completed[0].parameter,
            report.completed[0].occurrence
        ),
        (0, 0)
    );
    assert_eq!(report.counters.routing_rounds, 0);
    assert_eq!(
        result.optimization.as_ref().unwrap().candidates_evaluated,
        2
    );
    // The drawing is the view laid out with the ranks, and says so.
    assert!(result.options.simplex_ranks && result.options.simplex_ranks_candidate);
    assert!(result.options.single_pass && result.options.single_pass_auto);
    assert_eq!(geometry(&result), geometry(&view(better(), &ranked())));
    // Every edge is drawn, in its logical direction.
    assert_eq!(result.graph.edges.len(), input.graph.edges.len());
    for (a, b) in result.graph.edges.iter().zip(&input.graph.edges) {
        assert_eq!((&a.source, &a.target), (&b.source, &b.target));
    }
    assert_eq!(result.routes.len(), input.routes.len());
    assert!(result.geometry_issues.is_empty());
    assert_eq!(result.revision, input.revision);
    assert_eq!(result.layout_version, input.layout_version + 1);
    assert_eq!(result.base_revision, Some(input.revision));
    // Published once, against the identity of the input, so a host can discard it when that is stale.
    assert_eq!(publications.len(), 1);
    assert_eq!(publications[0].strategy, "simplex-ranks");
    assert_eq!(publications[0].input, report.input);
    assert!(publications[0].snapshot.options.simplex_ranks);
    assert_eq!(geometry(&publications[0].snapshot), geometry(&result));
    // A second refinement has nothing left to try, and keeps drawing and ranking.
    let (again, report) = refine(&result, 1, None);
    assert_eq!(reason(&report), Some("simplexRanks is on already"));
    assert_eq!(strategy(&again), "keep-existing");
    assert!(again.options.simplex_ranks);
    assert_eq!(geometry(&again), geometry(&result));
    // A result for the earlier layout version is refused.
    assert!(refine_snapshot_job(
        "job",
        &result,
        input.revision,
        input.layout_version,
        1,
        None,
        None
    )
    .is_err());
}

#[test]
fn without_a_better_key_the_drawing_and_the_default_ranks_stay() {
    let input = view(not_better(), &candidate());
    assert_ne!(
        geometry(&input),
        geometry(&view(not_better(), &ranked())),
        "the two rankings draw this view differently"
    );
    let (result, report) = refine(&input, 1, None);
    assert_eq!(report.completed.len(), 1);
    assert_eq!(strategy(&result), "keep-existing");
    assert!(!result.options.simplex_ranks);
    assert_eq!(geometry(&result), geometry(&input));
    // Apart from the option and the count, the answer is the one without the option.
    let (plain, _) = refine(&view(not_better(), &LayoutOptions::default()), 1, None);
    let mut expected = serde_json::to_value(&plain).unwrap();
    expected["options"] = serde_json::to_value(&input.options).unwrap();
    expected["optimization"]["candidatesEvaluated"] = json!(2);
    assert_eq!(serde_json::to_value(&result).unwrap(), expected);
}

#[test]
fn the_candidate_counts_in_the_work_budget_for_any_thread_count() {
    let input = view(better(), &candidate());
    let (limited, report) = refine(&input, 1, Some(0));
    assert!(report.completed.is_empty());
    assert!(limited.optimization.as_ref().unwrap().work_limited);
    assert_eq!(strategy(&limited), "keep-existing");
    assert!(!limited.options.simplex_ranks);
    assert_eq!(geometry(&limited), geometry(&input));
    let (one, _) = refine(&input, 1, Some(1));
    assert!(!one.optimization.as_ref().unwrap().work_limited);
    assert_eq!(strategy(&one), "simplex-ranks");
    for threads in [1, 4] {
        for work in [Some(1), None] {
            let (result, _) = refine(&input, threads, work);
            assert_eq!(
                serde_json::to_vec(&result).unwrap(),
                serde_json::to_vec(&one).unwrap(),
                "{threads} threads, work {work:?}"
            );
        }
    }
}

#[test]
fn a_stopped_job_keeps_the_drawing() {
    let input = view(better(), &candidate());
    let (result, report) = refine_snapshot_job(
        "job",
        &input,
        input.revision,
        input.layout_version,
        1,
        Some(&|| true),
        None,
    )
    .unwrap();
    assert!(report.completed.is_empty());
    assert!(result.optimization.as_ref().unwrap().interrupted);
    assert!(!result.options.simplex_ranks);
    assert_eq!(geometry(&result), geometry(&input));
}

#[test]
fn only_a_single_pass_drawing_has_the_candidate() {
    let before = LayoutOptions {
        single_pass_auto: false,
        single_pass: false,
        abstract_routing: false,
        ..candidate()
    };
    let input = import_graph_with(better(), 150, &before).unwrap();
    assert!(!input.options.single_pass);
    let (result, report) = refine(&input, 1, None);
    assert_eq!(reason(&report), Some("needs a single-pass drawing"));
    assert!(report
        .completed
        .iter()
        .all(|id| id.strategy != "simplex-ranks"));
    assert!(!result.options.simplex_ranks);

    // A view with buses that fell back still names single pass in its options; the diagnostic says it was not used.
    let mut fallen = view(better(), &candidate());
    fallen
        .layout_diagnostics
        .push("Single pass was not used: a reason".into());
    let (result, report) = refine(&fallen, 1, None);
    assert_eq!(reason(&report), Some("needs a single-pass drawing"));
    assert!(report.completed.is_empty());
    assert!(!result.options.simplex_ranks);
}

#[test]
fn edits_keep_the_chosen_ranking_and_an_import_starts_from_the_default() {
    let input = view(better(), &candidate());
    let (refined, _) = refine(&input, 1, None);
    assert!(refined.options.simplex_ranks);

    let container = refined
        .graph
        .nodes
        .iter()
        .find_map(|n| n.parent_id.clone())
        .unwrap();
    let closed = toggle_container(
        &refined,
        refined.revision,
        refined.layout_version,
        &container,
    )
    .unwrap();
    assert!(closed.options.simplex_ranks);
    let opened =
        toggle_container(&closed, closed.revision, closed.layout_version, &container).unwrap();
    assert!(opened.options.simplex_ranks);
    // Opened again, the view is the refined one: the ranks did not flip back.
    assert_eq!(geometry(&opened), geometry(&refined));

    let leaf = refined
        .graph
        .nodes
        .iter()
        .find(|n| {
            !refined
                .graph
                .nodes
                .iter()
                .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
        })
        .unwrap();
    let added = add_node(&refined, refined.revision, &leaf.id, "added").unwrap();
    assert!(added.options.simplex_ranks);

    let configured = configure_snapshot(
        &refined,
        refined.revision,
        refined.layout_version,
        &json!({"trackSpacing": 8.0}),
    )
    .unwrap();
    assert!(configured.options.simplex_ranks);

    // The ranking travels with the stored snapshot.
    let loaded: Snapshot = serde_json::from_str(&serde_json::to_string(&refined).unwrap()).unwrap();
    assert!(loaded.options.simplex_ranks);

    let fresh = import_graph(better(), 150).unwrap();
    assert!(!fresh.options.simplex_ranks);
}
