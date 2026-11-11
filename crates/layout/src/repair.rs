//! Geometric repair of organic areas (the root and rectangular containers). After the stress placement, the drawing is checked on its real geometry: the ports of
//! the straight edges as the layout draws them, the outlines of the nodes, and the clearance. Overlaps and straight edges
//! that enter a foreign outline are resolved by moving movable leaves of an organic area; a leaf of a container stays inside the
//! container's interior, whose size is kept. Pinned nodes never move and edges never bend.
//! When no admissible arrangement is found within the budget, the remaining conflicts are returned, not hidden.

use crate::{absolute_all, clearance, ports, spatial::SpatialHash, styles, vpsc};
use arqlet_model::math::PortableMath;
use arqlet_model::outline::{within, Outline};
use arqlet_model::{
    container_extent, AxisSize, GeometryIssue, Graph, LayoutOptions, Mode, NodeShape, NodeSize,
    Rect,
};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

/// The moves the search may spend: a base amount plus a share per node. Each accepted move lowers the conflict count or, at
/// equal count, the displacement from the stress position.
const MOVES_PER_NODE: usize = 120;
const BASE_MOVES: usize = 100;

/// The smallest top-left corner of a root node: the padding the unpinned placement leaves around the drawing.
const WALL: (f64, f64) = (24., 48.);

/// Unit directions a node may be moved in: the axes and the diagonals.
const DIRECTIONS: [(f64, f64); 16] = [
    (1., 0.),
    (0.923_879_532_511_286_7, 0.382_683_432_365_089_8),
    (0.707_106_781_186_547_5, 0.707_106_781_186_547_5),
    (0.382_683_432_365_089_8, 0.923_879_532_511_286_7),
    (0., 1.),
    (-0.382_683_432_365_089_8, 0.923_879_532_511_286_7),
    (-0.707_106_781_186_547_5, 0.707_106_781_186_547_5),
    (-0.923_879_532_511_286_7, 0.382_683_432_365_089_8),
    (-1., 0.),
    (-0.923_879_532_511_286_7, -0.382_683_432_365_089_8),
    (-0.707_106_781_186_547_5, -0.707_106_781_186_547_5),
    (-0.382_683_432_365_089_8, -0.923_879_532_511_286_7),
    (0., -1.),
    (0.382_683_432_365_089_8, -0.923_879_532_511_286_7),
    (0.707_106_781_186_547_5, -0.707_106_781_186_547_5),
    (0.923_879_532_511_286_7, -0.382_683_432_365_089_8),
];

/// How much a repair may do. A step of a drag repairs locally, the step that ends it moderately, anything else fully.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Effort {
    /// Moves of one node spacing at most, a small budget, no spreading.
    Local,
    /// Moves of up to two node spacings, no spreading.
    Moderate,
    /// Every step size, and the stuck arrangement is spread out and repaired again.
    Full,
}

/// A conflict between nodes, by their indices. The text is formed only for the conflicts that remain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Conflict {
    /// Two root nodes come closer than the clearance.
    Overlap { a: usize, b: usize },
    /// The straight edge from `source` to `target` enters the outline of `blocker`.
    Through {
        source: usize,
        target: usize,
        blocker: usize,
    },
    /// A movable node starts before the wall, or leaves the interior of its container.
    Outside { node: usize },
}

/// The geometry a repair searches over: the conflicts of an arrangement given the centres of all nodes.
pub(crate) trait Geometry {
    fn conflicts(&self, centres: &[(f64, f64)]) -> Vec<Conflict>;

    /// The conflicts and their total depth: how far the outlines reach into each other's clearance, or how close a
    /// segment passes to a blocker. The depth is positive while a conflict stands and shrinks as a node moves away, so
    /// a move that only makes a conflict shallower still counts as progress. A geometry without a measure reports zero.
    fn scan(&self, centres: &[(f64, f64)]) -> (Vec<Conflict>, f64) {
        (self.conflicts(centres), 0.)
    }

    /// The scan of `to`, an arrangement that differs from `from` at node `moved` only, where `from` was the last one
    /// scanned. A geometry may reuse what it found for `from`; the result is the same as `scan(to)`.
    fn scan_moved(
        &self,
        _from: &[(f64, f64)],
        to: &[(f64, f64)],
        _moved: usize,
    ) -> (Vec<Conflict>, f64) {
        self.scan(to)
    }

    /// Commits a single-node trial that the search accepted, when the geometry can reuse its trial state.
    /// Actual port segment when available; otherwise a centre chord is sufficient for proposing a move.
    fn segment(
        &self,
        source: usize,
        target: usize,
        centres: &[(f64, f64)],
    ) -> ((f64, f64), (f64, f64)) {
        (centres[source], centres[target])
    }

    fn commit_moved(&self, _from: &[(f64, f64)], _to: &[(f64, f64)], _moved: usize) {}

    /// Whether the geometry has used the effort it allows a repair. The search stops there and returns what it has: the
    /// conflicts that remain are reported, as for any other search that ends without a solution.
    fn exhausted(&self) -> bool {
        false
    }
}

/// The nodes of a repair: sizes, which of them may move, and the rules of the search.
pub(crate) struct Search<'a> {
    pub ids: &'a [String],
    pub sizes: &'a [(f64, f64)],
    pub movable: &'a [bool],
    /// Nodes that share a straight edge, by index. A repair grows its region along them.
    pub neighbours: &'a [Vec<usize>],
    pub clearance: f64,
    /// The grid the top-left corners snap to, or zero.
    pub grid: f64,
    pub wall: Option<(f64, f64)>,
    /// The interior of a container child, per node (empty when no node has one): the range its top-left corner may take.
    /// A node with a limit is not held by the wall.
    pub limits: &'a [Option<Limits>],
    /// The parent of each node (empty when all nodes share one area): a stuck arrangement is spread within each group.
    pub groups: &'a [Option<usize>],
    /// How much the repair may do (see `Effort`). What a smaller effort does not resolve is reported.
    pub effort: Effort,
    /// Whether a stuck arrangement may be spread out and repaired again (`settle`). A force-directed layout does not: the
    /// drawing is relaxed by the forces, and spreading it would make it jump.
    pub spread: bool,
    pub directed: bool,
}

/// Where the top-left corner of a container child may lie: in the container's interior (padding and header kept).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Limits {
    /// A rectangular container: the corner lies in this range.
    Box { min: (f64, f64), max: (f64, f64) },
    /// A circle container: the child's outline lies in the disc (`radius` already less the padding), below the header.
    Disc {
        centre: (f64, f64),
        radius: f64,
        header: f64,
        /// Whether the child is itself a disc.
        round: bool,
    },
}

/// The result of a repair: the centres after the search and the conflicts it could not resolve.
pub(crate) struct Settled {
    pub centres: Vec<(f64, f64)>,
    pub conflicts: Vec<Conflict>,
}

impl Search<'_> {
    /// Snaps a centre so that the top-left corner lies on the grid, when there is one.
    fn place(&self, i: usize, centre: (f64, f64)) -> (f64, f64) {
        if self.grid <= 0. {
            return centre;
        }
        let (w, h) = self.sizes[i];
        let snap = |v: f64| (v / self.grid).round() * self.grid;
        (
            snap(centre.0 - w / 2.) + w / 2.,
            snap(centre.1 - h / 2.) + h / 2.,
        )
    }

    /// The conflicts of an arrangement: the geometry's, and the nodes that start before the wall.
    fn evaluate(&self, geometry: &dyn Geometry, centres: &[(f64, f64)]) -> Vec<Conflict> {
        self.scan(geometry, centres).0
    }

    /// The conflicts of an arrangement with their total depth (see `Geometry::scan`).
    fn scan(&self, geometry: &dyn Geometry, centres: &[(f64, f64)]) -> (Vec<Conflict>, f64) {
        let (found, depth) = geometry.scan(centres);
        self.outside(centres, found, depth)
    }

    /// The scan of `to`, which differs from `from` at node `moved` only (see `Geometry::scan_moved`).
    fn scan_moved(
        &self,
        geometry: &dyn Geometry,
        from: &[(f64, f64)],
        to: &[(f64, f64)],
        moved: usize,
    ) -> (Vec<Conflict>, f64) {
        let (found, depth) = geometry.scan_moved(from, to, moved);
        self.outside(to, found, depth)
    }

    /// Adds the nodes that start before the wall or leave their container's interior.
    fn outside(
        &self,
        centres: &[(f64, f64)],
        mut found: Vec<Conflict>,
        mut depth: f64,
    ) -> (Vec<Conflict>, f64) {
        for i in 0..centres.len() {
            if !self.movable[i] {
                continue;
            }
            let (w, h) = self.sizes[i];
            let (x, y) = (centres[i].0 - w / 2., centres[i].1 - h / 2.);
            let shortfall = match (self.limits.get(i).copied().flatten(), self.wall) {
                (Some(Limits::Box { min, max }), _) => {
                    (min.0 - x).max(0.)
                        + (min.1 - y).max(0.)
                        + (x - max.0).max(0.)
                        + (y - max.1).max(0.)
                }
                (
                    Some(Limits::Disc {
                        centre,
                        radius,
                        header,
                        round,
                    }),
                    _,
                ) => {
                    let rect = Rect {
                        x,
                        y,
                        width: w,
                        height: h,
                    };
                    let outline = if round {
                        Outline::of(NodeShape::Circle, &rect)
                    } else {
                        Outline::Rect(rect)
                    };
                    (header - y).max(0.) + outline.overshoot_beyond_disc(centre.0, centre.1, radius)
                }
                (None, Some((wx, wy))) => (wx - x).max(0.) + (wy - y).max(0.),
                (None, None) => 0.,
            };
            if shortfall > 1e-9 {
                found.push(Conflict::Outside { node: i });
                depth += shortfall;
            }
        }
        (found, depth)
    }

    /// The conflicts as geometry issues, which make a drawing ineligible for publication while they remain.
    pub fn issues(&self, conflicts: &[Conflict]) -> Vec<GeometryIssue> {
        let id = |i: usize| self.ids[i].clone();
        conflicts
            .iter()
            .map(|conflict| match *conflict {
                Conflict::Overlap { a, b } => {
                    let fixed = !self.movable[a] && !self.movable[b];
                    GeometryIssue {
                        kind: if fixed { "fixed-overlap" } else { "overlap" }.into(),
                        node_ids: vec![id(a), id(b)],
                        message: format!("{} and {} come closer than the clearance", id(a), id(b)),
                    }
                }
                Conflict::Through {
                    source,
                    target,
                    blocker,
                } => GeometryIssue {
                    kind: "straight-through-node".into(),
                    node_ids: vec![id(source), id(target), id(blocker)],
                    message: format!(
                        "straight edge between {} and {} passes through node {}",
                        id(source),
                        id(target),
                        id(blocker)
                    ),
                },
                Conflict::Outside { node } => GeometryIssue {
                    kind: "outside-area".into(),
                    node_ids: vec![id(node)],
                    message: format!("node {} lies outside the area", id(node)),
                },
            })
            .collect()
    }
}

/// Repairs the arrangement from `start`: greedy single-node moves over the movable nodes that take part in a conflict. When
/// no move helps, the region grows by one neighbourhood at a time, up to every movable node. A pinned node never moves, so a
/// conflict between pinned nodes alone stays and is reported.
pub(crate) fn settle(search: &Search, geometry: &dyn Geometry, start: Vec<(f64, f64)>) -> Settled {
    settle_with_constraints(search, geometry, start, false, search.clearance)
}

fn settle_with_constraints(
    search: &Search,
    geometry: &dyn Geometry,
    start: Vec<(f64, f64)>,
    use_vpsc: bool,
    gap: f64,
) -> Settled {
    let start = if use_vpsc {
        constraint_start(search, geometry, &start, gap)
    } else {
        start
    };
    let mut best = greedy(search, geometry, start.clone());
    if best.conflicts.is_empty()
        || !search.spread
        || !search.movable.iter().any(|&m| m)
        || geometry.exhausted()
    {
        return best;
    }
    // Single moves are stuck: the nodes are larger than the arrangement was made for (a measurement, a resize), or the
    // conflicts need several nodes to give way. The arrangement is spread out in steps, each repaired again; the first
    // without conflicts wins, otherwise the one with the fewest and shallowest.
    let rank = |s: &Settled| {
        let (_, depth) = search.scan(geometry, &s.centres);
        (s.conflicts.len(), depth)
    };
    let mut best_rank = rank(&best);
    for factor in SPREAD_FACTORS {
        if geometry.exhausted() {
            break;
        }
        let candidate = greedy(search, geometry, spread(search, &start, factor));
        let candidate_rank = rank(&candidate);
        // Only fewer conflicts justify the spread: a spread arrangement can have less depth without being better, and a
        // repair that spreads again in every round would blow the drawing up.
        if candidate_rank.0 < best_rank.0 {
            best = candidate;
            best_rank = candidate_rank;
        }
        if best.conflicts.is_empty() {
            break;
        }
    }
    best
}

