//! Provisional shadow routing over rank/element/port order, never over coordinates.
//! Estimates are not clearance proofs and must not reject layout candidates.
use crate::layered_plan::{self, LayeredPlans};
use arqlet_model::Graph;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub(crate) type RankIndex<'a> = BTreeMap<&'a str, BTreeMap<&'a str, usize>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Unsupported {
    Unresolved,
    SelfLoop,
    ExplicitPort,
    MissingArea,
    ContainerEndpoint,
    MixedFlow,
    MissingBorder,
    MissingChain,
    InvalidRanks,
}

/// One logical edge, with sections in construction order. `reversed` never reverses its logical identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgePlan {
    pub edge: String,
    pub source: String,
    pub target: Option<String>,
    pub reversed: bool,
    pub sections: Vec<Section>,
    /// Connections across region boundaries; coordinates must later realize these explicitly.
    pub boundaries: Vec<Boundary>,
    pub unsupported: Option<Unsupported>,
    /// A leg of an edge that crosses the border of an area of another axis (ADR 0017); `edge` is then the leg's own id.
    pub leg: Option<Leg>,
}

/// Which leg of which edge an `EdgePlan` is, and where it begins and ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Leg {
    pub edge: String,
    pub index: usize,
    pub count: usize,
    pub start: LegEnd,
    pub end: LegEnd,
}

/// The id of a leg's `EdgePlan`; it is never the id of a graph edge.
pub fn leg_id(edge: &str, index: usize) -> String {
    format!("{edge}\u{1}leg{index}")
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Boundary {
    pub after_section: usize,
    pub container: String,
    pub border: String,
    pub entering: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub area: String,
    pub channel: usize,
    pub from: String,
    pub to: String,
}

/// An ordinal lane, not pixels. Slots of parallel edges remain distinct.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Lane {
    pub element: usize,
    pub port: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub edge: String,
    pub from: Lane,
    pub to: Lane,
    pub slot: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Channel {
    pub area: String,
    pub rank: usize,
    pub assignments: Vec<Assignment>,
    /// Ordering preferences to avoid a departing segment meeting another arriving segment.
    pub dependencies: Vec<(usize, usize)>,
    /// Preferences broken by deterministic cycle removal, not proven infeasibility.
    pub broken_dependencies: Vec<(usize, usize)>,
    pub order: Vec<usize>,
    /// Conservative abstract interval slots, not a physical width or a lower bound.
    pub tracks: usize,
    pub crossings: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    pub edges: usize,
    pub planned_edges: usize,
    pub unsupported_edges: usize,
    pub sections: usize,
    pub channels: usize,
    pub tracks: usize,
    pub max_tracks: usize,
    pub crossings: usize,
    pub dependencies: usize,
    pub broken_dependencies: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plan {
    pub summary: Summary,
    pub edges: Vec<EdgePlan>,
    pub channels: Vec<Channel>,
    /// Intentionally false: ordinal intervals do not establish geometry/clearance feasibility.
    pub safe_to_prune: bool,
}

fn chain(
    plans: &LayeredPlans,
    ranks: &RankIndex<'_>,
    area: &str,
    from: &str,
    to: &str,
) -> Result<Vec<Section>, Unsupported> {
    let plan = plans.get(area).ok_or(Unsupported::MissingArea)?;
    let dummies = plan
        .chains
        .get(&(from.into(), to.into()))
        .ok_or(Unsupported::MissingChain)?;
    let ranks = ranks.get(area).ok_or(Unsupported::MissingArea)?;
    let elements: Vec<&str> = std::iter::once(from)
        .chain(dummies.iter().map(String::as_str))
        .chain(std::iter::once(to))
        .collect();
    elements
        .windows(2)
        .map(|w| {
            let a = *ranks.get(w[0]).ok_or(Unsupported::InvalidRanks)?;
            let b = *ranks.get(w[1]).ok_or(Unsupported::InvalidRanks)?;
            if a.checked_add(1) != Some(b) {
                return Err(Unsupported::InvalidRanks);
            }
            Ok(Section {
                area: area.into(),
                channel: a,
                from: w[0].into(),
                to: w[1].into(),
            })
        })
        .collect()
}

/// The sections of an edge in one frame: `edge_legs` when the edge has one leg, `MixedFlow` otherwise (the constructor of the
/// other pipelines builds an edge in one frame).
pub(crate) fn edge_sections(
    plans: &LayeredPlans,
    parents: &BTreeMap<&str, &str>,
    ranks: &RankIndex<'_>,
    edge: &arqlet_model::Edge,
    ends: (&str, &str, &str),
) -> Result<(bool, Vec<Section>), Unsupported> {
    let (reversed, mut legs) = edge_legs(plans, parents, ranks, edge, ends)?;
    if legs.len() != 1 {
        return Err(Unsupported::MixedFlow);
    }
    Ok((reversed, legs.remove(0).sections))
}

/// Where a leg of an edge begins or ends (ADR 0017): at the edge's end, or at the border port of a child area whose flow runs
/// across its parent's, which the side node `side` of the child `area` stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LegEnd {
    End,
    Port { area: String, side: String },
}

/// One leg of an edge: sections in one area frame, in construction order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LegSections {
    pub sections: Vec<Section>,
    pub start: LegEnd,
    pub end: LegEnd,
}

