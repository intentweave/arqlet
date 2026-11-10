//! Channel-based orthogonal routing. Track lines run through the margin of every node and through the
//! port stubs; each edge is a shortest path over these lines with a bend penalty. Routes are routed in
//! rounds with a congestion penalty on shared segments (negotiated congestion), so parallel routes
//! take distinct tracks. Routes never enter a node's margin, except where the margin cannot be
//! avoided, which the caller can check with `hits`.
use super::simplify;
use arqlet_model::*;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

/// One constructor-internal DP call; benchmark-only attribution by logical edge and search phase.
#[cfg(feature = "benchmark-timings")]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructionSearch {
    pub edge: String,
    pub phase: &'static str,
    pub failed: bool,
    pub states: usize,
    pub transitions: usize,
}

/// Provisional work inside the layered constructor, separate from delegated fallback routing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConstructionCounters {
    #[cfg(feature = "benchmark-timings")]
    pub search_details: Vec<ConstructionSearch>,
    #[cfg(feature = "benchmark-timings")]
    pub planned_failures: Vec<(String, &'static str)>,
    pub preparations: usize,
    pub prepared_edges: usize,
    pub planned_attempts: usize,
    pub planned_obstacle_failures: usize,
    pub planned_track_failures: usize,
    pub edge_searches: usize,
    pub lane_searches: usize,
    /// Calls to the monotone dynamic programme, including unsuccessful and zero-corridor calls.
    pub searches: usize,
    pub failed_searches: usize,
    /// Corridor/lane cells allocated (n * m), not heap pops or A* expansions.
    pub states: usize,
    /// Clear lane-to-lane transitions evaluated inside corridor columns.
    pub transitions: usize,
    /// Native benchmark times; preparation includes section planning; search is constructor-internal only.
    pub preparation_nanos: u64,
    pub search_nanos: u64,
}

impl ConstructionCounters {
    pub fn add(&mut self, other: &Self) {
        #[cfg(feature = "benchmark-timings")]
        {
            self.search_details.extend_from_slice(&other.search_details);
            self.planned_failures
                .extend_from_slice(&other.planned_failures);
        }
        self.preparations += other.preparations;
        self.prepared_edges += other.prepared_edges;
        self.planned_attempts += other.planned_attempts;
        self.planned_obstacle_failures += other.planned_obstacle_failures;
        self.planned_track_failures += other.planned_track_failures;
        self.edge_searches += other.edge_searches;
        self.lane_searches += other.lane_searches;
        self.searches += other.searches;
        self.failed_searches += other.failed_searches;
        self.states += other.states;
        self.transitions += other.transitions;
        self.preparation_nanos = self
            .preparation_nanos
            .saturating_add(other.preparation_nanos);
        self.search_nanos = self.search_nanos.saturating_add(other.search_nanos);
    }
}

/// Timing subphases of one complete abstract-routing pass. These are native
/// benchmark timings and remain zero in ordinary and WASM builds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AbstractCompletionTimes {
    pub abstract_plan_nanos: u64,
    pub growth_nanos: u64,
    pub construction_nanos: u64,
    pub seed_preparation_nanos: u64,
    pub fallback_nanos: u64,
    pub track_repair_nanos: u64,
    pub finalization_nanos: u64,
    pub track_conflict_check_nanos: u64,
}

impl AbstractCompletionTimes {
    pub fn add(&mut self, other: &Self) {
        self.abstract_plan_nanos = self
            .abstract_plan_nanos
            .saturating_add(other.abstract_plan_nanos);
        self.growth_nanos = self.growth_nanos.saturating_add(other.growth_nanos);
        self.construction_nanos = self
            .construction_nanos
            .saturating_add(other.construction_nanos);
        self.seed_preparation_nanos = self
            .seed_preparation_nanos
            .saturating_add(other.seed_preparation_nanos);
        self.fallback_nanos = self.fallback_nanos.saturating_add(other.fallback_nanos);
        self.track_repair_nanos = self
            .track_repair_nanos
            .saturating_add(other.track_repair_nanos);
        self.finalization_nanos = self
            .finalization_nanos
            .saturating_add(other.finalization_nanos);
        self.track_conflict_check_nanos = self
            .track_conflict_check_nanos
            .saturating_add(other.track_conflict_check_nanos);
    }
}

