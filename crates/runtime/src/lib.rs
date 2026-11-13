//! Import, bounded view selection and revision-checked editing.
#[cfg(test)]
use arqlet_model::math::PortableMath;
pub mod budget;
pub mod components;
pub mod containers;
pub mod coordinator;
pub mod engine_session;
pub mod execution;
pub mod filter;
pub mod hierarchy;
pub mod history;
pub mod overview;
pub mod port_edit;
pub mod protocol;
pub mod session;
pub mod settle;
pub mod trace;

use execution::{
    ArchiveEntry, CandidateId, CandidateOutcome, CandidateRecord, Exclusion, InputIdentity,
    JobReport, Publication, StrategyCapabilities, WorkCounters,
};

/// The number of archive entries a refinement keeps.
pub const ARCHIVE_CAPACITY: usize = 8;

/// The bounded archive kept while a refinement reduces its candidates: the best `ARCHIVE_CAPACITY` measured candidates by
/// key, ties by candidate order, one entry per distinct geometry. It follows the rule of `archive_indices` and is kept
/// during the run, so every publication can carry it. Metadata only.
#[derive(Default)]
struct Archive {
    /// Best first. Each entry keeps its snapshot, so the archive can be refined; publications carry only the metadata.
    slots: Vec<(ArchiveEntry, Snapshot)>,
    seen: BTreeSet<String>,
    candidates: usize,
}

impl Archive {
    /// A candidate that produced no snapshot: it takes a position, but cannot enter the archive.
    fn skip(&mut self) {
        self.candidates += 1;
    }

    /// The metadata of the entries, best first.
    fn metadata(&self) -> Vec<ArchiveEntry> {
        self.slots.iter().map(|(entry, _)| entry.clone()).collect()
    }

    /// A candidate with a snapshot. Its geometry is recorded as seen even when unmeasurable, as the log's duplicate rule
    /// does; only a measured candidate can enter the archive.
    fn observe(&mut self, strategy: &str, option: &Snapshot, key: Option<&[i64]>) {
        let index = self.candidates;
        self.candidates += 1;
        let Some(fingerprint) = geometry_fingerprint(option) else {
            return;
        };
        if !self.seen.insert(fingerprint.clone()) {
            return;
        }
        let Some(key) = key else {
            return;
        };
        self.slots.push((
            ArchiveEntry {
                index,
                strategy: strategy.to_string(),
                key: key.to_vec(),
                geometry_fingerprint: fingerprint,
            },
            option.clone(),
        ));
        self.slots
            .sort_by(|a, b| a.0.key.cmp(&b.0.key).then(a.0.index.cmp(&b.0.index)));
        self.slots.truncate(ARCHIVE_CAPACITY);
    }
}
use arqlet_model::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[cfg(all(feature = "benchmark-timings", not(target_arch = "wasm32")))]
fn stage_timer_start() -> std::time::Instant {
    std::time::Instant::now()
}

#[cfg(not(all(feature = "benchmark-timings", not(target_arch = "wasm32"))))]
fn stage_timer_start() {}

#[cfg(all(feature = "benchmark-timings", not(target_arch = "wasm32")))]
fn stage_elapsed_nanos(start: std::time::Instant) -> u64 {
    u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(not(all(feature = "benchmark-timings", not(target_arch = "wasm32"))))]
fn stage_elapsed_nanos(_: ()) -> u64 {
    0
}

/// Upper bound for host-requested parallelism. Results never depend on the value.
pub const MAX_THREADS: usize = 8;

pub fn import_view(json: &str, limit: usize) -> Result<Snapshot, String> {
    let graph: Graph = serde_json::from_str(json).map_err(|e| e.to_string())?;
    import_graph(graph, limit)
}

/// The largest view an import may select (nodes, before the ancestors of the selected nodes are added).
pub const MAX_VIEW_LIMIT: usize = 10_000;

/// Selects the view of an import: the connected neighbourhood of the most connected node, then, when that neighbourhood
/// is exhausted before the limit, the neighbourhood of the next most connected node, until the limit is reached. A
/// graph whose nodes form one component is selected exactly as before. Ancestors of the selected nodes are added after.
/// The layout options an import starts from: the defaults, with the keys of `patch` set (as `configure` sets them).
/// The layout options of a JSON object of overrides over the defaults. Used by the import operation and the bench tool.
pub fn options_from_patch(patch: &serde_json::Value) -> Result<LayoutOptions, String> {
    let mut merged = serde_json::to_value(LayoutOptions::default()).map_err(|e| e.to_string())?;
    for (key, value) in patch.as_object().ok_or("Options must be an object")? {
        merged[key] = value.clone();
    }
    let mut options: LayoutOptions = serde_json::from_value(merged).map_err(|e| e.to_string())?;
    options.normalize_container_sizing();
    options.validate_container_sizing()?;
    Ok(options)
}

pub fn import_graph(graph: Graph, limit: usize) -> Result<Snapshot, String> {
    import_graph_with(graph, limit, &LayoutOptions::default())
}

/// An import with options: the rows, ports and labels a notation needs are checked by the first layout (ADR 0011).
pub fn import_graph_with(
    graph: Graph,
    limit: usize,
    options: &LayoutOptions,
) -> Result<Snapshot, String> {
    import_graph_counted(graph, limit, options).map(|(snapshot, _)| snapshot)
}

/// The source graph as the view projects it (provisional): the groups the data gives (`groups`) and then the communities
/// of the containers that are still large (`communities`) as containers of their own (`arqlet_layout::communities`). Only
/// the view sees them; the snapshot keeps the source graph itself, and every projection derives them again.
pub fn structured<'a>(
    source: &'a Graph,
    options: &LayoutOptions,
) -> Result<std::borrow::Cow<'a, Graph>, String> {
    if !derives_groups(options) {
        return Ok(std::borrow::Cow::Borrowed(source));
    }
    let given = arqlet_layout::communities::with_given_groups(source, &options.groups)?;
    let grouped = arqlet_layout::communities::with_community_containers_except(
        &given,
        options.communities,
        &hubs_of(&options.community_hubs),
    )?;
    // Containers shown in part (`reveal`): the children not shown gathered in a rest.
    arqlet_layout::communities::with_partial(&grouped, &options.partial)
        .map(std::borrow::Cow::Owned)
}

/// The hubs `communities` leaves out (`LayoutOptions::community_hubs`), for the layout crate.
pub(crate) fn hubs_of(hubs: &arqlet_model::CommunityHubs) -> arqlet_layout::overview::Hubs {
    arqlet_layout::overview::Hubs {
        min_links: hubs.min_links.map(f64::from),
        nodes: hubs.nodes.iter().cloned().collect(),
    }
}

/// Whether the view adds containers of its own (communities, given groups, rests of partly shown containers).
fn derives_groups(options: &LayoutOptions) -> bool {
    options.communities > 0 || !options.groups.is_empty() || !options.partial.is_empty()
}

/// Whether the view abstracts (provisional): with derived groups or an initial collapse, the edges between collapsed
/// containers are drawn by one route per pair with their weights (`merge_aggregated_pairs`).
pub(crate) fn abstracts(options: &LayoutOptions) -> bool {
    derives_groups(options) || options.level_of_detail.is_some()
}

/// Whether the projection draws the edges a collapsed container aggregates once per pair: with a node budget, and when
/// the view abstracts (its collapsed boxes gather many edges).
fn aggregates(options: &LayoutOptions) -> bool {
    options.visible_node_budget > 0 || abstracts(options)
}

/// The containers the first view of an import shows collapsed, besides a node budget's: those `level_of_detail` names
/// (at a depth or deeper, of a kind), else the groups and communities the view added.
fn initially_collapsed(view: &Graph, options: &LayoutOptions) -> BTreeSet<String> {
    let parent: BTreeMap<&str, Option<&str>> = view
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let containers: BTreeSet<&str> = view
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let depth = |id: &str| {
        let mut d = 0;
        let mut at = parent.get(id).copied().flatten();
        while let Some(p) = at {
            d += 1;
            at = parent.get(p).copied().flatten();
        }
        d
    };
    view.nodes
        .iter()
        .filter(|n| containers.contains(n.id.as_str()))
        .filter(|n| match &options.level_of_detail {
            Some(c) => c.depth.is_some_and(|d| depth(&n.id) >= d) || c.kinds.contains(&n.kind),
            None => {
                n.kind == arqlet_layout::communities::COMMUNITY_KIND
                    || n.kind == arqlet_layout::communities::GROUP_KIND
            }
        })
        .map(|n| n.id.clone())
        .collect()
}

/// The view of a source graph (`project_view`).
pub struct View {
    /// The structured graph (`structured`): the source graph with the communities as containers.
    pub graph: Graph,
    /// The collapsed containers that still exist in it (a community can be gone after the graph changed).
    pub collapsed: BTreeSet<String>,
    /// The projection that is laid out.
    pub projected: Graph,
    /// With communities: what each edge that stands for collapsed content represents (`aggregate_weights`).
    pub weights: BTreeMap<String, AggregateWeight>,
}

/// The edges a collapsed container aggregates, merged: with communities, one edge per pair of visible ends for both
/// directions, with its weights; else (node budget) one per pair and direction.
fn merge_view(
    view: &Graph,
    projected: Graph,
    options: &LayoutOptions,
) -> (Graph, BTreeMap<String, AggregateWeight>) {
    if abstracts(options) {
        arqlet_view::merge_aggregated_pairs(view, projected)
    } else {
        (
            arqlet_view::merge_aggregated(view, projected),
            BTreeMap::new(),
        )
    }
}

/// The view of a source graph: the structured graph, the collapsed containers that still exist in it (others are
/// dropped from the set), and the projection. With `merge`, the edges a collapsed container aggregates are merged when
/// `aggregates` holds (`merge_view`). (Edits other than `toggle` do not merge under a node budget, as before communities
/// existed; with communities every view merges.)
pub fn project_view(
    source: &Graph,
    collapsed: &[String],
    options: &LayoutOptions,
    merge: bool,
) -> Result<View, String> {
    let view = structured(source, options)?.into_owned();
    let containers: BTreeSet<&str> = view
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let collapsed: BTreeSet<String> = collapsed
        .iter()
        .filter(|id| !derives_groups(options) || containers.contains(id.as_str()))
        .cloned()
        .collect();
    let mut projected = arqlet_view::project(&view, &collapsed)?;
    let mut weights = BTreeMap::new();
    if merge && aggregates(options) {
        (projected, weights) = merge_view(&view, projected, options);
    }
    Ok(View {
        graph: view,
        collapsed,
        projected,
        weights,
    })
}

/// The options of a layout that tries single pass (`LayoutOptions::single_pass_auto`).
fn single_pass_options(options: &LayoutOptions) -> LayoutOptions {
    LayoutOptions {
        abstract_routing: true,
        single_pass: true,
        ..options.clone()
    }
}

/// `import_graph_with` that also returns the stage work of the first layout, for reports of where an import's time goes.
/// The view of an import within `limit` nodes: the connected neighbourhood of the most connected node, then of the next,
/// until the limit is reached (see `import_graph_counted`).
fn select_connected(graph: &Graph, limit: usize) -> BTreeSet<String> {
    let mut neighbors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in &graph.edges {
        let t = e.target.as_ref().unwrap();
        neighbors
            .entry(e.source.clone())
            .or_default()
            .insert(t.clone());
        neighbors
            .entry(t.clone())
            .or_default()
            .insert(e.source.clone());
    }
    // Seeds: the nodes with edges, most connected first (ties: smaller ID), then the nodes without edges in input order.
    let mut seeds: Vec<String> = neighbors.keys().cloned().collect();
    seeds.sort_by(|a, b| {
        neighbors[b]
            .len()
            .cmp(&neighbors[a].len())
            .then_with(|| a.cmp(b))
    });
    seeds.extend(
        graph
            .nodes
            .iter()
            .filter(|n| !neighbors.contains_key(&n.id))
            .map(|n| n.id.clone()),
    );
    let mut selected = BTreeSet::new();
    for seed in seeds {
        if selected.len() >= limit {
            break;
        }
        let mut queue = VecDeque::from([seed]);
        while let Some(id) = queue.pop_front() {
            if !selected.insert(id.clone()) {
                continue;
            }
            if selected.len() >= limit {
                break;
            }
            if let Some(ns) = neighbors.get(&id) {
                queue.extend(ns.iter().filter(|n| !selected.contains(*n)).cloned());
            }
        }
    }
    selected
}

/// Provisional: the view of an import within `limit`, counting what the first view shows when it abstracts (level of
/// detail, communities, groups). A container the view starts collapsed counts once and keeps its whole content, so that
/// it can be opened; a container an edge reaches while none of its content is selected keeps its content too and starts
/// collapsed (the second set). Otherwise as `select_connected`: the most connected neighbourhoods first, then the rest
/// in input order.
fn select_visible(
    graph: &Graph,
    limit: usize,
    options: &LayoutOptions,
) -> Result<(BTreeSet<String>, BTreeSet<String>), String> {
    let view = structured(graph, options)?;
    let collapsed = initially_collapsed(&view, options);
    let parent: BTreeMap<&str, Option<&str>> = view
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let containers: BTreeSet<&str> = view
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    // What stands for a node in the first view: the outermost collapsed container above it, or itself.
    let unit = |id: &str| -> String {
        let mut shown = id;
        let mut at = Some(id);
        while let Some(x) = at {
            if collapsed.contains(x) {
                shown = x;
            }
            at = parent.get(x).copied().flatten();
        }
        shown.to_string()
    };
    let mut neighbors: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in &graph.edges {
        let (a, b) = (unit(&e.source), unit(e.target.as_ref().unwrap()));
        neighbors.entry(a.clone()).or_default().insert(b.clone());
        neighbors.entry(b).or_default().insert(a);
    }
    let mut seeds: Vec<String> = neighbors.keys().cloned().collect();
    seeds.sort_by(|a, b| {
        neighbors[b]
            .len()
            .cmp(&neighbors[a].len())
            .then_with(|| a.cmp(b))
    });
    // Then what has no edges, in input order: every leaf's unit once.
    let mut seen: BTreeSet<String> = seeds.iter().cloned().collect();
    for node in &graph.nodes {
        if containers.contains(node.id.as_str()) {
            continue;
        }
        let u = unit(&node.id);
        if seen.insert(u.clone()) {
            seeds.push(u);
        }
    }
    let mut units = BTreeSet::new();
    for seed in seeds {
        if units.len() >= limit {
            break;
        }
        let mut queue = VecDeque::from([seed]);
        while let Some(id) = queue.pop_front() {
            if !units.insert(id.clone()) {
                continue;
            }
            if units.len() >= limit {
                break;
            }
            if let Some(ns) = neighbors.get(&id) {
                queue.extend(ns.iter().filter(|n| !units.contains(*n)).cloned());
            }
        }
    }
    // Back to the graph's nodes: every node whose unit is selected, a collapsed container with all it holds.
    let mut selected: BTreeSet<String> = graph
        .nodes
        .iter()
        .filter(|n| units.contains(&unit(&n.id)))
        .map(|n| n.id.clone())
        .collect();
    // A selected container that is not collapsed and holds nothing selected (an edge reached it): whole, collapsed.
    let holds_selected: BTreeSet<&str> = selected
        .iter()
        .flat_map(|id| {
            let mut up = Vec::new();
            let mut at = parent.get(id.as_str()).copied().flatten();
            while let Some(p) = at {
                up.push(p);
                at = parent.get(p).copied().flatten();
            }
            up
        })
        .collect();
    let mut pulled = BTreeSet::new();
    for id in &units {
        if containers.contains(id.as_str())
            && !collapsed.contains(id.as_str())
            && !holds_selected.contains(id.as_str())
        {
            pulled.insert(id.clone());
        }
    }
    if !pulled.is_empty() {
        for node in &graph.nodes {
            let mut at = parent.get(node.id.as_str()).copied().flatten();
            while let Some(p) = at {
                if pulled.contains(p) {
                    selected.insert(node.id.clone());
                    break;
                }
                at = parent.get(p).copied().flatten();
            }
        }
    }
    Ok((selected, pulled))
}

pub fn import_graph_counted(
    mut graph: Graph,
    limit: usize,
    options: &LayoutOptions,
) -> Result<(Snapshot, arqlet_layout::SearchCounters), String> {
    if limit == 0 || limit > MAX_VIEW_LIMIT {
        return Err(format!("View limit must be between 1 and {MAX_VIEW_LIMIT}"));
    }
    let single_pass = options
        .single_pass_auto
        .then(|| single_pass_options(options));
    let options = single_pass.as_ref().unwrap_or(options);
    validate(&graph)?;
    let omitted = graph.edges.iter().filter(|e| e.target.is_none()).count();
    graph.edges.retain(|e| e.target.is_some());
    // With an abstracting view, the limit counts what the first view shows (`select_visible`); otherwise the most
    // connected neighbourhoods of the graph's nodes.
    let (mut selected, pulled) = if abstracts(options) && graph.nodes.len() > limit {
        select_visible(&graph, limit, options)?
    } else {
        (select_connected(&graph, limit), BTreeSet::new())
    };
    let parents: BTreeMap<_, _> = graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.parent_id.clone()))
        .collect();
    for id in selected.clone() {
        let mut p = parents[&id].clone();
        while let Some(parent) = p {
            selected.insert(parent.clone());
            p = parents[&parent].clone();
        }
    }
    graph.nodes.retain(|n| selected.contains(&n.id));
    graph.ports.retain(|port| selected.contains(&port.node_id));
    graph
        .edges
        .retain(|e| selected.contains(&e.source) && selected.contains(e.target.as_ref().unwrap()));
    graph
        .edge_ports
        .retain(|b| graph.edges.iter().any(|e| e.id == b.edge_id));
    // Experimental (global layering, iteration 1): within a node budget the first view collapses containers, the
    // outermost first, and a collapsed container takes the size estimated from its content.
    if aggregates(options) {
        let view = structured(&graph, options)?;
        // The first view: within a node budget, the outermost containers collapsed; and those `initially_collapsed` names.
        let mut collapsed = if options.visible_node_budget > 0 {
            budget::initial_collapse(&view, options.visible_node_budget)
        } else {
            BTreeSet::new()
        };
        if abstracts(options) {
            collapsed.extend(initially_collapsed(&view, options));
            // Containers an edge pulled in without their content selected: whole, and collapsed (`select_visible`).
            collapsed.extend(
                pulled
                    .into_iter()
                    .filter(|id| view.nodes.iter().any(|n| &n.id == id)),
            );
        }
        let (projected, weights) =
            merge_view(&view, arqlet_view::project(&view, &collapsed)?, options);
        let options = view_options(&view, &collapsed, options, &weights);
        let (mut snapshot, stages) =
            arqlet_layout::compute_counted(&projected, None, &BTreeSet::new(), &options)?;
        snapshot.source_graph = Some(graph);
        snapshot.collapsed_ids = collapsed.into_iter().collect();
        snapshot.omitted_unresolved_edges = omitted;
        return Ok((snapshot, stages));
    }
    let (mut snapshot, stages) =
        arqlet_layout::compute_counted(&graph, None, &BTreeSet::new(), options)?;
    snapshot.source_graph = Some(graph);
    snapshot.omitted_unresolved_edges = omitted;
    Ok((snapshot, stages))
}
pub fn add_node(
    snapshot: &Snapshot,
    expected_revision: u64,
    source_id: &str,
    label: &str,
) -> Result<Snapshot, String> {
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    if snapshot.revision != expected_revision {
        return Err("Stale edit revision".into());
    }
    validate(&snapshot.graph)?;
    if snapshot
        .graph
        .nodes
        .iter()
        .any(|n| !snapshot.boxes.contains_key(&n.id))
    {
        return Err("Incomplete snapshot geometry".into());
    }
    let source = snapshot
        .graph
        .nodes
        .iter()
        .find(|n| n.id == source_id)
        .ok_or("Unknown source node")?;
    let mut graph = snapshot
        .source_graph
        .clone()
        .unwrap_or_else(|| snapshot.graph.clone());
    let id = format!("local:{}", snapshot.revision + 1);
    if graph.nodes.iter().any(|n| n.id == id) {
        return Err("Local node ID collision".into());
    }
    // The parent in the source graph: in the view it may be a community, which only the view has.
    let parent_id = graph
        .nodes
        .iter()
        .find(|n| n.id == source_id)
        .map_or_else(|| source.parent_id.clone(), |n| n.parent_id.clone());
    graph.nodes.push(Node {
        id: id.clone(),
        kind: "symbol".into(),
        label: label.chars().take(80).collect(),
        parent_id,
        shape: Default::default(),
    });
    graph.edges.push(Edge {
        id: format!("local-edge:{}", snapshot.revision + 1),
        kind: "calls".into(),
        source: source_id.into(),
        target: Some(id.clone()),
        weight: None,
    });
    let View {
        graph: view,
        collapsed,
        projected,
        weights,
    } = project_view(
        &graph,
        &snapshot.collapsed_ids,
        &snapshot.options,
        abstracts(&snapshot.options),
    )?;
    // The new node's containers in the view (communities included) are placed again.
    let mut dirty = BTreeSet::new();
    let mut parent = view
        .nodes
        .iter()
        .find(|n| n.id == id)
        .and_then(|n| n.parent_id.clone());
    while let Some(p) = parent {
        parent = view
            .nodes
            .iter()
            .find(|n| n.id == p)
            .and_then(|n| n.parent_id.clone());
        dirty.insert(p);
    }
    let options = if abstracts(&snapshot.options) {
        view_options(&view, &collapsed, &snapshot.options, &weights)
    } else {
        snapshot.options.clone()
    };
    let mut result = arqlet_layout::compute(&projected, Some(snapshot), &dirty, &options)?;
    result.source_graph = Some(graph);
    result.collapsed_ids = collapsed.into_iter().collect();
    Ok(result)
}

/// The layout options of a view: under a node budget, collapsed containers sized from their content. Not for communities:
/// measured on a flat call graph (1 866 nodes), content-sized collapsed communities made the first view 35 106 px wide
/// against 4 560 with plain boxes, and opening one moved the others as much either way. With communities, the weights of
/// the merged edges (`aggregate_weights`, replacing those of the previous view).
fn view_options(
    view: &Graph,
    collapsed: &BTreeSet<String>,
    options: &LayoutOptions,
    weights: &BTreeMap<String, AggregateWeight>,
) -> LayoutOptions {
    let mut options = options.clone();
    if options.visible_node_budget > 0 {
        options.collapsed_sizes = budget::estimated_sizes(view, collapsed);
    }
    if abstracts(&options) {
        options.aggregate_weights = weights.clone();
    }
    options
}
/// Change only view state. Hidden nodes and original edge endpoints remain canonical.
pub fn toggle_container(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    id: &str,
) -> Result<Snapshot, String> {
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    if snapshot.revision != expected || snapshot.layout_version != layout {
        return Err("Stale view request".into());
    }
    let canonical = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    // The view's containers: with communities, those too.
    let source = structured(canonical, &snapshot.options)?;
    let source = source.as_ref();
    if !snapshot.graph.nodes.iter().any(|n| n.id == id) {
        return Err("Container is not visible".into());
    }
    if !source
        .nodes
        .iter()
        .any(|n| n.parent_id.as_deref() == Some(id))
    {
        return Err("Not a container".into());
    }
    let mut collapsed: Vec<String> = snapshot.collapsed_ids.clone();
    let mut options = snapshot.options.clone();
    // The rest of a partly shown container: opening it shows every child of the container again.
    if let Some(container) = id.strip_prefix(arqlet_layout::communities::REST_PREFIX) {
        options.partial.remove(container);
        collapsed.retain(|c| c != id);
        return relayout_collapsed(
            snapshot,
            canonical,
            &options,
            collapsed,
            &[container.to_string()],
        );
    }
    // A container toggled as a whole is shown whole when it opens again.
    if options.partial.remove(id).is_some() {
        let rest = format!("{}{id}", arqlet_layout::communities::REST_PREFIX);
        collapsed.retain(|c| *c != rest);
    }
    let expanding = collapsed.iter().any(|c| c == id);
    if expanding {
        collapsed.retain(|c| c != id);
    } else {
        collapsed.push(id.to_string());
    }
    let opened = if expanding {
        vec![id.to_string()]
    } else {
        Vec::new()
    };
    relayout_collapsed(snapshot, canonical, &options, collapsed, &opened)
}

/// The view with `collapsed` as its collapsed containers, `opened` the ones that were expanded (`toggle`, `reveal`).
fn relayout_collapsed(
    snapshot: &Snapshot,
    canonical: &Graph,
    options: &LayoutOptions,
    collapsed: Vec<String>,
    opened: &[String],
) -> Result<Snapshot, String> {
    let expanding = !opened.is_empty();
    // With a node budget or communities, the edges a collapsed container aggregates are drawn once per pair, and the
    // collapsed containers keep sizes estimated from their content.
    let View {
        graph: source,
        collapsed,
        projected: graph,
        weights,
    } = project_view(canonical, &collapsed, options, true)?;
    let options = view_options(&source, &collapsed, options, &weights);
    // Expanding within a node budget is local (global layering plan, step 2 of the first view): the drawing continues the
    // earlier one; only the container and the areas around it are placed again (they start from their earlier order), the
    // other containers keep their boxes and plans, and the routes whose ends moved together are carried along.
    // Not for groups, communities or an initial collapse: there the local layout was slower and worse than laying out
    // the view again (at 50 000 nodes, opening 22 groups: 9.6 s and 484 routes handed to the completing router, against
    // 1.4 s and 19; it moved 220 of 291 boxes anyway), so their view is laid out from scratch and depends only on the
    // graph and the view, not on the order of the toggles (benchmarks/results/toggle-layout-20261112.md).
    let mut result = if expanding && options.visible_node_budget > 0 {
        let mut dirty: BTreeSet<String> = opened.iter().cloned().collect();
        for id in opened {
            let mut at = source
                .nodes
                .iter()
                .find(|n| &n.id == id)
                .and_then(|n| n.parent_id.clone());
            while let Some(p) = at {
                at = source
                    .nodes
                    .iter()
                    .find(|n| n.id == p)
                    .and_then(|n| n.parent_id.clone());
                dirty.insert(p);
            }
        }
        arqlet_layout::compute(&graph, Some(snapshot), &dirty, &options)?
    } else {
        arqlet_layout::compute(&graph, None, &BTreeSet::new(), &options)?
    };
    result.source_graph = Some(canonical.clone());
    result.collapsed_ids = collapsed.into_iter().collect();
    result.revision = snapshot.revision;
    result.base_revision = Some(snapshot.revision);
    result.layout_version = snapshot
        .layout_version
        .checked_add(1)
        .ok_or("Layout version overflow")?;
    result.omitted_unresolved_edges = snapshot.omitted_unresolved_edges;
    Ok(result)
}
/// Provisional (path cut-out, step 2): makes the nodes `ids` visible by expanding the collapsed containers that hold them
/// (in the view: communities and groups too); everything else stays as it is. With `partial`, a container opened this way
/// shows only the children on the way to those nodes, the others gathered in a collapsed rest (`LayoutOptions::partial`);
/// a node in a rest is moved out of it. Without, the containers open whole. Nodes that are visible already change nothing,
/// and the snapshot comes back as it was. A view request: stale revisions are refused.
pub fn reveal_nodes(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    ids: &[String],
    partial: bool,
) -> Result<Snapshot, String> {
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    if snapshot.revision != expected || snapshot.layout_version != layout {
        return Err("Stale view request".into());
    }
    let canonical = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    // The view without its rests: the way from each node up to the top.
    let mut options = snapshot.options.clone();
    let shown_in_part = std::mem::take(&mut options.partial);
    let base = structured(canonical, &options)?;
    let parent: BTreeMap<&str, Option<&str>> = base
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let mut collapsed: Vec<String> = snapshot.collapsed_ids.clone();
    let mut partly = shown_in_part.clone();
    let mut opened = BTreeSet::new();
    for id in ids {
        if !parent.contains_key(id.as_str()) {
            return Err(format!("Unknown node: {id}"));
        }
        let mut child = id.as_str();
        let mut at = parent[id.as_str()];
        while let Some(p) = at {
            let closed = collapsed.iter().any(|c| c == p);
            if closed || partly.contains_key(p) {
                if partial {
                    let shown = partly.entry(p.to_string()).or_default();
                    if !shown.iter().any(|c| c == child) {
                        shown.push(child.to_string());
                    }
                } else {
                    partly.remove(p);
                }
                if closed {
                    opened.insert(p.to_string());
                }
            }
            child = p;
            at = parent.get(p).copied().flatten();
        }
    }
    if opened.is_empty() && partly == shown_in_part {
        return Ok(snapshot.clone());
    }
    collapsed.retain(|c| !opened.contains(c));
    // Every rest starts collapsed; the rests of containers no longer shown in part go.
    for container in partly.keys() {
        let rest = format!("{}{container}", arqlet_layout::communities::REST_PREFIX);
        if !collapsed.contains(&rest) {
            collapsed.push(rest);
        }
    }
    collapsed.retain(
        |c| match c.strip_prefix(arqlet_layout::communities::REST_PREFIX) {
            Some(container) => partly.contains_key(container),
            None => true,
        },
    );
    options.partial = partly;
    let mut opened: Vec<String> = opened.into_iter().collect();
    if opened.is_empty() {
        // Only what a rest holds changed: the partly shown containers are placed again.
        opened = options.partial.keys().cloned().collect();
    }
    relayout_collapsed(snapshot, canonical, &options, collapsed, &opened)
}

pub fn set_direction(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    container: Option<&str>,
    direction: Option<DirectionConstraint>,
) -> Result<Snapshot, String> {
    if snapshot.revision != expected || snapshot.layout_version != layout {
        return Err("Stale direction request".into());
    }
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    let mut source = snapshot
        .source_graph
        .clone()
        .unwrap_or_else(|| snapshot.graph.clone());
    if let Some(id) = container {
        if !source
            .nodes
            .iter()
            .any(|n| n.parent_id.as_deref() == Some(id))
        {
            return Err("Not a container".into());
        }
        if let Some(value) = direction {
            source.container_directions.insert(id.into(), value);
        } else {
            source.container_directions.remove(id);
        }
    } else {
        source.default_direction = direction.ok_or("Root requires a direction")?;
    }
    recompute_source(snapshot, source)
}