fn constraint_start(
    search: &Search,
    geometry: &dyn Geometry,
    targets: &[(f64, f64)],
    gap: f64,
) -> Vec<(f64, f64)> {
    let bounds = position_bounds(search);
    let mut current = targets.to_vec();
    let mut constraints = BTreeMap::new();
    for _ in 0..vpsc::MAX_CONSTRAINT_ROUNDS {
        if geometry.exhausted() {
            break;
        }
        let found = search.evaluate(geometry, &current);
        let overlaps: Vec<(usize, usize)> = found
            .iter()
            .filter_map(|conflict| match *conflict {
                Conflict::Overlap { a, b } => Some((a.min(b), a.max(b))),
                _ => None,
            })
            .collect();
        if overlaps.is_empty() {
            return current;
        }

        let previous_count = constraints.len();
        for (a, b) in overlaps {
            constraints.entry((a, b)).or_insert_with(|| {
                vpsc::constraint_for_pair(a, b, targets, &current, search.sizes, gap)
            });
        }
        if constraints.len() == previous_count || constraints.values().any(Option::is_none) {
            break;
        }
        let separations: Vec<_> = constraints.values().filter_map(|c| *c).collect();
        let Some(next) = vpsc::solve(targets, search.movable, &bounds, &separations) else {
            break;
        };
        let placed: Vec<(f64, f64)> = next
            .into_iter()
            .enumerate()
            .map(|(i, centre)| {
                if search.movable[i] {
                    search.place(i, centre)
                } else {
                    centre
                }
            })
            .collect();
        if placed == current {
            break;
        }
        current = placed;
    }
    current
}

fn position_bounds(search: &Search) -> Vec<vpsc::PositionBounds> {
    (0..search.sizes.len())
        .map(|i| {
            let (width, height) = search.sizes[i];
            let half = (width / 2., height / 2.);
            match search.limits.get(i).copied().flatten() {
                Some(Limits::Box { min, max }) => vpsc::PositionBounds {
                    x: vpsc::AxisBounds {
                        min: Some(min.0 + half.0),
                        max: Some(max.0 + half.0),
                    },
                    y: vpsc::AxisBounds {
                        min: Some(min.1 + half.1),
                        max: Some(max.1 + half.1),
                    },
                },
                Some(Limits::Disc {
                    centre,
                    radius,
                    header,
                    ..
                }) => vpsc::PositionBounds {
                    x: vpsc::AxisBounds {
                        min: Some(centre.0 - radius + half.0),
                        max: Some(centre.0 + radius - half.0),
                    },
                    y: vpsc::AxisBounds {
                        min: Some((header + half.1).max(centre.1 - radius + half.1)),
                        max: Some(centre.1 + radius - half.1),
                    },
                },
                None => search
                    .wall
                    .map_or_else(vpsc::PositionBounds::default, |wall| vpsc::PositionBounds {
                        x: vpsc::AxisBounds {
                            min: Some(wall.0 + half.0),
                            max: None,
                        },
                        y: vpsc::AxisBounds {
                            min: Some(wall.1 + half.1),
                            max: None,
                        },
                    }),
            }
        })
        .collect()
}

/// The factors by which a stuck arrangement is spread out, tried in turn.
const SPREAD_FACTORS: [f64; 5] = [1.2, 1.4, 1.7, 2.0, 2.5];

/// The arrangement with its movable nodes spread by `factor` away from the top-left corner of their group (the nodes that
/// share a parent), so that nodes stay on the positive side of the area they are in. Fixed nodes stay where they are.
fn spread(search: &Search, centres: &[(f64, f64)], factor: f64) -> Vec<(f64, f64)> {
    let group = |i: usize| search.groups.get(i).copied().flatten();
    let corner = |i: usize| {
        (
            centres[i].0 - search.sizes[i].0 / 2.,
            centres[i].1 - search.sizes[i].1 / 2.,
        )
    };
    let mut anchors: BTreeMap<Option<usize>, (f64, f64)> = BTreeMap::new();
    for i in 0..centres.len() {
        let (x, y) = corner(i);
        let anchor = anchors.entry(group(i)).or_insert((x, y));
        anchor.0 = anchor.0.min(x);
        anchor.1 = anchor.1.min(y);
    }
    (0..centres.len())
        .map(|i| {
            if !search.movable[i] {
                return centres[i];
            }
            let (x, y) = corner(i);
            let (ax, ay) = anchors[&group(i)];
            let (w, h) = search.sizes[i];
            (
                ax + factor * (x - ax) + w / 2.,
                ay + factor * (y - ay) + h / 2.,
            )
        })
        .collect()
}

/// The greedy search of `settle` from one start.
fn greedy(search: &Search, geometry: &dyn Geometry, start: Vec<(f64, f64)>) -> Settled {
    let n = start.len();
    let original = start.clone();
    let mut centres: Vec<(f64, f64)> = start
        .iter()
        .enumerate()
        .map(|(i, &c)| search.place(i, c))
        .collect();
    let movable: Vec<usize> = (0..n).filter(|&i| search.movable[i]).collect();
    let mean_size = if n == 0 {
        0.
    } else {
        search.sizes.iter().map(|(w, h)| (w + h) / 2.).sum::<f64>() / n as f64
    };
    let base = (mean_size + search.clearance).max(1.);
    let steps = [
        base / 8.,
        base / 4.,
        base / 2.,
        0.75 * base,
        base,
        1.5 * base,
        2. * base,
        3. * base,
        4. * base,
        6. * base,
        8. * base,
        12. * base,
    ];
    // A local repair moves a node by up to one node spacing, no farther: nothing jumps during a drag.
    let steps: &[f64] = match search.effort {
        Effort::Local => &steps[..5],
        Effort::Moderate => &steps[..7],
        Effort::Full => &steps,
    };
    let budget = BASE_MOVES + MOVES_PER_NODE * movable.len();
    let displacement = |c: &[(f64, f64)]| -> f64 {
        c.iter()
            .zip(&original)
            .map(|(a, b)| (a.0 - b.0).powi(2) + (a.1 - b.1).powi(2))
            .sum()
    };
    let mut trial = centres.clone();
    let mut moves = 0;
    let mut expansion = 0;
    loop {
        let (found, depth) = search.scan(geometry, &centres);
        if found.is_empty() || moves >= budget || geometry.exhausted() {
            break;
        }
        // A move is better when it leaves fewer conflicts, at equal count less depth, at equal depth less displacement.
        let current = (found.len(), depth, displacement(&centres));
        let region = region(search, &centres, &found, &movable, expansion, base);
        let mut best: Option<((usize, f64, f64), usize, (f64, f64))> = None;
        // Try a small conflict-derived line search first. Only if it stalls, pay for all directions.
        let proposals = search
            .directed
            .then(|| directed_trials(search, geometry, &centres, &found, base));
        if let Some(proposals) = proposals {
            for (i, position) in proposals {
                if geometry.exhausted() {
                    break;
                }
                trial[i] = search.place(i, position);
                if trial[i] != centres[i] {
                    let (trial_found, trial_depth) =
                        search.scan_moved(geometry, &centres, &trial, i);
                    let key = (trial_found.len(), trial_depth, displacement(&trial));
                    if key < current && best.as_ref().is_none_or(|(b, _, _)| key < *b) {
                        best = Some((key, i, trial[i]));
                    }
                }
                trial[i] = centres[i];
            }
        }
        if best.is_none() {
            for &i in &region {
                if geometry.exhausted() {
                    break;
                }
                for (dx, dy) in DIRECTIONS {
                    for &step in steps {
                        if search.directed && geometry.exhausted() {
                            break;
                        }
                        let moved = (centres[i].0 + step * dx, centres[i].1 + step * dy);
                        trial[i] = search.place(i, moved);
                        let (trial_found, trial_depth) =
                            search.scan_moved(geometry, &centres, &trial, i);
                        let key = (trial_found.len(), trial_depth, displacement(&trial));
                        if key < current && best.as_ref().is_none_or(|(b, _, _)| key < *b) {
                            best = Some((key, i, trial[i]));
                        }
                        trial[i] = centres[i];
                    }
                }
            }
        }
        match best {
            Some((_, i, position)) => {
                trial[i] = position;
                geometry.commit_moved(&centres, &trial, i);
                centres[i] = position;
                moves += 1;
                expansion = 0;
            }
            None if expansion < 2 => expansion += 1,
            None => break,
        }
    }
    let conflicts = search.evaluate(geometry, &centres);
    Settled { centres, conflicts }
}

/// Stable, bounded proposals derived from the active conflicts. All are checked on the real geometry.
fn directed_trials(
    search: &Search,
    geometry: &dyn Geometry,
    centres: &[(f64, f64)],
    found: &[Conflict],
    base: f64,
) -> Vec<(usize, (f64, f64))> {
    let mut result = Vec::new();
    let max_move = base
        * match search.effort {
            Effort::Local => 1.,
            Effort::Moderate => 2.,
            Effort::Full => 12.,
        };
    let mut push = |i: usize, delta: (f64, f64)| {
        if !search.movable[i] {
            return;
        }
        let length = delta.0.p_hypot(delta.1);
        if !length.is_finite() || length <= 1e-9 {
            return;
        }
        for scale in [0.5_f64, 1., 1.5] {
            let scale = scale.min(max_move / length);
            let position = (
                centres[i].0 + delta.0 * scale,
                centres[i].1 + delta.1 * scale,
            );
            if !result.contains(&(i, position)) {
                result.push((i, position));
            }
        }
    };
    for conflict in found {
        match *conflict {
            Conflict::Overlap { a, b } => {
                let dx = centres[a].0 - centres[b].0;
                let dy = centres[a].1 - centres[b].1;
                let ox = ((search.sizes[a].0 + search.sizes[b].0) / 2. + search.clearance
                    - dx.abs())
                .max(0.);
                let oy = ((search.sizes[a].1 + search.sizes[b].1) / 2. + search.clearance
                    - dy.abs())
                .max(0.);
                let delta = if ox <= oy {
                    (if dx < 0. { -ox } else { ox }, 0.)
                } else {
                    (0., if dy < 0. { -oy } else { oy })
                };
                push(a, delta);
                push(b, (-delta.0, -delta.1));
            }
            Conflict::Through {
                source,
                target,
                blocker,
            } => {
                let (a, b) = geometry.segment(source, target, centres);
                let (delta, t) = crate::directed::segment_push(
                    centres[blocker],
                    a,
                    b,
                    search.sizes[blocker],
                    search.clearance,
                );
                push(blocker, delta);
                // A fixed blocker can also be cleared by moving a free endpoint away from it.
                if source != blocker && 1. - t > 1e-6 {
                    push(source, (-delta.0 / (1. - t), -delta.1 / (1. - t)));
                }
                if target != blocker && t > 1e-6 {
                    push(target, (-delta.0 / t, -delta.1 / t));
                }
            }
            Conflict::Outside { .. } => {} // Existing exhaustive search handles walls and curved container limits.
        }
    }
    result
}

/// Whether a pinned child overlaps another child (centres `centres`, sizes `sizes`): the boxes overlap, or for two circles
/// the discs do. Touching or closer-than-clearance children do not count; the geometry checks report only overlaps.
fn pin_overlaps_a_child(
    graph: &Graph,
    options: &LayoutOptions,
    ids: &[String],
    shapes: &[NodeShape],
    centres: &[(f64, f64)],
    sizes: &[(f64, f64)],
) -> bool {
    // A node inside another one (a container and its child) is not an overlap.
    let contains = |outer: usize, inner: usize| {
        let mut parent = graph.nodes[inner].parent_id.as_deref();
        while let Some(p) = parent {
            if p == ids[outer] {
                return true;
            }
            parent = graph
                .nodes
                .iter()
                .find(|n| n.id == p)
                .and_then(|n| n.parent_id.as_deref());
        }
        false
    };
    let overlaps = |a: usize, b: usize| {
        let dx = (centres[a].0 - centres[b].0).abs();
        let dy = (centres[a].1 - centres[b].1).abs();
        if shapes[a] == NodeShape::Circle && shapes[b] == NodeShape::Circle {
            let radius = |i: usize| sizes[i].0.min(sizes[i].1) / 2.;
            dx.p_hypot(dy) < radius(a) + radius(b)
        } else {
            dx < (sizes[a].0 + sizes[b].0) / 2. && dy < (sizes[a].1 + sizes[b].1) / 2.
        }
    };
    (0..ids.len())
        .filter(|&p| options.pins.contains_key(&ids[p]))
        .any(|p| {
            (0..ids.len()).any(|c| c != p && !contains(p, c) && !contains(c, p) && overlaps(p, c))
        })
}

/// The movable nodes the next move may touch: those in a conflict, then (expansion 1) their graph neighbours and nodes within
/// two steps of them, then (expansion 2) every movable node.
fn region(
    search: &Search,
    centres: &[(f64, f64)],
    found: &[Conflict],
    movable: &[usize],
    expansion: usize,
    base: f64,
) -> Vec<usize> {
    let mut focus = BTreeSet::new();
    for conflict in found {
        match *conflict {
            Conflict::Overlap { a, b } => focus.extend([a, b]),
            Conflict::Through {
                source,
                target,
                blocker,
            } => focus.extend([source, target, blocker]),
            Conflict::Outside { node } => {
                focus.insert(node);
            }
        }
    }
    let set: BTreeSet<usize> = match expansion {
        0 => focus.clone(),
        1 => {
            let mut grown = focus.clone();
            for &f in &focus {
                grown.extend(search.neighbours[f].iter().copied());
                for &m in movable {
                    let reach = (search.sizes[f].0
                        + search.sizes[f].1
                        + search.sizes[m].0
                        + search.sizes[m].1)
                        / 4.
                        + 2. * base;
                    let (dx, dy) = (centres[f].0 - centres[m].0, centres[f].1 - centres[m].1);
                    if dx.p_hypot(dy) <= reach {
                        grown.insert(m);
                    }
                }
            }
            grown
        }
        _ => movable.iter().copied().collect(),
    };
    set.into_iter().filter(|&i| search.movable[i]).collect()
}

