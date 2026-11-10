//! Bounded orthogonal lane search. This is a heuristic, not a complete path finder.
pub mod channel;
pub mod fixed_tracks;

use arqlet_model::outline::Outline;
use arqlet_model::*;
use std::collections::{BTreeMap, BTreeSet};
const EPS: f64 = 1e-6;
fn horizontal(a: &Point, b: &Point) -> bool {
    (a.y - b.y).abs() < EPS
}
fn vertical(a: &Point, b: &Point) -> bool {
    (a.x - b.x).abs() < EPS
}
fn inside(v: f64, a: f64, b: f64) -> bool {
    v > a.min(b) + EPS && v < a.max(b) - EPS
}
/// Closed bounding box as [min x, min y, max x, max y].
type Bounds = [f64; 4];

fn route_bounds(route: &Route) -> Bounds {
    route.points.iter().fold(
        [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ],
        |b, p| [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)],
    )
}

fn rect_bounds(r: &Rect) -> Bounds {
    [r.x, r.y, r.x + r.width, r.y + r.height]
}

/// Conservative prefilter: closed boxes that touch are not apart, so no real interaction is skipped.
fn apart(a: &Bounds, b: &Bounds) -> bool {
    a[2] < b[0] || b[2] < a[0] || a[3] < b[1] || b[3] < a[1]
}

/// A route with its bounding box computed once, for repeated pairwise tests.
#[derive(Clone, Copy)]
struct Probe<'a> {
    route: &'a Route,
    bounds: Bounds,
}

fn probe(route: &Route) -> Probe<'_> {
    Probe {
        route,
        bounds: route_bounds(route),
    }
}

/// The points of a route without the interior points that continue in the same direction. A straight run split at a
/// vertex is one line, so a crossing at that vertex is an interior crossing of the run. Reversals are kept.
fn straight_points(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if out
            .last()
            .is_some_and(|q| (q.x - p.x).abs() < EPS && (q.y - p.y).abs() < EPS)
        {
            continue;
        }
        while out.len() >= 2 && continues(&out[out.len() - 2], &out[out.len() - 1], p) {
            out.pop();
        }
        out.push(p.clone());
    }
    out
}

/// True when `b` lies between `a` and `c` on one line, moving the same way from `a` to `b` as from `b` to `c`.
fn continues(a: &Point, b: &Point, c: &Point) -> bool {
    let vertical = (a.x - b.x).abs() < EPS && (b.x - c.x).abs() < EPS;
    let horizontal = (a.y - b.y).abs() < EPS && (b.y - c.y).abs() < EPS;
    (vertical && (b.y - a.y) * (c.y - b.y) > 0.) || (horizontal && (b.x - a.x) * (c.x - b.x) > 0.)
}

impl std::ops::Deref for Probe<'_> {
    type Target = Route;
    fn deref(&self) -> &Route {
        self.route
    }
}

/// Strict interior crossings, excluding bend touches and shared endpoints.
/// Strict interior crossing of two routes. Straight runs split at a vertex count as one line, so a crossing at such a
/// vertex is found; the routing decisions (`score`, `route`) use `crosses_probe` on the points as drawn.
pub fn crosses(a: &Route, b: &Route) -> bool {
    crosses_points(
        &probe(a),
        &probe(b),
        &straight_points(&a.points),
        &straight_points(&b.points),
    )
}

fn crosses_probe(a: &Probe, b: &Probe) -> bool {
    crosses_points(a, b, &a.route.points, &b.route.points)
}

fn crosses_points(a: &Probe, b: &Probe, pa: &[Point], pb: &[Point]) -> bool {
    if apart(&a.bounds, &b.bounds) {
        return false;
    }
    pa.windows(2).any(|x| {
        pb.windows(2).any(|y| {
            if horizontal(&x[0], &x[1]) && vertical(&y[0], &y[1]) {
                inside(y[0].x, x[0].x, x[1].x) && inside(x[0].y, y[0].y, y[1].y)
            } else if vertical(&x[0], &x[1]) && horizontal(&y[0], &y[1]) {
                inside(x[0].x, y[0].x, y[1].x) && inside(y[0].y, x[0].y, x[1].y)
            } else if axis_aligned(&x[0], &x[1]) && axis_aligned(&y[0], &y[1]) {
                false
            } else {
                // A diagonal segment (a straight edge): the general strict crossing.
                strictly_crosses(&x[0], &x[1], &y[0], &y[1])
            }
        })
    })
}

fn axis_aligned(a: &Point, b: &Point) -> bool {
    horizontal(a, b) || vertical(a, b)
}

/// The side of the line `a`-`b` on which `c` lies: -1, 0 (on the line) or 1.
fn side(a: &Point, b: &Point, c: &Point) -> i8 {
    let d = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if d > EPS {
        1
    } else if d < -EPS {
        -1
    } else {
        0
    }
}

/// Strict crossing of two segments in their interiors; touching and shared endpoints do not count.
fn strictly_crosses(a: &Point, b: &Point, c: &Point, d: &Point) -> bool {
    side(a, b, c) * side(a, b, d) < 0 && side(c, d, a) * side(c, d, b) < 0
}

pub fn shares_segment(a: &Route, b: &Route) -> bool {
    shares_probe(&probe(a), &probe(b))
}

fn shares_probe(a: &Probe, b: &Probe) -> bool {
    if apart(&a.bounds, &b.bounds) {
        return false;
    }
    let overlap =
        |a: f64, b: f64, c: f64, d: f64| a.max(b).min(c.max(d)) - a.min(b).max(c.min(d)) > EPS;
    a.points.windows(2).any(|x| {
        b.points.windows(2).any(|y| {
            if horizontal(&x[0], &x[1]) && horizontal(&y[0], &y[1]) && (x[0].y - y[0].y).abs() < EPS
            {
                overlap(x[0].x, x[1].x, y[0].x, y[1].x)
            } else if vertical(&x[0], &x[1])
                && vertical(&y[0], &y[1])
                && (x[0].x - y[0].x).abs() < EPS
            {
                overlap(x[0].y, x[1].y, y[0].y, y[1].y)
            } else {
                false
            }
        })
    })
}
pub fn hits(route: &Route, r: &Rect) -> bool {
    hits_probe(&probe(route), r)
}

/// Whether a route enters an obstacle outline: a rectangle as `hits`, a disc by any segment entering it (a segment through
/// the empty corner of the disc's box does not).
pub fn hits_outline(route: &Route, outline: &Outline) -> bool {
    hits_outline_probe(&probe(route), outline)
}

fn hits_outline_probe(route: &Probe, outline: &Outline) -> bool {
    hits_outline_points(&route.bounds, &route.points, outline)
}

/// `hits_outline` on points whose bounding box is `bounds`, so that a caller can test a segment without building a route.
fn hits_outline_points(bounds: &Bounds, points: &[Point], outline: &Outline) -> bool {
    match outline {
        Outline::Rect(r) => hits_points(bounds, points, r),
        Outline::Disc { .. } => {
            if apart(bounds, &rect_bounds(&outline.bounds())) {
                return false;
            }
            points
                .windows(2)
                .any(|w| outline.segment_enters(w[0].x, w[0].y, w[1].x, w[1].y, 0.))
        }
    }
}

fn hits_probe(route: &Probe, r: &Rect) -> bool {
    hits_points(&route.bounds, &route.points, r)
}

fn hits_points(bounds: &Bounds, points: &[Point], r: &Rect) -> bool {
    if apart(bounds, &rect_bounds(r)) {
        return false;
    }
    points.windows(2).any(|w| {
        if horizontal(&w[0], &w[1]) {
            inside(w[0].y, r.y, r.y + r.height)
                && w[0].x.max(w[1].x) > r.x + EPS
                && w[0].x.min(w[1].x) < r.x + r.width - EPS
        } else if vertical(&w[0], &w[1]) {
            inside(w[0].x, r.x, r.x + r.width)
                && w[0].y.max(w[1].y) > r.y + EPS
                && w[0].y.min(w[1].y) < r.y + r.height - EPS
        } else {
            // An oblique segment (a straight edge) is judged on the outline itself: it hits when it enters the interior.
            Outline::Rect(r.clone()).segment_enters(w[0].x, w[0].y, w[1].x, w[1].y, 0.)
        }
    })
}
pub fn excluded(graph: &Graph, edge: &Edge) -> BTreeSet<String> {
    excluded_with(&parent_map(graph), edge)
}

/// Provisional: `excluded` for many edges of one graph, with the parent map built once.
pub struct Exclusions<'a>(NodeInfo<'a>);

impl<'a> Exclusions<'a> {
    pub fn new(graph: &'a Graph) -> Self {
        Exclusions(parent_map(graph))
    }

    /// The same set as `excluded(graph, edge)`.
    pub fn of(&self, edge: &Edge) -> BTreeSet<String> {
        excluded_with(&self.0, edge)
    }
}

/// Provisional: routes prepared once for many tests, so that `crosses`, `shares_segment` and `hits` on them do not rebuild
/// each route's bounds and straight runs per call. Each test gives the same answer as the free function.
pub struct PreparedRoutes<'a> {
    probes: Vec<Probe<'a>>,
    straight: Vec<Vec<Point>>,
}

impl<'a> PreparedRoutes<'a> {
    pub fn new(routes: &[&'a Route]) -> Self {
        PreparedRoutes {
            probes: routes.iter().map(|r| probe(r)).collect(),
            straight: routes.iter().map(|r| straight_points(&r.points)).collect(),
        }
    }

    /// `crosses(routes[i], routes[j])`.
    pub fn crosses(&self, i: usize, j: usize) -> bool {
        crosses_points(
            &self.probes[i],
            &self.probes[j],
            &self.straight[i],
            &self.straight[j],
        )
    }

    /// `shares_segment(routes[i], routes[j])`.
    pub fn shares_segment(&self, i: usize, j: usize) -> bool {
        shares_probe(&self.probes[i], &self.probes[j])
    }

    /// `hits(routes[i], r)`.
    pub fn hits(&self, i: usize, r: &Rect) -> bool {
        hits_probe(&self.probes[i], r)
    }

    /// `hits_outline(routes[i], outline)`.
    pub fn hits_outline(&self, i: usize, outline: &Outline) -> bool {
        hits_outline_probe(&self.probes[i], outline)
    }
}

/// The parent and the shape of every node, the two facts the router needs about a node.
type NodeInfo<'a> = BTreeMap<&'a str, (Option<&'a str>, NodeShape)>;

fn parent_map(graph: &Graph) -> NodeInfo<'_> {
    graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), (n.parent_id.as_deref(), n.shape)))
        .collect()
}

fn shape_in(info: &NodeInfo<'_>, id: &str) -> NodeShape {
    info.get(id)
        .map_or_else(NodeShape::default, |(_, shape)| *shape)
}

/// Endpoints and their ancestors. `parents` is precomputed by callers that test many edges.
fn excluded_with(parents: &NodeInfo<'_>, edge: &Edge) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for endpoint in [Some(edge.source.as_str()), edge.target.as_deref()] {
        let mut current = endpoint;
        while let Some(id) = current {
            ids.insert(id.to_string());
            current = parents.get(id).and_then(|(parent, _)| *parent);
        }
    }
    ids
}
pub fn length(route: &Route) -> f64 {
    route
        .points
        .windows(2)
        .map(|w| (w[0].x - w[1].x).abs() + (w[0].y - w[1].y).abs())
        .sum()
}
type Score = (usize, usize, usize, f64);

fn score(route: &Probe, obstacles: &[&Outline], earlier: &[Probe]) -> Score {
    (
        obstacles
            .iter()
            .filter(|o| hits_outline_probe(route, o))
            .count(),
        earlier.iter().filter(|r| shares_probe(route, r)).count(),
        earlier.iter().filter(|r| crosses_probe(route, r)).count(),
        length(route) + 4. * route.points.len() as f64,
    )
}

