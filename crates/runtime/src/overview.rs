//! Provisional: the `overview` operation (ADR 0018). Lays out a whole graph as discs in its containment hierarchy
//! (`arqlet_layout::overview`), running the containers of one level in parallel on native builds. The result is the same
//! for any thread count.
use arqlet_layout::overview::{
    self, Arrangement, Budget, Container, Fallback, Meter, OverviewHints, OverviewOptions, Previous,
};
use arqlet_model::{
    validate, ConfigSet, Graph, OverBudget, Overview, OverviewConfig, OverviewContinuation,
    OverviewIntent, OverviewPlace, OverviewPositions, OverviewRepresentation, OverviewStats,
    OverviewWork,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

/// The largest graph the operation accepts (nodes). A guard against unbounded requests, not a measured limit.
pub const MAX_OVERVIEW_NODES: usize = 1_000_000;

fn options_of(v: &Value) -> Result<OverviewOptions, String> {
    let mut options = OverviewOptions::default();
    let Some(o) = v.get("options") else {
        return Ok(options);
    };
    let o = o.as_object().ok_or("Overview options must be an object")?;
    for (key, value) in o {
        match key.as_str() {
            "nodeRadius" | "gap" | "padding" | "charge" | "gravity" => {
                let x = value
                    .as_f64()
                    .filter(|x| x.is_finite() && *x >= 0.)
                    .ok_or_else(|| format!("Overview option {key} must be a number >= 0"))?;
                match key.as_str() {
                    "nodeRadius" => options.node_radius = x.max(0.01),
                    "gap" => options.gap = x,
                    "charge" => options.charge = x,
                    "gravity" => options.gravity = x,
                    _ => options.padding = x,
                }
            }
            // Groups the data gives and hubs are read by `hierarchy_of`, which builds the clusters with them.
            "groups" | "hubs" => {}
            "chains" => {
                options.chains = value
                    .as_bool()
                    .ok_or("Overview option chains must be true or false")?;
            }
            // Provisional: the members of every bus line in a row (`OverviewOptions::rows`, filled with the graph).
            "busRows" => {
                if value
                    .as_bool()
                    .ok_or("Overview option busRows must be true or false")?
                {
                    options.rows.insert(usize::MAX, Vec::new());
                }
            }
            "satellites" => {
                options.satellites = value.as_f64().filter(|x| (0. ..=1.).contains(x)).ok_or(
                    "Overview option satellites must be a share between 0 and 1 (0: never)",
                )?;
            }
            "communities" => {
                options.communities = value
                    .as_u64()
                    .ok_or("Overview option communities must be a number of children (0: never)")?
                    as usize;
            }
            "ticks" => {
                options.ticks = value
                    .as_u64()
                    .filter(|t| (1..=10_000).contains(t))
                    .ok_or("Overview option ticks must be between 1 and 10000")?
                    as usize;
            }
            _ => return Err(format!("Unknown overview option: {key}")),
        }
    }
    Ok(options)
}

/// Provisional (#211): the work budget of a request (`workBudget`) and what to do when it runs out (`overBudget`).
#[derive(Clone, Copy, Debug)]
struct WorkRequest {
    limit: u64,
    refuse: bool,
}

fn work_of(v: &Value) -> Result<Option<WorkRequest>, String> {
    let limit = match v.get("workBudget") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            value
                .as_u64()
                .ok_or("Overview workBudget must be a non-negative integer")?,
        ),
    };
    let over: Option<OverBudget> = match v.get("overBudget") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            serde_json::from_value(value.clone())
                .map_err(|_| "Overview overBudget must be answer or refuse")?,
        ),
    };
    match (limit, over) {
        (None, Some(_)) => Err("Overview overBudget needs a workBudget".into()),
        (None, None) => Ok(None),
        (Some(limit), over) => Ok(Some(WorkRequest {
            limit,
            refuse: over == Some(OverBudget::Refuse),
        })),
    }
}

/// The budget of a request: unlimited without `workBudget` (the work is counted all the same).
fn budget_of(work: Option<WorkRequest>) -> Budget {
    work.map_or_else(Budget::unlimited, |w| Budget::new(w.limit))
}

