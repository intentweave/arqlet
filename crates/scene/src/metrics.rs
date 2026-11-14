//! One metric set for every engine, version 3. A drawing is absolute node rectangles plus absolute edge
//! polylines. Every metric is computed on every call, so no value is ever "not computed". Invalid drawings
//! (unknown or cyclic parents, unknown endpoints, edges without a route, non-finite geometry) are errors.
//! The optimizer's objective list in the layout crate decides what is improved; this module only measures.
use crate::geom::{
    clip_length, collinear_overlap, dedupe, dist, enters, offset, proper_crossing, touches, Rect,
    EPS,
};
use arqlet_model::Point;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const METRICS_VERSION: u32 = 3;
/// Minimum clearance to a foreign node or container border. Closer segments are "border hugging".
pub const HUG_MARGIN: f64 = 12.;
/// Default distance between tracks; closer parallel routes are a near-parallel conflict.
pub const DEFAULT_TRACK_SPACING: f64 = 6.;

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawNode {
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub container: bool,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// The node's shape: a circle is the disc inscribed in its box, so a line through an empty corner of the box, or one
    /// that ends on the circle, does not enter it. A rectangle when omitted (provisional).
    #[serde(default)]
    pub shape: arqlet_model::NodeShape,
}

/// Whether the segment `a`–`b` enters the node's outline: its box, or the disc of a circle (deeper than a rounding error, so
/// a segment that starts or ends on the circle and leaves it does not count).
fn enters_node(a: &Point, b: &Point, node: &DrawNode, r: &Rect) -> bool {
    if node.shape != arqlet_model::NodeShape::Circle {
        return enters(a, b, r);
    }
    let (cx, cy) = ((r.x0 + r.x1) / 2., (r.y0 + r.y1) / 2.);
    let radius = (r.x1 - r.x0).min(r.y1 - r.y0) / 2.;
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = dx * dx + dy * dy;
    let t = if length > 0. {
        (((cx - a.x) * dx + (cy - a.y) * dy) / length).clamp(0., 1.)
    } else {
        0.
    };
    let (px, py) = (a.x + dx * t - cx, a.y + dy * t - cy);
    let inner = (radius - 1e-3).max(0.);
    px * px + py * py < inner * inner
}

/// An edge polyline. Two routes may share a line only where both name the same explicit port at that end
/// (`sourcePort` / `targetPort`) or the same bundle (`bundleId`), and only along the first or last segment.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    pub points: Vec<Point>,
    #[serde(default)]
    pub source_port: Option<String>,
    #[serde(default)]
    pub target_port: Option<String>,
    #[serde(default)]
    pub bundle_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Drawing {
    pub nodes: Vec<DrawNode>,
    pub edges: Vec<DrawEdge>,
    #[serde(default = "default_track_spacing")]
    pub track_spacing: f64,
    /// Explicit buses (ADR 0005), each as its edges' ids: two edges of one bus may share segments and meet, so their shared
    /// lines, near-parallel runs and crossings are not counted. Empty when the drawing has none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buses: Vec<Vec<String>>,
}

fn default_track_spacing() -> f64 {
    DEFAULT_TRACK_SPACING
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub version: u32,
    pub nodes: usize,
    pub edges: usize,
    pub width: f64,
    pub height: f64,
    pub area: f64,
    /// Overlapping node pairs under the same parent.
    pub sibling_overlaps: usize,
    /// Overlapping node pairs under different parents (independent container branches).
    pub cross_branch_overlaps: usize,
    /// (edge, leaf node) pairs where a segment passes through the node interior, including the edge's own endpoint
    /// nodes. Containers are not leaves: routes may cross container borders by design.
    pub line_node_hits: usize,
    /// Edge pairs with an unintended collinear overlap. Overlaps are merged per line before they are measured.
    pub shared_line_pairs: usize,
    pub shared_line_length: f64,
    /// Edge pairs with parallel segments closer than the track spacing, not on one line.
    pub near_parallel_pairs: usize,
    pub near_parallel_length: f64,
    /// (edge, foreign leaf node) pairs with a segment within `HUG_MARGIN` of the border, without entering.
    pub hug_pairs: usize,
    /// Length of those segments inside the margin band, summed over pairs.
    pub hug_length: f64,
    /// (edge, container) pairs where a segment passes through the interior of a container the edge does not belong to.
    /// A foreign container is not a passage corridor (algorithm document, rule 6), so each such pair is a violation.
    pub container_traversals: usize,
    /// (edge, container) pairs where a segment runs within `HUG_MARGIN` outside a container border that the edge
    /// does not belong to. Reported apart from node distance.
    pub container_hug_pairs: usize,
    pub container_hug_length: f64,
    /// Proper crossing points between edges without a common endpoint node.
    pub crossings: usize,
    /// Edge pairs with at least one such crossing point (the engine's routed-crossings unit).
    pub crossing_pairs: usize,
    /// Crossing points between edges that share an endpoint node (reported separately).
    pub shared_endpoint_crossings: usize,
    /// Edge pairs with at least one shared-endpoint crossing point.
    pub shared_endpoint_crossing_pairs: usize,
    /// A segment endpoint lying on another edge's segment without crossing it.
    pub touches: usize,
    /// Direction changes after removing repeated and collinear points.
    pub bends: usize,
    pub bends_max: usize,
    pub bends_mean: f64,
    pub length: f64,
    pub length_mean: f64,
    pub length_max: f64,
}

/// Measures a drawing. Pure and deterministic; an invalid drawing is an error, never a score.
pub fn measure(drawing: &Drawing) -> Result<Metrics, String> {
    measure_with_clearance(drawing, HUG_MARGIN)
}

/// Measures a drawing with a node clearance: near-node proximity counts the segments within `clearance` of a foreign
/// leaf node. `measure` uses `HUG_MARGIN`. Container proximity always uses `HUG_MARGIN`.
/// The metrics with a required clearance: the routes keep it from every node, their own nodes included, beyond the stub
/// that leaves their port (ADR 0011).
pub fn measure_with_requirement(drawing: &Drawing, clearance: f64) -> Result<Metrics, String> {
    measure_inner(drawing, clearance, true)
}

pub fn measure_with_clearance(drawing: &Drawing, clearance: f64) -> Result<Metrics, String> {
    measure_inner(drawing, clearance, false)
}

fn measure_inner(drawing: &Drawing, clearance: f64, own_nodes: bool) -> Result<Metrics, String> {
    Ok(measure_variants(drawing, &[(clearance, own_nodes)])?.remove(0))
}

/// The metrics of one drawing under several node clearances at once, each `(clearance, own nodes)` as in
/// `measure_with_clearance` (false) and `measure_with_requirement` (true): only the routes' proximity to nodes depends on
/// them, so everything else (node overlaps, the pairwise statistics of the routes, bends and lengths) is measured once.
/// The same values as measuring each variant on its own.
pub fn measure_variants(
    drawing: &Drawing,
    variants: &[(f64, bool)],
) -> Result<Vec<Metrics>, String> {
    Ok(measure_variants_inner::<true, false, false>(drawing, variants)?.0)
}

/// Provisional source-level loop work, not CPU instructions or touch-helper work.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PairWork {
    pub bounds_tests: usize,
    pub candidate_pairs: usize,
    /// Loop slots in overlap/parallel/crossing passes, excluding touch checks.
    pub segment_pair_visits: usize,
    pub segment_bounds_computed: usize,
}

/// Provisional benchmark controls; not layout or metric semantics.
#[cfg(feature = "benchmark-pairs")]
pub enum PairMode {
    Legacy,
    Shared,
    Indexed,
}

/// Benchmark-only comparison with the previous exhaustive pair iteration.
/// Both paths include all drawing preparation and complete metric variants.
#[cfg(feature = "benchmark-pairs")]
pub fn measure_variants_with_pair_work(
    drawing: &Drawing,
    variants: &[(f64, bool)],
    mode: PairMode,
) -> Result<(Vec<Metrics>, PairWork), String> {
    match mode {
        PairMode::Legacy => measure_variants_inner::<true, true, true>(drawing, variants),
        PairMode::Shared => measure_variants_inner::<true, true, false>(drawing, variants),
        PairMode::Indexed => measure_variants_inner::<false, true, false>(drawing, variants),
    }
}

