//! Exact certificates for one repair decision, not a local-routing assumption.
use super::*;

type RouteKey = (String, usize, Vec<[u64; 2]>);
type OccupancyKey = Vec<(String, Vec<(bool, u64, u64, u64)>)>;
type ObstacleKey = Vec<(String, NodeShape, bool, [u64; 4])>;

#[derive(Clone, PartialEq)]
struct Input {
    route: RouteKey,
    params: [u64; 3],
    endpoints: (String, Option<String>),
    proposals: Vec<u64>,
    occupancy: OccupancyKey,
    obstacles: ObstacleKey,
}

#[derive(Clone)]
pub(super) struct Decision {
    input: Input,
    influence: Influence,
    chosen: Option<Route>,
    candidates: usize,
}

#[derive(Clone)]
struct Influence {
    axes: [Vec<Segment>; 2],
}

impl Influence {
    fn new(mut segments: Vec<Segment>) -> Self {
        segments.sort_unstable_by(|a, b| {
            a.horizontal
                .cmp(&b.horizontal)
                .then(a.coordinate.total_cmp(&b.coordinate))
                .then(a.low.total_cmp(&b.low))
                .then(a.high.total_cmp(&b.high))
        });
        let mut axes: [Vec<Segment>; 2] = Default::default();
        for s in segments {
            let axis = &mut axes[usize::from(s.horizontal)];
            if let Some(last) = axis.last_mut() {
                if last.coordinate.to_bits() == s.coordinate.to_bits() && s.low <= last.high {
                    last.high = last.high.max(s.high);
                    continue;
                }
            }
            axis.push(s);
        }
        Self { axes }
    }

    fn conflicts(&self, b: &Segment, track: f64) -> bool {
        let axis = &self.axes[usize::from(b.horizontal)];
        let start = axis.partition_point(|a| a.coordinate < b.coordinate - track);
        axis[start..]
            .iter()
            .take_while(|a| a.coordinate <= b.coordinate + track)
            .any(|a| {
                (a.coordinate - b.coordinate).abs() < track - EPS
                    && a.low < b.high
                    && b.low < a.high
            })
    }

    fn overlaps(&self, b: &Bounds) -> bool {
        self.axes.iter().enumerate().any(|(i, axis)| {
            let (lo, hi, from, to) = if i == 1 {
                (b[1], b[3], b[0], b[2])
            } else {
                (b[0], b[2], b[1], b[3])
            };
            let start = axis.partition_point(|a| a.coordinate < lo - EPS);
            axis[start..]
                .iter()
                .take_while(|a| a.coordinate <= hi + EPS)
                .any(|a| a.low <= to + EPS && from - EPS <= a.high)
        })
    }
}

pub(super) fn influence(route: &Route) -> Vec<Segment> {
    // Include tiny segments too: outline checks see actual endpoints, whereas
    // track conflicts omit sub-epsilon segments and project near-orthogonal ones.
    route
        .points
        .windows(2)
        .map(|w| {
            let (a, b) = (&w[0], &w[1]);
            let horizontal = horizontal(a, b);
            let (coordinate, from, to) = if horizontal {
                (a.y, a.x, b.x)
            } else {
                (a.x, a.y, b.y)
            };
            Segment {
                horizontal,
                coordinate,
                low: from.min(to),
                high: from.max(to),
            }
        })
        .collect()
}

/// One run-local entry per visited decision input. The same route may encounter
/// different occupancy after other routes move in later passes. A call replaces
/// the cache with only that call's states; rejected or cancelled calls retain
/// the previous cache. Forking keeps unrelated candidates from evicting it.
#[derive(Clone, Default)]
pub struct Session {
    entries: BTreeMap<String, Vec<Decision>>,
}

impl Session {
    pub fn repair(
        &mut self,
        graph: &Graph,
        rects: &BTreeMap<String, Rect>,
        moving: &[Route],
        fixed: &[Route],
        params: Params,
        stop: &dyn Fn() -> bool,
    ) -> Result<Repair, String> {
        let mut next = Self::default();
        let result = repair_impl(
            graph,
            rects,
            moving,
            fixed,
            params,
            stop,
            Some((&*self, &mut next)),
        )?;
        *self = next;
        Ok(result)
    }