/// The legs of an edge, in construction order (ADR 0017). The edge goes up from its first end to the area that holds both ends
/// and down to its last end, through the border nodes of containers of the same axis. At a container whose flow runs across
/// its parent's the leg ends at the container's side node and the next one begins at the container in its parent (going up),
/// or ends at the container and begins at its side node (going down). Every leg keeps one frame.
pub(crate) fn edge_legs(
    plans: &LayeredPlans,
    parents: &BTreeMap<&str, &str>,
    ranks: &RankIndex<'_>,
    edge: &arqlet_model::Edge,
    ends: (&str, &str, &str),
) -> Result<(bool, Vec<LegSections>), Unsupported> {
    let target = edge.target.as_deref().ok_or(Unsupported::Unresolved)?;
    if !parents.contains_key(edge.source.as_str()) || !parents.contains_key(target) {
        return Err(Unsupported::Unresolved);
    }
    if edge.source == target {
        return Err(Unsupported::SelfLoop);
    }
    let (area, source, destination) = ends;
    if source == destination {
        return Err(Unsupported::ContainerEndpoint);
    }
    let outer = &plans[area];
    let reversed = outer
        .reversed_links
        .contains(&(source.to_string(), destination.to_string()));
    let (first, last, from, to) = if reversed {
        (target, edge.source.as_str(), destination, source)
    } else {
        (edge.source.as_str(), target, source, destination)
    };
    let frame = |area: &str| -> Result<(bool, bool), Unsupported> {
        let plan = plans.get(area).ok_or(Unsupported::MissingArea)?;
        Ok((plan.vertical, plan.reversed))
    };
    let mut legs = Vec::new();
    let mut leg = LegSections {
        sections: Vec::new(),
        start: LegEnd::End,
        end: LegEnd::End,
    };
    // The frame of the current leg: that of the first area it has a section in.
    let mut leg_frame: Option<(bool, bool)> = None;
    let extend = |leg: &mut LegSections,
                  leg_frame: &mut Option<(bool, bool)>,
                  area: &str,
                  sections: Vec<Section>|
     -> Result<(), Unsupported> {
        if *leg_frame.get_or_insert(frame(area)?) != frame(area)? {
            return Err(Unsupported::MixedFlow);
        }
        leg.sections.extend(sections);
        Ok(())
    };
    let mut current = first;
    while current != from {
        let inside = *parents.get(current).ok_or(Unsupported::MissingArea)?;
        let plan = plans.get(inside).ok_or(Unsupported::MissingArea)?;
        if let Some(side) = plan.sides.get(&edge.id) {
            // A side node beside its leaf (ADR 0017, amendment A): the inner leg has no sections, it is one straight
            // segment from the leaf's port to the side port.
            if !layered_plan::lateral(plan, side, current) {
                extend(
                    &mut leg,
                    &mut leg_frame,
                    inside,
                    chain(plans, ranks, inside, current, side)?,
                )?;
            }
            leg.end = LegEnd::Port {
                area: inside.into(),
                side: side.clone(),
            };
            let port = leg.end.clone();
            legs.push(std::mem::replace(
                &mut leg,
                LegSections {
                    sections: Vec::new(),
                    start: port,
                    end: LegEnd::End,
                },
            ));
            leg_frame = None;
        } else {
            let border = plan
                .borders
                .get(&edge.id)
                .ok_or(Unsupported::MissingBorder)?;
            extend(
                &mut leg,
                &mut leg_frame,
                inside,
                chain(plans, ranks, inside, current, border)?,
            )?;
        }
        current = inside;
    }
    extend(
        &mut leg,
        &mut leg_frame,
        area,
        chain(plans, ranks, area, from, to)?,
    )?;
    let mut path = Vec::new();
    let mut current = last;
    while current != to {
        let up = *parents.get(current).ok_or(Unsupported::MissingArea)?;
        if up == "<root>" {
            return Err(Unsupported::MissingArea);
        }
        path.push((up, current));
        current = up;
    }
    for (container, child) in path.into_iter().rev() {
        let plan = plans.get(container).ok_or(Unsupported::MissingArea)?;
        if let Some(side) = plan.sides.get(&edge.id) {
            let port = LegEnd::Port {
                area: container.into(),
                side: side.clone(),
            };
            leg.end = port.clone();
            legs.push(std::mem::replace(
                &mut leg,
                LegSections {
                    sections: Vec::new(),
                    start: port,
                    end: LegEnd::End,
                },
            ));
            leg_frame = None;
            if !layered_plan::lateral(plan, side, child) {
                extend(
                    &mut leg,
                    &mut leg_frame,
                    container,
                    chain(plans, ranks, container, side, child)?,
                )?;
            }
        } else {
            let border = plan
                .borders
                .get(&edge.id)
                .ok_or(Unsupported::MissingBorder)?;
            extend(
                &mut leg,
                &mut leg_frame,
                container,
                chain(plans, ranks, container, border, child)?,
            )?;
        }
    }
    legs.push(leg);
    Ok((reversed, legs))
}

