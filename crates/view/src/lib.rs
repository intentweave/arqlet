//! Lossless view projection: canonical graph is owned by the runtime snapshot.
use arqlet_model::*;
use std::collections::{BTreeMap, BTreeSet};
pub fn project(source: &Graph, collapsed: &BTreeSet<String>) -> Result<Graph, String> {
    validate(source)?;
    let nodes: BTreeMap<_, _> = source.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    for id in collapsed {
        if !nodes.contains_key(id.as_str())
            || !source
                .nodes
                .iter()
                .any(|n| n.parent_id.as_ref() == Some(id))
        {
            return Err(format!("Not a container: {id}"));
        }
    }
    let owner = |id: &str| {
        let mut current = nodes[id].parent_id.as_deref();
        let mut visible = id.to_string();
        while let Some(p) = current {
            if collapsed.contains(p) {
                visible = p.into();
            }
            current = nodes[p].parent_id.as_deref();
        }
        visible
    };
    let visible: BTreeSet<_> = source
        .nodes
        .iter()
        .filter(|n| owner(&n.id) == n.id)
        .map(|n| n.id.clone())
        .collect();
    let mut graph = source.clone();
    graph.nodes.retain(|n| visible.contains(&n.id));
    graph.edges = source
        .edges
        .iter()
        .filter_map(|e| {
            let mut edge = e.clone();
            edge.source = owner(&e.source);
            edge.target = e.target.as_deref().map(owner);
            // Hide connections internal to an aggregate; retain true visible self-loops.
            if edge.target.as_ref() == Some(&edge.source)
                && (edge.source != e.source || edge.target != e.target)
            {
                return None;
            }
            Some(edge)
        })
        .collect();
    graph.ports.retain(|port| visible.contains(&port.node_id));
    graph.edge_ports = source
        .edge_ports
        .iter()
        .filter_map(|b| {
            let old = source.edges.iter().find(|e| e.id == b.edge_id)?;
            let new = graph.edges.iter().find(|e| e.id == b.edge_id)?;
            let mut binding = b.clone();
            if old.source != new.source {
                binding.source_port = None;
            }
            if old.target != new.target {
                binding.target_port = None;
            }
            Some(binding)
        })
        .collect();
    validate(&graph)?;
    Ok(graph)
}

/// Provisional (global layering, iteration 1): the projection with the edges that a collapsed container aggregates drawn
/// once per visible pair. An edge whose end was moved to a collapsed container is kept only when no earlier edge (in the
/// projection's order) joins the same visible pair in the same direction; edges between nodes that are both visible as
/// themselves are all kept, parallel ones included. The canonical graph is unchanged.
pub fn merge_aggregated(source: &Graph, mut projected: Graph) -> Graph {
    let original: BTreeMap<&str, (&str, Option<&str>)> = source
        .edges
        .iter()
        .map(|e| (e.id.as_str(), (e.source.as_str(), e.target.as_deref())))
        .collect();
    let mut seen: BTreeSet<(String, Option<String>)> = BTreeSet::new();
    projected.edges.retain(|e| {
        let aggregated = original
            .get(e.id.as_str())
            .is_some_and(|(s, t)| *s != e.source || *t != e.target.as_deref());
        !aggregated || seen.insert((e.source.clone(), e.target.clone()))
    });
    let kept: BTreeSet<&str> = projected.edges.iter().map(|e| e.id.as_str()).collect();
    let bindings: Vec<_> = projected
        .edge_ports
        .iter()
        .filter(|b| kept.contains(b.edge_id.as_str()))
        .cloned()
        .collect();
    projected.edge_ports = bindings;
    projected
}