/// A container as the answer names it: its node index, -1 for the top level.
fn container_index(container: Container) -> i64 {
    match container {
        Container::Root => -1,
        Container::Node(i) => i as i64,
    }
}

/// The refusal of a request with `overBudget: refuse` whose budget ran out (a stable prefix for hosts).
fn refusal(h: &overview::Hierarchy, budget: &Budget) -> String {
    let (phase, container) = budget.stopped.unwrap_or(("arrangement", Container::Root));
    let name = match container {
        Container::Root => "the top level".to_string(),
        Container::Node(i) => h.ids.get(i).cloned().unwrap_or_default(),
    };
    format!(
        "Overview work budget exhausted: the {phase} of {name} did not fit into the {} units left of {} ({} used)",
        budget.limit.saturating_sub(budget.used),
        budget.limit,
        budget.used
    )
}

/// The meter of one container arranged in parallel (#211): it stops at its own limit (the level's remaining budget)
/// and once the work of the whole level together exceeds that limit. `own` tells the first from the second.
#[cfg(not(target_arch = "wasm32"))]
struct Shared<'a> {
    used: u64,
    limit: u64,
    total: &'a std::sync::atomic::AtomicU64,
    own: bool,
}

#[cfg(not(target_arch = "wasm32"))]
impl Meter for Shared<'_> {
    fn spend(&mut self, units: u64) -> bool {
        self.used = self.used.saturating_add(units);
        if self.used > self.limit {
            self.own = true;
            return false;
        }
        let total = self
            .total
            .fetch_add(units, std::sync::atomic::Ordering::Relaxed)
            .saturating_add(units);
        total <= self.limit
    }
}

/// Arranges one level within the budget (#211, ADR 0018 addendum): the containers in index order, each with what is
/// left, until the first that does not fit; it and all after it get `None` (the caller's fallback). With threads the
/// containers run in parallel first (`Shared`), then are taken in index order; one stopped only by the level's shared
/// counter while all before it fit is arranged again with what is left. The result depends only on each container's
/// own work, so it is the same for any number of threads.
fn arrange_level(
    level: &[Container],
    threads: usize,
    budget: &mut Budget,
    arrange: &(dyn Fn(Container, &mut dyn Meter) -> Option<Arrangement> + Sync),
) -> Vec<Option<Arrangement>> {
    #[cfg(not(target_arch = "wasm32"))]
    if threads > 1 && level.len() > 1 && !budget.exhausted {
        let limit = budget.remaining();
        let total = std::sync::atomic::AtomicU64::new(0);
        let tried = crate::parallel_map(level, threads, |&container| {
            let mut meter = Shared {
                used: 0,
                limit,
                total: &total,
                own: false,
            };
            let done = arrange(container, &mut meter);
            (done, meter.used, meter.own)
        });
        let mut out = Vec::with_capacity(level.len());
        for (&container, (done, used, own)) in level.iter().zip(tried) {
            let done = match done {
                _ if budget.exhausted => None,
                Some(a) if used <= budget.remaining() => {
                    budget.accept("arrangement", used);
                    Some(a)
                }
                Some(_) => {
                    budget.stop("arrangement", container);
                    None
                }
                // Over the whole level's limit on its own: it does not fit into what is left either.
                None if own => {
                    budget.stop("arrangement", container);
                    None
                }
                None => budget.piece("arrangement", container, |meter| arrange(container, meter)),
            };
            out.push(done);
        }
        return out;
    }
    let _ = threads;
    level
        .iter()
        .map(|&container| budget.piece("arrangement", container, |meter| arrange(container, meter)))
        .collect()
}

/// The places of a request with `start.nodes`, and the clusters of the hierarchy that were kept from them.
type FromPlaces = (BTreeMap<String, OverviewPlace>, BTreeSet<usize>);

