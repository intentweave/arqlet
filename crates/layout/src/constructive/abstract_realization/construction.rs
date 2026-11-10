//! Provisional fixed-path reuse. Occupancy acceptance always uses current routes.
//! Clearance deliberately matches the constructor's inflated rectangular boxes,
//! not the separate fallback router's outline-based rules.
use super::*;

#[derive(Clone, Debug, Default)]
pub struct Times {
    pub edge_preparation: u64,
    pub channels: u64,
    pub coordination: u64,
    pub materialization: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Work {
    pub obstacle_preparations_avoided: usize,
    pub shared_indexes: usize,
    pub geometries_computed: usize,
    pub geometries_reused: usize,
    pub obstacle_decisions_computed: usize,
    pub obstacle_decisions_reused: usize,
    pub obstacle_tests: usize,
    pub occupancy_checks: usize,
}

impl Work {
    pub(super) fn since(&self, before: &Self) -> Self {
        Self {
            obstacle_preparations_avoided: self.obstacle_preparations_avoided
                - before.obstacle_preparations_avoided,
            shared_indexes: self.shared_indexes - before.shared_indexes,
            geometries_computed: self.geometries_computed - before.geometries_computed,
            geometries_reused: self.geometries_reused - before.geometries_reused,
            obstacle_decisions_computed: self.obstacle_decisions_computed
                - before.obstacle_decisions_computed,
            obstacle_decisions_reused: self.obstacle_decisions_reused
                - before.obstacle_decisions_reused,
            obstacle_tests: self.obstacle_tests - before.obstacle_tests,
            occupancy_checks: self.occupancy_checks - before.occupancy_checks,
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
struct GeometryKey {
    vertical: bool,
    reversed: bool,
    ends: [[u64; 2]; 2],
    corners: Vec<[u64; 2]>,
}

type ObstacleKey = Vec<(String, [u64; 4])>;

#[derive(Clone)]
struct Entry {
    key: GeometryKey,
    points: Vec<Point>,
    segments: Vec<Segment>,
    route: Route,
    obstacles: ObstacleKey,
    clear: bool,
}

#[derive(Clone, Default)]
pub(super) struct Session {
    entries: BTreeMap<String, Entry>,
    pub work: Work,
}

pub(super) struct Obstacles {
    boxes: Vec<(String, Rect)>,
    index: BoxIndex,
}

impl Obstacles {
    pub(super) fn new(boxes: &BTreeMap<String, Rect>, margin: f64) -> Self {
        let boxes: Vec<_> = boxes
            .iter()
            .map(|(id, r)| (id.clone(), inflate(r, margin)))
            .collect();
        let bounds: Vec<_> = boxes
            .iter()
            .map(|(_, r)| [r.x, r.y, r.x + r.width, r.y + r.height])
            .collect();
        Self {
            boxes,
            index: BoxIndex::new(&bounds),
        }
    }

    fn relevant(&self, points: &[Point], skip: &BTreeSet<&str>) -> Vec<usize> {
        let mut found = BTreeSet::new();
        for w in points.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            let q = [a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)];
            self.index.any(q, |i| {
                let (id, r) = &self.boxes[i];
                if !skip.contains(id.as_str())
                    && q[2] >= r.x
                    && q[0] <= r.x + r.width
                    && q[3] >= r.y
                    && q[1] <= r.y + r.height
                {
                    found.insert(i);
                }
                false
            });
        }
        found.into_iter().collect()
    }
}

/// The accepted segments by track line, so that the occupancy check of a route looks at the neighbouring lines instead of
/// every accepted segment. Its cost otherwise grows with the square of the segments (15 % of the synthetic-2k import,
/// benchmarks/results/single-pass-levers-20261106.md). It answers whether a conflict exists, which does not depend on the
/// order; the caller keeps the segments in acceptance order for the report of a failure.
pub(super) struct Occupancy {
    track: f64,
    lines: BTreeMap<(bool, i64), Vec<Segment>>,
}

impl Occupancy {
    pub(super) fn new(track: f64) -> Self {
        Self {
            track,
            lines: BTreeMap::new(),
        }
    }

    fn line(&self, coord: f64) -> i64 {
        (coord / self.track).floor() as i64
    }

    /// Conflicting segments are less than a track apart (`conflicts`), so they lie on the same or a neighbouring line.
    fn conflict(&self, segment: &Segment) -> bool {
        let line = self.line(segment.coord);
        (line - 1..=line + 1).any(|l| {
            self.lines
                .get(&(segment.horizontal, l))
                .is_some_and(|found| found.iter().any(|a| conflicts(a, segment, self.track)))
        })
    }