/// Search work of the channel router. Accumulates over every route and congestion round it is passed to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchCounters {
    /// A* expansions: heap pops whose state was current (stale pops are not counted).
    pub expansions: usize,
    /// Completed searches (one per routed edge per round), including searches that found no path.
    pub searches: usize,
    /// The largest number of expansions one search needed.
    pub largest_search: usize,
    /// Legacy track search: candidate routes evaluated (`best_route`), each one a full scan of the obstacles and of the
    /// routes placed before it, plus the early-exit comparisons. Counts evaluations, not A* expansions: the two are
    /// not comparable costs.
    pub legacy_candidates: usize,
    /// Legacy repair: heap pops of the grid search (`grid_route`), including pops of entries superseded later. Counts
    /// grid steps of this router only.
    pub legacy_grid_pops: usize,
    /// The grid pops of the legacy repair by route (keyed by the route's id), so that the work can be attributed to edges.
    pub legacy_grid_pops_by_route: std::collections::BTreeMap<String, usize>,
    /// Local repair of a drawing that continues an earlier one: routes that were reused as they were, and routes that were
    /// repaired again (the affected ones and the reused routes that ran into an obstacle).
    pub routes_kept: usize,
    /// Legacy repair: routes found on a grid taken from the obstacles near them, because the grid of the whole view was over the
    /// limit of the search.
    pub windowed_grids: usize,
    pub routes_rerouted: usize,
    /// Cumulative wall time spent in placement. This includes the organic stages below; native benchmark builds
    /// populate it, while the default core and WASM builds leave it zero. Stage times may overlap.
    pub placement_nanos: u64,
    /// Cumulative time in organic stress majorization (native benchmark builds only).
    pub organic_stress_nanos: u64,
    /// Cumulative time in organic pair separation.
    pub separation_nanos: u64,
    /// Cumulative time in geometric repair.
    pub repair_nanos: u64,
    /// Cumulative time resolving ports, including calls made during repair.
    pub port_resolution_nanos: u64,
    /// Cumulative time routing edges.
    pub routing_nanos: u64,
    /// Cumulative time computing corridor-clearance gaps.
    pub corridor_gap_nanos: u64,
    /// Timed subphases of abstract completion, nested within `routing_nanos`.
    pub abstract_completion: AbstractCompletionTimes,
    /// Number of corridor-clearance gap evaluations.
    pub corridor_gap_calculations: usize,
    /// Number of widening rounds that actually changed the requested gaps.
    pub clearance_widening_rounds: usize,
    /// Widening rounds that left every box geometry unchanged and therefore reused the previous routes.
    pub clearance_widening_noop_rounds: usize,
    /// Layered construction: routes taken from the plans' sections, and edges it handed to the fallback router.
    pub constructed_routes: usize,
    pub delegated_routes: usize,
    /// Complete abstract routing passes, replacing the legacy constructor loop.
    pub abstract_routing_passes: usize,
    /// Fixed-track repair segment projections materialized, including refreshes after accepted moves.
    pub fixed_track_segment_projections_computed: usize,
    /// Route-pair checks made using the cached fixed-track segment projections.
    pub fixed_track_projection_pair_checks: usize,
    /// Layered construction fallback reasons, keyed by route id. Entries are recorded for each construction pass.
    #[cfg(feature = "benchmark-timings")]
    pub delegation_reasons: Vec<(String, String)>,
    /// Cumulative time of the fallback router on the delegated edges (native benchmark builds only).
    pub delegated_nanos: u64,
    /// Complete layout passes (validation, placement, routing) and the placement passes within them: each port-sizing
    /// round and each placement again after the hierarchical sweep counts. Deterministic work units.
    pub layout_passes: usize,
    pub placement_passes: usize,
    /// Placement pass counts by caller-supplied reason, for candidate profiling.
    pub placement_pass_reasons: BTreeMap<String, usize>,
    /// Consumed at the next root `place` call; defaults to `unclassified` for direct/test calls.
    pub next_placement_pass_reason: Option<String>,
    /// Constructions that started from the preparation of the last track count on the same drawing instead of preparing
    /// their edges again. Deterministic.
    pub reused_preparations: usize,
    /// Coordinate-free region order computations and exact reuses within one layout pass.
    pub ordering_passes: usize,
    pub reused_orderings: usize,
    /// Cross-flow lane/track plans computed and reused independently of flow coordinates.
    pub cross_plans: usize,
    pub reused_cross_plans: usize,
    /// Region projection, ranks, dummy/border structure computed and reused without geometry.
    pub structural_plans: usize,
    pub reused_structural_plans: usize,
    /// Non-leaf region coordinate realizations computed or reused after realizing their children.
    pub region_realizations: usize,
    pub reused_region_realizations: usize,
    /// Full routing passes deferred until after their preparation's already-known channel widening.
    pub deferred_routing_passes: usize,
    pub construction: ConstructionCounters,
}

/// Which routes a later congestion round re-routes. `Shared` (the default) re-routes only routes that share a segment.
/// `Extended` also re-routes routes that cross another route (they pass a grid point another route passes) or run
/// along a node margin (hugging a node).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection {
    Shared,
    Extended,
}

/// The crossing cost of the extended selection: enough to make a crossing route search a different corridor.
pub const EXTENDED_CROSSING_COST: f64 = 50.;

/// The stop signal fired during a search. Nothing from the interrupted search may be published: the router falls
/// back to direct stubs for unroutable edges, so a partial route set would look like a valid, sparser drawing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stopped;

/// The stop signal is read every this many A* expansions. A checkpoint, not a time bound.
const STOP_CHECK_EXPANSIONS: usize = 256;

/// One edge to route: the port positions and the stub points just outside them.
#[derive(Clone, Debug)]
pub struct ChannelEnd {
    pub edge_id: String,
    pub source: String,
    pub target: String,
    pub start: Point,
    pub depart: Point,
    pub arrive: Point,
    pub end: Point,
}

const BEND_PENALTY: f64 = 4.;
/// Cost factor for a segment inside a foreign node's margin. Margins are preferred, not forbidden:
/// a tight layout must still be routable.
const MARGIN_FACTOR: f64 = 6.;
pub const ROUNDS: usize = 8;
/// From this round on only routes that still share a segment are ripped up and rerouted (local rounds). The earlier
/// rounds reroute every route, as before.
const LOCAL_FROM: usize = 4;
const EPS: f64 = 1e-6;
/// Directions: +x, -x, +y, -y, and "no direction yet" at the start.
const START: usize = 4;
const OPPOSITE: [usize; 4] = [1, 0, 3, 2];

struct Grid {
    /// With strict clearance a foreign node's margin is not entered at all, instead of being charged.
    strict_margins: bool,
    xs: Vec<f64>,
    ys: Vec<f64>,
    /// Real node interiors: never entered.
    blocked_h: Vec<bool>,
    blocked_v: Vec<bool>,
    /// Inside a node's margin but not its body: allowed at a high cost (or forbidden, with strict clearance).
    margin_h: Vec<bool>,
    margin_v: Vec<bool>,
    /// Containers whose interior contains each segment's midpoint (indexed like `segment`). A route may use a segment
    /// only if every such container owns one of its edge's endpoints (a foreign container is not a passage corridor).
    containers_at: Vec<Vec<usize>>,
    /// Node clearance bands (indices into the real-node list of `route_channels_raw`) holding each segment's midpoint.
    /// With strict clearance a margin segment is usable only when every such band belongs to the edge's endpoints.
    bands_at: Vec<Vec<usize>>,
}