/// The drawing of the root as the layout draws it: the boxes of every node, with the movable root leaves at the centres of
/// the search. Ports and absolute outlines are read from the same rules as the drawn routes.
struct RootDrawing<'a> {
    graph: &'a Graph,
    ports: &'a ports::PortContext<'a>,
    options: &'a LayoutOptions,
    ids: &'a [String],
    shapes: &'a [NodeShape],
    sizes: &'a [(f64, f64)],
    /// Boxes of every node before the search; only the movable ones are replaced.
    base: &'a BTreeMap<String, Rect>,
    movable: &'a [bool],
    /// The parent of each node, by index: the overlaps are checked among siblings.
    parents: &'a [Option<usize>],
    /// Straight edges as (source, target, edge id).
    edges: &'a [(usize, usize, String)],
    /// The containers of each edge's endpoints: a straight edge crosses them by design.
    containers: &'a [BTreeSet<usize>],
    clearance: f64,
    /// The distance two siblings keep: the required clearance, and at least the gap the geometry validator demands, so that
    /// a drawing the repair accepts is not reported again as too close.
    gap: f64,
    spatial_cell_size: f64,
    /// Whether a node holds children: moving it moves their absolute boxes too, which the incremental scan does not follow.
    has_children: &'a [bool],
    /// The parent of each node by id, for the absolute box of one moved node.
    parent_map: BTreeMap<&'a str, Option<&'a str>>,
    /// The edge index by edge id.
    edge_of: BTreeMap<&'a str, usize>,
    /// By node index: the ends of every edge at that node (its peers and itself), for the ports a move can change.
    peers: Vec<Vec<&'a str>>,
    /// By node index: its children, for the nodes that move with a moved container.
    children_of: Vec<Vec<usize>>,
    /// The scan of the arrangement the search stands on, which a trial that moves one node starts from.
    cache: RefCell<Option<Base>>,
    scratch: RefCell<TrialScratch<'a>>,
    /// The scans made so far by this layout's repairs, against `SCAN_BUDGET`.
    scans: &'a Cell<usize>,
    port_resolution_nanos: &'a Cell<u64>,
    /// The scans one layout's repairs may make: `SCAN_BUDGET`, `LOCAL_SCAN_BUDGET` under `scalableStress`, or
    /// `FORCE_SCAN_BUDGET` when the layout is force-directed.
    budget: usize,
}

/// How much larger than at the start of a repair a container may become on an axis that follows its content.
const MAX_GROWTH: f64 = 1.5;

/// The scans one repair may make. The search is deterministic, so the same input stops at the same place; the limit keeps a
/// drawing whose conflicts cannot be resolved (nodes that cannot move, a fixed container too small) from searching for
/// minutes. A repair that stops here reports the conflicts that remain.
const SCAN_BUDGET: usize = 200_000;

/// The budget when the layout is force-directed: every step of a drag recomputes, so a step may spend little on the repair and
/// report what remains; the next step goes on from there.
const FORCE_SCAN_BUDGET: usize = 15_000;

/// The budget of a local repair (a step of a drag): a few thousand scans keep a step quick.
const LOCAL_SCAN_BUDGET: usize = 1_500;

/// The budget of the repair that ends a drag.
const MODERATE_SCAN_BUDGET: usize = 8_000;

/// What a scan of one arrangement found, item by item, so that a trial that moves one node can reuse the items it does not
/// touch and sum them in the same order as a full scan.
struct Base {
    centres: Vec<(f64, f64)>,
    boxes: BTreeMap<String, Rect>,
    outlines: Vec<Outline>,
    /// Per edge, the drawn port positions at its source and target.
    ends: Vec<[Option<(f64, f64)>; 2]>,
    /// The depth of the clearance conflict for each pair (a, b), a < b, in triangular order.
    pairs: Vec<Option<f64>>,
    /// The depth of the conflict of edge e with node k, at `e * n + k`.
    through: Vec<Option<f64>>,
    /// The pairs (a, b) whose item is a conflict, in the order of `pairs`: a scan reads them instead of every pair.
    pair_hits: BTreeSet<(usize, usize)>,
    /// The (edge, node) whose item is a conflict, in the order of `through`.
    through_hits: BTreeSet<(usize, usize)>,
    spatial: SpatialHash,
}

struct TrialScratch<'a> {
    boxes: BTreeMap<String, Rect>,
    selected_nodes: Vec<&'a str>,
    resolved_ports: Vec<arqlet_model::ResolvedPort>,
    port_scratch: ports::PortResolveScratch,
    moved_outline: Option<Outline>,
    edge_ends: Vec<[Option<(f64, f64)>; 2]>,
    edge_changed: Vec<bool>,
    changed_edges: Vec<usize>,
    pair_depths: Vec<Option<f64>>,
    pair_changed: Vec<bool>,
    pair_dirty: Vec<usize>,
    changed_through: Vec<(usize, Option<f64>)>,
    candidate_nodes: Vec<usize>,
    through_moved: Vec<Option<f64>>,
    /// A moved container (`scan_subtree`): the nodes that move with it (itself first), whether a node is one of them, their
    /// new outlines, and the recomputed (edge, node) items of the unchanged edges at them, by item index in order.
    subtree: Vec<usize>,
    in_subtree: Vec<bool>,
    subtree_outlines: Vec<Outline>,
    subtree_through: Vec<(usize, Option<f64>)>,
}

impl<'a> TrialScratch<'a> {
    fn new(boxes: &BTreeMap<String, Rect>, nodes: usize, edges: usize) -> Self {
        Self {
            boxes: boxes.clone(),
            selected_nodes: Vec::with_capacity(edges.saturating_mul(2).saturating_add(1)),
            resolved_ports: Vec::with_capacity(edges.saturating_mul(2)),
            port_scratch: ports::PortResolveScratch::default(),
            moved_outline: None,
            edge_ends: vec![[None, None]; edges],
            edge_changed: vec![false; edges],
            changed_edges: Vec::with_capacity(edges),
            pair_depths: vec![None; nodes],
            pair_changed: vec![false; nodes],
            pair_dirty: Vec::with_capacity(nodes),
            changed_through: Vec::with_capacity(nodes.saturating_add(edges)),
            candidate_nodes: Vec::with_capacity(nodes),
            through_moved: vec![None; edges],
            subtree: Vec::new(),
            in_subtree: vec![false; nodes],
            subtree_outlines: Vec::new(),
            subtree_through: Vec::new(),
        }
    }

    fn reset(&mut self) {
        for i in 0..self.changed_edges.len() {
            self.edge_changed[self.changed_edges[i]] = false;
        }
        for i in 0..self.pair_dirty.len() {
            self.pair_changed[self.pair_dirty[i]] = false;
        }
        self.changed_edges.clear();
        self.pair_dirty.clear();
        self.selected_nodes.clear();
        self.resolved_ports.clear();
        self.moved_outline = None;
        self.changed_through.clear();
        self.candidate_nodes.clear();
        for i in 0..self.subtree.len() {
            self.in_subtree[self.subtree[i]] = false;
        }
        self.subtree.clear();
        self.subtree_outlines.clear();
        self.subtree_through.clear();
    }
}

/// Two sequences of (key, key, value), each in ascending key order, merged in ascending order; on equal keys the first
/// sequence's item comes first (as a stable sort of the two concatenated would place it).
fn merge_sorted(
    first: impl Iterator<Item = (usize, usize, f64)>,
    second: impl Iterator<Item = (usize, usize, f64)>,
) -> impl Iterator<Item = (usize, usize, f64)> {
    let mut first = first.peekable();
    let mut second = second.peekable();
    std::iter::from_fn(move || match (first.peek(), second.peek()) {
        (Some(a), Some(b)) => {
            if (b.0, b.1) < (a.0, a.1) {
                second.next()
            } else {
                first.next()
            }
        }
        (Some(_), None) => first.next(),
        (None, _) => second.next(),
    })
}

fn pair_slot(n: usize, a: usize, b: usize) -> usize {
    debug_assert!(a < b);
    a * n - a * (a + 1) / 2 + b - a - 1
}

fn outline_at<'a>(
    outlines: &'a [Outline],
    moved_outline: Option<(usize, &'a Outline)>,
    index: usize,
) -> &'a Outline {
    moved_outline
        .filter(|(moved, _)| *moved == index)
        .map_or(&outlines[index], |(_, outline)| outline)
}

/// How far two outlines reach into each other's clearance band `gap`: for two discs the shortfall of the centre distance,
/// otherwise the smaller shortfall over the two axes of their bounding boxes. Positive exactly while `within` holds for
/// two discs or two boxes, and an upper bound of it for a disc against a box.
fn overlap_depth(a: &Outline, b: &Outline, gap: f64) -> f64 {
    if let (
        Outline::Disc { x, y, radius },
        Outline::Disc {
            x: u,
            y: v,
            radius: s,
        },
    ) = (a, b)
    {
        return (radius + s + gap - (x - u).p_hypot(y - v)).max(0.);
    }
    let (p, q) = (a.bounds(), b.bounds());
    let overlap_x = (p.x + p.width).min(q.x + q.width) - p.x.max(q.x);
    let overlap_y = (p.y + p.height).min(q.y + q.height) - p.y.max(q.y);
    (overlap_x + gap).min(overlap_y + gap).max(0.)
}

/// The depth of a straight segment's conflict with an outline: how far the outline's reach (its radius, or half the
/// diagonal of its box) plus the margin exceeds the distance from its centre to the segment. It is positive whenever the
/// segment enters the grown outline, and falls as the outline moves away from the segment.
fn through_depth(outline: &Outline, p: (f64, f64), q: (f64, f64), margin: f64) -> f64 {
    let bounds = outline.bounds();
    let centre = (bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.);
    let reach = match outline {
        Outline::Disc { radius, .. } => *radius,
        Outline::Rect(r) => r.width.p_hypot(r.height) / 2.,
    };
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let length2 = dx * dx + dy * dy;
    let t = if length2 <= 0. {
        0.
    } else {
        (((centre.0 - p.0) * dx + (centre.1 - p.1) * dy) / length2).clamp(0., 1.)
    };
    let distance = (p.0 + t * dx - centre.0).p_hypot(p.1 + t * dy - centre.1);
    (reach + margin - distance).max(0.)
}

fn outline_center(outline: &Outline) -> (f64, f64) {
    let bounds = outline.bounds();
    (bounds.x + bounds.width / 2., bounds.y + bounds.height / 2.)
}

fn segment_bounds(p: (f64, f64), q: (f64, f64), padding: f64) -> ((f64, f64), (f64, f64)) {
    (
        (p.0.min(q.0) - padding, p.1.min(q.1) - padding),
        (p.0.max(q.0) + padding, p.1.max(q.1) + padding),
    )
}

/// Scans a test run answered from the cache (each of them is checked against a full scan).
#[cfg(test)]
static INCREMENTAL_SCANS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
/// The incremental scans of a moved container (`scan_subtree`), in tests.
#[cfg(test)]
static SUBTREE_SCANS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

