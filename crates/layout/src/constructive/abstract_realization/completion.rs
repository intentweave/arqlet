//! Complete abstract routing, shared by diagnostics and the opt-in layout path.
use super::*;
use arqlet_model::{Bundling, GeometryIssue, RoutingEngine, Snapshot};

#[derive(Clone, Copy, Debug)]
pub enum Method {
    Existing,
    Chains,
    ChainsFixedTracks,
    ChainsAligned,
    /// `ChainsAligned` construction without fallback search or track repair:
    /// delegated edges keep their port-to-port seed route.
    SinglePass,
}

#[derive(Debug, Default)]
pub struct Times {
    pub abstract_plan: u64,
    pub growth: u64,
    pub construction: u64,
    pub seeds: u64,
    pub fallback: u64,
    pub track_repair: u64,
    pub finalization: u64,
    pub evaluation: u64,
}

pub struct CompletedDrawing {
    /// Complete local-coordinate handoff, including the grown plans and ports.
    pub snapshot: Snapshot,
    pub scene: arqlet_scene::Scene,
    pub routes: Vec<Route>,
    pub fixed: Vec<Route>,
    pub delegated: Vec<(String, String)>,
    pub channels: Vec<Channel>,
    pub geometry_issues: Vec<GeometryIssue>,
    pub routing_issues: Vec<String>,
    pub key: Vec<i64>,
    pub metrics: arqlet_scene::metrics::Metrics,
    pub hard_violations: usize,
    /// Strict constructor spacing, including endpoint legs; unlike shared metrics
    /// this deliberately has no exemption for routes incident to the same node.
    pub track_conflicts: Vec<(String, String)>,
    pub search: crate::SearchCounters,
    pub track_repair: Option<arqlet_routing::fixed_tracks::Work>,
    pub planning: Option<planning::Work>,
    pub construction_reuse: Option<construction::Work>,
    pub construction_times: Option<construction::Times>,
    pub coordination_work: Option<coordination::Work>,
    pub times: Times,
}

pub(crate) struct CompletedLayout {
    pub snapshot: Snapshot,
    pub fixed: Vec<Route>,
    #[cfg(feature = "benchmark-timings")]
    pub delegated: Vec<(String, String)>,
    pub channels: Vec<Channel>,
    pub track_conflicts: Vec<(String, String)>,
    pub search: crate::SearchCounters,
    pub track_repair: Option<arqlet_routing::fixed_tracks::Work>,
    pub times: Times,
}

struct CompletionOutcome {
    drawing: Snapshot,
    fixed: Vec<Route>,
    delegated: Vec<(String, String)>,
    channels: Vec<Channel>,
    track_conflicts: Vec<(String, String)>,
    search: crate::SearchCounters,
    track_repair: Option<arqlet_routing::fixed_tracks::Work>,
    planning: Option<planning::Work>,
    construction_reuse: Option<construction::Work>,
    construction_times: Option<construction::Times>,
    coordination_work: Option<coordination::Work>,
    times: Times,
}

/// The strict acceptance gate of the provisional abstract-routing candidates.
pub fn has_strict_track_conflicts(snapshot: &Snapshot) -> bool {
    !arqlet_routing::fixed_tracks::conflicts(&snapshot.routes, snapshot.options.track_spacing)
        .is_empty()
}

/// Validate explicit opt-in requests even when their work budget runs no layouts.
pub fn check_capability(snapshot: &Snapshot) -> Result<(), String> {
    let options = &snapshot.options;
    if !options.layered_construction
        || !options.hierarchical_ordering
        || options.global_layering
        || options.visible_node_budget != 0
        || options.edge_routing.is_some()
    {
        return Err("Abstract routing requires unflattened hierarchical placement and default orthogonal routing without a view budget".into());
    }
    let plans = input_plans(snapshot)?;
    growth::check_capability(&snapshot.graph, &plans, options)
}