/// The hints of a request: `radii` (node id → radius, for clusters a host shows structured) and `start` to continue
/// from: `{x, y, r}` of a previous overview of the same graph, in its node order, or `{nodes}`, its places by node id,
/// of this graph or an earlier one (read by `hierarchy_of`). With the latter also what became of the places.
fn hints_of(
    v: &Value,
    h: &overview::Hierarchy,
    from_places: Option<&FromPlaces>,
) -> Result<(OverviewHints, Option<OverviewContinuation>), String> {
    let mut hints = OverviewHints::default();
    let mut continuation = None;
    let index: BTreeMap<&str, usize> = if v.get("radii").is_some() || v.get("start").is_some() {
        h.ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect()
    } else {
        BTreeMap::new()
    };
    if let Some(radii) = v.get("radii") {
        for (id, r) in radii
            .as_object()
            .ok_or("Overview radii must be an object")?
        {
            let i = *index
                .get(id.as_str())
                .ok_or_else(|| format!("Overview radius for an unknown node: {id}"))?;
            let r = r
                .as_f64()
                .filter(|r| r.is_finite() && *r > 0.)
                .ok_or_else(|| format!("Overview radius of {id} must be a number > 0"))?;
            hints.radius.insert(i, r);
        }
    }
    if let Some(start) = v.get("start") {
        let mut previous = match from_places {
            Some((places, kept)) => {
                let (previous, report) = overview::continue_by_id(h, places, kept);
                continuation = Some(report);
                previous
            }
            None => {
                let numbers = |key: &str| -> Result<Vec<f64>, String> {
                    let list = start[key]
                        .as_array()
                        .ok_or_else(|| format!("Overview start needs {key}"))?;
                    if list.len() != h.ids.len() {
                        return Err(format!(
                            "Overview start {key} has {} values for {} nodes; it must come from the same graph",
                            list.len(),
                            h.ids.len()
                        ));
                    }
                    list.iter()
                        .map(|x| {
                            x.as_f64().filter(|x| x.is_finite()).ok_or_else(|| {
                                format!("Overview start {key} must be finite numbers")
                            })
                        })
                        .collect()
                };
                let (x, y, r) = (numbers("x")?, numbers("y")?, numbers("r")?);
                Previous {
                    centre: x.into_iter().zip(y).collect(),
                    radius: r,
                    ..Previous::default()
                }
            }
        };
        // `settle: true` lets the changed containers settle again (for a host that animates the change).
        previous.settle = start["settle"].as_bool().unwrap_or(false);
        if let Some(moved) = start.get("moved") {
            previous.moved = moved
                .as_object()
                .ok_or("Overview start moved must be an object: node id -> {x, y}")?
                .iter()
                .map(|(id, p)| {
                    let i = *index
                        .get(id.as_str())
                        .ok_or_else(|| format!("Overview start moved an unknown node: {id}"))?;
                    if previous.anew.contains(&i) {
                        return Err(format!(
                            "Overview start moved {id}, which has no earlier place in its cluster"
                        ));
                    }
                    let at = |k: &str| {
                        p[k].as_f64().filter(|v| v.is_finite()).ok_or_else(|| {
                            format!("Overview start moved {id} needs finite x and y")
                        })
                    };
                    Ok((i, (at("x")?, at("y")?)))
                })
                .collect::<Result<_, String>>()?;
        }
        hints.start = Some(previous);
    }
    Ok((hints, continuation))
}

/// `start.nodes` of a request, when it has them: node id → `{x, y, r, parent?}`.
fn places_of(v: &Value) -> Result<Option<BTreeMap<String, OverviewPlace>>, String> {
    let Some(start) = v.get("start") else {
        return Ok(None);
    };
    let Some(nodes) = start.get("nodes") else {
        return Ok(None);
    };
    if ["x", "y", "r"].iter().any(|k| start.get(k).is_some()) {
        return Err(
            "Overview start takes x, y and r (by node index) or nodes (by node id), not both"
                .into(),
        );
    }
    nodes
        .as_object()
        .ok_or("Overview start nodes must be an object: node id -> {x, y, r, parent?}")?
        .iter()
        .map(|(id, p)| {
            let at = |k: &str| p[k].as_f64().filter(|v| v.is_finite());
            let parent = match p.get("parent") {
                None | Some(Value::Null) => None,
                Some(Value::String(parent)) => Some(parent.clone()),
                Some(_) => return Err(format!("Overview start nodes {id} parent must be an id")),
            };
            match (at("x"), at("y"), at("r").filter(|r| *r >= 0.)) {
                (Some(x), Some(y), Some(r)) => Ok((id.clone(), OverviewPlace { x, y, r, parent })),
                _ => Err(format!(
                    "Overview start nodes {id} needs finite x, y and r >= 0"
                )),
            }
        })
        .collect::<Result<_, String>>()
        .map(Some)
}