    pub(super) fn find(
        &self,
        current: &Route,
        others: &[(&str, &Tracks)],
        obstacles: &[(String, Rect)],
        info: &NodeInfo<'_>,
        edge: &Edge,
        skip: &BTreeSet<String>,
        params: Params,
        proposals: &[Option<Events>],
    ) -> Option<Decision> {
        let states = self.entries.get(&current.id)?;
        let key = route_key(current);
        let proposal_key = proposal_key(current, proposals);
        let params_key = [
            params.clearance.to_bits(),
            params.track.to_bits(),
            params.own_clearance.to_bits(),
        ];
        states
            .iter()
            .find(|state| {
                state.input.route == key
                    && state.input.proposals == proposal_key
                    && state.input.params == params_key
                    && state.input.endpoints == (edge.source.clone(), edge.target.clone())
                    && state.input
                        == input(
                            current,
                            others,
                            obstacles,
                            info,
                            edge,
                            skip,
                            params,
                            &state.influence,
                            proposals,
                        )
            })
            .cloned()
    }

    pub(super) fn keep(&mut self, decision: Decision) {
        let states = self
            .entries
            .entry(decision.input.route.0.clone())
            .or_default();
        if let Some(old) = states.iter_mut().find(|s| s.input == decision.input) {
            *old = decision;
        } else {
            states.push(decision);
        }
    }
}

fn route_key(route: &Route) -> RouteKey {
    (
        route.id.clone(),
        route.weight,
        route
            .points
            .iter()
            .map(|p| [p.x.to_bits(), p.y.to_bits()])
            .collect(),
    )
}

fn proposal_key(current: &Route, proposals: &[Option<Events>]) -> Vec<u64> {
    let mut key = Vec::new();
    for (si, proposal) in proposals.iter().enumerate() {
        let Some(p) = proposal else { continue };
        for &target in &p.targets {
            if si > 0
                && si + 2 < current.points.len()
                && target >= p.floor - EPS
                && target <= p.ceiling + EPS
            {
                key.extend([0, si as u64, target.to_bits()]);
            }
            for &(lo, hi, lower, upper) in &p.bypasses {
                if target < lower - EPS || target > upper + EPS {
                    continue;
                }
                key.extend([1, si as u64, lo.to_bits(), hi.to_bits(), target.to_bits()]);
            }
        }
    }
    key
}

fn input(
    current: &Route,
    others: &[(&str, &Tracks)],
    obstacles: &[(String, Rect)],
    info: &NodeInfo<'_>,
    edge: &Edge,
    skip: &BTreeSet<String>,
    params: Params,
    influence: &Influence,
    proposals: &[Option<Events>],
) -> Input {
    let occupancy = others
        .iter()
        .filter_map(|(id, other)| {
            let segments: Vec<_> = other
                .segments
                .iter()
                .filter(|b| influence.conflicts(b, params.track))
                .map(|s| {
                    (
                        s.horizontal,
                        s.coordinate.to_bits(),
                        s.low.to_bits(),
                        s.high.to_bits(),
                    )
                })
                .collect();
            (!segments.is_empty()).then(|| ((*id).to_string(), segments))
        })
        .collect();
    let obstacles = obstacles
        .iter()
        .filter_map(|(id, r)| {
            let endpoint = id == &edge.source || edge.target.as_ref() == Some(id);
            if !endpoint && skip.contains(id) {
                return None;
            }
            let shape = shape_in(info, id);
            let mut b = obstacle(id, r, shape, edge, params).bounds();
            if endpoint && params.own_clearance > 0. {
                let c = params.own_clearance;
                b.x -= c;
                b.y -= c;
                b.width += 2. * c;
                b.height += 2. * c;
            }
            let bounds = rect_bounds(&b);
            let overlaps = influence.overlaps(&bounds);
            (endpoint || overlaps).then(|| {
                (
                    id.clone(),
                    shape,
                    endpoint,
                    [
                        r.x.to_bits(),
                        r.y.to_bits(),
                        r.width.to_bits(),
                        r.height.to_bits(),
                    ],
                )
            })
        })
        .collect();
    Input {
        route: route_key(current),
        params: [
            params.clearance.to_bits(),
            params.track.to_bits(),
            params.own_clearance.to_bits(),
        ],
        endpoints: (edge.source.clone(), edge.target.clone()),
        proposals: proposal_key(current, proposals),
        occupancy,
        obstacles,
    }
}

impl Decision {
    pub(super) fn record(
        current: &Route,
        others: &[(&str, &Tracks)],
        obstacles: &[(String, Rect)],
        info: &NodeInfo<'_>,
        edge: &Edge,
        skip: &BTreeSet<String>,
        params: Params,
        influence: Vec<Segment>,
        proposals: &[Option<Events>],
        chosen: Option<Route>,
        candidates: usize,
    ) -> Self {
        let influence = Influence::new(influence);
        Self {
            input: input(
                current, others, obstacles, info, edge, skip, params, &influence, proposals,
            ),
            influence,
            chosen,
            candidates,
        }
    }

