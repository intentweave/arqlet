//! Provisional: graphs in the JSON form ELK uses, translated into a Arqlet graph and options, and the computed layout
//! written back into that form. Not a 1:1 port of ELK and no ELK code or dependency: the structure (`children`, `ports`,
//! `edges` with `sources` and `targets`, `labels`, `layoutOptions`) and the common options are read; what has no counterpart
//! is reported as a diagnostic, never silently dropped.
//!
//! Read:
//! - nodes with `width`/`height` (leaves keep their size; a node with children is a container and gets its size from the
//!   layout), the first label's text as the node label;
//! - ports by the node's `elk.portConstraints`: `FREE` leaves their sides to the layout; `FIXED_POS` and `FIXED_RATIO` place a
//!   port by its position, on its side (`elk.port.side`: `NORTH`, `EAST`, `SOUTH`, `WEST`) or else the border nearest it;
//!   otherwise a port with a side keeps it, spread over the side in the order of `elk.port.index` (clockwise, as ELK counts)
//!   or of the input; ports without a side are left to the layout. With explicit ports the translation turns on the
//!   provisional `portSides` option, so that single pass constructs edges at ports across or against the flow;
//! - edges anywhere in the hierarchy, between nodes or ports; an edge with several sources or targets (a hyperedge) becomes one
//!   edge per source and target, `"{id}#{k}"`; one with a single source or a single target becomes an explicit bus (its parts
//!   share a trunk) and gets ELK's `junctionPoints` where they part;
//! - the first label of an edge with a size, as the edge's label;
//! - `elk.nodeLabels.placement` (on the label, the node or the root) for where a node's label is written back; `OUTSIDE`
//!   labels get no room in the layout, which is reported;
//! - `elk.direction` (`RIGHT`, `DOWN`, `LEFT`, `UP`, `UNDEFINED`), `elk.algorithm` (`layered`, `mrtree` as tree, `force` and
//!   `stress` as organic, `radial`) and `elk.edgeRouting` (`ORTHOGONAL`, `POLYLINE`; `SPLINES` as polyline), on the root and
//!   on any node with children;
//! - on the root, the spacings `spacing.nodeNode` (`nodeSpacing`), `spacing.nodeNodeBetweenLayers` (`layerSpacing`),
//!   `spacing.edgeEdge` and `spacing.edgeEdgeBetweenLayers` (the smaller as `trackSpacing`) and `spacing.portPort`
//!   (`minPortSpacing`). Arqlet keeps its own label room and channel widths on top of them;
//! - on the root, `layered.considerModelOrder.strategy` other than `NONE` (provisional): ties follow the input order
//!   instead of the ids (`in_model_order` renames nodes, ports and edges in input order before the layout and
//!   `restore_ids` gives the answer its ids back), so the drawing no longer changes when nodes are renamed.
//!
//! Written back: `x`, `y`, `width`, `height` of every node relative to its parent, of every port relative to its node, of every
//! label relative to its owner; the root's size; and every edge's `sections` (`startPoint`, `endPoint`, `bendPoints`,
//! `incomingShape`, `outgoingShape`) relative to the node the edge is listed in, as ELK writes them by default, with that node
//! as the edge's `container`.

use arqlet_model::math::PortableMath;
use std::collections::{BTreeMap, BTreeSet};

use arqlet_model::{Point, Rect, Snapshot};
use serde_json::{json, Map, Value};

/// A graph in ELK's JSON form, translated.
#[derive(Clone, Debug)]
pub struct Translation {
    /// The Arqlet graph, as JSON in its contract form.
    pub graph: Value,
    /// The layout options it needs (node sizes, edge labels), as a patch over the defaults.
    pub options: Value,
    /// Every option and construct that has no counterpart, once each.
    pub diagnostics: Vec<String>,
    /// By ELK edge id: the Arqlet edges it became, in order, and the node it is listed in (`None`: the root).
    edges: BTreeMap<String, (Vec<String>, Option<String>)>,
    /// By ELK port id: its node, and the Arqlet port it became when it has a side.
    ports: BTreeMap<String, (String, Option<String>)>,
    /// By Arqlet edge: its ELK source and target (node or port ids), for the sections' shapes.
    ends: BTreeMap<String, (String, String)>,
}

/// The options as their short names: `org.eclipse.elk.direction` and `elk.direction` are both `direction`. The spacings
/// ELK defines for the layered algorithm (`elk.layered.spacing.nodeNodeBetweenLayers`, its official name) are read as the
/// spacing of the same name (`spacing.nodeNodeBetweenLayers`).
fn short(key: &str) -> &str {
    let key = key
        .strip_prefix("org.eclipse.elk.")
        .or_else(|| key.strip_prefix("elk."))
        .unwrap_or(key);
    match key.strip_prefix("layered.") {
        Some(rest) if rest.starts_with("spacing.") => rest,
        _ => key,
    }
}