/// Run either constructor on freshly widened geometry, then complete
/// every delegated edge with the existing legacy router, giving it fixed
/// occupancy for improvement. Its final repair/separation stages do not include
/// fixed occupancy. `ChainsFixedTracks` additionally repairs track spacing after
/// those stages, leaving constructed routes fixed. `track_conflicts` reports any
/// unresolved violations; the extra phase makes no completeness guarantee.
/// `ChainsAligned` changes the demand plan before growth, and may therefore
/// change node geometry; it is not an output-equivalent optimization.
/// Quality findings are returned, never converted to success or hidden. This
/// function does not publish or select candidates.
pub fn run(snapshot: &Snapshot, method: Method) -> Result<CompletedDrawing, String> {
    crate::uninterrupted(
        run_impl(snapshot, method, &mut planning::Session::default(), &|| {
            false
        })
        .and_then(evaluate),
    )
}

/// Evaluate the aligned trial using exact run-local planning dependencies.
/// Default sessions cache plans, not geometry. Optional fixed-path construction
/// reuse checks exact geometry/obstacles and keeps occupancy acceptance fresh;
/// optional fixed-track decision reuse checks its full repair dependencies.
/// Optional coordination sharing borrows only this grown drawing; geometric
/// projections cannot survive into the next evaluation or session fork.
pub fn run_in_session(
    snapshot: &Snapshot,
    session: &mut planning::Session,
) -> Result<CompletedDrawing, String> {
    crate::uninterrupted(
        run_impl(snapshot, Method::ChainsAligned, session, &|| false).and_then(evaluate),
    )
}

pub(crate) fn run_until(
    snapshot: &Snapshot,
    session: &mut planning::Session,
    stop: &dyn Fn() -> bool,
) -> Result<CompletedLayout, crate::Abort> {
    let method = if snapshot.options.single_pass {
        Method::SinglePass
    } else {
        Method::ChainsAligned
    };
    run_impl(snapshot, method, session, stop).map(|result| CompletedLayout {
        snapshot: result.drawing,
        fixed: result.fixed,
        #[cfg(feature = "benchmark-timings")]
        delegated: result.delegated,
        channels: result.channels,
        track_conflicts: result.track_conflicts,
        search: result.search,
        track_repair: result.track_repair,
        times: result.times,
    })
}