impl RootDrawing<'_> {
    fn pair_item(&self, a: usize, b: usize, outlines: &[Outline]) -> Option<f64> {
        self.pair_item_with(a, b, outlines, None)
    }

    fn pair_item_with<'a>(
        &self,
        a: usize,
        b: usize,
        outlines: &'a [Outline],
        moved_outline: Option<(usize, &'a Outline)>,
    ) -> Option<f64> {
        let (a_outline, b_outline) = (
            outline_at(outlines, moved_outline, a),
            outline_at(outlines, moved_outline, b),
        );
        (self.parents[a] == self.parents[b] && within(a_outline, b_outline, self.gap))
            .then(|| overlap_depth(a_outline, b_outline, self.gap))
    }

    fn through_item(
        &self,
        e: usize,
        k: usize,
        outlines: &[Outline],
        ends: &[Option<(f64, f64)>; 2],
    ) -> Option<f64> {
        self.through_item_with(e, k, outlines, ends, None)
    }

    fn through_item_with<'a>(
        &self,
        e: usize,
        k: usize,
        outlines: &'a [Outline],
        ends: &[Option<(f64, f64)>; 2],
        moved_outline: Option<(usize, &'a Outline)>,
    ) -> Option<f64> {
        let (Some(p), Some(q)) = (ends[0], ends[1]) else {
            return None;
        };
        if self.containers[e].contains(&k) {
            return None;
        }
        let (source, target, _) = &self.edges[e];
        // An endpoint is checked too: a segment that enters its own body behind the port is a conflict. A segment that
        // leaves the port outward touches the outline only at the port, which does not enter it.
        let margin = if k == *source || k == *target {
            0.
        } else {
            self.clearance
        };
        let outline = outline_at(outlines, moved_outline, k);
        outline
            .segment_enters(p.0, p.1, q.0, q.1, margin)
            .then(|| through_depth(outline, p, q, margin))
    }

    /// The boxes of every node with the movable ones at `centres`.
    fn boxes_at(&self, centres: &[(f64, f64)]) -> BTreeMap<String, Rect> {
        let mut boxes = self.base.clone();
        for (i, &c) in centres.iter().enumerate() {
            if self.movable[i] {
                boxes.insert(self.ids[i].clone(), self.box_at(i, c));
            }
        }
        boxes
    }

    fn box_at(&self, i: usize, c: (f64, f64)) -> Rect {
        let (w, h) = self.sizes[i];
        Rect {
            x: c.0 - w / 2.,
            y: c.1 - h / 2.,
            width: w,
            height: h,
        }
    }

    /// A full scan of an arrangement, item by item.
    fn compute(&self, centres: &[(f64, f64)]) -> Base {
        let boxes = self.boxes_at(centres);
        let absolute = absolute_all(self.graph, &boxes);
        let outlines: Vec<Outline> = self
            .ids
            .iter()
            .enumerate()
            .map(|(i, id)| Outline::of(self.shapes[i], &absolute[id.as_str()]))
            .collect();
        let centers: Vec<(f64, f64)> = outlines.iter().map(outline_center).collect();
        let spatial = SpatialHash::new(&centers, self.spatial_cell_size);
        let port_started = crate::stage_timer_start();
        let ports = self.ports.resolve_with(
            &boxes,
            self.options.port_placement,
            self.options.min_port_spacing,
            &self.options.node_rows,
            self.options.row_port_order,
        );
        self.port_resolution_nanos.set(
            self.port_resolution_nanos
                .get()
                .saturating_add(crate::elapsed_nanos(port_started)),
        );
        let mut ends = vec![[None, None]; self.edges.len()];
        for port in &ports {
            if let Some(&e) = self.edge_of.get(port.edge_id.as_str()) {
                let side = usize::from(port.endpoint != "source");
                ends[e][side] = Some((port.position.x, port.position.y));
            }
        }
        let n = outlines.len();
        let mut pairs = vec![None; n * n.saturating_sub(1) / 2];
        for a in 0..n {
            let mut next = a + 1;
            while let Some(b) = spatial.next_neighbor(centers[a], next, a) {
                next = b + 1;
                if self.parents[a] == self.parents[b] {
                    pairs[pair_slot(n, a, b)] = self.pair_item(a, b, &outlines);
                }
            }
        }
        let mut through = vec![None; self.edges.len() * n];
        let mut candidates = Vec::with_capacity(n);
        for (e, edge_ends) in ends.iter().enumerate() {
            let (Some(p), Some(q)) = (edge_ends[0], edge_ends[1]) else {
                continue;
            };
            let (min, max) = segment_bounds(p, q, self.spatial_cell_size);
            spatial.query_bounds(min, max, &mut candidates);
            for &k in &candidates {
                through[e * n + k] = self.through_item(e, k, &outlines, edge_ends);
            }
        }
        let pair_hits = (0..n)
            .flat_map(|a| (a + 1..n).map(move |b| (a, b)))
            .filter(|&(a, b)| pairs[pair_slot(n, a, b)].is_some())
            .collect();
        let through_hits = (0..self.edges.len())
            .flat_map(|e| (0..n).map(move |k| (e, k)))
            .filter(|&(e, k)| through[e * n + k].is_some())
            .collect();
        Base {
            centres: centres.to_vec(),
            boxes,
            outlines,
            ends,
            pairs,
            through,
            pair_hits,
            through_hits,
            spatial,
        }
    }

    /// The conflicts and their depth from the items, in the order of a full scan (the conflicting items only).
    fn assemble(&self, base: &Base) -> (Vec<Conflict>, f64) {
        let n = self.ids.len();
        let mut found = Vec::new();
        let mut depth = 0.;
        for &(a, b) in &base.pair_hits {
            if let Some(d) = base.pairs[pair_slot(n, a, b)] {
                found.push(Conflict::Overlap { a, b });
                depth += d;
            }
        }
        for &(e, k) in &base.through_hits {
            if let Some(d) = base.through[e * n + k] {
                let (source, target, _) = &self.edges[e];
                found.push(Conflict::Through {
                    source: *source,
                    target: *target,
                    blocker: k,
                });
                depth += d;
            }
        }
        (found, depth)
    }

    /// The conflicts and their depth from a one-node overlay, in the order of a full scan: the cached conflicts that the move
    /// leaves as they were, merged with the items it recomputed. Unchanged items are read from the cached base; only
    /// conflicts are visited, so a trial costs the conflicts and the moved node's items, not every pair.
    fn assemble_trial(
        &self,
        base: &Base,
        moved: usize,
        scratch: &TrialScratch<'_>,
    ) -> (Vec<Conflict>, f64) {
        let n = self.ids.len();
        let mut found = Vec::new();
        let mut depth = 0.;
        // Pairs: those without the moved node keep their cached item (read in order from the cached set); the moved node's
        // pairs are the recomputed ones (a pair with it that was not recomputed has no conflict). The two sets are disjoint
        // and each is in order, so they are merged instead of sorted together.
        let mut moved_pairs: Vec<(usize, usize, f64)> = Vec::new();
        for &other in &scratch.pair_dirty {
            if scratch.pair_changed[other] {
                if let Some(d) = scratch.pair_depths[other] {
                    moved_pairs.push((other.min(moved), other.max(moved), d));
                }
            }
        }
        moved_pairs.sort_by_key(|x| (x.0, x.1));
        moved_pairs.dedup_by(|x, y| (x.0, x.1) == (y.0, y.1));
        let kept_pairs = base
            .pair_hits
            .iter()
            .filter(|&&(a, b)| a != moved && b != moved)
            .filter_map(|&(a, b)| base.pairs[pair_slot(n, a, b)].map(|d| (a, b, d)));
        for (a, b, d) in merge_sorted(kept_pairs, moved_pairs.into_iter()) {
            found.push(Conflict::Overlap { a, b });
            depth += d;
        }
        // Edges: a changed edge has only its recomputed items; an unchanged one keeps its cached items except at the nodes
        // that moved, which it reads from the trial. The recomputed items are sorted (stably, in the order they were made);
        // the cached ones are in order already.
        let mut fresh: Vec<(usize, usize, f64)> = Vec::new();
        for e in 0..self.edges.len() {
            if !scratch.edge_changed[e] {
                if let Some(d) = scratch.through_moved[e] {
                    fresh.push((e, moved, d));
                }
            }
        }
        for &(index, item) in &scratch.subtree_through {
            if let Some(d) = item {
                fresh.push((index / n, index % n, d));
            }
        }
        for &(index, item) in &scratch.changed_through {
            if let Some(d) = item {
                fresh.push((index / n, index % n, d));
            }
        }
        fresh.sort_by_key(|x| (x.0, x.1));
        let kept = base
            .through_hits
            .iter()
            .filter(|&&(e, k)| !scratch.edge_changed[e] && !scratch.in_subtree[k])
            .filter_map(|&(e, k)| base.through[e * n + k].map(|d| (e, k, d)));
        for (e, k, d) in merge_sorted(kept, fresh.into_iter()) {
            let (source, target, _) = &self.edges[e];
            found.push(Conflict::Through {
                source: *source,
                target: *target,
                blocker: k,
            });
            depth += d;
        }
        (found, depth)
    }

    /// `scan_from_cache` for a moved container: it moves with everything inside it. Its pairs with its siblings change (the
    /// pairs inside it move together and keep their items); the ports of every node inside it and of their peers can
    /// change; and the (edge, node) items at the nodes inside it change for every edge. The new outlines are put into the
    /// cached base for the scan and taken out again afterwards.
    fn scan_subtree(
        &self,
        base: &mut Base,
        to: &[(f64, f64)],
        moved: usize,
    ) -> (Vec<Conflict>, f64) {
        #[cfg(test)]
        SUBTREE_SCANS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let n = self.ids.len();
        let mut scratch = self.scratch.borrow_mut();
        scratch.reset();
        // The nodes that move: the container and everything below it, the container first.
        scratch.subtree.push(moved);
        let mut next = 0;
        while next < scratch.subtree.len() {
            let at = scratch.subtree[next];
            next += 1;
            for &child in &self.children_of[at] {
                scratch.subtree.push(child);
            }
        }
        for i in 0..scratch.subtree.len() {
            let k = scratch.subtree[i];
            scratch.in_subtree[k] = true;
        }
        let moved_id = self.ids[moved].as_str();
        let old_box = std::mem::replace(
            scratch
                .boxes
                .get_mut(moved_id)
                .expect("cached boxes contain every graph node"),
            self.box_at(moved, to[moved]),
        );
        // The new outlines, swapped into the base (the old ones kept to put back), and the spatial index updated.
        let mut old: Vec<(Outline, (f64, f64))> = Vec::with_capacity(scratch.subtree.len());
        for i in 0..scratch.subtree.len() {
            let k = scratch.subtree[i];
            let absolute =
                crate::absolute_with(&self.parent_map, &scratch.boxes, self.ids[k].as_str());
            let outline = Outline::of(self.shapes[k], &absolute);
            let center = outline_center(&outline);
            let previous = std::mem::replace(&mut base.outlines[k], outline.clone());
            old.push((previous.clone(), outline_center(&previous)));
            base.spatial.update(k, center);
            scratch.subtree_outlines.push(outline);
        }
        // The ports of every node that moves and of its peers.
        for i in 0..scratch.subtree.len() {
            let k = scratch.subtree[i];
            let id = self.ids[k].as_str();
            scratch.selected_nodes.push(id);
            let peers: Vec<&str> = self.peers[k].clone();
            scratch.selected_nodes.extend(peers);
        }
        scratch.selected_nodes.sort_unstable();
        scratch.selected_nodes.dedup();
        let port_started = crate::stage_timer_start();
        {
            let TrialScratch {
                boxes,
                selected_nodes,
                resolved_ports,
                port_scratch,
                ..
            } = &mut *scratch;
            self.ports.resolve_nodes_into(
                boxes,
                self.options.port_placement,
                self.options.min_port_spacing,
                &self.options.node_rows,
                self.options.row_port_order,
                Some(selected_nodes.as_slice()),
                port_scratch,
                resolved_ports,
            );
        }
        self.port_resolution_nanos.set(
            self.port_resolution_nanos
                .get()
                .saturating_add(crate::elapsed_nanos(port_started)),
        );
        for i in 0..scratch.resolved_ports.len() {
            let update = {
                let port = &scratch.resolved_ports[i];
                self.edge_of.get(port.edge_id.as_str()).map(|&e| {
                    (
                        e,
                        usize::from(port.endpoint != "source"),
                        (port.position.x, port.position.y),
                    )
                })
            };
            if let Some((e, side, position)) = update {
                if !scratch.edge_changed[e] {
                    scratch.edge_changed[e] = true;
                    scratch.changed_edges.push(e);
                    scratch.edge_ends[e] = base.ends[e];
                }
                scratch.edge_ends[e][side] = Some(position);
            }
        }
        // The container's pairs with its siblings (the nodes inside it have their siblings inside it too).
        let new_center = outline_center(&base.outlines[moved]);
        let mut next = 0;
        while let Some(other) = base.spatial.next_neighbor(new_center, next, moved) {
            next = other + 1;
            if self.parents[moved] == self.parents[other] {
                scratch.pair_changed[other] = true;
                scratch.pair_dirty.push(other);
                let (a, b) = (other.min(moved), other.max(moved));
                scratch.pair_depths[other] = self.pair_item_with(a, b, &base.outlines, None);
            }
        }
        for e in 0..self.edges.len() {
            scratch.through_moved[e] = None;
            let ends = if scratch.edge_changed[e] {
                scratch.edge_ends[e]
            } else {
                base.ends[e]
            };
            let (Some(p), Some(q)) = (ends[0], ends[1]) else {
                continue;
            };
            let (min, max) = segment_bounds(p, q, self.spatial_cell_size);
            if scratch.edge_changed[e] {
                base.spatial
                    .query_bounds(min, max, &mut scratch.candidate_nodes);
                for i in 0..scratch.candidate_nodes.len() {
                    let k = scratch.candidate_nodes[i];
                    let item = self.through_item_with(e, k, &base.outlines, &ends, None);
                    scratch.changed_through.push((e * n + k, item));
                }
            } else {
                for i in 0..scratch.subtree.len() {
                    let k = scratch.subtree[i];
                    let item = base
                        .spatial
                        .point_in_bounds(k, min, max)
                        .then(|| self.through_item_with(e, k, &base.outlines, &ends, None))
                        .flatten();
                    scratch.subtree_through.push((e * n + k, item));
                }
            }
        }
        scratch.subtree_through.sort_by_key(|item| item.0);
        scratch.moved_outline = Some(base.outlines[moved].clone());
        let result = self.assemble_trial(base, moved, &scratch);
        for (i, (outline, center)) in old.into_iter().enumerate() {
            let k = scratch.subtree[i];
            base.outlines[k] = outline;
            base.spatial.update(k, center);
        }
        *scratch
            .boxes
            .get_mut(moved_id)
            .expect("cached boxes contain every graph node") = old_box;
        result
    }

    /// The scan of `to`, which differs from the cached arrangement only at `moved`: the items that do not involve the moved
    /// node or the ports it can change are taken from the cache. `None` when the cache is not the arrangement `from` or
    /// the moved node holds children.
    fn scan_from_cache(
        &self,
        from: &[(f64, f64)],
        to: &[(f64, f64)],
        moved: usize,
    ) -> Option<(Vec<Conflict>, f64)> {
        let mut cache = self.cache.borrow_mut();
        let base = cache.as_mut()?;
        if base.centres != from || !self.movable[moved] {
            return None;
        }
        if self.has_children[moved] {
            return Some(self.scan_subtree(base, to, moved));
        }
        let n = self.ids.len();
        let mut scratch = self.scratch.borrow_mut();
        scratch.reset();
        scratch.subtree.push(moved);
        scratch.in_subtree[moved] = true;
        let moved_id = self.ids[moved].as_str();
        let old_box = std::mem::replace(
            scratch
                .boxes
                .get_mut(moved_id)
                .expect("cached boxes contain every graph node"),
            self.box_at(moved, to[moved]),
        );
        let absolute = crate::absolute_with(&self.parent_map, &scratch.boxes, moved_id);
        let moved_outline = Outline::of(self.shapes[moved], &absolute);
        let old_center = outline_center(&base.outlines[moved]);
        let new_center = outline_center(&moved_outline);
        base.spatial.update(moved, new_center);
        scratch.moved_outline = Some(moved_outline);
        // The ports of the moved node and of its peers can change: a node spreads its ports by where their peers are.
        scratch.selected_nodes.push(moved_id);
        scratch
            .selected_nodes
            .extend(self.peers[moved].iter().copied());
        scratch.selected_nodes.sort_unstable();
        scratch.selected_nodes.dedup();
        let port_started = crate::stage_timer_start();
        {
            let TrialScratch {
                boxes,
                selected_nodes,
                resolved_ports,
                port_scratch,
                ..
            } = &mut *scratch;
            self.ports.resolve_nodes_into(
                boxes,
                self.options.port_placement,
                self.options.min_port_spacing,
                &self.options.node_rows,
                self.options.row_port_order,
                Some(selected_nodes.as_slice()),
                port_scratch,
                resolved_ports,
            );
        }
        self.port_resolution_nanos.set(
            self.port_resolution_nanos
                .get()
                .saturating_add(crate::elapsed_nanos(port_started)),
        );
        for i in 0..scratch.resolved_ports.len() {
            let update = {
                let port = &scratch.resolved_ports[i];
                self.edge_of.get(port.edge_id.as_str()).map(|&e| {
                    (
                        e,
                        usize::from(port.endpoint != "source"),
                        (port.position.x, port.position.y),
                    )
                })
            };
            if let Some((e, side, position)) = update {
                if !scratch.edge_changed[e] {
                    scratch.edge_changed[e] = true;
                    scratch.changed_edges.push(e);
                    scratch.edge_ends[e] = base.ends[e];
                }
                scratch.edge_ends[e][side] = Some(position);
            }
        }
        let moved_outline = scratch
            .moved_outline
            .take()
            .expect("the moved outline was prepared");
        let mut next = 0;
        while let Some(other) = base.spatial.next_neighbor(new_center, next, moved) {
            next = other + 1;
            if self.parents[moved] == self.parents[other] {
                scratch.pair_changed[other] = true;
                scratch.pair_dirty.push(other);
                let (a, b) = (other.min(moved), other.max(moved));
                scratch.pair_depths[other] =
                    self.pair_item_with(a, b, &base.outlines, Some((moved, &moved_outline)));
            }
        }
        for e in 0..self.edges.len() {
            let ends = if scratch.edge_changed[e] {
                scratch.edge_ends[e]
            } else {
                base.ends[e]
            };
            let (Some(p), Some(q)) = (ends[0], ends[1]) else {
                scratch.through_moved[e] = None;
                continue;
            };
            let (min, max) = segment_bounds(p, q, self.spatial_cell_size);
            if scratch.edge_changed[e] {
                base.spatial
                    .query_bounds(min, max, &mut scratch.candidate_nodes);
                for i in 0..scratch.candidate_nodes.len() {
                    let k = scratch.candidate_nodes[i];
                    let item = self.through_item_with(
                        e,
                        k,
                        &base.outlines,
                        &ends,
                        Some((moved, &moved_outline)),
                    );
                    scratch.changed_through.push((e * n + k, item));
                }
            } else {
                scratch.through_moved[e] = base
                    .spatial
                    .point_in_bounds(moved, min, max)
                    .then(|| {
                        self.through_item_with(
                            e,
                            moved,
                            &base.outlines,
                            &ends,
                            Some((moved, &moved_outline)),
                        )
                    })
                    .flatten();
            }
        }
        scratch.moved_outline = Some(moved_outline);
        let result = self.assemble_trial(base, moved, &scratch);
        base.spatial.update(moved, old_center);
        *scratch
            .boxes
            .get_mut(moved_id)
            .expect("cached boxes contain every graph node") = old_box;
        Some(result)
    }
}

