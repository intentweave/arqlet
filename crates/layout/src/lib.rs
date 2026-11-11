//! Deterministic prototype: nested layered placement and simple orthogonal routes.
//! Bounded ordering and routing heuristics with configurable endpoint ports.
pub mod abstract_routing;
mod block_placement;
mod border_alignment;
mod bus;
mod bus_layout;
pub mod constructive;
mod engine_choice;
mod global;
pub mod hierarchy_order;
mod labels;
pub mod layered_plan;
mod ports;
pub mod ranking;
pub mod structure;
mod styles;
use arqlet_model::*;
mod directed;
mod organic;
pub mod overview;
mod placement_geometry;
mod placement_order;
mod placement_region;
mod placement_structure;
mod repair;
mod spatial;
mod tree;
mod vpsc;
use arqlet_routing::channel::Selection;
/// The channel router's search work, as reported by the layout functions that route.
pub use arqlet_routing::channel::{AbstractCompletionTimes, ConstructionCounters, SearchCounters};
use std::collections::{BTreeMap, BTreeSet};

// Wall-clock reads are available only in the native benchmark build; the default core and WASM builds stay clock-free.
#[cfg(all(feature = "benchmark-timings", not(target_arch = "wasm32")))]
pub(crate) fn stage_timer_start() -> std::time::Instant {
    std::time::Instant::now()
}

#[cfg(not(all(feature = "benchmark-timings", not(target_arch = "wasm32"))))]
pub(crate) fn stage_timer_start() {}

#[cfg(all(feature = "benchmark-timings", not(target_arch = "wasm32")))]
pub(crate) fn elapsed_nanos(start: std::time::Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(not(all(feature = "benchmark-timings", not(target_arch = "wasm32"))))]
pub(crate) fn elapsed_nanos(_: ()) -> u64 {
    0
}

/// Why a candidate construction ended without a snapshot: the stop signal fired, or the construction failed.
#[derive(Debug)]
enum Abort {
    Stopped,
    Failed(String),
}

impl From<String> for Abort {
    fn from(message: String) -> Self {
        Abort::Failed(message)
    }
}

impl From<&str> for Abort {
    fn from(message: &str) -> Self {
        Abort::Failed(message.into())
    }
}

impl From<arqlet_routing::channel::Stopped> for Abort {
    fn from(_: arqlet_routing::channel::Stopped) -> Self {
        Abort::Stopped
    }
}

/// One candidate construction: the stop signal it checks and the channel-router work it accumulates.
struct Run<'a> {
    stop: &'a dyn Fn() -> bool,
    search: arqlet_routing::channel::SearchCounters,
    /// Experimental (`layeredConstruction`): the derived layout graphs of the layered areas the placement arranged.
    plans: layered_plan::LayeredPlans,
    /// Experimental: the stored plans of the drawing a pass without an earlier snapshot refines (see `candidate_from`).
    earlier: BTreeMap<String, arqlet_model::StoredAreaPlan>,
    /// Experimental (consistent layered pipeline): the channels the last construction found too narrow for their tracks
    /// (`constructive::Construction::narrow_channels`).
    narrow: Vec<(String, usize, f64)>,
    capacity_seeds: Vec<CapacitySeed>,
    /// Provisional (consistent layered pipeline): the preparation of the last track count, which found no channel too
    /// narrow, so that the routing of the same drawing starts from it. The next `route_all` takes it, whichever engine it
    /// uses; the construction checks that boxes and ports are those it was computed on.
    prepared: Option<constructive::Preparation>,
    #[cfg(test)]
    disable_phase_reuse: bool,
    #[cfg(test)]
    disable_coordinate_reuse: bool,
    #[cfg(test)]
    disable_structure_reuse: bool,
    #[cfg(test)]
    disable_region_reuse: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CapacitySeed {
    pub(crate) area: String,
    pub(crate) rank: usize,
    pub(crate) physical_tracks: usize,
    pub(crate) seeded_width: f64,
    pub(crate) initial_available: f64,
}

/// Pass-local graph lookups in stable ID order. The canonical graph retains its input order.
struct GraphIndex<'a> {
    nodes: Vec<&'a Node>,
    by_id: BTreeMap<&'a str, usize>,
    parents: Vec<Option<usize>>,
    roots: Vec<usize>,
    children: Vec<Vec<usize>>,
    adjacency: Vec<Vec<usize>>,
    edge_by_id: BTreeMap<&'a str, usize>,
    excluded: Vec<Vec<usize>>,
    /// The indices of every edge's source and target, in the graph's edge order (`None` for an unknown or missing end).
    edge_ends: Vec<(Option<usize>, Option<usize>)>,
}

impl<'a> GraphIndex<'a> {
    fn new(graph: &'a Graph) -> Self {
        let mut nodes: Vec<_> = graph.nodes.iter().collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        let by_id: BTreeMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| (node.id.as_str(), i))
            .collect();
        let parents: Vec<Option<usize>> = nodes
            .iter()
            .map(|node| {
                node.parent_id
                    .as_deref()
                    .and_then(|parent| by_id.get(parent).copied())
            })
            .collect();
        let mut roots = Vec::new();
        let mut children = vec![Vec::new(); nodes.len()];
        for (child, parent) in parents.iter().enumerate() {
            if let Some(parent) = parent {
                children[*parent].push(child);
            } else {
                roots.push(child);
            }
        }
        let mut adjacency = vec![Vec::new(); nodes.len()];
        let mut edge_by_id = BTreeMap::new();
        let mut excluded = Vec::with_capacity(graph.edges.len());
        let edge_ends = graph
            .edges
            .iter()
            .map(|edge| {
                (
                    by_id.get(edge.source.as_str()).copied(),
                    edge.target.as_deref().and_then(|t| by_id.get(t).copied()),
                )
            })
            .collect();
        for (edge_index, edge) in graph.edges.iter().enumerate() {
            edge_by_id.insert(edge.id.as_str(), edge_index);
            if let Some(target) = edge.target.as_deref() {
                adjacency[by_id[edge.source.as_str()]].push(by_id[target]);
            }
            let mut ids = BTreeSet::new();
            for endpoint in [Some(edge.source.as_str()), edge.target.as_deref()] {
                let mut current = endpoint.and_then(|id| by_id.get(id).copied());
                while let Some(node) = current {
                    ids.insert(node);
                    current = parents[node];
                }
            }
            excluded.push(ids.into_iter().collect());
        }
        for targets in &mut adjacency {
            targets.sort_unstable();
            targets.dedup();
        }
        Self {
            nodes,
            by_id,
            parents,
            roots,
            children,
            adjacency,
            edge_by_id,
            excluded,
            edge_ends,
        }
    }

    fn index_of(&self, id: &str) -> Option<usize> {
        self.by_id.get(id).copied()
    }

    fn node(&self, id: &str) -> Option<&'a Node> {
        self.index_of(id).map(|i| self.nodes[i])
    }

    fn children_of(&self, parent: Option<&str>) -> &[usize] {
        match parent {
            Some(id) => self
                .index_of(id)
                .map_or(&[], |index| self.children[index].as_slice()),
            None => &self.roots,
        }
    }

    fn excluded_for_edge(&self, edge_id: &str) -> Option<&[usize]> {
        self.edge_by_id
            .get(edge_id)
            .and_then(|index| self.excluded.get(*index))
            .map(Vec::as_slice)
    }
}

impl<'a> Run<'a> {
    fn new(stop: &'a dyn Fn() -> bool) -> Self {
        Run {
            stop,
            search: Default::default(),
            plans: Default::default(),
            earlier: Default::default(),
            narrow: Vec::new(),
            capacity_seeds: Vec::new(),
            prepared: None,
            #[cfg(test)]
            disable_phase_reuse: false,
            #[cfg(test)]
            disable_coordinate_reuse: false,
            #[cfg(test)]
            disable_structure_reuse: false,
            #[cfg(test)]
            disable_region_reuse: false,
        }
    }
}

/// The result of a construction started without a stop signal, which therefore cannot report `Stopped`.
fn uninterrupted<T>(result: Result<T, Abort>) -> Result<T, String> {
    match result {
        Ok(snapshot) => Ok(snapshot),
        Err(Abort::Failed(message)) => Err(message),
        Err(Abort::Stopped) => Err(UNINTERRUPTED.into()),
    }
}

/// Rows belong to leaf nodes of the graph; their IDs are unique within a node, and their sizes are finite and positive
/// (ADR 0011). Anything else is refused by name.
fn check_rows(graph: &Graph, options: &LayoutOptions) -> Result<(), String> {
    for (node, rows) in &options.node_rows {
        let Some(n) = graph.nodes.iter().find(|n| &n.id == node) else {
            return Err(format!("Rows name an unknown node: {node}"));
        };
        if graph
            .nodes
            .iter()
            .any(|c| c.parent_id.as_deref() == Some(node.as_str()))
        {
            return Err(format!(
                "Rows belong to a leaf node, and {} has children",
                n.id
            ));
        }
        let mut seen = BTreeSet::new();
        for row in rows {
            if row.id.is_empty() || !seen.insert(row.id.as_str()) {
                return Err(format!(
                    "Row IDs of {node} must be non-empty and unique: {}",
                    row.id
                ));
            }
            if ![row.width, row.height]
                .iter()
                .all(|v| v.is_finite() && *v > 0.)
            {
                return Err(format!(
                    "Row {} of {node} needs a finite positive width and height",
                    row.id
                ));
            }
        }
    }
    // A row port names a row its node has, and sits on the left or right side (ADR 0011).
    for port in &graph.ports {
        let Some(row) = &port.row else {
            continue;
        };
        let rows = options
            .node_rows
            .get(&port.node_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        if !rows.iter().any(|r| &r.id == row) {
            return Err(format!(
                "Port {} binds to row {row}, which {} does not have",
                port.id, port.node_id
            ));
        }
        if !matches!(port.side, PortSide::Left | PortSide::Right) {
            return Err(format!(
                "Port {} binds to row {row}: a row port sits on the left or right side",
                port.id
            ));
        }
    }
    Ok(())
}

pub fn compute(
    graph: &Graph,
    previous: Option<&Snapshot>,
    dirty: &BTreeSet<String>,
    options: &LayoutOptions,
) -> Result<Snapshot, String> {
    compute_counted(graph, previous, dirty, options).map(|(snapshot, _)| snapshot)
}

/// `compute` that also returns the stage work of the first layout (placement, routing, and so on), so that a caller can
/// report where the time of an import goes. The snapshot is the same as `compute` returns.
pub fn compute_counted(
    graph: &Graph,
    previous: Option<&Snapshot>,
    dirty: &BTreeSet<String>,
    options: &LayoutOptions,
) -> Result<(Snapshot, SearchCounters), String> {
    check_rows(graph, options)?;
    tree::check_tree_areas(graph)?;
    organic::check_organic_areas(graph, options.sparse_stress, options.scalable_stress)?;
    let mut run = Run::new(&|| false);
    let mut snapshot = uninterrupted(compute_mode(graph, previous, dirty, 0, options, &mut run))?;
    let stages = run.search.clone();
    // An edit that laid out and routed only part of the drawing must not leave it worse than it was. Routes that run into a node
    // are counted before and after. If the local result has more, the complete layout decides and is kept when it has fewer; a
    // step of a drag only reports it (a preview), because the step that ends the gesture decides.
    let used_local = run.search.routes_kept + run.search.routes_rerouted > 0;
    if let Some(earlier) = previous.filter(|_| used_local) {
        // Only edges the earlier drawing had are compared: the route of a new edge may be unresolved without the edit being worse.
        let had: BTreeSet<&str> = earlier.graph.edges.iter().map(|e| e.id.as_str()).collect();
        let hitting = |s: &Snapshot| {
            let routes: Vec<Route> = s
                .routes
                .iter()
                .filter(|r| had.contains(r.id.as_str()))
                .cloned()
                .collect();
            arqlet_routing::routes_hitting_nodes(
                &s.graph,
                &absolute_all(&s.graph, &s.boxes),
                &routes,
                routing_params(options),
            )
        };
        let (now, then) = (hitting(&snapshot), hitting(earlier));
        let mut report = arqlet_model::EditReport {
            routes_kept: run.search.routes_kept,
            routes_rerouted: run.search.routes_rerouted,
            routes_through_nodes: now,
            routes_through_nodes_before: then,
            ..Default::default()
        };
        let dragging = earlier
            .gesture
            .as_ref()
            .is_some_and(|g| g.active && !g.ending);
        if now > then && dragging {
            report.preview = true;
            snapshot.routing_issues.push(format!(
                "Preview: {now} routes run through nodes (before the move: {then}); the placement is completed when the move ends"
            ));
        } else if now > then {
            let everything: BTreeSet<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
            let complete = uninterrupted(compute_mode(
                graph,
                previous,
                &everything,
                0,
                options,
                &mut Run::new(&|| false),
            ))?;
            let fewer = hitting(&complete);
            if fewer < now {
                snapshot = complete;
                report.complete_fallback = true;
                report.routes_through_nodes = fewer;
            }
        }
        snapshot.edit_report = Some(report);
    }
    // A drawing that still runs routes through nodes may be routed by the other engine (P3, er-diagram).
    let dragging = previous
        .and_then(|p| p.gesture.as_ref())
        .is_some_and(|g| g.active && !g.ending);
    // Experimental (consistent layered pipeline): with hierarchical ordering the routes come from the plans; a conflict that
    // remains is reported, not answered by routing the whole drawing again with both engines.
    // Experimental (scalable organic placement): its areas are large, and routing the conflicting routes again (every route
    // of a dense drawing) or the whole drawing with both engines is not bounded by their size (2000 children did not finish
    // in 30 minutes); the conflicts that remain are reported.
    let whole_reroute = !snapshot.options.scalable_stress;
    if whole_reroute
        && !dragging
        && !snapshot.route_conflicts.is_empty()
        && !snapshot.options.hierarchical_ordering
    {
        if let Some(better) = engine_choice::improve(graph, previous, &snapshot, &finish)? {
            return Ok((better, stages));
        }
    }
    // With hierarchical ordering, only the routes in conflict are routed again (around all others, `reroute_with`); the
    // result is kept when it has fewer conflicts. A conflict that remains (an edge between areas of different flow axes,
    // which has no sections yet, searched into a node) goes to the choice of the routing engine as without the pipeline:
    // a route through a node is a hard violation.
    if whole_reroute
        && !dragging
        && !snapshot.route_conflicts.is_empty()
        && snapshot.options.hierarchical_ordering
    {
        let mut routed = reroute(&snapshot);
        labels::decorate(&mut routed);
        let mut repaired = finish(routed)?;
        labels::decorate(&mut repaired);
        if repaired.route_conflicts.len() < snapshot.route_conflicts.len() {
            snapshot = repaired;
        }
        if whole_reroute && !snapshot.route_conflicts.is_empty() {
            if let Some(better) = engine_choice::improve(graph, previous, &snapshot, &finish)? {
                return Ok((better, stages));
            }
        }
    }
    finish(snapshot).map(|snapshot| (snapshot, stages))
}

/// The steps that give a complete layout its drawn routes, labels and diagnostics. The port search uses the same steps for
/// every candidate, so a candidate is judged on what would be drawn.
fn finish(mut snapshot: Snapshot) -> Result<Snapshot, String> {
    let straight = styles::plan_straight_edges(&snapshot)?;
    styles::apply_straight_edges(&mut snapshot, &straight)?;
    // The legacy improvement would replace the channel router's routes; the channel engine keeps its own.
    if snapshot.options.routing_engine != RoutingEngine::Channel {
        styles::route_orthogonal_around(
            &mut snapshot,
            &straight,
            &|| false,
            &mut arqlet_routing::channel::SearchCounters::default(),
        )
        .map_err(|_| "uninterrupted search was interrupted".to_string())?;
    }
    // The labels and the route diagnostics were derived from the routes before the straight segments replaced them. With
    // straight edges they are derived again from the drawn routes, and the buses are drawn again on the orthogonal routes that
    // were routed anew. Without, nothing changes.
    if !straight.is_empty() {
        labels::decorate(&mut snapshot);
        apply_buses(&mut snapshot);
    }
    for diagnostic in styles::capability_diagnostics(&snapshot) {
        if !snapshot.routing_issues.contains(&diagnostic) {
            snapshot.routing_issues.push(diagnostic);
        }
    }
    Ok(snapshot)
}

/// The Optimize-time crossing step (P3): routes that cross each other are separated by the side search. Returns the better
/// drawing, or `None` when nothing is better. Not part of the layout; refinement calls it on its final drawing.
pub fn relieve_crossings(snapshot: &Snapshot) -> Result<Option<Snapshot>, String> {
    engine_choice::relieve(snapshot, &finish)
}

pub fn refine(graph: &Graph) -> Result<Snapshot, String> {
    uninterrupted(compute_mode(
        graph,
        None,
        &BTreeSet::new(),
        1,
        &LayoutOptions::default(),
        &mut Run::new(&|| false),
    ))
}
/// Upper bound of port-sizing rounds. Each round only grows leaves, so the loop is finite.
const PORT_SIZING_ROUNDS: usize = 4;

/// Computes a view. With `single_pass_auto`, a single pass that fails is computed again with the other pipeline from a fresh
/// start, so that this drawing is the one it would be without the option, and the reason is reported. Any failure counts, not
/// only the known unsupported cases: a failure of the other pipeline is then the error the caller sees.
fn compute_mode(
    graph: &Graph,
    previous: Option<&Snapshot>,
    dirty: &BTreeSet<String>,
    optimize: usize,
    options: &LayoutOptions,
    run: &mut Run,
) -> Result<Snapshot, Abort> {
    // Backbone buses take part in the layout as virtual nodes (`bus_layout`); the result is given back in the canonical graph.
    if let Some((expanded, expanded_options, expansion)) = bus_layout::expand(graph, options) {
        let earlier = previous.map(|p| bus_layout::expand_previous(p, &expanded));
        let snapshot = compute_mode_plain(
            &expanded,
            earlier.as_ref(),
            dirty,
            optimize,
            &expanded_options,
            run,
        )?;
        let mut snapshot = bus_layout::collapse(snapshot, graph, options, &expansion);
        apply_buses(&mut snapshot);
        return Ok(snapshot);
    }
    compute_mode_plain(graph, previous, dirty, optimize, options, run)
}

fn compute_mode_plain(
    graph: &Graph,
    previous: Option<&Snapshot>,
    dirty: &BTreeSet<String>,
    optimize: usize,
    options: &LayoutOptions,
    run: &mut Run,
) -> Result<Snapshot, Abort> {
    if !(options.single_pass_auto && options.single_pass) {
        return compute_requested(graph, previous, dirty, optimize, options, run);
    }
    let reason = if let Some(reason) = single_pass_cannot_run(graph, options) {
        reason
    } else {
        let earlier = run.earlier.clone();
        match compute_requested(graph, previous, dirty, optimize, options, run) {
            Err(Abort::Failed(reason)) => {
                *run = Run {
                    earlier,
                    ..Run::new(run.stop)
                };
                reason
            }
            outcome => return outcome,
        }
    };
    let other = LayoutOptions {
        abstract_routing: false,
        single_pass: false,
        ..options.clone()
    };
    let mut snapshot = compute_requested(graph, previous, dirty, optimize, &other, run)?;
    snapshot
        .layout_diagnostics
        .push(format!("Single pass was not used: {reason}"));
    Ok(snapshot)
}

/// Why single pass cannot run on this graph, where the graph already says so. Without this check the attempt fails only after
/// its placement, which on a view with organic areas costs as much as the layout that follows
/// (benchmarks/results/single-pass-auto-20261107.md). An area that chooses its direction is left to the attempt: its sense and
/// axis are known only after its placement.
fn single_pass_cannot_run(graph: &Graph, options: &LayoutOptions) -> Option<String> {
    if !graph.routing_orthogonal_only() && !options.single_pass_mixed {
        return Some("Routing completion requires the legacy router and unbundled default orthogonal routing".into());
    }
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|node| node.parent_id.as_deref())
        .collect();
    // Mixed axes are taken (ADR 0017 with amendment A): over five seeds every one of the 11 mixed datasets is better than the
    // refined default (benchmarks/results/single-pass-idle-20261108.md).
    for area in std::iter::once(None).chain(containers.into_iter().map(Some)) {
        match graph.area_mode(area) {
            Ok(Mode::Orientation(orientation)) => {
                if orientation.reversed() {
                    return Some(
                        "Fixed-order channel growth requires positive flow directions".into(),
                    );
                }
            }
            Ok(Mode::Chooser(_)) | Err(_) => {}
            // With `singlePassMixed`, areas without a fixed flow keep their own placement (organic, tree, radial).
            Ok(_) if options.single_pass_mixed => {}
            Ok(_) => {
                return Some("Abstract routing requires unflattened hierarchical placement and default orthogonal routing without a view budget".into());
            }
        }
    }
    None
}

/// Computes a view with the given options. With a minimum port spacing, leaves grow until each side has room for its ports.
fn compute_requested(
    graph: &Graph,
    previous: Option<&Snapshot>,
    dirty: &BTreeSet<String>,
    optimize: usize,
    options: &LayoutOptions,
    run: &mut Run,
) -> Result<Snapshot, Abort> {
    let requested_options = options;
    let (selected_options, layout_diagnostics) =
        select_pipeline(graph, requested_options).map_err(Abort::from)?;
    if options.single_pass && !options.abstract_routing {
        return Err(Abort::Failed("singlePass requires abstractRouting".into()));
    }
    if options.abstract_routing
        && (!selected_options.layered_construction
            || !selected_options.hierarchical_ordering
            || options.global_layering
            || options.visible_node_budget != 0
            || options.edge_routing.is_some())
    {
        return Err(Abort::Failed(
            "Abstract routing requires unflattened hierarchical placement and default orthogonal routing without a view budget"
                .into(),
        ));
    }
    let mut snapshot =
        compute_mode_selected(graph, previous, dirty, optimize, &selected_options, run)?;
    snapshot.options = requested_options.clone();
    snapshot.layout_diagnostics.extend(layout_diagnostics);
    Ok(snapshot)
}

fn compute_mode_selected(
    graph: &Graph,
    previous: Option<&Snapshot>,
    dirty: &BTreeSet<String>,
    optimize: usize,
    options: &LayoutOptions,
    run: &mut Run,
) -> Result<Snapshot, Abort> {
    // Experimental (plan section 4a, port slots): the ports of a node side keep at least a track apart, so that the routes
    // from the sections do not start or end closer than the track spacing; the leaves grow to the ports they have.
    let spacing = if options.layered_construction {
        options.min_port_spacing.max(options.track_spacing)
    } else {
        options.min_port_spacing
    };
    if spacing <= 0. {
        return compute_once(
            graph,
            previous,
            dirty,
            optimize,
            options,
            &BTreeMap::new(),
            run,
        );
    }
    // An edit with the layered construction keeps the port slots of the drawing it continues (sized from its ports), so it
    // stays a local edit.
    if let Some(p) = previous.filter(|_| options.layered_construction) {
        let containers: BTreeSet<&str> = graph
            .nodes
            .iter()
            .filter_map(|n| n.parent_id.as_deref())
            .collect();
        let minimum: BTreeMap<String, (f64, f64)> = port_demand(p, spacing)
            .into_iter()
            .filter(|(id, _)| !containers.contains(id.as_str()))
            .collect();
        return compute_once(graph, previous, dirty, optimize, options, &minimum, run);
    }
    // Sizing needs complete placement: a cached box from before a minimum would hide a violation.
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let empty = BTreeSet::new();
    let mut minimum: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut snapshot = compute_once(graph, None, &empty, optimize, options, &minimum, run)?;
    for _ in 0..PORT_SIZING_ROUNDS {
        let mut grew = false;
        for (id, (width, height)) in port_demand(&snapshot, spacing) {
            if containers.contains(id.as_str()) {
                continue;
            }
            let Some(current) = snapshot.boxes.get(&id) else {
                continue;
            };
            if current.width + 1e-9 < width || current.height + 1e-9 < height {
                let entry = minimum.entry(id).or_insert((0., 0.));
                entry.0 = entry.0.max(width);
                entry.1 = entry.1.max(height);
                grew = true;
            }
        }
        if !grew {
            break;
        }
        snapshot = compute_once(graph, None, &empty, optimize, options, &minimum, run)?;
    }
    // Anything still below its demand is reported, not hidden (the spacing the layered construction adds is not reported).
    if options.min_port_spacing <= 0. {
        return Ok(snapshot);
    }
    let unmet: Vec<String> = port_demand(&snapshot, spacing)
        .into_iter()
        .filter(|(id, (width, height))| {
            !containers.contains(id.as_str())
                && snapshot
                    .boxes
                    .get(id)
                    .is_some_and(|b| b.width + 1e-9 < *width || b.height + 1e-9 < *height)
        })
        .map(|(id, _)| id)
        .collect();
    for id in unmet {
        snapshot
            .routing_issues
            .push(format!("Port spacing at node {id} cannot be reached"));
    }
    Ok(snapshot)
}

/// Minimum leaf size that gives each side room for its ports: (slots + 1) × spacing along that side,
/// which is exactly the spacing that port placement produces.
/// The pinned children of an area, as the centres they take in the area's frame (a pin is the top-left corner).
/// The children a force-directed drag may move: the changed children and their graph neighbours within `DRAG_HOPS` hops.
const DRAG_HOPS: usize = 2;

fn drag_region(
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    dirty: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut region: BTreeSet<String> = ids
        .iter()
        .filter(|id| dirty.contains(*id))
        .cloned()
        .collect();
    for _ in 0..DRAG_HOPS {
        let reached: Vec<String> = links
            .iter()
            .flat_map(|(a, b)| {
                let mut pairs = Vec::new();
                if region.contains(a) {
                    pairs.push(b.clone());
                }
                if region.contains(b) {
                    pairs.push(a.clone());
                }
                pairs
            })
            .collect();
        region.extend(reached);
    }
    region
}

fn organic_fixed(
    options: &LayoutOptions,
    children: &[&Node],
    sizes: &BTreeMap<String, (f64, f64)>,
) -> BTreeMap<String, (f64, f64)> {
    children
        .iter()
        .filter_map(|n| {
            let p = options.pins.get(&n.id)?;
            let (w, h) = sizes[&n.id];
            Some((n.id.clone(), (p.x + w / 2., p.y + h / 2.)))
        })
        .collect()
}

fn port_demand(snapshot: &Snapshot, spacing: f64) -> BTreeMap<String, (f64, f64)> {
    // Distinct positions per side: bundle members share their leader's port and need no extra room.
    let mut slots: BTreeMap<(&str, u8), BTreeSet<u64>> = BTreeMap::new();
    for port in &snapshot.resolved_ports {
        let along = match port.side {
            PortSide::Left | PortSide::Right => port.position.y,
            PortSide::Top | PortSide::Bottom => port.position.x,
        };
        slots
            .entry((port.node_id.as_str(), port.side as u8))
            .or_default()
            .insert(along.to_bits());
    }
    let nodes: BTreeSet<&str> = snapshot
        .resolved_ports
        .iter()
        .map(|p| p.node_id.as_str())
        .collect();
    let count = |node: &str, side: PortSide| {
        slots
            .get(&(node, side as u8))
            .map_or(0, |positions| positions.len())
    };
    nodes
        .into_iter()
        .map(|node| {
            // Left and right ports run along the height; top and bottom ports along the width.
            let along_height = count(node, PortSide::Left).max(count(node, PortSide::Right));
            let along_width = count(node, PortSide::Top).max(count(node, PortSide::Bottom));
            (
                node.to_string(),
                (
                    (along_width + 1) as f64 * spacing,
                    (along_height + 1) as f64 * spacing,
                ),
            )
        })
        .collect()
}

/// Free positioning inside the raster: within each layer, a node slides across the layer towards the
/// median centre of its linked nodes. Neighbours keep their order and a gap, so nothing overlaps, and
/// the arrangement's extent is kept. Three sweeps; the default keeps the packed positions.
/// A continued area within a node budget: the children keep their earlier places; each child whose size changed moves the
/// other children whose boxes lie wholly right of its earlier box on by its change in width, and those wholly below it by
/// its change in height (the dummies of the area's plan likewise), so that nothing that did not overlap overlaps. The area
/// grows by what its content grew beyond its earlier extent. Returns the area's size.
fn push_grown(
    parent: Option<&str>,
    children: &[&Node],
    sizes: &BTreeMap<String, (f64, f64)>,
    boxes: &mut BTreeMap<String, Rect>,
    plans: &mut layered_plan::LayeredPlans,
) -> (f64, f64) {
    let mut rects: BTreeMap<String, Rect> = children
        .iter()
        .map(|c| (c.id.clone(), boxes[&c.id].clone()))
        .collect();
    let extent = |rects: &BTreeMap<String, Rect>| {
        rects.values().fold((0f64, 0f64), |(w, h), r| {
            (w.max(r.x + r.width), h.max(r.y + r.height))
        })
    };
    let before = extent(&rects);
    let area = parent.unwrap_or("<root>").to_string();
    for child in children {
        let r = rects[&child.id].clone();
        let (width, height) = sizes[&child.id];
        let (dw, dh) = (width - r.width, height - r.height);
        if dw.abs() < 1e-9 && dh.abs() < 1e-9 {
            continue;
        }
        let push = |other: &mut Rect| {
            if other.x >= r.x + r.width - 1e-9 {
                other.x += dw;
            }
            if other.y >= r.y + r.height - 1e-9 {
                other.y += dh;
            }
        };
        for (id, other) in rects.iter_mut() {
            if id != &child.id {
                push(other);
            }
        }
        if let Some(plan) = plans.get_mut(&area) {
            for d in plan.dummies.values_mut() {
                push(d);
            }
        }
        let own = rects.get_mut(&child.id).expect("a child of the area");
        own.width = width;
        own.height = height;
    }
    let after = extent(&rects);
    let earlier = parent
        .and_then(|p| boxes.get(p))
        .map(|b| (b.width, b.height))
        .unwrap_or(before);
    boxes.extend(rects);
    (
        earlier.0 + (after.0 - before.0).max(0.),
        earlier.1 + (after.1 - before.1).max(0.),
    )
}

/// Experimental (plan section 4a, step 4): widens every channel between neighbouring ranks to the tracks of its lane
/// changes and the margin from both ranks, moving the later ranks along the flow. Returns the total shift.
fn widen_channels(
    placed: &mut BTreeMap<String, Rect>,
    level_list: &[&Vec<String>],
    links: &BTreeSet<(String, String)>,
    vertical: bool,
    options: &LayoutOptions,
    grid: f64,
) -> f64 {
    let margin = routing_params(options).clearance;
    let track = options.track_spacing;
    let main = |r: &Rect| {
        if vertical {
            (r.y, r.height)
        } else {
            (r.x, r.width)
        }
    };
    let centre = |r: &Rect| {
        if vertical {
            r.x + r.width / 2.
        } else {
            r.y + r.height / 2.
        }
    };
    let mut shift = 0.;
    let mut shifts = vec![0.; level_list.len()];
    for k in 0..level_list.len().saturating_sub(1) {
        let (this, next) = (level_list[k], level_list[k + 1]);
        let far = this
            .iter()
            .filter_map(|id| placed.get(id))
            .map(|r| main(r).0 + main(r).1)
            .fold(f64::MIN, f64::max);
        let near = next
            .iter()
            .filter_map(|id| placed.get(id))
            .map(|r| main(r).0)
            .fold(f64::MAX, f64::min);
        let spans: Vec<(f64, f64)> = links
            .iter()
            .filter(|(a, b)| this.contains(a) && next.contains(b))
            .filter_map(|(a, b)| Some((centre(placed.get(a)?), centre(placed.get(b)?))))
            .filter(|(a, b)| (a - b).abs() > 0.5)
            .collect();
        let mut order: Vec<usize> = (0..spans.len()).collect();
        order.sort_by(|&i, &j| {
            (spans[i].0 + spans[i].1)
                .total_cmp(&(spans[j].0 + spans[j].1))
                .then(i.cmp(&j))
        });
        let (_, tracks) = constructive::track_slots(&spans, &order, track);
        let need = tracks as f64 * track + 2. * margin;
        shift += snap((need - (near - far)).max(0.), grid);
        shifts[k + 1] = shift;
    }
    for (k, ids) in level_list.iter().enumerate() {
        for id in ids.iter() {
            if let Some(r) = placed.get_mut(id) {
                if vertical {
                    r.y += shifts[k];
                } else {
                    r.x += shifts[k];
                }
            }
        }
    }
    shift
}

/// Experimental (consistent layered pipeline): the places of the ports of the leaf children of `area` along their node sides
/// (from the side's start), by (edge, source end). A port lies on the side its chain leaves by (the chain begins at the leaf)
/// or enters by (it ends there); an edge without a chain in the area takes the flow side of its end. The ports of a side
/// follow the order of the next element on their chains (a container there at the place of the edge's border node in it),
/// then the edge id, and lie a track apart, centred on the side.
#[allow(clippy::too_many_arguments)]
/// `through_edges_once` without the edges the parent itself takes across its own parent (alternating axes over three levels):
/// the outer area goes first, since its decision does not depend on this one.
fn through_edges(
    graph: &Graph,
    index: &GraphIndex<'_>,
    plans: &layered_plan::LayeredPlans,
    area: &str,
    lists: (&[(String, Option<String>)], &[(String, Option<String>)]),
) -> BTreeSet<String> {
    let mut out = through_edges_once(graph, index, plans, area, lists);
    let middle = index
        .index_of(area)
        .and_then(|i| index.parents[i])
        .map(|i| index.nodes[i].id.as_str());
    if let (Some(middle), false) = (middle, out.is_empty()) {
        // The middle area's own edges across its parent, as it would find them (every edge counts as leaving and entering).
        let all: Vec<(String, Option<String>)> = out
            .iter()
            .map(|e| (e.clone(), Some(String::new())))
            .collect();
        let theirs = through_edges_once(graph, index, plans, middle, (&all, &all));
        out.retain(|e| !theirs.contains(e));
    }
    out
}

/// ADR 0017, amendment A3: the edges of `area` (a container whose flow runs across its parent's) that leave or enter the parent
/// too, into an area of `area`'s axis, on a side of the parent where `area` stands outermost in its rank (in the parent's plan
/// of the earlier pass, side nodes not counted). Empty without that plan.
fn through_edges_once(
    graph: &Graph,
    index: &GraphIndex<'_>,
    plans: &layered_plan::LayeredPlans,
    area: &str,
    (entering, leaving): (&[(String, Option<String>)], &[(String, Option<String>)]),
) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let parent_of = |id: &str| -> Option<&str> {
        index
            .index_of(id)
            .and_then(|i| index.parents[i])
            .map(|i| index.nodes[i].id.as_str())
    };
    let Some(middle) = parent_of(area) else {
        return out;
    };
    let above = parent_of(middle);
    let axis = |id: Option<&str>| match direction_for(graph, id) {
        Mode::Orientation(o) if !o.reversed() => Some(o.vertical()),
        _ => None,
    };
    if axis(Some(area)).is_none() || axis(Some(area)) != axis(above) {
        return out;
    }
    let Some(plan) = plans.get(middle) else {
        return out;
    };
    let is_side = |id: &str| id.starts_with(SIDE_IN_PREFIX) || id.starts_with(SIDE_OUT_PREFIX);
    let Some(rank) = plan.ranks.iter().find(|r| r.iter().any(|x| x == area)) else {
        return out;
    };
    let inner: Vec<&String> = rank.iter().filter(|x| !is_side(x)).collect();
    let first = inner.first().is_some_and(|x| x.as_str() == area);
    let last = inner.last().is_some_and(|x| x.as_str() == area);
    let inside = |node: &str, of: &str| {
        let mut at = Some(node);
        while let Some(x) = at {
            if x == of {
                return true;
            }
            at = parent_of(x);
        }
        false
    };
    for (list, outermost) in [(entering, first), (leaving, last)] {
        if !outermost {
            continue;
        }
        for (edge, child) in list {
            if child.is_none() {
                continue;
            }
            let Some(&e) = index.edge_by_id.get(edge.as_str()) else {
                continue;
            };
            let ends = [
                Some(graph.edges[e].source.as_str()),
                graph.edges[e].target.as_deref(),
            ];
            // The other end lies outside the middle area, and the middle area has a side node for the edge (it crosses
            // into its parent across the flow).
            let leaves = ends.into_iter().flatten().any(|n| !inside(n, middle));
            if leaves && plan.sides.contains_key(edge) {
                out.insert(edge.clone());
            }
        }
    }
    out
}

/// ADR 0017, amendment A: moves a side node into its leaf's rank (or beside a child area that leaves by a border node of its
/// own, amendment A3) where the leaf stands last (leaving) or first (entering)
/// among the rank's elements other than side nodes, and drops its link; the side nodes of each rank then get their lengths
/// along the flow again (a track per side node of a kind, as at their creation). A rank left empty is removed.
fn lateral_sides(
    levels: &mut BTreeMap<usize, Vec<String>>,
    (order_links, chains, link_edges): (
        &mut BTreeSet<(String, String)>,
        &mut BTreeMap<(String, String), Vec<String>>,
        &mut BTreeMap<(String, String), Vec<String>>,
    ),
    sizes: &mut BTreeMap<String, (f64, f64)>,
    sides: &BTreeMap<String, String>,
    is_leaf: impl Fn(&str, &str) -> bool,
    (side, along_y): (f64, bool),
) {
    let is_side = |id: &str| id.starts_with(SIDE_IN_PREFIX) || id.starts_with(SIDE_OUT_PREFIX);
    let rank_of: BTreeMap<String, usize> = levels
        .iter()
        .flat_map(|(r, ids)| ids.iter().map(move |id| (id.clone(), *r)))
        .collect();
    let mut moves: Vec<(String, String, bool)> = Vec::new();
    for (edge, node) in sides {
        let leaving = node.starts_with(SIDE_OUT_PREFIX);
        let link = order_links
            .iter()
            .find(|(a, b)| if leaving { b == node } else { a == node })
            .cloned();
        let Some((a, b)) = link else { continue };
        let child = if leaving { a } else { b };
        if !is_leaf(&child, edge)
            || chains
                .get(&if leaving {
                    (child.clone(), node.clone())
                } else {
                    (node.clone(), child.clone())
                })
                .is_some_and(|d| !d.is_empty())
        {
            continue;
        }
        let Some(&r) = rank_of.get(&child) else {
            continue;
        };
        let inner: Vec<&String> = levels[&r].iter().filter(|id| !is_side(id)).collect();
        let outermost = if leaving { inner.last() } else { inner.first() };
        if outermost.map(|id| id.as_str()) == Some(child.as_str()) {
            moves.push((node.clone(), child, leaving));
        }
    }
    if moves.is_empty() {
        return;
    }
    for (node, child, leaving) in &moves {
        for ids in levels.values_mut() {
            ids.retain(|id| id != node);
        }
        let r = rank_of[child];
        let ids = levels.get_mut(&r).expect("rank of the leaf");
        if *leaving {
            ids.push(node.clone());
        } else {
            ids.insert(0, node.clone());
        }
        let key = if *leaving {
            (child.clone(), node.clone())
        } else {
            (node.clone(), child.clone())
        };
        order_links.remove(&key);
        chains.remove(&key);
        link_edges.remove(&key);
    }
    levels.retain(|_, ids| !ids.is_empty());
    for ids in levels.values() {
        for kind in [SIDE_IN_PREFIX, SIDE_OUT_PREFIX] {
            let group: Vec<&String> = ids.iter().filter(|id| id.starts_with(kind)).collect();
            let length = side * group.len() as f64;
            for id in group {
                sizes.insert(
                    id.clone(),
                    if along_y {
                        (side, length)
                    } else {
                        (length, side)
                    },
                );
            }
        }
    }
}

fn plan_port_offsets(
    graph: &Graph,
    index: &GraphIndex<'_>,
    area: Option<&str>,
    chains: &BTreeMap<(String, String), Vec<String>>,
    (borders, side_nodes): (&BTreeMap<String, String>, &BTreeMap<String, String>),
    level_list: &[&Vec<String>],
    placed: &BTreeMap<String, Rect>,
    vertical: bool,
    plans: &layered_plan::LayeredPlans,
    track: f64,
    chain_ends: bool,
) -> BTreeMap<(String, bool), f64> {
    let area_index = area.and_then(|a| index.index_of(a));
    // The child of the area that holds the node at `at` (the node itself when it is one).
    let element_at = |mut at: usize| -> Option<usize> {
        loop {
            let up = index.parents[at];
            if up == area_index {
                return Some(at);
            }
            at = up?;
        }
    };
    let place_of: BTreeMap<&str, f64> = level_list
        .iter()
        .flat_map(|ids| {
            ids.iter()
                .enumerate()
                .map(|(i, id)| (id.as_str(), i as f64))
        })
        .collect();
    // The place of an edge's border node in a container (0..1 within its rank).
    let border_place = |container: &str, edge: &str| -> Option<f64> {
        let plan = plans.get(container)?;
        if let Some(b) = plan.borders.get(edge) {
            return plan.ranks.iter().find_map(|rank| {
                rank.iter()
                    .position(|x| x == b)
                    .map(|j| (j as f64 + 0.5) / rank.len() as f64)
            });
        }
        // Single pass (ADR 0017): the port of a side node lies on the side node's own lane, the first side node of its
        // rank farthest along the flow (`prepare_leg`), so its place along the side runs against the rank's order.
        // Along the side the ports follow their side nodes' ranks first (`hierarchy_order` counts them the same way).
        let side = plan.sides.get(edge)?;
        let kind = if side.starts_with(SIDE_OUT_PREFIX) {
            SIDE_OUT_PREFIX
        } else {
            SIDE_IN_PREFIX
        };
        let r = plan.ranks.iter().position(|rank| rank.contains(side))?;
        let peers: Vec<&String> = plan.ranks[r]
            .iter()
            .filter(|id| id.starts_with(kind))
            .collect();
        let k = peers.iter().position(|id| *id == side)?;
        let n = peers.len() as f64;
        Some((r as f64 + ((n - 1. - k as f64) + 0.5) / n) / plan.ranks.len() as f64)
    };
    let mut sides: BTreeMap<(String, bool), Vec<(f64, String, bool)>> = BTreeMap::new();
    for (e, edge) in graph.edges.iter().enumerate() {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        if edge.source == target {
            continue;
        }
        let (source_at, target_at) = index.edge_ends[e];
        for (end, end_at, other_at, is_source) in [
            (edge.source.as_str(), source_at, target_at, true),
            (target, target_at, source_at, false),
        ] {
            let Some(end_at) = end_at else { continue };
            if element_at(end_at) != Some(end_at) || !index.children[end_at].is_empty() {
                continue;
            }
            let other_el = other_at
                .and_then(element_at)
                .map(|at| index.nodes[at].id.clone())
                .or_else(|| borders.get(&edge.id).cloned())
                // An edge to the outside of an area of another axis runs through its side node (ADR 0017).
                .or_else(|| side_nodes.get(&edge.id).cloned());
            if other_el.as_deref() == Some(end) {
                continue;
            }
            let (out, next) = match other_el.as_ref().and_then(|o| {
                chains
                    .get(&(end.to_string(), o.clone()))
                    .map(|d| (true, d.first().unwrap_or(o).clone()))
                    .or_else(|| {
                        chains
                            .get(&(o.clone(), end.to_string()))
                            .map(|d| (false, d.last().unwrap_or(o).clone()))
                    })
            }) {
                Some(found) => found,
                None => (is_source, String::new()),
            };
            // The edge's place at the container it runs to: with `chain_ends` (single pass) also when a chain of dummies
            // lies between, so that the edges of one chain leave the node in the order they reach the container's border
            // nodes. Without it every edge of such a chain took the same place, and the ports fell into the order of the
            // edge ids, crossing each other on the chain (measured on cloud-architecture,
            // `benchmarks/results/single-pass-gate-mixed-20261107.md`).
            let at_end = if chain_ends {
                other_el.as_deref().unwrap_or(next.as_str())
            } else {
                next.as_str()
            };
            let key = place_of
                .get(next.as_str())
                .copied()
                .unwrap_or(f64::MAX / 2.)
                + border_place(at_end, &edge.id).unwrap_or(0.5);
            sides.entry((end.to_string(), out)).or_default().push((
                key,
                edge.id.clone(),
                is_source,
            ));
        }
    }
    let mut out = BTreeMap::new();
    for ((node, _), mut list) in sides {
        let Some(r) = placed.get(&node) else { continue };
        let length = if vertical { r.width } else { r.height };
        list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        // The ports of a side lie a track apart, centred on the side, so that the edges of one chain enter its dummies
        // (whose lanes are a track apart) on the lanes they leave by.
        let n = list.len() as f64;
        let first = length / 2. - (n - 1.) * track / 2.;
        for (k, (_, edge, is_source)) in list.into_iter().enumerate() {
            out.insert((edge, is_source), first + k as f64 * track);
        }
    }
    out
}

/// Experimental (consistent layered pipeline): the channels too narrow for the tracks of the routes from the sections, by
/// a construction that only counts (`constructive::Spacing::probe`), with the ports the routing will use. The preparation
/// is returned whole (`narrow_channels`), so that a routing of the same drawing can start from it.
fn count_narrow_channels(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    options: &LayoutOptions,
    port_context: &ports::PortContext<'_>,
    plans: &layered_plan::LayeredPlans,
    counters: &mut SearchCounters,
) -> constructive::Preparation {
    let rects = absolute_all(graph, boxes);
    let resolved = port_context
        .clone()
        .with_order_keys(layered_plan::port_keys(plans, graph))
        .with_end_sides(layered_plan::end_sides(plans, graph, options.single_pass))
        .with_lanes(layered_plan::port_lanes(plans, graph, &rects))
        .resolve_with(
            boxes,
            options.port_placement,
            options.min_port_spacing,
            &options.node_rows,
            options.row_port_order,
        );
    let spacing = constructive::Spacing {
        probe: true,
        ..constructive_spacing(options)
    };
    let prepared =
        constructive::prepare_planned(graph, &rects, &resolved, spacing, &BTreeMap::new(), plans);
    counters.construction.add(&prepared.work);
    prepared
}

/// The key in the corridor gaps under which the channel after rank `rank` of `area` keeps the width the construction found
/// missing there (`constructive::Construction::narrow_channels`); no node id starts with the marker.
fn channel_key(area: &str, rank: usize) -> (String, String) {
    (area.to_string(), format!("\u{1}channel:{rank}"))
}

fn record_capacity_seeds(
    area: &str,
    levels: &[&Vec<String>],
    placed: &BTreeMap<String, Rect>,
    vertical: bool,
    track_counts: &[usize],
    track: f64,
    margin: f64,
    output: &mut Vec<CapacitySeed>,
) {
    let main = |rect: &Rect| {
        if vertical {
            (rect.y, rect.height)
        } else {
            (rect.x, rect.width)
        }
    };
    for rank in 0..levels.len().saturating_sub(1) {
        let far = levels[rank]
            .iter()
            .filter_map(|id| placed.get(id))
            .map(|rect| {
                let (start, size) = main(rect);
                start + size
            })
            .fold(f64::NEG_INFINITY, f64::max);
        let near = levels[rank + 1]
            .iter()
            .filter_map(|id| placed.get(id))
            .map(|rect| main(rect).0)
            .fold(f64::INFINITY, f64::min);
        if far.is_finite() && near.is_finite() {
            let tracks = track_counts.get(rank).copied().unwrap_or(0);
            output.push(CapacitySeed {
                area: area.into(),
                rank,
                physical_tracks: tracks,
                seeded_width: if tracks == 0 {
                    0.
                } else {
                    tracks as f64 * track + 2. * margin
                },
                initial_available: near - far,
            });
        }
    }
}

/// Experimental (consistent layered pipeline, step 2): `widen_channels` with the lane changes counted edge by edge, as the
/// routes from the sections make them. Along each chain the edges keep one order, that of their lanes where the chain
/// begins; an edge takes the lane of its border node at a container (`border_lanes`: offset across inside the container, by
/// container and edge), and its place among the chain's edges, spread evenly over the element, at a dummy or a leaf. The
/// changes of a channel take their tracks in `constructive::dependent_order`.
#[allow(clippy::too_many_arguments)]
fn channel_tracks_by_edges(
    placed: &BTreeMap<String, Rect>,
    level_list: &[&Vec<String>],
    chains: &BTreeMap<(String, String), Vec<String>>,
    link_edges: &BTreeMap<(String, String), Vec<String>>,
    border_lanes: &BTreeMap<(String, String), f64>,
    vertical: bool,
    track: f64,
) -> Vec<usize> {
    // This demand uses the cross-axis lane domains only. Main-axis coordinates
    // are still provisional when placement calls it; moving ranks afterwards
    // therefore realizes these channel lower bounds without redoing placement.
    let across = |r: &Rect| {
        if vertical {
            (r.x, r.width)
        } else {
            (r.y, r.height)
        }
    };
    let rank_of: std::collections::HashMap<&str, usize> = level_list
        .iter()
        .enumerate()
        .flat_map(|(k, ids)| ids.iter().map(move |id| (id.as_str(), k)))
        .collect();
    // The lanes by borrowed key, so that a lookup allocates nothing.
    let border_lanes: std::collections::HashMap<(&str, &str), f64> = border_lanes
        .iter()
        .map(|((element, edge), &offset)| ((element.as_str(), edge.as_str()), offset))
        .collect();
    let lane = |element: &str, edge: &str, place: usize, count: usize| -> Option<f64> {
        let r = placed.get(element)?;
        let (lo, size) = across(r);
        Some(match border_lanes.get(&(element, edge)) {
            Some(offset) => lo + offset,
            None => lo + size * (place as f64 + 0.5) / count as f64,
        })
    };
    let mut spans: Vec<Vec<(f64, f64)>> = vec![Vec::new(); level_list.len()];
    for ((a, b), dummies) in chains {
        let Some(edges) = link_edges.get(&(a.clone(), b.clone())) else {
            continue;
        };
        let mut edges: Vec<&String> = edges.iter().collect();
        edges.sort_by(|x, y| {
            let key = |e: &str| border_lanes.get(&(a.as_str(), e)).copied();
            key(x)
                .partial_cmp(&key(y))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(x.cmp(y))
        });
        let elements: Vec<&String> = std::iter::once(a)
            .chain(dummies.iter())
            .chain(std::iter::once(b))
            .collect();
        for w in elements.windows(2) {
            let Some(&k) = rank_of.get(w[0].as_str()) else {
                continue;
            };
            if rank_of.get(w[1].as_str()) != Some(&(k + 1)) {
                continue;
            }
            for (i, e) in edges.iter().enumerate() {
                let (Some(x), Some(y)) =
                    (lane(w[0], e, i, edges.len()), lane(w[1], e, i, edges.len()))
                else {
                    continue;
                };
                if (x - y).abs() > 0.5 {
                    spans[k].push((x, y));
                }
            }
        }
    }
    spans
        .iter()
        .take(level_list.len().saturating_sub(1))
        .map(|spans| {
            let order = constructive::dependent_order(spans, track);
            constructive::track_slots(spans, &order, track).1
        })
        .collect()
}

fn realize_channel_widths(
    placed: &mut BTreeMap<String, Rect>,
    level_list: &[&Vec<String>],
    tracks: &[usize],
    vertical: bool,
    options: &LayoutOptions,
    (area, gaps): (&str, &BTreeMap<(String, String), f64>),
) -> f64 {
    let margin = routing_params(options).clearance;
    let track = options.track_spacing;
    let grid = options.placement_grid;
    let main = |r: &Rect| {
        if vertical {
            (r.y, r.height)
        } else {
            (r.x, r.width)
        }
    };
    let mut shift = 0.;
    let mut shifts = vec![0.; level_list.len()];
    for k in 0..level_list.len().saturating_sub(1) {
        let (this, next) = (level_list[k], level_list[k + 1]);
        let far = this
            .iter()
            .filter_map(|id| placed.get(id))
            .map(|r| main(r).0 + main(r).1)
            .fold(f64::MIN, f64::max);
        let near = next
            .iter()
            .filter_map(|id| placed.get(id))
            .map(|r| main(r).0)
            .fold(f64::MAX, f64::min);
        // What the construction found missing in this channel (`channel_key`), on top of the tracks counted here.
        let reported = gaps.get(&channel_key(area, k)).copied().unwrap_or(0.);
        let need = tracks[k] as f64 * track + 2. * margin + reported;
        shift += snap((need - (near - far)).max(0.), grid);
        shifts[k + 1] = shift;
    }
    for (k, ids) in level_list.iter().enumerate() {
        for id in ids.iter() {
            if let Some(r) = placed.get_mut(id) {
                if vertical {
                    r.y += shifts[k];
                } else {
                    r.x += shifts[k];
                }
            }
        }
    }
    shift
}

#[cfg(test)]
mod physical_channel_capacity_tests {
    use super::*;

    #[test]
    fn cross_axis_port_domains_determine_rank_capacity_independent_of_primary_positions() {
        let levels = vec![vec!["a".into(), "c".into()], vec!["b".into(), "d".into()]];
        let level_list: Vec<_> = levels.iter().collect();
        let chains = BTreeMap::from([
            (("a".into(), "d".into()), Vec::new()),
            (("c".into(), "b".into()), Vec::new()),
        ]);
        let link_edges = BTreeMap::from([
            (("a".into(), "d".into()), vec!["ad".into()]),
            (("c".into(), "b".into()), vec!["cb".into()]),
        ]);
        let placed = BTreeMap::from([
            (
                "a".into(),
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 20.,
                    height: 20.,
                },
            ),
            (
                "c".into(),
                Rect {
                    x: 0.,
                    y: 80.,
                    width: 20.,
                    height: 20.,
                },
            ),
            (
                "b".into(),
                Rect {
                    x: 40.,
                    y: 40.,
                    width: 20.,
                    height: 20.,
                },
            ),
            (
                "d".into(),
                Rect {
                    x: 40.,
                    y: 120.,
                    width: 20.,
                    height: 20.,
                },
            ),
        ]);
        let tracks = channel_tracks_by_edges(
            &placed,
            &level_list,
            &chains,
            &link_edges,
            &BTreeMap::new(),
            false,
            6.,
        );
        assert_eq!(tracks, vec![2]);

        let shifted: BTreeMap<_, _> = placed
            .iter()
            .map(|(id, rect)| {
                let mut shifted = rect.clone();
                shifted.x += 1_000.;
                (id.clone(), shifted)
            })
            .collect();
        assert_eq!(
            channel_tracks_by_edges(
                &shifted,
                &level_list,
                &chains,
                &link_edges,
                &BTreeMap::new(),
                false,
                6.,
            ),
            tracks
        );

        let mut realized = placed;
        realize_channel_widths(
            &mut realized,
            &level_list,
            &tracks,
            false,
            &LayoutOptions::default(),
            ("<root>", &BTreeMap::new()),
        );
        let far = ["a", "c"]
            .iter()
            .map(|id| realized[*id].x + realized[*id].width)
            .fold(f64::NEG_INFINITY, f64::max);
        let near = ["b", "d"]
            .iter()
            .map(|id| realized[*id].x)
            .fold(f64::INFINITY, f64::min);
        let required = 2. * routing_params(&LayoutOptions::default()).clearance + 2. * 6.;
        assert!(near - far >= required);
    }
}

/// Experimental (consistent layered pipeline): the slots of every chain's edges in its dummies, by (dummy, edge), from the
/// dummy's start across the flow. The edges keep the pattern of their lanes where the chain begins (at a leaf's ports a
/// track apart, at a container's border nodes as far apart as those): a dummy is as wide as that pattern and a track, so
/// that the chain runs straight from its start. (Dummies a track apart instead, when the pattern spreads far, were measured
/// on nine views: more crossings, 2165 against 1849, for few fewer bends.) The dummies are resized, and every rank is pushed apart where a dummy
/// grew (`gap` beside a node, one `track` between two dummies).
fn chain_slots(
    placed: &mut BTreeMap<String, Rect>,
    levels: &BTreeMap<usize, Vec<String>>,
    (chains, link_edges, lanes): (
        &BTreeMap<(String, String), Vec<String>>,
        &BTreeMap<(String, String), Vec<String>>,
        &BTreeMap<(String, String), f64>,
    ),
    vertical: bool,
    (gap, track): (f64, f64),
) -> BTreeMap<(String, String), f64> {
    let mut out = BTreeMap::new();
    for ((a, b), dummies) in chains {
        if dummies.is_empty() {
            continue;
        }
        let Some(edges) = link_edges.get(&(a.clone(), b.clone())) else {
            continue;
        };
        let at_start: Vec<Option<f64>> = edges
            .iter()
            .map(|e| lanes.get(&(a.clone(), e.clone())).copied())
            .collect();
        let mut order: Vec<(f64, &String)> = if at_start.iter().all(Option::is_some) {
            edges
                .iter()
                .zip(&at_start)
                .map(|(e, l)| (l.expect("all lanes"), e))
                .collect()
        } else {
            edges
                .iter()
                .enumerate()
                .map(|(k, e)| (k as f64 * track, e))
                .collect()
        };
        order.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(y.1)));
        let first = order.first().map_or(0., |o| o.0);
        let width = order.last().map_or(0., |o| o.0) - first + track;
        for d in dummies {
            if let Some(r) = placed.get_mut(d) {
                if vertical {
                    r.width = width;
                } else {
                    r.height = width;
                }
            }
            for (lane, e) in &order {
                out.insert((d.clone(), (*e).clone()), lane - first + track / 2.);
            }
        }
    }
    // Every rank keeps its order and gaps after the resizing.
    let space = |a: &str, b: &str| {
        if a.starts_with(DUMMY_PREFIX) && b.starts_with(DUMMY_PREFIX) {
            track
        } else {
            gap
        }
    };
    for ids in levels.values() {
        for i in 1..ids.len() {
            let (Some(prev), Some(cur)) = (placed.get(&ids[i - 1]), placed.get(&ids[i])) else {
                continue;
            };
            let end = if vertical {
                prev.x + prev.width
            } else {
                prev.y + prev.height
            } + space(&ids[i - 1], &ids[i]);
            let start = if vertical { cur.x } else { cur.y };
            if start < end {
                let rect = placed.get_mut(&ids[i]).expect("placed element");
                if vertical {
                    rect.x = end;
                } else {
                    rect.y = end;
                }
            }
        }
    }
    out
}

/// Experimental (consistent layered pipeline): aligns the lanes of the edges across the flow. Every element of a chain
/// (a leaf, a container, a dummy, a border node) is crossed by the chain's edges at their lanes (`lanes`: by element and
/// edge, from the element's start; at a dummy, the edge's slot among the chain's edges in the order of their lanes where the
/// chain begins). In sweeps along the flow and against it, every element moves to the median of the places at which its
/// edges' lanes meet the lanes of its neighbours on the chains, keeping its rank's order and the gaps: `gap` beside a node,
/// one `track` between two dummies. Returns the far end across the flow of everything placed, and the ranks of border nodes
/// whose order changed.
/// Provisional (option `portSides`): for the explicit ports of the leaves of an area, the lanes by (element, edge), from the
/// leaf's start across the flow, and by leaf the number of leads on its low and high side. A port on its flow side lies at its
/// configured place; any other is met at its lead's lane (`constructive::plan_leads`, the plan the construction draws), so
/// the placement aligns the other end with that lane and keeps the lanes beside the side free.
#[allow(clippy::type_complexity)]
fn explicit_lanes(
    graph: &Graph,
    index: &GraphIndex<'_>,
    area: Option<&str>,
    levels: &BTreeMap<usize, Vec<String>>,
    placed: &BTreeMap<String, Rect>,
    vertical: bool,
    (margin, track): (f64, f64),
) -> (
    BTreeMap<(String, String), f64>,
    BTreeMap<String, (f64, f64)>,
) {
    let rank: BTreeMap<&str, usize> = levels
        .iter()
        .flat_map(|(r, ids)| ids.iter().map(move |id| (id.as_str(), *r)))
        .collect();
    let area_index = area.and_then(|a| index.index_of(a));
    // The element of the area that holds a node.
    let element = |id: &str| -> Option<&str> {
        let mut at = index.index_of(id)?;
        loop {
            let up = index.parents[at];
            if up == area_index {
                return Some(index.nodes[at].id.as_str());
            }
            at = up?;
        }
    };
    let ports: BTreeMap<&str, &arqlet_model::Port> =
        graph.ports.iter().map(|p| (p.id.as_str(), p)).collect();
    let edges: BTreeMap<&str, &Edge> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut lanes = BTreeMap::new();
    let mut ends = Vec::new();
    for ports_of_edge in &graph.edge_ports {
        let Some(edge) = edges.get(ports_of_edge.edge_id.as_str()) else {
            continue;
        };
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        if edge.source == target {
            continue;
        }
        for (port, is_source) in [
            (&ports_of_edge.source_port, true),
            (&ports_of_edge.target_port, false),
        ] {
            let Some(port) = port.as_deref().and_then(|p| ports.get(p)) else {
                continue;
            };
            if !placed.contains_key(&port.node_id)
                || !rank.contains_key(port.node_id.as_str())
                || !index.children_of(Some(port.node_id.as_str())).is_empty()
            {
                continue;
            }
            let length =
                placed
                    .get(&port.node_id)
                    .map_or(0., |r| if vertical { r.width } else { r.height });
            if matches!(
                (port.side, vertical),
                (PortSide::Left | PortSide::Right, false)
                    | (PortSide::Top | PortSide::Bottom, true)
            ) {
                lanes.insert(
                    (port.node_id.clone(), edge.id.clone()),
                    port.offset * length,
                );
            }
            let (s, t) = (element(&edge.source), element(target));
            let reversed = matches!((s.and_then(|s| rank.get(s)), t.and_then(|t| rank.get(t))), (Some(a), Some(b)) if b < a);
            ends.push(constructive::LeadEnd {
                node: port.node_id.clone(),
                edge: edge.id.clone(),
                endpoint: if is_source { "source" } else { "target" }.into(),
                side: port.side,
                // Along the flow within the node: the offset of a port on a side across the flow; a port on a side along
                // it lies at the near (0) or far (1) end.
                along: match (port.side, vertical) {
                    (PortSide::Top | PortSide::Bottom, false)
                    | (PortSide::Left | PortSide::Right, true) => port.offset,
                    (PortSide::Left, false) | (PortSide::Top, true) => 0.,
                    _ => 1.,
                },
                // Across the flow within the node: the offset of a port on a side along the flow; one across it lies at the
                // low (0) or high (1) side.
                across: match (port.side, vertical) {
                    (PortSide::Left | PortSide::Right, false)
                    | (PortSide::Top | PortSide::Bottom, true) => port.offset,
                    (PortSide::Top, false) | (PortSide::Left, true) => 0.,
                    _ => 1.,
                },
                leaving: is_source != reversed,
                vertical,
            });
        }
    }
    let mut room: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for (edge, endpoint, high, i, _) in constructive::plan_leads(&ends) {
        let e = ends
            .iter()
            .find(|e| e.edge == edge && e.endpoint == endpoint)
            .expect("planned end");
        let length = placed
            .get(&e.node)
            .map_or(0., |r| if vertical { r.width } else { r.height });
        let away = margin + i as f64 * track;
        lanes.insert(
            (e.node.clone(), edge),
            if high { length + away } else { -away },
        );
        let r = room.entry(e.node.clone()).or_default();
        if high {
            r.1 += 1.;
        } else {
            r.0 += 1.;
        }
    }
    (lanes, room)
}

fn align_lanes(
    placed: &mut BTreeMap<String, Rect>,
    levels: &BTreeMap<usize, Vec<String>>,
    (chains, link_edges, lanes): (
        &BTreeMap<(String, String), Vec<String>>,
        &BTreeMap<(String, String), Vec<String>>,
        &BTreeMap<(String, String), f64>,
    ),
    vertical: bool,
    (gap, track): (f64, f64),
    blocks: bool,
    room: &BTreeMap<String, (f64, f64)>,
) -> (f64, BTreeMap<usize, Vec<String>>) {
    let across = |r: &Rect| {
        if vertical {
            (r.x, r.width)
        } else {
            (r.y, r.height)
        }
    };
    // The lanes by borrowed key, so that a lookup allocates nothing.
    let lanes: std::collections::HashMap<(&str, &str), f64> = lanes
        .iter()
        .map(|((element, edge), &offset)| ((element.as_str(), edge.as_str()), offset))
        .collect();
    // By element: (neighbour on a chain, own lane offset, neighbour's lane offset) for every edge of the chain.
    let mut meets: BTreeMap<&str, Vec<(&str, f64, f64)>> = BTreeMap::new();
    // Every segment of every chain's edges, for block placement.
    let mut segments: Vec<(&str, f64, &str, f64)> = Vec::new();
    for ((a, b), dummies) in chains {
        let Some(edges) = link_edges.get(&(a.clone(), b.clone())) else {
            continue;
        };
        let mut edges: Vec<&String> = edges.iter().collect();
        edges.sort_by(|x, y| {
            let key = |e: &str| lanes.get(&(a.as_str(), e)).copied();
            key(x)
                .partial_cmp(&key(y))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(x.cmp(y))
        });
        let m = edges.len();
        let offset = |x: &str, k: usize, e: &str| -> Option<f64> {
            lanes.get(&(x, e)).copied().or_else(|| {
                let (_, size) = across(placed.get(x)?);
                Some(size * (k as f64 + 0.5) / m as f64)
            })
        };
        let elements: Vec<&String> = std::iter::once(a)
            .chain(dummies.iter())
            .chain(std::iter::once(b))
            .collect();
        // The dummies of a chain follow one end, so that the chain changes lane at most once: its start, or its end when
        // the start is a border node (which follows itself). The ends look at their neighbours on the chain.
        let upstream = !(a.starts_with(BORDER_PREFIX) && !b.starts_with(BORDER_PREFIX));
        let last = elements.len() - 1;
        for (i, w) in elements.windows(2).enumerate() {
            let (x_end, y_end) = (i == 0, i + 1 == last);
            for (k, e) in edges.iter().enumerate() {
                let (Some(p), Some(q)) = (offset(w[0], k, e), offset(w[1], k, e)) else {
                    continue;
                };
                if blocks {
                    segments.push((w[0], p, w[1], q));
                }
                if x_end || !upstream {
                    meets.entry(w[0]).or_default().push((w[1], p, q));
                }
                if y_end || upstream {
                    meets.entry(w[1]).or_default().push((w[0], q, p));
                }
            }
        }
    }
    let low = if vertical { 24. } else { 48. };
    // The sweeps work on indices: every element of the ranks and the chains, with its place across the flow (when it is
    // placed) and its size; the places go back to `placed` after the sweeps.
    let mut slot: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for id in levels.values().flatten().map(String::as_str).chain(
        meets
            .iter()
            .flat_map(|(id, list)| std::iter::once(*id).chain(list.iter().map(|(n, _, _)| *n))),
    ) {
        let next = slot.len();
        slot.entry(id).or_insert(next);
    }
    let mut names: Vec<&str> = vec![""; slot.len()];
    for (id, &i) in &slot {
        names[i] = id;
    }
    let border: Vec<bool> = names
        .iter()
        .map(|id| id.starts_with(BORDER_PREFIX))
        .collect();
    let dummy: Vec<bool> = names
        .iter()
        .map(|id| id.starts_with(DUMMY_PREFIX))
        .collect();
    let mut start: Vec<Option<f64>> = names
        .iter()
        .map(|id| placed.get(*id).map(|r| across(r).0))
        .collect();
    let size: Vec<f64> = names
        .iter()
        .map(|id| placed.get(*id).map_or(0., |r| across(r).1))
        .collect();
    let mut near: Vec<Vec<(usize, f64, f64)>> = vec![Vec::new(); names.len()];
    for (id, list) in &meets {
        near[slot[id]] = list
            .iter()
            .map(|(n, own, theirs)| (slot[n], *own, *theirs))
            .collect();
    }
    // Port leads (option `portSides`): the leads into the gap between two neighbours each need a lane of their own; the
    // gap holds one, every further one widens it by a track.
    let leads = |id: usize, high: bool| {
        room.get(names[id])
            .map_or(0., |&(low, hi)| if high { hi } else { low })
    };
    let space = |a: usize, b: usize| {
        if dummy[a] && dummy[b] {
            track
        } else {
            gap + (leads(a, true) + leads(b, false) - 1.).max(0.) * track
        }
    };
    let mut ranks: Vec<Vec<usize>> = levels
        .values()
        .map(|ids| ids.iter().map(|id| slot[id.as_str()]).collect())
        .collect();
    // A rank of border nodes takes the order of the lanes its edges arrive on, so that every border node can lie on its
    // edge's lane (each has one neighbour inside the container).
    let border_rank = |ids: &Vec<usize>| !ids.is_empty() && ids.iter().all(|&id| border[id]);
    // Border nodes follow: an element other than a border node looks at its other neighbours, when it has any.
    let target_of = |start: &[Option<f64>], id: usize| -> Option<f64> {
        let list = &near[id];
        if list.is_empty() {
            return None;
        }
        let follower = |n: usize| border[n] && !border[id];
        let lead = list.iter().any(|&(n, _, _)| !follower(n));
        let mut targets: Vec<f64> = list
            .iter()
            .filter(|&&(n, _, _)| !lead || !follower(n))
            .filter_map(|&(n, own, theirs)| Some(start[n]? + theirs - own))
            .collect();
        if targets.is_empty() {
            return None;
        }
        targets.sort_by(f64::total_cmp);
        Some(targets[targets.len() / 2])
    };
    // Eight sweeps along the flow and against it, then one that places only the ranks of border nodes on the lanes their
    // neighbours have reached.
    for sweep in 0..9 {
        let order: Vec<usize> = if sweep % 2 == 0 {
            (0..ranks.len()).collect()
        } else {
            (0..ranks.len()).rev().collect()
        };
        for r in order {
            if sweep == 8 && !border_rank(&ranks[r]) {
                continue;
            }
            if border_rank(&ranks[r]) {
                let mut keyed: Vec<(f64, usize, usize)> = ranks[r]
                    .iter()
                    .enumerate()
                    .map(|(i, &id)| (target_of(&start, id).unwrap_or(f64::MAX), i, id))
                    .collect();
                keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                ranks[r] = keyed.into_iter().map(|k| k.2).collect();
            }
            // The rank at once: the places nearest to the targets (least squares, weighted by the edges) that keep the
            // rank's order and gaps, by pooling neighbours that would overlap (isotonic regression).
            let ids = ranks[r].clone();
            // Each element's distance from the rank's start when everything before it is packed.
            let mut offsets = vec![0.; ids.len()];
            for i in 1..ids.len() {
                offsets[i] = offsets[i - 1] + size[ids[i - 1]] + space(ids[i - 1], ids[i]);
            }
            let wanted: Vec<(f64, f64)> = ids
                .iter()
                .enumerate()
                .map(|(i, &id)| {
                    let current = start[id].unwrap_or(low);
                    match target_of(&start, id) {
                        Some(t) => (t - offsets[i], near[id].len() as f64),
                        // Without edges an element keeps its place, lightly.
                        None => (current - offsets[i], 0.01),
                    }
                })
                .collect();
            // Pool adjacent violators: blocks of (weighted mean, weight, count), non-decreasing.
            let mut blocks: Vec<(f64, f64, usize)> = Vec::new();
            for &(v, w) in &wanted {
                blocks.push((v, w, 1));
                while blocks.len() > 1 {
                    let n = blocks.len();
                    if blocks[n - 2].0 <= blocks[n - 1].0 {
                        break;
                    }
                    let (b, a) = (blocks.pop().expect("block"), blocks.pop().expect("block"));
                    let weight = a.1 + b.1;
                    blocks.push(((a.0 * a.1 + b.0 * b.1) / weight, weight, a.2 + b.2));
                }
            }
            let mut i = 0;
            for (value, _, count) in blocks {
                for _ in 0..count {
                    if let Some(place) = start[ids[i]].as_mut() {
                        *place = value.max(low) + offsets[i];
                    }
                    i += 1;
                }
            }
        }
    }
    if blocks {
        // Provisional (single pass): straight blocks on the ranks the sweeps ordered (border ranks follow their lanes).
        let segments: Vec<block_placement::Segment> = segments
            .iter()
            .map(|(a, p, b, q)| block_placement::Segment {
                upper: slot[a],
                upper_offset: *p,
                lower: slot[b],
                lower_offset: *q,
            })
            .collect();
        let starts = block_placement::place(&block_placement::Input {
            ranks: &ranks,
            size: &size,
            dummy: &dummy,
            segments: &segments,
            space: &space,
        });
        let mut reach: f64 = 0.;
        for (id, place) in names.iter().zip(&starts) {
            if let (Some(place), Some(rect)) = (place, placed.get_mut(*id)) {
                if vertical {
                    rect.x = place + low;
                } else {
                    rect.y = place + low;
                }
            }
        }
        for r in placed.values() {
            let (start, size) = across(r);
            reach = reach.max(start + size);
        }
        let ranks: Vec<Vec<String>> = ranks
            .iter()
            .map(|ids| ids.iter().map(|&i| names[i].to_string()).collect())
            .collect();
        let reordered = levels
            .keys()
            .zip(ranks)
            .filter(|(k, ids)| levels[*k] != *ids)
            .map(|(k, ids)| (*k, ids))
            .collect();
        return (reach, reordered);
    }
    for (id, place) in names.iter().zip(&start) {
        if let (Some(place), Some(rect)) = (place, placed.get_mut(*id)) {
            if vertical {
                rect.x = *place;
            } else {
                rect.y = *place;
            }
        }
    }
    let ranks: Vec<Vec<String>> = ranks
        .iter()
        .map(|ids| ids.iter().map(|&i| names[i].to_string()).collect())
        .collect();
    let reach = placed
        .values()
        .map(|r| {
            let (start, size) = across(r);
            start + size
        })
        .fold(0., f64::max);
    let reordered = levels
        .keys()
        .zip(ranks)
        .filter(|(k, ids)| levels[*k] != *ids)
        .map(|(k, ids)| (*k, ids))
        .collect();
    (reach, reordered)
}

/// Experimental (plan section 4a, step 4): puts the dummies of every chain on one line across the flow, the line of its
/// dummy farthest along the cross axis; the elements after a moved dummy in its rank move on as far as needed to keep the
/// distances they had. Repeated until nothing moves (at most four rounds; a later chain may push an earlier one off its
/// line). Returns the far end across the flow of everything placed in the ranks.
fn straighten_chains(
    placed: &mut BTreeMap<String, Rect>,
    levels: &BTreeMap<usize, Vec<String>>,
    chains: &BTreeMap<(String, String), Vec<String>>,
    vertical: bool,
) -> f64 {
    let start = |r: &Rect| if vertical { r.x } else { r.y };
    let end = |r: &Rect| {
        if vertical {
            r.x + r.width
        } else {
            r.y + r.height
        }
    };
    // The distance between neighbours in each rank, kept when one of them moves.
    let mut rank_of: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut spacing: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for (&level, ids) in levels {
        for (i, id) in ids.iter().enumerate() {
            rank_of.insert(id.as_str(), (level, i));
            if i > 0 {
                if let (Some(a), Some(b)) = (placed.get(&ids[i - 1]), placed.get(id)) {
                    spacing.insert((level, i), (start(b) - end(a)).max(0.));
                }
            }
        }
    }
    for _ in 0..4 {
        let mut moved = false;
        for dummies in chains.values().filter(|d| d.len() > 1) {
            let Some(line) = dummies
                .iter()
                .filter_map(|d| placed.get(d).map(&start))
                .reduce(f64::max)
            else {
                continue;
            };
            for d in dummies {
                let (Some(&(level, index)), Some(r)) = (rank_of.get(d.as_str()), placed.get(d))
                else {
                    continue;
                };
                let delta = line - start(r);
                if delta <= 1e-6 {
                    continue;
                }
                moved = true;
                let ids = &levels[&level];
                let mut previous_end = f64::NEG_INFINITY;
                for (i, id) in ids.iter().enumerate().skip(index) {
                    let Some(r) = placed.get_mut(id) else {
                        continue;
                    };
                    let wanted = if i == index {
                        start(r) + delta
                    } else {
                        start(r).max(previous_end + spacing.get(&(level, i)).copied().unwrap_or(0.))
                    };
                    if vertical {
                        r.x = wanted;
                    } else {
                        r.y = wanted;
                    }
                    previous_end = end(r);
                }
            }
        }
        if !moved {
            break;
        }
    }
    levels
        .values()
        .flatten()
        .filter_map(|id| placed.get(id).map(&end))
        .fold(0., f64::max)
}

fn slide_free(
    placed: &mut BTreeMap<String, Rect>,
    levels: &BTreeMap<usize, Vec<String>>,
    links: &BTreeSet<(String, String)>,
    vertical: bool,
    gap: f64,
    extent: f64,
) {
    let cross = |r: &Rect| {
        if vertical {
            (r.x, r.width)
        } else {
            (r.y, r.height)
        }
    };
    let low = if vertical { 24. } else { 48. };
    for _ in 0..3 {
        for ids in levels.values() {
            for (index, id) in ids.iter().enumerate() {
                let Some(current) = placed.get(id).cloned() else {
                    continue;
                };
                let centres: Vec<f64> = links
                    .iter()
                    .filter_map(|(a, b)| {
                        let other = if a == id {
                            b
                        } else if b == id {
                            a
                        } else {
                            return None;
                        };
                        placed.get(other).map(|r| {
                            let (start, size) = cross(r);
                            start + size / 2.
                        })
                    })
                    .collect();
                if centres.is_empty() {
                    continue;
                }
                let target = centres.iter().sum::<f64>() / centres.len() as f64;
                let (_, size) = cross(&current);
                let left = if index > 0 {
                    let (start, width) = cross(&placed[&ids[index - 1]]);
                    start + width + gap
                } else {
                    low
                };
                let right = if index + 1 < ids.len() {
                    cross(&placed[&ids[index + 1]]).0 - gap - size
                } else {
                    extent - gap - size
                };
                if left > right {
                    continue;
                }
                let start = (target - size / 2.).clamp(left, right);
                let rect = placed.get_mut(id).expect("placed node");
                if vertical {
                    rect.x = start;
                } else {
                    rect.y = start;
                }
            }
        }
    }
}

/// The candidate-track router, as in the prototype: initial routes, then improvement or repair.
fn legacy_routes(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    optimize: usize,
    options: &LayoutOptions,
    port_context: &ports::PortContext<'_>,
    stop: &dyn Fn() -> bool,
    search: &mut arqlet_routing::channel::SearchCounters,
    plans: &layered_plan::LayeredPlans,
    narrow: &mut Vec<(String, usize, f64)>,
    prepared: Option<constructive::Preparation>,
) -> Result<Vec<Route>, Abort> {
    let initial_routes = route(graph, boxes, options, search, port_context);
    // Experimental (scalable organic placement): when every edge can only be drawn straight, the orthogonal routes would be
    // replaced by the straight edges anyway; the routes from port to port stand in for them, and the edges a straight line
    // cannot take are routed around the nodes afterwards (`styles::route_orthogonal_around`).
    if options.scalable_stress && styles::only_straight(graph, options) {
        return Ok(initial_routes);
    }
    if options.layered_construction {
        return Ok(layered_construction_routes(
            graph,
            boxes,
            optimize,
            options,
            port_context,
            initial_routes,
            stop,
            search,
            plans,
            narrow,
            prepared,
        )?);
    }
    if optimize > 0 {
        let rects = absolute_all(graph, boxes);
        Ok(arqlet_routing::improve_until_counted(
            graph,
            &rects,
            &initial_routes,
            routing_params(options),
            stop,
            search,
        )?)
    } else {
        Ok(arqlet_routing::repair_with_counted(
            graph,
            &absolute_all(graph, boxes),
            &initial_routes,
            routing_params(options),
            search,
        ))
    }
}

/// The id prefix of a long-edge dummy (experimental layered construction). A control character, so no node id can start
/// with it by accident.
const DUMMY_PREFIX: &str = "\u{1}dummy:";
/// The prefix of a border node (plan section 4a, step 6): a dummy where an edge crosses its container's border.
const BORDER_PREFIX: &str = "\u{1}dummy:border:";
/// Side nodes (ADR 0017): where an edge enters or leaves an area on a side of its parent's flow.
pub(crate) const SIDE_IN_PREFIX: &str = "\u{1}dummy:side-in:";
pub(crate) const SIDE_OUT_PREFIX: &str = "\u{1}dummy:side-out:";

/// The spacing of the constructed layered routes: the same obstacle margin and track spacing the general router uses.
pub fn constructive_spacing(options: &LayoutOptions) -> constructive::Spacing {
    constructive::Spacing {
        margin: routing_params(options).clearance,
        track: options.track_spacing,
        through_borders: options.hierarchical_ordering,
        probe: false,
        port_sides: options.port_sides && options.single_pass,
    }
}

/// Moves the first (`at_start`) or last point of an orthogonal route to `to`, a port slot on the same side of the node: the
/// end segment keeps its direction, and a jog at its far end joins the old course.
///
/// With `slide` (experimental, consistent layered pipeline), the end segment moves to the new slot as far as its first corner
/// instead, so that the route does not join its old course on the slot another edge now takes; a route without a corner
/// gets the jog.
fn reattach(points: &mut Vec<Point>, at_start: bool, to: Point, slide: bool) {
    if points.len() < 2 {
        return;
    }
    if !at_start {
        points.reverse();
    }
    let (end, next) = (points[0].clone(), points[1].clone());
    let horizontal = (end.y - next.y).abs() < 1e-9;
    if slide {
        let on_line = |p: &Point| {
            if horizontal {
                (p.y - end.y).abs() < 1e-9
            } else {
                (p.x - end.x).abs() < 1e-9
            }
        };
        let corner = (1..points.len())
            .take_while(|&i| on_line(&points[i]))
            .last();
        if let Some(k) = corner.filter(|&k| k + 1 < points.len()) {
            for p in points[1..=k].iter_mut() {
                if horizontal {
                    p.y = to.y;
                } else {
                    p.x = to.x;
                }
            }
            points[0] = to;
            if !at_start {
                points.reverse();
            }
            return;
        }
    }
    let jog = if horizontal {
        Point { x: next.x, y: to.y }
    } else {
        Point { x: to.x, y: next.y }
    };
    points[0] = to;
    points.insert(1, jog);
    if !at_start {
        points.reverse();
    }
}

/// Provisional (experimental layered construction, plan section 4a): the derived layout graphs of the layered areas, as the
/// complete first layout of `graph` with `options` leaves them (the layout of an import, without refinement). For tests and
/// measurements; the layout passes keep their own plans.
pub fn layered_plans(graph: &Graph, options: &LayoutOptions) -> layered_plan::LayeredPlans {
    let mut run = Run::new(&|| false);
    let all: BTreeSet<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    match compute_mode(graph, None, &all, 0, options, &mut run) {
        Ok(_) => run.plans,
        Err(_) => layered_plan::LayeredPlans::new(),
    }
}

/// Provisional (experimental layered construction): the routes the construction builds for a drawing, from the ports the
/// layout resolves for it (not from the published ports, which take the ends of the finished routes).
pub fn layered_constructed_routes(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    options: &LayoutOptions,
) -> Vec<Route> {
    layered_construction(graph, boxes, options).routes
}

/// Diagnostic (plan section 4a): the whole construction of `layered_constructed_routes`, with the delegated edges and how
/// many edges were built from the plans' sections.
pub fn layered_construction(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    options: &LayoutOptions,
) -> constructive::Construction {
    // The ports and sections follow the plans, as in the layout pass; the plans of a first placement are those of an import.
    let plans = layered_plans(graph, options);
    let resolved = ports::PortContext::new(graph)
        .with_flow_sides(options.layered_construction)
        .with_order_keys(layered_plan::port_keys(&plans, graph))
        .with_end_sides(layered_plan::end_sides(&plans, graph, options.single_pass))
        .resolve_with(
            boxes,
            options.port_placement,
            options.min_port_spacing,
            &options.node_rows,
            options.row_port_order,
        );
    let rects = absolute_all(graph, boxes);
    constructive::construct_planned(
        graph,
        &rects,
        &resolved,
        constructive_spacing(options),
        &BTreeMap::new(),
        &plans,
    )
}

/// The routes of a layered area's constructed edges are kept as constructed; the other edges are routed (or repaired)
/// around them, as fixed occupancy. The result lists the routes in the order of the graph's edges.
#[allow(clippy::too_many_arguments)]
fn layered_construction_routes(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    optimize: usize,
    options: &LayoutOptions,
    port_context: &ports::PortContext<'_>,
    initial_routes: Vec<Route>,
    stop: &dyn Fn() -> bool,
    search: &mut arqlet_routing::channel::SearchCounters,
    plans: &layered_plan::LayeredPlans,
    narrow: &mut Vec<(String, usize, f64)>,
    prepared: Option<constructive::Preparation>,
) -> Result<Vec<Route>, Abort> {
    let rects = absolute_all(graph, boxes);
    let resolved = port_context.resolve_with(
        boxes,
        options.port_placement,
        options.min_port_spacing,
        &options.node_rows,
        options.row_port_order,
    );
    let params = routing_params(options);
    let spacing = constructive_spacing(options);
    let built = constructive::construct_planned_from(
        graph,
        &rects,
        &resolved,
        spacing,
        &BTreeMap::new(),
        plans,
        prepared,
    );
    search.reused_preparations += usize::from(built.reused_preparation);
    search.construction.add(&built.work);
    *narrow = built.narrow_channels.clone();
    let constructed: BTreeMap<&str, &Route> =
        built.routes.iter().map(|r| (r.id.as_str(), r)).collect();
    // A delegated edge whose port slot the construction moved starts from the new slot; the router keeps a route's ends.
    let moved: BTreeMap<&str, (&Option<Point>, &Option<Point>)> = built
        .moved_ends
        .iter()
        .map(|(id, source, target)| (id.as_str(), (source, target)))
        .collect();
    let rest: Vec<Route> = initial_routes
        .into_iter()
        .filter(|r| !constructed.contains_key(r.id.as_str()))
        .map(|mut r| {
            if let Some((source, target)) = moved.get(r.id.as_str()) {
                if let Some(p) = source {
                    reattach(
                        &mut r.points,
                        true,
                        p.clone(),
                        options.hierarchical_ordering,
                    );
                }
                if let Some(p) = target {
                    reattach(
                        &mut r.points,
                        false,
                        p.clone(),
                        options.hierarchical_ordering,
                    );
                }
            }
            r
        })
        .collect();
    search.constructed_routes += built.routes.len();
    search.delegated_routes += rest.len();
    #[cfg(feature = "benchmark-timings")]
    search.delegation_reasons.extend(
        built
            .delegated
            .iter()
            .map(|(id, reason)| (id.clone(), format!("{reason:?}"))),
    );
    let delegated_started = stage_timer_start();
    let routed = if optimize > 0 {
        arqlet_routing::improve_until_counted_with_fixed(
            graph,
            &rects,
            &rest,
            &built.routes,
            params,
            stop,
            search,
        )?
    } else {
        arqlet_routing::repair_with_counted(graph, &rects, &rest, params, search)
    };
    search.delegated_nanos = search
        .delegated_nanos
        .saturating_add(elapsed_nanos(delegated_started));
    let mut by_id: BTreeMap<String, Route> =
        routed.into_iter().map(|r| (r.id.clone(), r)).collect();
    for route in &built.routes {
        by_id.insert(route.id.clone(), route.clone());
    }
    Ok(graph
        .edges
        .iter()
        .filter_map(|e| by_id.remove(e.id.as_str()))
        .collect())
}

/// Stub length outside each port, as in the prototype's routes.
const STUB: f64 = 20.;

/// Message for uninterruptible call sites: a search with a stop callback that never fires cannot return `Stopped`.
const UNINTERRUPTED: &str = "a search without a stop signal cannot be interrupted";

/// Channel routing for every edge: ports, stubs and node margins come from the layout.
/// Every congestion round of the channel router for this view, each as a complete route set.
/// The channel ends of a view (ports and stubs) and the absolute rectangles they are routed in.
fn channel_ends(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    options: &LayoutOptions,
    counters: &mut SearchCounters,
    port_context: &ports::PortContext<'_>,
) -> (
    BTreeMap<String, Rect>,
    Vec<arqlet_routing::channel::ChannelEnd>,
) {
    let port_started = stage_timer_start();
    let resolved = port_context.resolve_with(
        boxes,
        options.port_placement,
        options.min_port_spacing,
        &options.node_rows,
        options.row_port_order,
    );
    counters.port_resolution_nanos = counters
        .port_resolution_nanos
        .saturating_add(elapsed_nanos(port_started));
    let by_end: BTreeMap<(&str, &str), &ResolvedPort> = resolved
        .iter()
        .map(|p| ((p.edge_id.as_str(), p.endpoint.as_str()), p))
        .collect();
    let rects = absolute_all(graph, boxes);
    let margin = routing_margin(options);
    // Stubs start 20 units outside a port, but a stub inside another node's margin cannot be routed
    // from. Such stubs are shortened, in steps, until they lie outside every margin.
    let margins: Vec<Rect> = rects
        .values()
        .map(|r| Rect {
            x: r.x - margin,
            y: r.y - margin,
            width: r.width + 2. * margin,
            height: r.height + 2. * margin,
        })
        .collect();
    // With a required clearance the stub starts outside the clearance of its own node too: the search starts there, so the
    // route leaves the port and then keeps the clearance (ADR 0005). The stub is the attachment exception for its own node.
    let required = options.required_clearance.unwrap_or(0.);
    let stub = |port: &ResolvedPort| -> Point {
        let first = if required > 0. {
            margin.max(STUB)
        } else {
            STUB
        };
        for distance in [first, STUB, 12., 8., 6.] {
            let point = ports::outward(port, distance);
            let inside = margins.iter().zip(rects.keys()).any(|(m, id)| {
                (required <= 0. || id != &port.node_id)
                    && point.x > m.x + 1e-6
                    && point.x < m.x + m.width - 1e-6
                    && point.y > m.y + 1e-6
                    && point.y < m.y + m.height - 1e-6
            });
            if !inside {
                return point;
            }
        }
        ports::outward(port, STUB)
    };
    let ends: Vec<arqlet_routing::channel::ChannelEnd> = graph
        .edges
        .iter()
        .filter_map(|edge| {
            let target = edge.target.as_deref()?;
            let source_port = by_end.get(&(edge.id.as_str(), "source"))?;
            let target_port = by_end.get(&(edge.id.as_str(), "target"))?;
            Some(arqlet_routing::channel::ChannelEnd {
                edge_id: edge.id.clone(),
                source: edge.source.clone(),
                target: target.to_string(),
                start: source_port.position.clone(),
                depart: stub(source_port),
                arrive: stub(target_port),
                end: target_port.position.clone(),
            })
        })
        .collect();
    (rects, ends)
}

fn channel_rounds(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    options: &LayoutOptions,
    crossing: f64,
    selection: arqlet_routing::channel::Selection,
    counters: &mut arqlet_routing::channel::SearchCounters,
    stop: &dyn Fn() -> bool,
    port_context: &ports::PortContext<'_>,
) -> Result<Vec<Vec<Route>>, arqlet_routing::channel::Stopped> {
    let (rects, ends) = channel_ends(graph, boxes, options, counters, port_context);
    let margin = routing_margin(options);
    arqlet_routing::channel::route_channel_rounds(
        graph,
        &rects,
        &ends,
        margin,
        options.track_spacing,
        crossing,
        options.strict_clearance,
        selection,
        counters,
        stop,
    )
}

/// Routes only the `changed` edges of a channel view, around the unchanged `fixed` routes (see `route_active`).
fn channel_subset_routes(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    options: &LayoutOptions,
    changed: &BTreeSet<String>,
    fixed: &[Route],
    counters: &mut arqlet_routing::channel::SearchCounters,
    stop: &dyn Fn() -> bool,
    port_context: &ports::PortContext<'_>,
) -> Result<Option<Vec<Route>>, arqlet_routing::channel::Stopped> {
    let (rects, ends) = channel_ends(graph, boxes, options, counters, port_context);
    let margin = routing_margin(options);
    arqlet_routing::channel::route_active(
        graph,
        &rects,
        &ends,
        margin,
        options.track_spacing,
        fixed,
        changed,
        options.strict_clearance,
        counters,
        stop,
    )
}

/// Channel routes for a view. `select = false` gives the last congestion round (the initial layout, and the
/// baseline every candidate is compared with). `select = true` lets an earlier round replace it when its shared key
/// is strictly smaller; a round whose measurement fails is excluded, never scored. Selection is used only as an
/// extra refinement candidate (see `reroute`), so the baseline stays in the candidate set.
fn best_channel_routes(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    options: &LayoutOptions,
    select: bool,
    crossing: f64,
    selection: arqlet_routing::channel::Selection,
    counters: &mut arqlet_routing::channel::SearchCounters,
    stop: &dyn Fn() -> bool,
    port_context: &ports::PortContext<'_>,
) -> Result<Vec<Route>, arqlet_routing::channel::Stopped> {
    let mut rounds = channel_rounds(
        graph,
        boxes,
        options,
        crossing,
        selection,
        counters,
        stop,
        port_context,
    )?;
    let Some(last) = rounds.pop() else {
        return Ok(Vec::new());
    };
    if !select {
        return Ok(last);
    }
    let port_started = stage_timer_start();
    let ports: BTreeMap<(String, String), String> = port_context
        .resolve_with(
            boxes,
            options.port_placement,
            options.min_port_spacing,
            &options.node_rows,
            options.row_port_order,
        )
        .into_iter()
        .map(|p| ((p.edge_id, p.endpoint), p.id))
        .collect();
    counters.port_resolution_nanos = counters
        .port_resolution_nanos
        .saturating_add(elapsed_nanos(port_started));
    let projected = if needs_projected(&options.objectives) {
        projected_of(
            graph,
            boxes,
            options.objectives.contains(&Objective::EndToEnd),
        )
    } else {
        (0, 0.)
    };
    let key = |routes: &[Route]| {
        let drawn = match options.bundling {
            Bundling::None => routes.to_vec(),
            Bundling::Complete => bundle_complete(graph, routes.to_vec()),
        };
        conflict_key(
            &drawing_of(graph, boxes, &drawn, &ports, options.track_spacing),
            &options.objectives,
            projected,
            clearance(options),
        )
    };
    let mut best = last;
    let mut best_key = key(&best).ok();
    for round in rounds.into_iter().rev() {
        if let Ok(candidate) = key(&round) {
            if best_key
                .as_ref()
                .is_none_or(|known| improves(&candidate, known))
            {
                best = round;
                best_key = Some(candidate);
            }
        }
    }
    Ok(best)
}

/// The shared conflict key of a published snapshot, in the configured objective order. Lower is better; keys compare
/// lexicographically. The same function scores channel rounds and gates the refinement result.
/// Re-applies the straight edges of a snapshot to its routes (see `styles::apply_straight_edges`).
pub use styles::apply_straight_edges;

pub fn published_key(snapshot: &Snapshot) -> Result<Vec<i64>, String> {
    Ok(published_evaluation(snapshot)?.0)
}

/// The published conflict key and the shared metric of a snapshot (`published_key`, `published_metrics`) from one
/// measurement of its drawing: the two differ only in the clearance the routes keep from nodes, and the rest is measured
/// once (`arqlet_scene::metrics::measure_variants`). The same values as the two on their own.
pub fn published_evaluation(
    snapshot: &Snapshot,
) -> Result<(Vec<i64>, arqlet_scene::metrics::Metrics), String> {
    let drawing = published_drawing(snapshot);
    let key_clearance = clearance(&snapshot.options);
    let mut variants = vec![(key_clearance, false)];
    if let Some(required) = snapshot.options.required_clearance {
        variants.push((required, true));
    }
    let mut measured = arqlet_scene::metrics::measure_variants(&drawing, &variants)?;
    let published = if measured.len() == 2 {
        measured.pop().expect("the published variant")
    } else {
        measured[0].clone()
    };
    let key_metrics = measured.remove(0);
    let projected = if needs_projected(&snapshot.options.objectives) {
        projected_of(
            &snapshot.graph,
            &snapshot.boxes,
            snapshot.options.objectives.contains(&Objective::EndToEnd),
        )
    } else {
        (0, 0.)
    };
    let key = key_of(&key_metrics, &snapshot.options.objectives, projected);
    Ok((key, published))
}

/// The single acceptance rule for published conflict keys: a candidate improves an incumbent when its key is
/// smaller. Every acceptance site calls this, so a change of rule (docs/adr/0002-quality-model.md) is one edit.
pub fn improves(candidate: &[i64], incumbent: &[i64]) -> bool {
    candidate < incumbent
}

/// The distance routes keep from node borders. A required clearance is strictly inside this distance: routes lie a track
/// spacing outside it, so none is counted as within the clearance.
fn routing_margin(options: &LayoutOptions) -> f64 {
    let base = options.routing_clearance.max(NODE_MARGIN);
    options
        .required_clearance
        .map_or(base, |c| base.max(c + options.track_spacing))
}

/// The shadow M0 evaluation of a published snapshot under the default quality model. It is reported only and never
/// used for acceptance; see docs/adr/0002-quality-model.md. Measured on the same drawing as `published_key`.
pub fn shadow_evaluation(snapshot: &Snapshot) -> Result<arqlet_scene::quality::Evaluation, String> {
    let metrics = published_metrics(snapshot)?;
    arqlet_scene::quality::evaluate_with(
        &metrics,
        &arqlet_scene::quality::QualityModel::shadow_default(),
        snapshot.options.required_clearance.is_some(),
    )
}

/// The shared metric of a snapshot, measured on the drawing the published key uses.
pub fn published_metrics(snapshot: &Snapshot) -> Result<arqlet_scene::metrics::Metrics, String> {
    let drawing = published_drawing(snapshot);
    // A required clearance is a requirement for the routes' own nodes too (ADR 0011); the preferred margin is a weight.
    match snapshot.options.required_clearance {
        Some(required) => arqlet_scene::metrics::measure_with_requirement(&drawing, required),
        None => {
            arqlet_scene::metrics::measure_with_clearance(&drawing, clearance(&snapshot.options))
        }
    }
}

/// The hard violations of a measured drawing (ADR 0002, ADR 0005): node overlaps, edge-node hits and container
/// traversals, unintended shared edge segments and, when a clearance is required, near-node pairs. A published result
/// without any is valid. The sum orders two invalid results: fewer is a repair, whatever the kind.
pub fn hard_violations(
    metrics: &arqlet_scene::metrics::Metrics,
    clearance_required: bool,
) -> usize {
    metrics.sibling_overlaps
        + metrics.cross_branch_overlaps
        + metrics.line_node_hits
        + metrics.container_traversals
        + metrics.shared_line_pairs
        + if clearance_required {
            metrics.hug_pairs
        } else {
            0
        }
}

/// The node clearance route proximity is measured with: the required one when set, otherwise the preferred margin.
/// Rounds in which an area that holds a grown container arranges again.
const REPLACE_ROUNDS: usize = 2;

/// Repairs the organic areas. A container grows for its children after the repair; when the area that holds it is not organic
/// (layered, tree or radial), that area made no room for the growth, so it arranges again from the repaired boxes: the
/// children of every area keep their repaired positions as the warm start, and the area that holds the grown container places
/// it with its new size. A bounded number of rounds; what remains is reported by the repair and the validator.
fn repair_and_replace(
    graph: &Graph,
    index: &GraphIndex<'_>,
    port_context: &ports::PortContext<'_>,
    options: &LayoutOptions,
    boxes: &mut BTreeMap<String, Rect>,
    memo: &mut repair::Memo,
    (optimize, minimum, gaps, orientations): (
        usize,
        &BTreeMap<String, (f64, f64)>,
        &BTreeMap<(String, String), f64>,
        &mut BTreeMap<String, Orientation>,
    ),
    search: &mut SearchCounters,
    plans: &mut layered_plan::LayeredPlans,
    ordering: &mut placement_order::Memo,
    capacity_seeds: &mut Vec<CapacitySeed>,
) -> Vec<arqlet_model::GeometryIssue> {
    let repair_started = stage_timer_start();
    let port_resolution_nanos = std::cell::Cell::new(0);
    // The containers whose parent area is not organic: only they need the parent to make room.
    let held: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|n| {
            let node = index.index_of(&n.id).expect("validated graph node");
            !index.children[node].is_empty()
                && graph
                    .area_mode(index.parents[node].map(|parent| index.nodes[parent].id.as_str()))
                    != Ok(Mode::Organic)
        })
        .map(|n| n.id.as_str())
        .collect();
    let all: BTreeSet<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    let mut issues;
    let mut round = 0;
    loop {
        let before: Vec<(f64, f64)> = held
            .iter()
            .map(|id| (boxes[*id].width, boxes[*id].height))
            .collect();
        // After the last re-arranging a repair may not grow a container: nothing would make room for it afterwards.
        issues = memo.run(
            graph,
            options,
            boxes,
            round < REPLACE_ROUNDS,
            &port_resolution_nanos,
            port_context,
        );
        let grew = held
            .iter()
            .zip(&before)
            .any(|(id, b)| boxes[*id].width > b.0 + 1e-9 || boxes[*id].height > b.1 + 1e-9);
        if !grew || round >= REPLACE_ROUNDS {
            search.port_resolution_nanos = search
                .port_resolution_nanos
                .saturating_add(port_resolution_nanos.get());
            search.repair_nanos = search
                .repair_nanos
                .saturating_add(elapsed_nanos(repair_started));
            return issues;
        }
        round += 1;
        let placement_started = stage_timer_start();
        search.next_placement_pass_reason = Some("repair_replacement".into());
        place(
            None,
            graph,
            index,
            boxes,
            false,
            &all,
            &mut Vec::new(),
            orientations,
            optimize,
            options,
            minimum,
            gaps,
            search,
            plans,
            ordering,
            capacity_seeds,
        );
        search.placement_nanos = search
            .placement_nanos
            .saturating_add(elapsed_nanos(placement_started));
    }
}

/// Whether the root and every container with children are arranged organically.
fn all_areas_organic(graph: &Graph) -> bool {
    let areas: BTreeSet<Option<&str>> = std::iter::once(None)
        .chain(
            graph
                .nodes
                .iter()
                .filter_map(|n| n.parent_id.as_deref())
                .map(Some),
        )
        .collect();
    areas
        .into_iter()
        .all(|area| graph.area_mode(area) == Ok(Mode::Organic))
}

fn clearance(options: &LayoutOptions) -> f64 {
    options
        .required_clearance
        .unwrap_or(arqlet_scene::metrics::HUG_MARGIN)
}

/// The drawing the published key is measured on: the drawn routes of a snapshot with their ports.
pub fn published_drawing(snapshot: &Snapshot) -> arqlet_scene::metrics::Drawing {
    let mut drawn: Vec<Route> = snapshot
        .routes
        .iter()
        .filter(|r| r.bundle_of.is_none())
        .cloned()
        .collect();
    // With a routing style set, the drawing is what the scene draws (a polyline is pulled taut), not the orthogonal route
    // the router produced. The default has no style and measures the routes as they are.
    if snapshot.graph.default_routing.is_some() || !snapshot.graph.container_routing.is_empty() {
        if let Ok(scene) = arqlet_scene::build_corner_scene(snapshot) {
            let corners: BTreeMap<&str, &Vec<Point>> = scene
                .routes
                .iter()
                .map(|r| (r.id.as_str(), &r.points))
                .collect();
            for route in &mut drawn {
                if let Some(points) = corners.get(route.id.as_str()) {
                    route.points = (*points).clone();
                }
            }
        }
    }
    let ports: BTreeMap<(String, String), String> = snapshot
        .resolved_ports
        .iter()
        .map(|p| ((p.edge_id.clone(), p.endpoint.clone()), p.id.clone()))
        .collect();
    let mut drawing = drawing_of(
        &snapshot.graph,
        &snapshot.boxes,
        &drawn,
        &ports,
        snapshot.options.track_spacing,
    );
    drawing.buses = snapshot
        .graph
        .buses
        .iter()
        .map(|b| b.edges.clone())
        .collect();
    drawing
}

/// Shared conflict values in the configured objective order. Each objective maps to its shared-metric counterpart:
/// units follow the metric (pairs, points, hundredths). `ProjectedCrossings` and `EndToEnd` come from `projected`,
/// the same values the quality comparison uses, so the key and the comparison cannot disagree. A failed
/// measurement is an error; no candidate is scored as if it were empty.
fn conflict_key(
    drawing: &arqlet_scene::metrics::Drawing,
    objectives: &[Objective],
    projected: (usize, f64),
    clearance: f64,
) -> Result<Vec<i64>, String> {
    let m = arqlet_scene::metrics::measure_with_clearance(drawing, clearance)?;
    Ok(key_of(&m, objectives, projected))
}

/// The conflict key of measured metrics (`conflict_key`).
fn key_of(
    m: &arqlet_scene::metrics::Metrics,
    objectives: &[Objective],
    projected: (usize, f64),
) -> Vec<i64> {
    objectives
        .iter()
        .map(|objective| match objective {
            Objective::SiblingOverlaps => (m.sibling_overlaps + m.cross_branch_overlaps) as i64,
            Objective::EdgeNodeHits => (m.line_node_hits + m.container_traversals) as i64,
            Objective::SharedRoutePairs => m.shared_line_pairs as i64,
            Objective::NearNodeSegments => m.hug_pairs as i64,
            Objective::RoutedCrossings => (m.crossings + m.shared_endpoint_crossings) as i64,
            Objective::Bends => m.bends as i64,
            Objective::Cost => (m.length * 100.).round() as i64,
            Objective::ProjectedCrossings => projected.0 as i64,
            Objective::EndToEnd => end_to_end_units(projected.1),
        })
        .collect()
}

/// Absolute geometry of drawn routes for the shared metric. Port identities come from the resolved ports.
fn drawing_of(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    routes: &[Route],
    ports: &BTreeMap<(String, String), String>,
    track_spacing: f64,
) -> arqlet_scene::metrics::Drawing {
    use arqlet_scene::metrics::{DrawEdge, DrawNode, Drawing};
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let containers: BTreeSet<&str> = parents.values().flatten().copied().collect();
    let absolute = absolute_all(graph, boxes);
    let nodes = graph
        .nodes
        .iter()
        .map(|n| {
            let rect = &absolute[&n.id];
            DrawNode {
                id: n.id.clone(),
                parent_id: n.parent_id.clone(),
                container: containers.contains(n.id.as_str()),
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            }
        })
        .collect();
    let by_id: BTreeMap<&str, &Route> = routes.iter().map(|r| (r.id.as_str(), r)).collect();
    let edges = graph
        .edges
        .iter()
        .filter_map(|e| {
            let target = e.target.clone()?;
            let route = by_id.get(e.id.as_str())?;
            Some(DrawEdge {
                id: e.id.clone(),
                source: e.source.clone(),
                target,
                points: route.points.clone(),
                source_port: ports.get(&(e.id.clone(), "source".into())).cloned(),
                target_port: ports.get(&(e.id.clone(), "target".into())).cloned(),
                bundle_id: None,
            })
        })
        .collect();
    Drawing {
        nodes,
        edges,
        track_spacing,
        buses: Vec::new(),
    }
}

/// Routing parameters from the view options; the defaults reproduce the prototype.
fn routing_params(options: &LayoutOptions) -> arqlet_routing::Params {
    arqlet_routing::Params {
        // A required clearance inflates the obstacles by a track more than itself, as `routing_margin` does.
        clearance: options
            .required_clearance
            .map_or(options.routing_clearance, |c| {
                options.routing_clearance.max(c + options.track_spacing)
            }),
        track: options.track_spacing,
        corridors: options.routing_corridors,
        // Shared edge segments are hard once a clearance is required (ADR 0005), so the tracks are separated then too.
        spread: options.routing_track_separation || options.required_clearance.is_some(),
        rail_offset: if options.required_clearance.is_some() {
            options.track_spacing
        } else {
            12.
        },
        own_clearance: options
            .required_clearance
            .map_or(0., |c| c + options.track_spacing),
    }
}

/// Rounds up to the placement grid; a zero grid leaves the value unchanged.
/// The steps of stress relaxation a force-directed continuation takes from the positions it starts with. Few, so that a
/// drag moves the neighbours a little at a time and each step stays quick.
const FORCE_STEPS: usize = 8;

/// The least diameter of a circle node that has no measured size (an engineering default, not a measurement): it grows with
/// the label.
const DEFAULT_CIRCLE_DIAMETER: f64 = 72.;

fn snap(value: f64, grid: f64) -> f64 {
    if grid > 0. {
        (value / grid).ceil() * grid
    } else {
        value
    }
}

/// Clearance kept between a pinned leaf and the siblings around it.
const PIN_CLEARANCE: f64 = 12.;

/// Whether two sibling rectangles are closer than the pin clearance.
fn crowded(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width + PIN_CLEARANCE
        && b.x < a.x + a.width + PIN_CLEARANCE
        && a.y < b.y + b.height + PIN_CLEARANCE
        && b.y < a.y + a.height + PIN_CLEARANCE
}

/// A pin on a node that is not in the visible view has no effect; say so rather than drop it silently.
pub(crate) fn pin_diagnostics(graph: &Graph, options: &LayoutOptions) -> Vec<String> {
    options
        .pins
        .keys()
        .filter(|id| !graph.nodes.iter().any(|node| node.id == **id))
        .map(|id| format!("Pin on {id} has no effect: the node is not in the visible view"))
        .collect()
}

/// Extra gap for each pair of consecutive sibling leaves whose corridor is too narrow for the routes that run through
/// it. A corridor with `k` routes needs `2 × margin + (k − 1) × track`: a clearance band on each side and one track per
/// extra route. Keys are ordered both ways, so placement finds the pair in either order. Pairs that already have room
/// get no entry, so a layout with no narrow corridor is unchanged.
fn corridor_gaps(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    routes: &[Route],
    options: &LayoutOptions,
) -> BTreeMap<(String, String), f64> {
    let margin = routing_margin(options);
    let track = options.track_spacing;
    let parent_of: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    // Routes that overlap another route: a corridor that holds one needs a spare track, so the next routing has room to
    // move it apart.
    let overlapped: BTreeSet<&str> = routes
        .iter()
        .enumerate()
        .filter(|(i, a)| {
            a.bundle_of.is_none()
                && routes.iter().enumerate().any(|(j, b)| {
                    *i != j && b.bundle_of.is_none() && arqlet_routing::shares_segment(a, b)
                })
        })
        .map(|(_, r)| r.id.as_str())
        .collect();
    let absolute = absolute_all(graph, boxes);
    // Siblings that are containers have corridors too (a container beside a leaf, or two containers). Only siblings are
    // paired below, so no descendant of a container lies in the corridor between it and its sibling.
    // The nodes of one parent, in id order: only they are paired, so the pairs of all nodes are never looked at.
    let mut siblings: BTreeMap<Option<&str>, Vec<(&String, &Rect)>> = BTreeMap::new();
    for (id, rect) in &absolute {
        let parent = parent_of.get(id.as_str()).copied().flatten();
        siblings.entry(parent).or_default().push((id, rect));
    }
    // Experimental (plan section 4a): the tracks the constructed routes will need, reserved before they are routed. Every edge
    // is lifted to the smallest container holding both ends; there it runs from the child that holds its source (A) to the child
    // that holds its target (B). A corridor along the flow before B, or after A, needs a track for each such edge.
    let (planned_in, planned_out) = if options.layered_construction {
        planned_tracks(graph, &parent_of, &absolute)
    } else {
        (BTreeMap::new(), BTreeMap::new())
    };
    let mut gaps = BTreeMap::new();
    for (parent, leaves) in &siblings {
        let flow_axis = if options.layered_construction {
            fixed_flow_axis(graph, *parent)
        } else {
            None
        };
        for (i, (a_id, a)) in leaves.iter().enumerate() {
            for (b_id, b) in leaves.iter().skip(i + 1) {
                // The corridor lies between the two boxes along one axis, over their overlap on the other.
                let overlap = |lo1: f64, hi1: f64, lo2: f64, hi2: f64| (lo1.max(lo2), hi1.min(hi2));
                let (over_lo, over_hi, gap_lo, gap_hi, horizontal_run) = {
                    let (ox0, ox1) = overlap(a.x, a.x + a.width, b.x, b.x + b.width);
                    let (oy0, oy1) = overlap(a.y, a.y + a.height, b.y, b.y + b.height);
                    if ox1 - ox0 > EPS && a.y + a.height <= b.y {
                        (ox0, ox1, a.y + a.height, b.y, true)
                    } else if ox1 - ox0 > EPS && b.y + b.height <= a.y {
                        (ox0, ox1, b.y + b.height, a.y, true)
                    } else if oy1 - oy0 > EPS && a.x + a.width <= b.x {
                        (oy0, oy1, a.x + a.width, b.x, false)
                    } else if oy1 - oy0 > EPS && b.x + b.width <= a.x {
                        (oy0, oy1, b.x + b.width, a.x, false)
                    } else {
                        continue;
                    }
                };
                if gap_hi - gap_lo <= EPS {
                    continue;
                }
                // Only consecutive siblings: a third leaf inside the corridor makes the pair non-adjacent.
                let blocked = leaves.iter().any(|(_, c)| {
                    // Only siblings can lie between two siblings (this group holds nothing else); an enclosing container is not a
                    // third node.
                    // The leaf's extent along the gap axis must reach into the corridor, its extent across must reach the overlap.
                    let (along0, along1, across0, across1) = if horizontal_run {
                        (c.y, c.y + c.height, c.x, c.x + c.width)
                    } else {
                        (c.x, c.x + c.width, c.y, c.y + c.height)
                    };
                    along0 < gap_hi - EPS
                        && along1 > gap_lo + EPS
                        && across0 < over_hi - EPS
                        && across1 > over_lo + EPS
                });
                if blocked {
                    continue;
                }
                // Routes that run along the corridor: a segment across it, inside the overlap.
                let users: BTreeSet<&str> = routes
                    .iter()
                    .filter(|route| {
                        route.points.windows(2).any(|w| {
                            if horizontal_run {
                                (w[0].y - w[1].y).abs() < EPS
                                    && w[0].y > gap_lo + EPS
                                    && w[0].y < gap_hi - EPS
                                    && w[0].x.min(w[1].x) < over_hi
                                    && w[0].x.max(w[1].x) > over_lo
                            } else {
                                (w[0].x - w[1].x).abs() < EPS
                                    && w[0].x > gap_lo + EPS
                                    && w[0].x < gap_hi - EPS
                                    && w[0].y.min(w[1].y) < over_hi
                                    && w[0].y.max(w[1].y) > over_lo
                            }
                        })
                    })
                    .map(|route| route.id.as_str())
                    .collect();
                let spare = if users.iter().any(|id| overlapped.contains(id)) {
                    track
                } else {
                    0.
                };
                // A corridor along the area's flow holds the planned tracks of the edges that enter the later box or leave the
                // earlier one (experimental, see `planned_tracks`).
                let planned = match flow_axis {
                    Some(vertical) if vertical == horizontal_run => {
                        let a_first = if horizontal_run {
                            a.y + a.height <= b.y
                        } else {
                            a.x + a.width <= b.x
                        };
                        let (earlier, later) = if a_first {
                            (a_id.as_str(), b_id.as_str())
                        } else {
                            (b_id.as_str(), a_id.as_str())
                        };
                        planned_in
                            .get(later)
                            .copied()
                            .unwrap_or(0)
                            .max(planned_out.get(earlier).copied().unwrap_or(0))
                    }
                    _ => 0,
                };
                let count = users.len().max(planned);
                let needed = 2. * margin + count.saturating_sub(1) as f64 * track + spare;
                let gap = gap_hi - gap_lo;
                if needed > gap + EPS {
                    let extra = needed - gap;
                    gaps.insert(((*a_id).clone(), (*b_id).clone()), extra);
                    gaps.insert(((*b_id).clone(), (*a_id).clone()), extra);
                }
            }
        }
    }
    gaps
}

/// The axis of an area's fixed flow: `Some(true)` for top-to-bottom, `Some(false)` for left-to-right, `None` otherwise.
fn fixed_flow_axis(graph: &Graph, area: Option<&str>) -> Option<bool> {
    let direction = area
        .and_then(|id| graph.container_directions.get(id))
        .unwrap_or(&graph.default_direction);
    match direction {
        DirectionConstraint::Fixed(Orientation::Lr)
        | DirectionConstraint::Fixed(Orientation::Rl) => Some(false),
        DirectionConstraint::Fixed(Orientation::Tb)
        | DirectionConstraint::Fixed(Orientation::Bt) => Some(true),
        _ => None,
    }
}

/// The planned tracks of the edges, lifted to their common container: for each child of a container, the number of edges that
/// enter it from an earlier sibling and that leave it for a later sibling, along the container's fixed flow. Edges whose ends lie
/// one inside the other, or whose common container has no fixed flow, are not counted.
fn planned_tracks(
    graph: &Graph,
    parent_of: &BTreeMap<&str, Option<&str>>,
    absolute: &BTreeMap<String, Rect>,
) -> (BTreeMap<String, usize>, BTreeMap<String, usize>) {
    let chain = |id: &str| -> Vec<Option<&str>> {
        let mut out = Vec::new();
        let mut at = parent_of.get(id).copied().flatten();
        loop {
            out.push(at);
            match at {
                Some(a) => at = parent_of.get(a).copied().flatten(),
                None => break,
            }
        }
        out
    };
    // The child of `common` that holds `id` (or `id` itself when it is a child of `common`).
    let below = |id: &str, common: Option<&str>| -> Option<String> {
        let mut at = id;
        loop {
            let parent = parent_of.get(at).copied().flatten();
            if parent == common {
                return Some(at.to_string());
            }
            at = parent?;
        }
    };
    let (mut entering, mut leaving) = (BTreeMap::new(), BTreeMap::new());
    for edge in &graph.edges {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        if edge.source == target {
            continue;
        }
        let (s_chain, t_chain) = (chain(&edge.source), chain(target));
        if s_chain.contains(&Some(target)) || t_chain.contains(&Some(edge.source.as_str())) {
            continue;
        }
        let Some(common) = s_chain.iter().find(|a| t_chain.contains(a)).copied() else {
            continue;
        };
        let Some(vertical) = fixed_flow_axis(graph, common) else {
            continue;
        };
        let (Some(a), Some(b)) = (below(&edge.source, common), below(target, common)) else {
            continue;
        };
        if a == b {
            continue;
        }
        let (Some(ra), Some(rb)) = (absolute.get(&a), absolute.get(&b)) else {
            continue;
        };
        let forward = if vertical {
            ra.y + ra.height <= rb.y
        } else {
            ra.x + ra.width <= rb.x
        };
        if !forward {
            continue;
        }
        *leaving.entry(a).or_insert(0) += 1;
        *entering.entry(b).or_insert(0) += 1;
    }
    (entering, leaving)
}

fn corridor_gaps_counted(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    routes: &[Route],
    options: &LayoutOptions,
    counters: &mut SearchCounters,
) -> BTreeMap<(String, String), f64> {
    let started = stage_timer_start();
    let gaps = corridor_gaps(graph, boxes, routes, options);
    counters.corridor_gap_calculations += 1;
    counters.corridor_gap_nanos = counters
        .corridor_gap_nanos
        .saturating_add(elapsed_nanos(started));
    gaps
}

/// Rounds of corridor widening and re-routing for a required clearance.
const WIDENING_ROUNDS: usize = 3;

/// The routes of the current placement with the configured engine.
pub(crate) fn route_all(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    optimize: usize,
    options: &LayoutOptions,
    channel: bool,
    run: &mut Run,
    port_context: &ports::PortContext<'_>,
) -> Result<Vec<Route>, Abort> {
    let prepared = run.prepared.take();
    if channel {
        best_channel_routes(
            graph,
            boxes,
            options,
            false,
            0.,
            Selection::Shared,
            &mut run.search,
            run.stop,
            port_context,
        )
        .map_err(Abort::from)
    } else if options.layered_construction && !run.plans.is_empty() {
        // Experimental (plan section 4a, step 3): the ports of a node side in a layered area follow the order of the plan.
        let mut keyed = port_context
            .clone()
            .with_order_keys(layered_plan::port_keys(&run.plans, graph))
            .with_end_sides(layered_plan::end_sides(
                &run.plans,
                graph,
                options.single_pass,
            ));
        if options.hierarchical_ordering {
            keyed = keyed.with_lanes(layered_plan::port_lanes(
                &run.plans,
                graph,
                &absolute_all(graph, boxes),
            ));
        }
        legacy_routes(
            graph,
            boxes,
            optimize,
            options,
            &keyed,
            run.stop,
            &mut run.search,
            &run.plans,
            &mut run.narrow,
            prepared,
        )
    } else {
        legacy_routes(
            graph,
            boxes,
            optimize,
            options,
            port_context,
            run.stop,
            &mut run.search,
            &run.plans,
            &mut run.narrow,
            prepared,
        )
    }
}

/// The drawing an edit continues: its view, boxes and routes.
struct Before<'a> {
    graph: &'a Graph,
    boxes: &'a BTreeMap<String, Rect>,
    routes: &'a [Route],
}

/// Routes of a placement that continues `before`, when only part of the drawing changed: the routes of edges that the change
/// does not touch stay as they were, the others are routed again, and a kept route that now runs into a node is repaired too.
/// Deterministic for the same earlier drawing and the same change, and not equal to a complete routing: the unaffected part
/// stays stable. `None` when the drawing cannot be continued this way (another graph, bundles, straight edges, another
/// router, or more than half of the nodes moved); the caller routes everything.
fn local_routes(
    graph: &Graph,
    index: &GraphIndex<'_>,
    port_context: &ports::PortContext<'_>,
    boxes: &BTreeMap<String, Rect>,
    before: &Before,
    options: &LayoutOptions,
    search: &mut arqlet_routing::channel::SearchCounters,
    plans: &layered_plan::LayeredPlans,
) -> Option<Vec<Route>> {
    if options.routing_engine != RoutingEngine::Legacy || options.bundling != Bundling::None {
        return None;
    }
    if !styles::straight_candidates(graph, options).is_empty() {
        return None;
    }
    // Everything the earlier drawing had is still there, unchanged; an edit may have added nodes and edges, which have no
    // earlier route and no earlier box.
    let now_nodes: BTreeMap<&str, &Node> = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let now_edges: BTreeMap<&str, &Edge> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let same_nodes = before.graph.nodes.iter().all(|a| {
        now_nodes
            .get(a.id.as_str())
            .is_some_and(|b| a.shape == b.shape && a.parent_id == b.parent_id)
    });
    // The earlier edges that this drawing no longer has as they were (in a view within a node budget, expanding a container
    // moves the ends of the edges it aggregated to its children): they are routed again as new edges.
    let changed: BTreeSet<&str> = before
        .graph
        .edges
        .iter()
        .filter(|a| {
            !now_edges
                .get(a.id.as_str())
                .is_some_and(|b| a.source == b.source && a.target == b.target)
        })
        .map(|a| a.id.as_str())
        .collect();
    let same_edges = changed.is_empty() || options.visible_node_budget > 0;
    if !same_nodes || !same_edges {
        return None;
    }
    let old = absolute_all(before.graph, before.boxes);
    let new = absolute_all(graph, boxes);
    // How each node moved: by a distance with its size kept (a block that moves as a whole, such as the content of a container
    // that a growing sibling pushed aside), or otherwise (resized, new).
    let shift: BTreeMap<&str, Option<(f64, f64)>> = graph
        .nodes
        .iter()
        .map(|n| {
            let by = match (old.get(&n.id), new.get(&n.id)) {
                (Some(a), Some(b))
                    if (a.width - b.width).abs() < 1e-9 && (a.height - b.height).abs() < 1e-9 =>
                {
                    Some((b.x - a.x, b.y - a.y))
                }
                _ => None,
            };
            (n.id.as_str(), by)
        })
        .collect();
    let same_shift = |a: Option<(f64, f64)>, b: Option<(f64, f64)>| match (a, b) {
        (Some(a), Some(b)) => (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6,
        _ => false,
    };
    let mut seeds = route(graph, boxes, options, search, port_context);
    let kept_old: BTreeMap<&str, &Route> = before
        .routes
        .iter()
        .filter(|r| r.bundle_of.is_none() && !changed.contains(r.id.as_str()))
        .map(|r| (r.id.as_str(), r))
        .collect();
    let edges: BTreeMap<&str, &Edge> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let moved = |a: &Point, b: &Point| (a.x - b.x).abs() > 1e-6 || (a.y - b.y).abs() > 1e-6;
    // The earlier route of an edge whose two ends moved by the same distance, carried along; an edge whose ends moved
    // differently, or whose end does not meet its port any more, is routed again.
    let mut kept: Vec<Route> = Vec::new();
    let mut affected: BTreeSet<String> = BTreeSet::new();
    let margin = NODE_MARGIN;
    for seed in &seeds {
        let carried = edges.get(seed.id.as_str()).and_then(|edge| {
            let target = edge.target.as_deref()?;
            let by = shift.get(edge.source.as_str()).copied().flatten()?;
            same_shift(Some(by), shift.get(target).copied().flatten()).then_some(())?;
            let route = kept_old.get(seed.id.as_str())?;
            let points: Vec<Point> = route
                .points
                .iter()
                .map(|p| Point {
                    x: p.x + by.0,
                    y: p.y + by.1,
                })
                .collect();
            let ends_meet = match (
                seed.points.first(),
                points.first(),
                seed.points.last(),
                points.last(),
            ) {
                (Some(a), Some(b), Some(c), Some(d)) => !moved(a, b) && !moved(c, d),
                _ => false,
            };
            ends_meet.then(|| {
                (
                    Route {
                        points,
                        ..(*route).clone()
                    },
                    by,
                    *edge,
                )
            })
        });
        let Some((route, by, edge)) = carried else {
            affected.insert(seed.id.clone());
            continue;
        };
        // The carried route must keep clear of every node that did not move with it. The nodes it belongs to (the ends and
        // their containers) are not obstacles for it.
        let own = index.excluded_for_edge(&edge.id)?;
        let blocked = graph.nodes.iter().any(|n| {
            let node_index = index.index_of(&n.id).expect("indexed graph node");
            if own.binary_search(&node_index).is_ok()
                || same_shift(Some(by), shift.get(n.id.as_str()).copied().flatten())
            {
                return false;
            }
            new.get(&n.id).is_some_and(|r| {
                arqlet_routing::hits(
                    &route,
                    &Rect {
                        x: r.x - margin,
                        y: r.y - margin,
                        width: r.width + 2. * margin,
                        height: r.height + 2. * margin,
                    },
                )
            })
        });
        if blocked {
            affected.insert(seed.id.clone());
        }
        kept.push(route);
    }
    // Experimental (plan section 4a, reuse in edits): the edges the edit affects are built from the sections of the plans,
    // around the routes it carries along; only those the construction cannot build go to the repair.
    if options.layered_construction && !plans.is_empty() && !affected.is_empty() {
        let resolved = port_context.resolve_with(
            boxes,
            options.port_placement,
            options.min_port_spacing,
            &options.node_rows,
            options.row_port_order,
        );
        let fixed: Vec<Route> = kept
            .iter()
            .filter(|r| !affected.contains(&r.id))
            .cloned()
            .collect();
        let built = constructive::construct_planned_subset(
            graph,
            &new,
            &resolved,
            constructive_spacing(options),
            &BTreeMap::new(),
            plans,
            &affected,
            &fixed,
        );
        search.construction.add(&built.work);
        // A delegated edge whose port slot the construction moved starts from the new slot.
        for (id, source, target) in &built.moved_ends {
            if let Some(seed) = seeds.iter_mut().find(|r| &r.id == id) {
                if let Some(p) = source {
                    reattach(
                        &mut seed.points,
                        true,
                        p.clone(),
                        options.hierarchical_ordering,
                    );
                }
                if let Some(p) = target {
                    reattach(
                        &mut seed.points,
                        false,
                        p.clone(),
                        options.hierarchical_ordering,
                    );
                }
            }
        }
        for route in built.routes {
            affected.remove(&route.id);
            kept.retain(|r| r.id != route.id);
            kept.push(route);
        }
    }
    Some(arqlet_routing::repair_subset_counted(
        graph,
        &new,
        &seeds,
        &kept,
        &affected,
        routing_params(options),
        search,
    ))
}

fn compute_once(
    graph: &Graph,
    previous: Option<&Snapshot>,
    dirty: &BTreeSet<String>,
    optimize: usize,
    options: &LayoutOptions,
    minimum: &BTreeMap<String, (f64, f64)>,
    run: &mut Run,
) -> Result<Snapshot, Abort> {
    run.search.layout_passes += 1;
    let mut ordering = placement_order::Memo::default();
    #[cfg(test)]
    {
        ordering.disabled = run.disable_phase_reuse;
        ordering.coordinates.get_mut().disabled =
            run.disable_coordinate_reuse || run.disable_phase_reuse;
        ordering.structure.disabled = run.disable_structure_reuse || run.disable_phase_reuse;
        ordering.regions.disabled = run.disable_region_reuse || run.disable_phase_reuse;
    }
    validate(graph)?;
    let index = GraphIndex::new(graph);
    let port_context = ports::PortContext::new(graph).with_flow_sides(options.layered_construction);
    for label in options
        .edge_labels
        .values()
        .chain(options.port_labels.values())
    {
        if ![label.width, label.height]
            .iter()
            .all(|v| v.is_finite() && *v > 0. && *v <= 10000.)
        {
            return Err(Abort::Failed("Invalid label dimensions".into()));
        }
    }
    // Boxes of nodes that the graph no longer has are dropped; the rest seed the layout as its warm start.
    let known: BTreeSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut boxes: BTreeMap<String, Rect> = previous
        .map(|s| {
            s.boxes
                .iter()
                .filter(|(id, _)| known.contains(id.as_str()))
                .map(|(id, r)| (id.clone(), r.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut changed = Vec::new();
    // The orientations free directions chose: those of the areas an edit does not place again are those of the earlier drawing.
    let mut orientations: BTreeMap<String, Orientation> = BTreeMap::new();
    if let Some(p) = previous {
        for (area, orientation) in &p.area_orientations {
            if area == "<root>" || known.contains(area.as_str()) {
                orientations.insert(area.clone(), *orientation);
            }
        }
    }
    // An edit that lays out only part of the drawing starts from the corridor widths of the drawing it continues: an area that
    // is placed again is as wide as it was, and only what the change makes too narrow is widened further.
    let mut gaps: BTreeMap<(String, String), f64> = BTreeMap::new();
    if let Some(p) = previous.filter(|_| dirty.len() < graph.nodes.len()) {
        for gap in &p.corridor_gaps {
            if known.contains(gap.before.as_str()) && known.contains(gap.after.as_str()) {
                gaps.insert((gap.before.clone(), gap.after.clone()), gap.extra);
                gaps.insert((gap.after.clone(), gap.before.clone()), gap.extra);
            }
        }
    }
    // Experimental (plan section 4a, reuse): an area the edit does not place again keeps the plan of the drawing it continues,
    // so that the steps after the placement see the plans of all layered areas, not only of those placed again. The plans of
    // the other areas only give the order their placement starts from, and are dropped after it.
    let mut reused = BTreeSet::new();
    if options.layered_construction {
        let earlier = previous.map_or(&run.earlier, |p| &p.layered_plans);
        let (kept, starts) = reused_plans(earlier, previous.is_some(), &index, dirty);
        reused = kept.keys().cloned().collect();
        run.plans = starts;
        run.plans.extend(kept);
    }
    // Single pass, the phases of an import and what is known about their cost (numbers and method:
    // benchmarks/results/single-pass-levers-20261106.md):
    // 1. Placement that only orders, then the hierarchical sweep (`hierarchy_order`). The runs end at their first sweep
    //    without improvement; this changes the output, and lowers the cost on the small views only.
    // 2. Placement with geometry, the largest part after the completion. Its cost is spread over many functions; not
    //    optimized yet.
    // 3. Completion (`abstract_realization::completion`), the largest part: the plan, then a realization that only measures
    //    channel demand (`growth::widen_planned`), then the realization on the grown geometry.
    //    - Both realizations run `joint::reserve`, which reads the flow coordinates the growth changes, so the demand
    //      realization is not reused. What it could share is a few percent.
    //    - Searches removed, each output-identical: track lines for the occupancy check (`construction::Occupancy`),
    //      `ports::PortEnds`, channels by area and rank, the children walk of the growth, the reach filter of
    //      `ports::boundaries`. They help on large views far more than on the code view.
    //    - `Session::plan` has no search left to remove; `Geometry::rect` is a small share.
    // String-keyed maps and allocation remain the largest cost across all phases, and would need a change of data structures.
    //
    // The links the cycle break reverses depend only on each area's own children, so they are known before any area is
    // placed, and the first placement already gives a backward edge between containers its final border sides.
    let reversals_known = options.single_pass
        && options.abstract_routing
        && options.layered_construction
        && options.hierarchical_ordering
        && previous.is_none();
    if let Some(reversals) = reversals_known
        .then(|| known_reversals(graph, &index, options))
        .flatten()
    {
        ordering.reversals = std::rc::Rc::new(reversals);
        // The first pass only orders when every layered area has its orientation: its plans start the hierarchical
        // ordering, and the placement after it is the only one with coordinates.
        ordering.order_only = std::iter::once(None)
            .chain(index.nodes.iter().map(|n| Some(n.id.as_str())))
            .all(|area| {
                !matches!(direction_for(graph, area), Mode::Chooser(_))
                    || index.children_of(area).is_empty()
            });
    }
    let order_only = ordering.order_only;
    let placement_started = stage_timer_start();
    run.search.next_placement_pass_reason = Some(
        if order_only {
            "initial_order"
        } else {
            "initial"
        }
        .into(),
    );
    place(
        None,
        graph,
        &index,
        &mut boxes,
        previous.is_none(),
        dirty,
        &mut changed,
        &mut orientations,
        optimize,
        options,
        minimum,
        &gaps,
        &mut run.search,
        &mut run.plans,
        &mut ordering,
        &mut run.capacity_seeds,
    );
    run.plans
        .retain(|area, _| reused.contains(area) || changed.iter().any(|c| c == area));
    ordering.order_only = false;
    // Experimental (consistent layered pipeline, step 1): the plans of all areas are ordered together, a container's border
    // order agreeing with the order outside in every sweep; when that lowers the crossings of all plans, the drawing is
    // placed again with these orders (the ranks, dummies and border nodes are the same).
    if options.layered_construction && options.hierarchical_ordering && previous.is_none() {
        let parent_of: BTreeMap<String, String> = graph
            .nodes
            .iter()
            .map(|n| {
                (
                    n.id.clone(),
                    n.parent_id.clone().unwrap_or_else(|| "<root>".into()),
                )
            })
            .collect();
        let edges: BTreeMap<String, (String, String)> = graph
            .edges
            .iter()
            .filter_map(|e| Some((e.id.clone(), (e.source.clone(), e.target.clone()?))))
            .collect();
        // A backward edge between containers takes its border nodes on the other sides (`reversed_above`), which the
        // first placement could not know: its areas above were not placed yet. When there is such an edge, the drawing is
        // placed again with the plans of the first placement at hand (the ranks of every area are the same).
        let backward_between = graph.edges.iter().any(|e| {
            e.target.as_deref().is_some_and(|t| {
                parent_of.get(&e.source) != parent_of.get(t)
                    && reversed_above(&index, &run.plans, &ordering.reversals, &e.source, t)
            })
        });
        if options.abstract_routing {
            // Both corrections depend on the first plans, not on the intermediate geometry: choose their final order
            // before rebuilding so the backward-border correction and hierarchical ordering share one placement.
            // Single pass: the backward-border correction changes the border ranks, so it comes first and the
            // hierarchical order is chosen on the ranks the final placement keeps.
            let mut backward_between = backward_between && ordering.reversals.is_empty();
            if options.single_pass && backward_between {
                boxes = BTreeMap::new();
                changed.clear();
                run.search.next_placement_pass_reason = Some("backward_edge_rebuild".into());
                place(
                    None,
                    graph,
                    &index,
                    &mut boxes,
                    true,
                    dirty,
                    &mut changed,
                    &mut orientations,
                    optimize,
                    options,
                    minimum,
                    &gaps,
                    &mut run.search,
                    &mut run.plans,
                    &mut ordering,
                    &mut run.capacity_seeds,
                );
                backward_between = false;
            }
            let mut ordered = run.plans.clone();
            let runs = if options.single_pass {
                hierarchy_order::RUNS_SINGLE_PASS
            } else {
                hierarchy_order::RUNS
            };
            let idle_limit = if options.single_pass {
                hierarchy_order::IDLE_SWEEPS_SINGLE_PASS
            } else {
                hierarchy_order::IDLE_SWEEPS
            };
            let (before, after) = hierarchy_order::hierarchical_sweep(
                &mut ordered,
                &parent_of,
                &edges,
                runs,
                idle_limit,
                options.order_seed,
            );
            let hierarchy_improved = after < before;
            if hierarchy_improved {
                // Single pass keeps the border ranks the sweep agreed across container borders; the placement improves only
                // the inner ranks around them (measured: fewer crossings than re-ordering every rank, fewer bends than
                // freezing every rank).
                if options.single_pass {
                    for plan in ordered.values_mut() {
                        plan.fixed_borders = true;
                    }
                }
                run.plans = ordered;
            }
            if backward_between || hierarchy_improved || order_only {
                boxes = BTreeMap::new();
                changed.clear();
                run.search.next_placement_pass_reason = Some(
                    match (backward_between, hierarchy_improved) {
                        (true, true) => "backward_edge_hierarchical_ordering_rebuild",
                        (true, false) => "backward_edge_rebuild",
                        (false, true) => "hierarchical_ordering_rebuild",
                        (false, false) => "order_only_placement",
                    }
                    .into(),
                );
                place(
                    None,
                    graph,
                    &index,
                    &mut boxes,
                    true,
                    dirty,
                    &mut changed,
                    &mut orientations,
                    optimize,
                    options,
                    minimum,
                    &gaps,
                    &mut run.search,
                    &mut run.plans,
                    &mut ordering,
                    &mut run.capacity_seeds,
                );
            }
        } else {
            if backward_between {
                boxes = BTreeMap::new();
                changed.clear();
                run.search.next_placement_pass_reason = Some("backward_edge_rebuild".into());
                place(
                    None,
                    graph,
                    &index,
                    &mut boxes,
                    true,
                    dirty,
                    &mut changed,
                    &mut orientations,
                    optimize,
                    options,
                    minimum,
                    &gaps,
                    &mut run.search,
                    &mut run.plans,
                    &mut ordering,
                    &mut run.capacity_seeds,
                );
            }
            let mut ordered = run.plans.clone();
            let (before, after) = hierarchy_order::hierarchical_sweep(
                &mut ordered,
                &parent_of,
                &edges,
                hierarchy_order::RUNS,
                hierarchy_order::IDLE_SWEEPS,
                options.order_seed,
            );
            if after < before {
                // The orders start the placement, which still improves each area's order from there (measured: following
                // them as they are drew more crossings).
                run.plans = ordered;
                boxes = BTreeMap::new();
                changed.clear();
                run.search.next_placement_pass_reason =
                    Some("hierarchical_ordering_rebuild".into());
                place(
                    None,
                    graph,
                    &index,
                    &mut boxes,
                    true,
                    dirty,
                    &mut changed,
                    &mut orientations,
                    optimize,
                    options,
                    minimum,
                    &gaps,
                    &mut run.search,
                    &mut run.plans,
                    &mut ordering,
                    &mut run.capacity_seeds,
                );
            }
        }
    }
    if options.single_pass && options.abstract_routing {
        // Each area was placed on its own; a border node now takes the lane of its edge outside or inside, where the edge
        // changed lane on both sides of the border (`border_alignment`).
        // One pass: repeating it until nothing moves saved 102 more bends on the uniform views but drew 97 more crossings and
        // delegated 4 more edges (`benchmarks/results/single-pass-bends-20261107.md`).
        border_alignment::align_borders(graph, &boxes, &mut run.plans, options.track_spacing);
    }
    run.search.placement_nanos = run
        .search
        .placement_nanos
        .saturating_add(elapsed_nanos(placement_started));
    let channel = options.routing_engine == RoutingEngine::Channel;
    if options.strict_clearance && channel {
        // Strict clearance needs room. The demand for each corridor comes from unconstrained routes: under strict
        // clearance the routes themselves fall back, so they would show no demand. Then place again.
        let unconstrained = LayoutOptions {
            strict_clearance: false,
            ..options.clone()
        };
        let routing_started = stage_timer_start();
        let first_result = best_channel_routes(
            graph,
            &boxes,
            &unconstrained,
            false,
            0.,
            Selection::Shared,
            &mut run.search,
            run.stop,
            &port_context,
        );
        run.search.routing_nanos = run
            .search
            .routing_nanos
            .saturating_add(elapsed_nanos(routing_started));
        let first = first_result?;
        gaps = corridor_gaps_counted(graph, &boxes, &first, options, &mut run.search);
        if !gaps.is_empty() {
            run.search.clearance_widening_rounds += 1;
            boxes = BTreeMap::new();
            changed.clear();
            let placement_started = stage_timer_start();
            run.search.next_placement_pass_reason = Some("strict_clearance_widening".into());
            place(
                None,
                graph,
                &index,
                &mut boxes,
                true,
                dirty,
                &mut changed,
                &mut orientations,
                optimize,
                options,
                minimum,
                &gaps,
                &mut run.search,
                &mut run.plans,
                &mut ordering,
                &mut run.capacity_seeds,
            );
            run.search.placement_nanos = run
                .search
                .placement_nanos
                .saturating_add(elapsed_nanos(placement_started));
        }
    }
    // An organic root is repaired on its drawn geometry before the routes are made: the routes follow the repaired boxes.
    // A step of a drag (the earlier snapshot has an active gesture) repairs locally; the step that ends the gesture does not.
    let effort = match previous
        .and_then(|p| p.gesture.as_ref())
        .filter(|g| g.active)
    {
        Some(g) if g.ending => repair::Effort::Moderate,
        Some(_) => repair::Effort::Local,
        None => repair::Effort::Full,
    };
    let mut repair_memo = repair::Memo::new(effort);
    let mut repair_issues = repair_and_replace(
        graph,
        &index,
        &port_context,
        options,
        &mut boxes,
        &mut repair_memo,
        (optimize, minimum, &gaps, &mut orientations),
        &mut run.search,
        &mut run.plans,
        &mut ordering,
        &mut run.capacity_seeds,
    );
    if options.abstract_routing {
        if (run.stop)() {
            return Err(Abort::Stopped);
        }
        let started = stage_timer_start();
        let resolved = port_context
            .clone()
            .with_order_keys(layered_plan::port_keys(&run.plans, graph))
            .with_end_sides(layered_plan::end_sides(
                &run.plans,
                graph,
                options.single_pass,
            ))
            .with_lanes(layered_plan::port_lanes(
                &run.plans,
                graph,
                &absolute_all(graph, &boxes),
            ))
            .resolve_with(
                &boxes,
                options.port_placement,
                options.min_port_spacing,
                &options.node_rows,
                options.row_port_order,
            );
        run.search.port_resolution_nanos = run
            .search
            .port_resolution_nanos
            .saturating_add(elapsed_nanos(started));
        let mut input = placed_snapshot(
            graph,
            previous,
            options,
            boxes,
            Vec::new(),
            orientations,
            &gaps,
            repair_issues,
            changed,
            &run.plans,
        );
        input.resolved_ports = resolved;
        let capacity_seeds = run.capacity_seeds.clone();
        let started = stage_timer_start();
        use constructive::abstract_realization::{completion, planning};
        let completed = completion::run_until(
            &input,
            &mut planning::Session::with_coordination_reuse(),
            run.stop,
        )?;
        run.search.abstract_routing_passes += 1;
        run.search.abstract_completion.abstract_plan_nanos = run
            .search
            .abstract_completion
            .abstract_plan_nanos
            .saturating_add(completed.times.abstract_plan);
        run.search.abstract_completion.growth_nanos = run
            .search
            .abstract_completion
            .growth_nanos
            .saturating_add(completed.times.growth);
        run.search.abstract_completion.construction_nanos = run
            .search
            .abstract_completion
            .construction_nanos
            .saturating_add(completed.times.construction);
        run.search.abstract_completion.seed_preparation_nanos = run
            .search
            .abstract_completion
            .seed_preparation_nanos
            .saturating_add(completed.times.seeds);
        run.search.abstract_completion.fallback_nanos = run
            .search
            .abstract_completion
            .fallback_nanos
            .saturating_add(completed.times.fallback);
        run.search.abstract_completion.track_repair_nanos = run
            .search
            .abstract_completion
            .track_repair_nanos
            .saturating_add(completed.times.track_repair);
        run.search.abstract_completion.finalization_nanos = run
            .search
            .abstract_completion
            .finalization_nanos
            .saturating_add(completed.times.finalization);
        run.search.abstract_completion.track_conflict_check_nanos = run
            .search
            .abstract_completion
            .track_conflict_check_nanos
            .saturating_add(completed.times.evaluation);
        run.search.construction.add(&completed.search.construction);
        run.search.constructed_routes += completed.search.constructed_routes;
        run.search.delegated_routes += completed.search.delegated_routes;
        if let Some(work) = &completed.track_repair {
            run.search.fixed_track_segment_projections_computed +=
                work.segment_projections_computed;
            run.search.fixed_track_projection_pair_checks += work.projection_pair_checks;
        }
        run.search.legacy_candidates += completed.search.legacy_candidates;
        run.search.legacy_grid_pops += completed.search.legacy_grid_pops;
        for (id, pops) in &completed.search.legacy_grid_pops_by_route {
            *run.search
                .legacy_grid_pops_by_route
                .entry(id.clone())
                .or_default() += pops;
        }
        run.search.windowed_grids += completed.search.windowed_grids;
        run.search.delegated_nanos = run
            .search
            .delegated_nanos
            .saturating_add(completed.times.fallback);
        #[cfg(feature = "benchmark-timings")]
        run.search.delegation_reasons.extend(completed.delegated);
        run.search.routing_nanos = run
            .search
            .routing_nanos
            .saturating_add(elapsed_nanos(started));
        if (run.stop)() {
            return Err(Abort::Stopped);
        }
        let mut snapshot = completed.snapshot;
        apply_buses(&mut snapshot);
        snapshot.geometry_issues.extend(input.geometry_issues);
        snapshot.layout_diagnostics.push(if options.single_pass {
            format!(
                "Single pass: {} constructed, {} delegated drawn as seed routes, {} unresolved strict track pairs",
                completed.fixed.len(),
                completed.search.delegated_routes,
                completed.track_conflicts.len()
            )
        } else {
            format!(
                "Abstract routing: {} constructed, {} delegated, {} unresolved strict track pairs",
                completed.fixed.len(),
                completed.search.delegated_routes,
                completed.track_conflicts.len()
            )
        });
        let seeds: BTreeMap<_, _> = capacity_seeds
            .into_iter()
            .map(|seed| ((seed.area.clone(), seed.rank), seed))
            .collect();
        for channel in completed.channels {
            let Some(seed) = seeds.get(&(channel.area.clone(), channel.rank)) else {
                snapshot.layout_diagnostics.push(format!(
                    "Abstract capacity channel: area={} rank={} seed=unavailable physical_required={:.3} final_available={:.3} physical_missing={:.3}",
                    channel.area, channel.rank, channel.required, channel.available, channel.missing
                ));
                continue;
            };
            let (physical_tracks, seeded_width, initial_available) = (
                seed.physical_tracks,
                seed.seeded_width,
                seed.initial_available,
            );
            let late_shortfall = (channel.required - initial_available).max(0.);
            let late_shift = channel.available - initial_available;
            let final_surplus = (channel.available - channel.required).max(0.);
            snapshot.layout_diagnostics.push(format!(
                "Abstract capacity channel: area={} rank={} physical_tracks={} seeded_width={:.3} initial_available={:.3} physical_required={:.3} final_available={:.3} late_shift={:.3} late_shortfall={:.3} final_surplus={:.3} physical_missing={:.3}",
                channel.area,
                channel.rank,
                physical_tracks,
                seeded_width,
                initial_available,
                channel.required,
                channel.available,
                late_shift,
                late_shortfall,
                final_surplus,
                channel.missing
            ));
        }
        return Ok(snapshot);
    }
    // An edit that lays out only part of the drawing routes only the part it touched.
    let local = previous.filter(|_| dirty.len() < graph.nodes.len());
    let routing_started = stage_timer_start();
    // With the plans, the ports of the local routing follow their order as in the full routing (`route_all`).
    let local_ports = if options.layered_construction && !run.plans.is_empty() {
        port_context
            .clone()
            .with_order_keys(layered_plan::port_keys(&run.plans, graph))
            .with_end_sides(layered_plan::end_sides(
                &run.plans,
                graph,
                options.single_pass,
            ))
    } else {
        port_context.clone()
    };
    // Experimental (consistent layered pipeline): before the routes are built, their tracks are counted, and the channels
    // too narrow for them are widened (the drawing is placed again; at most three rounds, since the lanes an edge keeps
    // depend on the room the ranks leave), so that the construction builds once and searches no route that only failed for
    // want of room.
    if options.layered_construction
        && options.hierarchical_ordering
        && local.is_none()
        && !run.plans.is_empty()
    {
        for _ in 0..3 {
            let mut probe = count_narrow_channels(
                graph,
                &boxes,
                options,
                &port_context,
                &run.plans,
                &mut run.search,
            );
            if probe.narrow_channels.is_empty() {
                // The drawing is final: the routing below starts from this preparation.
                run.prepared = Some(probe);
                break;
            }
            for (area, rank, missing) in std::mem::take(&mut probe.narrow_channels) {
                *gaps.entry(channel_key(&area, rank)).or_insert(0.) += missing;
            }
            boxes = BTreeMap::new();
            changed.clear();
            let placement_started = stage_timer_start();
            run.search.next_placement_pass_reason = Some("legacy_channel_capacity_retry".into());
            place(
                None,
                graph,
                &index,
                &mut boxes,
                true,
                dirty,
                &mut changed,
                &mut orientations,
                optimize,
                options,
                minimum,
                &gaps,
                &mut run.search,
                &mut run.plans,
                &mut ordering,
                &mut run.capacity_seeds,
            );
            run.search.placement_nanos = run
                .search
                .placement_nanos
                .saturating_add(elapsed_nanos(placement_started));
            repair_issues = repair_and_replace(
                graph,
                &index,
                &port_context,
                options,
                &mut boxes,
                &mut repair_memo,
                (optimize, minimum, &gaps, &mut orientations),
                &mut run.search,
                &mut run.plans,
                &mut ordering,
                &mut run.capacity_seeds,
            );
        }
    }
    // The last bounded probe may have widened the drawing without probing its new coordinates. For the planned
    // constructor, its next preparation already determines late widening; no route on that drawing would survive it.
    let defer_routing = !channel
        && options.layered_construction
        && options.hierarchical_ordering
        && local.is_none()
        && !run.plans.is_empty()
        && run.prepared.is_none();
    #[cfg(test)]
    let defer_routing = defer_routing && !run.disable_phase_reuse;
    let deferred = if defer_routing {
        let probe = count_narrow_channels(
            graph,
            &boxes,
            options,
            &port_context,
            &run.plans,
            &mut run.search,
        );
        if probe.narrow_channels.is_empty() {
            run.prepared = Some(probe);
            false
        } else {
            run.narrow = probe.narrow_channels;
            run.search.deferred_routing_passes += 1;
            true
        }
    } else {
        false
    };
    let route_result = if deferred {
        None
    } else {
        Some(
            match local.and_then(|p| {
                local_routes(
                    graph,
                    &index,
                    &local_ports,
                    &boxes,
                    &Before {
                        graph: &p.graph,
                        boxes: &p.boxes,
                        routes: &p.routes,
                    },
                    options,
                    &mut run.search,
                    &run.plans,
                )
            }) {
                Some(routes) => Ok(routes),
                None => route_all(
                    graph,
                    &boxes,
                    optimize,
                    options,
                    channel,
                    run,
                    &port_context,
                ),
            },
        )
    };
    run.search.routing_nanos = run
        .search
        .routing_nanos
        .saturating_add(elapsed_nanos(routing_started));
    let mut routes = route_result.transpose()?;
    // Experimental (consistent layered pipeline): the channels the construction still found too narrow for their tracks
    // (after the count before the routing) are widened by what they miss, once: the ranks after them move along the flow and
    // the lanes across it stay.
    if options.layered_construction
        && options.hierarchical_ordering
        && local.is_none()
        && !run.narrow.is_empty()
    {
        for (area, rank, missing) in std::mem::take(&mut run.narrow) {
            *gaps.entry(channel_key(&area, rank)).or_insert(0.) += missing;
        }
        boxes = BTreeMap::new();
        changed.clear();
        let placement_started = stage_timer_start();
        run.search.next_placement_pass_reason = Some("constructor_capacity_retry".into());
        place(
            None,
            graph,
            &index,
            &mut boxes,
            true,
            dirty,
            &mut changed,
            &mut orientations,
            optimize,
            options,
            minimum,
            &gaps,
            &mut run.search,
            &mut run.plans,
            &mut ordering,
            &mut run.capacity_seeds,
        );
        run.search.placement_nanos = run
            .search
            .placement_nanos
            .saturating_add(elapsed_nanos(placement_started));
        repair_issues = repair_and_replace(
            graph,
            &index,
            &port_context,
            options,
            &mut boxes,
            &mut repair_memo,
            (optimize, minimum, &gaps, &mut orientations),
            &mut run.search,
            &mut run.plans,
            &mut ordering,
            &mut run.capacity_seeds,
        );
        let routing_started = stage_timer_start();
        routes = Some(route_all(
            graph,
            &boxes,
            optimize,
            options,
            channel,
            run,
            &port_context,
        )?);
        run.search.routing_nanos = run
            .search
            .routing_nanos
            .saturating_add(elapsed_nanos(routing_started));
    }
    let mut routes = routes.expect("deferred routing is completed after channel widening");
    // Corridor widening re-places the drawing from scratch and only layered areas read the gaps, for the routes that run in
    // corridors. With hierarchical ordering the channels are already as wide as their tracks (counted before the routing),
    // and the rounds are skipped: they only answered searched routes, and cost a placement and a routing each. When every area is organic, or every edge is drawn straight, the rounds change nothing but would drop the
    // warm start and repeat the stress and the repair, so they are skipped.
    if options.required_clearance.is_some()
        && !all_areas_organic(graph)
        && !styles::all_edges_straight(graph, options)
        && !options.hierarchical_ordering
    {
        // Space request (algorithm plan A2, A4a): the routes that are drawn show how many of them share a corridor. A
        // corridor that is too narrow for them, with a clearance on each side and a track for each extra route, is
        // widened in the placement, and the edges are routed again. A bounded number of rounds; what remains is
        // reported by the clearance and overlap checks, not hidden.
        for _ in 0..WIDENING_ROUNDS {
            let mut residual =
                corridor_gaps_counted(graph, &boxes, &routes, options, &mut run.search);
            // An edit that lays out only part of the drawing widens only what it touched: a corridor that was too narrow
            // before and lies in an area the edit did not place again stays as the earlier drawing left it.
            if local.is_some() {
                residual.retain(|(a, b), _| {
                    let parent = index.node(a).and_then(|n| n.parent_id.as_deref());
                    dirty.contains(a)
                        || dirty.contains(b)
                        || parent.is_some_and(|p| dirty.contains(p))
                });
            }
            if residual.is_empty() {
                break;
            }
            run.search.clearance_widening_rounds += 1;
            let mut widened = BTreeSet::new();
            for (pair, extra) in residual {
                if let Some(area) = index.node(&pair.0).and_then(|n| n.parent_id.clone()) {
                    widened.insert(area);
                }
                *gaps.entry(pair).or_insert(0.) += extra;
            }
            if local.is_some() {
                // Only the areas whose corridors are widened, and the containers above them, are placed again: the rest of
                // the drawing keeps its boxes, and only the routes the change touches are made again.
                let (earlier_boxes, earlier_routes) = (boxes.clone(), routes.clone());
                let mut again = dirty.clone();
                for area in widened {
                    let mut at = Some(area);
                    while let Some(id) = at {
                        at = index.node(&id).and_then(|n| n.parent_id.clone());
                        again.insert(id);
                    }
                }
                let placement_started = stage_timer_start();
                run.search.next_placement_pass_reason = Some("local_clearance_widening".into());
                place(
                    None,
                    graph,
                    &index,
                    &mut boxes,
                    false,
                    &again,
                    &mut changed,
                    &mut orientations,
                    optimize,
                    options,
                    minimum,
                    &gaps,
                    &mut run.search,
                    &mut run.plans,
                    &mut ordering,
                    &mut run.capacity_seeds,
                );
                run.search.placement_nanos = run
                    .search
                    .placement_nanos
                    .saturating_add(elapsed_nanos(placement_started));
                if same_box_geometries(&boxes, &earlier_boxes) {
                    run.search.clearance_widening_noop_rounds += 1;
                    routes = earlier_routes;
                    continue;
                }
                repair_issues = repair_and_replace(
                    graph,
                    &index,
                    &port_context,
                    options,
                    &mut boxes,
                    &mut repair_memo,
                    (optimize, minimum, &gaps, &mut orientations),
                    &mut run.search,
                    &mut run.plans,
                    &mut ordering,
                    &mut run.capacity_seeds,
                );
                let routing_started = stage_timer_start();
                let route_result = match local_routes(
                    graph,
                    &index,
                    &port_context,
                    &boxes,
                    &Before {
                        graph,
                        boxes: &earlier_boxes,
                        routes: &earlier_routes,
                    },
                    options,
                    &mut run.search,
                    &run.plans,
                ) {
                    Some(routes) => Ok(routes),
                    None => route_all(
                        graph,
                        &boxes,
                        optimize,
                        options,
                        channel,
                        run,
                        &port_context,
                    ),
                };
                run.search.routing_nanos = run
                    .search
                    .routing_nanos
                    .saturating_add(elapsed_nanos(routing_started));
                routes = route_result?;
                continue;
            }
            let (earlier_boxes, earlier_routes) = (boxes.clone(), routes.clone());
            let mut again = BTreeSet::new();
            for area in widened {
                let mut at = Some(area);
                while let Some(id) = at {
                    at = index.node(&id).and_then(|n| n.parent_id.clone());
                    again.insert(id);
                }
            }
            if !options.incremental_clearance_widening {
                boxes = BTreeMap::new();
                changed.clear();
            }
            let (full, dirty_areas) = if options.incremental_clearance_widening {
                (false, &again)
            } else {
                (true, dirty)
            };
            let placement_started = stage_timer_start();
            run.search.next_placement_pass_reason = Some("clearance_widening".into());
            place(
                None,
                graph,
                &index,
                &mut boxes,
                full,
                dirty_areas,
                &mut changed,
                &mut orientations,
                optimize,
                options,
                minimum,
                &gaps,
                &mut run.search,
                &mut run.plans,
                &mut ordering,
                &mut run.capacity_seeds,
            );
            run.search.placement_nanos = run
                .search
                .placement_nanos
                .saturating_add(elapsed_nanos(placement_started));
            if same_box_geometries(&boxes, &earlier_boxes) {
                run.search.clearance_widening_noop_rounds += 1;
                routes = earlier_routes;
                continue;
            }
            // The placement was rebuilt, so the root is repaired again before its routes are made.
            repair_issues = repair_and_replace(
                graph,
                &index,
                &port_context,
                options,
                &mut boxes,
                &mut repair_memo,
                (optimize, minimum, &gaps, &mut orientations),
                &mut run.search,
                &mut run.plans,
                &mut ordering,
                &mut run.capacity_seeds,
            );
            let routing_started = stage_timer_start();
            let route_result = if options.incremental_clearance_widening {
                match local_routes(
                    graph,
                    &index,
                    &port_context,
                    &boxes,
                    &Before {
                        graph,
                        boxes: &earlier_boxes,
                        routes: &earlier_routes,
                    },
                    options,
                    &mut run.search,
                    &run.plans,
                ) {
                    Some(routes) => Ok(routes),
                    None => route_all(
                        graph,
                        &boxes,
                        optimize,
                        options,
                        channel,
                        run,
                        &port_context,
                    ),
                }
            } else {
                route_all(
                    graph,
                    &boxes,
                    optimize,
                    options,
                    channel,
                    run,
                    &port_context,
                )
            };
            run.search.routing_nanos = run
                .search
                .routing_nanos
                .saturating_add(elapsed_nanos(routing_started));
            routes = route_result?;
        }
    }
    let mut geometry_issues =
        arqlet_scene::validate_geometry(graph, &boxes, arqlet_scene::GeometryRules::default());
    geometry_issues.extend(repair_issues);
    let mut snapshot = placed_snapshot(
        graph,
        previous,
        options,
        boxes,
        routes,
        orientations,
        &gaps,
        geometry_issues,
        changed,
        &run.plans,
    );
    finish_routes(&mut snapshot, true);
    labels::decorate(&mut snapshot);
    Ok(snapshot)
}

#[allow(clippy::too_many_arguments)]
fn placed_snapshot(
    graph: &Graph,
    previous: Option<&Snapshot>,
    options: &LayoutOptions,
    boxes: BTreeMap<String, Rect>,
    routes: Vec<Route>,
    orientations: BTreeMap<String, Orientation>,
    gaps: &BTreeMap<(String, String), f64>,
    geometry_issues: Vec<arqlet_model::GeometryIssue>,
    mut changed: Vec<String>,
    plans: &layered_plan::LayeredPlans,
) -> Snapshot {
    Snapshot {
        labels: vec![],
        layout_diagnostics: Vec::new(),
        // Each corridor that strict clearance had to widen is reported, so the bottleneck is visible.
        edit_report: None,
        area_orientations: orientations,
        area_strategies: Default::default(),
        corridor_gaps: gaps
            .iter()
            .filter(|((a, b), _)| a < b)
            .map(|((a, b), extra)| arqlet_model::CorridorGap {
                before: a.clone(),
                after: b.clone(),
                extra: *extra,
            })
            .collect(),
        routing_issues: pin_diagnostics(graph, options)
            .into_iter()
            .chain(
                gaps.iter()
                    .filter(|((a, b), _)| a < b)
                    .map(|((a, b), extra)| {
                        format!(
                            "Corridor between {a} and {b} widened by {extra:.0} for the routes through it"
                        )
                    }),
            )
            .collect(),
        route_conflicts: vec![],
        resolved_ports: vec![],
        boundary_crossings: vec![],
        straight_edges: vec![],
        bus_boxes: BTreeMap::new(),
        gesture: None,
        layered_plans: plans
            .iter()
            .map(|(area, plan)| (area.clone(), plan.to_stored()))
            .collect(),
        options: options.clone(),
        source_graph: None,
        collapsed_ids: vec![],
        schema_version: 1,
        geometry_issues,
        optimization: None,
        layout_version: previous.map_or(0, |s| s.layout_version + 1),
        engine_version: arqlet_model::ENGINE_VERSION.into(),
        revision: previous.map_or(0, |s| s.revision + 1),
        base_revision: previous.map(|s| s.revision),
        graph: graph.clone(),
        boxes,
        routes,
        recomputed_containers: {
            let mut seen = BTreeSet::new();
            changed.retain(|c| seen.insert(c.clone()));
            changed
        },
        omitted_unresolved_edges: previous.map_or(0, |s| s.omitted_unresolved_edges),
    }
}

/// Selects the layered pipeline only when the view has at least one direction-based area. Other area modes keep their
/// strategy-specific placement; a view containing only those modes uses the previous pipeline and records why.
fn select_pipeline(
    graph: &Graph,
    options: &LayoutOptions,
) -> Result<(LayoutOptions, Vec<String>), String> {
    if !options.layered_construction {
        return Ok((options.clone(), Vec::new()));
    }
    let mut fallback_reasons = Vec::new();
    if options.placement_grid > 0. {
        fallback_reasons.push(
            "The hierarchical pipeline does not yet support placementGrid; the previous layout pipeline was used"
                .to_string(),
        );
    }
    if options.placement_search > 0 {
        fallback_reasons.push(
            "The hierarchical pipeline does not yet support placementSearch; the previous layout pipeline was used"
                .to_string(),
        );
    }
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|node| node.parent_id.as_deref())
        .collect();
    if options
        .pins
        .keys()
        .any(|id| !containers.contains(id.as_str()))
    {
        fallback_reasons.push(
            "The hierarchical pipeline does not yet support pinned leaf placement; the previous layout pipeline was used"
                .to_string(),
        );
    }
    if !fallback_reasons.is_empty() {
        let mut selected = options.clone();
        selected.layered_construction = false;
        selected.hierarchical_ordering = false;
        return Ok((selected, fallback_reasons));
    }

    let areas: BTreeSet<Option<&str>> = std::iter::once(None)
        .chain(containers.into_iter().map(Some))
        .collect();
    let mut ranked_area = false;
    for area in areas {
        match graph.area_mode(area) {
            Ok(Mode::Orientation(_) | Mode::Chooser(_)) => ranked_area = true,
            Ok(_) => {}
            Err(error) => return Err(error),
        }
    }
    if ranked_area {
        return Ok((options.clone(), Vec::new()));
    }

    let mut selected = options.clone();
    selected.layered_construction = false;
    selected.hierarchical_ordering = false;
    Ok((
        selected,
        vec![
            "No direction-based hierarchical areas are available; the previous layout pipeline was used"
                .into(),
        ],
    ))
}
/// Experimental (consistent layered pipeline): whether the edge from `source` to `target` runs against the flow where its
/// ends meet: the plan of the smallest area that holds both reversed the link between the children that hold them (the
/// layered method's cycle break). Read from the plans at hand, which during a placement are those of the pass before for
/// the areas above the one being placed; `false` while that area has no plan yet.
fn reversed_above(
    index: &GraphIndex<'_>,
    plans: &layered_plan::LayeredPlans,
    reversals: &BTreeMap<String, BTreeSet<(String, String)>>,
    source: &str,
    target: &str,
) -> bool {
    // The nodes from each end up to the top, by index (the root area after the last).
    let path = |id: &str| -> Option<Vec<usize>> {
        let mut out = vec![index.index_of(id)?];
        while let Some(p) = index.parents[*out.last().expect("a node")] {
            out.push(p);
        }
        Some(out)
    };
    let (Some(s), Some(t)) = (path(source), path(target)) else {
        return false;
    };
    // The smallest common area (a node, or the root after both paths), and the elements below it on either side.
    let (k, j, area) = match s.iter().position(|a| t.contains(a)) {
        Some(k) => (
            k,
            t.iter().position(|a| *a == s[k]).expect("common"),
            index.nodes[s[k]].id.as_str(),
        ),
        None => (s.len(), t.len(), "<root>"),
    };
    if k == 0 || j == 0 {
        return false;
    }
    layered_plan::reversed_links(plans, reversals, area).is_some_and(|reversed_links| {
        reversed_links.contains(&(
            index.nodes[s[k - 1]].id.clone(),
            index.nodes[t[j - 1]].id.clone(),
        ))
    })
}

/// Experimental: the stored plans `earlier` of the drawing a pass continues (`partial`: an edit that places only the dirty
/// areas) or refines, split into the plans the pass keeps and those that only give the order a placement starts from. A
/// partial pass keeps the plan of an area that still exists, is not dirty and whose plan holds exactly the area's children
/// (besides dummies and border nodes); the root is always placed again. Plans of another format are dropped.
fn reused_plans(
    earlier: &BTreeMap<String, arqlet_model::StoredAreaPlan>,
    partial: bool,
    index: &GraphIndex<'_>,
    dirty: &BTreeSet<String>,
) -> (layered_plan::LayeredPlans, layered_plan::LayeredPlans) {
    let (mut kept, mut starts) = (
        layered_plan::LayeredPlans::new(),
        layered_plan::LayeredPlans::new(),
    );
    for (area, stored) in earlier {
        if area != "<root>" && index.node(area).is_none() {
            continue;
        }
        let Some(plan) = layered_plan::AreaPlan::from_stored(stored) else {
            continue;
        };
        if !partial || area == "<root>" || dirty.contains(area) {
            starts.insert(area.clone(), plan);
            continue;
        }
        let children: BTreeSet<&str> = index
            .children_of(Some(area))
            .iter()
            .map(|&i| index.nodes[i].id.as_str())
            .collect();
        let members: BTreeSet<&str> = plan
            .ranks
            .iter()
            .flatten()
            .map(String::as_str)
            .filter(|id| !id.starts_with(DUMMY_PREFIX))
            .collect();
        if members == children {
            kept.insert(area.clone(), plan);
        } else {
            starts.insert(area.clone(), plan);
        }
    }
    (kept, starts)
}

// Eight arguments: the recursive placement context. Splitting it would obscure the recursion.
/// The links the cycle break reverses in every layered area, from the ranks alone; `None` when an area is flattened (global
/// layering), whose ranks follow from its containers' plans.
fn known_reversals(
    graph: &Graph,
    index: &GraphIndex<'_>,
    options: &LayoutOptions,
) -> Option<BTreeMap<String, BTreeSet<(String, String)>>> {
    let mut out = BTreeMap::new();
    for parent in std::iter::once(None).chain(index.nodes.iter().map(|n| Some(n.id.as_str()))) {
        let children: Vec<&Node> = index
            .children_of(parent)
            .iter()
            .map(|&i| index.nodes[i])
            .collect();
        if children.is_empty()
            || !matches!(
                direction_for(graph, parent),
                Mode::Orientation(_) | Mode::Chooser(_)
            )
        {
            continue;
        }
        if !global::Flattening::of(parent, graph, index, options, &children).is_empty() {
            return None;
        }
        let (_, links, ranks) = area_ranks(&children, index, options, false);
        out.insert(
            parent.unwrap_or("<root>").to_string(),
            links
                .into_iter()
                .filter(|(a, b)| ranks[b] < ranks[a])
                .collect(),
        );
    }
    Some(out)
}

/// The child of an area that holds a node (the node itself or an ancestor), by position among the area's children.
fn owner_among(
    child_position: &[Option<usize>],
    index: &GraphIndex<'_>,
    id: usize,
) -> Option<usize> {
    let mut current = Some(id);
    while let Some(node) = current {
        if let Some(position) = child_position[node] {
            return Some(position);
        }
        current = index.parents[node];
    }
    None
}

/// The links between an area's children and their ranks (the cycle break, and network simplex when chosen). The ranks depend
/// only on the area's own children, so the links a cycle break reverses are known before any area is placed.
#[allow(clippy::type_complexity)]
fn area_ranks(
    children: &[&Node],
    index: &GraphIndex<'_>,
    options: &LayoutOptions,
    flattened: bool,
) -> (
    Vec<Option<usize>>,
    BTreeSet<(String, String)>,
    BTreeMap<String, usize>,
) {
    let child_ids: BTreeSet<_> = children.iter().map(|n| n.id.clone()).collect();
    let mut child_position = vec![None; index.nodes.len()];
    for (position, child) in children.iter().enumerate() {
        child_position[index.index_of(&child.id).expect("indexed graph child")] = Some(position);
    }
    let owner = |id: usize| owner_among(&child_position, index, id);
    let mut links = BTreeSet::new();
    for (source, targets) in index.adjacency.iter().enumerate() {
        let Some(a) = owner(source) else {
            continue;
        };
        for &target in targets {
            if let Some(b) = owner(target) {
                if a != b {
                    links.insert((children[a].id.clone(), children[b].id.clone()));
                }
            }
        }
    }
    // Kahn ordering with a deterministic cycle break. Logical edge directions stay intact. Each step takes the first child in
    // id order with no link from a child still remaining, or the first remaining child when every one has such a link (a
    // cycle). Kept with in-degrees and per-child link lists in id order, so a step does not scan every link.
    let ids: Vec<&String> = child_ids.iter().collect();
    let slot: BTreeMap<&String, usize> = ids.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    let mut outgoing: Vec<Vec<usize>> = vec![Vec::new(); ids.len()];
    let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); ids.len()];
    for (a, b) in &links {
        let (i, j) = (slot[a], slot[b]);
        outgoing[i].push(j);
        incoming[j].push(i);
    }
    let mut indegree: Vec<usize> = incoming.iter().map(Vec::len).collect();
    let mut remaining: BTreeSet<usize> = (0..ids.len()).collect();
    let mut free: BTreeSet<usize> = (0..ids.len()).filter(|&i| indegree[i] == 0).collect();
    let mut rank_of = vec![0usize; ids.len()];
    let mut kahn_slots: Vec<usize> = Vec::with_capacity(ids.len());
    while let Some(&first) = remaining.first() {
        let next = free.first().copied().unwrap_or(first);
        remaining.remove(&next);
        free.remove(&next);
        kahn_slots.push(next);
        let rank = rank_of[next];
        for &b in &outgoing[next] {
            if remaining.contains(&b) {
                rank_of[b] = rank_of[b].max(rank + 1);
                indegree[b] -= 1;
                if indegree[b] == 0 {
                    free.insert(b);
                }
            }
        }
    }
    // Global layering: the cycles of the flattened containers run across levels, and a link whose target the cycle break
    // took first would stay inside one rank. The order the nodes were taken in is a topological order once every link
    // against it is reversed; the ranks are the longest paths along it, so that no link lies inside a rank. The single pass
    // needs the same: a link inside a rank has no chain, so its edge could not be constructed.
    if flattened || options.single_pass {
        let mut position = vec![0usize; ids.len()];
        for (p, &i) in kahn_slots.iter().enumerate() {
            position[i] = p;
        }
        // Every link oriented along the order, listed at its later end.
        let mut before: Vec<Vec<usize>> = vec![Vec::new(); ids.len()];
        for (i, targets) in outgoing.iter().enumerate() {
            for &j in targets {
                let (from, to) = if position[i] < position[j] {
                    (i, j)
                } else {
                    (j, i)
                };
                before[to].push(from);
            }
        }
        for &id in &kahn_slots {
            rank_of[id] = before[id]
                .iter()
                .map(|&from| rank_of[from] + 1)
                .max()
                .unwrap_or(0);
        }
    }
    let mut ranks: BTreeMap<String, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| ((*id).clone(), rank_of[i]))
        .collect();
    let kahn_order: Vec<String> = kahn_slots.iter().map(|&i| ids[i].clone()).collect();
    // Experimental (consistent layered pipeline, option `simplexRanks`): the ranks minimise the summed spans of the links,
    // weighted by their edges (network simplex), for the links oriented as the cycle break took the nodes.
    if options.hierarchical_ordering && options.simplex_ranks && !flattened && !links.is_empty() {
        let position: BTreeMap<&String, usize> = kahn_order
            .iter()
            .enumerate()
            .map(|(i, id)| (id, i))
            .collect();
        let mut weight: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        for &ends in &index.edge_ends {
            let (Some(s), Some(t)) = ends else {
                continue;
            };
            if let (Some(a), Some(b)) = (owner(s), owner(t)) {
                if a == b {
                    continue;
                }
                let (pa, pb) = (position[&children[a].id], position[&children[b].id]);
                let key = if pa < pb { (pa, pb) } else { (pb, pa) };
                *weight.entry(key).or_insert(0.) += 1.;
            }
        }
        let oriented: Vec<(usize, usize, f64)> =
            weight.into_iter().map(|((a, b), w)| (a, b, w)).collect();
        let solved = ranking::network_simplex(kahn_order.len(), &oriented);
        for (i, id) in kahn_order.iter().enumerate() {
            ranks.insert(id.clone(), solved[i]);
        }
    }
    (child_position, links, ranks)
}

#[allow(clippy::too_many_arguments)]
fn place(
    parent: Option<&str>,
    graph: &Graph,
    index: &GraphIndex<'_>,
    boxes: &mut BTreeMap<String, Rect>,
    full: bool,
    dirty: &BTreeSet<String>,
    changed: &mut Vec<String>,
    orientations: &mut BTreeMap<String, Orientation>,
    optimize: usize,
    options: &LayoutOptions,
    minimum: &BTreeMap<String, (f64, f64)>,
    gaps: &BTreeMap<(String, String), f64>,
    counters: &mut SearchCounters,
    plans: &mut layered_plan::LayeredPlans,
    ordering: &mut placement_order::Memo,
    capacity_seeds: &mut Vec<CapacitySeed>,
) -> (f64, f64) {
    if parent.is_none() {
        counters.placement_passes += 1;
        let reason = counters
            .next_placement_pass_reason
            .take()
            .unwrap_or_else(|| "unclassified".into());
        *counters.placement_pass_reasons.entry(reason).or_default() += 1;
        capacity_seeds.clear();
    }
    if let Some(p) = parent {
        if !full && !dirty.contains(p) {
            if let Some(r) = boxes.get(p) {
                return (r.width, r.height);
            }
        }
    }
    let children: Vec<&Node> = index
        .children_of(parent)
        .iter()
        .map(|&i| index.nodes[i])
        .collect();
    // Experimental (global layering, iteration 3): the containers of the same flow axis inside this area, down to the leaves,
    // take part with their children in this area's ranks, each as one contiguous block; the leaves (and the containers of
    // another axis or rule) are the units of the arrangement instead of the children.
    let flattening = global::Flattening::of(parent, graph, index, options, &children);
    let children: Vec<&Node> = if flattening.is_empty() {
        children
    } else {
        flattening.units(index, children)
    };
    if children.is_empty() {
        let rows = parent
            .and_then(|p| options.node_rows.get(p))
            .filter(|r| !r.is_empty());
        // The outline of the node decides the box it occupies: a square or a circle is as large on both axes.
        let shape = parent
            .and_then(|p| index.node(p))
            .map(|n| n.shape)
            .unwrap_or_default();
        let (mut width, mut height) =
            match (rows, parent, options.node_sizes.get(parent.unwrap_or(""))) {
                (Some(rows), _, _) => {
                    let width = rows.iter().map(|r| r.width).fold(0., f64::max)
                        + 2. * arqlet_model::ROW_PADDING;
                    let height = rows.iter().map(|r| r.height).sum::<f64>()
                        + arqlet_model::ROW_GAP * (rows.len() - 1) as f64
                        + 2. * arqlet_model::ROW_PADDING;
                    (width, height)
                }
                (None, Some(_), Some(size)) => (size.width, size.height),
                // Without a measured size a circle takes a modest diameter: fitting the rectangle default (180 x 54) to a
                // circle would make it 180 wide. The consumer's measurement replaces it with the size of the drawn label.
                _ if shape == arqlet_model::NodeShape::Circle => {
                    // The label is drawn at 13 px, about 7.5 units a character (the viewer shows 23 at most): the diameter
                    // is the diagonal of the label's box, so the text lies in the disc.
                    let letters = parent
                        .and_then(|p| index.node(p))
                        .map_or(0, |n| n.label.chars().count().min(23))
                        as f64;
                    let diameter =
                        DEFAULT_CIRCLE_DIAMETER.max((7.5 * letters + 16.).hypot(22.).ceil());
                    (diameter, diameter)
                }
                _ => (180., 54.),
            };
        if let Some((min_width, min_height)) = parent.and_then(|p| minimum.get(p)) {
            width = width.max(*min_width);
            height = height.max(*min_height);
        }
        // A collapsed container takes the size estimated from its content (global layering, iteration 1).
        if let Some(size) = parent.and_then(|p| options.collapsed_sizes.get(p)) {
            width = width.max(size.width);
            height = height.max(size.height);
        }
        (width, height) = shape.fit(width, height);
        let grid = options.placement_grid;
        return (snap(width, grid), snap(height, grid));
    }
    changed.push(parent.unwrap_or("<root>").into());
    let mut sizes = BTreeMap::new();
    for n in &children {
        sizes.insert(
            n.id.clone(),
            place(
                Some(&n.id),
                graph,
                index,
                boxes,
                full,
                dirty,
                changed,
                orientations,
                optimize,
                options,
                minimum,
                gaps,
                counters,
                plans,
                ordering,
                capacity_seeds,
            ),
        );
    }
    // A view within a node budget continues its earlier drawing (an expanded container, global layering plan step 2): an
    // area whose children all have their earlier boxes keeps their places, and a child that grew pushes the children right
    // of it and below it on by its growth, instead of arranging the area again.
    if options.visible_node_budget > 0
        && !full
        && flattening.is_empty()
        && children.iter().all(|c| boxes.contains_key(&c.id))
    {
        return push_grown(parent, &children, &sizes, boxes, plans);
    }
    let layered = matches!(
        direction_for(graph, parent),
        Mode::Orientation(_) | Mode::Chooser(_)
    );
    let reversals = ordering.reversals.clone();
    let structural_input = placement_structure::Input::new(
        parent, &children, graph, index, options, plans, &reversals,
    );
    let build_structure = || {
        let mut virtual_sizes = BTreeMap::new();
        let (child_position, links, mut ranks) =
            area_ranks(&children, index, options, !flattening.is_empty());
        let owner = |id: usize| owner_among(&child_position, index, id);
        let mut levels: BTreeMap<usize, Vec<String>> = BTreeMap::new();
        for n in &children {
            levels.entry(ranks[&n.id]).or_default().push(n.id.clone());
        }
        // Experimental (plan section 4a, `layeredConstruction`): long-edge dummies. A link that skips ranks gets a dummy in every
        // rank it passes, as wide as the tracks of its edges. The dummies take part in the order (the link is replaced by the
        // chain through them) and take a place in their rank, so the passage is reserved; they are removed from the result, and the
        // place they held stays free. Without the option the order uses the links as they are.
        let mut order_links = links.clone();
        // Experimental: the dummies of every link in flow direction, and the links the cycle break reversed (the area's plan).
        let mut chains: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        let mut reversed_links: BTreeSet<(String, String)> = BTreeSet::new();
        let mut borders: BTreeMap<String, String> = BTreeMap::new();
        let mut sides: BTreeMap<String, String> = BTreeMap::new();
        // The edges of every link (as `multiplicity` counts them), for the tracks of the channels.
        let mut link_edges: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        if options.layered_construction && layered {
            let mut multiplicity: BTreeMap<(String, String), usize> = BTreeMap::new();
            for (edge, &ends) in graph.edges.iter().zip(&index.edge_ends) {
                let (Some(s), Some(t)) = ends else {
                    continue;
                };
                if let (Some(a), Some(b)) = (owner(s), owner(t)) {
                    if a != b {
                        *multiplicity
                            .entry((children[a].id.clone(), children[b].id.clone()))
                            .or_insert(0) += 1;
                        link_edges
                            .entry((children[a].id.clone(), children[b].id.clone()))
                            .or_default()
                            .push(edge.id.clone());
                    }
                }
            }
            // A backward link takes part reversed, as in the layered method's cycle break: its dummies and its place in the order
            // are those of the forward link from its target to its source.
            let backward: Vec<(String, String)> = links
                .iter()
                .filter(|(a, b)| ranks[b] < ranks[a])
                .cloned()
                .collect();
            for (a, b) in &backward {
                reversed_links.insert((a.clone(), b.clone()));
                order_links.remove(&(a.clone(), b.clone()));
                order_links.insert((b.clone(), a.clone()));
                let m = multiplicity.remove(&(a.clone(), b.clone())).unwrap_or(0);
                *multiplicity.entry((b.clone(), a.clone())).or_insert(0) += m;
                let moved = link_edges
                    .remove(&(a.clone(), b.clone()))
                    .unwrap_or_default();
                link_edges
                    .entry((b.clone(), a.clone()))
                    .or_default()
                    .extend(moved);
            }
            // Border nodes (step 6): in a container whose flow runs along the same axis as its parent's, an edge between a child
            // and the outside gets a virtual node in an extra rank at the border it leaves (after the last rank) or enters (before
            // the first). Its link to the child takes part in the order and the placement like any other, so the container's
            // border order follows its inside.
            let same_axis_container = parent.is_some_and(|p| {
                let outer = index.node(p).and_then(|n| n.parent_id.as_deref());
                match (direction_for(graph, parent), direction_for(graph, outer)) {
                    (Mode::Orientation(inner), Mode::Orientation(outer)) => {
                        inner.vertical() == outer.vertical()
                            && !inner.reversed()
                            && !outer.reversed()
                    }
                    _ => false,
                }
            });
            // Side nodes (single pass, ADR 0017): in a container whose flow runs across its parent's, an edge between a child
            // and the outside leaves or enters on a side the parent's flow uses, through a virtual node in the rank after or
            // before its child, last or first across the flow. Only under single pass, so the other pipelines are unchanged.
            let cross_axis_container = options.single_pass
                && parent.is_some_and(|p| {
                    let outer = index.node(p).and_then(|n| n.parent_id.as_deref());
                    match (direction_for(graph, parent), direction_for(graph, outer)) {
                        (Mode::Orientation(inner), Mode::Orientation(outer)) => {
                            inner.vertical() != outer.vertical()
                                && !inner.reversed()
                                && !outer.reversed()
                        }
                        _ => false,
                    }
                });
            if same_axis_container || cross_axis_container {
                let mut leaving: Vec<(String, Option<String>)> = Vec::new();
                let mut entering: Vec<(String, Option<String>)> = Vec::new();
                for (edge, &ends) in graph.edges.iter().zip(&index.edge_ends) {
                    let (Some(target), (Some(s), Some(t))) = (edge.target.as_deref(), ends) else {
                        continue;
                    };
                    // Experimental (consistent layered pipeline): an edge that runs against the flow where its ends meet is drawn
                    // as a forward edge from its target to its source, so it enters a container that holds its source and leaves
                    // one that holds its target.
                    // Only an edge that crosses the area's border needs it (the lookup walks the areas above).
                    let (from, to) = (owner(s), owner(t));
                    let crosses = match (from, to) {
                        (Some(_), None) | (None, Some(_)) => true,
                        (None, None) => {
                            options.hierarchical_ordering
                                && edge.source != target
                                && (parent == Some(edge.source.as_str()) || parent == Some(target))
                        }
                        _ => false,
                    };
                    if !crosses {
                        continue;
                    }
                    let backward = options.hierarchical_ordering
                        && reversed_above(index, plans, &reversals, &edge.source, target);
                    match (from, to) {
                        (Some(a), None) if backward => {
                            entering.push((edge.id.clone(), Some(children[a].id.clone())))
                        }
                        (None, Some(b)) if backward => {
                            leaving.push((edge.id.clone(), Some(children[b].id.clone())))
                        }
                        (Some(a), None) => {
                            leaving.push((edge.id.clone(), Some(children[a].id.clone())))
                        }
                        (None, Some(b)) => {
                            entering.push((edge.id.clone(), Some(children[b].id.clone())))
                        }
                        // Experimental (consistent layered pipeline): an edge of the container itself to the outside has a
                        // border node too, without a link inside; its port takes the node's lane, among the others.
                        (None, None) if options.hierarchical_ordering && edge.source != target => {
                            let (out, into) = if backward {
                                (&mut entering, &mut leaving)
                            } else {
                                (&mut leaving, &mut entering)
                            };
                            if parent == Some(edge.source.as_str()) {
                                out.push((edge.id.clone(), None));
                            } else if parent == Some(target) {
                                into.push((edge.id.clone(), None));
                            }
                        }
                        _ => {}
                    }
                }
                let side = snap(options.track_spacing, options.placement_grid);
                // ADR 0017, amendment A3: an edge that leaves this area of the other axis than its parent, and its parent too,
                // into an area of this area's axis crosses the parent beside this area when this area stands outermost on that
                // side of its rank there (in the parent's plan of the earlier pass). It then leaves this area by a border node
                // in its own flow, as across a border of the same axis, and the parent's side node stands beside this area.
                let through: BTreeSet<String> = if cross_axis_container {
                    through_edges(
                        graph,
                        index,
                        plans,
                        parent.expect("a container"),
                        (&entering, &leaving),
                    )
                } else {
                    BTreeSet::new()
                };
                let (border_entering, entering): (Vec<_>, Vec<_>) = entering
                    .into_iter()
                    .partition(|(e, _)| !cross_axis_container || through.contains(e));
                let (border_leaving, leaving): (Vec<_>, Vec<_>) = leaving
                    .into_iter()
                    .partition(|(e, _)| !cross_axis_container || through.contains(e));
                if cross_axis_container {
                    // A side node per edge with a child inside; an edge of the container itself stays without one.
                    let entering: Vec<(&String, &String)> = entering
                        .iter()
                        .filter_map(|(e, c)| Some((e, c.as_ref()?)))
                        .collect();
                    let leaving: Vec<(&String, &String)> = leaving
                        .iter()
                        .filter_map(|(e, c)| Some((e, c.as_ref()?)))
                        .collect();
                    if entering.iter().any(|(_, child)| ranks[*child] == 0) {
                        for r in ranks.values_mut() {
                            *r += 1;
                        }
                        levels = levels.into_iter().map(|(r, ids)| (r + 1, ids)).collect();
                    }
                    for (edge, child) in entering {
                        let node = format!("{SIDE_IN_PREFIX}{edge}");
                        let rank = ranks[child] - 1;
                        levels.entry(rank).or_default().insert(0, node.clone());
                        ranks.insert(node.clone(), rank);
                        virtual_sizes.insert(node.clone(), (side, side));
                        order_links.insert((node.clone(), child.clone()));
                        multiplicity.insert((node.clone(), child.clone()), 1);
                        link_edges.insert((node.clone(), child.clone()), vec![edge.clone()]);
                        sides.insert(edge.clone(), node);
                    }
                    for (edge, child) in leaving {
                        let node = format!("{SIDE_OUT_PREFIX}{edge}");
                        let rank = ranks[child] + 1;
                        levels.entry(rank).or_default().push(node.clone());
                        ranks.insert(node.clone(), rank);
                        virtual_sizes.insert(node.clone(), (side, side));
                        order_links.insert((child.clone(), node.clone()));
                        multiplicity.insert((child.clone(), node.clone()), 1);
                        link_edges.insert((child.clone(), node.clone()), vec![edge.clone()]);
                        sides.insert(edge.clone(), node);
                    }
                    // The side nodes of one rank share their place along the flow; each is as long along it as their ports
                    // need (`prepare_leg` staggers them a track apart), so the ports stay within the rank.
                    let along_y = matches!(direction_for(graph, parent), Mode::Orientation(o) if o.vertical());
                    for ids in levels.values() {
                        for kind in [SIDE_IN_PREFIX, SIDE_OUT_PREFIX] {
                            let group: Vec<&String> =
                                ids.iter().filter(|id| id.starts_with(kind)).collect();
                            let length = side * group.len() as f64;
                            for id in group {
                                virtual_sizes.insert(
                                    id.clone(),
                                    if along_y {
                                        (side, length)
                                    } else {
                                        (length, side)
                                    },
                                );
                            }
                        }
                    }
                }
                if !border_entering.is_empty() || !border_leaving.is_empty() {
                    let (entering, leaving) = (border_entering, border_leaving);
                    if !entering.is_empty() {
                        // The entering border takes rank 0; every other rank moves one on.
                        for r in ranks.values_mut() {
                            *r += 1;
                        }
                        levels = levels.into_iter().map(|(r, ids)| (r + 1, ids)).collect();
                    }
                    let last = levels.keys().copied().max().unwrap_or(0) + 1;
                    for (edge, child) in &entering {
                        let node = format!("{BORDER_PREFIX}{edge}");
                        levels.entry(0).or_default().push(node.clone());
                        ranks.insert(node.clone(), 0);
                        virtual_sizes.insert(node.clone(), (side, side));
                        if let Some(child) = child {
                            order_links.insert((node.clone(), child.clone()));
                            multiplicity.insert((node.clone(), child.clone()), 1);
                            link_edges.insert((node.clone(), child.clone()), vec![edge.clone()]);
                        }
                        borders.insert(edge.clone(), node);
                    }
                    for (edge, child) in &leaving {
                        let node = format!("{BORDER_PREFIX}{edge}");
                        levels.entry(last).or_default().push(node.clone());
                        ranks.insert(node.clone(), last);
                        virtual_sizes.insert(node.clone(), (side, side));
                        if let Some(child) = child {
                            order_links.insert((child.clone(), node.clone()));
                            multiplicity.insert((child.clone(), node.clone()), 1);
                            link_edges.insert((child.clone(), node.clone()), vec![edge.clone()]);
                        }
                        borders.insert(edge.clone(), node);
                    }
                }
            }
            let normalized: BTreeSet<(String, String)> = order_links.clone();
            for (a, b) in &normalized {
                let (ra, rb) = (ranks[a], ranks[b]);
                if rb == ra + 1 {
                    chains.insert((a.clone(), b.clone()), Vec::new());
                }
                if rb <= ra + 1 {
                    continue;
                }
                let tracks = multiplicity
                    .get(&(a.clone(), b.clone()))
                    .copied()
                    .unwrap_or(1)
                    .max(1);
                let side = snap(
                    tracks as f64 * options.track_spacing,
                    options.placement_grid,
                );
                order_links.remove(&(a.clone(), b.clone()));
                let mut previous = a.clone();
                let mut chain = Vec::new();
                for rank in ra + 1..rb {
                    let dummy = format!("{DUMMY_PREFIX}{a}->{b}@{rank}");
                    levels.entry(rank).or_default().push(dummy.clone());
                    ranks.insert(dummy.clone(), rank);
                    virtual_sizes.insert(dummy.clone(), (side, side));
                    order_links.insert((previous, dummy.clone()));
                    chain.push(dummy.clone());
                    previous = dummy;
                }
                order_links.insert((previous, b.clone()));
                chains.insert((a.clone(), b.clone()), chain);
            }
        }
        // Component packing (opt-in): each weakly connected component of the children is ordered on its own, and the
        // components occupy consecutive blocks of every level, so they do not interleave along the cross axis. With the
        // option off every component index is 0, and the orderings below are unchanged.
        let component_of: BTreeMap<String, usize> = if options.component_packing {
            let index: BTreeMap<&str, usize> = children
                .iter()
                .enumerate()
                .map(|(i, n)| (n.id.as_str(), i))
                .collect();
            let pairs: Vec<(usize, usize)> = links
                .iter()
                .map(|(a, b)| (index[a.as_str()], index[b.as_str()]))
                .collect();
            let mut map = BTreeMap::new();
            for (c, members) in structure::analyse(children.len(), &pairs)
                .components
                .iter()
                .enumerate()
            {
                for &i in members {
                    map.insert(children[i].id.clone(), c);
                }
            }
            map
        } else {
            BTreeMap::new()
        };
        if options.component_packing {
            for ids in levels.values_mut() {
                ids.sort_by_key(|id| component_of.get(id).copied().unwrap_or(0));
            }
        }
        placement_structure::Plan {
            links,
            levels,
            order_links,
            chains,
            reversed_links,
            borders,
            sides,
            link_edges,
            virtual_sizes,
            components: component_of,
        }
    };
    let (structure, structure_reused) = if flattening.is_empty() {
        ordering.structure.plan(
            parent.unwrap_or("<root>"),
            structural_input.clone(),
            build_structure,
        )
    } else {
        (build_structure(), false)
    };
    counters.structural_plans += usize::from(!structure_reused);
    counters.reused_structural_plans += usize::from(structure_reused);
    let placement_structure::Plan {
        links,
        mut levels,
        mut order_links,
        mut chains,
        reversed_links,
        borders,
        sides,
        mut link_edges,
        virtual_sizes,
        components: component_of,
    } = structure;
    sizes.extend(virtual_sizes);
    // Current leaf sizes, not structural-cache values, determine port capacity.
    if options.layered_construction && layered && options.hierarchical_ordering {
        if let Mode::Orientation(o) = direction_for(graph, parent) {
            let spacing = options.min_port_spacing.max(options.track_spacing);
            let mut ports: BTreeMap<(&str, bool), usize> = BTreeMap::new();
            for ((a, b), edges) in &link_edges {
                *ports.entry((a.as_str(), true)).or_default() += edges.len();
                *ports.entry((b.as_str(), false)).or_default() += edges.len();
            }
            for child in &children {
                if !index.children_of(Some(child.id.as_str())).is_empty() {
                    continue;
                }
                let n = ports
                    .get(&(child.id.as_str(), true))
                    .copied()
                    .unwrap_or(0)
                    .max(ports.get(&(child.id.as_str(), false)).copied().unwrap_or(0));
                let need = snap((n + 1) as f64 * spacing, options.placement_grid);
                if let Some(size) = sizes.get_mut(&child.id) {
                    let across = if o.vertical() {
                        &mut size.0
                    } else {
                        &mut size.1
                    };
                    if *across < need {
                        *across = need;
                    }
                }
            }
        }
    }
    let port_offsets: std::cell::RefCell<BTreeMap<(String, bool), f64>> = Default::default();
    let border_orders: std::cell::RefCell<BTreeMap<usize, Vec<String>>> = Default::default();
    let lane_offsets: std::cell::RefCell<BTreeMap<(String, String), f64>> = Default::default();
    let mut region_capacity_seeds = Vec::new();
    if flattening.is_empty() {
        let reused = ordering.order(
            parent.unwrap_or("<root>"),
            &mut levels,
            &order_links,
            &component_of,
            optimize,
            options.layered_construction && layered,
            plans.get(parent.unwrap_or("<root>")),
            &options.order_hints,
        );
        counters.reused_orderings += usize::from(reused);
        counters.ordering_passes += usize::from(!reused);
        // Side nodes stand on their side of the area: the entering ones first across the flow, the leaving ones last. Within
        // each block they follow their own child's place in its rank, so that no two legs inside the area cross and the ports
        // keep the children's order along the border: `prepare_leg` staggers the first of a block deepest, and the leg of the
        // child first across the flow reaches the deepest port. A leaving leg then crosses another only where their children
        // and their far ends are in opposite orders, which no order of the ports avoids.
        if !sides.is_empty() {
            let place: BTreeMap<&String, usize> = levels
                .values()
                .flat_map(|ids| ids.iter().enumerate().map(|(i, id)| (id, i)))
                .collect();
            let child_place = |node: &String| -> usize {
                order_links
                    .iter()
                    .find_map(|(a, b)| {
                        if a == node {
                            place.get(b).copied()
                        } else if b == node {
                            place.get(a).copied()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(usize::MAX)
            };
            // The place across the parent's flow, in the parent's plan of the earlier pass, of the element an edge meets
            // outside this area: the next one on its chain there. Without that plan every edge has the same place.
            let area_name = parent.unwrap_or("<root>");
            let outer_area: &str = parent
                .and_then(|p| index.index_of(p))
                .and_then(|i| index.parents[i])
                .map_or("<root>", |i| index.nodes[i].id.as_str());
            let node_edge: BTreeMap<&String, &String> = sides.iter().map(|(e, n)| (n, e)).collect();
            let outer_plan = (area_name != "<root>")
                .then(|| plans.get(outer_area))
                .flatten();
            let outside = |node: &String| -> f64 {
                let (Some(plan), Some(edge)) = (outer_plan, node_edge.get(node)) else {
                    return 0.;
                };
                let Some(&e) = index.edge_by_id.get(edge.as_str()) else {
                    return 0.;
                };
                let ends = [
                    Some(graph.edges[e].source.as_str()),
                    graph.edges[e].target.as_deref(),
                ];
                // The end outside this area, as an element of the outer area (or the outer area's own node for the edge).
                let inside = |n: &str| {
                    let mut at = Some(n);
                    while let Some(x) = at {
                        if x == area_name {
                            return true;
                        }
                        at = index
                            .index_of(x)
                            .and_then(|i| index.parents[i])
                            .map(|i| index.nodes[i].id.as_str());
                    }
                    false
                };
                let Some(other) = ends.into_iter().flatten().find(|n| !inside(n)) else {
                    return 0.;
                };
                let mut element = other;
                loop {
                    let up = index
                        .index_of(element)
                        .and_then(|i| index.parents[i])
                        .map_or("<root>", |i| index.nodes[i].id.as_str());
                    if up == outer_area {
                        break;
                    }
                    if up == "<root>" {
                        element = "";
                        break;
                    }
                    element = up;
                }
                let element = if element.is_empty() {
                    match plan.borders.get(*edge).or_else(|| plan.sides.get(*edge)) {
                        Some(b) => b.as_str(),
                        None => return 0.,
                    }
                } else {
                    element
                };
                // The next element on the chain between this area and that element.
                let next = plan.chains.iter().find_map(|((a, b), dummies)| {
                    if a == area_name && b == element {
                        Some(dummies.first().map_or(b.as_str(), String::as_str))
                    } else if a == element && b == area_name {
                        Some(dummies.last().map_or(a.as_str(), String::as_str))
                    } else {
                        None
                    }
                });
                let Some(next) = next else { return 0. };
                plan.ranks
                    .iter()
                    .find_map(|rank| {
                        rank.iter()
                            .position(|x| x == next)
                            .map(|i| (i as f64 + 0.5) / rank.len() as f64)
                    })
                    .unwrap_or(0.)
            };
            let mut ordered = levels.clone();
            for ids in ordered.values_mut() {
                let kind = |id: &String| {
                    if id.starts_with(SIDE_IN_PREFIX) {
                        0
                    } else if id.starts_with(SIDE_OUT_PREFIX) {
                        2
                    } else {
                        1
                    }
                };
                let mut keyed: Vec<(usize, usize, f64, String)> = ids
                    .drain(..)
                    .enumerate()
                    .map(|(i, id)| {
                        let k = kind(&id);
                        let far = if k == 1 { 0. } else { outside(&id) };
                        (k, if k == 1 { i } else { child_place(&id) }, far, id)
                    })
                    .collect();
                // Side nodes of one child: by their edges' places outside, the farthest across first, since the first of a
                // block takes the deepest port (`prepare_leg`); then the legs of one child do not cross outside.
                keyed.sort_by(|a, b| {
                    a.0.cmp(&b.0)
                        .then(a.1.cmp(&b.1))
                        .then(b.2.total_cmp(&a.2))
                        .then(a.3.cmp(&b.3))
                });
                ids.extend(keyed.into_iter().map(|k| k.3));
            }
            levels = ordered;
            // ADR 0017, amendment A: a side node beside its leaf. When the leaf stands last across its rank (first, for an
            // entering edge), nothing lies between it and the side its parent's flow uses, so its side node moves into the
            // leaf's rank, after (before) it, without a link: the inner leg is one straight segment across the flow.
            // Only in the placement that makes the geometry: the hierarchical order is chosen on the side nodes after their
            // leaves, and keeps its places; moving them before it reordered the ranks against the drawing.
            let along_y =
                matches!(direction_for(graph, parent), Mode::Orientation(o) if o.vertical());
            if !ordering.order_only {
                lateral_sides(
                    &mut levels,
                    (&mut order_links, &mut chains, &mut link_edges),
                    &mut sizes,
                    &sides,
                    // A leaf, or a child area that leaves by a border node of its own for the edge (amendment A3).
                    |id, edge| {
                        index
                            .index_of(id)
                            .is_some_and(|i| index.children[i].is_empty())
                            || plans.get(id).is_some_and(|p| {
                                // Only a child of the other axis than this area (amendment A3); a child of this area's
                                // axis has border nodes as the ordinary case.
                                p.vertical != along_y && p.borders.contains_key(edge)
                            })
                    },
                    (snap(options.track_spacing, options.placement_grid), along_y),
                );
            }
        }
    } else {
        placement_order::compute(
            &mut levels,
            &order_links,
            &component_of,
            optimize,
            false,
            None,
            &BTreeMap::new(),
        );
        counters.ordering_passes += 1;
    }
    if ordering.order_only {
        if let (Mode::Orientation(o), true) = (
            direction_for(graph, parent),
            options.layered_construction && layered && flattening.is_empty(),
        ) {
            let area = parent.unwrap_or("<root>").to_string();
            let earlier = plans.get(&area);
            let (fixed, fixed_borders) =
                earlier.map_or((false, false), |p| (p.fixed, p.fixed_borders));
            plans.insert(
                area,
                layered_plan::AreaPlan {
                    vertical: o.vertical(),
                    reversed: o.reversed(),
                    ranks: levels.values().cloned().collect(),
                    chains,
                    reversed_links,
                    borders,
                    sides: sides.clone(),
                    fixed,
                    fixed_borders,
                    ..Default::default()
                },
            );
        }
        return (0., 0.);
    }
    let region_input = (full && options.layered_construction && layered && flattening.is_empty())
        .then(|| {
            placement_region::Input::new(
                parent.unwrap_or("<root>"),
                structural_input,
                &levels,
                &sizes,
                gaps,
                &children,
                plans,
                optimize,
            )
        });
    if let Some(input) = &region_input {
        if let Some(realized) = ordering.regions.get(parent.unwrap_or("<root>"), input) {
            counters.reused_region_realizations += 1;
            if options.abstract_routing {
                capacity_seeds.extend(realized.capacity_seeds.iter().cloned());
            }
            boxes.extend(realized.boxes);
            plans.insert(parent.unwrap_or("<root>").into(), realized.plan);
            if let Some(orientation) = realized.orientation {
                orientations.insert(parent.unwrap_or("<root>").into(), orientation);
            }
            return realized.size;
        }
    }
    counters.region_realizations += 1;
    // Experimental (plan section 4a, step 2): the order of the ranks, dummies included, improved by sweeps in both directions
    // with transpositions, counting the crossings and keeping the best order (`layered_plan::improve_order`).
    // Global layering (iteration 3): the blocks of every element of a rank, outermost first: the flattened containers around
    // a unit; for a dummy, those around both ends of its edge, and the edge's chain as a block of its own.
    let mut dummy_path: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if !flattening.is_empty() {
        for ((a, b), dummies) in &chains {
            let (pa, pb) = (flattening.path(a), flattening.path(b));
            let mut p: Vec<String> = pa
                .iter()
                .zip(&pb)
                .take_while(|(x, y)| x == y)
                .map(|(x, _)| x.clone())
                .collect();
            p.push(format!("{DUMMY_PREFIX}chain:{a}->{b}"));
            for d in dummies {
                dummy_path.insert(d.clone(), p.clone());
            }
        }
    }
    let element_path = |id: &str| -> Vec<String> {
        dummy_path
            .get(id)
            .cloned()
            .unwrap_or_else(|| flattening.path(id))
    };
    let mut hierarchy_keys = layered_plan::HierarchyKeys::default();
    if options.layered_construction && layered && !flattening.is_empty() {
        let mut ordered: Vec<Vec<String>> = levels.values().cloned().collect();
        let sections: Vec<(String, String)> = order_links.iter().cloned().collect();
        // The start (global layering): the order of the per-area layout, placed on copies: a container's place in its
        // parent's plan is its block's key, a unit's place in its container's plan its own key, and a chain takes the
        // middle of its two ends. The sweeps keep only what improves on it.
        let area_id = parent.unwrap_or("<root>").to_string();
        let per_area = {
            let off = LayoutOptions {
                global_layering: false,
                ..options.clone()
            };
            let (mut scratch_boxes, mut scratch_changed) = (BTreeMap::new(), Vec::new());
            let mut scratch_orientations = orientations.clone();
            let mut scratch_plans = layered_plan::LayeredPlans::new();
            if parent.is_none() {
                counters.next_placement_pass_reason = Some("global_layering_trial".into());
            }
            place(
                parent,
                graph,
                index,
                &mut scratch_boxes,
                true,
                dirty,
                &mut scratch_changed,
                &mut scratch_orientations,
                optimize,
                &off,
                minimum,
                gaps,
                counters,
                &mut scratch_plans,
                &mut placement_order::Memo::default(),
                &mut Vec::new(),
            );
            scratch_plans
        };
        let relative = |area: &str, id: &str| -> Option<f64> {
            per_area.get(area)?.ranks.iter().find_map(|rank| {
                rank.iter()
                    .position(|x| x == id)
                    .map(|i| (i as f64 + 0.5) / rank.len() as f64)
            })
        };
        let mut start = layered_plan::HierarchyKeys::default();
        for (container, around) in &flattening.parent {
            let area = around.clone().unwrap_or_else(|| area_id.clone());
            if let Some(k) = relative(&area, container) {
                start.blocks.insert(container.clone(), k);
            }
        }
        for unit in &children {
            let area = flattening
                .container_of
                .get(&unit.id)
                .cloned()
                .unwrap_or_else(|| area_id.clone());
            if let Some(k) = relative(&area, &unit.id) {
                start.elements.insert(unit.id.clone(), k);
            }
        }
        for (a, b) in chains.keys() {
            let Some(chain_path) = chains[&(a.clone(), b.clone())]
                .first()
                .and_then(|d| dummy_path.get(d))
            else {
                continue;
            };
            let level = chain_path.len() - 1;
            let key_of = |end: &String| {
                let p = flattening.path(end);
                match p.get(level) {
                    Some(block) => start.blocks.get(block).copied(),
                    None => start.elements.get(end).copied(),
                }
            };
            if let (Some(ka), Some(kb)) = (key_of(a), key_of(b)) {
                start
                    .blocks
                    .insert(chain_path[level].clone(), (ka + kb) / 2.);
            }
        }
        hierarchy_keys =
            layered_plan::hierarchical_order(&mut ordered, &sections, &element_path, Some(&start));
        for (ids, better) in levels.values_mut().zip(ordered) {
            *ids = better;
        }
    }
    let label_width = options
        .edge_labels
        .values()
        .chain(options.port_labels.values())
        .map(|l| l.width + 12.)
        .fold(0., f64::max);
    let label_height = options
        .edge_labels
        .values()
        .chain(options.port_labels.values())
        .map(|l| l.height + 12.)
        .fold(0., f64::max);
    let grid = options.placement_grid;
    // A required clearance needs room for a route to pass between two nodes: the clearance on both sides plus a track.
    // Every gap between siblings and between levels is at least that wide, so the layout has space in general.
    let clearance_gap = options
        .required_clearance
        .map_or(0., |c| 2. * c + options.track_spacing);
    let mut arrange = |vertical: bool| {
        // `nodeSpacing` and `layerSpacing` replace the base gaps when set (provisional; the ELK adapter maps ELK's spacings
        // to them); a negative or non-finite value counts as none.
        let base = |set: Option<f64>, default: f64| {
            set.filter(|v| v.is_finite() && *v >= 0.).unwrap_or(default)
        };
        let cross_gap = snap(
            (base(options.node_spacing, 24.) + if vertical { label_width } else { label_height })
                .max(clearance_gap),
            grid,
        );
        let primary_gap = snap(
            (base(options.layer_spacing, 64.)
                + 2. * if vertical { label_height } else { label_width })
            .max(clearance_gap),
            grid,
        );
        let cross_total = |ids: &Vec<String>| {
            ids.iter()
                .map(|id| {
                    if vertical {
                        sizes[id].0 + cross_gap
                    } else {
                        sizes[id].1 + cross_gap
                    }
                })
                .sum::<f64>()
                // The widened corridors count: levels are centred on their real extent (see `corridor_gaps`).
                + ids
                    .windows(2)
                    .filter_map(|pair| gaps.get(&(pair[0].clone(), pair[1].clone())))
                    .sum::<f64>()
        };
        let max_cross = levels.values().map(cross_total).fold(0., f64::max);
        let mut primary = snap(if vertical { 48. } else { 24. }, grid);
        let mut cross_extent: f64 = snap(if vertical { 48. } else { 90. }, grid);
        let mut placed = BTreeMap::new();
        let level_list: Vec<&Vec<String>> = levels.values().collect();
        for (level, ids) in level_list.iter().enumerate() {
            let band = ids
                .iter()
                .map(|id| if vertical { sizes[id].1 } else { sizes[id].0 })
                .fold(0., f64::max);
            let mut cross = snap(if vertical { 24. } else { 48. }, grid);
            if optimize >= 3 {
                cross = snap(cross + (max_cross - cross_total(ids)) / 2., grid);
            }
            for (i, id) in ids.iter().enumerate() {
                let (width, height) = sizes[id];
                let (x, y) = if vertical {
                    (cross, primary)
                } else {
                    (primary, cross)
                };
                placed.insert(
                    id.clone(),
                    Rect {
                        x,
                        y,
                        width,
                        height,
                    },
                );
                // The widening of a corridor to the next sibling (see `corridor_gaps`).
                let widened = snap(
                    ids.get(i + 1)
                        .and_then(|next| gaps.get(&(id.clone(), next.clone())))
                        .copied()
                        .unwrap_or(0.),
                    grid,
                );
                cross += if vertical {
                    width + cross_gap
                } else {
                    height + cross_gap
                } + widened;
            }
            cross_extent = cross_extent.max(cross);
            // Experimental (plan section 4a): the corridor to the next level is widened like a corridor inside a level, by the
            // largest width `corridor_gaps` asks for between a node of this level and a node of the next. Without the option
            // the levels keep their fixed gap, so default output is unchanged.
            let between = if options.layered_construction {
                level_list
                    .get(level + 1)
                    .map(|next| {
                        let next: BTreeSet<&str> = next.iter().map(String::as_str).collect();
                        ids.iter()
                            .flat_map(|a| {
                                gaps.range((a.clone(), String::new())..)
                                    .take_while(move |((x, _), _)| x == a)
                                    .filter(|((_, b), _)| next.contains(b.as_str()))
                                    .map(|(_, extra)| *extra)
                            })
                            .fold(0., f64::max)
                    })
                    .unwrap_or(0.)
            } else {
                0.
            };
            // A border rank (step 6) needs only the margin and a track before or after it; the channel widening adds what its
            // lane changes need.
            let border_level =
                |ids: &Vec<String>| ids.iter().all(|id| id.starts_with(BORDER_PREFIX));
            let gap = if options.layered_construction
                && (border_level(ids)
                    || level_list
                        .get(level + 1)
                        .is_some_and(|next| border_level(next)))
            {
                snap(
                    2. * routing_params(options).clearance + options.track_spacing,
                    grid,
                )
            } else {
                primary_gap
            };
            primary += band + snap(gap, grid) + snap(between, grid);
        }
        // Global layering (iteration 3): the blocks are packed across the flow, nested, each over the ranks it spans and a
        // flattened container with room for its header and padding, so that every container is one rectangle and the
        // dummies of a chain lie on one line; the ranks where containers begin or end move apart for their margins.
        let global = !flattening.is_empty();
        if global {
            let rules = arqlet_scene::GeometryRules::default();
            let containers: BTreeSet<String> = flattening.children.keys().cloned().collect();
            let reach = global::skyline(
                &level_list,
                &mut placed,
                &element_path,
                &hierarchy_keys,
                &containers,
                vertical,
                cross_gap,
                snap(if vertical { 24. } else { 48. }, grid),
                (rules.header, rules.padding),
            );
            cross_extent = cross_extent.max(reach + cross_gap);
            primary += global::main_margins(
                &level_list,
                &mut placed,
                &element_path,
                &containers,
                vertical,
                (rules.header, rules.padding),
            );
        }
        let reuse_cross = options.layered_construction && options.hierarchical_ordering && !global;
        if options.free_placement && grid <= 0. && !global && !reuse_cross {
            slide_free(
                &mut placed,
                &levels,
                &links,
                vertical,
                cross_gap,
                cross_extent,
            );
        }
        let mut physical_track_counts = Vec::new();
        // Experimental (plan section 4a, step 4): the sections of the plan, dummies included, slide towards the median of
        // their neighbours, so that the dummies of a long edge come to lie on one line. (Widening every channel to its lane
        // changes was measured and removed: it cost 20 % of the width of layerweave-code and no crossings,
        // `benchmarks/results/layered-space-20261101.md`.)
        if options.layered_construction {
            // Experimental (consistent layered pipeline): with hierarchical ordering, where every edge crosses every element
            // of its chain (the place of a leaf's port, of a container's border node, of its slot in a dummy), from the
            // element's start across the flow; the placement aligns these lanes and counts the tracks from them.
            let (explicit, lead_room) = if options.port_sides && options.single_pass {
                explicit_lanes(
                    graph,
                    index,
                    parent,
                    &levels,
                    &placed,
                    vertical,
                    (routing_params(options).clearance, options.track_spacing),
                )
            } else {
                Default::default()
            };
            let mut lanes: Option<BTreeMap<(String, String), f64>> =
                options.hierarchical_ordering.then(|| {
                    *port_offsets.borrow_mut() = plan_port_offsets(
                        graph,
                        index,
                        parent,
                        &chains,
                        (&borders, &sides),
                        &level_list,
                        &placed,
                        vertical,
                        plans,
                        options.min_port_spacing.max(options.track_spacing),
                        options.single_pass,
                    );
                    let mut lanes: BTreeMap<(String, String), f64> = children
                        .iter()
                        .filter_map(|c| Some((c.id.as_str(), plans.get(&c.id)?)))
                        .flat_map(|(c, plan)| {
                            plan.borders.iter().filter_map(move |(edge, b)| {
                                let d = plan.dummies.get(b)?;
                                let centre = if vertical {
                                    d.x + d.width / 2.
                                } else {
                                    d.y + d.height / 2.
                                };
                                Some(((c.to_string(), edge.clone()), centre))
                            })
                        })
                        .collect();
                    // Single pass (ADR 0017): an edge into a child of the other axis meets the child at a side node's port.
                    // Its exact place is left to the realization (taking it here aligned other lanes onto it, measured in
                    // `benchmarks/results/single-pass-bends-20261107.md`), but its order along the side is known: by the side
                    // node's rank, then the first of a rank farthest along (`prepare_leg`). The lanes spread the edges of each
                    // side over the child in that order, so that the chains start in the order their ports have.
                    if options.single_pass {
                        for child in &children {
                            let (Some(plan), Some(rect)) =
                                (plans.get(&child.id), placed.get(&child.id))
                            else {
                                continue;
                            };
                            let size = if vertical { rect.width } else { rect.height };
                            for kind in [SIDE_IN_PREFIX, SIDE_OUT_PREFIX] {
                                let mut keyed: Vec<(usize, usize, &String)> = Vec::new();
                                for (r, rank) in plan.ranks.iter().enumerate() {
                                    let peers: Vec<&String> =
                                        rank.iter().filter(|id| id.starts_with(kind)).collect();
                                    for (k, side) in peers.iter().enumerate() {
                                        if let Some((edge, _)) =
                                            plan.sides.iter().find(|(_, s)| s == side)
                                        {
                                            keyed.push((r, peers.len() - 1 - k, edge));
                                        }
                                    }
                                }
                                keyed.sort();
                                let m = keyed.len() as f64;
                                for (i, (_, _, edge)) in keyed.into_iter().enumerate() {
                                    lanes.insert(
                                        (child.id.clone(), edge.clone()),
                                        size * (i as f64 + 0.5) / m,
                                    );
                                }
                            }
                        }
                    }
                    // A leaf's ports lie where the plan put them.
                    // Sources first, so a self-loop keeps the offset of its target end.
                    let offsets = port_offsets.borrow();
                    for want_source in [true, false] {
                        for ((id, is_source), &offset) in offsets.iter() {
                            if *is_source != want_source {
                                continue;
                            }
                            let Some(&e) = index.edge_by_id.get(id.as_str()) else {
                                continue;
                            };
                            let edge = &graph.edges[e];
                            let node = if want_source {
                                Some(edge.source.as_str())
                            } else {
                                edge.target.as_deref()
                            };
                            if let Some(node) = node {
                                lanes.insert((node.to_string(), edge.id.clone()), offset);
                            }
                        }
                    }
                    // Port sides (option `portSides`): an explicit port lies where it is configured, and a port across the
                    // flow is met at its lead's lane, so the placement aligns the other end with that lane.
                    lanes.extend(explicit.iter().map(|(k, v)| (k.clone(), *v)));
                    lanes
                });
            // A side node beside a grandchild that leaves by a border node of its own (amendment A3) has its port on that border
            // node's lane (`layered_plan::lateral_child_lane`). The tracks of the channels are counted at that exact place; the
            // alignment keeps the side's order instead (aligning other lanes onto the exact places narrowed the channels on
            // synthetic-2k, `benchmarks/results/single-pass-lateral-20261108.md`).
            let mut exact_lanes: BTreeMap<(String, String), f64> = BTreeMap::new();
            if options.single_pass && lanes.is_some() {
                for child in &children {
                    let Some(plan) = plans.get(&child.id) else {
                        continue;
                    };
                    for (edge, side) in &plan.sides {
                        if plan.chains.keys().any(|(a, b)| a == side || b == side) {
                            continue;
                        }
                        let Some(rank) = plan.ranks.iter().find(|r| r.contains(side)) else {
                            continue;
                        };
                        let lane = rank.iter().find_map(|inner| {
                            let p = plans.get(inner)?;
                            let d = p.dummies.get(p.borders.get(edge)?)?;
                            let r = boxes.get(inner)?;
                            Some(if vertical {
                                r.x + d.x + d.width / 2.
                            } else {
                                r.y + d.y + d.height / 2.
                            })
                        });
                        if let Some(lane) = lane {
                            exact_lanes.insert((child.id.clone(), edge.clone()), lane);
                        }
                    }
                }
            }
            let counted = |lanes: &BTreeMap<(String, String), f64>| {
                let mut all = lanes.clone();
                all.extend(exact_lanes.iter().map(|(k, v)| (k.clone(), *v)));
                all
            };
            let planned_tracks = if reuse_cross {
                let lanes = lanes.as_ref().expect("hierarchical lane planning");
                let input = placement_geometry::Input {
                    levels: &levels,
                    links: &links,
                    chains: &chains,
                    edges: &link_edges,
                    lanes,
                    across: placement_geometry::project(&placed, vertical),
                    extent: cross_extent,
                    gap: cross_gap,
                    track: options.track_spacing,
                    free: options.free_placement && grid <= 0.,
                    room: lead_room.clone(),
                };
                let (cross, reused) = ordering.coordinates.borrow_mut().plan(
                    parent.unwrap_or("<root>"),
                    vertical,
                    input,
                    || {
                        if options.free_placement && grid <= 0. {
                            slide_free(
                                &mut placed,
                                &levels,
                                &links,
                                vertical,
                                cross_gap,
                                cross_extent,
                            );
                        }
                        let slots = chain_slots(
                            &mut placed,
                            &levels,
                            (&chains, &link_edges, lanes),
                            vertical,
                            (cross_gap, options.track_spacing),
                        );
                        let mut lanes = lanes.clone();
                        lanes.extend(slots.clone());
                        let (reach, reordered) = align_lanes(
                            &mut placed,
                            &levels,
                            (&chains, &link_edges, &lanes),
                            vertical,
                            (cross_gap, options.track_spacing),
                            options.single_pass,
                            &lead_room,
                        );
                        placement_geometry::CrossPlan {
                            tracks: channel_tracks_by_edges(
                                &placed,
                                &level_list,
                                &chains,
                                &link_edges,
                                &counted(&lanes),
                                vertical,
                                options.track_spacing,
                            ),
                            across: placement_geometry::project(&placed, vertical),
                            extent: cross_extent.max(reach + cross_gap),
                            border_orders: reordered,
                            slots,
                        }
                    },
                );
                counters.cross_plans += usize::from(!reused);
                counters.reused_cross_plans += usize::from(reused);
                cross.apply(&mut placed, vertical);
                cross_extent = cross.extent;
                *border_orders.borrow_mut() = cross.border_orders;
                *lane_offsets.borrow_mut() = cross.slots;
                Some(cross.tracks)
            } else {
                if let Some(lanes) = lanes.as_mut() {
                    let slots = chain_slots(
                        &mut placed,
                        &levels,
                        (&chains, &link_edges, lanes),
                        vertical,
                        (cross_gap, options.track_spacing),
                    );
                    lanes.extend(slots.clone());
                    *lane_offsets.borrow_mut() = slots;
                }
                None
            };
            if !global && !reuse_cross {
                match &lanes {
                    Some(lanes) => {
                        let (reach, reordered) = align_lanes(
                            &mut placed,
                            &levels,
                            (&chains, &link_edges, lanes),
                            vertical,
                            (cross_gap, options.track_spacing),
                            options.single_pass,
                            &lead_room,
                        );
                        *border_orders.borrow_mut() = reordered;
                        cross_extent = cross_extent.max(reach + cross_gap);
                    }
                    None => {
                        slide_free(
                            &mut placed,
                            &levels,
                            &order_links,
                            vertical,
                            cross_gap,
                            cross_extent,
                        );
                        // The dummies of every long edge on one line: a bend costs more than the room it saves (the
                        // objectives rank bends before length).
                        let reach = straighten_chains(&mut placed, &levels, &chains, vertical);
                        cross_extent = cross_extent.max(reach + cross_gap);
                    }
                }
            }
            // Space for the tracks (plan section 4a, step 4): every channel between two ranks is as wide as its lane changes
            // need, a track each (changes whose spans do not overlap share one, `constructive::track_slots`) and the
            // margin from both ranks; the ranks after it move along the flow. Without it, the tracks of a crowded channel
            // come closer than the track spacing, and the routes from the sections give way to the search.
            primary += match &lanes {
                // Experimental (consistent layered pipeline, step 2): the tracks are counted edge by edge, at the lanes
                // the routes from the sections take (`widen_channels_by_edges`).
                Some(lanes) => {
                    let tracks = planned_tracks.unwrap_or_else(|| {
                        channel_tracks_by_edges(
                            &placed,
                            &level_list,
                            &chains,
                            &link_edges,
                            &counted(lanes),
                            vertical,
                            options.track_spacing,
                        )
                    });
                    physical_track_counts.clone_from(&tracks);
                    realize_channel_widths(
                        &mut placed,
                        &level_list,
                        &tracks,
                        vertical,
                        options,
                        (parent.unwrap_or("<root>"), gaps),
                    )
                }
                None => widen_channels(
                    &mut placed,
                    &level_list,
                    &order_links,
                    vertical,
                    options,
                    grid,
                ),
            };
        }
        let mut size = if vertical {
            (snap(cross_extent, grid), snap(primary - 40., grid))
        } else {
            (snap(primary - 40., grid), snap(cross_extent, grid))
        };
        // A pinned leaf keeps its position in this container's frame. A free sibling that comes too close to a pin moves
        // to the nearest clear spot beside it; when no spot is clear, the overlap stays and is reported as a geometry issue.
        for child in &children {
            if let Some(pin) = options.pins.get(&child.id) {
                let (width, height) = sizes[&child.id];
                placed.insert(
                    child.id.clone(),
                    Rect {
                        x: pin.x,
                        y: pin.y,
                        width,
                        height,
                    },
                );
            }
        }
        // A pinned child that reaches past the content extent enlarges the container, with the geometry padding, so
        // that it stays inside its border. Without pins the extent is unchanged.
        let padding = arqlet_scene::GeometryRules::default().padding;
        for child in &children {
            if let Some(pin) = options.pins.get(&child.id) {
                let rect = &placed[&child.id];
                size.0 = size.0.max(snap(pin.x + rect.width + padding, grid));
                size.1 = size.1.max(snap(pin.y + rect.height + padding, grid));
            }
        }
        if !options.pins.is_empty() {
            let pins: Vec<Rect> = children
                .iter()
                .filter(|child| options.pins.contains_key(&child.id))
                .map(|child| placed[&child.id].clone())
                .collect();
            for child in &children {
                if options.pins.contains_key(&child.id) {
                    continue;
                }
                let current = placed[&child.id].clone();
                if !pins.iter().any(|pin| crowded(&current, pin)) {
                    continue;
                }
                let nearest = pins
                    .iter()
                    .flat_map(|pin| {
                        [
                            (pin.x + pin.width + PIN_CLEARANCE, current.y),
                            (current.x, pin.y + pin.height + PIN_CLEARANCE),
                            (pin.x - current.width - PIN_CLEARANCE, current.y),
                            (current.x, pin.y - current.height - PIN_CLEARANCE),
                        ]
                    })
                    .filter(|(x, y)| *x >= 0. && *y >= 0.)
                    .map(|(x, y)| Rect {
                        x: snap(x, grid),
                        y: snap(y, grid),
                        width: current.width,
                        height: current.height,
                    })
                    .filter(|spot| {
                        placed
                            .iter()
                            .all(|(id, other)| *id == child.id || !crowded(spot, other))
                    })
                    .min_by(|a, b| {
                        let distance = |r: &Rect| (r.x - current.x).abs() + (r.y - current.y).abs();
                        distance(a).total_cmp(&distance(b))
                    });
                if let Some(spot) = nearest {
                    placed.insert(child.id.clone(), spot);
                }
            }
            let right = placed.values().map(|r| r.x + r.width).fold(0., f64::max);
            let bottom = placed.values().map(|r| r.y + r.height).fold(0., f64::max);
            size.0 = size.0.max(snap(right + 24., grid));
            size.1 = size.1.max(snap(bottom + 24., grid));
        }
        if options.abstract_routing {
            record_capacity_seeds(
                parent.unwrap_or("<root>"),
                &level_list,
                &placed,
                vertical,
                &physical_track_counts,
                options.track_spacing,
                routing_params(options).clearance,
                &mut region_capacity_seeds,
            );
        }
        (placed, size)
    };
    // Sum of center distances over the links between this area's children.
    let end_to_end = |placed: &BTreeMap<String, Rect>| -> f64 {
        links
            .iter()
            .filter_map(|(a, b)| Some((placed.get(a)?, placed.get(b)?)))
            .map(|(a, b)| {
                let dx = a.x + a.width / 2. - b.x - b.width / 2.;
                let dy = a.y + a.height / 2. - b.y - b.height / 2.;
                dx.hypot(dy)
            })
            .sum()
    };
    // A reversed orientation is the mirror image of the forward one on its rank axis, within the area's own extent.
    let mirrored = |(mut placed, size): (BTreeMap<String, Rect>, (f64, f64)),
                    orientation: Orientation| {
        if orientation.reversed() && !placed.is_empty() {
            let lo_x = placed.values().map(|r| r.x).fold(f64::INFINITY, f64::min);
            let hi_x = placed
                .values()
                .map(|r| r.x + r.width)
                .fold(f64::NEG_INFINITY, f64::max);
            let lo_y = placed.values().map(|r| r.y).fold(f64::INFINITY, f64::min);
            let hi_y = placed
                .values()
                .map(|r| r.y + r.height)
                .fold(f64::NEG_INFINITY, f64::max);
            for r in placed.values_mut() {
                if orientation.vertical() {
                    r.y = lo_y + hi_y - r.y - r.height;
                } else {
                    r.x = lo_x + hi_x - r.x - r.width;
                }
            }
        }
        (placed, size)
    };
    let (placed, size) = match direction_for(graph, parent) {
        Mode::Orientation(orientation) => mirrored(arrange(orientation.vertical()), orientation),
        Mode::Chooser(Chooser::Compact) => {
            let lr = arrange(false);
            let tb = arrange(true);
            let prefer_tb = match options.hybrid_by {
                // Local compactness objective: area plus a penalty for very long diagrams.
                HybridCriterion::Compact => {
                    let cost = |(w, h): (f64, f64)| w * h + 0.25 * w.max(h).powi(2);
                    cost(tb.1) < cost(lr.1)
                }
                HybridCriterion::EndToEnd => end_to_end(&tb.0) < end_to_end(&lr.0),
            };
            // The orientation the free direction chose for this area is part of the result: the tree shows it.
            orientations.insert(
                parent.unwrap_or("<root>").into(),
                if prefer_tb {
                    Orientation::Tb
                } else {
                    Orientation::Lr
                },
            );
            if prefer_tb {
                tb
            } else {
                lr
            }
        }
        Mode::Chooser(Chooser::Shortest) => {
            let lr = arrange(false);
            let tb = arrange(true);
            let prefer_tb = end_to_end(&tb.0) < end_to_end(&lr.0);
            orientations.insert(
                parent.unwrap_or("<root>").into(),
                if prefer_tb {
                    Orientation::Tb
                } else {
                    Orientation::Lr
                },
            );
            if prefer_tb {
                tb
            } else {
                lr
            }
        }
        // The area was checked by `compute` to form a forest, so the placement can read it directly.
        Mode::Tree(orientation) => {
            let ids: Vec<String> = children.iter().map(|n| n.id.clone()).collect();
            let link_set: BTreeSet<(String, String)> = links.iter().cloned().collect();
            tree::tree_arrange(orientation, &ids, &link_set, &sizes, 64., 24., grid)
        }
        Mode::Organic => {
            let ids: Vec<String> = children.iter().map(|n| n.id.clone()).collect();
            let link_set: BTreeSet<(String, String)> = links.iter().cloned().collect();
            // A warm start: children that already have a box from an earlier layout begin where they are. The centre of
            // the earlier box is kept, so a child whose size changed stays centred where it was.
            let start: Option<BTreeMap<String, (f64, f64)>> = ids
                .iter()
                .map(|id| {
                    boxes
                        .get(id)
                        .map(|r| (id.clone(), (r.x + r.width / 2., r.y + r.height / 2.)))
                })
                .collect();
            let mut fixed = organic_fixed(options, &children, &sizes);
            // Force-directed: a continuation relaxes the stress from the positions for a few steps and holds only the pins.
            let relax = if options.force_directed {
                FORCE_STEPS
            } else {
                0
            };
            // Continuation holds the arrangement: when the layout goes on from an earlier one and only some children are
            // dirty, the others keep their centres through the stress and the separation, as pinned children do. Only
            // the geometric repair may still move them, and only to resolve a conflict.
            if let (false, false, Some(start)) = (full, options.force_directed, start.as_ref()) {
                let dirty_children = ids.iter().filter(|id| dirty.contains(*id)).count();
                if dirty_children > 0 && dirty_children < ids.len() {
                    for id in ids.iter().filter(|id| !dirty.contains(*id)) {
                        fixed.entry(id.clone()).or_insert(start[id]);
                    }
                }
            }
            // A force-directed drag relaxes only the region the change reaches (P2.7): the changed children and their graph
            // neighbours within `DRAG_HOPS`. The other children keep their centres, so the cost follows the region, not the area.
            if let (false, true, Some(start)) = (full, options.force_directed, start.as_ref()) {
                let region = drag_region(&ids, &link_set, &dirty);
                if region.len() < ids.len() {
                    for id in ids.iter().filter(|id| !region.contains(*id)) {
                        fixed.entry(id.clone()).or_insert(start[id]);
                    }
                }
            }
            organic::organic_arrange_counted(
                &ids,
                &link_set,
                &sizes,
                start.as_ref(),
                options
                    .organic_iterations
                    .unwrap_or(organic::DEFAULT_ITERATIONS),
                grid,
                &fixed,
                relax,
                options.vpsc_overlap_removal,
                options.node_edge_repulsion,
                options.sparse_stress,
                options.size_aware_stress,
                options.scalable_stress,
                counters,
            )
        }
        Mode::Radial => {
            let ids: Vec<String> = children.iter().map(|n| n.id.clone()).collect();
            let link_set: BTreeSet<(String, String)> = links.iter().cloned().collect();
            tree::radial_arrange(&ids, &link_set, &sizes, grid)
        }
    };
    // A pinned child keeps its position in this container's frame whatever the arrangement (ADR 0009), and the container
    // grows to hold it with the geometry padding. The layered arrangement does the same itself; this covers the others.
    // Experimental: the area's plan, kept for the steps after the placement (`layered_plan::AreaPlan`).
    if options.layered_construction && layered {
        let area = parent.unwrap_or("<root>").to_string();
        let orientation = match direction_for(graph, parent) {
            Mode::Orientation(o) => Some(o),
            Mode::Chooser(_) => orientations.get(&area).copied(),
            _ => None,
        };
        for (rank, ids) in border_orders.take() {
            levels.insert(rank, ids);
        }
        if let Some(o) = orientation {
            // A plan whose order was given keeps that mark, so that every later placement of the same pass (the widening of
            // its channels) follows the same order.
            let fixed = plans.get(&area).is_some_and(|plan| plan.fixed);
            let fixed_borders = plans.get(&area).is_some_and(|plan| plan.fixed_borders);
            plans.insert(
                area,
                layered_plan::AreaPlan {
                    vertical: o.vertical(),
                    reversed: o.reversed(),
                    ranks: levels.values().cloned().collect(),
                    chains,
                    reversed_links,
                    borders,
                    sides: sides.clone(),
                    fixed,
                    fixed_borders,
                    port_offsets: port_offsets.take(),
                    lane_offsets: lane_offsets.take(),
                    dummies: placed
                        .iter()
                        .filter(|(id, _)| id.starts_with(DUMMY_PREFIX))
                        .map(|(id, r)| (id.clone(), r.clone()))
                        .collect(),
                },
            );
        }
    }
    let (mut placed, mut size) = (placed, size);
    // The long-edge dummies held their place in the arrangement; they are not nodes of the drawing.
    placed.retain(|id, _| !id.starts_with(DUMMY_PREFIX));
    let padding = arqlet_scene::GeometryRules::default().padding;
    for child in &children {
        if let Some(pin) = options.pins.get(&child.id) {
            let (width, height) = sizes[&child.id];
            placed.insert(
                child.id.clone(),
                Rect {
                    x: pin.x,
                    y: pin.y,
                    width,
                    height,
                },
            );
            size.0 = size.0.max(snap(pin.x + width + padding, grid));
            size.1 = size.1.max(snap(pin.y + height + padding, grid));
        }
    }
    // A circle container is a disc: its children lie inside the disc, below the header, so the box is a square around
    // them. Children are centred in the disc unless one of them is pinned (a pin is a position in this frame).
    let circle = parent
        .and_then(|p| index.node(p))
        .is_some_and(|n| n.shape == arqlet_model::NodeShape::Circle);
    let mut size = size;
    if circle {
        let shapes: BTreeMap<&str, arqlet_model::NodeShape> =
            children.iter().map(|n| (n.id.as_str(), n.shape)).collect();
        let pinned = children.iter().any(|c| options.pins.contains_key(&c.id));
        // An organic area that continues an earlier layout keeps its children where they are drawn: the disc scales about
        // its old centre. Otherwise (a first layout, an arrangement made again, a pinned child) the block of children is
        // centred in the disc, or stays in the frame when a child is pinned.
        let earlier = parent
            .filter(|_| {
                !full && !pinned && graph.area_mode(parent) == Ok(arqlet_model::Mode::Organic)
            })
            .and_then(|p| boxes.get(p))
            .map(|r| r.width.max(r.height))
            .filter(|_| children.iter().all(|c| boxes.contains_key(&c.id)));
        size = match earlier {
            Some(old) => fit_disc_about_centre(&mut placed, &boxes, &shapes, old, grid, size),
            None => fit_disc(&mut placed, &shapes, !pinned, grid, size),
        };
    }
    // An authored size (ADR 0010) sets the extent of a container: a minimum grows it, a fixed size keeps it. A fixed
    // axis below its content is left as set, and the geometry validator reports the children that do not fit. The
    // parent level reads this extent, so ancestors follow.
    let size = match parent {
        Some(p) => {
            let content = arqlet_model::NodeSize {
                width: size.0,
                height: size.1,
            };
            let (sized, _) =
                arqlet_model::container_extent(options.container_sizing.get(p), content);
            if circle {
                // A circle takes the larger side on both axes.
                let side = sized.width.max(sized.height);
                (side, side)
            } else {
                (sized.width, sized.height)
            }
        }
        None => size,
    };
    // A circle container with a pinned child keeps the corner of its box where it was when it continues an earlier layout: a pin
    // is a position in the container's frame, and the area that holds the container would keep its centre when its diameter
    // changes, which moves the frame and every child with it.
    if !full && graph.area_mode(parent) == Ok(arqlet_model::Mode::Organic) {
        for child in children.iter().filter(|c| {
            c.shape == arqlet_model::NodeShape::Circle
                && !options.pins.contains_key(&c.id)
                && graph.nodes.iter().any(|n| {
                    n.parent_id.as_deref() == Some(&c.id) && options.pins.contains_key(&n.id)
                })
        }) {
            if let (Some(old), Some(r)) = (boxes.get(&child.id), placed.get_mut(&child.id)) {
                r.x = old.x;
                r.y = old.y;
            }
        }
    }
    // Global layering (iteration 3): every flattened container is the rectangle around its children, with its header above
    // and its padding around them (the arrangement left that room); everything inside is given in its container's frame.
    if !flattening.is_empty() {
        let rules = arqlet_scene::GeometryRules::default();
        global::frames(&flattening, &mut placed, (rules.header, rules.padding));
        changed.extend(flattening.children.keys().cloned());
    }
    if options.abstract_routing {
        capacity_seeds.extend(region_capacity_seeds.iter().cloned());
    }
    if let Some(input) = region_input {
        let area = parent.unwrap_or("<root>");
        ordering.regions.insert(
            area,
            input,
            placement_region::Realization {
                boxes: placed.clone(),
                size,
                plan: plans.get(area).expect("realized hierarchical area").clone(),
                capacity_seeds: region_capacity_seeds,
                orientation: match direction_for(graph, parent) {
                    Mode::Chooser(_) => orientations.get(area).copied(),
                    _ => None,
                },
            },
        );
    }
    boxes.extend(placed);
    size
}

/// The disc of a circle container that scales about its old centre. The children keep the places they have in the old frame
/// (the arrangement may have moved them all by the same amount, which is taken out: the median shift of the children against
/// their earlier boxes), and the diameter is the smallest at which they lie in the disc around the old centre, below the
/// header. Growing or shrinking the box about its centre moves each child by half the change in the frame, so nothing moves
/// on the drawing. `old` is the diameter of the earlier box.
fn fit_disc_about_centre(
    placed: &mut BTreeMap<String, Rect>,
    earlier: &BTreeMap<String, Rect>,
    shapes: &BTreeMap<&str, arqlet_model::NodeShape>,
    old: f64,
    grid: f64,
    fallback: (f64, f64),
) -> (f64, f64) {
    use arqlet_model::outline::Outline;
    let rules = arqlet_scene::GeometryRules::default();
    let median = |mut v: Vec<f64>| {
        v.sort_by(f64::total_cmp);
        v.get(v.len() / 2).copied().unwrap_or(0.)
    };
    let (dx, dy) = (
        median(
            placed
                .iter()
                .filter_map(|(id, r)| earlier.get(id).map(|e| r.x - e.x))
                .collect(),
        ),
        median(
            placed
                .iter()
                .filter_map(|(id, r)| earlier.get(id).map(|e| r.y - e.y))
                .collect(),
        ),
    );
    let in_old_frame: BTreeMap<String, Rect> = placed
        .iter()
        .map(|(id, r)| {
            (
                id.clone(),
                Rect {
                    x: r.x - dx,
                    y: r.y - dy,
                    width: r.width,
                    height: r.height,
                },
            )
        })
        .collect();
    let step = if grid > 0. { grid } else { 1. };
    let fits = |d: f64| {
        let grow = (d - old) / 2.;
        in_old_frame.iter().all(|(id, r)| {
            let shape = shapes.get(id.as_str()).copied().unwrap_or_default();
            r.y + grow >= rules.header - 1e-9
                && Outline::of(shape, r).overshoot_beyond_disc(
                    old / 2.,
                    old / 2.,
                    d / 2. - rules.padding,
                ) <= 1e-9
        })
    };
    let mut d = snap(2. * rules.padding, grid).max(step);
    let limit = old * 40.;
    while d <= limit {
        if fits(d) {
            let grow = (d - old) / 2.;
            for (id, r) in placed.iter_mut() {
                let base = &in_old_frame[id];
                r.x = base.x + grow;
                r.y = base.y + grow;
            }
            return (d, d);
        }
        d += step;
    }
    fallback
}

/// The square box of a circle container and the placement of its children in the disc. The disc is inscribed in the box;
/// its radius less the geometry padding bounds the children, and their tops lie below the header. With `centre` the block of
/// children is moved to the middle of the disc, as low as the header needs; without it (a child is pinned) the children stay
/// where they are and the diameter grows until they fit, or to the content `fallback` when none does. The diameter steps by
/// the placement grid (one unit without one).
fn fit_disc(
    placed: &mut BTreeMap<String, Rect>,
    shapes: &BTreeMap<&str, arqlet_model::NodeShape>,
    centre: bool,
    grid: f64,
    fallback: (f64, f64),
) -> (f64, f64) {
    use arqlet_model::outline::Outline;
    let rules = arqlet_scene::GeometryRules::default();
    let (x0, y0, x1, y1) = placed.values().fold(
        (f64::MAX, f64::MAX, f64::MIN, f64::MIN),
        |(a, b, c, d), r| {
            (
                a.min(r.x),
                b.min(r.y),
                c.max(r.x + r.width),
                d.max(r.y + r.height),
            )
        },
    );
    if placed.is_empty() {
        return fallback;
    }
    let (block_w, block_h) = (x1 - x0, y1 - y0);
    let step = if grid > 0. { grid } else { 1. };
    let shift_for = |d: f64| {
        if centre {
            (
                d / 2. - block_w / 2. - x0,
                rules.header.max(d / 2. - block_h / 2.) - y0,
            )
        } else {
            (0., 0.)
        }
    };
    let fits = |d: f64| {
        let (dx, dy) = shift_for(d);
        placed.iter().all(|(id, r)| {
            let moved = Rect {
                x: r.x + dx,
                y: r.y + dy,
                width: r.width,
                height: r.height,
            };
            let shape = shapes.get(id.as_str()).copied().unwrap_or_default();
            moved.y >= rules.header - 1e-9
                && Outline::of(shape, &moved).overshoot_beyond_disc(
                    d / 2.,
                    d / 2.,
                    d / 2. - rules.padding,
                ) <= 1e-9
        })
    };
    let mut d = snap(block_w.max(block_h).max(2. * rules.padding), grid).max(step);
    let limit = 40. * block_w.max(block_h).max(100.);
    while d <= limit {
        if fits(d) {
            let (dx, dy) = shift_for(d);
            for r in placed.values_mut() {
                r.x += dx;
                r.y += dy;
            }
            return (d, d);
        }
        d += step;
    }
    // No diameter holds the children where they are pinned: the box follows the content, and the validator reports them.
    let side = fallback.0.max(fallback.1);
    (side, side)
}

pub fn absolute(graph: &Graph, boxes: &BTreeMap<String, Rect>, id: &str) -> Rect {
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    absolute_with(&parents, boxes, id)
}

/// Absolute rectangles for all nodes in one pass. Accumulation order matches `absolute`,
/// so the values are bit-identical; hot loops should use this instead of calling `absolute`.
pub fn absolute_all(graph: &Graph, boxes: &BTreeMap<String, Rect>) -> BTreeMap<String, Rect> {
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), absolute_with(&parents, boxes, &n.id)))
        .collect()
}

fn absolute_with(
    parents: &BTreeMap<&str, Option<&str>>,
    boxes: &BTreeMap<String, Rect>,
    id: &str,
) -> Rect {
    let mut r = boxes[id].clone();
    let mut parent = parents[id];
    while let Some(p) = parent {
        r.x += boxes[p].x;
        r.y += boxes[p].y;
        parent = parents[p];
    }
    r
}
fn route(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    options: &LayoutOptions,
    counters: &mut SearchCounters,
    port_context: &ports::PortContext<'_>,
) -> Vec<Route> {
    let port_started = stage_timer_start();
    let ports = port_context.resolve_with(
        boxes,
        options.port_placement,
        options.min_port_spacing,
        &options.node_rows,
        options.row_port_order,
    );
    counters.port_resolution_nanos = counters
        .port_resolution_nanos
        .saturating_add(elapsed_nanos(port_started));
    route_from_ports(graph, &ports, options.self_loop_clearance)
}

/// The initial routes of a drawing and the ports they start from, for a composition whose boxes are fixed: the routes are
/// the regular seed the router improves, so every border crossing is an ordinary route with ports at both ends.
pub fn seed_drawing(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    options: &LayoutOptions,
) -> (Vec<Route>, Vec<arqlet_model::ResolvedPort>) {
    let ports = ports::PortContext::new(graph)
        .with_flow_sides(options.layered_construction)
        .resolve_with(
            boxes,
            options.port_placement,
            options.min_port_spacing,
            &options.node_rows,
            options.row_port_order,
        );
    let routes = route_from_ports(graph, &ports, options.self_loop_clearance);
    (routes, ports)
}

fn route_from_ports(
    graph: &Graph,
    ports: &[arqlet_model::ResolvedPort],
    self_loop_clearance: f64,
) -> Vec<Route> {
    let ends = ports::PortEnds::new(ports);
    graph
        .edges
        .iter()
        .filter_map(|e| {
            e.target.as_ref()?;
            let source = ends.get(&e.id, "source")?;
            let target = ends.get(&e.id, "target")?;
            let start = source.position.clone();
            let end = target.position.clone();
            // An edge that returns to its own node gets the loop clearance (zero keeps the standard loop).
            let loop_room = if source.node_id == target.node_id {
                self_loop_clearance
            } else {
                0.
            };
            let depart = ports::outward(source, 20. + loop_room);
            let arrive = ports::outward(target, 20. + loop_room);
            let points = if (source.side == PortSide::Right
                && target.side == PortSide::Left
                && arrive.x > depart.x)
                || (source.side == PortSide::Left
                    && target.side == PortSide::Right
                    && arrive.x < depart.x)
            {
                let x = (depart.x + arrive.x) / 2.;
                vec![
                    start,
                    depart.clone(),
                    Point { x, y: depart.y },
                    Point { x, y: arrive.y },
                    arrive,
                    end,
                ]
            } else if (source.side == PortSide::Bottom
                && target.side == PortSide::Top
                && arrive.y > depart.y)
                || (source.side == PortSide::Top
                    && target.side == PortSide::Bottom
                    && arrive.y < depart.y)
            {
                let y = (depart.y + arrive.y) / 2.;
                vec![
                    start,
                    depart.clone(),
                    Point { x: depart.x, y },
                    Point { x: arrive.x, y },
                    arrive,
                    end,
                ]
            } else {
                let y = depart.y.min(arrive.y) - 24. - loop_room;
                vec![
                    start,
                    depart.clone(),
                    Point { x: depart.x, y },
                    Point { x: arrive.x, y },
                    arrive,
                    end,
                ]
            };
            Some(Route::new(e.id.clone(), points))
        })
        .collect()
}

/// Deterministic full-view ordering strategies; graph semantics and rank constraints remain unchanged.
/// Provisional: the strategy list will change with the ordering work.
pub const CANDIDATES: [(usize, &str); 4] = [
    (1, "forward-barycenter"),
    (2, "alternating-barycenter"),
    (3, "forward-median-centered"),
    (4, "alternating-median-centered"),
];

/// Computes one candidate. Each call is independent, so hosts may run them concurrently.
pub fn candidate(graph: &Graph, mode: usize, options: &LayoutOptions) -> Result<Snapshot, String> {
    uninterrupted(compute_mode(
        graph,
        None,
        &BTreeSet::new(),
        mode,
        options,
        &mut Run::new(&|| false),
    ))
}

/// `candidate` with checkpoints. `Ok((None, _))` when `stop` fires before the construction completes; no partial
/// snapshot is returned. The second value is the channel router's search work, interrupted or not.
pub fn candidate_until(
    graph: &Graph,
    mode: usize,
    options: &LayoutOptions,
    stop: &dyn Fn() -> bool,
) -> Result<(Option<Snapshot>, arqlet_routing::channel::SearchCounters), String> {
    candidate_from(graph, mode, options, stop, &BTreeMap::new())
}

/// As `candidate_until`, with the stored plans of the drawing the candidate refines (`Snapshot::layered_plans`): with the
/// experimental option `layeredConstruction`, each layered area starts its order from its earlier plan.
pub fn candidate_from(
    graph: &Graph,
    mode: usize,
    options: &LayoutOptions,
    stop: &dyn Fn() -> bool,
    earlier: &BTreeMap<String, arqlet_model::StoredAreaPlan>,
) -> Result<(Option<Snapshot>, arqlet_routing::channel::SearchCounters), String> {
    let mut run = Run::new(stop);
    run.earlier = earlier.clone();
    let outcome = compute_mode(graph, None, &BTreeSet::new(), mode, options, &mut run);
    let snapshot = match outcome {
        Ok(snapshot) => Some(snapshot),
        Err(Abort::Stopped) => None,
        Err(Abort::Failed(message)) => return Err(message),
    };
    Ok((snapshot, run.search))
}

/// Measure sibling overlaps, foreign-node hits, routed pairs and projected crossings.
pub fn quality(snapshot: &Snapshot, reference: &Snapshot) -> Quality {
    quality_with(snapshot, reference, &DEFAULT_OBJECTIVES)
}

/// As `quality`. The end-to-end sum is computed only when an objective needs it, so default
/// reports stay unchanged.
/// Quality of a snapshot against a reference: the thin form of `quality_of`.
pub fn quality_with(
    snapshot: &Snapshot,
    reference: &Snapshot,
    objectives: &[Objective],
) -> Quality {
    quality_of(
        &snapshot.graph,
        &snapshot.boxes,
        &snapshot.routes,
        (&reference.graph, &reference.boxes),
        objectives,
    )
}

/// Counts the proper crossings of the lines between node pairs. A sweep over boxes sorted by their left edge tests
/// only pairs whose boxes overlap. This is exact: a proper crossing needs overlapping boxes, and the boxes are padded
/// so that rounding in the orientation tests cannot drop a crossing. `count_all_pairs` is the reference.
fn projected_crossing_count<F: Fn(&str) -> (f64, f64)>(
    pairs: &[(String, String)],
    center: F,
) -> usize {
    const PAD: f64 = 1e-6;
    struct Line<'a> {
        a: &'a str,
        b: &'a str,
        p: (f64, f64),
        q: (f64, f64),
        min_x: f64,
        max_x: f64,
        min_y: f64,
        max_y: f64,
    }
    let mut lines: Vec<Line> = pairs
        .iter()
        .map(|(a, b)| {
            let (p, q) = (center(a), center(b));
            Line {
                a: a.as_str(),
                b: b.as_str(),
                p,
                q,
                min_x: p.0.min(q.0) - PAD,
                max_x: p.0.max(q.0) + PAD,
                min_y: p.1.min(q.1) - PAD,
                max_y: p.1.max(q.1) + PAD,
            }
        })
        .collect();
    lines.sort_by(|x, y| x.min_x.total_cmp(&y.min_x));
    let mut count = 0;
    for (i, l) in lines.iter().enumerate() {
        for r in &lines[i + 1..] {
            if r.min_x > l.max_x {
                break;
            }
            if r.min_y > l.max_y || l.min_y > r.max_y {
                continue;
            }
            if l.a == r.a || l.a == r.b || l.b == r.a || l.b == r.b {
                continue;
            }
            if proper_crossing(l.p, l.q, r.p, r.q) {
                count += 1;
            }
        }
    }
    count
}

/// The reference for `projected_crossing_count`: every pair of lines, checked directly. Used by tests only.
#[cfg(test)]
fn count_all_pairs<F: Fn(&str) -> (f64, f64)>(pairs: &[(String, String)], center: F) -> usize {
    let mut count = 0;
    for (i, (a, b)) in pairs.iter().enumerate() {
        for (c, d) in &pairs[i + 1..] {
            if a == c || a == d || b == c || b == d {
                continue;
            }
            if proper_crossing(center(a), center(b), center(c), center(d)) {
                count += 1;
            }
        }
    }
    count
}

/// The shared quantization of the end-to-end length: hundredths. The published key and the quality comparison both
/// use it, so they decide ties the same way.
fn end_to_end_units(length: f64) -> i64 {
    (length * 100.).round() as i64
}

/// Projected relationships of a drawing: crossings of the straight lines between node centres, and the sum of
/// endpoint distances. Both use the same absolute rectangles as the routed measures, so the published key and the
/// quality comparison agree.
fn projected_measures(
    graph: &Graph,
    rects: &BTreeMap<&str, Rect>,
    with_end_to_end: bool,
) -> (usize, f64) {
    let center = |id: &str| {
        let r = &rects[id];
        (r.x + r.width / 2., r.y + r.height / 2.)
    };
    // Parallel edges count as one projected relationship; shared endpoints are not crossings.
    let pairs: BTreeSet<_> = graph
        .edges
        .iter()
        .filter_map(|e| {
            e.target.as_ref().map(|t| {
                if e.source < *t {
                    (e.source.clone(), t.clone())
                } else {
                    (t.clone(), e.source.clone())
                }
            })
        })
        .filter(|(a, b)| a != b)
        .collect();
    let pairs: Vec<_> = pairs.into_iter().collect();
    let crossings = projected_crossing_count(&pairs, center);
    // Only computed when requested: the value is reported in the metadata, and default metadata stays unchanged.
    let end_to_end: f64 = if with_end_to_end {
        graph
            .edges
            .iter()
            .filter(|e| e.target.is_some() && e.target.as_deref() != Some(e.source.as_str()))
            .filter_map(|e| {
                let a = rects.get(e.source.as_str())?;
                let b = rects.get(e.target.as_deref()?)?;
                let dx = a.x + a.width / 2. - b.x - b.width / 2.;
                let dy = a.y + a.height / 2. - b.y - b.height / 2.;
                Some(dx.hypot(dy))
            })
            .sum()
    } else {
        0.
    };
    (crossings, end_to_end)
}

/// Whether a key needs the projected measures at all.
fn needs_projected(objectives: &[Objective]) -> bool {
    objectives
        .iter()
        .any(|o| matches!(o, Objective::ProjectedCrossings | Objective::EndToEnd))
}

/// The projected measures of a graph and its boxes, for the published key.
fn projected_of(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    with_end_to_end: bool,
) -> (usize, f64) {
    let owned = absolute_all(graph, boxes);
    let rects: BTreeMap<&str, Rect> = owned
        .iter()
        .map(|(id, r)| (id.as_str(), r.clone()))
        .collect();
    projected_measures(graph, &rects, with_end_to_end)
}

/// Quality from a graph, boxes and routes alone, so a candidate can be scored without a snapshot copy.
/// `reference` supplies the graph and boxes that movement is measured against.
pub fn quality_of(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    routes: &[Route],
    reference: (&Graph, &BTreeMap<String, Rect>),
    objectives: &[Objective],
) -> Quality {
    let graph = &graph;
    // Bundled members are not drawn; only drawn routes count toward length, crossings and sharing.
    let drawn: Vec<&Route> = routes.iter().filter(|r| r.bundle_of.is_none()).collect();
    let owned = absolute_all(graph, boxes);
    let rects: BTreeMap<&str, Rect> = owned
        .iter()
        .map(|(id, r)| (id.as_str(), r.clone()))
        .collect();
    let mut overlaps = 0;
    for (i, a) in graph.nodes.iter().enumerate() {
        for b in &graph.nodes[i + 1..] {
            if a.parent_id != b.parent_id {
                continue;
            }
            let x = &rects[a.id.as_str()];
            let y = &rects[b.id.as_str()];
            if x.x < y.x + y.width
                && y.x < x.x + x.width
                && x.y < y.y + y.height
                && y.y < x.y + x.height
            {
                overlaps += 1;
            }
        }
    }
    let length = drawn
        .iter()
        .flat_map(|r| r.points.windows(2))
        .map(|w| (w[0].x - w[1].x).abs() + (w[0].y - w[1].y).abs())
        .sum();
    let mut movement = 0.;
    let reference_rects = absolute_all(reference.0, reference.1);
    for n in &graph.nodes {
        if reference.1.contains_key(&n.id) {
            let old = &reference_rects[&n.id];
            let now = &rects[n.id.as_str()];
            movement += (old.x - now.x).abs() + (old.y - now.y).abs();
        }
    }
    let width = rects.values().map(|r| r.x + r.width).fold(0., f64::max);
    let height = rects.values().map(|r| r.y + r.height).fold(0., f64::max);
    let edge_map: BTreeMap<_, _> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let exclusions = arqlet_routing::Exclusions::new(graph);
    let prepared = arqlet_routing::PreparedRoutes::new(&drawn);
    let mut edge_node_hits = 0;
    let mut routed_crossings = 0;
    let mut shared_route_pairs = 0;
    for (i, r) in drawn.iter().enumerate() {
        let Some(edge) = edge_map.get(r.id.as_str()) else {
            continue;
        };
        let skip = exclusions.of(edge);
        edge_node_hits += rects
            .iter()
            .filter(|(id, rect)| {
                (!skip.contains(**id)
                    || **id == edge.source
                    || edge.target.as_deref() == Some(**id))
                    && prepared.hits(i, rect)
            })
            .count();
        for j in i + 1..drawn.len() {
            if prepared.crosses(i, j) {
                routed_crossings += 1;
            }
            if prepared.shares_segment(i, j) {
                shared_route_pairs += 1;
            }
        }
    }
    // Segments within the node margin of a foreign leaf, without entering it: routes that hug borders.
    let near_node_segments = if objectives.contains(&Objective::NearNodeSegments) {
        let containers: BTreeSet<&str> = graph
            .nodes
            .iter()
            .filter_map(|n| n.parent_id.as_deref())
            .collect();
        let mut count = 0;
        for (i, route) in drawn.iter().enumerate() {
            let Some(edge) = edge_map.get(route.id.as_str()) else {
                continue;
            };
            let skip = exclusions.of(edge);
            for (id, rect) in &rects {
                if containers.contains(id)
                    || skip.contains(*id)
                    || *id == edge.source
                    || edge.target.as_deref() == Some(*id)
                {
                    continue;
                }
                if prepared.hits(i, rect) {
                    continue;
                }
                let near = route
                    .points
                    .windows(2)
                    .any(|w| segment_rect_distance(&w[0], &w[1], rect) < NODE_MARGIN);
                if near {
                    count += 1;
                }
            }
        }
        count
    } else {
        0
    };
    let (crossings, end_to_end) =
        projected_measures(graph, &rects, objectives.contains(&Objective::EndToEnd));
    // Route speed: each direction change slows the eye, so fewer bends rank above shorter lines.
    let bends = if objectives.contains(&Objective::Bends) {
        drawn.iter().map(|r| bend_count(&r.points)).sum()
    } else {
        0
    };
    Quality {
        bends,
        edge_node_hits,
        routed_crossings,
        shared_route_pairs,
        sibling_overlaps: overlaps,
        projected_crossings: crossings,
        route_length: length,
        area: width * height,
        movement,
        end_to_end,
        near_node_segments,
    }
}
/// Number of interior points where a route changes direction. Repeated points are ignored.
fn bend_count(points: &[Point]) -> usize {
    let mut distinct: Vec<&Point> = Vec::with_capacity(points.len());
    for p in points {
        if distinct
            .last()
            .is_none_or(|q| (q.x - p.x).abs() > 1e-6 || (q.y - p.y).abs() > 1e-6)
        {
            distinct.push(p);
        }
    }
    distinct
        .windows(3)
        .filter(|w| {
            let (ax, ay) = (w[1].x - w[0].x, w[1].y - w[0].y);
            let (bx, by) = (w[2].x - w[1].x, w[2].y - w[1].y);
            (ax * by - ay * bx).abs() > 1e-6
        })
        .count()
}

fn proper_crossing(a: (f64, f64), b: (f64, f64), c: (f64, f64), d: (f64, f64)) -> bool {
    let orient = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| {
        (q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)
    };
    let opposite = |x: f64, y: f64| (x > 1e-6 && y < -1e-6) || (x < -1e-6 && y > 1e-6);
    opposite(orient(a, b, c), orient(a, b, d)) && opposite(orient(c, d, a), orient(c, d, b))
}
/// Overlaps and route conflicts take precedence; then balance length, movement and area.
pub fn better(a: &Quality, b: &Quality) -> bool {
    better_by(a, b, &DEFAULT_OBJECTIVES)
}

/// Lexicographic comparison in the configured objective order. Each discrete objective decides when
/// the values differ; `Cost` decides with a tolerance and ends the comparison.
pub fn better_by(a: &Quality, b: &Quality, objectives: &[Objective]) -> bool {
    for objective in objectives {
        let (x, y) = match objective {
            Objective::SiblingOverlaps => (a.sibling_overlaps as f64, b.sibling_overlaps as f64),
            Objective::EdgeNodeHits => (a.edge_node_hits as f64, b.edge_node_hits as f64),
            Objective::SharedRoutePairs => {
                (a.shared_route_pairs as f64, b.shared_route_pairs as f64)
            }
            Objective::RoutedCrossings => (a.routed_crossings as f64, b.routed_crossings as f64),
            Objective::ProjectedCrossings => {
                (a.projected_crossings as f64, b.projected_crossings as f64)
            }
            Objective::EndToEnd => (
                end_to_end_units(a.end_to_end) as f64,
                end_to_end_units(b.end_to_end) as f64,
            ),
            Objective::NearNodeSegments => {
                (a.near_node_segments as f64, b.near_node_segments as f64)
            }
            Objective::Bends => (a.bends as f64, b.bends as f64),
            Objective::Cost => {
                let cost = |q: &Quality| q.route_length + 0.15 * q.movement + 0.001 * q.area;
                return cost(a) < cost(b) - 0.001;
            }
        };
        if x != y {
            return x < y;
        }
    }
    false
}

/// Bundle members are drawn along their leader's route, so their endpoints must sit where the leader's
/// endpoints are. Otherwise each member shows a port dot that no drawn line reaches.
fn align_bundled_ports(ports: &mut [ResolvedPort], routes: &[Route]) {
    let leaders: BTreeMap<&str, &str> = routes
        .iter()
        .filter_map(|r| r.bundle_of.as_deref().map(|leader| (r.id.as_str(), leader)))
        .collect();
    let leader_positions: BTreeMap<(String, String), Point> = ports
        .iter()
        .filter(|p| !leaders.contains_key(p.edge_id.as_str()))
        .map(|p| ((p.edge_id.clone(), p.endpoint.clone()), p.position.clone()))
        .collect();
    for port in ports.iter_mut() {
        if let Some(leader) = leaders.get(port.edge_id.as_str()) {
            let key = (leader.to_string(), port.endpoint.clone());
            if let Some(position) = leader_positions.get(&key) {
                port.position = position.clone();
            }
        }
    }
}

/// Complete bundling: edges with the same source and the same target share one drawn route. The
/// leader is the lowest edge id; members keep their identity and point to the leader.
fn bundle_complete(graph: &Graph, mut routes: Vec<Route>) -> Vec<Route> {
    let endpoints: BTreeMap<&str, (&str, Option<&str>)> = graph
        .edges
        .iter()
        .map(|e| (e.id.as_str(), (e.source.as_str(), e.target.as_deref())))
        .collect();
    let mut groups: BTreeMap<(&str, Option<&str>), Vec<usize>> = BTreeMap::new();
    for (index, route) in routes.iter().enumerate() {
        // Explicit endpoint contracts cannot be replaced by a leader's ports.
        let bound = graph
            .edge_ports
            .iter()
            .any(|p| p.edge_id == route.id && (p.source_port.is_some() || p.target_port.is_some()));
        // Loops of one node stay separate routes: each is drawn on its own turn around the node and stays selectable.
        if !bound {
            if let Some(key) = endpoints
                .get(route.id.as_str())
                .filter(|(source, target)| Some(*source) != *target)
            {
                groups.entry(*key).or_default().push(index);
            }
        }
    }
    for indices in groups.values().filter(|indices| indices.len() > 1) {
        let mut ordered = indices.clone();
        ordered.sort_by(|a, b| routes[*a].id.cmp(&routes[*b].id));
        let leader = ordered[0];
        let leader_id = routes[leader].id.clone();
        let points = routes[leader].points.clone();
        routes[leader].weight = ordered.len();
        for &member in &ordered[1..] {
            routes[member].points = points.clone();
            routes[member].bundle_of = Some(leader_id.clone());
        }
    }
    routes
}
#[cfg(test)]
mod quality_tests {
    use super::*;
    #[test]
    fn crossing_excludes_touches_and_collinear() {
        assert!(proper_crossing((0., 0.), (2., 2.), (0., 2.), (2., 0.)));
        assert!(!proper_crossing((0., 0.), (2., 2.), (2., 2.), (3., 0.)));
        assert!(!proper_crossing((0., 0.), (2., 0.), (1., 0.), (3., 0.)));
    }
    #[test]
    fn crossings_outrank_length_and_motion_breaks_ties() {
        let a = Quality {
            bends: 0,
            edge_node_hits: 0,
            routed_crossings: 0,
            shared_route_pairs: 0,
            sibling_overlaps: 0,
            projected_crossings: 0,
            near_node_segments: 0,
            route_length: 100.,
            area: 100.,
            movement: 0.,
            end_to_end: 0.,
        };
        let mut b = a.clone();
        b.projected_crossings = 1;
        b.route_length = 1.;
        assert!(better(&a, &b));
        b = a.clone();
        b.movement = 20.;
        assert!(!better(&b, &a));
        b.sibling_overlaps = 1;
        assert!(!better(&b, &a));
    }
}

/// Routing-only candidate preserves every node position. Bundle members follow their leader. Channel views use
/// the best congestion round; `reroute_last_round` is the same candidate with the last round, kept as a baseline.
pub fn reroute(snapshot: &Snapshot) -> Snapshot {
    let (result, _) = reroute_counted(snapshot, true, &|| false);
    result.expect("a search without a stop signal cannot be interrupted")
}

/// The routing-only candidate with the last congestion round (channel views), the baseline for `reroute`.
pub fn reroute_last_round(snapshot: &Snapshot) -> Snapshot {
    let (result, _) = reroute_counted(snapshot, false, &|| false);
    result.expect("a search without a stop signal cannot be interrupted")
}

/// `reroute` (`select = true`) or `reroute_last_round` (`select = false`), with the channel router's search work.
/// `Err(Stopped)` when `stop` interrupts the search; the partial result is then discarded.
/// The counters are returned even when the search is interrupted: the work done before the stop is still reported.
pub fn reroute_counted(
    snapshot: &Snapshot,
    select: bool,
    stop: &dyn Fn() -> bool,
) -> (
    Result<Snapshot, arqlet_routing::channel::Stopped>,
    arqlet_routing::channel::SearchCounters,
) {
    let mut counters = arqlet_routing::channel::SearchCounters::default();
    let result = reroute_with(snapshot, select, Selection::Shared, &mut counters, stop);
    (result, counters)
}

/// The routing-only candidate with the extended conflict selection (experimental, P1): the last-round and best-round
/// selection of `reroute_counted`, with later rounds that also re-route crossing routes and hugging routes.
pub fn reroute_extended_counted(
    snapshot: &Snapshot,
    stop: &dyn Fn() -> bool,
) -> (
    Result<Snapshot, arqlet_routing::channel::Stopped>,
    arqlet_routing::channel::SearchCounters,
) {
    let mut counters = arqlet_routing::channel::SearchCounters::default();
    let result = reroute_with(snapshot, true, Selection::Extended, &mut counters, stop);
    (result, counters)
}

fn reroute_with(
    snapshot: &Snapshot,
    select: bool,
    selection: Selection,
    counters: &mut arqlet_routing::channel::SearchCounters,
    stop: &dyn Fn() -> bool,
) -> Result<Snapshot, arqlet_routing::channel::Stopped> {
    let mut result = snapshot.clone();
    let port_context = ports::PortContext::new(&snapshot.graph)
        .with_flow_sides(snapshot.options.layered_construction);
    let straight: BTreeSet<String> = snapshot.straight_edges.iter().cloned().collect();
    if !straight.is_empty()
        && snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.target.is_some())
            .all(|e| straight.contains(&e.id))
    {
        // Every edge is straight: nothing for the orthogonal router to improve. The graph was validated by `compute`.
        let port_started = stage_timer_start();
        styles::apply_straight_edges(&mut result, &straight).ok();
        counters.port_resolution_nanos = counters
            .port_resolution_nanos
            .saturating_add(elapsed_nanos(port_started));
        return Ok(result);
    }
    if snapshot.options.routing_engine == RoutingEngine::Channel {
        let routing_started = stage_timer_start();
        let routes = best_channel_routes(
            &snapshot.graph,
            &snapshot.boxes,
            &snapshot.options,
            select,
            0.,
            selection,
            counters,
            stop,
            &port_context,
        );
        counters.routing_nanos = counters
            .routing_nanos
            .saturating_add(elapsed_nanos(routing_started));
        result.routes = routes?;
        let port_started = stage_timer_start();
        styles::apply_straight_edges(&mut result, &straight).ok();
        counters.port_resolution_nanos = counters
            .port_resolution_nanos
            .saturating_add(elapsed_nanos(port_started));
        finish_routes(&mut result, true);
        result.recomputed_containers.clear();
        labels::decorate(&mut result);
        return Ok(result);
    }
    if !straight.is_empty() {
        // The orthogonal edges are improved around the straight segments, which stay where the ports put them.
        styles::route_orthogonal_around(&mut result, &straight, stop, counters)?;
        finish_routes(&mut result, false);
        result.recomputed_containers.clear();
        labels::decorate(&mut result);
        return Ok(result);
    }
    let rects = absolute_all(&snapshot.graph, &snapshot.boxes);
    let drawn: Vec<Route> = snapshot
        .routes
        .iter()
        .filter(|r| r.bundle_of.is_none())
        .cloned()
        .collect();
    let routing_started = stage_timer_start();
    // Experimental (consistent layered pipeline): with hierarchical ordering most routes come from the plans' sections, and
    // re-routing them all would give that up; only the routes that share a segment with another or run through a node are
    // routed again, around the others, which stay as they are.
    let improved = if snapshot.options.hierarchical_ordering {
        let conflicting = snapshot.route_conflicts.iter().map(|c| c.edge_id.as_str());
        let refs: Vec<&Route> = drawn.iter().collect();
        let prepared = arqlet_routing::PreparedRoutes::new(&refs);
        let prepared = &prepared;
        let drawn_ref = &drawn;
        let shared: BTreeSet<&str> = (0..drawn.len())
            .flat_map(|i| {
                (i + 1..drawn.len())
                    .filter(move |&j| prepared.shares_segment(i, j))
                    .flat_map(move |j| [drawn_ref[i].id.as_str(), drawn_ref[j].id.as_str()])
            })
            .chain(conflicting)
            .collect();
        let (moving, fixed): (Vec<Route>, Vec<Route>) = drawn
            .iter()
            .cloned()
            .partition(|r| shared.contains(r.id.as_str()));
        arqlet_routing::improve_until_counted_with_fixed(
            &snapshot.graph,
            &rects,
            &moving,
            &fixed,
            routing_params(&snapshot.options),
            stop,
            counters,
        )
        .map(|routes| routes.into_iter().chain(fixed).collect::<Vec<_>>())
    } else {
        arqlet_routing::improve_until_counted(
            &snapshot.graph,
            &rects,
            &drawn,
            routing_params(&snapshot.options),
            stop,
            counters,
        )
    };
    counters.routing_nanos = counters
        .routing_nanos
        .saturating_add(elapsed_nanos(routing_started));
    let improved: BTreeMap<String, Vec<Point>> =
        improved?.into_iter().map(|r| (r.id, r.points)).collect();
    result.routes = snapshot
        .routes
        .iter()
        .map(|r| {
            let leader = r.bundle_of.as_deref().unwrap_or(&r.id);
            let mut route = r.clone();
            route.points = improved[leader].clone();
            route
        })
        .collect();
    // Straight edges keep their segments: the improvement above may have moved them, the orthogonal routes did not need them.
    let port_started = stage_timer_start();
    styles::apply_straight_edges(&mut result, &straight).ok();
    counters.port_resolution_nanos = counters
        .port_resolution_nanos
        .saturating_add(elapsed_nanos(port_started));
    // The legacy re-route maps bundle members onto their leaders itself, so no bundling step runs here.
    finish_routes(&mut result, false);
    result.recomputed_containers.clear();
    labels::decorate(&mut result);
    Ok(result)
}

/// Explicit buses (provisional, `bus::draw_buses`) on `routes`, with their diagnostics in the snapshot (replacing earlier
/// ones).
fn draw_buses_on(snapshot: &mut Snapshot, routes: &mut [Route]) {
    snapshot
        .layout_diagnostics
        .retain(|d| !d.starts_with("Bus "));
    if snapshot.graph.buses.is_empty() {
        return;
    }
    let rects = absolute_all(&snapshot.graph, &snapshot.boxes);
    let trunks = bus_layout::trunks(snapshot);
    let found = bus::draw_buses(
        &snapshot.graph,
        &rects,
        routes,
        &snapshot.graph.buses,
        routing_margin(&snapshot.options),
        &trunks,
    );
    snapshot.layout_diagnostics.extend(found);
}

/// Explicit buses on a finished snapshot (the single-pass completion, which does not run `finish_routes`): the routes get
/// their trunks, the automatic ports move to the routes' new ends, and the boundary crossings and route diagnostics follow.
fn apply_buses(snapshot: &mut Snapshot) {
    if snapshot.graph.buses.is_empty() {
        return;
    }
    let mut routes = std::mem::take(&mut snapshot.routes);
    draw_buses_on(snapshot, &mut routes);
    let by_id: BTreeMap<&str, &Route> = routes.iter().map(|r| (r.id.as_str(), r)).collect();
    for port in snapshot.resolved_ports.iter_mut().filter(|p| !p.explicit) {
        if let Some(route) = by_id.get(port.edge_id.as_str()) {
            let end = if port.endpoint == "source" {
                route.points.first()
            } else {
                route.points.last()
            };
            if let Some(end) = end {
                port.position = end.clone();
            }
        }
    }
    let drawn: Vec<Route> = routes
        .iter()
        .filter(|r| r.bundle_of.is_none())
        .cloned()
        .collect();
    snapshot.boundary_crossings =
        ports::boundaries(&snapshot.graph, &snapshot.boxes, &drawn, &BTreeSet::new());
    snapshot.routes = routes;
    labels::decorate(snapshot);
}

/// The post-routing tail shared by every candidate: bundling (when `bundle`), port resolution, port alignment
/// to bundle leaders, and boundary crossings. Routes, ports and crossings all come from the same final routes.
fn finish_routes(snapshot: &mut Snapshot, bundle: bool) {
    let routes = std::mem::take(&mut snapshot.routes);
    let mut routes = match (bundle, snapshot.options.bundling) {
        (true, Bundling::Complete) => bundle_complete(&snapshot.graph, routes),
        _ => routes,
    };
    // Explicit buses (provisional): their edges share a trunk on the finished routes; the ports below follow the routes.
    draw_buses_on(snapshot, &mut routes);
    // The same port rule as the routing: with the experimental layered construction, automatic ports of forward edges sit
    // on the flow sides, so the published ports are the ones the routes start and end at.
    let mut resolved = ports::PortContext::new(&snapshot.graph)
        .with_flow_sides(snapshot.options.layered_construction)
        .resolve_with(
            &snapshot.boxes,
            snapshot.options.port_placement,
            snapshot.options.min_port_spacing,
            &snapshot.options.node_rows,
            snapshot.options.row_port_order,
        );
    align_bundled_ports(&mut resolved, &routes);
    // Experimental (layered construction): the construction may give a constructed edge another port slot on the same side,
    // so a port takes the end of its route when that end lies on the port's side of its node.
    if snapshot.options.layered_construction {
        let rects = absolute_all(&snapshot.graph, &snapshot.boxes);
        let by_id: BTreeMap<&str, &Route> = routes.iter().map(|r| (r.id.as_str(), r)).collect();
        for port in resolved.iter_mut().filter(|p| !p.explicit) {
            let (Some(route), Some(r)) =
                (by_id.get(port.edge_id.as_str()), rects.get(&port.node_id))
            else {
                continue;
            };
            let end = if port.endpoint == "source" {
                route.points.first()
            } else {
                route.points.last()
            };
            let Some(end) = end else {
                continue;
            };
            const EPS: f64 = 1e-6;
            let within_x = end.x >= r.x - EPS && end.x <= r.x + r.width + EPS;
            let within_y = end.y >= r.y - EPS && end.y <= r.y + r.height + EPS;
            let on_side = match port.side {
                PortSide::Left => (end.x - r.x).abs() < EPS && within_y,
                PortSide::Right => (end.x - r.x - r.width).abs() < EPS && within_y,
                PortSide::Top => (end.y - r.y).abs() < EPS && within_x,
                PortSide::Bottom => (end.y - r.y - r.height).abs() < EPS && within_x,
            };
            if on_side {
                port.position = end.clone();
            }
        }
    }
    let drawn: Vec<Route> = routes
        .iter()
        .filter(|r| r.bundle_of.is_none())
        .cloned()
        .collect();
    snapshot.boundary_crossings =
        ports::boundaries(&snapshot.graph, &snapshot.boxes, &drawn, &BTreeSet::new());
    snapshot.resolved_ports = resolved;
    snapshot.routes = routes;
}

const EPS: f64 = 1e-9;
/// Distance within which a segment counts as hugging a foreign node.
pub const NODE_MARGIN: f64 = 12.;

/// Distance between an axis-aligned segment and a rectangle; zero when they touch.
fn segment_rect_distance(a: &Point, b: &Point, r: &Rect) -> f64 {
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    let dx = (r.x - x1).max(0.).max(x0 - (r.x + r.width));
    let dy = (r.y - y1).max(0.).max(y0 - (r.y + r.height));
    dx.hypot(dy)
}

/// A placement move: the nodes it moves, the first node's target, and the partner's target for swaps.
type SearchMove<'a> = (Vec<&'a str>, (f64, f64), Option<(f64, f64)>);

/// Routing-aware placement search. Starting from a view, leaves move to positions suggested by their
/// neighbours' centres, or swap positions with a nearby sibling. Each candidate is re-routed locally
/// (see `local_candidate`) and kept only if the configured objectives improve and the geometry stays
/// valid. Greedy first improvement; `budget` bounds the number of evaluations. A final full re-route
/// is kept only if it scores better. Returns the input when nothing improves.
pub fn search_placement(snapshot: &Snapshot, budget: usize) -> Result<Snapshot, String> {
    search_placement_until(snapshot, budget, &|| false)
}

/// Cooperative cancellation between complete candidates; returns the best completed geometry.
/// The host supplies the clock/cancellation policy, keeping this core portable to WASM.
pub fn search_placement_until(
    snapshot: &Snapshot,
    budget: usize,
    stop: &dyn Fn() -> bool,
) -> Result<Snapshot, String> {
    search_placement_counted(snapshot, budget, stop, &mut Default::default())
        .map(|(search, _)| search)
}

/// `search_placement_until` with the number of placement trials evaluated (rejected trials included) and the channel
/// router's work for every trial's re-route, accumulated in `search`.
pub fn search_placement_counted(
    snapshot: &Snapshot,
    budget: usize,
    stop: &dyn Fn() -> bool,
    search: &mut arqlet_routing::channel::SearchCounters,
) -> Result<(Snapshot, usize), String> {
    if stop() {
        return Ok((snapshot.clone(), 0));
    }

    let graph = &snapshot.graph;
    let port_context =
        ports::PortContext::new(graph).with_flow_sides(snapshot.options.layered_construction);
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let leaves: Vec<&str> = graph
        .nodes
        .iter()
        .map(|n| n.id.as_str())
        .filter(|id| !containers.contains(id))
        .collect();
    let mut neighbours: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for edge in &graph.edges {
        if let Some(target) = edge.target.as_deref().filter(|t| *t != edge.source) {
            neighbours
                .entry(edge.source.as_str())
                .or_default()
                .insert(target);
            neighbours
                .entry(target)
                .or_default()
                .insert(edge.source.as_str());
        }
    }
    let grid = snapshot.options.placement_grid;
    let mut current = snapshot.clone();
    // Candidates are compared on the published key, the same values the metrics report, so the search cannot accept
    // a route set that the published result would score worse.
    let mut best_key = published_key(snapshot)?;
    let mut spent = 0;
    let mut improved = true;
    while improved && spent < budget && !stop() {
        improved = false;
        for leaf in &leaves {
            if spent >= budget || stop() {
                break;
            }
            if snapshot.options.pins.contains_key(*leaf) {
                continue;
            }
            let absolute = absolute_all(graph, &current.boxes);
            let own = absolute[*leaf].clone();
            let parent = parents[leaf].map(|p| (absolute[p].clone(), current.boxes[p].clone()));
            let mut moves: Vec<SearchMove> = Vec::new();
            let centres: Vec<(f64, f64)> = neighbours
                .get(leaf)
                .into_iter()
                .flatten()
                .filter_map(|other| absolute.get(*other))
                .map(|r| (r.x + r.width / 2., r.y + r.height / 2.))
                .collect();
            let mut targets: Vec<(f64, f64)> = centres
                .iter()
                .flat_map(|(cx, cy)| [(cx - own.width / 2., own.y), (own.x, cy - own.height / 2.)])
                .collect();
            targets.sort_by(|a, b| {
                ((a.0 - own.x).abs() + (a.1 - own.y).abs())
                    .total_cmp(&((b.0 - own.x).abs() + (b.1 - own.y).abs()))
                    .then_with(|| a.0.total_cmp(&b.0))
                    .then_with(|| a.1.total_cmp(&b.1))
            });
            targets.dedup_by(|a, b| (a.0 - b.0).abs() < EPS && (a.1 - b.1).abs() < EPS);
            for target in targets.into_iter().take(6) {
                moves.push((vec![*leaf], target, None));
            }
            let mut siblings: Vec<&str> = leaves
                .iter()
                .copied()
                .filter(|other| {
                    other != leaf
                        && parents[other] == parents[leaf]
                        && !snapshot.options.pins.contains_key(*other)
                })
                .collect();
            siblings.sort_by(|a, b| {
                let da = (absolute[*a].x - own.x).abs() + (absolute[*a].y - own.y).abs();
                let db = (absolute[*b].x - own.x).abs() + (absolute[*b].y - own.y).abs();
                da.total_cmp(&db).then_with(|| a.cmp(b))
            });
            for other in siblings.into_iter().take(3) {
                let target = (absolute[other].x, absolute[other].y);
                moves.push((vec![*leaf, other], target, Some((own.x, own.y))));
            }
            for (moved, target, partner_target) in moves {
                if spent >= budget || stop() {
                    break;
                }
                let mut boxes = current.boxes.clone();
                let mut placed = true;
                {
                    let mut place = |id: &str, x: f64, y: f64| {
                        let size = absolute[id].clone();
                        let (mut x, mut y) = (x, y);
                        let (ox, oy) = match &parent {
                            Some((origin, container)) => {
                                let (min_x, max_x) = (
                                    origin.x + 12.,
                                    origin.x + container.width - 12. - size.width,
                                );
                                let (min_y, max_y) = (
                                    origin.y + 36.,
                                    origin.y + container.height - 12. - size.height,
                                );
                                if max_x < min_x || max_y < min_y {
                                    placed = false;
                                    return;
                                }
                                x = x.clamp(min_x, max_x);
                                y = y.clamp(min_y, max_y);
                                // With a placement grid, moves land on grid lines or are rejected.
                                if grid > 0. {
                                    let (gx, gy) =
                                        ((x / grid).round() * grid, (y / grid).round() * grid);
                                    if gx < min_x || gx > max_x || gy < min_y || gy > max_y {
                                        placed = false;
                                        return;
                                    }
                                    x = gx;
                                    y = gy;
                                }
                                (origin.x, origin.y)
                            }
                            None => (0., 0.),
                        };
                        let rect = boxes.get_mut(id).expect("moved box");
                        rect.x = x - ox;
                        rect.y = y - oy;
                    };
                    place(moved[0], target.0, target.1);
                    if let (Some(partner), Some((px, py))) = (moved.get(1), partner_target) {
                        place(partner, px, py);
                    }
                }
                if !placed || same_boxes(&boxes, &current.boxes) {
                    continue;
                }
                spent += 1;
                let Some(candidate) =
                    local_candidate(&current, &boxes, &moved, stop, search, &port_context)
                else {
                    continue;
                };
                let Ok(key) = published_key(&candidate) else {
                    continue;
                };
                if !is_ineligible_geometry(
                    &candidate.geometry_issues,
                    &candidate.graph,
                    &candidate.options.container_sizing,
                ) && improves(&key, &best_key)
                {
                    best_key = key;
                    current = candidate;
                    improved = true;
                    break;
                }
            }
        }
    }
    // Local moves leave untouched routes as they were; one full re-route can still improve them.
    if let Some(full) = (!stop())
        .then(|| assemble_with(&current, &current.boxes, 0., stop, search, &port_context))
        .flatten()
    {
        if let Ok(key) = published_key(&full) {
            if !is_ineligible_geometry(
                &full.geometry_issues,
                &full.graph,
                &full.options.container_sizing,
            ) && improves(&key, &best_key)
            {
                current = full;
            }
        }
    }
    current.optimization = None;
    Ok((current, spent))
}

/// `assemble` with a crossing cost for the channel router (see `FLAT_CROSSING_COST`); zero is the normal search.
/// `None` when the boxes are unusable, or when `stop` fires during routing (no partial routes are returned).
fn assemble_with(
    base: &Snapshot,
    boxes: &BTreeMap<String, Rect>,
    crossing: f64,
    stop: &dyn Fn() -> bool,
    search: &mut arqlet_routing::channel::SearchCounters,
    port_context: &ports::PortContext<'_>,
) -> Option<Snapshot> {
    let graph = &base.graph;
    let rects = absolute_all(graph, boxes);
    let routing_started = stage_timer_start();
    let routed_result = if base.options.routing_engine == RoutingEngine::Channel {
        best_channel_routes(
            graph,
            boxes,
            &base.options,
            false,
            crossing,
            Selection::Shared,
            search,
            stop,
            port_context,
        )
    } else {
        let initial = route(graph, boxes, &base.options, search, port_context);
        arqlet_routing::improve_until_counted(
            graph,
            &rects,
            &initial,
            routing_params(&base.options),
            stop,
            search,
        )
    };
    search.routing_nanos = search
        .routing_nanos
        .saturating_add(elapsed_nanos(routing_started));
    let routed = routed_result.ok()?;
    let mut candidate = base.clone();
    candidate.boxes = boxes.clone();
    candidate.routes = routed;
    finish_routes(&mut candidate, true);
    candidate.geometry_issues =
        arqlet_scene::validate_geometry(graph, boxes, arqlet_scene::GeometryRules::default());
    candidate.layout_version = base.layout_version + 1;
    labels::decorate(&mut candidate);
    Some(candidate)
}

/// Candidate for one moved leaf. Only the leaf's edges and the routes that touch its old or new
/// position are re-searched; other routes stay fixed. Bundled views use the full re-route, because
/// members must follow their leaders.
/// The edges a change of the `moved` nodes can affect: every edge incident to a moved node, and every route that
/// touches a moved node's old or new box. A route that a node leaves or enters is therefore included. Absolute
/// rectangles (`old`, `new`) describe the placement before and after the change.
fn affected_edges(
    graph: &Graph,
    routes: &[Route],
    old: &BTreeMap<String, Rect>,
    new: &BTreeMap<String, Rect>,
    moved: &[&str],
) -> BTreeSet<String> {
    let mut affected: BTreeSet<String> = graph
        .edges
        .iter()
        .filter(|e| {
            e.target.is_some()
                && moved
                    .iter()
                    .any(|m| e.source == *m || e.target.as_deref() == Some(*m))
        })
        .map(|e| e.id.clone())
        .collect();
    for route in routes {
        if moved
            .iter()
            .any(|m| arqlet_routing::hits(route, &old[*m]) || arqlet_routing::hits(route, &new[*m]))
        {
            affected.insert(route.id.clone());
        }
    }
    affected
}

/// Edges whose port positions change when `boxes` replaces the snapshot's boxes. Port sides and orders depend on the
/// peers' positions, so a move can shift ports of edges that are not incident to the moved node.
fn port_changed_edges(
    base: &Snapshot,
    boxes: &BTreeMap<String, Rect>,
    port_context: &ports::PortContext<'_>,
) -> BTreeSet<String> {
    let options = &base.options;
    let positions = |resolved: Vec<ResolvedPort>| -> BTreeMap<(String, String), (f64, f64)> {
        resolved
            .into_iter()
            .map(|p| ((p.edge_id, p.endpoint), (p.position.x, p.position.y)))
            .collect()
    };
    let before = positions(port_context.resolve_with(
        &base.boxes,
        options.port_placement,
        options.min_port_spacing,
        &options.node_rows,
        options.row_port_order,
    ));
    let after = positions(port_context.resolve_with(
        boxes,
        options.port_placement,
        options.min_port_spacing,
        &options.node_rows,
        options.row_port_order,
    ));
    after
        .iter()
        .filter(|(key, pos)| {
            !before
                .get(*key)
                .is_some_and(|old| (old.0 - pos.0).abs() < EPS && (old.1 - pos.1).abs() < EPS)
        })
        .map(|(key, _)| key.0.clone())
        .collect()
}

/// The bounding box of a polyline, widened by one unit so that a straight run has an interior.
fn bounding_box(points: &[Point]) -> Option<Rect> {
    let first = points.first()?;
    let (mut min_x, mut max_x, mut min_y, mut max_y) = (first.x, first.x, first.y, first.y);
    for p in points {
        min_x = min_x.min(p.x);
        max_x = max_x.max(p.x);
        min_y = min_y.min(p.y);
        max_y = max_y.max(p.y);
    }
    Some(Rect {
        x: min_x - 1.,
        y: min_y - 1.,
        width: max_x - min_x + 2.,
        height: max_y - min_y + 2.,
    })
}

/// True when every segment of the route is horizontal or vertical.
fn is_orthogonal(route: &Route) -> bool {
    route
        .points
        .windows(2)
        .all(|w| (w[0].x - w[1].x).abs() < EPS || (w[0].y - w[1].y).abs() < EPS)
}

/// The channel router's local transaction: the affected edges, and the edges whose ports moved, are re-routed around
/// the unchanged routes, which keep their geometry. `None` when the router is stopped, a fixed route cannot be placed,
/// or the composed result is invalid; the caller then re-routes completely.
fn local_channel_candidate(
    base: &Snapshot,
    boxes: &BTreeMap<String, Rect>,
    moved: &[&str],
    stop: &dyn Fn() -> bool,
    search: &mut arqlet_routing::channel::SearchCounters,
    port_context: &ports::PortContext<'_>,
) -> Option<Snapshot> {
    let graph = &base.graph;
    let old = absolute_all(graph, &base.boxes);
    let new = absolute_all(graph, boxes);
    let mut changed = affected_edges(graph, &base.routes, &old, &new, moved);
    changed.extend(port_changed_edges(base, boxes, port_context));
    // A route that is not axis-aligned is the router's direct fallback for an edge it could not place (an explicit
    // conflict). It is re-routed with the others instead of being kept as fixed geometry.
    changed.extend(
        base.routes
            .iter()
            .filter(|route| !is_orthogonal(route))
            .map(|route| route.id.clone()),
    );
    // Escalation: when the composed result has a hard geometry problem, the fixed routes that run through the new
    // placement are released (they become changed) and the set is routed again. Bounded by ESCALATION_ROUNDS; after
    // that the caller falls back to a complete re-route.
    for round in 0..=ESCALATION_ROUNDS {
        let fixed: Vec<Route> = base
            .routes
            .iter()
            .filter(|route| !changed.contains(&route.id))
            .cloned()
            .collect();
        let mut fresh: BTreeMap<String, Route> = channel_subset_routes(
            graph,
            boxes,
            &base.options,
            &changed,
            &fixed,
            search,
            stop,
            port_context,
        )
        .ok()??
        .into_iter()
        .map(|route| (route.id.clone(), route))
        .collect();
        let mut routes = Vec::with_capacity(base.routes.len());
        for route in &base.routes {
            if changed.contains(&route.id) {
                routes.push(fresh.remove(&route.id)?);
            } else {
                routes.push(route.clone());
            }
        }
        let mut candidate = base.clone();
        candidate.boxes = boxes.clone();
        candidate.routes = routes;
        finish_routes(&mut candidate, true);
        candidate.geometry_issues =
            arqlet_scene::validate_geometry(graph, boxes, arqlet_scene::GeometryRules::default());
        if candidate.geometry_issues.is_empty() {
            candidate.layout_version = base.layout_version + 1;
            labels::decorate(&mut candidate);
            return Some(candidate);
        }
        if round == ESCALATION_ROUNDS {
            break;
        }
        // Region around the conflict: fixed routes that run through the bounding box of a re-routed edge. Those are
        // the routes whose corridors the re-routed edge competes for; releasing them lets the router move them too.
        let regions: Vec<Rect> = candidate
            .routes
            .iter()
            .filter(|route| changed.contains(&route.id))
            .filter_map(|route| bounding_box(&route.points))
            .collect();
        let released: Vec<String> = fixed
            .iter()
            .filter(|route| {
                regions
                    .iter()
                    .any(|region| arqlet_routing::hits(route, region))
            })
            .map(|route| route.id.clone())
            .collect();
        if released.is_empty() {
            break;
        }
        changed.extend(released);
    }
    None
}

/// How many times a local transaction releases conflicting fixed routes before it gives up (explicit fallback).
const ESCALATION_ROUNDS: usize = 2;

fn local_candidate(
    base: &Snapshot,
    boxes: &BTreeMap<String, Rect>,
    moved: &[&str],
    stop: &dyn Fn() -> bool,
    search: &mut arqlet_routing::channel::SearchCounters,
    port_context: &ports::PortContext<'_>,
) -> Option<Snapshot> {
    if base.options.routing_engine == RoutingEngine::Channel
        && base.options.bundling != Bundling::Complete
    {
        if let Some(candidate) =
            local_channel_candidate(base, boxes, moved, stop, search, port_context)
        {
            return Some(candidate);
        }
    }
    if base.options.bundling == Bundling::Complete
        || base.options.routing_engine == RoutingEngine::Channel
    {
        // The explicit fallback: a complete re-route, used only when the local transaction cannot place its routes.
        return assemble_with(base, boxes, 0., stop, search, port_context);
    }
    let graph = &base.graph;
    let old = absolute_all(graph, &base.boxes);
    let new = absolute_all(graph, boxes);
    let changed = affected_edges(graph, &base.routes, &old, &new, moved);
    let fresh: BTreeMap<String, Route> = route(graph, boxes, &base.options, search, port_context)
        .into_iter()
        .map(|r| (r.id.clone(), r))
        .collect();
    let mut routes = base.routes.clone();
    for route in routes.iter_mut() {
        if let Some(initial) = fresh.get(&route.id).filter(|_| changed.contains(&route.id)) {
            *route = initial.clone();
        }
    }
    let routes = arqlet_routing::improve_subset_counted(
        graph,
        &new,
        &routes,
        &changed,
        routing_params(&base.options),
        search,
    );
    let mut candidate = base.clone();
    candidate.boxes = boxes.clone();
    candidate.routes = routes;
    finish_routes(&mut candidate, true);
    candidate.geometry_issues =
        arqlet_scene::validate_geometry(graph, boxes, arqlet_scene::GeometryRules::default());
    candidate.layout_version = base.layout_version + 1;
    labels::decorate(&mut candidate);
    Some(candidate)
}

/// True when two box maps place every node at the same position.
fn same_boxes(a: &BTreeMap<String, Rect>, b: &BTreeMap<String, Rect>) -> bool {
    a.len() == b.len()
        && a.iter().all(|(id, r)| {
            b.get(id)
                .is_some_and(|o| (o.x - r.x).abs() < EPS && (o.y - r.y).abs() < EPS)
        })
}

fn same_box_geometries(a: &BTreeMap<String, Rect>, b: &BTreeMap<String, Rect>) -> bool {
    a.len() == b.len()
        && a.iter().all(|(id, r)| {
            b.get(id).is_some_and(|o| {
                r.x.to_bits() == o.x.to_bits()
                    && r.y.to_bits() == o.y.to_bits()
                    && r.width.to_bits() == o.width.to_bits()
                    && r.height.to_bits() == o.height.to_bits()
            })
        })
}

#[cfg(test)]
mod round_tests {
    use super::*;

    /// Every congestion round is a complete candidate: one route per drawn edge, in each round. While round
    /// selection is off, the published routes are the last round.
    #[test]
    fn channel_rounds_are_complete_candidates() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/viewer/fixtures/state-tcp.snapshot.json");
        let snapshot: Snapshot =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let port_context = ports::PortContext::new(&snapshot.graph);
        let rounds = channel_rounds(
            &snapshot.graph,
            &snapshot.boxes,
            &snapshot.options,
            0.,
            arqlet_routing::channel::Selection::Shared,
            &mut Default::default(),
            &|| false,
            &port_context,
        )
        .expect(UNINTERRUPTED);
        let drawn: BTreeSet<&str> = snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.target.is_some())
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(rounds.len(), arqlet_routing::channel::ROUNDS);
        for round in &rounds {
            let ids: BTreeSet<&str> = round.iter().map(|r| r.id.as_str()).collect();
            assert_eq!(ids, drawn, "a round holds one route per drawn edge");
        }
        let published = best_channel_routes(
            &snapshot.graph,
            &snapshot.boxes,
            &snapshot.options,
            false,
            0.,
            Selection::Shared,
            &mut Default::default(),
            &|| false,
            &port_context,
        )
        .expect(UNINTERRUPTED);
        assert_eq!(
            serde_json::to_value(&published).unwrap(),
            serde_json::to_value(rounds.last().unwrap()).unwrap()
        );
    }

    /// A candidate that cannot be measured has no key: it is never scored as if it were empty.
    #[test]
    fn an_unmeasurable_candidate_has_no_key() {
        let drawing = arqlet_scene::metrics::Drawing {
            nodes: vec![arqlet_scene::metrics::DrawNode {
                id: "a".into(),
                parent_id: Some("missing".into()),
                container: false,
                x: 0.,
                y: 0.,
                width: 20.,
                height: 20.,
            }],
            edges: vec![],
            track_spacing: 6.,
            buses: Vec::new(),
        };
        assert!(conflict_key(
            &drawing,
            &DEFAULT_OBJECTIVES,
            (0, 0.),
            arqlet_scene::metrics::HUG_MARGIN
        )
        .is_err());
    }

    /// The key follows the configured objective order, so reordering the objectives reorders the key.
    #[test]
    fn the_key_follows_the_configured_objective_order() {
        let drawing = arqlet_scene::metrics::Drawing {
            nodes: vec![],
            edges: vec![],
            track_spacing: 6.,
            buses: Vec::new(),
        };
        let by_cost = conflict_key(
            &drawing,
            &[Objective::Cost, Objective::EdgeNodeHits],
            (0, 0.),
            arqlet_scene::metrics::HUG_MARGIN,
        )
        .unwrap();
        let by_hits = conflict_key(
            &drawing,
            &[Objective::EdgeNodeHits, Objective::Cost],
            (0, 0.),
            arqlet_scene::metrics::HUG_MARGIN,
        )
        .unwrap();
        assert_eq!(by_cost.len(), 2);
        assert_eq!(by_hits, vec![by_cost[1], by_cost[0]]);
    }
}

#[cfg(test)]
mod pin_tests {
    use super::*;

    fn node(id: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind: "symbol".into(),
            label: id.into(),
            parent_id: parent.map(String::from),
            shape: Default::default(),
        }
    }

    #[test]
    fn a_corridor_with_two_routes_is_widened_by_one_track() {
        // Two stacked leaves inside `c`: the gap between them is 24 (2 x margin 12 exactly). Two routes run through
        // it, so the corridor needs 2 x 12 + 1 x 6 = 30, an extra 6. A corridor with one route keeps its gap.
        let graph = graph(vec![
            node("c", None),
            node("a", Some("c")),
            node("b", Some("c")),
        ]);
        let boxes = BTreeMap::from([
            (
                "c".to_string(),
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 200.,
                    height: 200.,
                },
            ),
            (
                "a".to_string(),
                Rect {
                    x: 10.,
                    y: 10.,
                    width: 50.,
                    height: 30.,
                },
            ),
            (
                "b".to_string(),
                Rect {
                    x: 10.,
                    y: 64.,
                    width: 50.,
                    height: 30.,
                },
            ),
        ]);
        let through =
            |id: &str, y: f64| Route::new(id, vec![Point { x: 0., y }, Point { x: 80., y }]);
        let options = LayoutOptions::default();
        // Two routes on tracks of their own: the corridor needs one track more than it has.
        let two = corridor_gaps(
            &graph,
            &boxes,
            &[through("r1", 50.), through("r2", 56.)],
            &options,
        );
        assert_eq!(two.get(&("a".to_string(), "b".to_string())), Some(&6.));
        assert_eq!(two.get(&("b".to_string(), "a".to_string())), Some(&6.));
        let one = corridor_gaps(&graph, &boxes, &[through("r1", 54.)], &options);
        assert!(one.is_empty(), "one route fits the corridor as placed");
        // Two routes on one line overlap: the corridor also keeps a spare track so the next routing can move them apart.
        let overlapping = corridor_gaps(
            &graph,
            &boxes,
            &[through("r1", 54.), through("r2", 54.)],
            &options,
        );
        assert_eq!(
            overlapping.get(&("a".to_string(), "b".to_string())),
            Some(&12.)
        );
    }

    fn graph(nodes: Vec<Node>) -> Graph {
        Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            nodes,
            edges: vec![],
            ports: vec![],
            edge_ports: vec![],
            buses: Vec::new(),
        }
    }

    fn pinned(pins: &[(&str, f64, f64)]) -> LayoutOptions {
        let mut options = LayoutOptions::default();
        for (id, x, y) in pins {
            options.pins.insert((*id).into(), Point { x: *x, y: *y });
        }
        options
    }

    /// The shadow evaluation is reproducible and scores the maximum when nothing is drawn.
    #[test]
    fn shadow_evaluation_is_reproducible_and_maximal_without_edges() {
        let graph = graph(vec![node("a", None), node("b", None)]);
        let snapshot = compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        let first = shadow_evaluation(&snapshot).unwrap();
        assert_eq!(first, shadow_evaluation(&snapshot).unwrap());
        assert_eq!(first.total, 100_000_000);
    }

    /// A routed edge gives finite utilities in 0..=1 for every term.
    #[test]
    fn shadow_evaluation_of_a_routed_edge_is_finite() {
        let mut graph = graph(vec![node("a", None), node("b", None)]);
        graph.edges.push(Edge {
            id: "e".into(),
            kind: "flow".into(),
            source: "a".into(),
            target: Some("b".into()),
        });
        let snapshot = compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        let eval = shadow_evaluation(&snapshot).unwrap();
        assert!(eval
            .terms
            .iter()
            .all(|t| t.utility.is_finite() && (0. ..=1.).contains(&t.utility)));
        assert!(eval.total > 0);
    }

    #[test]
    fn a_pinned_root_leaf_keeps_its_position() {
        let graph = graph(vec![node("a", None), node("b", None)]);
        let snapshot = compute(
            &graph,
            None,
            &BTreeSet::new(),
            &pinned(&[("a", 300., 100.)]),
        )
        .unwrap();
        let a = snapshot.boxes["a"].clone();
        assert_eq!((a.x, a.y), (300., 100.));
    }

    #[test]
    fn a_pinned_child_is_placed_in_its_container_frame() {
        let graph = graph(vec![
            node("p", None),
            node("x", Some("p")),
            node("y", Some("p")),
        ]);
        let options = pinned(&[("x", 20., 40.)]);
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let container = absolute(&snapshot.graph, &snapshot.boxes, "p");
        let child = absolute(&snapshot.graph, &snapshot.boxes, "x");
        assert_eq!((child.x - container.x, child.y - container.y), (20., 40.));
    }

    #[test]
    fn a_pinned_container_keeps_its_place_and_lays_out_its_children_inside() {
        let graph = graph(vec![
            node("p", None),
            node("q", Some("p")),
            node("x", Some("q")),
            node("y", Some("q")),
            node("z", Some("p")),
        ]);
        let options = pinned(&[("p", 400., 120.), ("q", 30., 200.)]);
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let p = snapshot.boxes["p"].clone();
        let q = snapshot.boxes["q"].clone();
        assert_eq!((p.x, p.y), (400., 120.));
        assert_eq!((q.x, q.y), (30., 200.), "the pin is in the parent's frame");
        let outer = absolute(&snapshot.graph, &snapshot.boxes, "q");
        for child in ["x", "y"] {
            let inner = absolute(&snapshot.graph, &snapshot.boxes, child);
            assert!(inner.x >= outer.x && inner.x + inner.width <= outer.x + outer.width);
            assert!(inner.y >= outer.y && inner.y + inner.height <= outer.y + outer.height);
        }
    }

    #[test]
    fn the_container_grows_to_contain_a_pinned_child() {
        let graph = graph(vec![node("p", None), node("x", Some("p"))]);
        let options = pinned(&[("x", 900., 700.)]);
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let container = absolute(&snapshot.graph, &snapshot.boxes, "p");
        let child = absolute(&snapshot.graph, &snapshot.boxes, "x");
        assert!(child.x + child.width <= container.x + container.width);
        assert!(child.y + child.height <= container.y + container.height);
    }

    #[test]
    fn a_free_sibling_moves_out_of_the_way_of_a_pin() {
        let graph = graph(vec![
            node("p", None),
            node("x", Some("p")),
            node("y", Some("p")),
        ]);
        let free = compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        let spot = free.boxes["y"].clone();
        let options = pinned(&[("x", spot.x, spot.y)]);
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let x = snapshot.boxes["x"].clone();
        let y = snapshot.boxes["y"].clone();
        assert_eq!((x.x, x.y), (spot.x, spot.y));
        let apart = x.x + x.width + PIN_CLEARANCE <= y.x
            || y.x + y.width + PIN_CLEARANCE <= x.x
            || x.y + x.height + PIN_CLEARANCE <= y.y
            || y.y + y.height + PIN_CLEARANCE <= x.y;
        assert!(apart, "the free sibling keeps the clearance from the pin");
    }

    #[test]
    fn the_placement_search_never_moves_a_pinned_leaf() {
        let graph = graph(vec![node("a", None), node("b", None), node("c", None)]);
        let options = pinned(&[("a", 300., 100.)]);
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let searched = search_placement(&snapshot, 200).unwrap();
        assert_eq!(searched.boxes["a"].x, 300.);
        assert_eq!(searched.boxes["a"].y, 100.);
    }
}

#[cfg(test)]
mod cross_check_tests {
    use super::*;

    /// The engine's quality values and the shared metric (version 3) must agree on the four conflict counts they both
    /// report. Where they differ, the placement search compares on the published key instead (see
    /// `search_placement_until`); a new difference fails here.
    #[test]
    fn engine_quality_agrees_with_the_shared_metric() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graphs = [
            root.join("examples/fixtures/congestion-trap.json"),
            root.join("examples/fixtures/small.json"),
            root.join("benchmarks/inputs/state-tcp.json"),
            root.join("benchmarks/inputs/uml-class.json"),
        ];
        for path in graphs {
            let graph: Graph =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let snapshot =
                compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
            let engine = quality_with(&snapshot, &snapshot, &DEFAULT_OBJECTIVES);
            // The engine counts near-node pairs within the clearance of the options (the default requires one).
            let metric = arqlet_scene::metrics::measure_with_clearance(
                &published_drawing(&snapshot),
                clearance(&snapshot.options),
            )
            .unwrap();
            let name = path.display().to_string();
            assert_eq!(
                engine.edge_node_hits,
                metric.line_node_hits + metric.container_traversals,
                "node hits: {name}"
            );
            assert_eq!(
                engine.shared_route_pairs, metric.shared_line_pairs,
                "shared lines: {name}"
            );
            // The engine counts route pairs; the published key counts crossing points (see `conflict_key`).
            assert_eq!(
                engine.routed_crossings,
                metric.crossing_pairs + metric.shared_endpoint_crossing_pairs,
                "crossing pairs: {name}"
            );
            assert_eq!(
                engine.near_node_segments, metric.hug_pairs,
                "hugging: {name}"
            );
        }
    }
}

#[cfg(test)]
mod pin_diagnostic_tests {
    use super::*;

    #[test]
    fn a_pin_on_a_node_outside_the_view_is_reported_not_dropped() {
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            nodes: vec![Node {
                id: "a".into(),
                kind: "symbol".into(),
                label: "a".into(),
                parent_id: None,
                shape: Default::default(),
            }],
            edges: vec![],
            ports: vec![],
            edge_ports: vec![],
            buses: Vec::new(),
        };
        let mut options = LayoutOptions::default();
        options.pins.insert("gone".into(), Point { x: 1., y: 1. });
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        assert_eq!(
            snapshot.routing_issues,
            vec!["Pin on gone has no effect: the node is not in the visible view".to_string()]
        );
    }
}

/// Experiment (A4b, flat region): node positions from the Chrobak–Payne grid of the skeleton, then constrained
/// compaction. Each corridor starts at its grid spacing and shrinks toward its demand minimum; a shrink is kept only
/// if the complete candidate (routes, ports, labels, validation) is geometrically valid, keeps every node's port order
/// equal to the drawn rotation, and has a strictly smaller published key. The key order decides; length comes last.
/// Only for flat, biconnected views; off by default (option `flatRegion`).
pub fn flat_candidate(base: &Snapshot) -> Result<Option<Snapshot>, String> {
    flat_candidate_until(base, &|| false, &mut Default::default())
}

/// `flat_candidate` with checkpoints: `Ok(None)` when `stop` fires; the compaction never returns a partial drawing.
/// The channel router's work for every trial is accumulated in `search`.
pub fn flat_candidate_until(
    base: &Snapshot,
    stop: &dyn Fn() -> bool,
    search: &mut arqlet_routing::channel::SearchCounters,
) -> Result<Option<Snapshot>, String> {
    let graph = &base.graph;
    let port_context =
        ports::PortContext::new(graph).with_flow_sides(base.options.layered_construction);
    if graph.nodes.iter().any(|n| n.parent_id.is_some()) {
        return Ok(None);
    }
    let index: BTreeMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut edges = Vec::new();
    for edge in &graph.edges {
        if let Some(target) = edge.target.as_deref() {
            edges.push((index[edge.source.as_str()], index[target]));
        }
    }
    let (skeleton, _) = arqlet_topology::graph::Graph::from_edges(graph.nodes.len(), &edges)
        .map_err(|e| e.to_string())?;
    let drawing = match arqlet_topology::draw(&skeleton) {
        Ok(drawing) => drawing,
        Err(arqlet_topology::TopologyError::NotBiconnected)
        | Err(arqlet_topology::TopologyError::NonPlanar) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let sizes: Vec<(f64, f64)> = graph
        .nodes
        .iter()
        .map(|n| (base.boxes[&n.id].width, base.boxes[&n.id].height))
        .collect();
    let ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    let grid = CorridorGrid::new(&drawing.positions, &sizes, &ids);
    let mut angular = base.clone();
    angular.options.port_placement = PortPlacement::Angular;
    let mut cw = grid.start_cols.clone();
    let mut rh = grid.start_rows.clone();
    let check = |candidate: &Snapshot| -> Option<Vec<i64>> {
        if is_ineligible_geometry(
            &candidate.geometry_issues,
            &candidate.graph,
            &candidate.options.container_sizing,
        ) || !ports::overflowing_sides(
            &candidate.graph,
            &candidate.boxes,
            candidate.options.min_port_spacing,
            &candidate.options.node_rows,
        )
        .is_empty()
            || port_rotation_ok(candidate, &drawing.positions, &skeleton, &ids).is_err()
        {
            return None;
        }
        published_key(candidate).ok()
    };
    if stop() {
        return Ok(None);
    }
    let start = assemble_with(
        &angular,
        &grid.boxes(&cw, &rh),
        FLAT_CROSSING_COST,
        stop,
        search,
        &port_context,
    )
    .ok_or("flat candidate: start is unusable")?;
    let mut best_key =
        check(&start).ok_or("flat candidate: start violates the rotation or geometry")?;
    let mut best = start;
    let mut improved = true;
    let mut passes = 0;
    while improved && passes < FLAT_PASSES {
        improved = false;
        passes += 1;
        for vertical in [false, true] {
            let count = if vertical { rh.len() } else { cw.len() };
            for k in 0..count {
                let floor = if vertical {
                    grid.row_min[k]
                } else {
                    grid.col_min[k]
                };
                let width = if vertical { rh[k] } else { cw[k] };
                let mut step = width - floor;
                while step > FLAT_STEP {
                    if stop() {
                        return Ok(None);
                    }
                    let trial = width - step;
                    if vertical {
                        rh[k] = trial;
                    } else {
                        cw[k] = trial;
                    }
                    let accepted = assemble_with(
                        &angular,
                        &grid.boxes(&cw, &rh),
                        FLAT_CROSSING_COST,
                        stop,
                        search,
                        &port_context,
                    )
                    .and_then(|candidate| check(&candidate).map(|key| (candidate, key)))
                    .filter(|(_, key)| improves(key, &best_key));
                    if let Some((candidate, key)) = accepted {
                        best = candidate;
                        best_key = key;
                        improved = true;
                        break;
                    }
                    if vertical {
                        rh[k] = width;
                    } else {
                        cw[k] = width;
                    }
                    step /= 2.;
                }
            }
        }
    }
    Ok(Some(best))
}

/// Cost of a route step onto a point that another route passes through, in the flat-region experiment.
const FLAT_CROSSING_COST: f64 = 200.0;

/// Smallest corridor width, in layout units. Zero: the compaction may close a corridor completely; every shrink is
/// still checked against the complete candidate (validity, rotation, key), so no crossing or hugging is added.
const FLAT_MARGIN: f64 = 0.;

/// Layout units per grid step before compaction; the corridors shrink from here.
const FLAT_UNIT: f64 = 240.;

/// Compaction passes over all corridors, and the smallest shrink tried (layout units).
const FLAT_PASSES: usize = 8;
const FLAT_STEP: f64 = 1.;

/// The corridor structure of a grid drawing: used columns and rows in order, the nodes of each, and the widths of
/// the corridors between them (their starting widths and minima).
struct CorridorGrid {
    col_of: Vec<usize>,
    row_of: Vec<usize>,
    col_width: Vec<f64>,
    row_height: Vec<f64>,
    col_min: Vec<f64>,
    row_min: Vec<f64>,
    start_cols: Vec<f64>,
    start_rows: Vec<f64>,
    sizes: Vec<(f64, f64)>,
    ids: Vec<String>,
}

impl CorridorGrid {
    fn new(positions: &[(i64, i64)], sizes: &[(f64, f64)], ids: &[String]) -> Self {
        let mut cols: Vec<i64> = positions.iter().map(|p| p.0).collect();
        cols.sort_unstable();
        cols.dedup();
        let mut rows: Vec<i64> = positions.iter().map(|p| p.1).collect();
        rows.sort_unstable();
        rows.dedup();
        let col_of: Vec<usize> = positions
            .iter()
            .map(|p| cols.binary_search(&p.0).unwrap_or(0))
            .collect();
        let row_of: Vec<usize> = positions
            .iter()
            .map(|p| rows.binary_search(&p.1).unwrap_or(0))
            .collect();
        let mut col_width = vec![0.0f64; cols.len()];
        let mut row_height = vec![0.0f64; rows.len()];
        for v in 0..positions.len() {
            col_width[col_of[v]] = col_width[col_of[v]].max(sizes[v].0);
            row_height[row_of[v]] = row_height[row_of[v]].max(sizes[v].1);
        }
        Self {
            col_of,
            row_of,
            col_width,
            row_height,
            col_min: vec![FLAT_MARGIN; cols.len().saturating_sub(1)],
            row_min: vec![FLAT_MARGIN; rows.len().saturating_sub(1)],
            start_cols: cols
                .windows(2)
                .map(|w| (w[1] - w[0]) as f64 * FLAT_UNIT)
                .collect(),
            start_rows: rows
                .windows(2)
                .map(|w| (w[1] - w[0]) as f64 * FLAT_UNIT)
                .collect(),
            sizes: sizes.to_vec(),
            ids: ids.to_vec(),
        }
    }

    /// Boxes for corridor widths `cw` (between used columns) and `rh` (between used rows). Each column (row) is as wide
    /// (tall) as its widest node; nodes are centred in their column and row.
    fn boxes(&self, cw: &[f64], rh: &[f64]) -> BTreeMap<String, Rect> {
        let place = |widths: &[f64], gaps: &[f64]| -> Vec<f64> {
            let mut out = Vec::with_capacity(widths.len());
            let mut x = 0.0;
            for (i, w) in widths.iter().enumerate() {
                out.push(x + w / 2.);
                x += w;
                if i < gaps.len() {
                    x += gaps[i];
                }
            }
            out
        };
        let x = place(&self.col_width, cw);
        let y = place(&self.row_height, rh);
        let mut boxes = BTreeMap::new();
        for (v, id) in self.ids.iter().enumerate() {
            let (w, h) = self.sizes[v];
            boxes.insert(
                id.clone(),
                Rect {
                    x: x[self.col_of[v]] - w / 2.,
                    y: y[self.row_of[v]] - h / 2.,
                    width: w,
                    height: h,
                },
            );
        }
        boxes
    }
}

/// Whether the automatic ports of every node with at least three neighbours follow the drawn rotation (`positions` and
/// the skeleton's neighbour order), read clockwise around the box. One global orientation is required; nodes with two
/// neighbours cannot show one.
fn port_rotation_ok(
    snapshot: &Snapshot,
    positions: &[(i64, i64)],
    skeleton: &arqlet_topology::graph::Graph,
    ids: &[String],
) -> Result<(), String> {
    let index: BTreeMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let ends: BTreeMap<&str, (&str, &str)> = snapshot
        .graph
        .edges
        .iter()
        .filter_map(|e| {
            e.target
                .as_deref()
                .map(|t| (e.id.as_str(), (e.source.as_str(), t)))
        })
        .collect();
    let mut orientation: Option<bool> = None;
    for (v, id) in ids.iter().enumerate() {
        let drawn = arqlet_topology::rotation::angular_order(v, skeleton.adj(v), positions);
        if drawn.len() < 3 {
            continue;
        }
        let mut ports: Vec<(u8, f64, usize)> = snapshot
            .resolved_ports
            .iter()
            .filter(|p| &p.node_id == id)
            .map(|p| {
                let (source, target) = ends[p.edge_id.as_str()];
                let peer = index[if p.endpoint == "source" {
                    target
                } else {
                    source
                }];
                let (rank, along) = match p.side {
                    PortSide::Top => (0, p.position.x),
                    PortSide::Right => (1, p.position.y),
                    PortSide::Bottom => (2, -p.position.x),
                    PortSide::Left => (3, -p.position.y),
                };
                (rank, along, peer)
            })
            .collect();
        ports.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
        // Parallel edges share a neighbour and sit next to each other; the skeleton keeps one of them.
        let mut seq: Vec<usize> = Vec::new();
        for peer in ports.iter().map(|p| p.2) {
            if seq.last() != Some(&peer) {
                seq.push(peer);
            }
        }
        let reversed: Vec<usize> = drawn.iter().rev().copied().collect();
        let this = if same_cycle(&seq, &drawn) {
            true
        } else if same_cycle(&seq, &reversed) {
            false
        } else {
            return Err(format!(
                "node {id}: port order {seq:?} differs from drawn rotation {drawn:?}"
            ));
        };
        match orientation {
            None => orientation = Some(this),
            Some(o) if o != this => return Err(format!("node {id} is mirrored")),
            Some(_) => {}
        }
    }
    Ok(())
}

/// Whether `a` and `b` are the same cyclic sequence (rotation allowed, reversal not).
fn same_cycle(a: &[usize], b: &[usize]) -> bool {
    a.len() == b.len()
        && (a.is_empty()
            || b.iter()
                .position(|&x| x == a[0])
                .is_some_and(|s| (0..a.len()).all(|i| a[i] == b[(s + i) % b.len()])))
}

#[cfg(test)]
mod flat_tests {
    use super::*;

    /// Lesbar on state-tcp: the published result is recorded in `benchmarks/results/profiles-20261021T151732Z.jsonl`
    /// (hits 0, shared 0, hugging 2, crossings 5, length 4428). The flat candidate is measured with the same metric.
    #[test]
    fn flat_candidate_on_state_tcp_against_the_lesbar_result() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/state-tcp.json")).unwrap(),
        )
        .unwrap();
        let preset: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("presets/lesbar.json")).unwrap(),
        )
        .unwrap();
        let config: ConfigSet = serde_json::from_value(preset["configSet"].clone()).unwrap();
        let mut options = LayoutOptions::default();
        config.apply(&mut options).unwrap();
        let base = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let show = |label: &str, s: &Snapshot| {
            let m = arqlet_scene::metrics::measure(&published_drawing(s)).unwrap();
            eprintln!(
                "FLAT {label}: hits {} shared {} hug {} cross {} bends {} length {:.0} geometry_issues {}",
                m.line_node_hits + m.container_traversals,
                m.shared_line_pairs,
                m.hug_pairs,
                m.crossings + m.shared_endpoint_crossings,
                m.bends,
                m.length,
                s.geometry_issues.len()
            );
        };
        show("initial", &base);
        let candidate = flat_candidate(&base)
            .unwrap()
            .expect("state-tcp is a flat, biconnected graph");
        show("flat", &candidate);
        assert!(
            candidate.geometry_issues.is_empty(),
            "{:?}",
            candidate.geometry_issues
        );
    }

    /// The ports of every node, read clockwise around its box, follow the drawn rotation (one global orientation): the
    /// port order is the embedding's order, as the flat candidate intends. The check is the shared helper the
    /// compaction uses.
    #[test]
    fn angular_ports_follow_the_drawn_rotation() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/state-tcp.json")).unwrap(),
        )
        .unwrap();
        let preset: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("presets/lesbar.json")).unwrap(),
        )
        .unwrap();
        let config: ConfigSet = serde_json::from_value(preset["configSet"].clone()).unwrap();
        let mut options = LayoutOptions::default();
        config.apply(&mut options).unwrap();
        let base = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let candidate = flat_candidate(&base).unwrap().expect("flat");
        let index: BTreeMap<&str, usize> = graph
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let edges: Vec<(usize, usize)> = graph
            .edges
            .iter()
            .filter_map(|e| {
                e.target
                    .as_deref()
                    .map(|t| (index[e.source.as_str()], index[t]))
            })
            .collect();
        let skeleton = arqlet_topology::graph::Graph::from_edges(graph.nodes.len(), &edges)
            .unwrap()
            .0;
        let drawing = arqlet_topology::draw(&skeleton).unwrap();
        let ids: Vec<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
        port_rotation_ok(&candidate, &drawing.positions, &skeleton, &ids)
            .unwrap_or_else(|e| panic!("{e}"));
    }

    /// A hub with twelve leaves on a 20-unit box: its side cannot hold the ports `pitch` apart, so the check reports it;
    /// a hub with two leaves on its normal box fits.
    #[test]
    fn overflowing_port_slots_are_reported() {
        let star = |leaves: usize| {
            let mut nodes = vec![
                serde_json::json!({"id": "hub", "kind": "k", "label": "hub", "parentId": null}),
            ];
            let mut edges = Vec::new();
            for i in 0..leaves {
                nodes.push(serde_json::json!({"id": format!("l{i}"), "kind": "k", "label": "l", "parentId": null}));
                edges.push(serde_json::json!({"id": format!("e{i}"), "kind": "k", "source": "hub", "target": format!("l{i}")}));
            }
            let value = serde_json::json!({"containerDirections": {}, "defaultDirection": "lr", "nodes": nodes, "edges": edges});
            let graph: Graph = serde_json::from_value(value).unwrap();
            let base = compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
            (graph, base)
        };
        let (graph, base) = star(12);
        let small: BTreeMap<String, Rect> = base
            .boxes
            .iter()
            .map(|(id, r)| {
                (
                    id.clone(),
                    Rect {
                        x: r.x,
                        y: r.y,
                        width: 20.0,
                        height: 20.0,
                    },
                )
            })
            .collect();
        let overflow = ports::overflowing_sides(&graph, &small, 0., &BTreeMap::new());
        assert!(
            !overflow.is_empty(),
            "twelve ports on a 20-unit side must overflow"
        );
        let (graph, base) = star(2);
        let roomy = base.boxes.clone();
        assert!(ports::overflowing_sides(&graph, &roomy, 0., &BTreeMap::new()).is_empty());
    }
}

#[cfg(test)]
mod projected_tests {
    use super::*;

    /// Integer grids give many collinear, touching and shared-endpoint cases: the sweep must count them as the
    /// all-pairs reference does.
    #[test]
    fn the_sweep_counts_the_same_crossings_as_the_reference() {
        let mut seed: u64 = 7;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((seed >> 33) % 12) as f64
        };
        let mut total = 0;
        for round in 0..300usize {
            let n = 4 + round % 9;
            let points: Vec<(f64, f64)> = (0..n).map(|_| (next(), next())).collect();
            let center = |id: &str| points[id[1..].parse::<usize>().unwrap()];
            let mut pairs = Vec::new();
            for a in 0..n {
                for b in a + 1..n {
                    if (a * 7 + b * 3 + round) % 3 != 0 {
                        pairs.push((format!("n{a}"), format!("n{b}")));
                    }
                }
            }
            let swept = projected_crossing_count(&pairs, center);
            assert_eq!(swept, count_all_pairs(&pairs, center), "round {round}");
            total += swept;
        }
        assert!(
            total > 0,
            "the cases must contain crossings to mean anything"
        );
    }

    #[test]
    fn touching_and_collinear_lines_are_not_crossings() {
        // A line ending on another line, and two collinear lines on one axis: neither is a proper crossing.
        let points = [
            (0.0, 0.0),
            (10.0, 0.0),
            (5.0, 0.0),
            (5.0, 5.0),
            (20.0, 0.0),
            (30.0, 0.0),
        ];
        let center = |id: &str| points[id[1..].parse::<usize>().unwrap()];
        let pairs = vec![
            ("n0".to_string(), "n1".to_string()),
            ("n2".to_string(), "n3".to_string()),
            ("n4".to_string(), "n5".to_string()),
        ];
        assert_eq!(projected_crossing_count(&pairs, center), 0);
        assert_eq!(count_all_pairs(&pairs, center), 0);
    }
}

#[cfg(test)]
mod tie_tests {
    use super::*;

    /// The key and the comparison quantize the end-to-end length the same way, so values within one hundredth are a
    /// tie for both: neither candidate is published over the other on that objective.
    #[test]
    fn end_to_end_ties_are_the_same_in_the_key_and_the_comparison() {
        let quality = |end_to_end: f64| Quality {
            bends: 0,
            edge_node_hits: 0,
            routed_crossings: 0,
            shared_route_pairs: 0,
            sibling_overlaps: 0,
            projected_crossings: 0,
            near_node_segments: 0,
            route_length: 0.,
            area: 0.,
            movement: 0.,
            end_to_end,
        };
        let objectives = [Objective::EndToEnd];
        let (a, b) = (quality(10.001), quality(10.004));
        assert!(!better_by(&a, &b, &objectives));
        assert!(!better_by(&b, &a, &objectives));
        assert_eq!(
            end_to_end_units(a.end_to_end),
            end_to_end_units(b.end_to_end)
        );
        let (c, d) = (quality(10.00), quality(10.02));
        assert!(better_by(&c, &d, &objectives));
        assert!(end_to_end_units(c.end_to_end) < end_to_end_units(d.end_to_end));
    }

    #[test]
    fn flat_candidate_stopped_during_compaction_returns_no_drawing() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/state-tcp.json")).unwrap(),
        )
        .unwrap();
        let base = compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        let mut search = Default::default();
        assert!(flat_candidate_until(&base, &|| true, &mut search)
            .unwrap()
            .is_none());
        assert!(flat_candidate_until(&base, &|| false, &mut search)
            .unwrap()
            .is_some());
    }

    #[test]
    fn affected_edges_include_incident_and_route_crossing_edges_only() {
        let graph: Graph = serde_json::from_value(serde_json::json!({
            "nodes": [
                {"id": "a", "kind": "symbol", "label": "A", "parentId": null},
                {"id": "b", "kind": "symbol", "label": "B", "parentId": null},
                {"id": "c", "kind": "symbol", "label": "C", "parentId": null},
                {"id": "d", "kind": "symbol", "label": "D", "parentId": null}
            ],
            "edges": [
                {"id": "e1", "kind": "calls", "source": "a", "target": "b"},
                {"id": "e2", "kind": "calls", "source": "b", "target": "c"},
                {"id": "e3", "kind": "calls", "source": "a", "target": "c"},
                {"id": "e4", "kind": "calls", "source": "c", "target": "d"}
            ]
        }))
        .unwrap();
        let rect = |x: f64, y: f64| Rect {
            x,
            y,
            width: 20.,
            height: 20.,
        };
        let old: BTreeMap<String, Rect> = [
            ("a", rect(0., 40.)),
            ("b", rect(100., 40.)),
            ("c", rect(200., 40.)),
            ("d", rect(200., 300.)),
        ]
        .into_iter()
        .map(|(id, r)| (id.to_string(), r))
        .collect();
        let mut new = old.clone();
        new.insert("b".into(), rect(100., 190.));
        // e3 runs straight through b's old box; e4 stays far below it.
        let routes = vec![
            Route::new(
                "e1",
                vec![Point { x: 20., y: 50. }, Point { x: 100., y: 50. }],
            ),
            Route::new(
                "e2",
                vec![Point { x: 120., y: 50. }, Point { x: 200., y: 50. }],
            ),
            Route::new(
                "e3",
                vec![Point { x: 20., y: 50. }, Point { x: 200., y: 50. }],
            ),
            Route::new(
                "e4",
                vec![Point { x: 220., y: 310. }, Point { x: 300., y: 310. }],
            ),
        ];
        let affected = affected_edges(&graph, &routes, &old, &new, &["b"]);
        assert_eq!(
            affected,
            ["e1", "e2", "e3"].into_iter().map(String::from).collect()
        );
    }

    #[test]
    fn channel_local_move_onto_another_node_is_refused_not_published() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/state-tcp.json")).unwrap(),
        )
        .unwrap();
        let options = LayoutOptions {
            routing_engine: RoutingEngine::Channel,
            ..LayoutOptions::default()
        };
        let base = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let leaves: Vec<String> = graph
            .nodes
            .iter()
            .filter(|n| {
                !graph
                    .nodes
                    .iter()
                    .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
            })
            .map(|n| n.id.clone())
            .collect();
        let (mover, target) = (&leaves[0], &leaves[1]);
        // Put the first leaf exactly on the second one: a hard overlap that no route can resolve.
        let mut boxes = base.boxes.clone();
        let target_box = boxes[target].clone();
        boxes.insert(mover.clone(), target_box);
        let mut search = Default::default();
        let port_context = ports::PortContext::new(&base.graph);
        assert!(
            local_channel_candidate(
                &base,
                &boxes,
                &[mover.as_str()],
                &|| false,
                &mut search,
                &port_context
            )
            .is_none(),
            "an overlapping placement is refused, so the caller falls back explicitly"
        );
    }

    #[test]
    fn channel_local_move_keeps_unchanged_routes_and_composes_a_valid_result() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/state-tcp.json")).unwrap(),
        )
        .unwrap();
        let options = LayoutOptions {
            routing_engine: RoutingEngine::Channel,
            ..LayoutOptions::default()
        };
        let base = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        let leaf = graph
            .nodes
            .iter()
            .find(|n| {
                !graph
                    .nodes
                    .iter()
                    .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
            })
            .map(|n| n.id.clone())
            .unwrap();
        let mut boxes = base.boxes.clone();
        boxes.get_mut(&leaf).unwrap().x += 12.;
        let mut search = Default::default();
        let port_context = ports::PortContext::new(&base.graph);
        let candidate = local_channel_candidate(
            &base,
            &boxes,
            &[leaf.as_str()],
            &|| false,
            &mut search,
            &port_context,
        )
        .expect("the local transaction places its routes");
        assert!(candidate.geometry_issues.is_empty());
        let old = absolute_all(&base.graph, &base.boxes);
        let new = absolute_all(&base.graph, &boxes);
        let changed = affected_edges(&base.graph, &base.routes, &old, &new, &[leaf.as_str()]);
        for route in base.routes.iter().filter(|r| !changed.contains(&r.id)) {
            let kept = candidate.routes.iter().find(|r| r.id == route.id).unwrap();
            assert_eq!(
                kept.points, route.points,
                "unchanged route {} keeps its geometry",
                route.id
            );
        }
    }
}

#[cfg(test)]
mod structure_fixture_tests {
    use super::*;

    fn fixture(name: &str) -> Graph {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure").join(name)).unwrap(),
        )
        .unwrap()
    }

    /// The candidate tracks sit beyond an inflated obstacle by the track spacing when a clearance is required, and by the
    /// prototype's 12 otherwise; the prototype value keeps default output unchanged.
    #[test]
    fn the_candidate_track_offset_follows_the_required_clearance() {
        let plain = routing_params(&LayoutOptions {
            required_clearance: None,
            ..LayoutOptions::default()
        });
        assert_eq!(plain.rail_offset, 12.);
        // The default requires a clearance (ADR 0005), so its tracks lie a track spacing beyond the inflated obstacle.
        assert_eq!(
            routing_params(&LayoutOptions::default()).rail_offset,
            LayoutOptions::default().track_spacing
        );
        let required = routing_params(&LayoutOptions {
            required_clearance: Some(9.),
            ..LayoutOptions::default()
        });
        assert_eq!(required.rail_offset, LayoutOptions::default().track_spacing);
    }

    /// The legacy routing-only job honours the stop signal: a stop during its search returns no snapshot, and the work up to
    /// the stop is still counted.
    #[test]
    fn the_legacy_routing_only_job_is_interruptible() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/state-tcp.json")).unwrap(),
        )
        .unwrap();
        let snapshot = compute(
            &graph,
            None,
            &BTreeSet::new(),
            &LayoutOptions {
                layered_construction: false,
                hierarchical_ordering: false,
                ..LayoutOptions::default()
            },
        )
        .unwrap();
        let (complete, full) = reroute_counted(&snapshot, true, &|| false);
        assert!(complete.is_ok());
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let stop = || calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 3;
        let (interrupted, partial) = reroute_counted(&snapshot, true, &stop);
        assert!(interrupted.is_err(), "the stop was not honoured");
        assert!(
            partial.legacy_candidates > 0 && partial.legacy_candidates < full.legacy_candidates
        );
    }

    /// With component packing, the components of every level are consecutive blocks along the cross axis: read from top to
    /// bottom, the component indices of each level never decrease. The layout has no geometry issues.
    #[test]
    fn packed_components_occupy_consecutive_blocks_of_each_level() {
        let graph = fixture("disconnected.json");
        let options = LayoutOptions {
            component_packing: true,
            ..LayoutOptions::default()
        };
        let snapshot = compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        assert!(
            snapshot.geometry_issues.is_empty(),
            "{:?}",
            snapshot.geometry_issues
        );
        // Chains a-b-c and d-e-f are components 0 and 1; the isolated node is component 2.
        let component = |id: &str| match id {
            "a" | "b" | "c" => 0,
            "d" | "e" | "f" => 1,
            "iso" => 2,
            other => panic!("unexpected node {other}"),
        };
        let mut levels: BTreeMap<i64, Vec<(f64, usize)>> = BTreeMap::new();
        for (id, rect) in &snapshot.boxes {
            levels
                .entry(rect.x.round() as i64)
                .or_default()
                .push((rect.y, component(id)));
        }
        for (x, mut row) in levels {
            row.sort_by(|a, b| a.0.total_cmp(&b.0));
            assert!(
                row.windows(2).all(|w| w[0].1 <= w[1].1),
                "level at x={x}: components interleave {row:?}"
            );
        }
    }

    /// Every structure fixture places without geometry issues: no overlapping nodes, no missing boxes, and one route
    /// per edge with the same ids. Components and cross-links are placed, not dropped. The disconnected fixture
    /// shares levels between its chains today (see `structure::analyse_graph`); this test does not assert how they
    /// are arranged, only that the placement is valid.
    #[test]
    fn structure_fixtures_place_without_overlaps() {
        for name in [
            "singleton.json",
            "disconnected.json",
            "forest.json",
            "tree-cross-link.json",
        ] {
            let graph = fixture(name);
            let snapshot =
                compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
            assert!(
                snapshot.geometry_issues.is_empty(),
                "{name}: {:?}",
                snapshot.geometry_issues
            );
            assert_eq!(
                snapshot.boxes.len(),
                graph.nodes.len(),
                "{name}: every node is boxed"
            );
            let mut route_ids: Vec<&str> = snapshot.routes.iter().map(|r| r.id.as_str()).collect();
            let mut edge_ids: Vec<&str> = graph.edges.iter().map(|e| e.id.as_str()).collect();
            route_ids.sort_unstable();
            edge_ids.sort_unstable();
            assert_eq!(route_ids, edge_ids, "{name}: one route per edge");
        }
    }
}

#[cfg(test)]
mod shape_tests {
    use super::*;

    fn graph(shape: &str) -> Graph {
        let shape_field = if shape.is_empty() {
            String::new()
        } else {
            format!(",\"shape\":\"{shape}\"")
        };
        serde_json::from_str(&format!(
            r#"{{"defaultDirection":"lr","containerDirections":{{}},"nodes":[{{"id":"a","kind":"symbol","label":"A","parentId":null{shape_field}}},{{"id":"b","kind":"symbol","label":"B","parentId":null}}],"edges":[{{"id":"ab","kind":"flow","source":"a","target":"b"}}]}}"#
        ))
        .unwrap()
    }

    /// A circle leaf is as large on both axes: the larger of its requested sides. A square is the same. A rectangle, the
    /// default, keeps its size; the serialized default is omitted.
    #[test]
    fn circle_and_square_leaves_are_square_and_rect_leaves_are_unchanged() {
        let options = LayoutOptions::default();
        for (shape, square) in [("circle", true), ("square", true), ("", false)] {
            let g = graph(shape);
            let s = compute(&g, None, &BTreeSet::new(), &options).unwrap();
            let a = &s.boxes["a"];
            if square {
                assert_eq!(a.width, a.height, "{shape}: {a:?}");
            } else {
                assert_ne!(a.width, a.height, "rect keeps its default size: {a:?}");
            }
        }
        let text = serde_json::to_string(&graph("")).unwrap();
        assert!(
            !text.contains("\"shape\""),
            "the default shape is omitted: {text}"
        );
        let text = serde_json::to_string(&graph("circle")).unwrap();
        assert!(text.contains("\"shape\":\"circle\""), "{text}");
    }

    fn circle_pair(shape_b: &str) -> Graph {
        let shape_field = if shape_b.is_empty() {
            String::new()
        } else {
            format!(",\"shape\":\"{shape_b}\"")
        };
        serde_json::from_str(&format!(
            r#"{{"defaultDirection":"lr","containerDirections":{{}},"nodes":[{{"id":"a","kind":"symbol","label":"A","parentId":null,"shape":"circle"}},{{"id":"b","kind":"symbol","label":"B","parentId":null{shape_field}}}],"edges":[{{"id":"ab","kind":"flow","source":"a","target":"b"}}]}}"#
        ))
        .unwrap()
    }

    /// A circle port lies on its outline: its distance from the circle's centre is the radius, and the port faces its
    /// neighbour.
    #[test]
    fn a_circle_port_lies_on_the_outline_facing_its_neighbour() {
        let g = circle_pair("");
        let s = compute(&g, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        let port = s.resolved_ports.iter().find(|p| p.node_id == "a").unwrap();
        let absolute = absolute_all(&g, &s.boxes);
        let r = &absolute["a"];
        let centre = (r.x + r.width / 2., r.y + r.height / 2.);
        let radius = r.width.min(r.height) / 2.;
        let distance = (port.position.x - centre.0).hypot(port.position.y - centre.1);
        assert!(
            (distance - radius).abs() < 1e-6,
            "port at {distance}, radius {radius}"
        );
        let peer = &absolute["b"];
        assert!(peer.x > r.x + r.width / 2.);
        assert_eq!(port.side, PortSide::Right);
    }

    /// Two circles in diagonally neighbouring boxes do not collide, although their boxes overlap; a rectangle in the same
    /// place does collide.
    #[test]
    fn circles_are_checked_by_their_disc_not_their_box() {
        let g: Graph = serde_json::from_str(
            r#"{"nodes":[{"id":"a","kind":"symbol","label":"A","parentId":null,"shape":"circle"},{"id":"b","kind":"symbol","label":"B","parentId":null,"shape":"circle"}],"edges":[]}"#,
        )
        .unwrap();
        let mut boxes = BTreeMap::new();
        boxes.insert(
            "a".to_string(),
            Rect {
                x: 0.,
                y: 0.,
                width: 100.,
                height: 100.,
            },
        );
        boxes.insert(
            "b".to_string(),
            Rect {
                x: 90.,
                y: 90.,
                width: 100.,
                height: 100.,
            },
        );
        let issues =
            arqlet_scene::validate_geometry(&g, &boxes, arqlet_scene::GeometryRules::default());
        assert!(
            issues.iter().all(|i| i.kind != "node_overlap"),
            "{issues:?}"
        );
        let mut rects = g.clone();
        for n in &mut rects.nodes {
            n.shape = arqlet_model::NodeShape::Rect;
        }
        let issues =
            arqlet_scene::validate_geometry(&rects, &boxes, arqlet_scene::GeometryRules::default());
        assert!(issues.iter().any(|i| i.kind == "node_overlap"));
    }
}

#[cfg(test)]
mod routing_declaration_tests {
    use super::*;

    fn pair(routing: &str) -> Graph {
        serde_json::from_str(&format!(
            r#"{{"nodes":[{{"id":"a","kind":"node","label":"A","parentId":null}},{{"id":"b","kind":"node","label":"B","parentId":null}}],"edges":[{{"id":"ab","kind":"flow","source":"a","target":"b"}}],"defaultRouting":"{routing}"}}"#
        ))
        .unwrap()
    }

    /// A declared straight root draws each edge as one segment, with the layout option left at its default.
    #[test]
    fn a_declared_straight_root_draws_straight_segments() {
        let s = compute(
            &pair("straight"),
            None,
            &BTreeSet::new(),
            &LayoutOptions::default(),
        )
        .unwrap();
        assert_eq!(s.routes.len(), 1);
        assert_eq!(s.routes[0].points.len(), 2);
    }

    /// An option that routes differently from an authored requirement is a conflict, named with both sources.
    #[test]
    fn a_conflicting_option_is_refused() {
        let g: Graph = serde_json::from_str(
            r#"{"nodes":[{"id":"g","kind":"group","label":"G","parentId":null},{"id":"a","kind":"node","label":"A","parentId":"g"},{"id":"b","kind":"node","label":"B","parentId":"g"}],"edges":[{"id":"ab","kind":"flow","source":"a","target":"b"}],"containerRouting":{"g":"orthogonal"}}"#,
        )
        .unwrap();
        let options = LayoutOptions {
            edge_routing: Some(EdgeRouting::Straight),
            ..LayoutOptions::default()
        };
        let err = compute(&g, None, &BTreeSet::new(), &options).unwrap_err();
        assert!(err.contains("routing conflict for g"), "{err}");
    }

    /// A container's routing requirement is drawn: its inner edge is a straight segment (two points) in the drawing.
    #[test]
    fn a_container_routing_is_drawn_for_its_inner_edges() {
        let g: Graph = serde_json::from_str(
            r#"{"nodes":[{"id":"g","kind":"group","label":"G","parentId":null},{"id":"a","kind":"node","label":"A","parentId":"g"},{"id":"b","kind":"node","label":"B","parentId":"g"}],"edges":[{"id":"ab","kind":"flow","source":"a","target":"b"}],"containerRouting":{"g":"straight"}}"#,
        )
        .unwrap();
        let laid = compute(&g, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        let route = laid.routes.iter().find(|r| r.id == "ab").unwrap();
        assert_eq!(route.points.len(), 2, "{:?}", route.points);
    }

    /// A declared rounded root keeps the orthogonal routes and marks them for the scene; plain orthogonal does not.
    #[test]
    fn a_rounded_root_marks_its_routes_in_the_scene() {
        let plain = compute(
            &pair("orthogonal"),
            None,
            &BTreeSet::new(),
            &LayoutOptions::default(),
        )
        .unwrap();
        let rounded = compute(
            &pair("roundedOrthogonal"),
            None,
            &BTreeSet::new(),
            &LayoutOptions::default(),
        )
        .unwrap();
        assert_eq!(plain.routes[0].points, rounded.routes[0].points);
        assert!(!arqlet_scene::build_scene(&plain).unwrap().routes[0].rounded);
        assert!(arqlet_scene::build_scene(&rounded).unwrap().routes[0].rounded);
    }

    /// A list that names both orthogonal styles keeps the plain baseline; a list with only the rounded one rounds.
    #[test]
    fn a_list_with_orthogonal_stays_plain() {
        let rounded = |list: &str| {
            let g: Graph = serde_json::from_str(&format!(
                r#"{{"nodes":[{{"id":"a","kind":"node","label":"A","parentId":null}},{{"id":"b","kind":"node","label":"B","parentId":null}}],"edges":[{{"id":"ab","kind":"flow","source":"a","target":"b"}}],"defaultRouting":{list}}}"#
            ))
            .unwrap();
            let s = compute(&g, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
            arqlet_scene::build_scene(&s).unwrap().routes[0].rounded
        };
        assert!(!rounded(r#"["orthogonal","roundedOrthogonal"]"#));
        assert!(rounded(r#"["roundedOrthogonal"]"#));
        assert!(!rounded(r#""auto""#));
    }

    /// A polyline root keeps the routes of the layout and draws them pulled taut in the scene: the ends stay, no segment
    /// enters a node it does not belong to, and the polyline has no more points than the orthogonal route.
    #[test]
    fn a_polyline_root_pulls_the_scene_routes_taut() {
        let graph = |routing: &str| -> Graph {
            serde_json::from_str(&format!(
                r#"{{"nodes":[{{"id":"a","kind":"node","label":"A","parentId":null}},{{"id":"b","kind":"node","label":"B","parentId":null}},{{"id":"c","kind":"node","label":"C","parentId":null}},{{"id":"d","kind":"node","label":"D","parentId":null}}],"edges":[{{"id":"ab","kind":"flow","source":"a","target":"b"}},{{"id":"ac","kind":"flow","source":"a","target":"c"}},{{"id":"bd","kind":"flow","source":"b","target":"d"}},{{"id":"cd","kind":"flow","source":"c","target":"d"}},{{"id":"ad","kind":"flow","source":"a","target":"d"}}],"defaultRouting":"{routing}"}}"#
            ))
            .unwrap()
        };
        // Polyline is not a single-pass style: it pulls the route of the pipeline before single pass taut (ADR 0016).
        let legacy = LayoutOptions {
            single_pass_auto: false,
            single_pass: false,
            abstract_routing: false,
            ..LayoutOptions::default()
        };
        let draw = |routing: &str| {
            let s = compute(&graph(routing), None, &BTreeSet::new(), &legacy).unwrap();
            arqlet_scene::build_scene(&s).unwrap()
        };
        let plain = draw("orthogonal");
        let taut = draw("polyline");
        assert_eq!(plain.routes.len(), taut.routes.len());
        for (a, b) in plain.routes.iter().zip(&taut.routes) {
            assert_eq!(a.points.first(), b.points.first());
            assert_eq!(a.points.last(), b.points.last());
            assert!(b.points.len() <= a.points.len(), "{}", a.id);
            assert!(!b.rounded);
            for w in b.points.windows(2) {
                for n in &taut.nodes {
                    if n.node.id == a.source || n.node.id == a.target {
                        continue;
                    }
                    let r = arqlet_scene::geom::Rect {
                        x0: n.bounds.x,
                        y0: n.bounds.y,
                        x1: n.bounds.x + n.bounds.width,
                        y1: n.bounds.y + n.bounds.height,
                    };
                    assert!(
                        !arqlet_scene::geom::enters(&w[0], &w[1], &r),
                        "{} enters {}",
                        a.id,
                        n.node.id
                    );
                }
            }
        }
        assert!(plain
            .routes
            .iter()
            .zip(&taut.routes)
            .any(|(a, b)| b.points.len() < a.points.len()));
    }

    /// A rounded polyline draws the pulled route, marked rounded, and keeps the bends further from the nodes than a plain
    /// polyline does.
    #[test]
    fn a_rounded_polyline_is_pulled_taut_and_marked_rounded() {
        let draw = |routing: &str| {
            let g: Graph = serde_json::from_str(&format!(
                r#"{{"nodes":[{{"id":"a","kind":"node","label":"A","parentId":null}},{{"id":"b","kind":"node","label":"B","parentId":null}},{{"id":"c","kind":"node","label":"C","parentId":null}}],"edges":[{{"id":"ab","kind":"flow","source":"a","target":"b"}},{{"id":"ac","kind":"flow","source":"a","target":"c"}},{{"id":"bc","kind":"flow","source":"b","target":"c"}}],"defaultRouting":"{routing}"}}"#
            ))
            .unwrap();
            let s = compute(&g, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
            arqlet_scene::build_scene(&s).unwrap()
        };
        let plain = draw("polyline");
        let rounded = draw("roundedPolyline");
        assert!(plain.routes.iter().all(|r| !r.rounded));
        assert!(rounded.routes.iter().all(|r| r.rounded));
        for (a, b) in plain.routes.iter().zip(&rounded.routes) {
            assert_eq!(a.points.first(), b.points.first());
            assert_eq!(a.points.last(), b.points.last());
        }
    }

    /// A self-loop on a circle leaves and returns at two different points of its border, on every routing style.
    #[test]
    fn a_self_loop_on_a_circle_has_two_distinct_ports() {
        for routing in ["orthogonal", "straight"] {
            let g: Graph = serde_json::from_str(&format!(
                r#"{{"nodes":[{{"id":"a","kind":"node","label":"A","parentId":null,"shape":"circle"}},{{"id":"b","kind":"node","label":"B","parentId":null}}],"edges":[{{"id":"ab","kind":"flow","source":"a","target":"b"}},{{"id":"aa","kind":"flow","source":"a","target":"a"}}],"defaultRouting":"{routing}"}}"#
            ))
            .unwrap();
            let s = compute(&g, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
            let port = |endpoint: &str| {
                s.resolved_ports
                    .iter()
                    .find(|p| p.edge_id == "aa" && p.endpoint == endpoint)
                    .map(|p| (p.position.x, p.position.y))
                    .unwrap()
            };
            let (from, to) = (port("source"), port("target"));
            assert!(
                (from.0 - to.0).hypot(from.1 - to.1) > 10.,
                "{routing}: {from:?} {to:?}"
            );
            let route = s.routes.iter().find(|r| r.id == "aa").unwrap();
            assert!(route.points.len() > 2, "{routing}: {:?}", route.points);
        }
    }

    /// A free direction (`auto`) chooses an orientation per area, and the snapshot says which. A declared direction chooses nothing, so
    /// nothing is recorded and the output of such a layout is unchanged.
    #[test]
    fn the_orientation_a_free_direction_chose_is_in_the_snapshot() {
        let graph = |direction: &str| -> Graph {
            serde_json::from_str(&format!(
                r#"{{"nodes":[{{"id":"g","kind":"n","label":"G","parentId":null}},{{"id":"a","kind":"n","label":"A","parentId":"g"}},{{"id":"b","kind":"n","label":"B","parentId":"g"}},{{"id":"c","kind":"n","label":"C","parentId":"g"}}],"edges":[{{"id":"ab","kind":"f","source":"a","target":"b"}},{{"id":"bc","kind":"f","source":"b","target":"c"}}],"defaultDirection":"{direction}"}}"#
            ))
            .unwrap()
        };
        let free = compute(
            &graph("auto"),
            None,
            &BTreeSet::new(),
            &LayoutOptions::default(),
        )
        .unwrap();
        assert!(
            free.area_orientations.contains_key("<root>"),
            "{:?}",
            free.area_orientations
        );
        assert!(
            free.area_orientations.contains_key("g"),
            "{:?}",
            free.area_orientations
        );
        let declared = compute(
            &graph("lr"),
            None,
            &BTreeSet::new(),
            &LayoutOptions::default(),
        )
        .unwrap();
        assert!(declared.area_orientations.is_empty());
        assert!(serde_json::to_value(&declared)
            .unwrap()
            .get("areaOrientations")
            .is_none());
        // An edit that places only one area again keeps the choice of the others.
        let dirty: BTreeSet<String> = ["g".to_string()].into();
        let again = compute(
            &graph("auto"),
            Some(&free),
            &dirty,
            &LayoutOptions::default(),
        )
        .unwrap();
        assert_eq!(again.area_orientations, free.area_orientations);
    }

    /// Loops do not disturb the arrangement of any strategy: the drawing is complete, finite, and every loop is its own route
    /// with bends, also in a tree (where the tree skeleton ignores loops) and in an organic area.
    #[test]
    fn several_loops_are_drawn_in_every_strategy() {
        for strategy in ["none", "organic", "tree", "radial"] {
            let direction = if matches!(strategy, "radial" | "organic") {
                "auto"
            } else {
                "lr"
            };
            let g: Graph = serde_json::from_str(&format!(
                r#"{{"nodes":[{{"id":"a","kind":"n","label":"A","parentId":null}},{{"id":"b","kind":"n","label":"B","parentId":null}},{{"id":"c","kind":"n","label":"C","parentId":null,"shape":"circle"}}],"edges":[{{"id":"ab","kind":"f","source":"a","target":"b"}},{{"id":"ac","kind":"f","source":"a","target":"c"}},{{"id":"a1","kind":"f","source":"a","target":"a"}},{{"id":"a2","kind":"f","source":"a","target":"a"}},{{"id":"b1","kind":"f","source":"b","target":"b"}},{{"id":"c1","kind":"f","source":"c","target":"c"}},{{"id":"c2","kind":"f","source":"c","target":"c"}}],"defaultStrategy":"{strategy}","defaultDirection":"{direction}"}}"#
            ))
            .unwrap();
            let s = compute(&g, None, &BTreeSet::new(), &LayoutOptions::default())
                .unwrap_or_else(|e| panic!("{strategy}: {e}"));
            assert_eq!(s.boxes.len(), 3, "{strategy}");
            assert!(
                s.boxes
                    .values()
                    .all(|b| [b.x, b.y, b.width, b.height].iter().all(|v| v.is_finite())),
                "{strategy}"
            );
            for id in ["a1", "a2", "b1", "c1", "c2"] {
                let route = s
                    .routes
                    .iter()
                    .find(|r| r.id == id)
                    .unwrap_or_else(|| panic!("{strategy}: no route {id}"));
                assert!(route.bundle_of.is_none(), "{strategy}: {id}");
                assert!(
                    route.points.len() > 2,
                    "{strategy}: {id} {:?}",
                    route.points
                );
                assert!(
                    route
                        .points
                        .iter()
                        .all(|p| p.x.is_finite() && p.y.is_finite()),
                    "{strategy}: {id}"
                );
            }
        }
    }

    /// A label the user put somewhere stays there while the place is free; where it is not, the label is placed again and the
    /// issue says so.
    #[test]
    fn a_label_stays_where_the_user_put_it_while_the_place_is_free() {
        let g: Graph = serde_json::from_str(
            r#"{"nodes":[{"id":"a","kind":"n","label":"A","parentId":null},{"id":"b","kind":"n","label":"B","parentId":null}],"edges":[{"id":"ab","kind":"f","source":"a","target":"b"}]}"#,
        )
        .unwrap();
        let mut options = LayoutOptions::default();
        options.edge_labels.insert(
            "ab".into(),
            LabelSpec {
                text: "calls".into(),
                width: 60.,
                height: 18.,
            },
        );
        let base = compute(&g, None, &BTreeSet::new(), &options).unwrap();
        let automatic = base
            .labels
            .iter()
            .find(|l| l.id == "ab")
            .unwrap()
            .bounds
            .clone();
        // A free place far from everything.
        options
            .label_positions
            .insert("ab".into(), Point { x: 900., y: 700. });
        let moved = compute(&g, None, &BTreeSet::new(), &options).unwrap();
        let at = &moved.labels.iter().find(|l| l.id == "ab").unwrap().bounds;
        assert_eq!((at.x, at.y), (900., 700.));
        assert!(!moved
            .routing_issues
            .iter()
            .any(|i| i.contains("does not fit")));
        // On top of a node the place is not free: the label is placed again, as without a position, and the issue says so.
        let node = &base.boxes["a"];
        options.label_positions.insert(
            "ab".into(),
            Point {
                x: node.x + 5.,
                y: node.y + 5.,
            },
        );
        let blocked = compute(&g, None, &BTreeSet::new(), &options).unwrap();
        let again = &blocked.labels.iter().find(|l| l.id == "ab").unwrap().bounds;
        assert_eq!((again.x, again.y), (automatic.x, automatic.y));
        assert!(
            blocked
                .routing_issues
                .iter()
                .any(|i| i.contains("label ab does not fit")),
            "{:?}",
            blocked.routing_issues
        );
    }

    /// Complete bundling merges parallel edges, but never the loops of one node.
    #[test]
    fn loops_of_one_node_are_not_bundled() {
        let g: Graph = serde_json::from_str(
            r#"{"nodes":[{"id":"a","kind":"node","label":"A","parentId":null},{"id":"b","kind":"node","label":"B","parentId":null}],"edges":[{"id":"p1","kind":"f","source":"a","target":"b"},{"id":"p2","kind":"f","source":"a","target":"b"},{"id":"l1","kind":"f","source":"a","target":"a"},{"id":"l2","kind":"f","source":"a","target":"a"}]}"#,
        )
        .unwrap();
        let route = |id: &str| Route::new(id, vec![Point { x: 0., y: 0. }, Point { x: 1., y: 1. }]);
        let routes = bundle_complete(&g, vec![route("p1"), route("p2"), route("l1"), route("l2")]);
        let leader_of = |id: &str| {
            routes
                .iter()
                .find(|r| r.id == id)
                .unwrap()
                .bundle_of
                .clone()
        };
        assert_eq!(leader_of("p2").as_deref(), Some("p1"));
        assert_eq!((leader_of("l1"), leader_of("l2")), (None, None));
    }

    /// The metrics measure the drawn geometry: with a polyline style the bends are those of the pulled route, and the
    /// default configuration measures the routes as they are.
    #[test]
    fn the_published_drawing_is_what_the_scene_draws() {
        let build = |routing: Option<&str>| {
            let style = routing.map_or(String::new(), |r| format!(r#","defaultRouting":"{r}""#));
            let g: Graph = serde_json::from_str(&format!(
                r#"{{"nodes":[{{"id":"a","kind":"node","label":"A","parentId":null}},{{"id":"b","kind":"node","label":"B","parentId":null}},{{"id":"c","kind":"node","label":"C","parentId":null}}],"edges":[{{"id":"ab","kind":"flow","source":"a","target":"b"}},{{"id":"ac","kind":"flow","source":"a","target":"c"}},{{"id":"bc","kind":"flow","source":"b","target":"c"}}]{style}}}"#
            ))
            .unwrap();
            compute(&g, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap()
        };
        let points_of = |s: &Snapshot| -> Vec<(String, Vec<(f64, f64)>)> {
            published_drawing(s)
                .edges
                .iter()
                .map(|e| (e.id.clone(), e.points.iter().map(|p| (p.x, p.y)).collect()))
                .collect()
        };
        let routes_of = |s: &Snapshot| -> Vec<(String, Vec<(f64, f64)>)> {
            s.routes
                .iter()
                .map(|r| (r.id.clone(), r.points.iter().map(|p| (p.x, p.y)).collect()))
                .collect()
        };
        let corners_of = |s: &Snapshot| -> Vec<(String, Vec<(f64, f64)>)> {
            arqlet_scene::build_corner_scene(s)
                .unwrap()
                .routes
                .iter()
                .map(|r| (r.id.clone(), r.points.iter().map(|p| (p.x, p.y)).collect()))
                .collect()
        };
        // The default measures the routes as the router produced them.
        let plain = build(None);
        assert_eq!(points_of(&plain), routes_of(&plain));
        // A polyline style measures the pulled route, and a rounded one its corners, not the samples of its arcs.
        let taut = build(Some("polyline"));
        assert_eq!(points_of(&taut), corners_of(&taut));
        let rounded = build(Some("roundedPolyline"));
        assert_eq!(points_of(&rounded), corners_of(&rounded));
    }

    /// The polyline is drawn only when no plainer non-straight style is listed with it.
    #[test]
    fn a_list_with_a_plainer_style_is_not_drawn_as_polyline() {
        use arqlet_model::RoutingStyle::*;
        let list = |styles: &[RoutingStyle]| RoutingConstraint::Allowed(styles.to_vec());
        assert!(list(&[Polyline]).picks(Polyline));
        assert!(!list(&[Polyline, Orthogonal]).picks(Polyline));
        assert!(!list(&[Polyline, RoundedOrthogonal]).picks(Polyline));
        assert!(list(&[RoundedPolyline]).picks(RoundedPolyline));
        assert!(list(&[RoundedPolyline]).rounds());
        assert!(!list(&[RoundedPolyline, Polyline]).rounds());
        assert!(list(&[Polyline, Straight]).picks(Polyline));
        assert!(!RoutingConstraint::Auto.picks(Polyline));
    }

    /// The crossing markers of a pulled route lie on the border of their container and on the route that is drawn, not
    /// on the orthogonal route they were first found on.
    #[test]
    fn polyline_crossings_lie_on_the_drawn_route() {
        let g: Graph = serde_json::from_str(
            r#"{"nodes":[{"id":"g1","kind":"group","label":"G1","parentId":null},{"id":"g2","kind":"group","label":"G2","parentId":null},{"id":"a","kind":"node","label":"A","parentId":"g1"},{"id":"a2","kind":"node","label":"A2","parentId":"g1"},{"id":"b","kind":"node","label":"B","parentId":"g2"},{"id":"b2","kind":"node","label":"B2","parentId":"g2"}],"edges":[{"id":"ab","kind":"flow","source":"a","target":"b"},{"id":"a2b2","kind":"flow","source":"a2","target":"b2"},{"id":"ab2","kind":"flow","source":"a","target":"b2"},{"id":"a2b","kind":"flow","source":"a2","target":"b"}],"defaultRouting":"polyline"}"#,
        )
        .unwrap();
        let s = compute(&g, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        let scene = arqlet_scene::build_scene(&s).unwrap();
        assert!(!scene.crossings.is_empty());
        for c in &scene.crossings {
            let container = scene
                .nodes
                .iter()
                .find(|n| n.node.id == c.container_id)
                .unwrap();
            let b = &container.bounds;
            let on_border = ((c.position.x - b.x).abs() < 1e-6
                || (c.position.x - (b.x + b.width)).abs() < 1e-6
                || (c.position.y - b.y).abs() < 1e-6
                || (c.position.y - (b.y + b.height)).abs() < 1e-6)
                && c.position.x >= b.x - 1e-6
                && c.position.x <= b.x + b.width + 1e-6
                && c.position.y >= b.y - 1e-6
                && c.position.y <= b.y + b.height + 1e-6;
            assert!(
                on_border,
                "{c:?} is not on the border of {}",
                c.container_id
            );
            let route = scene.routes.iter().find(|r| r.id == c.edge_id).unwrap();
            let on_route = route.points.windows(2).any(|w| {
                let cross = (w[1].x - w[0].x) * (c.position.y - w[0].y)
                    - (w[1].y - w[0].y) * (c.position.x - w[0].x);
                let len = (w[1].x - w[0].x).hypot(w[1].y - w[0].y).max(1.);
                cross.abs() / len < 1e-6
                    && c.position.x >= w[0].x.min(w[1].x) - 1e-6
                    && c.position.x <= w[0].x.max(w[1].x) + 1e-6
                    && c.position.y >= w[0].y.min(w[1].y) - 1e-6
                    && c.position.y <= w[0].y.max(w[1].y) + 1e-6
            });
            assert!(on_route, "{c:?} is not on the drawn route {}", c.edge_id);
        }
    }

    /// A style the layout does not draw is refused with its name.
    #[test]
    fn an_undrawn_style_is_refused() {
        let err = compute(
            &pair("curve"),
            None,
            &BTreeSet::new(),
            &LayoutOptions::default(),
        )
        .unwrap_err();
        assert!(err.contains("curve is not drawn yet"), "{err}");
    }
}

#[cfg(test)]
mod routing_style_tests {
    use super::*;

    /// Root straight, one container orthogonal: the edges inside the container are orthogonal, the outer edge is a
    /// straight segment. The drawing mixes the styles and is not refused.
    #[test]
    fn a_drawing_mixes_straight_and_orthogonal_edges_by_area() {
        let g: Graph = serde_json::from_str(
            r#"{"nodes":[{"id":"g","kind":"group","label":"G","parentId":null},{"id":"a","kind":"node","label":"A","parentId":"g"},{"id":"b","kind":"node","label":"B","parentId":"g"},{"id":"c","kind":"node","label":"C","parentId":null}],"edges":[{"id":"ab","kind":"flow","source":"a","target":"b"},{"id":"ac","kind":"flow","source":"a","target":"c"}],"defaultRouting":"straight","containerRouting":{"g":"orthogonal"}}"#,
        )
        .unwrap();
        let s = compute(&g, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        assert_eq!(s.straight_edges, vec!["ac".to_string()]);
        let ac = s.routes.iter().find(|r| r.id == "ac").unwrap();
        assert_eq!(ac.points.len(), 2);
        let ab = s.routes.iter().find(|r| r.id == "ab").unwrap();
        assert!(
            ab.points
                .windows(2)
                .all(|w| w[0].x == w[1].x || w[0].y == w[1].y),
            "orthogonal segments only"
        );
    }

    /// The list order is no preference: the same allowed set gives the same drawing in either order.
    #[test]
    fn allowed_list_order_does_not_change_the_drawing() {
        let make = |list: &str| -> Graph {
            serde_json::from_str(&format!(
                r#"{{"nodes":[{{"id":"a","kind":"node","label":"A","parentId":null}},{{"id":"b","kind":"node","label":"B","parentId":null}},{{"id":"c","kind":"node","label":"C","parentId":null}}],"edges":[{{"id":"ab","kind":"flow","source":"a","target":"b"}},{{"id":"bc","kind":"flow","source":"b","target":"c"}}],"defaultRouting":{list}}}"#
            ))
            .unwrap()
        };
        let one = compute(
            &make(r#"["straight","orthogonal"]"#),
            None,
            &BTreeSet::new(),
            &LayoutOptions::default(),
        )
        .unwrap();
        let two = compute(
            &make(r#"["orthogonal","straight"]"#),
            None,
            &BTreeSet::new(),
            &LayoutOptions::default(),
        )
        .unwrap();
        assert_eq!(one.straight_edges, two.straight_edges);
        let points = |s: &Snapshot| {
            s.routes
                .iter()
                .map(|r| r.points.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(points(&one), points(&two));
    }

    /// An explicit root `orthogonal` is a binding requirement: a container's legacy straight option conflicts with it by name.
    #[test]
    fn the_legacy_option_conflicts_with_an_authored_orthogonal_root() {
        let g: Graph = serde_json::from_str(
            r#"{"nodes":[{"id":"a","kind":"node","label":"A","parentId":null},{"id":"b","kind":"node","label":"B","parentId":null}],"edges":[{"id":"ab","kind":"flow","source":"a","target":"b"}],"defaultRouting":"orthogonal"}"#,
        )
        .unwrap();
        let options = LayoutOptions {
            edge_routing: Some(EdgeRouting::Straight),
            ..LayoutOptions::default()
        };
        let err = compute(&g, None, &BTreeSet::new(), &options).unwrap_err();
        assert!(err.contains("routing conflict for the root"), "{err}");
    }

    /// The legacy option applies where nothing is authored: the root edge takes it, and the container's own entry is kept.
    #[test]
    fn the_legacy_option_applies_only_where_nothing_is_authored() {
        let g: Graph = serde_json::from_str(
            r#"{"nodes":[{"id":"g","kind":"group","label":"G","parentId":null},{"id":"a","kind":"node","label":"A","parentId":"g"},{"id":"b","kind":"node","label":"B","parentId":"g"},{"id":"c","kind":"node","label":"C","parentId":null},{"id":"d","kind":"node","label":"D","parentId":null}],"edges":[{"id":"ab","kind":"flow","source":"a","target":"b"},{"id":"cd","kind":"flow","source":"c","target":"d"}],"containerRouting":{"g":"straight"}}"#,
        )
        .unwrap();
        // The option agrees with the container's entry, so no conflict; it decides the root's edge.
        let options = LayoutOptions {
            edge_routing: Some(EdgeRouting::Straight),
            ..LayoutOptions::default()
        };
        let s = compute(&g, None, &BTreeSet::new(), &options).unwrap();
        assert_eq!(s.straight_edges, vec!["ab".to_string(), "cd".to_string()]);
        let plain = compute(&g, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
        assert_eq!(
            plain.straight_edges,
            vec!["ab".to_string()],
            "without the option the root edge is orthogonal"
        );
    }
}

#[cfg(test)]
mod auto_choice_tests {
    use super::*;

    /// The fixture with its routing replaced: `routing` is a style name, or a JSON value written as text.
    fn with_routing(json: &str, root: &str, container: Option<(&str, &str)>) -> Graph {
        let mut v: serde_json::Value = serde_json::from_str(json).unwrap();
        v["defaultRouting"] = serde_json::from_str(root).unwrap();
        if let Some((id, value)) = container {
            v["containerRouting"] = serde_json::json!({ id: serde_json::from_str::<serde_json::Value>(value).unwrap() });
        }
        serde_json::from_value(v).unwrap()
    }

    fn drawn(json: &str, root: &str, container: Option<(&str, &str)>) -> Snapshot {
        compute(
            &with_routing(json, root, container),
            None,
            &BTreeSet::new(),
            &LayoutOptions {
                layered_construction: false,
                hierarchical_ordering: false,
                ..LayoutOptions::default()
            },
        )
        .unwrap()
    }

    /// Where a straight segment scores better than the orthogonal route it replaces (no bends), `auto` draws it straight,
    /// and its conflict key equals the one of drawing the whole graph straight.
    #[test]
    fn auto_draws_straight_when_it_scores_better() {
        let json = include_str!("../../../examples/fixtures/routing/auto-straight-wins.json");
        let auto = drawn(json, "\"auto\"", None);
        assert!(
            auto.straight_edges.contains(&"ab".to_string()),
            "{:?}",
            auto.straight_edges
        );
        let straight = drawn(json, "\"straight\"", None);
        assert_eq!(published_key(&auto), published_key(&straight));
    }

    /// Where a straight segment would run through a node, `auto` keeps the orthogonal route, and the drawing has no routing
    /// issue.
    #[test]
    fn auto_keeps_orthogonal_when_straight_would_cross_a_node() {
        let json = include_str!("../../../examples/fixtures/routing/auto-orthogonal-wins.json");
        let auto = drawn(json, "\"auto\"", None);
        assert!(
            !auto.straight_edges.contains(&"ab".to_string()),
            "{:?}",
            auto.straight_edges
        );
        assert!(auto.routing_issues.is_empty(), "{:?}", auto.routing_issues);
    }

    /// The refinement keeps the straight edges on their ports and their style, and the orthogonal edges orthogonal.
    #[test]
    fn refinement_keeps_straight_edges_on_their_ports() {
        let json = include_str!("../../../examples/fixtures/routing/auto-two-areas.json");
        let before = drawn(json, "\"auto\"", Some(("g", "\"auto\"")));
        assert!(!before.straight_edges.is_empty());
        let after = reroute(&before);
        assert_eq!(after.straight_edges, before.straight_edges);
        for id in &after.straight_edges {
            let route = after.routes.iter().find(|r| &r.id == id).unwrap();
            let end = |endpoint: &str| {
                after
                    .resolved_ports
                    .iter()
                    .find(|p| &p.edge_id == id && p.endpoint == endpoint)
                    .unwrap()
                    .position
                    .clone()
            };
            assert_eq!(
                route.points,
                vec![end("source"), end("target")],
                "straight edge {id}"
            );
        }
        for route in after
            .routes
            .iter()
            .filter(|r| !after.straight_edges.contains(&r.id))
        {
            assert!(
                route
                    .points
                    .windows(2)
                    .all(|w| w[0].x == w[1].x || w[0].y == w[1].y),
                "orthogonal {}",
                route.id
            );
        }
    }

    /// Two areas, one choice each. The automatic result is the best of the fixed combinations of the root and the container
    /// (by the conflict key), and it draws no routing issue. The choice is per area in sequence, not a global search.
    #[test]
    fn auto_matches_the_best_combination_of_two_areas() {
        let json = include_str!("../../../examples/fixtures/routing/auto-two-areas.json");
        let auto = drawn(json, "\"auto\"", Some(("g", "\"auto\"")));
        assert!(auto.routing_issues.is_empty(), "{:?}", auto.routing_issues);
        let auto_key = published_key(&auto).unwrap();
        let mut best: Option<Vec<i64>> = None;
        for root in ["\"straight\"", "\"orthogonal\""] {
            for inner in ["\"straight\"", "\"orthogonal\"", "\"auto\""] {
                let key = published_key(&drawn(json, root, Some(("g", inner)))).unwrap();
                if best.as_ref().is_none_or(|b| key < *b) {
                    best = Some(key);
                }
            }
        }
        assert_eq!(Some(auto_key), best);
    }
}
