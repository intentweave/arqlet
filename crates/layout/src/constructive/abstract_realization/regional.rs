//! Detached adjacent-rank trials. Only the selected area is placed; positive
//! size changes propagate without ancestor reordering. Full routing is mandatory.
use super::*;
use arqlet_model::Snapshot;

fn same_rect(a: &Rect, b: &Rect) -> bool {
    [a.x, a.y, a.width, a.height]
        .iter()
        .zip([b.x, b.y, b.width, b.height])
        .all(|(a, b)| a.to_bits() == b.to_bits())
}

fn same_plan(a: &arqlet_model::StoredAreaPlan, b: &arqlet_model::StoredAreaPlan) -> bool {
    a.format == b.format
        && a.vertical == b.vertical
        && a.reversed == b.reversed
        && a.ranks == b.ranks
        && a.chains == b.chains
        && a.reversed_links == b.reversed_links
        && a.borders == b.borders
        && a.dummies.len() == b.dummies.len()
        && a.dummies
            .iter()
            .all(|(id, r)| b.dummies.get(id).is_some_and(|s| same_rect(r, s)))
}

fn same_port(a: &ResolvedPort, b: &ResolvedPort) -> bool {
    a.id == b.id
        && a.node_id == b.node_id
        && a.edge_id == b.edge_id
        && a.endpoint == b.endpoint
        && a.explicit == b.explicit
        && a.side == b.side
        && a.position.x.to_bits() == b.position.x.to_bits()
        && a.position.y.to_bits() == b.position.y.to_bits()
}