/// Recomputes the view from an edited source graph: the revision stays, the layout version advances, and a custom change
/// leaves its preset.
fn recompute_source(snapshot: &Snapshot, source: Graph) -> Result<Snapshot, String> {
    let communities = abstracts(&snapshot.options);
    let View {
        graph: view,
        collapsed,
        projected: graph,
        weights,
    } = project_view(
        &source,
        &snapshot.collapsed_ids,
        &snapshot.options,
        communities,
    )?;
    // A direction change is a custom change: the snapshot no longer matches its preset.
    let mut options = if communities {
        view_options(&view, &collapsed, &snapshot.options, &weights)
    } else {
        snapshot.options.clone()
    };
    options.config_id = None;
    let mut result = arqlet_layout::compute(&graph, None, &BTreeSet::new(), &options)?;
    result.source_graph = Some(source);
    result.collapsed_ids = collapsed.into_iter().collect();
    result.revision = snapshot.revision;
    result.base_revision = Some(snapshot.revision);
    result.layout_version = snapshot
        .layout_version
        .checked_add(1)
        .ok_or("Version overflow")?;
    result.omitted_unresolved_edges = snapshot.omitted_unresolved_edges;
    Ok(result)
}

/// Sets or inherits the layout strategy of the root or of a container. The request names the same way as direction: a
/// container without a value inherits again.
pub fn set_strategy(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    container: Option<&str>,
    strategy: Option<StrategyConstraint>,
) -> Result<Snapshot, String> {
    if snapshot.revision != expected || snapshot.layout_version != layout {
        return Err("Stale strategy request".into());
    }
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    let mut source = snapshot
        .source_graph
        .clone()
        .unwrap_or_else(|| snapshot.graph.clone());
    if let Some(id) = container {
        if !source
            .nodes
            .iter()
            .any(|n| n.parent_id.as_deref() == Some(id))
        {
            return Err("Not a container".into());
        }
        match strategy {
            Some(value) => {
                source.container_strategies.insert(id.into(), value);
            }
            None => {
                source.container_strategies.remove(id);
            }
        }
    } else {
        source.default_strategy = strategy.ok_or("Root requires a strategy")?;
    }
    recompute_source(snapshot, source)
}

/// Sets or clears the routing constraint of a container, or of the root (`None` clears it: nothing is declared, and the
/// layout's fallback applies). A container entry that is cleared inherits again.
pub fn set_routing(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    container: Option<&str>,
    routing: Option<arqlet_model::RoutingConstraint>,
) -> Result<Snapshot, String> {
    if snapshot.revision != expected || snapshot.layout_version != layout {
        return Err("Stale routing request".into());
    }
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    let mut source = snapshot
        .source_graph
        .clone()
        .unwrap_or_else(|| snapshot.graph.clone());
    match container {
        Some(id) => {
            if !source
                .nodes
                .iter()
                .any(|n| n.parent_id.as_deref() == Some(id))
            {
                return Err("Not a container".into());
            }
            match routing {
                Some(value) => {
                    source.container_routing.insert(id.into(), value);
                }
                None => {
                    source.container_routing.remove(id);
                }
            }
        }
        None => source.default_routing = routing,
    }
    recompute_source(snapshot, source)
}

/// Recomputes the view with new options. The graph revision stays; the layout version advances.
/// The authored fixed sizes (ADR 0010) that the children do not fit: a fixed axis below the extent the children need,
/// with the padding of the geometry rules. The conflict names the container and the size it would need, and the boxes
/// keep the authored size.
pub(crate) fn fixed_size_conflicts(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    sizing: &BTreeMap<String, ContainerSizing>,
) -> Vec<GeometryIssue> {
    let padding = arqlet_scene::GeometryRules::default().padding;
    let mut issues = Vec::new();
    for (id, entry) in sizing {
        if !boxes.contains_key(id) {
            continue;
        }
        let children: Vec<&Rect> = graph
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == Some(id.as_str()))
            .filter_map(|n| boxes.get(&n.id))
            .collect();
        if children.is_empty() {
            continue;
        }
        let right = children
            .iter()
            .map(|r| r.x + r.width)
            .fold(f64::MIN, f64::max)
            + padding;
        let bottom = children
            .iter()
            .map(|r| r.y + r.height)
            .fold(f64::MIN, f64::max)
            + padding;
        for (axis, fixed, need) in [
            ("width", entry.width, right),
            ("height", entry.height, bottom),
        ] {
            let Some(AxisSize::Fixed(value)) = fixed else {
                continue;
            };
            if need > value + 1e-6 {
                issues.push(GeometryIssue {
                    kind: "container-fixed-below-content".into(),
                    node_ids: vec![id.clone()],
                    message: format!(
                        "fixed {axis} {value} is below the required {}; the children do not fit",
                        need.ceil()
                    ),
                });
            }
        }
    }
    issues
}

/// A measured size within this many units of the size already in use is not a change (the same tolerance the page used
/// before the engine took it over). The measurement's revision is still recorded.
pub const MEASURE_TOLERANCE: f64 = 4.0;

fn recompute_with(snapshot: &Snapshot, options: &LayoutOptions) -> Result<Snapshot, String> {
    // The configId is a label, not a geometry input: it is compared like appearance.
    let mut geometry_options = options.clone();
    geometry_options.appearance = snapshot.options.appearance.clone();
    geometry_options.config_id = snapshot.options.config_id.clone();
    // The revision of a measurement says which report is newest; it does not change the layout. A change that carries
    // nothing else is recorded and does not advance the layout version, so work that waits for this layout stays valid.
    let mut only_revisions = options.clone();
    only_revisions.geometry_revisions = snapshot.options.geometry_revisions.clone();
    if only_revisions == snapshot.options {
        let mut result = snapshot.clone();
        result.options = options.clone();
        result.recomputed_containers.clear();
        return Ok(result);
    }
    geometry_options.geometry_revisions = snapshot.options.geometry_revisions.clone();
    if geometry_options == snapshot.options {
        let mut result = snapshot.clone();
        result.options = options.clone();
        result.base_revision = Some(snapshot.revision);
        result.layout_version = snapshot
            .layout_version
            .checked_add(1)
            .ok_or("Layout version overflow")?;
        result.recomputed_containers.clear();
        return Ok(result);
    }
    recompute_layout(snapshot, options, local_dirty(snapshot, options))
}

/// Whether an area of the graph is arranged organically: only there a continuation holds the unedited nodes in place.
fn uses_organic(graph: &Graph) -> bool {
    let organic = |c: &StrategyConstraint| c.default_choice() == LayoutStrategy::Organic;
    organic(&graph.default_strategy) || graph.container_strategies.values().any(organic)
}

/// The edited nodes and the containers above them: the set a continuation re-places, as `add_node` marks the parent chain.
fn dirty_with_ancestors(
    graph: &Graph,
    edited: impl IntoIterator<Item = String>,
) -> BTreeSet<String> {
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let mut dirty = BTreeSet::new();
    for id in edited {
        let mut current = Some(id.as_str().to_string());
        while let Some(node) = current {
            if !dirty.insert(node.clone()) {
                break;
            }
            current = parents
                .get(node.as_str())
                .copied()
                .flatten()
                .map(str::to_string);
        }
    }
    dirty
}

/// The nodes a change of options touches when it is local: pins and requested sizes, nothing else. Only these nodes and the
/// containers above them are placed again; every other area keeps its boxes. `None` for any other change, which re-places
/// every node.
fn local_dirty(snapshot: &Snapshot, options: &LayoutOptions) -> Option<BTreeSet<String>> {
    let source = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    let mut rest = options.clone();
    rest.pins = snapshot.options.pins.clone();
    rest.order_hints = snapshot.options.order_hints.clone();
    rest.node_sizes = snapshot.options.node_sizes.clone();
    // The revision of a measurement says which report is newest; it is not a layout input.
    rest.geometry_revisions = snapshot.options.geometry_revisions.clone();
    rest.appearance = snapshot.options.appearance.clone();
    rest.config_id = snapshot.options.config_id.clone();
    if rest != snapshot.options {
        return None;
    }
    let mut edited = BTreeSet::new();
    for id in snapshot.options.pins.keys().chain(options.pins.keys()) {
        let before = snapshot.options.pins.get(id);
        let after = options.pins.get(id);
        if before.map(|p| (p.x, p.y)) != after.map(|p| (p.x, p.y)) {
            edited.insert(id.clone());
        }
    }
    for id in snapshot
        .options
        .order_hints
        .keys()
        .chain(options.order_hints.keys())
    {
        if snapshot.options.order_hints.get(id) != options.order_hints.get(id) {
            edited.insert(id.clone());
        }
    }
    for id in snapshot
        .options
        .node_sizes
        .keys()
        .chain(options.node_sizes.keys())
    {
        let before = snapshot.options.node_sizes.get(id);
        let after = options.node_sizes.get(id);
        if before.map(|z| (z.width, z.height)) != after.map(|z| (z.width, z.height)) {
            edited.insert(id.clone());
        }
    }
    if edited.is_empty() {
        return None;
    }
    Some(dirty_with_ancestors(source, edited))
}

/// Lays out the snapshot's source graph again with `options`, starting from the snapshot's positions (a warm start). With
/// `dirty` only those nodes (and the containers above them) are placed again; `None` places every node.
fn recompute_layout(
    snapshot: &Snapshot,
    options: &LayoutOptions,
    dirty: Option<BTreeSet<String>>,
) -> Result<Snapshot, String> {
    let source = snapshot
        .source_graph
        .clone()
        .unwrap_or_else(|| snapshot.graph.clone());
    let communities = abstracts(options);
    let View {
        graph: view,
        collapsed,
        projected: graph,
        weights,
    } = project_view(&source, &snapshot.collapsed_ids, options, communities)?;
    let sized;
    let options = if communities {
        sized = view_options(&view, &collapsed, options, &weights);
        &sized
    } else {
        options
    };
    // Warm start: children begin where the previous layout put them. Every node is placed again, so that a changed size or
    // a changed container reaches the drawing; only the start positions are taken from the previous layout.
    let dirty = dirty.unwrap_or_else(|| graph.nodes.iter().map(|n| n.id.clone()).collect());
    let mut result = arqlet_layout::compute(&graph, Some(snapshot), &dirty, options)?;
    result.source_graph = Some(source);
    result.collapsed_ids = collapsed.into_iter().collect();
    result.revision = snapshot.revision;
    result.base_revision = Some(snapshot.revision);
    result.layout_version = snapshot
        .layout_version
        .checked_add(1)
        .ok_or("Layout version overflow")?;
    result.omitted_unresolved_edges = snapshot.omitted_unresolved_edges;
    Ok(result)
}

/// Sets the outline of a node (`rect`, `square` or `circle`) and recomputes. The other nodes keep their positions as a warm
/// start; the node's own geometry follows its new outline. A shape that is already set changes nothing.
pub fn set_shape(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    node: &str,
    shape: NodeShape,
) -> Result<Snapshot, String> {
    check_current(snapshot, expected, layout)?;
    let mut source = snapshot
        .source_graph
        .clone()
        .unwrap_or_else(|| snapshot.graph.clone());
    let target = source
        .nodes
        .iter_mut()
        .find(|n| n.id == node)
        .ok_or_else(|| format!("Unknown node in shape change: {node}"))?;
    if target.shape == shape {
        return Ok(snapshot.clone());
    }
    target.shape = shape;
    let dirty = uses_organic(&source).then(|| dirty_with_ancestors(&source, [node.to_string()]));
    let mut changed = snapshot.clone();
    changed.source_graph = Some(source);
    recompute_layout(&changed, &snapshot.options, dirty)
}

fn check_current(snapshot: &Snapshot, expected: u64, layout: u64) -> Result<(), String> {
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    if snapshot.revision != expected || snapshot.layout_version != layout {
        return Err("Stale options request".into());
    }
    Ok(())
}

/// Applies a partial options object. A `preset` key first sets the candidate scope; explicit keys
/// then override it. Unknown keys are rejected by the options schema.
pub fn configure_snapshot(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    patch: &serde_json::Value,
) -> Result<Snapshot, String> {
    check_current(snapshot, expected, layout)?;
    let mut options = snapshot.options.clone();
    if let Some(value) = patch.get("configSet") {
        if patch.as_object().is_none_or(|p| p.len() != 1) {
            return Err("Apply configSet separately from option overrides".into());
        }
        let config: ConfigSet = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
        config.apply(&mut options)?;
        // Applying the set that is already active changes nothing, so it does not advance the layout version.
        if options == snapshot.options {
            return Ok(snapshot.clone());
        }
        return recompute_with(snapshot, &options);
    }
    // Any option change outside a config set makes the options custom: the preset identity no longer holds.
    options.config_id = None;
    if let Some(name) = patch.get("preset").and_then(|p| p.as_str()) {
        let preset = LayoutOptions::preset(name).ok_or("Unknown preset")?;
        options.candidates = preset.candidates;
        options.routing_candidate = preset.routing_candidate;
    }
    let mut merged = serde_json::to_value(&options).map_err(|e| e.to_string())?;
    for (key, value) in patch.as_object().ok_or("Options must be an object")? {
        if key == "preset" {
            continue;
        }
        if key == "containerSizing" {
            // Per container (ADR 0010): an entry replaces that container's size, null removes it, and the others stay.
            let entries = value
                .as_object()
                .ok_or("containerSizing must be an object keyed by container ID")?;
            let mut sizing = merged
                .get("containerSizing")
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default();
            for (id, entry) in entries {
                if entry.is_null() {
                    sizing.remove(id);
                } else {
                    sizing.insert(id.clone(), entry.clone());
                }
            }
            merged["containerSizing"] = serde_json::Value::Object(sizing);
            continue;
        }
        merged[key] = value.clone();
    }
    let mut options: LayoutOptions = serde_json::from_value(merged).map_err(|e| e.to_string())?;
    if patch.get("singlePassAuto") == Some(&serde_json::Value::Bool(true)) {
        options = single_pass_options(&options);
    }
    options.normalize_container_sizing();
    options.validate_container_sizing()?;
    validate_pins(&snapshot.graph, &options.pins)?;
    validate_order_hints(&snapshot.graph, &options.order_hints)?;
    recompute_with(snapshot, &options)
}

/// A drag of one node on a drawing that single pass made (`single_pass_auto`), which has no pins. Its steps only move the
/// node, as a preview, so that the drawing stays a single-pass one. A release turns the drop into an order hint: the node
/// stands after the sibling whose place across the flow is before the drop's, in its rank, and the layout is made again
/// with single pass. A finish without release pins the node where it was dropped, which leaves single pass. A cancel puts
/// the node back. `None` for every other gesture, which the pins handle.
fn move_on_single_pass(snapshot: &Snapshot, step: &MoveStep) -> Option<Result<Snapshot, String>> {
    let options = &snapshot.options;
    let active = snapshot.gesture.as_ref().filter(|g| g.active);
    let preview = |position: &Point, gesture: Gesture| -> Result<Snapshot, String> {
        let mut result = snapshot.clone();
        let rect = result
            .boxes
            .get_mut(&step.node)
            .ok_or_else(|| format!("Unknown node: {}", step.node))?;
        rect.x = position.x;
        rect.y = position.y;
        result.base_revision = Some(snapshot.revision);
        result.layout_version = snapshot
            .layout_version
            .checked_add(1)
            .ok_or("Layout version overflow")?;
        result.gesture = Some(gesture);
        Ok(result)
    };
    match step.phase {
        MovePhase::Begin => {
            if !(options.single_pass && options.single_pass_auto)
                || !step.others.is_empty()
                || active.is_some()
            {
                return None;
            }
            Some((|| {
                if let Some(last) = snapshot
                    .gesture
                    .as_ref()
                    .filter(|g| g.id == step.gesture_id)
                {
                    if step.seq <= last.seq {
                        return Err(
                            "Stale move step: the gesture already applied a later step".into()
                        );
                    }
                }
                let position = step
                    .position
                    .as_ref()
                    .ok_or("A move begins with a position")?;
                let at = snapshot
                    .boxes
                    .get(&step.node)
                    .ok_or_else(|| format!("Unknown node: {}", step.node))?;
                preview(
                    position,
                    Gesture {
                        id: step.gesture_id.clone(),
                        seq: step.seq,
                        node: step.node.clone(),
                        active: true,
                        previous: options.pins.get(&step.node).cloned(),
                        previous_others: BTreeMap::new(),
                        ending: false,
                        origin: Some(Point { x: at.x, y: at.y }),
                    },
                )
            })())
        }
        MovePhase::Update | MovePhase::Finish | MovePhase::Cancel => {
            let open = active.filter(|g| g.id == step.gesture_id && g.node == step.node)?;
            let origin = open.origin.clone()?;
            Some((|| {
                if step.seq <= open.seq {
                    return Err("Stale move step: the gesture already applied a later step".into());
                }
                let mut next = open.clone();
                next.seq = step.seq;
                if let MovePhase::Update = step.phase {
                    let position = step.position.as_ref().ok_or("An update needs a position")?;
                    return preview(position, next);
                }
                next.active = false;
                next.origin = None;
                let dropped = snapshot
                    .boxes
                    .get(&step.node)
                    .cloned()
                    .ok_or("Unknown node")?;
                let dropped = step.position.clone().unwrap_or(Point {
                    x: dropped.x,
                    y: dropped.y,
                });
                // The layout continues the drawing from before the gesture.
                let mut from = snapshot.clone();
                if let Some(rect) = from.boxes.get_mut(&step.node) {
                    rect.x = origin.x;
                    rect.y = origin.y;
                }
                from.gesture = None;
                let mut options = snapshot.options.clone();
                match step.phase {
                    MovePhase::Cancel => {
                        let mut result = preview(&origin, next)?;
                        result.gesture.as_mut().expect("set by preview").active = false;
                        return Ok(result);
                    }
                    MovePhase::Finish if !step.release => {
                        options.pins.insert(step.node.clone(), dropped);
                    }
                    _ => {
                        if let Some(hint) = drop_order(snapshot, &step.node, &dropped) {
                            options.order_hints.insert(step.node.clone(), hint);
                        }
                    }
                }
                validate_pins(&snapshot.graph, &options.pins)?;
                let mut result = recompute_with(&from, &options)?;
                result.gesture = Some(next);
                Ok(result)
            })())
        }
    }
}

/// The order a drop chooses in the node's rank: after the last other node of the rank whose middle across the flow lies
/// before the middle of the dropped box, in the frame of its parent. `None` when the node is in no stored rank.
fn drop_order(snapshot: &Snapshot, node: &str, dropped: &Point) -> Option<arqlet_model::OrderHint> {
    let parent = snapshot
        .graph
        .nodes
        .iter()
        .find(|n| n.id == node)?
        .parent_id
        .clone()
        .unwrap_or_else(|| "<root>".into());
    let plan = snapshot.layered_plans.get(&parent)?;
    let rank = plan
        .ranks
        .iter()
        .find(|ids| ids.iter().any(|id| id == node))?;
    let size = snapshot.boxes.get(node)?;
    let middle = |x: f64, y: f64, width: f64, height: f64| {
        if plan.vertical {
            x + width / 2.
        } else {
            y + height / 2.
        }
    };
    let drop = middle(dropped.x, dropped.y, size.width, size.height);
    let after = rank
        .iter()
        .filter(|id| id.as_str() != node)
        .filter_map(|id| {
            snapshot
                .boxes
                .get(id)
                .map(|r| (id, middle(r.x, r.y, r.width, r.height)))
        })
        .filter(|(_, at)| *at < drop)
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id.clone());
    Some(arqlet_model::OrderHint { after })
}

/// Applies one step of a move gesture and recomputes the layout with the pins it leaves. The recomputation is the full
/// one: an interactive preview is the host's job, and the engine's result is the binding geometry.
pub fn move_node(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    step: &MoveStep,
) -> Result<Snapshot, String> {
    check_current(snapshot, expected, layout)?;
    if let Some(result) = move_on_single_pass(snapshot, step) {
        return result;
    }
    let mut options = snapshot.options.clone();
    let current = snapshot.gesture.clone();
    let active = current.as_ref().filter(|g| g.active);
    let gesture = match step.phase {
        MovePhase::Begin => {
            if active.is_some() {
                return Err("Another move gesture is active; finish or cancel it first".into());
            }
            if let Some(last) = current.as_ref().filter(|g| g.id == step.gesture_id) {
                if step.seq <= last.seq {
                    return Err("Stale move step: the gesture already applied a later step".into());
                }
            }
            let position = step
                .position
                .clone()
                .ok_or("A move begins with a position")?;
            // A container moves like a leaf: its pin is in its parent's frame, and its children keep their positions in
            // its frame, so they move with it (ADR 0009, section 4).
            let previous = options.pins.get(&step.node).cloned();
            options.pins.insert(step.node.clone(), position);
            if step.others.contains_key(&step.node) {
                return Err("The moved nodes name the grabbed node twice".into());
            }
            let mut previous_others = BTreeMap::new();
            for (id, at) in &step.others {
                previous_others.insert(id.clone(), options.pins.get(id).cloned());
                options.pins.insert(id.clone(), at.clone());
            }
            Gesture {
                id: step.gesture_id.clone(),
                seq: step.seq,
                node: step.node.clone(),
                active: true,
                previous,
                previous_others,
                ending: false,
                origin: None,
            }
        }
        MovePhase::Update | MovePhase::Finish | MovePhase::Cancel => {
            let open = active
                .filter(|g| g.id == step.gesture_id && g.node == step.node)
                .ok_or("No active move gesture with this id for this node")?;
            if step.seq <= open.seq {
                return Err("Stale move step: the gesture already applied a later step".into());
            }
            let mut next = open.clone();
            next.seq = step.seq;
            // The moved set is the one the gesture began with; an update names the same nodes.
            let began: BTreeSet<&String> = open.previous_others.keys().collect();
            if matches!(step.phase, MovePhase::Update)
                && step.others.keys().collect::<BTreeSet<_>>() != began
            {
                return Err("An update must move the nodes the gesture began with".into());
            }
            match step.phase {
                MovePhase::Update => {
                    let position = step.position.clone().ok_or("An update needs a position")?;
                    options.pins.insert(step.node.clone(), position);
                    for (id, at) in &step.others {
                        options.pins.insert(id.clone(), at.clone());
                    }
                }
                MovePhase::Finish => {
                    if step.release {
                        options.pins.remove(&step.node);
                        for id in open.previous_others.keys() {
                            options.pins.remove(id);
                        }
                    }
                    next.active = false;
                }
                MovePhase::Cancel => {
                    match &open.previous {
                        Some(previous) => {
                            options.pins.insert(step.node.clone(), previous.clone());
                        }
                        None => {
                            options.pins.remove(&step.node);
                        }
                    }
                    for (id, previous) in &open.previous_others {
                        match previous {
                            Some(previous) => {
                                options.pins.insert(id.clone(), previous.clone());
                            }
                            None => {
                                options.pins.remove(id);
                            }
                        }
                    }
                    next.active = false;
                }
                MovePhase::Begin => unreachable!("begin is handled above"),
            }
            next
        }
    };
    validate_pins(&snapshot.graph, &options.pins)?;
    // The layout reads an active gesture of the snapshot it continues as "this is a step of a drag" and repairs locally. The
    // steps between the begin and the end are such steps; the step that ends the gesture continues a snapshot marked as
    // ending (a moderate repair), and the begin one without the gesture (a complete repair).
    let ended;
    let from = match step.phase {
        MovePhase::Update => snapshot,
        // The step that ends the gesture continues from a snapshot marked as ending: a moderate repair.
        MovePhase::Finish | MovePhase::Cancel => {
            ended = Snapshot {
                gesture: snapshot
                    .gesture
                    .clone()
                    .map(|g| Gesture { ending: true, ..g }),
                ..snapshot.clone()
            };
            &ended
        }
        MovePhase::Begin => {
            ended = Snapshot {
                gesture: None,
                ..snapshot.clone()
            };
            &ended
        }
    };
    let mut result = recompute_with(from, &options)?;
    result.gesture = Some(gesture);
    Ok(result)
}

/// Pins name nodes of the graph, leaves or containers, and carry finite positions in the parent's frame. A pinned
/// container keeps its place; its children are still laid out inside it. An unknown node is refused explicitly.
/// An order hint names a node and a sibling under the same parent (`LayoutOptions::order_hints`).
pub fn validate_order_hints(
    graph: &Graph,
    hints: &BTreeMap<String, arqlet_model::OrderHint>,
) -> Result<(), String> {
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    for (id, hint) in hints {
        let parent = parents
            .get(id.as_str())
            .ok_or_else(|| format!("Order hint names an unknown node: {id}"))?;
        if let Some(after) = &hint.after {
            if after == id || parents.get(after.as_str()) != Some(parent) {
                return Err(format!(
                    "Order hint of {id} must name a sibling, not {after}"
                ));
            }
        }
    }
    Ok(())
}

pub fn validate_pins(graph: &Graph, pins: &BTreeMap<String, Point>) -> Result<(), String> {
    for (id, point) in pins {
        if !graph.nodes.iter().any(|node| node.id == *id) {
            return Err(format!("Pin names an unknown node: {id}"));
        }
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(format!("Pin position of {id} must be finite"));
        }
    }
    Ok(())
}

/// Merges the measured leaf sizes reported by the visualization and recomputes the layout.
pub fn measure_snapshot(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    sizes: BTreeMap<String, NodeSize>,
) -> Result<Snapshot, String> {
    check_current(snapshot, expected, layout)?;
    let known: BTreeSet<&str> = snapshot.graph.nodes.iter().map(|n| n.id.as_str()).collect();
    for (id, size) in &sizes {
        if !known.contains(id.as_str()) {
            return Err(format!("Unknown node in measurement: {id}"));
        }
        if ![size.width, size.height]
            .iter()
            .all(|v| v.is_finite() && *v > 0.)
        {
            return Err(format!("Invalid measurement for {id}"));
        }
    }
    let mut options = snapshot.options.clone();
    options.node_sizes.extend(sizes);
    recompute_with(snapshot, &options)
}

/// Applies geometry observations from the consumer: for each element the realized outline and the revision of that
/// measurement. It is the typed form of `measure_snapshot` and updates the same state (`node_sizes`), plus the latest revision
/// per element. Nothing is applied when any measurement is refused:
/// - an unknown element, a non-finite or non-positive extent, or an outline of another kind than the node's shape (the shape
///   is a requirement; change it with `set_shape` first);
/// - a revision older than the one already applied for that element, which is a late measurement and is never applied silently.
///   The same revision again is accepted, so a retry is harmless.
pub fn measure_contours(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
    measurements: BTreeMap<String, Measurement>,
) -> Result<Snapshot, String> {
    check_current(snapshot, expected, layout)?;
    let shapes: BTreeMap<&str, NodeShape> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.shape))
        .collect();
    let mut refused = Vec::new();
    for (id, measurement) in &measurements {
        let Some(&shape) = shapes.get(id.as_str()) else {
            return Err(format!("Unknown node in measurement: {id}"));
        };
        if !measurement.contour.is_valid() {
            return Err(format!("Invalid measurement for {id}"));
        }
        if !measurement.contour.fits(shape) {
            return Err(format!(
                "The reported outline of {id} does not match its shape ({shape:?}); change the shape first"
            ));
        }
        if let Some(&current) = snapshot.options.geometry_revisions.get(id) {
            if measurement.geometry_revision < current {
                refused.push(format!(
                    "{id} (revision {}, current {current})",
                    measurement.geometry_revision
                ));
            }
        }
    }
    if !refused.is_empty() {
        return Err(format!("Stale measurement for {}", refused.join(", ")));
    }
    let mut options = snapshot.options.clone();
    for (id, measurement) in measurements {
        let size = measurement.contour.size();
        // The size in use: an earlier report, else the box the layout gave the node.
        let in_use = snapshot
            .options
            .node_sizes
            .get(&id)
            .map(|s| (s.width, s.height))
            .or_else(|| snapshot.boxes.get(&id).map(|b| (b.width, b.height)));
        let unchanged = in_use.is_some_and(|(w, h)| {
            (size.width - w).abs() <= MEASURE_TOLERANCE
                && (size.height - h).abs() <= MEASURE_TOLERANCE
        });
        if !unchanged {
            options.node_sizes.insert(id.clone(), size);
        }
        options
            .geometry_revisions
            .insert(id, measurement.geometry_revision);
    }
    recompute_with(snapshot, &options)
}

/// Compare bounded ordering/placement candidates using geometry and stability.
/// Graph revision stays unchanged while layout version advances.
pub fn refine_snapshot(
    snapshot: &Snapshot,
    expected: u64,
    layout_version: u64,
) -> Result<Snapshot, String> {
    refine_snapshot_with(snapshot, expected, layout_version, 1)
}

/// `threads` changes only wall-clock time. Candidates are evaluated concurrently, but the
/// selection runs in fixed candidate order, so the result is identical for every thread count.
pub fn refine_snapshot_with(
    snapshot: &Snapshot,
    expected: u64,
    layout_version: u64,
    threads: usize,
) -> Result<Snapshot, String> {
    refine_snapshot_until(snapshot, expected, layout_version, threads, None)
}

/// A supplied stop callback is checked before each candidate starts and inside the placement search. Candidates
/// that already started run to their next checkpoint, and completed candidates remain eligible. Parallel evaluation
/// is unchanged; only the stop signal is shared, so the selection is still fixed-order.
pub fn refine_snapshot_until(
    snapshot: &Snapshot,
    expected: u64,
    layout_version: u64,
    threads: usize,
    stop: Option<&(dyn Fn() -> bool + Sync)>,
) -> Result<Snapshot, String> {
    refine_snapshot_limited(snapshot, expected, layout_version, threads, stop, None)
}

/// Refinement with a deterministic work budget: `work` counts candidate jobs (orderings, routing, search, and the
/// global routing rounds) in their fixed order. The same budget cuts at the same job on every run and on every
/// target, unlike a time budget. `work_limited` in the report says whether the budget dropped any job.
/// Placement search has its own deterministic budget (`placementSearch`).
pub fn refine_snapshot_limited(
    snapshot: &Snapshot,
    expected: u64,
    layout_version: u64,
    threads: usize,
    stop: Option<&(dyn Fn() -> bool + Sync)>,
    work: Option<usize>,
) -> Result<Snapshot, String> {
    refinement_core(
        "",
        snapshot,
        expected,
        layout_version,
        threads,
        stop,
        work,
        &mut |_: &Publication| {},
    )
    .map(|(refined, _)| refined)
}