impl Grid {
    fn nx(&self) -> usize {
        self.xs.len()
    }
    fn ny(&self) -> usize {
        self.ys.len()
    }
    /// Segment index: horizontal segments first, then vertical ones.
    fn segment(&self, i: usize, j: usize, dir: usize) -> usize {
        let horizontal = self.nx().saturating_sub(1) * self.ny();
        match dir {
            0 => i * self.ny() + j,
            1 => (i - 1) * self.ny() + j,
            2 => horizontal + i * (self.ny() - 1) + j,
            _ => horizontal + i * (self.ny() - 1) + (j - 1),
        }
    }
    fn segments(&self) -> usize {
        self.nx().saturating_sub(1) * self.ny() + self.nx() * self.ny().saturating_sub(1)
    }
    fn index(&self, values: &[f64], value: f64) -> Option<usize> {
        let next = values.partition_point(|v| *v < value);
        if next > 0 && (values[next - 1] - value).abs() < EPS {
            Some(next - 1)
        } else if next < values.len() && (values[next] - value).abs() < EPS {
            Some(next)
        } else {
            None
        }
    }
}

struct SearchScratch {
    dist: Vec<f64>,
    prev: Vec<usize>,
    touched: Vec<usize>,
    heap: BinaryHeap<Reverse<(u64, u64, usize)>>,
}

impl SearchScratch {
    fn new(total: usize) -> Self {
        Self {
            dist: vec![f64::INFINITY; total],
            prev: vec![usize::MAX; total],
            touched: Vec::new(),
            heap: BinaryHeap::new(),
        }
    }

    fn reset(&mut self) {
        for state in self.touched.drain(..) {
            self.dist[state] = f64::INFINITY;
            self.prev[state] = usize::MAX;
        }
        self.heap.clear();
    }

    fn set(&mut self, state: usize, distance: f64, previous: usize) {
        if self.dist[state].is_infinite() {
            self.touched.push(state);
        }
        self.dist[state] = distance;
        self.prev[state] = previous;
    }
}

fn unique(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(f64::total_cmp);
    values.dedup_by(|a, b| (*a - *b).abs() < EPS);
    values
}

/// Routes every end. `margin` is the clearance around foreign nodes; `track` offsets extra track lines.
pub fn route_channels(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    ends: &[ChannelEnd],
    margin: f64,
    track: f64,
) -> Vec<Route> {
    // A search without a stop signal cannot be interrupted.
    let rounds = route_channels_raw(
        graph,
        rects,
        ends,
        margin,
        track,
        0.,
        &[],
        None,
        false,
        Selection::Shared,
        &mut SearchCounters::default(),
        &|| false,
    )
    .expect("a search without a stop signal cannot be interrupted");
    simplify(rounds.into_iter().last().unwrap_or_default())
}

/// Routes only the `active` edges, around `fixed` routes that keep their geometry. The fixed routes occupy the grid
/// (their corners become grid lines, their segments and points carry congestion and crossing costs), but they are
/// not obstacles: a route may still cross or share a segment at the cost the search assigns. Returns the active
/// routes of the last round, simplified. `Ok(None)` when a fixed route is not axis-aligned, since its segments
/// cannot be placed on the grid; the caller decides the fallback.
#[allow(clippy::too_many_arguments)]
pub fn route_active(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    ends: &[ChannelEnd],
    margin: f64,
    track: f64,
    fixed: &[Route],
    active: &BTreeSet<String>,
    strict: bool,
    counters: &mut SearchCounters,
    stop: &dyn Fn() -> bool,
) -> Result<Option<Vec<Route>>, Stopped> {
    let axis_aligned = fixed.iter().all(|route| {
        route
            .points
            .windows(2)
            .all(|w| (w[0].x - w[1].x).abs() < EPS || (w[0].y - w[1].y).abs() < EPS)
    });
    if !axis_aligned {
        return Ok(None);
    }
    let rounds = route_channels_raw(
        graph,
        rects,
        ends,
        margin,
        track,
        0.,
        fixed,
        Some(active),
        strict,
        Selection::Shared,
        counters,
        stop,
    )?;
    let last = rounds.into_iter().last().unwrap_or_default();
    let active_routes: Vec<Route> = last
        .into_iter()
        .filter(|route| active.contains(&route.id))
        .collect();
    Ok(Some(simplify(active_routes)))
}