fn options_of(node: &Value) -> BTreeMap<String, String> {
    node.get("layoutOptions")
        .or_else(|| node.get("properties"))
        .and_then(Value::as_object)
        .map(|o| {
            o.iter()
                .map(|(k, v)| {
                    let value = match v {
                        Value::String(s) => s.clone(),
                        other => other.to_string(),
                    };
                    (short(k).to_string(), value)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn number(node: &Value, key: &str) -> Option<f64> {
    node.get(key).and_then(Value::as_f64)
}

/// A node label's place by `elk.nodeLabels.placement` (`H_LEFT`/`H_CENTER`/`H_RIGHT`, `V_TOP`/`V_CENTER`/`V_BOTTOM`,
/// `INSIDE`/`OUTSIDE`, in any order and notation): horizontal and vertical as -1, 0, 1, and outside. A value without any of
/// these is none.
#[derive(Clone, Copy, Debug, PartialEq)]
struct LabelPlacement {
    h: i8,
    v: i8,
    outside: bool,
}

fn label_placement(value: &str) -> Option<LabelPlacement> {
    let mut out = LabelPlacement {
        h: 0,
        v: 0,
        outside: false,
    };
    let mut any = false;
    for token in value
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|t| !t.is_empty())
    {
        any = true;
        match token {
            "H_LEFT" => out.h = -1,
            "H_CENTER" => out.h = 0,
            "H_RIGHT" => out.h = 1,
            "V_TOP" => out.v = -1,
            "V_CENTER" => out.v = 0,
            "V_BOTTOM" => out.v = 1,
            "OUTSIDE" => out.outside = true,
            "INSIDE" => out.outside = false,
            _ => {}
        }
    }
    any.then_some(out)
}

/// The label's offset from its node's corner: inside, by the placement within the node with `PADDING`; outside, beside the
/// side it names with the same gap (a vertical side first: `V_TOP` and `V_BOTTOM` put it above and below; otherwise left
/// or right).
fn label_offset(p: LabelPlacement, node: (f64, f64), label: (f64, f64)) -> (f64, f64) {
    const PADDING: f64 = 5.;
    let ((w, h), (lw, lh)) = (node, label);
    let inside = |k: i8, size: f64, own: f64| match k {
        -1 => PADDING,
        1 => size - own - PADDING,
        _ => (size - own) / 2.,
    };
    if !p.outside {
        return (inside(p.h, w, lw), inside(p.v, h, lh));
    }
    match (p.v, p.h) {
        (-1, _) => (inside(p.h, w, lw), -lh - PADDING),
        (1, _) => (inside(p.h, w, lw), h + PADDING),
        (_, -1) => (-lw - PADDING, inside(0, h, lh)),
        _ => (w + PADDING, inside(0, h, lh)),
    }
}

fn id_of(node: &Value) -> Result<String, String> {
    match node.get("id") {
        Some(Value::String(s)) => Ok(s.clone()),
        Some(Value::Number(n)) => Ok(n.to_string()),
        _ => Err(format!("ELK element without an id: {node}")),
    }
}

/// The options of one area that have a Arqlet counterpart, and the names of those that have none.
struct AreaOptions {
    direction: Option<&'static str>,
    strategy: Option<&'static str>,
    routing: Option<&'static str>,
    /// Arqlet options by name, from ELK's spacings: they apply to the whole graph.
    spacing: BTreeMap<&'static str, f64>,
    /// `separateConnectedComponents`: whether components are laid out as blocks of their own (Arqlet's
    /// `componentPacking`). Without it, Arqlet's layout already stacks unconnected parts across the flow, so ELK's
    /// default (yes) needs nothing; only an explicit `true` asks for the packing.
    separate: Option<bool>,
}

fn area_options(
    options: &BTreeMap<String, String>,
    diagnostics: &mut BTreeSet<String>,
) -> AreaOptions {
    let mut out = AreaOptions {
        direction: None,
        strategy: None,
        routing: None,
        spacing: BTreeMap::new(),
        separate: None,
    };
    for (key, value) in options {
        let value = value.trim_matches('"');
        match key.as_str() {
            "direction" => {
                out.direction = match value {
                    "RIGHT" => Some("lr"),
                    "DOWN" => Some("tb"),
                    "LEFT" => Some("rl"),
                    "UP" => Some("bt"),
                    "UNDEFINED" => Some("auto"),
                    other => {
                        diagnostics.insert(format!(
                            "direction {other} is not known; the default is used"
                        ));
                        None
                    }
                }
            }
            "algorithm" => {
                out.strategy = match value.rsplit('.').next().unwrap_or(value) {
                    "layered" => Some("none"),
                    "mrtree" => Some("tree"),
                    "force" | "stress" => Some("organic"),
                    "radial" => Some("radial"),
                    other => {
                        diagnostics.insert(format!(
                            "algorithm {other} has no counterpart; the layered layout is used"
                        ));
                        None
                    }
                }
            }
            "edgeRouting" => {
                out.routing = match value {
                    "ORTHOGONAL" | "UNDEFINED" => None,
                    "POLYLINE" => Some("polyline"),
                    "SPLINES" => {
                        diagnostics.insert("edgeRouting SPLINES is drawn as POLYLINE".into());
                        Some("polyline")
                    }
                    other => {
                        diagnostics.insert(format!(
                            "edgeRouting {other} is not known; ORTHOGONAL is used"
                        ));
                        None
                    }
                }
            }
            // Spacings: between neighbours of a layer, between layers, between parallel edges, between ports of a side.
            "spacing.nodeNode"
            | "spacing.nodeNodeBetweenLayers"
            | "spacing.edgeEdge"
            | "spacing.edgeEdgeBetweenLayers"
            | "spacing.edgeNode"
            | "spacing.edgeNodeBetweenLayers"
            | "spacing.portPort" => {
                let Some(v) = value
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite() && *v >= 0.)
                else {
                    diagnostics.insert(format!(
                        "{key} {value} is not a non-negative number; not used"
                    ));
                    continue;
                };
                // The distance between an edge and a node it does not attach to: Arqlet's `routingClearance`, one value
                // for both of ELK's; the larger one, so that both hold.
                if key.starts_with("spacing.edgeNode") {
                    let entry = out.spacing.entry("routingClearance").or_insert(v);
                    *entry = entry.max(v);
                    continue;
                }
                let name = match key.as_str() {
                    "spacing.nodeNode" => "nodeSpacing",
                    "spacing.nodeNodeBetweenLayers" => "layerSpacing",
                    "spacing.portPort" => "minPortSpacing",
                    _ => "trackSpacing",
                };
                // Two edge spacings: the smaller one, so that both hold.
                let entry = out.spacing.entry(name).or_insert(v);
                *entry = entry.min(v);
            }
            "separateConnectedComponents" => match value {
                "true" => out.separate = Some(true),
                "false" => out.separate = Some(false),
                other => {
                    diagnostics.insert(format!(
                        "separateConnectedComponents {other} is not true or false; not used"
                    ));
                }
            },
            // Read before the translation (`wants_model_order`).
            "layered.considerModelOrder.strategy" | "considerModelOrder.strategy" => {}
            // Read where they apply (ports), or the defaults of Arqlet already are their meaning.
            "port.side"
            | "port.index"
            | "portConstraints"
            | "hierarchyHandling"
            | "nodeLabels.placement" => {}
            other => {
                diagnostics.insert(format!("option {other} has no counterpart and is not used"));
            }
        }
    }
    out
}

/// Translates a graph in ELK's JSON form. The root is the graph itself, not a node.
pub fn translate(elk: &Value) -> Result<Translation, String> {
    let mut nodes = Vec::new();
    let mut ports = Vec::new();
    let mut edge_ports = Vec::new();
    let mut edges = Vec::new();
    let mut node_sizes = Map::new();
    let mut edge_labels = Map::new();
    let mut container_directions = Map::new();
    let mut container_strategies = Map::new();
    let mut container_routing = Map::new();
    let mut diagnostics = BTreeSet::new();
    let mut port_map: BTreeMap<String, (String, Option<String>)> = BTreeMap::new();
    let mut known_nodes: BTreeSet<String> = BTreeSet::new();
    // (edge, the node it is listed in)
    let mut listed: Vec<(Value, Option<String>)> = Vec::new();

    let root = area_options(&options_of(elk), &mut diagnostics);
    for edge in elk
        .get("edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        listed.push((edge.clone(), None));
    }
    // Depth first: (node, parent).
    let mut stack: Vec<(Value, Option<String>)> = elk
        .get("children")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .rev()
        .map(|c| (c.clone(), None))
        .collect();
    while let Some((node, parent)) = stack.pop() {
        let id = id_of(&node)?;
        if !known_nodes.insert(id.clone()) {
            return Err(format!("ELK node {id} appears twice"));
        }
        let children: Vec<Value> = node
            .get("children")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let label = node
            .get("labels")
            .and_then(Value::as_array)
            .and_then(|l| l.first())
            .and_then(|l| l.get("text"))
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        nodes.push(json!({
            "id": id,
            "kind": if children.is_empty() { "node" } else { "container" },
            "label": label,
            "parentId": parent,
        }));
        let options = options_of(&node);
        let placement = node
            .get("labels")
            .and_then(Value::as_array)
            .and_then(|l| l.first())
            .and_then(|l| options_of(l).get("nodeLabels.placement").cloned())
            .or_else(|| options.get("nodeLabels.placement").cloned());
        if placement
            .as_deref()
            .and_then(label_placement)
            .is_some_and(|p| p.outside)
        {
            diagnostics.insert(
                "nodeLabels.placement OUTSIDE: the label is placed beside its node, but the layout keeps no room for it".into(),
            );
        }
        let (width, height) = (number(&node, "width"), number(&node, "height"));
        if children.is_empty() {
            if let (Some(w), Some(h)) = (width, height) {
                if w > 0. && h > 0. {
                    node_sizes.insert(id.clone(), json!({"width": w, "height": h}));
                }
            }
        } else {
            let area = area_options(&options, &mut diagnostics);
            if !area.spacing.is_empty() {
                diagnostics.insert(
                    "spacings apply to the whole graph; only the root's values are used".into(),
                );
            }
            // An organic area needs a free direction (a ranked layout is never replaced silently by a stress placement).
            let direction = area.direction.or((area.strategy == Some("organic")
                || area.strategy == Some("radial"))
            .then_some("auto"));
            if let Some(d) = direction {
                container_directions.insert(id.clone(), json!(d));
            }
            if let Some(s) = area.strategy {
                container_strategies.insert(id.clone(), json!(s));
            }
            if let Some(r) = area.routing {
                container_routing.insert(id.clone(), json!(r));
            }
        }
        // Ports by `elk.portConstraints`: `FREE` leaves sides and order to the layout; `FIXED_POS` and `FIXED_RATIO` place a port
        // by its position (its side, when it has none, is the nearest border); otherwise a port with a side keeps it, in the
        // order of `elk.port.index` (clockwise, as ELK counts) or of the input, spread over the side.
        let constraint = options
            .get("portConstraints")
            .map(|c| c.trim_matches('"').to_string())
            .unwrap_or_default();
        let free = constraint == "FREE";
        let by_position = constraint == "FIXED_POS" || constraint == "FIXED_RATIO";
        let node_ports: Vec<Value> = node
            .get("ports")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let size_of = |p: &Value| {
            (
                number(p, "width").unwrap_or(0.),
                number(p, "height").unwrap_or(0.),
            )
        };
        let side_of = |port: &Value| -> Option<&'static str> {
            if free {
                return None;
            }
            let declared = match options_of(port)
                .get("port.side")
                .map(|s| s.trim_matches('"').to_string())
                .as_deref()
            {
                Some("NORTH") => Some("top"),
                Some("EAST") => Some("right"),
                Some("SOUTH") => Some("bottom"),
                Some("WEST") => Some("left"),
                _ => None,
            };
            declared.or_else(|| {
                // By position: the border nearest the port's centre.
                if !by_position {
                    return None;
                }
                let (pw, ph) = size_of(port);
                let (cx, cy) = (number(port, "x")? + pw / 2., number(port, "y")? + ph / 2.);
                let (w, h) = (width?, height?);
                [
                    ("top", cy),
                    ("bottom", h - cy),
                    ("left", cx),
                    ("right", w - cx),
                ]
                .into_iter()
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(side, _)| side)
            })
        };
        let index_of = |port: &Value| -> Option<f64> {
            options_of(port)
                .get("port.index")
                .and_then(|v| v.trim_matches('"').parse().ok())
        };
        // By side, the ports in their order along the side from its start (left or top): ELK's index runs clockwise, so
        // the bottom and left sides are counted backwards.
        let mut by_side: BTreeMap<&'static str, Vec<(f64, usize, String)>> = BTreeMap::new();
        for (k, port) in node_ports.iter().enumerate() {
            if let Some(side) = side_of(port) {
                let index = index_of(port).unwrap_or(k as f64);
                by_side
                    .entry(side)
                    .or_default()
                    .push((index, k, id_of(port)?));
            }
        }
        for (side, list) in by_side.iter_mut() {
            list.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            if matches!(*side, "bottom" | "left") {
                list.reverse();
            }
        }
        for port in &node_ports {
            let pid = id_of(port)?;
            let Some(side) = side_of(port) else {
                port_map.insert(pid, (id.clone(), None));
                continue;
            };
            let along = |p: &Value| -> Option<f64> {
                let (pw, ph) = size_of(p);
                match side {
                    "left" | "right" => Some((number(p, "y")? + ph / 2.) / height?),
                    _ => Some((number(p, "x")? + pw / 2.) / width?),
                }
            };
            let offset = match by_position.then(|| along(port)).flatten() {
                Some(o) => o.clamp(0., 1.),
                None => {
                    let peers = &by_side[side];
                    let k = peers.iter().position(|p| p.2 == pid).unwrap_or(0);
                    (k as f64 + 1.) / (peers.len() as f64 + 1.)
                }
            };
            let lw = format!("elk-port:{pid}");
            ports.push(json!({"id": lw, "nodeId": id, "side": side, "offset": offset}));
            port_map.insert(pid, (id.clone(), Some(lw)));
        }
        for edge in node
            .get("edges")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            listed.push((edge.clone(), Some(id.clone())));
        }
        for child in children.into_iter().rev() {
            stack.push((child, Some(id.clone())));
        }
    }

    let mut edge_map: BTreeMap<String, (Vec<String>, Option<String>)> = BTreeMap::new();
    let mut buses: Vec<Value> = Vec::new();
    let mut ends: BTreeMap<String, (String, String)> = BTreeMap::new();
    let endpoint_ids = |edge: &Value, many: &str, one: &str| -> Vec<String> {
        match edge.get(many).and_then(Value::as_array) {
            Some(list) => list
                .iter()
                .filter_map(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .collect(),
            None => edge
                .get(one)
                .and_then(Value::as_str)
                .map(|s| vec![s.to_string()])
                .unwrap_or_default(),
        }
    };
    for (edge, container) in &listed {
        let id = id_of(edge)?;
        if edge_map.contains_key(&id) {
            return Err(format!("ELK edge {id} appears twice"));
        }
        let sources = endpoint_ids(edge, "sources", "source");
        let targets = endpoint_ids(edge, "targets", "target");
        if sources.is_empty() || targets.is_empty() {
            return Err(format!("ELK edge {id} needs a source and a target"));
        }
        let pairs: Vec<(String, String)> = sources
            .iter()
            .flat_map(|s| targets.iter().map(move |t| (s.clone(), t.clone())))
            .collect();
        // A hyperedge with one source or one target is drawn as a bus (its parts share a trunk); one with several of both is
        // split into an edge per pair.
        let bus = pairs.len() > 1 && (sources.len() == 1 || targets.len() == 1);
        if pairs.len() > 1 && !bus {
            diagnostics.insert(
                "hyperedges with several sources and several targets are split into one edge per source and target".into(),
            );
        }
        let resolve = |end: &str| -> Result<(String, Option<String>), String> {
            if let Some((node, port)) = port_map.get(end) {
                return Ok((node.clone(), port.clone()));
            }
            if known_nodes.contains(end) {
                return Ok((end.to_string(), None));
            }
            Err(format!(
                "ELK edge {id} refers to {end}, which is no node or port"
            ))
        };
        let label = edge
            .get("labels")
            .and_then(Value::as_array)
            .and_then(|l| l.first())
            .cloned();
        let mut lw_ids = Vec::new();
        for (k, (s, t)) in pairs.iter().enumerate() {
            let lw = if pairs.len() == 1 {
                id.clone()
            } else {
                format!("{id}#{k}")
            };
            let ((source, source_port), (target, target_port)) = (resolve(s)?, resolve(t)?);
            edges.push(json!({"id": lw, "kind": "edge", "source": source, "target": target}));
            if source_port.is_some() || target_port.is_some() {
                edge_ports.push(
                    json!({"edgeId": lw, "sourcePort": source_port, "targetPort": target_port}),
                );
            }
            if let Some(label) = &label {
                if let (Some(w), Some(h)) = (number(label, "width"), number(label, "height")) {
                    let text = label.get("text").and_then(Value::as_str).unwrap_or("");
                    edge_labels.insert(lw.clone(), json!({"text": text, "width": w, "height": h}));
                }
            }
            ends.insert(lw.clone(), (s.clone(), t.clone()));
            lw_ids.push(lw);
        }
        if bus {
            buses.push(json!({"id": id, "edges": lw_ids.clone()}));
        }
        edge_map.insert(id, (lw_ids, container.clone()));
    }

    let root_direction = root
        .direction
        .or((root.strategy == Some("organic") || root.strategy == Some("radial")).then_some("auto"))
        // ELK's layered default flows to the right.
        .unwrap_or("lr");
    let explicit_ports = !ports.is_empty();
    let mut graph = json!({
        "defaultDirection": root_direction,
        "containerDirections": container_directions,
        "defaultStrategy": root.strategy.unwrap_or("none"),
        "containerStrategies": container_strategies,
        "nodes": nodes,
        "edges": edges,
        "ports": ports,
        "edgePorts": edge_ports,
    });
    if let Some(r) = root.routing {
        graph["defaultRouting"] = json!(r);
    }
    if !container_routing.is_empty() {
        graph["containerRouting"] = Value::Object(container_routing);
    }
    let mut options = json!({});
    for (name, value) in &root.spacing {
        options[*name] = json!(value);
    }
    if !node_sizes.is_empty() {
        options["nodeSizes"] = Value::Object(node_sizes);
    }
    if !edge_labels.is_empty() {
        options["edgeLabels"] = Value::Object(edge_labels);
    }
    // Explicit ports on any side: the provisional `portSides` construction (measured on graphs with ports on all four sides
    // against the fallback router: fewer shared lines, crossings and border hugging, a few more bends;
    // benchmarks/results/elk-port-sides-20261108.md). A request's own `options` can turn it off.
    if explicit_ports {
        options["portSides"] = json!(true);
    }
    // Connected components as blocks of their own (`componentPacking`) when the graph asks for it. Measured on grouped and
    // unconnected graphs (10 groups or islands of 50 nodes), turning it on by default did not make the drawings smaller.
    if root.separate == Some(true) {
        options["componentPacking"] = json!(true);
    }
    // Nodes without any edge in a grid instead of one long row (`isolatedGrid`), as ELK packs its components (its default
    // for `separateConnectedComponents` is true, so only `false` turns this off), where they are at least half of the
    // nodes: on an evaluation's graphs without edges the row (19 624 x 172 for 74 nodes, 60 032 x 172 for 256) became a
    // block (2 520 x 1 972, 4 392 x 3 172); on one with 13 of 500 nodes unconnected the grid made the drawing 26 % larger
    // and added crossings, so it is left out there (benchmarks/results/elk-isolated-grid-20261112.md).
    let unconnected = {
        let mut linked = std::collections::BTreeSet::new();
        for e in graph["edges"].as_array().into_iter().flatten() {
            for end in ["source", "target"] {
                if let Some(id) = e[end].as_str() {
                    linked.insert(id.to_string());
                }
            }
        }
        let leaves: Vec<&Value> = graph["nodes"].as_array().into_iter().flatten().collect();
        let alone = leaves
            .iter()
            .filter(|n| n["id"].as_str().is_some_and(|id| !linked.contains(id)))
            .count();
        alone * 2 >= leaves.len() && alone >= 3
    };
    if root.separate != Some(false) && unconnected {
        options["isolatedGrid"] = json!(true);
    }
    if !buses.is_empty() {
        graph["buses"] = Value::Array(buses);
    }
    Ok(Translation {
        graph,
        options,
        diagnostics: diagnostics.into_iter().collect(),
        edges: edge_map,
        ports: port_map,
        ends,
    })
}

/// Where the polylines of a bus part: for every pair that starts (or ends) at one point, the end of the way they share from
/// there. The shared end itself is no junction.
fn junctions(paths: &[&Vec<Point>]) -> Vec<Point> {
    const EPS: f64 = 1e-6;
    let same = |a: &Point, b: &Point| (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS;
    // The point at which two polylines from one start stop running together.
    let part = |a: &[Point], b: &[Point]| -> Option<Point> {
        if !same(&a[0], &b[0]) {
            return None;
        }
        let (mut i, mut j) = (0, 0);
        let mut at = a[0].clone();
        while i + 1 < a.len() && j + 1 < b.len() {
            let (da, db) = (
                (a[i + 1].x - at.x, a[i + 1].y - at.y),
                (b[j + 1].x - at.x, b[j + 1].y - at.y),
            );
            let (la, lb) = (da.0.p_hypot(da.1), db.0.p_hypot(db.1));
            if la < EPS {
                i += 1;
                continue;
            }
            if lb < EPS {
                j += 1;
                continue;
            }
            let along =
                ((da.0 / la - db.0 / lb).abs() < EPS) && ((da.1 / la - db.1 / lb).abs() < EPS);
            if !along {
                break;
            }
            let step = la.min(lb);
            at = Point {
                x: at.x + da.0 / la * step,
                y: at.y + da.1 / la * step,
            };
            if (la - step).abs() < EPS {
                i += 1;
            }
            if (lb - step).abs() < EPS {
                j += 1;
            }
        }
        (!same(&at, &a[0])).then_some(at)
    };
    let mut out: Vec<Point> = Vec::new();
    for (k, a) in paths.iter().enumerate() {
        for b in &paths[k + 1..] {
            let reversed = |p: &Vec<Point>| p.iter().rev().cloned().collect::<Vec<_>>();
            let found = part(a, b).or_else(|| part(&reversed(a), &reversed(b)));
            if let Some(p) = found {
                let is_end = paths
                    .iter()
                    .any(|q| same(&q[0], &p) || same(q.last().expect("two points"), &p));
                if !is_end && !out.iter().any(|q| same(q, &p)) {
                    out.push(p);
                }
            }
        }
    }
    out.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    out
}

fn point(p: &Point, origin: (f64, f64)) -> Value {
    json!({"x": p.x - origin.0, "y": p.y - origin.1})
}

/// Writes the layout of `snapshot` (computed from `translation`) into a copy of the ELK graph `elk`.
pub fn write_back(elk: &Value, snapshot: &Snapshot, translation: &Translation) -> Value {
    let parent: BTreeMap<&str, Option<&str>> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let absolute = |id: &str| -> (f64, f64) {
        let mut at = Some(id);
        let (mut x, mut y) = (0., 0.);
        while let Some(n) = at {
            if let Some(r) = snapshot.boxes.get(n) {
                x += r.x;
                y += r.y;
            }
            at = parent.get(n).copied().flatten();
        }
        (x, y)
    };
    let mut out = elk.clone();
    // The root's size: everything placed, from the origin.
    let (mut w, mut h) = (0f64, 0f64);
    for (id, r) in &snapshot.boxes {
        if parent.get(id.as_str()).copied().flatten().is_none() {
            w = w.max(r.x + r.width);
            h = h.max(r.y + r.height);
        }
    }
    out["x"] = json!(0.);
    out["y"] = json!(0.);
    out["width"] = json!(w + 24.);
    out["height"] = json!(h + 24.);

    // A port's place: where an edge that uses it starts or ends (the resolved port), relative to its node.
    let mut port_points: BTreeMap<String, Point> = BTreeMap::new();
    for (lw, (s, t)) in &translation.ends {
        for (end, endpoint) in [(s, "source"), (t, "target")] {
            if translation.ports.contains_key(end) {
                if let Some(p) = snapshot
                    .resolved_ports
                    .iter()
                    .find(|p| &p.edge_id == lw && p.endpoint == endpoint)
                {
                    port_points.entry(end.clone()).or_insert(p.position.clone());
                }
            }
        }
    }
    let routes: BTreeMap<&str, &Vec<Point>> = snapshot
        .routes
        .iter()
        .map(|r| (r.id.as_str(), &r.points))
        .collect();
    let labels: BTreeMap<&str, &Rect> = snapshot
        .labels
        .iter()
        .filter(|l| l.kind == "edge")
        .map(|l| (l.id.as_str(), &l.bounds))
        .collect();

    let root_id = id_of(elk).unwrap_or_else(|_| "root".into());
    let write_edges = |node: &mut Value, container: Option<&str>| {
        let origin = container.map_or((0., 0.), absolute);
        let Some(list) = node.get_mut("edges").and_then(Value::as_array_mut) else {
            return;
        };
        for edge in list {
            let Ok(id) = id_of(edge) else { continue };
            let Some((lw_ids, _)) = translation.edges.get(&id) else {
                continue;
            };
            let mut sections = Vec::new();
            for (k, lw) in lw_ids.iter().enumerate() {
                let Some(points) = routes.get(lw.as_str()) else {
                    continue;
                };
                if points.len() < 2 {
                    continue;
                }
                let (s, t) = &translation.ends[lw];
                let mut section = json!({
                    "id": format!("{id}_s{k}"),
                    "startPoint": point(&points[0], origin),
                    "endPoint": point(points.last().expect("two points"), origin),
                    "incomingShape": s,
                    "outgoingShape": t,
                });
                // Only real bends: a point on the line of its neighbours is none.
                let bends: Vec<Value> = (1..points.len() - 1)
                    .filter(|&i| {
                        let (a, b, c) = (&points[i - 1], &points[i], &points[i + 1]);
                        let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
                        cross.abs() > 1e-9
                    })
                    .map(|i| point(&points[i], origin))
                    .collect();
                if !bends.is_empty() {
                    section["bendPoints"] = Value::Array(bends);
                }
                sections.push(section);
            }
            // A hyperedge drawn as a bus: where its sections part, as ELK's junction points.
            if lw_ids.len() > 1 {
                let paths: Vec<&Vec<Point>> = lw_ids
                    .iter()
                    .filter_map(|lw| routes.get(lw.as_str()).copied())
                    .filter(|p| p.len() >= 2)
                    .collect();
                let junctions = junctions(&paths);
                if !junctions.is_empty() {
                    edge["junctionPoints"] =
                        Value::Array(junctions.iter().map(|p| point(p, origin)).collect());
                }
            }
            edge["sections"] = Value::Array(sections);
            // The node whose frame the coordinates are in, as ELK names it.
            edge["container"] = json!(container.map_or(root_id.as_str(), |c| c));
            if let Some(bounds) = lw_ids.first().and_then(|f| labels.get(f.as_str())) {
                if let Some(label) = edge
                    .get_mut("labels")
                    .and_then(Value::as_array_mut)
                    .and_then(|l| l.first_mut())
                {
                    label["x"] = json!(bounds.x - origin.0);
                    label["y"] = json!(bounds.y - origin.1);
                }
            }
        }
    };
    write_edges(&mut out, None);

    // Every node, depth first, with the place of its ports and labels.
    fn walk(
        node: &mut Value,
        snapshot: &Snapshot,
        absolute: &dyn Fn(&str) -> (f64, f64),
        port_points: &BTreeMap<String, Point>,
        write_edges: &dyn Fn(&mut Value, Option<&str>),
        root_placement: Option<LabelPlacement>,
    ) {
        let Some(children) = node.get_mut("children").and_then(Value::as_array_mut) else {
            return;
        };
        for child in children {
            let Ok(id) = id_of(child) else { continue };
            let own = options_of(child);
            if let Some(r) = snapshot.boxes.get(&id) {
                child["x"] = json!(r.x);
                child["y"] = json!(r.y);
                child["width"] = json!(r.width);
                child["height"] = json!(r.height);
                let leaf = child
                    .get("children")
                    .and_then(Value::as_array)
                    .is_none_or(|c| c.is_empty());
                if let Some(label) = child
                    .get_mut("labels")
                    .and_then(Value::as_array_mut)
                    .and_then(|l| l.first_mut())
                {
                    let (lw, lh) = (
                        number(label, "width").unwrap_or(0.),
                        number(label, "height").unwrap_or(0.),
                    );
                    // By `nodeLabels.placement` (the label's, the node's, else the root's); without one, leaves have the label
                    // in the middle and containers inside the top left corner, above their children.
                    let placement = options_of(label)
                        .get("nodeLabels.placement")
                        .or(own.get("nodeLabels.placement"))
                        .and_then(|v| label_placement(v))
                        .or(root_placement);
                    let (x, y) = match placement {
                        Some(p) => label_offset(p, (r.width, r.height), (lw, lh)),
                        None if leaf => ((r.width - lw) / 2., (r.height - lh) / 2.),
                        None => (8., 4.),
                    };
                    label["x"] = json!(x);
                    label["y"] = json!(y);
                }
                let origin = absolute(&id);
                if let Some(list) = child.get_mut("ports").and_then(Value::as_array_mut) {
                    for port in list {
                        let Ok(pid) = id_of(port) else { continue };
                        if let Some(p) = port_points.get(&pid) {
                            let (pw, ph) = (
                                number(port, "width").unwrap_or(0.),
                                number(port, "height").unwrap_or(0.),
                            );
                            port["x"] = json!(p.x - origin.0 - pw / 2.);
                            port["y"] = json!(p.y - origin.1 - ph / 2.);
                        }
                    }
                }
            }
            write_edges(child, Some(&id));
            walk(
                child,
                snapshot,
                absolute,
                port_points,
                write_edges,
                root_placement,
            );
        }
    }
    let root_placement = options_of(elk)
        .get("nodeLabels.placement")
        .and_then(|v| label_placement(v));
    walk(
        &mut out,
        snapshot,
        &absolute,
        &port_points,
        &write_edges,
        root_placement,
    );
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        json!({
            "id": "root",
            "layoutOptions": {"elk.algorithm": "layered", "elk.direction": "DOWN", "elk.spacing.nodeNode": 20},
            "children": [
                {"id": "a", "width": 40, "height": 30, "labels": [{"text": "A", "width": 10, "height": 10}],
                 "ports": [{"id": "a_out", "width": 4, "height": 4, "layoutOptions": {"elk.port.side": "SOUTH"}}]},
                {"id": "group", "layoutOptions": {"elk.direction": "RIGHT"},
                 "children": [{"id": "b", "width": 40, "height": 30}, {"id": "c", "width": 40, "height": 30}],
                 "edges": [{"id": "bc", "sources": ["b"], "targets": ["c"]}]}
            ],
            "edges": [
                {"id": "ab", "sources": ["a_out"], "targets": ["b"], "labels": [{"text": "x", "width": 12, "height": 8}]},
                {"id": "hyper", "sources": ["a"], "targets": ["b", "c"]}
            ]
        })
    }

    #[test]
    fn translates_structure_ports_hyperedges_and_options() {
        let t = translate(&sample()).unwrap();
        assert_eq!(t.graph["defaultDirection"], "tb");
        assert_eq!(t.graph["containerDirections"]["group"], "lr");
        let ids: Vec<&str> = t.graph["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["id"].as_str().unwrap())
            .collect();
        assert!(ids.contains(&"hyper#0") && ids.contains(&"hyper#1") && ids.contains(&"bc"));
        assert_eq!(t.graph["ports"][0]["side"], "bottom");
        assert_eq!(t.graph["edgePorts"][0]["sourcePort"], "elk-port:a_out");
        assert_eq!(t.options["nodeSizes"]["a"]["width"], 40.);
        assert_eq!(t.options["edgeLabels"]["ab"]["text"], "x");
        assert_eq!(t.options["nodeSpacing"], 20.);
        assert!(!t.diagnostics.iter().any(|d| d.contains("spacing.nodeNode")));
        // The hyperedge with one source becomes a bus of its parts.
        assert!(!t.diagnostics.iter().any(|d| d.contains("hyperedges")));
        assert_eq!(t.graph["buses"][0]["edges"].as_array().unwrap().len(), 2);
        let graph: arqlet_model::Graph = serde_json::from_value(t.graph.clone()).unwrap();
        assert_eq!(graph.nodes.len(), 4);
    }

    #[test]
    fn unknown_endpoints_and_duplicate_ids_are_errors() {
        let mut bad = sample();
        bad["edges"][0]["targets"] = json!(["nowhere"]);
        assert!(translate(&bad).unwrap_err().contains("nowhere"));
        let mut twice = sample();
        twice["children"][1]["children"][1]["id"] = json!("b");
        assert!(translate(&twice).unwrap_err().contains("twice"));
    }

    #[test]
    fn algorithms_map_to_strategies_and_free_their_direction() {
        let t = translate(&json!({
            "id": "root",
            "children": [{"id": "g", "layoutOptions": {"org.eclipse.elk.algorithm": "org.eclipse.elk.force"},
                          "children": [{"id": "x", "width": 10, "height": 10}]}],
        }))
        .unwrap();
        assert_eq!(t.graph["containerStrategies"]["g"], "organic");
        assert_eq!(t.graph["containerDirections"]["g"], "auto");
        assert_eq!(t.graph["defaultDirection"], "lr");
    }

    #[test]
    fn port_constraints_and_indices_place_ports() {
        let node = |constraints: &str| {
            json!({
                "id": "root",
                "children": [{"id": "n", "width": 100, "height": 40,
                    "layoutOptions": {"elk.portConstraints": constraints},
                    "ports": [
                        {"id": "p1", "x": 75, "y": -2, "width": 4, "height": 4, "layoutOptions": {"elk.port.index": 1}},
                        {"id": "p0", "x": 15, "y": -2, "width": 4, "height": 4, "layoutOptions": {"elk.port.side": "NORTH", "elk.port.index": 0}}
                    ]}],
            })
        };
        // By position: p1 gets the side nearest it (top) and its place there.
        let t = translate(&node("FIXED_POS")).unwrap();
        let p1 = t.graph["ports"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["id"] == "elk-port:p1")
            .unwrap()
            .clone();
        assert_eq!(p1["side"], "top");
        assert!((p1["offset"].as_f64().unwrap() - 0.77).abs() < 1e-9);
        // Free: no explicit ports.
        assert!(translate(&node("FREE")).unwrap().graph["ports"]
            .as_array()
            .unwrap()
            .is_empty());
        // Fixed side: only the port with a side, in index order.
        let t = translate(&node("FIXED_SIDE")).unwrap();
        assert_eq!(t.graph["ports"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn node_label_placements() {
        let p = |v: &str| label_placement(v).unwrap();
        assert_eq!(
            label_offset(p("[H_CENTER, V_TOP, INSIDE]"), (100., 40.), (20., 10.)),
            (40., 5.)
        );
        assert_eq!(
            label_offset(p("H_LEFT V_BOTTOM"), (100., 40.), (20., 10.)),
            (5., 25.)
        );
        assert_eq!(
            label_offset(p("OUTSIDE V_BOTTOM H_CENTER"), (100., 40.), (20., 10.)),
            (40., 45.)
        );
        assert_eq!(
            label_offset(p("OUTSIDE H_RIGHT V_CENTER"), (100., 40.), (20., 10.)),
            (105., 15.)
        );
        assert_eq!(label_placement(""), None);
    }

    #[test]
    fn layered_spacings_are_read_under_their_official_names() {
        let elk = json!({"id": "root", "layoutOptions": {
            "elk.layered.spacing.nodeNodeBetweenLayers": "20",
            "org.eclipse.elk.layered.spacing.edgeEdgeBetweenLayers": 7,
            "elk.spacing.nodeNode": "15"},
            "children": [{"id": "a", "width": 10, "height": 10}, {"id": "b", "width": 10, "height": 10}],
            "edges": [{"id": "e", "sources": ["a"], "targets": ["b"]}]});
        let t = translate(&elk).unwrap();
        assert_eq!(t.options["layerSpacing"], json!(20.0));
        assert_eq!(t.options["nodeSpacing"], json!(15.0));
        assert_eq!(t.options["trackSpacing"], json!(7.0));
        assert!(
            t.diagnostics.iter().all(|d| !d.contains("spacing")),
            "{:?}",
            t.diagnostics
        );
    }

    #[test]
    fn separate_connected_components_is_read() {
        let graph = |options: Value| {
            json!({"id": "root", "layoutOptions": options,
            "children": [{"id": "a", "width": 10, "height": 10}, {"id": "b", "width": 10, "height": 10}], "edges": []})
        };
        // Not set, or false: nothing changes, and it is no longer reported as unknown.
        for options in [json!({}), json!({"elk.separateConnectedComponents": false})] {
            let t = translate(&graph(options)).unwrap();
            assert!(t.options.get("componentPacking").is_none());
            assert!(t
                .diagnostics
                .iter()
                .all(|d| !d.contains("separateConnectedComponents")));
        }
        let on = translate(&graph(json!({"elk.separateConnectedComponents": true}))).unwrap();
        assert_eq!(on.options["componentPacking"], json!(true));
        // Mostly unconnected nodes go into a grid unless the graph turns the separation off.
        let row = |options: Value| {
            json!({"id": "root", "layoutOptions": options,
            "children": (0..6).map(|i| json!({"id": format!("n{i}"), "width": 10, "height": 10})).collect::<Vec<_>>(),
            "edges": [{"id": "e", "sources": ["n0"], "targets": ["n1"]}]})
        };
        assert_eq!(
            translate(&row(json!({}))).unwrap().options["isolatedGrid"],
            json!(true)
        );
        assert!(
            translate(&row(json!({"elk.separateConnectedComponents": false})))
                .unwrap()
                .options
                .get("isolatedGrid")
                .is_none()
        );
        let connected = json!({"id": "root", "children": (0..6).map(|i| json!({"id": format!("n{i}"), "width": 10, "height": 10})).collect::<Vec<_>>(),
            "edges": (0..5).map(|i| json!({"id": format!("e{i}"), "sources": [format!("n{i}")], "targets": [format!("n{}", i + 1)]})).collect::<Vec<_>>()});
        assert!(translate(&connected)
            .unwrap()
            .options
            .get("isolatedGrid")
            .is_none());
        let bad = translate(&graph(json!({"elk.separateConnectedComponents": "maybe"}))).unwrap();
        assert!(bad
            .diagnostics
            .iter()
            .any(|d| d.contains("separateConnectedComponents")));
    }
}

/// Provisional: whether the graph asks for the input order to decide ties (`layered.considerModelOrder.strategy` other
/// than `NONE` on the root), as ELK follows its model order. Arqlet breaks ties by id; `in_model_order` makes its ids
/// follow the input order for such a graph.
pub fn wants_model_order(elk: &Value) -> bool {
    options_of(elk).iter().any(|(k, v)| {
        matches!(
            k.as_str(),
            "layered.considerModelOrder.strategy" | "considerModelOrder.strategy"
        ) && v.trim_matches('"') != "NONE"
    })
}

/// The prefix of the ids `in_model_order` gives; `restore_ids` turns them back.
const ORDER_PREFIX: &str = "arqlet-order-";

/// A copy of the graph whose nodes, ports and edges have ids in input order (`arqlet-order-00000000`, …), so that ties
/// broken by id follow the input; with the map from the new ids back to the given ones. Edge ends follow the renaming.
pub fn in_model_order(elk: &Value) -> (Value, BTreeMap<String, String>) {
    fn assign(node: &Value, next: &mut usize, map: &mut BTreeMap<String, String>) {
        for child in node["children"].as_array().into_iter().flatten() {
            for item in
                std::iter::once(child).chain(child["ports"].as_array().into_iter().flatten())
            {
                if let Some(id) = item["id"].as_str() {
                    map.insert(id.to_string(), format!("{ORDER_PREFIX}{next:08}"));
                    *next += 1;
                }
            }
            assign(child, next, map);
        }
        for edge in node["edges"].as_array().into_iter().flatten() {
            if let Some(id) = edge["id"].as_str() {
                map.insert(id.to_string(), format!("{ORDER_PREFIX}{next:08}"));
                *next += 1;
            }
        }
    }
    fn rename(node: &mut Value, map: &BTreeMap<String, String>) {
        let swap = |v: &mut Value| {
            if let Some(new) = v.as_str().and_then(|id| map.get(id)) {
                *v = Value::String(new.clone());
            }
        };
        if let Some(children) = node["children"].as_array_mut() {
            for child in children {
                swap(&mut child["id"]);
                if let Some(ports) = child["ports"].as_array_mut() {
                    for port in ports {
                        swap(&mut port["id"]);
                    }
                }
                rename(child, map);
            }
        }
        if let Some(edges) = node["edges"].as_array_mut() {
            for edge in edges {
                swap(&mut edge["id"]);
                for end in ["sources", "targets"] {
                    if let Some(ends) = edge[end].as_array_mut() {
                        ends.iter_mut().for_each(swap);
                    }
                }
            }
        }
    }
    let mut map = BTreeMap::new();
    assign(elk, &mut 0, &mut map);
    let mut out = elk.clone();
    rename(&mut out, &map);
    let back = map.into_iter().map(|(old, new)| (new, old)).collect();
    (out, back)
}

/// Turns the ids `in_model_order` gave back into the given ones, in a laid-out graph (nodes, ports, edges, their ends and
/// the shapes their sections name) and in text (diagnostics).
pub fn restore_ids(out: &mut Value, back: &BTreeMap<String, String>) {
    match out {
        Value::String(s) => {
            if let Some(old) = back.get(s.as_str()) {
                *s = old.clone();
            } else if s.contains(ORDER_PREFIX) {
                for (new, old) in back.iter().rev() {
                    if s.contains(new.as_str()) {
                        *s = s.replace(new.as_str(), old);
                    }
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| restore_ids(v, back)),
        Value::Object(map) => map.values_mut().for_each(|v| restore_ids(v, back)),
        _ => {}
    }
}