/// Refinement that also returns its `JobReport` (input identity, candidate IDs, aggregate and per-candidate
/// work counters). `job_id` is an opaque host value that is echoed back and never affects the result.
pub fn refine_snapshot_job(
    job_id: &str,
    snapshot: &Snapshot,
    expected: u64,
    layout_version: u64,
    threads: usize,
    stop: Option<&(dyn Fn() -> bool + Sync)>,
    work: Option<usize>,
) -> Result<(Snapshot, JobReport), String> {
    refine_snapshot_progressive(
        job_id,
        snapshot,
        expected,
        layout_version,
        threads,
        stop,
        work,
        &mut |_: &Publication| {},
    )
}

/// `refine_snapshot_job` that also publishes every accepted improvement over the input as soon as it is selected.
/// The result is the last publication, or the input. Publications never increase the conflict key, and a stop
/// signal ends the job after the candidate in progress; the host discards publications whose identity is stale.
#[allow(clippy::too_many_arguments)]
pub fn refine_snapshot_progressive(
    job_id: &str,
    snapshot: &Snapshot,
    expected: u64,
    layout_version: u64,
    threads: usize,
    stop: Option<&(dyn Fn() -> bool + Sync)>,
    work: Option<usize>,
    publish: &mut dyn FnMut(&Publication),
) -> Result<(Snapshot, JobReport), String> {
    let (mut refined, mut report) = refinement_core(
        job_id,
        snapshot,
        expected,
        layout_version,
        threads,
        stop,
        work,
        publish,
    )?;
    // Optimize-time crossing step: routes that cross each other are separated when the drawing allows it. It belongs to
    // the job (what the Optimize button delivers), not to the refinement core that the diagnostics compare against. A stop
    // signal skips it; drawings with straight edges keep their straight edges, so the step is skipped there.
    let cancelled = stop.is_some_and(|stop| stop());
    // `singlePass` constructs once: no later correction of the constructed routes.
    if snapshot.straight_edges.is_empty() && !cancelled && !snapshot.options.single_pass {
        let started = stage_timer_start();
        if let Some(mut better) = arqlet_layout::relieve_crossings(&refined)? {
            // The report describes the drawing that is delivered, so its `after` quality is taken again.
            let after =
                arqlet_layout::quality_with(&better, snapshot, &snapshot.options.objectives);
            if let Some(report) = better.optimization.as_mut() {
                report.after = after;
            }
            refined = better;
        }
        report.counters.relieve_nanos = stage_elapsed_nanos(started);
    }
    Ok((refined, report))
}

/// The refinement without the Optimize-time crossing step: the candidates and the selection, and the straight edges of the
/// input. The diagnostics (candidate logs, archives) and the layout tests compare against this.
#[allow(clippy::too_many_arguments)]
fn refinement_core(
    job_id: &str,
    snapshot: &Snapshot,
    expected: u64,
    layout_version: u64,
    threads: usize,
    stop: Option<&(dyn Fn() -> bool + Sync)>,
    work: Option<usize>,
    publish: &mut dyn FnMut(&Publication),
) -> Result<(Snapshot, JobReport), String> {
    let (refined, report, _, _) = refine_logged(
        job_id,
        snapshot,
        expected,
        layout_version,
        threads,
        stop,
        work,
        false,
        publish,
    )?;
    // A straight edge that the snapshot declares stays straight: the orthogonal routes of the refinement do not replace it.
    // The set comes from the input: the refinement's own result does not carry it.
    let mut refined = refined;
    if !snapshot.straight_edges.is_empty() {
        let straight: BTreeSet<String> = snapshot.straight_edges.iter().cloned().collect();
        arqlet_layout::apply_straight_edges(&mut refined, &straight)?;
    }
    Ok((refined, report))
}

/// Refinement with the candidate log: every complete candidate of the run, in reduction order. The result is the same
/// snapshot `refine_snapshot_progressive` returns for the same budget; the log is diagnostics only.
pub fn refine_snapshot_logged(
    snapshot: &Snapshot,
    expected: u64,
    layout_version: u64,
    threads: usize,
    work: Option<usize>,
) -> Result<(Snapshot, JobReport, Vec<CandidateRecord>), String> {
    refine_logged(
        "logged",
        snapshot,
        expected,
        layout_version,
        threads,
        None,
        work,
        true,
        &mut |_| {},
    )
    .map(|(refined, report, records, _)| (refined, report, records))
}

/// The progressive refinement. With `logging`, each candidate the reduction sees is also recorded; the recording only
/// reads values the selection already has, so it never changes the selected candidate.
fn refine_logged(
    job_id: &str,
    snapshot: &Snapshot,
    expected: u64,
    layout_version: u64,
    threads: usize,
    stop: Option<&(dyn Fn() -> bool + Sync)>,
    work: Option<usize>,
    logging: bool,
    publish: &mut dyn FnMut(&Publication),
) -> Result<(Snapshot, JobReport, Vec<CandidateRecord>, Vec<ArchiveEntry>), String> {
    let interrupted = std::sync::atomic::AtomicBool::new(false);
    let should_stop = || {
        if interrupted.load(std::sync::atomic::Ordering::Relaxed) {
            return true;
        }
        if stop.is_some_and(|f| f()) {
            interrupted.store(true, std::sync::atomic::Ordering::Relaxed);
            return true;
        }
        false
    };
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    if snapshot.revision != expected || snapshot.layout_version != layout_version {
        return Err("Stale refinement request".into());
    }
    if snapshot.graph.nodes.len() > 1000 || snapshot.graph.edges.len() > 2000 {
        return Err("Prototype refinement size limit exceeded".into());
    }
    validate_requirements(snapshot)?;
    validate(&snapshot.graph)?;
    if snapshot.options.abstract_routing {
        arqlet_layout::constructive::abstract_realization::completion::check_capability(snapshot)?;
    }
    if snapshot
        .graph
        .nodes
        .iter()
        .any(|n| !snapshot.boxes.contains_key(&n.id))
    {
        return Err("Incomplete snapshot geometry".into());
    }
    for r in snapshot.boxes.values() {
        if ![r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite())
            || r.width <= 0.
            || r.height <= 0.
        {
            return Err("Invalid geometry".into());
        }
    }
    let current_issues = arqlet_scene::validate_geometry(
        &snapshot.graph,
        &snapshot.boxes,
        arqlet_scene::GeometryRules::default(),
    );
    let input_valid = current_issues.is_empty();
    let mut have_valid = input_valid;
    let before = arqlet_layout::quality_with(snapshot, snapshot, &snapshot.options.objectives);
    let mut best = before.clone();
    let mut candidate = snapshot.clone();
    candidate.geometry_issues = current_issues;
    candidate.recomputed_containers.clear();
    let mut strategy = "keep-existing".to_string();
    // Ordering strategies plus routing-only; each job depends only on the input snapshot.
    let (planned, excluded) = plan_first_stage(snapshot);
    let (jobs, mut work_limited) = allocate_first_stage(planned, work);
    let mut work_used = jobs.len();
    let planned: Vec<(CandidateId, Job)> = candidate_ids(&jobs).into_iter().zip(jobs).collect();
    let mut completed = Vec::new();
    let mut candidate_work = Vec::new();
    let mut routing_rounds = 0;
    let mut work_total = WorkCounters::default();
    let mut records: Vec<CandidateRecord> = Vec::new();
    // Candidates are chosen by the shared conflict key (the published result's measure), not by the engine's
    // quality: a candidate that cannot be measured is not eligible.
    // One measurement of the input gives its key and, when the input is kept, its near-node pairs.
    // `singlePass` plans no candidate and no global round, so nothing is compared with the key and the input is kept.
    let compared = !(snapshot.options.single_pass && planned.is_empty());
    let input_evaluation = if input_valid && compared {
        arqlet_layout::published_evaluation(snapshot).ok()
    } else {
        None
    };
    let input_hugs = input_evaluation.as_ref().map(|(_, m)| m.hug_pairs);
    let mut best_key = input_evaluation.map(|(key, _)| key);
    // The input's key is measured once; `best_key` always holds the key of `candidate`, so the gate reuses both.
    let input_key = best_key.clone();
    // Whether the incumbent's published drawing has no near-node pairs, when its evaluation measured it.
    let mut best_clear: Option<bool> = None;
    let input = InputIdentity::new(
        snapshot.revision,
        snapshot.layout_version,
        &snapshot.options,
    )?;
    let mut sequence = 0;
    // Publishes an accepted improvement over the input. It carries the same identity fields the final result gets.
    let mut archive = Archive::default();
    let mut emit = |option: &Snapshot,
                    name: &str,
                    key: Vec<i64>,
                    quality: &arqlet_model::Quality,
                    evaluated: usize,
                    archive: &[ArchiveEntry]| {
        sequence += 1;
        let mut published = option.clone();
        published.source_graph = snapshot.source_graph.clone();
        published.collapsed_ids = snapshot.collapsed_ids.clone();
        published.revision = snapshot.revision;
        published.base_revision = Some(snapshot.revision);
        published.layout_version = snapshot.layout_version.saturating_add(1);
        published.omitted_unresolved_edges = snapshot.omitted_unresolved_edges;
        // The report of an earlier edit does not describe a refined result.
        published.edit_report = None;
        published.optimization = Some(OptimizationReport {
            before: before.clone(),
            after: quality.clone(),
            candidates_evaluated: evaluated,
            interrupted: false,
            work_limited: false,
            stop_reason: None,
            strategy: name.to_string(),
        });
        // The score of the published result: its shadow tiers, measured on the same drawing as its key.
        let shadow = arqlet_layout::shadow_evaluation(&published).ok();
        publish(&Publication {
            archive: archive.to_vec(),
            hard_violations: shadow.as_ref().map(|e| e.hard_violations),
            shadow_total: shadow.as_ref().map(|e| e.total),
            job_id: job_id.to_string(),
            sequence,
            strategy: name.to_string(),
            key,
            candidates_evaluated: evaluated,
            input: input.clone(),
            snapshot: published,
        });
    };
    // Candidates are evaluated in batches of `threads` (one at a time on a single thread). A batch runs concurrently,
    // and its results are reduced in configured order, so the selection is identical for every batch size. A candidate
    // that has not started when the stop signal is set is skipped; a running one stops at its own checkpoints.
    let mut started = 0;
    let mut count = 1;
    // On WASM the candidates of a batch run one after another, so a batch of more than one only delays the first result.
    let batch_size = if cfg!(target_arch = "wasm32") {
        1
    } else {
        threads.max(1)
    };
    'candidates: for batch in planned.chunks(batch_size) {
        if should_stop() {
            break;
        }
        let evaluated = parallel_map(batch, threads, |(id, job)| {
            if should_stop() {
                return None;
            }
            let mut work = WorkCounters::default();
            let result =
                evaluate_against_until(snapshot, snapshot, *job, None, &should_stop, &mut work);
            // A candidate that finished while the stop signal was set is discarded: it may be an interrupted search.
            let finished = (!should_stop()).then_some(result);
            Some((id.clone(), work, finished))
        });
        let measured = evaluated.iter().flatten().count();
        started += measured;
        count += measured;
        for (id, work, finished) in evaluated.into_iter().flatten() {
            // Once the stop signal fires, no further candidate is measured or selected.
            if should_stop() {
                break 'candidates;
            }
            work_total.add_work(&work);
            candidate_work.push(execution::CandidateWork {
                candidate: id.clone(),
                counters: work.clone(),
            });
            let Some(result) = finished else {
                continue;
            };
            completed.push(id.clone());
            let origin = Origin {
                input: &input,
                phase: "candidate",
                round: None,
                id: Some(id),
                parent: "input".into(),
            };
            let Some((name, option, q, key, clear)) = result? else {
                archive.skip();
                if logging {
                    log_candidate(&mut records, &origin, None, &work, None, None);
                }
                continue;
            };
            archive.observe(&name, &option, key.as_deref());
            let index = logging.then(|| {
                log_candidate(
                    &mut records,
                    &origin,
                    Some(&name),
                    &work,
                    Some(&option),
                    key.clone(),
                )
            });
            let Some(key) = key else {
                continue;
            };
            if !have_valid
                || best_key
                    .as_ref()
                    .is_none_or(|known| arqlet_layout::improves(&key, known))
            {
                have_valid = true;
                let improves_input = input_key
                    .as_ref()
                    .is_none_or(|known| arqlet_layout::improves(&key, known));
                best_key = Some(key.clone());
                best_clear = clear;
                best = q.clone();
                candidate = option;
                strategy = name.clone();
                mark(&mut records, index, |r| r.accepted = true);
                if improves_input {
                    emit(&candidate, &name, key, &best, count, &archive.metadata());
                    mark(&mut records, index, |r| r.published = true);
                }
            }
        }
    }
    // Archive refinement (opt-in): the archive entries after the incumbent each get one routing pass, in archive order.
    // The entries are chosen by their archive rank alone, not by whether they were published; the incumbent's geometry is
    // skipped. A pass that beats the incumbent wins under the shared key, like any round.
    if snapshot.options.archive_refinement > 0 && !should_stop() {
        let incumbent = geometry_fingerprint(&candidate);
        let mut runners: Vec<(String, Snapshot)> = archive
            .slots
            .iter()
            .filter(|(entry, _)| Some(&entry.geometry_fingerprint) != incumbent.as_ref())
            .take(snapshot.options.archive_refinement)
            .map(|(entry, option)| (entry.strategy.clone(), option.clone()))
            .collect();
        let allowed = work.map_or(usize::MAX, |limit| limit.saturating_sub(work_used));
        if runners.len() > allowed {
            runners.truncate(allowed);
            work_limited = true;
        }
        work_used += runners.len();
        count += runners.len();
        let results = parallel_map(&runners, threads, |(name, option)| {
            let mut work = WorkCounters::default();
            let (routed, search) = arqlet_layout::reroute_counted(option, true, &should_stop);
            work.absorb_search(&search);
            (name.clone(), work, routed)
        });
        // An interrupted pass is discarded whole, like an interrupted round.
        if !should_stop() {
            let mut winner: Option<(
                String,
                Snapshot,
                arqlet_model::Quality,
                Vec<i64>,
                Option<usize>,
            )> = None;
            for (name, work, routed) in results {
                work_total.add_work(&work);
                let origin = Origin {
                    input: &input,
                    phase: "archive",
                    round: None,
                    id: None,
                    parent: name.clone(),
                };
                let Ok(option) = routed else {
                    archive.skip();
                    continue;
                };
                let routed_name = format!("routed-{name}");
                let key = arqlet_layout::published_key(&option).ok();
                archive.observe(&routed_name, &option, key.as_deref());
                let index = logging.then(|| {
                    log_candidate(
                        &mut records,
                        &origin,
                        Some(&routed_name),
                        &work,
                        Some(&option),
                        key.clone(),
                    )
                });
                let Some(key) = key else {
                    continue;
                };
                let beats_best = best_key
                    .as_ref()
                    .is_none_or(|known| arqlet_layout::improves(&key, known));
                let beats_winner = winner
                    .as_ref()
                    .is_none_or(|(_, _, _, known, _)| arqlet_layout::improves(&key, known));
                if beats_best && beats_winner {
                    let q = arqlet_layout::quality_with(
                        &option,
                        snapshot,
                        &snapshot.options.objectives,
                    );
                    winner = Some((routed_name, option, q, key, index));
                }
            }
            if let Some((name, option, q, key, index)) = winner {
                let improves_input = input_key
                    .as_ref()
                    .is_none_or(|known| arqlet_layout::improves(&key, known));
                have_valid = true;
                best = q.clone();
                best_key = Some(key.clone());
                best_clear = None;
                candidate = option;
                strategy = name.clone();
                mark(&mut records, index, |r| r.accepted = true);
                if improves_input {
                    emit(&candidate, &name, key, &best, count, &archive.metadata());
                    mark(&mut records, index, |r| r.published = true);
                }
            }
        }
    }
    // `singlePass` excludes the routing jobs from the first stage; the global rounds rerun them, so they are skipped too.
    let global_rounds = if snapshot.options.single_pass {
        0
    } else {
        GLOBAL_ROUNDS
    };
    for _ in 0..global_rounds {
        if should_stop() {
            break;
        }
        let channel = snapshot.options.routing_engine == RoutingEngine::Channel;
        let round_jobs: Vec<Job> = [Job::Routing, Job::RoutingLast]
            .into_iter()
            .filter(|job| {
                let capabilities = job.capabilities();
                capabilities.global_rounds && (channel || !capabilities.channel_engine)
            })
            .collect();
        if work.is_some_and(|limit| work_used + round_jobs.len() > limit) {
            work_limited = true;
            break;
        }
        work_used += round_jobs.len();
        count += 1;
        routing_rounds += 1;
        let current = candidate.clone();
        // The incumbent's measurement, against the same reference as the round's results.
        let known_key = best_key.clone();
        let known = known_key.as_deref().map(|key| (&best, key, best_clear));
        let results = parallel_map(&round_jobs, threads, |&job| {
            let mut work = WorkCounters::default();
            let result =
                evaluate_against_until(&current, snapshot, job, known, &should_stop, &mut work);
            (work, result)
        });
        // An interrupted round is discarded whole; the last complete candidate stays the result.
        if should_stop() {
            break;
        }
        let mut round: Option<(
            String,
            Snapshot,
            arqlet_model::Quality,
            Vec<i64>,
            Option<bool>,
            Option<usize>,
        )> = None;
        for (work, result) in results {
            work_total.add_work(&work);
            let origin = Origin {
                input: &input,
                phase: "round",
                round: Some(routing_rounds),
                id: None,
                parent: strategy.clone(),
            };
            let Some((name, option, q, key, clear)) = result? else {
                archive.skip();
                if logging {
                    log_candidate(&mut records, &origin, None, &work, None, None);
                }
                continue;
            };
            archive.observe(&name, &option, key.as_deref());
            let index = logging.then(|| {
                log_candidate(
                    &mut records,
                    &origin,
                    Some(&name),
                    &work,
                    Some(&option),
                    key.clone(),
                )
            });
            let Some(key) = key else {
                continue;
            };
            let beats_best = best_key
                .as_ref()
                .is_none_or(|known| arqlet_layout::improves(&key, known));
            let beats_round = round
                .as_ref()
                .is_none_or(|(_, _, _, r, _, _)| arqlet_layout::improves(&key, r));
            if beats_best && beats_round {
                round = Some((name, option, q, key, clear, index));
            }
        }
        let Some((name, option, q, key, clear, index)) = round else {
            break;
        };
        let improves_input = input_key
            .as_ref()
            .is_none_or(|known| arqlet_layout::improves(&key, known));
        have_valid = true;
        best = q.clone();
        best_key = Some(key.clone());
        best_clear = clear;
        candidate = option;
        strategy = name.clone();
        mark(&mut records, index, |r| r.accepted = true);
        if improves_input {
            emit(&candidate, &name, key, &best, count, &archive.metadata());
            mark(&mut records, index, |r| r.published = true);
        }
    }
    // Published-result gate: the refined result replaces the input only if its shared conflict key is strictly
    // smaller in the configured objective order. An equal key keeps the input; so does a failed measurement.
    if input_valid {
        if let (Some(input_key), Some(output_key)) = (input_key, best_key.clone()) {
            if output_key >= input_key && strategy != "keep-existing" {
                candidate = snapshot.clone();
                best = before.clone();
                strategy = "keep-existing".into();
            }
        }
    }
    // The final result is the last accepted candidate, unless the gate kept the input.
    if strategy != "keep-existing" {
        let last = records.iter().rposition(|r| r.accepted);
        mark(&mut records, last, |r| r.selected = true);
    }
    if !have_valid {
        let diagnostic = format!(
            "No valid placement candidate; retained the existing geometry with {} reported geometry issue(s)",
            candidate.geometry_issues.len()
        );
        if !candidate.routing_issues.contains(&diagnostic) {
            candidate.routing_issues.push(diagnostic);
        }
    }
    // A required clearance that the result still violates is reported, never hidden: the refinement keeps the input when
    // no candidate qualifies, and the diagnostic says why the input did not meet the requirement.
    let known_hugs = if strategy == "keep-existing" {
        input_hugs
    } else {
        None
    };
    for issue in [
        unmet_clearance_issue(&candidate, known_hugs),
        extent_issue(&candidate),
    ]
    .into_iter()
    .flatten()
    {
        if !candidate.routing_issues.contains(&issue) {
            candidate.routing_issues.push(issue);
        }
    }
    candidate.edit_report = None;
    candidate.optimization = Some(OptimizationReport {
        before,
        after: best,
        candidates_evaluated: count,
        interrupted: should_stop(),
        work_limited,
        // A stop callback without a time budget is a cancellation; the time-budget path relabels it below.
        stop_reason: if should_stop() {
            Some(arqlet_model::StopReason::Cancelled)
        } else if work_limited {
            Some(arqlet_model::StopReason::WorkBudget)
        } else {
            None
        },
        strategy,
    });
    candidate.source_graph = snapshot.source_graph.clone();
    candidate.collapsed_ids = snapshot.collapsed_ids.clone();
    candidate.revision = snapshot.revision;
    candidate.layout_version = snapshot
        .layout_version
        .checked_add(1)
        .ok_or("Layout version overflow")?;
    candidate.base_revision = Some(snapshot.revision);
    candidate.omitted_unresolved_edges = snapshot.omitted_unresolved_edges;
    let report = JobReport {
        job_id: job_id.to_string(),
        input: InputIdentity::new(
            snapshot.revision,
            snapshot.layout_version,
            &snapshot.options,
        )?,
        excluded: excluded.clone(),
        counters: WorkCounters {
            candidates_started: started,
            candidates_completed: completed.len(),
            routing_rounds,
            ..work_total
        },
        candidate_work,
        completed,
    };
    Ok((candidate, report, records, archive.metadata()))
}

/// Where a logged candidate came from.
struct Origin<'a> {
    input: &'a InputIdentity,
    phase: &'static str,
    round: Option<usize>,
    id: Option<CandidateId>,
    parent: String,
}

/// Appends one record and returns its index. `option` is `None` when the job produced no candidate; `key` is `None`
/// when the candidate could not be measured.
fn log_candidate(
    records: &mut Vec<CandidateRecord>,
    origin: &Origin,
    name: Option<&str>,
    work: &WorkCounters,
    option: Option<&Snapshot>,
    key: Option<Vec<i64>>,
) -> usize {
    let metrics = option.and_then(|s| arqlet_layout::published_metrics(s).ok());
    let required = option.is_some_and(|o| o.options.required_clearance.is_some());
    let shadow = metrics.as_ref().and_then(|m| {
        arqlet_scene::quality::evaluate_with(
            m,
            &arqlet_scene::quality::QualityModel::shadow_default(),
            required,
        )
        .ok()
    });
    let geometry_fingerprint = option.and_then(geometry_fingerprint);
    let duplicate_of = geometry_fingerprint.as_ref().and_then(|f| {
        records
            .iter()
            .position(|r| r.geometry_fingerprint.as_ref() == Some(f))
    });
    let outcome = match (option, &key) {
        (None, _) => CandidateOutcome::NoResult,
        (Some(_), None) => CandidateOutcome::Unmeasurable,
        (Some(_), Some(_)) => CandidateOutcome::Measured,
    };
    records.push(CandidateRecord {
        index: records.len(),
        input: origin.input.clone(),
        phase: origin.phase,
        round: origin.round,
        id: origin.id.clone(),
        strategy: name.map(String::from),
        parent: origin.parent.clone(),
        outcome,
        hard_violations: shadow.as_ref().map(|e| e.hard_violations),
        metrics,
        key,
        shadow_total: shadow.map(|e| e.total),
        work: work.clone(),
        geometry_fingerprint,
        duplicate_of,
        snapshot: option.cloned(),
        accepted: false,
        selected: false,
        published: false,
    });
    records.len() - 1
}

/// Sets a flag on the record at `index`, when there is one.
fn mark(
    records: &mut [CandidateRecord],
    index: Option<usize>,
    set: impl FnOnce(&mut CandidateRecord),
) {
    if let Some(record) = index.and_then(|i| records.get_mut(i)) {
        set(record);
    }
}

/// FNV-1a over the serialized boxes and routes: equal fingerprints mean identical drawn geometry.
fn geometry_fingerprint(snapshot: &Snapshot) -> Option<String> {
    let bytes = serde_json::to_vec(&(&snapshot.boxes, &snapshot.routes)).ok()?;
    let hash = bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3)
    });
    Some(format!("{hash:016x}"))
}

/// Plans the first stage from the strategies' declared capabilities against the view's options and structure. Returns the
/// planned jobs, in the order they always ran, and the exclusions: each strategy that is not planned, with the reason.
/// Options state what is requested or allowed; the planner decides what applies to this view. Budget cuts are not
/// exclusions; they are reported by `work_limited`.
fn plan_first_stage(snapshot: &Snapshot) -> (Vec<Job>, Vec<Exclusion>) {
    let options = &snapshot.options;
    let channel = options.routing_engine == RoutingEngine::Channel;
    let has_containers = snapshot.graph.nodes.iter().any(|n| n.parent_id.is_some());
    let mut planned = Vec::new();
    let mut excluded = Vec::new();
    let mut exclude = |job: Job, reason: &str| {
        excluded.push(Exclusion {
            strategy: job.capabilities().name,
            reason: reason.to_string(),
        })
    };
    if options.single_pass {
        for job in [
            Job::Ordering(0),
            Job::Routing,
            Job::RoutingLast,
            Job::RoutingExtended,
            Job::Search,
            Job::FlatRegion,
            Job::OtherPortPlacement,
            Job::RowPortMiddle,
            Job::UnpackedComponents,
            Job::RoutedOrdering(0),
        ] {
            exclude(job, "singlePass constructs once");
        }
        return (planned, excluded);
    }
    planned.extend(options.candidates.iter().map(|m| Job::Ordering(*m)));
    if options.routing_candidate {
        planned.push(Job::Routing);
        if channel {
            planned.push(Job::RoutingLast);
        } else {
            exclude(Job::RoutingLast, "needs the channel routing engine");
        }
    } else {
        exclude(Job::Routing, "routingCandidate is off");
        exclude(Job::RoutingLast, "routingCandidate is off");
    }
    if !options.extended_conflict_selection {
        exclude(Job::RoutingExtended, "extendedConflictSelection is off");
    } else if !channel {
        exclude(Job::RoutingExtended, "needs the channel routing engine");
    } else {
        planned.push(Job::RoutingExtended);
    }
    if options.placement_search > 0 {
        planned.push(Job::Search);
    } else {
        exclude(Job::Search, "placementSearch is 0");
    }
    if !options.flat_region {
        exclude(Job::FlatRegion, "flatRegion is off");
    } else if has_containers {
        exclude(Job::FlatRegion, "needs a graph without containers");
    } else {
        planned.push(Job::FlatRegion);
    }
    // Optimized ports are a preference, not a rule: the symmetric layout of the same boxes is also a candidate, and
    // the shared key decides. The default (symmetric) has no alternative, so its outputs do not change.
    if options.port_placement == PortPlacement::Optimized {
        planned.push(Job::OtherPortPlacement);
    } else {
        exclude(Job::OtherPortPlacement, "portPlacement is not optimized");
    }
    // Row ports ordered by their peers are a preference: the middle of each row is the alternative (ADR 0011).
    let has_row_ports = snapshot.graph.ports.iter().any(|p| p.row.is_some());
    if !has_row_ports {
        exclude(Job::RowPortMiddle, "no port is bound to a row");
    } else if options.row_port_order == arqlet_model::RowPortOrder::Peer {
        planned.push(Job::RowPortMiddle);
    } else {
        exclude(Job::RowPortMiddle, "rowPortOrder is middle");
    }
    // Component packing is a preference too: the unpacked layout is the alternative, and only a container with more than
    // one component has anything to unpack.
    let multi_component = arqlet_layout::structure::analyse_graph(&snapshot.graph)
        .iter()
        .any(|p| p.structure.components.len() > 1);
    if !options.component_packing {
        exclude(Job::UnpackedComponents, "componentPacking is off");
    } else if !multi_component {
        exclude(
            Job::UnpackedComponents,
            "no container has more than one component",
        );
    } else {
        planned.push(Job::UnpackedComponents);
    }
    // Opt-in: each ordering candidate also competes in its routed form.
    if options.routed_candidates {
        planned.extend(options.candidates.iter().map(|m| Job::RoutedOrdering(*m)));
    } else {
        exclude(Job::RoutedOrdering(0), "routedCandidates is off");
    }
    (planned, excluded)
}

/// The first-stage allocation under a job budget (`work`, counted in jobs). When the budget cuts the plan, the jobs that
/// most often gave the best candidate run first (`budget_rank`); the rest are dropped. The flag says whether the budget
/// cut anything. Without a cut the planned order is kept, so reduction order and ties are the same as an unbudgeted run.
fn allocate_first_stage(mut jobs: Vec<Job>, limit: Option<usize>) -> (Vec<Job>, bool) {
    match limit {
        Some(limit) if jobs.len() > limit => {
            jobs.sort_by_key(|job| budget_rank(*job));
            jobs.truncate(limit);
            (jobs, true)
        }
        _ => (jobs, false),
    }
}

/// How early a job runs under a budget; lower runs first, ties keep the planned order. The ranks are how often each job
/// gave the best first-stage candidate on the tune views (`benchmarks/allocation_simulation.py`): routing 0.6, orderings 2
/// and 4 0.2, ordering 3 0.1, ordering 1 0.0. Judged on the judge views, they reach the best candidate with a budget of 2
/// on 6 of 10 views (planned order 4) and of 3 on 9 of 10 (planned order 5). Provisional: ten views each.
fn budget_rank(job: Job) -> usize {
    match job {
        Job::Routing => 0,
        Job::Ordering(2) | Job::Ordering(4) => 1,
        Job::Ordering(3) => 2,
        Job::Ordering(1) => 3,
        _ => 4,
    }
}

/// Rejects requirement values the geometry cannot take. A non-finite or non-positive clearance or extent is invalid. A
/// clearance whose inflation would overflow the coordinates is refused. Large but representable values are respected as
/// given: no requirement is capped automatically.
fn validate_requirements(snapshot: &Snapshot) -> Result<(), String> {
    if let Some(clearance) = snapshot.options.required_clearance {
        if !(clearance.is_finite() && clearance > 0.) {
            return Err("Required clearance must be a positive number".into());
        }
        let largest = snapshot
            .boxes
            .values()
            .flat_map(|r| [r.x, r.y, r.x + r.width, r.y + r.height])
            .fold(0., |m: f64, v| m.max(v.abs()));
        // Obstacles are inflated by the clearance and routes lie a track further out. Distances between them are squared,
        // so the inflated magnitude must stay finite when squared as well.
        let inflated = largest + 4. * clearance + 4. * snapshot.options.track_spacing;
        if !(inflated * inflated).is_finite() {
            return Err(
                "Required clearance is too large for the coordinates of this layout".into(),
            );
        }
    }
    if let Some(extent) = snapshot.options.max_extent {
        if !(extent.is_finite() && extent > 0.) {
            return Err("Maximum extent must be a positive number".into());
        }
    }
    Ok(())
}