/// The representation values of a request (ADR 0018, addendum; ADR 0015): `intent` (the user's decisions by node id)
/// and the `overview` part of `configSet` (rules by node kind, and whether the zoom decides the rest). A decision for a
/// node the graph does not have is refused; a rule for a kind the graph does not have is reported, not refused (a config
/// set serves many graphs).
fn representation_of(
    v: &Value,
    h: &overview::Hierarchy,
    kinds: &[&str],
) -> Result<OverviewRepresentation, String> {
    let intent: OverviewIntent = match v.get("intent") {
        Some(i) => {
            serde_json::from_value(i.clone()).map_err(|e| format!("Overview intent: {e}"))?
        }
        None => OverviewIntent::default(),
    };
    let config: OverviewConfig = match v.get("configSet") {
        Some(c) => {
            let set: ConfigSet =
                serde_json::from_value(c.clone()).map_err(|e| format!("Config set: {e}"))?;
            set.overview.unwrap_or_default()
        }
        None => OverviewConfig::default(),
    };
    let known: std::collections::BTreeSet<&str> = h.ids.iter().map(String::as_str).collect();
    if let Some(id) = intent
        .decisions
        .keys()
        .find(|id| !known.contains(id.as_str()))
    {
        return Err(format!("Overview decision for an unknown node: {id}"));
    }
    let unknown_kinds = config
        .rules
        .keys()
        .filter(|k| !kinds.contains(&k.as_str()))
        .cloned()
        .collect();
    Ok(OverviewRepresentation {
        decisions: intent.decisions,
        rules: config.rules,
        auto: config.auto.unwrap_or(true),
        unknown_kinds,
        // The level of detail, when the config set gives one.
        level_of_detail: config.level_of_detail,
    })
}

/// Rounds a coordinate to hundredths, which keeps the response small; the layout itself is computed in full precision.
fn round(x: f64) -> f64 {
    (x * 100.).round() / 100.
}

/// The overview of a graph: `{ok, overview}` with the nodes as parallel arrays (index = input order).
pub fn overview_response(v: &Value, threads: usize) -> Value {
    match overview_of(v, threads) {
        Ok(overview) => json!({"ok": true, "overview": overview}),
        Err(e) => json!({"ok": false, "error": e}),
    }
}

pub(crate) fn overview_of(v: &Value, threads: usize) -> Result<Overview, String> {
    let graph = graph_of(v)?;
    let options = options_of(v)?;
    let work = work_of(v)?;
    let mut budget = budget_of(work);
    let (graph, h, from_places) = hierarchy_of(&graph, v, &options, &mut budget)?;
    lay_out(
        &graph,
        &h,
        from_places.as_ref(),
        v,
        &options,
        threads,
        work.map(|w| (w, budget)),
    )
}

/// The graph of a request, checked.
pub(crate) fn graph_of(v: &Value) -> Result<Graph, String> {
    let graph: Graph = serde_json::from_value(v["graph"].clone()).map_err(|e| e.to_string())?;
    if graph.nodes.len() > MAX_OVERVIEW_NODES {
        return Err(format!(
            "An overview takes at most {MAX_OVERVIEW_NODES} nodes"
        ));
    }
    validate(&graph)?;
    Ok(graph)
}

/// The options the cluster hierarchy depends on (and nothing else): a session lays out again on the hierarchy it has
/// while they stay the same.
fn hierarchy_key(v: &Value, options: &OverviewOptions) -> Value {
    json!({
        "communities": options.communities,
        "satellites": options.satellites,
        "groups": v["options"].get("groups"),
        "hubs": v["options"].get("hubs"),
    })
}