impl Geometry for RootDrawing<'_> {
    fn segment(
        &self,
        source: usize,
        target: usize,
        centres: &[(f64, f64)],
    ) -> ((f64, f64), (f64, f64)) {
        if let Some(base) = self.cache.borrow().as_ref() {
            if let Some((e, _)) = self
                .edges
                .iter()
                .enumerate()
                .find(|(_, (s, t, _))| *s == source && *t == target)
            {
                if let [Some(a), Some(b)] = base.ends[e] {
                    return (a, b);
                }
            }
        }
        (centres[source], centres[target])
    }

    fn conflicts(&self, centres: &[(f64, f64)]) -> Vec<Conflict> {
        self.scan(centres).0
    }

    fn exhausted(&self) -> bool {
        self.scans.get() >= self.budget
    }

    fn scan(&self, centres: &[(f64, f64)]) -> (Vec<Conflict>, f64) {
        self.scans.set(self.scans.get() + 1);
        if let Some(result) = self
            .cache
            .borrow()
            .as_ref()
            .filter(|base| base.centres == centres)
            .map(|base| self.assemble(base))
        {
            return result;
        }
        let base = self.compute(centres);
        let result = self.assemble(&base);
        self.scratch.borrow_mut().boxes.clone_from(&base.boxes);
        *self.cache.borrow_mut() = Some(base);
        result
    }

    fn scan_moved(
        &self,
        from: &[(f64, f64)],
        to: &[(f64, f64)],
        moved: usize,
    ) -> (Vec<Conflict>, f64) {
        self.scans.set(self.scans.get() + 1);
        match self.scan_from_cache(from, to, moved) {
            Some(result) => {
                #[cfg(test)]
                {
                    INCREMENTAL_SCANS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let full = self.compute(to);
                    let expected = self.assemble(&full);
                    assert_eq!(
                        result.0, expected.0,
                        "the incremental scan finds other conflicts"
                    );
                    assert_eq!(
                        result.1.to_bits(),
                        expected.1.to_bits(),
                        "the incremental scan sums another depth"
                    );
                }
                result
            }
            None => {
                let base = self.compute(to);
                self.assemble(&base)
            }
        }
    }

    fn commit_moved(&self, from: &[(f64, f64)], to: &[(f64, f64)], moved: usize) {
        if self.scan_from_cache(from, to, moved).is_none() {
            return;
        }
        let mut scratch = self.scratch.borrow_mut();
        let mut cache = self.cache.borrow_mut();
        let Some(base) = cache.as_mut() else {
            return;
        };
        if base.centres != from {
            return;
        }
        let moved_id = self.ids[moved].as_str();
        let new_box = self.box_at(moved, to[moved]);
        *base
            .boxes
            .get_mut(moved_id)
            .expect("cached boxes contain every graph node") = new_box.clone();
        *scratch
            .boxes
            .get_mut(moved_id)
            .expect("cached boxes contain every graph node") = new_box;
        base.centres[moved] = to[moved];
        if scratch.subtree_outlines.is_empty() {
            base.outlines[moved] = scratch
                .moved_outline
                .take()
                .expect("the moved outline was prepared");
            base.spatial
                .update(moved, outline_center(&base.outlines[moved]));
        } else {
            // A moved container: every node inside it has its new outline.
            for i in 0..scratch.subtree.len() {
                let k = scratch.subtree[i];
                base.outlines[k] = scratch.subtree_outlines[i].clone();
                base.spatial.update(k, outline_center(&base.outlines[k]));
            }
        }
        let n = self.ids.len();
        for other in 0..n {
            if other != moved {
                let (a, b) = (other.min(moved), other.max(moved));
                let item = if scratch.pair_changed[other] {
                    scratch.pair_depths[other]
                } else {
                    None
                };
                base.pairs[pair_slot(n, a, b)] = item;
                if item.is_some() {
                    base.pair_hits.insert((a, b));
                } else {
                    base.pair_hits.remove(&(a, b));
                }
            }
        }
        let set_through = |base: &mut Base, e: usize, k: usize, item: Option<f64>| {
            base.through[e * n + k] = item;
            if item.is_some() {
                base.through_hits.insert((e, k));
            } else {
                base.through_hits.remove(&(e, k));
            }
        };
        for &edge in &scratch.changed_edges {
            base.ends[edge] = scratch.edge_ends[edge];
            let start = edge * n;
            base.through[start..start + n].fill(None);
            let gone: Vec<(usize, usize)> = base
                .through_hits
                .range((edge, 0)..(edge + 1, 0))
                .copied()
                .collect();
            for key in gone {
                base.through_hits.remove(&key);
            }
        }
        if scratch.subtree_outlines.is_empty() {
            for edge in 0..self.edges.len() {
                if !scratch.edge_changed[edge] {
                    set_through(base, edge, moved, scratch.through_moved[edge]);
                }
            }
        } else {
            for &(index, item) in &scratch.subtree_through {
                set_through(base, index / n, index % n, item);
            }
        }
        for &(index, item) in &scratch.changed_through {
            set_through(base, index / n, index % n, item);
        }
        #[cfg(test)]
        {
            let full = self.compute(to);
            assert_eq!(
                self.assemble(base),
                self.assemble(&full),
                "committing an incremental scan differs from a full scan"
            );
        }
        scratch.reset();
    }
}

/// The last repair of one layout: the widening rounds place the drawing again, and an area that the widening does not change
/// comes back with the same boxes. The repair is deterministic, so the same input boxes give the same boxes and issues.
#[derive(Default)]
pub(crate) struct Memo {
    last: Option<(
        BTreeMap<String, Rect>,
        BTreeMap<String, Rect>,
        Vec<GeometryIssue>,
    )>,
    /// The size of each node the first time a repair of this layout saw it: the growth of a container is measured against
    /// it, however many rounds the repair and the re-arranging take.
    reference: BTreeMap<String, (f64, f64)>,
    /// The scans made by every repair of this layout, against `SCAN_BUDGET`.
    scans: Cell<usize>,
    /// Whether the memoized repair let containers grow.
    grow: bool,
    /// How much the repairs of this layout may do.
    effort: Effort,
}

/// Whether two sets of boxes are the same, value by value.
fn same_boxes(a: &BTreeMap<String, Rect>, b: &BTreeMap<String, Rect>) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|((ka, ra), (kb, rb))| {
            ka == kb && (ra.x, ra.y, ra.width, ra.height) == (rb.x, rb.y, rb.width, rb.height)
        })
}

impl Memo {
    /// A memo for one layout, with the effort its repairs may spend.
    pub(crate) fn new(effort: Effort) -> Self {
        Memo {
            effort,
            ..Memo::default()
        }
    }

    /// `grow` lets a container follow its content (see `repair_areas`); the last round of a re-arranging passes `false`.
    pub(crate) fn run(
        &mut self,
        graph: &Graph,
        options: &LayoutOptions,
        boxes: &mut BTreeMap<String, Rect>,
        grow: bool,
        port_resolution_nanos: &Cell<u64>,
        port_context: &ports::PortContext<'_>,
    ) -> Vec<GeometryIssue> {
        if let Some((input, output, issues)) = &self.last {
            if same_boxes(input, boxes) && self.grow == grow {
                *boxes = output.clone();
                return issues.clone();
            }
        }
        let input = boxes.clone();
        for n in &graph.nodes {
            if let Some(r) = boxes.get(&n.id) {
                self.reference
                    .entry(n.id.clone())
                    .or_insert((r.width, r.height));
            }
        }
        let issues = repair_areas(
            graph,
            options,
            boxes,
            &self.reference,
            &self.scans,
            grow,
            self.effort,
            port_resolution_nanos,
            port_context,
        );
        self.grow = grow;
        self.last = Some((input, boxes.clone(), issues.clone()));
        issues
    }
}

/// Rounds of repair and container fit: a container that grows for its children can reach a sibling, which the next round
/// repairs. What remains after the last round is reported.
const FIT_ROUNDS: usize = 4;

/// Repairs the organic areas of a drawing: the root and the containers whose strategy is organic. The boxes are changed only
/// when the drawing has a conflict, and only unpinned leaves of an organic area move. A container whose size follows its
/// content grows to hold its children (it never shrinks); an authored fixed size is kept, and the children that do not fit
/// are reported. Returns the conflicts that remain, as diagnostics.
pub(crate) fn repair_areas(
    graph: &Graph,
    options: &LayoutOptions,
    boxes: &mut BTreeMap<String, Rect>,
    reference: &BTreeMap<String, (f64, f64)>,
    scans: &Cell<usize>,
    grow: bool,
    effort: Effort,
    port_resolution_nanos: &Cell<u64>,
    port_context: &ports::PortContext<'_>,
) -> Vec<GeometryIssue> {
    for _ in 0..FIT_ROUNDS {
        let issues = repair_once(
            graph,
            options,
            boxes,
            reference,
            scans,
            grow,
            effort,
            port_resolution_nanos,
            port_context,
        );
        if !grow || !fit_containers(graph, options, boxes, reference) {
            return issues;
        }
    }
    // The last fit grew a container after the issues were read: read them again on the final boxes.
    repair_once(
        graph,
        options,
        boxes,
        reference,
        scans,
        grow,
        effort,
        port_resolution_nanos,
        port_context,
    )
}