/// The diagnostic for a drawing whose larger extent exceeds the requested limit. The refinement does not search for
/// extent, so this reports the result as it is; it does not prove that no layout within the limit exists.
fn extent_issue(snapshot: &Snapshot) -> Option<String> {
    let limit = snapshot.options.max_extent?;
    let x0 = snapshot
        .boxes
        .values()
        .map(|r| r.x)
        .fold(f64::INFINITY, f64::min);
    let y0 = snapshot
        .boxes
        .values()
        .map(|r| r.y)
        .fold(f64::INFINITY, f64::min);
    let x1 = snapshot
        .boxes
        .values()
        .map(|r| r.x + r.width)
        .fold(f64::NEG_INFINITY, f64::max);
    let y1 = snapshot
        .boxes
        .values()
        .map(|r| r.y + r.height)
        .fold(f64::NEG_INFINITY, f64::max);
    let extent = (x1 - x0).max(y1 - y0);
    (extent > limit).then(|| {
        format!("extent limit {limit} exceeded: the drawing spans {extent:.0} units; this does not prove that no layout within the limit exists")
    })
}

/// The diagnostic for a required clearance that the snapshot's drawn routes still violate, or `None` when it is met or
/// not required. A refinement that finds no qualifying candidate keeps its input, so this reports the violation.
/// `known_hugs` is the near-node count of an earlier measurement of the same drawing, if any.
fn unmet_clearance_issue(snapshot: &Snapshot, known_hugs: Option<usize>) -> Option<String> {
    let clearance = snapshot.options.required_clearance?;
    let hugs = known_hugs.unwrap_or_else(|| {
        arqlet_layout::published_metrics(snapshot).map_or(usize::MAX, |m| m.hug_pairs)
    });
    (hugs > 0)
        .then(|| format!("required clearance {clearance} not met: {hugs} near-node pairs remain"))
}

/// The bounded archive of a refinement: the indices of the best `capacity` measured candidates, best first. Candidates
/// are ordered by conflict key, then by log index, so ties resolve the same way for every thread count. A geometry that
/// repeats an earlier candidate is skipped, and the archive never holds more than `capacity` entries.
pub fn archive_indices(records: &[CandidateRecord], capacity: usize) -> Vec<usize> {
    let mut measured: Vec<&CandidateRecord> = records
        .iter()
        .filter(|r| r.outcome == CandidateOutcome::Measured && r.duplicate_of.is_none())
        .collect();
    measured.sort_by(|a, b| a.key.cmp(&b.key).then(a.index.cmp(&b.index)));
    measured
        .into_iter()
        .take(capacity)
        .map(|r| r.index)
        .collect()
}

/// Candidate IDs for planned jobs, in configured order. The occurrence counts earlier identical entries.
fn candidate_ids(jobs: &[Job]) -> Vec<CandidateId> {
    jobs.iter()
        .enumerate()
        .map(|(index, job)| {
            let (strategy, parameter) = job.strategy();
            let occurrence = jobs[..index]
                .iter()
                .filter(|other| other.strategy() == (strategy, parameter))
                .count();
            CandidateId {
                strategy,
                parameter,
                occurrence,
            }
        })
        .collect()
}

/// Maps `items` in order. Native builds split the work across scoped threads; WASM runs serially.
fn parallel_map<T, R, F>(items: &[T], threads: usize, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync,
{
    #[cfg(not(target_arch = "wasm32"))]
    {
        if threads > 1 && items.len() > 1 {
            let next = std::sync::atomic::AtomicUsize::new(0);
            let (sender, receiver) = std::sync::mpsc::channel();
            return std::thread::scope(|scope| {
                for _ in 0..threads.min(items.len()) {
                    let sender = sender.clone();
                    let next = &next;
                    let f = &f;
                    scope.spawn(move || loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if index >= items.len() {
                            break;
                        }
                        sender
                            .send((index, f(&items[index])))
                            .expect("candidate result receiver stays alive");
                    });
                }
                drop(sender);
                let mut results: Vec<Option<R>> =
                    std::iter::repeat_with(|| None).take(items.len()).collect();
                for (index, result) in receiver {
                    results[index] = Some(result);
                }
                results
                    .into_iter()
                    .map(|result| result.expect("every candidate index is dispatched once"))
                    .collect()
            });
        }
    }
    let _ = threads;
    items.iter().map(f).collect()
}

/// One refinement job: an ordering strategy (by mode number) or the routing-only candidate.
#[derive(Clone, Copy)]
enum Job {
    /// The flat-region candidate (A4b experiment): only when `flatRegion` is set.
    FlatRegion,
    Ordering(usize),
    Routing,
    /// The same boxes with the other port placement (see `with_other_port_placement`).
    OtherPortPlacement,
    /// The same boxes with the row ports of a notation at the middle of their rows (ADR 0011): the alternative to the
    /// ordering by the peers, which the shared key compares.
    RowPortMiddle,
    /// The same view with component packing off: the alternative when `componentPacking` is set; the key decides.
    UnpackedComponents,
    /// An ordering candidate re-routed before the key compares it (opt-in, `routedCandidates`).
    RoutedOrdering(usize),
    /// Routing-only with the last congestion round (channel views): the baseline next to `Routing`.
    RoutingLast,
    /// Routing with the extended conflict selection (experimental, P1): only when `extendedConflictSelection` is set.
    RoutingExtended,
    Search,
}

impl Job {
    /// The declared capabilities of this strategy. The first-stage gates in `refine_logged` and the round job list
    /// implement these declarations; `capabilities_match_the_planned_jobs` checks them.
    fn capabilities(self) -> StrategyCapabilities {
        let (name, first_stage, global_rounds, channel_engine, option) = match self {
            Job::Ordering(_) => ("ordering", true, false, false, None),
            Job::RoutedOrdering(_) => (
                "routed-ordering",
                true,
                false,
                false,
                Some("routedCandidates"),
            ),
            Job::Routing => ("routing", true, true, false, Some("routingCandidate")),
            Job::RoutingLast => ("routing-last", true, true, true, Some("routingCandidate")),
            Job::RoutingExtended => (
                "routing-extended",
                true,
                false,
                true,
                Some("extendedConflictSelection"),
            ),
            Job::Search => ("search", true, false, false, Some("placementSearch")),
            Job::FlatRegion => ("flat-region", true, false, false, Some("flatRegion")),
            Job::OtherPortPlacement => (
                "other-port-placement",
                true,
                false,
                false,
                Some("portPlacement"),
            ),
            Job::RowPortMiddle => ("row-port-middle", true, false, false, Some("rowPortOrder")),
            Job::UnpackedComponents => (
                "unpacked-components",
                true,
                false,
                false,
                Some("componentPacking"),
            ),
        };
        StrategyCapabilities {
            name,
            first_stage,
            global_rounds,
            channel_engine,
            option,
            graph: if matches!(self, Job::FlatRegion) {
                "flat"
            } else {
                "any"
            },
            port_override: matches!(self, Job::FlatRegion).then_some("angular"),
        }
    }

    /// Fixed strategy name and stable parameter; the basis of `CandidateId`.
    fn strategy(self) -> (&'static str, usize) {
        match self {
            Job::Ordering(mode) => ("ordering", mode),
            Job::RoutedOrdering(mode) => ("routed-ordering", mode),
            Job::Routing => ("routing", 0),
            Job::RoutingLast => ("routing-last", 0),
            Job::RoutingExtended => ("routing-extended", 0),
            Job::Search => ("search", 0),
            Job::FlatRegion => ("flat-region", 0),
            Job::OtherPortPlacement => ("other-port-placement", 0),
            Job::RowPortMiddle => ("row-port-middle", 0),
            Job::UnpackedComponents => ("unpacked-components", 0),
        }
    }
}

/// Runs `job` on `input` and scores the result against `reference` (movement is measured from it).
/// Returns `None` when the candidate violates geometry rules; such candidates are never accepted. `work` accumulates
/// the search work of this job (A* expansions for routing jobs, placement trials for the placement search).
/// An evaluated candidate: its strategy name, the snapshot, its quality, its published conflict key, and whether its
/// published drawing has no near-node pairs (`None` when it was not measured).
type Evaluated = (
    String,
    Snapshot,
    arqlet_model::Quality,
    Option<Vec<i64>>,
    Option<bool>,
);

/// The measurement of a job's input, when it is known: its quality against the same reference, its published key, and
/// whether it has no near-node pairs. A job whose result is the input itself takes them over instead of measuring again.
type KnownInput<'a> = Option<(&'a arqlet_model::Quality, &'a [i64], Option<bool>)>;

fn evaluate_against_until(
    input: &Snapshot,
    reference: &Snapshot,
    job: Job,
    known: KnownInput<'_>,
    stop: &(dyn Fn() -> bool + Sync),
    work: &mut WorkCounters,
) -> Result<Option<Evaluated>, String> {
    let started = stage_timer_start();
    let result = evaluate_job(input, reference, job, known, stop, work);
    work.job_nanos = work.job_nanos.saturating_add(stage_elapsed_nanos(started));
    result
}

fn evaluate_job(
    input: &Snapshot,
    reference: &Snapshot,
    job: Job,
    known: KnownInput<'_>,
    stop: &(dyn Fn() -> bool + Sync),
    work: &mut WorkCounters,
) -> Result<Option<Evaluated>, String> {
    let snapshot = input;
    let (name, mut option) = match job {
        Job::Ordering(mode) => {
            let name = arqlet_layout::CANDIDATES
                .iter()
                .find(|(m, _)| *m == mode)
                .map_or("unknown", |(_, name)| name);
            let (candidate, search) = arqlet_layout::candidate_from(
                &snapshot.graph,
                mode,
                &snapshot.options,
                stop,
                &snapshot.layered_plans,
            )?;
            work.absorb_search(&search);
            let Some(candidate) = candidate else {
                return Ok(None);
            };
            (name.to_string(), candidate)
        }
        Job::RoutedOrdering(mode) => {
            // The ordering candidate, then a routing pass on it: its layout competes with its own routing.
            let name = arqlet_layout::CANDIDATES
                .iter()
                .find(|(m, _)| *m == mode)
                .map_or("unknown", |(_, name)| name);
            let (candidate, search) = arqlet_layout::candidate_from(
                &snapshot.graph,
                mode,
                &snapshot.options,
                stop,
                &snapshot.layered_plans,
            )?;
            work.absorb_search(&search);
            let Some(candidate) = candidate else {
                return Ok(None);
            };
            let (routed, search) = arqlet_layout::reroute_counted(&candidate, true, stop);
            work.absorb_search(&search);
            let Ok(routed) = routed else {
                return Ok(None);
            };
            (format!("routed-{name}"), routed)
        }
        Job::FlatRegion => {
            let mut search = arqlet_layout::SearchCounters::default();
            let flat = arqlet_layout::flat_candidate_until(snapshot, stop, &mut search)?;
            work.absorb_search(&search);
            match flat {
                Some(candidate) => ("flat-region".to_string(), candidate),
                None => return Ok(None),
            }
        }
        // An interrupted routing search yields no candidate, but the work it did before the stop is still counted.
        Job::Routing => {
            let (routed, search) = arqlet_layout::reroute_counted(snapshot, true, stop);
            work.absorb_search(&search);
            let Ok(routed) = routed else {
                return Ok(None);
            };
            ("routing-only".to_string(), routed)
        }
        Job::RoutingExtended => {
            let (routed, search) = arqlet_layout::reroute_extended_counted(snapshot, stop);
            work.absorb_search(&search);
            let Ok(routed) = routed else {
                return Ok(None);
            };
            ("routing-extended".to_string(), routed)
        }
        Job::RoutingLast => {
            let (routed, search) = arqlet_layout::reroute_counted(snapshot, false, stop);
            work.absorb_search(&search);
            let Ok(routed) = routed else {
                return Ok(None);
            };
            ("routing-last-round".to_string(), routed)
        }
        Job::RowPortMiddle => {
            // The same pipeline with the row ports at the middle of their rows, then refined as the view is refined.
            let mut middle = snapshot.options.clone();
            middle.row_port_order = arqlet_model::RowPortOrder::Middle;
            let computed = recompute_with(snapshot, &middle)?;
            let refined = refine_snapshot_limited(
                &computed,
                computed.revision,
                computed.layout_version,
                1,
                Some(stop),
                None,
            )?;
            ("row-port-middle".to_string(), refined)
        }
        Job::OtherPortPlacement => {
            // The full symmetric pipeline for this view: recompute from the source graph with symmetric ports, then refine
            // it exactly as a symmetric view is refined. A symmetric view has no alternative, so this does not recurse.
            let mut symmetric = snapshot.options.clone();
            symmetric.port_placement = PortPlacement::Symmetric;
            let computed = recompute_with(snapshot, &symmetric)?;
            // The nested refinement checks the same stop signal; an interrupted one is discarded by the caller.
            let refined = refine_snapshot_limited(
                &computed,
                computed.revision,
                computed.layout_version,
                1,
                Some(stop),
                None,
            )?;
            ("symmetric-ports".to_string(), refined)
        }
        Job::UnpackedComponents => {
            // The same view with component packing off, recomputed from the source graph and refined as usual.
            let mut unpacked = snapshot.options.clone();
            unpacked.component_packing = false;
            let computed = recompute_with(snapshot, &unpacked)?;
            let refined = refine_snapshot_limited(
                &computed,
                computed.revision,
                computed.layout_version,
                1,
                Some(stop),
                None,
            )?;
            ("unpacked-components".to_string(), refined)
        }
        Job::Search => {
            let mut search = arqlet_layout::SearchCounters::default();
            let placement_started = stage_timer_start();
            let result = arqlet_layout::search_placement_counted(
                snapshot,
                snapshot.options.placement_search,
                stop,
                &mut search,
            );
            work.placement_nanos = work
                .placement_nanos
                .saturating_add(stage_elapsed_nanos(placement_started));
            let (searched, trials) = result?;
            work.placement_trials += trials;
            work.absorb_search(&search);
            ("placement-search".to_string(), searched)
        }
    };
    let scoring_started = stage_timer_start();
    let scored = (|| {
        option.geometry_issues = arqlet_scene::validate_geometry(
            &option.graph,
            &option.boxes,
            arqlet_scene::GeometryRules::default(),
        );
        // Authored fixed sizes below their content are reported and kept (ADR 0010); only other issues make a candidate
        // ineligible.
        option.geometry_issues.extend(fixed_size_conflicts(
            &option.graph,
            &option.boxes,
            &option.options.container_sizing,
        ));
        if is_ineligible_geometry(
            &option.geometry_issues,
            &option.graph,
            &option.options.container_sizing,
        ) {
            return Ok(None);
        }
        if option.options.abstract_routing
            && matches!(job, Job::Ordering(_) | Job::RoutedOrdering(_))
            && arqlet_layout::constructive::abstract_realization::completion::has_strict_track_conflicts(&option)
        {
            return Ok(None);
        }
        // A result that is its input, byte for byte, has the input's measurement. With a required clearance it is eligible
        // when the input keeps the clearance, and otherwise not: it repairs none of the input's own violations (below).
        if let Some((quality, key, clear)) = known {
            if serde_json::to_vec(&option).ok() == serde_json::to_vec(snapshot).ok() {
                match (option.options.required_clearance, clear) {
                    (None, _) | (Some(_), Some(true)) => {
                        work.reused_evaluations += 1;
                        return Ok(Some((
                            name,
                            option,
                            quality.clone(),
                            Some(key.to_vec()),
                            clear,
                        )));
                    }
                    (Some(_), Some(false)) => {
                        work.reused_evaluations += 1;
                        return Ok(None);
                    }
                    (Some(_), None) => {}
                }
            }
        }
        // A required clearance is a hard constraint: a candidate with near-node proximity is eligible only as a repair of an
        // input that has more hard violations in total (overlapping edges, for one). It then stays an open conflict, reported
        // as unmet by `unmet_clearance_issue`. An unmeasurable candidate is not eligible.
        // The key and the metric of the candidate from one measurement (`published_evaluation`); the key goes to the
        // selection with the candidate.
        let evaluation = arqlet_layout::published_evaluation(&option);
        if option.options.required_clearance.is_some() {
            match &evaluation {
                Ok((_, metrics)) if metrics.hug_pairs == 0 => {}
                Ok((_, metrics)) => {
                    let input = arqlet_layout::published_metrics(snapshot)
                        .map(|m| arqlet_layout::hard_violations(&m, true))
                        .unwrap_or(0);
                    if arqlet_layout::hard_violations(metrics, true) >= input {
                        return Ok(None);
                    }
                }
                Err(_) => return Ok(None),
            }
        }
        let quality = arqlet_layout::quality_with(&option, reference, &snapshot.options.objectives);
        let (key, clear) = match evaluation {
            Ok((key, metrics)) => (Some(key), Some(metrics.hug_pairs == 0)),
            Err(_) => (None, None),
        };
        Ok(Some((name, option, quality, key, clear)))
    })();
    work.scoring_nanos = work
        .scoring_nanos
        .saturating_add(stage_elapsed_nanos(scoring_started));
    scored
}

/// Global rounds: after the candidates, the routing pass runs again on the current best. A round keeps its
/// single best result only if the configured objectives improve; the loop ends when a round improves
/// nothing, or after `GLOBAL_ROUNDS`. The placement search is not repeated: it is the expensive pass
/// and runs once, in the candidate stage.
const GLOBAL_ROUNDS: usize = 3;