/// Benchmark-only uninstrumented reference, preserving the former pair loop.
#[cfg(feature = "benchmark-pairs")]
pub fn measure_variants_exhaustive(
    drawing: &Drawing,
    variants: &[(f64, bool)],
) -> Result<Vec<Metrics>, String> {
    Ok(measure_variants_inner::<true, false, true>(drawing, variants)?.0)
}

/// Benchmark-only spatial route-pair generation with the optimized pair evaluator.
#[cfg(feature = "benchmark-pairs")]
pub fn measure_variants_indexed(
    drawing: &Drawing,
    variants: &[(f64, bool)],
) -> Result<Vec<Metrics>, String> {
    Ok(measure_variants_inner::<false, false, false>(drawing, variants)?.0)
}

fn measure_variants_inner<const EXHAUSTIVE: bool, const COUNT: bool, const LEGACY: bool>(
    drawing: &Drawing,
    variants: &[(f64, bool)],
) -> Result<(Vec<Metrics>, PairWork), String> {
    let parents = validate(drawing)?;
    let rects: Vec<(&DrawNode, Rect)> = drawing.nodes.iter().map(|n| (n, rect_of(n))).collect();
    let leaves: Vec<&(&DrawNode, Rect)> = rects.iter().filter(|(n, _)| !n.container).collect();
    let containers: Vec<&(&DrawNode, Rect)> = rects.iter().filter(|(n, _)| n.container).collect();
    let routes: Vec<Route> = drawing
        .edges
        .iter()
        .map(Route::new_inner::<LEGACY>)
        .collect();

    let mut m = Metrics {
        version: METRICS_VERSION,
        nodes: drawing.nodes.len(),
        edges: drawing.edges.len(),
        ..Metrics::default()
    };
    if !rects.is_empty() {
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for (_, r) in &rects {
            min_x = min_x.min(r.x0);
            min_y = min_y.min(r.y0);
            max_x = max_x.max(r.x1);
            max_y = max_y.max(r.y1);
        }
        m.width = max_x - min_x;
        m.height = max_y - min_y;
        m.area = m.width * m.height;
    }

    // Node overlaps: nested pairs (a container and its descendants) are not overlaps.
    for (i, (a, ra)) in rects.iter().enumerate() {
        for (b, rb) in rects.iter().skip(i + 1) {
            if ra.x0 < rb.x1 - EPS
                && rb.x0 < ra.x1 - EPS
                && ra.y0 < rb.y1 - EPS
                && rb.y0 < ra.y1 - EPS
                && !is_nested(&parents, &a.id, &b.id)
            {
                if a.parent_id == b.parent_id {
                    m.sibling_overlaps += 1;
                } else {
                    m.cross_branch_overlaps += 1;
                }
            }
        }
    }

    // Pairwise edge metrics.
    let mut shared_pairs = BTreeSet::new();
    let mut near_pairs = BTreeSet::new();
    let mut crossing_pairs = BTreeSet::new();
    let mut shared_endpoint_crossing_pairs = BTreeSet::new();
    let mut work = PairWork::default();
    if COUNT && !LEGACY {
        work.segment_bounds_computed = routes.iter().map(|r| r.segs.len()).sum();
    }
    let index = (!EXHAUSTIVE).then(|| PairIndex::new(&routes));
    let mut candidates = Vec::new();
    let mut accumulate = |p: &Route, q: &Route, work: &mut PairWork| {
        if COUNT {
            work.candidate_pairs += 1;
            let slots = p.segs.len() * q.segs.len();
            work.segment_pair_visits += slots * if LEGACY { 2 } else { 1 };
            if LEGACY {
                work.segment_bounds_computed += slots * 4;
            }
        }
        let stats = if LEGACY {
            pair_stats_legacy(p, q, drawing.track_spacing)
        } else {
            pair_stats(p, q, drawing.track_spacing)
        };
        let pair = (p.edge.id.clone(), q.edge.id.clone());
        // Within one declared bus a shared trunk, and branches meeting it, are the drawing asked for (ADR 0005): two members
        // of one bus count no shared line, near-parallel run or crossing.
        if same_bus(drawing, &p.edge.id, &q.edge.id) {
            return;
        }
        if stats.shared > EPS {
            shared_pairs.insert(pair.clone());
            m.shared_line_length += stats.shared;
        }
        if stats.near > EPS {
            near_pairs.insert(pair.clone());
            m.near_parallel_length += stats.near;
        }
        if stats.crossings > 0 {
            m.crossings += stats.crossings;
            crossing_pairs.insert(pair.clone());
        }
        if stats.shared_endpoint_crossings > 0 {
            m.shared_endpoint_crossings += stats.shared_endpoint_crossings;
            shared_endpoint_crossing_pairs.insert(pair);
        }
        m.touches += stats.touches;
    };
    for (i, p) in routes.iter().enumerate() {
        if let Some(index) = &index {
            candidates.clear();
            index.query::<COUNT>(
                i,
                p.bounds,
                reach(drawing.track_spacing),
                &mut candidates,
                &mut work,
            );
            // Preserve the old (i, j) accumulation order, including floating sums.
            candidates.sort_unstable();
            for &j in &candidates {
                accumulate(p, &routes[j], &mut work);
            }
        } else {
            for q in routes.iter().skip(i + 1) {
                if COUNT {
                    work.bounds_tests += 1;
                }
                if near_boxes(p.bounds, q.bounds, reach(drawing.track_spacing)) {
                    accumulate(p, q, &mut work);
                }
            }
        }
    }
    m.shared_line_pairs = shared_pairs.len();
    m.crossing_pairs = crossing_pairs.len();
    m.shared_endpoint_crossing_pairs = shared_endpoint_crossing_pairs.len();
    m.near_parallel_pairs = near_pairs.len();

    let bend_counts: Vec<f64> = routes.iter().map(|r| r.bends as f64).collect();
    let lengths: Vec<f64> = routes.iter().map(|r| r.length).collect();
    m.bends = routes.iter().map(|r| r.bends).sum();
    m.bends_max = routes.iter().map(|r| r.bends).max().unwrap_or(0);
    m.bends_mean = mean(&bend_counts);
    m.length = lengths.iter().sum();
    m.length_mean = mean(&lengths);
    m.length_max = lengths.iter().copied().fold(0., f64::max);
    // Per-edge metrics under each variant: node and container interaction.
    let measured = variants
        .iter()
        .map(|&(clearance, own_nodes)| {
            let mut m = m.clone();
            for route in &routes {
                let stats = node_stats(route, &leaves, &containers, &parents, clearance, own_nodes);
                m.line_node_hits += stats.hits.len();
                m.hug_pairs += stats.hug.len();
                m.hug_length += stats.hug.iter().map(|(_, length)| length).sum::<f64>();
                m.container_traversals += stats.traversals.len();
                m.container_hug_pairs += stats.container_hug.len();
                m.container_hug_length += stats
                    .container_hug
                    .iter()
                    .map(|(_, length)| length)
                    .sum::<f64>();
            }
            m
        })
        .collect();
    Ok((measured, work))
}

struct PairIndex {
    nodes: Vec<PairNode>,
    order: Vec<usize>,
    bounds: Vec<(f64, f64, f64, f64)>,
}

struct PairNode {
    bounds: (f64, f64, f64, f64),
    max_index: usize,
    range: std::ops::Range<usize>,
    children: Option<(usize, usize)>,
}