/// The graph with the groups the data gives, and its cluster hierarchy: with `start.nodes`, the clusters of those
/// places continue (`clusters_by_id`). Louvain runs within `budget` (#211).
fn hierarchy_of(
    graph: &Graph,
    v: &Value,
    options: &OverviewOptions,
    budget: &mut Budget,
) -> Result<(Graph, overview::Hierarchy, Option<FromPlaces>), String> {
    // Groups the data gives (node id → path of group names), as containers of kind `group`, the same as an import's
    // `groups`; communities then divide what is still large.
    let groups: std::collections::BTreeMap<String, Vec<String>> = match v["options"].get("groups") {
        Some(g) => serde_json::from_value(g.clone()).map_err(|e| {
            format!("Overview option groups must map node ids to paths of names: {e}")
        })?,
        None => Default::default(),
    };
    // Hubs: left out of Louvain, so that one hub does not pull its container into one community.
    let hubs: arqlet_model::CommunityHubs = match v["options"].get("hubs") {
        Some(h) => serde_json::from_value(h.clone())
            .map_err(|e| format!("Overview option hubs must be {{minLinks?, nodes?}}: {e}"))?,
        None => Default::default(),
    };
    // #147: buses given as data (`Graph.buses` with taps) are nodes of shape `busLine` with one edge per tap, as the
    // layout and `trace` see them, so the overview draws their bus lines too.
    let expanded = arqlet_layout::with_bus_nodes(graph);
    let graph =
        arqlet_layout::communities::with_given_groups(expanded.as_ref().unwrap_or(graph), &groups)?;
    let base = overview::hierarchy(&graph);
    let hubs = crate::hubs_of(&hubs);
    if let Some(places) = places_of(v)? {
        let (h, kept) = overview::clusters_by_id_within(
            base,
            &places,
            options.communities,
            &hubs,
            options.satellites,
            budget,
        )?;
        return Ok((graph, h, Some((places, kept))));
    }
    let h = overview::with_satellites(
        overview::with_communities_within(base, options.communities, &hubs, budget)?,
        options.satellites,
    )?;
    Ok((graph, h, None))
}