/// Scene v1 for a snapshot: `{ok, scene}`. The scene is a projection; it never changes the snapshot.
/// Shared metric set for any drawing (see arqlet_scene::metrics). Engine-independent: the input is
/// absolute node rectangles and edge polylines, whatever engine produced them.
/// The legacy dispatcher's form (`dispatch_legacy`); the protocol answers with a typed value now.
#[cfg(test)]
fn metrics_response(v: &serde_json::Value) -> String {
    let result = serde_json::from_value::<arqlet_scene::metrics::Drawing>(v["drawing"].clone())
        .map_err(|e| e.to_string())
        .and_then(|drawing| arqlet_scene::metrics::measure(&drawing))
        .and_then(|metrics| serde_json::to_value(metrics).map_err(|e| e.to_string()));
    match result {
        Ok(metrics) => serde_json::json!({"ok": true, "metrics": metrics}).to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}

/// The conflicts of one route in a drawing (`{ok, conflicts}`): the per-route view of the shared metrics.
fn route_conflicts_response(v: &serde_json::Value) -> String {
    let result = serde_json::from_value::<arqlet_scene::metrics::Drawing>(v["drawing"].clone())
        .map_err(|e| e.to_string())
        .and_then(|drawing| {
            arqlet_scene::metrics::route_conflicts(
                &drawing,
                v["edgeId"].as_str().unwrap_or_default(),
            )
        })
        .and_then(|conflicts| serde_json::to_value(conflicts).map_err(|e| e.to_string()));
    match result {
        Ok(conflicts) => serde_json::json!({"ok": true, "conflicts": conflicts}).to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}

mod reproduce;

/// Curated reference operations (provisional C0). Validation and canonical text belong to the reference crate; this
/// only maps JSON to them. A structurally invalid document is an error, never a report.
fn reference_response(op: &str, v: &serde_json::Value) -> String {
    let result: Result<serde_json::Value, String> = match op {
        "referenceCreate" => serde_json::from_value::<Snapshot>(v["snapshot"].clone())
            .map_err(|e| e.to_string())
            .and_then(|snapshot| {
                arqlet_reference::create(&snapshot, v["referenceId"].as_str().unwrap_or_default())
            })
            .and_then(|document| serde_json::to_value(document).map_err(|e| e.to_string()))
            .map(|reference| serde_json::json!({"reference": reference})),
        "referenceRefresh" => arqlet_reference::refresh(&v["reference"])
            .and_then(|document| serde_json::to_value(document).map_err(|e| e.to_string()))
            .map(|reference| serde_json::json!({"reference": reference})),
        "referenceReroute" => arqlet_reference::reroute(&v["reference"])
            .and_then(|document| serde_json::to_value(document).map_err(|e| e.to_string()))
            .map(|reference| serde_json::json!({"reference": reference})),
        "referenceValidate" => arqlet_reference::validate(&v["reference"])
            .and_then(|report| serde_json::to_value(report).map_err(|e| e.to_string()))
            .map(|report| serde_json::json!({"report": report})),
        "referenceSvg" => serde_json::from_value::<arqlet_reference::document::ReferenceDocument>(
            v["reference"].clone(),
        )
        .map_err(|e| e.to_string())
        .and_then(|document| arqlet_reference::svg::export(&document))
        .map(|svg| serde_json::json!({"svg": svg})),
        "referenceFromSvg" => arqlet_reference::svg::import(v["svg"].as_str().unwrap_or_default())
            .and_then(|document| serde_json::to_value(document).map_err(|e| e.to_string()))
            .map(|reference| serde_json::json!({"reference": reference})),
        "referenceReproduce" => reproduce::reproduce(v),
        "referenceRepair" => reproduce::repair(v),
        _ => arqlet_reference::canonical_text(&v["reference"])
            .map(|canonical| serde_json::json!({"canonical": canonical})),
    };
    match result {
        Ok(mut body) => {
            body["ok"] = serde_json::json!(true);
            body.to_string()
        }
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}

/// Provisional: lays out a graph given in the JSON form ELK uses (`arqlet_elk`) and returns it with the layout written in,
/// as ELK returns it, with the diagnostics of what had no counterpart and of the layout (a fallback, for example). The request
/// may add Arqlet `options` (they win over the translated ones), `refine: true` to refine the layout, `threads`, and
/// `snapshot: true` to get the Arqlet snapshot as well.
fn elk_response(v: &serde_json::Value) -> String {
    let result = (|| -> Result<serde_json::Value, String> {
        let elk = &v["graph"];
        let translation = arqlet_elk::translate(elk)?;
        let graph: Graph =
            serde_json::from_value(translation.graph.clone()).map_err(|e| e.to_string())?;
        let mut patch = translation.options.clone();
        if let Some(extra) = v.get("options").and_then(serde_json::Value::as_object) {
            for (key, value) in extra {
                patch[key] = value.clone();
            }
        }
        let options = options_from_patch(&patch)?;
        let limit = graph.nodes.len().max(1);
        if limit > MAX_VIEW_LIMIT {
            return Err(format!(
                "An ELK graph may have at most {MAX_VIEW_LIMIT} nodes"
            ));
        }
        let mut snapshot = import_graph_with(graph, limit, &options)?;
        if v["refine"].as_bool() == Some(true) {
            let threads = v["threads"]
                .as_u64()
                .unwrap_or(1)
                .clamp(1, MAX_THREADS as u64) as usize;
            snapshot = refine_snapshot_with(
                &snapshot,
                snapshot.revision,
                snapshot.layout_version,
                threads,
            )?;
        }
        let diagnostics: Vec<String> = translation
            .diagnostics
            .iter()
            .cloned()
            .chain(
                snapshot
                    .layout_diagnostics
                    .iter()
                    .filter(|d| d.starts_with("Single pass was not used"))
                    .cloned(),
            )
            .chain(snapshot.geometry_issues.iter().map(|i| i.message.clone()))
            .collect();
        let mut out = serde_json::json!({
            "graph": arqlet_elk::write_back(elk, &snapshot, &translation),
            "diagnostics": diagnostics,
        });
        // For evaluation: the Arqlet snapshot behind the result, for the `metrics` operation.
        if v["snapshot"].as_bool() == Some(true) {
            out["snapshot"] = serde_json::to_value(&snapshot).map_err(|e| e.to_string())?;
        }
        Ok(out)
    })();
    match result {
        Ok(mut value) => {
            value["ok"] = serde_json::json!(true);
            value.to_string()
        }
        Err(error) => serde_json::json!({"ok": false, "error": error}).to_string(),
    }
}

/// The component prototype as an operation: refines a flat graph component by component and returns the chosen snapshot
/// (the composition or the joint layout, whichever the shared key prefers) with the per-component report.
fn component_response(v: &serde_json::Value) -> String {
    let graph: Graph = match serde_json::from_value(v["graph"].clone()) {
        Ok(graph) => graph,
        Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
    };
    let options: LayoutOptions = if v["options"].is_null() {
        LayoutOptions::default()
    } else {
        match serde_json::from_value(v["options"].clone()) {
            Ok(options) => options,
            Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    };
    match components::refine_by_components(&graph, &options) {
        Ok(result) => serde_json::json!({
            "ok": true,
            "chosen": result.chosen,
            "components": result.components.iter().map(|c| serde_json::json!({
                "nodes": c.nodes,
                "strategy": c.strategy,
                "key": c.key,
            })).collect::<Vec<_>>(),
            "composedKey": result.composed_key,
            "jointKey": result.joint_key,
            "snapshot": result.snapshot,
        })
        .to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}

/// The container prototype as an operation: refines a flat graph with one container of leaf children under a fixed boundary,
/// and returns the chosen snapshot (the composition or the joint layout) with its report.
fn container_response(v: &serde_json::Value) -> String {
    let graph: Graph = match serde_json::from_value(v["graph"].clone()) {
        Ok(graph) => graph,
        Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
    };
    let options: LayoutOptions = if v["options"].is_null() {
        LayoutOptions::default()
    } else {
        match serde_json::from_value(v["options"].clone()) {
            Ok(options) => options,
            Err(e) => return serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
        }
    };
    match containers::refine_by_fixed_boundary(&graph, &options) {
        Ok(result) => serde_json::json!({
            "ok": true,
            "chosen": result.chosen,
            "container": result.container,
            "interiorStrategy": result.interior_strategy,
            "outerStrategy": result.outer_strategy,
            "composedKey": result.composed_key,
            "jointKey": result.joint_key,
            "snapshot": result.snapshot,
        })
        .to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}

/// The common entry: reads the structure of the graph, refines it by the path that applies, and reports the decision with
/// the paths that did not apply.
fn structured_response(v: &serde_json::Value) -> String {
    match structured_result(v) {
        Ok(reply) => reply.to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}

/// The structured optimization of the current view. The request is the view's snapshot and the revision and layout
/// version it was read at; the adapter derives the graph and options from the snapshot. The result follows the same
/// revision rules as `refine`: same graph revision, base revision the request's revision, layout version plus one. The
/// decision travels with it, so a lost composition is reported, not hidden.
fn structured_result(v: &serde_json::Value) -> Result<serde_json::Value, String> {
    let snapshot: Snapshot =
        serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
    let expected = v["expectedRevision"].as_u64();
    let layout = v["expectedLayoutVersion"].as_u64().unwrap_or(0);
    if snapshot.revision != expected.unwrap_or(u64::MAX) || snapshot.layout_version != layout {
        return Err("Stale refinement request".into());
    }
    if snapshot.schema_version != 1 || snapshot.engine_version != arqlet_model::ENGINE_VERSION {
        return Err("Incompatible snapshot".into());
    }
    if snapshot.graph.nodes.len() > 1000 || snapshot.graph.edges.len() > 2000 {
        return Err("Prototype refinement size limit exceeded".into());
    }
    validate_requirements(&snapshot)?;
    validate(&snapshot.graph)?;
    let (mut result, decision) =
        coordinator::refine_structured(&snapshot.graph, &snapshot.options)?;
    result.revision = snapshot.revision;
    result.base_revision = Some(snapshot.revision);
    result.layout_version = snapshot.layout_version.saturating_add(1);
    result.source_graph = snapshot.source_graph.clone();
    result.collapsed_ids = snapshot.collapsed_ids.clone();
    result.omitted_unresolved_edges = snapshot.omitted_unresolved_edges;
    // What the search chose for an area whose strategy is not fixed: the drawing shows it, so the settings can name it.
    for (part, name) in &decision.strategies {
        let (area, node) = if part == "root" {
            ("<root>", None)
        } else {
            (part.as_str(), Some(part.as_str()))
        };
        let free = !matches!(
            snapshot.graph.strategy_constraint(node),
            StrategyConstraint::Fixed(_)
        );
        if let (true, Some(strategy)) = (free, arqlet_model::strategy_from_name(name)) {
            result.area_strategies.insert(area.to_string(), strategy);
        }
    }
    let key = |k: &Option<Vec<i64>>| k.as_ref().map(|k| serde_json::json!(k));
    Ok(serde_json::json!({
        "ok": true,
        "route": decision.route,
        "notApplied": decision.not_applied.iter().map(|(path, reason)| serde_json::json!({
            "path": path,
            "reason": reason,
        })).collect::<Vec<_>>(),
        "decision": {
            "route": decision.route,
            "composedKey": key(&decision.composed_key),
            "jointKey": key(&decision.joint_key),
            "details": decision.details.iter().map(|(part, strategy)| serde_json::json!({
                "part": part,
                "strategy": strategy,
            })).collect::<Vec<_>>(),
        },
        "snapshot": result,
    }))
}

/// The legacy dispatcher's form (`dispatch_legacy`); the protocol answers with a typed value now.
#[cfg(test)]
fn scene_response(v: &serde_json::Value) -> String {
    let result = serde_json::from_value::<Snapshot>(v["snapshot"].clone())
        .map_err(|e| e.to_string())
        .and_then(|snapshot| filter::scene_of(&snapshot))
        .and_then(|scene| serde_json::to_value(scene).map_err(|e| e.to_string()));
    match result {
        Ok(scene) => serde_json::json!({"ok": true, "scene": scene}).to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}

fn refine_request(
    snapshot: &Snapshot,
    expected: u64,
    version: u64,
    threads: usize,
    budget: Option<&serde_json::Value>,
    work: Option<&serde_json::Value>,
) -> Result<Snapshot, String> {
    // A work budget is deterministic and works on every target; it may be combined with a time budget.
    let work = match work {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(
            value
                .as_u64()
                .and_then(|n| usize::try_from(n).ok())
                .ok_or("workBudget must be a non-negative integer")?,
        ),
    };
    let Some(budget) = budget else {
        return refine_snapshot_limited(snapshot, expected, version, threads, None, work);
    };
    let millis = budget
        .as_u64()
        .ok_or("budgetMs must be a non-negative integer")?;
    #[cfg(not(target_arch = "wasm32"))]
    {
        let start = std::time::Instant::now();
        let duration = std::time::Duration::from_millis(millis);
        let mut refined = refine_snapshot_limited(
            snapshot,
            expected,
            version,
            threads,
            Some(&|| start.elapsed() >= duration),
            work,
        )?;
        if let Some(report) = refined.optimization.as_mut().filter(|r| r.interrupted) {
            report.stop_reason = Some(arqlet_model::StopReason::TimeBudget);
        }
        Ok(refined)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = millis;
        Err("budgetMs requires a host clock; use native dispatch or refine_snapshot_until with a host callback".into())
    }
}

/// `dispatch` for hosts that take progressive results. For `refineProgressive` (request fields as for `refine`, plus a
/// host `jobId`), every accepted improvement is handed to `publish` as a JSON message while the refinement is still
/// running: `{ok, publication: {jobId, sequence, strategy, key, candidatesEvaluated, input, snapshot}}`. The response
/// is the final `{ok, jobId, snapshot}`. Every other operation behaves exactly like `dispatch`.
pub fn dispatch_streaming(input: &str, publish: &mut dyn FnMut(&str)) -> String {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(input) else {
        return dispatch(input);
    };
    if v["op"].as_str() != Some("refineProgressive") {
        return dispatch(input);
    }
    let job_id = v["jobId"].as_str().unwrap_or("").to_string();
    let result = (|| -> Result<Snapshot, String> {
        let threads = v["threads"]
            .as_u64()
            .unwrap_or(1)
            .clamp(1, MAX_THREADS as u64) as usize;
        let snapshot: Snapshot =
            serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
        let expected = v["expectedRevision"]
            .as_u64()
            .ok_or("Missing expectedRevision")?;
        let version = v["expectedLayoutVersion"]
            .as_u64()
            .ok_or("Missing expectedLayoutVersion")?;
        let (refined, _) = refine_snapshot_progressive(
            &job_id,
            &snapshot,
            expected,
            version,
            threads,
            None,
            None,
            &mut |p: &Publication| {
                // Every streamed publication is the best layout found so far; the job's final result is the response itself.
                let envelope = arqlet_model::PublicationEnvelope {
                    job_id: p.job_id.clone(),
                    sequence: p.sequence,
                    kind: arqlet_model::PublicationKind::Best,
                    strategy: p.strategy.clone(),
                    key: p.key.clone(),
                    candidates_evaluated: p.candidates_evaluated,
                    input: arqlet_model::PublicationInput {
                        graph_revision: p.input.graph_revision,
                        base_layout_version: p.input.base_layout_version,
                        options_key: p.input.options_key.clone(),
                    },
                    snapshot: p.snapshot.clone(),
                    publication_version: execution::PUBLICATION_VERSION,
                    score: arqlet_model::PublicationScore {
                        quality_version: arqlet_scene::quality::QUALITY_VERSION,
                        hard_violations: p.hard_violations,
                        shadow_total: p.shadow_total,
                    },
                    archive: p
                        .archive
                        .iter()
                        .map(|e| arqlet_model::ArchiveSummary {
                            index: e.index,
                            strategy: e.strategy.clone(),
                            key: e.key.clone(),
                            geometry_fingerprint: e.geometry_fingerprint.clone(),
                        })
                        .collect(),
                };
                let message = serde_json::json!({"ok": true, "publication": envelope});
                publish(&message.to_string());
            },
        )?;
        Ok(refined)
    })();
    match result {
        Ok(s) => serde_json::json!({"ok": true, "jobId": job_id, "snapshot": s}).to_string(),
        Err(e) => serde_json::json!({"ok": false, "error": e}).to_string(),
    }
}

/// JSON transport shared by browser WASM, the native command and the backend.
/// Every operation accepts an optional `threads` field (clamped to 1..=MAX_THREADS).
/// The response is `{ok, snapshot}` or `{ok:false, error}`; it never panics on bad input.
/// The engine's one entry point: a JSON request in, a JSON answer out (ADR 0019: through the typed [`protocol`]).
pub fn dispatch(input: &str) -> String {
    match protocol::Request::from_json(input) {
        Ok(request) => protocol::handle(request).to_json(),
        Err(error) => protocol::Response::Error(error).to_json(),
    }
}

/// The dispatcher before the protocol was typed, kept for the test that compares both answer for answer.
#[cfg(test)]
pub(crate) fn dispatch_legacy(input: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(input) {
        if v["op"].as_str() == Some("optionDefaults") {
            // The engine defaults. The UI reads them instead of copying values; absent keys are defaults too.
            let mut options =
                serde_json::to_value(arqlet_model::LayoutOptions::default()).unwrap_or_default();
            options["layeredConstruction"] = serde_json::json!(true);
            options["hierarchicalOrdering"] = serde_json::json!(true);
            return serde_json::json!({"ok": true, "options": options}).to_string();
        }
        if v["op"].as_str() == Some("scene") {
            return scene_response(&v);
        }
        if v["op"].as_str() == Some("trace") {
            return trace::trace_response(&v).to_string();
        }
        if v["op"].as_str() == Some("neighbourhood") {
            return trace::neighbourhood_response(&v).to_string();
        }
        if v["op"].as_str() == Some("componentRefine") {
            return component_response(&v);
        }
        if v["op"].as_str() == Some("elkLayout") {
            return elk_response(&v);
        }
        if v["op"].as_str() == Some("overview") {
            let threads = v["threads"]
                .as_u64()
                .unwrap_or(1)
                .clamp(1, MAX_THREADS as u64) as usize;
            return overview::overview_response(&v, threads).to_string();
        }
        if v["op"].as_str() == Some("containerRefine") {
            return container_response(&v);
        }
        if v["op"].as_str() == Some("structuredRefine") {
            return structured_response(&v);
        }
        if v["op"].as_str() == Some("metrics") {
            return metrics_response(&v);
        }
        if v["op"].as_str() == Some("intent") {
            // The decisions of a snapshot as one value, for a host that keeps an undo stack.
            return match serde_json::from_value::<Snapshot>(v["snapshot"].clone()) {
                Ok(snapshot) => {
                    serde_json::json!({"ok": true, "intent": history::intent_of(&snapshot)})
                        .to_string()
                }
                Err(e) => serde_json::json!({"ok": false, "error": e.to_string()}).to_string(),
            };
        }
        if v["op"].as_str() == Some("routeConflicts") {
            return route_conflicts_response(&v);
        }
        if let Some(
            op @ ("referenceCreate" | "referenceRefresh" | "referenceReroute" | "referenceValidate"
            | "referenceCanonical" | "referenceSvg" | "referenceFromSvg"
            | "referenceReproduce" | "referenceRepair"),
        ) = v["op"].as_str()
        {
            return reference_response(op, &v);
        }
    }
    let result = (|| -> Result<Snapshot, String> {
        let v: serde_json::Value = serde_json::from_str(input).map_err(|e| e.to_string())?;
        let threads = v["threads"]
            .as_u64()
            .unwrap_or(1)
            .clamp(1, MAX_THREADS as u64) as usize;
        if v["op"].as_str() == Some("import") {
            let graph: Graph =
                serde_json::from_value(v["graph"].clone()).map_err(|e| e.to_string())?;
            let limit = usize::try_from(v["limit"].as_u64().ok_or("Missing limit")?)
                .map_err(|e| e.to_string())?;
            let options = match v.get("options") {
                Some(patch) => options_from_patch(patch)?,
                None => LayoutOptions::default(),
            };
            return import_graph_with(graph, limit, &options);
        }
        let snapshot: Snapshot =
            serde_json::from_value(v["snapshot"].clone()).map_err(|e| e.to_string())?;
        let expected = v["expectedRevision"]
            .as_u64()
            .ok_or("Missing expectedRevision")?;
        match v["op"].as_str() {
            Some("direction") => set_direction(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                v["nodeId"].as_str(),
                if v["direction"].is_null() {
                    None
                } else {
                    Some(
                        serde_json::from_value(v["direction"].clone())
                            .map_err(|e| e.to_string())?,
                    )
                },
            ),
            Some("strategy") => set_strategy(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                v["nodeId"].as_str(),
                if v["strategy"].is_null() {
                    None
                } else {
                    Some(arqlet_model::parse_strategy(&v["strategy"]).map_err(|e| e.to_string())?)
                },
            ),
            Some("routing") => set_routing(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                v["nodeId"].as_str(),
                if v["routing"].is_null() {
                    None
                } else {
                    Some(arqlet_model::parse_routing(&v["routing"]).map_err(|e| e.to_string())?)
                },
            ),
            Some("move") => move_node(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                &serde_json::from_value(v.clone()).map_err(|e| e.to_string())?,
            ),
            Some("add") => add_node(
                &snapshot,
                expected,
                v["sourceId"].as_str().ok_or("Missing sourceId")?,
                v["label"].as_str().unwrap_or("New node"),
            ),
            Some("toggle") => toggle_container(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                v["nodeId"].as_str().ok_or("Missing nodeId")?,
            ),
            Some("reveal") => reveal_nodes(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                &serde_json::from_value::<Vec<String>>(v["nodeIds"].clone())
                    .map_err(|_| "Missing nodeIds")?,
                v["partial"].as_bool().unwrap_or(true),
            ),
            Some("configure") => configure_snapshot(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                &v["options"],
            ),
            Some("port") => port_edit::set_port(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                v["edgeId"].as_str().ok_or("Missing edgeId")?,
                v["endpoint"].as_str().ok_or("Missing endpoint")?,
                if v["side"].is_null() {
                    None
                } else {
                    Some((
                        serde_json::from_value(v["side"].clone()).map_err(|e| e.to_string())?,
                        v["offset"].as_f64().ok_or("Missing offset")?,
                    ))
                },
            ),
            Some("settle") => settle::settle_snapshot(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
            ),
            Some("restore") => history::restore_intent(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                &serde_json::from_value(v["intent"].clone()).map_err(|e| e.to_string())?,
            ),
            Some("shape") => set_shape(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                v["nodeId"].as_str().ok_or("Missing nodeId")?,
                serde_json::from_value(v["shape"].clone()).map_err(|e| e.to_string())?,
            ),
            // Typed geometry observations (`measurements`: outline and revision per element) or, as before, plain `sizes`.
            Some("measure") if v.get("measurements").is_some() => measure_contours(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                serde_json::from_value(v["measurements"].clone()).map_err(|e| e.to_string())?,
            ),
            Some("measure") => measure_snapshot(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                serde_json::from_value(v["sizes"].clone()).map_err(|e| e.to_string())?,
            ),
            // Without a publication callback the progressive operation is a plain refinement with the same result.
            Some("refine") | Some("refineProgressive") => refine_request(
                &snapshot,
                expected,
                v["expectedLayoutVersion"]
                    .as_u64()
                    .ok_or("Missing expectedLayoutVersion")?,
                threads,
                v.get("budgetMs"),
                v.get("workBudget"),
            ),
            _ => Err("Unsupported operation".into()),
        }
    })();
    match result {
        Ok(s) => serde_json::json!({"ok":true,"snapshot":s}).to_string(),
        Err(e) => serde_json::json!({"ok":false,"error":e}).to_string(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// The pipeline before single pass became the default (ADR 0016). These tests exercise its refinement portfolio, its
    /// placement search and their reports, which single pass does not run.
    fn legacy() -> LayoutOptions {
        LayoutOptions {
            single_pass_auto: false,
            single_pass: false,
            abstract_routing: false,
            ..LayoutOptions::default()
        }
    }

    fn import_graph(graph: Graph, limit: usize) -> Result<Snapshot, String> {
        import_graph_with(graph, limit, &legacy())
    }

    fn import_view(json: &str, limit: usize) -> Result<Snapshot, String> {
        let graph: Graph = serde_json::from_str(json).map_err(|e| e.to_string())?;
        import_graph(graph, limit)
    }

    fn previous_pipeline_options() -> LayoutOptions {
        LayoutOptions {
            layered_construction: false,
            hierarchical_ordering: false,
            ..legacy()
        }
    }

    fn import_previous_view(json: &str, limit: usize) -> Result<Snapshot, String> {
        let graph: Graph = serde_json::from_str(json).map_err(|error| error.to_string())?;
        import_graph_with(graph, limit, &previous_pipeline_options())
    }

    fn deterministic_work(
        c: &WorkCounters,
    ) -> (
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
        usize,
    ) {
        (
            c.candidates_started,
            c.candidates_completed,
            c.routing_rounds,
            c.astar_expansions,
            c.astar_searches,
            c.largest_astar_search,
            c.placement_trials,
            c.legacy_candidates,
            c.legacy_grid_pops,
            c.corridor_gap_calculations,
            c.clearance_widening_rounds,
            c.clearance_widening_noop_rounds,
        )
    }

    // Deliberately overlapping boxes keep these fallback tests bounded: they use the full graph inputs without running
    // their expensive initial layout.
    fn invalid_snapshot(graph_json: &str) -> Snapshot {
        let graph: Graph = serde_json::from_str(graph_json).unwrap();
        let boxes: BTreeMap<String, Rect> = graph
            .nodes
            .iter()
            .map(|n| {
                (
                    n.id.clone(),
                    Rect {
                        x: 0.,
                        y: 0.,
                        width: 100.,
                        height: 100.,
                    },
                )
            })
            .collect();
        let routes = graph
            .edges
            .iter()
            .filter(|e| e.target.is_some())
            .map(|e| {
                Route::new(
                    e.id.clone(),
                    vec![Point { x: 0., y: 0. }, Point { x: 10., y: 10. }],
                )
            })
            .collect();
        let omitted_unresolved_edges = graph.edges.iter().filter(|e| e.target.is_none()).count();
        Snapshot {
            labels: Vec::new(),
            routing_issues: Vec::new(),
            layout_diagnostics: Vec::new(),
            corridor_gaps: Vec::new(),
            edit_report: None,
            area_orientations: BTreeMap::new(),
            area_strategies: BTreeMap::new(),
            route_conflicts: Vec::new(),
            schema_version: 1,
            engine_version: arqlet_model::ENGINE_VERSION.into(),
            revision: 0,
            layout_version: 0,
            base_revision: None,
            options: legacy(),
            graph,
            source_graph: None,
            collapsed_ids: Vec::new(),
            boxes,
            routes,
            recomputed_containers: Vec::new(),
            optimization: None,
            omitted_unresolved_edges,
            geometry_issues: Vec::new(),
            resolved_ports: Vec::new(),
            boundary_crossings: Vec::new(),
            straight_edges: Vec::new(),
            bus_boxes: BTreeMap::new(),
            gesture: None,
            layered_plans: Default::default(),
        }
    }

    fn assert_invalid_input_is_retained(snapshot: Snapshot) {
        let previous_boxes = snapshot.boxes.clone();
        let previous_routes = snapshot.routes.clone();
        let (retained, report) = refine_snapshot_job(
            "phase0-no-candidate",
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            1,
            None,
            Some(0),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&retained.boxes).unwrap(),
            serde_json::to_value(previous_boxes).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&retained.routes).unwrap(),
            serde_json::to_value(previous_routes).unwrap()
        );
        assert!(!retained.geometry_issues.is_empty());
        assert!(retained
            .routing_issues
            .iter()
            .any(|issue| issue.contains("retained the existing geometry")));
        assert_eq!(
            retained.optimization.as_ref().unwrap().strategy,
            "keep-existing"
        );
        assert_eq!(report.counters.candidates_started, 0);
    }

    #[test]
    fn mixed_large_graph_keeps_complete_input_when_no_candidate_is_valid() {
        let snapshot = invalid_snapshot(include_str!(
            "../../../examples/fixtures/phase0/mixed-large.json"
        ));
        assert_invalid_input_is_retained(snapshot);
    }

    #[test]
    fn nested_tree_radial_graph_keeps_complete_input_when_no_candidate_is_valid() {
        let snapshot = invalid_snapshot(include_str!(
            "../../../benchmarks/inputs/nested-tree-radial.json"
        ));
        assert_invalid_input_is_retained(snapshot);
    }

    #[test]
    fn refinement_keeps_invalid_input_when_all_candidates_are_ineligible() {
        let mut snapshot = import_view(&fixture(), 20).unwrap();
        let pin = Point { x: 0., y: 0. };
        for id in ["a", "b"] {
            snapshot.options.pins.insert(id.into(), pin.clone());
            let rect = snapshot.boxes.get_mut(id).unwrap();
            rect.x = pin.x;
            rect.y = pin.y;
        }
        let previous_boxes = serde_json::to_value(&snapshot.boxes).unwrap();
        let previous_routes = serde_json::to_value(&snapshot.routes).unwrap();
        let (retained, report) = refine_snapshot_job(
            "phase0-all-ineligible",
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            1,
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&retained.boxes).unwrap(),
            previous_boxes
        );
        assert_eq!(
            serde_json::to_value(&retained.routes).unwrap(),
            previous_routes
        );
        assert!(!retained.geometry_issues.is_empty());
        assert_eq!(
            retained.optimization.as_ref().unwrap().strategy,
            "keep-existing"
        );
        assert!(report.counters.candidates_completed > 0);
    }

    #[test]
    fn small_view_work_counters_stay_within_the_phase0_guard() {
        let snapshot = import_view(&fixture(), 20).unwrap();
        let (_, report) = refine_snapshot_job(
            "phase0-work-guard",
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            1,
            None,
            None,
        )
        .unwrap();
        let counters = &report.counters;
        assert!(counters.legacy_candidates <= 50_000);
        assert!(
            counters.clearance_widening_rounds <= report.candidate_work.len() * 4,
            "unexpected clearance widening work: {} rounds across {} candidates",
            counters.clearance_widening_rounds,
            report.candidate_work.len()
        );
        assert!(counters.corridor_gap_calculations <= report.candidate_work.len() * 4);
        #[cfg(feature = "benchmark-timings")]
        assert!(report.candidate_work.iter().any(|candidate| {
            candidate.counters.placement_nanos > 0
                && candidate.counters.port_resolution_nanos > 0
                && candidate.counters.routing_nanos > 0
                && candidate.counters.scoring_nanos > 0
        }));
    }

    #[test]
    fn incremental_clearance_option_is_omitted_unless_enabled() {
        let defaults = serde_json::to_value(legacy()).unwrap();
        assert!(defaults.get("incrementalClearanceWidening").is_none());
        let enabled = LayoutOptions {
            incremental_clearance_widening: true,
            ..legacy()
        };
        assert_eq!(
            serde_json::to_value(enabled).unwrap()["incrementalClearanceWidening"],
            true
        );
    }

    #[test]
    fn cancelled_refinement_keeps_complete_geometry_and_rejects_stale_requests() {
        let snapshot = import_view(&fixture(), 20).unwrap();
        let next = refine_snapshot_until(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            4,
            Some(&|| true),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&next.boxes).unwrap(),
            serde_json::to_value(&snapshot.boxes).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&next.routes).unwrap(),
            serde_json::to_value(&snapshot.routes).unwrap()
        );
        assert_eq!(next.revision, snapshot.revision);
        assert_eq!(next.layout_version, snapshot.layout_version + 1);
        let report = next.optimization.unwrap();
        assert!(report.interrupted);
        assert_eq!(report.candidates_evaluated, 1);
        assert!(refine_snapshot_until(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version + 1,
            1,
            Some(&|| true)
        )
        .is_err());
    }

    #[test]
    fn placement_cancellation_returns_a_complete_nonworse_snapshot() {
        use std::cell::Cell;
        let snapshot = import_view(&dense_graph().to_string(), 100).unwrap();
        let checks = Cell::new(0);
        let stop = || {
            checks.set(checks.get() + 1);
            checks.get() >= 8
        };
        let next = arqlet_layout::search_placement_until(&snapshot, 200, &stop).unwrap();
        assert!(checks.get() >= 8);
        assert!(next.geometry_issues.is_empty());
        assert_eq!(next.routes.len(), snapshot.routes.len());
        let before =
            arqlet_layout::quality_with(&snapshot, &snapshot, &snapshot.options.objectives);
        let after = arqlet_layout::quality_with(&next, &snapshot, &snapshot.options.objectives);
        assert!(!arqlet_layout::better_by(
            &before,
            &after,
            &snapshot.options.objectives
        ));
    }

    #[test]
    fn native_zero_budget_is_reported_and_invalid_budget_rejected() {
        let snapshot = import_view(&fixture(), 20).unwrap();
        let mut request = serde_json::json!({"op":"refine", "snapshot":snapshot,
            "expectedRevision":snapshot.revision,"expectedLayoutVersion":snapshot.layout_version,"budgetMs":0});
        let result: serde_json::Value =
            serde_json::from_str(&dispatch(&request.to_string())).unwrap();
        assert_eq!(result["ok"], true);
        assert_eq!(result["snapshot"]["optimization"]["interrupted"], true);
        request["budgetMs"] = serde_json::json!(-1);
        let result: serde_json::Value =
            serde_json::from_str(&dispatch(&request.to_string())).unwrap();
        assert_eq!(result["ok"], false);
    }

    fn fixture() -> String {
        serde_json::json!({"nodes":[{"id":"root","kind":"directory","label":"root","parentId":null},{"id":"a","kind":"symbol","label":"A","parentId":"root"},{"id":"b","kind":"symbol","label":"B","parentId":"root"}],"edges":[{"id":"1","kind":"calls","source":"a","target":"b"},{"id":"2","kind":"calls","source":"b","target":"a"},{"id":"3","kind":"calls","source":"a","target":"a"},{"id":"4","kind":"calls","source":"a","target":"b"},{"id":"5","kind":"calls","source":"a","target":null}]}).to_string()
    }
    #[test]
    fn import_keeps_cycles_and_parallel_edges_but_omits_unresolved() {
        let s = import_view(&fixture(), 20).unwrap();
        assert_eq!(s.graph.edges.len(), 4);
        assert_eq!(s.routes.len(), 4);
        assert_eq!(s.omitted_unresolved_edges, 1);
        assert!(s.boxes.values().all(|r| r.x.is_finite() && r.width > 0.));
    }
    #[test]
    fn roundtrip_edit_and_stale_rejection() {
        let s = import_view(&fixture(), 20).unwrap();
        let s: Snapshot = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        let next = add_node(&s, 0, "a", "Added").unwrap();
        assert_eq!(next.revision, 1);
        assert_eq!(next.graph.nodes.len(), 4);
        assert!(add_node(&next, 0, "a", "Stale").is_err());
        assert_eq!(next.base_revision, Some(0));
    }
    /// Cyclic, multi-edge graph large enough that the parallel candidate split uses several threads.
    pub(crate) fn dense_graph() -> serde_json::Value {
        let nodes: Vec<_> = (0..16)
            .map(|i| serde_json::json!({"id":format!("n{i}"),"kind":"symbol","label":format!("N{i}"),"parentId":null}))
            .collect();
        let edges: Vec<_> = (0..16)
            .flat_map(|i| [(i * 7 + 3) % 16, (i * 5 + 1) % 16].map(|t| (i, t)))
            .enumerate()
            .map(|(k, (s, t))| serde_json::json!({"id":format!("e{k}"),"kind":"calls","source":format!("n{s}"),"target":format!("n{t}")}))
            .collect();
        serde_json::json!({"nodes":nodes,"edges":edges})
    }
    #[test]
    fn import_operation_matches_native_import() {
        let request = serde_json::json!({"op":"import","graph":serde_json::from_str::<serde_json::Value>(&fixture()).unwrap(),"limit":20});
        let response: serde_json::Value =
            serde_json::from_str(&dispatch(&request.to_string())).unwrap();
        assert_eq!(response["ok"], true);
        let direct = import_view(&fixture(), 20).unwrap();
        assert_eq!(
            response["snapshot"]["graph"],
            serde_json::to_value(&direct.graph).unwrap()
        );
        assert_eq!(
            response["snapshot"]["boxes"],
            serde_json::to_value(&direct.boxes).unwrap()
        );
        let missing = dispatch(r#"{"op":"import","graph":{"nodes":[],"edges":[]}}"#);
        assert!(missing.contains("\"ok\":false"));
    }
    #[test]
    fn refinement_is_identical_for_any_thread_count() {
        let s = import_view(&dense_graph().to_string(), 40).unwrap();
        let request = |threads: usize| {
            serde_json::json!({"op":"refine","snapshot":s,"expectedRevision":s.revision,"expectedLayoutVersion":s.layout_version,"threads":threads}).to_string()
        };
        let serial = dispatch(&request(1));
        assert!(serial.contains("\"ok\":true"));
        for threads in [2, 4, MAX_THREADS, 1000] {
            assert_eq!(dispatch(&request(threads)), serial, "threads = {threads}");
        }
    }
    #[test]
    fn stop_callback_that_never_fires_matches_unstopped_refinement() {
        let s = import_view(&dense_graph().to_string(), 40).unwrap();
        let unstopped =
            refine_snapshot_limited(&s, s.revision, s.layout_version, 4, None, None).unwrap();
        for threads in [1, 4] {
            let stopped =
                refine_snapshot_until(&s, s.revision, s.layout_version, threads, Some(&|| false))
                    .unwrap();
            assert_eq!(
                serde_json::to_string(&stopped).unwrap(),
                serde_json::to_string(&unstopped).unwrap(),
                "threads = {threads}"
            );
        }
    }

    #[test]
    fn published_improvements_strictly_decrease_and_end_at_the_result() {
        let s = import_view(&dense_graph().to_string(), 40).unwrap();
        let input_key = arqlet_layout::published_key(&s).unwrap();
        for threads in [1, 4] {
            let mut published: Vec<execution::Publication> = Vec::new();
            let (refined, _) = refine_snapshot_progressive(
                "",
                &s,
                s.revision,
                s.layout_version,
                threads,
                None,
                None,
                &mut |p: &execution::Publication| published.push(p.clone()),
            )
            .unwrap();
            // The fixture must actually improve, or the checks below would prove nothing.
            assert!(
                !published.is_empty(),
                "the fixture publishes at least one improvement"
            );
            for (k, p) in published.iter().enumerate() {
                assert_eq!(p.sequence, k + 1);
                assert!(p.key < input_key, "each publication improves on the input");
                assert_eq!(p.input.graph_revision, s.revision);
                if k > 0 {
                    assert!(p.key < published[k - 1].key, "keys strictly decrease");
                }
            }
            if let Some(last) = published.last() {
                assert_eq!(
                    serde_json::to_value(&last.snapshot.boxes).unwrap(),
                    serde_json::to_value(&refined.boxes).unwrap(),
                    "the last publication is the result (threads {threads})"
                );
                assert_eq!(
                    serde_json::to_value(&last.snapshot.routes).unwrap(),
                    serde_json::to_value(&refined.routes).unwrap()
                );
            }
        }
    }

    #[test]
    fn streaming_dispatch_hands_each_publication_over_before_the_response() {
        let s = import_view(&dense_graph().to_string(), 40).unwrap();
        let request = serde_json::json!({"op": "refineProgressive", "jobId": "job-7",
            "snapshot": s, "expectedRevision": s.revision, "expectedLayoutVersion": s.layout_version,
            "threads": 2});
        let mut messages: Vec<serde_json::Value> = Vec::new();
        let response = dispatch_streaming(&request.to_string(), &mut |m: &str| {
            messages.push(serde_json::from_str(m).unwrap());
        });
        let response: serde_json::Value = serde_json::from_str(&response).unwrap();
        assert_eq!(response["ok"], true);
        assert_eq!(response["jobId"], "job-7");
        assert!(
            !messages.is_empty(),
            "the fixture publishes at least one improvement"
        );
        for (k, m) in messages.iter().enumerate() {
            assert_eq!(m["publication"]["jobId"], "job-7");
            assert_eq!(m["publication"]["sequence"], k + 1);
            assert_eq!(m["publication"]["publicationVersion"], 4);
            assert_eq!(m["publication"]["kind"], "best");
            assert!(m["publication"]["archive"].is_array());
            assert_eq!(m["publication"]["score"]["qualityVersion"], 2);
            assert!(m["publication"]["score"]["hardViolations"].is_u64());
            assert!(m["publication"]["score"]["shadowTotal"].is_i64());
        }
        assert_eq!(
            messages.last().unwrap()["publication"]["snapshot"]["boxes"],
            response["snapshot"]["boxes"]
        );
        // Other operations keep their plain response.
        let plain = dispatch_streaming(
            &serde_json::json!({"op": "optionDefaults"}).to_string(),
            &mut |_: &str| panic!("only refineProgressive publishes"),
        );
        assert!(plain.contains("\"ok\":true"));
    }
    #[test]
    fn channel_routes_respect_containers_and_stay_orthogonal() {
        // workflow-dag: five stage containers hold most nodes. Edges between stages must pass around the foreign
        // stages through the corridors outside them; none may fall back to a diagonal or pass through a node.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/workflow-dag.json")).unwrap(),
        )
        .unwrap();
        let mut s = import_graph_with(graph, 100, &previous_pipeline_options()).unwrap();
        s.options.routing_engine = RoutingEngine::Channel;
        let base = arqlet_layout::compute(&s.graph, None, &BTreeSet::new(), &s.options).unwrap();
        let drawn = base
            .graph
            .edges
            .iter()
            .filter(|e| e.target.is_some())
            .count();
        assert_eq!(base.routes.len(), drawn, "every drawn edge has a route");
        for route in &base.routes {
            for w in route.points.windows(2) {
                assert!(
                    (w[0].x - w[1].x).abs() < 1e-6 || (w[0].y - w[1].y).abs() < 1e-6,
                    "route {} has a diagonal segment",
                    route.id
                );
            }
        }
        assert!(
            base.routing_issues.is_empty(),
            "no route passes a node or a foreign container: {:?}",
            base.routing_issues.iter().take(3).collect::<Vec<_>>()
        );
    }

    #[test]
    fn extended_conflict_selection_removes_a_crossing_the_shared_selection_keeps() {
        // state-order: the default channel refinement keeps three crossing pairs; re-routing the crossing routes with a
        // crossing cost removes one, and the published key takes that candidate.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/state-order.json")).unwrap(),
        )
        .unwrap();
        let mut s = import_graph_with(graph, 100, &previous_pipeline_options()).unwrap();
        s.options.routing_engine = RoutingEngine::Channel;
        let refine = |s: &Snapshot| {
            refine_snapshot_limited(s, s.revision, s.layout_version, 1, None, None).unwrap()
        };
        let plain = refine(&s);
        s.options.extended_conflict_selection = true;
        let extended = refine(&s);
        let crossings = |s: &Snapshot| s.optimization.as_ref().unwrap().after.routed_crossings;
        assert!(crossings(&extended) < crossings(&plain));
        assert_eq!(
            extended.optimization.as_ref().unwrap().strategy,
            "routing-extended"
        );
    }

    #[test]
    fn an_interrupted_extended_selection_keeps_a_complete_result() {
        let mut s = import_view(&dense_graph().to_string(), 40).unwrap();
        s.options.routing_engine = RoutingEngine::Channel;
        s.options.extended_conflict_selection = true;
        let (refined, report) = refine_snapshot_job(
            "",
            &s,
            s.revision,
            s.layout_version,
            1,
            Some(&|| true),
            None,
        )
        .unwrap();
        assert!(!report
            .completed
            .iter()
            .any(|c| c.strategy == "routing-extended"));
        let drawn: BTreeSet<&str> = refined
            .graph
            .edges
            .iter()
            .filter(|e| e.target.is_some())
            .map(|e| e.id.as_str())
            .collect();
        let routed: BTreeSet<&str> = refined.routes.iter().map(|r| r.id.as_str()).collect();
        assert!(drawn.is_subset(&routed), "every drawn edge keeps a route");
    }

    #[test]
    fn extended_conflict_selection_adds_a_candidate_and_is_off_by_default() {
        let mut s = import_view(&dense_graph().to_string(), 40).unwrap();
        s.options.routing_engine = RoutingEngine::Channel;
        let (plain, plain_report) =
            refine_snapshot_job("", &s, s.revision, s.layout_version, 1, None, None).unwrap();
        assert!(!plain_report
            .completed
            .iter()
            .any(|c| c.strategy == "routing-extended"));
        s.options.extended_conflict_selection = true;
        let (extended, extended_report) =
            refine_snapshot_job("", &s, s.revision, s.layout_version, 1, None, None).unwrap();
        assert!(extended_report
            .completed
            .iter()
            .any(|c| c.strategy == "routing-extended"));
        // The extra candidate is chosen by the published key like every other candidate.
        let key = |snapshot: &Snapshot| arqlet_layout::published_key(snapshot).unwrap();
        assert!(
            key(&extended) <= key(&plain),
            "the published key does not get worse"
        );
        // The option is serialized only when it is set, so default snapshots keep their bytes.
        let default_json =
            serde_json::to_string(&import_view(&dense_graph().to_string(), 40).unwrap().options)
                .unwrap();
        assert!(!default_json.contains("extendedConflictSelection"));
    }

    #[test]
    fn job_report_is_identical_for_any_thread_count_and_echoes_job_id() {
        // The channel router is the only engine with A* expansions; the default legacy engine adds none.
        let mut s = import_view(&dense_graph().to_string(), 40).unwrap();
        s.options.routing_engine = RoutingEngine::Channel;
        let run = |threads: usize| {
            let (refined, report) = refine_snapshot_job(
                "job-1",
                &s,
                s.revision,
                s.layout_version,
                threads,
                None,
                None,
            )
            .unwrap();
            (serde_json::to_string(&refined).unwrap(), report)
        };
        let (serial_geometry, serial_report) = run(1);
        for threads in [2, MAX_THREADS] {
            let (geometry, report) = run(threads);
            assert_eq!(geometry, serial_geometry, "threads = {threads}");
            assert_eq!(report.job_id, serial_report.job_id);
            assert_eq!(report.input, serial_report.input);
            assert_eq!(report.completed, serial_report.completed);
            assert_eq!(report.excluded, serial_report.excluded);
            assert_eq!(
                deterministic_work(&report.counters),
                deterministic_work(&serial_report.counters),
                "threads = {threads}"
            );
            assert_eq!(
                report
                    .candidate_work
                    .iter()
                    .map(|w| (&w.candidate, deterministic_work(&w.counters)))
                    .collect::<Vec<_>>(),
                serial_report
                    .candidate_work
                    .iter()
                    .map(|w| (&w.candidate, deterministic_work(&w.counters)))
                    .collect::<Vec<_>>(),
                "threads = {threads}"
            );
        }
        assert_eq!(serial_report.job_id, "job-1");
        assert_eq!(serial_report.input.graph_revision, s.revision);
        assert_eq!(serial_report.input.base_layout_version, s.layout_version);
        let counters = &serial_report.counters;
        assert_eq!(counters.candidates_started, counters.candidates_completed);
        assert_eq!(serial_report.completed.len(), counters.candidates_completed);
        assert!(counters.candidates_started > 0);
        assert!(counters.routing_rounds > 0);
        assert!(counters.astar_expansions > 0);
    }

    #[test]
    fn immediate_stop_starts_no_candidate_and_keeps_the_input_geometry() {
        let s = import_view(&dense_graph().to_string(), 40).unwrap();
        let (refined, report) = refine_snapshot_job(
            "",
            &s,
            s.revision,
            s.layout_version,
            4,
            Some(&|| true),
            None,
        )
        .unwrap();
        assert_eq!(report.counters, WorkCounters::default());
        assert!(report.completed.is_empty());
        assert_eq!(
            serde_json::to_value(&refined.boxes).unwrap(),
            serde_json::to_value(&s.boxes).unwrap()
        );
    }

    #[test]
    fn an_interrupted_routing_search_publishes_no_partial_routes() {
        let mut s = import_view(&dense_graph().to_string(), 40).unwrap();
        s.options.routing_engine = RoutingEngine::Channel;
        // The stop signal fires after a few checks, while routing candidates are still in progress.
        let checks = std::sync::atomic::AtomicUsize::new(0);
        let stop = || checks.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 3;
        let (refined, report) =
            refine_snapshot_job("", &s, s.revision, s.layout_version, 1, Some(&stop), None)
                .unwrap();
        assert!(report.counters.candidates_completed < report.counters.candidates_started);
        assert!(refined.optimization.as_ref().is_some_and(|r| r.interrupted));
        let drawn: BTreeSet<&str> = refined
            .graph
            .edges
            .iter()
            .filter(|e| e.target.is_some())
            .map(|e| e.id.as_str())
            .collect();
        let routed: BTreeSet<&str> = refined.routes.iter().map(|r| r.id.as_str()).collect();
        assert!(
            drawn.is_subset(&routed),
            "every drawn edge keeps a complete route"
        );
    }

    #[test]
    fn an_ordering_candidate_stopped_mid_construction_returns_no_snapshot() {
        let s = import_view(&dense_graph().to_string(), 40).unwrap();
        let mode = arqlet_layout::CANDIDATES[0].0;
        let (stopped, _) =
            arqlet_layout::candidate_until(&s.graph, mode, &s.options, &|| true).unwrap();
        assert!(stopped.is_none());
        let (finished, _) =
            arqlet_layout::candidate_until(&s.graph, mode, &s.options, &|| false).unwrap();
        assert!(finished.is_some());
        // With the channel router, the ordering candidate's own routing is counted.
        let mut channel = s.options.clone();
        channel.routing_engine = RoutingEngine::Channel;
        let (_, search) =
            arqlet_layout::candidate_until(&s.graph, mode, &channel, &|| false).unwrap();
        assert!(search.expansions > 0);
    }

    #[test]
    fn placement_search_re_routes_are_counted_in_the_report() {
        let mut s = import_view(&dense_graph().to_string(), 40).unwrap();
        s.options.routing_engine = RoutingEngine::Channel;
        let report = |s: &Snapshot| {
            refine_snapshot_job("", s, s.revision, s.layout_version, 1, None, None)
                .unwrap()
                .1
                .counters
        };
        let without = report(&s);
        s.options.placement_search = 20;
        let with = report(&s);
        assert!(with.placement_trials > 0);
        assert!(with.astar_expansions > without.astar_expansions);
        assert!(with.astar_searches > without.astar_searches);
        assert!(with.largest_astar_search > 0);
        assert!(with.largest_astar_search <= with.astar_expansions);
    }

    #[test]
    fn placement_search_counts_its_trials_in_the_report() {
        let mut s = import_view(&dense_graph().to_string(), 40).unwrap();
        s.options.placement_search = 20;
        let (_, report) =
            refine_snapshot_job("", &s, s.revision, s.layout_version, 1, None, None).unwrap();
        assert!(report.counters.placement_trials > 0);
        assert!(report.counters.placement_trials <= 20);
    }

    #[test]
    fn candidate_ids_count_repeated_strategies_in_configured_order() {
        let ids = candidate_ids(&[
            Job::Ordering(1),
            Job::Routing,
            Job::Ordering(1),
            Job::Ordering(2),
        ]);
        let names: Vec<_> = ids
            .iter()
            .map(|id| (id.strategy, id.parameter, id.occurrence))
            .collect();
        assert_eq!(
            names,
            vec![
                ("ordering", 1, 0),
                ("routing", 0, 0),
                ("ordering", 1, 1),
                ("ordering", 2, 0)
            ]
        );
    }

    /// Two edges between the same endpoints plus one edge in the opposite direction.
    fn parallel_graph() -> serde_json::Value {
        serde_json::json!({"nodes":[
            {"id":"a","kind":"symbol","label":"A","parentId":null},
            {"id":"b","kind":"symbol","label":"B","parentId":null}],
          "edges":[
            {"id":"e1","kind":"calls","source":"a","target":"b"},
            {"id":"e2","kind":"calls","source":"a","target":"b"},
            {"id":"e3","kind":"calls","source":"b","target":"a"}]})
    }
    #[test]
    fn measurement_updates_merge_and_labels_reserve_free_rectangles() {
        let initial = import_graph(serde_json::from_value(parallel_graph()).unwrap(), 10).unwrap();
        let first = measure_snapshot(
            &initial,
            initial.revision,
            initial.layout_version,
            BTreeMap::from([(
                "a".into(),
                NodeSize {
                    width: 240.,
                    height: 70.,
                },
            )]),
        )
        .unwrap();
        let second = measure_snapshot(
            &first,
            first.revision,
            first.layout_version,
            BTreeMap::from([(
                "b".into(),
                NodeSize {
                    width: 220.,
                    height: 60.,
                },
            )]),
        )
        .unwrap();
        assert_eq!(second.boxes["a"].width, 240.);
        assert_eq!(second.boxes["b"].width, 220.);
        let labelled = configure_snapshot(
            &second,
            second.revision,
            second.layout_version,
            &serde_json::json!({"edgeLabels":{"e1":{"text":"calls","width":48,"height":22}},
                "portLabels":{"auto:e1:source":{"text":"out","width":32,"height":22}}}),
        )
        .unwrap();
        assert_eq!(labelled.labels.len(), 2);
        for label in &labelled.labels {
            for route in &labelled.routes {
                assert!(!arqlet_routing::hits(route, &label.bounds));
            }
            for rect in labelled.boxes.values() {
                assert!(
                    label.bounds.x >= rect.x + rect.width
                        || label.bounds.x + label.bounds.width <= rect.x
                        || label.bounds.y >= rect.y + rect.height
                        || label.bounds.y + label.bounds.height <= rect.y
                );
            }
        }
        assert!(configure_snapshot(
            &second,
            second.revision,
            second.layout_version,
            &serde_json::json!({"edgeLabels":{"e1":{"text":"bad","width":-1,"height":22}}})
        )
        .is_err());
    }
    #[test]
    fn bundling_preserves_explicit_port_contracts() {
        let mut graph: Graph = serde_json::from_value(parallel_graph()).unwrap();
        graph.ports.push(Port {
            id: "out".into(),
            node_id: "a".into(),
            side: PortSide::Bottom,
            offset: 0.3,
            row: None,
        });
        graph.edge_ports.push(EdgePorts {
            edge_id: "e2".into(),
            source_port: Some("out".into()),
            target_port: None,
        });
        let snapshot = import_graph(graph, 10).unwrap();
        let bundled = configure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            &serde_json::json!({"bundling":"complete"}),
        )
        .unwrap();
        assert!(bundled.routes.iter().all(|r| r.bundle_of.is_none()));
        let port = bundled
            .resolved_ports
            .iter()
            .find(|p| p.id == "out")
            .unwrap();
        let route = bundled.routes.iter().find(|r| r.id == "e2").unwrap();
        assert_eq!(route.points[0].x, port.position.x);
        assert_eq!(route.points[0].y, port.position.y);
    }
    #[test]
    fn config_sets_are_strict_and_preserve_measurements_and_geometry() {
        let initial = import_graph(serde_json::from_value(parallel_graph()).unwrap(), 10).unwrap();
        let measured = measure_snapshot(
            &initial,
            initial.revision,
            initial.layout_version,
            BTreeMap::from([(
                "a".into(),
                NodeSize {
                    width: 240.,
                    height: 70.,
                },
            )]),
        )
        .unwrap();
        let mut config = ConfigSet::from_options("test".into(), &measured.options);
        config.appearance.as_mut().unwrap().show_ports = false;
        let apply = |value: serde_json::Value| {
            configure_snapshot(
                &measured,
                measured.revision,
                measured.layout_version,
                &serde_json::json!({"configSet":value}),
            )
        };
        let value = serde_json::to_value(&config).unwrap();
        let changed = apply(value.clone()).unwrap();
        assert!(!changed.options.appearance.show_ports);
        assert_eq!(changed.options.node_sizes, measured.options.node_sizes);
        assert_eq!(
            serde_json::to_value(&changed.routes).unwrap(),
            serde_json::to_value(&measured.routes).unwrap()
        );
        assert_eq!(
            ConfigSet::from_options("test".into(), &changed.options),
            config
        );
        for (path, bad) in [
            ("schemaVersion", serde_json::json!(2)),
            ("unknown", serde_json::json!(true)),
        ] {
            let mut v = value.clone();
            v[path] = bad;
            assert!(apply(v).is_err());
        }
        let mut v = value.clone();
        v["routing"]["style"] = serde_json::json!("curved");
        assert!(apply(v).is_err());
        let mut v = value.clone();
        v["layout"]["algorithm"] = serde_json::json!("sequence");
        assert!(apply(v).is_err());
        let mut v = value.clone();
        v["layout"]["candidates"] = serde_json::json!([1, 1]);
        assert!(apply(v).is_err());
    }
    #[test]
    fn complete_bundling_shares_one_route_with_weight() {
        let mut graph: Graph = serde_json::from_value(parallel_graph()).unwrap();
        graph.default_direction = DirectionConstraint::Fixed(Orientation::Lr);
        let snapshot = import_graph(graph, 10).unwrap();
        let options = LayoutOptions {
            bundling: Bundling::Complete,
            ..legacy()
        };
        let bundled = configure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            &serde_json::json!({"bundling": "complete"}),
        )
        .unwrap();
        assert_eq!(bundled.options, options);
        let leader = bundled.routes.iter().find(|r| r.id == "e1").unwrap();
        let member = bundled.routes.iter().find(|r| r.id == "e2").unwrap();
        assert_eq!(leader.weight, 2);
        assert!(leader.bundle_of.is_none());
        assert_eq!(member.bundle_of.as_deref(), Some("e1"));
        assert_eq!(member.points.len(), leader.points.len());
        // The reverse edge is a different direction and is not bundled.
        let reverse = bundled.routes.iter().find(|r| r.id == "e3").unwrap();
        assert_eq!(reverse.weight, 1);
        assert!(reverse.bundle_of.is_none());
        // Drawn-route metrics do not count members separately.
        let quality = arqlet_layout::quality(&bundled, &bundled);
        assert_eq!(quality.shared_route_pairs, 0);
    }
    /// Bundle members must end where their leader's drawn route ends, both after layout and after
    /// the routing-only refinement, which recomputes ports.
    #[test]
    fn bundle_members_share_the_leader_endpoints_after_layout_and_refinement() {
        let mut graph: Graph = serde_json::from_value(parallel_graph()).unwrap();
        graph.default_direction = DirectionConstraint::Fixed(Orientation::Lr);
        let snapshot = import_graph(graph, 10).unwrap();
        let bundled = configure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            &serde_json::json!({"bundling": "complete"}),
        )
        .unwrap();
        let refined = refine_snapshot(&bundled, bundled.revision, bundled.layout_version).unwrap();
        for snap in [&bundled, &refined] {
            let position = |edge: &str, endpoint: &str| {
                snap.resolved_ports
                    .iter()
                    .find(|p| p.edge_id == edge && p.endpoint == endpoint)
                    .unwrap()
                    .position
                    .clone()
            };
            for endpoint in ["source", "target"] {
                let (leader, member) = (position("e1", endpoint), position("e2", endpoint));
                assert_eq!(
                    (leader.x, leader.y),
                    (member.x, member.y),
                    "{endpoint} port of member differs"
                );
            }
            let leader_route = snap.routes.iter().find(|r| r.id == "e1").unwrap();
            assert_eq!(leader_route.points[0].y, position("e1", "source").y);
        }
    }
    /// A hub with many incoming edges, the case in the exported view where arrowheads piled up.
    fn hub_snapshot(sources: usize) -> Snapshot {
        let mut nodes =
            vec![serde_json::json!({"id":"hub","kind":"symbol","label":"Hub","parentId":null})];
        let mut edges = Vec::new();
        for i in 0..sources {
            nodes.push(serde_json::json!({"id":format!("s{i}"),"kind":"symbol","label":format!("S{i}"),"parentId":null}));
            edges.push(serde_json::json!({"id":format!("e{i}"),"kind":"calls","source":format!("s{i}"),"target":"hub"}));
        }
        let graph: Graph = serde_json::from_value(serde_json::json!({
            "defaultDirection":"lr","nodes":nodes,"edges":edges}))
        .unwrap();
        import_graph(graph, 100).unwrap()
    }
    #[test]
    fn port_spacing_grows_nodes_until_every_side_has_room() {
        let base = hub_snapshot(12);
        // Spacing 20 needs 11 slots × 20 = 220 along the busiest side, wider than the default 180.
        let spaced = configure_snapshot(
            &base,
            base.revision,
            base.layout_version,
            &serde_json::json!({"minPortSpacing": 20}),
        )
        .unwrap();
        let hub = spaced.boxes["hub"].clone();
        assert!(
            hub.width >= 220. - 1e-6 || hub.height >= 220. - 1e-6,
            "hub {hub:?} was not grown for its ports"
        );
        // Ports on each side of the hub keep at least the requested spacing.
        for side in [
            PortSide::Left,
            PortSide::Right,
            PortSide::Top,
            PortSide::Bottom,
        ] {
            let mut along: Vec<f64> = spaced
                .resolved_ports
                .iter()
                .filter(|p| p.node_id == "hub" && p.side == side)
                .map(|p| {
                    if matches!(side, PortSide::Left | PortSide::Right) {
                        p.position.y
                    } else {
                        p.position.x
                    }
                })
                .collect();
            along.sort_by(f64::total_cmp);
            for pair in along.windows(2) {
                assert!(
                    pair[1] - pair[0] >= 20. - 1e-6,
                    "{side:?} ports too close: {pair:?}"
                );
            }
        }
        assert!(!spaced
            .routing_issues
            .iter()
            .any(|i| i.contains("nicht erreichbar")));
        assert!(
            base.boxes["hub"].height >= 54.,
            "the hierarchical pipeline must retain the prototype minimum size"
        );
    }
    #[test]
    fn placement_grid_puts_boxes_and_positions_on_grid_lines() {
        let base = hub_snapshot(6);
        let gridded = configure_snapshot(
            &base,
            base.revision,
            base.layout_version,
            &serde_json::json!({"placementGrid": 8}),
        )
        .unwrap();
        assert!(gridded.options.layered_construction);
        for rect in gridded.boxes.values() {
            for v in [rect.x, rect.y, rect.width, rect.height] {
                assert!(
                    ((v / 8.).round() * 8. - v).abs() < 1e-9,
                    "{v} is off the grid"
                );
            }
        }
    }
    /// Free positioning must keep containers' padding and header: the slide once let a child touch the
    /// container's far edge, which made every candidate invalid.
    #[test]
    fn free_placement_keeps_container_geometry_valid() {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for (c, container) in ["left", "right"].iter().enumerate() {
            nodes.push(serde_json::json!({"id":container,"kind":"container","label":container,"parentId":null}));
            for i in 0..3 {
                nodes.push(serde_json::json!({"id":format!("{container}{i}"),"kind":"symbol","label":format!("n{c}{i}"),"parentId":container}));
            }
        }
        for i in 0..3 {
            edges.push(serde_json::json!({"id":format!("e{i}"),"kind":"calls","source":format!("left{i}"),"target":format!("right{}", 2 - i)}));
        }
        let graph: Graph = serde_json::from_value(serde_json::json!({
            "defaultDirection":"lr","containerDirections":{"left":"tb","right":"tb"},
            "nodes":nodes,"edges":edges}))
        .unwrap();
        let base = import_graph(graph, 20).unwrap();
        let slid = configure_snapshot(
            &base,
            base.revision,
            base.layout_version,
            &serde_json::json!({"freePlacement": true}),
        )
        .unwrap();
        assert!(
            slid.geometry_issues.is_empty(),
            "{:?}",
            slid.geometry_issues
        );
        let refined = refine_snapshot(&slid, slid.revision, slid.layout_version);
        assert!(refined.is_ok(), "{:?}", refined.err());
    }
    /// The placement search is a refinement candidate: its result must keep valid geometry and must
    /// never be worse than the view it started from under the configured objectives.
    #[test]
    fn placement_search_keeps_geometry_and_is_never_worse() {
        let base = import_view(&dense_graph().to_string(), 40).unwrap();
        let searched = configure_snapshot(
            &base,
            base.revision,
            base.layout_version,
            &serde_json::json!({"placementSearch": 60}),
        )
        .unwrap();
        assert!(searched
            .layout_diagnostics
            .iter()
            .any(|message| message.contains("placementSearch")));
        let refined =
            refine_snapshot(&searched, searched.revision, searched.layout_version).unwrap();
        assert!(
            refined.geometry_issues.is_empty(),
            "{:?}",
            refined.geometry_issues
        );
        let report = refined.optimization.as_ref().unwrap();
        assert!(
            !arqlet_layout::better_by(&report.before, &report.after, &refined.options.objectives),
            "refinement made the view worse"
        );
    }
    /// With a placement grid, the placement search may only move nodes onto grid lines.
    #[test]
    fn placement_search_respects_the_placement_grid() {
        let base = import_view(&dense_graph().to_string(), 40).unwrap();
        let gridded = configure_snapshot(
            &base,
            base.revision,
            base.layout_version,
            &serde_json::json!({"placementGrid": 8, "placementSearch": 80}),
        )
        .unwrap();
        assert!(gridded.options.layered_construction);
        assert!(gridded
            .layout_diagnostics
            .iter()
            .any(|message| message.contains("placementGrid")));
        let refined = refine_snapshot(&gridded, gridded.revision, gridded.layout_version).unwrap();
        assert!(
            refined.geometry_issues.is_empty(),
            "{:?}",
            refined.geometry_issues
        );
        for rect in refined.boxes.values() {
            for v in [rect.x, rect.y] {
                assert!(
                    ((v / 8.).round() * 8. - v).abs() < 1e-9,
                    "{v} is off the grid"
                );
            }
        }
    }
    #[test]
    fn default_options_are_omitted_from_serialized_snapshot() {
        // The public default (single pass, ADR 0016), not this module's `legacy` import.
        let snapshot = super::import_view(&fixture(), 20).unwrap();
        let json = serde_json::to_value(&snapshot).unwrap();
        assert!(json.get("options").is_none());
        assert!(serde_json::to_value(&snapshot.routes[0])
            .unwrap()
            .get("weight")
            .is_none());
    }
    #[test]
    fn configure_changes_candidates_and_advances_layout_version() {
        let snapshot = import_view(&fixture(), 20).unwrap();
        let fast = configure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            &serde_json::json!({"preset": "fast"}),
        )
        .unwrap();
        assert_eq!(fast.options.candidates, vec![1]);
        assert_eq!(fast.layout_version, snapshot.layout_version + 1);
        assert_eq!(fast.revision, snapshot.revision);
        let refined = refine_snapshot(&fast, fast.revision, fast.layout_version).unwrap();
        // Baseline, ordering, routing candidate, and one global routing round.
        assert_eq!(refined.optimization.unwrap().candidates_evaluated, 4);
        assert!(
            configure_snapshot(&snapshot, snapshot.revision, 99, &serde_json::json!({})).is_err()
        );
        assert!(configure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            &serde_json::json!({"preset": "unknown"})
        )
        .is_err());
    }
    #[test]
    fn shortest_direction_picks_the_smaller_end_to_end_sum() {
        let mut graph: Graph = serde_json::from_value(parallel_graph()).unwrap();
        graph.default_direction = DirectionConstraint::Free;
        graph.default_strategy = StrategyConstraint::Fixed(LayoutStrategy::Shortest);
        let snapshot = import_graph(graph, 10).unwrap();
        // Both layouts are valid; the chosen one must not have a longer end-to-end sum than the other.
        let mut lr: Graph = serde_json::from_value(parallel_graph()).unwrap();
        lr.default_direction = DirectionConstraint::Fixed(Orientation::Lr);
        let lr_snapshot = import_graph(lr, 10).unwrap();
        let mut tb: Graph = serde_json::from_value(parallel_graph()).unwrap();
        tb.default_direction = DirectionConstraint::Fixed(Orientation::Tb);
        let tb_snapshot = import_graph(tb, 10).unwrap();
        let span = |s: &Snapshot| {
            let c = |id: &str| {
                (
                    s.boxes[id].x + s.boxes[id].width / 2.,
                    s.boxes[id].y + s.boxes[id].height / 2.,
                )
            };
            let (a, b) = (c("a"), c("b"));
            (a.0 - b.0).p_hypot(a.1 - b.1)
        };
        assert!((span(&snapshot) - span(&lr_snapshot).min(span(&tb_snapshot))).abs() < 1e-9);
    }
    #[test]
    fn measured_node_sizes_change_geometry_and_reject_unknown_nodes() {
        let snapshot = import_view(&fixture(), 20).unwrap();
        let measured = measure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            BTreeMap::from([(
                "a".to_string(),
                NodeSize {
                    width: 400.,
                    height: 90.,
                },
            )]),
        )
        .unwrap();
        assert_eq!(measured.boxes["a"].width, 400.);
        assert_eq!(measured.boxes["a"].height, 90.);
        assert_eq!(measured.layout_version, snapshot.layout_version + 1);
        assert!(measure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            BTreeMap::from([(
                "missing".to_string(),
                NodeSize {
                    width: 1.,
                    height: 1.
                }
            )])
        )
        .is_err());
        assert!(measure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            BTreeMap::from([(
                "a".to_string(),
                NodeSize {
                    width: 0.,
                    height: 1.
                }
            )])
        )
        .is_err());
    }
    fn geometry_of(snapshot: &Snapshot) -> String {
        serde_json::to_string(&(&snapshot.boxes, &snapshot.routes)).unwrap()
    }
    fn contour(width: f64, height: f64, revision: u64) -> BTreeMap<String, Measurement> {
        BTreeMap::from([(
            "a".to_string(),
            Measurement {
                contour: Contour::Rect { width, height },
                geometry_revision: revision,
            },
        )])
    }
    #[test]
    fn a_measurement_that_changes_nothing_does_not_lay_out_again() {
        let snapshot = import_view(&fixture(), 20).unwrap();
        let (w, h) = (snapshot.boxes["a"].width, snapshot.boxes["a"].height);
        // The size the layout already gave the node, reported with a revision: recorded, not laid out.
        let same = measure_contours(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            contour(w, h, 1),
        )
        .unwrap();
        assert_eq!(same.layout_version, snapshot.layout_version);
        assert_eq!(geometry_of(&same), geometry_of(&snapshot));
        assert_eq!(same.options.geometry_revisions["a"], 1);
        assert!(same.options.node_sizes.is_empty());
        // The same size again with a newer revision, and a change within the tolerance: still no layout.
        let again =
            measure_contours(&same, same.revision, same.layout_version, contour(w, h, 2)).unwrap();
        assert_eq!(again.layout_version, snapshot.layout_version);
        assert_eq!(again.options.geometry_revisions["a"], 2);
        let near = measure_contours(
            &again,
            again.revision,
            again.layout_version,
            contour(w + MEASURE_TOLERANCE, h, 3),
        )
        .unwrap();
        assert_eq!(near.layout_version, snapshot.layout_version);
        assert!(near.options.node_sizes.is_empty());
        // A change beyond the tolerance lays out, and a late report is still refused.
        let grown = measure_contours(
            &near,
            near.revision,
            near.layout_version,
            contour(w + MEASURE_TOLERANCE + 1., h, 4),
        )
        .unwrap();
        assert_eq!(grown.layout_version, snapshot.layout_version + 1);
        assert_eq!(grown.boxes["a"].width, w + MEASURE_TOLERANCE + 1.);
        assert!(measure_contours(
            &grown,
            grown.revision,
            grown.layout_version,
            contour(w, h, 3)
        )
        .unwrap_err()
        .contains("Stale measurement"));
    }
    /// A pin or a requested size lays out only the node's area and the containers above it, and routes only the edges the change
    /// touches. The boxes are those a complete layout gives with the same options. The routes may differ from a complete
    /// routing, but they must be as valid (no more node hits or overlaps), and the routes of edges the change does not touch
    /// stay as they were.
    #[test]
    fn a_local_edit_places_like_a_complete_layout_and_routes_validly() {
        let mut fewer = false;
        for path in [
            "../../examples/fixtures/structure/nested.json",
            "../../examples/fixtures/structure/container.json",
            "../../examples/fixtures/structure/portfolio-nested.json",
        ] {
            let text = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
            )
            .unwrap();
            let base = import_previous_view(&text, 200).unwrap();
            let leaves: Vec<String> = base
                .graph
                .nodes
                .iter()
                .filter(|n| {
                    !base
                        .graph
                        .nodes
                        .iter()
                        .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
                })
                .map(|n| n.id.clone())
                .collect();
            for id in leaves {
                let at = base.boxes[&id].clone();
                let mut options = base.options.clone();
                options.pins.insert(
                    id.clone(),
                    Point {
                        x: at.x + 30.,
                        y: at.y + 10.,
                    },
                );
                options.node_sizes.insert(
                    id.clone(),
                    NodeSize {
                        width: at.width + 20.,
                        height: at.height,
                    },
                );
                let local = recompute_with(&base, &options).unwrap();
                let complete = recompute_layout(&base, &options, None).unwrap();
                let boxes_of = |s: &Snapshot| serde_json::to_string(&s.boxes).unwrap();
                assert_eq!(boxes_of(&local), boxes_of(&complete), "{path}: {id}");
                let hard = |s: &Snapshot| {
                    let m = arqlet_scene::metrics::measure(&arqlet_layout::published_drawing(s))
                        .unwrap();
                    m.line_node_hits
                        + m.container_traversals
                        + m.sibling_overlaps
                        + m.cross_branch_overlaps
                };
                assert!(
                    hard(&local) <= hard(&complete),
                    "{path}: {id}: {} against {}",
                    hard(&local),
                    hard(&complete)
                );
                let same_as_base = |s: &Snapshot| {
                    s.routes
                        .iter()
                        .filter(|r| {
                            base.routes.iter().find(|b| b.id == r.id).is_some_and(|b| {
                                serde_json::to_string(&b.points).unwrap()
                                    == serde_json::to_string(&r.points).unwrap()
                            })
                        })
                        .count()
                };
                assert!(
                    same_as_base(&local) >= same_as_base(&complete),
                    "{path}: {id}"
                );
                assert!(
                    local.recomputed_containers.len() <= complete.recomputed_containers.len(),
                    "{path}: {id}"
                );
                fewer |= local.recomputed_containers.len() < complete.recomputed_containers.len();
            }
        }
        assert!(
            fewer,
            "a local edit never laid out fewer areas than a complete layout"
        );
    }
    /// The contract of a local edit (it may differ from a complete layout): it never leaves the drawing with more routes through
    /// nodes than it had, or than a complete layout with the same options has; the routes of edges the edit does not touch stay
    /// as they were; and the same edit on the same drawing gives the same result.
    #[test]
    fn a_local_edit_never_leaves_the_drawing_worse_and_is_deterministic() {
        for path in [
            "../../examples/fixtures/congestion-trap.json",
            "../../examples/fixtures/structure/nested.json",
            "../../examples/fixtures/structure/portfolio-nested.json",
        ] {
            let text = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path),
            )
            .unwrap();
            let base = import_previous_view(&text, 200).unwrap();
            let hitting = |s: &Snapshot| {
                arqlet_routing::routes_hitting_nodes(
                    &s.graph,
                    &arqlet_layout::absolute_all(&s.graph, &s.boxes),
                    &s.routes,
                    arqlet_routing::Params::default(),
                )
            };
            let leaves: Vec<String> = base
                .graph
                .nodes
                .iter()
                .filter(|n| {
                    !base
                        .graph
                        .nodes
                        .iter()
                        .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
                })
                .map(|n| n.id.clone())
                .collect();
            for id in leaves {
                let at = base.boxes[&id].clone();
                for grow in [40., 600.] {
                    let mut options = base.options.clone();
                    options.node_sizes.insert(
                        id.clone(),
                        NodeSize {
                            width: at.width + grow,
                            height: at.height,
                        },
                    );
                    let local = recompute_with(&base, &options).unwrap();
                    let again = recompute_with(&base, &options).unwrap();
                    assert_eq!(
                        geometry_of(&local),
                        geometry_of(&again),
                        "{path}: {id}: deterministic"
                    );
                    let complete = recompute_layout(&base, &options, None).unwrap();
                    assert!(
                        hitting(&local) <= hitting(&base).max(hitting(&complete)),
                        "{path}: {id} +{grow}: {} routes through nodes, base {}, complete {}",
                        hitting(&local),
                        hitting(&base),
                        hitting(&complete)
                    );
                    let same = |s: &Snapshot| {
                        s.routes
                            .iter()
                            .filter(|r| {
                                base.routes.iter().find(|b| b.id == r.id).is_some_and(|b| {
                                    serde_json::to_string(&b.points).unwrap()
                                        == serde_json::to_string(&r.points).unwrap()
                                })
                            })
                            .count()
                    };
                    assert!(
                        same(&local) >= same(&complete),
                        "{path}: {id} +{grow}: {} routes kept, a complete layout keeps {}",
                        same(&local),
                        same(&complete)
                    );
                }
            }
        }
    }
    /// The report of a local edit says what was kept and made again; a complete layout and a refined result carry none; a new node
    /// is a local edit too (its edge has no earlier route, every other route is kept or carried).
    #[test]
    fn the_report_of_a_local_edit_counts_the_routes_and_a_new_node_is_local() {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/fixtures/structure/portfolio-nested.json"),
        )
        .unwrap();
        let base = import_view(&text, 200).unwrap();
        assert!(
            base.edit_report.is_none(),
            "a complete layout has no report"
        );
        let leaf = base
            .graph
            .nodes
            .iter()
            .find(|n| {
                !base
                    .graph
                    .nodes
                    .iter()
                    .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
            })
            .unwrap()
            .id
            .clone();
        let at = base.boxes[&leaf].clone();
        let mut options = base.options.clone();
        options.node_sizes.insert(
            leaf.clone(),
            NodeSize {
                width: at.width + 40.,
                height: at.height,
            },
        );
        let edited = recompute_with(&base, &options).unwrap();
        let report = edited
            .edit_report
            .clone()
            .expect("a local edit reports its work");
        assert_eq!(
            report.routes_kept + report.routes_rerouted,
            edited
                .routes
                .iter()
                .filter(|r| r.bundle_of.is_none())
                .count()
        );
        assert!(report.routes_kept > 0, "{report:?}");
        assert!(
            report.routes_through_nodes
                <= report
                    .routes_through_nodes_before
                    .max(report.routes_through_nodes)
        );
        // A new node: the report exists, the new edge is routed, the others are kept or carried.
        let added = add_node(&base, base.revision, &leaf, "Added").unwrap();
        let report = added
            .edit_report
            .clone()
            .expect("adding a node is a local edit");
        assert!(
            report.routes_rerouted >= 1 && report.routes_kept > 0,
            "{report:?}"
        );
        assert_eq!(added.graph.nodes.len(), base.graph.nodes.len() + 1);
        // A refinement does not carry the report of the edit.
        let refined =
            refine_snapshot_with(&edited, edited.revision, edited.layout_version, 1).unwrap();
        assert!(refined.edit_report.is_none());
    }
    #[test]
    fn appearance_and_labels_do_not_change_the_layout_identity() {
        let snapshot = import_view(&fixture(), 20).unwrap();
        let mut options = snapshot.options.clone();
        options.config_id = Some("labelled@1".to_string());
        let relabelled = recompute_with(&snapshot, &options).unwrap();
        assert_eq!(geometry_of(&relabelled), geometry_of(&snapshot));
    }
    #[test]
    fn objective_order_decides_the_refinement_choice() {
        let snapshot = import_view(&fixture(), 20).unwrap();
        // A: shorter but one routed crossing. B: longer and crossing-free.
        let base = arqlet_layout::quality(&snapshot, &snapshot);
        let mut a = base.clone();
        a.route_length = 10.;
        a.routed_crossings = 1;
        let mut b = base;
        b.route_length = 100.;
        b.routed_crossings = 0;
        // Default order: crossings come first, so B wins.
        assert!(arqlet_layout::better_by(
            &b,
            &a,
            &arqlet_model::DEFAULT_OBJECTIVES
        ));
        assert!(!arqlet_layout::better_by(
            &a,
            &b,
            &arqlet_model::DEFAULT_OBJECTIVES
        ));
        // Length first reverses the decision for the same two layouts.
        let length_first = [Objective::Cost, Objective::RoutedCrossings];
        assert!(arqlet_layout::better_by(&a, &b, &length_first));
    }
    /// Every preset in the repository is a strict config set, applies, and names itself `<id>@<version>`.
    #[test]
    fn shipped_presets_parse_apply_and_are_named() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets");
        let mut seen = 0;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let preset: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let id = preset["id"].as_str().unwrap();
            let version = preset["version"].as_u64().unwrap();
            let mut options = legacy();
            let config: ConfigSet = serde_json::from_value(preset["configSet"].clone())
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            config
                .apply(&mut options)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(options.config_id, Some(format!("{id}@{version}")));
            assert!(preset["graph"]["defaultDirection"].is_string());
            assert!(preset["budgetMs"].is_u64() && preset["refine"].is_boolean());
            seen += 1;
        }
        assert_eq!(seen, 5);
    }
    /// The engine's quality counts are the shared metrics in their own units: routed crossings are crossing pairs
    /// plus shared-endpoint pairs, and edge-node hits are line-node hits plus foreign container traversals.
    /// This pins that correspondence, so either side changing alone fails here.
    #[test]
    fn engine_counts_are_the_shared_metrics_in_their_units() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../packages/viewer/fixtures/state-tcp.snapshot.json");
        let snapshot: Snapshot =
            serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let quality =
            arqlet_layout::quality_with(&snapshot, &snapshot, &arqlet_model::DEFAULT_OBJECTIVES);
        let measured = arqlet_scene::metrics::measure(&drawing_of(&snapshot)).unwrap();
        assert_eq!(
            quality.edge_node_hits,
            measured.line_node_hits + measured.container_traversals
        );
        assert_eq!(
            quality.routed_crossings,
            measured.crossing_pairs + measured.shared_endpoint_crossing_pairs
        );
    }

    /// Absolute geometry of a snapshot for the shared metrics (the same mapping as benchmarks/profile_runner.py).
    fn drawing_of(snapshot: &Snapshot) -> arqlet_scene::metrics::Drawing {
        let parents: BTreeMap<&str, Option<&str>> = snapshot
            .graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
            .collect();
        let containers: BTreeSet<&str> = parents.values().flatten().copied().collect();
        let absolute = |id: &str| {
            let mut rect = snapshot.boxes[id].clone();
            let mut parent = parents[id];
            while let Some(p) = parent {
                rect.x += snapshot.boxes[p].x;
                rect.y += snapshot.boxes[p].y;
                parent = parents[p];
            }
            rect
        };
        let nodes = snapshot
            .graph
            .nodes
            .iter()
            .map(|n| {
                let rect = absolute(&n.id);
                arqlet_scene::metrics::DrawNode {
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
        let routes: BTreeMap<&str, &Route> =
            snapshot.routes.iter().map(|r| (r.id.as_str(), r)).collect();
        let port = |edge: &str, endpoint: &str| {
            snapshot
                .resolved_ports
                .iter()
                .find(|p| p.edge_id == edge && p.endpoint == endpoint)
                .map(|p| p.id.clone())
        };
        let edges = snapshot
            .graph
            .edges
            .iter()
            .filter(|e| e.target.is_some())
            .filter_map(|e| {
                let route = routes.get(e.id.as_str())?;
                if route.bundle_of.is_some() {
                    return None;
                }
                Some(arqlet_scene::metrics::DrawEdge {
                    id: e.id.clone(),
                    source: e.source.clone(),
                    target: e.target.clone()?,
                    points: route.points.clone(),
                    source_port: port(&e.id, "source"),
                    target_port: port(&e.id, "target"),
                    bundle_id: None,
                })
            })
            .collect();
        arqlet_scene::metrics::Drawing {
            nodes,
            edges,
            track_spacing: arqlet_scene::metrics::DEFAULT_TRACK_SPACING,
            buses: Vec::new(),
        }
    }
    /// A stopped refinement says why: the work budget, the time budget, or nothing when it completed.
    #[test]
    fn stop_reason_names_what_stopped_the_refinement() {
        let imported = import_view(&fixture(), 20).unwrap();
        let reason = |s: &Snapshot| s.optimization.as_ref().unwrap().stop_reason;
        let complete =
            refine_snapshot(&imported, imported.revision, imported.layout_version).unwrap();
        assert_eq!(reason(&complete), None);
        let work = refine_snapshot_limited(
            &imported,
            imported.revision,
            imported.layout_version,
            1,
            None,
            Some(1),
        )
        .unwrap();
        assert_eq!(reason(&work), Some(arqlet_model::StopReason::WorkBudget));
        let time = refine_request(
            &imported,
            imported.revision,
            imported.layout_version,
            1,
            Some(&serde_json::json!(0)),
            None,
        )
        .unwrap();
        assert_eq!(reason(&time), Some(arqlet_model::StopReason::TimeBudget));
    }

    /// A work budget counts candidate jobs in a fixed order: the same budget gives the same snapshot every time, it
    /// reports when it cut work, and a budget that covers all jobs changes nothing.
    #[test]
    fn work_budget_is_deterministic_and_reported() {
        let imported = import_view(&fixture(), 20).unwrap();
        let limited = |work: usize| {
            refine_snapshot_limited(
                &imported,
                imported.revision,
                imported.layout_version,
                1,
                None,
                Some(work),
            )
            .unwrap()
        };
        let one = limited(1);
        assert!(one.optimization.as_ref().unwrap().work_limited);
        assert_eq!(
            serde_json::to_vec(&one).unwrap(),
            serde_json::to_vec(&limited(1)).unwrap(),
            "the same budget gives the same snapshot"
        );
        let unlimited =
            refine_snapshot(&imported, imported.revision, imported.layout_version).unwrap();
        let generous = limited(10_000);
        assert!(!generous.optimization.as_ref().unwrap().work_limited);
        assert_eq!(
            serde_json::to_vec(&generous).unwrap(),
            serde_json::to_vec(&unlimited).unwrap(),
            "a budget that covers every job changes nothing"
        );
    }

    /// Logging changes nothing: under a deterministic work budget the logged run returns the same bytes as the plain run
    /// for one and for four threads, and the log itself is identical for both thread counts.
    #[test]
    fn candidate_log_does_not_change_the_result_and_is_thread_invariant() {
        let imported = import_view(&fixture(), 20).unwrap();
        let summary = |records: &[CandidateRecord]| -> Vec<String> {
            records
                .iter()
                .map(|r| {
                    format!(
                        "{} {} {:?} {:?} {:?} {:?} {:?} {:?} {:?} {} {} {} {:?}",
                        r.index,
                        r.phase,
                        r.round,
                        r.strategy,
                        r.outcome,
                        r.key,
                        r.hard_violations,
                        r.shadow_total,
                        r.geometry_fingerprint,
                        r.accepted,
                        r.selected,
                        r.published,
                        r.duplicate_of,
                    )
                })
                .collect()
        };
        for work in [None, Some(1), Some(10_000)] {
            let mut logs = Vec::new();
            for threads in [1, 4] {
                let plain = refine_snapshot_limited(
                    &imported,
                    imported.revision,
                    imported.layout_version,
                    threads,
                    None,
                    work,
                )
                .unwrap();
                let (logged, _, records) = refine_snapshot_logged(
                    &imported,
                    imported.revision,
                    imported.layout_version,
                    threads,
                    work,
                )
                .unwrap();
                assert_eq!(
                    serde_json::to_vec(&plain).unwrap(),
                    serde_json::to_vec(&logged).unwrap(),
                    "logging changed the result (work {work:?}, threads {threads})"
                );
                assert!(!records.is_empty(), "the run logged no candidates");
                logs.push(summary(&records));
            }
            assert_eq!(
                logs[0], logs[1],
                "the log depends on the thread count (work {work:?})"
            );
        }
    }

    /// The selected record is the final result: one record, with the result's geometry, when a candidate was selected;
    /// none when the input is kept. Every accepted record is a measured candidate, and duplicates point backwards.
    #[test]
    fn the_selected_record_is_the_final_geometry() {
        let imported = import_view(&fixture(), 20).unwrap();
        let (refined, _, records) = refine_snapshot_logged(
            &imported,
            imported.revision,
            imported.layout_version,
            1,
            None,
        )
        .unwrap();
        let selected: Vec<_> = records.iter().filter(|r| r.selected).collect();
        let kept = refined.optimization.as_ref().unwrap().strategy == "keep-existing";
        if kept {
            assert!(selected.is_empty());
        } else {
            assert_eq!(selected.len(), 1);
            assert_eq!(
                selected[0].geometry_fingerprint,
                geometry_fingerprint(&refined),
                "the selected record has the result's geometry"
            );
        }
        for record in &records {
            if record.accepted {
                assert_eq!(record.outcome, CandidateOutcome::Measured);
            }
            if let Some(first) = record.duplicate_of {
                assert!(first < record.index);
                assert_eq!(
                    records[first].geometry_fingerprint,
                    record.geometry_fingerprint
                );
            }
        }
    }

    /// A required clearance is a hard constraint: whatever the refinement publishes has no route within that clearance of
    /// a foreign node, or the input is kept as it was. The same request also rejects a non-positive clearance.
    #[test]
    fn a_required_clearance_is_never_published_violated() {
        let mut imported = import_view(&fixture(), 20).unwrap();
        imported.options.required_clearance = Some(20.);
        let (refined, _, _) = refine_snapshot_logged(
            &imported,
            imported.revision,
            imported.layout_version,
            1,
            None,
        )
        .unwrap();
        let hugs = arqlet_layout::published_metrics(&refined)
            .unwrap()
            .hug_pairs;
        let kept = refined.optimization.as_ref().unwrap().strategy == "keep-existing";
        assert!(
            hugs == 0 || kept,
            "a published result has {hugs} near-node pairs"
        );
        imported.options.required_clearance = Some(0.);
        assert!(refine_snapshot_logged(
            &imported,
            imported.revision,
            imported.layout_version,
            1,
            None,
        )
        .is_err());
    }

    /// A required clearance that the input violates is reported as a routing issue, not silently dropped.
    #[test]
    fn an_unmet_required_clearance_is_reported() {
        let mut snapshot = import_graph_with(
            serde_json::from_value(dense_graph()).unwrap(),
            40,
            &previous_pipeline_options(),
        )
        .unwrap();
        assert!(
            unmet_clearance_issue(&snapshot, None).is_none(),
            "no clearance is required"
        );
        snapshot.options.required_clearance = Some(12.);
        let issue = unmet_clearance_issue(&snapshot, None)
            .expect("the default layout violates a clearance of 12");
        assert!(
            issue.starts_with("required clearance 12 not met:"),
            "{issue}"
        );
    }

    /// Invalid requirement values are refused; large valid ones are respected, not capped.
    #[test]
    fn requirement_values_are_validated_not_capped() {
        let base = import_graph(serde_json::from_value(dense_graph()).unwrap(), 40).unwrap();
        let refine = |clearance: f64| {
            let mut s = base.clone();
            s.options.required_clearance = Some(clearance);
            refine_snapshot_logged(&s, s.revision, s.layout_version, 1, None).map(|(r, _, _)| r)
        };
        for invalid in [f64::NAN, f64::INFINITY, -1., 0.] {
            assert!(refine(invalid).is_err(), "{invalid} is refused");
        }
        assert!(
            refine(1e300).is_err(),
            "an inflation that overflows is refused"
        );
        assert!(
            refine(1e6).is_ok(),
            "a large valid clearance is respected, not capped"
        );
        assert!(
            refine(1e12).is_ok(),
            "a clearance far beyond any drawing is still respected"
        );
    }

    /// A drawing larger than the extent limit reports it; the limit is never enforced by the search.
    #[test]
    fn an_extent_above_the_limit_is_reported() {
        let mut s = import_graph(serde_json::from_value(dense_graph()).unwrap(), 40).unwrap();
        s.options.max_extent = Some(10.);
        let issue = extent_issue(&s).expect("the default layout is larger than 10 units");
        assert!(issue.starts_with("extent limit 10 exceeded"), "{issue}");
        s.options.max_extent = Some(1e9);
        assert!(extent_issue(&s).is_none());
        s.options.max_extent = Some(-1.);
        assert!(refine_snapshot_logged(&s, s.revision, s.layout_version, 1, None).is_err());
    }

    /// Every publication carries the archive as it stands then: sorted by key, bounded, one entry per geometry, and its best
    /// entry is no worse than the published result. The archive of the last logged run equals `archive_indices`.
    #[test]
    fn publications_carry_the_bounded_archive() {
        let imported = import_previous_view(&fixture(), 20).unwrap();
        let mut publications = Vec::new();
        let (_, _, records, final_archive) = refine_logged(
            "archive",
            &imported,
            imported.revision,
            imported.layout_version,
            1,
            None,
            None,
            true,
            &mut |p: &Publication| publications.push(p.clone()),
        )
        .unwrap();
        assert!(
            !publications.is_empty(),
            "the fixture publishes improvements"
        );
        for p in &publications {
            assert!(p.archive.len() <= ARCHIVE_CAPACITY);
            assert!(
                p.archive.windows(2).all(|w| w[0].key <= w[1].key),
                "archive is ordered"
            );
            let fingerprints: BTreeSet<_> =
                p.archive.iter().map(|e| &e.geometry_fingerprint).collect();
            assert_eq!(
                fingerprints.len(),
                p.archive.len(),
                "one entry per geometry"
            );
            assert!(
                p.archive[0].key <= p.key,
                "the archive's best entry is no worse than the publication"
            );
        }
        // The archive kept during the run is the archive of the log, entry for entry.
        let log_archive: Vec<(usize, String, Vec<i64>)> =
            archive_indices(&records, ARCHIVE_CAPACITY)
                .iter()
                .map(|&i| {
                    (
                        records[i].index,
                        records[i].strategy.clone().unwrap(),
                        records[i].key.clone().unwrap(),
                    )
                })
                .collect();
        let run_archive: Vec<(usize, String, Vec<i64>)> = final_archive
            .iter()
            .map(|e| (e.index, e.strategy.clone(), e.key.clone()))
            .collect();
        assert_eq!(run_archive, log_archive);
    }

    /// The legacy work counters are deterministic: the same job on the same input counts the same work, and a run stopped
    /// early still reports the work it did before the stop.
    #[test]
    fn legacy_work_counters_are_reproducible_and_survive_an_interruption() {
        let graph: Graph = serde_json::from_value(dense_graph()).unwrap();
        let options = previous_pipeline_options();
        let (complete, first) =
            arqlet_layout::candidate_until(&graph, 1, &options, &|| false).unwrap();
        assert!(complete.is_some());
        assert!(
            first.legacy_candidates > 0,
            "the legacy router evaluated no candidate: {first:?}"
        );
        let (_, second) = arqlet_layout::candidate_until(&graph, 1, &options, &|| false).unwrap();
        assert_eq!(first.legacy_candidates, second.legacy_candidates);
        assert_eq!(first.legacy_grid_pops, second.legacy_grid_pops);
        assert_eq!(
            first.clearance_widening_rounds,
            second.clearance_widening_rounds
        );
        #[cfg(feature = "benchmark-timings")]
        {
            assert!(first.routing_nanos > 0);
            assert!(second.routing_nanos > 0);
        }
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let stop = || calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 3;
        let (interrupted, partial) =
            arqlet_layout::candidate_until(&graph, 1, &options, &stop).unwrap();
        assert!(interrupted.is_none(), "the stop was not reached");
        assert!(
            partial.legacy_candidates > 0,
            "the work before the stop is reported"
        );
        assert!(partial.legacy_candidates < first.legacy_candidates);
    }

    /// A budget that cuts the plan runs the likeliest job first; a budget that does not cut leaves the plan unchanged.
    #[test]
    fn the_first_stage_budget_runs_the_likeliest_jobs_first() {
        let names = |(jobs, cut): (Vec<Job>, bool)| {
            (jobs.iter().map(|j| j.strategy()).collect::<Vec<_>>(), cut)
        };
        let plan = || vec![Job::Ordering(1), Job::Routing, Job::Search];
        let all = vec![("ordering", 1), ("routing", 0), ("search", 0)];
        assert_eq!(
            names(allocate_first_stage(plan(), Some(2))),
            (vec![("routing", 0), ("ordering", 1)], true)
        );
        assert_eq!(
            names(allocate_first_stage(plan(), Some(3))),
            (all.clone(), false)
        );
        assert_eq!(names(allocate_first_stage(plan(), None)), (all, false));
    }

    /// Stopped in the middle of a run, the refinement still returns complete geometry that is no worse than its input,
    /// and its archive holds measured candidates only.
    #[test]
    fn a_mid_run_stop_keeps_a_complete_archive_and_result() {
        let snapshot = import_view(&fixture(), 20).unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let stop = || calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 25;
        let (refined, _, records, _) = refine_logged(
            "cancel",
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            1,
            Some(&stop),
            None,
            true,
            &mut |_| {},
        )
        .unwrap();
        assert!(
            refined.optimization.as_ref().unwrap().interrupted,
            "the stop was not reached"
        );
        assert!(
            !records.is_empty(),
            "the stop fired before any candidate finished"
        );
        assert!(refined.geometry_issues.is_empty());
        assert!(
            arqlet_layout::published_key(&refined).unwrap()
                <= arqlet_layout::published_key(&snapshot).unwrap()
        );
        let archive = archive_indices(&records, 4);
        assert!(archive
            .iter()
            .all(|&i| records[i].outcome == CandidateOutcome::Measured));
    }

    /// With component packing set, the unpacked alternative is planned where a container has several components, and the
    /// key decides between the two layouts.
    #[test]
    fn the_unpacked_alternative_runs_where_a_container_has_components() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/bpmn-pools.json")).unwrap(),
        )
        .unwrap();
        let mut snapshot = import_graph(graph, 100).unwrap();
        snapshot.options.component_packing = true;
        let (_, report, records) = refine_snapshot_logged(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            1,
            None,
        )
        .unwrap();
        assert!(
            records.iter().any(|r| r
                .id
                .as_ref()
                .is_some_and(|id| id.strategy == "unpacked-components")),
            "the unpacked alternative is in the log"
        );
        assert!(!report
            .excluded
            .iter()
            .any(|e| e.strategy == "unpacked-components"));
    }

    /// A strategy that does not apply to the view is excluded with the reason, not silently dropped.
    #[test]
    fn inapplicable_strategies_are_excluded_with_a_reason() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure/disconnected.json"))
                .unwrap(),
        )
        .unwrap();
        // A limit of 3 selects one component of this graph: the view has no second component to pack, so the packing
        // strategy is excluded with its reason.
        let mut snapshot = import_graph(graph, 3).unwrap();
        snapshot.options.component_packing = true;
        let (_, report, _) = refine_snapshot_logged(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            1,
            None,
        )
        .unwrap();
        let reason = |name: &str| {
            report
                .excluded
                .iter()
                .find(|e| e.strategy == name)
                .map(|e| e.reason.as_str())
        };
        assert_eq!(
            reason("unpacked-components"),
            Some("no container has more than one component")
        );
        assert_eq!(
            reason("routing-last"),
            Some("needs the channel routing engine")
        );
        assert_eq!(reason("flat-region"), Some("flatRegion is off"));
    }

    /// With `routedCandidates` each ordering candidate also competes in its routed form; without it none is planned.
    #[test]
    fn routed_ordering_candidates_are_planned_only_when_requested() {
        let base = import_graph(serde_json::from_value(dense_graph()).unwrap(), 40).unwrap();
        let routed_names = |snapshot: &Snapshot| {
            refine_snapshot_logged(
                snapshot,
                snapshot.revision,
                snapshot.layout_version,
                1,
                None,
            )
            .unwrap()
            .2
            .iter()
            .filter_map(|r| {
                r.id.as_ref()
                    .filter(|id| id.strategy == "routed-ordering")
                    .map(|id| id.parameter)
            })
            .collect::<Vec<_>>()
        };
        assert!(routed_names(&base).is_empty());
        let mut on = base.clone();
        on.options.routed_candidates = true;
        let planned = routed_names(&on);
        assert_eq!(
            planned,
            on.options.candidates.clone(),
            "one routed job per ordering mode"
        );
    }

    #[test]
    fn the_hierarchical_default_keeps_every_configured_ordering_candidate() {
        let snapshot = import_graph(serde_json::from_value(dense_graph()).unwrap(), 40).unwrap();
        assert!(snapshot.options.layered_construction);
        let (planned, _) = plan_first_stage(&snapshot);
        let candidates: Vec<_> = planned
            .iter()
            .filter_map(|job| match job {
                Job::Ordering(candidate) => Some(*candidate),
                _ => None,
            })
            .collect();
        assert_eq!(candidates, snapshot.options.candidates);
    }

    /// The common entry picks the path by structure and reports what did not apply; its result is the joint layout's key or
    /// better.
    #[test]
    fn the_structured_entry_reports_its_decision() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure/container.json"))
                .unwrap(),
        )
        .unwrap();
        let laid = arqlet_layout::compute(&graph, None, &BTreeSet::new(), &legacy()).unwrap();
        let (view, _, _) =
            refine_snapshot_logged(&laid, laid.revision, laid.layout_version, 1, None).unwrap();
        let reply: serde_json::Value = serde_json::from_str(&dispatch(
            &serde_json::json!({
                "op": "structuredRefine",
                "snapshot": view,
                "expectedRevision": view.revision,
                "expectedLayoutVersion": view.layout_version,
            })
            .to_string(),
        ))
        .unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        assert!(
            matches!(reply["route"].as_str(), Some("container" | "joint")),
            "{reply}"
        );
        assert_eq!(reply["snapshot"]["baseRevision"], view.revision);
        assert_eq!(reply["snapshot"]["layoutVersion"], view.layout_version + 1);
        let stale = serde_json::from_str::<serde_json::Value>(&dispatch(
            &serde_json::json!({
                "op": "structuredRefine",
                "snapshot": view,
                "expectedRevision": view.revision + 1,
                "expectedLayoutVersion": view.layout_version,
            })
            .to_string(),
        ))
        .unwrap();
        assert_eq!(stale["ok"], false);
        assert!(reply["notApplied"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["path"] == "components"));
        assert_eq!(reply["snapshot"]["boxes"].as_object().unwrap().len(), 6);
    }

    /// The container operation publishes the chosen snapshot with its report, through the same dispatch.
    #[test]
    fn the_container_operation_publishes_the_chosen_result() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure/container.json"))
                .unwrap(),
        )
        .unwrap();
        let reply: serde_json::Value = serde_json::from_str(&dispatch(
            &serde_json::json!({"op": "containerRefine", "graph": graph}).to_string(),
        ))
        .unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(reply["container"], "box");
        assert!(reply["chosen"] == "composed" || reply["chosen"] == "joint");
        assert_eq!(reply["snapshot"]["boxes"].as_object().unwrap().len(), 6);
        assert_eq!(reply["snapshot"]["routes"].as_array().unwrap().len(), 4);
        let bad = serde_json::from_str::<serde_json::Value>(&dispatch(
            &serde_json::json!({"op": "containerRefine", "graph": {"nodes": 1}}).to_string(),
        ))
        .unwrap();
        assert_eq!(bad["ok"], false);
    }

    /// The component operation publishes the chosen snapshot with its report, through the same dispatch as the other operations.
    #[test]
    fn the_component_operation_publishes_the_chosen_result() {
        let graph = serde_json::json!({
            "nodes": [
                {"id": "a", "kind": "symbol", "label": "a", "parentId": null},
                {"id": "b", "kind": "symbol", "label": "b", "parentId": null},
                {"id": "c", "kind": "symbol", "label": "c", "parentId": null},
                {"id": "d", "kind": "symbol", "label": "d", "parentId": null}
            ],
            "edges": [
                {"id": "1", "kind": "flow", "source": "a", "target": "b"},
                {"id": "2", "kind": "flow", "source": "c", "target": "d"}
            ]
        });
        let reply: serde_json::Value = serde_json::from_str(&dispatch(
            &serde_json::json!({"op": "componentRefine", "graph": graph}).to_string(),
        ))
        .unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        assert!(reply["chosen"] == "composed" || reply["chosen"] == "joint");
        assert_eq!(reply["components"].as_array().unwrap().len(), 2);
        assert_eq!(reply["snapshot"]["boxes"].as_object().unwrap().len(), 4);
        assert_eq!(reply["snapshot"]["routes"].as_array().unwrap().len(), 2);
        let bad = serde_json::from_str::<serde_json::Value>(&dispatch(
            &serde_json::json!({"op": "componentRefine", "graph": {"nodes": 1}}).to_string(),
        ))
        .unwrap();
        assert_eq!(bad["ok"], false);
    }

    /// Acceptance for archive refinement: a runner-up that was not the first-stage favourite wins after one limited routing
    /// pass, under the shared key, and the result is no worse than without the pass. On org-chart the routed runner-up wins.
    #[test]
    fn an_archive_runner_up_can_win_after_one_routing_pass() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/org-chart.json")).unwrap(),
        )
        .unwrap();
        // The archive pass is checked without a required clearance: the claim about org-chart was measured with
        // proximity as a weighted term. Under the default clearance the fixture does not show a winning runner-up.
        let base = import_graph_with(
            graph,
            100,
            &LayoutOptions {
                required_clearance: None,
                ..previous_pipeline_options()
            },
        )
        .unwrap();
        let key = |s: &Snapshot| arqlet_layout::published_key(s).unwrap();
        let (plain, _, _) =
            refine_snapshot_logged(&base, base.revision, base.layout_version, 1, None).unwrap();
        let mut limited = base.clone();
        limited.options.archive_refinement = 1;
        let (refined, _, records) =
            refine_snapshot_logged(&limited, base.revision, base.layout_version, 1, None).unwrap();
        assert!(
            records.iter().any(|r| r.phase == "archive" && r.accepted),
            "an archive pass won"
        );
        assert!(
            key(&refined) < key(&plain),
            "the runner-up improves the key"
        );
    }

    /// The declared capabilities name every strategy the planner can produce, once, with the same names as the job
    /// identities, and the round jobs are exactly the declared round strategies.
    #[test]
    fn capabilities_match_the_planned_jobs() {
        let jobs = [
            Job::Ordering(1),
            Job::Routing,
            Job::RoutingLast,
            Job::RoutingExtended,
            Job::Search,
            Job::FlatRegion,
            Job::OtherPortPlacement,
            Job::UnpackedComponents,
            Job::RoutedOrdering(1),
        ];
        let mut names = BTreeSet::new();
        for job in jobs {
            let capabilities = job.capabilities();
            assert_eq!(capabilities.name, job.strategy().0);
            assert!(names.insert(capabilities.name), "duplicate strategy name");
            assert!(capabilities.first_stage);
            assert_eq!(
                capabilities.global_rounds,
                matches!(job, Job::Routing | Job::RoutingLast),
                "{} has a different round set",
                capabilities.name
            );
            if matches!(job, Job::RoutingLast | Job::RoutingExtended) {
                assert!(capabilities.channel_engine);
            }
            // The flat region is the only strategy that needs a flat graph and overrides the port placement, and both
            // are declared, so a reader sees the contract without opening the adapter.
            let flat = matches!(job, Job::FlatRegion);
            assert_eq!(capabilities.graph == "flat", flat);
            assert_eq!(capabilities.port_override.is_some(), flat);
        }
    }

    /// The archive is bounded, ordered by key with the index as tie-break, and its best entry is the selected candidate
    /// (the selection keeps the smallest key among the measured candidates).
    #[test]
    fn the_archive_is_bounded_ordered_and_starts_with_the_selection() {
        let imported = import_view(&fixture(), 20).unwrap();
        let (refined, _, records) = refine_snapshot_logged(
            &imported,
            imported.revision,
            imported.layout_version,
            1,
            None,
        )
        .unwrap();
        let archive = archive_indices(&records, 3);
        assert!(archive.len() <= 3);
        let keys: Vec<_> = archive.iter().map(|&i| records[i].key.clone()).collect();
        assert!(
            keys.windows(2).all(|w| w[0] <= w[1]),
            "archive is not ordered"
        );
        if let Some(selected) = records.iter().find(|r| r.selected) {
            assert_eq!(archive.first(), Some(&selected.index));
            assert_eq!(
                geometry_fingerprint(&refined),
                selected.geometry_fingerprint
            );
        }
        assert!(archive_indices(&records, 0).is_empty());
    }

    /// Property over fixtures and presets: a refined result never publishes a higher shared conflict key than its
    /// input. The congestion trap (three parallel edges through one corridor beside a wall) is hand-built; the
    /// state machine is a frozen input. The largest preset (qualitaet) is covered by the benchmark matrix instead.
    #[test]
    fn refinement_never_publishes_a_worse_key_than_its_input() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let presets = root.join("presets");
        let graphs = [
            root.join("examples/fixtures/congestion-trap.json"),
            root.join("benchmarks/inputs/state-tcp.json"),
        ];
        for path in graphs {
            let graph: Graph =
                serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
            let imported = import_graph(graph, 100).unwrap();
            for entry in std::fs::read_dir(&presets).unwrap() {
                let file = entry.unwrap().path();
                let preset: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
                if preset["id"] == "qualitaet" {
                    continue;
                }
                let input = configure_snapshot(
                    &imported,
                    imported.revision,
                    imported.layout_version,
                    &serde_json::json!({"configSet": preset["configSet"].clone()}),
                )
                .unwrap();
                let refined =
                    refine_snapshot(&input, input.revision, input.layout_version).unwrap();
                let before = arqlet_layout::published_key(&input).unwrap();
                let after = arqlet_layout::published_key(&refined).unwrap();
                assert!(
                    after <= before,
                    "{} with {}: key {:?} is worse than input {:?}",
                    path.display(),
                    file.display(),
                    after,
                    before
                );
            }
        }
    }
    /// Optimized ports are never published worse than symmetric ones under the shared key: the refined optimized view
    /// is compared with the refined symmetric view of the same source, and the better one is kept.
    #[test]
    fn optimized_ports_are_never_worse_than_symmetric() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/state-tcp.json")).unwrap(),
        )
        .unwrap();
        let imported = import_graph(graph, 100).unwrap();
        let view = |placement: &str| {
            let configured = configure_snapshot(
                &imported,
                imported.revision,
                imported.layout_version,
                &serde_json::json!({"portPlacement": placement}),
            )
            .unwrap();
            refine_snapshot(&configured, configured.revision, configured.layout_version).unwrap()
        };
        let symmetric = view("symmetric");
        let optimized = view("optimized");
        assert!(
            arqlet_layout::published_key(&optimized).unwrap()
                <= arqlet_layout::published_key(&symmetric).unwrap(),
            "the optimized view publishes a key no worse than the symmetric one"
        );
    }

    /// A layout preset leaves the user's display preference alone; a direction change is a custom change.
    #[test]
    fn presets_keep_appearance_and_direction_changes_clear_the_name() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets");
        let preset: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("lesbar.json")).unwrap())
                .unwrap();
        let snapshot = import_view(&fixture(), 20).unwrap();
        let hidden = configure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            &serde_json::json!({"appearance": {"showPorts": false}}),
        )
        .unwrap();
        let named = configure_snapshot(
            &hidden,
            hidden.revision,
            hidden.layout_version,
            &serde_json::json!({"configSet": preset["configSet"].clone()}),
        )
        .unwrap();
        assert!(
            !named.options.appearance.show_ports,
            "the preset keeps the port display"
        );
        let turned = set_direction(
            &named,
            named.revision,
            named.layout_version,
            None,
            Some(DirectionConstraint::Fixed(Orientation::Tb)),
        )
        .unwrap();
        assert_eq!(
            turned.options.config_id, None,
            "a manual direction change is custom"
        );
    }

    /// Clearing the name or changing only the display does not recompute the layout.
    #[test]
    fn display_and_name_changes_keep_the_geometry() {
        let snapshot = import_view(&fixture(), 20).unwrap();
        let hidden = configure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            &serde_json::json!({"appearance": {"showPorts": false}}),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&hidden.boxes).unwrap(),
            serde_json::to_value(&snapshot.boxes).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&hidden.routes).unwrap(),
            serde_json::to_value(&snapshot.routes).unwrap()
        );
    }
    /// A config set names the options; any custom change clears the name, and reapplying the same set
    /// keeps the snapshot as it is (no new layout version).
    #[test]
    fn config_id_follows_config_sets_and_clears_on_custom_changes() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../presets");
        let preset: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("lesbar.json")).unwrap())
                .unwrap();
        let config = preset["configSet"].clone();
        let snapshot = import_view(&fixture(), 20).unwrap();
        let named = configure_snapshot(
            &snapshot,
            snapshot.revision,
            snapshot.layout_version,
            &serde_json::json!({"configSet": config}),
        )
        .unwrap();
        assert_eq!(named.options.config_id.as_deref(), Some("lesbar@1"));
        let again = configure_snapshot(
            &named,
            named.revision,
            named.layout_version,
            &serde_json::json!({"configSet": config}),
        )
        .unwrap();
        assert_eq!(
            again.layout_version, named.layout_version,
            "reapplying is a no-op"
        );
        let custom = configure_snapshot(
            &named,
            named.revision,
            named.layout_version,
            &serde_json::json!({"bundling": "none"}),
        )
        .unwrap();
        assert_eq!(custom.options.config_id, None);
    }
    /// The viewer fixtures are engine output: scene fixtures must equal what the engine builds from
    /// the snapshot fixtures, so the JavaScript tests cannot drift from the Rust geometry.
    #[test]
    fn viewer_scene_fixtures_match_the_engine() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/viewer/fixtures");
        for name in ["state-tcp", "bpmn-pools"] {
            let snapshot: Snapshot = serde_json::from_str(
                &std::fs::read_to_string(dir.join(format!("{name}.snapshot.json"))).unwrap(),
            )
            .unwrap();
            let expected: serde_json::Value = serde_json::from_str(
                &std::fs::read_to_string(dir.join(format!("{name}.scene.json"))).unwrap(),
            )
            .unwrap();
            let actual =
                serde_json::to_value(arqlet_scene::build_scene(&snapshot).unwrap()).unwrap();
            // JavaScript writes 318, Rust writes 318.0: compare numbers as floats.
            assert_eq!(
                as_floats(actual),
                as_floats(expected),
                "{name} scene fixture is stale"
            );
        }
    }
    fn as_floats(value: serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Number(n) => serde_json::json!(n.as_f64()),
            serde_json::Value::Array(items) => items.into_iter().map(as_floats).collect(),
            serde_json::Value::Object(map) => {
                map.into_iter().map(|(k, v)| (k, as_floats(v))).collect()
            }
            other => other,
        }
    }
    #[test]
    fn malformed_parent_is_rejected() {
        let mut v: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        v["nodes"][0]["parentId"] = serde_json::json!("a");
        assert!(import_view(&v.to_string(), 20).is_err());
    }
    #[test]
    fn refinement_preserves_graph_revision_and_rejects_stale_layout() {
        let s = import_view(&fixture(), 20).unwrap();
        let refined = refine_snapshot(&s, s.revision, s.layout_version).unwrap();
        assert_eq!(refined.revision, s.revision);
        assert_eq!(refined.layout_version, s.layout_version + 1);
        assert_eq!(
            serde_json::to_value(&refined.graph).unwrap(),
            serde_json::to_value(&s.graph).unwrap()
        );
        assert!(refine_snapshot(&refined, s.revision, s.layout_version).is_err());
        let edited = add_node(&refined, refined.revision, "a", "After refine").unwrap();
        assert_eq!(edited.revision, s.revision + 1);
    }
    #[test]
    fn refinement_improves_crossed_two_layer_fixture() {
        let graph = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: ["a", "b", "c", "d"]
                .iter()
                .map(|id| Node {
                    id: (*id).into(),
                    kind: "symbol".into(),
                    label: (*id).into(),
                    parent_id: None,
                    shape: Default::default(),
                })
                .collect(),
            edges: vec![
                Edge {
                    id: "ad".into(),
                    kind: "calls".into(),
                    source: "a".into(),
                    target: Some("d".into()),
                    weight: None,
                },
                Edge {
                    id: "bc".into(),
                    kind: "calls".into(),
                    source: "b".into(),
                    target: Some("c".into()),
                    weight: None,
                },
            ],
            buses: Vec::new(),
        };
        let s =
            arqlet_layout::compute(&graph, None, &BTreeSet::new(), &previous_pipeline_options())
                .unwrap();
        let refined = refine_snapshot(&s, 0, 0).unwrap();
        let length = |s: &Snapshot| {
            s.routes
                .iter()
                .flat_map(|r| r.points.windows(2))
                .map(|w| (w[0].x - w[1].x).abs() + (w[0].y - w[1].y).abs())
                .sum::<f64>()
        };
        assert!(length(&refined) < length(&s));
        let again = refine_snapshot(&refined, 0, 1).unwrap();
        assert_eq!(
            serde_json::to_value(&again.boxes).unwrap(),
            serde_json::to_value(&refined.boxes).unwrap()
        );
    }
    #[test]
    fn refinement_repairs_overlapping_input_and_does_not_trust_cached_diagnostics() {
        let mut s = import_view(&fixture(), 20).unwrap();
        let a = s.boxes["a"].clone();
        s.boxes.insert("b".into(), a);
        s.geometry_issues.clear();
        assert!(!arqlet_scene::validate_geometry(
            &s.graph,
            &s.boxes,
            arqlet_scene::GeometryRules::default()
        )
        .is_empty());
        let repaired = refine_snapshot(&s, s.revision, s.layout_version).unwrap();
        assert!(repaired.geometry_issues.is_empty());
        assert!(arqlet_scene::validate_geometry(
            &repaired.graph,
            &repaired.boxes,
            arqlet_scene::GeometryRules::default()
        )
        .is_empty());
        assert_ne!(repaired.optimization.unwrap().strategy, "keep-existing");
    }
    #[test]
    fn collapse_refine_expand_preserves_canonical_graph() {
        let original = import_view(&fixture(), 20).unwrap();
        let collapsed = toggle_container(&original, 0, 0, "root").unwrap();
        assert_eq!(collapsed.graph.nodes.len(), 1);
        assert!(collapsed.graph.edges.is_empty());
        assert_eq!(collapsed.revision, original.revision);
        assert!(toggle_container(&collapsed, 0, 0, "root").is_err());
        let refined = refine_snapshot(&collapsed, 0, collapsed.layout_version).unwrap();
        let expanded = toggle_container(&refined, 0, refined.layout_version, "root").unwrap();
        assert_eq!(
            serde_json::to_value(&expanded.graph).unwrap(),
            serde_json::to_value(&original.graph).unwrap()
        );
        assert!(expanded.geometry_issues.is_empty());
    }
    #[test]
    fn external_edges_and_edits_survive_collapsed_view() {
        let mut input: serde_json::Value = serde_json::from_str(&fixture()).unwrap();
        input["nodes"].as_array_mut().unwrap().push(
            serde_json::json!({"id":"outside","kind":"symbol","label":"Outside","parentId":null}),
        );
        input["edges"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"id":"cross","kind":"calls","source":"a","target":"outside"}));
        let original = import_view(&input.to_string(), 20).unwrap();
        let collapsed = toggle_container(&original, 0, 0, "root").unwrap();
        let edge = collapsed
            .graph
            .edges
            .iter()
            .find(|e| e.id == "cross")
            .unwrap();
        assert_eq!(edge.source, "root");
        assert_eq!(edge.target.as_deref(), Some("outside"));
        let edited = add_node(&collapsed, 0, "outside", "Added while closed").unwrap();
        let expanded =
            toggle_container(&edited, edited.revision, edited.layout_version, "root").unwrap();
        assert!(expanded.graph.nodes.iter().any(|n| n.id == "a"));
        assert!(expanded.graph.nodes.iter().any(|n| n.id == "local:1"));
        assert_eq!(
            expanded
                .graph
                .edges
                .iter()
                .find(|e| e.id == "cross")
                .unwrap()
                .source,
            "a"
        );
        assert_eq!(expanded.graph.edges.len(), original.graph.edges.len() + 1);
    }
    /// Reversed orientations are the mirror images of the forward ones on their rank axis: a chain a -> b runs leftwards
    /// under `rl` and upwards under `bt`, every route stays complete, and the geometry is valid.
    #[test]
    fn reversed_orientations_mirror_the_forward_arrangement() {
        let s = import_view(&fixture(), 20).unwrap();
        let rl = set_direction(
            &s,
            0,
            0,
            None,
            Some(DirectionConstraint::Fixed(Orientation::Rl)),
        )
        .unwrap();
        assert!(rl.boxes["b"].x < rl.boxes["a"].x, "rl runs right to left");
        assert_eq!(rl.boxes["b"].y, rl.boxes["a"].y);
        assert!(rl.geometry_issues.is_empty(), "{:?}", rl.geometry_issues);
        assert_eq!(rl.routes.len(), s.routes.len());
        let bt = set_direction(
            &s,
            0,
            0,
            None,
            Some(DirectionConstraint::Fixed(Orientation::Bt)),
        )
        .unwrap();
        assert!(bt.boxes["b"].y < bt.boxes["a"].y, "bt runs bottom to top");
        assert_eq!(bt.boxes["b"].x, bt.boxes["a"].x);
        assert!(bt.geometry_issues.is_empty(), "{:?}", bt.geometry_issues);
        assert_eq!(bt.routes.len(), s.routes.len());
    }

    #[test]
    fn directions_inherit_override_and_survive_collapse() {
        let s = import_view(&fixture(), 20).unwrap();
        let tb = set_direction(
            &s,
            0,
            0,
            None,
            Some(DirectionConstraint::Fixed(Orientation::Tb)),
        )
        .unwrap();
        assert!(tb.boxes["b"].y > tb.boxes["a"].y);
        assert_eq!(tb.boxes["b"].x, tb.boxes["a"].x);
        assert_eq!(
            direction_for(&tb.graph, Some("root")),
            Mode::Orientation(Orientation::Tb)
        );
        let lr = set_direction(
            &tb,
            0,
            tb.layout_version,
            Some("root"),
            Some(DirectionConstraint::Fixed(Orientation::Lr)),
        )
        .unwrap();
        assert!(lr.boxes["b"].x > lr.boxes["a"].x);
        let closed = toggle_container(&lr, 0, lr.layout_version, "root").unwrap();
        let opened = toggle_container(&closed, 0, closed.layout_version, "root").unwrap();
        assert_eq!(
            direction_for(&opened.graph, Some("root")),
            Mode::Orientation(Orientation::Lr)
        );
        let inherited =
            set_direction(&opened, 0, opened.layout_version, Some("root"), None).unwrap();
        assert_eq!(
            direction_for(&inherited.graph, Some("root")),
            Mode::Orientation(Orientation::Tb)
        );
        let hybrid = set_direction(
            &inherited,
            0,
            inherited.layout_version,
            None,
            Some(DirectionConstraint::Free),
        )
        .unwrap();
        assert!(hybrid.geometry_issues.is_empty());
        assert!(set_direction(
            &hybrid,
            0,
            0,
            None,
            Some(DirectionConstraint::Fixed(Orientation::Lr))
        )
        .is_err());
    }
}