impl PairIndex {
    fn new(routes: &[Route<'_>]) -> Self {
        let mut index = Self {
            nodes: Vec::new(),
            order: (0..routes.len()).collect(),
            bounds: routes.iter().map(|r| r.bounds).collect(),
        };
        if !routes.is_empty() {
            index.build(routes, 0..routes.len());
        }
        index
    }

    fn build(&mut self, routes: &[Route<'_>], range: std::ops::Range<usize>) -> usize {
        let mut bounds = routes[self.order[range.start]].bounds;
        let mut max_index = 0;
        for &i in &self.order[range.clone()] {
            let b = routes[i].bounds;
            bounds = (
                bounds.0.min(b.0),
                bounds.1.min(b.1),
                bounds.2.max(b.2),
                bounds.3.max(b.3),
            );
            max_index = max_index.max(i);
        }
        let node = self.nodes.len();
        self.nodes.push(PairNode {
            bounds,
            max_index,
            range: range.clone(),
            children: None,
        });
        if range.len() > 8 {
            let x_axis = bounds.2 - bounds.0 >= bounds.3 - bounds.1;
            self.order[range.clone()].sort_unstable_by(|&a, &b| {
                let a_bound = routes[a].bounds;
                let b_bound = routes[b].bounds;
                let coord = |bounds: (f64, f64, f64, f64)| if x_axis { bounds.0 } else { bounds.1 };
                coord(a_bound).total_cmp(&coord(b_bound)).then(a.cmp(&b))
            });
            let middle = range.start + range.len() / 2;
            let left = self.build(routes, range.start..middle);
            let right = self.build(routes, middle..range.end);
            self.nodes[node].children = Some((left, right));
        }
        node
    }

    fn query<const COUNT: bool>(
        &self,
        after: usize,
        bounds: (f64, f64, f64, f64),
        margin: f64,
        out: &mut Vec<usize>,
        work: &mut PairWork,
    ) {
        if !self.nodes.is_empty() {
            self.query_node::<COUNT>(0, after, bounds, margin, out, work);
        }
    }

    fn query_node<const COUNT: bool>(
        &self,
        node: usize,
        after: usize,
        bounds: (f64, f64, f64, f64),
        margin: f64,
        out: &mut Vec<usize>,
        work: &mut PairWork,
    ) {
        let node = &self.nodes[node];
        if node.max_index <= after {
            return;
        }
        if COUNT {
            work.bounds_tests += 1;
        }
        if !near_boxes(bounds, node.bounds, margin) {
            return;
        }
        if let Some((left, right)) = node.children {
            self.query_node::<COUNT>(left, after, bounds, margin, out, work);
            self.query_node::<COUNT>(right, after, bounds, margin, out, work);
        } else {
            for &i in &self.order[node.range.clone()] {
                if i > after {
                    if COUNT {
                        work.bounds_tests += 1;
                    }
                    if near_boxes(bounds, self.bounds[i], margin) {
                        out.push(i);
                    }
                }
            }
        }
    }
}

/// Node and container interaction of one route. `hits` and `traversals` include the route's own endpoints where
/// they apply; hugging excludes the own endpoints, which start at their own border.
struct NodeStats<'a> {
    hits: Vec<&'a str>,
    traversals: Vec<&'a str>,
    hug: Vec<(&'a str, f64)>,
    container_hug: Vec<(&'a str, f64)>,
}

fn node_stats<'a>(
    route: &Route<'a>,
    leaves: &[&(&'a DrawNode, Rect)],
    containers: &[&(&'a DrawNode, Rect)],
    parents: &BTreeMap<&str, Option<&str>>,
    clearance: f64,
    own_nodes: bool,
) -> NodeStats<'a> {
    let mut stats = NodeStats {
        hits: Vec::new(),
        traversals: Vec::new(),
        hug: Vec::new(),
        container_hug: Vec::new(),
    };
    // A node the route stays clear of, by more than every margin below, adds nothing; the box test skips it.
    let apart = |r: &Rect, margin: f64| {
        !near_boxes(
            route.bounds,
            (r.x0, r.y0, r.x1, r.y1),
            margin.max(0.) + 1000. * EPS,
        )
    };
    for (node, r) in leaves {
        if apart(r, clearance) {
            continue;
        }
        if route.segs.iter().any(|(a, b)| enters_node(a, b, node, r)) {
            stats.hits.push(node.id.as_str());
            continue;
        }
        if node.id == route.edge.source || node.id == route.edge.target {
            // A required clearance applies to a route's own nodes too, beyond the stub that leaves its port (ADR 0011).
            if own_nodes {
                let hug: f64 = trim_stub(&route.segs, clearance)
                    .iter()
                    .map(|(a, b)| clip_length(a, b, &r.inflate(clearance)))
                    .sum();
                if hug > EPS {
                    stats.hug.push((node.id.as_str(), hug));
                }
            }
            continue;
        }
        let hug = route.clip_length_within(r, clearance);
        if hug > EPS {
            stats.hug.push((node.id.as_str(), hug));
        }
    }
    for (container, r) in containers {
        if apart(r, HUG_MARGIN) {
            continue;
        }
        let owns_endpoint = [route.edge.source.as_str(), route.edge.target.as_str()]
            .iter()
            .any(|id| *id == container.id || is_ancestor(parents, &container.id, id));
        if owns_endpoint {
            continue;
        }
        if route.segs.iter().any(|(a, b)| enters(a, b, r)) {
            stats.traversals.push(container.id.as_str());
            continue;
        }
        let hug = route.clip_length_within(r, HUG_MARGIN);
        if hug > EPS {
            stats.container_hug.push((container.id.as_str(), hug));
        }
    }
    stats
}

/// Shared statistics of two routes: unintended shared length, near-parallel length, crossing points (split by
/// whether the routes share an endpoint node), and touches.
#[derive(Debug, PartialEq)]
struct PairStats {
    shared: f64,
    near: f64,
    crossings: usize,
    shared_endpoint_crossings: usize,
    touches: usize,
}

/// Whether two boxes (min x, min y, max x, max y) come within `margin` of each other.
fn near_boxes(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64), margin: f64) -> bool {
    a.0 <= b.2 + margin && b.0 <= a.2 + margin && a.1 <= b.3 + margin && b.1 <= a.3 + margin
}

/// The farthest apart two routes (or segments) can lie and still share length, run near-parallel, cross or touch: the
/// near-parallel spacing, or the tolerance of the other tests.
fn reach(track_spacing: f64) -> f64 {
    track_spacing.max(0.) + 10. * EPS
}

/// `pair_stats`, or no statistics at all when the routes lie too far apart for any of them (the same result, faster).
fn pair_stats_near(p: &Route, q: &Route, track_spacing: f64) -> Option<PairStats> {
    near_boxes(p.bounds, q.bounds, reach(track_spacing)).then(|| pair_stats(p, q, track_spacing))
}

fn pair_stats(p: &Route, q: &Route, track_spacing: f64) -> PairStats {
    let common_node = [p.edge.source.as_str(), p.edge.target.as_str()]
        .iter()
        .any(|id| *id == q.edge.source || *id == q.edge.target);
    let mut lines: BTreeMap<LineId, Line> = BTreeMap::new();
    let mut near = 0.0;
    let (mut crossings, mut shared_endpoint_crossings) = (0, 0);
    let margin = reach(track_spacing);
    for ((a, b), bounds) in p.segs.iter().zip(&p.segment_bounds) {
        for ((c, d), other_bounds) in q.segs.iter().zip(&q.segment_bounds) {
            if !near_boxes(*bounds, *other_bounds, margin) {
                continue;
            }
            if let Some(overlap) = collinear_overlap(a, b, c, d) {
                let canonical = Canonical::of(a, b);
                let base = dot(a, canonical.u);
                let (lo, hi) = if canonical.sign {
                    (base + overlap.lo, base + overlap.hi)
                } else {
                    (base - overlap.hi, base - overlap.lo)
                };
                lines
                    .entry(canonical.id)
                    .or_insert_with(|| Line::new(canonical))
                    .raw
                    .push((lo, hi));
            }
            near += near_parallel_overlap(a, b, c, d, track_spacing);
            if proper_crossing(a, b, c, d) {
                if common_node {
                    shared_endpoint_crossings += 1;
                } else {
                    crossings += 1;
                }
            }
        }
    }
    if !lines.is_empty() {
        for end in p.ends() {
            for other in q.ends() {
                if !same(&end.point, &other.point) || !end.identifies_with(&other) {
                    continue;
                }
                let stub = end.stub.min(other.stub);
                for line in lines.values_mut() {
                    if line.on_line(&end.point) {
                        let t = dot(&end.point, line.canonical.u);
                        line.exempt.push((t - stub, t + stub));
                    }
                }
            }
        }
    }
    PairStats {
        shared: lines.values().map(Line::remaining).sum(),
        near,
        crossings,
        shared_endpoint_crossings,
        touches: if common_node {
            0
        } else {
            touches(&p.segs, &q.segs) + touches(&q.segs, &p.segs)
        },
    }
}

// Exact former evaluator retained as an independent test/benchmark reference.
fn pair_stats_legacy(p: &Route, q: &Route, track_spacing: f64) -> PairStats {
    let common_node = [p.edge.source.as_str(), p.edge.target.as_str()]
        .iter()
        .any(|id| *id == q.edge.source || *id == q.edge.target);
    let mut lines: BTreeMap<LineId, Line> = BTreeMap::new();
    let mut near = 0.0;
    let seg_box = |a: &Point, b: &Point| (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y));
    let margin = reach(track_spacing);
    for (a, b) in &p.segs {
        for (c, d) in &q.segs {
            // Segments farther apart than any test reaches add nothing.
            if !near_boxes(seg_box(a, b), seg_box(c, d), margin) {
                continue;
            }
            if let Some(overlap) = collinear_overlap(a, b, c, d) {
                let canonical = Canonical::of(a, b);
                // X(s) = a + s·u_ab, so X·u = a·u + s·σ with σ the sign relating u_ab to u.
                let base = dot(a, canonical.u);
                let (lo, hi) = if canonical.sign {
                    (base + overlap.lo, base + overlap.hi)
                } else {
                    (base - overlap.hi, base - overlap.lo)
                };
                lines
                    .entry(canonical.id)
                    .or_insert_with(|| Line::new(canonical))
                    .raw
                    .push((lo, hi));
            }
            near += near_parallel_overlap(a, b, c, d, track_spacing);
        }
    }
    // An identified shared end exempts its stub on the same line: the first or last segment, up to its length.
    for end in p.ends() {
        for other in q.ends() {
            if !same(&end.point, &other.point) || !end.identifies_with(&other) {
                continue;
            }
            let stub = end.stub.min(other.stub);
            for line in lines.values_mut() {
                if line.on_line(&end.point) {
                    let t = dot(&end.point, line.canonical.u);
                    line.exempt.push((t - stub, t + stub));
                }
            }
        }
    }
    let shared: f64 = lines.values().map(Line::remaining).sum();
    let (mut crossings, mut shared_endpoint_crossings) = (0, 0);
    for (a, b) in &p.segs {
        for (c, d) in &q.segs {
            if !near_boxes(seg_box(a, b), seg_box(c, d), margin) {
                continue;
            }
            if proper_crossing(a, b, c, d) {
                if common_node {
                    shared_endpoint_crossings += 1;
                } else {
                    crossings += 1;
                }
            }
        }
    }
    let touches = if common_node {
        0
    } else {
        touches(&p.segs, &q.segs) + touches(&q.segs, &p.segs)
    };
    PairStats {
        shared,
        near,
        crossings,
        shared_endpoint_crossings,
        touches,
    }
}

/// Every conflict of one route, named by the partner it conflicts with. The totals of `measure` are these lists
/// summed over all routes, so the per-route view and the totals cannot disagree.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteConflicts {
    pub edge: String,
    pub line_node_hits: Vec<String>,
    pub container_traversals: Vec<String>,
    pub hugging: Vec<String>,
    pub container_hugging: Vec<String>,
    pub shared_lines: Vec<String>,
    pub near_parallel: Vec<String>,
    pub crossings: Vec<String>,
    pub shared_endpoint_crossings: Vec<String>,
    pub bends: usize,
    pub length: f64,
}

/// The conflicts of the route `edge_id`. An unknown edge or an invalid drawing is an error.
/// Whether two edges belong to one declared bus of the drawing.
fn same_bus(drawing: &Drawing, a: &str, b: &str) -> bool {
    drawing
        .buses
        .iter()
        .any(|bus| bus.iter().any(|e| e == a) && bus.iter().any(|e| e == b))
}

pub fn route_conflicts(drawing: &Drawing, edge_id: &str) -> Result<RouteConflicts, String> {
    let parents = validate(drawing)?;
    let rects: Vec<(&DrawNode, Rect)> = drawing.nodes.iter().map(|n| (n, rect_of(n))).collect();
    let leaves: Vec<&(&DrawNode, Rect)> = rects.iter().filter(|(n, _)| !n.container).collect();
    let containers: Vec<&(&DrawNode, Rect)> = rects.iter().filter(|(n, _)| n.container).collect();
    let routes: Vec<Route> = drawing.edges.iter().map(Route::new).collect();
    let index = routes
        .iter()
        .position(|r| r.edge.id == edge_id)
        .ok_or_else(|| format!("no route {edge_id}"))?;
    let route = &routes[index];
    let nodes = node_stats(route, &leaves, &containers, &parents, HUG_MARGIN, false);
    let mut conflicts = RouteConflicts {
        edge: edge_id.to_string(),
        line_node_hits: nodes.hits.iter().map(|s| s.to_string()).collect(),
        container_traversals: nodes.traversals.iter().map(|s| s.to_string()).collect(),
        hugging: nodes.hug.iter().map(|(s, _)| s.to_string()).collect(),
        container_hugging: nodes
            .container_hug
            .iter()
            .map(|(s, _)| s.to_string())
            .collect(),
        bends: route.bends,
        length: route.length,
        ..RouteConflicts::default()
    };
    for other in routes
        .iter()
        .filter(|r| r.edge.id != edge_id && !same_bus(drawing, edge_id, &r.edge.id))
    {
        let Some(stats) = pair_stats_near(route, other, drawing.track_spacing) else {
            continue;
        };
        let id = other.edge.id.clone();
        if stats.shared > EPS {
            conflicts.shared_lines.push(id.clone());
        }
        if stats.near > EPS {
            conflicts.near_parallel.push(id.clone());
        }
        if stats.crossings > 0 {
            conflicts.crossings.push(id.clone());
        }
        if stats.shared_endpoint_crossings > 0 {
            conflicts.shared_endpoint_crossings.push(id);
        }
    }
    Ok(conflicts)
}

/// Checks the drawing and returns the parent of every node. Errors name the first problem found.
fn validate(drawing: &Drawing) -> Result<BTreeMap<&str, Option<&str>>, String> {
    let mut parents = BTreeMap::new();
    for n in &drawing.nodes {
        if parents
            .insert(n.id.as_str(), n.parent_id.as_deref())
            .is_some()
        {
            return Err(format!("duplicate node id {}", n.id));
        }
        if ![n.x, n.y, n.width, n.height].iter().all(|v| v.is_finite())
            || n.width <= 0.
            || n.height <= 0.
        {
            return Err(format!("node {} has invalid geometry", n.id));
        }
    }
    for n in &drawing.nodes {
        let mut current = n.parent_id.as_deref();
        let mut steps = 0;
        while let Some(parent) = current {
            match parents.get(parent) {
                None => return Err(format!("node {} names unknown parent {}", n.id, parent)),
                Some(next) => current = *next,
            }
            steps += 1;
            if steps > drawing.nodes.len() {
                return Err(format!("containment cycle at node {}", n.id));
            }
        }
    }
    for e in &drawing.edges {
        if !parents.contains_key(e.source.as_str()) || !parents.contains_key(e.target.as_str()) {
            return Err(format!("edge {} names an unknown node", e.id));
        }
        if !e.points.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            return Err(format!("edge {} has a non-finite point", e.id));
        }
        if dedupe(&e.points).len() < 2 {
            return Err(format!("edge {} has no route", e.id));
        }
    }
    Ok(parents)
}

fn dot(p: &Point, u: (f64, f64)) -> f64 {
    p.x * u.0 + p.y * u.1
}

/// Hashable line identity: canonical direction and offset, rounded.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct LineId {
    ux: i64,
    uy: i64,
    offset: i64,
}