/// Every congestion round as a complete route set (at most `ROUNDS`, each simplified like the result).
/// Round 0 has no congestion penalty; later rounds spread the routes. The layout crate scores each set as a
/// whole candidate; this function makes no judgement itself. `stop` is checked at every edge and every
/// `STOP_CHECK_EXPANSIONS` expansions; when it fires, the whole set is discarded.
pub fn route_channel_rounds(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    ends: &[ChannelEnd],
    margin: f64,
    track: f64,
    crossing: f64,
    strict: bool,
    selection: Selection,
    counters: &mut SearchCounters,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Vec<Route>>, Stopped> {
    Ok(route_channels_raw(
        graph,
        rects,
        ends,
        margin,
        track,
        crossing,
        &[],
        None,
        strict,
        selection,
        counters,
        stop,
    )?
    .into_iter()
    .map(simplify)
    .collect())
}

/// `crossing` is the cost of a step onto a grid point that other routes already pass through (zero keeps the
/// congestion-only search). Crossings and touches of routes are discouraged by this cost; the layout decides whether to use it.
#[allow(clippy::too_many_arguments)]
fn route_channels_raw(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    ends: &[ChannelEnd],
    margin: f64,
    track: f64,
    crossing: f64,
    fixed: &[Route],
    active: Option<&BTreeSet<String>>,
    strict: bool,
    selection: Selection,
    counters: &mut SearchCounters,
    stop: &dyn Fn() -> bool,
) -> Result<Vec<Vec<Route>>, Stopped> {
    // Re-routing a crossing route only helps when stepping onto a crossing costs something; without a crossing cost
    // the search returns the same path. The extended selection therefore charges crossings.
    let crossing = match selection {
        Selection::Extended => crossing.max(EXTENDED_CROSSING_COST),
        Selection::Shared => crossing,
    };
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let real: Vec<Rect> = rects
        .iter()
        .filter(|(id, _)| !containers.contains(id.as_str()))
        .map(|(_, r)| r.clone())
        .collect();
    let blocks: Vec<Rect> = real
        .iter()
        .map(|r| Rect {
            x: r.x - margin,
            y: r.y - margin,
            width: r.width + 2. * margin,
            height: r.height + 2. * margin,
        })
        .collect();
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for b in &blocks {
        xs.extend([b.x, b.x + b.width, b.x - track, b.x + b.width + track]);
        ys.extend([b.y, b.y + b.height, b.y - track, b.y + b.height + track]);
    }
    // Container borders are track lines too: a route that must pass around a container needs the corridor outside it,
    // and without these lines the grid has no such corridor (the container itself is not an obstacle).
    for id in &containers {
        if let Some(r) = rects.get(*id) {
            xs.extend([r.x, r.x + r.width, r.x - track, r.x + r.width + track]);
            ys.extend([r.y, r.y + r.height, r.y - track, r.y + r.height + track]);
        }
    }
    for end in ends {
        for p in [&end.start, &end.depart, &end.arrive, &end.end] {
            xs.push(p.x);
            ys.push(p.y);
        }
    }
    // Fixed routes contribute their corners, so each of their segments lies exactly on grid lines.
    for route in fixed {
        for p in &route.points {
            xs.push(p.x);
            ys.push(p.y);
        }
    }
    let xs = unique(xs);
    let ys = unique(ys);
    let mut grid = Grid {
        strict_margins: strict,
        xs,
        ys,
        blocked_h: Vec::new(),
        blocked_v: Vec::new(),
        margin_h: Vec::new(),
        margin_v: Vec::new(),
        containers_at: Vec::new(),
        bands_at: Vec::new(),
    };
    // A segment is blocked when it passes through the interior of a margin box.
    let blocked_h = |x0: f64, x1: f64, y: f64| {
        blocks.iter().any(|b| {
            y > b.y + EPS && y < b.y + b.height - EPS && x1 > b.x + EPS && x0 < b.x + b.width - EPS
        })
    };
    let blocked_v = |x: f64, y0: f64, y1: f64| {
        blocks.iter().any(|b| {
            x > b.x + EPS && x < b.x + b.width - EPS && y1 > b.y + EPS && y0 < b.y + b.height - EPS
        })
    };
    let (nx, ny) = (grid.nx(), grid.ny());
    let real_h = |x0: f64, x1: f64, y: f64| {
        real.iter().any(|b| {
            y > b.y + EPS && y < b.y + b.height - EPS && x1 > b.x + EPS && x0 < b.x + b.width - EPS
        })
    };
    let real_v = |x: f64, y0: f64, y1: f64| {
        real.iter().any(|b| {
            x > b.x + EPS && x < b.x + b.width - EPS && y1 > b.y + EPS && y0 < b.y + b.height - EPS
        })
    };
    grid.blocked_h = (0..nx.saturating_sub(1) * ny)
        .map(|k| {
            let (i, j) = (k / ny, k % ny);
            real_h(grid.xs[i], grid.xs[i + 1], grid.ys[j])
        })
        .collect();
    grid.blocked_v = (0..nx * ny.saturating_sub(1))
        .map(|k| {
            let (i, j) = (k / (ny - 1), k % (ny - 1));
            real_v(grid.xs[i], grid.ys[j], grid.ys[j + 1])
        })
        .collect();
    grid.margin_h = (0..nx.saturating_sub(1) * ny)
        .map(|k| {
            let (i, j) = (k / ny, k % ny);
            blocked_h(grid.xs[i], grid.xs[i + 1], grid.ys[j])
        })
        .collect();
    grid.margin_v = (0..nx * ny.saturating_sub(1))
        .map(|k| {
            let (i, j) = (k / (ny - 1), k % (ny - 1));
            blocked_v(grid.xs[i], grid.ys[j], grid.ys[j + 1])
        })
        .collect();

    // Containers and their parents: which containers each edge owns, and which segments lie in foreign interiors.
    let container_ids: Vec<&str> = containers.iter().copied().collect();
    let container_index: BTreeMap<&str, usize> = container_ids
        .iter()
        .enumerate()
        .map(|(k, id)| (*id, k))
        .collect();
    let container_rects: Vec<&Rect> = container_ids
        .iter()
        .filter_map(|id| rects.get(*id))
        .collect();
    let parent_of: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref().map(|p| (n.id.as_str(), p)))
        .collect();
    let inside = |p: (f64, f64), r: &Rect| {
        p.0 > r.x + EPS
            && p.0 < r.x + r.width - EPS
            && p.1 > r.y + EPS
            && p.1 < r.y + r.height - EPS
    };
    let (hcount, vcount) = (nx.saturating_sub(1) * ny, nx * ny.saturating_sub(1));
    let mids: Vec<(f64, f64)> = (0..hcount + vcount)
        .map(|k| {
            if k < hcount {
                let (x, j) = (k / ny, k % ny);
                ((grid.xs[x] + grid.xs[x + 1]) / 2., grid.ys[j])
            } else {
                let v = k - hcount;
                let (i, y) = (v / (ny - 1), v % (ny - 1));
                (grid.xs[i], (grid.ys[y] + grid.ys[y + 1]) / 2.)
            }
        })
        .collect();
    grid.containers_at = mids
        .iter()
        .map(|&mid| {
            container_rects
                .iter()
                .enumerate()
                .filter(|(_, r)| inside(mid, r))
                .map(|(c, _)| c)
                .collect()
        })
        .collect();
    // Node bands, in the order of `blocks` (real nodes, as built above). Containers have no band: they are corridors.
    let band_ids: Vec<&str> = rects
        .keys()
        .filter(|id| !containers.contains(id.as_str()))
        .map(|id| id.as_str())
        .collect();
    grid.bands_at = mids
        .iter()
        .map(|&mid| {
            blocks
                .iter()
                .enumerate()
                .filter(|(_, b)| inside(mid, b))
                .map(|(k, _)| k)
                .collect()
        })
        .collect();

    let mut order: Vec<usize> = (0..ends.len())
        .filter(|k| active.is_none_or(|set| set.contains(&ends[*k].edge_id)))
        .collect();
    order.sort_by(|a, b| ends[*a].edge_id.cmp(&ends[*b].edge_id));
    let mut usage = vec![0usize; grid.segments()];
    // Interior grid points passed by routes, indexed like the search states (point index i * ny + j).
    let mut through = vec![0usize; grid.nx() * grid.ny()];
    mark_fixed(&grid, fixed, &mut usage, &mut through);
    let mut paths: Vec<Option<Vec<(usize, usize)>>> = vec![None; ends.len()];
    let mut scratch = SearchScratch::new(grid.nx() * grid.ny() * 5);
    let mut rounds: Vec<Vec<Route>> = Vec::new();
    for round in 0..ROUNDS {
        // With fixed routes, round 0 already charges shared segments, so the new routes keep clear of them.
        let penalty = if round == 0 {
            if fixed.is_empty() {
                0.
            } else {
                1.
            }
        } else {
            (1u64 << (round - 1)) as f64
        };
        for &k in &order {
            if stop() {
                return Err(Stopped);
            }
            if round >= LOCAL_FROM {
                let conflicted = paths[k].as_ref().is_some_and(|path| {
                    let shared = path
                        .windows(2)
                        .any(|w| usage[segment_between(&grid, w[0], w[1])] > 1);
                    if shared || selection == Selection::Shared {
                        return shared;
                    }
                    // Extended: a route that crosses another one (its interior point is passed by another route too,
                    // counted in `through` before this route is ripped up) or hugs a node margin.
                    let crosses = interior(path)
                        .iter()
                        .any(|&(i, j)| through[i * grid.ny() + j] > 1);
                    let hugs = path
                        .windows(2)
                        .any(|w| on_margin(&grid, segment_between(&grid, w[0], w[1])));
                    crosses || hugs
                });
                if !conflicted {
                    continue;
                }
            }
            if let Some(path) = paths[k].take() {
                for pair in path.windows(2) {
                    let seg = segment_between(&grid, pair[0], pair[1]);
                    usage[seg] -= 1;
                }
                for &(i, j) in interior(&path) {
                    through[i * grid.ny() + j] -= 1;
                }
            }
            let end = &ends[k];
            let from = (
                grid.index(&grid.xs, end.depart.x),
                grid.index(&grid.ys, end.depart.y),
            );
            let to = (
                grid.index(&grid.xs, end.arrive.x),
                grid.index(&grid.ys, end.arrive.y),
            );
            let mut owned = BTreeSet::new();
            for node in [end.source.as_str(), end.target.as_str()] {
                let mut current = Some(node);
                while let Some(id) = current {
                    if let Some(&c) = container_index.get(id) {
                        owned.insert(c);
                    }
                    current = parent_of.get(id).copied();
                }
            }
            let own_bands: BTreeSet<usize> = band_ids
                .iter()
                .enumerate()
                .filter(|(_, id)| **id == end.source.as_str() || **id == end.target.as_str())
                .map(|(b, _)| b)
                .collect();
            let path = match (from, to) {
                ((Some(fi), Some(fj)), (Some(ti), Some(tj))) => shortest_with_scratch(
                    &grid,
                    (fi, fj),
                    (ti, tj),
                    &usage,
                    &through,
                    crossing,
                    penalty,
                    true,
                    &owned,
                    &own_bands,
                    &mut scratch,
                    counters,
                    stop,
                )?,
                _ => None,
            };
            if let Some(path) = &path {
                for pair in path.windows(2) {
                    let seg = segment_between(&grid, pair[0], pair[1]);
                    usage[seg] += 1;
                }
                for &(i, j) in interior(path) {
                    through[i * grid.ny() + j] += 1;
                }
            }
            paths[k] = path;
        }
        rounds.push(routes_of(&grid, ends, &paths));
    }
    Ok(rounds)
}