/// Lays out a hierarchy (of `graph`, with its groups) as the request `v` says (options, hints, representation). With a
/// work budget (#211; its request and the budget the clustering left), the containers are arranged within it and the
/// answer reports the work.
fn lay_out(
    graph: &Graph,
    h: &overview::Hierarchy,
    from_places: Option<&FromPlaces>,
    v: &Value,
    options: &OverviewOptions,
    threads: usize,
    work: Option<(WorkRequest, Budget)>,
) -> Result<Overview, String> {
    let (mut hints, continuation) = hints_of(v, h, from_places)?;
    let (refuse, mut budget) = match work {
        Some((request, budget)) => (request.refuse, Some(budget)),
        None => (false, None),
    };
    if refuse && budget.as_ref().is_some_and(|b| b.exhausted) {
        return Err(refusal(h, budget.as_ref().unwrap()));
    }
    let mut packed: Vec<i64> = Vec::new();
    // `edgeData`: each edge's kind and weight in the answer.
    let edge_data = edge_data_of(v)?;
    // `busRows`: the members of every node of shape `busLine`, from the resolved edges (other buses left out).
    let rows_wanted = options.rows.contains_key(&usize::MAX);
    let mut options = options.clone();
    if rows_wanted {
        options.rows.clear();
        let bus: Vec<bool> = graph
            .nodes
            .iter()
            .map(|n| n.shape == arqlet_model::NodeShape::BusLine)
            .collect();
        let is_bus = |i: usize| bus.get(i).copied().unwrap_or(false);
        for &(a, b) in &h.edges {
            if is_bus(a) && !is_bus(b) {
                options.rows.entry(a).or_default().push(b);
            } else if is_bus(b) && !is_bus(a) {
                options.rows.entry(b).or_default().push(a);
            }
        }
        for members in options.rows.values_mut() {
            members.sort_unstable();
            members.dedup();
        }
    }
    let options = &options;
    let levels = overview::levels(&h);
    let mut arrangements: BTreeMap<Container, Arrangement> = BTreeMap::new();
    for level in &levels {
        let radius = overview::radii(&h, &arrangements, &options, &hints);
        let done = match budget.as_mut() {
            None => crate::parallel_map(level, threads, |&container| {
                overview::arrange(&h, container, &radius, &options, hints.start.as_ref())
            }),
            Some(budget) => {
                let start = hints.start.as_ref();
                let tried = arrange_level(level, threads, budget, &|container, meter| {
                    overview::arrange_metered(&h, container, &radius, &options, start, meter)
                });
                if refuse && budget.exhausted {
                    return Err(refusal(h, budget));
                }
                level
                    .iter()
                    .zip(tried)
                    .map(|(&container, done)| {
                        done.unwrap_or_else(|| {
                            let (a, how) =
                                overview::fallback(&h, container, &radius, &options, start);
                            if how == Fallback::Packed {
                                packed.push(container_index(container));
                            }
                            a
                        })
                    })
                    .collect()
            }
        };
        arrangements.extend(level.iter().copied().zip(done));
        overview::carry_shifts(&mut hints, level, &arrangements);
    }
    packed.sort_unstable();
    let work = budget.map(|b| {
        let mut unclustered: Vec<i64> = b.unclustered.iter().map(|&c| container_index(c)).collect();
        unclustered.sort_unstable();
        OverviewWork {
            budget: b.limit,
            used: b.used,
            clustering: b.clustering,
            limited: b.exhausted,
            unclustered,
            packed,
        }
    });
    let o = overview::assemble(&h, &arrangements, &options, &hints);
    let parent: Vec<i64> = h
        .parent
        .iter()
        .map(|p| p.map_or(-1, |p| p as i64))
        .collect();
    let edges: Vec<u32> = h
        .edges
        .iter()
        .flat_map(|&(a, b)| [a as u32, b as u32])
        .collect();
    // The links of every container between its children, flat: child, child, number of edges.
    let mut links: Vec<f64> = Vec::new();
    for l in &h.links {
        for &(a, b, w) in l {
            links.extend([a as f64, b as f64, w]);
        }
    }
    let containers = arrangements.len() - 1;
    // Node kinds as a table and an index per node, for rules of the data (ADR 0018, addendum).
    let mut kinds: Vec<&str> = Vec::new();
    let mut kind_index: BTreeMap<&str, u32> = BTreeMap::new();
    // Clusters the overview added are of kind `community`.
    let kind: Vec<u32> = graph
        .nodes
        .iter()
        .map(|node| node.kind.as_str())
        .chain(std::iter::repeat_n("community", h.ids.len() - h.real))
        .map(|k| {
            *kind_index.entry(k).or_insert_with(|| {
                kinds.push(k);
                kinds.len() as u32 - 1
            })
        })
        .collect();
    let representation = representation_of(v, h, &kinds)?;
    let labels: Vec<&str> = graph.nodes.iter().map(|n| n.label.as_str()).collect();
    // Kinds as a table in order of first use, as for nodes; weights only when an edge counts more than once.
    let (mut edge_kinds, mut edge_kind, mut edge_weight) = (None, None, None);
    if edge_data {
        let resolved = overview::resolved_edges(graph);
        let mut table: Vec<String> = Vec::new();
        let mut at: BTreeMap<&str, u32> = BTreeMap::new();
        edge_kind = Some(
            resolved
                .iter()
                .map(|e| {
                    *at.entry(e.kind.as_str()).or_insert_with(|| {
                        table.push(e.kind.clone());
                        table.len() as u32 - 1
                    })
                })
                .collect(),
        );
        edge_kinds = Some(table);
        edge_weight = resolved
            .iter()
            .any(|e| e.count() > 1)
            .then(|| resolved.iter().map(|e| e.count() as u32).collect());
    }
    let count = |n: usize| n as u32;
    Ok(Overview {
        schema_version: 1,
        representation,
        ids: h.ids.clone(),
        // Each node's label, so that a viewer names nodes as the data does; the clusters the overview added are named
        // after their most connected member (`cluster_labels`).
        labels: labels
            .iter()
            .map(|l| l.to_string())
            .chain(overview::cluster_labels(h, &labels))
            .collect(),
        parent,
        x: o.x.iter().map(|&x| round(x)).collect(),
        y: o.y.iter().map(|&y| round(y)).collect(),
        r: o.radius.iter().map(|&r| round(r)).collect(),
        root_radius: round(o.root_radius),
        kinds: kinds.iter().map(|k| k.to_string()).collect(),
        kind,
        bus_lines: overview::bus_lines(graph, &h.parent, &h.edges, &o.x, &o.y, options.node_radius)
            .into_iter()
            .map(|mut l| {
                for v in [&mut l.x1, &mut l.y1, &mut l.x2, &mut l.y2] {
                    *v = round(*v);
                }
                l.feet.iter_mut().for_each(|v| *v = round(*v));
                l
            })
            .collect(),
        edges,
        links,
        stats: OverviewStats {
            nodes: count(h.ids.len()),
            containers: count(containers),
            communities: count(h.ids.len() - h.real),
            levels: count(levels.len()),
            edges: count(h.edges.len()),
            unresolved_edges: count(graph.edges.len() - h.edges.len()),
            overlaps: count(o.overlaps),
        },
        edge_kinds,
        edge_kind,
        edge_weight,
        continuation,
        work,
    })
}