/// Same ordering as `score`, but gives up as soon as the lexicographic prefix proves the candidate
/// is not strictly better than `limit`. The selected route is therefore identical to a full score.
fn better_score(
    route: &Probe,
    obstacles: &[&Outline],
    earlier: &[Probe],
    limit: &Score,
) -> Option<Score> {
    let mut hit = 0;
    for o in obstacles {
        if hits_outline_probe(route, o) {
            hit += 1;
            if hit > limit.0 {
                return None;
            }
        }
    }
    let mut shared = 0;
    for r in earlier {
        if shares_probe(route, r) {
            shared += 1;
            if hit == limit.0 && shared > limit.1 {
                return None;
            }
        }
    }
    let mut crossed = 0;
    for r in earlier {
        if crosses_probe(route, r) {
            crossed += 1;
            if hit == limit.0 && shared == limit.1 && crossed > limit.2 {
                return None;
            }
        }
    }
    let total = length(route) + 4. * route.points.len() as f64;
    let candidate = (hit, shared, crossed, total);
    (candidate < *limit).then_some(candidate)
}
fn clean(mut route: Route) -> Route {
    route
        .points
        .dedup_by(|a, b| (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS);
    let mut i = 1;
    while i + 1 < route.points.len() {
        let a = &route.points[i - 1];
        let b = &route.points[i];
        let c = &route.points[i + 1];
        if (horizontal(a, b) && horizontal(b, c) && b.x >= a.x.min(c.x) && b.x <= a.x.max(c.x))
            || (vertical(a, b) && vertical(b, c) && b.y >= a.y.min(c.y) && b.y <= a.y.max(c.y))
        {
            route.points.remove(i);
        } else {
            i += 1;
        }
    }
    route
}
/// Routing parameters. `clearance` keeps routes away from other nodes' edges; `track` is the spacing
/// between parallel routes. The defaults reproduce the prototype's behaviour exactly.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub clearance: f64,
    pub track: f64,
    /// Adds corridor centres as candidate tracks. Off by default: it changes the prototype's routes.
    pub corridors: bool,
    /// Spreads clusters of parallel segments to the track spacing after routing. Off by default.
    pub spread: bool,
    /// Distance of the candidate tracks beyond an obstacle boundary. The prototype uses 12; a required clearance uses the
    /// track spacing, because the obstacle is already inflated by the clearance.
    pub rail_offset: f64,
    /// The clearance kept from the edge's own nodes beyond the stub at its port (ADR 0005). Zero leaves the stubs and
    /// rails at the prototype's distances. With a value, the stub leaves the port at least this far and the rails
    /// beside an own node lie beyond it.
    pub own_clearance: f64,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            clearance: 0.,
            track: 6.,
            corridors: false,
            spread: false,
            rail_offset: 12.,
            own_clearance: 0.,
        }
    }
}

/// A node's rectangle as an obstacle. Endpoints of the routed edge keep their true body, so the stub
/// leaving a port is not judged against its own node.
fn obstacle(id: &str, rect: &Rect, shape: NodeShape, edge: &Edge, params: Params) -> Outline {
    if params.clearance <= 0. || id == edge.source || edge.target.as_deref() == Some(id) {
        return Outline::of(shape, rect);
    }
    // The clearance grows the box, and a circle's disc inscribed in it grows by the same amount.
    Outline::of(
        shape,
        &Rect {
            x: rect.x - params.clearance,
            y: rect.y - params.clearance,
            width: rect.width + 2. * params.clearance,
            height: rect.height + 2. * params.clearance,
        },
    )
}

pub fn improve(graph: &Graph, rects: &BTreeMap<String, Rect>, routes: &[Route]) -> Vec<Route> {
    improve_with(graph, rects, routes, Params::default())
}

/// Removes spikes: a route that doubles back onto its own previous segment gains nothing and adds two
/// bends. Only points that lie on the previous segment are removed, so the drawn geometry is unchanged
/// apart from the retraced part.
pub fn simplify(routes: Vec<Route>) -> Vec<Route> {
    routes
        .into_iter()
        .map(|mut route| {
            route.points = simplify_points(&route.points);
            route
        })
        .collect()
}

fn simplify_points(points: &[Point]) -> Vec<Point> {
    const EPS: f64 = 1e-6;
    let same = |a: &Point, b: &Point| (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS;
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_some_and(|q| same(q, p)) {
            continue;
        }
        out.push(p.clone());
        while out.len() >= 3 {
            let n = out.len();
            let (a, b, c) = (&out[n - 3], &out[n - 2], &out[n - 1]);
            let (ab, ac) = ((b.x - a.x, b.y - a.y), (c.x - a.x, c.y - a.y));
            let collinear = (ab.0 * ac.1 - ab.1 * ac.0).abs() < EPS;
            let within = ab.0 * ac.0 + ab.1 * ac.1 >= -EPS
                && ac.0 * ac.0 + ac.1 * ac.1 <= ab.0 * ab.0 + ab.1 * ab.1 + EPS;
            if !(collinear && within) {
                break;
            }
            // c lies on the segment a-b: the route retraces b, so b is removed.
            out.remove(n - 2);
            if out.len() >= 2 && same(&out[out.len() - 2], &out[out.len() - 1]) {
                out.pop();
            }
        }
    }
    out
}

pub fn improve_with(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    params: Params,
) -> Vec<Route> {
    improve_until(graph, rects, routes, params, &|| false)
        .expect("a search without a stop signal cannot be interrupted")
}