fn run_impl(
    snapshot: &Snapshot,
    method: Method,
    session: &mut planning::Session,
    stop: &dyn Fn() -> bool,
) -> Result<CompletionOutcome, crate::Abort> {
    let checkpoint = || {
        if stop() {
            Err(crate::Abort::Stopped)
        } else {
            Ok(())
        }
    };
    checkpoint()?;
    session.physical_tracks = snapshot.options.abstract_routing;
    session.explicit_ports = matches!(method, Method::SinglePass);
    let before = session.work.clone();
    let construction_before = session.construction.as_ref().map(|s| s.work.clone());
    let plans = input_plans(snapshot)?;
    let mut times = Times::default();
    let started = crate::stage_timer_start();
    let selected = if matches!(method, Method::ChainsAligned | Method::SinglePass) {
        Some(session.plan(&snapshot.graph, &plans)?)
    } else {
        None
    };
    times.abstract_plan = crate::elapsed_nanos(started);
    let started = crate::stage_timer_start();
    let absolute = crate::absolute_all(&snapshot.graph, &snapshot.boxes);
    let grown = if let Some(plan) = &selected {
        growth::widen_planned(
            &snapshot.graph,
            &absolute,
            &snapshot.resolved_ports,
            &plans,
            &snapshot.options,
            plan,
            Some(session),
            matches!(method, Method::SinglePass),
        )?
    } else {
        growth::widen(
            &snapshot.graph,
            &absolute,
            &snapshot.resolved_ports,
            &plans,
            &snapshot.options,
        )?
    };
    times.growth = crate::elapsed_nanos(started);
    checkpoint()?;
    let mut ports = grown.ports;
    let spacing = crate::constructive_spacing(&snapshot.options);
    let mut search = crate::SearchCounters::default();
    let started = crate::stage_timer_start();
    let mut construction_times = None;
    let mut coordination_work = None;
    let mut channels = Vec::new();
    let (fixed, delegated): (Vec<Route>, Vec<(String, String)>) = match method {
        Method::Chains | Method::ChainsFixedTracks | Method::ChainsAligned | Method::SinglePass => {
            let mut trial = if let Some(plan) = &selected {
                validate_inputs(&snapshot.graph, &grown.boxes, &ports, &grown.plans, spacing)?;
                realize_plan(
                    &snapshot.graph,
                    &grown.boxes,
                    &ports,
                    &grown.plans,
                    spacing,
                    plan,
                    Coordination::Chains,
                    false,
                    Some(session),
                )?
            } else {
                realize_chains(&snapshot.graph, &grown.boxes, &ports, &grown.plans, spacing)?
            };
            search.construction.add(&trial.work);
            // Single pass: self-loops are nested at a node's freest corner, clear of every route built, as the default
            // constructs them (`nest_self_loops`), instead of going to the seed routes. The other chain methods keep
            // delegating them.
            if matches!(method, Method::SinglePass) {
                let loops: Vec<String> = trial
                    .delegated
                    .iter()
                    .filter(|(_, f)| {
                        matches!(
                            f,
                            super::Failure::Structural(
                                crate::abstract_routing::Unsupported::SelfLoop
                            )
                        )
                    })
                    .map(|(id, _)| id.clone())
                    .collect();
                if !loops.is_empty() {
                    let built: Vec<super::super::Segment> = trial
                        .routes
                        .iter()
                        .flat_map(|r| super::super::segments_of(&r.points))
                        .collect();
                    let ids: Vec<&str> = loops.iter().map(String::as_str).collect();
                    let parents = super::super::parents_of(&snapshot.graph);
                    let nested = super::super::nest_self_loops(
                        &grown.boxes,
                        &ports,
                        &parents,
                        spacing,
                        &ids,
                        &built,
                    );
                    trial.delegated.retain(|(id, _)| !nested.contains_key(id));
                    trial.routes.extend(nested.into_values());
                }
            }
            // A self-loop is nested at a corner of its node (`nest_self_loops`), which places its ends there: its ports
            // take them, as the existing constructor's routes do below.
            for route in &trial.routes {
                let looped =
                    snapshot.graph.edges.iter().any(|e| {
                        e.id == route.id && e.target.as_deref() == Some(e.source.as_str())
                    });
                if !looped {
                    continue;
                }
                for port in ports.iter_mut().filter(|p| p.edge_id == route.id) {
                    let position = if port.endpoint == "source" {
                        route.points.first()
                    } else {
                        route.points.last()
                    };
                    if let Some(position) = position {
                        port.position = position.clone();
                    }
                }
            }
            construction_times = Some(trial.times);
            coordination_work = Some(trial.coordination_work);
            channels = trial.channels.clone();
            (
                trial.routes,
                trial
                    .delegated
                    .into_iter()
                    .map(|(id, f)| (id, format!("{f:?}")))
                    .collect(),
            )
        }
        Method::Existing => {
            let trial = super::super::construct_planned(
                &snapshot.graph,
                &grown.boxes,
                &ports,
                spacing,
                &BTreeMap::new(),
                &grown.plans,
            );
            search.construction.add(&trial.work);
            // The existing constructor may change automatic slots. Use its
            // actual endpoints for fixed routes and its explicit delegated moves.
            for (id, source, target) in &trial.moved_ends {
                for port in ports.iter_mut().filter(|p| p.edge_id == *id) {
                    if let Some(position) = if port.endpoint == "source" {
                        source
                    } else {
                        target
                    } {
                        port.position = position.clone();
                    }
                }
            }
            for route in &trial.routes {
                for port in ports.iter_mut().filter(|p| p.edge_id == route.id) {
                    let position = if port.endpoint == "source" {
                        route.points.first()
                    } else {
                        route.points.last()
                    };
                    port.position = position
                        .ok_or_else(|| format!("Empty constructed route {}", route.id))?
                        .clone();
                }
            }
            (
                trial.routes,
                trial
                    .delegated
                    .into_iter()
                    .map(|(id, f)| (id, format!("{f:?}")))
                    .collect(),
            )
        }
    };
    times.construction = crate::elapsed_nanos(started);
    checkpoint()?;
    let started = crate::stage_timer_start();
    let fixed_ids: BTreeSet<_> = fixed.iter().map(|r| r.id.as_str()).collect();
    let seeds: Vec<_> = crate::route_from_ports(
        &snapshot.graph,
        &ports,
        snapshot.options.self_loop_clearance,
    )
    .into_iter()
    .filter(|r| !fixed_ids.contains(r.id.as_str()))
    .collect();
    let seeds = if matches!(method, Method::SinglePass) {
        corner_loops(
            &snapshot.graph,
            &grown.boxes,
            &ports,
            (
                snapshot.options.self_loop_clearance,
                snapshot.options.required_clearance.unwrap_or(6.),
            ),
            seeds,
        )
    } else {
        seeds
    };
    if fixed_ids.len() != fixed.len() {
        return Err("Duplicate fixed completion route".into());
    }
    let delegated_ids: BTreeSet<_> = delegated.iter().map(|(id, _)| id.as_str()).collect();
    let seed_ids: BTreeSet<_> = seeds.iter().map(|r| r.id.as_str()).collect();
    if delegated_ids != seed_ids || delegated_ids.len() != delegated.len() {
        return Err("Completion delegation and fallback seed partition differ".into());
    }
    search.constructed_routes = fixed.len();
    search.delegated_routes = seeds.len();
    times.seeds = crate::elapsed_nanos(started);
    let started = crate::stage_timer_start();
    let fallback = if matches!(method, Method::SinglePass) {
        seeds
    } else {
        arqlet_routing::improve_until_counted_with_fixed(
            &snapshot.graph,
            &grown.boxes,
            &seeds,
            &fixed,
            crate::routing_params(&snapshot.options),
            stop,
            &mut search,
        )?
    };
    times.fallback = crate::elapsed_nanos(started);
    let (fallback, track_repair) =
        if matches!(method, Method::ChainsFixedTracks | Method::ChainsAligned) {
            let started = crate::stage_timer_start();
            let repair = session.repair(
                &snapshot.graph,
                &grown.boxes,
                &fallback,
                &fixed,
                crate::routing_params(&snapshot.options),
                stop,
            );
            checkpoint()?;
            let repair = repair?;
            times.track_repair = crate::elapsed_nanos(started);
            (repair.routes, Some(repair.work))
        } else {
            (fallback, None)
        };
    let started = crate::stage_timer_start();
    let mut by_id = BTreeMap::new();
    for route in fallback.into_iter().chain(fixed.iter().cloned()) {
        if by_id.insert(route.id.clone(), route).is_some() {
            return Err("Duplicate completed route".into());
        }
    }
    let mut routes = Vec::new();
    for edge in &snapshot.graph.edges {
        if edge.target.is_some() {
            routes.push(
                by_id
                    .remove(&edge.id)
                    .ok_or_else(|| format!("Missing completed route {}", edge.id))?,
            );
        }
    }
    if !by_id.is_empty() {
        return Err("Completed routes contain unknown or unresolved edges".into());
    }
    let ends = crate::ports::PortEnds::new(&ports);
    for route in &routes {
        let source = ends
            .get(&route.id, "source")
            .ok_or_else(|| format!("Missing completion source {}", route.id))?;
        let target = ends
            .get(&route.id, "target")
            .ok_or_else(|| format!("Missing completion target {}", route.id))?;
        let matches = |a: Option<&Point>, b: &Point| a.is_some_and(|a| a.x == b.x && a.y == b.y);
        if !matches(route.points.first(), &source.position)
            || !matches(route.points.last(), &target.position)
        {
            return Err(format!("Completion changed resolved endpoint {}", route.id).into());
        }
        if route
            .points
            .iter()
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
            || route
                .points
                .windows(2)
                .any(|p| (p[0].x - p[1].x).abs() > EPS && (p[0].y - p[1].y).abs() > EPS)
        {
            return Err(format!("Nonfinite or nonorthogonal completed route {}", route.id).into());
        }
    }
    // Keep canonical graph and local node coordinates distinct from scene boxes.
    let mut drawing = snapshot.clone();
    drawing.boxes = grown
        .boxes
        .iter()
        .map(|(id, r)| {
            let parent = snapshot
                .graph
                .nodes
                .iter()
                .find(|n| n.id == *id)
                .and_then(|n| n.parent_id.as_ref());
            let mut local = r.clone();
            if let Some(parent) = parent {
                local.x -= grown.boxes[parent].x;
                local.y -= grown.boxes[parent].y;
            }
            (id.clone(), local)
        })
        .collect();
    drawing.routes = routes;
    drawing.resolved_ports = ports;
    drawing.layered_plans = grown
        .plans
        .iter()
        .map(|(area, plan)| (area.clone(), plan.to_stored()))
        .collect();
    drawing.corridor_gaps.clear();
    drawing.boundary_crossings = crate::ports::boundaries(
        &drawing.graph,
        &drawing.boxes,
        &drawing.routes,
        &BTreeSet::new(),
    );
    drawing.geometry_issues = arqlet_scene::validate_geometry(
        &drawing.graph,
        &drawing.boxes,
        arqlet_scene::GeometryRules::default(),
    );
    crate::labels::decorate(&mut drawing);
    times.finalization = crate::elapsed_nanos(started);
    let started = crate::stage_timer_start();
    let track_conflicts = arqlet_routing::fixed_tracks::conflicts(&drawing.routes, spacing.track);
    times.evaluation = crate::elapsed_nanos(started);
    checkpoint()?;
    Ok(CompletionOutcome {
        drawing,
        fixed,
        delegated,
        channels,
        track_conflicts,
        search,
        track_repair,
        planning: selected.as_ref().map(|_| session.work.since(&before)),
        construction_reuse: session
            .construction
            .as_ref()
            .zip(construction_before.as_ref())
            .map(|(s, b)| s.work.since(b)),
        construction_times,
        coordination_work,
        times,
    })
}