/// The `edgeData` of a request.
fn edge_data_of(v: &Value) -> Result<bool, String> {
    match v.get("edgeData") {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err("Overview edgeData must be true or false".into()),
    }
}

/// Provisional (ADR 0019 for the overview): what an engine session keeps of an overview between requests: the graph,
/// the cluster hierarchy with the options it was built with, and the last positions. A host sends the graph once; later
/// requests leave it out, and a `start` without positions (`{"settle": …}` or `{}`) continues from the last layout, if
/// the options the clusters depend on (`communities`, `satellites`, `groups`) are the same. A `start` with `nodes`
/// (places by id) continues from those instead, with their clusters and with a new graph too; later requests with a
/// `start` build on these clusters, one without clusters the graph anew. The
/// answers are the same as those of `dispatch` given the graph and the positions in full.
#[derive(Default)]
pub struct OverviewSession {
    graph: Option<Graph>,
    /// The options the hierarchy depends on, the graph with its groups, the hierarchy, and whether its clusters are
    /// those of places by id (`start.nodes`) instead of the graph's own.
    hierarchy: Option<(Value, Graph, overview::Hierarchy, bool)>,
    /// The clustering work of that hierarchy (#211), charged again when a request with a work budget reuses it, and
    /// whether its clustering was complete (a hierarchy with containers the budget left unsplit is not reused).
    clustering: (u64, bool),
    /// The cluster options and the positions (`x`, `y`, `r`, as answered) of the last layout, and whether it was
    /// answered with the edges' data.
    last: Option<(Value, Value, bool)>,
    /// The numbers of the last answer to a request with `columns: true`; dropped by the next request.
    columns: Option<crate::columns::OverviewColumns>,
}

impl OverviewSession {
    /// The heap the overview session holds: its graph, the graph with its groups and the last answer (estimated, see
    /// `crate::memory`).
    pub fn heap_estimate(&self) -> usize {
        self.graph.as_ref().map_or(0, crate::memory::graph)
            + self.hierarchy.as_ref().map_or(0, |(options, graph, ..)| {
                crate::memory::value(options) + crate::memory::graph(graph)
            })
            + self.last.as_ref().map_or(0, |(a, b, _)| {
                crate::memory::value(a) + crate::memory::value(b)
            })
    }

    /// Whether the session has a graph for an overview.
    pub fn is_open(&self) -> bool {
        self.graph.is_some()
    }

    /// The graph of the session's overview, if it has one.
    pub fn graph(&self) -> Option<&Graph> {
        self.graph.as_ref()
    }

    /// The columns of the last request with `columns: true` (provisional, ADR 0019 step 4), valid until the next request.
    pub fn columns(&self) -> Option<&crate::columns::OverviewColumns> {
        self.columns.as_ref()
    }

    /// Answers one `overview` request (`{ok, overview}`, or `{ok: false, error}`); a refused request changes nothing.
    /// With `"columns": true` the numeric arrays stay in the session (`columns()`) and the answer's `overview` lists them.
    pub fn run(&mut self, v: &Value, threads: usize) -> Value {
        self.columns = None;
        let columnar = v["columns"].as_bool() == Some(true);
        match self.overview(v, threads) {
            Ok((overview, same)) if columnar => {
                let columns = crate::columns::OverviewColumns::of(
                    overview,
                    same && v["positionsOnly"].as_bool() == Some(true),
                );
                let answer = json!({"ok": true, "overview": columns.describe()});
                self.columns = Some(columns);
                answer
            }
            // `positionsOnly`: on the clusters of the last overview, only what moves (`OverviewPositions`); the host keeps
            // the rest from its last answer.
            Ok((overview, true)) if v["positionsOnly"].as_bool() == Some(true) => {
                json!({"ok": true, "overview": OverviewPositions::of(overview)})
            }
            Ok((overview, _)) => json!({"ok": true, "overview": overview}),
            Err(e) => json!({"ok": false, "error": e}),
        }
    }