/// `improve_with` with checkpoints: `stop` is read before every route and between the passes. An interrupted run
/// returns `Stopped` and no partial route set.
pub fn improve_until(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    params: Params,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Route>, channel::Stopped> {
    improve_until_counted(
        graph,
        rects,
        routes,
        params,
        stop,
        &mut channel::SearchCounters::default(),
    )
}

/// `improve_until` that also adds the work it performs to `counters`. The counters are updated as the work happens, so
/// an interrupted run keeps the work it had already done.
pub fn improve_until_counted(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    params: Params,
    stop: &dyn Fn() -> bool,
    counters: &mut channel::SearchCounters,
) -> Result<Vec<Route>, channel::Stopped> {
    improve_until_counted_with_fixed(graph, rects, routes, &[], params, stop, counters)
}

/// `improve_until_counted` with fixed segments: routes that are not re-routed but occupy space. Each candidate of `routes`
/// pays for its crossings with them, including diagonal segments of straight edges. The fixed routes are not in the result.
pub fn improve_until_counted_with_fixed(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    fixed: &[Route],
    params: Params,
    stop: &dyn Fn() -> bool,
    counters: &mut channel::SearchCounters,
) -> Result<Vec<Route>, channel::Stopped> {
    Ok(simplify(improve_raw(
        graph, rects, routes, fixed, params, stop, counters,
    )?))
}

fn improve_raw(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    fixed: &[Route],
    params: Params,
    stop: &dyn Fn() -> bool,
    counters: &mut channel::SearchCounters,
) -> Result<Vec<Route>, channel::Stopped> {
    let first = improve_pass(graph, rects, routes, fixed, false, params, stop, counters)?;
    let second = improve_pass(graph, rects, &first, fixed, true, params, stop, counters)?;
    if stop() {
        return Err(channel::Stopped);
    }
    let routed = repair_with_counted(graph, rects, &second, params, counters);
    if !params.spread {
        return Ok(routed);
    }
    if stop() {
        return Err(channel::Stopped);
    }
    let spread = spread_tracks(graph, rects, &routed, params);
    if stop() {
        return Err(channel::Stopped);
    }
    // The separation builds its groups once, so the moves of one group can bring back an overlap in another. The
    // overlaps that remain are resolved one by one, and only by moves that lower the overlap count (ADR 0005).
    Ok(resolve_overlaps(graph, rects, &spread, params))
}

/// Rounds of the overlap resolution in one routing run.
const OVERLAP_ROUNDS: usize = 8;

/// The free span of a corridor around a segment: the nearest obstacle boundary on each side (clearance included, the
/// own clearance for the edge's own nodes) over the segment's range. Obstacles that contain the segment's line are
/// ignored: the segment already violates them and cannot be judged by a span.
#[allow(clippy::too_many_arguments)]
fn corridor_span(
    edge: &Edge,
    excluded: &BTreeSet<String>,
    obstacles: &[(String, Rect)],
    info: &NodeInfo<'_>,
    params: Params,
    horizontal_segment: bool,
    line: f64,
    range: (f64, f64),
) -> (f64, f64) {
    let mut floor = f64::NEG_INFINITY;
    let mut ceiling = f64::INFINITY;
    for (id, rect) in obstacles {
        let endpoint = id == &edge.source || edge.target.as_deref() == Some(id.as_str());
        if !(endpoint || !excluded.contains(id.as_str())) {
            continue;
        }
        let outline = obstacle(
            id,
            rect,
            info.get(id.as_str())
                .map_or_else(NodeShape::default, |(_, s)| *s),
            edge,
            params,
        );
        let mut b = outline.bounds();
        if endpoint && params.own_clearance > 0. {
            b = Rect {
                x: b.x - params.own_clearance,
                y: b.y - params.own_clearance,
                width: b.width + 2. * params.own_clearance,
                height: b.height + 2. * params.own_clearance,
            };
        }
        let (along0, along1, across0, across1) = if horizontal_segment {
            (b.x, b.x + b.width, b.y, b.y + b.height)
        } else {
            (b.y, b.y + b.height, b.x, b.x + b.width)
        };
        if along1.min(range.1) - along0.max(range.0) <= EPS {
            continue;
        }
        if across1 <= line + EPS {
            floor = floor.max(across1);
        } else if across0 >= line - EPS {
            ceiling = ceiling.min(across0);
        }
    }
    (floor, ceiling)
}

/// Resolves the exact overlaps that remain after the track separation. An interior segment of an overlapping route moves
/// to another track of its corridor; the move is kept only when the route then overlaps fewer routes, keeps its stub
/// directions and has no more hits on nodes it may not cross. Every kept move lowers the number of overlapping pairs, so
/// the rounds end; what cannot be resolved stays and is reported by the metrics.
fn resolve_overlaps(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    params: Params,
) -> Vec<Route> {
    let edges: BTreeMap<_, _> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let info = parent_map(graph);
    let obstacles: Vec<(String, Rect)> = rects
        .iter()
        .map(|(id, r)| (id.clone(), r.clone()))
        .collect();
    let mut result = routes.to_vec();
    let overlaps = |all: &[Route], index: usize, route: &Route| -> usize {
        all.iter()
            .enumerate()
            .filter(|(j, other)| {
                *j != index && other.bundle_of.is_none() && shares_segment(route, other)
            })
            .count()
    };
    for _ in 0..OVERLAP_ROUNDS {
        let mut changed = false;
        for index in 0..result.len() {
            if result[index].bundle_of.is_some() || result[index].points.len() < 4 {
                continue;
            }
            let current = overlaps(&result, index, &result[index]);
            if current == 0 {
                continue;
            }
            let Some(edge) = edges.get(result[index].id.as_str()) else {
                continue;
            };
            let skip = excluded_with(&info, edge);
            let hits_of = |route: &Route| {
                obstacles
                    .iter()
                    .filter(|(id, rect)| {
                        let endpoint =
                            id == &edge.source || edge.target.as_deref() == Some(id.as_str());
                        (endpoint || !skip.contains(id.as_str()))
                            && hits_outline(
                                route,
                                &obstacle(
                                    id,
                                    rect,
                                    info.get(id.as_str())
                                        .map_or_else(NodeShape::default, |(_, s)| *s),
                                    edge,
                                    params,
                                ),
                            )
                    })
                    .count()
            };
            let hits_before = hits_of(&result[index]);
            let stubs_before = stub_signs(&result[index]);
            let mut best: Option<(usize, f64, Route)> = None;
            for si in 1..result[index].points.len() - 2 {
                let (a, b) = (&result[index].points[si], &result[index].points[si + 1]);
                let horizontal_segment = horizontal(a, b);
                if !horizontal_segment && !vertical(a, b) {
                    continue;
                }
                let line = if horizontal_segment { a.y } else { a.x };
                let range = if horizontal_segment {
                    (a.x.min(b.x), a.x.max(b.x))
                } else {
                    (a.y.min(b.y), a.y.max(b.y))
                };
                let (floor, ceiling) = corridor_span(
                    edge,
                    &skip,
                    &obstacles,
                    &info,
                    params,
                    horizontal_segment,
                    line,
                    range,
                );
                for step in 1..=6 {
                    for sign in [-1., 1.] {
                        let shift = sign * step as f64 * params.track;
                        let target = line + shift;
                        if target < floor - EPS || target > ceiling + EPS {
                            continue;
                        }
                        let mut candidate = result[index].clone();
                        if horizontal_segment {
                            candidate.points[si].y = target;
                            candidate.points[si + 1].y = target;
                        } else {
                            candidate.points[si].x = target;
                            candidate.points[si + 1].x = target;
                        }
                        if stub_signs(&candidate) != stubs_before
                            || hits_of(&candidate) > hits_before
                        {
                            continue;
                        }
                        let count = overlaps(&result, index, &candidate);
                        if count < current
                            && best
                                .as_ref()
                                .is_none_or(|(c, s, _)| (count, shift.abs()) < (*c, *s))
                        {
                            best = Some((count, shift.abs(), candidate));
                        }
                    }
                }
            }
            if let Some((_, _, route)) = best {
                result[index] = route;
                changed = true;
            } else if let Some((first, second, partner)) =
                pair_move(&edges, &info, &obstacles, &result, index, params)
            {
                result[index] = first;
                result[partner] = second;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    result
}

/// The nodes a route hits that it may not cross: its own endpoints and every node that is not excluded for its edge.
fn node_hits(
    route: &Route,
    edge: &Edge,
    skip: &BTreeSet<String>,
    obstacles: &[(String, Rect)],
    info: &NodeInfo<'_>,
    params: Params,
) -> usize {
    obstacles
        .iter()
        .filter(|(id, rect)| {
            let endpoint = id == &edge.source || edge.target.as_deref() == Some(id.as_str());
            (endpoint || !skip.contains(id.as_str()))
                && hits_outline(
                    route,
                    &obstacle(
                        id,
                        rect,
                        info.get(id.as_str())
                            .map_or_else(NodeShape::default, |(_, s)| *s),
                        edge,
                        params,
                    ),
                )
        })
        .count()
}

/// Indices of the first pair of collinear, overlapping segments of two routes.
fn first_shared_segments(a: &Route, b: &Route) -> Option<(usize, usize)> {
    let overlap =
        |a: f64, b: f64, c: f64, d: f64| a.max(b).min(c.max(d)) - a.min(b).max(c.min(d)) > EPS;
    for (i, x) in a.points.windows(2).enumerate() {
        for (j, y) in b.points.windows(2).enumerate() {
            let shared = if horizontal(&x[0], &x[1])
                && horizontal(&y[0], &y[1])
                && (x[0].y - y[0].y).abs() < EPS
            {
                overlap(x[0].x, x[1].x, y[0].x, y[1].x)
            } else if vertical(&x[0], &x[1])
                && vertical(&y[0], &y[1])
                && (x[0].x - y[0].x).abs() < EPS
            {
                overlap(x[0].y, x[1].y, y[0].y, y[1].y)
            } else {
                false
            };
            if shared {
                return Some((i, j));
            }
        }
    }
    None
}

/// Moves of a route that could clear an overlap at `near`: the interior segments at and beside it shift by one or two
/// tracks inside their corridor, keep the stub directions and add no node hit. Shifting the segment after a stub
/// changes the stub's length, which is how two facing stubs get out of each other's way.
fn shifted_candidates(
    route: &Route,
    near: usize,
    edge: &Edge,
    skip: &BTreeSet<String>,
    obstacles: &[(String, Rect)],
    info: &NodeInfo<'_>,
    params: Params,
) -> Vec<(f64, Route)> {
    let mut out = Vec::new();
    if route.points.len() < 4 {
        return out;
    }
    let stubs = stub_signs(route);
    let hits = node_hits(route, edge, skip, obstacles, info, params);
    let first = near.saturating_sub(1).max(1);
    let last = (near + 1).min(route.points.len() - 3);
    for si in first..=last {
        let (a, b) = (&route.points[si], &route.points[si + 1]);
        let horizontal_segment = horizontal(a, b);
        if !horizontal_segment && !vertical(a, b) {
            continue;
        }
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
        for step in [-2., -1., 1., 2.] {
            let shift = step * params.track;
            let target = line + shift;
            if target < floor - EPS || target > ceiling + EPS {
                continue;
            }
            let mut candidate = route.clone();
            if horizontal_segment {
                candidate.points[si].y = target;
                candidate.points[si + 1].y = target;
            } else {
                candidate.points[si].x = target;
                candidate.points[si + 1].x = target;
            }
            if stub_signs(&candidate) == stubs
                && node_hits(&candidate, edge, skip, obstacles, info, params) <= hits
            {
                out.push((shift.abs(), candidate));
            }
        }
    }
    out
}

/// A joint move for two overlapping routes when no move of either alone lowers the overlap count: both shift, and the
/// pair is kept only when the overlaps of the two routes, together, drop. Returns the new routes and the partner's index.
fn pair_move(
    edges: &BTreeMap<&str, &Edge>,
    info: &NodeInfo<'_>,
    obstacles: &[(String, Rect)],
    result: &[Route],
    index: usize,
    params: Params,
) -> Option<(Route, Route, usize)> {
    let count = |route: &Route, skip_a: usize, skip_b: usize| -> usize {
        result
            .iter()
            .enumerate()
            .filter(|(j, other)| {
                *j != skip_a
                    && *j != skip_b
                    && other.bundle_of.is_none()
                    && shares_segment(route, other)
            })
            .count()
    };
    let edge_a = edges.get(result[index].id.as_str())?;
    let skip_a = excluded_with(info, edge_a);
    for partner in 0..result.len() {
        if partner == index
            || result[partner].bundle_of.is_some()
            || !shares_segment(&result[index], &result[partner])
        {
            continue;
        }
        let Some(edge_b) = edges.get(result[partner].id.as_str()) else {
            continue;
        };
        let skip_b = excluded_with(info, edge_b);
        let (near_a, near_b) = first_shared_segments(&result[index], &result[partner])?;
        let mut moves_a = shifted_candidates(
            &result[index],
            near_a,
            edge_a,
            &skip_a,
            obstacles,
            info,
            params,
        );
        moves_a.push((0., result[index].clone()));
        let mut moves_b = shifted_candidates(
            &result[partner],
            near_b,
            edge_b,
            &skip_b,
            obstacles,
            info,
            params,
        );
        moves_b.push((0., result[partner].clone()));
        let before =
            count(&result[index], index, partner) + count(&result[partner], index, partner) + 1;
        let mut best: Option<(usize, f64, Route, Route)> = None;
        for (cost_a, a) in &moves_a {
            for (cost_b, b) in &moves_b {
                if *cost_a == 0. && *cost_b == 0. {
                    continue;
                }
                let between = usize::from(shares_segment(a, b));
                let after = count(a, index, partner) + count(b, index, partner) + between;
                if after < before
                    && best
                        .as_ref()
                        .is_none_or(|(n, c, _, _)| (after, cost_a + cost_b) < (*n, *c))
                {
                    best = Some((after, cost_a + cost_b, a.clone(), b.clone()));
                }
            }
        }
        if let Some((_, _, a, b)) = best {
            return Some((a, b, partner));
        }
    }
    None
}

/// Direction of the first and last segment, as signs per axis.
fn stub_signs(route: &Route) -> [(i8, i8); 2] {
    let sign = |a: f64| {
        if a > EPS {
            1
        } else if a < -EPS {
            -1
        } else {
            0
        }
    };
    let n = route.points.len();
    let first = (&route.points[0], &route.points[1]);
    let last = (&route.points[n - 2], &route.points[n - 1]);
    [
        (sign(first.1.x - first.0.x), sign(first.1.y - first.0.y)),
        (sign(last.1.x - last.0.x), sign(last.1.y - last.0.y)),
    ]
}

/// Track separation: parallel segments of different routes that lie closer than `track` (but are not
/// the same line) are spread around their mean, in route order. A move is kept only if the route still
/// avoids every foreign node. Port stubs (the first and last two segments) never move.
pub fn spread_tracks(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    params: Params,
) -> Vec<Route> {
    let edges: BTreeMap<_, _> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut result = routes.to_vec();
    // (route index, segment index, horizontal?, coordinate, range start, range end)
    let mut segments = Vec::new();
    // For each segment, its two ends: the position along the segment and the side its neighbouring segment turns to
    // (-1 towards lower coordinates across the segment, +1 towards higher, 0 when it does not turn).
    let mut turns: Vec<[(f64, i8); 2]> = Vec::new();
    let side = |from: f64, to: f64| -> i8 {
        if to > from + EPS {
            1
        } else if to < from - EPS {
            -1
        } else {
            0
        }
    };
    for (ri, route) in result.iter().enumerate() {
        if route.bundle_of.is_some() || route.points.len() < 4 {
            continue;
        }
        // Interior segments only: both endpoints of a moved segment must be bend points, not the ports.
        for si in 1..route.points.len() - 2 {
            let (before, a, b, after) = (
                &route.points[si - 1],
                &route.points[si],
                &route.points[si + 1],
                &route.points[si + 2],
            );
            if horizontal(a, b) {
                segments.push((ri, si, true, a.y, a.x.min(b.x), a.x.max(b.x)));
                turns.push([(a.x, side(a.y, before.y)), (b.x, side(b.y, after.y))]);
            } else if vertical(a, b) {
                segments.push((ri, si, false, a.x, a.y.min(b.y), a.y.max(b.y)));
                turns.push([(a.y, side(a.x, before.x)), (b.y, side(b.x, after.x))]);
            }
        }
    }
    // Clusters: chains of segments of different routes that are parallel, overlap and lie within `track`.
    let mut cluster = vec![usize::MAX; segments.len()];
    let mut next = 0;
    for i in 0..segments.len() {
        if cluster[i] != usize::MAX {
            continue;
        }
        cluster[i] = next;
        let mut stack = vec![i];
        while let Some(current) = stack.pop() {
            let (ri, _, hi, ci, li, ui) = segments[current];
            for j in 0..segments.len() {
                let (rj, _, hj, cj, lj, uj) = segments[j];
                if cluster[j] == usize::MAX
                    && ri != rj
                    && hi == hj
                    && (ci - cj).abs() < params.track
                    && ui.min(uj) - li.max(lj) > EPS
                {
                    cluster[j] = next;
                    stack.push(j);
                }
            }
        }
        next += 1;
    }
    let info = parent_map(graph);
    let obstacles: Vec<(String, Rect)> = rects
        .iter()
        .map(|(id, r)| (id.clone(), r.clone()))
        .collect();
    for group in 0..next {
        let members: Vec<usize> = (0..segments.len())
            .filter(|i| cluster[*i] == group)
            .collect();
        let routes_in: BTreeSet<usize> = members.iter().map(|i| segments[*i].0).collect();
        if routes_in.len() < 2 {
            continue;
        }
        let mean = members.iter().map(|i| segments[*i].3).sum::<f64>() / members.len() as f64;
        // The order across the corridor avoids crossings: of two segments, the one whose turning neighbours would
        // otherwise run through the other goes on the side that spares them. A member's score is the crossings it
        // saves by lying on the lower side of the others; members are sorted by it (ties: route, segment).
        let crossings_if_lower = |lower: usize, upper: usize| -> i64 {
            let (_, _, _, _, lo_u, hi_u) = segments[upper];
            let (_, _, _, _, lo_l, hi_l) = segments[lower];
            let into_upper = turns[lower]
                .iter()
                .filter(|(position, side)| {
                    *side > 0 && *position > lo_u + EPS && *position < hi_u - EPS
                })
                .count();
            let into_lower = turns[upper]
                .iter()
                .filter(|(position, side)| {
                    *side < 0 && *position > lo_l + EPS && *position < hi_l - EPS
                })
                .count();
            (into_upper + into_lower) as i64
        };
        let score = |member: usize| -> i64 {
            members
                .iter()
                .filter(|other| **other != member)
                .map(|other| {
                    crossings_if_lower(*other, member) - crossings_if_lower(member, *other)
                })
                .sum()
        };
        let mut ordered = members.clone();
        ordered.sort_by_key(|i| (-score(*i), segments[*i].0, segments[*i].1));
        // Tracks in that order. Layered: a segment lies above every overlapping segment before it, so the order holds
        // for every overlapping pair. Coloured: the lowest track no overlapping earlier segment holds; it needs fewer
        // tracks but may swap a pair. The layered tracks are used when they fit the corridor.
        let overlap = |a: usize, b: usize| {
            let (_, _, _, _, lo, hi) = segments[a];
            let (_, _, _, _, l, u) = segments[b];
            hi.min(u) - lo.max(l) > EPS
        };
        let mut layered: Vec<usize> = Vec::with_capacity(ordered.len());
        let mut coloured: Vec<usize> = Vec::with_capacity(ordered.len());
        for (position, member) in ordered.iter().enumerate() {
            let earlier: Vec<usize> = (0..position)
                .filter(|e| overlap(*member, ordered[*e]))
                .collect();
            layered.push(earlier.iter().map(|e| layered[*e] + 1).max().unwrap_or(0));
            let taken: BTreeSet<usize> = earlier.iter().map(|e| coloured[*e]).collect();
            coloured.push((0..).find(|k| !taken.contains(k)).unwrap_or(0));
        }
        // The free span of the corridor around the group: the nearest obstacle boundary (clearance included) on each
        // side, over the range of each member. The tracks lie inside it; a group that does not fit stays as it is.
        let mut floor = f64::NEG_INFINITY;
        let mut ceiling = f64::INFINITY;
        for member in &ordered {
            let (ri, _, horizontal_segment, _, lo, hi) = segments[*member];
            let Some(edge) = edges.get(result[ri].id.as_str()) else {
                continue;
            };
            let skip = excluded_with(&info, edge);
            for (id, rect) in &obstacles {
                let endpoint = id == &edge.source || edge.target.as_deref() == Some(id.as_str());
                if !(endpoint || !skip.contains(id.as_str())) {
                    continue;
                }
                let outline = obstacle(
                    id,
                    rect,
                    info.get(id.as_str())
                        .map_or_else(NodeShape::default, |(_, s)| *s),
                    edge,
                    params,
                );
                let mut b = outline.bounds();
                if endpoint && params.own_clearance > 0. {
                    // The tracks keep the own clearance from the edge's own nodes.
                    b = Rect {
                        x: b.x - params.own_clearance,
                        y: b.y - params.own_clearance,
                        width: b.width + 2. * params.own_clearance,
                        height: b.height + 2. * params.own_clearance,
                    };
                }
                let (along0, along1, across0, across1) = if horizontal_segment {
                    (b.x, b.x + b.width, b.y, b.y + b.height)
                } else {
                    (b.y, b.y + b.height, b.x, b.x + b.width)
                };
                if along1.min(hi) - along0.max(lo) <= EPS {
                    continue;
                }
                if across1 <= mean + EPS {
                    floor = floor.max(across1);
                } else if across0 >= mean - EPS {
                    ceiling = ceiling.min(across0);
                }
            }
        }
        let span = ceiling - floor;
        let width = |tracks: &[usize]| tracks.iter().max().map_or(0, |m| *m) as f64 * params.track;
        let (track_of, needed) = if span >= width(&layered) - EPS {
            let needed = width(&layered);
            (layered, needed)
        } else if span >= width(&coloured) - EPS {
            let needed = width(&coloured);
            (coloured, needed)
        } else {
            continue;
        };
        let start = (mean - needed / 2.).max(floor).min(ceiling - needed);
        for (order, member) in ordered.iter().enumerate() {
            let (ri, si, horizontal_segment, _, _, _) = segments[*member];
            let target = start + track_of[order] as f64 * params.track;
            let mut candidate = result[ri].clone();
            if horizontal_segment {
                candidate.points[si].y = target;
                candidate.points[si + 1].y = target;
            } else {
                candidate.points[si].x = target;
                candidate.points[si + 1].x = target;
            }
            // Port stubs keep their direction: a moved bend may not turn a stub back towards its port.
            if stub_signs(&candidate) != stub_signs(&result[ri]) {
                continue;
            }
            // A move is kept only if it adds no hit against nodes the route may not cross.
            let Some(edge) = edges.get(candidate.id.as_str()) else {
                continue;
            };
            let skip = excluded_with(&info, edge);
            let count = |route: &Route| {
                obstacles
                    .iter()
                    .filter(|(id, rect)| {
                        let endpoint =
                            id == &edge.source || edge.target.as_deref() == Some(id.as_str());
                        (endpoint || !skip.contains(id.as_str()))
                            && hits_outline(
                                route,
                                &obstacle(
                                    id,
                                    rect,
                                    info.get(id.as_str())
                                        .map_or_else(NodeShape::default, |(_, s)| *s),
                                    edge,
                                    params,
                                ),
                            )
                    })
                    .count()
            };
            if count(&candidate) <= count(&result[ri]) {
                result[ri] = candidate;
            }
        }
    }
    result
}
/// Best route for one edge among candidate tracks, scored against the obstacles and the occupied routes.
fn best_route(
    original: &Route,
    edge: &Edge,
    rects: &BTreeMap<String, Rect>,
    parents: &NodeInfo<'_>,
    occupied: &[Probe],
    params: Params,
    counters: &mut channel::SearchCounters,
) -> Route {
    let excluded = excluded_with(parents, edge);
    let owned: Vec<Outline> = rects
        .iter()
        .filter(|(id, _)| {
            !excluded.contains(*id) || *id == &edge.source || edge.target.as_ref() == Some(*id)
        })
        .map(|(id, r)| obstacle(id, r, shape_in(parents, id), edge, params))
        .collect();
    let obstacles: Vec<&Outline> = owned.iter().collect();
    // Own nodes keep the true body for the hit test; their rails lie beyond the own clearance.
    let own: Vec<bool> = rects
        .keys()
        .filter(|id| {
            !excluded.contains(*id) || *id == &edge.source || edge.target.as_ref() == Some(*id)
        })
        .map(|id| id == &edge.source || edge.target.as_ref() == Some(id))
        .collect();
    let start = original.points.first().unwrap();
    let end = original.points.last().unwrap();
    let mut options = vec![original.clone()];
    // Preserve first/last segments and therefore their endpoint directions.
    let stub = |endpoint: &Point, toward: &Point, distance: f64| {
        let dx = toward.x - endpoint.x;
        let dy = toward.y - endpoint.y;
        Point {
            x: endpoint.x
                + if dx.abs() < EPS {
                    0.
                } else {
                    dx.signum() * distance
                },
            y: endpoint.y
                + if dy.abs() < EPS {
                    0.
                } else {
                    dy.signum() * distance
                },
        }
    };
    // Both axes get independent tracks, including offsets from already occupied
    // tracks. Bound the search for the small-view refinement budget.
    let rails = |vertical: bool| {
        let coord = |p: &Point| if vertical { p.x } else { p.y };
        let a = coord(start);
        let b = coord(end);
        let mut values: Vec<f64> = obstacles
            .iter()
            .zip(&own)
            .flat_map(|(o, own)| {
                let mut r = o.bounds();
                if *own && params.own_clearance > 0. {
                    r = Rect {
                        x: r.x - params.own_clearance,
                        y: r.y - params.own_clearance,
                        width: r.width + 2. * params.own_clearance,
                        height: r.height + 2. * params.own_clearance,
                    };
                }
                let mut tracks = if vertical {
                    vec![r.x - params.rail_offset, r.x + r.width + params.rail_offset]
                } else {
                    vec![
                        r.y - params.rail_offset,
                        r.y + r.height + params.rail_offset,
                    ]
                };
                // A disc has free corners inside its box: tracks just inside the box reach them.
                if matches!(o, Outline::Disc { .. }) {
                    tracks.extend(if vertical {
                        [r.x + params.rail_offset, r.x + r.width - params.rail_offset]
                    } else {
                        [
                            r.y + params.rail_offset,
                            r.y + r.height - params.rail_offset,
                        ]
                    });
                    // The box edges themselves touch the disc at one point: a track there is free and shorter than one
                    // beyond the box.
                    tracks.extend(if vertical {
                        [r.x, r.x + r.width]
                    } else {
                        [r.y, r.y + r.height]
                    });
                }
                tracks
            })
            .collect();
        values.extend([a.min(b) - 24., a.max(b) + 24., (a + b) / 2.]);
        for route in occupied {
            for w in route.points.windows(2) {
                if (vertical && (w[0].x - w[1].x).abs() < EPS)
                    || (!vertical && (w[0].y - w[1].y).abs() < EPS)
                {
                    let c = coord(&w[0]);
                    values.extend([c - params.track, c + params.track]);
                }
            }
        }
        values.sort_by(|x, y| x.total_cmp(y));
        values.dedup_by(|x, y| (*x - *y).abs() < EPS);
        // Corridor centres: the middle of each free gap between obstacle boundaries, so a route can
        // run in the middle of a corridor instead of hugging a boundary.
        if params.corridors {
            let mut boundaries: Vec<f64> = obstacles
                .iter()
                .flat_map(|o| {
                    let r = o.bounds();
                    if vertical {
                        [r.x, r.x + r.width]
                    } else {
                        [r.y, r.y + r.height]
                    }
                })
                .collect();
            boundaries.sort_by(|x, y| x.total_cmp(y));
            boundaries.dedup_by(|x, y| (*x - *y).abs() < EPS);
            for pair in boundaries.windows(2) {
                if pair[1] - pair[0] > 2. * params.track {
                    values.push((pair[0] + pair[1]) / 2.);
                }
            }
        }
        values.sort_by(|x, y| x.total_cmp(y));
        values.dedup_by(|x, y| (*x - *y).abs() < EPS);
        values.sort_by(|x, y| {
            ((x - a).abs() + (x - b).abs())
                .total_cmp(&((y - a).abs() + (y - b).abs()))
                .then_with(|| ((x - (a + b) / 2.).abs()).total_cmp(&(y - (a + b) / 2.).abs()))
                .then_with(|| x.total_cmp(y))
        });
        values.truncate(48);
        values
    };
    let xs = rails(true);
    let ys = rails(false);
    let stubs = if params.own_clearance > 0. {
        [
            params.own_clearance,
            params.own_clearance + 6.,
            params.own_clearance + 12.,
        ]
    } else {
        [6., 12., 18.]
    };
    for distance in stubs {
        let departure = stub(start, &original.points[1], distance);
        let arrival = stub(end, &original.points[original.points.len() - 2], distance);
        for &y in &ys {
            options.push(clean(Route {
                weight: 1,
                bundle_of: None,
                id: original.id.clone(),
                points: vec![
                    start.clone(),
                    departure.clone(),
                    Point { x: departure.x, y },
                    Point { x: arrival.x, y },
                    arrival.clone(),
                    end.clone(),
                ],
            }));
        }
        for &x in &xs {
            options.push(clean(Route {
                weight: 1,
                bundle_of: None,
                id: original.id.clone(),
                points: vec![
                    start.clone(),
                    departure.clone(),
                    Point { x, y: departure.y },
                    Point { x, y: arrival.y },
                    arrival.clone(),
                    end.clone(),
                ],
            }));
        }
    }
    // With an own clearance, the part of a route between its stubs keeps that clearance from the edge's own nodes. The
    // stubs leave the body by construction, so only the middle is judged; a route that runs along its own node's
    // border is worse than any that does not.
    let zones: Vec<Outline> = if params.own_clearance > 0. {
        rects
            .iter()
            .filter(|(id, _)| *id == &edge.source || edge.target.as_ref() == Some(*id))
            .map(|(id, r)| {
                let c = params.own_clearance;
                let zone = Rect {
                    x: r.x - c,
                    y: r.y - c,
                    width: r.width + 2. * c,
                    height: r.height + 2. * c,
                };
                Outline::of(shape_in(parents, id), &zone)
            })
            .collect()
    } else {
        Vec::new()
    };
    let in_zone = |route: &Route| -> usize {
        if zones.is_empty() || route.points.len() < 4 {
            return 0;
        }
        let inner = Route::new("", route.points[1..route.points.len() - 1].to_vec());
        let middle = probe(&inner);
        zones
            .iter()
            .filter(|z| hits_outline_probe(&middle, z))
            .count()
    };
    let mut best = original.clone();
    let mut best_zone = in_zone(&best);
    counters.legacy_candidates += 1;
    let mut best_score = score(&probe(&best), &obstacles, occupied);
    for option in options {
        counters.legacy_candidates += 1;
        let zone = in_zone(&option);
        if zone > best_zone {
            continue;
        }
        if zone < best_zone {
            best_score = score(&probe(&option), &obstacles, occupied);
            best_zone = zone;
            best = option;
        } else if let Some(candidate) =
            better_score(&probe(&option), &obstacles, occupied, &best_score)
        {
            best_score = candidate;
            best = option;
        }
    }
    best
}

fn improve_pass(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    fixed: &[Route],
    reserve_later: bool,
    params: Params,
    stop: &dyn Fn() -> bool,
    counters: &mut channel::SearchCounters,
) -> Result<Vec<Route>, channel::Stopped> {
    let edges: BTreeMap<_, _> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let parents = parent_map(graph);
    let fixed_probes: Vec<Probe> = fixed.iter().map(probe).collect();
    let mut result: Vec<Route> = Vec::new();
    for (index, original) in routes.iter().enumerate() {
        if stop() {
            return Err(channel::Stopped);
        }
        // References only: copying whole routes for every candidate dominated the search cost.
        let mut occupied: Vec<Probe> = result.iter().map(probe).collect();
        occupied.extend(fixed_probes.iter().map(|p| Probe {
            route: p.route,
            bounds: p.bounds,
        }));
        if reserve_later {
            occupied.extend(routes[index + 1..].iter().map(probe));
        }
        let edge = edges[original.id.as_str()];
        if original.points.len() < 2 {
            result.push(original.clone());
            continue;
        }
        let best = best_route(original, edge, rects, &parents, &occupied, params, counters);
        result.push(best);
    }
    Ok(result)
}
/// Repair remaining node penetrations with a bounded rectilinear visibility grid.
/// Ancestor interiors are traversable; endpoint bodies are obstacles too.
pub fn repair(graph: &Graph, rects: &BTreeMap<String, Rect>, routes: &[Route]) -> Vec<Route> {
    repair_with(graph, rects, routes, Params::default())
}

pub fn repair_with(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    params: Params,
) -> Vec<Route> {
    repair_with_counted(
        graph,
        rects,
        routes,
        params,
        &mut channel::SearchCounters::default(),
    )
}

/// `repair_with` that also adds the grid work it performs to `counters`.
pub fn repair_with_counted(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    params: Params,
    counters: &mut channel::SearchCounters,
) -> Vec<Route> {
    simplify(repair_raw(graph, rects, routes, params, counters))
}

/// Repairs only the `affected` edges of a drawing that continues an earlier one. The other edges keep their route from `kept`;
/// a kept route that now runs into an obstacle is repaired as well, so the result has no node hit that the full repair would
/// have removed. An edge without a kept route counts as affected. The result lists the routes in the order of `seeds`.
pub fn repair_subset_counted(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    seeds: &[Route],
    kept: &[Route],
    affected: &BTreeSet<String>,
    params: Params,
    counters: &mut channel::SearchCounters,
) -> Vec<Route> {
    let kept_by_id: BTreeMap<&str, &Route> = kept.iter().map(|r| (r.id.as_str(), r)).collect();
    let (fresh, old): (Vec<Route>, Vec<Route>) = seeds
        .iter()
        .map(|seed| match kept_by_id.get(seed.id.as_str()) {
            Some(route) if !affected.contains(&seed.id) => (None, Some((*route).clone())),
            _ => (Some(seed.clone()), None),
        })
        .fold((Vec::new(), Vec::new()), |(mut fresh, mut old), (a, b)| {
            fresh.extend(a);
            old.extend(b);
            (fresh, old)
        });
    counters.routes_rerouted += fresh.len();
    counters.routes_kept += old.len();
    let mut done: BTreeMap<String, Route> =
        simplify(repair_raw(graph, rects, &fresh, params, counters))
            .into_iter()
            .map(|r| (r.id.clone(), r))
            .collect();
    done.extend(
        simplify(repair_raw(graph, rects, &old, params, counters))
            .into_iter()
            .map(|r| (r.id.clone(), r)),
    );
    seeds
        .iter()
        .map(|seed| done.remove(&seed.id).unwrap_or_else(|| seed.clone()))
        .collect()
}

/// How many routes run into a node that they may not pass through, by the obstacle rules of the repair (ancestors of the ends
/// are traversable, the ends themselves are obstacles). The check that a local repair did not leave the drawing worse.
pub fn routes_hitting_nodes(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    params: Params,
) -> usize {
    let edges: BTreeMap<_, _> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let info = parent_map(graph);
    // An obstacle is its node's box grown by the clearance: a node whose grown box does not meet the route's box cannot be
    // hit, and for most nodes of a large view that decides without building the obstacle or the set of ancestors.
    let grow = params.clearance.max(0.);
    routes
        .iter()
        .filter(|route| route.bundle_of.is_none())
        .filter(|route| {
            let Some(edge) = edges.get(route.id.as_str()) else {
                return false;
            };
            let bounds = route_bounds(route);
            let near: Vec<(&String, &Rect)> = rects
                .iter()
                .filter(|(_, r)| {
                    !apart(
                        &bounds,
                        &[
                            r.x - grow,
                            r.y - grow,
                            r.x + r.width + grow,
                            r.y + r.height + grow,
                        ],
                    )
                })
                .collect();
            if near.is_empty() {
                return false;
            }
            let skip = excluded_with(&info, edge);
            near.into_iter()
                .filter(|(id, _)| {
                    !skip.contains(*id) || *id == &edge.source || edge.target.as_ref() == Some(*id)
                })
                .any(|(id, r)| {
                    hits_outline(route, &obstacle(id, r, shape_in(&info, id), edge, params))
                })
        })
        .count()
}

fn repair_raw(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    params: Params,
    counters: &mut channel::SearchCounters,
) -> Vec<Route> {
    let edges: BTreeMap<_, _> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let info = parent_map(graph);
    routes
        .iter()
        .map(|route| {
            let Some(edge) = edges.get(route.id.as_str()) else {
                return route.clone();
            };
            // Most routes run into nothing: only the nodes whose grown box meets the route's box can be hit, so the obstacles of all
            // the others are built only for a route that does hit one (the grid search needs them all).
            let skip = excluded_with(&info, edge);
            let allowed = |id: &String| {
                !skip.contains(id) || id == &edge.source || edge.target.as_ref() == Some(id)
            };
            let bounds = route_bounds(route);
            let grow = params.clearance.max(0.);
            let hit = rects
                .iter()
                .filter(|(id, r)| {
                    allowed(id)
                        && !apart(
                            &bounds,
                            &[
                                r.x - grow,
                                r.y - grow,
                                r.x + r.width + grow,
                                r.y + r.height + grow,
                            ],
                        )
                })
                .any(|(id, r)| {
                    hits_outline(route, &obstacle(id, r, shape_in(&info, id), edge, params))
                });
            if !hit {
                return route.clone();
            }
            let owned: Vec<Outline> = rects
                .iter()
                .filter(|(id, _)| {
                    !skip.contains(*id) || *id == &edge.source || edge.target.as_ref() == Some(*id)
                })
                .map(|(id, r)| obstacle(id, r, shape_in(&info, id), edge, params))
                .collect();
            let obstacles: Vec<&Outline> = owned.iter().collect();
            let own: Vec<bool> = rects
                .keys()
                .filter(|id| {
                    !skip.contains(*id) || *id == &edge.source || edge.target.as_ref() == Some(*id)
                })
                .map(|id| id == &edge.source || edge.target.as_ref() == Some(id))
                .collect();
            if obstacles.iter().any(|o| hits_outline(route, o)) {
                grid_route(route, &obstacles, &own, params.own_clearance, counters)
                    .unwrap_or_else(|| route.clone())
            } else {
                route.clone()
            }
        })
        .collect()
}

/// A uniform grid over a set of boxes, to find the boxes that meet a query box without looking at all of them. `any_near`
/// tests exactly the boxes `apart` does not separate from the query (closed boxes that touch count as meeting).
struct BoxIndex {
    origin: (f64, f64),
    cell: f64,
    columns: usize,
    rows: usize,
    cells: Vec<Vec<u32>>,
}

impl BoxIndex {
    const TARGET_CELLS: f64 = 48.;

    fn new(boxes: &[Bounds]) -> Self {
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for b in boxes {
            x0 = x0.min(b[0]);
            y0 = y0.min(b[1]);
            x1 = x1.max(b[2]);
            y1 = y1.max(b[3]);
        }
        if boxes.is_empty() || !(x1 - x0).is_finite() || !(y1 - y0).is_finite() {
            return Self {
                origin: (0., 0.),
                cell: 1.,
                columns: 0,
                rows: 0,
                cells: Vec::new(),
            };
        }
        let cell = ((x1 - x0).max(y1 - y0) / Self::TARGET_CELLS).max(1.);
        let columns = ((x1 - x0) / cell) as usize + 1;
        let rows = ((y1 - y0) / cell) as usize + 1;
        let mut index = Self {
            origin: (x0, y0),
            cell,
            columns,
            rows,
            cells: vec![Vec::new(); columns * rows],
        };
        for (i, b) in boxes.iter().enumerate() {
            let (c0, c1, r0, r1) = index.span(b);
            for r in r0..=r1 {
                for c in c0..=c1 {
                    index.cells[r * columns + c].push(i as u32);
                }
            }
        }
        index
    }

    /// The cells a closed box covers, clamped to the grid.
    fn span(&self, b: &Bounds) -> (usize, usize, usize, usize) {
        let col = |x: f64| {
            (((x - self.origin.0) / self.cell).floor().max(0.) as usize).min(self.columns - 1)
        };
        let row = |y: f64| {
            (((y - self.origin.1) / self.cell).floor().max(0.) as usize).min(self.rows - 1)
        };
        (col(b[0]), col(b[2]), row(b[1]), row(b[3]))
    }

    /// Whether `test` holds for any box that meets `query`; a box in several cells may be tested again.
    fn any_near(
        &self,
        boxes: &[Bounds],
        query: &Bounds,
        mut test: impl FnMut(usize) -> bool,
    ) -> bool {
        if self.cells.is_empty() {
            return false;
        }
        let (c0, c1, r0, r1) = self.span(query);
        for r in r0..=r1 {
            for c in c0..=c1 {
                for &i in &self.cells[r * self.columns + c] {
                    if !apart(query, &boxes[i as usize]) && test(i as usize) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

fn grid_route(
    route: &Route,
    obstacles: &[&Outline],
    own: &[bool],
    own_clearance: f64,
    counters: &mut channel::SearchCounters,
) -> Option<Route> {
    let before = counters.legacy_grid_pops;
    let found = grid_route_search(route, obstacles, own, own_clearance, counters);
    *counters
        .legacy_grid_pops_by_route
        .entry(route.id.clone())
        .or_default() += counters.legacy_grid_pops - before;
    found
}

fn grid_route_search(
    route: &Route,
    obstacles: &[&Outline],
    own: &[bool],
    own_clearance: f64,
    counters: &mut channel::SearchCounters,
) -> Option<Route> {
    if route.points.len() < 2 {
        return None;
    }
    let start = route.points.first()?;
    let end = route.points.last()?;
    // The stub leaves the port at least the own clearance, and the lines beside an own node lie beyond it.
    let stub_length = own_clearance.max(6.);
    let stub = |p: &Point, q: &Point| Point {
        x: p.x
            + if (q.x - p.x).abs() < EPS {
                0.
            } else {
                (q.x - p.x).signum() * stub_length
            },
        y: p.y
            + if (q.y - p.y).abs() < EPS {
                0.
            } else {
                (q.y - p.y).signum() * stub_length
            },
    };
    let a = stub(start, &route.points[1]);
    let b = stub(end, &route.points[route.points.len() - 2]);
    // The grid lines: beside every obstacle (those that meet `window`, when there is one) and at the two ends of the route.
    let lines = |window: Option<&Bounds>| -> (Vec<f64>, Vec<f64>) {
        let mut xs = vec![a.x, b.x];
        let mut ys = vec![a.y, b.y];
        for (index, o) in obstacles.iter().enumerate() {
            let mut r = o.bounds();
            if own.get(index) == Some(&true) && own_clearance > 0. {
                r = Rect {
                    x: r.x - own_clearance,
                    y: r.y - own_clearance,
                    width: r.width + 2. * own_clearance,
                    height: r.height + 2. * own_clearance,
                };
            }
            if window.is_some_and(|w| apart(w, &rect_bounds(&r))) {
                continue;
            }
            xs.extend([r.x - 6., r.x + r.width + 6.]);
            ys.extend([r.y - 6., r.y + r.height + 6.]);
            // A disc's free corners inside its box need grid lines too.
            if matches!(o, Outline::Disc { .. }) {
                xs.extend([r.x + 6., r.x + r.width - 6.]);
                ys.extend([r.y + 6., r.y + r.height - 6.]);
            }
        }
        xs.sort_by(f64::total_cmp);
        xs.dedup();
        ys.sort_by(f64::total_cmp);
        ys.dedup();
        (xs, ys)
    };
    let (xs, ys) = lines(None);
    if xs.len().checked_mul(ys.len())? <= GRID_LIMIT {
        return search_grid(route, obstacles, start, end, (&a, &b), xs, ys, counters);
    }
    // The whole view gives more grid lines than the search can hold. Before the route is given up, the lines are taken only from
    // the obstacles near it, in windows that grow (the freedom of a step is still tested against every obstacle, so a route found
    // in a window is as valid as one found on the whole grid). Views within the limit never get here, so their output is unchanged.
    let around = route_bounds(route);
    for reach in WINDOW_REACH {
        let window = [
            around[0] - reach,
            around[1] - reach,
            around[2] + reach,
            around[3] + reach,
        ];
        let (xs, ys) = lines(Some(&window));
        if xs.len().checked_mul(ys.len())? > GRID_LIMIT {
            break;
        }
        if let Some(found) = search_grid(route, obstacles, start, end, (&a, &b), xs, ys, counters) {
            counters.windowed_grids += 1;
            return Some(found);
        }
    }
    None
}

/// The most grid cells the repair search holds for one route.
const GRID_LIMIT: usize = 120_000;
/// How far around a route's box (layout units) the windows reach, when the whole view gives too many grid lines.
const WINDOW_REACH: [f64; 3] = [120., 480., 1920.];

/// A* over the grid `xs` x `ys` from `a` to `b`, using the Dijkstra tie order for equal-cost paths.
#[allow(clippy::too_many_arguments)]
fn search_grid(
    route: &Route,
    obstacles: &[&Outline],
    start: &Point,
    end: &Point,
    (a, b): (&Point, &Point),
    xs: Vec<f64>,
    ys: Vec<f64>,
    counters: &mut channel::SearchCounters,
) -> Option<Route> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let width = xs.len();
    let count = width.checked_mul(ys.len())?;
    let at = |p: &Point| {
        Some(
            ys.binary_search_by(|v| v.total_cmp(&p.y)).ok()? * width
                + xs.binary_search_by(|v| v.total_cmp(&p.x)).ok()?,
        )
    };
    let source = at(a)?;
    let target = at(b)?;
    let point = |i: usize| Point {
        x: xs[i % width],
        y: ys[i / width],
    };
    // The same prefilter `hits_outline` applies first, here without building a route for every grid step: only the obstacles
    // whose box meets the segment's box are tested, so the result is the one a test against all obstacles gives.
    let boxes: Vec<Bounds> = obstacles.iter().map(|o| rect_bounds(&o.bounds())).collect();
    let grid = BoxIndex::new(&boxes);
    let free = |p: &Point, q: &Point| {
        let reach = [p.x.min(q.x), p.y.min(q.y), p.x.max(q.x), p.y.max(q.y)];
        let segment = [p.clone(), q.clone()];
        !grid.any_near(&boxes, &reach, |i| {
            hits_outline_points(&reach, &segment, obstacles[i])
        })
    };
    if !free(start, a) || !free(b, end) {
        return None;
    }
    let mut costs = vec![u64::MAX; count];
    let mut queue = BinaryHeap::new();
    costs[source] = 0;
    let target_point = point(target);
    let remaining_cost = |i: usize| {
        let p = point(i);
        (((p.x - target_point.x).abs() + (p.y - target_point.y).abs()) * 1000.)
            .floor()
            .min(u64::MAX as f64) as u64
    };
    let edge_cost = |p: &Point, q: &Point| {
        (((p.x - q.x).abs() + (p.y - q.y).abs()) * 1000.)
            .ceil()
            .min(u64::MAX as f64) as u64
    };
    queue.push(Reverse((remaining_cost(source), 0u64, source)));
    let mut target_cost = None;
    // Drain the optimal f-score frontier so equal-cost predecessors are available for Dijkstra-compatible reconstruction.
    while let Some(Reverse((estimate, cost, i))) = queue.pop() {
        if target_cost.is_some_and(|best| estimate > best) {
            break;
        }
        counters.legacy_grid_pops += 1;
        if cost != costs[i] {
            continue;
        }
        if i == target {
            target_cost = Some(cost);
            continue;
        }
        let p = point(i);
        let neighbors = [
            (i % width > 0).then(|| i - 1),
            (i % width + 1 < width).then(|| i + 1),
            (i >= width).then(|| i - width),
            (i + width < count).then(|| i + width),
        ];
        for j in neighbors.into_iter().flatten() {
            let q = point(j);
            let next = cost.saturating_add(edge_cost(&p, &q)).saturating_add(1);
            if next < costs[j] && free(&p, &q) {
                costs[j] = next;
                queue.push(Reverse((next.saturating_add(remaining_cost(j)), next, j)));
            }
        }
    }
    if costs[target] == u64::MAX {
        return None;
    }
    let mut path = vec![end.clone()];
    let mut cursor = target;
    loop {
        path.push(point(cursor));
        if cursor == source {
            break;
        }
        let p = point(cursor);
        let parents = [
            (cursor % width > 0).then(|| cursor - 1),
            (cursor % width + 1 < width).then(|| cursor + 1),
            (cursor >= width).then(|| cursor - width),
            (cursor + width < count).then(|| cursor + width),
        ];
        cursor = parents
            .into_iter()
            .flatten()
            .filter_map(|parent| {
                if costs[parent] == u64::MAX {
                    return None;
                }
                let q = point(parent);
                let candidate = costs[parent]
                    .saturating_add(edge_cost(&q, &p))
                    .saturating_add(1);
                (candidate == costs[cursor] && free(&q, &p)).then_some((costs[parent], parent))
            })
            // Dijkstra's queue takes the first shortest predecessor by (distance, grid index).
            .min()
            .map(|(_, parent)| parent)?;
    }
    path.push(start.clone());
    path.reverse();
    let mut repaired = route.clone();
    repaired.points = path;
    Some(clean(repaired))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn r(points: &[(f64, f64)]) -> Route {
        Route {
            weight: 1,
            bundle_of: None,
            id: "e".into(),
            points: points.iter().map(|(x, y)| Point { x: *x, y: *y }).collect(),
        }
    }
    #[test]
    fn corridor_search_avoids_staggered_obstacles_and_endpoint_bodies() {
        let a = Rect {
            x: 0.,
            y: 0.,
            width: 10.,
            height: 10.,
        };
        let b = Rect {
            x: 100.,
            y: 0.,
            width: 10.,
            height: 10.,
        };
        let one = Rect {
            x: 25.,
            y: -20.,
            width: 15.,
            height: 50.,
        };
        let two = Rect {
            x: 60.,
            y: -40.,
            width: 15.,
            height: 50.,
        };
        let outlines: Vec<Outline> = [&a, &b, &one, &two]
            .into_iter()
            .map(|r| Outline::Rect(r.clone()))
            .collect();
        let obstacles: Vec<&Outline> = outlines.iter().collect();
        let original = r(&[(10., 5.), (16., 5.), (94., 5.), (100., 5.)]);
        let repaired = grid_route(
            &original,
            &obstacles,
            &[],
            0.,
            &mut channel::SearchCounters::default(),
        )
        .unwrap();
        assert!(obstacles.iter().all(|o| !hits_outline(&repaired, o)));
        assert!(repaired
            .points
            .windows(2)
            .all(|w| horizontal(&w[0], &w[1]) || vertical(&w[0], &w[1])));
        assert!(repaired.points[1].x > 10.);
        assert!(repaired.points[repaired.points.len() - 2].x < 100.);
    }
    /// A view with so many distinct coordinates that the grid of the whole view is over the limit of the search: the route is not given
    /// up, it is found on a grid from the obstacles near it, and still avoids every obstacle (also the far ones).
    #[test]
    fn a_route_is_found_when_the_grid_of_the_whole_view_is_over_the_limit() {
        let mut outlines: Vec<Outline> = [
            (0., 0., 10., 10.),
            (100., 0., 10., 10.),
            (25., -20., 15., 50.),
            (60., -40., 15., 50.),
        ]
        .into_iter()
        .map(|(x, y, width, height)| {
            Outline::Rect(Rect {
                x,
                y,
                width,
                height,
            })
        })
        .collect();
        // Far from the route, 400 boxes at coordinates of their own: 1,600 lines on each axis.
        for i in 0..400 {
            let t = i as f64;
            outlines.push(Outline::Rect(Rect {
                x: 1000. + t * 7.31,
                y: 1000. + t * 5.17,
                width: 3.,
                height: 3.,
            }));
        }
        let obstacles: Vec<&Outline> = outlines.iter().collect();
        let original = r(&[(10., 5.), (16., 5.), (94., 5.), (100., 5.)]);
        let mut counters = channel::SearchCounters::default();
        let repaired = grid_route(&original, &obstacles, &[], 0., &mut counters)
            .expect("the route is found on a window of the grid");
        assert_eq!(counters.windowed_grids, 1);
        assert!(obstacles.iter().all(|o| !hits_outline(&repaired, o)));
        assert!(repaired
            .points
            .windows(2)
            .all(|w| horizontal(&w[0], &w[1]) || vertical(&w[0], &w[1])));
        // Without the far boxes the whole grid is within the limit and no window is used.
        let near: Vec<&Outline> = outlines[..4].iter().collect();
        let mut counters = channel::SearchCounters::default();
        assert!(grid_route(&original, &near, &[], 0., &mut counters).is_some());
        assert_eq!(counters.windowed_grids, 0);
    }
    /// A straight run split at a vertex is one line: a route crossing it exactly at that vertex crosses once. Touching
    /// an end does not cross; a reversal (spike) is kept as drawn; redundant points on a run change nothing.
    #[test]
    fn crossing_at_a_split_vertex_is_found() {
        let whole = r(&[(0., 5.), (10., 5.)]);
        let split = r(&[(0., 5.), (4., 5.), (10., 5.)]);
        let redundant = r(&[(0., 5.), (2., 5.), (4., 5.), (4., 5.), (7., 5.), (10., 5.)]);
        let through_vertex = r(&[(4., 0.), (4., 10.)]);
        assert!(crosses(&whole, &through_vertex));
        assert!(crosses(&split, &through_vertex));
        assert!(crosses(&redundant, &through_vertex));
        let ends_on_run = r(&[(10., 0.), (10., 5.)]);
        assert!(!crosses(&whole, &ends_on_run));
        assert!(!crosses(&split, &ends_on_run));
        let spike = r(&[(0., 5.), (4., 5.), (2., 5.), (10., 5.)]);
        assert!(crosses(&spike, &r(&[(3., 0.), (3., 10.)])));
        let spike_tip = r(&[(0., 5.), (4., 5.), (2., 5.)]);
        assert!(!crosses(&spike_tip, &through_vertex));
        // Known, deliberate difference: the routing decisions (`score`, `route`) still use the drawn points through
        // `crosses_probe`, which misses the crossing at a split vertex. Only the quality values use `crosses`.
        assert!(!crosses_probe(&probe(&split), &probe(&through_vertex)));
    }

    #[test]
    fn detects_interior_crossing_but_not_endpoint() {
        assert!(crosses(
            &r(&[(0., 5.), (10., 5.)]),
            &r(&[(5., 0.), (5., 10.)])
        ));
        assert!(!crosses(
            &r(&[(0., 5.), (5., 5.)]),
            &r(&[(5., 0.), (5., 10.)])
        ));
    }
    #[test]
    fn shared_stretches_are_separate_from_crossings() {
        let a = r(&[(0., 0.), (10., 0.)]);
        let b = r(&[(5., 0.), (15., 0.)]);
        assert!(shares_segment(&a, &b));
        assert!(!crosses(&a, &b));
    }
    #[test]
    fn separates_shared_tracks_on_both_axes() {
        for transpose in [false, true] {
            let mut routes = Vec::new();
            let mut edges = Vec::new();
            for i in 0..6 {
                let y = i as f64 * 10.;
                let mut route = r(&[
                    (0., y),
                    (12., y),
                    (50., y),
                    (50., 100. + y),
                    (88., 100. + y),
                    (100., 100. + y),
                ]);
                route.id = format!("e{i}");
                if transpose {
                    for p in &mut route.points {
                        std::mem::swap(&mut p.x, &mut p.y);
                    }
                }
                edges.push(Edge {
                    id: route.id.clone(),
                    source: format!("a{i}"),
                    target: Some(format!("b{i}")),
                    kind: "calls".into(),
                });
                routes.push(route);
            }
            let graph = Graph {
                default_direction: DirectionConstraint::Fixed(Orientation::Lr),
                container_directions: BTreeMap::new(),
                default_strategy: Default::default(),
                container_strategies: Default::default(),
                default_routing: Default::default(),
                container_routing: Default::default(),
                nodes: vec![],
                edges,
                ports: vec![],
                edge_ports: vec![],
                buses: Vec::new(),
            };
            assert!(shares_segment(&routes[0], &routes[1]));
            let result = improve(&graph, &BTreeMap::new(), &routes);
            for (i, route) in result.iter().enumerate() {
                assert_eq!(route.points.first().unwrap().x, routes[i].points[0].x);
                assert_eq!(route.points.first().unwrap().y, routes[i].points[0].y);
                assert_eq!(
                    route.points.last().unwrap().x,
                    routes[i].points.last().unwrap().x
                );
                assert_eq!(
                    route.points.last().unwrap().y,
                    routes[i].points.last().unwrap().y
                );
                assert!(route
                    .points
                    .windows(2)
                    .all(|w| horizontal(&w[0], &w[1]) || vertical(&w[0], &w[1])));
                for other in &result[..i] {
                    assert!(!shares_segment(route, other));
                }
            }
        }
    }
    /// Own clearance (ADR 0005): beyond its stubs, a route keeps the clearance from its own nodes too. A loop under
    /// its node, whose cheapest track lies on the node's border because a foreign node sits just below, is moved away.
    #[test]
    fn a_loop_keeps_the_own_clearance_beyond_its_stubs() {
        let node = |id: &str| Node {
            id: id.into(),
            label: id.into(),
            kind: "symbol".into(),
            parent_id: None,
            shape: Default::default(),
        };
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![node("a"), node("f")],
            edges: vec![Edge {
                id: "e".into(),
                kind: "calls".into(),
                source: "a".into(),
                target: Some("a".into()),
            }],
            buses: Vec::new(),
        };
        let rect = |y: f64| Rect {
            x: 0.,
            y,
            width: 40.,
            height: 40.,
        };
        // The track beyond the inflated foreign node (top 58, clearance 12, track 6) lies at y = 40: the border of `a`.
        let rects = BTreeMap::from([("a".into(), rect(0.)), ("f".into(), rect(58.))]);
        let along = r(&[
            (40., 20.),
            (58., 20.),
            (58., 40.),
            (-18., 40.),
            (-18., 20.),
            (0., 20.),
        ]);
        let params = Params {
            clearance: 12.,
            own_clearance: 12.,
            rail_offset: 6.,
            ..Params::default()
        };
        let improved = improve_with(&graph, &rects, &[along], params);
        let route = &improved[0];
        let n = route.points.len();
        let body = &rects["a"];
        for w in route.points[1..n - 1].windows(2) {
            for step in 0..=10 {
                let t = f64::from(step) / 10.;
                let p = Point {
                    x: w[0].x + (w[1].x - w[0].x) * t,
                    y: w[0].y + (w[1].y - w[0].y) * t,
                };
                let d = (body.x - p.x)
                    .max(0.)
                    .max(p.x - body.x - body.width)
                    .hypot((body.y - p.y).max(0.).max(p.y - body.y - body.height));
                assert!(
                    d >= 12. - 1e-6,
                    "{:?} comes within the own clearance of its node",
                    route.points
                );
            }
        }
    }
    fn two_edge_graph(nodes: &[&str], edges: &[(&str, &str, &str)]) -> Graph {
        let node = |id: &str| Node {
            id: id.into(),
            label: id.into(),
            kind: "symbol".into(),
            parent_id: None,
            shape: Default::default(),
        };
        Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: nodes.iter().map(|id| node(id)).collect(),
            edges: edges
                .iter()
                .map(|(id, source, target)| Edge {
                    id: (*id).into(),
                    kind: "calls".into(),
                    source: (*source).into(),
                    target: Some((*target).into()),
                })
                .collect(),
            buses: Vec::new(),
        }
    }

    fn named(id: &str, points: &[(f64, f64)]) -> Route {
        let mut route = r(points);
        route.id = id.into();
        route
    }

    /// Track order avoids crossings: two parallel verticals in one corridor, the second nested inside the first, both
    /// turning to the left at their ends. The nested one goes on the lower side; in route order it would put the outer
    /// one there, and the nested one's two turns would cross it.
    #[test]
    fn nested_tracks_are_ordered_so_that_their_turns_do_not_cross() {
        let graph = two_edge_graph(&["a", "b"], &[("outer", "a", "b"), ("inner", "a", "b")]);
        let routes = [
            named("outer", &[(0., 10.), (100., 10.), (100., 90.), (0., 90.)]),
            named("inner", &[(0., 30.), (100., 30.), (100., 70.), (0., 70.)]),
        ];
        let params = Params {
            spread: true,
            ..Params::default()
        };
        let spread = spread_tracks(&graph, &BTreeMap::new(), &routes, params);
        let x = |id: &str| spread.iter().find(|r| r.id == id).unwrap().points[1].x;
        assert!(
            x("inner") < x("outer"),
            "{} against {}",
            x("inner"),
            x("outer")
        );
        assert!(!crosses(&spread[0], &spread[1]), "{spread:?}");
    }

    /// Two facing stubs on one line: the gap between the nodes holds both only when each is short. No move of one route
    /// alone helps, so the pair moves together.
    #[test]
    fn facing_stubs_on_one_line_are_moved_apart_together() {
        let graph = two_edge_graph(&["a", "b", "c", "d"], &[("e1", "a", "c"), ("e2", "b", "d")]);
        let rect = |x: f64, y: f64| Rect {
            x,
            y,
            width: 40.,
            height: 40.,
        };
        let rects = BTreeMap::from([
            ("a".into(), rect(0., 0.)),
            ("b".into(), rect(0., 70.)),
            ("c".into(), rect(300., 0.)),
            ("d".into(), rect(300., 70.)),
        ]);
        let routes = [
            named(
                "e1",
                &[
                    (20., 40.),
                    (20., 58.),
                    (100., 58.),
                    (100., 20.),
                    (300., 20.),
                ],
            ),
            named(
                "e2",
                &[
                    (20., 70.),
                    (20., 52.),
                    (120., 52.),
                    (120., 90.),
                    (300., 90.),
                ],
            ),
        ];
        assert!(shares_segment(&routes[0], &routes[1]), "the stubs overlap");
        let params = Params {
            clearance: 12.,
            own_clearance: 12.,
            rail_offset: 6.,
            ..Params::default()
        };
        let resolved = resolve_overlaps(&graph, &rects, &routes, params);
        assert!(!shares_segment(&resolved[0], &resolved[1]), "{resolved:?}");
        for route in &resolved {
            let stub = (route.points[1].y - route.points[0].y).abs();
            assert!(stub >= 12. - 1e-6, "{route:?} has a stub of {stub}");
        }
    }

    /// Clearance: a route that grazes a foreign node must leave the margin around it.
    #[test]
    fn clearance_keeps_routes_away_from_foreign_nodes() {
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: ["a", "b", "block"]
                .iter()
                .map(|id| Node {
                    id: (*id).into(),
                    label: (*id).into(),
                    kind: "symbol".into(),
                    parent_id: None,
                    shape: Default::default(),
                })
                .collect(),
            edges: vec![Edge {
                id: "e".into(),
                kind: "calls".into(),
                source: "a".into(),
                target: Some("b".into()),
            }],
            buses: Vec::new(),
        };
        let rect = |x: f64, y: f64, width: f64, height: f64| Rect {
            x,
            y,
            width,
            height,
        };
        let rects = BTreeMap::from([
            ("a".into(), rect(0., 0., 10., 10.)),
            ("b".into(), rect(90., 0., 10., 10.)),
            // Block leaves a 2-unit gap above the straight route at y = 5.
            ("block".into(), rect(40., -3., 20., 10.)),
        ]);
        let straight = r(&[(10., 5.), (90., 5.)]);
        let params = Params {
            clearance: 5.,
            ..Params::default()
        };
        let improved = improve_with(&graph, &rects, &[straight], params);
        let margin = rect(40. - 5., -3. - 5., 20. + 10., 10. + 10.);
        assert!(
            !hits(&improved[0], &margin),
            "route still within the clearance of the block"
        );
        assert_eq!(improved[0].points.first().unwrap().x, 10.);
        assert_eq!(improved[0].points.last().unwrap().x, 90.);
    }
    /// Track separation never increases the number of close parallel segments of different routes.
    #[test]
    fn track_separation_does_not_increase_close_parallel_segments() {
        let mut routes = Vec::new();
        let mut edges = Vec::new();
        let mut rects = BTreeMap::new();
        for i in 0..4 {
            let y = 40. + i as f64 * 2.;
            let mut route = r(&[
                (0., y),
                (20., y),
                (20., y + 60.),
                (80., y + 60.),
                (100., y + 60.),
                (120., y + 60.),
            ]);
            route.id = format!("e{i}");
            routes.push(route);
            edges.push(Edge {
                id: format!("e{i}"),
                kind: "calls".into(),
                source: format!("a{i}"),
                target: Some(format!("b{i}")),
            });
            rects.insert(
                format!("a{i}"),
                Rect {
                    x: -20.,
                    y: y - 5.,
                    width: 20.,
                    height: 10.,
                },
            );
            rects.insert(
                format!("b{i}"),
                Rect {
                    x: 120.,
                    y: y + 55.,
                    width: 20.,
                    height: 10.,
                },
            );
        }
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            nodes: vec![],
            edges,
            ports: vec![],
            edge_ports: vec![],
            buses: Vec::new(),
        };
        let close = |list: &[Route]| {
            let mut n = 0;
            for (i, a) in list.iter().enumerate() {
                for b in &list[i + 1..] {
                    for x in a.points.windows(2).filter(|w| horizontal(&w[0], &w[1])) {
                        for y in b.points.windows(2).filter(|w| horizontal(&w[0], &w[1])) {
                            let d = (x[0].y - y[0].y).abs();
                            if d > EPS && d < 8. && x[0].x.max(x[1].x) > y[0].x.min(y[1].x) {
                                n += 1;
                            }
                        }
                    }
                }
            }
            n
        };
        let base = improve_with(
            &graph,
            &rects,
            &routes,
            Params {
                track: 10.,
                ..Params::default()
            },
        );
        let spread = improve_with(
            &graph,
            &rects,
            &routes,
            Params {
                track: 10.,
                spread: true,
                ..Params::default()
            },
        );
        assert!(close(&spread) <= close(&base));
        for (route, before) in spread.iter().zip(&routes) {
            assert_eq!(
                route.points.first().unwrap().y,
                before.points.first().unwrap().y
            );
        }
    }
    #[test]
    fn detours_around_middle_obstacle() {
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: ["a", "b", "block"]
                .iter()
                .map(|id| Node {
                    id: (*id).into(),
                    label: (*id).into(),
                    kind: "symbol".into(),
                    parent_id: None,
                    shape: Default::default(),
                })
                .collect(),
            edges: vec![Edge {
                id: "e".into(),
                kind: "calls".into(),
                source: "a".into(),
                target: Some("b".into()),
            }],
            buses: Vec::new(),
        };
        let rects = BTreeMap::from([
            (
                "a".into(),
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 10.,
                    height: 10.,
                },
            ),
            (
                "b".into(),
                Rect {
                    x: 90.,
                    y: 0.,
                    width: 10.,
                    height: 10.,
                },
            ),
            (
                "block".into(),
                Rect {
                    x: 40.,
                    y: 0.,
                    width: 20.,
                    height: 10.,
                },
            ),
        ]);
        let original = r(&[(10., 5.), (20., 5.), (80., 5.), (90., 5.)]);
        assert!(hits(&original, &rects["block"]));
        let improved = improve(&graph, &rects, &[original.clone()]);
        assert!(!hits(&improved[0], &rects["block"]));
        assert_eq!(improved[0].points.first().unwrap().x, 10.);
        assert_eq!(improved[0].points.last().unwrap().x, 90.);
    }
}

/// Re-searches only the routes named in `changed`. Each is scored against the obstacles and against all
/// other current routes, which stay fixed. Used by the placement search for local moves.
pub fn improve_subset(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    changed: &BTreeSet<String>,
    params: Params,
) -> Vec<Route> {
    improve_subset_counted(
        graph,
        rects,
        routes,
        changed,
        params,
        &mut channel::SearchCounters::default(),
    )
}

/// `improve_subset` that also adds the candidate evaluations it performs to `counters`.
pub fn improve_subset_counted(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &[Route],
    changed: &BTreeSet<String>,
    params: Params,
    counters: &mut channel::SearchCounters,
) -> Vec<Route> {
    let edges: BTreeMap<_, _> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let parents = parent_map(graph);
    let mut result = routes.to_vec();
    for index in 0..result.len() {
        let original = result[index].clone();
        let Some(edge) = edges.get(original.id.as_str()) else {
            continue;
        };
        if !changed.contains(&original.id) || original.points.len() < 2 {
            continue;
        }
        let occupied: Vec<Probe> = result
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, r)| probe(r))
            .collect();
        let best = best_route(
            &original, edge, rects, &parents, &occupied, params, counters,
        );
        result[index] = best;
    }
    result
}