    fn insert(&mut self, segment: Segment) {
        let line = self.line(segment.coord);
        self.lines
            .entry((segment.horizontal, line))
            .or_default()
            .push(segment);
    }
}

impl Session {
    pub(super) fn retain(&mut self, visited: &BTreeSet<String>) {
        self.entries.retain(|id, _| visited.contains(id));
    }

    pub(super) fn build(
        &mut self,
        prepared: &Prepared,
        corners: &[(f64, f64)],
        obstacles: &Obstacles,
        skip: &BTreeSet<&str>,
        assigned: &mut Vec<Segment>,
        occupied: &mut Occupancy,
        counters: &RefCell<ConstructionCounters>,
    ) -> Option<(Route, usize)> {
        counters.borrow_mut().planned_attempts += 1;
        let bits = |p: &Point| [p.x.to_bits(), p.y.to_bits()];
        let key = GeometryKey {
            vertical: prepared.vertical,
            reversed: prepared.reversed,
            ends: [bits(&prepared.start), bits(&prepared.end)],
            corners: corners
                .iter()
                .map(|&(a, b)| [a.to_bits(), b.to_bits()])
                .collect(),
        };
        let entry = self
            .entries
            .entry(prepared.edge_id.clone())
            .or_insert_with(|| Entry {
                key: key.clone(),
                points: Vec::new(),
                segments: Vec::new(),
                route: Route::new(&prepared.edge_id, Vec::new()),
                obstacles: Vec::new(),
                clear: false,
            });
        let geometry_reused = entry.key == key && !entry.points.is_empty();
        if geometry_reused {
            self.work.geometries_reused += 1;
        } else {
            entry.key = key;
            entry.points = planned_points(prepared, corners);
            entry.segments = segments_of(&entry.points);
            entry.route = finish_planned(prepared, entry.points.clone());
            self.work.geometries_computed += 1;
        }
        let relevant = obstacles.relevant(&entry.points, skip);
        let obstacle_key: Vec<_> = relevant
            .iter()
            .map(|&i| {
                let (id, r) = &obstacles.boxes[i];
                (
                    id.clone(),
                    [
                        r.x.to_bits(),
                        r.y.to_bits(),
                        r.width.to_bits(),
                        r.height.to_bits(),
                    ],
                )
            })
            .collect();
        if geometry_reused && entry.obstacles == obstacle_key {
            self.work.obstacle_decisions_reused += 1;
        } else {
            self.work.obstacle_decisions_computed += 1;
            entry.clear = entry.points.windows(2).all(|w| {
                !relevant.iter().any(|&i| {
                    self.work.obstacle_tests += 1;
                    segment_hits(w[0].clone(), w[1].clone(), &obstacles.boxes[i].1)
                })
            });
            entry.obstacles = obstacle_key;
        }
        if !entry.clear {
            counters.borrow_mut().planned_obstacle_failures += 1;
            #[cfg(feature = "benchmark-timings")]
            counters
                .borrow_mut()
                .planned_failures
                .push((prepared.edge_id.clone(), "obstacle"));
            return None;
        }
        self.work.occupancy_checks += 1;
        if entry.segments.iter().any(|s| occupied.conflict(s)) {
            counters.borrow_mut().planned_track_failures += 1;
            #[cfg(feature = "benchmark-timings")]
            counters
                .borrow_mut()
                .planned_failures
                .push((prepared.edge_id.clone(), "track"));
            return None;
        }
        assigned.extend_from_slice(&entry.segments);
        for &segment in &entry.segments {
            occupied.insert(segment);
        }
        Some((entry.route.clone(), corners.len() / 2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    fn prepared(vertical: bool, reversed: bool) -> Prepared {
        let start = to_point(vertical, (0., 0.));
        let end = to_point(vertical, (100., 20.));
        Prepared {
            edge_id: "e".into(),
            from_node: "a".into(),
            to_node: "b".into(),
            vertical,
            reversed,
            from: (0., 0.),
            to: (100., 20.),
            band: (6., 94.),
            obstacles: Vec::new(),
            obstacle_index: std::rc::Rc::new(BoxIndex::new(&[])),
            start,
            end,
            lead: (Vec::new(), Vec::new()),
        }
    }

    fn check(
        session: &mut Session,
        p: &Prepared,
        corners: &[(f64, f64)],
        boxes: &BTreeMap<String, Rect>,
        skip: &BTreeSet<&str>,
        taken: &[Segment],
        margin: f64,
        track: f64,
    ) -> (bool, Work) {
        let mut eager = p.clone();
        eager.obstacles = boxes
            .iter()
            .filter(|(id, _)| !skip.contains(id.as_str()))
            .map(|(_, r)| inflate(r, margin))
            .collect();
        let before = session.work.clone();
        let counters = RefCell::new(ConstructionCounters::default());
        let fresh_counters = RefCell::new(ConstructionCounters::default());
        let mut assigned = taken.to_vec();
        let mut fresh_assigned = taken.to_vec();
        let mut occupied = Occupancy::new(track);
        for &segment in taken {
            occupied.insert(segment);
        }
        let cached = session.build(
            p,
            corners,
            &Obstacles::new(boxes, margin),
            skip,
            &mut assigned,
            &mut occupied,
            &counters,
        );
        let fresh = build_planned(&eager, corners, &mut fresh_assigned, track, &fresh_counters);
        assert_eq!(
            serde_json::to_value(&cached).unwrap(),
            serde_json::to_value(&fresh).unwrap()
        );
        assert_eq!(*counters.borrow(), *fresh_counters.borrow());
        let key = |segments: &[Segment]| {
            segments
                .iter()
                .map(|s| {
                    (
                        s.horizontal,
                        s.coord.to_bits(),
                        s.range.0.to_bits(),
                        s.range.1.to_bits(),
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(key(&assigned), key(&fresh_assigned));
        (cached.is_some(), session.work.since(&before))
    }

    #[test]
    fn occupancy_finds_exactly_the_conflicts_of_a_scan_over_all_segments() {
        let segment = |horizontal: bool, coord: f64, from: f64| Segment {
            horizontal,
            coord,
            range: (from, from + 30.),
        };
        for track in [0.5, 8., 12.5, 40.] {
            let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
            let mut next = move || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            let mut occupied = Occupancy::new(track);
            let mut taken: Vec<Segment> = Vec::new();
            let (mut hits, mut misses) = (0, 0);
            for i in 0..4000 {
                // Many coordinates exactly a track or just under it apart, on both sides of zero and of a line border.
                let steps = (next() % 80) as f64 - 40.;
                let offset = [0., track - 2e-6, track - 5e-7, track, 1e-7][(next() % 5) as usize];
                let s = segment(
                    next() % 2 == 0,
                    steps * track + offset,
                    (next() % 400) as f64 - 200.,
                );
                let scan = taken.iter().any(|a| conflicts(a, &s, track));
                assert_eq!(occupied.conflict(&s), scan, "track {track}, segment {i}");
                if scan {
                    hits += 1;
                } else {
                    misses += 1;
                }
                if i % 8 == 0 {
                    taken.push(s);
                    occupied.insert(s);
                }
            }
            assert!(hits > 100 && misses > 100, "{hits} hits, {misses} misses");
        }
    }

    #[test]
    fn exact_path_and_clearance_reuse_never_replay_track_acceptance() {
        for vertical in [false, true] {
            for reversed in [false, true] {
                let mut session = Session::default();
                let mut p = prepared(vertical, reversed);
                let mut boxes = BTreeMap::from([("remote".into(), rect(1000., 1000., 20., 20.))]);
                let corners = vec![(50., 0.), (50., 20.)];
                let skip = BTreeSet::new();
                let (accepted, w) = check(&mut session, &p, &corners, &boxes, &skip, &[], 2., 6.);
                assert!(accepted);
                assert_eq!(w.geometries_computed, 1);
                let (accepted, w) = check(&mut session, &p, &corners, &boxes, &skip, &[], 2., 6.);
                assert!(accepted);
                assert_eq!(w.geometries_reused, 1);
                assert_eq!(w.obstacle_decisions_reused, 1);
                let taken = segments_of(&planned_points(&p, &corners));
                let (accepted, w) =
                    check(&mut session, &p, &corners, &boxes, &skip, &taken, 2., 6.);
                assert!(!accepted);
                assert_eq!(w.occupancy_checks, 1);
                assert_eq!(w.obstacle_decisions_reused, 1);
                assert!(check(&mut session, &p, &corners, &boxes, &skip, &[], 2., 6.).0);

                let obstacle = if vertical {
                    rect(-1., 20., 2., 10.)
                } else {
                    rect(20., -1., 10., 2.)
                };
                boxes.insert("foreign".into(), obstacle);
                let (accepted, w) = check(&mut session, &p, &corners, &boxes, &skip, &[], 2., 6.);
                assert!(!accepted);
                assert_eq!(w.obstacle_decisions_computed, 1);
                let (accepted, w) = check(&mut session, &p, &corners, &boxes, &skip, &[], 2., 6.);
                assert!(!accepted);
                assert_eq!(w.obstacle_decisions_reused, 1);
                let (accepted, w) = check(
                    &mut session,
                    &p,
                    &corners,
                    &boxes,
                    &BTreeSet::from(["foreign"]),
                    &[],
                    2.,
                    6.,
                );
                assert!(accepted);
                assert_eq!(w.obstacle_decisions_computed, 1);
                boxes.remove("foreign");
                boxes.get_mut("remote").unwrap().y += 100.;
                let (accepted, w) = check(&mut session, &p, &corners, &boxes, &skip, &[], 2., 6.);
                assert!(accepted);
                assert_eq!(w.obstacle_decisions_reused, 1);

                let shifted = vec![(55., 0.), (55., 20.)];
                assert_eq!(
                    check(&mut session, &p, &shifted, &boxes, &skip, &[], 2., 6.)
                        .1
                        .geometries_computed,
                    1
                );
                p.end = to_point(vertical, (100., 21.));
                assert_eq!(
                    check(&mut session, &p, &shifted, &boxes, &skip, &[], 2., 6.)
                        .1
                        .geometries_computed,
                    1
                );
                p.reversed = !p.reversed;
                assert_eq!(
                    check(&mut session, &p, &shifted, &boxes, &skip, &[], 2., 6.)
                        .1
                        .geometries_computed,
                    1
                );
                session.retain(&BTreeSet::new());
                assert!(session.entries.is_empty());
                assert_eq!(
                    check(&mut session, &p, &shifted, &boxes, &skip, &[], 2., 6.)
                        .1
                        .geometries_computed,
                    1
                );
            }
        }
    }

    #[test]
    fn shared_index_covers_actual_segment_hits_including_tolerance_and_clearance() {
        let mut seed = 81_u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            ((seed >> 32) % 201) as f64 - 100.
        };
        let boxes: BTreeMap<_, _> = (0..128)
            .map(|i| {
                (
                    format!("box{i}"),
                    rect(next(), next(), next().abs(), next().abs()),
                )
            })
            .collect();
        let mut points = Vec::new();
        for i in 0..256 {
            let (x, y) = (next(), next());
            points.push(Point { x, y });
            points.push(if i % 2 == 0 {
                Point {
                    x: next(),
                    y: y + EPS / 2.,
                }
            } else {
                Point {
                    x: x + EPS / 2.,
                    y: next(),
                }
            });
        }
        points.extend([
            Point { x: 0., y: 0. },
            Point {
                x: EPS / 2.,
                y: EPS / 2.,
            },
        ]);
        for margin in [0., EPS / 2., 6., 20.] {
            let obstacles = Obstacles::new(&boxes, margin);
            let skip = BTreeSet::from(["box0", "box1"]);
            for pair in points.chunks_exact(2) {
                let relevant = obstacles.relevant(pair, &skip);
                for (i, (id, r)) in obstacles.boxes.iter().enumerate() {
                    if !skip.contains(id.as_str())
                        && segment_hits(pair[0].clone(), pair[1].clone(), r)
                    {
                        assert!(relevant.contains(&i), "missed {id} at margin {margin}");
                    }
                }
            }
        }
    }

    #[test]
    fn changing_clearance_and_obstacles_matches_eager_construction_on_both_axes() {
        for vertical in [false, true] {
            let mut session = Session::default();
            let p = prepared(vertical, false);
            let corners = vec![(50., 0.), (50., 20.)];
            for i in 0..128 {
                let r = rect(10. + (i % 61) as f64, -15. + (i % 37) as f64, 10., 3.);
                let r = if vertical {
                    rect(r.y, r.x, r.height, r.width)
                } else {
                    r
                };
                let boxes = BTreeMap::from([("foreign".into(), r)]);
                let skip = if i % 3 == 0 {
                    BTreeSet::from(["foreign"])
                } else {
                    BTreeSet::new()
                };
                let margin = (i % 7) as f64;
                let track = 1. + (i % 11) as f64;
                check(
                    &mut session,
                    &p,
                    &corners,
                    &boxes,
                    &skip,
                    &[],
                    margin,
                    track,
                );
                let (_, w) = check(
                    &mut session,
                    &p,
                    &corners,
                    &boxes,
                    &skip,
                    &[],
                    margin,
                    track,
                );
                assert_eq!(w.geometries_computed, 0);
                assert_eq!(w.obstacle_decisions_computed, 0);
            }
        }
    }
}
