//! Provisional: a backbone bus (`Graph::buses`, edges with several sources and several targets) as an element of the layout
//! (ADR 0005 amendment). Before the layout, the bus becomes a thin virtual node in the area that holds all its members' ends,
//! and every member `s → t` becomes `s → bus` and `bus → t`, so the ordering, the placement and the routing see the bus from
//! the start. After the layout, the canonical graph and options come back: the two parts of every member are joined, the
//! bus's box is kept in `Snapshot::bus_boxes`, and the members are drawn on one line where the layout put the bus
//! (`bus::draw_buses` with that line as the trunk). The canonical graph is never changed; only the layout sees the virtual
//! node. Fans (one shared end) stay with the drawing step alone.

use crate::{direction_for, Mode};
use arqlet_model::{Edge, EdgePorts, Graph, LayoutOptions, Node, NodeSize, Snapshot};
use std::collections::{BTreeMap, BTreeSet};

/// The prefix of a bus's virtual node.
pub(crate) const BUS_NODE_PREFIX: &str = "\u{1}bus:";
const IN_SUFFIX: &str = "\u{1}in";
const OUT_SUFFIX: &str = "\u{1}out";
/// The thin side of a bus node (across the flow it is thin, along the ranks it is a short line).
const THICKNESS: f64 = 8.;
const LENGTH: f64 = 40.;

/// What the expansion did, to undo it after the layout.
#[derive(Clone, Debug, Default)]
pub(crate) struct Expansion {
    /// By bus id: its virtual node and its members.
    pub buses: BTreeMap<String, (String, Vec<String>)>,
}

impl Expansion {
    pub fn is_empty(&self) -> bool {
        self.buses.is_empty()
    }
}