#[cfg(test)]
mod channel_tests {
    use super::channel::{route_channels, ChannelEnd};
    use super::*;
    #[test]
    fn channel_route_avoids_the_margin_and_parallel_routes_take_distinct_tracks() {
        let rect = |x: f64, y: f64, w: f64, h: f64| Rect {
            x,
            y,
            width: w,
            height: h,
        };
        let rects = BTreeMap::from([
            ("a".to_string(), rect(0., 0., 20., 20.)),
            ("b".to_string(), rect(200., 0., 20., 20.)),
            // Block sits on the straight line between a and b.
            ("block".to_string(), rect(90., -10., 40., 30.)),
        ]);
        let nodes = ["a", "b", "block"]
            .iter()
            .map(|id| Node {
                id: (*id).into(),
                kind: "symbol".into(),
                label: (*id).into(),
                parent_id: None,
                shape: Default::default(),
            })
            .collect();
        let edges: Vec<Edge> = (0..2)
            .map(|i| Edge {
                id: format!("e{i}"),
                kind: "calls".into(),
                source: "a".into(),
                target: Some("b".into()),
            })
            .collect();
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes,
            edges,
            buses: Vec::new(),
        };
        let ends: Vec<ChannelEnd> = (0..2)
            .map(|i| ChannelEnd {
                edge_id: format!("e{i}"),
                source: "a".into(),
                target: "b".into(),
                start: Point { x: 20., y: 10. },
                depart: Point { x: 34., y: 10. },
                arrive: Point { x: 186., y: 10. },
                end: Point { x: 200., y: 10. },
            })
            .collect();
        let routes = route_channels(&graph, &rects, &ends, 10., 8.);
        for route in &routes {
            assert!(
                !hits(
                    route,
                    &Rect {
                        x: 80.,
                        y: -20.,
                        width: 60.,
                        height: 50.
                    }
                ),
                "route enters the margin of the block"
            );
        }
        // Routes between the same ports must not share a segment: the congestion penalty separates them.
        // The stubs are identical by construction; compare only the routed middle of each path.
        let middle = |r: &Route| Route::new(r.id.clone(), r.points[2..r.points.len() - 2].to_vec());
        assert!(!shares_segment(&middle(&routes[0]), &middle(&routes[1])));
    }
}