#[cfg(test)]
mod container_sizing_runtime_tests {
    use super::*;

    fn nested() -> Snapshot {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure/nested.json")).unwrap(),
        )
        .unwrap();
        import_graph(graph, 10).unwrap()
    }

    /// Configures the view and refines it with the structured entry, as the playground does after an edit: container
    /// sizes take effect in the composition, not in the flat recomputation.
    fn configured(snapshot: &Snapshot, patch: serde_json::Value) -> Result<Snapshot, String> {
        let configured =
            configure_snapshot(snapshot, snapshot.revision, snapshot.layout_version, &patch)?;
        let reply: serde_json::Value = serde_json::from_str(&dispatch(
            &serde_json::json!({
                "op": "structuredRefine",
                "snapshot": configured,
                "expectedRevision": configured.revision,
                "expectedLayoutVersion": configured.layout_version,
            })
            .to_string(),
        ))
        .unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        Ok(serde_json::from_value(reply["snapshot"].clone()).unwrap())
    }

    #[test]
    fn a_pinned_child_outside_the_content_stays_inside_under_a_fixed_width() {
        let mut base = nested();
        base.options
            .pins
            .insert("a".into(), arqlet_model::Point { x: 1600., y: 40. });
        let fixed = configured(
            &base,
            serde_json::json!({"containerSizing": {"inner": {"width": {"fixed": 2916}}}}),
        )
        .unwrap();
        assert!(
            !fixed
                .geometry_issues
                .iter()
                .any(|i| i.kind == "container_bounds"
                    && i.node_ids.first().map(String::as_str) == Some("a")),
            "{:?}",
            fixed.geometry_issues
        );
    }

    #[test]
    fn a_pinned_grandchild_stays_inside_a_fixed_ancestor() {
        // The dragged leaf sits in `inner`, which sits in `outer`: the fixed width of `outer` is the border that holds it.
        let mut base = nested();
        base.options
            .pins
            .insert("a".into(), arqlet_model::Point { x: 1600., y: 40. });
        let fixed = configured(
            &base,
            serde_json::json!({"containerSizing": {"outer": {"width": {"fixed": 2916}}}}),
        )
        .unwrap();
        let outer = fixed.boxes["outer"].clone();
        let a = fixed.boxes["a"].clone();
        assert!(
            a.x + a.width <= outer.x + outer.width + 1e-6,
            "a {a:?} outside outer {outer:?}: {:?}",
            fixed.geometry_issues
        );
    }

    #[test]
    fn a_pinned_child_outside_the_content_enlarges_its_container() {
        // A dragged leaf keeps its pin; the container must still hold it, or the drop would leave it outside its border.
        let base = nested();
        let mut options = base.options.clone();
        options
            .pins
            .insert("a".into(), arqlet_model::Point { x: 900., y: 40. });
        let laid = arqlet_layout::compute(&base.graph, None, &BTreeSet::new(), &options).unwrap();
        let issues = arqlet_scene::validate_geometry(
            &base.graph,
            &laid.boxes,
            arqlet_scene::GeometryRules::default(),
        );
        assert!(
            !issues.iter().any(|i| i.kind == "container_bounds"
                && i.node_ids.first().map(String::as_str) == Some("a")),
            "{issues:?}"
        );
    }

    #[test]
    fn an_import_fills_its_limit_from_several_components() {
        // The dataset has no edges between its areas: one neighbourhood cannot reach the limit, so the import continues
        // with the next most connected node.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/nested-tree-radial.json"))
                .unwrap(),
        )
        .unwrap();
        let snapshot = import_graph(graph, 60).unwrap();
        assert!(
            snapshot.graph.nodes.len() >= 60,
            "{}",
            snapshot.graph.nodes.len()
        );
    }

    #[test]
    fn the_view_limit_is_capped_at_ten_thousand() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure/nested.json")).unwrap(),
        )
        .unwrap();
        assert!(import_graph(graph.clone(), MAX_VIEW_LIMIT).is_ok());
        assert_eq!(
            import_graph(graph, MAX_VIEW_LIMIT + 1).unwrap_err(),
            "View limit must be between 1 and 10000"
        );
    }

    #[test]
    fn a_fixed_size_below_its_content_is_refined_and_kept_as_a_named_conflict() {
        // A fixed size the children do not fit is an authored conflict (ADR 0010): the search must still produce a
        // layout, keep the fixed size, and name the conflict. It must not refuse the whole view.
        let base = nested();
        let refined = configured(
            &base,
            serde_json::json!({"containerSizing": {"inner": {"width": {"fixed": 20}}}}),
        )
        .unwrap();
        assert!((refined.boxes["inner"].width - 20.).abs() < 1e-6);
        assert!(
            refined
                .geometry_issues
                .iter()
                .any(|i| i.kind == "container-fixed-below-content"),
            "{:?}",
            refined.geometry_issues
        );
    }

    #[test]
    fn a_minimum_on_an_inner_container_grows_it_and_its_ancestor() {
        let base = nested();
        let before = base.boxes["inner"].width;
        let grown = configured(
            &base,
            serde_json::json!({"containerSizing": {"inner": {"width": {"min": before + 200.}}}}),
        )
        .unwrap();
        assert!((grown.boxes["inner"].width - (before + 200.)).abs() < 1e-6);
        assert!(grown.boxes["outer"].width >= grown.boxes["inner"].width);
    }

    #[test]
    fn a_fixed_size_below_its_content_is_kept_and_reported() {
        // The plain layout keeps the authored size; the geometry validator reports the children that do not fit.
        // The structured refinement refuses such a layout explicitly (see ADR 0010, open items), so only the plain one is
        // checked here.
        let base = nested();
        let mut options = base.options.clone();
        options.container_sizing.insert(
            "inner".into(),
            ContainerSizing {
                width: Some(AxisSize::Fixed(20.)),
                height: None,
            },
        );
        let laid = arqlet_layout::compute(&base.graph, None, &BTreeSet::new(), &options).unwrap();
        assert!((laid.boxes["inner"].width - 20.).abs() < 1e-6);
        let issues = arqlet_scene::validate_geometry(
            &base.graph,
            &laid.boxes,
            arqlet_scene::GeometryRules::default(),
        );
        assert!(
            issues
                .iter()
                .any(|i| i.node_ids.iter().any(|id| id == "inner")),
            "{issues:?}"
        );
    }

    #[test]
    fn a_fixed_size_below_its_content_is_named_with_the_size_it_needs() {
        let base = nested();
        let mut options = base.options.clone();
        options.container_sizing.insert(
            "inner".into(),
            ContainerSizing {
                width: Some(AxisSize::Fixed(20.)),
                height: None,
            },
        );
        let laid = arqlet_layout::compute(&base.graph, None, &BTreeSet::new(), &options).unwrap();
        let conflicts = fixed_size_conflicts(&base.graph, &laid.boxes, &options.container_sizing);
        assert_eq!(conflicts.len(), 1, "{conflicts:?}");
        assert_eq!(conflicts[0].kind, "container-fixed-below-content");
        assert_eq!(conflicts[0].node_ids, vec!["inner".to_string()]);
        assert!(
            conflicts[0].message.contains("below the required"),
            "{}",
            conflicts[0].message
        );
        assert!(is_authored_conflict(
            &conflicts[0],
            &options.container_sizing
        ));
    }

    /// The playground sets a container's routing through the `routing` operation and then shows the accepted drawing: the
    /// inner edge of the container must be drawn straight.
    #[test]
    fn a_container_routing_set_through_the_operation_is_drawn() {
        let base = nested();
        let reply: serde_json::Value = serde_json::from_str(&dispatch(
            &serde_json::json!({
                "op": "routing",
                "snapshot": base,
                "nodeId": "inner",
                "routing": "straight",
                "expectedRevision": base.revision,
                "expectedLayoutVersion": base.layout_version,
            })
            .to_string(),
        ))
        .unwrap();
        assert_eq!(reply["ok"], true, "{reply}");
        let drawn: Snapshot = serde_json::from_value(reply["snapshot"].clone()).unwrap();
        let inner_edge = drawn.routes.iter().find(|r| r.id == "2").unwrap();
        assert_eq!(inner_edge.points.len(), 2, "{:?}", inner_edge.points);
        // The playground draws the scene of the snapshot, so the scene must carry the same straight edge.
        let scene: serde_json::Value = serde_json::from_str(&dispatch(
            &serde_json::json!({"op": "scene", "snapshot": drawn}).to_string(),
        ))
        .unwrap();
        let scene_edge = scene["scene"]["routes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == "2")
            .unwrap();
        assert_eq!(
            scene_edge["points"].as_array().unwrap().len(),
            2,
            "{scene_edge}"
        );
        assert!(
            drawn.straight_edges.contains(&"2".to_string()),
            "{:?}",
            drawn.straight_edges
        );
        // The background refinement that the playground runs after an edit must keep the straight edge.
        let refined: serde_json::Value = serde_json::from_str(&dispatch(
            &serde_json::json!({
                "op": "refine",
                "snapshot": drawn,
                "expectedRevision": drawn.revision,
                "expectedLayoutVersion": drawn.layout_version,
            })
            .to_string(),
        ))
        .unwrap();
        let refined_snapshot: Snapshot =
            serde_json::from_value(refined["snapshot"].clone()).unwrap();
        assert!(
            refined_snapshot.straight_edges.contains(&"2".to_string()),
            "refined set {:?}",
            refined_snapshot.straight_edges
        );
        let refined_edge = refined_snapshot
            .routes
            .iter()
            .find(|r| r.id == "2")
            .unwrap();
        assert_eq!(
            refined_edge.points.len(),
            2,
            "refinement reroutes: {:?}",
            refined_edge.points
        );
    }

    #[test]
    fn the_plain_layout_also_honours_a_minimum() {
        let base = nested();
        let mut options = base.options.clone();
        options.container_sizing.insert(
            "inner".into(),
            ContainerSizing {
                width: Some(AxisSize::Min(900.)),
                height: None,
            },
        );
        let laid = arqlet_layout::compute(&base.graph, None, &BTreeSet::new(), &options).unwrap();
        assert!(
            laid.boxes["inner"].width >= 900. - 1e-6,
            "{:?}",
            laid.boxes["inner"]
        );
    }

    #[test]
    fn an_entry_replaces_only_its_container_and_null_removes_it() {
        let base = nested();
        let both = configured(
            &base,
            serde_json::json!({"containerSizing": {
                "inner": {"width": {"min": 300}},
                "outer": {"height": {"min": 400}}
            }}),
        )
        .unwrap();
        let only_outer = configured(
            &both,
            serde_json::json!({"containerSizing": {"inner": null}}),
        )
        .unwrap();
        assert_eq!(
            only_outer
                .options
                .container_sizing
                .keys()
                .collect::<Vec<_>>(),
            vec!["outer"]
        );
        assert!((only_outer.boxes["outer"].height - 400.).abs() < 1e-6);
    }

    #[test]
    fn an_invalid_size_is_rejected_before_layout() {
        let base = nested();
        assert!(configured(
            &base,
            serde_json::json!({"containerSizing": {"inner": {"width": {"min": -5}}}})
        )
        .is_err());
        assert!(configured(&base, serde_json::json!({"containerSizing": {"inner": 7}})).is_err());
        assert_eq!(base.revision, base.revision);
    }
}
