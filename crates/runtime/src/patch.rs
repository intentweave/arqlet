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
    // A change of memberships only moves nothing (ADR 0022 §5b) as long as no changed dimension can own nodes: a dimension
    // held softly is drawn from the boxes the layout already has, so the view is answered from the layout it has, with the
    // new revision. A dimension that owns nodes under the grouping priority (M1) shapes the view's tree, so a change of it
    // takes the full path below and is projected again.
    let touched = patch
        .memberships
        .iter()
        .flat_map(|m| m.dimensions.iter().map(|d| d.id.as_str()))
        .chain(
            patch
                .remove_memberships
                .iter()
                .map(|r| r.dimension.as_str()),
        );
    let shapes_the_tree = touched
        .into_iter()
        .any(|d| arqlet_model::grouping::can_own(&snapshot.options.grouping_priority, d));
    if patch.memberships_only() && !shapes_the_tree {
        let source = snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph);
        let mut graph = source.clone();
        graph.memberships = patched_memberships(&graph.memberships, patch)?;
        validate(&graph)?;
        let mut result = snapshot.clone();
        result.graph.memberships = graph.memberships.clone();
        if snapshot.source_graph.is_some() {
            result.source_graph = Some(graph.into());
        }
        // The kept structure names the earlier source graph; the next projection builds it again.
        result.structure = None;
        result.revision = snapshot.revision + 1;
        result.base_revision = Some(snapshot.revision);
        result.layout_version = snapshot
            .layout_version
            .checked_add(1)
            .ok_or("Layout version overflow")?;
        return Ok(result);
    }
    let mut graph = snapshot
        .source_graph
        .as_deref()
        .unwrap_or(&snapshot.graph)
        .clone();
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
    // Provisional (ADR 0022 §5b): the memberships changed by dimension and group; validated with the graph below.
    graph.memberships = patched_memberships(&graph.memberships, patch)?;
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
    result.source_graph = Some(graph.into());
    result.collapsed_ids = collapsed.into_iter().collect();
    result.omitted_unresolved_edges = omitted;
    Ok(result)
}

/// The memberships after a patch: `removeMemberships` first (a whole dimension, or a group with the groups nested in it;
/// unknown ones refused), then `memberships` (a dimension of the same id takes the groups given, replacing a group of the
/// same id and appending new ones, and a label if one is given; a new dimension is appended).
fn patched_memberships(
    old: &arqlet_model::Memberships,
    patch: &GraphPatch,
) -> Result<arqlet_model::Memberships, String> {
    let mut out = old.clone();
    for removal in &patch.remove_memberships {
        let at = out
            .dimensions
            .iter()
            .position(|d| d.id == removal.dimension)
            .ok_or_else(|| format!("Unknown membership dimension: {}", removal.dimension))?;
        match &removal.group {
            None => {
                out.dimensions.remove(at);
            }
            Some(group) => {
                let dimension = &mut out.dimensions[at];
                if !dimension.groups.iter().any(|g| &g.id == group) {
                    return Err(format!(
                        "Unknown membership group: {}/{group}",
                        removal.dimension
                    ));
                }
                // The group and every group nested in it, by walking up each group's parents.
                let parent: BTreeMap<String, Option<String>> = dimension
                    .groups
                    .iter()
                    .map(|g| (g.id.clone(), g.parent.clone()))
                    .collect();
                let inside = |id: &str| {
                    let mut at = Some(id.to_string());
                    let mut steps = 0usize;
                    while let Some(current) = at {
                        if &current == group {
                            return true;
                        }
                        steps += 1;
                        if steps > parent.len() {
                            return false;
                        }
                        at = parent.get(&current).cloned().flatten();
                    }
                    false
                };
                dimension.groups.retain(|g| !inside(&g.id));
            }
        }
    }
    if let Some(added) = &patch.memberships {
        for dimension in &added.dimensions {
            match out.dimensions.iter_mut().find(|d| d.id == dimension.id) {
                Some(existing) => {
                    if !dimension.label.is_empty() {
                        existing.label = dimension.label.clone();
                    }
                    for group in &dimension.groups {
                        match existing.groups.iter_mut().find(|g| g.id == group.id) {
                            Some(g) => *g = group.clone(),
                            None => existing.groups.push(group.clone()),
                        }
                    }
                }
                None => out.dimensions.push(dimension.clone()),
            }
        }
    }
    Ok(out)
}