fn borders(plan: &crate::layered_plan::AreaPlan) -> Vec<(String, usize, usize, [u64; 4])> {
    let ids: BTreeSet<_> = plan.borders.values().collect();
    plan.ranks
        .iter()
        .enumerate()
        .flat_map(|(rank, elements)| {
            let ids = &ids;
            elements
                .iter()
                .enumerate()
                .filter_map(move |(position, id)| {
                    if !ids.contains(id) {
                        return None;
                    }
                    plan.dummies.get(id).map(|r| {
                        (
                            id.clone(),
                            rank,
                            position,
                            [
                                r.x.to_bits(),
                                r.y.to_bits(),
                                r.width.to_bits(),
                                r.height.to_bits(),
                            ],
                        )
                    })
                })
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Swap {
    pub area: String,
    pub rank: usize,
    pub left: usize,
}

/// Private input remains bound to its source, never a publishable snapshot.
pub struct Candidate<'a> {
    source: &'a Snapshot,
    input: Snapshot,
    pub placed_areas: Vec<String>,
    pub propagated_areas: Vec<String>,
    pub changed_boxes: Vec<String>,
    pub moved_absolute: Vec<String>,
    pub changed_plans: Vec<String>,
    pub changed_ports: Vec<String>,
    pub work: crate::SearchCounters,
    pub crossings_before: usize,
    pub crossings_after: usize,
}

impl Candidate<'_> {
    pub fn complete(
        &self,
        session: &mut planning::Session,
    ) -> Result<completion::CompletedDrawing, String> {
        completion::run_in_session(&self.input, session)
    }

    pub fn boxes(&self) -> &BTreeMap<String, Rect> {
        &self.input.boxes
    }

    pub fn plans(&self) -> &BTreeMap<String, arqlet_model::StoredAreaPlan> {
        &self.input.layered_plans
    }

    pub fn ports(&self) -> &[ResolvedPort] {
        &self.input.resolved_ports
    }

    pub fn source(&self) -> &Snapshot {
        self.source
    }
}

fn inputs(snapshot: &Snapshot) -> Result<LayeredPlans, String> {
    if !snapshot.options.layered_construction
        || !snapshot.options.hierarchical_ordering
        || snapshot.options.global_layering
        || snapshot.options.visible_node_budget != 0
    {
        return Err(
            "Regional trials require unflattened hierarchical placement without a view budget"
                .into(),
        );
    }
    let plans: LayeredPlans = snapshot
        .layered_plans
        .iter()
        .map(|(area, p)| {
            crate::layered_plan::AreaPlan::from_stored(p)
                .map(|p| (area.clone(), p))
                .ok_or_else(|| format!("Unsupported regional plan format for {area}"))
        })
        .collect::<Result<_, _>>()?;
    growth::check_capability(&snapshot.graph, &plans, &snapshot.options)?;
    if snapshot.boxes.len() != snapshot.graph.nodes.len()
        || snapshot
            .graph
            .nodes
            .iter()
            .any(|n| !snapshot.boxes.contains_key(&n.id))
    {
        return Err("Regional trials require exactly one box per node".into());
    }
    validate_inputs(
        &snapshot.graph,
        &crate::absolute_all(&snapshot.graph, &snapshot.boxes),
        &snapshot.resolved_ports,
        &plans,
        crate::constructive_spacing(&snapshot.options),
    )?;
    Ok(plans)
}

/// All adjacent real-sibling swaps, in stable area/rank order. No quality
/// estimate removes a move; callers must state any diagnostic sampling budget.
pub fn swaps(snapshot: &Snapshot) -> Result<Vec<Swap>, String> {
    let plans = inputs(snapshot)?;
    let nodes: BTreeSet<_> = snapshot.graph.nodes.iter().map(|n| n.id.as_str()).collect();
    Ok(plans
        .iter()
        .flat_map(|(area, plan)| {
            let nodes = &nodes;
            plan.ranks.iter().enumerate().flat_map(move |(rank, ids)| {
                ids.windows(2).enumerate().filter_map(move |(left, pair)| {
                    (nodes.contains(pair[0].as_str()) && nodes.contains(pair[1].as_str())).then(
                        || Swap {
                            area: area.clone(),
                            rank,
                            left,
                        },
                    )
                })
            })
        })
        .collect())
}

pub fn prepare<'a>(snapshot: &'a Snapshot, swap: &Swap) -> Result<Candidate<'a>, String> {
    let old_plans = inputs(snapshot)?;
    let mut plans = old_plans.clone();
    let plan = plans.get_mut(&swap.area).ok_or("Unknown regional area")?;
    let crossings_before = plan.crossings();
    let rank = plan
        .ranks
        .get_mut(swap.rank)
        .ok_or("Unknown regional rank")?;
    let pair = rank
        .get(swap.left..)
        .and_then(|r| r.get(..2))
        .ok_or("Invalid regional pair")?;
    if pair
        .iter()
        .any(|id| !snapshot.graph.nodes.iter().any(|n| n.id == *id))
    {
        return Err("Regional pair must contain two real siblings, not borders or dummies".into());
    }
    rank.swap(swap.left, swap.left + 1);
    let crossings_after = plan.crossings();
    for plan in plans.values_mut() {
        plan.fixed = true;
    }
    let graph = &snapshot.graph;
    let index = crate::GraphIndex::new(graph);
    let mut input = snapshot.clone();
    let mut work = crate::SearchCounters::default();
    let mut ordering = crate::placement_order::Memo::default();
    let gaps: BTreeMap<_, _> = snapshot
        .corridor_gaps
        .iter()
        .flat_map(|g| {
            [
                ((g.before.clone(), g.after.clone()), g.extra),
                ((g.after.clone(), g.before.clone()), g.extra),
            ]
        })
        .collect();
    let mut placed_areas = Vec::new();
    let mut propagated_areas = Vec::new();
    let parent = (swap.area != "<root>").then_some(swap.area.as_str());
    let dirty = BTreeSet::from([swap.area.clone()]);
    let size = crate::place(
        parent,
        graph,
        &index,
        &mut input.boxes,
        false,
        &dirty,
        &mut placed_areas,
        &mut input.area_orientations,
        0,
        &snapshot.options,
        &BTreeMap::new(),
        &gaps,
        &mut work,
        &mut plans,
        &mut ordering,
        &mut Vec::new(),
    );
    if let Some(area) = parent {
        let mut child = area.to_string();
        let mut size = size;
        loop {
            let rect = input
                .boxes
                .get(&child)
                .ok_or("Missing regional container box")?;
            // This trial changes order, not the footprint budget. Never shrink
            // an existing frame and re-center every unrelated sibling.
            size = (size.0.max(rect.width), size.1.max(rect.height));
            if size == (rect.width, rect.height) {
                break;
            }
            let parent = index
                .node(&child)
                .ok_or("Unknown regional container")?
                .parent_id
                .as_deref();
            let children: Vec<_> = index
                .children_of(parent)
                .iter()
                .map(|&i| index.nodes[i])
                .collect();
            let mut sizes: BTreeMap<_, _> = children
                .iter()
                .map(|n| {
                    let r = &input.boxes[&n.id];
                    (n.id.clone(), (r.width, r.height))
                })
                .collect();
            sizes.insert(child, size);
            size = crate::push_grown(parent, &children, &sizes, &mut input.boxes, &mut plans);
            propagated_areas.push(parent.unwrap_or("<root>").into());
            let Some(parent) = parent else { break };
            child = parent.into();
        }
    }
    let old_absolute = crate::absolute_all(graph, &snapshot.boxes);
    let absolute = crate::absolute_all(graph, &input.boxes);
    let moved: BTreeSet<_> = absolute
        .iter()
        .filter(|(id, r)| !old_absolute.get(*id).is_some_and(|s| same_rect(r, s)))
        .map(|(id, _)| id.clone())
        .collect();
    let old_keys = crate::layered_plan::port_keys(&old_plans, graph);
    let keys = crate::layered_plan::port_keys(&plans, graph);
    let mut affected = moved.clone();
    for (area, plan) in &plans {
        if area != "<root>" && borders(plan) != borders(&old_plans[area]) {
            affected.insert(area.clone());
        }
    }
    for edge in &graph.edges {
        let Some(target) = &edge.target else { continue };
        if moved.contains(&edge.source) || moved.contains(target) {
            affected.insert(edge.source.clone());
            affected.insert(target.clone());
        }
        for (node, source) in [(&edge.source, true), (target, false)] {
            let key = (edge.id.clone(), source);
            if old_keys.get(&key) != keys.get(&key) {
                affected.insert(node.clone());
            }
        }
    }
    // Ports in every reflowed frame also depend on its newly planned lanes.
    for node in &graph.nodes {
        if placed_areas
            .iter()
            .any(|area| node.parent_id.as_deref().unwrap_or("<root>") == area)
        {
            affected.insert(node.id.clone());
        }
    }
    let ports = crate::ports::PortContext::new(graph)
        .with_flow_sides(true)
        .with_order_keys(keys)
        .with_lanes(crate::layered_plan::port_lanes(&plans, graph, &absolute))
        .resolve_with(
            &input.boxes,
            snapshot.options.port_placement,
            snapshot.options.min_port_spacing,
            &snapshot.options.node_rows,
            snapshot.options.row_port_order,
        );
    for port in &mut input.resolved_ports {
        if affected.contains(&port.node_id) {
            *port = ports
                .iter()
                .find(|p| p.id == port.id)
                .ok_or_else(|| format!("Missing regional port {}", port.id))?
                .clone();
        }
    }
    input.layered_plans = plans
        .iter()
        .map(|(area, p)| (area.clone(), p.to_stored()))
        .collect();
    Ok(Candidate {
        changed_boxes: input
            .boxes
            .iter()
            .filter(|(id, r)| !snapshot.boxes.get(*id).is_some_and(|s| same_rect(r, s)))
            .map(|(id, _)| id.clone())
            .collect(),
        moved_absolute: moved.into_iter().collect(),
        changed_plans: input
            .layered_plans
            .iter()
            .filter(|(id, p)| {
                !snapshot
                    .layered_plans
                    .get(*id)
                    .is_some_and(|s| same_plan(p, s))
            })
            .map(|(id, _)| id.clone())
            .collect(),
        changed_ports: input
            .resolved_ports
            .iter()
            .filter(|p| !snapshot.resolved_ports.iter().any(|s| same_port(p, s)))
            .map(|p| p.id.clone())
            .collect(),
        source: snapshot,
        input,
        placed_areas,
        propagated_areas,
        work,
        crossings_before,
        crossings_after,
    })
}