/// The graph and options the layout sees: every backbone bus as a virtual node with its members split at it. `None` when
/// the graph has no backbone bus that can be placed (its members' ends must lie in areas of one fixed flow).
pub(crate) fn expand(
    graph: &Graph,
    options: &LayoutOptions,
) -> Option<(Graph, LayoutOptions, Expansion)> {
    if graph.buses.is_empty() {
        return None;
    }
    let parent: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let chain = |id: &str| -> Vec<Option<String>> {
        let mut out = Vec::new();
        let mut at = parent.get(id).copied().flatten();
        loop {
            out.push(at.map(str::to_string));
            match at {
                Some(a) => at = parent.get(a).copied().flatten(),
                None => break,
            }
        }
        out
    };
    let edges: BTreeMap<&str, &Edge> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut expanded = graph.clone();
    let mut layout_options = options.clone();
    let mut expansion = Expansion::default();
    let mut kept_buses = Vec::new();
    let mut split: BTreeSet<String> = BTreeSet::new();
    for bus in &graph.buses {
        let members: Vec<&Edge> = bus
            .edges
            .iter()
            .filter_map(|id| edges.get(id.as_str()).copied())
            .filter(|e| e.target.as_deref().is_some_and(|t| t != e.source))
            .filter(|e| !split.contains(&e.id))
            .collect();
        let sources: BTreeSet<&str> = members.iter().map(|e| e.source.as_str()).collect();
        let targets: BTreeSet<&str> = members.iter().filter_map(|e| e.target.as_deref()).collect();
        // A bus with fewer than two drawn members, or one with a node on both sides, stays with the drawing step.
        // A node on both sides (a gateway that receives and sends on the bus) would close a cycle through one bus node; such
        // a bus is drawn by the drawing step, one line per channel group.
        // A fan (one shared end) stays with the drawing step: as a node of its own with a rank of its own it was measured
        // and not kept (every fan-out of the corpus as a bus: three times the crossings, `benchmarks/results/bus-20261108.md`).
        if members.len() < 2
            || sources.len() < 2
            || targets.len() < 2
            || !sources.is_disjoint(&targets)
        {
            kept_buses.push(bus.clone());
            continue;
        }
        // The area that holds every member's ends: the deepest area common to all their ancestor chains.
        let chains: Vec<Vec<Option<String>>> = members
            .iter()
            .flat_map(|e| {
                [
                    chain(&e.source),
                    chain(e.target.as_deref().expect("a target")),
                ]
            })
            .collect();
        let area = chains[0]
            .iter()
            .find(|a| chains.iter().all(|c| c.contains(a)))
            .cloned()
            .flatten();
        // The bus lies across the area's flow; an area without a fixed flow keeps the drawing step alone.
        let Mode::Orientation(o) = direction_for(graph, area.as_deref()) else {
            kept_buses.push(bus.clone());
            continue;
        };
        let node = format!("{BUS_NODE_PREFIX}{}", bus.id);
        if graph.nodes.iter().any(|n| n.id == node) {
            kept_buses.push(bus.clone());
            continue;
        }
        let virtual_node = Node {
            id: node.clone(),
            kind: "bus".into(),
            label: bus.id.clone(),
            parent_id: area.clone(),
            shape: Default::default(),
        };
        expanded.nodes.push(virtual_node);
        let (width, height) = if o.vertical() {
            (LENGTH, THICKNESS)
        } else {
            (THICKNESS, LENGTH)
        };
        layout_options
            .node_sizes
            .insert(node.clone(), NodeSize { width, height });
        for e in &members {
            split.insert(e.id.clone());
        }
        expansion.buses.insert(
            bus.id.clone(),
            (node, members.iter().map(|e| e.id.clone()).collect()),
        );
    }
    if expansion.is_empty() {
        return None;
    }
    // Every split member becomes two edges through its bus's node, with its ports and label on the part at that end.
    let node_of: BTreeMap<&str, &str> = expansion
        .buses
        .values()
        .flat_map(|(node, members)| members.iter().map(move |m| (m.as_str(), node.as_str())))
        .collect();
    expanded.edges = graph
        .edges
        .iter()
        .flat_map(|e| match node_of.get(e.id.as_str()) {
            None => vec![e.clone()],
            Some(&node) => {
                let mut into = e.clone();
                into.id = format!("{}{IN_SUFFIX}", e.id);
                into.target = Some(node.to_string());
                let mut out = e.clone();
                out.id = format!("{}{OUT_SUFFIX}", e.id);
                out.source = node.to_string();
                vec![into, out]
            }
        })
        .collect();
    expanded.edge_ports = graph
        .edge_ports
        .iter()
        .flat_map(|p| match node_of.get(p.edge_id.as_str()) {
            None => vec![p.clone()],
            Some(_) => vec![
                EdgePorts {
                    edge_id: format!("{}{IN_SUFFIX}", p.edge_id),
                    source_port: p.source_port.clone(),
                    target_port: None,
                },
                EdgePorts {
                    edge_id: format!("{}{OUT_SUFFIX}", p.edge_id),
                    source_port: None,
                    target_port: p.target_port.clone(),
                },
            ],
        })
        .filter(|p| p.source_port.is_some() || p.target_port.is_some())
        .collect();
    expanded.buses = kept_buses;
    for member in node_of.keys() {
        if let Some(label) = layout_options.edge_labels.remove(*member) {
            layout_options
                .edge_labels
                .insert(format!("{member}{IN_SUFFIX}"), label);
        }
    }
    Some((expanded, layout_options, expansion))
}

/// The earlier snapshot as the expanded layout saw it, so that an incremental layout continues it: the expanded graph and
/// the bus nodes' boxes back in place.
pub(crate) fn expand_previous(previous: &Snapshot, graph: &Graph) -> Snapshot {
    let mut out = previous.clone();
    out.graph = graph.clone();
    for (id, rect) in &previous.bus_boxes {
        out.boxes.insert(id.clone(), rect.clone());
    }
    out
}

