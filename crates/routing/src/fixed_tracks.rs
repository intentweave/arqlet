//! Provisional final compaction around immutable routes. Used by the abstract
//! routing opt-in, not the default router. Crossings remain allowed; parallel
//! track distance does not.
use super::*;
mod memo;
pub use memo::Session;

#[derive(Clone, Copy)]
struct Segment {
    horizontal: bool,
    coordinate: f64,
    low: f64,
    high: f64,
}

#[derive(Clone)]
struct Tracks {
    bounds: Bounds,
    segments: Vec<Segment>,
}

fn segments(route: &Route) -> Tracks {
    let segments = route
        .points
        .windows(2)
        .filter_map(|w| {
            let (a, b) = (&w[0], &w[1]);
            if horizontal(a, b) && (a.x - b.x).abs() > EPS {
                Some(Segment {
                    horizontal: true,
                    coordinate: a.y,
                    low: a.x.min(b.x),
                    high: a.x.max(b.x),
                })
            } else if vertical(a, b) && (a.y - b.y).abs() > EPS {
                Some(Segment {
                    horizontal: false,
                    coordinate: a.x,
                    low: a.y.min(b.y),
                    high: a.y.max(b.y),
                })
            } else {
                None
            }
        })
        .collect();
    Tracks {
        bounds: route_bounds(route),
        segments,
    }
}

fn conflict(a: &Tracks, b: &Tracks, track: f64) -> bool {
    let expanded = [
        a.bounds[0] - track,
        a.bounds[1] - track,
        a.bounds[2] + track,
        a.bounds[3] + track,
    ];
    if apart(&expanded, &b.bounds) {
        return false;
    }
    a.segments.iter().any(|a| {
        b.segments.iter().any(|b| {
            a.horizontal == b.horizontal
                && (a.coordinate - b.coordinate).abs() < track - EPS
                && a.low < b.high
                && b.low < a.high
        })
    })
}

/// All strict parallel track-conflict pairs in input order, including endpoint
/// legs. There is no shared-endpoint exemption. Orthogonal, unbundled routes
/// and finite positive spacing are prerequisites.
pub fn conflicts(routes: &[Route], track: f64) -> Vec<(String, String)> {
    let segments: Vec<_> = routes.iter().map(segments).collect();
    conflict_pairs(routes, &segments, track)
}

fn conflict_pairs(routes: &[Route], segments: &[Tracks], track: f64) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for (i, a) in segments.iter().enumerate() {
        for (j, b) in segments.iter().enumerate().skip(i + 1) {
            if conflict(a, b, track) {
                pairs.push((routes[i].id.clone(), routes[j].id.clone()));
            }
        }
    }
    pairs
}

#[derive(Debug, Default)]
pub struct Work {
    pub candidates: usize,
    pub moves: usize,
    pub passes: usize,
    pub segment_projections_computed: usize,
    pub projection_pair_checks: usize,
    pub initial_pairs: usize,
    pub remaining_pairs: usize,
    pub decisions_computed: usize,
    pub decisions_reused: usize,
    pub candidates_avoided: usize,
}

#[derive(Debug)]
pub struct Repair {
    /// Only moving routes, in their original order.
    pub routes: Vec<Route>,
    /// Includes moving/moving and moving/fixed pairs, never fixed/fixed.
    pub unresolved: Vec<(String, String)>,
    pub work: Work,
}

struct Events {
    floor: f64,
    ceiling: f64,
    bypasses: Vec<(f64, f64, f64, f64)>,
    targets: Vec<f64>,
}