/// Occupies the grid segments and interior points that the fixed routes pass through. Corners lie on grid lines
/// (the grid was built from them), so every axis-aligned piece maps onto consecutive grid steps.
fn mark_fixed(grid: &Grid, fixed: &[Route], usage: &mut [usize], through: &mut [usize]) {
    let ny = grid.ny();
    for route in fixed {
        for pair in route.points.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if (a.y - b.y).abs() < EPS {
                let (Some(j), Some(i0), Some(i1)) = (
                    grid.index(&grid.ys, a.y),
                    grid.index(&grid.xs, a.x.min(b.x)),
                    grid.index(&grid.xs, a.x.max(b.x)),
                ) else {
                    continue;
                };
                for i in i0..i1 {
                    usage[segment_between(grid, (i, j), (i + 1, j))] += 1;
                }
                for i in i0 + 1..i1 {
                    through[i * ny + j] += 1;
                }
            } else if (a.x - b.x).abs() < EPS {
                let (Some(i), Some(j0), Some(j1)) = (
                    grid.index(&grid.xs, a.x),
                    grid.index(&grid.ys, a.y.min(b.y)),
                    grid.index(&grid.ys, a.y.max(b.y)),
                ) else {
                    continue;
                };
                for j in j0..j1 {
                    usage[segment_between(grid, (i, j), (i, j + 1))] += 1;
                }
                for j in j0 + 1..j1 {
                    through[i * ny + j] += 1;
                }
            }
        }
    }
}

/// The routes of one round: the grid path of each edge, framed by its stubs.
fn routes_of(
    grid: &Grid,
    ends: &[ChannelEnd],
    paths: &[Option<Vec<(usize, usize)>>],
) -> Vec<Route> {
    ends.iter()
        .enumerate()
        .map(|(k, end)| {
            let mut points = vec![end.start.clone(), end.depart.clone()];
            if let Some(path) = &paths[k] {
                for &(i, j) in path {
                    points.push(Point {
                        x: grid.xs[i],
                        y: grid.ys[j],
                    });
                }
            } else {
                // Unroutable with the current margins: an orthogonal connection from the departure stub to the arrival
                // stub through one corner (vertical first). It may cross nodes; that is the conflict to report.
                points.push(Point {
                    x: end.depart.x,
                    y: end.arrive.y,
                });
            }
            points.push(end.arrive.clone());
            points.push(end.end.clone());
            Route::new(end.edge_id.clone(), without_collinear(points))
        })
        .collect()
}

/// The points of a path without its two ends (empty for paths of at most two points).
fn interior(path: &[(usize, usize)]) -> &[(usize, usize)] {
    if path.len() <= 2 {
        &[]
    } else {
        &path[1..path.len() - 1]
    }
}