/// Grows every container whose size follows its content to hold its children with the geometry padding, deepest first.
/// Returns whether any box changed. A fixed axis is kept; the validator reports children that do not fit.
fn fit_containers(
    graph: &Graph,
    options: &LayoutOptions,
    boxes: &mut BTreeMap<String, Rect>,
    reference: &BTreeMap<String, (f64, f64)>,
) -> bool {
    let rules = arqlet_scene::GeometryRules::default();
    let parent_of: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let depth = |id: &str| {
        let mut d = 0;
        let mut next = parent_of.get(id).copied().flatten();
        while let Some(p) = next {
            d += 1;
            next = parent_of.get(p).copied().flatten();
        }
        d
    };
    let mut containers: Vec<&str> = parent_of
        .values()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    containers.sort_by_key(|id| std::cmp::Reverse(depth(id)));
    let mut grew = false;
    for c in containers {
        if graph
            .nodes
            .iter()
            .any(|n| n.id == c && n.shape == NodeShape::Circle)
        {
            // A circle container is a square box around a disc: it grows to the smallest diameter that holds its children
            // where they are (their positions are in its frame), within the growth limit. An authored size is kept.
            let sizing = options.container_sizing.get(c);
            if sizing.is_some_and(|s| s.width.is_some() || s.height.is_some()) {
                continue;
            }
            let Some(current) = boxes.get(c).map(|r| r.width.max(r.height)) else {
                continue;
            };
            let children: Vec<(Rect, NodeShape)> = graph
                .nodes
                .iter()
                .filter(|n| n.parent_id.as_deref() == Some(c))
                .filter_map(|n| boxes.get(&n.id).map(|r| (r.clone(), n.shape)))
                .collect();
            let fits = |d: f64| {
                children.iter().all(|(r, shape)| {
                    r.y >= rules.header - 1e-9
                        && Outline::of(*shape, r).overshoot_beyond_disc(
                            d / 2.,
                            d / 2.,
                            d / 2. - rules.padding,
                        ) <= 1e-9
                })
            };
            if fits(current) {
                continue;
            }
            let first = reference.get(c).map_or(current, |(w, h)| w.max(*h)).max(1.);
            let cap = (first * MAX_GROWTH).max(current);
            let step = if options.placement_grid > 0. {
                options.placement_grid
            } else {
                1.
            };
            let mut d = current + step;
            while d <= cap {
                if fits(d) {
                    if let Some(old) = boxes.get_mut(c) {
                        old.width = d;
                        old.height = d;
                    }
                    grew = true;
                    break;
                }
                d += step;
            }
            continue;
        }
        let (mut right, mut bottom) = (0_f64, 0_f64);
        for n in graph
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == Some(c))
        {
            if let Some(r) = boxes.get(&n.id) {
                right = right.max(r.x + r.width + rules.padding);
                bottom = bottom.max(r.y + r.height + rules.padding);
            }
        }
        let sizing = options.container_sizing.get(c);
        let fixed = |axis: Option<AxisSize>| matches!(axis, Some(AxisSize::Fixed(_)));
        let (extent, _) = container_extent(
            sizing,
            NodeSize {
                width: right,
                height: bottom,
            },
        );
        let Some(old) = boxes.get_mut(c) else {
            continue;
        };
        let width = if fixed(sizing.and_then(|s| s.width)) {
            old.width
        } else {
            old.width
                .max(crate::snap(extent.width, options.placement_grid))
        };
        let height = if fixed(sizing.and_then(|s| s.height)) {
            old.height
        } else {
            old.height
                .max(crate::snap(extent.height, options.placement_grid))
        };
        if width > old.width + 1e-9 || height > old.height + 1e-9 {
            old.width = width;
            old.height = height;
            grew = true;
        }
    }
    grew
}