    pub(super) fn apply(&self, work: &mut Work) -> Option<Route> {
        work.decisions_reused += 1;
        work.candidates_avoided += self.candidates;
        self.chosen.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{fixture, route};

    #[test]
    fn outline_influence_includes_tiny_and_near_orthogonal_segments() {
        let tiny = route("tiny", &[(0., 0.), (EPS / 2., EPS / 2.)]);
        assert!(segments(&tiny).segments.is_empty());
        let index = Influence::new(influence(&tiny));
        assert!(index.overlaps(&[EPS / 4., EPS / 4., EPS / 2., EPS / 2.]));
        let tilted = route("tilted", &[(0., 0.), (100., EPS / 2.)]);
        let index = Influence::new(influence(&tilted));
        assert!(index.overlaps(&[99., EPS / 4., 100., EPS / 2.]));
    }

    #[test]
    fn indexed_influence_matches_direct_segments_at_tolerance_boundaries() {
        let mut seed = 59_u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed >> 32) % 23) as f64
        };
        let samples: Vec<_> = (0..256)
            .map(|i| {
                let (lo, hi) = (next(), next());
                Segment {
                    horizontal: i % 2 == 0,
                    coordinate: next(),
                    low: lo.min(hi),
                    high: lo.max(hi),
                }
            })
            .filter(|s| s.low < s.high)
            .collect();
        let index = Influence::new(samples.clone());
        for b in &samples {
            for delta in [0., 6. - 2. * EPS, 6. - 0.5 * EPS, 6., 7.] {
                let mut b = *b;
                b.coordinate += delta;
                let expected = samples.iter().any(|a| {
                    a.horizontal == b.horizontal
                        && (a.coordinate - b.coordinate).abs() < 6. - EPS
                        && a.low < b.high
                        && b.low < a.high
                });
                assert_eq!(index.conflicts(&b, 6.), expected);
                let bounds = [b.low, b.coordinate, b.high, b.coordinate + delta];
                let expected = samples.iter().any(|s| {
                    let r = if s.horizontal {
                        [s.low, s.coordinate, s.high, s.coordinate]
                    } else {
                        [s.coordinate, s.low, s.coordinate, s.high]
                    };
                    let expanded = [r[0] - EPS, r[1] - EPS, r[2] + EPS, r[3] + EPS];
                    !apart(&expanded, &bounds)
                });
                assert_eq!(index.overlaps(&bounds), expected);
            }
        }
    }
    use super::*;

    fn identical(a: &Repair, b: &Repair) {
        assert_eq!(a.routes.len(), b.routes.len());
        for (a, b) in a.routes.iter().zip(&b.routes) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.points, b.points);
            assert_eq!(a.weight, b.weight);
        }
        assert_eq!(a.unresolved, b.unresolved);
        assert_eq!(a.work.moves, b.work.moves);
        assert_eq!(a.work.passes, b.work.passes);
        assert_eq!(a.work.initial_pairs, b.work.initial_pairs);
        assert_eq!(a.work.remaining_pairs, b.work.remaining_pairs);
    }

    #[test]
    fn warm_repair_replays_moves_and_failed_decisions_and_keeps_cancellation() {
        let (mut graph, rects, moving, mut fixed) = fixture();
        for failed in [false, true] {
            if failed {
                graph.edges[1].source = "a".into();
                graph.edges[1].target = Some("b".into());
                fixed = vec![route("fixed", &[(10., 5.), (100., 5.)])];
            }
            let mut session = Session::default();
            let fresh = repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap();
            let cold = session
                .repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                    false
                })
                .unwrap();
            identical(&fresh, &cold);
            assert!(cold.work.decisions_computed > 0);
            let warm = session
                .repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                    false
                })
                .unwrap();
            identical(&fresh, &warm);
            assert_eq!(warm.work.decisions_computed, 0);
            assert!(warm.work.decisions_reused > 0);
            assert_eq!(warm.work.candidates, 0);
            assert_eq!(warm.work.candidates_avoided, fresh.work.candidates);
            assert!(session
                .repair(&graph, &rects, &moving, &fixed, Params::default(), &|| true)
                .is_err());
            let again = session
                .repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                    false
                })
                .unwrap();
            assert_eq!(
                again.work.decisions_computed, 0,
                "cancelled calls must not erase the old cache"
            );
        }
    }

    #[test]
    fn remote_geometry_reuses_but_projected_events_and_corridor_limiters_invalidate() {
        let (mut graph, mut rects, moving, mut fixed) = fixture();
        graph.nodes.push(Node {
            id: "remote".into(),
            kind: "n".into(),
            label: "".into(),
            parent_id: None,
            shape: NodeShape::Rect,
        });
        rects.insert(
            "remote".into(),
            Rect {
                x: 10000.,
                y: 10000.,
                width: 10.,
                height: 10.,
            },
        );
        graph.edges.push(Edge {
            id: "remote".into(),
            kind: "e".into(),
            source: "c".into(),
            target: Some("d".into()),
            weight: None,
        });
        fixed.push(route("remote", &[(10000., 10000.), (10100., 10000.)]));
        let mut baseline = Session::default();
        baseline
            .repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap();
        rects.get_mut("remote").unwrap().x += 10.;
        fixed[1].points.iter_mut().for_each(|p| p.x += 10.);
        let mut fork = baseline.clone();
        let cached = fork
            .repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap();
        identical(
            &cached,
            &repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap(),
        );
        assert_eq!(
            cached.work.decisions_computed, 0,
            "unrelated geometry must not invalidate"
        );
        graph.nodes[0].label = "metadata is not geometry".into();
        assert!(
            fork.repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap()
            .work
            .decisions_reused
                > 0
        );
        // A projected event outside every admissible corridor does not produce
        // a proposal. It may change without invalidating the decision.
        fixed[1] = route("remote", &[(40., 10000.), (70., 10000.)]);
        let mut fork = baseline.clone();
        let cached = fork
            .repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap();
        identical(
            &cached,
            &repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap(),
        );
        assert_eq!(cached.work.decisions_computed, 0);
        fixed[1] = route("remote", &[(40., 14.), (70., 14.)]);
        let cached = fork
            .repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap();
        identical(
            &cached,
            &repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap(),
        );
        assert!(
            cached.work.decisions_computed > 0,
            "a newly admissible event must invalidate"
        );
        fixed.pop();
        // A newly tighter projected obstacle changes the actual corridor.
        rects.get_mut("remote").unwrap().x = 40.;
        rects.get_mut("remote").unwrap().y = 12.;
        let cached = baseline
            .repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap();
        identical(
            &cached,
            &repair(&graph, &rects, &moving, &fixed, Params::default(), &|| {
                false
            })
            .unwrap(),
        );
        assert!(cached.work.decisions_computed > 0);
    }

    #[test]
    fn dependency_changes_match_fresh_with_both_axes_and_generated_occupancy() {
        let (mut graph, mut rects, mut moving, fixed) = fixture();
        graph.nodes.push(Node {
            id: "obstacle".into(),
            kind: "n".into(),
            label: "".into(),
            parent_id: None,
            shape: NodeShape::Rect,
        });
        rects.insert(
            "obstacle".into(),
            Rect {
                x: 40.,
                y: 1000.,
                width: 10.,
                height: 10.,
            },
        );
        graph.edges.push(Edge {
            id: "second".into(),
            kind: "e".into(),
            source: "c".into(),
            target: Some("d".into()),
            weight: None,
        });
        moving.push(route("second", &[(40., 23.), (70., 23.)]));
        let mut session = Session::default();
        let mut seed = 49_u64;
        for i in 0..128 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let value = (seed >> 32) % 23;
            let params = Params {
                track: 4. + (i % 4) as f64,
                clearance: (i % 3) as f64,
                own_clearance: (i % 2) as f64,
                ..Params::default()
            };
            let mut rects = rects.clone();
            let mut graph = graph.clone();
            let mut fixed = fixed.clone();
            let mut moving = moving.clone();
            if i % 3 == 0 {
                graph.nodes[0].parent_id = Some("obstacle".into());
            }
            if i % 7 == 0 {
                graph.edges[0].source = "c".into();
            }
            if i % 11 == 0 {
                moving[0].points[0].x -= 3.;
            }
            rects.get_mut("obstacle").unwrap().y = value as f64;
            graph.nodes[2].shape = if i % 2 == 0 {
                NodeShape::Circle
            } else {
                NodeShape::Rect
            };
            fixed[0].points[1].y = 7. + (value % 3) as f64;
            fixed[0].points[2].y = fixed[0].points[1].y;
            if i % 5 == 0 {
                moving.reverse();
            }
            if i % 2 == 1 {
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
            let cold = repair(&graph, &rects, &moving, &fixed, params, &|| false).unwrap();
            let cached = session
                .repair(&graph, &rects, &moving, &fixed, params, &|| false)
                .unwrap();
            identical(&cold, &cached);
            let warm = session
                .repair(&graph, &rects, &moving, &fixed, params, &|| false)
                .unwrap();
            identical(&cold, &warm);
        }
    }
}
