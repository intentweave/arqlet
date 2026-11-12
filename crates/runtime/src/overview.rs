//! Provisional: the `overview` operation (ADR 0018). Lays out a whole graph as discs in its containment hierarchy
//! (`arqlet_layout::overview`), running the containers of one level in parallel on native builds. The result is the same
//! for any thread count.
use arqlet_layout::overview::{
    self, Arrangement, Container, OverviewHints, OverviewOptions, Previous,
};
use arqlet_model::{validate, ConfigSet, Graph, OverviewConfig, OverviewIntent};
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
        });
    }
    Ok(hints)
}

/// The representation values of a request (ADR 0018, addendum; ADR 0015): `intent` (the user's decisions by node id)
/// and the `overview` part of `configSet` (rules by node kind, and whether the zoom decides the rest). A decision for a
/// node the graph does not have is refused; a rule for a kind the graph does not have is reported, not refused (a config
/// set serves many graphs).
fn representation_of(v: &Value, h: &overview::Hierarchy, kinds: &[&str]) -> Result<Value, String> {
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
    let unknown_kinds: Vec<&String> = config
        .rules
        .keys()
        .filter(|k| !kinds.contains(&k.as_str()))
        .collect();
    Ok(json!({
        "decisions": intent.decisions,
        "rules": config.rules,
        "auto": config.auto.unwrap_or(true),
        "unknownKinds": unknown_kinds,
    }))
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

fn overview_of(v: &Value, threads: usize) -> Result<Value, String> {
    let graph: Graph = serde_json::from_value(v["graph"].clone()).map_err(|e| e.to_string())?;
    if graph.nodes.len() > MAX_OVERVIEW_NODES {
        return Err(format!(
            "An overview takes at most {MAX_OVERVIEW_NODES} nodes"
        ));
    }
    validate(&graph)?;
    let options = options_of(v)?;
    let h = overview::with_communities(overview::hierarchy(&graph), options.communities)?;
    let hints = hints_of(v, &h)?;
    let levels = overview::levels(&h);
    let mut arrangements: BTreeMap<Container, Arrangement> = BTreeMap::new();
    for level in &levels {
        let radius = overview::radii(&h, &arrangements, &options, &hints);
        let done = crate::parallel_map(level, threads, |&container| {
            overview::arrange(&h, container, &radius, &options, hints.start.as_ref())
        });
        arrangements.extend(level.iter().copied().zip(done));
    }
    let o = overview::assemble(&h, &arrangements, &options, &hints);
    let parent: Vec<i64> = h
        .parent
        .iter()
        .map(|p| p.map_or(-1, |p| p as i64))
        .collect();
    let edges: Vec<usize> = h.edges.iter().flat_map(|&(a, b)| [a, b]).collect();
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
    let mut kind_index: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    // Clusters the overview added are of kind `community`.
    let kind: Vec<usize> = graph
        .nodes
        .iter()
        .map(|node| node.kind.as_str())
        .chain(std::iter::repeat_n("community", h.ids.len() - h.real))
        .map(|k| {
            *kind_index.entry(k).or_insert_with(|| {
                kinds.push(k);
                kinds.len() - 1
            })
        })
        .collect();
    let representation = representation_of(v, &h, &kinds)?;
    Ok(json!({
        "schemaVersion": 1,
        "representation": representation,
        "ids": h.ids,
        // Each node's label (empty for the clusters the overview added), so that a viewer names nodes as the data does.
        "labels": graph.nodes.iter().map(|n| n.label.as_str()).chain(std::iter::repeat_n("", h.ids.len() - h.real)).collect::<Vec<_>>(),
        "parent": parent,
        "x": o.x.iter().map(|&x| round(x)).collect::<Vec<_>>(),
        "y": o.y.iter().map(|&y| round(y)).collect::<Vec<_>>(),
        "r": o.radius.iter().map(|&r| round(r)).collect::<Vec<_>>(),
        "rootRadius": round(o.root_radius),
        "kinds": kinds,
        "kind": kind,
        "edges": edges,
        "links": links,
        "stats": {
            "nodes": h.ids.len(),
            "containers": containers,
            "communities": h.ids.len() - h.real,
            "levels": levels.len(),
            "edges": h.edges.len(),
            "unresolvedEdges": graph.edges.len() - h.edges.len(),
            "overlaps": o.overlaps,
        },
    }))
}