/// One repair of the organic areas on the current boxes (see `repair_areas`).
fn repair_once(
    graph: &Graph,
    options: &LayoutOptions,
    boxes: &mut BTreeMap<String, Rect>,
    reference: &BTreeMap<String, (f64, f64)>,
    scans: &Cell<usize>,
    grow: bool,
    effort: Effort,
    port_resolution_nanos: &Cell<u64>,
    port_context: &ports::PortContext<'_>,
) -> Vec<GeometryIssue> {
    let organic = |parent: Option<&str>| graph.area_mode(parent) == Ok(Mode::Organic);
    let parents_with_children: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    let index: BTreeMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let Some(sizes) = ids
        .iter()
        .map(|id| boxes.get(id).map(|r| (r.width, r.height)))
        .collect::<Option<Vec<_>>>()
    else {
        return vec![];
    };
    // A pin over a child of a non-organic area (P3): the layered placement does not look at pins, so the child under the
    // pin may move. Without such an overlap, the non-organic areas are left as they were.
    let absolute = absolute_all(graph, boxes);
    let pin_centres: Vec<(f64, f64)> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let r = &absolute[id];
            (r.x + sizes[i].0 / 2., r.y + sizes[i].1 / 2.)
        })
        .collect();
    let shapes_of: Vec<NodeShape> = graph.nodes.iter().map(|n| n.shape).collect();
    let pin_overlap = !options.pins.is_empty()
        && pin_overlaps_a_child(graph, options, &ids, &shapes_of, &pin_centres, &sizes);
    if !pin_overlap && !organic(None) && !parents_with_children.iter().any(|p| organic(Some(p))) {
        return vec![];
    }
    let parents: Vec<Option<usize>> = graph
        .nodes
        .iter()
        .map(|n| n.parent_id.as_deref().and_then(|p| index.get(p).copied()))
        .collect();
    let movable: Vec<bool> = (0..ids.len())
        .map(|i| {
            !options.pins.contains_key(&ids[i])
                && (pin_overlap || organic(graph.nodes[i].parent_id.as_deref()))
        })
        .collect();
    // A container child lies in the interior of its container (padding and header kept); the container's size is kept.
    let rules = arqlet_scene::GeometryRules::default();
    let limits: Vec<Option<Limits>> = (0..ids.len())
        .map(|i| {
            let p = parents[i]?;
            let container = boxes.get(&ids[p])?;
            let (w, h) = sizes[i];
            // An axis that follows its content may grow with the children (`fit_containers`); a fixed one bounds them.
            let sizing = options.container_sizing.get(&ids[p]);
            let first = reference
                .get(&ids[p])
                .copied()
                .unwrap_or((container.width, container.height));
            let cap = |first: f64, now: f64| (first * MAX_GROWTH).max(now);
            // The extent the children may use: a container that follows its content may grow by `MAX_GROWTH` of its first
            // size, no more (a conflict nothing can resolve would push its children out, and the container after them,
            // round after round); a fixed one, or any when growing is off, keeps what it has.
            let room =
                |_axis: Option<AxisSize>, extent: f64, own: f64| extent - rules.padding - own;
            let follows =
                |axis: Option<AxisSize>| grow && !matches!(axis, Some(AxisSize::Fixed(_)));
            if graph.nodes[p].shape == NodeShape::Circle {
                return Some(Limits::Disc {
                    centre: (container.width / 2., container.height / 2.),
                    radius: container.width.min(container.height) / 2. - rules.padding,
                    header: rules.header,
                    round: graph.nodes[i].shape == NodeShape::Circle,
                });
            }
            Some(Limits::Box {
                min: (rules.padding, rules.header),
                max: (
                    room(
                        sizing.and_then(|s| s.width),
                        if follows(sizing.and_then(|s| s.width)) {
                            cap(first.0, container.width)
                        } else {
                            container.width
                        },
                        w,
                    ),
                    room(
                        sizing.and_then(|s| s.height),
                        if follows(sizing.and_then(|s| s.height)) {
                            cap(first.1, container.height)
                        } else {
                            container.height
                        },
                        h,
                    ),
                ),
            })
        })
        .collect();
    let straight = styles::straight_candidates(graph, options);
    let mut edges = Vec::new();
    let mut containers = Vec::new();
    for edge in graph.edges.iter().filter(|e| straight.contains(&e.id)) {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        let (s, t) = (index[edge.source.as_str()], index[target]);
        if s == t {
            continue;
        }
        // The ancestors of both endpoints: a straight edge may cross the containers its endpoints sit in.
        let mut around = BTreeSet::new();
        for start in [parents[s], parents[t]] {
            let mut next = start;
            while let Some(p) = next {
                around.insert(p);
                next = parents[p];
            }
        }
        edges.push((s, t, edge.id.clone()));
        containers.push(around);
    }
    let mut neighbours = vec![Vec::new(); ids.len()];
    for (s, t, _) in &edges {
        neighbours[*s].push(*t);
        neighbours[*t].push(*s);
    }
    let shapes: Vec<NodeShape> = graph.nodes.iter().map(|n| n.shape).collect();
    let has_children: Vec<bool> = (0..ids.len()).map(|i| parents.contains(&Some(i))).collect();
    let gap = clearance(options).max(arqlet_scene::GeometryRules::default().gap);
    let max_extent = sizes
        .iter()
        .map(|(width, height)| (*width).max(*height))
        .fold(0., f64::max);
    let spatial_cell_size = (max_extent + gap).max(1.);
    let index_of: BTreeMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut peers: Vec<Vec<&str>> = vec![Vec::new(); ids.len()];
    for edge in &graph.edges {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        for end in [edge.source.as_str(), target] {
            if let Some(&i) = index_of.get(end) {
                peers[i].push(edge.source.as_str());
                peers[i].push(target);
            }
        }
    }
    let geometry = RootDrawing {
        graph,
        ports: port_context,
        options,
        ids: &ids,
        shapes: &shapes,
        sizes: &sizes,
        base: boxes,
        movable: &movable,
        parents: &parents,
        edges: &edges,
        containers: &containers,
        clearance: clearance(options),
        gap,
        spatial_cell_size,
        has_children: &has_children,
        parent_map: graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
            .collect(),
        edge_of: edges
            .iter()
            .enumerate()
            .map(|(e, (_, _, id))| (id.as_str(), e))
            .collect(),
        peers,
        children_of: {
            let mut children_of = vec![Vec::new(); ids.len()];
            for (i, parent) in parents.iter().enumerate() {
                if let Some(p) = parent {
                    children_of[*p].push(i);
                }
            }
            children_of
        },
        cache: RefCell::new(None),
        scratch: RefCell::new(TrialScratch::new(boxes, ids.len(), edges.len())),
        scans,
        port_resolution_nanos,
        budget: if effort == Effort::Local {
            LOCAL_SCAN_BUDGET
        } else if effort == Effort::Moderate {
            MODERATE_SCAN_BUDGET
        } else if options.scalable_stress {
            // Experimental (scalable organic placement): the force refinement pushes nodes off edges already, and every
            // trial costs in proportion to the conflicts that remain; the moderate budget took twice the time for the same
            // result at 500 to 2000 children (benchmarks/results/organic-scalable-20261109.md).
            LOCAL_SCAN_BUDGET
        } else if options.force_directed {
            FORCE_SCAN_BUDGET
        } else {
            SCAN_BUDGET
        },
    };
    let start: Vec<(f64, f64)> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            let r = &boxes[id];
            (r.x + sizes[i].0 / 2., r.y + sizes[i].1 / 2.)
        })
        .collect();
    let search = Search {
        ids: &ids,
        sizes: &sizes,
        movable: &movable,
        neighbours: &neighbours,
        clearance: clearance(options),
        grid: options.placement_grid,
        wall: Some(WALL),
        limits: &limits,
        groups: &parents,
        effort,
        spread: effort == Effort::Full && !options.force_directed,
        directed: options.directed_repair,
    };
    let found = search.evaluate(&geometry, &start);
    if found.is_empty() {
        return vec![];
    }
    // A step of a force-directed drag does not repair: the forces decide where the nodes go, a repair would make them jump,
    // and the drawing is a preview until the drop. The conflicts that stand are reported; the drop repairs them.
    if effort == Effort::Local && options.force_directed {
        return search.issues(&found);
    }
    let settled = if options.vpsc_overlap_removal {
        settle_with_constraints(&search, &geometry, start.clone(), true, gap)
    } else {
        settle(&search, &geometry, start.clone())
    };
    let mut changed = Vec::new();
    for i in 0..ids.len() {
        if movable[i] && settled.centres[i] != start[i] {
            let (w, h) = sizes[i];
            let c = settled.centres[i];
            changed.push((
                ids[i].clone(),
                Rect {
                    x: c.0 - w / 2.,
                    y: c.1 - h / 2.,
                    width: w,
                    height: h,
                },
            ));
        }
    }
    boxes.extend(changed);
    search.issues(&settled.conflicts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::facing_sides;
    use arqlet_model::PortSide;

    /// A small geometry for the search tests: straight edges between centres, with the port rule of the layout (a rectangle's
    /// middle side facing its peer, a circle's outline toward its peer) and the same conflict rules.
    struct Pairs {
        ids: Vec<String>,
        sizes: Vec<(f64, f64)>,
        shapes: Vec<NodeShape>,
        edges: Vec<(usize, usize)>,
        movable: Vec<bool>,
        clearance: f64,
    }

    impl Pairs {
        fn outline(&self, i: usize, c: (f64, f64)) -> Outline {
            let (w, h) = self.sizes[i];
            Outline::of(
                self.shapes[i],
                &Rect {
                    x: c.0 - w / 2.,
                    y: c.1 - h / 2.,
                    width: w,
                    height: h,
                },
            )
        }
    }

    impl Geometry for Pairs {
        fn conflicts(&self, centres: &[(f64, f64)]) -> Vec<Conflict> {
            let outlines: Vec<Outline> = (0..centres.len())
                .map(|i| self.outline(i, centres[i]))
                .collect();
            let mut found = Vec::new();
            for a in 0..outlines.len() {
                for b in a + 1..outlines.len() {
                    if within(&outlines[a], &outlines[b], self.clearance) {
                        found.push(Conflict::Overlap { a, b });
                    }
                }
            }
            for &(s, t) in &self.edges {
                let rect = |i: usize| {
                    let (w, h) = self.sizes[i];
                    Rect {
                        x: centres[i].0 - w / 2.,
                        y: centres[i].1 - h / 2.,
                        width: w,
                        height: h,
                    }
                };
                let (rs, rt) = (rect(s), rect(t));
                let (ss, st) = facing_sides(&rs, &rt, Mode::Organic, false);
                // The drawn rule: a rectangle's port is the middle of the side that faces its peer; a circle's port is
                // its outline toward the peer.
                let end = |i: usize, side: PortSide, peer: (f64, f64), r: &Rect| {
                    if self.shapes[i] == NodeShape::Circle {
                        self.outline(i, centres[i])
                            .attachment_toward(peer.0, peer.1)
                    } else {
                        Some(match side {
                            PortSide::Left => (r.x, r.y + r.height / 2.),
                            PortSide::Right => (r.x + r.width, r.y + r.height / 2.),
                            PortSide::Top => (r.x + r.width / 2., r.y),
                            PortSide::Bottom => (r.x + r.width / 2., r.y + r.height),
                        })
                    }
                };
                let (Some(p), Some(q)) = (end(s, ss, centres[t], &rs), end(t, st, centres[s], &rt))
                else {
                    continue;
                };
                for (k, outline) in outlines.iter().enumerate() {
                    let margin = if k == s || k == t { 0. } else { self.clearance };
                    if outline.segment_enters(p.0, p.1, q.0, q.1, margin) {
                        found.push(Conflict::Through {
                            source: s,
                            target: t,
                            blocker: k,
                        });
                    }
                }
            }
            found
        }
    }

    fn names(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("n{i}")).collect()
    }

    fn pairs(
        sizes: &[(f64, f64)],
        shapes: &[NodeShape],
        edges: &[(usize, usize)],
        movable: &[bool],
    ) -> Pairs {
        Pairs {
            ids: names(sizes.len()),
            sizes: sizes.to_vec(),
            shapes: shapes.to_vec(),
            edges: edges.to_vec(),
            movable: movable.to_vec(),
            clearance: 12.,
        }
    }

    /// Runs the search over a test geometry with the given start.
    fn run(geometry: &Pairs, start: Vec<(f64, f64)>, wall: Option<(f64, f64)>) -> Settled {
        let neighbours: Vec<Vec<usize>> = (0..geometry.ids.len())
            .map(|i| {
                geometry
                    .edges
                    .iter()
                    .filter_map(|&(s, t)| (s == i).then_some(t).or((t == i).then_some(s)))
                    .collect()
            })
            .collect();
        let search = Search {
            ids: &geometry.ids,
            sizes: &geometry.sizes,
            movable: &geometry.movable,
            neighbours: &neighbours,
            clearance: geometry.clearance,
            grid: 0.,
            wall,
            limits: &[],
            groups: &[],
            effort: Effort::Full,
            spread: true,
            directed: false,
        };
        settle(&search, geometry, start)
    }

    #[test]
    fn directed_repair_clears_a_blocker_with_fewer_scans_and_preserves_pins() {
        struct Counted<'a> {
            inner: &'a Pairs,
            scans: Cell<usize>,
        }
        impl Geometry for Counted<'_> {
            fn conflicts(&self, centres: &[(f64, f64)]) -> Vec<Conflict> {
                self.scans.set(self.scans.get() + 1);
                self.inner.conflicts(centres)
            }
        }
        let g = pairs(
            &[(40., 40.); 3],
            &[NodeShape::Rect; 3],
            &[(0, 1)],
            &[false, false, true],
        );
        let run = |directed| {
            let geometry = Counted {
                inner: &g,
                scans: Cell::new(0),
            };
            let neighbours = vec![vec![1], vec![0], vec![]];
            let search = Search {
                ids: &g.ids,
                sizes: &g.sizes,
                movable: &g.movable,
                neighbours: &neighbours,
                clearance: g.clearance,
                grid: 0.,
                wall: None,
                limits: &[],
                groups: &[],
                effort: Effort::Full,
                spread: false,
                directed,
            };
            let result = settle(&search, &geometry, vec![(0., 0.), (240., 0.), (120., 0.)]);
            (result, geometry.scans.get())
        };
        let (old, old_scans) = run(false);
        let (new, new_scans) = run(true);
        assert!(old.conflicts.is_empty());
        assert!(new.conflicts.is_empty());
        assert_eq!(&new.centres[..2], &[(0., 0.), (240., 0.)]);
        assert!(
            new_scans < old_scans,
            "directed={new_scans}, exhaustive={old_scans}"
        );
        assert_eq!(run(true).0.centres, new.centres);
    }

    #[test]
    fn directed_search_falls_back_for_walls_and_reports_fixed_conflicts() {
        let g = pairs(&[(40., 40.); 2], &[NodeShape::Rect; 2], &[], &[true, false]);
        let neighbours = vec![vec![1], vec![0]];
        let search = Search {
            ids: &g.ids,
            sizes: &g.sizes,
            movable: &g.movable,
            neighbours: &neighbours,
            clearance: g.clearance,
            grid: 0.,
            wall: Some((0., 0.)),
            limits: &[],
            groups: &[],
            effort: Effort::Full,
            spread: false,
            directed: true,
        };
        // Outside conflicts have no directed proposal: the exhaustive fallback must move the first node.
        let result = settle(&search, &g, vec![(-30., 50.), (200., 50.)]);
        assert!(result.conflicts.is_empty());
        assert_eq!(result.centres[1], (200., 50.));
        let fixed = pairs(
            &[(40., 40.); 2],
            &[NodeShape::Rect; 2],
            &[],
            &[false, false],
        );
        let fixed_search = Search {
            movable: &fixed.movable,
            ..search
        };
        let result = settle(&fixed_search, &fixed, vec![(50., 50.), (50., 50.)]);
        assert!(!result.conflicts.is_empty());
        assert_eq!(result.centres, vec![(50., 50.); 2]);
    }

    #[test]
    fn a_clear_arrangement_is_returned_unchanged() {
        let g = pairs(
            &[(40., 40.), (40., 40.)],
            &[NodeShape::Rect, NodeShape::Circle],
            &[(0, 1)],
            &[true, true],
        );
        let start = vec![(0., 0.), (200., 0.)];
        let settled = run(&g, start.clone(), None);
        assert_eq!(settled.centres, start);
        assert!(settled.conflicts.is_empty());
    }

    #[test]
    fn constraint_prepass_keeps_pins_and_respects_container_bounds() {
        let geometry = pairs(&[(40., 40.); 2], &[NodeShape::Rect; 2], &[], &[false, true]);
        let neighbours = vec![vec![]; 2];
        let limits = [Some(Limits::Box {
            min: (55., 0.),
            max: (80., 100.),
        }); 2];
        let search = Search {
            ids: &geometry.ids,
            sizes: &geometry.sizes,
            movable: &geometry.movable,
            neighbours: &neighbours,
            clearance: geometry.clearance,
            grid: 0.,
            wall: None,
            limits: &limits,
            groups: &[],
            effort: Effort::Full,
            spread: false,
            directed: false,
        };
        let start = vec![(20., 50.), (40., 50.)];
        let settled = settle_with_constraints(&search, &geometry, start, true, 12.);
        assert_eq!(settled.centres[0], (20., 50.));
        assert!(
            (settled.centres[1].0 - 75.).abs() < 1e-7,
            "{:?}",
            settled.centres
        );
        assert!(settled.conflicts.is_empty(), "{:?}", settled.conflicts);
    }

    #[test]
    fn a_movable_blocker_between_two_fixed_ends_moves_aside() {
        // a and b are fixed at the ends of a straight edge; c sits on the segment and is free to move.
        let g = pairs(
            &[(40., 40.), (40., 40.), (40., 40.)],
            &[NodeShape::Rect; 3],
            &[(0, 1)],
            &[false, false, true],
        );
        let settled = run(&g, vec![(0., 0.), (300., 0.), (150., 0.)], None);
        assert!(settled.conflicts.is_empty(), "{:?}", settled.conflicts);
        assert_eq!(settled.centres[0], (0., 0.), "fixed nodes do not move");
        assert_eq!(settled.centres[1], (300., 0.), "fixed nodes do not move");
        assert!(
            settled.centres[2].1.abs() > 20.,
            "the blocker left the segment"
        );
    }

    #[test]
    fn a_fixed_blocker_moves_the_movable_end_to_a_clear_side() {
        // c is fixed on the segment between a and b, which are free: the edge must not run through c.
        let g = pairs(
            &[(40., 40.), (40., 40.), (40., 40.)],
            &[NodeShape::Rect; 3],
            &[(0, 1)],
            &[true, true, false],
        );
        let settled = run(&g, vec![(0., 0.), (300., 0.), (150., 0.)], None);
        assert!(settled.conflicts.is_empty(), "{:?}", settled.conflicts);
        assert_eq!(
            settled.centres[2],
            (150., 0.),
            "the fixed node does not move"
        );
    }

    #[test]
    fn contradictory_fixed_nodes_are_reported_not_hidden() {
        // Two fixed nodes overlap: no move can help, and the conflict stays in the result.
        let g = pairs(
            &[(40., 40.), (40., 40.)],
            &[NodeShape::Rect; 2],
            &[],
            &[false, false],
        );
        let settled = run(&g, vec![(0., 0.), (10., 0.)], None);
        assert_eq!(settled.centres, vec![(0., 0.), (10., 0.)]);
        assert_eq!(settled.conflicts, vec![Conflict::Overlap { a: 0, b: 1 }]);
        let text = Search {
            ids: &g.ids,
            sizes: &g.sizes,
            movable: &g.movable,
            neighbours: &[],
            clearance: 12.,
            grid: 0.,
            wall: None,
            limits: &[],
            groups: &[],
            effort: Effort::Full,
            spread: true,
            directed: false,
        }
        .issues(&settled.conflicts);
        assert_eq!(text[0].kind, "fixed-overlap");
        assert_eq!(text[0].node_ids, ["n0", "n1"]);
    }

    #[test]
    fn a_circle_is_judged_by_its_disc_not_its_box() {
        // The ends are discs, so the segment runs along y = x. c's box corner at (30, 30) lies on it, but its disc (radius 30,
        // centre (60, 0)) stays 12.4 away from the segment: the edge is clear.
        let g = pairs(
            &[(20., 20.), (20., 20.), (60., 60.)],
            &[NodeShape::Circle, NodeShape::Circle, NodeShape::Circle],
            &[(0, 1)],
            &[false, false, false],
        );
        let settled = run(&g, vec![(0., 0.), (200., 200.), (60., 0.)], None);
        assert!(settled.conflicts.is_empty(), "{:?}", settled.conflicts);
    }

    #[test]
    fn the_wall_keeps_movable_nodes_inside_the_area() {
        let g = pairs(&[(40., 40.)], &[NodeShape::Rect], &[], &[true]);
        let settled = run(&g, vec![(0., 0.)], Some((24., 48.)));
        assert!(settled.conflicts.is_empty(), "{:?}", settled.conflicts);
        assert!(settled.centres[0].0 - 20. >= 24. - 1e-9);
        assert!(settled.centres[0].1 - 20. >= 48. - 1e-9);
    }

    #[test]
    fn the_repair_is_deterministic() {
        let g = pairs(
            &[(40., 40.); 4],
            &[
                NodeShape::Rect,
                NodeShape::Circle,
                NodeShape::Rect,
                NodeShape::Circle,
            ],
            &[(0, 2), (1, 3)],
            &[true, false, true, true],
        );
        let start = vec![(0., 0.), (100., 0.), (50., 0.), (50., 60.)];
        assert_eq!(
            run(&g, start.clone(), None).centres,
            run(&g, start, None).centres
        );
    }

    #[test]
    fn a_settled_result_is_a_fixed_point() {
        let g = pairs(
            &[(40., 40.), (40., 40.), (40., 40.)],
            &[NodeShape::Rect; 3],
            &[(0, 1)],
            &[false, false, true],
        );
        let first = run(&g, vec![(0., 0.), (300., 0.), (150., 0.)], None);
        let second = run(&g, first.centres.clone(), None);
        assert_eq!(second.centres, first.centres);
        assert!(second.conflicts.is_empty());
    }
}

#[cfg(test)]
mod root_tests {
    use crate::compute;
    use arqlet_model::math::PortableMath;
    use arqlet_model::{Graph, LayoutOptions, Point};
    use std::collections::BTreeSet;