    /// The overview, and whether it was laid out on the clusters of the session's last overview, with the same
    /// `edgeData` (so that a positions-only answer is enough).
    fn overview(&mut self, v: &Value, threads: usize) -> Result<(Overview, bool), String> {
        let options = options_of(v)?;
        let new_graph = if v.get("graph").is_some() {
            Some(graph_of(v)?)
        } else {
            None
        };
        let graph = match (&new_graph, &self.graph) {
            (Some(g), _) | (None, Some(g)) => g,
            (None, None) => {
                return Err(
                    "The session has no overview yet: send the first overview with its graph."
                        .into(),
                )
            }
        };
        // The hierarchy the session has, while the graph and the options it depends on stay the same; places by id bring
        // their own clusters, and a request without `start` does not build on those of earlier places.
        let key = hierarchy_key(v, &options);
        let by_id = v["start"].get("nodes").is_some();
        let continues = v.get("start").is_some();
        let work = work_of(v)?;
        let mut budget = budget_of(work);
        // A hierarchy is reused only if its clustering was complete and fits this budget: then `dispatch` would build the
        // same one with the same work, which is charged.
        let (clustered, complete) = self.clustering;
        let reuse = new_graph.is_none()
            && !by_id
            && complete
            && clustered <= budget.limit
            && self
                .hierarchy
                .as_ref()
                .is_some_and(|(k, _, _, from_places)| *k == key && (continues || !from_places));
        let built = if reuse {
            budget.accept("clustering", clustered);
            None
        } else {
            Some(hierarchy_of(graph, v, &options, &mut budget)?)
        };
        let clustering = (budget.clustering, !budget.exhausted);
        let (grouped, h, from_places) = match (&built, &self.hierarchy) {
            (Some((g, h, from_places)), _) => (g, h, from_places.as_ref()),
            (None, Some((_, g, h, _))) => (g, h, None),
            (None, None) => return Err("The overview session lost its hierarchy".into()),
        };
        // A start without positions continues from the last layout of the same clusters.
        let mut request = v.clone();
        if let Some(start) = request.get_mut("start").and_then(Value::as_object_mut) {
            if !start.contains_key("x") && !start.contains_key("nodes") {
                let last =
                    match (&new_graph, &self.last) {
                        (None, Some((clusters, last, _))) if *clusters == key => last,
                        (None, Some(_)) => return Err(
                            "The clusters changed since the last overview; lay out without start."
                                .into(),
                        ),
                        _ => {
                            return Err(
                                "No overview to continue from: lay out without start first.".into(),
                            )
                        }
                    };
                for key in ["x", "y", "r"] {
                    start.insert(key.into(), last[key].clone());
                }
            }
        }
        let overview = lay_out(
            grouped,
            h,
            from_places,
            &request,
            &options,
            threads,
            work.map(|w| (w, budget)),
        )?;
        // The same clusters as the last overview of this session: its ids, labels, parents, kinds, edges and links hold,
        // and so do the edges' data, which the host has when that answer had them.
        let edge_data = edge_data_of(v)?;
        let same_clusters = new_graph.is_none()
            && self
                .last
                .as_ref()
                .is_some_and(|(clusters, _, data)| *clusters == key && *data == edge_data)
            && self
                .hierarchy
                .as_ref()
                .is_some_and(|(_, _, last, _)| last.ids == h.ids && last.parent == h.parent);
        // Accepted: keep what the next request builds on.
        let last = (
            key.clone(),
            json!({"x": overview.x, "y": overview.y, "r": overview.r}),
            edge_data,
        );
        if let Some((g, h, from_places)) = built {
            self.hierarchy = Some((key, g, h, from_places.is_some()));
            self.clustering = clustering;
        }
        if let Some(g) = new_graph {
            self.graph = Some(g);
        }
        self.last = Some(last);
        Ok((overview, same_clusters))
    }
}