fn events(
    current: &Route,
    si: usize,
    others: &[&Tracks],
    edge: &Edge,
    skip: &BTreeSet<String>,
    obstacles: &[(String, Rect)],
    info: &NodeInfo<'_>,
    params: Params,
) -> Option<Events> {
    let (a, b) = (&current.points[si], &current.points[si + 1]);
    if (a.x - b.x).abs() <= EPS && (a.y - b.y).abs() <= EPS {
        return None;
    }
    let horizontal_segment = horizontal(a, b);
    let line = if horizontal_segment { a.y } else { a.x };
    let range = if horizontal_segment {
        (a.x.min(b.x), a.x.max(b.x))
    } else {
        (a.y.min(b.y), a.y.max(b.y))
    };
    let (floor, ceiling) = corridor_span(
        edge,
        skip,
        obstacles,
        info,
        params,
        horizontal_segment,
        line,
        range,
    );
    let across = |p: &Point| if horizontal_segment { p.y } else { p.x };
    let along = |p: &Point| if horizontal_segment { p.x } else { p.y };
    let interior = si > 0 && si + 2 < current.points.len();
    let mut targets = vec![floor, ceiling];
    if interior {
        targets.extend([
            across(&current.points[si - 1]),
            across(&current.points[si + 2]),
        ]);
    }
    let mut blocked: Option<(f64, f64)> = None;
    let mut bypasses = Vec::new();
    for other in others {
        let mut span: Option<(f64, f64)> = None;
        for segment in &other.segments {
            if segment.horizontal == horizontal_segment
                && (segment.coordinate - line).abs() < params.track - EPS
                && segment.low < range.1
                && range.0 < segment.high
            {
                let (low, high) = (segment.low.max(range.0), segment.high.min(range.1));
                span = Some(span.map_or((low, high), |(l, h)| (l.min(low), h.max(high))));
            }
        }
        bypasses.extend(span);
    }
    for other in others.iter().flat_map(|other| &other.segments) {
        if other.horizontal == horizontal_segment {
            if other.low < range.1 && range.0 < other.high {
                targets.extend([
                    other.coordinate - params.track,
                    other.coordinate + params.track,
                ]);
                if (other.coordinate - line).abs() < params.track - EPS {
                    let (low, high) = (other.low.max(range.0), other.high.min(range.1));
                    blocked = Some(blocked.map_or((low, high), |(l, h)| (l.min(low), h.max(high))));
                }
            }
        } else if [along(a), along(b)]
            .iter()
            .any(|coordinate| (*coordinate - other.coordinate).abs() < params.track - EPS)
        {
            targets.extend([other.low, other.high]);
        }
    }
    bypasses.extend(blocked);
    let bypasses = bypasses
        .into_iter()
        .map(|(low, high)| {
            let (lo, hi) = (
                (low - params.track).max(range.0),
                (high + params.track).min(range.1),
            );
            let (lower, upper) = corridor_span(
                edge,
                skip,
                obstacles,
                info,
                params,
                horizontal_segment,
                line,
                (lo, hi),
            );
            targets.extend([lower, upper]);
            (lo, hi, lower, upper)
        })
        .collect();
    targets.retain(|v| v.is_finite() && (*v - line).abs() > EPS);
    targets.sort_by(f64::total_cmp);
    targets.dedup_by(|a, b| *a == *b);
    Some(Events {
        floor,
        ceiling,
        bypasses,
        targets,
    })
}

/// Compact only moving routes after all legacy repair/spread passes. Candidate
/// coordinates are exact occupancy/corridor events, not an offset search.
/// A blocked interval may receive a local orthogonal bypass; this can add bends.
/// A move must remove a conflicting partner without adding another, preserve
/// endpoint directions and introduce no node/required-own-clearance hit.
/// Thus the finite pair count strictly decreases; unresolved pairs are explicit,
/// not a feasibility claim. Fixed/fixed conflicts are outside this repair.
pub fn repair(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    moving: &[Route],
    fixed: &[Route],
    params: Params,
    stop: &dyn Fn() -> bool,
) -> Result<Repair, String> {
    repair_impl(graph, rects, moving, fixed, params, stop, None)
}