/// True when the segment lies in a node's clearance band (hugging).
fn on_margin(grid: &Grid, seg: usize) -> bool {
    let horizontal = grid.nx().saturating_sub(1) * grid.ny();
    if seg < horizontal {
        grid.margin_h[seg]
    } else {
        grid.margin_v[seg - horizontal]
    }
}

fn segment_between(grid: &Grid, a: (usize, usize), b: (usize, usize)) -> usize {
    if a.1 == b.1 {
        let i = a.0.min(b.0);
        grid.segment(i + 1, a.1, 1)
    } else {
        let j = a.1.min(b.1);
        grid.segment(a.0, j + 1, 3)
    }
}

/// Shortest path over (node, direction) states. Returns the node sequence from `from` to `to`.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn shortest(
    grid: &Grid,
    from: (usize, usize),
    to: (usize, usize),
    usage: &[usize],
    through: &[usize],
    crossing: f64,
    penalty: f64,
    owned: &BTreeSet<usize>,
    own_bands: &BTreeSet<usize>,
    counters: &mut SearchCounters,
    stop: &dyn Fn() -> bool,
) -> Result<Option<Vec<(usize, usize)>>, Stopped> {
    shortest_with_heuristic(
        grid, from, to, usage, through, crossing, penalty, true, owned, own_bands, counters, stop,
    )
}

/// `Err(Stopped)` when `stop` fires at a checkpoint; `Ok(None)` when no path exists. Records the search in `counters`.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn shortest_with_heuristic(
    grid: &Grid,
    from: (usize, usize),
    to: (usize, usize),
    usage: &[usize],
    through: &[usize],
    crossing: f64,
    penalty: f64,
    use_heuristic: bool,
    owned: &BTreeSet<usize>,
    own_bands: &BTreeSet<usize>,
    counters: &mut SearchCounters,
    stop: &dyn Fn() -> bool,
) -> Result<Option<Vec<(usize, usize)>>, Stopped> {
    let mut scratch = SearchScratch::new(grid.nx() * grid.ny() * 5);
    shortest_with_scratch(
        grid,
        from,
        to,
        usage,
        through,
        crossing,
        penalty,
        use_heuristic,
        owned,
        own_bands,
        &mut scratch,
        counters,
        stop,
    )
}

#[allow(clippy::too_many_arguments)]
fn shortest_with_scratch(
    grid: &Grid,
    from: (usize, usize),
    to: (usize, usize),
    usage: &[usize],
    through: &[usize],
    crossing: f64,
    penalty: f64,
    use_heuristic: bool,
    owned: &BTreeSet<usize>,
    own_bands: &BTreeSet<usize>,
    scratch: &mut SearchScratch,
    counters: &mut SearchCounters,
    stop: &dyn Fn() -> bool,
) -> Result<Option<Vec<(usize, usize)>>, Stopped> {
    let before = counters.expansions;
    let path = shortest_search(
        grid,
        from,
        to,
        usage,
        through,
        crossing,
        penalty,
        use_heuristic,
        owned,
        own_bands,
        scratch,
        counters,
        stop,
    )?;
    counters.searches += 1;
    counters.largest_search = counters.largest_search.max(counters.expansions - before);
    Ok(path)
}

#[allow(clippy::too_many_arguments)]
fn shortest_search(
    grid: &Grid,
    from: (usize, usize),
    to: (usize, usize),
    usage: &[usize],
    through: &[usize],
    crossing: f64,
    penalty: f64,
    use_heuristic: bool,
    owned: &BTreeSet<usize>,
    own_bands: &BTreeSet<usize>,
    scratch: &mut SearchScratch,
    counters: &mut SearchCounters,
    stop: &dyn Fn() -> bool,
) -> Result<Option<Vec<(usize, usize)>>, Stopped> {
    // Manhattan distance is a lower bound: congestion, margins and bends only add cost.
    let heuristic = |i: usize, j: usize| {
        if use_heuristic {
            (grid.xs[i] - grid.xs[to.0]).abs() + (grid.ys[j] - grid.ys[to.1]).abs()
        } else {
            0.
        }
    };
    let (nx, ny) = (grid.nx(), grid.ny());
    let state = |i: usize, j: usize, d: usize| (i * ny + j) * 5 + d;
    scratch.reset();
    let start = state(from.0, from.1, START);
    scratch.set(start, 0., usize::MAX);
    scratch.heap.push(Reverse((
        heuristic(from.0, from.1).to_bits(),
        0f64.to_bits(),
        start,
    )));
    let mut goal = None;
    while let Some(Reverse((_, bits, s))) = scratch.heap.pop() {
        let d = f64::from_bits(bits);
        if d > scratch.dist[s] {
            continue;
        }
        counters.expansions += 1;
        if counters.expansions % STOP_CHECK_EXPANSIONS == 0 && stop() {
            return Err(Stopped);
        }
        let (node, dir) = (s / 5, s % 5);
        let (i, j) = (node / ny, node % ny);
        if (i, j) == to {
            goal = Some(s);
            break;
        }
        for next_dir in 0..4 {
            if dir < 4 && OPPOSITE[dir] == next_dir {
                continue;
            }
            let (ni, nj) = match next_dir {
                0 if i + 1 < nx => (i + 1, j),
                1 if i > 0 => (i - 1, j),
                2 if j + 1 < ny => (i, j + 1),
                3 if j > 0 => (i, j - 1),
                _ => continue,
            };
            let seg = if next_dir < 2 {
                let x = i.min(ni);
                if grid.blocked_h[x * ny + j] {
                    continue;
                }
                grid.segment(x + 1, j, 1)
            } else {
                let y = j.min(nj);
                if grid.blocked_v[i * (ny - 1) + y] {
                    continue;
                }
                grid.segment(i, y + 1, 3)
            };
            if grid.containers_at[seg].iter().any(|c| !owned.contains(c)) {
                continue;
            }
            let length = (grid.xs[ni] - grid.xs[i]).abs() + (grid.ys[nj] - grid.ys[j]).abs();
            let bend = if dir < 4 && dir != next_dir {
                BEND_PENALTY
            } else {
                0.
            };
            let margin = if next_dir < 2 {
                grid.margin_h[i.min(ni) * ny + j]
            } else {
                grid.margin_v[i * (ny - 1) + j.min(nj)]
            };
            // A margin of a node the edge owns is usable only where the route attaches to its ports: the segments at the
            // search's start and goal. Further along, the own node keeps its clearance too (ADR 0005).
            let attaches = [from, to]
                .iter()
                .any(|end| (i, j) == *end || (ni, nj) == *end);
            if margin
                && grid.strict_margins
                && grid.bands_at[seg]
                    .iter()
                    .any(|b| !own_bands.contains(b) || !attaches)
            {
                continue;
            }
            let factor = if margin { MARGIN_FACTOR } else { 1. };
            let cost = length * factor * (1. + penalty * usage[seg] as f64)
                + bend
                + crossing * through[ni * ny + nj] as f64;
            let ns = state(ni, nj, next_dir);
            if d + cost < scratch.dist[ns] {
                scratch.set(ns, d + cost, s);
                scratch.heap.push(Reverse((
                    (d + cost + heuristic(ni, nj)).to_bits(),
                    (d + cost).to_bits(),
                    ns,
                )));
            }
        }
    }
    let Some(mut s) = goal else {
        return Ok(None);
    };
    let mut nodes = vec![(s / 5 / ny, s / 5 % ny)];
    while scratch.prev[s] != usize::MAX {
        s = scratch.prev[s];
        nodes.push((s / 5 / ny, s / 5 % ny));
    }
    nodes.reverse();
    nodes.dedup();
    Ok(Some(nodes))
}