#[cfg(test)]
mod simplify_tests {
    use super::*;
    #[test]
    fn spike_onto_the_previous_segment_is_removed() {
        let point = |x: f64, y: f64| Point { x, y };
        // The route leaves its port, doubles back for a stub, then continues.
        let spiked = Route::new(
            "e".to_string(),
            vec![
                point(0., 0.),
                point(6., 0.),
                point(0., 0.),
                point(0., 10.),
                point(20., 10.),
            ],
        );
        let cleaned = simplify(vec![spiked]);
        let coords: Vec<(f64, f64)> = cleaned[0].points.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(coords, vec![(0., 0.), (0., 10.), (20., 10.)]);
    }
    #[test]
    fn a_real_turn_is_kept() {
        let point = |x: f64, y: f64| Point { x, y };
        let turn = Route::new(
            "e".to_string(),
            vec![point(0., 0.), point(10., 0.), point(10., 10.)],
        );
        assert_eq!(simplify(vec![turn])[0].points.len(), 3);
    }
}

#[cfg(test)]
mod diagonal_crossing_tests {
    use super::*;

    fn route(points: &[(f64, f64)]) -> Route {
        Route {
            id: "r".into(),
            points: points.iter().map(|&(x, y)| Point { x, y }).collect(),
            weight: 1,
            bundle_of: None,
        }
    }

