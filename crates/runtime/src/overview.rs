//! Provisional: the `overview` operation (ADR 0018). Lays out a whole graph as discs in its containment hierarchy
//! (`arqlet_layout::overview`), running the containers of one level in parallel on native builds. The result is the same
//! for any thread count.
use arqlet_layout::overview::{
    self, Arrangement, Container, OverviewHints, OverviewOptions, Previous,
};
use arqlet_model::{
    validate, ConfigSet, Graph, Overview, OverviewConfig, OverviewIntent, OverviewPositions,
    OverviewRepresentation, OverviewStats,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

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

/// The hints of a request: `radii` (node id → radius, for clusters a host shows structured) and `start` (`{x, y, r}` of a
/// previous overview of the same graph, in its node order) to continue from.
fn hints_of(v: &Value, h: &overview::Hierarchy) -> Result<OverviewHints, String> {
    let mut hints = OverviewHints::default();
    if let Some(radii) = v.get("radii") {
        let index: std::collections::BTreeMap<&str, usize> = h
            .ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
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
                    x.as_f64()
                        .filter(|x| x.is_finite())
                        .ok_or_else(|| format!("Overview start {key} must be finite numbers"))
                })
                .collect()
        };
        let (x, y, r) = (numbers("x")?, numbers("y")?, numbers("r")?);
        hints.start = Some(Previous {
            centre: x.into_iter().zip(y).collect(),
            radius: r,
            // `settle: true` lets the changed containers settle again (for a host that animates the change).
            settle: start["settle"].as_bool().unwrap_or(false),
            moved: match start.get("moved") {
                None => BTreeMap::new(),
                Some(moved) => {
                    let index: BTreeMap<&str, usize> = h
                        .ids
                        .iter()
                        .enumerate()
                        .map(|(i, id)| (id.as_str(), i))
                        .collect();
                    moved
                        .as_object()
                        .ok_or("Overview start moved must be an object: node id -> {x, y}")?
                        .iter()
                        .map(|(id, p)| {
                            let i = *index.get(id.as_str()).ok_or_else(|| {
                                format!("Overview start moved an unknown node: {id}")
                            })?;
                            let at = |k: &str| {
                                p[k].as_f64().filter(|v| v.is_finite()).ok_or_else(|| {
                                    format!("Overview start moved {id} needs finite x and y")
                                })
                            };
                            Ok((i, (at("x")?, at("y")?)))
                        })
                        .collect::<Result<_, String>>()?
                }
            },
        });
    }
    Ok(hints)
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
    let (graph, h) = hierarchy_of(&graph, v, &options)?;
    lay_out(&graph, &h, v, &options, threads)
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

/// The graph with the groups the data gives, and its cluster hierarchy.
fn hierarchy_of(
    graph: &Graph,
    v: &Value,
    options: &OverviewOptions,
) -> Result<(Graph, overview::Hierarchy), String> {
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
    let h = overview::with_satellites(
        overview::with_communities_except(
            overview::hierarchy(&graph),
            options.communities,
            &crate::hubs_of(&hubs),
        )?,
        options.satellites,
    )?;
    Ok((graph, h))
}

/// Lays out a hierarchy (of `graph`, with its groups) as the request `v` says (options, hints, representation).
fn lay_out(
    graph: &Graph,
    h: &overview::Hierarchy,
    v: &Value,
    options: &OverviewOptions,
    threads: usize,
) -> Result<Overview, String> {
    let mut hints = hints_of(v, h)?;
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
        let done = crate::parallel_map(level, threads, |&container| {
            overview::arrange(&h, container, &radius, &options, hints.start.as_ref())
        });
        arrangements.extend(level.iter().copied().zip(done));
        overview::carry_shifts(&mut hints, level, &arrangements);
    }
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
    })
}

/// Provisional (ADR 0019 for the overview): what an engine session keeps of an overview between requests: the graph,
/// the cluster hierarchy with the options it was built with, and the last positions. A host sends the graph once; later
/// requests leave it out, and a `start` without positions (`{"settle": …}` or `{}`) continues from the last layout, if
/// the options the clusters depend on (`communities`, `satellites`, `groups`) are the same. The
/// answers are the same as those of `dispatch` given the graph and the positions in full.
#[derive(Default)]
pub struct OverviewSession {
    graph: Option<Graph>,
    /// The options the hierarchy depends on, the graph with its groups, and the hierarchy.
    hierarchy: Option<(Value, Graph, overview::Hierarchy)>,
    /// The cluster options and the positions (`x`, `y`, `r`, as answered) of the last layout.
    last: Option<(Value, Value)>,
    /// The numbers of the last answer to a request with `columns: true`; dropped by the next request.
    columns: Option<crate::columns::OverviewColumns>,
}

impl OverviewSession {
    /// The heap the overview session holds: its graph, the graph with its groups and the last answer (estimated, see
    /// `crate::memory`).
    pub fn heap_estimate(&self) -> usize {
        self.graph.as_ref().map_or(0, crate::memory::graph)
            + self.hierarchy.as_ref().map_or(0, |(options, graph, _)| {
                crate::memory::value(options) + crate::memory::graph(graph)
            })
            + self.last.as_ref().map_or(0, |(a, b)| {
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

    /// The overview, and whether it was laid out on the clusters of the session's last overview.
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
        // The hierarchy the session has, while the graph and the options it depends on stay the same.
        let key = hierarchy_key(v, &options);
        let reuse =
            new_graph.is_none() && self.hierarchy.as_ref().is_some_and(|(k, _, _)| *k == key);
        let built = if reuse {
            None
        } else {
            Some(hierarchy_of(graph, v, &options)?)
        };
        let (grouped, h) = match (&built, &self.hierarchy) {
            (Some((g, h)), _) | (None, Some((_, g, h))) => (g, h),
            (None, None) => return Err("The overview session lost its hierarchy".into()),
        };
        // A start without positions continues from the last layout of the same clusters.
        let mut request = v.clone();
        if let Some(start) = request.get_mut("start").and_then(Value::as_object_mut) {
            if !start.contains_key("x") {
                let last =
                    match (&new_graph, &self.last) {
                        (None, Some((clusters, last))) if *clusters == key => last,
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
        let overview = lay_out(grouped, h, &request, &options, threads)?;
        // The same clusters as the last overview of this session: its ids, labels, parents, kinds, edges and links hold.
        let same_clusters = new_graph.is_none()
            && self
                .last
                .as_ref()
                .is_some_and(|(clusters, _)| *clusters == key);
        // Accepted: keep what the next request builds on.
        let last = (
            key.clone(),
            json!({"x": overview.x, "y": overview.y, "r": overview.r}),
        );
        if let Some((g, h)) = built {
            self.hierarchy = Some((key, g, h));
        }
        if let Some(g) = new_graph {
            self.graph = Some(g);
        }
        self.last = Some(last);
        Ok((overview, same_clusters))
    }
}