/// The crossings of container borders between consecutive sections of one leg.
fn boundaries_of(sections: &[Section]) -> Vec<Boundary> {
    sections
        .windows(2)
        .enumerate()
        .filter_map(|(i, s)| {
            if s[0].area == s[1].area {
                None
            } else if s[0].to == s[1].area {
                Some(Boundary {
                    after_section: i,
                    container: s[1].area.clone(),
                    border: s[1].from.clone(),
                    entering: true,
                })
            } else {
                Some(Boundary {
                    after_section: i,
                    container: s[0].area.clone(),
                    border: s[0].to.clone(),
                    entering: false,
                })
            }
        })
        .collect()
}

fn between(value: &Lane, a: &Lane, b: &Lane) -> bool {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    lo < value && value < hi
}

pub(super) fn channel(area: String, rank: usize, mut assignments: Vec<Assignment>) -> Channel {
    assignments.sort_by(|a, b| a.edge.cmp(&b.edge));
    let mut dependencies = Vec::new();
    let mut crossings = 0;
    for (i, a) in assignments.iter().enumerate() {
        for (j, b) in assignments.iter().enumerate().skip(i + 1) {
            crossings += usize::from(a.from.cmp(&b.from) != a.to.cmp(&b.to));
            let cost = |a: &Assignment, b: &Assignment| {
                usize::from(between(&a.to, &b.from, &b.to))
                    + usize::from(between(&b.from, &a.from, &a.to))
            };
            match cost(a, b).cmp(&cost(b, a)) {
                std::cmp::Ordering::Less => dependencies.push((i, j)),
                std::cmp::Ordering::Greater => dependencies.push((j, i)),
                std::cmp::Ordering::Equal => {}
            }
        }
    }
    let mut incoming = vec![Vec::new(); assignments.len()];
    let mut outgoing = vec![Vec::new(); assignments.len()];
    for &(a, b) in &dependencies {
        incoming[b].push(a);
        outgoing[a].push(b);
    }
    let mut indegree: Vec<_> = incoming.iter().map(Vec::len).collect();
    let mut remaining: BTreeSet<usize> = (0..assignments.len()).collect();
    let mut ready: BTreeSet<usize> = remaining
        .iter()
        .copied()
        .filter(|&i| indegree[i] == 0)
        .collect();
    let mut order = Vec::new();
    let mut broken = Vec::new();
    while !remaining.is_empty() {
        let next = if let Some(next) = ready.pop_first() {
            next
        } else {
            let next = *remaining.first().expect("nonempty channel");
            broken.extend(
                incoming[next]
                    .iter()
                    .filter(|a| remaining.contains(a))
                    .map(|&a| (a, next)),
            );
            next
        };
        remaining.remove(&next);
        for &to in &outgoing[next] {
            if remaining.contains(&to) {
                indegree[to] -= 1;
                if indegree[to] == 0 {
                    ready.insert(to);
                }
            }
        }
        order.push(next);
    }
    let mut tracks = 0;
    for (at, &i) in order.iter().enumerate() {
        let a = &assignments[i];
        let (lo, hi) = if a.from < a.to {
            (&a.from, &a.to)
        } else {
            (&a.to, &a.from)
        };
        let slot = order[..at]
            .iter()
            .filter_map(|&j| {
                let b = &assignments[j];
                let (left, right) = if b.from < b.to {
                    (&b.from, &b.to)
                } else {
                    (&b.to, &b.from)
                };
                (lo <= right && left <= hi).then_some(b.slot + 1)
            })
            .max()
            .unwrap_or(0);
        assignments[i].slot = slot;
        tracks = tracks.max(slot + 1);
    }
    Channel {
        area,
        rank,
        assignments,
        dependencies,
        broken_dependencies: broken,
        order,
        tracks,
        crossings,
    }
}