    /// A diagonal straight segment crosses an orthogonal route in their interiors; touching at an endpoint does not count.
    #[test]
    fn a_diagonal_segment_crosses_an_orthogonal_route() {
        let across = route(&[(0., 50.), (100., 50.)]);
        let diagonal = route(&[(20., 0.), (80., 100.)]);
        assert!(crosses(&across, &diagonal));
        let touching = route(&[(0., 50.), (50., 50.)]);
        assert!(!crosses(&touching, &diagonal));
    }
}

#[cfg(test)]
mod contour_acceptance {
    use super::*;

    fn node(id: &str, shape: NodeShape) -> Node {
        Node {
            id: id.into(),
            label: id.into(),
            kind: "node".into(),
            parent_id: None,
            shape,
        }
    }

    /// Edge a→b across the box of x. The circle variant is a disc inscribed in that box.
    fn scene(shape: NodeShape) -> (Graph, BTreeMap<String, Rect>) {
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: BTreeMap::new(),
            default_routing: None,
            container_routing: BTreeMap::new(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![
                node("a", NodeShape::Rect),
                node("b", NodeShape::Rect),
                node("x", shape),
            ],
            edges: vec![Edge {
                id: "ab".into(),
                kind: "flow".into(),
                source: "a".into(),
                target: Some("b".into()),
            }],
            buses: Vec::new(),
        };
        let rects = BTreeMap::from([
            (
                "a".to_string(),
                Rect {
                    x: 0.,
                    y: -5.,
                    width: 20.,
                    height: 20.,
                },
            ),
            (
                "b".to_string(),
                Rect {
                    x: 300.,
                    y: -5.,
                    width: 20.,
                    height: 20.,
                },
            ),
            (
                "x".to_string(),
                Rect {
                    x: 100.,
                    y: 0.,
                    width: 100.,
                    height: 100.,
                },
            ),
        ]);
        (graph, rects)
    }

