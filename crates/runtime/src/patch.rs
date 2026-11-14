//! Provisional: the `patch` operation. Changes the source graph of a view without a new import: adds nodes and edges, and
//! removes nodes (each with what it holds and the edges at them) and edges. The view keeps its state (collapsed
//! containers, filter, options). An abstracting view (level of detail, communities, groups) is laid out from scratch, as a
//! toggle lays it out, so the drawing is the one a fresh import of the changed graph gets with the same view; other views
//! are laid out again from their earlier positions. A graph change: the revision and the layout version advance; stale
//! requests are refused.
use crate::{abstracts, project_view, view_options, View};
use arqlet_model::{validate, GraphPatch, Snapshot};
use std::collections::{BTreeMap, BTreeSet};

pub fn patch_snapshot(
    snapshot: &Snapshot,
    expected_revision: u64,
    expected_layout_version: u64,
    patch: &GraphPatch,
) -> Result<Snapshot, String> {
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    if snapshot.revision != expected_revision || snapshot.layout_version != expected_layout_version
    {
        return Err("Stale edit revision".into());
    }
    if patch.is_empty() {
        return Ok(snapshot.clone());
    }
    let mut graph = snapshot
        .source_graph
        .clone()
        .unwrap_or_else(|| snapshot.graph.clone());
    // Removals: the named nodes with everything below them, the edges at them, and the named edges.
    let known_nodes: BTreeSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    if let Some(id) = patch
        .remove_nodes
        .iter()
        .find(|id| !known_nodes.contains(id.as_str()))
    {
        return Err(format!("Unknown node: {id}"));
    }
    let known_edges: BTreeSet<&str> = graph.edges.iter().map(|e| e.id.as_str()).collect();
    if let Some(id) = patch
        .remove_edges
        .iter()
        .find(|id| !known_edges.contains(id.as_str()))
    {
        return Err(format!("Unknown edge: {id}"));
    }
    let parent: BTreeMap<String, Option<String>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.parent_id.clone()))
        .collect();
    let named: BTreeSet<&str> = patch.remove_nodes.iter().map(String::as_str).collect();
    let removed: BTreeSet<String> = graph
        .nodes
        .iter()
        .filter(|n| {
            let mut at = Some(n.id.as_str());
            while let Some(x) = at {
                if named.contains(x) {
                    return true;
                }
                at = parent.get(x).and_then(|p| p.as_deref());
            }
            false
        })
        .map(|n| n.id.clone())
        .collect();
    let removed_edges: BTreeSet<&str> = patch.remove_edges.iter().map(String::as_str).collect();
    graph.nodes.retain(|n| !removed.contains(&n.id));
    graph.edges.retain(|e| {
        !removed_edges.contains(e.id.as_str())
            && !removed.contains(&e.source)
            && e.target.as_ref().is_none_or(|t| !removed.contains(t))
    });
    // Additions: new ids only; an edge without a target is left out, as at an import.
    let mut omitted = snapshot.omitted_unresolved_edges;
    let ids: BTreeSet<String> = graph.nodes.iter().map(|n| n.id.clone()).collect();
    for node in &patch.nodes {
        if ids.contains(&node.id) {
            return Err(format!("Node exists: {}", node.id));
        }
        graph.nodes.push(node.clone());
    }
    let edge_ids: BTreeSet<String> = graph.edges.iter().map(|e| e.id.clone()).collect();
    for edge in &patch.edges {
        if edge_ids.contains(&edge.id) {
            return Err(format!("Edge exists: {}", edge.id));
        }
        if edge.target.is_none() {
            omitted += 1;
            continue;
        }
        graph.edges.push(edge.clone());
    }
    let kept_edges: BTreeSet<String> = graph.edges.iter().map(|e| e.id.clone()).collect();
    graph.ports.retain(|p| !removed.contains(&p.node_id));
    graph.edge_ports.retain(|b| kept_edges.contains(&b.edge_id));
    for bus in &mut graph.buses {
        bus.edges.retain(|e| kept_edges.contains(e));
    }
    graph.buses.retain(|b| !b.edges.is_empty());
    validate(&graph)?;
    // The view state that names removed nodes forgets them.
    let mut options = snapshot.options.clone();
    for id in &removed {
        options.pins.remove(id);
        options.order_hints.remove(id);
        options.container_sizing.remove(id);
        options.node_sizes.remove(id);
        options.node_rows.remove(id);
        options.groups.remove(id);
        options.partial.remove(id);
    }
    options.hidden.nodes.retain(|id| !removed.contains(id));
    // Collapsed containers that still are containers of the view (communities and groups included) stay collapsed.
    let structured = crate::structured(&graph, &options)?;
    let containers: BTreeSet<&str> = structured
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let collapsed: Vec<String> = snapshot
        .collapsed_ids
        .iter()
        .filter(|id| containers.contains(id.as_str()))
        .cloned()
        .collect();
    drop(structured);
    let View {
        graph: view,
        collapsed,
        projected,
        weights,
    } = project_view(&graph, &collapsed, &options, abstracts(&options))?;
    let layout_options = if abstracts(&options) {
        view_options(&view, &collapsed, &options, &weights)
    } else {
        options
    };
    // An abstracting view (level of detail, communities, groups) is laid out from scratch, as a toggle lays it out: there a
    // warm start was slower and worse (benchmarks/results/toggle-layout-20261112.md; a patch that loaded 1 173 member
    // edges into an opened container gave a drawing 45 times taller from the earlier places), and the drawing depends
    // only on the graph and the view. Otherwise every node is placed again, starting from where the earlier layout put it.
    let mut result = if abstracts(&layout_options) {
        arqlet_layout::compute(&projected, None, &BTreeSet::new(), &layout_options)?
    } else {
        let dirty: BTreeSet<String> = projected.nodes.iter().map(|n| n.id.clone()).collect();
        arqlet_layout::compute(&projected, Some(snapshot), &dirty, &layout_options)?
    };
    result.revision = snapshot.revision + 1;
    result.base_revision = Some(snapshot.revision);
    result.layout_version = snapshot
        .layout_version
        .checked_add(1)
        .ok_or("Layout version overflow")?;
    result.source_graph = Some(graph);
    result.collapsed_ids = collapsed.into_iter().collect();
    result.omitted_unresolved_edges = omitted;
    Ok(result)
}