fn evaluate(mut outcome: CompletionOutcome) -> Result<CompletedDrawing, crate::Abort> {
    let started = crate::stage_timer_start();
    let scene = arqlet_scene::build_scene(&outcome.drawing)?;
    outcome.times.finalization = outcome
        .times
        .finalization
        .saturating_add(crate::elapsed_nanos(started));
    let started = crate::stage_timer_start();
    let (key, metrics) = crate::published_evaluation(&outcome.drawing)?;
    let hard_violations = crate::hard_violations(
        &metrics,
        outcome.drawing.options.required_clearance.is_some(),
    );
    outcome.times.evaluation = outcome
        .times
        .evaluation
        .saturating_add(crate::elapsed_nanos(started));
    Ok(CompletedDrawing {
        scene,
        routes: outcome.drawing.routes.clone(),
        fixed: outcome.fixed,
        delegated: outcome.delegated,
        channels: outcome.channels,
        geometry_issues: outcome.drawing.geometry_issues.clone(),
        routing_issues: outcome.drawing.routing_issues.clone(),
        key,
        metrics,
        hard_violations,
        track_conflicts: outcome.track_conflicts,
        search: outcome.search,
        track_repair: outcome.track_repair,
        planning: outcome.planning,
        construction_reuse: outcome.construction_reuse,
        construction_times: outcome.construction_times,
        coordination_work: outcome.coordination_work,
        times: outcome.times,
        snapshot: outcome.drawing,
    })
}