/// A segment's line in canonical form: the unit direction with x > 0 (or x = 0 and y > 0), the signed offset of
/// the line along the normal, and whether the segment itself runs along the canonical direction.
#[derive(Clone, Copy)]
struct Canonical {
    id: LineId,
    u: (f64, f64),
    offset: f64,
    sign: bool,
}

impl Canonical {
    fn of(a: &Point, b: &Point) -> Self {
        let len = dist(a, b);
        let (dx, dy) = ((b.x - a.x) / len, (b.y - a.y) / len);
        let sign = dx > EPS || (dx.abs() <= EPS && dy >= 0.);
        let u = if sign { (dx, dy) } else { (-dx, -dy) };
        let offset = -u.1 * a.x + u.0 * a.y;
        let id = LineId {
            ux: (u.0 * 1e6).round() as i64,
            uy: (u.1 * 1e6).round() as i64,
            offset: (offset * 1e3).round() as i64,
        };
        Canonical {
            id,
            u,
            offset,
            sign,
        }
    }
}

/// Intervals on one line, in canonical coordinates: the raw overlaps and the stub exemptions.
struct Line {
    canonical: Canonical,
    raw: Vec<(f64, f64)>,
    exempt: Vec<(f64, f64)>,
}

impl Line {
    fn new(canonical: Canonical) -> Self {
        Line {
            canonical,
            raw: Vec::new(),
            exempt: Vec::new(),
        }
    }
    fn on_line(&self, p: &Point) -> bool {
        let normal = -self.canonical.u.1 * p.x + self.canonical.u.0 * p.y;
        (normal - self.canonical.offset).abs() < EPS * 10.
    }
    /// Overlap length not covered by an exemption. Overlaps are merged first, so a stretch shared by three routes
    /// is measured once here and counted as pairs by the caller.
    fn remaining(&self) -> f64 {
        let raw = union(&self.raw);
        let exempt = union(&self.exempt);
        measure_of(&raw) - intersection_measure(&raw, &exempt)
    }
}

