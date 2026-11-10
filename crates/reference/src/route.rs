//! Orthogonal re-routing of pending routes by a grid search with penalties.
//!
//! Node interiors, crossings with routes that stay in place and overlapping lines are penalised far above any
//! length or bend cost. A route therefore goes through them only when every other route does too, and the validator
//! reports what remains. Each route leaves its source port perpendicular to the source side and arrives at its target
//! port perpendicular to the target side, so the endpoint stubs are part of the search, not a repair afterwards.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use arqlet_model::{Point, PortSide, Rect};

/// A segment through a node interior. Larger than any route in a realistic view.
const NODE_HIT: f64 = 10_000.0;
/// A crossing with a fixed route.
const CROSSING: f64 = 1_000.0;
/// Collinear overlap with a fixed route.
const OVERLAP: f64 = 1_000.0;
/// A change of direction. Small: a bend is cheaper than a long detour, but not free.
const BEND: f64 = 8.0;
/// Distance from a node side at which grid lines are placed, so routes can run beside a node.
const CLEARANCE: f64 = 12.0;
const EPS: f64 = 1e-9;

/// Re-routing passes. The first routes each pending route against those already routed; later passes re-route
/// every pending route against the whole current layout.
pub const PASSES: usize = 6;

/// Directions 0..4 are right, left, down, up. `NONE` is the start state of an unconstrained endpoint.
const DIRS: [(f64, f64); 4] = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)];
const NONE: usize = 4;

/// One route request: its endpoints and sides, the node boxes that are obstacles, and the routes that stay put.
pub struct Request<'a> {
    pub start: Point,
    pub start_side: Option<PortSide>,
    pub end: Point,
    pub end_side: Option<PortSide>,
    pub obstacles: &'a [Rect],
    /// Every node box, passable or not: their clearance lines are grid lines, so a route can arrive from outside
    /// at a port on any side.
    pub grid_rects: &'a [Rect],
    pub fixed: &'a [Vec<Point>],
    /// Extra coordinates that must be grid lines (other ports and route vertices).
    pub tracks: &'a [Point],
}

/// The best route and its penalty. Zero penalty means no node hit, crossing or overlap.
pub struct Routed {
    pub points: Vec<Point>,
    pub penalty: f64,
}

/// The cheapest orthogonal route for `request`. Always returns a route: penalties, not refusals, decide.
pub fn route(request: &Request) -> Option<Routed> {
    let (xs, ys) = grid_lines(request);
    let nx = xs.len();
    let ny = ys.len();
    let index =
        |value: f64, lines: &[f64]| lines.iter().position(|line| (line - value).abs() <= EPS);
    let start = (index(request.start.x, &xs)?, index(request.start.y, &ys)?);
    let goal = (index(request.end.x, &xs)?, index(request.end.y, &ys)?);

    let open: Vec<bool> = (0..nx * ny)
        .map(|k| {
            let point = Point {
                x: xs[k / ny],
                y: ys[k % ny],
            };
            !request
                .obstacles
                .iter()
                .any(|rect| strictly_inside(rect, &point))
        })
        .collect();
    let segments: Vec<(Point, Point)> = request
        .fixed
        .iter()
        .flat_map(|route| {
            route
                .windows(2)
                .map(|pair| (pair[0].clone(), pair[1].clone()))
        })
        .collect();
    let fixed_index = SegmentIndex::new(&segments, &xs, &ys);
    // A grid edge has one penalty whatever direction it is entered from: computed once, then reused.
    let mut edge_cost = vec![f64::NAN; nx * ny * 2];
    let state = |i: usize, j: usize, dir: usize| ((i * ny + j) * 5) + dir;
    let mut dist = vec![f64::INFINITY; nx * ny * 5];
    let mut parent = vec![usize::MAX; nx * ny * 5];
    // The penalty of the best path to each state, carried along the same parent chain as its cost.
    let mut penalty_at = vec![0.0; nx * ny * 5];
    let mut heap = BinaryHeap::new();

    let first: Vec<usize> = match request.start_side {
        Some(side) => vec![direction_of(outward(side))],
        None => (0..4).collect(),
    };
    for dir in first {
        let at = state(start.0, start.1, dir);
        dist[at] = 0.0;
        heap.push(Entry { cost: 0.0, at });
    }
    let inward = request.end_side.map(|side| direction_of(inward(side)));

    while let Some(Entry { cost, at }) = heap.pop() {
        if cost > dist[at] {
            continue;
        }
        let dir = at % 5;
        let cell = at / 5;
        let (i, j) = (cell / ny, cell % ny);
        if (i, j) == goal && inward.map_or(true, |wanted| wanted == dir) {
            return Some(Routed {
                points: trace(at, &parent, &xs, &ys, ny),
                penalty: penalty_at[at],
            });
        }
        for next in 0..4 {
            if dir != NONE && next == opposite(dir) {
                continue;
            }
            let (dx, dy) = DIRS[next];
            let ni = i as isize + dx as isize;
            let nj = j as isize + dy as isize;
            if ni < 0 || nj < 0 || ni >= nx as isize || nj >= ny as isize {
                continue;
            }
            let (ni, nj) = (ni as usize, nj as usize);
            if !open[ni * ny + nj] {
                continue;
            }
            let a = Point { x: xs[i], y: ys[j] };
            let b = Point {
                x: xs[ni],
                y: ys[nj],
            };
            let key = if ni != i {
                2 * (i.min(ni) * ny + j)
            } else {
                2 * (i * ny + j.min(nj)) + 1
            };
            if edge_cost[key].is_nan() {
                edge_cost[key] = if ni != i {
                    edge_penalty(&a, &b, true, j, request, &segments, &fixed_index)
                } else {
                    edge_penalty(&a, &b, false, i, request, &segments, &fixed_index)
                };
            }
            let added = edge_cost[key];
            let mut step = (a.x - b.x).abs() + (a.y - b.y).abs() + added;
            if dir != NONE && next != dir {
                step += BEND;
            }
            let to = state(ni, nj, next);
            let total = cost + step;
            if total < dist[to] {
                dist[to] = total;
                parent[to] = at;
                penalty_at[to] = penalty_at[at] + added;
                heap.push(Entry {
                    cost: total,
                    at: to,
                });
            }
        }
    }
    None
}

