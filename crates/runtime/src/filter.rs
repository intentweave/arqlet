//! Hiding by kind or by node (provisional, Request 2 of an external evaluation): the `filter` operation sets what a view
//! hides (`LayoutOptions::hidden`), and the scene leaves it out. The layout keeps the room of what is hidden, so what
//! stays keeps its place: no new import, no lost mental map. An edge that stands for collapsed content counts only the
//! edges still shown, and goes when none is left.
use arqlet_model::{Hidden, Snapshot};
use arqlet_scene::Scene;
use std::collections::{BTreeMap, BTreeSet};

/// Sets what the view hides; `hidden` replaces what was hidden before (empty: show everything). The geometry stays as it
/// is; the layout version counts the change, so that a scene of the old view is recognised as stale. A view request:
/// stale revisions are refused, and an unknown node is refused.
pub fn filter_view(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    hidden: Hidden,
) -> Result<Snapshot, String> {
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    if snapshot.revision != expected || snapshot.layout_version != layout {
        return Err("Stale view request".into());
    }
    let canonical = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    // Known nodes: those of the view with nothing hidden (a compacting filter leaves the hidden out of the view).
    let mut shown_all = snapshot.options.clone();
    shown_all.hidden = Hidden::default();
    let view = crate::structured(canonical, &shown_all)?;
    let known: BTreeSet<&str> = view.nodes.iter().map(|n| n.id.as_str()).collect();
    if let Some(id) = hidden.nodes.iter().find(|id| !known.contains(id.as_str())) {
        return Err(format!("Unknown node: {id}"));
    }
    if snapshot.options.hidden == hidden {
        return Ok(snapshot.clone());
    }
    // Compacting (now or before): the view is laid out again without what is hidden; the rest closes up.
    if hidden.compacts() || snapshot.options.hidden.compacts() {
        let mut options = snapshot.options.clone();
        options.hidden = hidden;
        return crate::recompute_layout(snapshot, &options, None);
    }
    let mut next = snapshot.clone();
    next.options.hidden = hidden;
    next.layout_version += 1;
    Ok(next)
}

/// The scene of a snapshot, without what its view hides.
pub fn scene_of(snapshot: &Snapshot) -> Result<Scene, String> {
    let mut scene = arqlet_scene::build_scene(snapshot)?;
    if !snapshot.options.hidden.is_empty() {
        hide(snapshot, &mut scene)?;
    }
    Ok(scene)
}