/// Sorted, disjoint union of intervals.
fn union(intervals: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut sorted: Vec<(f64, f64)> = intervals
        .iter()
        .copied()
        .filter(|(lo, hi)| hi > lo)
        .collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, f64)> = Vec::new();
    for (lo, hi) in sorted {
        match out.last_mut() {
            Some(last) if lo <= last.1 + EPS => last.1 = last.1.max(hi),
            _ => out.push((lo, hi)),
        }
    }
    out
}

fn measure_of(intervals: &[(f64, f64)]) -> f64 {
    intervals.iter().map(|(lo, hi)| hi - lo).sum()
}

/// Measure of the intersection of two disjoint, sorted unions.
fn intersection_measure(a: &[(f64, f64)], b: &[(f64, f64)]) -> f64 {
    let mut total = 0.;
    for (alo, ahi) in a {
        for (blo, bhi) in b {
            total += (ahi.min(*bhi) - alo.max(*blo)).max(0.);
        }
    }
    total
}

/// Length over which two parallel segments run side by side closer than `spacing` (and not on one line).
fn near_parallel_overlap(a: &Point, b: &Point, c: &Point, d: &Point, spacing: f64) -> f64 {
    let len = dist(a, b);
    let len2 = dist(c, d);
    if len < EPS || len2 < EPS {
        return 0.;
    }
    let (ux, uy) = ((b.x - a.x) / len, (b.y - a.y) / len);
    let (vx, vy) = ((d.x - c.x) / len2, (d.y - c.y) / len2);
    if (ux * vy - uy * vx).abs() > 1e-6 {
        return 0.;
    }
    let gap = offset(a, b, c).abs();
    if gap <= EPS || gap >= spacing {
        return 0.;
    }
    let tc = (c.x - a.x) * ux + (c.y - a.y) * uy;
    let td = (d.x - a.x) * ux + (d.y - a.y) * uy;
    (tc.max(td).min(len) - tc.min(td).max(0.)).max(0.)
}

/// One edge with its derived data. The polyline is normalized first: repeated and forward-collinear points are
/// removed, so inserting a redundant point never changes a stub or a measurement.
struct Route<'a> {
    edge: &'a DrawEdge,
    segs: Vec<(Point, Point)>,
    length: f64,
    bends: usize,
    /// The bounding box of the polyline: (min x, min y, max x, max y).
    bounds: (f64, f64, f64, f64),
    segment_bounds: Vec<(f64, f64, f64, f64)>,
}

/// An end of a route: its point, the identity it may share (explicit port or bundle), and its stub length.
struct End<'a> {
    point: Point,
    port: Option<&'a str>,
    bundle: Option<&'a str>,
    stub: f64,
}

impl End<'_> {
    fn identifies_with(&self, other: &End<'_>) -> bool {
        (self.port.is_some() && self.port == other.port)
            || (self.bundle.is_some() && self.bundle == other.bundle)
    }
}

impl<'a> Route<'a> {
    fn new(edge: &'a DrawEdge) -> Self {
        Self::new_inner::<false>(edge)
    }

    fn new_inner<const LEGACY: bool>(edge: &'a DrawEdge) -> Self {
        let points = normalize(&dedupe(&edge.points));
        let segs: Vec<(Point, Point)> = points
            .windows(2)
            .map(|w| (w[0].clone(), w[1].clone()))
            .collect();
        let length = segs.iter().map(|(a, b)| dist(a, b)).sum();
        let segment_bounds = if LEGACY {
            Vec::new()
        } else {
            segs.iter()
                .map(|(a, b)| (a.x.min(b.x), a.y.min(b.y), a.x.max(b.x), a.y.max(b.y)))
                .collect()
        };
        let bounds = points.iter().fold(
            (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
            |(x0, y0, x1, y1), p| (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y)),
        );
        Route {
            edge,
            segs,
            length,
            bends: bends_of(&points),
            bounds,
            segment_bounds,
        }
    }
    fn ends(&self) -> Vec<End<'a>> {
        let mut ends = Vec::new();
        if let Some((a, b)) = self.segs.first() {
            ends.push(End {
                point: a.clone(),
                port: self.edge.source_port.as_deref(),
                bundle: self.edge.bundle_id.as_deref(),
                stub: dist(a, b),
            });
        }
        if let Some((a, b)) = self.segs.last() {
            ends.push(End {
                point: b.clone(),
                port: self.edge.target_port.as_deref(),
                bundle: self.edge.bundle_id.as_deref(),
                stub: dist(a, b),
            });
        }
        ends
    }
    fn clip_length_within(&self, r: &Rect, margin: f64) -> f64 {
        let inflated = r.inflate(margin);
        self.segs
            .iter()
            .map(|(a, b)| clip_length(a, b, &inflated))
            .sum()
    }
}

/// The route without its first and last `skip` length: the stubs that leave the ports of its own nodes.
fn trim_stub(segs: &[(Point, Point)], skip: f64) -> Vec<(Point, Point)> {
    let front = cut_front(segs, skip);
    let mut reversed: Vec<(Point, Point)> = front
        .iter()
        .rev()
        .map(|(a, b)| (b.clone(), a.clone()))
        .collect();
    reversed = cut_front(&reversed, skip);
    reversed
        .iter()
        .rev()
        .map(|(a, b)| (b.clone(), a.clone()))
        .collect()
}

/// The segments after removing `skip` length from their start.
fn cut_front(segs: &[(Point, Point)], skip: f64) -> Vec<(Point, Point)> {
    let mut remaining = skip;
    let mut out = Vec::new();
    for (a, b) in segs {
        if !out.is_empty() {
            out.push((a.clone(), b.clone()));
            continue;
        }
        let length = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        if remaining >= length {
            remaining -= length;
            continue;
        }
        let t = remaining / length.max(EPS);
        let start = Point {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
        };
        out.push((start, b.clone()));
    }
    out
}