fn repair_impl(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    moving: &[Route],
    fixed: &[Route],
    params: Params,
    stop: &dyn Fn() -> bool,
    mut memo: Option<(&Session, &mut Session)>,
) -> Result<Repair, String> {
    arqlet_model::validate(graph)?;
    if !params.track.is_finite()
        || params.track <= 0.
        || !params.clearance.is_finite()
        || params.clearance < 0.
        || !params.own_clearance.is_finite()
        || params.own_clearance < 0.
    {
        return Err(
            "Fixed-track repair requires finite positive spacing and nonnegative clearances".into(),
        );
    }
    if graph.nodes.iter().any(|n| !rects.contains_key(&n.id))
        || rects.values().any(|r| {
            ![r.x, r.y, r.width, r.height, r.x + r.width, r.y + r.height]
                .iter()
                .all(|v| v.is_finite())
                || r.width <= 0.
                || r.height <= 0.
        })
    {
        return Err("Fixed-track repair requires finite positive rectangles for every node".into());
    }
    let edges: BTreeMap<_, _> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut ids = BTreeSet::new();
    for route in moving.iter().chain(fixed) {
        if !ids.insert(route.id.as_str())
            || !edges.contains_key(route.id.as_str())
            || route.bundle_of.is_some()
            || route.points.len() < 2
            || route
                .points
                .iter()
                .any(|p| !p.x.is_finite() || !p.y.is_finite())
            || route.points.windows(2).any(|w| !axis_aligned(&w[0], &w[1]))
        {
            return Err(format!(
                "Invalid, duplicate or nonorthogonal fixed-track route {}",
                route.id
            ));
        }
    }
    let mut result = moving.to_vec();
    let mut result_segments: Vec<_> = result.iter().map(segments).collect();
    let fixed_segments: Vec<_> = fixed.iter().map(segments).collect();
    let mut work = Work::default();
    work.segment_projections_computed = result_segments.len() + fixed_segments.len();
    let pairs = |routes: &[Route], route_segments: &[Tracks], work: &mut Work| {
        let mut pairs = Vec::new();
        for i in 0..route_segments.len() {
            for j in i + 1..route_segments.len() {
                work.projection_pair_checks += 1;
                if conflict(&route_segments[i], &route_segments[j], params.track) {
                    pairs.push((routes[i].id.clone(), routes[j].id.clone()));
                }
            }
            for (j, fixed_track) in fixed_segments.iter().enumerate() {
                work.projection_pair_checks += 1;
                if conflict(&route_segments[i], fixed_track, params.track) {
                    pairs.push((routes[i].id.clone(), fixed[j].id.clone()));
                }
            }
        }
        pairs
    };
    let initial = pairs(&result, &result_segments, &mut work);
    work.initial_pairs = initial.len();
    if stop() {
        return Err("Fixed-track repair stopped".into());
    }
    if initial.is_empty() {
        work.remaining_pairs = 0;
        return Ok(Repair {
            routes: result,
            unresolved: initial,
            work,
        });
    }
    let obstacles: Vec<_> = rects
        .iter()
        .map(|(id, r)| (id.clone(), r.clone()))
        .collect();
    let info = parent_map(graph);
    loop {
        if stop() {
            return Err("Fixed-track repair stopped".into());
        }
        work.passes += 1;
        let mut changed = false;
        for index in 0..result.len() {
            if stop() {
                return Err("Fixed-track repair stopped".into());
            }
            let current = &result[index];
            let others: Vec<_> = result_segments
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != index)
                .map(|(_, tracks)| tracks)
                .chain(fixed_segments.iter())
                .collect();
            let current_segments = &result_segments[index];
            let partners: BTreeSet<_> = others
                .iter()
                .enumerate()
                .filter(|(_, other)| conflict(current_segments, other, params.track))
                .map(|(i, _)| i)
                .collect();
            if partners.is_empty() {
                continue;
            }
            let edge = edges[current.id.as_str()];
            let skip = excluded_with(&info, edge);
            let proposals = (0..current.points.len() - 1)
                .map(|si| {
                    if stop() {
                        return Err("Fixed-track repair stopped".to_string());
                    }
                    Ok(events(
                        current, si, &others, edge, &skip, &obstacles, &info, params,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?;
            let other_ids: Vec<_> = result
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != index)
                .map(|(_, r)| r.id.as_str())
                .chain(fixed.iter().map(|r| r.id.as_str()))
                .collect();
            let dependencies: Vec<_> = other_ids
                .iter()
                .copied()
                .zip(others.iter().copied())
                .collect();
            if let Some((previous, next)) = memo.as_mut() {
                if let Some(decision) = previous.find(
                    current,
                    &dependencies,
                    &obstacles,
                    &info,
                    edge,
                    &skip,
                    params,
                    &proposals,
                ) {
                    if let Some(route) = decision.apply(&mut work) {
                        result[index] = route;
                        result_segments[index] = segments(&result[index]);
                        work.moves += 1;
                        changed = true;
                    }
                    next.keep(decision);
                    continue;
                }
            }
            work.decisions_computed += 1;
            let candidates_before = work.candidates;
            let recording = memo.is_some();
            let mut influence = if recording {
                memo::influence(current)
            } else {
                Vec::new()
            };
            let own_zones: Vec<_> = rects
                .iter()
                .filter(|(id, _)| *id == &edge.source || edge.target.as_ref() == Some(*id))
                .map(|(id, r)| {
                    let c = params.own_clearance;
                    Outline::of(
                        shape_in(&info, id),
                        &Rect {
                            x: r.x - c,
                            y: r.y - c,
                            width: r.width + 2. * c,
                            height: r.height + 2. * c,
                        },
                    )
                })
                .collect();
            let hits = |route: &Route| -> BTreeSet<usize> {
                let mut hits: BTreeSet<_> = obstacles
                    .iter()
                    .enumerate()
                    .filter(|(_, (id, r))| {
                        let endpoint = id == &edge.source || edge.target.as_ref() == Some(id);
                        (endpoint || !skip.contains(id))
                            && hits_outline(
                                route,
                                &obstacle(id, r, shape_in(&info, id), edge, params),
                            )
                    })
                    .map(|(i, _)| i)
                    .collect();
                if params.own_clearance > 0. && route.points.len() >= 4 {
                    let inner = Route::new("", route.points[1..route.points.len() - 1].to_vec());
                    hits.extend(
                        own_zones
                            .iter()
                            .enumerate()
                            .filter(|(_, zone)| hits_outline(&inner, zone))
                            .map(|(i, _)| obstacles.len() + i),
                    );
                }
                hits
            };
            let before_hits = hits(current);
            let signs = stub_signs(current);
            let mut best: Option<(usize, usize, f64, Route)> = None;
            let mut consider = |candidate: Route, displacement: f64| {
                work.candidates += 1;
                let candidate = clean(candidate);
                if candidate.points.len() < 2
                    || candidate.points.first() != current.points.first()
                    || candidate.points.last() != current.points.last()
                    || candidate
                        .points
                        .iter()
                        .any(|p| !p.x.is_finite() || !p.y.is_finite())
                    || candidate
                        .points
                        .windows(2)
                        .any(|w| !axis_aligned(&w[0], &w[1]))
                    || stub_signs(&candidate) != signs
                {
                    return;
                }
                let candidate_segments = segments(&candidate);
                if recording {
                    influence.extend(memo::influence(&candidate));
                }
                let mut count = 0;
                for (j, other) in others.iter().enumerate() {
                    if conflict(&candidate_segments, other, params.track) {
                        if !partners.contains(&j) {
                            return;
                        }
                        count += 1;
                    }
                }
                if count < partners.len()
                    && best.as_ref().is_none_or(|(c, bends, d, _)| {
                        (count, candidate.points.len(), displacement) < (*c, *bends, *d)
                    })
                    && hits(&candidate).is_subset(&before_hits)
                {
                    best = Some((count, candidate.points.len(), displacement, candidate));
                }
            };
            for si in 0..current.points.len() - 1 {
                if stop() {
                    return Err("Fixed-track repair stopped".into());
                }
                let (a, b) = (&current.points[si], &current.points[si + 1]);
                if (a.x - b.x).abs() <= EPS && (a.y - b.y).abs() <= EPS {
                    continue;
                }
                let horizontal_segment = horizontal(a, b);
                let line = if horizontal_segment { a.y } else { a.x };
                let Some(proposal) = &proposals[si] else {
                    continue;
                };
                let (floor, ceiling) = (proposal.floor, proposal.ceiling);
                let bypasses = &proposal.bypasses;
                let along = |p: &Point| if horizontal_segment { p.x } else { p.y };
                let interior = si > 0 && si + 2 < current.points.len();
                for &target in &proposal.targets {
                    if stop() {
                        return Err("Fixed-track repair stopped".into());
                    }
                    if interior && target >= floor - EPS && target <= ceiling + EPS {
                        let mut candidate = current.clone();
                        if horizontal_segment {
                            candidate.points[si].y = target;
                            candidate.points[si + 1].y = target;
                        } else {
                            candidate.points[si].x = target;
                            candidate.points[si + 1].x = target;
                        }
                        consider(candidate, (target - line).abs());
                    }
                    for &(lo, hi, lower, upper) in bypasses {
                        if target < lower - EPS || target > upper + EPS {
                            continue;
                        }
                        // Bypass only the occupied span. Its ends are track-distance
                        // events, so the two jogs do not sit on the blocker's bends.
                        let (start, end) = if along(a) <= along(b) {
                            (lo, hi)
                        } else {
                            (hi, lo)
                        };
                        let point = |along, across| {
                            if horizontal_segment {
                                Point {
                                    x: along,
                                    y: across,
                                }
                            } else {
                                Point {
                                    x: across,
                                    y: along,
                                }
                            }
                        };
                        let mut candidate = current.clone();
                        candidate.points.splice(
                            si + 1..si + 1,
                            [
                                point(start, line),
                                point(start, target),
                                point(end, target),
                                point(end, line),
                            ],
                        );
                        consider(candidate, (target - line).abs());
                    }
                }
            }
            let chosen = best.map(|(_, _, _, route)| route);
            if let Some((_, next)) = memo.as_mut() {
                next.keep(memo::Decision::record(
                    current,
                    &dependencies,
                    &obstacles,
                    &info,
                    edge,
                    &skip,
                    params,
                    influence,
                    &proposals,
                    chosen.clone(),
                    work.candidates - candidates_before,
                ));
            }
            if let Some(candidate) = chosen {
                result[index] = candidate;
                result_segments[index] = segments(&result[index]);
                work.segment_projections_computed += 1;
                work.moves += 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let unresolved = pairs(&result, &result_segments, &mut work);
    work.remaining_pairs = unresolved.len();
    Ok(Repair {
        routes: result,
        unresolved,
        work,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn route(id: &str, points: &[(f64, f64)]) -> Route {
        Route::new(
            id,
            points.iter().map(|(x, y)| Point { x: *x, y: *y }).collect(),
        )
    }

    pub(super) fn fixture() -> (Graph, BTreeMap<String, Rect>, Vec<Route>, Vec<Route>) {
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: Default::default(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: ["a", "b", "c", "d"]
                .map(|id| Node {
                    id: id.into(),
                    kind: "node".into(),
                    label: id.into(),
                    parent_id: None,
                    shape: NodeShape::Rect,
                })
                .to_vec(),
            edges: [("moving", "a", "b"), ("fixed", "c", "d")]
                .map(|(id, source, target)| Edge {
                    id: id.into(),
                    kind: "edge".into(),
                    source: source.into(),
                    target: Some(target.into()),
                })
                .to_vec(),
            buses: Vec::new(),
        };
        let rects = [
            ("a", 0., 0.),
            ("b", 100., 0.),
            ("c", 30., 18.),
            ("d", 70., 18.),
        ]
        .map(|(id, x, y)| {
            (
                id.into(),
                Rect {
                    x,
                    y,
                    width: 10.,
                    height: 10.,
                },
            )
        })
        .into_iter()
        .collect();
        (
            graph,
            rects,
            vec![route("moving", &[(10., 5.), (100., 5.)])],
            vec![route(
                "fixed",
                &[(40., 23.), (40., 8.), (70., 8.), (70., 23.)],
            )],
        )
    }

    #[test]
    fn local_bypass_preserves_ports_and_fixed_routes_on_both_axes() {
        let (graph, mut rects, mut moving, mut fixed) = fixture();
        for transpose in [false, true] {
            if transpose {
                for r in rects.values_mut() {
                    std::mem::swap(&mut r.x, &mut r.y);
                    std::mem::swap(&mut r.width, &mut r.height);
                }
                for r in moving.iter_mut().chain(&mut fixed) {
                    for p in &mut r.points {
                        std::mem::swap(&mut p.x, &mut p.y);
                    }
                }
            }
            let before = fixed[0].points.clone();
            let result = repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap();
            assert_eq!(result.work.initial_pairs, 1);
            assert_eq!(result.work.moves, 1);
            assert_eq!(result.work.segment_projections_computed, 3);
            assert!(result.work.projection_pair_checks >= 2);
            assert!(result.unresolved.is_empty());
            assert_eq!(result.routes[0].points.first(), moving[0].points.first());
            assert_eq!(result.routes[0].points.last(), moving[0].points.last());
            assert_eq!(stub_signs(&result.routes[0]), stub_signs(&moving[0]));
            assert_eq!(fixed[0].points, before);
            assert_eq!(
                routes_hitting_nodes(&graph, &rects, &result.routes, Params::default()),
                0
            );
            assert_eq!(result.routes[0].points.len(), 6);
        }
    }

    #[test]
    fn conflict_free_repair_returns_unchanged_routes_without_a_repair_pass() {
        let (graph, rects, moving, _) = fixture();
        let fixed = vec![route("fixed", &[(40., 100.), (70., 100.)])];
        let result = repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
            false
        })
        .unwrap();
        assert_eq!(result.routes.len(), moving.len());
        assert_eq!(result.routes[0].id, moving[0].id);
        assert_eq!(
            result.routes[0]
                .points
                .iter()
                .map(|p| (p.x, p.y))
                .collect::<Vec<_>>(),
            moving[0]
                .points
                .iter()
                .map(|p| (p.x, p.y))
                .collect::<Vec<_>>()
        );
        assert!(result.unresolved.is_empty());
        assert_eq!(result.work.initial_pairs, 0);
        assert_eq!(result.work.remaining_pairs, 0);
        assert_eq!(result.work.passes, 0);
        assert_eq!(result.work.candidates, 0);
        assert_eq!(result.work.segment_projections_computed, 2);
        assert_eq!(result.work.projection_pair_checks, 1);
    }

    #[test]
    fn conflict_free_repair_still_observes_cancellation() {
        let (graph, rects, moving, _) = fixture();
        let fixed = vec![route("fixed", &[(40., 100.), (70., 100.)])];
        assert_eq!(
            repair(&graph, &rects, &moving, &fixed, Params::default(), &|| true).unwrap_err(),
            "Fixed-track repair stopped"
        );
    }

    #[test]
    fn unrepairable_shared_departure_is_reported_without_claiming_feasibility() {
        let (mut graph, rects, moving, _) = fixture();
        graph.edges[1].source = "a".into();
        graph.edges[1].target = Some("b".into());
        let fixed = vec![route("fixed", &[(10., 5.), (100., 5.)])];
        let result = repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
            false
        })
        .unwrap();
        assert_eq!(result.work.moves, 0);
        assert_eq!(result.work.initial_pairs, 1);
        assert_eq!(result.work.remaining_pairs, 1);
        assert_eq!(result.unresolved, vec![("moving".into(), "fixed".into())]);
        assert_eq!(result.routes[0].points, moving[0].points);
    }

    #[test]
    fn spacing_boundary_and_perpendicular_crossings_are_not_conflicts() {
        let a = route("a", &[(0., 0.), (10., 0.)]);
        for (dy, expected) in [(0., 1), (6. - 2e-6, 1), (6. - 0.5e-6, 0), (6., 0), (7., 0)] {
            let b = route("b", &[(0., dy), (10., dy)]);
            assert_eq!(conflicts(&[a.clone(), b], 6.).len(), expected);
        }
        let perpendicular = route("b", &[(5., -10.), (5., 10.)]);
        assert!(conflicts(&[a.clone(), perpendicular], 6.).is_empty());
        let touching = route("c", &[(10., 0.), (20., 0.)]);
        assert!(conflicts(&[a, touching], 6.).is_empty());
    }

    #[test]
    fn invalid_inputs_and_cancellation_are_explicit_errors() {
        let (graph, rects, moving, fixed) = fixture();
        assert!(
            repair(&graph, &rects, &moving, &fixed, Params::default(), &|| true)
                .unwrap_err()
                .contains("stopped")
        );
        let mut bad = fixed.clone();
        bad[0].id = moving[0].id.clone();
        assert!(repair(&graph, &rects, &moving, &bad, Params::default(), &|| false).is_err());
        bad = fixed.clone();
        bad[0].points[1].x += 1.;
        assert!(repair(&graph, &rects, &moving, &bad, Params::default(), &|| false).is_err());
        assert!(repair(
            &graph,
            &rects,
            &moving,
            &fixed,
            Params {
                track: f64::NAN,
                ..Params::default()
            },
            &|| false
        )
        .is_err());
    }

    #[test]
    fn new_obstacle_hits_are_rejected_and_required_own_clearance_is_kept() {
        let (mut graph, mut rects, moving, fixed) = fixture();
        graph.nodes.push(Node {
            id: "obstacle".into(),
            kind: "node".into(),
            label: "".into(),
            parent_id: None,
            shape: NodeShape::Rect,
        });
        rects.insert(
            "obstacle".into(),
            Rect {
                x: 40.,
                y: 0.,
                width: 30.,
                height: 3.,
            },
        );
        let result = repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
            false
        })
        .unwrap();
        assert_eq!(
            routes_hitting_nodes(&graph, &rects, &result.routes, Params::default()),
            0
        );
        assert!(result.unresolved.is_empty());
        assert!(result.routes[0].points.iter().any(|p| p.y > 8.));
        let (graph, rects, moving, fixed) = fixture();
        let params = Params {
            own_clearance: 6.,
            ..Params::default()
        };
        let result = repair(&graph, &rects, &moving, &fixed, params, &|| false).unwrap();
        assert!(result.unresolved.is_empty());
        let inner = Route::new(
            "",
            result.routes[0].points[1..result.routes[0].points.len() - 1].to_vec(),
        );
        for id in ["a", "b"] {
            let r = &rects[id];
            assert!(!hits_outline(
                &inner,
                &Outline::Rect(Rect {
                    x: r.x - 6.,
                    y: r.y - 6.,
                    width: r.width + 12.,
                    height: r.height + 12.
                })
            ));
        }
    }

    #[test]
    fn moving_pairs_and_unrelated_routes_are_handled_without_touching_fixed_input() {
        let (mut graph, rects, mut moving, mut fixed) = fixture();
        graph.edges.push(Edge {
            id: "second".into(),
            kind: "edge".into(),
            source: "c".into(),
            target: Some("d".into()),
        });
        let mut second = fixed[0].clone();
        second.id = "second".into();
        moving.push(second);
        fixed.clear();
        let original = conflicts(&moving, 6.);
        let repaired = repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
            false
        })
        .unwrap();
        assert!(repaired
            .unresolved
            .iter()
            .all(|pair| original.contains(pair)));
        assert!(repaired.unresolved.is_empty());
        assert_eq!(repaired.routes[1].points, moving[1].points);
        assert!(fixed.is_empty());
    }

    #[test]
    fn repair_is_deterministic_idempotent_and_never_introduces_conflict_partners() {
        let (graph, rects, mut moving, mut fixed) = fixture();
        for step in 0..64 {
            let dy = step as f64 / 16.;
            fixed[0].points[1].y = 5. + dy;
            fixed[0].points[2].y = 5. + dy;
            moving[0].points = vec![Point { x: 10., y: 5. }, Point { x: 100., y: 5. }];
            let a = repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap();
            let b = repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap();
            assert_eq!(a.routes[0].points, b.routes[0].points);
            assert!(a.work.remaining_pairs <= a.work.initial_pairs);
            assert!(a.work.moves <= a.work.initial_pairs);
            let again = repair(
                &graph,
                &rects,
                &a.routes,
                &fixed,
                Params::default(),
                &|| false,
            )
            .unwrap();
            assert_eq!(a.routes[0].points, again.routes[0].points);
            assert_eq!(again.work.moves, 0);
        }
    }
}