/// The canonical snapshot from the expanded layout: the canonical graph and options, the bus nodes' boxes apart, and every
/// member's two parts joined (its source's port and its target's port).
pub(crate) fn collapse(
    mut snapshot: Snapshot,
    graph: &Graph,
    options: &LayoutOptions,
    expansion: &Expansion,
) -> Snapshot {
    let members: BTreeSet<&str> = expansion
        .buses
        .values()
        .flat_map(|(_, m)| m.iter().map(String::as_str))
        .collect();
    let original = |id: &str| -> Option<(String, bool)> {
        if let Some(m) = id.strip_suffix(IN_SUFFIX) {
            return members.contains(m).then(|| (m.to_string(), true));
        }
        if let Some(m) = id.strip_suffix(OUT_SUFFIX) {
            return members.contains(m).then(|| (m.to_string(), false));
        }
        None
    };
    snapshot.bus_boxes = expansion
        .buses
        .values()
        .filter_map(|(node, _)| snapshot.boxes.remove(node).map(|r| (node.clone(), r)))
        .collect();
    // The routes: the part into the bus, then the part out of it.
    let mut parts: BTreeMap<String, (Vec<arqlet_model::Point>, Vec<arqlet_model::Point>)> =
        BTreeMap::new();
    let mut routes = Vec::new();
    for route in std::mem::take(&mut snapshot.routes) {
        match original(&route.id) {
            Some((member, true)) => parts.entry(member).or_default().0 = route.points,
            Some((member, false)) => parts.entry(member).or_default().1 = route.points,
            None => routes.push(route),
        }
    }
    for (member, (into, out)) in parts {
        let mut points = into;
        points.extend(out);
        routes.push(arqlet_model::Route::new(member, points));
    }
    let order: BTreeMap<&str, usize> = graph
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    routes.sort_by_key(|r| order.get(r.id.as_str()).copied().unwrap_or(usize::MAX));
    snapshot.routes = routes;
    // The ports: the source end of the part into the bus, the target end of the part out of it.
    snapshot
        .resolved_ports
        .retain_mut(|p| match original(&p.edge_id) {
            Some((member, into)) => {
                let keep = (into && p.endpoint == "source") || (!into && p.endpoint == "target");
                if keep {
                    p.edge_id = member;
                }
                keep
            }
            None => true,
        });
    snapshot.boundary_crossings.retain_mut(|c| {
        if let Some((member, _)) = original(&c.edge_id) {
            c.edge_id = member;
        }
        true
    });
    for id in &mut snapshot.straight_edges {
        if let Some((member, _)) = original(id) {
            *id = member;
        }
    }
    snapshot.straight_edges.dedup();
    snapshot.graph = graph.clone();
    snapshot.options = options.clone();
    snapshot
}

/// The trunk of every placed bus: the middle of its node's box along the flow, by bus id (absolute coordinates).
pub(crate) fn trunks(snapshot: &Snapshot) -> BTreeMap<String, f64> {
    let rects = crate::absolute_all(&snapshot.graph, &snapshot.boxes);
    let mut out = BTreeMap::new();
    for bus in &snapshot.graph.buses {
        let node = format!("{BUS_NODE_PREFIX}{}", bus.id);
        let Some(r) = snapshot.bus_boxes.get(&node) else {
            continue;
        };
        let parent = expansion_parent(snapshot, &bus.edges);
        let origin = parent
            .as_deref()
            .and_then(|p| rects.get(p))
            .map_or((0., 0.), |p| (p.x, p.y));
        let vertical = r.width >= r.height;
        out.insert(
            bus.id.clone(),
            if vertical {
                origin.1 + r.y + r.height / 2.
            } else {
                origin.0 + r.x + r.width / 2.
            },
        );
    }
    out
}

/// The area of a bus's node: the deepest area common to its members' ends (as `expand` chose it).
fn expansion_parent(snapshot: &Snapshot, members: &[String]) -> Option<String> {
    let parent: BTreeMap<&str, Option<&str>> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let chain = |id: &str| -> Vec<Option<String>> {
        let mut out = Vec::new();
        let mut at = parent.get(id).copied().flatten();
        loop {
            out.push(at.map(str::to_string));
            match at {
                Some(a) => at = parent.get(a).copied().flatten(),
                None => break,
            }
        }
        out
    };
    let chains: Vec<Vec<Option<String>>> = snapshot
        .graph
        .edges
        .iter()
        .filter(|e| members.contains(&e.id))
        .flat_map(|e| {
            let mut c = vec![chain(&e.source)];
            if let Some(t) = e.target.as_deref() {
                c.push(chain(t));
            }
            c
        })
        .collect();
    let first = chains.first()?;
    first
        .iter()
        .find(|a| chains.iter().all(|c| c.contains(a)))
        .cloned()
        .flatten()
}