/// Removes interior points that lie straight between their neighbours in the same direction. A point where the
/// route reverses stays, so backtracking is still visible.
fn normalize(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        while out.len() >= 2 {
            let (a, b) = (&out[out.len() - 2], &out[out.len() - 1]);
            let (ab, bp) = ((b.x - a.x, b.y - a.y), (p.x - b.x, p.y - b.y));
            let collinear = (ab.0 * bp.1 - ab.1 * bp.0).abs() < EPS;
            let forward = ab.0 * bp.0 + ab.1 * bp.1 > 0.;
            if collinear && forward {
                out.pop();
            } else {
                break;
            }
        }
        out.push(p.clone());
    }
    out
}

/// Absolute rectangle of a node.
fn rect_of(n: &DrawNode) -> Rect {
    Rect {
        x0: n.x,
        y0: n.y,
        x1: n.x + n.width,
        y1: n.y + n.height,
    }
}

fn is_nested(parents: &BTreeMap<&str, Option<&str>>, a: &str, b: &str) -> bool {
    is_ancestor(parents, a, b) || is_ancestor(parents, b, a)
}

/// True when `ancestor` is reached from `id` by following parent links.
fn is_ancestor(parents: &BTreeMap<&str, Option<&str>>, ancestor: &str, id: &str) -> bool {
    let mut current = parents.get(id).copied().flatten();
    while let Some(parent) = current {
        if parent == ancestor {
            return true;
        }
        current = parents.get(parent).copied().flatten();
    }
    false
}