fn input_plans(snapshot: &Snapshot) -> Result<LayeredPlans, String> {
    crate::validate(&snapshot.graph)?;
    if snapshot.options.routing_engine != RoutingEngine::Legacy
        || snapshot.options.bundling != Bundling::None
        || !snapshot.straight_edges.is_empty()
        || (!snapshot.graph.routing_orthogonal_only() && !snapshot.options.single_pass_mixed)
    {
        return Err("Routing completion requires the legacy router and unbundled default orthogonal routing".into());
    }
    if snapshot.boxes.len() != snapshot.graph.nodes.len()
        || snapshot
            .graph
            .nodes
            .iter()
            .any(|n| !snapshot.boxes.contains_key(&n.id))
    {
        return Err("Routing completion requires exactly one box per node".into());
    }
    snapshot
        .layered_plans
        .iter()
        .map(|(area, p)| {
            crate::layered_plan::AreaPlan::from_stored(p)
                .map(|p| (area.clone(), p))
                .ok_or_else(|| format!("Unsupported completion plan format for {area}"))
        })
        .collect()
}

/// A self-loop whose ports lie on adjacent sides turns around the node's corner: it leaves and enters along the
/// port normals and meets at the corner, so it stays outside its node and needs no room beyond the port offset.
fn corner_loops(
    graph: &arqlet_model::Graph,
    boxes: &BTreeMap<String, arqlet_model::Rect>,
    ports: &[ResolvedPort],
    (self_loop_clearance, clearance): (f64, f64),
    mut seeds: Vec<Route>,
) -> Vec<Route> {
    use arqlet_model::PortSide;
    let flow = |side: PortSide| matches!(side, PortSide::Top | PortSide::Bottom);
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let preferred = 20. + self_loop_clearance;
    for route in &mut seeds {
        let Some(edge) = graph.edges.iter().find(|e| e.id == route.id) else {
            continue;
        };
        if edge.target.as_deref() != Some(edge.source.as_str()) {
            continue;
        }
        let end = |endpoint: &str| {
            ports
                .iter()
                .find(|p| p.edge_id == route.id && p.endpoint == endpoint)
        };
        let (Some(source), Some(target)) = (end("source"), end("target")) else {
            continue;
        };
        if flow(source.side) == flow(target.side) {
            continue;
        }
        // The loop runs beside its node, so other nodes on that side (siblings in the same rank, at the sibling gap) limit
        // how far it may go out: it keeps more than the clearance to them, or else runs in the middle of the free room.
        let mut own = vec![edge.source.as_str()];
        while let Some(Some(parent)) = parents.get(own[own.len() - 1]) {
            own.push(parent);
        }
        let free = |port: &ResolvedPort, other: &ResolvedPort| {
            let reach = crate::ports::outward(other, preferred);
            let (lo, hi) = if flow(port.side) {
                let (a, b) = (port.position.x, reach.x);
                (a.min(b) - clearance, a.max(b) + clearance)
            } else {
                let (a, b) = (port.position.y, reach.y);
                (a.min(b) - clearance, a.max(b) + clearance)
            };
            boxes
                .iter()
                .filter(|(id, _)| !own.contains(&id.as_str()))
                .filter_map(|(_, r)| {
                    let (span, near) = match port.side {
                        PortSide::Left => {
                            ((r.y, r.y + r.height), port.position.x - (r.x + r.width))
                        }
                        PortSide::Right => ((r.y, r.y + r.height), r.x - port.position.x),
                        PortSide::Top => ((r.x, r.x + r.width), port.position.y - (r.y + r.height)),
                        PortSide::Bottom => ((r.x, r.x + r.width), r.y - port.position.y),
                    };
                    (span.0 < hi && span.1 > lo && near >= 0.).then_some(near)
                })
                .fold(f64::INFINITY, f64::min)
        };
        let room = free(source, target).min(free(target, source));
        let distance = if room - preferred > clearance {
            preferred
        } else {
            preferred.min(room / 2.)
        };
        let depart = crate::ports::outward(source, distance);
        let arrive = crate::ports::outward(target, distance);
        let corner = if flow(source.side) {
            Point {
                x: arrive.x,
                y: depart.y,
            }
        } else {
            Point {
                x: depart.x,
                y: arrive.y,
            }
        };
        route.points = vec![
            source.position.clone(),
            depart,
            corner,
            arrive,
            target.position.clone(),
        ];
    }
    seeds
}
