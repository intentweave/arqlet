//! Reproducible latency measurement for one graph: view import, refinement at several thread
//! counts, and one local edit. Prints one JSON object. Build and run with --release.
use arqlet_model::{
    ConfigSet, Contour, Graph, Measurement, MovePhase, MoveStep, NodeShape, Point, Snapshot,
};
use arqlet_runtime::{
    add_node, archive_indices, configure_snapshot, measure_contours, move_node,
    refine_snapshot_job, refine_snapshot_limited, refine_snapshot_logged,
    refine_snapshot_progressive, refine_snapshot_until, MAX_THREADS,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    env, fs,
    path::Path,
    process::Command,
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

const USAGE: &str = "Usage: bench --graph GRAPH.json [--limit 100] [--reps 5] [--threads 1,2,4|none] [--name NAME] [--cancel-after-ms 1,5,20] [--candidate-log DIR] [--refine-options JSON] [--layout-only 1] [--pipeline-only 1] [--abstract-plan 1] [--abstract-realization 1] [--routing-completion 1] [--routing-planning 1] [--regional-routing 1] [--regional-repair 1] [--regional-construction 1] [--regional-coordination 1]";

struct Args {
    graph: String,
    limit: usize,
    reps: usize,
    threads: Vec<usize>,
    name: String,
    /// Cancellation probes: the stop signal fires this many milliseconds after the refinement starts.
    cancel_after_ms: Vec<u64>,
    /// A preset file whose configuration set is applied before measuring; adds the work counters to the output.
    preset: Option<String>,
    /// A JSON object of layout options merged over the defaults (for example `{"routingEngine":"channel"}`).
    options: Option<String>,
    /// Options applied only to refinement, preserving the imported snapshot geometry for paired experiments.
    refine_options: Option<String>,
    /// Options given to the import itself, as the import operation takes them (needed by options that change the first layout
    /// and its size bound, such as `sparseStress`).
    import_options: Option<String>,
    /// Publication timeline: time of each published improvement from the start of one refinement (flag value 1).
    progress: bool,
    /// Writes the candidate log of the view (one line per candidate, geometry of new candidates) into this directory.
    candidate_log: Option<String>,
    /// Reports the constructive layered routes of the import instead of refining (flag value 1; plan section 4a).
    constructive: bool,
    /// Reports the repair search work per edge of the refined layout (flag value 1; diagnostic).
    repair_work: bool,
    /// Times layout computation directly on the supplied graph, without view selection or process/JSON transport.
    layout_only: bool,
    /// Omit edit/interaction probes, which may require unsupported capabilities.
    pipeline_only: bool,
    /// Builds a geometry-free shadow plan outside the engine clocks, including candidate logs.
    abstract_plan: bool,
    /// Also realizes physical channel lanes without applying the resulting trial routes.
    abstract_realization: bool,
    /// Detached full routing/scene comparison on the same grown import geometry.
    routing_completion: bool,
    routing_planning: bool,
    regional_routing: bool,
    regional_repair: bool,
    regional_construction: bool,
    regional_coordination: bool,
}

fn parse() -> Result<Args, String> {
    let mut args = Args {
        graph: String::new(),
        limit: 100,
        reps: 5,
        threads: vec![1],
        name: String::new(),
        cancel_after_ms: Vec::new(),
        preset: None,
        options: None,
        refine_options: None,
        import_options: None,
        progress: false,
        candidate_log: None,
        constructive: false,
        repair_work: false,
        layout_only: false,
        pipeline_only: false,
        abstract_plan: false,
        abstract_realization: false,
        routing_completion: false,
        routing_planning: false,
        regional_routing: false,
        regional_repair: false,
        regional_construction: false,
        regional_coordination: false,
    };
    let mut input = env::args().skip(1);
    while let Some(flag) = input.next() {
        let value = input.next().ok_or(format!("Missing value for {flag}"))?;
        match flag.as_str() {
            "--graph" => args.graph = value,
            "--limit" => args.limit = value.parse().map_err(|_| "Invalid --limit")?,
            "--reps" => args.reps = value.parse().map_err(|_| "Invalid --reps")?,
            "--threads" if value == "none" => args.threads.clear(),
            "--threads" => {
                args.threads = value
                    .split(',')
                    .map(|t| t.parse::<usize>().map(|t| t.clamp(1, MAX_THREADS)))
                    .collect::<Result<_, _>>()
                    .map_err(|_| "Invalid --threads")?
            }
            "--name" => args.name = value,
            "--preset" => args.preset = Some(value),
            "--options" => args.options = Some(value),
            "--refine-options" => args.refine_options = Some(value),
            "--import-options" => args.import_options = Some(value),
            "--progress" => args.progress = value == "1",
            "--constructive" => args.constructive = value == "1",
            "--repair-work" => args.repair_work = value == "1",
            "--layout-only" => args.layout_only = value == "1",
            "--pipeline-only" => args.pipeline_only = value == "1",
            "--abstract-plan" => args.abstract_plan = value == "1",
            "--abstract-realization" => args.abstract_realization = value == "1",
            "--routing-completion" => args.routing_completion = value == "1",
            "--routing-planning" => args.routing_planning = value == "1",
            "--regional-routing" => args.regional_routing = value == "1",
            "--regional-repair" => args.regional_repair = value == "1",
            "--regional-construction" => args.regional_construction = value == "1",
            "--regional-coordination" => args.regional_coordination = value == "1",
            "--candidate-log" => args.candidate_log = Some(value),
            "--cancel-after-ms" => {
                args.cancel_after_ms = value
                    .split(',')
                    .map(|t| t.parse::<u64>())
                    .collect::<Result<_, _>>()
                    .map_err(|_| "Invalid --cancel-after-ms")?
            }
            _ => return Err(format!("Unknown flag {flag}\n{USAGE}")),
        }
    }
    if args.graph.is_empty() || args.reps == 0 {
        return Err(USAGE.into());
    }
    if usize::from(args.regional_repair)
        + usize::from(args.regional_construction)
        + usize::from(args.regional_coordination)
        > 1
    {
        return Err(
            "Select only one of --regional-repair, --regional-construction and --regional-coordination to isolate reuse costs"
                .into(),
        );
    }
    args.regional_routing |=
        args.regional_repair || args.regional_construction || args.regional_coordination;
    if args.regional_routing && args.routing_planning {
        return Err("Select either --regional-routing or --routing-planning".into());
    }
    args.routing_completion |= args.routing_planning || args.regional_routing;
    args.abstract_plan |= args.abstract_realization;
    if args.abstract_plan && args.layout_only {
        return Err("--abstract-plan requires snapshot mode, not --layout-only".into());
    }
    if args.routing_completion
        && (args.layout_only
            || args.constructive
            || args.repair_work
            || args.abstract_plan
            || args.candidate_log.is_some()
            || args.progress)
    {
        return Err("--routing-completion is a separate diagnostic mode".into());
    }
    if args.routing_completion && !cfg!(feature = "benchmark-timings") {
        return Err(
            "--routing-completion requires --features benchmark-timings for phase measurement"
                .into(),
        );
    }
    if args.name.is_empty() {
        args.name = args.graph.clone();
    }
    Ok(args)
}

fn round(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

fn planning_work(
    work: &arqlet_layout::constructive::abstract_realization::planning::Work,
) -> Value {
    json!({
        "ordinalComputations":work.ordinal_computations,"ordinalReuses":work.ordinal_reuses,
        "laneComputations":work.lane_computations,"laneReuses":work.lane_reuses,
        "channelComputations":work.channel_computations,"channelReuses":work.channel_reuses,
        "preferredChangesBefore":work.preferred_changes_before,"preferredChangesAfter":work.preferred_changes_after,
    })
}

fn completion_phases(
    t: &arqlet_layout::constructive::abstract_realization::completion::Times,
) -> [f64; 8] {
    [
        t.abstract_plan,
        t.growth,
        t.construction,
        t.seeds,
        t.fallback,
        t.track_repair,
        t.finalization,
        t.evaluation,
    ]
    .map(|n| n as f64 / 1_000_000.)
}

fn construction_reuse_work(
    w: &arqlet_layout::constructive::abstract_realization::construction::Work,
) -> Value {
    json!({
        "obstaclePreparationsAvoided":w.obstacle_preparations_avoided,"sharedIndexes":w.shared_indexes,
        "geometriesComputed":w.geometries_computed,"geometriesReused":w.geometries_reused,
        "obstacleDecisionsComputed":w.obstacle_decisions_computed,"obstacleDecisionsReused":w.obstacle_decisions_reused,
        "obstacleTests":w.obstacle_tests,"occupancyChecks":w.occupancy_checks,
    })
}

fn coordination_work(
    w: &arqlet_layout::constructive::abstract_realization::coordination::Work,
) -> Value {
    json!({"spanComputations":w.span_computations,"spanReuses":w.span_reuses,
        "domainComputations":w.domain_computations,"domainReuses":w.domain_reuses})
}

fn construction_details(
    drawing: &arqlet_layout::constructive::abstract_realization::completion::CompletedDrawing,
) -> Result<[f64; 4], String> {
    let t = drawing
        .construction_times
        .as_ref()
        .ok_or("Missing detailed constructor phase times")?;
    Ok([
        t.edge_preparation,
        t.channels,
        t.coordination,
        t.materialization,
    ]
    .map(|n| n as f64 / 1_000_000.))
}

fn construction_detail_stats(samples: &[[f64; 4]]) -> Value {
    json!([
        "edgePreparation",
        "channels",
        "coordination",
        "materialization"
    ]
    .iter()
    .enumerate()
    .map(|(i, name)| (
        (*name).to_string(),
        stats(samples.iter().map(|s| s[i]).collect())
    ))
    .collect::<serde_json::Map<String, Value>>())
}

fn completion_variant(
    base: &Snapshot,
    drawing: &arqlet_layout::constructive::abstract_realization::completion::CompletedDrawing,
    samples: &[f64],
    phase_samples: &[[f64; 8]],
) -> Result<Value, String> {
    let mut phases = serde_json::Map::new();
    for (index, label) in [
        "abstractPlan",
        "growth",
        "construction",
        "seeds",
        "fallback",
        "trackRepair",
        "finalization",
        "evaluation",
    ]
    .iter()
    .enumerate()
    {
        phases.insert(
            label.to_string(),
            stats(phase_samples.iter().map(|s| s[index]).collect()),
        );
    }
    Ok(json!({
        "totalMs":stats(samples.to_vec()),
        "samplesMs":samples.iter().map(|v| round(*v)).collect::<Vec<_>>(),
        "phaseMs":phases,
        "phaseSamplesMs":phase_samples.iter().map(|s| s.map(round)).collect::<Vec<_>>(),
        "sceneFingerprint":fingerprint(&serde_json::to_vec(&drawing.scene).map_err(|e| e.to_string())?),
        "constructed":drawing.fixed.len(),"delegated":drawing.delegated,
        "fixedRoutes":drawing.fixed,"routes":drawing.routes,
        "metrics":drawing.metrics,"key":drawing.key,"hardViolations":drawing.hard_violations,
        "trackConflicts":drawing.track_conflicts,"trackSpacing":base.options.track_spacing,
        "hardMetricValid":drawing.hard_violations == 0,
        "strictTrackValid":drawing.track_conflicts.is_empty(),
        "eligible":completion_eligible(drawing),
        "geometryIssues":drawing.geometry_issues,"routingIssues":drawing.routing_issues,
        "scene":drawing.scene,
        "work":{
            "construction":construction_json(&drawing.search.construction),
            "legacyCandidates":drawing.search.legacy_candidates,
            "legacyGridPops":drawing.search.legacy_grid_pops,
            "legacyGridPopsByRoute":drawing.search.legacy_grid_pops_by_route,
            "windowedGrids":drawing.search.windowed_grids,
            "planning":drawing.planning.as_ref().map(planning_work),
            "constructionReuse":drawing.construction_reuse.as_ref().map(construction_reuse_work),
            "coordination":drawing.coordination_work.as_ref().map(coordination_work),
            "trackRepair":drawing.track_repair.as_ref().map(|w| json!({
                "candidates":w.candidates,"moves":w.moves,"passes":w.passes,
                "segmentProjectionsComputed":w.segment_projections_computed,
                "projectionPairChecks":w.projection_pair_checks,
                "initialPairs":w.initial_pairs,"remainingPairs":w.remaining_pairs,
                "decisionsComputed":w.decisions_computed,"decisionsReused":w.decisions_reused,
                "candidatesAvoided":w.candidates_avoided,
            })),
        },
    }))
}

fn completion_report(base: &Snapshot, reps: usize, prepared: bool) -> Result<Value, String> {
    use arqlet_layout::constructive::abstract_realization::completion::{self, Method};
    use arqlet_layout::constructive::abstract_realization::planning::Session;
    let before = serde_json::to_vec(base).map_err(|e| e.to_string())?;
    let names = [
        "existing",
        "chains",
        "chainsFixedTracks",
        "chainsAligned",
        "chainsAlignedCached",
    ];
    let n = if prepared { 5 } else { 3 };
    let mut samples = vec![Vec::new(); n];
    let mut phase_samples = vec![Vec::new(); n];
    let mut signatures = vec![None; n];
    let mut outputs: Vec<_> = (0..n).map(|_| None).collect();
    let mut session = Session::default();
    // Warm up each once; rotate and reverse measured order to reduce fixed-order bias.
    for round_index in 0..=reps {
        let mut order: Vec<_> = (0..n).collect();
        order.rotate_left(round_index % n);
        if round_index % 2 == 1 {
            order.reverse();
        }
        for i in order {
            let started = Instant::now();
            let drawing = if i == 4 {
                completion::run_in_session(base, &mut session)?
            } else {
                completion::run(
                    base,
                    [
                        Method::Existing,
                        Method::Chains,
                        Method::ChainsFixedTracks,
                        Method::ChainsAligned,
                    ][i],
                )?
            };
            let elapsed = started.elapsed().as_secs_f64() * 1000.;
            let geometry = serde_json::to_vec(&drawing.scene).map_err(|e| e.to_string())?;
            let signature = fingerprint(&geometry);
            if signatures[i].as_ref().is_some_and(|s| s != &signature) {
                return Err("Completion geometry differs between repetitions".into());
            }
            signatures[i] = Some(signature);
            if round_index > 0 {
                samples[i].push(elapsed);
                phase_samples[i].push(completion_phases(&drawing.times));
            }
            outputs[i] = Some(drawing);
        }
    }
    if before != serde_json::to_vec(base).map_err(|e| e.to_string())? {
        return Err("Completion diagnostic mutated its input snapshot".into());
    }
    let mut variants = serde_json::Map::new();
    for (i, label) in names[..n].iter().enumerate() {
        let drawing = outputs[i].take().ok_or("Missing completion output")?;
        variants.insert(
            label.to_string(),
            completion_variant(base, &drawing, &samples[i], &phase_samples[i])?,
        );
    }
    Ok(json!({
        "kind": "detached-routing-completion",
        "applied": false, "inputUnchanged": true, "reps": reps,
        "environment": environment(),
        "note": "Same input ranks/ports. First three variants use identical grown geometry; aligned variants choose ordinal order and common cross-lane preferences before final flow growth, so may request different widths. Existing constructor may move automatic ports. All use the same legacy fallback; fixedTracks/aligned variants add final track repair. AlignedCached retains exact run-local planning entries across repetitions; aligned starts cold. No cached routes, obstacle decisions or scores. All decorate labels/build scenes/evaluate shared metrics and strict spacing. One warmup each, rotating/reversed measured order. Includes planning/growth/fallback/repair/scene/metrics; excludes prior placement/import, process/JSON and actual refinement. No candidate acceptance or production speedup claim.",
        "inputOptions": base.options,
        "inputSnapshotFingerprint": fingerprint(&before),
        "inputGeometryFingerprint": fingerprint(&serde_json::to_vec(&(&base.boxes, &base.routes)).map_err(|e| e.to_string())?),
        "inputKey": arqlet_layout::published_key(base)?,
        "inputMetrics": arqlet_layout::published_metrics(base)?,
        "variants": variants,
        "planningCandidateSequence": if prepared {Some(planning_sequence(base)?)} else {None},
    }))
}

fn completion_eligible(
    drawing: &arqlet_layout::constructive::abstract_realization::completion::CompletedDrawing,
) -> bool {
    drawing.hard_violations == 0
        && drawing.track_conflicts.is_empty()
        && drawing.geometry_issues.is_empty()
        && drawing.routing_issues.is_empty()
}

fn regional_report(
    base: &Snapshot,
    reps: usize,
    repair_reuse: bool,
    construction_reuse: bool,
    coordination_reuse: bool,
) -> Result<Value, String> {
    use arqlet_layout::constructive::abstract_realization::{completion, planning, regional};
    let before = serde_json::to_vec(base).map_err(|e| e.to_string())?;
    let available = regional::swaps(base)?;
    // Explicit diagnostic sample: first move in each non-root area, up to eight.
    // This does not replace or prune the production refinement portfolio.
    let mut areas = BTreeSet::new();
    let selected: Vec<_> = available
        .iter()
        .filter(|s| s.area != "<root>" && areas.insert(s.area.clone()))
        .take(8)
        .collect();
    if selected.is_empty() {
        return Err("Regional benchmark requires a non-root real-sibling swap".into());
    }
    let n = selected.len() + 1;
    let mut samples = vec![Vec::new(); n];
    let mut phases = vec![Vec::new(); n];
    let mut cold_samples = vec![Vec::new(); n];
    let mut placement_samples = vec![Vec::new(); n];
    let mut signatures = vec![None; n];
    let mut outputs: Vec<_> = (0..n).map(|_| None).collect();
    let mut placements = vec![Value::Null; n];
    let mut planning_samples = vec![Vec::new(); n];
    let detailed_construction = construction_reuse || coordination_reuse;
    let mut session = if coordination_reuse {
        planning::Session::with_coordination_reuse()
    } else if construction_reuse {
        planning::Session::with_construction_reuse()
    } else if repair_reuse {
        planning::Session::with_repair_reuse()
    } else {
        planning::Session::default()
    };
    let initial_started = Instant::now();
    let primed = completion::run_in_session(base, &mut session)?;
    let initial_ms = initial_started.elapsed().as_secs_f64() * 1000.;
    let mut sequence_samples = Vec::new();
    let mut cold_repair = vec![Value::Null; n];
    let mut cold_phases = vec![Vec::new(); n];
    let mut shared_samples = vec![Vec::new(); n];
    let mut shared_phases = vec![Vec::new(); n];
    let mut shared_work = vec![Value::Null; n];
    let mut cold_coordination = vec![Value::Null; n];
    let mut shared_coordination = vec![Value::Null; n];
    let mut coordination_samples = vec![Vec::<Value>::new(); n];
    let mut detail_samples = vec![vec![Vec::<[f64; 4]>::new(); n]; 3];
    for round_index in 0..=reps {
        let sequence_started = Instant::now();
        let mut order: Vec<_> = (0..n).collect();
        order.rotate_left(round_index % n);
        if round_index % 2 == 1 {
            order.reverse();
        }
        for i in order {
            let mut cached = None;
            let mut cold = None;
            let mut shared_cold = None;
            // Alternating cold/cached order; both include fresh local placement.
            let arms = if detailed_construction {
                let mut arms = vec![0, 1, 2];
                arms.rotate_left(round_index % 3);
                if round_index % 2 == 1 {
                    arms.reverse();
                }
                arms
            } else if round_index % 2 == 0 {
                vec![1, 0]
            } else {
                vec![0, 1]
            };
            for arm in arms {
                let warm = arm == 1;
                let started = Instant::now();
                let placement = if i == 0 {
                    None
                } else {
                    Some(regional::prepare(base, selected[i - 1])?)
                };
                let placement_ms = started.elapsed().as_secs_f64() * 1000.;
                let mut branch = if warm {
                    session.clone()
                } else if arm == 2 && coordination_reuse {
                    planning::Session::with_coordination_reuse()
                } else if arm == 2 || coordination_reuse {
                    planning::Session::with_construction_reuse()
                } else {
                    planning::Session::default()
                };
                let drawing = match &placement {
                    Some(p) => p.complete(&mut branch)?,
                    None => completion::run_in_session(base, &mut branch)?,
                };
                let elapsed = started.elapsed().as_secs_f64() * 1000.;
                if detailed_construction && round_index > 0 {
                    detail_samples[arm][i].push(construction_details(&drawing)?);
                }
                if warm {
                    if round_index > 0 {
                        samples[i].push(elapsed);
                        placement_samples[i].push(placement_ms);
                        phases[i].push(completion_phases(&drawing.times));
                        planning_samples[i].push(drawing.planning.as_ref().map(planning_work));
                        if coordination_reuse {
                            coordination_samples[i].push(coordination_work(
                                drawing
                                    .coordination_work
                                    .as_ref()
                                    .ok_or("Missing forked coordination counters")?,
                            ));
                        }
                    }
                    if let Some(p) = placement {
                        placements[i] = json!({
                            "swap":{"area":selected[i-1].area,"rank":selected[i-1].rank,"left":selected[i-1].left},
                            "placedAreas":p.placed_areas,"propagatedAreas":p.propagated_areas,"changedBoxes":p.changed_boxes,
                            "movedAbsolute":p.moved_absolute,"changedPlans":p.changed_plans,
                            "changedPorts":p.changed_ports,
                            "orderCrossingsBefore":p.crossings_before,"orderCrossingsAfter":p.crossings_after,
                            "regionRealizations":p.work.region_realizations,
                            "boxes":p.boxes(),"plans":p.plans(),"ports":p.ports(),
                        });
                    }
                    cached = Some(drawing);
                } else if arm == 2 {
                    if round_index > 0 {
                        shared_samples[i].push(elapsed);
                        shared_phases[i].push(completion_phases(&drawing.times));
                    }
                    shared_work[i] = construction_reuse_work(
                        drawing
                            .construction_reuse
                            .as_ref()
                            .ok_or("Missing cold shared-index construction counters")?,
                    );
                    shared_coordination[i] = drawing
                        .coordination_work
                        .as_ref()
                        .map(coordination_work)
                        .ok_or("Missing shared coordination counters")?;
                    shared_cold = Some(drawing);
                } else {
                    if round_index > 0 {
                        cold_samples[i].push(elapsed);
                        cold_phases[i].push(completion_phases(&drawing.times));
                    }
                    cold_repair[i] = json!(drawing.track_repair.as_ref().map(|w| json!({
                        "candidates":w.candidates,"moves":w.moves,"passes":w.passes,
                        "segmentProjectionsComputed":w.segment_projections_computed,
                        "projectionPairChecks":w.projection_pair_checks,
                        "initialPairs":w.initial_pairs,"remainingPairs":w.remaining_pairs,
                        "decisionsComputed":w.decisions_computed,"decisionsReused":w.decisions_reused,
                        "candidatesAvoided":w.candidates_avoided,
                    })));
                    cold_coordination[i] = drawing
                        .coordination_work
                        .as_ref()
                        .map(coordination_work)
                        .ok_or("Missing cold coordination counters")?;
                    cold = Some(drawing);
                }
            }
            let drawing = cached.ok_or("Missing regional cached drawing")?;
            let cold = cold.ok_or("Missing regional cold drawing")?;
            let bytes = serde_json::to_vec(&drawing.scene).map_err(|e| e.to_string())?;
            if detailed_construction {
                let shared = shared_cold.ok_or("Missing cold shared-index drawing")?;
                for other in [&cold, &shared] {
                    let mut a = drawing.search.construction.clone();
                    let mut b = other.search.construction.clone();
                    a.preparation_nanos = 0;
                    b.preparation_nanos = 0;
                    a.search_nanos = 0;
                    b.search_nanos = 0;
                    if a != b
                        || drawing.delegated != other.delegated
                        || serde_json::to_vec(&drawing.fixed).map_err(|e| e.to_string())?
                            != serde_json::to_vec(&other.fixed).map_err(|e| e.to_string())?
                        || bytes != serde_json::to_vec(&other.scene).map_err(|e| e.to_string())?
                        || drawing.key != other.key
                        || drawing.track_conflicts != other.track_conflicts
                    {
                        return Err(format!(
                            "Regional trial {i} differs between eager/shared/forked construction"
                        ));
                    }
                }
            }
            if bytes != serde_json::to_vec(&cold.scene).map_err(|e| e.to_string())?
                || drawing.key != cold.key
                || drawing.track_conflicts != cold.track_conflicts
            {
                return Err(format!(
                    "Regional trial {i} differs between cold and cached"
                ));
            }
            let signature = fingerprint(&bytes);
            if signatures[i].as_ref().is_some_and(|s| s != &signature) {
                return Err(format!("Regional trial {i} differs between repetitions"));
            }
            signatures[i] = Some(signature);
            outputs[i] = Some(drawing);
        }
        if round_index > 0 {
            sequence_samples.push(sequence_started.elapsed().as_secs_f64() * 1000.);
        }
    }
    let mut variants = serde_json::Map::new();
    for i in 0..n {
        let drawing = outputs[i].take().ok_or("Missing regional output")?;
        let mut variant = completion_variant(base, &drawing, &samples[i], &phases[i])?;
        let v = variant
            .as_object_mut()
            .ok_or("Invalid completion variant")?;
        v.insert("placement".into(), placements[i].clone());
        v.insert("placementMs".into(), stats(placement_samples[i].clone()));
        v.insert("coldTotalMs".into(), stats(cold_samples[i].clone()));
        v.insert("coldTrackRepair".into(), cold_repair[i].clone());
        v.insert(
            "coldTrackRepairMs".into(),
            stats(cold_phases[i].iter().map(|p| p[5]).collect()),
        );
        v.insert("planningSamples".into(), json!(planning_samples[i]));
        v.insert("coldCachedIdentical".into(), json!(true));
        if coordination_reuse {
            v.insert("coordinationSamples".into(), json!(coordination_samples[i]));
        }
        if detailed_construction {
            for (arm, label) in ["cold", "forked", "coldShared"].iter().enumerate() {
                v.insert(
                    format!("{label}ConstructionDetailMs"),
                    construction_detail_stats(&detail_samples[arm][i]),
                );
                v.insert(
                    format!("{label}ConstructionDetailSamplesMs"),
                    json!(detail_samples[arm][i]
                        .iter()
                        .map(|s| s.map(round))
                        .collect::<Vec<_>>()),
                );
            }
            v.insert("coldSharedTotalMs".into(), stats(shared_samples[i].clone()));
            v.insert(
                "coldSharedConstructionMs".into(),
                stats(shared_phases[i].iter().map(|p| p[2]).collect()),
            );
            v.insert(
                "coldConstructionMs".into(),
                stats(cold_phases[i].iter().map(|p| p[2]).collect()),
            );
            v.insert("coldSharedWork".into(), shared_work[i].clone());
            v.insert("coldSharedIdentical".into(), json!(true));
            v.insert("coldCoordination".into(), cold_coordination[i].clone());
            v.insert(
                "coldSharedCoordination".into(),
                shared_coordination[i].clone(),
            );
            v.insert(
                "coldConstructionSamplesMs".into(),
                json!(cold_phases[i]
                    .iter()
                    .map(|p| round(p[2]))
                    .collect::<Vec<_>>()),
            );
            v.insert(
                "coldSharedConstructionSamplesMs".into(),
                json!(shared_phases[i]
                    .iter()
                    .map(|p| round(p[2]))
                    .collect::<Vec<_>>()),
            );
        }
        v.insert(
            "diagnosticImprovement".into(),
            json!(
                completion_eligible(&drawing)
                    && (!completion_eligible(&primed)
                        || arqlet_layout::improves(&drawing.key, &primed.key))
            ),
        );
        variants.insert(
            if i == 0 {
                "baseline".into()
            } else {
                format!("regional{i}")
            },
            variant,
        );
    }
    if before != serde_json::to_vec(base).map_err(|e| e.to_string())? {
        return Err("Regional benchmark mutated source snapshot".into());
    }
    Ok(json!({
        "kind":"detached-regional-routing","applied":false,"inputUnchanged":true,
        "reps":reps,"environment":environment(),"inputOptions":base.options,
        "inputSnapshotFingerprint":fingerprint(&before),
        "inputGeometryFingerprint":fingerprint(&serde_json::to_vec(&(&base.boxes,&base.routes)).map_err(|e| e.to_string())?),
        "sourceBoxes":base.boxes,"sourcePlans":base.layered_plans,"sourcePorts":base.resolved_ports,
        "availableSwaps":available.len(),"diagnosticBudget":8,"evaluatedSwaps":selected.len(),
        "selection":"First adjacent real-sibling swap in each non-root area in stable area/rank order, at most eight. No proxy pruning, no production portfolio change.",
        "note":"One warmup, rotating/reversed candidate order and alternating cold/cached measurements (three rotated arms in the construction/coordination controls). Each candidate starts from the same immutable input and forks the base planning session, not the preceding trial's keys; cloning cost is included. Only its target frame is placed; positive size changes push dependent siblings/ancestors using the existing growth helper, without shrinking frames or reordering ancestors. Changed border dependencies are routed afresh. Totals include local placement/propagation and complete global growth/routing/repair/labels/scoring; exclude import and transport. No general route or score cache; optional fixed-path geometry/clearance reuse, within-drawing coordination projection sharing or exact fixed-track decision reuse is disclosed separately. Cold/cached scene/key/conflict identity checked; full hard/track/geometry/routing eligibility required for diagnostic improvement. No publication or production refinement change.",
        "initialBaselineMs":round(initial_ms),
        "repairReuse":repair_reuse,
        "constructionReuse":construction_reuse,
        "coordinationReuse":coordination_reuse,
        "coordinationControl":if coordination_reuse {Some("Three rotated/reversed arms: previous shared-construction path with fresh caches and uncached coordination geometry; fresh shared-construction path with a within-drawing coordination context; and a fork of the primed construction/planning session with a NEW coordination context. Thus cold versus shared-cold isolates within-run projection sharing, not eager obstacle preparation. All arms rerun every chain probe, joint/continuity solver, fallback, final repair and score. Complete scenes/fixed routes/delegations/keys/conflicts and deterministic constructor counters match. No final repair decision reuse.")} else {None},
        "coordinationReuseContract":if coordination_reuse {Some("A context borrows exactly one immutable grown geometry, lazily memoizing channel main spans by area/rank and junction rectangles by area/element across the chain probe and final solver passes. It caches exact successful values or explicit failures. Endpoint lane domains and orientation projection remain fresh. Contexts are discarded before any placement/growth/plan change and NEVER survive between candidates or in planning session forks; no stale geometric cache is accepted. Constraint discovery, sorting, tie-breaking, chain cuts, compaction, failure/witness reporting and route acceptance are unchanged. Counters separate actual projections and within-run reuse, not solved regions or skipped candidates.")} else {None},
        "constructionControl":if construction_reuse {Some("Three rotated/reversed arms per trial: original eager cold construction, new shared-index path with empty caches, and a fork of the primed shared-index session. All include fresh placement; only the fork includes cloning. Complete scenes, fixed routes, delegation, keys, conflicts and deterministic constructor counters match. Fresh control separates shared preparation from warm geometry/clearance/planning reuse; final repair reuse is disabled.")} else {None},
        "constructionReuseContract":if construction_reuse {Some("Fixed-path endpoint/flow preparation omits per-edge search obstacles and indexes. One fresh inflated-box index is shared by the final construction. Exact start/end, orientation, logical reversal and ordered corners key each path geometry. Fresh segment-box queries collect all potentially intersecting non-excluded inflated obstacles; exact identity/rectangle keys gate clearance-decision reuse, including prior obstacle rejection. Endpoint/ancestor exclusions are refreshed. Track acceptance always scans current assigned routes in graph order, never replays acceptance. Coordination, growth, fallback, final repair and scoring remain fresh. Cold path is the original eager preparation, not the new path with an empty cache; totals combine preparation sharing, exact cache reuse and planning reuse.")} else {None},
        "repairReuseContract":if repair_reuse {Some("Individual fixed-track decisions replay only after the exact current route, parameters, endpoints, ordered admissible proposal operations and all candidate-influencing occupancy/outline dependencies match. Events and corridor bounds are freshly generated, including remote projected tracks and limiters; only unchanged actual proposal operations may reuse. Indexed influence covers current and all geometrically valid candidate segments, conservatively including tiny and near-orthogonal segments. Both successful and unsuccessful bounded decisions may replay. Global validation, initial/final pairs, fallback, labels and scoring stay fresh. No general route, path-search or score reuse.")} else {None},
        "pairedSequenceMs":stats(sequence_samples),
        "variants":variants,
    }))
}

fn planning_sequence(base: &Snapshot) -> Result<Value, String> {
    use arqlet_layout::constructive::abstract_realization::{completion, planning};
    let mut session = planning::Session::default();
    let mut rows = Vec::new();
    for mode in [0, 1, 2, 3, 4, 0] {
        let candidate = if mode == 0 {
            base.clone()
        } else {
            arqlet_layout::candidate(&base.graph, mode, &base.options)?
        };
        let started = Instant::now();
        let cached = completion::run_in_session(&candidate, &mut session)?;
        let cached_ms = started.elapsed().as_secs_f64() * 1000.;
        let started = Instant::now();
        let cold = completion::run(&candidate, completion::Method::ChainsAligned)?;
        let cold_ms = started.elapsed().as_secs_f64() * 1000.;
        let baseline = completion::run(&candidate, completion::Method::ChainsFixedTracks)?;
        let eligible = completion_eligible(&cached);
        let baseline_eligible = completion_eligible(&baseline);
        let selected = if eligible
            && (!baseline_eligible || arqlet_layout::improves(&cached.key, &baseline.key))
        {
            Some("aligned")
        } else if baseline_eligible {
            Some("baseline")
        } else {
            None
        };
        let bytes = serde_json::to_vec(&cached.scene).map_err(|e| e.to_string())?;
        if bytes != serde_json::to_vec(&cold.scene).map_err(|e| e.to_string())?
            || cached.key != cold.key
            || cached.track_conflicts != cold.track_conflicts
        {
            return Err(format!(
                "Prepared candidate {mode} differs between cold and cached evaluation"
            ));
        }
        rows.push(json!({
            "mode":mode,"sceneFingerprint":fingerprint(&bytes),
            "cachedMs":round(cached_ms),"coldMs":round(cold_ms),
            "planning":cached.planning.as_ref().map(planning_work),
            "coldPlanning":cold.planning.as_ref().map(planning_work),
            "constructed":cached.fixed.len(),"delegated":cached.delegated.len(),
            "hardViolations":cached.hard_violations,"trackConflicts":cached.track_conflicts.len(),
            "metrics":cached.metrics,"key":cached.key,"coldCachedIdentical":true,
            "eligible":eligible,
            "baseline":{"eligible":baseline_eligible,"hardViolations":baseline.hard_violations,
                "trackConflicts":baseline.track_conflicts.len(),"key":baseline.key,"metrics":baseline.metrics},
            "diagnosticSelection":selected,
        }));
    }
    Ok(json!({
        "note":"Separate sequential diagnostic, not refinement timing or runtime acceptance. Each snapshot freshly validates topology/geometry; only exact ordinal assignments, lane domains and ordered cross spans reuse. Cold/cached scene/key/conflict identity checked. Public candidate placement is outside these times; routing, obstacle checks, fallback, labels and scoring still rerun globally. Diagnostic selection requires full hard/geometry/routing/spacing eligibility, then published-key improvement; null means neither strategy passed, not infeasibility. No heuristic proxy discards candidates.",
        "rows":rows,
    }))
}

fn abstract_plan_json(snapshot: &Snapshot, realize: bool) -> Result<Value, String> {
    let plans = snapshot
        .layered_plans
        .iter()
        .map(|(area, stored)| {
            arqlet_layout::layered_plan::AreaPlan::from_stored(stored)
                .map(|p| (area.clone(), p))
                .ok_or_else(|| format!("Unsupported stored plan format for {area}"))
        })
        .collect::<Result<_, _>>()?;
    let started = Instant::now();
    let plan = arqlet_layout::abstract_routing::build(&snapshot.graph, &plans)?;
    let elapsed = started.elapsed().as_secs_f64() * 1000.;
    let s = &plan.summary;
    let mut out = json!({
        "format": 1,
        "stage": "shadow-from-ordered-ranks",
        "safeToPrune": plan.safe_to_prune,
        "note": "Ordinal topology/slot estimates only; no physical clearance, area, label or routability proof. Computed outside layout/refinement timing.",
        "analysisMs": round(elapsed),
        "summary": {
            "edges": s.edges, "plannedEdges": s.planned_edges, "unsupportedEdges": s.unsupported_edges,
            "sections": s.sections, "channels": s.channels, "tracks": s.tracks,
            "maxTracks": s.max_tracks, "crossings": s.crossings,
            "dependencies": s.dependencies, "brokenDependencies": s.broken_dependencies,
        },
        "edges": plan.edges.iter().map(|e| json!({
            "edge": e.edge, "source": e.source, "target": e.target, "reversed": e.reversed,
            "unsupported": e.unsupported.as_ref().map(|r| format!("{r:?}")),
            "boundaries": e.boundaries.iter().map(|b| json!({
                "afterSection": b.after_section, "container": b.container,
                "border": b.border, "entering": b.entering,
            })).collect::<Vec<_>>(),
            "sections": e.sections.iter().map(|s| json!({
                "area": s.area, "channel": s.channel, "from": s.from, "to": s.to,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "channels": plan.channels.iter().map(|c| json!({
            "area": c.area, "rank": c.rank, "tracks": c.tracks, "crossings": c.crossings,
            "dependencies": c.dependencies, "brokenDependencies": c.broken_dependencies, "order": c.order,
            "assignments": c.assignments.iter().map(|a| json!({
                "edge": a.edge, "from": [a.from.element, a.from.port],
                "to": [a.to.element, a.to.port], "slot": a.slot,
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    if realize {
        use arqlet_layout::constructive::abstract_realization::{self, growth, Failure};
        let started = Instant::now();
        let boxes = arqlet_layout::absolute_all(&snapshot.graph, &snapshot.boxes);
        let report = abstract_realization::realize(
            &snapshot.graph,
            &boxes,
            &snapshot.resolved_ports,
            &plans,
            arqlet_layout::constructive_spacing(&snapshot.options),
        )?;
        let failure = |f: &Failure| match f {
            Failure::Structural(reason) => {
                json!({"kind": "structural", "reason": format!("{reason:?}")})
            }
            Failure::Preparation(reason) => {
                json!({"kind": "preparation", "reason": format!("{reason:?}")})
            }
            Failure::MissingGeometry { area, element } => {
                json!({"kind": "missingGeometry", "area": area, "element": element})
            }
            Failure::InvalidGeometry { area, element } => {
                json!({"kind": "invalidGeometry", "area": area, "element": element})
            }
            Failure::LaneCapacity {
                area,
                element,
                required,
                available,
            } => json!({
                "kind": "laneCapacity", "area": area, "element": element, "required": required, "available": available,
            }),
            Failure::LongitudinalCapacity {
                junction,
                required,
                available,
            } => json!({
                "kind": "longitudinalCapacity", "junction": junction,
                "required": required, "available": available,
            }),
            Failure::NarrowChannel { area, rank } => {
                json!({"kind": "narrowChannel", "area": area, "rank": rank})
            }
            Failure::JointOrder { section } => json!({"kind": "jointOrder", "section": section}),
            Failure::JointCapacity {
                section,
                required,
                available,
            } => json!({
                "kind": "jointCapacity", "section": section, "required": required, "available": available,
            }),
            Failure::NonMonotone => json!({"kind": "nonMonotone"}),
            Failure::Obstacle => json!({"kind": "obstacle"}),
            Failure::TrackConflict {
                other,
                along_flow,
                separation,
                overlap,
            } => json!({
                "kind": "trackConflict", "other": other, "alongFlow": along_flow,
                "separation": separation, "overlap": overlap,
            }),
        };
        let continuity_json = |c: &abstract_realization::ContinuitySummary| {
            json!({
                "reservations": c.reservations, "overlappingPairs": c.overlapping_pairs,
                "constraints": c.constraints, "crossAreaConstraints": c.cross_area_constraints,
                "shifted": c.shifted, "rejectedEdges": c.rejected_edges,
                "separationConstraints": c.separation_constraints,
                "lanes": c.lanes.iter().map(|r| json!({
                    "edge": r.edge, "junction": r.junction, "area": r.area,
                    "main": r.main, "bounds": r.bounds, "preferred": r.preferred,
                    "coordinate": r.coordinate,
                })).collect::<Vec<_>>(),
                "capacityWitnesses": c.capacity_witnesses.iter().map(|w| json!({
                    "edge": w.edge, "junction": w.junction, "chain": w.chain,
                    "lower": w.lower, "upper": w.upper, "required": w.required,
                    "deficit": w.deficit,
                })).collect::<Vec<_>>(),
            })
        };
        let joint_json = |j: &abstract_realization::JointSummary| {
            json!({
                "longitudinalPairs": j.longitudinal_pairs,
                "temporalConstraints": j.temporal_constraints, "unseparablePairs": j.unseparable_pairs,
                "temporalOrders": j.temporal_orders,
                "crossFlowConstraints": j.cross_flow_constraints, "blockedEdges": j.blocked_edges,
                "capacityEdges": j.capacity_edges, "shifted": j.shifted,
                "precedences": j.precedences,
                "bends": j.bends.iter().map(|b| json!({
                    "edge": b.edge, "section": b.section, "area": b.area, "rank": b.rank,
                    "bounds": b.bounds, "preferred": b.preferred, "coordinate": b.coordinate,
                })).collect::<Vec<_>>(),
            })
        };
        let physical_json = |report: &abstract_realization::Realization| {
            json!({
                "stage": "physical-trial-on-fixed-geometry",
                "applied": false,
                "note": "Search-free trial routes checked against constructor obstacle clearance and accepted occupancy. Not a complete layout; labels and final layout quality are not evaluated. Deficits are reported, not compressed or applied.",
                "realizedEdges": report.routes.len(),
                "delegatedEdges": report.delegated.len(),
                "routes": report.routes,
                "delegated": report.delegated.iter().map(|(edge, f)| json!({"edge": edge, "failure": failure(f)})).collect::<Vec<_>>(),
                "channels": report.channels.iter().map(|c| json!({
                    "area": c.area, "rank": c.rank, "available": c.available, "required": c.required,
                    "missing": c.missing, "tracks": c.tracks,
                    "assignments": c.assignments.iter().map(|(edge, slot, coordinate)| json!({
                        "edge": edge, "slot": slot, "coordinate": coordinate,
                    })).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
                "work": construction_json(&report.work),
                "continuity": report.continuity.as_ref().map(&continuity_json),
                "joint": report.joint.as_ref().map(&joint_json),
                "chains": report.chains.as_ref().map(|c| json!({
                    "planningPasses": c.planning_passes, "probeJoint": joint_json(&c.probe_joint),
                    "probeContinuity": continuity_json(&c.probe_continuity),
                    "orders": c.orders, "uncutWitnesses": c.uncut_witnesses,
                    "cuts": c.cuts.iter().map(|cut| json!({
                        "witness": cut.witness, "pair": cut.pair,
                        "slack": cut.slack, "displacement": cut.displacement,
                    })).collect::<Vec<_>>(),
                })),
                "spacing": {
                    "margin": arqlet_layout::constructive_spacing(&snapshot.options).margin,
                    "track": snapshot.options.track_spacing,
                },
            })
        };
        out["realization"] = physical_json(&report);
        out["realization"]["boxes"] = json!(boxes);
        out["realization"]["resolvedPorts"] = json!(snapshot.resolved_ports);
        out["realization"]["analysisMs"] = json!(round(started.elapsed().as_secs_f64() * 1000.));
        match growth::check_capability(&snapshot.graph, &plans, &snapshot.options) {
            Err(reason) => {
                for key in [
                    "grownRealization",
                    "coordinatedRealization",
                    "jointRealization",
                    "chainRealization",
                ] {
                    out[key] = json!({"unsupported": reason, "applied": false});
                }
            }
            Ok(()) => {
                let started = Instant::now();
                let grown = growth::widen(
                    &snapshot.graph,
                    &boxes,
                    &snapshot.resolved_ports,
                    &plans,
                    &snapshot.options,
                )?;
                let report = abstract_realization::realize(
                    &snapshot.graph,
                    &grown.boxes,
                    &grown.ports,
                    &grown.plans,
                    arqlet_layout::constructive_spacing(&snapshot.options),
                )?;
                let mut trial = physical_json(&report);
                trial["stage"] = json!("physical-trial-after-fixed-order-growth");
                trial["note"] = json!("Separate trial geometry: flow coordinates and container extents grown bottom-up; no placement, path search or candidate acceptance. Ranks/cross coordinates preserved; pins/fixed sizes/grid/extent bounds/flattening/mixed flow unsupported. Not applied to snapshot; labels/full layout quality not evaluated.");
                trial["analysisMs"] = json!(round(started.elapsed().as_secs_f64() * 1000.));
                trial["boxes"] = json!(grown.boxes);
                trial["vertical"] = json!(plans["<root>"].vertical);
                trial["resolvedPorts"] = json!(grown.ports);
                out["grownRealization"] = trial;
                let started = Instant::now();
                let report = abstract_realization::realize_coordinated(
                    &snapshot.graph,
                    &grown.boxes,
                    &grown.ports,
                    &grown.plans,
                    arqlet_layout::constructive_spacing(&snapshot.options),
                )?;
                let mut trial = physical_json(&report);
                trial["stage"] = json!("physical-trial-with-longitudinal-reservations");
                trial["note"] = json!("Global longitudinal reservations on fixed channel tracks across area boundaries; junction lanes bounded by existing dummy rectangles, endpoints fixed. Deterministic preferred-lane order, no path search or candidate pruning. Order-specific capacity failures are not unroutability proofs. Final clearance/occupancy checks remain mandatory. Not applied to snapshot.");
                trial["analysisMs"] = json!(round(started.elapsed().as_secs_f64() * 1000.));
                trial["boxes"] = json!(grown.boxes);
                trial["vertical"] = json!(plans["<root>"].vertical);
                trial["resolvedPorts"] = json!(grown.ports);
                out["coordinatedRealization"] = trial;
                let started = Instant::now();
                let report = abstract_realization::realize_joint(
                    &snapshot.graph,
                    &grown.boxes,
                    &grown.ports,
                    &grown.plans,
                    arqlet_layout::constructive_spacing(&snapshot.options),
                )?;
                let mut trial = physical_json(&report);
                trial["stage"] = json!("physical-trial-with-joint-bend-order");
                trial["note"] = json!("Selected endpoint/lane/bend order on existing node geometry. Bends compacted inside channel bounds before bounded lane assignment. No path search, port changes or candidate pruning; selected-order failures are not infeasibility proofs. Final route checks remain authoritative. Not applied to snapshot.");
                trial["analysisMs"] = json!(round(started.elapsed().as_secs_f64() * 1000.));
                trial["boxes"] = json!(grown.boxes);
                trial["vertical"] = json!(plans["<root>"].vertical);
                trial["resolvedPorts"] = json!(grown.ports);
                out["jointRealization"] = trial;
                let started = Instant::now();
                let report = abstract_realization::realize_chains(
                    &snapshot.graph,
                    &grown.boxes,
                    &grown.ports,
                    &grown.plans,
                    arqlet_layout::constructive_spacing(&snapshot.options),
                )?;
                let mut trial = physical_json(&report);
                trial["stage"] = json!("physical-trial-with-capacity-chain-cuts");
                trial["note"] = json!("Exactly two planning passes: probe joint bend/lane compaction, derive one feasible temporal cut per propagated capacity witness, then replan from original preferences. No route search, node/port changes or best-result selection. Cuts and failures are selected-order diagnostics, not infeasibility proofs. Final route checks remain authoritative. Not applied to snapshot.");
                trial["analysisMs"] = json!(round(started.elapsed().as_secs_f64() * 1000.));
                trial["boxes"] = json!(grown.boxes);
                trial["vertical"] = json!(plans["<root>"].vertical);
                trial["resolvedPorts"] = json!(grown.ports);
                out["chainRealization"] = trial;
            }
        }
    }
    Ok(out)
}

fn delegation_reasons(c: &arqlet_runtime::execution::WorkCounters) -> Value {
    #[cfg(feature = "benchmark-timings")]
    {
        json!(c.delegation_reasons)
    }
    #[cfg(not(feature = "benchmark-timings"))]
    {
        let _ = c;
        json!([])
    }
}

fn stats(mut samples: Vec<f64>) -> Value {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    let median = if n % 2 == 1 {
        samples[n / 2]
    } else {
        (samples[n / 2 - 1] + samples[n / 2]) / 2.0
    };
    json!({
        "medianMs": round(median),
        "minMs": round(samples[0]),
        "maxMs": round(samples[n - 1]),
        "samples": n,
    })
}

fn construction_json(c: &arqlet_layout::ConstructionCounters) -> Value {
    let ms = |nanos: u64| round(nanos as f64 / 1_000_000.0);
    let mut out = json!({
        "preparations": c.preparations,
        "preparedEdges": c.prepared_edges,
        "plannedAttempts": c.planned_attempts,
        "plannedObstacleFailures": c.planned_obstacle_failures,
        "plannedTrackFailures": c.planned_track_failures,
        "edgeSearches": c.edge_searches,
        "laneSearches": c.lane_searches,
        "searches": c.searches,
        "failedSearches": c.failed_searches,
        "states": c.states,
        "transitions": c.transitions,
        "preparationMs": ms(c.preparation_nanos),
        "searchMs": ms(c.search_nanos),
    });
    #[cfg(feature = "benchmark-timings")]
    {
        out["searchDetails"] = json!(c
            .search_details
            .iter()
            .map(|s| json!({
                "edge": s.edge, "phase": s.phase, "failed": s.failed,
                "states": s.states, "transitions": s.transitions,
            }))
            .collect::<Vec<_>>());
        out["plannedFailures"] = json!(c.planned_failures);
    }
    // Keep the diagnostic schema explicit even in builds without per-edge recording.
    out["searchDetailsRecorded"] = json!(cfg!(feature = "benchmark-timings"));
    out
}

fn work_json(c: &arqlet_runtime::execution::WorkCounters) -> Value {
    let ms = |nanos: u64| round(nanos as f64 / 1_000_000.0);
    let stage_times = json!({
        "placement": ms(c.placement_nanos),
        "organicStress": ms(c.organic_stress_nanos),
        "separation": ms(c.separation_nanos),
        "repair": ms(c.repair_nanos),
        "portResolution": ms(c.port_resolution_nanos),
        "routing": ms(c.routing_nanos),
        "corridorGaps": ms(c.corridor_gap_nanos),
        "scoring": ms(c.scoring_nanos),
        "relieveCrossings": ms(c.relieve_nanos),
        "delegated": ms(c.delegated_nanos),
        "job": ms(c.job_nanos),
    });
    let abstract_completion = abstract_completion_json(&c.abstract_completion);
    json!({
        "candidatesStarted": c.candidates_started,
        "candidatesCompleted": c.candidates_completed,
        "routingRounds": c.routing_rounds,
        "astarExpansions": c.astar_expansions,
        "astarSearches": c.astar_searches,
        "largestAstarSearch": c.largest_astar_search,
        "placementTrials": c.placement_trials,
        "legacyCandidates": c.legacy_candidates,
        "legacyGridPops": c.legacy_grid_pops,
        "legacyGridPopsByRoute": c.legacy_grid_pops_by_route,
        "delegationReasons": delegation_reasons(c),
        "corridorGapCalculations": c.corridor_gap_calculations,
        "clearanceWideningRounds": c.clearance_widening_rounds,
        "clearanceWideningNoopRounds": c.clearance_widening_noop_rounds,
        "constructedRoutes": c.constructed_routes,
        "delegatedRoutes": c.delegated_routes,
        "abstractRoutingPasses": c.abstract_routing_passes,
        "fixedTrackSegmentProjectionsComputed": c.fixed_track_segment_projections_computed,
        "fixedTrackProjectionPairChecks": c.fixed_track_projection_pair_checks,
        "reusedEvaluations": c.reused_evaluations,
        "layoutPasses": c.layout_passes,
        "placementPasses": c.placement_passes,
        "placementPassReasons": c.placement_pass_reasons,
        "reusedPreparations": c.reused_preparations,
        "orderingPasses": c.ordering_passes,
        "reusedOrderings": c.reused_orderings,
        "crossPlans": c.cross_plans,
        "reusedCrossPlans": c.reused_cross_plans,
        "structuralPlans": c.structural_plans,
        "reusedStructuralPlans": c.reused_structural_plans,
        "regionRealizations": c.region_realizations,
        "reusedRegionRealizations": c.reused_region_realizations,
        "deferredRoutingPasses": c.deferred_routing_passes,
        "construction": construction_json(&c.construction),
        "stageTimesMs": stage_times,
        "abstractCompletionMs": abstract_completion,
    })
}

fn abstract_completion_json(times: &arqlet_layout::AbstractCompletionTimes) -> Value {
    let ms = |nanos: u64| round(nanos as f64 / 1_000_000.0);
    json!({
        "abstractPlan": ms(times.abstract_plan_nanos),
        "growth": ms(times.growth_nanos),
        "construction": ms(times.construction_nanos),
        "seedPreparation": ms(times.seed_preparation_nanos),
        "fallback": ms(times.fallback_nanos),
        "trackRepair": ms(times.track_repair_nanos),
        "finalization": ms(times.finalization_nanos),
        "trackConflictCheck": ms(times.track_conflict_check_nanos),
    })
}

/// Per edge of a refined layout: the repair search work it costs (grid pops, one route at a time), whether the layered construction
/// could build it on this drawing (and if not, why), and its bends and length. Diagnostic (plan section 4a).
fn repair_work_report(
    name: &str,
    snapshot: &Snapshot,
    work: &std::collections::BTreeMap<String, usize>,
) -> Result<Value, String> {
    let rects = arqlet_layout::absolute_all(&snapshot.graph, &snapshot.boxes);
    let built = arqlet_layout::constructive::construct(
        &snapshot.graph,
        &rects,
        &snapshot.resolved_ports,
        arqlet_layout::constructive_spacing(&snapshot.options),
        &std::collections::BTreeMap::new(),
    );
    let constructed: std::collections::BTreeSet<&str> =
        built.routes.iter().map(|r| r.id.as_str()).collect();
    let reasons: std::collections::BTreeMap<&str, String> = built
        .delegated
        .iter()
        .map(|(id, reason)| (id.as_str(), format!("{reason:?}")))
        .collect();
    let mut edges = Vec::new();
    for route in &snapshot.routes {
        let length: f64 = route
            .points
            .windows(2)
            .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
            .sum();
        edges.push(json!({
            "id": route.id,
            "pops": work.get(&route.id).copied().unwrap_or(0),
            "constructed": constructed.contains(route.id.as_str()),
            "reason": reasons.get(route.id.as_str()).cloned().unwrap_or_default(),
            "bends": route.points.len().saturating_sub(2),
            "length": round(length),
        }));
    }
    Ok(json!({"dataset": name, "edges": edges}))
}

/// The stage times of the first layout of an import, in the same names as the refinement's work counters.
fn stage_json(c: &arqlet_layout::SearchCounters) -> Value {
    let ms = |nanos: u64| round(nanos as f64 / 1_000_000.0);
    json!({
        "placement": ms(c.placement_nanos),
        "organicStress": ms(c.organic_stress_nanos),
        "separation": ms(c.separation_nanos),
        "repair": ms(c.repair_nanos),
        "portResolution": ms(c.port_resolution_nanos),
        "routing": ms(c.routing_nanos),
        "corridorGaps": ms(c.corridor_gap_nanos),
        "layoutPasses": c.layout_passes,
        "abstractRoutingPasses": c.abstract_routing_passes,
        "placementPasses": c.placement_passes,
        "placementPassReasons": c.placement_pass_reasons,
        "reusedPreparations": c.reused_preparations,
        "orderingPasses": c.ordering_passes,
        "reusedOrderings": c.reused_orderings,
        "crossPlans": c.cross_plans,
        "reusedCrossPlans": c.reused_cross_plans,
        "structuralPlans": c.structural_plans,
        "reusedStructuralPlans": c.reused_structural_plans,
        "regionRealizations": c.region_realizations,
        "reusedRegionRealizations": c.reused_region_realizations,
        "deferredRoutingPasses": c.deferred_routing_passes,
        "construction": construction_json(&c.construction),
        "abstractCompletionMs": abstract_completion_json(&c.abstract_completion),
    })
}

/// One unmeasured warm-up run, then `reps` timed runs. Returns the last result.
fn timed<T>(reps: usize, mut run: impl FnMut() -> Result<T, String>) -> Result<(Value, T), String> {
    let mut last = run()?;
    let mut samples = Vec::with_capacity(reps);
    for _ in 0..reps {
        let start = Instant::now();
        last = run()?;
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    Ok((stats(samples), last))
}

/// FNV-1a over the serialized snapshot; equal fingerprints mean byte-identical output.
fn fingerprint(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn command(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn environment() -> Value {
    json!({
        "os": env::consts::OS,
        "arch": env::consts::ARCH,
        // Load average at the time of the run; a measurement on a busy machine is not an idle-machine result.
        "loadAverage": command("sysctl", &["-n", "vm.loadavg"]),
        "cpuModel": command("sysctl", &["-n", "machdep.cpu.brand_string"]),
        "logicalCpus": thread::available_parallelism().map_or(1, |n| n.get()),
        "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
        "rustc": command("rustc", &["-V"]),
        "gitRevision": command("git", &["rev-parse", "--short", "HEAD"]),
        "gitDirty": command("git", &["status", "--porcelain"]).map(|s| !s.is_empty()),
    })
}

/// One cancellation probe: the stop signal first answers `true` after `after`. The overrun is the time from that first
/// answer to the return. The probe also checks that every drawn edge keeps a route, whether or not the signal fired.
fn cancel_probe(base: &Snapshot, after: Duration, threads: usize) -> Result<Value, String> {
    let start = Instant::now();
    let fired: OnceLock<Instant> = OnceLock::new();
    let stop = || {
        if start.elapsed() >= after {
            fired.get_or_init(Instant::now);
            true
        } else {
            false
        }
    };
    let refined = refine_snapshot_until(
        base,
        base.revision,
        base.layout_version,
        threads,
        Some(&stop),
    )?;
    let returned = Instant::now();
    let routed: BTreeSet<&str> = refined.routes.iter().map(|r| r.id.as_str()).collect();
    let complete_routes = refined
        .graph
        .edges
        .iter()
        .filter(|e| e.target.is_some())
        .all(|e| routed.contains(e.id.as_str()));
    let interrupted = refined.optimization.as_ref().is_some_and(|r| r.interrupted);
    let overrun_ms = fired
        .get()
        .map(|first| round(returned.duration_since(*first).as_secs_f64() * 1000.0));
    Ok(json!({
        "returnedMs": round(returned.duration_since(start).as_secs_f64() * 1000.0),
        "interrupted": interrupted,
        "overrunMs": overrun_ms,
        "completeRoutes": complete_routes,
    }))
}

/// What one edit cost and changed: the time, the layout versions it advanced, how many areas it laid out again of all areas,
/// and the size of the snapshot that travels in and out of the engine (every request carries the whole snapshot).
fn edit_json(before: &Snapshot, after: &Snapshot, stats: Value) -> Value {
    let areas = before
        .graph
        .nodes
        .iter()
        .filter(|n| {
            before
                .graph
                .nodes
                .iter()
                .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
        })
        .count()
        + 1;
    json!({
        "time": stats,
        "layoutVersionAdvanced": after.layout_version - before.layout_version,
        "areasRecomputed": after.recomputed_containers.len(),
        "areasTotal": areas,
        "requestSnapshotBytes": serde_json::to_vec(before).map(|b| b.len()).unwrap_or(0),
        "replySnapshotBytes": serde_json::to_vec(after).map(|b| b.len()).unwrap_or(0),
    })
}

/// The interactive edits on a published snapshot, each timed from the same start: a drag (begin, updates, finish), a
/// measurement that changes a size, the same measurement again with a newer revision, and a pin. Reported per edit.
fn interaction(snapshot: &Snapshot, reps: usize) -> Result<Value, String> {
    let graph = &snapshot.graph;
    let leaf = graph
        .nodes
        .iter()
        .find(|n| {
            !graph
                .nodes
                .iter()
                .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
        })
        .ok_or("No leaf node to edit")?;
    let node = leaf.id.clone();
    let (circle, square) = (
        leaf.shape == NodeShape::Circle,
        leaf.shape == NodeShape::Square,
    );
    let box_ = snapshot.boxes[&node].clone();
    let mut out = serde_json::Map::new();

    eprintln!("interaction: start");
    // A drag of 6 updates of 25 units; the begin, an update and the finish are timed separately.
    let mut begin = Vec::new();
    let mut update = Vec::new();
    let mut finish = Vec::new();
    let mut last_update = None;
    for round in 0..=reps {
        let step = |snapshot: &Snapshot, seq: u64, phase: MovePhase, at: Option<(f64, f64)>| {
            let s = MoveStep {
                gesture_id: format!("bench-{round}"),
                seq,
                phase,
                node: node.clone(),
                position: at.map(|(x, y)| Point { x, y }),
                release: false,
                others: Default::default(),
            };
            move_node(snapshot, snapshot.revision, snapshot.layout_version, &s)
        };
        let t = Instant::now();
        let mut current = step(snapshot, 1, MovePhase::Begin, Some((box_.x, box_.y)))?;
        let began = t.elapsed().as_secs_f64() * 1000.0;
        let mut updates = Vec::new();
        for k in 1..=6u64 {
            let before = current.clone();
            let t = Instant::now();
            current = step(
                &current,
                1 + k,
                MovePhase::Update,
                Some((box_.x + 25. * k as f64, box_.y)),
            )?;
            updates.push(t.elapsed().as_secs_f64() * 1000.0);
            last_update = Some((before, current.clone()));
        }
        let t = Instant::now();
        step(&current, 8, MovePhase::Finish, None)?;
        let finished = t.elapsed().as_secs_f64() * 1000.0;
        if round > 0 {
            begin.push(began);
            update.extend(updates);
            finish.push(finished);
        }
    }
    if let Some((before, after)) = &last_update {
        out.insert("dragUpdate".into(), edit_json(before, after, stats(update)));
    }
    out.insert("dragBegin".into(), stats(begin));
    out.insert("dragFinish".into(), stats(finish));

    eprintln!("interaction: drag done");
    // A measurement that changes the size of the leaf, then the same size again with a newer revision.
    let measurement = |width: f64, revision: u64| {
        std::collections::BTreeMap::from([(
            node.clone(),
            Measurement {
                contour: if circle {
                    Contour::Circle { diameter: width }
                } else {
                    Contour::Rect {
                        width,
                        height: if square { width } else { box_.height },
                    }
                },
                geometry_revision: revision,
            },
        )])
    };
    let (changed_ms, changed) = timed(reps, || {
        measure_contours(
            snapshot,
            snapshot.revision,
            snapshot.layout_version,
            measurement(box_.width + 40., 1),
        )
    })?;
    out.insert(
        "measureChanged".into(),
        edit_json(snapshot, &changed, changed_ms),
    );
    let (same_ms, same) = timed(reps, || {
        measure_contours(
            &changed,
            changed.revision,
            changed.layout_version,
            measurement(box_.width + 40., 2),
        )
    })?;
    out.insert(
        "measureSameSize".into(),
        edit_json(&changed, &same, same_ms),
    );

    // A pin at the current box.
    let (pin_ms, pinned) = timed(reps, || {
        configure_snapshot(
            snapshot,
            snapshot.revision,
            snapshot.layout_version,
            &json!({"pins": {node.clone(): {"x": box_.x, "y": box_.y}}}),
        )
    })?;
    out.insert("pin".into(), edit_json(snapshot, &pinned, pin_ms));
    out.insert("node".into(), json!(node));
    Ok(Value::Object(out))
}

fn run(args: &Args) -> Result<Value, String> {
    let text = fs::read_to_string(&args.graph).map_err(|e| e.to_string())?;
    let graph: Graph = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if args.layout_only {
        let patch = match &args.options {
            Some(json) => serde_json::from_str(json).map_err(|e| e.to_string())?,
            None => serde_json::Value::Object(Default::default()),
        };
        let options = arqlet_runtime::options_from_patch(&patch)?;
        let (layout_ms, snapshot) = timed(args.reps, || {
            arqlet_layout::compute_counted(&graph, None, &BTreeSet::new(), &options)
                .map(|(snapshot, _)| snapshot)
        })?;
        return Ok(json!({
            "dataset": args.name,
            "layoutMode": "in-process-compute",
            "graphNodes": graph.nodes.len(),
            "graphEdges": graph.edges.len(),
            "reps": args.reps,
            "layoutMs": layout_ms,
            "snapshot": snapshot,
        }));
    }
    // The clone is part of each import sample; it is the same for every run.
    let import_options = match &args.import_options {
        Some(json) => arqlet_runtime::options_from_patch(
            &serde_json::from_str(json).map_err(|e| e.to_string())?,
        )?,
        None => arqlet_model::LayoutOptions::default(),
    };
    let (import_ms, (base, import_stages)) = timed(args.reps, || {
        arqlet_runtime::import_graph_counted(graph.clone(), args.limit, &import_options)
    })?;
    eprintln!("import done: {import_ms}");
    let base = match &args.preset {
        Some(path) => apply_preset(&base, path)?,
        None => base,
    };
    let base = match &args.options {
        Some(json) => apply_options(&base, json)?,
        None => base,
    };
    let base = match &args.refine_options {
        Some(json) => apply_refine_options(&base, json)?,
        None => base,
    };
    if args.routing_completion {
        if args.regional_routing {
            return regional_report(
                &base,
                args.reps,
                args.regional_repair,
                args.regional_construction,
                args.regional_coordination,
            );
        }
        return completion_report(&base, args.reps, args.routing_planning);
    }
    if args.constructive {
        return constructive_report(&args.graph, &base, &import_ms);
    }
    let input_geometry =
        serde_json::to_vec(&(&base.boxes, &base.routes)).map_err(|e| e.to_string())?;
    let input_geometry_fingerprint = fingerprint(&input_geometry);

    // `--threads none` measures import and edit only, for views where refinement is too slow.
    let mut refine_ms = serde_json::Map::new();
    let mut refine_core_ms = serde_json::Map::new();
    let mut geometry_fingerprints = Vec::new();
    let mut fingerprints = Vec::new();
    let mut core_fingerprints = Vec::new();
    let mut core_keys = serde_json::Map::new();
    let mut work_by_threads = serde_json::Map::new();
    let mut candidate_work_by_threads = serde_json::Map::new();
    let mut refined: Option<Snapshot> = None;
    for &threads in &args.threads {
        let (stats, (result, report)) = timed(args.reps, || {
            refine_snapshot_job(
                "",
                &base,
                base.revision,
                base.layout_version,
                threads,
                None,
                None,
            )
        })?;
        let (core_stats, core_result) = timed(args.reps, || {
            refine_snapshot_limited(
                &base,
                base.revision,
                base.layout_version,
                threads,
                None,
                None,
            )
        })?;
        let bytes = serde_json::to_vec(&result).map_err(|e| e.to_string())?;
        let core_bytes = serde_json::to_vec(&core_result).map_err(|e| e.to_string())?;
        core_keys.insert(
            threads.to_string(),
            json!(arqlet_layout::published_key(&core_result)?),
        );
        fingerprints.push(fingerprint(&bytes));
        core_fingerprints.push(fingerprint(&core_bytes));
        // Geometry only (boxes and routes): comparable across metadata changes such as labels or counters.
        let geometry =
            serde_json::to_vec(&(&result.boxes, &result.routes)).map_err(|e| e.to_string())?;
        geometry_fingerprints.push(fingerprint(&geometry));
        refine_ms.insert(threads.to_string(), stats);
        refine_core_ms.insert(threads.to_string(), core_stats);
        if args.repair_work {
            return repair_work_report(
                &args.graph,
                &result,
                &report.counters.legacy_grid_pops_by_route,
            );
        }
        work_by_threads.insert(threads.to_string(), work_json(&report.counters));
        candidate_work_by_threads.insert(
            threads.to_string(),
            json!(report
                .candidate_work
                .iter()
                .map(|candidate| json!({
                    "strategy": candidate.candidate.strategy,
                    "parameter": candidate.candidate.parameter,
                    "occurrence": candidate.candidate.occurrence,
                    "work": work_json(&candidate.counters),
                }))
                .collect::<Vec<_>>()),
        );
        refined = Some(result);
    }
    if fingerprints.iter().any(|f| f != &fingerprints[0]) {
        return Err("Refinement output differs across thread counts".into());
    }
    if core_fingerprints.iter().any(|f| f != &core_fingerprints[0]) {
        return Err("Core refinement output differs across thread counts".into());
    }
    let refined: Snapshot = refined.unwrap_or_else(|| base.clone());

    let source = match base.graph.edges.first() {
        Some(edge) => edge.source.clone(),
        None => base.graph.nodes.first().ok_or("Empty view")?.id.clone(),
    };
    let edit_ms = if args.pipeline_only {
        Value::Null
    } else {
        timed(args.reps, || {
            add_node(&refined, refined.revision, &source, "Benchmark node")
        })?
        .0
    };
    eprintln!("add_node done: {edit_ms}");
    let snapshot_bytes = serde_json::to_vec(&refined)
        .map_err(|e| e.to_string())?
        .len();

    let mut cancellation = Vec::new();
    let threads = args.threads.last().copied().unwrap_or(1);
    for &ms in &args.cancel_after_ms {
        let mut probes = Vec::with_capacity(args.reps);
        for _ in 0..args.reps {
            probes.push(cancel_probe(&base, Duration::from_millis(ms), threads)?);
        }
        cancellation.push(json!({ "afterMs": ms, "threads": threads, "probes": probes }));
    }

    if let Some(dir) = &args.candidate_log {
        write_candidate_log(dir, args, &base, &refined)?;
    }
    // Near-node pairs of the result at the preferred margin, whatever clearance was required (routes are unchanged).
    let preferred_hugs = {
        let mut preferred = refined.clone();
        preferred.options.required_clearance = None;
        arqlet_layout::published_metrics(&preferred)
            .ok()
            .map(|m| m.hug_pairs)
    };
    let mut out = json!({
        "dataset": args.name,
        "limit": args.limit,
        "graphNodes": graph.nodes.len(),
        "graphEdges": graph.edges.len(),
        "viewNodes": base.graph.nodes.len(),
        "viewEdges": base.graph.edges.len(),
        "omittedUnresolvedEdges": base.omitted_unresolved_edges,
        "reps": args.reps,
        "importMs": import_ms,
        "importStages": stage_json(&import_stages),
        "refineMs": refine_ms,
        "refineCoreMs": refine_core_ms,
        "work": work_by_threads,
        "candidateWork": candidate_work_by_threads,
        "stageTimeNote": "Cumulative candidate-stage times in ms; enabled only with the benchmark-timings feature, zero on WASM; nested stages overlap and parallel candidates are summed.",
        "refineFingerprint": fingerprints.first(),
        "refineCoreFingerprint": core_fingerprints.first(),
        "refineCorePublishedKeys": core_keys,
        "geometryFingerprint": geometry_fingerprints.first(),
        "inputGeometryFingerprint": input_geometry_fingerprint,
        "threadsIdentical": true,
        // Routes with a diagonal segment: conflicts drawn as direct connections, which the orthogonal view must not show.
        "routingIssues": refined.routing_issues.len(),
        "nonOrthogonalRoutes":refined.routes.iter().filter(|r| r.points.windows(2).any(|w| (w[0].x - w[1].x).abs() > 1e-6 && (w[0].y - w[1].y).abs() > 1e-6)).count(),
        "editMs": edit_ms,
        "interaction": if args.pipeline_only {Value::Null} else {interaction(&refined, args.reps)?},
        "pipelineOnly": args.pipeline_only,
        "snapshotBytes": snapshot_bytes,
        "optimization": refined.optimization,
        // Shadow M0 scores of the input and the refined result, with the published keys they were compared on.
        "shadowQuality": shadow_json(&refined),
        "inputShadowQuality": shadow_json(&base),
        "publishedKey": arqlet_layout::published_key(&refined).ok(),
        "inputPublishedKey": arqlet_layout::published_key(&base).ok(),
        "publishedHugPairs": arqlet_layout::published_metrics(&refined).ok().map(|m| m.hug_pairs),
        "structure": structure_json(&base),
        "publishedHugPairsAtPreferred": preferred_hugs.clone(),
        "inputPublishedHugPairs": arqlet_layout::published_metrics(&base).ok().map(|m| m.hug_pairs),
        "environment": environment(),
        "effectiveOptions": &base.options,
    });
    if let Some(path) = &args.preset {
        out["preset"] = json!(path);
    }
    if args.abstract_plan {
        out["inputAbstractRouting"] = abstract_plan_json(&base, args.abstract_realization)?;
        out["abstractRouting"] = abstract_plan_json(&refined, args.abstract_realization)?;
    }
    if let Some(options) = &args.refine_options {
        out["refineOptions"] = json!(options);
    }
    if args.progress {
        // Time to each published improvement, and the total, from one refinement at the largest thread count.
        let threads = args.threads.last().copied().unwrap_or(1);
        let start = Instant::now();
        let mut timeline = Vec::new();
        refine_snapshot_progressive(
            "",
            &base,
            base.revision,
            base.layout_version,
            threads,
            None,
            None,
            &mut |p: &arqlet_runtime::execution::Publication| {
                timeline.push(json!({
                    "sequence": p.sequence,
                    "strategy": p.strategy,
                    "candidatesEvaluated": p.candidates_evaluated,
                    "atMs": round(start.elapsed().as_secs_f64() * 1000.0),
                }));
            },
        )?;
        out["progress"] = json!({
            "threads": threads,
            "totalMs": round(start.elapsed().as_secs_f64() * 1000.0),
            "publications": timeline,
        });
    }
    if !cancellation.is_empty() {
        out["cancellation"] = json!(cancellation);
    }
    Ok(out)
}

/// Merges a JSON object of options over the snapshot's options and recomputes the view with them.
fn apply_options(base: &Snapshot, json: &str) -> Result<Snapshot, String> {
    let options = merge_options(base, json)?;
    arqlet_layout::compute(&base.graph, None, &BTreeSet::new(), &options)
}

fn apply_refine_options(base: &Snapshot, json: &str) -> Result<Snapshot, String> {
    let mut snapshot = base.clone();
    snapshot.options = merge_options(base, json)?;
    Ok(snapshot)
}

fn merge_options(base: &Snapshot, json: &str) -> Result<arqlet_model::LayoutOptions, String> {
    let mut merged = serde_json::to_value(&base.options).map_err(|e| e.to_string())?;
    let patch: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let (Some(target), Some(patch)) = (merged.as_object_mut(), patch.as_object()) else {
        return Err("--options must be a JSON object".into());
    };
    for (key, value) in patch {
        target.insert(key.clone(), value.clone());
    }
    serde_json::from_value(merged).map_err(|e| e.to_string())
}

/// Applies a preset's configuration set and recomputes the view, so the measurement uses the options the app uses.
fn apply_preset(base: &Snapshot, path: &str) -> Result<Snapshot, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    let preset: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let config: ConfigSet =
        serde_json::from_value(preset["configSet"].clone()).map_err(|e| e.to_string())?;
    let mut options = base.options.clone();
    config.apply(&mut options)?;
    arqlet_layout::compute(&base.graph, None, &BTreeSet::new(), &options)
}

fn main() {
    let result = parse().and_then(|args| run(&args));
    match result {
        Ok(value) => println!("{value}"),
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(1);
        }
    }
}

/// The shadow M0 evaluation as JSON: version, total and each term's raw value and utility.
/// The crossings among the constructed routes alone (the drawing of only those routes): whether the tracks fit together.
fn constructed_metrics(
    base: &Snapshot,
    routes: &[arqlet_model::Route],
    ports: &[arqlet_model::ResolvedPort],
) -> Value {
    let mut only = base.clone();
    only.routes = routes.to_vec();
    only.resolved_ports = ports.to_vec();
    match arqlet_layout::published_metrics(&only) {
        Ok(m) => {
            json!({"crossings": m.crossings, "atSharedEndpoints": m.shared_endpoint_crossings})
        }
        Err(error) => json!({"error": error}),
    }
}

/// The acceptance numbers of the constructive layered routes (plan section 4a), measured on the import. Four variants run on
/// the same import: the baseline, the flow of `auto` areas taken from the layout (`orientations`), the flow-facing ports
/// (a ceiling experiment), and both. Each variant's `combined` drawing has its constructed routes in place and the import
/// routes for the rest.
fn constructive_report(name: &str, base: &Snapshot, import_ms: &Value) -> Result<Value, String> {
    let rects = arqlet_layout::absolute_all(&base.graph, &base.boxes);
    // The margin is the clearance the routing parameters give the obstacles: the required clearance plus one track.
    let options = &base.options;
    let track = options.track_spacing;
    let spacing = arqlet_layout::constructive::Spacing {
        margin: options
            .required_clearance
            .map_or(options.routing_clearance, |c| {
                options.routing_clearance.max(c + track)
            }),
        track,
        through_borders: false,
        probe: false,
        port_sides: false,
    };
    let none = std::collections::BTreeMap::new();
    // Port sides (flow-facing or the automatic ones) and the order in which the edges take their tracks.
    let variants = [
        (
            "baseline",
            false,
            arqlet_layout::constructive::EdgeOrder::Input,
        ),
        (
            "flowPorts",
            true,
            arqlet_layout::constructive::EdgeOrder::Input,
        ),
        (
            "flowPortsShortFirst",
            true,
            arqlet_layout::constructive::EdgeOrder::ShortFirst,
        ),
        (
            "flowPortsSourcePort",
            true,
            arqlet_layout::constructive::EdgeOrder::SourcePort,
        ),
    ];
    let mut out = serde_json::Map::new();
    for (label, use_flow_ports, edge_order) in variants {
        let orientations = &none;
        let ports = if use_flow_ports {
            arqlet_layout::constructive::flow_ports(
                &base.graph,
                &rects,
                &base.resolved_ports,
                orientations,
            )
        } else {
            base.resolved_ports.clone()
        };
        let started = std::time::Instant::now();
        let built = arqlet_layout::constructive::construct_ordered(
            &base.graph,
            &rects,
            &ports,
            spacing,
            orientations,
            edge_order,
        );
        let construct_ms = started.elapsed().as_secs_f64() * 1000.;
        let mut reasons: std::collections::BTreeMap<String, usize> = Default::default();
        for (_, reason) in &built.delegated {
            *reasons.entry(format!("{reason:?}")).or_default() += 1;
        }
        let built_by_id: std::collections::BTreeMap<&str, &arqlet_model::Route> =
            built.routes.iter().map(|r| (r.id.as_str(), r)).collect();
        let mut combined = base.clone();
        combined.resolved_ports = ports.clone();
        combined.routes = base
            .routes
            .iter()
            .map(|r| {
                built_by_id
                    .get(r.id.as_str())
                    .map_or_else(|| r.clone(), |c| (*c).clone())
            })
            .collect();
        out.insert(
            label.to_string(),
            json!({
                "constructed": built.routes.len(),
                "delegated": built.delegated.len(),
                "delegatedBy": reasons,
                "laneChanges": built.lane_changes,
                "constructMs": round(construct_ms),
                "combinedShadow": shadow_json(&combined),
                "constructedCrossings": constructed_metrics(base, &built.routes, &ports),
            }),
        );
    }
    let edges = base
        .graph
        .edges
        .iter()
        .filter(|e| e.target.is_some())
        .count();
    Ok(json!({
        "dataset": name,
        "edges": edges,
        "importMs": import_ms,
        "importShadow": shadow_json(base),
        "variants": out,
    }))
}

fn shadow_json(snapshot: &Snapshot) -> Value {
    match arqlet_layout::shadow_evaluation(snapshot) {
        Ok(eval) => json!({
            "version": eval.version,
            "hardViolations": eval.hard_violations,
            "total": eval.total,
            "terms": eval
                .terms
                .iter()
                .map(|t| json!({ "name": t.name, "raw": t.raw, "utility": t.utility }))
                .collect::<Vec<_>>(),
        }),
        Err(error) => json!({ "error": error }),
    }
}

/// The number of archive entries kept per view in the candidate log.
use arqlet_runtime::ARCHIVE_CAPACITY;

/// Writes the candidate log of one view: `DIR/<name>-<limit>.jsonl` with one line per complete candidate in reduction
/// order, and `DIR/<name>-<limit>/geometry/<index>.json` for each candidate whose geometry is not a duplicate. The log
/// is produced at the last configured thread count; it does not depend on it.
fn write_candidate_log(
    dir: &str,
    args: &Args,
    base: &Snapshot,
    refined: &Snapshot,
) -> Result<(), String> {
    let threads = args.threads.last().copied().unwrap_or(1);
    let (logged, _, records) =
        refine_snapshot_logged(base, base.revision, base.layout_version, threads, None)?;
    let archive = archive_indices(&records, ARCHIVE_CAPACITY);
    // Logging must not change the result. The plain result was computed at the same thread count (when one was run).
    if !args.threads.is_empty()
        && serde_json::to_vec(&logged).map_err(|e| e.to_string())?
            != serde_json::to_vec(refined).map_err(|e| e.to_string())?
    {
        return Err("The candidate log changed the refinement result".into());
    }
    let stem = format!("{}-{}", args.name, args.limit);
    fs::create_dir_all(Path::new(dir).join(&stem).join("geometry")).map_err(|e| e.to_string())?;
    let mut lines = String::new();
    let ms = |nanos: u64| round(nanos as f64 / 1_000_000.0);
    for record in &records {
        let geometry_file = match (&record.snapshot, record.duplicate_of) {
            (Some(snapshot), None) => {
                let path = format!("{stem}/geometry/{}.json", record.index);
                let geometry = json!({ "boxes": snapshot.boxes, "routes": snapshot.routes });
                fs::write(Path::new(dir).join(&path), geometry.to_string())
                    .map_err(|e| e.to_string())?;
                Some(path)
            }
            _ => None,
        };
        let mut row = json!({
            "index": record.index,
            "phase": record.phase,
            "round": record.round,
            "id": record.id.as_ref().map(|id| json!({
                "strategy": id.strategy,
                "parameter": id.parameter,
                "occurrence": id.occurrence,
            })),
            "strategy": record.strategy,
            "parent": record.parent,
            "outcome": format!("{:?}", record.outcome),
            "input": {
                "graphRevision": record.input.graph_revision,
                "baseLayoutVersion": record.input.base_layout_version,
                "optionsKey": record.input.options_key,
            },
            "metrics": record.metrics,
            "hardViolations": record.hard_violations,
            "key": record.key,
            "shadowTotal": record.shadow_total,
            "work": work_json(&record.work),
            // Wall times of the job's stages in ms (benchmark-timings builds only; zero otherwise). Stages overlap: the
            // routing contains the delegated edges' fallback.
            "stageMs": {
                "job": ms(record.work.job_nanos),
                "placement": ms(record.work.placement_nanos),
                "routing": ms(record.work.routing_nanos),
                "delegated": ms(record.work.delegated_nanos),
                "portResolution": ms(record.work.port_resolution_nanos),
                "scoring": ms(record.work.scoring_nanos),
            },
            "geometryFingerprint": record.geometry_fingerprint,
            "duplicateOf": record.duplicate_of,
            "archiveRank": archive.iter().position(|&i| i == record.index),
            "accepted": record.accepted,
            "selected": record.selected,
            "published": record.published,
            "geometryFile": geometry_file,
        });
        if args.abstract_plan {
            if let Some(snapshot) = &record.snapshot {
                row["abstractRouting"] = abstract_plan_json(snapshot, args.abstract_realization)?;
            } else {
                row["abstractRouting"] = json!({
                    "unavailable": "No materialized snapshot; see duplicateOf or candidate outcome.",
                });
            }
        }
        lines.push_str(&row.to_string());
        lines.push('\n');
    }
    fs::write(Path::new(dir).join(format!("{stem}.jsonl")), lines).map_err(|e| e.to_string())
}

/// Per-parent structure of the view graph (weak components, strong components, forest and DAG flags), as `place` sees
/// the children of each container. Reports only.
fn structure_json(base: &Snapshot) -> Value {
    arqlet_layout::structure::analyse_graph(&base.graph)
        .iter()
        .map(|p| {
            json!({
                "parent": p.parent,
                "children": p.children.len(),
                "components": p.structure.components.iter().map(|c| c.len()).collect::<Vec<_>>(),
                "strongComponents": p.structure.strong_components,
                "isDag": p.structure.is_dag,
                "isForest": p.structure.is_forest,
            })
        })
        .collect::<Vec<_>>()
        .into()
}