/// Grid coordinates: node sides and their clearance, the tracks, and midpoints of wide gaps.
fn grid_lines(request: &Request) -> (Vec<f64>, Vec<f64>) {
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for rect in request.grid_rects.iter().chain(request.obstacles) {
        xs.extend([
            rect.x,
            rect.x + rect.width,
            rect.x - CLEARANCE,
            rect.x + rect.width + CLEARANCE,
        ]);
        ys.extend([
            rect.y,
            rect.y + rect.height,
            rect.y - CLEARANCE,
            rect.y + rect.height + CLEARANCE,
        ]);
    }
    for point in request.tracks.iter().chain([&request.start, &request.end]) {
        xs.push(point.x);
        ys.push(point.y);
    }
    for route in request.fixed {
        for point in route {
            xs.push(point.x);
            ys.push(point.y);
        }
    }
    (with_gaps(xs), with_gaps(ys))
}

fn with_gaps(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(|a, b| a.total_cmp(b));
    values.dedup_by(|a, b| (*a - *b).abs() <= EPS);
    let mut result = Vec::with_capacity(values.len() * 2);
    for pair in values.windows(2) {
        result.push(pair[0]);
        if pair[1] - pair[0] > 2.0 * CLEARANCE {
            result.push((pair[0] + pair[1]) / 2.0);
        }
    }
    if let Some(last) = values.last() {
        result.push(*last);
    }
    result
}

/// The fixed segments that can touch a grid line: those whose span covers the row (for horizontal candidates) or the
/// column (for vertical ones). A segment outside that span cannot overlap or cross a candidate on that line, so the
/// index only skips segments whose penalty is zero.
struct SegmentIndex {
    rows: Vec<Vec<usize>>,
    cols: Vec<Vec<usize>>,
}

impl SegmentIndex {
    fn new(segments: &[(Point, Point)], xs: &[f64], ys: &[f64]) -> Self {
        let mut rows = vec![Vec::new(); ys.len()];
        let mut cols = vec![Vec::new(); xs.len()];
        for (s, (a, b)) in segments.iter().enumerate() {
            match (span(a.x, b.x, xs), span(a.y, b.y, ys)) {
                (Some((xl, xh)), Some((yl, yh))) => {
                    for row in rows.iter_mut().take(yh + 1).skip(yl) {
                        row.push(s);
                    }
                    for col in cols.iter_mut().take(xh + 1).skip(xl) {
                        col.push(s);
                    }
                }
                // A point that is not a grid line cannot be placed: the segment is kept for every line.
                _ => {
                    rows.iter_mut().for_each(|row| row.push(s));
                    cols.iter_mut().for_each(|col| col.push(s));
                }
            }
        }
        SegmentIndex { rows, cols }
    }
}