fn same(a: &Point, b: &Point) -> bool {
    (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// Direction changes of a route; collinear and repeated points do not count.
fn bends_of(points: &[Point]) -> usize {
    points
        .windows(3)
        .filter(|w| {
            let (ax, ay) = (w[1].x - w[0].x, w[1].y - w[0].y);
            let (bx, by) = (w[2].x - w[1].x, w[2].y - w[1].y);
            (ax * by - ay * bx).abs() > EPS
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_pair_parity(drawing: &Drawing) {
        let variants = [(0., false), (HUG_MARGIN, false), (24., true)];
        let (old, old_work) =
            measure_variants_inner::<true, true, true>(drawing, &variants).unwrap();
        let (indexed, work) =
            measure_variants_inner::<false, true, false>(drawing, &variants).unwrap();
        assert_eq!(indexed, old);
        assert_eq!(
            serde_json::to_vec(&indexed).unwrap(),
            serde_json::to_vec(&old).unwrap()
        );
        assert_eq!(work.candidate_pairs, old_work.candidate_pairs);
        assert_eq!(work.segment_pair_visits * 2, old_work.segment_pair_visits);
        assert_eq!(measure_variants(drawing, &variants).unwrap(), old);
    }

    #[test]
    fn indexed_pair_generation_preserves_exact_order_and_metrics() {
        for seed in 0..64 {
            let mut d = drawing(vec![node("a", -20., -20.), node("b", 600., 600.)], vec![]);
            for i in 0..(16 + seed * 2) {
                let x = ((i * 73 + seed * 11) % 521) as f64;
                let y = ((i * 31 + seed * 47) % 509) as f64;
                let mut e = edge(
                    &format!("e{i}"),
                    if i % 3 == 0 { "b" } else { "a" },
                    if i % 5 == 0 { "a" } else { "b" },
                    match i % 6 {
                        0 => vec![p(x, y), p(x + 80., y), p(x + 80., y + 120.)],
                        1 => vec![p(x, y), p(x + 70., y + 60.), p(x - 30., y + 90.)],
                        2 => vec![p(x, y), p(x, y), p(x, y + 1e-5)],
                        3 => vec![p(x, y), p(x + 100., y), p(x - 50., y)],
                        4 => vec![p(x, y), p(x + 1e-8, y + 1e-8), p(x + 80., y + 1e-8)],
                        _ => vec![p(x, y), p(x + 40., y), p(x + 80., y)],
                    },
                );
                e.source_port = (i % 2 == 0).then(|| "shared".into());
                e.bundle_id = (i % 7 == 0).then(|| "bundle".into());
                d.edges.push(e);
            }
            for spacing in [-1., 0., 1e-8, 6., 100., f64::INFINITY, f64::NAN] {
                d.track_spacing = spacing;
                assert_pair_parity(&d);
            }
        }
    }

    #[test]
    fn pair_index_is_conservative_at_boundaries_and_extreme_coordinates() {
        let mut d = drawing(vec![node("a", 0., 0.)], vec![]);
        for (i, (x, y)) in [
            (0., 0.),
            (0., 6.),
            (0., 6. + 10. * EPS),
            (0., 6. + 11. * EPS),
            (-0., 0.),
            (-1e300, -1e300),
            (1e300, 1e300),
            (1e300, -1e300),
            (-1e300, 1e300),
            (50., 50.),
            (50., 56.),
            (50., 56. + 10. * EPS),
        ]
        .into_iter()
        .enumerate()
        {
            d.edges.push(edge(
                &format!("e{i}"),
                "a",
                "a",
                vec![p(x, y), p(x + 10., y)],
            ));
        }
        let routes: Vec<_> = d.edges.iter().map(Route::new).collect();
        let index = PairIndex::new(&routes);
        for margin in [0., EPS, 6. + 10. * EPS, 1e300, f64::INFINITY] {
            for (i, r) in routes.iter().enumerate() {
                let mut found = Vec::new();
                index.query::<true>(i, r.bounds, margin, &mut found, &mut PairWork::default());
                found.sort_unstable();
                let expected: Vec<_> = routes
                    .iter()
                    .enumerate()
                    .skip(i + 1)
                    .filter(|(_, q)| near_boxes(r.bounds, q.bounds, margin))
                    .map(|(j, _)| j)
                    .collect();
                assert_eq!(found, expected);
            }
        }
    }

    #[test]
    fn sparse_pair_generation_avoids_the_quadratic_iteration() {
        let mut d = drawing(vec![node("a", 0., 0.)], vec![]);
        for i in 0..1024 {
            let x = (i % 32) as f64 * 100.;
            let y = (i / 32) as f64 * 100.;
            d.edges.push(edge(
                &format!("e{i}"),
                "a",
                "a",
                vec![p(x, y), p(x + 10., y)],
            ));
        }
        let (old, old_work) =
            measure_variants_inner::<true, true, true>(&d, &[(12., false)]).unwrap();
        let (new, work) =
            measure_variants_inner::<false, true, false>(&d, &[(12., false)]).unwrap();
        assert_eq!(new, old);
        assert_eq!(old_work.bounds_tests, 1024 * 1023 / 2);
        assert_eq!(work.candidate_pairs, 0);
        assert!(work.bounds_tests < old_work.bounds_tests / 8, "{work:?}");
        for e in &mut d.edges {
            e.points = vec![p(0., 0.), p(100., 100.)];
        }
        assert_pair_parity(&d);
        let (_, dense) = measure_variants_inner::<false, true, false>(&d, &[]).unwrap();
        assert_eq!(
            dense.candidate_pairs,
            1024 * 1023 / 2,
            "dense work remains explicit"
        );
        assert_pair_parity(&drawing(vec![], vec![]));
    }

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }
    fn node(id: &str, x: f64, y: f64) -> DrawNode {
        node_sized(id, x, y, 20.)
    }
    fn node_sized(id: &str, x: f64, y: f64, size: f64) -> DrawNode {
        DrawNode {
            id: id.into(),
            parent_id: None,
            container: false,
            x,
            y,
            width: size,
            height: size,
            shape: Default::default(),
        }
    }
    fn edge(id: &str, s: &str, t: &str, points: Vec<Point>) -> DrawEdge {
        DrawEdge {
            id: id.into(),
            source: s.into(),
            target: t.into(),
            points,
            source_port: None,
            target_port: None,
            bundle_id: None,
        }
    }
    #[test]
    fn a_required_clearance_applies_to_the_routes_own_nodes_beyond_the_port_stub() {
        // A node 20 wide at the origin. A route leaves its right side, turns down beside the node and runs under it at 4
        // units below its bottom edge: beyond the 12-unit stub it hugs its own node, so a required clearance of 12 counts it.
        // A route that leaves straight away from the node does not.
        let hugging = drawing(
            vec![node("a", 0., 0.), node("b", 200., 200.)],
            vec![edge(
                "e",
                "a",
                "b",
                vec![p(20., 10.), p(24., 10.), p(24., 24.), p(200., 24.)],
            )],
        );
        assert!(measure_with_requirement(&hugging, 12.).unwrap().hug_pairs > 0);
        // The default measure keeps the old rule: own nodes are not measured.
        assert_eq!(measure_with_clearance(&hugging, 12.).unwrap().hug_pairs, 0);
        let straight = drawing(
            vec![node("a", 0., 0.), node("b", 200., 0.)],
            vec![edge("e", "a", "b", vec![p(20., 10.), p(200., 10.)])],
        );
        assert_eq!(
            measure_with_requirement(&straight, 12.).unwrap().hug_pairs,
            0
        );
    }

    fn with_source_port(mut e: DrawEdge, port: &str) -> DrawEdge {
        e.source_port = Some(port.into());
        e
    }
    /// Nodes at the origin for every id that the edges name, so only the intended invalidity is tested.
    fn named(ids: &[&str]) -> Vec<DrawNode> {
        ids.iter().map(|id| node(id, 0., 0.)).collect()
    }
    fn drawing(nodes: Vec<DrawNode>, edges: Vec<DrawEdge>) -> Drawing {
        Drawing {
            nodes,
            edges,
            track_spacing: DEFAULT_TRACK_SPACING,
            buses: Vec::new(),
        }
    }
    trait WithNodes {
        fn with_nodes(self, ids: &[&str]) -> Self;
    }
    impl WithNodes for Drawing {
        fn with_nodes(mut self, ids: &[&str]) -> Self {
            self.nodes = named(ids);
            self
        }
    }

    #[test]
    fn a_clean_drawing_has_no_violations() {
        let m = measure(&drawing(
            vec![node("a", 0., 0.), node("b", 200., 0.)],
            vec![edge("e", "a", "b", vec![p(20., 10.), p(200., 10.)])],
        ))
        .unwrap();
        assert_eq!(m.version, METRICS_VERSION);
        assert_eq!(m.line_node_hits, 0);
        assert_eq!(m.crossings + m.shared_endpoint_crossings, 0);
        assert_eq!(m.hug_pairs, 0);
        assert_eq!(m.bends, 0);
        assert!((m.length - 180.).abs() < 1e-9);
    }

    #[test]
    fn crossing_and_bend_are_counted() {
        let m = measure(&drawing(
            vec![
                node("a", 0., 0.),
                node("b", 100., 0.),
                node("c", 50., 100.),
                node("d", 50., -100.),
            ],
            vec![
                edge("e1", "a", "b", vec![p(20., 10.), p(100., 10.)]),
                edge("e2", "c", "d", vec![p(60., 100.), p(60., -100.)]),
                edge(
                    "e3",
                    "a",
                    "c",
                    vec![p(10., 20.), p(10., 110.), p(50., 110.)],
                ),
            ],
        ))
        .unwrap();
        assert_eq!(m.crossings, 1);
        assert_eq!(m.bends, 1);
    }

    #[test]
    fn line_through_a_node_is_a_hit_and_near_border_is_hugging() {
        let through = drawing(
            vec![node("a", 0., 0.), node("b", 300., 0.), node("x", 100., 0.)],
            vec![edge("e", "a", "b", vec![p(20., 10.), p(300., 10.)])],
        );
        assert_eq!(measure(&through).unwrap().line_node_hits, 1);
        let near = drawing(
            vec![node("a", 0., 0.), node("b", 300., 0.), node("x", 100., 20.)],
            vec![edge("e", "a", "b", vec![p(20., 10.), p(300., 10.)])],
        );
        let m = measure(&near).unwrap();
        assert_eq!(m.line_node_hits, 0);
        assert_eq!(m.hug_pairs, 1);
        assert!(m.hug_length > 0.);
    }

    #[test]
    fn redundant_points_change_neither_stub_nor_metrics() {
        // Edge a is split by a collinear point; the shared stub must be the same stub in both drawings.
        let make = |split: bool| {
            drawing(
                vec![],
                vec![
                    with_source_port(
                        edge(
                            "a",
                            "one",
                            "two",
                            if split {
                                vec![p(0., 0.), p(10., 0.), p(40., 0.)]
                            } else {
                                vec![p(0., 0.), p(40., 0.)]
                            },
                        ),
                        "port",
                    ),
                    with_source_port(
                        edge(
                            "b",
                            "one",
                            "three",
                            vec![p(0., 0.), p(30., 0.), p(30., 20.)],
                        ),
                        "port",
                    ),
                    edge("c", "four", "five", vec![p(10., -10.), p(10., 10.)]),
                ],
            )
            .with_nodes(&["one", "two", "three", "four", "five"])
        };
        let (split, unsplit) = (
            measure(&make(true)).unwrap(),
            measure(&make(false)).unwrap(),
        );
        assert_eq!(split, unsplit);
        assert_eq!(
            split.shared_line_length, 0.,
            "both routes name the same port: the stub is intended"
        );
        assert_eq!(split.crossings, 2);
    }

    #[test]
    fn coincident_unrelated_endpoints_are_a_shared_line() {
        let m = measure(
            &drawing(
                vec![],
                vec![
                    edge("a", "one", "two", vec![p(0., 0.), p(10., 0.)]),
                    edge("b", "three", "four", vec![p(0., 0.), p(10., 0.)]),
                ],
            )
            .with_nodes(&["one", "two", "three", "four"]),
        )
        .unwrap();
        assert_eq!(m.shared_line_length, 10.);
    }

    #[test]
    fn shared_stub_needs_the_same_explicit_port() {
        let stub = |ports: bool| {
            let mut e1 = edge(
                "e1",
                "a",
                "b",
                vec![p(20., 10.), p(100., 10.), p(300., 10.)],
            );
            let mut e2 = edge(
                "e2",
                "a",
                "c",
                vec![p(20., 10.), p(100., 10.), p(100., 110.), p(300., 110.)],
            );
            if ports {
                e1 = with_source_port(e1, "out");
                e2 = with_source_port(e2, "out");
            }
            drawing(
                vec![
                    node("a", 0., 0.),
                    node("b", 300., 0.),
                    node("c", 300., 100.),
                ],
                vec![e1, e2],
            )
        };
        assert_eq!(
            measure(&stub(false)).unwrap().shared_line_pairs,
            1,
            "no identity: unintended"
        );
        assert_eq!(
            measure(&stub(true)).unwrap().shared_line_pairs,
            0,
            "same port along the stub: intended"
        );
    }

    #[test]
    fn a_re_join_after_a_bend_is_unintended_even_with_the_same_port() {
        // Both routes leave port "out" together for 80 units; e2 bends away and rejoins the line at x = 200.
        // The stub exemption covers the shared start only, so the rejoined stretch (100 units) is a conflict.
        let e1 = with_source_port(edge("e1", "a", "b", vec![p(20., 10.), p(300., 10.)]), "out");
        let e2 = with_source_port(
            edge(
                "e2",
                "a",
                "c",
                vec![
                    p(20., 10.),
                    p(100., 10.),
                    p(100., 40.),
                    p(200., 40.),
                    p(200., 10.),
                    p(300., 10.),
                ],
            ),
            "out",
        );
        let m = measure(&drawing(
            vec![node("a", 0., 0.), node("b", 300., 0.), node("c", 300., 60.)],
            vec![e1, e2],
        ))
        .unwrap();
        assert_eq!(m.shared_line_pairs, 1);
        assert!((m.shared_line_length - 100.).abs() < 1e-6);
    }

    #[test]
    fn nested_nodes_are_not_overlaps_siblings_and_branches_are_classified() {
        let container = |id: &str, x: f64, y: f64, size: f64| DrawNode {
            container: true,
            ..node_sized(id, x, y, size)
        };
        let child = |id: &str, parent: &str, x: f64, y: f64| DrawNode {
            parent_id: Some(parent.into()),
            ..node_sized(id, x, y, 20.)
        };
        let m = measure(&drawing(
            vec![
                container("box", 0., 0., 100.),
                child("inner", "box", 5., 5.),
                child("sibling", "box", 10., 10.),
                container("second", 50., 50., 100.),
                child("other", "second", 60., 60.),
            ],
            vec![],
        ))
        .unwrap();
        assert_eq!(
            m.sibling_overlaps, 2,
            "inner/sibling under box; box/second at top level"
        );
        assert_eq!(m.cross_branch_overlaps, 1, "box/other across branches");
    }

    #[test]
    fn near_parallel_tracks_are_their_own_category() {
        let near = drawing(
            vec![
                node("a", 0., 0.),
                node("b", 300., 0.),
                node("c", 0., 40.),
                node("d", 300., 40.),
            ],
            vec![
                edge("e1", "a", "b", vec![p(20., 10.), p(300., 10.)]),
                edge("e2", "c", "d", vec![p(20., 14.), p(300., 14.)]),
            ],
        );
        let m = measure(&near).unwrap();
        assert_eq!(m.near_parallel_pairs, 1);
        assert_eq!(m.shared_line_pairs, 0, "parallel is not overlapping");
        let mut far = near.clone();
        far.edges[1].points = vec![p(20., 20.), p(300., 20.)];
        let m = measure(&far).unwrap();
        assert_eq!(
            m.near_parallel_pairs, 0,
            "10 units apart is beyond the track spacing"
        );
    }

    #[test]
    fn invalid_drawings_are_errors() {
        let unknown_parent = {
            let mut n = node("a", 0., 0.);
            n.parent_id = Some("missing".into());
            drawing(vec![n], vec![])
        };
        assert!(measure(&unknown_parent)
            .unwrap_err()
            .contains("unknown parent"));
        let cycle = {
            let mut a = node("a", 0., 0.);
            let mut b = node("b", 0., 0.);
            a.parent_id = Some("b".into());
            b.parent_id = Some("a".into());
            drawing(vec![a, b], vec![])
        };
        assert!(measure(&cycle).unwrap_err().contains("cycle"));
        let no_route = drawing(
            vec![node("a", 0., 0.), node("b", 100., 0.)],
            vec![edge("e", "a", "b", vec![p(20., 10.)])],
        );
        assert!(measure(&no_route).unwrap_err().contains("no route"));
        let dangling = drawing(
            vec![node("a", 0., 0.)],
            vec![edge("e", "a", "ghost", vec![p(20., 10.), p(100., 10.)])],
        );
        assert!(measure(&dangling).unwrap_err().contains("unknown node"));
        let mut nonfinite = drawing(
            vec![node("a", 0., 0.)],
            vec![edge("e", "a", "a", vec![p(0., 0.), p(f64::NAN, 1.)])],
        );
        for invalid in [&unknown_parent, &cycle, &no_route, &dangling, &nonfinite] {
            assert_eq!(
                measure_variants_inner::<true, true, true>(invalid, &[(12., false)]).unwrap_err(),
                measure_variants_inner::<false, true, false>(invalid, &[(12., false)]).unwrap_err(),
            );
        }
        nonfinite.edges[0].points[1] = p(f64::INFINITY, 0.);
        assert!(measure(&nonfinite).is_err());
    }

    #[test]
    fn a_container_border_the_route_does_not_belong_to_is_hugged_separately() {
        let mut inner = node("inner", 120., 20.);
        inner.parent_id = Some("box".into());
        let mut boxed = node_sized("box", 100., -50., 200.);
        boxed.container = true;
        // The route runs 6 units above the container's top border, outside it, and belongs to neither endpoint.
        let m = measure(&drawing(
            vec![boxed, inner, node("a", 0., 0.), node("b", 400., 0.)],
            vec![edge("e", "a", "b", vec![p(20., -56.), p(400., -56.)])],
        ))
        .unwrap();
        assert_eq!(m.container_hug_pairs, 1);
        assert_eq!(m.hug_pairs, 0, "container distance is not node distance");
    }

    /// Every diagnostic fixture states the conflict counts its drawing must produce. A failing fixture names
    /// the field and both values.
    #[test]
    fn diagnostic_fixtures_produce_their_expected_conflicts() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let fixture: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let drawing: Drawing = serde_json::from_value(fixture["drawing"].clone()).unwrap();
            assert_pair_parity(&drawing);
            let measured = serde_json::to_value(measure(&drawing).unwrap()).unwrap();
            for (field, expected) in fixture["expect"].as_object().unwrap() {
                let actual = &measured[field];
                let same = actual
                    .as_f64()
                    .zip(expected.as_f64())
                    .is_some_and(|(a, e)| (a - e).abs() < 1e-6);
                assert!(
                    same,
                    "{}: {field} is {actual}, expected {expected}",
                    path.display()
                );
            }
            checked += 1;
        }
        assert!(checked >= 8, "all fixtures are checked");
    }

    /// Per-route lists are the totals split by route: summed over all routes they reproduce the totals of `measure`.
    #[test]
    fn route_conflicts_add_up_to_the_totals() {
        let drawing = drawing(
            vec![
                node("a", 0., 0.),
                node("b", 300., 0.),
                node("x", 100., 0.),
                node("c", 300., 60.),
            ],
            vec![
                edge("e1", "a", "b", vec![p(20., 10.), p(300., 10.)]),
                edge(
                    "e2",
                    "a",
                    "c",
                    vec![p(20., 30.), p(150., 30.), p(150., 90.), p(300., 90.)],
                ),
                edge(
                    "e3",
                    "c",
                    "b",
                    vec![p(300., 70.), p(200., 70.), p(200., 40.), p(20., 40.)],
                ),
            ],
        );
        let totals = measure(&drawing).unwrap();
        let per: Vec<RouteConflicts> = drawing
            .edges
            .iter()
            .map(|e| route_conflicts(&drawing, &e.id).unwrap())
            .collect();
        let crossings: usize = per.iter().map(|c| c.crossings.len()).sum();
        let hits: usize = per.iter().map(|c| c.line_node_hits.len()).sum();
        let hugging: usize = per.iter().map(|c| c.hugging.len()).sum();
        assert_eq!(hits, totals.line_node_hits);
        assert_eq!(hugging, totals.hug_pairs);
        // Each crossing pair is listed once from each side.
        assert_eq!(crossings, 2 * totals.crossing_pairs);
        assert!(route_conflicts(&drawing, "missing").is_err());
    }
    #[test]
    fn a_circle_is_measured_as_its_disc() {
        // A circle at (100, 0) of size 60; a line through the empty corner of its box, and one that ends on its outline and
        // leaves it, do not enter it; one through its middle does.
        let circle = |id: &str| DrawNode {
            shape: arqlet_model::NodeShape::Circle,
            ..node_sized(id, 100., 0., 60.)
        };
        let other = node_sized("a", 0., 200., 20.);
        let far = node_sized("b", 300., 200., 20.);
        let measure = |points: Vec<Point>, target: &str, nodes: Vec<DrawNode>| {
            measure(&drawing(nodes, vec![edge("e", "a", target, points)]))
                .unwrap()
                .line_node_hits
        };
        let corner = vec![
            p(0., 210.),
            p(0., 3.),
            p(103., 3.),
            p(103., -40.),
            p(310., -40.),
            p(310., 200.),
        ];
        assert_eq!(
            measure(
                corner.clone(),
                "b",
                vec![other.clone(), far.clone(), circle("c")]
            ),
            0
        );
        assert_eq!(
            measure(
                corner,
                "b",
                vec![other.clone(), far.clone(), node_sized("c", 100., 0., 60.)]
            ),
            1
        );
        let through = vec![p(10., 200.), p(10., 30.), p(310., 30.), p(310., 200.)];
        assert_eq!(
            measure(through, "b", vec![other.clone(), far, circle("c")]),
            1
        );
        // An edge that ends on the circle (at its left point) does not count as entering it.
        let ending = vec![p(10., 200.), p(10., 30.), p(100., 30.)];
        assert_eq!(measure(ending, "c", vec![other, circle("c")]), 0);
    }
}