    /// The route from a's side, into the free top-left corner of x's box, round the top of the box and into b.
    fn corner_route() -> Route {
        Route::new(
            "ab",
            [
                (20., 5.),
                (108., 5.),
                (108., -12.),
                (294., -12.),
                (294., 5.),
                (300., 5.),
            ]
            .iter()
            .map(|&(x, y)| Point { x, y })
            .collect(),
        )
    }

    fn improve(shape: NodeShape, initial: &Route, clearance: f64) -> (Route, Outline) {
        let (graph, rects) = scene(shape);
        let params = Params {
            clearance,
            ..Params::default()
        };
        let out = improve_until_counted_with_fixed(
            &graph,
            &rects,
            std::slice::from_ref(initial),
            &[],
            params,
            &|| false,
            &mut channel::SearchCounters::default(),
        )
        .unwrap();
        let info = parent_map(&graph);
        let outline = obstacle(
            "x",
            &rects["x"],
            shape_in(&info, "x"),
            &graph.edges[0],
            Params::default(),
        );
        (out.into_iter().next().unwrap(), outline)
    }

    /// Free corner: a route through the empty corner of a circle's box is valid for the circle; the same route is not valid
    /// for a rectangle, which blocks it, and the search returns a route that does not enter the rectangle.
    #[test]
    fn a_free_corner_of_a_circle_box_is_passable_and_a_rectangle_blocks_it() {
        let (circle_route, circle) = improve(NodeShape::Circle, &corner_route(), 0.);
        assert!(
            !hits_outline(&circle_route, &circle),
            "circle: {:?}",
            circle_route.points
        );
        let (rect_route, rect) = improve(NodeShape::Rect, &corner_route(), 0.);
        assert!(
            !hits_outline(&rect_route, &rect),
            "rectangle: {:?}",
            rect_route.points
        );
        assert!(hits_outline(
            &corner_route(),
            &Outline::Rect(Rect {
                x: 100.,
                y: 0.,
                width: 100.,
                height: 100.
            })
        ));
    }