/// Build a deterministic shadow plan from validated graph structure and ordered ranks.
/// Does not read boxes, dummy rectangles, port/lane offsets, host APIs or routing results.
/// Every logical edge is either planned completely or has an explicit unsupported reason.
pub fn build(graph: &Graph, plans: &LayeredPlans) -> Result<Plan, String> {
    build_with(graph, plans, false, channel)
}

/// `explicit_ports`: edges with an explicit port are planned like any other (single pass); the realization delegates the ones
/// whose port does not lie on the side its flow leaves or enters by (`Delegation::PortSide`). Otherwise they are declared
/// unsupported here.
pub(super) fn build_with(
    graph: &Graph,
    plans: &LayeredPlans,
    explicit_ports: bool,
    mut schedule: impl FnMut(String, usize, Vec<Assignment>) -> Channel,
) -> Result<Plan, String> {
    crate::validate(graph)?;
    for (area, plan) in plans {
        let mut elements = BTreeSet::new();
        for id in plan.ranks.iter().flatten() {
            if !elements.insert(id) {
                return Err(format!(
                    "Abstract routing: duplicate element {id} in area {area}"
                ));
            }
        }
    }
    let parents: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref().unwrap_or("<root>")))
        .collect();
    let explicit: BTreeSet<&str> = graph
        .edge_ports
        .iter()
        .filter(|p| p.source_port.is_some() || p.target_port.is_some())
        .map(|p| p.edge_id.as_str())
        .collect();
    let ranks: RankIndex<'_> = plans
        .iter()
        .map(|(area, plan)| {
            (
                area.as_str(),
                plan.ranks
                    .iter()
                    .enumerate()
                    .flat_map(|(r, ids)| ids.iter().map(move |id| (id.as_str(), r)))
                    .collect(),
            )
        })
        .collect();
    let mut edges = Vec::new();
    for edge in &graph.edges {
        let outcome = if !explicit_ports && explicit.contains(edge.id.as_str()) {
            Err(Unsupported::ExplicitPort)
        } else {
            match edge.target.as_deref() {
                None => Err(Unsupported::Unresolved),
                Some(target) if target == edge.source => Err(Unsupported::SelfLoop),
                Some(target) if !parents.contains_key(target) => Err(Unsupported::Unresolved),
                Some(target) => layered_plan::plan_ends(plans, &parents, &edge.source, target)
                    .ok_or(Unsupported::MissingArea)
                    .and_then(|(area, source, destination)| {
                        edge_legs(
                            plans,
                            &parents,
                            &ranks,
                            edge,
                            (&area, &source, &destination),
                        )
                    }),
            }
        };
        let (reversed, mut legs, unsupported) = match outcome {
            Ok((reversed, legs)) => (reversed, legs, None),
            Err(reason) => (false, Vec::new(), Some(reason)),
        };
        // An edge across the border of an area of another axis is planned as its legs, each like an edge of its own.
        if legs.len() > 1 {
            let count = legs.len();
            for (index, leg) in legs.into_iter().enumerate() {
                edges.push(EdgePlan {
                    edge: leg_id(&edge.id, index),
                    source: edge.source.clone(),
                    target: edge.target.clone(),
                    reversed,
                    boundaries: boundaries_of(&leg.sections),
                    sections: leg.sections,
                    unsupported: None,
                    leg: Some(Leg {
                        edge: edge.id.clone(),
                        index,
                        count,
                        start: leg.start,
                        end: leg.end,
                    }),
                });
            }
            continue;
        }
        let sections = legs.pop().map(|leg| leg.sections).unwrap_or_default();
        let boundaries = boundaries_of(&sections);
        edges.push(EdgePlan {
            edge: edge.id.clone(),
            source: edge.source.clone(),
            target: edge.target.clone(),
            reversed,
            sections,
            boundaries,
            unsupported,
            leg: None,
        });
    }
    edges.sort_by(|a, b| a.edge.cmp(&b.edge));
    // Order incident edges by the neighbour's element order, then by logical edge ID.
    let positions: HashMap<(&str, &str), usize> = plans
        .iter()
        .flat_map(|(area, plan)| {
            plan.ranks.iter().flat_map(move |rank| {
                rank.iter()
                    .enumerate()
                    .map(move |(position, id)| ((area.as_str(), id.as_str()), position))
            })
        })
        .collect();
    // Each group gets its slots independently, so the order of the groups does not matter.
    // Where a leg meets a container at a border port (ADR 0017): the port's place along the container's side, its side node's
    // rank and then its depth among the side nodes of that rank (`prepare_leg` staggers the first deepest). The slots of a
    // container follow these places, so the ordinal order is the drawn order of the ports.
    let port_place = |end: &LegEnd| -> (usize, usize) {
        let LegEnd::Port { area, side } = end else {
            return (0, 0);
        };
        let Some(plan) = plans.get(area) else {
            return (0, 0);
        };
        let kind = if side.starts_with(crate::SIDE_OUT_PREFIX) {
            crate::SIDE_OUT_PREFIX
        } else {
            crate::SIDE_IN_PREFIX
        };
        plan.ranks
            .iter()
            .enumerate()
            .find_map(|(r, rank)| {
                let block: Vec<&String> = rank.iter().filter(|id| id.starts_with(kind)).collect();
                let k = block.iter().position(|id| *id == side)?;
                Some((r, block.len() - 1 - k))
            })
            .unwrap_or((0, 0))
    };
    let mut ports: HashMap<(&str, &str, bool), Vec<(usize, (usize, usize), &str)>> = HashMap::new();
    for edge in &edges {
        let last = edge.sections.len().saturating_sub(1);
        for (i, s) in edge.sections.iter().enumerate() {
            let at = |end: Option<&LegEnd>| end.map_or((0, 0), &port_place);
            let start = edge.leg.as_ref().filter(|_| i == 0).map(|l| &l.start);
            let end = edge.leg.as_ref().filter(|_| i == last).map(|l| &l.end);
            ports.entry((&s.area, &s.from, true)).or_default().push((
                positions[&(s.area.as_str(), s.to.as_str())],
                at(start),
                &edge.edge,
            ));
            ports.entry((&s.area, &s.to, false)).or_default().push((
                positions[&(s.area.as_str(), s.from.as_str())],
                at(end),
                &edge.edge,
            ));
        }
    }
    let mut slots = HashMap::new();
    let real_nodes: HashSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    for ((area, element, outgoing), mut edges) in ports {
        if real_nodes.contains(element) {
            edges.sort();
        } else {
            // A long-edge dummy keeps the same parallel-edge order on both sides.
            edges.sort_by_key(|(_, _, edge)| *edge);
        }
        edges.dedup();
        for (slot, (_, _, edge)) in edges.into_iter().enumerate() {
            slots.insert((area, element, outgoing, edge), slot);
        }
    }
    let mut channels: BTreeMap<(&str, usize), Vec<Assignment>> = BTreeMap::new();
    for edge in &edges {
        for s in &edge.sections {
            let lane = |element: &str, outgoing: bool| Lane {
                element: positions[&(s.area.as_str(), element)],
                port: slots[&(s.area.as_str(), element, outgoing, edge.edge.as_str())],
            };
            channels
                .entry((s.area.as_str(), s.channel))
                .or_default()
                .push(Assignment {
                    edge: edge.edge.clone(),
                    from: lane(&s.from, true),
                    to: lane(&s.to, false),
                    slot: 0,
                });
        }
    }
    let channels: Vec<_> = channels
        .into_iter()
        .map(|((area, rank), a)| schedule(area.to_string(), rank, a))
        .collect();
    let summary = Summary {
        edges: edges.len(),
        planned_edges: edges.iter().filter(|e| e.unsupported.is_none()).count(),
        unsupported_edges: edges.iter().filter(|e| e.unsupported.is_some()).count(),
        sections: edges.iter().map(|e| e.sections.len()).sum(),
        channels: channels.len(),
        tracks: channels.iter().map(|c| c.tracks).sum(),
        max_tracks: channels.iter().map(|c| c.tracks).max().unwrap_or(0),
        crossings: channels.iter().map(|c| c.crossings).sum(),
        dependencies: channels.iter().map(|c| c.dependencies.len()).sum(),
        broken_dependencies: channels.iter().map(|c| c.broken_dependencies.len()).sum(),
    };
    Ok(Plan {
        edges,
        channels,
        summary,
        safe_to_prune: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layered_plan::AreaPlan;
    use serde_json::json;

    fn graph(mut nodes: serde_json::Value, mut edges: serde_json::Value) -> Graph {
        for node in nodes.as_array_mut().unwrap() {
            node["kind"] = json!("node");
            node["label"] = node["id"].clone();
        }
        for edge in edges.as_array_mut().unwrap() {
            edge["kind"] = json!("edge");
        }
        serde_json::from_value(json!({
            "defaultDirection": "lr", "containerDirections": {},
            "defaultStrategy": "none", "containerStrategies": {},
            "nodes": nodes, "edges": edges,
        }))
        .unwrap()
    }

    fn area(ranks: &[&[&str]], links: &[(&str, &str, &[&str])]) -> AreaPlan {
        AreaPlan {
            ranks: ranks
                .iter()
                .map(|rank| rank.iter().map(|s| s.to_string()).collect())
                .collect(),
            chains: links
                .iter()
                .map(|(a, b, d)| {
                    (
                        (a.to_string(), b.to_string()),
                        d.iter().map(|s| s.to_string()).collect(),
                    )
                })
                .collect(),
            ..AreaPlan::default()
        }
    }

    fn parallel() -> (Graph, LayeredPlans) {
        let graph = graph(
            json!([{"id":"a"},{"id":"b"}]),
            json!([
                {"id":"p","source":"a","target":"b"},
                {"id":"q","source":"a","target":"b"},
                {"id":"r","source":"b","target":"a"},
                {"id":"loop","source":"a","target":"a"},
                {"id":"unresolved","source":"a","target":null}
            ]),
        );
        let mut root = area(&[&["a"], &["d"], &["b"]], &[("a", "b", &["d"])]);
        root.reversed_links.insert(("b".into(), "a".into()));
        (graph, BTreeMap::from([("<root>".into(), root)]))
    }

    #[test]
    fn keeps_parallel_ids_logical_cycles_and_explicit_unsupported_edges() {
        let (graph, plans) = parallel();
        let plan = build(&graph, &plans).unwrap();
        assert_eq!(plan.summary.edges, 5);
        assert_eq!(plan.summary.planned_edges, 3);
        assert_eq!(plan.summary.unsupported_edges, 2);
        assert_eq!(plan.summary.sections, 6);
        assert!(!plan.safe_to_prune);
        let reverse = plan.edges.iter().find(|e| e.edge == "r").unwrap();
        assert_eq!(
            (&reverse.source, reverse.target.as_deref()),
            (&"b".to_string(), Some("a"))
        );
        assert!(reverse.reversed);
        assert_eq!(reverse.sections[0].from, "a");
        assert_eq!(
            plan.edges
                .iter()
                .find(|e| e.edge == "loop")
                .unwrap()
                .unsupported,
            Some(Unsupported::SelfLoop)
        );
        assert_eq!(
            plan.edges
                .iter()
                .find(|e| e.edge == "unresolved")
                .unwrap()
                .unsupported,
            Some(Unsupported::Unresolved)
        );
        for channel in &plan.channels {
            assert_eq!(channel.assignments.len(), 3);
            assert_eq!(
                channel
                    .assignments
                    .iter()
                    .map(|a| a.from.port)
                    .collect::<BTreeSet<_>>()
                    .len(),
                3
            );
        }
        for edge in ["p", "q", "r"] {
            let a = plan.channels[0]
                .assignments
                .iter()
                .find(|a| a.edge == edge)
                .unwrap();
            let b = plan.channels[1]
                .assignments
                .iter()
                .find(|a| a.edge == edge)
                .unwrap();
            assert_eq!(a.to, b.from, "dummy must preserve parallel lane order");
        }
        let mut permuted = graph.clone();
        permuted.edges.reverse();
        assert_eq!(plan, build(&permuted, &plans).unwrap());
    }

    fn nested() -> (Graph, LayeredPlans) {
        let graph = graph(
            json!([
                {"id":"c"},{"id":"a","parentId":"c"},
                {"id":"d"},{"id":"b","parentId":"d"}
            ]),
            json!([{"id":"e","source":"a","target":"b"}]),
        );
        let root = area(&[&["c"], &["d"]], &[("c", "d", &[])]);
        let mut c = area(&[&["a"], &["exit"]], &[("a", "exit", &[])]);
        c.borders.insert("e".into(), "exit".into());
        let mut d = area(&[&["enter"], &["b"]], &[("enter", "b", &[])]);
        d.borders.insert("e".into(), "enter".into());
        (
            graph,
            BTreeMap::from([("<root>".into(), root), ("c".into(), c), ("d".into(), d)]),
        )
    }

    #[test]
    fn plans_nested_borders_without_geometry_and_reports_mixed_axes() {
        let (graph, mut plans) = nested();
        let plan = build(&graph, &plans).unwrap();
        assert_eq!(plan.summary.planned_edges, 1);
        assert_eq!(
            plan.edges[0]
                .sections
                .iter()
                .map(|s| s.area.as_str())
                .collect::<Vec<_>>(),
            vec!["c", "<root>", "d"]
        );
        assert_eq!(
            plan.edges[0].boundaries,
            vec![
                Boundary {
                    after_section: 0,
                    container: "c".into(),
                    border: "exit".into(),
                    entering: false
                },
                Boundary {
                    after_section: 1,
                    container: "d".into(),
                    border: "enter".into(),
                    entering: true
                },
            ]
        );
        for p in plans.values_mut() {
            p.dummies.insert(
                "unused".into(),
                arqlet_model::Rect {
                    x: f64::NAN,
                    y: -10000.,
                    width: 90000.,
                    height: 0.,
                },
            );
            p.port_offsets.insert(("e".into(), true), f64::INFINITY);
            p.lane_offsets.insert(("unused".into(), "e".into()), -0.);
            p.fixed = true;
        }
        assert_eq!(plan, build(&graph, &plans).unwrap());
        for p in plans.values_mut() {
            p.reversed = true;
        }
        assert_eq!(
            plan,
            build(&graph, &plans).unwrap(),
            "mirroring does not change ordinal topology"
        );
        plans.get_mut("d").unwrap().vertical = true;
        let mixed = build(&graph, &plans).unwrap();
        assert_eq!(mixed.edges[0].unsupported, Some(Unsupported::MixedFlow));
        assert!(
            mixed.edges[0].sections.is_empty(),
            "never publish a partial edge"
        );
        assert!(mixed.channels.is_empty());
    }

    #[test]
    fn missing_borders_chains_and_bad_ranks_are_explicit() {
        let (graph, mut plans) = nested();
        plans.get_mut("d").unwrap().borders.clear();
        assert_eq!(
            build(&graph, &plans).unwrap().edges[0].unsupported,
            Some(Unsupported::MissingBorder)
        );
        let (graph, mut plans) = parallel();
        plans.get_mut("<root>").unwrap().chains.clear();
        let plan = build(&graph, &plans).unwrap();
        assert_eq!(plan.summary.planned_edges, 0);
        assert_eq!(
            plan.edges
                .iter()
                .find(|e| e.edge == "p")
                .unwrap()
                .unsupported,
            Some(Unsupported::MissingChain)
        );
        let (graph, mut plans) = parallel();
        plans.get_mut("<root>").unwrap().ranks.swap(0, 1);
        assert_eq!(
            build(&graph, &plans)
                .unwrap()
                .edges
                .iter()
                .find(|e| e.edge == "p")
                .unwrap()
                .unsupported,
            Some(Unsupported::InvalidRanks)
        );
        plans
            .get_mut("<root>")
            .unwrap()
            .ranks
            .push(vec!["a".into()]);
        assert!(build(&graph, &plans)
            .unwrap_err()
            .contains("duplicate element"));
    }

    #[test]
    fn flattened_members_do_not_need_nonexistent_inner_plans() {
        let (graph, _) = nested();
        let plans =
            BTreeMap::from([("<root>".into(), area(&[&["a"], &["b"]], &[("a", "b", &[])]))]);
        let plan = build(&graph, &plans).unwrap();
        assert_eq!(plan.summary.planned_edges, 1);
        assert_eq!(plan.edges[0].sections.len(), 1);
        assert!(plan.edges[0].boundaries.is_empty());
    }

    #[test]
    fn nested_cycle_reversal_and_multiple_boundary_levels_keep_the_edge() {
        let (mut graph, mut plans) = nested();
        graph.nodes.push(arqlet_model::Node {
            id: "inner".into(),
            kind: "container".into(),
            label: "inner".into(),
            parent_id: Some("c".into()),
            shape: Default::default(),
        });
        graph
            .nodes
            .iter_mut()
            .find(|n| n.id == "a")
            .unwrap()
            .parent_id = Some("inner".into());
        let mut inner = area(&[&["a"], &["innerExit"]], &[("a", "innerExit", &[])]);
        inner.borders.insert("e".into(), "innerExit".into());
        plans.insert("inner".into(), inner);
        let c = plans.get_mut("c").unwrap();
        c.ranks[0] = vec!["inner".into()];
        c.chains = BTreeMap::from([(("inner".into(), "exit".into()), Vec::new())]);
        let forward = build(&graph, &plans).unwrap();
        assert_eq!(forward.summary.planned_edges, 1);
        assert_eq!(forward.edges[0].boundaries.len(), 3);
        graph.edges[0].source = "b".into();
        graph.edges[0].target = Some("a".into());
        plans
            .get_mut("<root>")
            .unwrap()
            .reversed_links
            .insert(("d".into(), "c".into()));
        for vertical in [true, false] {
            for reversed in [true, false] {
                for p in plans.values_mut() {
                    p.vertical = vertical;
                    p.reversed = reversed;
                }
                let plan = build(&graph, &plans).unwrap();
                assert!(plan.edges[0].reversed);
                assert_eq!(plan.edges[0].source, "b");
                assert_eq!(plan.edges[0].target.as_deref(), Some("a"));
                assert_eq!(plan.edges[0].sections, forward.edges[0].sections);
                assert_eq!(plan.edges[0].boundaries, forward.edges[0].boundaries);
            }
        }
    }

    #[test]
    fn honors_explicit_ports_by_declaring_them_unsupported_not_overriding_them() {
        let (mut graph, plans) = parallel();
        graph.ports = serde_json::from_value(json!([{
            "id": "out", "nodeId": "a", "side": "right", "offset": 0.5
        }]))
        .unwrap();
        graph.edge_ports = serde_json::from_value(json!([{
            "edgeId": "p", "sourcePort": "out"
        }]))
        .unwrap();
        let plan = build(&graph, &plans).unwrap();
        assert_eq!(
            plan.edges
                .iter()
                .find(|e| e.edge == "p")
                .unwrap()
                .unsupported,
            Some(Unsupported::ExplicitPort)
        );
        assert_eq!(
            plan.summary.edges,
            plan.summary.planned_edges + plan.summary.unsupported_edges
        );
    }

    #[test]
    fn interval_slots_are_disjoint_and_all_unbroken_preferences_hold() {
        for count in 0..30 {
            let assignments = (0..count)
                .map(|i| Assignment {
                    edge: format!("e{i:02}"),
                    from: Lane {
                        element: (i * 13 + 3) % 17,
                        port: i,
                    },
                    to: Lane {
                        element: (i * 7 + 1) % 19,
                        port: i,
                    },
                    slot: 0,
                })
                .collect();
            let plan = channel("<root>".into(), 0, assignments);
            assert_eq!(
                plan.order.iter().copied().collect::<BTreeSet<_>>().len(),
                count
            );
            let positions: BTreeMap<_, _> = plan
                .order
                .iter()
                .enumerate()
                .map(|(i, &v)| (v, i))
                .collect();
            for &(a, b) in &plan.dependencies {
                assert!(
                    positions[&a] < positions[&b] || plan.broken_dependencies.contains(&(a, b))
                );
            }
            for (i, a) in plan.assignments.iter().enumerate() {
                for b in &plan.assignments[i + 1..] {
                    if a.slot == b.slot {
                        assert!(
                            a.from.clone().max(a.to.clone()) < b.from.clone().min(b.to.clone())
                                || b.from.clone().max(b.to.clone())
                                    < a.from.clone().min(a.to.clone())
                        );
                    }
                }
            }
        }
    }
}