/// The grid indices of the lines that `p` and `q` lie on, as an inclusive range.
fn span(p: f64, q: f64, lines: &[f64]) -> Option<(usize, usize)> {
    let low = lines.partition_point(|line| *line < p.min(q) - EPS);
    let high = lines.partition_point(|line| *line <= p.max(q) + EPS);
    let first = lines
        .get(low)
        .filter(|line| (**line - p.min(q)).abs() <= EPS)
        .map(|_| low);
    let last = high
        .checked_sub(1)
        .filter(|&k| (lines[k] - p.max(q)).abs() <= EPS);
    match (first, last) {
        (Some(first), Some(last)) if first <= last => Some((first, last)),
        _ => None,
    }
}

/// The penalty of the grid edge a–b on its row or column: the same value `segment_penalty` gives, computed from the
/// fixed segments that can touch that line.
fn edge_penalty(
    a: &Point,
    b: &Point,
    horizontal: bool,
    line: usize,
    request: &Request,
    segments: &[(Point, Point)],
    index: &SegmentIndex,
) -> f64 {
    let mid = Point {
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
    };
    let mut penalty = 0.0;
    if request
        .obstacles
        .iter()
        .any(|rect| strictly_inside(rect, &mid))
    {
        penalty += NODE_HIT;
    }
    let listed = if horizontal {
        &index.rows[line]
    } else {
        &index.cols[line]
    };
    for &s in listed {
        let (c, d) = &segments[s];
        penalty += pair_penalty(a, b, c, d);
    }
    penalty
}

/// The penalty of the axis-aligned segment a–b: node interior, crossings and overlaps with fixed routes.
#[cfg(test)]
fn segment_penalty(a: &Point, b: &Point, request: &Request) -> f64 {
    let mid = Point {
        x: (a.x + b.x) / 2.0,
        y: (a.y + b.y) / 2.0,
    };
    let mut penalty = 0.0;
    if request
        .obstacles
        .iter()
        .any(|rect| strictly_inside(rect, &mid))
    {
        penalty += NODE_HIT;
    }
    for route in request.fixed {
        for pair in route.windows(2) {
            penalty += pair_penalty(a, b, &pair[0], &pair[1]);
        }
    }
    penalty
}

fn pair_penalty(a: &Point, b: &Point, c: &Point, d: &Point) -> f64 {
    let a_horizontal = (a.y - b.y).abs() <= EPS;
    let c_horizontal = (c.y - d.y).abs() <= EPS;
    let a_vertical = (a.x - b.x).abs() <= EPS;
    let c_vertical = (c.x - d.x).abs() <= EPS;
    if a_horizontal && c_horizontal && (a.y - c.y).abs() <= EPS {
        let overlap = a.x.max(b.x).min(c.x.max(d.x)) - a.x.min(b.x).max(c.x.min(d.x));
        return if overlap > EPS { OVERLAP } else { 0.0 };
    }
    if a_vertical && c_vertical && (a.x - c.x).abs() <= EPS {
        let overlap = a.y.max(b.y).min(c.y.max(d.y)) - a.y.min(b.y).max(c.y.min(d.y));
        return if overlap > EPS { OVERLAP } else { 0.0 };
    }
    // Perpendicular. The crossing point must lie strictly inside the fixed segment and within the candidate. A grid
    // cell may end exactly on the fixed line, so the candidate's ends count; the fixed route's corners do not.
    if a_horizontal && c_vertical {
        let (x, y) = (c.x, a.y);
        let on_fixed = y > c.y.min(d.y) + EPS && y < c.y.max(d.y) - EPS;
        let on_candidate = x >= a.x.min(b.x) - EPS && x <= a.x.max(b.x) + EPS;
        return if on_fixed && on_candidate {
            CROSSING
        } else {
            0.0
        };
    }
    if a_vertical && c_horizontal {
        let (x, y) = (a.x, c.y);
        let on_fixed = x > c.x.min(d.x) + EPS && x < c.x.max(d.x) - EPS;
        let on_candidate = y >= a.y.min(b.y) - EPS && y <= a.y.max(b.y) + EPS;
        return if on_fixed && on_candidate {
            CROSSING
        } else {
            0.0
        };
    }
    0.0
}

fn strictly_inside(rect: &Rect, point: &Point) -> bool {
    point.x > rect.x + EPS
        && point.x < rect.x + rect.width - EPS
        && point.y > rect.y + EPS
        && point.y < rect.y + rect.height - EPS
}

/// The direction that leaves a node side.
fn outward(side: PortSide) -> (f64, f64) {
    match side {
        PortSide::Left => (-1.0, 0.0),
        PortSide::Right => (1.0, 0.0),
        PortSide::Top => (0.0, -1.0),
        PortSide::Bottom => (0.0, 1.0),
    }
}

/// The direction that enters a node side from outside.
fn inward(side: PortSide) -> (f64, f64) {
    let (x, y) = outward(side);
    (-x, -y)
}