    /// An organic root with straight edges between the given nodes. Pins are options, not part of the graph.
    fn root(nodes: &[(&str, &str)], edges: &[(&str, &str)]) -> Graph {
        let nodes_json: Vec<_> = nodes
            .iter()
            .map(|(id, shape)| {
                serde_json::json!({
                    "id": id,
                    "kind": "node",
                    "label": id,
                    "parentId": null,
                    "shape": shape,
                })
            })
            .collect();
        let edges_json: Vec<_> = edges
            .iter()
            .enumerate()
            .map(|(i, (s, t))| {
                serde_json::json!({"id": format!("e{i}"), "kind": "flow", "source": s, "target": t})
            })
            .collect();
        serde_json::from_value(serde_json::json!({
            "nodes": nodes_json,
            "edges": edges_json,
            "defaultDirection": "auto",
            "defaultStrategy": "organic",
            "defaultRouting": "straight",
        }))
        .expect("the test graph is valid")
    }

    fn options(pins: &[(&str, f64, f64)]) -> LayoutOptions {
        let mut options = LayoutOptions::default();
        for (id, x, y) in pins {
            options.pins.insert(id.to_string(), Point { x: *x, y: *y });
        }
        options
    }

    fn straight_through(snapshot: &arqlet_model::Snapshot) -> Vec<String> {
        snapshot
            .geometry_issues
            .iter()
            .filter(|i| i.kind == "straight-through-node")
            .map(|i| i.message.clone())
            .collect()
    }

    #[test]
    fn a_free_blocker_is_moved_off_a_straight_edge_between_pinned_ends() {
        let pins = [("a", 0., 0.), ("b", 600., 0.)];
        let graph = root(
            &[("a", "rect"), ("b", "rect"), ("c", "rect")],
            &[("a", "b"), ("a", "c")],
        );
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options(&pins)).unwrap();
        assert!(
            straight_through(&snapshot).is_empty(),
            "{:?}",
            snapshot.geometry_issues
        );
        assert_eq!((snapshot.boxes["a"].x, snapshot.boxes["a"].y), (0., 0.));
        assert_eq!((snapshot.boxes["b"].x, snapshot.boxes["b"].y), (600., 0.));
        // The search answered its trials from the cache, and every such answer was compared with a full scan.
        assert!(super::INCREMENTAL_SCANS.load(std::sync::atomic::Ordering::Relaxed) > 0);
    }

    #[test]
    fn moved_containers_are_scanned_incrementally_and_match_a_full_scan() {
        // Containers (with a nested one) under an organic root, a pinned straight edge across them: the repair moves
        // containers, and every incremental scan of a moved container is compared with a full scan (`scan`, test mode).
        let mut nodes = vec![
            serde_json::json!({"id": "a", "kind": "node", "label": "a", "parentId": null, "shape": "rect"}),
            serde_json::json!({"id": "b", "kind": "node", "label": "b", "parentId": null, "shape": "rect"}),
        ];
        let mut edges =
            vec![serde_json::json!({"id": "ab", "kind": "flow", "source": "a", "target": "b"})];
        for c in 0..4 {
            nodes.push(serde_json::json!({"id": format!("c{c}"), "kind": "group", "label": format!("c{c}"), "parentId": null, "shape": "rect", "layoutStrategy": "organic"}));
            for k in 0..3 {
                nodes.push(serde_json::json!({"id": format!("c{c}n{k}"), "kind": "node", "label": "n", "parentId": format!("c{c}"), "shape": "rect"}));
            }
            edges.push(serde_json::json!({"id": format!("c{c}e"), "kind": "flow", "source": format!("c{c}n0"), "target": format!("c{c}n1")}));
            edges.push(serde_json::json!({"id": format!("x{c}"), "kind": "flow", "source": "a", "target": format!("c{c}n2")}));
        }
        nodes.push(serde_json::json!({"id": "inner", "kind": "group", "label": "inner", "parentId": "c0", "shape": "rect", "layoutStrategy": "organic"}));
        nodes.push(serde_json::json!({"id": "deep", "kind": "node", "label": "deep", "parentId": "inner", "shape": "rect"}));
        edges.push(
            serde_json::json!({"id": "deep-b", "kind": "flow", "source": "deep", "target": "b"}),
        );
        let graph: Graph = serde_json::from_value(serde_json::json!({
            "nodes": nodes,
            "edges": edges,
            "defaultDirection": "auto",
            "defaultStrategy": "organic",
            "defaultRouting": "straight",
        }))
        .expect("the test graph is valid");
        let pins = [("a", 0., 0.), ("b", 500., 0.)];
        let before = super::SUBTREE_SCANS.load(std::sync::atomic::Ordering::Relaxed);
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options(&pins)).unwrap();
        assert!(!snapshot.boxes.is_empty());
        assert!(
            super::SUBTREE_SCANS.load(std::sync::atomic::Ordering::Relaxed) > before,
            "the repair moved no container incrementally"
        );
    }

    #[test]
    fn pinned_nodes_keep_their_pins_through_the_repair() {
        let pins = [("a", 100., 100.), ("b", 400., 100.), ("c", 250., 100.)];
        let graph = root(
            &[
                ("a", "circle"),
                ("b", "rect"),
                ("c", "circle"),
                ("d", "rect"),
            ],
            &[("a", "b"), ("c", "d")],
        );
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options(&pins)).unwrap();
        for (id, x, y) in pins {
            assert_eq!(
                (snapshot.boxes[id].x, snapshot.boxes[id].y),
                (x, y),
                "pin of {id}"
            );
        }
    }

    #[test]
    fn contradictory_pins_are_reported_as_geometry_issues() {
        let pins = [("a", 0., 0.), ("b", 10., 0.)];
        let graph = root(&[("a", "rect"), ("b", "rect")], &[("a", "b")]);
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options(&pins)).unwrap();
        assert!(
            snapshot
                .geometry_issues
                .iter()
                .any(|i| i.kind == "fixed-overlap" && i.node_ids == ["a", "b"]),
            "{:?}",
            snapshot.geometry_issues
        );
    }

    #[test]
    fn the_route_diagnostics_describe_the_drawn_straight_segment() {
        // a and b are pinned at the ends of a straight edge and d is pinned on the segment: the edge cannot be cleared.
        let pins = [("a", 0., 0.), ("b", 600., 0.), ("d", 300., 0.)];
        let graph = root(
            &[("a", "rect"), ("b", "rect"), ("d", "rect")],
            &[("a", "b"), ("d", "a")],
        );
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options(&pins)).unwrap();
        // The diagnostics of the routes and the findings about the segment agree: both name the blocked edge.
        assert!(
            snapshot
                .route_conflicts
                .iter()
                .any(|c| c.edge_id == "e0" && c.node_id == "d"),
            "{:?}",
            snapshot.route_conflicts
        );
        assert!(
            snapshot
                .routing_issues
                .iter()
                .any(|i| i == "straight edge e0 passes through node d"),
            "{:?}",
            snapshot.routing_issues
        );
        // The route diagnostic is about the drawn route: the segment of e0 really enters d.
        let route = snapshot.routes.iter().find(|r| r.id == "e0").unwrap();
        assert_eq!(route.points.len(), 2, "a straight edge is one segment");
    }

    #[test]
    fn a_clear_straight_drawing_reports_no_route_conflict() {
        let pins = [("a", 0., 0.), ("b", 600., 0.)];
        let graph = root(
            &[("a", "rect"), ("b", "rect"), ("c", "rect")],
            &[("a", "b"), ("a", "c")],
        );
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options(&pins)).unwrap();
        assert!(
            snapshot.route_conflicts.is_empty(),
            "{:?}",
            snapshot.route_conflicts
        );
        assert!(
            !snapshot
                .routing_issues
                .iter()
                .any(|i| i.contains("passes through node")),
            "{:?}",
            snapshot.routing_issues
        );
    }

    #[test]
    fn a_warm_start_on_an_unchanged_drawing_does_not_drift() {
        let graph = root(
            &[
                ("a", "circle"),
                ("b", "rect"),
                ("c", "rect"),
                ("d", "circle"),
            ],
            &[("a", "b"), ("b", "c"), ("c", "d"), ("d", "a")],
        );
        let options = LayoutOptions::default();
        let first = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let every: BTreeSet<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
        let mut current = first.clone();
        for round in 0..3 {
            current = compute(&graph, Some(&current), &every, &options).unwrap();
            for id in ["a", "b", "c", "d"] {
                let (p, q) = (&first.boxes[id], &current.boxes[id]);
                assert!(
                    (p.x - q.x).p_hypot(p.y - q.y) < 1e-6,
                    "{id} drifted in round {round}: {:?} -> {:?}",
                    (p.x, p.y),
                    (q.x, q.y)
                );
            }
        }
    }

    #[test]
    fn a_warm_start_from_a_pin_change_keeps_the_pin_and_the_clear_drawing() {
        let graph = root(
            &[
                ("a", "circle"),
                ("b", "rect"),
                ("c", "rect"),
                ("d", "circle"),
            ],
            &[("a", "b"), ("b", "c"), ("c", "d"), ("d", "a")],
        );
        let first = compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        let every: BTreeSet<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
        let pins = [("b", 500., 400.)];
        let warm = compute(&graph, Some(&first), &every, &options(&pins)).unwrap();
        assert_eq!(
            (warm.boxes["b"].x, warm.boxes["b"].y),
            (500., 400.),
            "the pin holds"
        );
        assert!(
            straight_through(&warm).is_empty(),
            "{:?}",
            warm.geometry_issues
        );
    }
}

#[cfg(test)]
mod measure {
    use crate::{absolute_all, compute};
    use arqlet_model::{
        outline::collides, outline::within, outline::Outline, Graph, LayoutOptions,
    };
    use std::collections::BTreeSet;

    /// A deterministic mixed diagram: every third node is a circle, the rest rectangles, and the edges come from a fixed
    /// linear congruential sequence. Organic root, straight edges.
    fn diagram(seed: u64, nodes: usize, edges: usize) -> Graph {
        let mut state = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut next = |bound: usize| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as usize) % bound
        };
        let nodes_json: Vec<_> = (0..nodes)
            .map(|i| {
                serde_json::json!({
                    "id": format!("n{i}"),
                    "kind": "node",
                    "label": format!("N{i}"),
                    "parentId": null,
                    "shape": if i % 3 == 0 { "circle" } else { "rect" },
                })
            })
            .collect();
        let mut edges_json = Vec::new();
        // Straight routing refuses parallel and opposite edges, which are outside this step: one edge per node pair.
        let mut pairs = BTreeSet::new();
        while edges_json.len() < edges {
            let a = next(nodes);
            let b = next(nodes);
            if a != b && pairs.insert((a.min(b), a.max(b))) {
                edges_json.push(serde_json::json!({
                    "id": format!("e{}", edges_json.len()),
                    "kind": "flow",
                    "source": format!("n{a}"),
                    "target": format!("n{b}"),
                }));
            }
        }
        serde_json::from_value(serde_json::json!({
            "nodes": nodes_json,
            "edges": edges_json,
            "defaultDirection": "auto",
            "defaultStrategy": "organic",
            "defaultRouting": "straight",
        }))
        .expect("the measurement graph is valid")
    }

    /// Prints the counts per seed: the engine's straight-edge diagnostic, the repair's unresolved conflicts, and node pairs
    /// whose real outlines overlap or come closer than the default clearance. Run with `--ignored --nocapture`.
    #[test]
    #[ignore = "measurement: prints counts per seed"]
    fn straight_conflicts_on_mixed_diagrams() {
        let (mut through_total, mut overlap_total, mut clearance_total, mut unresolved_total) =
            (0, 0, 0, 0);
        for seed in 1..=20u64 {
            let graph = diagram(seed, 14, 18);
            let snapshot = compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default())
                .expect("the diagram lays out");
            let through = snapshot
                .routing_issues
                .iter()
                .filter(|issue| {
                    issue.starts_with("straight edge") && issue.contains("passes through node")
                })
                .count();
            let unresolved = snapshot
                .geometry_issues
                .iter()
                .filter(|issue| {
                    matches!(
                        issue.kind.as_str(),
                        "overlap" | "fixed-overlap" | "straight-through-node" | "outside-area"
                    )
                })
                .count();
            let absolute = absolute_all(&snapshot.graph, &snapshot.boxes);
            let outlines: Vec<Outline> = snapshot
                .graph
                .nodes
                .iter()
                .map(|n| Outline::of(n.shape, &absolute[n.id.as_str()]))
                .collect();
            let (mut overlap, mut clearance) = (0, 0);
            for i in 0..outlines.len() {
                for j in i + 1..outlines.len() {
                    if collides(&outlines[i], &outlines[j]) {
                        overlap += 1;
                    } else if within(&outlines[i], &outlines[j], 12.) {
                        clearance += 1;
                    }
                }
            }
            println!(
                "seed {seed:2} through {through:2} overlap {overlap} clearance {clearance} unresolved {unresolved}"
            );
            through_total += through;
            overlap_total += overlap;
            clearance_total += clearance;
            unresolved_total += unresolved;
        }
        println!(
            "total through {through_total} overlap {overlap_total} clearance {clearance_total} unresolved {unresolved_total}"
        );
    }
}

impl Default for Effort {
    fn default() -> Self {
        Effort::Full
    }
}