    /// Required clearance: a route round a circle keeps the clearance along its whole length.
    #[test]
    fn a_route_round_a_circle_keeps_the_required_clearance() {
        let (route, outline) = improve(NodeShape::Circle, &corner_route(), 9.);
        let inflated = match outline {
            Outline::Disc { x, y, radius } => Outline::Disc {
                x,
                y,
                radius: radius + 9.,
            },
            other => other,
        };
        assert!(!hits_outline(&route, &inflated), "{:?}", route.points);
    }

    fn boxed(id: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            label: id.into(),
            kind: "node".into(),
            parent_id: parent.map(str::to_string),
            shape: NodeShape::Rect,
        }
    }

    /// Nested containers, absolute boxes: `g` holds `c` (the source); `h` is a sibling of `g` below the route to `d`, which is at the root.
    fn nested() -> (Graph, BTreeMap<String, Rect>) {
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: BTreeMap::new(),
            default_routing: None,
            container_routing: BTreeMap::new(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![
                boxed("g", None),
                boxed("c", Some("g")),
                boxed("h", None),
                boxed("d", None),
            ],
            edges: vec![Edge {
                id: "cd".into(),
                kind: "flow".into(),
                source: "c".into(),
                target: Some("d".into()),
            }],
            buses: Vec::new(),
        };
        let rects = BTreeMap::from([
            (
                "g".to_string(),
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 200.,
                    height: 300.,
                },
            ),
            (
                "c".to_string(),
                Rect {
                    x: 40.,
                    y: 60.,
                    width: 50.,
                    height: 50.,
                },
            ),
            (
                "h".to_string(),
                Rect {
                    x: 250.,
                    y: 200.,
                    width: 100.,
                    height: 100.,
                },
            ),
            (
                "d".to_string(),
                Rect {
                    x: 420.,
                    y: 100.,
                    width: 50.,
                    height: 50.,
                },
            ),
        ]);
        (graph, rects)
    }

    fn route_points(id: &str, points: &[(f64, f64)]) -> Route {
        Route::new(id, points.iter().map(|&(x, y)| Point { x, y }).collect())
    }

    /// The own container of an edge's endpoint is crossed: a route that leaves `g` through its border, and nothing else, is
    /// not repaired.
    #[test]
    fn a_route_may_leave_its_own_container() {
        let (graph, rects) = nested();
        let leaving = route_points("cd", &[(90., 85.), (230., 85.), (230., 125.), (420., 125.)]);
        let repaired = repair_with_counted(
            &graph,
            &rects,
            std::slice::from_ref(&leaving),
            Params::default(),
            &mut channel::SearchCounters::default(),
        );
        assert_eq!(repaired[0].points, leaving.points);
    }

    /// A sibling container of an endpoint's ancestor is protected: a route through `h` is repaired around it.
    #[test]
    fn a_route_may_not_cross_a_sibling_container() {
        let (graph, rects) = nested();
        let through = route_points(
            "cd",
            &[
                (90., 85.),
                (230., 85.),
                (230., 250.),
                (330., 250.),
                (330., 125.),
                (420., 125.),
            ],
        );
        let repaired = repair_with_counted(
            &graph,
            &rects,
            std::slice::from_ref(&through),
            Params::default(),
            &mut channel::SearchCounters::default(),
        );
        let h = Outline::Rect(rects["h"].clone());
        assert!(!hits_outline(&repaired[0], &h), "{:?}", repaired[0].points);
    }

    /// A circle that grows over a route invalidates it; the search after the change routes around the larger disc.
    #[test]
    fn a_grown_circle_is_routed_around_after_the_change() {
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: BTreeMap::new(),
            default_routing: None,
            container_routing: BTreeMap::new(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![
                Node {
                    id: "a".into(),
                    label: "a".into(),
                    kind: "node".into(),
                    parent_id: None,
                    shape: NodeShape::Rect,
                },
                Node {
                    id: "b".into(),
                    label: "b".into(),
                    kind: "node".into(),
                    parent_id: None,
                    shape: NodeShape::Rect,
                },
                Node {
                    id: "x".into(),
                    label: "x".into(),
                    kind: "node".into(),
                    parent_id: None,
                    shape: NodeShape::Circle,
                },
            ],
            edges: vec![Edge {
                id: "ab".into(),
                kind: "flow".into(),
                source: "a".into(),
                target: Some("b".into()),
            }],
            buses: Vec::new(),
        };
        let mut rects = BTreeMap::from([
            (
                "a".to_string(),
                Rect {
                    x: 0.,
                    y: -5.,
                    width: 20.,
                    height: 20.,
                },
            ),
            (
                "b".to_string(),
                Rect {
                    x: 300.,
                    y: -5.,
                    width: 20.,
                    height: 20.,
                },
            ),
            (
                "x".to_string(),
                Rect {
                    x: 100.,
                    y: 0.,
                    width: 100.,
                    height: 100.,
                },
            ),
        ]);
        let before = improve_until_counted(
            &graph,
            &rects,
            &[corner_route()],
            Params::default(),
            &|| false,
            &mut channel::SearchCounters::default(),
        )
        .unwrap();
        // The circle grows by 30 on every side: the old route now runs through its new disc.
        rects.insert(
            "x".into(),
            Rect {
                x: 70.,
                y: -30.,
                width: 160.,
                height: 160.,
            },
        );
        let grown = Outline::of(NodeShape::Circle, &rects["x"]);
        assert!(
            hits_outline(&before[0], &grown),
            "the change must matter for this test"
        );
        let after = improve_until_counted(
            &graph,
            &rects,
            &before,
            Params::default(),
            &|| false,
            &mut channel::SearchCounters::default(),
        )
        .unwrap();
        assert!(!hits_outline(&after[0], &grown), "{:?}", after[0].points);
    }
}

#[cfg(test)]
mod search_corner_tests {
    use super::*;

    fn scene(shape: NodeShape) -> (Graph, BTreeMap<String, Rect>) {
        let node = |id: &str, shape: NodeShape| Node {
            id: id.into(),
            label: id.into(),
            kind: "node".into(),
            parent_id: None,
            shape,
        };
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: BTreeMap::new(),
            default_routing: None,
            container_routing: BTreeMap::new(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![
                node("a", NodeShape::Rect),
                node("b", NodeShape::Rect),
                node("x", shape),
            ],
            edges: vec![Edge {
                id: "ab".into(),
                kind: "flow".into(),
                source: "a".into(),
                target: Some("b".into()),
            }],
            buses: Vec::new(),
        };
        let rects = BTreeMap::from([
            (
                "a".to_string(),
                Rect {
                    x: 0.,
                    y: -5.,
                    width: 20.,
                    height: 20.,
                },
            ),
            (
                "b".to_string(),
                Rect {
                    x: 300.,
                    y: -5.,
                    width: 20.,
                    height: 20.,
                },
            ),
            (
                "x".to_string(),
                Rect {
                    x: 100.,
                    y: 0.,
                    width: 100.,
                    height: 100.,
                },
            ),
        ]);
        (graph, rects)
    }

    /// From a naive start straight through the middle of the box, the search finds a route along the top of the box, where
    /// the disc only touches: it does not enter the disc, and it is shorter than the route round the box that a rectangle
    /// gets in the same scene. The search proposes the box edge as a track for discs only, so the comparison is between the
    /// candidate sets the shapes get, not only between the shapes themselves.
    #[test]
    fn the_search_finds_the_free_top_of_a_circle_box_by_itself() {
        let naive = Route::new(
            "ab",
            vec![Point { x: 20., y: 5. }, Point { x: 300., y: 5. }],
        );
        let run = |shape: NodeShape| {
            let (graph, rects) = scene(shape);
            let out = improve_until_counted(
                &graph,
                &rects,
                std::slice::from_ref(&naive),
                Params::default(),
                &|| false,
                &mut channel::SearchCounters::default(),
            )
            .unwrap();
            let disc = Outline::of(NodeShape::Circle, &rects["x"]);
            (out.into_iter().next().unwrap(), disc)
        };
        let (circle, disc) = run(NodeShape::Circle);
        assert!(!hits_outline(&circle, &disc), "{:?}", circle.points);
        assert!(
            circle.points.iter().any(|p| p.y == 0.),
            "the route uses the box top: {:?}",
            circle.points
        );
        let (rectangle, _) = run(NodeShape::Rect);
        assert!(
            length(&circle) < length(&rectangle),
            "circle {} rectangle {}",
            length(&circle),
            length(&rectangle)
        );
    }
}