/// Removes points that lie on the straight line between their neighbours.
fn without_collinear(mut points: Vec<Point>) -> Vec<Point> {
    points.dedup_by(|a, b| (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS);
    let mut i = 1;
    while i + 1 < points.len() {
        let (a, b, c) = (&points[i - 1], &points[i], &points[i + 1]);
        let straight = ((a.x - b.x).abs() < EPS && (b.x - c.x).abs() < EPS)
            || ((a.y - b.y).abs() < EPS && (b.y - c.y).abs() < EPS);
        if straight {
            points.remove(i);
        } else {
            i += 1;
        }
    }
    points
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn binary_grid_lookup_matches_the_linear_tolerance_rule() {
        let values = vec![0., 1.1 * EPS, 2.2 * EPS, 4.];
        let grid = Grid {
            strict_margins: false,
            xs: values.clone(),
            ys: Vec::new(),
            blocked_h: Vec::new(),
            blocked_v: Vec::new(),
            margin_h: Vec::new(),
            margin_v: Vec::new(),
            containers_at: Vec::new(),
            bands_at: Vec::new(),
        };
        for value in [
            -EPS,
            0.,
            0.9 * EPS,
            1.1 * EPS,
            1.7 * EPS,
            2.2 * EPS,
            3.1 * EPS,
            4.,
            4. + EPS,
        ] {
            assert_eq!(
                grid.index(&values, value),
                values.iter().position(|v| (*v - value).abs() < EPS),
                "value {value}"
            );
        }
    }

    #[test]
    fn reusable_scratch_matches_fresh_searches() {
        let grid = Grid {
            strict_margins: false,
            xs: vec![0., 10., 20.],
            ys: vec![0., 10., 20.],
            blocked_h: vec![false; 6],
            blocked_v: vec![false; 6],
            margin_h: vec![false; 6],
            margin_v: vec![false; 6],
            containers_at: vec![Vec::new(); 12],
            bands_at: vec![Vec::new(); 12],
        };
        let usage = vec![0; grid.segments()];
        let through = vec![0; grid.nx() * grid.ny()];
        let owned = BTreeSet::new();
        let own_bands = BTreeSet::new();
        let mut scratch = SearchScratch::new(grid.nx() * grid.ny() * 5);
        for (from, to) in [((0, 0), (2, 2)), ((2, 0), (0, 2)), ((1, 2), (1, 0))] {
            let reused = shortest_with_scratch(
                &grid,
                from,
                to,
                &usage,
                &through,
                0.,
                0.,
                true,
                &owned,
                &own_bands,
                &mut scratch,
                &mut SearchCounters::default(),
                &|| false,
            )
            .unwrap();
            let fresh = shortest(
                &grid,
                from,
                to,
                &usage,
                &through,
                0.,
                0.,
                &owned,
                &own_bands,
                &mut SearchCounters::default(),
                &|| false,
            )
            .unwrap();
            assert_eq!(reused, fresh);
        }
    }

    #[test]
    fn astar_matches_dijkstra_cost_with_obstacles_and_congestion() {
        let mut grid = Grid {
            strict_margins: false,
            xs: vec![0., 10., 20., 30.],
            ys: vec![0., 7., 14., 21.],
            blocked_h: vec![false; 12],
            blocked_v: vec![false; 12],
            margin_h: vec![false; 12],
            margin_v: vec![false; 12],
            containers_at: vec![Vec::new(); 24],
            bands_at: Vec::new(),
        };
        grid.blocked_h[5] = true;
        grid.margin_v[4] = true;
        let usage: Vec<_> = (0..grid.segments()).map(|i| i % 3).collect();
        let score = |path: &Vec<(usize, usize)>| {
            let mut cost = 0.;
            let mut last = None;
            for pair in path.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let horizontal = a.1 == b.1;
                let margin = if horizontal {
                    grid.margin_h[a.0.min(b.0) * 4 + a.1]
                } else {
                    grid.margin_v[a.0 * 3 + a.1.min(b.1)]
                };
                let length =
                    (grid.xs[a.0] - grid.xs[b.0]).abs() + (grid.ys[a.1] - grid.ys[b.1]).abs();
                cost += length
                    * if margin { MARGIN_FACTOR } else { 1. }
                    * (1. + 2. * usage[segment_between(&grid, a, b)] as f64);
                if last.is_some_and(|v| v != horizontal) {
                    cost += BEND_PENALTY;
                }
                last = Some(horizontal);
            }
            cost
        };
        for a in 0..16 {
            for b in 0..16 {
                let from = (a / 4, a % 4);
                let to = (b / 4, b % 4);
                let fast = shortest_with_heuristic(
                    &grid,
                    from,
                    to,
                    &usage,
                    &vec![0; grid.nx() * grid.ny()],
                    0.,
                    2.,
                    true,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    &mut SearchCounters::default(),
                    &|| false,
                )
                .unwrap()
                .unwrap();
                let reference = shortest_with_heuristic(
                    &grid,
                    from,
                    to,
                    &usage,
                    &vec![0; grid.nx() * grid.ny()],
                    0.,
                    2.,
                    false,
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    &mut SearchCounters::default(),
                    &|| false,
                )
                .unwrap()
                .unwrap();
                assert!((score(&fast) - score(&reference)).abs() < EPS);
            }
        }
        grid.blocked_h.fill(true);
        grid.blocked_v.fill(true);
        assert!(shortest(
            &grid,
            (0, 0),
            (3, 3),
            &usage,
            &vec![0; grid.nx() * grid.ny()],
            0.,
            2.,
            &BTreeSet::new(),
            &BTreeSet::new(),
            &mut SearchCounters::default(),
            &|| false,
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn a_fired_stop_signal_interrupts_the_search_and_never_yields_a_path() {
        // Without a heuristic the search expands well over one checkpoint interval before it reaches the far corner.
        let n = 30;
        let grid = Grid {
            strict_margins: false,
            xs: (0..n).map(|i| i as f64 * 10.).collect(),
            ys: (0..n).map(|i| i as f64 * 10.).collect(),
            blocked_h: vec![false; (n - 1) * n],
            blocked_v: vec![false; n * (n - 1)],
            margin_h: vec![false; (n - 1) * n],
            margin_v: vec![false; n * (n - 1)],
            containers_at: vec![Vec::new(); 2 * n * n - 2 * n],
            bands_at: Vec::new(),
        };
        let usage = vec![0; grid.segments()];
        let through = vec![0; grid.nx() * grid.ny()];
        let search = |stop: &dyn Fn() -> bool| {
            shortest_with_heuristic(
                &grid,
                (0, 0),
                (n - 1, n - 1),
                &usage,
                &through,
                0.,
                0.,
                false,
                &BTreeSet::new(),
                &BTreeSet::new(),
                &mut SearchCounters::default(),
                stop,
            )
        };
        assert_eq!(search(&|| true), Err(Stopped));
        assert!(search(&|| false).unwrap().is_some());
    }

    #[test]
    fn strict_clearance_keeps_a_route_out_of_a_foreign_band() {
        // Wall `f` sits between `a` and `b`. Its clearance band (margin 10) covers the direct corridor, so the strict
        // search must go around the band. No drawn segment may run through the band interior.
        let graph = Graph {
            default_direction: Default::default(),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: Vec::new(),
            edge_ports: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            buses: Vec::new(),
        };
        let rect = |x: f64, y: f64, width: f64, height: f64| Rect {
            x,
            y,
            width,
            height,
        };
        let rects = BTreeMap::from([
            ("a".to_string(), rect(0., 0., 20., 20.)),
            ("b".to_string(), rect(100., 0., 20., 20.)),
            ("f".to_string(), rect(40., -5., 20., 30.)),
        ]);
        let ends = vec![ChannelEnd {
            edge_id: "e".to_string(),
            source: "a".to_string(),
            target: "b".to_string(),
            start: Point { x: 20., y: 10. },
            depart: Point { x: 22., y: 10. },
            arrive: Point { x: 98., y: 10. },
            end: Point { x: 100., y: 10. },
        }];
        // Band of `f`: x in (30, 70), y in (-15, 35).
        let inside_band = |a: &Point, b: &Point| {
            let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
            let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
            let overlap = x1.min(70.) - x0.max(30.);
            overlap > EPS && y0 > -15. + EPS && y1 < 35. - EPS
        };
        let rounds = route_channel_rounds(
            &graph,
            &rects,
            &ends,
            10.,
            8.,
            0.,
            true,
            Selection::Shared,
            &mut SearchCounters::default(),
            &|| false,
        )
        .unwrap();
        let routes = rounds.last().unwrap();
        assert_eq!(routes.len(), 1, "the edge is routed around the band");
        for w in routes[0].points.windows(2) {
            assert!(
                (w[0].x - w[1].x).abs() < EPS || (w[0].y - w[1].y).abs() < EPS,
                "the route is orthogonal"
            );
            assert!(
                !inside_band(&w[0], &w[1]),
                "segment {:?} -> {:?} runs through the clearance band",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn an_unroutable_edge_is_drawn_orthogonally() {
        let grid = Grid {
            strict_margins: false,
            xs: Vec::new(),
            ys: Vec::new(),
            blocked_h: Vec::new(),
            blocked_v: Vec::new(),
            margin_h: Vec::new(),
            margin_v: Vec::new(),
            containers_at: Vec::new(),
            bands_at: Vec::new(),
        };
        let end = ChannelEnd {
            edge_id: "e".into(),
            source: "a".into(),
            target: "b".into(),
            start: Point { x: 0., y: 0. },
            depart: Point { x: 20., y: 0. },
            arrive: Point { x: 120., y: 60. },
            end: Point { x: 100., y: 60. },
        };
        let routes = routes_of(&grid, &[end], &[None]);
        let points = &routes[0].points;
        for pair in points.windows(2) {
            assert!(
                (pair[0].x - pair[1].x).abs() < EPS || (pair[0].y - pair[1].y).abs() < EPS,
                "segment {:?} -> {:?} is not orthogonal",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn a_non_axis_aligned_fixed_route_is_refused_not_placed() {
        let graph = Graph {
            default_direction: Default::default(),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: Vec::new(),
            edge_ports: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            buses: Vec::new(),
        };
        let rects = BTreeMap::from([(
            "a".to_string(),
            Rect {
                x: 0.,
                y: 0.,
                width: 10.,
                height: 10.,
            },
        )]);
        let diagonal = Route::new("e", vec![Point { x: 0., y: 0. }, Point { x: 5., y: 5. }]);
        let refused = route_active(
            &graph,
            &rects,
            &[],
            5.,
            8.,
            &[diagonal],
            &BTreeSet::new(),
            false,
            &mut SearchCounters::default(),
            &|| false,
        )
        .unwrap();
        assert!(refused.is_none());
    }
}