/// Leaves out of `scene` what the snapshot's view hides. The bounds stay, so that the drawing does not jump.
fn hide(snapshot: &Snapshot, scene: &mut Scene) -> Result<(), String> {
    let hidden = &snapshot.options.hidden;
    let canonical = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    // The whole view (with the containers it adds), and the nodes it shows now.
    let view = crate::structured(canonical, &snapshot.options)?;
    let parent: BTreeMap<&str, Option<&str>> = view
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let kind: BTreeMap<&str, &str> = view
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.kind.as_str()))
        .collect();
    let named: BTreeSet<&str> = hidden.nodes.iter().map(String::as_str).collect();
    let kinds: BTreeSet<&str> = hidden.node_kinds.iter().map(String::as_str).collect();
    let edge_kinds: BTreeSet<&str> = hidden.edge_kinds.iter().map(String::as_str).collect();
    // A node is hidden when it or a container above it is named or of a hidden kind.
    let is_hidden = |id: &str| {
        let mut at = Some(id);
        while let Some(x) = at {
            if named.contains(x) || kind.get(x).is_some_and(|k| kinds.contains(k)) {
                return true;
            }
            at = parent.get(x).copied().flatten();
        }
        false
    };
    let shown: BTreeSet<&str> = snapshot.graph.nodes.iter().map(|n| n.id.as_str()).collect();
    // The node that stands for `id` in the view now: itself, or the collapsed container that holds it.
    let shown_as = |id: &str| -> Option<String> {
        let mut at = Some(id);
        while let Some(x) = at {
            if shown.contains(x) {
                return Some(x.to_string());
            }
            at = parent.get(x).copied().flatten();
        }
        None
    };
    // The edges still shown, counted between the nodes that stand for their ends.
    let mut count: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut edge_kind: BTreeMap<&str, &str> = BTreeMap::new();
    let mut edge_count: BTreeMap<&str, usize> = BTreeMap::new();
    for edge in &canonical.edges {
        edge_kind.insert(edge.id.as_str(), edge.kind.as_str());
        edge_count.insert(edge.id.as_str(), edge.count());
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        if edge_kinds.contains(edge.kind.as_str()) || is_hidden(&edge.source) || is_hidden(target) {
            continue;
        }
        if let (Some(a), Some(b)) = (shown_as(&edge.source), shown_as(target)) {
            // By weight (`Edge::weight`); saturating, as `usize` is 32-bit in WASM.
            let n = count.entry((a, b)).or_default();
            *n = n.saturating_add(edge.count());
        }
    }
    let collapsed: BTreeSet<&str> = snapshot.collapsed_ids.iter().map(String::as_str).collect();
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    scene.routes.retain_mut(|route| {
        let keep = if is_hidden(&route.source) || is_hidden(&route.target) {
            false
        } else if collapsed.contains(route.source.as_str())
            || collapsed.contains(route.target.as_str())
        {
            // It stands for collapsed content: the edges still shown between its ends, in both directions.
            let forward = count
                .get(&(route.source.clone(), route.target.clone()))
                .copied()
                .unwrap_or(0);
            let backward = count
                .get(&(route.target.clone(), route.source.clone()))
                .copied()
                .unwrap_or(0);
            if forward + backward == 0 {
                false
            } else {
                route.weight = forward.max(1);
                if route.back_weight.is_some() {
                    route.back_weight = Some(backward);
                }
                true
            }
        } else {
            !edge_kinds.contains(route.kind.as_str())
        };
        if keep && !route.members.is_empty() {
            let before = route.members.len();
            route.members.retain(|m| {
                !edge_kind
                    .get(m.as_str())
                    .is_some_and(|k| edge_kinds.contains(k))
            });
            // A bundle that lost members weighs what it still carries.
            if route.members.len() < before {
                let of = |id: &str| edge_count.get(id).copied().unwrap_or(1);
                route.weight = route
                    .members
                    .iter()
                    .fold(of(&route.id), |w, m| w.saturating_add(of(m)));
            }
        } else if !keep {
            dropped.insert(route.id.clone());
        }
        keep
    });
    scene.nodes.retain(|n| !is_hidden(&n.node.id));
    scene
        .labels
        .retain(|l| !is_hidden(&l.id) && !dropped.contains(&l.id));
    scene
        .ports
        .retain(|p| !is_hidden(&p.node_id) && !dropped.contains(&p.edge_id));
    scene
        .crossings
        .retain(|c| !is_hidden(&c.container_id) && !dropped.contains(&c.edge_id));
    Ok(())
}

/// The view without what `hidden` hides (a compacting filter): the nodes named or of a hidden kind with everything below
/// them, the edges of a hidden kind or with an end left out, and the ports, edge ports and bus members that refer to them.
pub(crate) fn without_hidden(view: &arqlet_model::Graph, hidden: &Hidden) -> arqlet_model::Graph {
    let parent: BTreeMap<&str, Option<&str>> = view
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let kind: BTreeMap<&str, &str> = view
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.kind.as_str()))
        .collect();
    let gone = |id: &str| {
        let mut at = Some(id);
        while let Some(x) = at {
            if hidden.nodes.iter().any(|n| n == x)
                || kind
                    .get(x)
                    .is_some_and(|k| hidden.node_kinds.iter().any(|h| h == k))
            {
                return true;
            }
            at = parent.get(x).copied().flatten();
        }
        false
    };
    let mut out = view.clone();
    out.nodes.retain(|n| !gone(&n.id));
    let kept: BTreeSet<String> = out.nodes.iter().map(|n| n.id.clone()).collect();
    out.edges.retain(|e| {
        !hidden.edge_kinds.contains(&e.kind)
            && kept.contains(&e.source)
            && e.target.as_ref().is_none_or(|t| kept.contains(t))
    });
    let edges: BTreeSet<String> = out.edges.iter().map(|e| e.id.clone()).collect();
    out.ports.retain(|p| kept.contains(&p.node_id));
    out.edge_ports.retain(|b| edges.contains(&b.edge_id));
    for bus in &mut out.buses {
        bus.edges.retain(|e| edges.contains(e));
    }
    out.buses.retain(|b| !b.edges.is_empty());
    out
}