fn direction_of(vector: (f64, f64)) -> usize {
    DIRS.iter()
        .position(|dir| *dir == vector)
        .expect("unit axis vector")
}

fn opposite(dir: usize) -> usize {
    match dir {
        0 => 1,
        1 => 0,
        2 => 3,
        _ => 2,
    }
}

/// The corner points of the path ending in state `at`, with straight-through points removed.
fn trace(mut at: usize, parent: &[usize], xs: &[f64], ys: &[f64], ny: usize) -> Vec<Point> {
    let mut cells = Vec::new();
    loop {
        let cell = at / 5;
        cells.push(Point {
            x: xs[cell / ny],
            y: ys[cell % ny],
        });
        if parent[at] == usize::MAX {
            break;
        }
        at = parent[at];
    }
    cells.reverse();
    let mut result: Vec<Point> = Vec::new();
    for point in cells {
        if result
            .last()
            .is_some_and(|last| (last.x - point.x).abs() <= EPS && (last.y - point.y).abs() <= EPS)
        {
            continue;
        }
        result.push(point);
    }
    let mut corners: Vec<Point> = Vec::new();
    for index in 0..result.len() {
        let keep = index == 0
            || index + 1 == result.len()
            || !straight(&result[index - 1], &result[index], &result[index + 1]);
        if keep {
            corners.push(result[index].clone());
        }
    }
    corners
}

fn straight(a: &Point, b: &Point, c: &Point) -> bool {
    ((a.x - b.x).abs() <= EPS && (b.x - c.x).abs() <= EPS)
        || ((a.y - b.y).abs() <= EPS && (b.y - c.y).abs() <= EPS)
}

#[derive(PartialEq)]
struct Entry {
    cost: f64,
    at: usize,
}

impl Eq for Entry {}

impl Ord for Entry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .total_cmp(&self.cost)
            .then_with(|| other.at.cmp(&self.at))
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod edge_penalty_tests {
    use super::*;

    /// The indexed edge penalty must equal the reference penalty `segment_penalty` on every grid edge: then the search
    /// sees the same costs, and every route it returns is unchanged.
    #[test]
    fn the_indexed_edge_penalty_equals_the_reference_penalty() {
        let mut seed: u64 = 11;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) % 16) as f64 * 10.0
        };
        for round in 0..40 {
            let obstacles: Vec<Rect> = (0..3 + round % 5)
                .map(|_| Rect {
                    x: next(),
                    y: next(),
                    width: 10.0 + next() / 2.0,
                    height: 10.0 + next() / 2.0,
                })
                .collect();
            let fixed: Vec<Vec<Point>> = (0..2 + round % 4)
                .map(|_| {
                    let (x, y) = (next(), next());
                    vec![
                        Point { x, y },
                        Point {
                            x,
                            y: y + next() + 5.0,
                        },
                        Point {
                            x: x + next() + 5.0,
                            y: y + next() + 5.0,
                        },
                    ]
                })
                .collect();
            let start = Point {
                x: next(),
                y: next(),
            };
            let end = Point {
                x: next() + 3.0,
                y: next() + 7.0,
            };
            let request = Request {
                start: start.clone(),
                start_side: None,
                end: end.clone(),
                end_side: None,
                obstacles: &obstacles,
                grid_rects: &obstacles,
                fixed: &fixed,
                tracks: &[],
            };
            let (xs, ys) = grid_lines(&request);
            let segments: Vec<(Point, Point)> = fixed
                .iter()
                .flat_map(|route| {
                    route
                        .windows(2)
                        .map(|pair| (pair[0].clone(), pair[1].clone()))
                })
                .collect();
            let index = SegmentIndex::new(&segments, &xs, &ys);
            for i in 0..xs.len() {
                for j in 0..ys.len() {
                    if i + 1 < xs.len() {
                        let a = Point { x: xs[i], y: ys[j] };
                        let b = Point {
                            x: xs[i + 1],
                            y: ys[j],
                        };
                        let reference = segment_penalty(&a, &b, &request);
                        let indexed = edge_penalty(&a, &b, true, j, &request, &segments, &index);
                        assert_eq!(
                            indexed, reference,
                            "round {round}, horizontal edge at column {i}, row {j}"
                        );
                    }
                    if j + 1 < ys.len() {
                        let a = Point { x: xs[i], y: ys[j] };
                        let b = Point {
                            x: xs[i],
                            y: ys[j + 1],
                        };
                        let reference = segment_penalty(&a, &b, &request);
                        let indexed = edge_penalty(&a, &b, false, i, &request, &segments, &index);
                        assert_eq!(
                            indexed, reference,
                            "round {round}, vertical edge at column {i}, row {j}"
                        );
                    }
                }
            }
        }
    }
}
