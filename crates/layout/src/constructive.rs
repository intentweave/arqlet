//! Constructive routes for layered areas (prototype, plan section 4a, steps 1 and 2).
//!
//! An edge whose endpoints are leaves of one area with a fixed left-to-right or top-to-bottom flow gets an orthogonal
//! route built from its two ports. The route runs monotonically along the flow: it leaves the source horizontally,
//! changes lane once in each corridor between neighbouring ranks (a section of a long edge), and enters the target
//! horizontally. The corridors are the free gaps between the boxes, and the lanes are the free gaps across the flow. A
//! dynamic programme over these corridors picks the cheapest such path, and the path is kept apart from the segments
//! already assigned. Every other edge is delegated to the existing router, and the reason is recorded. The construction
//! is internal to this prototype: it changes no snapshot and no default output.

use crate::layered_plan::LayeredPlans;
use crate::ConstructionCounters;
pub mod abstract_realization;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use arqlet_model::{
    DirectionConstraint, Graph, Orientation, Point, PortSide, Rect, ResolvedPort, Route,
};

const EPS: f64 = 1e-6;
const TRACK_SLOT_INDEX_THRESHOLD: usize = 512;
/// The cost of one change of lane, in layout units. It makes the path prefer few bends to a shorter one with more bends.
const BEND: f64 = 24.;
/// The cost of crossing a segment already built. It outweighs any length and bend, so a search avoids a crossing first, as
/// the general router does.
const CROSS: f64 = 1e6;

/// Why an edge is not constructed. The edge then keeps the route of the existing router.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delegation {
    /// A self-loop has its own handling.
    SelfLoop,
    /// A port or a box of an endpoint is missing from the drawing.
    Unresolved,
    /// An endpoint is a container: its ports are not on a leaf's border. (Not used since containers are endpoints like
    /// leaves; kept so that recorded reasons stay readable.)
    ContainerEndpoint,
    /// The route would pass areas whose flows differ (or one end lies inside the other), so it is not one monotone path.
    CrossesArea,
    /// The area's flow is free, reversed, or not a line (`auto`, `rl`, `bt`, allowed sets).
    NoFixedFlow,
    /// The target does not lie after the source in the flow, or both lie in the same rank.
    NotForward,
    /// An endpoint's port is not on the side the flow leaves or enters through (the automatic ports chose another side).
    PortSide,
    /// No monotone path through the corridors keeps the margin from the boxes: the boxes leave no way at all.
    NoFreeTrack,
    /// A monotone path between the boxes exists, but every one comes within the track spacing of a route already built:
    /// the corridors have too few tracks for the edges that use them.
    TrackTaken,
}

/// The routes of the constructed edges, and the edges that stay with the existing router.
#[derive(Debug, Default)]
pub struct Construction {
    pub work: ConstructionCounters,
    pub routes: Vec<Route>,
    pub delegated: Vec<(String, Delegation)>,
    /// The changes of lane the constructed routes make, one per corridor they cross (summed over the routes).
    pub lane_changes: usize,
    /// The port slots the construction gave delegated edges: (edge, new source end, new target end), an end only when it
    /// moved. The existing router draws such an edge from these ends, so that no two edges share a slot.
    pub moved_ends: Vec<(String, Option<Point>, Option<Point>)>,
    /// How many edges have a route from the plan's sections (step 5), and how many of them were built from it.
    pub sectioned: usize,
    pub from_sections: usize,
    /// Experimental (consistent layered pipeline): the channels too narrow for their tracks, as (area, rank before the
    /// channel, missing width along the flow). Widening them moves only ranks along the flow, so the tracks stay the same.
    pub narrow_channels: Vec<(String, usize, f64)>,
    /// Provisional: whether the construction started from a probe's preparation (`construct_planned_from`).
    pub reused_preparation: bool,
}

/// The spacing a construction keeps: `margin` is the distance a track keeps from every box it passes (the required
/// clearance plus one track, as the routing parameters give it), and `track` the distance between two tracks.
#[derive(Clone, Copy, Debug)]
pub struct Spacing {
    pub margin: f64,
    pub track: f64,
    /// Experimental (consistent layered pipeline, step 2; with `hierarchicalOrdering`): an edge with an end inside a
    /// container of the same axis is built through the containers' border nodes.
    pub through_borders: bool,
    /// Only count: the construction stops after the routes from the sections have their tracks, and reports the channels
    /// too narrow for them (`Construction::narrow_channels`); nothing is built or searched.
    pub probe: bool,
    /// Provisional (option `portSides`, single pass): an end whose port lies across the flow or against it gets a lead around
    /// its node (`Prepared::lead`).
    pub port_sides: bool,
}

/// Constructs the routes of the edges in a layered area. `boxes` are absolute, and `ports` are the resolved ports of the
/// drawing. Edges are processed in input order, so the result is deterministic.
pub fn construct(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    spacing: Spacing,
    orientations: &BTreeMap<String, Orientation>,
) -> Construction {
    construct_ordered(graph, boxes, ports, spacing, orientations, EdgeOrder::Input)
}

/// The order in which the edges take their tracks. The tracks of an earlier edge are not given to a later one, so the
/// order decides which edges find a free track when the corridors are tight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeOrder {
    /// The order of the graph's edges.
    Input,
    /// Short edges first: the distance between the two ports.
    ShortFirst,
    /// The source ports in the order of their sides and their position along the side, so that tracks leaving one side
    /// follow the port order.
    SourcePort,
}

/// `construct` with the edges taken in the given order. Routes are returned in the order they were built.
pub fn construct_ordered(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    spacing: Spacing,
    orientations: &BTreeMap<String, Orientation>,
    order: EdgeOrder,
) -> Construction {
    construct_with(
        graph,
        boxes,
        ports,
        spacing,
        orientations,
        order,
        None,
        None,
        &[],
        None,
    )
}

/// Provisional (plan section 4a, step 5): `construct` with the plans of the layered areas. An edge whose ends are children
/// of one planned area is built from its sections: along the lane of its start port, through the places of its dummies, to
/// the lane of its end port, changing lane only in the channels between ranks, on the track of its place in the channel's
/// order. Such routes are built first; where one meets an obstacle or a route already built, the edge is searched as before.
pub fn construct_planned(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    spacing: Spacing,
    orientations: &BTreeMap<String, Orientation>,
    plans: &LayeredPlans,
) -> Construction {
    construct_planned_from(graph, boxes, ports, spacing, orientations, plans, None)
}

/// Provisional (consistent layered pipeline, reuse within a layout pass): the first part of `construct_planned`, as the
/// probe computes it (every edge checked and its ends prepared, then the routes from the plans' sections and the channels
/// too narrow for them), with the inputs it was computed from. A construction on the same boxes, ports, spacing and plans
/// starts from it instead of computing it again; the result is the same.
pub struct Preparation {
    boxes: BTreeMap<String, Rect>,
    ports: Vec<ResolvedPort>,
    spacing: Spacing,
    prepared: Vec<(usize, Result<Prepared, Delegation>)>,
    sections: BTreeMap<usize, Vec<(f64, f64)>>,
    cross_sections: BTreeSet<usize>,
    /// The channels too narrow for their tracks (`Construction::narrow_channels`).
    pub narrow_channels: Vec<(String, usize, f64)>,
    /// Work already performed by this probe; consuming the preparation does not count it a second time.
    pub work: ConstructionCounters,
}

impl Preparation {
    /// Whether a construction on these inputs computes this preparation: the same boxes, ports and spacing, bit for bit.
    /// The plans and the orientations are the caller's to keep the same.
    fn fits(
        &self,
        boxes: &BTreeMap<String, Rect>,
        ports: &[ResolvedPort],
        spacing: Spacing,
    ) -> bool {
        let rect = |a: &Rect, b: &Rect| {
            [a.x, a.y, a.width, a.height]
                .iter()
                .zip([b.x, b.y, b.width, b.height])
                .all(|(x, y)| x.to_bits() == y.to_bits())
        };
        let point =
            |a: &Point, b: &Point| a.x.to_bits() == b.x.to_bits() && a.y.to_bits() == b.y.to_bits();
        let port = |a: &ResolvedPort, b: &ResolvedPort| {
            a.id == b.id
                && a.node_id == b.node_id
                && a.edge_id == b.edge_id
                && a.endpoint == b.endpoint
                && a.side == b.side
                && point(&a.position, &b.position)
                && a.explicit == b.explicit
        };
        self.spacing.margin.to_bits() == spacing.margin.to_bits()
            && self.spacing.track.to_bits() == spacing.track.to_bits()
            && self.spacing.through_borders == spacing.through_borders
            && self.boxes.len() == boxes.len()
            && self
                .boxes
                .iter()
                .zip(boxes)
                .all(|((i, a), (j, b))| i == j && rect(a, b))
            && self.ports.len() == ports.len()
            && self.ports.iter().zip(ports).all(|(a, b)| port(a, b))
    }
}

/// The probe of `construct_planned` (`Spacing::probe`): only its preparation, which reports the channels too narrow for the
/// tracks of the routes from the sections. Provisional.
pub fn prepare_planned(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    spacing: Spacing,
    orientations: &BTreeMap<String, Orientation>,
    plans: &LayeredPlans,
) -> Preparation {
    let work = RefCell::new(ConstructionCounters::default());
    let parents = parents_of(graph);
    let containers = containers_of(graph);
    let indices: Vec<usize> = (0..graph.edges.len()).collect();
    let (prepared, (sections, cross_sections, narrow_channels)) = prepare_all(
        graph,
        boxes,
        ports,
        spacing,
        orientations,
        indices,
        Some(plans),
        None,
        (&parents, &containers),
        &work,
    );
    Preparation {
        boxes: boxes.clone(),
        ports: ports.to_vec(),
        spacing,
        prepared,
        sections,
        cross_sections,
        narrow_channels,
        work: work.into_inner(),
    }
}

/// `construct_planned`, started from `preparation` when it fits these inputs (`Preparation::fits`); otherwise computed in
/// full. The plans and orientations must be those the preparation was computed with. Provisional.
pub fn construct_planned_from(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    spacing: Spacing,
    orientations: &BTreeMap<String, Orientation>,
    plans: &LayeredPlans,
    preparation: Option<Preparation>,
) -> Construction {
    construct_with(
        graph,
        boxes,
        ports,
        spacing,
        orientations,
        EdgeOrder::Input,
        Some(plans),
        None,
        &[],
        preparation.filter(|p| p.fits(boxes, ports, spacing)),
    )
}

/// Provisional (plan section 4a, reuse in edits): `construct_planned` for the edges `only`, around the routes `fixed`, which
/// count as built (an edit keeps them). The port slots and the corridor order are decided among the edges `only`.
#[allow(clippy::too_many_arguments)]
pub fn construct_planned_subset(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    spacing: Spacing,
    orientations: &BTreeMap<String, Orientation>,
    plans: &LayeredPlans,
    only: &BTreeSet<String>,
    fixed: &[Route],
) -> Construction {
    construct_with(
        graph,
        boxes,
        ports,
        spacing,
        orientations,
        EdgeOrder::Input,
        Some(plans),
        Some(only),
        fixed,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn construct_with(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    spacing: Spacing,
    orientations: &BTreeMap<String, Orientation>,
    order: EdgeOrder,
    plans: Option<&LayeredPlans>,
    only: Option<&BTreeSet<String>>,
    fixed: &[Route],
    preparation: Option<Preparation>,
) -> Construction {
    let work = RefCell::new(ConstructionCounters::default());
    let parents = parents_of(graph);
    let containers = containers_of(graph);
    let assigned: Vec<Segment> = fixed.iter().flat_map(|r| segments_of(&r.points)).collect();
    let mut result = Construction::default();
    let mut indices: Vec<usize> = (0..graph.edges.len()).collect();
    let source_port = |id: &str| {
        ports
            .iter()
            .find(|p| p.edge_id == id && p.endpoint == "source")
            .map(|p| (side_rank(p.side), p.position.x, p.position.y))
            .unwrap_or((u8::MAX, f64::MAX, f64::MAX))
    };
    match order {
        EdgeOrder::Input => {}
        EdgeOrder::ShortFirst => {
            let span = |i: usize| {
                let e = &graph.edges[i];
                let ends = |endpoint: &str| {
                    ports
                        .iter()
                        .find(|p| p.edge_id == e.id && p.endpoint == endpoint)
                        .map(|p| (p.position.x, p.position.y))
                };
                match (
                    ends("source"),
                    e.target.as_deref().and_then(|_| ends("target")),
                ) {
                    (Some(a), Some(b)) => ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt(),
                    _ => f64::MAX,
                }
            };
            indices.sort_by(|&a, &b| span(a).total_cmp(&span(b)).then(a.cmp(&b)));
        }
        EdgeOrder::SourcePort => {
            indices.sort_by(|&a, &b| {
                let (ka, kb) = (
                    source_port(&graph.edges[a].id),
                    source_port(&graph.edges[b].id),
                );
                ka.0.cmp(&kb.0)
                    .then(ka.1.total_cmp(&kb.1))
                    .then(ka.2.total_cmp(&kb.2))
                    .then(a.cmp(&b))
            });
        }
    }
    // Every edge is checked, then its way is planned over the gaps' centres (`prepare_all`), unless a probe on the same
    // inputs prepared it already.
    result.reused_preparation = preparation.is_some();
    let (prepared, (sections, cross_sections, narrow)) = match preparation {
        Some(p) => (
            p.prepared,
            (p.sections, p.cross_sections, p.narrow_channels),
        ),
        None => prepare_all(
            graph,
            boxes,
            ports,
            spacing,
            orientations,
            indices,
            plans,
            only,
            (&parents, &containers),
            &work,
        ),
    };
    result.narrow_channels = narrow;
    if spacing.probe {
        result.work = work.into_inner();
        return result;
    }
    // One construction with the port slots of the `eligible` edges reordered (see below). An edge whose slot was reordered must
    // be constructed: the general routing would draw it from its own slot, which another edge may hold by then.
    // The ports follow the ways: on each node side, the constructed edges keep the port slots they have, but take them in the
    // order of the lane they turn to first (where the route starts) or come from last (where it ends), as the first planned
    // ways give it. The ways of the edges whose port moved are then planned again.
    // With routes through border nodes, an edge built from its sections takes no part in the ways: its port follows the
    // plan and its route is built before every searched one, so its way is not searched.
    let from_plan = |slot: usize| {
        spacing.through_borders && sections.contains_key(&slot) && !cross_sections.contains(&slot)
    };
    let first_ways: Vec<Vec<((f64, f64), f64, f64)>> = prepared
        .iter()
        .enumerate()
        .map(|(slot, (_, outcome))| {
            if from_plan(slot) {
                return Vec::new();
            }
            outcome
                .as_ref()
                .map(|p| lane_changes(p, spacing.track, &work))
                .unwrap_or_default()
        })
        .collect();
    // The outcome of every edge, and the edges whose port slot moved.
    type Attempt = (
        Vec<Result<(Route, usize), Delegation>>,
        BTreeSet<usize>,
        Vec<Option<(Point, Point)>>,
        usize,
    );
    let attempt = |eligible: &BTreeSet<usize>| -> Attempt {
        let mut prepared: Vec<Result<Prepared, Delegation>> = prepared
            .iter()
            .map(|(_, outcome)| outcome.clone())
            .collect();
        let mut moved = BTreeSet::new();
        // By node, end (start or end of the route) and flow axis: the axis fixes the side (the flow's out or in side), so
        // only ports on one side of a node trade places, never a port on a top side with one on a left side.
        let mut sides: BTreeMap<(String, bool, bool), Vec<(usize, f64)>> = BTreeMap::new();
        for (slot, outcome) in prepared.iter().enumerate() {
            if !eligible.contains(&slot) {
                continue;
            }
            if let Ok(p) = outcome {
                let changes = &first_ways[slot];
                let leaving = changes.first().map_or(p.to.1, |c| c.2);
                let entering = changes.last().map_or(p.from.1, |c| c.1);
                sides
                    .entry((p.from_node.clone(), true, p.vertical))
                    .or_default()
                    .push((slot, leaving));
                sides
                    .entry((p.to_node.clone(), false, p.vertical))
                    .or_default()
                    .push((slot, entering));
            }
        }
        for ((_, at_start, _), mut members) in sides {
            if members.len() < 2 {
                continue;
            }
            let mut places: Vec<f64> = members
                .iter()
                .filter_map(|(slot, _)| prepared[*slot].as_ref().ok())
                .map(|p| if at_start { p.from.1 } else { p.to.1 })
                .collect();
            places.sort_by(f64::total_cmp);
            members.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
            for ((slot, _), place) in members.iter().zip(places) {
                if let Ok(p) = prepared[*slot].as_mut() {
                    let end = if at_start { &mut p.from } else { &mut p.to };
                    if (end.1 - place).abs() < EPS {
                        continue;
                    }
                    end.1 = place;
                    moved.insert(*slot);
                    if at_start {
                        p.start = to_point(p.vertical, p.from);
                    } else {
                        p.end = to_point(p.vertical, p.to);
                    }
                }
            }
        }
        let mut corridors: BTreeMap<(bool, (i64, i64)), (Vec<(usize, f64, f64)>, (f64, f64))> =
            BTreeMap::new();
        for (slot, outcome) in prepared.iter().enumerate() {
            if let Ok(p) = outcome {
                let way = if moved.contains(&slot) {
                    lane_changes(p, spacing.track, &work)
                } else {
                    first_ways[slot].clone()
                };
                for (gap, before, after) in way {
                    let entry = corridors
                        .entry((p.vertical, gap_key(gap)))
                        .or_insert_with(|| (Vec::new(), gap));
                    entry.0.push((slot, before, after));
                }
            }
        }
        let mut desired: BTreeMap<usize, Desired> = BTreeMap::new();
        for ((_, key), (changes, gap)) in &corridors {
            let spans: Vec<(f64, f64)> = changes.iter().map(|c| (c.1, c.2)).collect();
            let order = corridor_order(&spans);
            let (slots, count) = track_slots(&spans, &order, spacing.track);
            let tracks = ordered_tracks(*gap, count, spacing.track);
            for &i in &order {
                desired
                    .entry(changes[i].0)
                    .or_default()
                    .insert(*key, tracks[slots[i]]);
            }
        }
        let unbound = Desired::new();
        let mut assigned = assigned.clone();
        // The edges built from their sections come first; the others are searched around them.
        let mut built: Vec<Option<Result<(Route, usize), Delegation>>> = vec![None; prepared.len()];
        let mut from_sections = 0;
        for (&slot, corners) in sections
            .iter()
            .filter(|(slot, _)| !cross_sections.contains(slot))
        {
            if let Ok(p) = &prepared[slot] {
                if let Some(route) = build_planned(p, corners, &mut assigned, spacing.track, &work)
                {
                    built[slot] = Some(Ok(route));
                    from_sections += 1;
                }
            }
        }
        let mut outcomes: Vec<Result<(Route, usize), Delegation>> = prepared
            .iter()
            .enumerate()
            .map(|(slot, outcome)| match (built[slot].take(), outcome) {
                (Some(route), _) => route,
                (None, _) if cross_sections.contains(&slot) => Err(Delegation::NoFreeTrack),
                (None, Ok(p)) => build_edge(
                    p,
                    &mut assigned,
                    spacing.track,
                    desired.get(&slot).unwrap_or(&unbound),
                    &work,
                ),
                (None, Err(reason)) => Err(*reason),
            })
            .collect();
        // The edges out of containers of the other axis come last: the better of their route from the sections and their
        // searched route, by the crossings with all routes built (their border crossings are not yet ordered with the
        // containers' own edges, plan section 4a, step 6).
        let crossings_with = |route: &Route, built: &[Segment]| -> usize {
            segments_of(&route.points)
                .iter()
                .map(|s| built.iter().filter(|b| crosses(s, b)).count())
                .sum()
        };
        for &slot in &cross_sections {
            let Ok(p) = &prepared[slot] else { continue };
            let before = assigned.len();
            let searched = build_edge(
                p,
                &mut assigned,
                spacing.track,
                desired.get(&slot).unwrap_or(&unbound),
                &work,
            );
            assigned.truncate(before);
            // A port the reordering moved no longer starts the route from the sections.
            let planned = if moved.contains(&slot) {
                None
            } else {
                build_planned(p, &sections[&slot], &mut assigned, spacing.track, &work)
            };
            assigned.truncate(before);
            let choice = match (planned, searched) {
                (Some(a), Ok(b)) => {
                    if crossings_with(&a.0, &assigned) <= crossings_with(&b.0, &assigned) {
                        from_sections += 1;
                        Ok(a)
                    } else {
                        Ok(b)
                    }
                }
                (Some(a), Err(_)) => {
                    from_sections += 1;
                    Ok(a)
                }
                (None, searched) => searched,
            };
            if let Ok((route, _)) = &choice {
                assigned.extend(segments_of(&route.points));
            }
            outcomes[slot] = choice;
        }
        // The logical ends of every prepared edge after the reordering (source first, also for a reversed edge).
        let ends = prepared
            .iter()
            .map(|outcome| {
                outcome.as_ref().ok().map(|p| {
                    if p.reversed {
                        (p.end.clone(), p.start.clone())
                    } else {
                        (p.start.clone(), p.end.clone())
                    }
                })
            })
            .collect();
        (outcomes, moved, ends, from_sections)
    };
    // One construction with every edge's port slot in the order of the ways. A delegated edge whose slot moved keeps the new
    // slot: the existing router draws it from there (`moved_ends`).
    let eligible: BTreeSet<usize> = (0..prepared.len())
        .filter(|&slot| {
            prepared[slot].1.is_ok()
                && (!sections.contains_key(&slot) || cross_sections.contains(&slot))
        })
        .collect();
    let (outcomes, moved, ends, from_sections) = attempt(&eligible);
    result.sectioned = sections.len();
    result.from_sections = from_sections;
    // The self-loops of a node are nested around the corner between their two port sides, clear of the routes built.
    let built: Vec<Segment> = outcomes
        .iter()
        .flatten()
        .flat_map(|(route, _)| segments_of(&route.points))
        .chain(fixed.iter().flat_map(|r| segments_of(&r.points)))
        .collect();
    let loops: Vec<&str> = prepared
        .iter()
        .filter(|(_, outcome)| matches!(outcome, Err(Delegation::SelfLoop)))
        .map(|(index, _)| graph.edges[*index].id.as_str())
        .collect();
    let mut nested = nest_self_loops(boxes, ports, &parents, spacing, &loops, &built);
    for (slot, ((index, original), outcome)) in prepared.iter().zip(outcomes).enumerate() {
        let edge = &graph.edges[*index];
        if outcome.is_err() && moved.contains(&slot) {
            if let (Ok(p), Some((source, target))) = (original, &ends[slot]) {
                let (old_source, old_target) = if p.reversed {
                    (&p.end, &p.start)
                } else {
                    (&p.start, &p.end)
                };
                let differs =
                    |a: &Point, b: &Point| (a.x - b.x).abs() > EPS || (a.y - b.y).abs() > EPS;
                result.moved_ends.push((
                    edge.id.clone(),
                    differs(source, old_source).then(|| source.clone()),
                    differs(target, old_target).then(|| target.clone()),
                ));
            }
        }
        let outcome = match nested.remove(edge.id.as_str()) {
            Some(route) => Ok((route, 0)),
            None => outcome,
        };
        match outcome {
            Ok((route, lane_changes)) => {
                result.lane_changes += lane_changes;
                result.routes.push(route);
            }
            Err(reason) => result.delegated.push((edge.id.clone(), reason)),
        }
    }
    result.work = work.into_inner();
    result
}

/// The outward normal of a node side.
fn normal(side: PortSide) -> (f64, f64) {
    match side {
        PortSide::Left => (-1., 0.),
        PortSide::Right => (1., 0.),
        PortSide::Top => (0., -1.),
        PortSide::Bottom => (0., 1.),
    }
}

/// The routes of self-loops, nested: the loops of one node whose ports lie on the same two adjacent sides run around the
/// corner between them. The innermost loop takes the two ports nearest that corner and passes at the margin; every further
/// loop takes the next two ports and passes one track further out, so the loops of a node never cross. A node whose loop
/// would meet another box or a route already built keeps its loops for the general routing.
fn nest_self_loops(
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    parents: &BTreeMap<&str, Option<&str>>,
    spacing: Spacing,
    loops: &[&str],
    built: &[Segment],
) -> BTreeMap<String, Route> {
    let port = |edge: &str, endpoint: &str| {
        ports
            .iter()
            .find(|p| p.edge_id == edge && p.endpoint == endpoint)
    };
    // The loops of each node by their pair of sides.
    let mut groups: BTreeMap<(String, u8, u8), Vec<&str>> = BTreeMap::new();
    for &id in loops {
        let (Some(a), Some(b)) = (port(id, "source"), port(id, "target")) else {
            continue;
        };
        let (na, nb) = (normal(a.side), normal(b.side));
        // Adjacent sides only: their normals are perpendicular.
        if na.0 * nb.0 + na.1 * nb.1 != 0. {
            continue;
        }
        groups
            .entry((a.node_id.clone(), a.side as u8, b.side as u8))
            .or_default()
            .push(id);
    }
    let mut out = BTreeMap::new();
    for ((node, _, _), ids) in groups {
        let Some(r) = boxes.get(&node) else {
            continue;
        };
        let (a_side, b_side) = (
            port(ids[0], "source").unwrap().side,
            port(ids[0], "target").unwrap().side,
        );
        // The lines of the two sides, and the corner between them.
        let line = |side: PortSide| match side {
            PortSide::Left => r.x,
            PortSide::Right => r.x + r.width,
            PortSide::Top => r.y,
            PortSide::Bottom => r.y + r.height,
        };
        let horizontal_normal = |side: PortSide| matches!(side, PortSide::Left | PortSide::Right);
        let (corner_x, corner_y) = if horizontal_normal(a_side) {
            (line(a_side), line(b_side))
        } else {
            (line(b_side), line(a_side))
        };
        // Ports along a side, nearest the corner first.
        let distance = |p: &Point, side: PortSide| {
            if horizontal_normal(side) {
                (p.y - corner_y).abs()
            } else {
                (p.x - corner_x).abs()
            }
        };
        let mut starts: Vec<Point> = ids
            .iter()
            .map(|id| port(id, "source").unwrap().position.clone())
            .collect();
        let mut ends: Vec<Point> = ids
            .iter()
            .map(|id| port(id, "target").unwrap().position.clone())
            .collect();
        starts.sort_by(|p, q| distance(p, a_side).total_cmp(&distance(q, a_side)));
        ends.sort_by(|p, q| distance(p, b_side).total_cmp(&distance(q, b_side)));
        let (na, nb) = (normal(a_side), normal(b_side));
        let mut ancestors: BTreeSet<&str> = BTreeSet::from([node.as_str()]);
        let mut at = parents.get(node.as_str()).copied().flatten();
        while let Some(a) = at {
            ancestors.insert(a);
            at = parents.get(a).copied().flatten();
        }
        let obstacles: Vec<Rect> = boxes
            .iter()
            .filter(|(id, _)| !ancestors.contains(id.as_str()))
            .map(|(_, b)| inflate(b, spacing.margin))
            .collect();
        let mut routes = Vec::new();
        let mut taken: Vec<Segment> = Vec::new();
        let mut clear = true;
        for (i, id) in ids.iter().enumerate() {
            let offset = spacing.margin + i as f64 * spacing.track;
            let (s, t) = (&starts[i], &ends[i]);
            let corner = Point {
                x: corner_x + offset * (na.0 + nb.0),
                y: corner_y + offset * (na.1 + nb.1),
            };
            let out_of_s = Point {
                x: if horizontal_normal(a_side) {
                    corner.x
                } else {
                    s.x
                },
                y: if horizontal_normal(a_side) {
                    s.y
                } else {
                    corner.y
                },
            };
            let into_t = Point {
                x: if horizontal_normal(b_side) {
                    corner.x
                } else {
                    t.x
                },
                y: if horizontal_normal(b_side) {
                    t.y
                } else {
                    corner.y
                },
            };
            let points = vec![s.clone(), out_of_s, corner, into_t, t.clone()];
            let segments = segments_of(&points);
            let hits = points.windows(2).any(|w| {
                obstacles
                    .iter()
                    .any(|o| segment_hits(w[0].clone(), w[1].clone(), o))
            });
            let meets = segments.iter().any(|seg| {
                built
                    .iter()
                    .chain(&taken)
                    .any(|b| conflicts(b, seg, spacing.track) || crosses(b, seg))
            });
            if hits || meets {
                clear = false;
                break;
            }
            taken.extend(segments);
            routes.push(Route::new(*id, straighten(points)));
        }
        if clear {
            for route in routes {
                out.insert(route.id.clone(), route);
            }
        }
    }
    out
}

/// Whether two segments cross properly (perpendicular, each strictly inside the other's range).
fn crosses(a: &Segment, b: &Segment) -> bool {
    a.horizontal != b.horizontal
        && a.coord > b.range.0 + EPS
        && a.coord < b.range.1 - EPS
        && b.coord > a.range.0 + EPS
        && b.coord < a.range.1 - EPS
}

/// The routes of the prepared edges that run inside one planned layered area, from the plan's sections (step 5): per slot,
/// the corners in the flow's frame without the two ends. The route keeps the lane of its start port up to the first channel,
/// takes the lane of each dummy through the dummy's rank (parallel edges share the dummy's width, in the order of their ids),
/// and ends on the lane of its end port; it changes lane only in the channel between two ranks, on the track of its place in
/// the order of that channel's changes (`corridor_order`). An edge whose chain is missing, or whose channel has no room,
/// gets no route here.
fn section_routes(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    prepared: &[(usize, Result<Prepared, Delegation>)],
    plans: &LayeredPlans,
    spacing: Spacing,
) -> (
    BTreeMap<usize, Vec<(f64, f64)>>,
    BTreeSet<usize>,
    Vec<(String, usize, f64)>,
) {
    let parent: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref().unwrap_or("<root>")))
        .collect();
    // The areas from a node up to the root, each with the child of it that holds the node.
    let path = |id: &str| -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut child = id.to_string();
        while let Some(&area) = parent.get(child.as_str()) {
            out.push((area.to_string(), child.clone()));
            if area == "<root>" {
                break;
            }
            child = area.to_string();
        }
        out
    };
    // The steps of every edge that can be built from sections. A step changes lane (if at all) in the channel after rank
    // `rank` of `area`, to the lane of `target`, an element of `target_area`; `group` names the chain whose dummies parallel
    // edges share. The steps follow the chain of the smallest area that holds both ends between the children that hold
    // them; an end inside a container of the same axis (step 6) is reached through the container's border node, along the
    // chain to it inside the container.
    struct Step {
        area: String,
        rank: usize,
        target: String,
        target_area: String,
        group: Option<(String, (String, String))>,
    }
    struct Planned {
        steps: Vec<Step>,
        cross: bool,
    }
    // The first rank of every element of every area, built once instead of scanning the ranks for each step.
    let ranks_of: BTreeMap<&str, BTreeMap<&str, usize>> = plans
        .iter()
        .map(|(area, plan)| {
            let mut of = BTreeMap::new();
            for (r, rank) in plan.ranks.iter().enumerate() {
                for id in rank {
                    of.entry(id.as_str()).or_insert(r);
                }
            }
            (area.as_str(), of)
        })
        .collect();
    let rank_in = |area: &str, id: &str| -> Option<usize> { ranks_of.get(area)?.get(id).copied() };
    let chain_elements = |area: &str, from: &str, to: &str| -> Option<Vec<String>> {
        let dummies = plans
            .get(area)?
            .chains
            .get(&(from.to_string(), to.to_string()))?;
        let mut elements = vec![from.to_string()];
        elements.extend(dummies.iter().cloned());
        elements.push(to.to_string());
        Some(elements)
    };
    let along =
        |area: &str, elements: &[String], group: Option<(String, String)>| -> Option<Vec<Step>> {
            elements
                .windows(2)
                .map(|w| {
                    Some(Step {
                        area: area.to_string(),
                        rank: rank_in(area, &w[0])?,
                        target: w[1].clone(),
                        target_area: area.to_string(),
                        group: group.clone().map(|key| (area.to_string(), key)),
                    })
                })
                .collect()
        };
    let mut planned: BTreeMap<usize, Planned> = BTreeMap::new();
    for (slot, (index, outcome)) in prepared.iter().enumerate() {
        let Ok(p) = outcome else { continue };
        let edge = &graph.edges[*index];
        let Some(target_node) = edge.target.as_deref() else {
            continue;
        };
        let Some((area, source, target)) =
            crate::layered_plan::plan_ends(plans, &parent, &edge.source, target_node)
        else {
            continue;
        };
        if source == target {
            continue;
        }
        let Some(plan) = plans.get(&area).filter(|plan| !plan.reversed) else {
            continue;
        };
        if plan.vertical != p.vertical {
            continue;
        }
        let cross = edge.source != source || target_node != target;
        // Through border nodes: every container on the way has one for the edge, so the route goes up to the common area
        // along each container's chain to its border node, follows the chain there, and goes down the same way.
        // A backward edge (its link reversed by the cycle break) runs from its target to its source, through the border nodes
        // the placement gave it on the other sides, and is built that way (the prepared edge is reversed too).
        let through_borders = || -> Option<Vec<Step>> {
            let (backward, sections) = crate::abstract_routing::edge_sections(
                plans,
                &parent,
                &ranks_of,
                edge,
                (&area, &source, &target),
            )
            .ok()?;
            if backward != p.reversed {
                return None;
            }
            let group = if backward {
                (target.clone(), source.clone())
            } else {
                (source.clone(), target.clone())
            };
            Some(
                sections
                    .iter()
                    .enumerate()
                    .map(|(i, s)| {
                        let entering = sections.get(i + 1).filter(|next| next.area == s.to);
                        Step {
                            area: s.area.clone(),
                            rank: s.channel,
                            target: entering.map_or_else(|| s.to.clone(), |next| next.from.clone()),
                            target_area: entering
                                .map_or_else(|| s.area.clone(), |next| next.area.clone()),
                            group: (s.area == area && entering.is_none())
                                .then(|| (area.clone(), group.clone())),
                        }
                    })
                    .collect(),
            )
        };
        let has_border = |end: &str| {
            parent
                .get(end)
                .and_then(|a| plans.get(*a))
                .is_some_and(|inner| inner.borders.contains_key(&edge.id))
        };
        if cross && spacing.through_borders && (has_border(&edge.source) || has_border(target_node))
        {
            if let Some(steps) = through_borders() {
                planned.insert(
                    slot,
                    Planned {
                        steps,
                        cross: false,
                    },
                );
            }
            continue;
        }
        // An end inside a child container without a border node for the edge (a container of the other axis) is built from
        // the common chain from its port's lane; one inside a container of the same axis is left to the search.
        let inner = |id: &str, owner: &str| {
            path(id)
                .iter()
                .take_while(|(a, _)| *a != area)
                .any(|(a, _)| {
                    plans
                        .get(a)
                        .is_some_and(|inner| inner.vertical == plan.vertical)
                })
                && id != owner
        };
        if inner(&edge.source, &source) || inner(target_node, &target) {
            continue;
        }
        let logical = (source.clone(), target.clone());
        let backward = plan.reversed_links.contains(&logical);
        if backward != p.reversed {
            continue;
        }
        let key = if backward {
            (target.clone(), source.clone())
        } else {
            logical
        };
        let Some(elements) = chain_elements(&area, &key.0, &key.1) else {
            continue;
        };
        let Some(steps) = along(&area, &elements, Some(key)) else {
            continue;
        };
        planned.insert(slot, Planned { steps, cross });
    }
    // Parallel edges of one chain share its dummies: their place among them, by edge id.
    let mut siblings: BTreeMap<(String, (String, String)), Vec<(String, usize)>> = BTreeMap::new();
    for (&slot, pl) in &planned {
        let id = graph.edges[prepared[slot].0].id.clone();
        let mut groups: Vec<&(String, (String, String))> =
            pl.steps.iter().filter_map(|s| s.group.as_ref()).collect();
        groups.dedup();
        for group in groups {
            siblings
                .entry(group.clone())
                .or_default()
                .push((id.clone(), slot));
        }
    }
    // Parallel edges between containers take their places in the order of their border nodes, so that they do not cross
    // where they leave or enter a container; then by edge id.
    // Absolute boxes of the elements of an area: children from `boxes`, dummies from the plan, moved by the area's origin.
    let rect_of = |area: &str, id: &str| -> Option<Rect> {
        let plan = plans.get(area)?;
        match plan.dummies.get(id) {
            Some(d) => {
                let (ox, oy) = if area == "<root>" {
                    (0., 0.)
                } else {
                    boxes.get(area).map(|r| (r.x, r.y))?
                };
                Some(Rect {
                    x: d.x + ox,
                    y: d.y + oy,
                    width: d.width,
                    height: d.height,
                })
            }
            None => boxes.get(id).cloned(),
        }
    };
    let main_span = |vertical: bool, r: &Rect| {
        if vertical {
            (r.y, r.y + r.height)
        } else {
            (r.x, r.x + r.width)
        }
    };
    let across_span = |vertical: bool, r: &Rect| {
        if vertical {
            (r.x, r.x + r.width)
        } else {
            (r.y, r.y + r.height)
        }
    };
    // The edges that share a chain's dummies take their places among them in the order of the lanes on which they reach
    // the chain: an edge through a border node on the lane of its border node at the container the chain begins with, an
    // edge that begins at that element itself on its port's lane; so the bundle does not cross itself where it begins.
    let entry_lane = |edge: &str, slot: usize, from: &str| -> f64 {
        let p = prepared[slot].1.as_ref().ok();
        let border = plans
            .get(from)
            .and_then(|plan| plan.borders.get(edge))
            .and_then(|b| rect_of(from, b));
        match (border, p) {
            (Some(r), Some(p)) => {
                let (lo, hi) = across_span(p.vertical, &r);
                (lo + hi) / 2.
            }
            (None, Some(p)) => p.from.1,
            _ => 0.,
        }
    };
    for ((_, (from, _)), list) in siblings.iter_mut() {
        list.sort_by(|a, b| {
            entry_lane(&a.0, a.1, from)
                .total_cmp(&entry_lane(&b.0, b.1, from))
                .then(a.cmp(b))
        });
        list.dedup();
    }
    // The channel after rank `r` of an area: from the far edge of rank r to the near edge of rank r + 1, less the margin.
    let mut channel_spans: BTreeMap<(String, usize), Option<(f64, f64)>> = BTreeMap::new();
    let mut channel_of = |area: &'_ str, r: usize| -> Option<(f64, f64)> {
        *channel_spans
            .entry((area.to_string(), r))
            .or_insert_with(|| {
                let plan = plans.get(area)?;
                let (this, next) = (plan.ranks.get(r)?, plan.ranks.get(r + 1)?);
                let far = this
                    .iter()
                    .filter_map(|id| rect_of(area, id))
                    .map(|b| main_span(plan.vertical, &b).1)
                    .fold(f64::MIN, f64::max);
                let near = next
                    .iter()
                    .filter_map(|id| rect_of(area, id))
                    .map(|b| main_span(plan.vertical, &b).0)
                    .fold(f64::MAX, f64::min);
                let (lo, hi) = (far + spacing.margin, near - spacing.margin);
                (hi > lo + EPS).then_some((lo, hi))
            })
    };
    // Whether `lane` is clear of every element of rank `r` of `area` except `own` (with half a track and the margin).
    // (Requiring the lanes between it and the slot to be clear too kept the crossings and cost bends, measured on nine
    // views.) The spans across the flow of every rank's elements are taken once: the stretch search below asks often.
    let rank_spans: BTreeMap<&str, Vec<Vec<(&str, f64, f64)>>> = plans
        .iter()
        .map(|(area, plan)| {
            let spans = plan
                .ranks
                .iter()
                .map(|rank| {
                    rank.iter()
                        .filter_map(|id| {
                            let rect = rect_of(area, id)?;
                            let (lo, hi) = across_span(plan.vertical, &rect);
                            Some((id.as_str(), lo, hi))
                        })
                        .collect()
                })
                .collect();
            (area.as_str(), spans)
        })
        .collect();
    let lane_free = |area: &str, r: usize, own: &str, lane: f64| -> bool {
        let Some(rank) = rank_spans.get(area).and_then(|ranks| ranks.get(r)) else {
            return false;
        };
        let clear = spacing.margin + spacing.track / 2.;
        rank.iter()
            .all(|&(id, lo, hi)| lane < lo - clear || lane > hi + clear || id == own)
    };
    // The lanes of every planned edge, and its lane changes per channel.
    let mut lanes: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    let mut changes: BTreeMap<(String, usize), Vec<(usize, usize, f64, f64)>> = BTreeMap::new();
    let mut failed: BTreeSet<usize> = BTreeSet::new();
    for (&slot, pl) in &planned {
        let p = prepared[slot].1.as_ref().unwrap();
        let mut lane_list = vec![p.from.1];
        for (k, step) in pl.steps.iter().enumerate() {
            if k + 1 == pl.steps.len() {
                lane_list.push(p.to.1);
                break;
            }
            let Some(r) = rect_of(&step.target_area, &step.target) else {
                failed.insert(slot);
                break;
            };
            let (place, count) = match &step.group {
                Some(group) => {
                    let list = &siblings[group];
                    (
                        list.iter().position(|(_, s)| *s == slot).unwrap_or(0),
                        list.len(),
                    )
                }
                None => (0, 1),
            };
            let (lo, hi) = across_span(p.vertical, &r);
            // A dummy whose slots the placement set (`AreaPlan::lane_offsets`) holds the edge there.
            let edge_id = graph.edges[prepared[slot].0].id.as_str();
            let planned = plans.get(&step.target_area).and_then(|plan| {
                plan.lane_offsets
                    .get(&(step.target.clone(), edge_id.to_string()))
            });
            let lane = match planned {
                Some(offset) => lo + offset,
                None => lo + (hi - lo) * (place as f64 + 0.5) / count as f64,
            };
            lane_list.push(lane);
        }
        if failed.contains(&slot) || lane_list.len() != pl.steps.len() + 1 {
            failed.insert(slot);
            continue;
        }
        // With routes through border nodes, the lanes between two fixed points of the edge (its ports, and lanes at
        // elements of other areas) are chosen over the whole stretch: at a dummy of its own area the edge may take any lane
        // that nothing else in that rank covers (a node, a container, another chain's dummy). From the stretch's first
        // lane, the edge keeps its lane while it is free; where it is not, it takes the slot (of its slots in the stretch's
        // dummies) that stays free longest, which makes the fewest changes among them; it reaches the stretch's end lane
        // in the last channel.
        if spacing.through_borders {
            let n = pl.steps.len();
            let flexible = |i: usize| {
                i > 0 && i < n && {
                    let step = &pl.steps[i - 1];
                    step.target_area == step.area && step.target.starts_with('\u{1}')
                }
            };
            let free_at = |i: usize, lane: f64, slots: &[f64]| {
                let step = &pl.steps[i - 1];
                (lane - slots[i]).abs() < EPS
                    || lane_free(&step.area, step.rank + 1, &step.target, lane)
            };
            let slots = lane_list.clone();
            let mut i = 1;
            while i < n {
                if !flexible(i) {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < n && flexible(i) {
                    i += 1;
                }
                let end = i - 1;
                let from = lane_list[start - 1];
                // The candidates are the edge's slots in the stretch's dummies: taking the stretch's end lane early as well
                // saved more bends but cost crossings (measured on nine views: 2208 against 2005).
                let candidates: Vec<f64> = slots[start..=end].to_vec();
                let reach = |lane: f64, j: usize| {
                    (j..=end).take_while(|&k| free_at(k, lane, &slots)).count()
                };
                let mut current = from;
                for j in start..=end {
                    if !free_at(j, current, &slots) {
                        // The candidate free here that stays free longest (the latest slot on a tie).
                        current = candidates
                            .iter()
                            .copied()
                            .filter(|&c| free_at(j, c, &slots))
                            .max_by_key(|&c| reach(c, j))
                            .unwrap_or(slots[j]);
                    }
                    lane_list[j] = current;
                }
            }
        }
        for (k, step) in pl.steps.iter().enumerate() {
            if (lane_list[k] - lane_list[k + 1]).abs() < EPS {
                continue;
            }
            if channel_of(&step.area, step.rank).is_none() {
                failed.insert(slot);
                break;
            }
            changes
                .entry((step.area.clone(), step.rank))
                .or_default()
                .push((slot, k, lane_list[k], lane_list[k + 1]));
        }
        lanes.insert(slot, lane_list);
    }
    // The track of every change: its place in the channel's order.
    let mut narrow = Vec::new();
    let mut tracks: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for ((area, r), list) in &changes {
        let list: Vec<&(usize, usize, f64, f64)> =
            list.iter().filter(|c| !failed.contains(&c.0)).collect();
        let Some(span) = channel_of(area, *r) else {
            continue;
        };
        let spans: Vec<(f64, f64)> = list.iter().map(|c| (c.2, c.3)).collect();
        let order = if spacing.through_borders {
            dependent_order(&spans, spacing.track)
        } else {
            corridor_order(&spans)
        };
        let (slots, count) = track_slots(&spans, &order, spacing.track);
        let need = count.saturating_sub(1) as f64 * spacing.track;
        if need > span.1 - span.0 + EPS {
            narrow.push((area.clone(), *r, need - (span.1 - span.0)));
        }
        // With routes through border nodes the channel was made as wide as its tracks: they keep the track spacing when they
        // fit exactly, too.
        let positions = if spacing.through_borders {
            spaced_tracks(span, count, spacing.track)
        } else {
            ordered_tracks(span, count, spacing.track)
        };
        for &i in &order {
            tracks.insert((list[i].0, list[i].1), positions[slots[i]]);
        }
    }
    let mut out = BTreeMap::new();
    for (slot, lane_list) in lanes {
        if failed.contains(&slot) {
            continue;
        }
        let mut corners = Vec::new();
        let mut complete = true;
        for i in 0..lane_list.len() - 1 {
            if (lane_list[i] - lane_list[i + 1]).abs() < EPS {
                continue;
            }
            match tracks.get(&(slot, i)) {
                Some(&x) => {
                    corners.push((x, lane_list[i]));
                    corners.push((x, lane_list[i + 1]));
                }
                None => complete = false,
            }
        }
        if complete {
            out.insert(slot, corners);
        }
    }
    let cross = planned
        .iter()
        .filter(|(slot, pl)| pl.cross && out.contains_key(slot))
        .map(|(slot, _)| *slot)
        .collect();
    (out, cross, narrow)
}

/// The planned route of a prepared edge, when it misses every obstacle and keeps the track spacing from the routes already
/// built; it is then recorded like a constructed one.
fn build_planned(
    p: &Prepared,
    corners: &[(f64, f64)],
    assigned: &mut Vec<Segment>,
    track: f64,
    work: &RefCell<ConstructionCounters>,
) -> Option<(Route, usize)> {
    work.borrow_mut().planned_attempts += 1;
    let points = planned_points(p, corners);
    let clear = points.windows(2).all(|w| {
        !p.obstacles
            .iter()
            .any(|o| segment_hits(w[0].clone(), w[1].clone(), o))
    });
    let segments = segments_of(&points);
    if !clear {
        work.borrow_mut().planned_obstacle_failures += 1;
        #[cfg(feature = "benchmark-timings")]
        work.borrow_mut()
            .planned_failures
            .push((p.edge_id.clone(), "obstacle"));
        return None;
    }
    if segments
        .iter()
        .any(|s| assigned.iter().any(|a| conflicts(a, s, track)))
    {
        work.borrow_mut().planned_track_failures += 1;
        #[cfg(feature = "benchmark-timings")]
        work.borrow_mut()
            .planned_failures
            .push((p.edge_id.clone(), "track"));
        return None;
    }
    assigned.extend(segments);
    Some((finish_planned(p, points), corners.len() / 2))
}

fn planned_points(p: &Prepared, corners: &[(f64, f64)]) -> Vec<Point> {
    p.lead
        .0
        .iter()
        .cloned()
        .chain(std::iter::once(p.start.clone()))
        .chain(corners.iter().map(|&c| to_point(p.vertical, c)))
        .chain(std::iter::once(p.end.clone()))
        .chain(p.lead.1.iter().cloned())
        .collect()
}

fn finish_planned(p: &Prepared, points: Vec<Point>) -> Route {
    let mut points = straighten(points);
    if p.reversed {
        points.reverse();
    }
    Route::new(p.edge_id.as_str(), points)
}

/// An axis-aligned segment of a constructed route in absolute coordinates, so that routes of both flows share one record.
/// `horizontal` segments have a constant y (`coord`), the others a constant x; `range` is the extent along the segment.
#[derive(Clone, Copy, Debug)]
struct Segment {
    horizontal: bool,
    coord: f64,
    range: (f64, f64),
}

/// The absolute segments of a polyline; zero-length legs are skipped.
fn segments_of(points: &[Point]) -> Vec<Segment> {
    points
        .windows(2)
        .filter_map(|w| {
            let (a, b) = (&w[0], &w[1]);
            if (a.y - b.y).abs() < EPS {
                ((a.x - b.x).abs() > EPS).then(|| Segment {
                    horizontal: true,
                    coord: a.y,
                    range: (a.x.min(b.x), a.x.max(b.x)),
                })
            } else if (a.x - b.x).abs() < EPS {
                Some(Segment {
                    horizontal: false,
                    coord: a.x,
                    range: (a.y.min(b.y), a.y.max(b.y)),
                })
            } else {
                // Not axis-aligned: the construction never produces one.
                None
            }
        })
        .collect()
}

/// An edge that passed every check, with what its search needs: the flow, its two ends in the flow's frame, the band its
/// lane changes lie in, and the inflated obstacles.
/// The opt-in fixed-path realization omits search obstacles/index preparation
/// and uses its shared obstacle context instead; it never calls the path search.
#[derive(Clone)]
struct Prepared {
    edge_id: String,
    /// The nodes the route is built from and to (target and source for a reversed edge).
    from_node: String,
    to_node: String,
    /// A backward edge, built from its target to its source; its points are reversed at the end, so the route keeps the
    /// logical direction.
    reversed: bool,
    vertical: bool,
    from: (f64, f64),
    to: (f64, f64),
    band: (f64, f64),
    obstacles: Vec<Rect>,
    /// The index over `obstacles`, built once and shared by every search of the edge.
    obstacle_index: std::rc::Rc<BoxIndex>,
    start: Point,
    end: Point,
    /// With `Spacing::port_sides`: the points from the port to `start` (before it) and from `end` to the port (after it), in
    /// build order; empty for an end whose port lies on its flow side.
    lead: (Vec<Point>, Vec<Point>),
}

/// The track an edge must take in a corridor, by the corridor's key (`gap_key`): its place in the corridor's order.
type Desired = BTreeMap<(i64, i64), f64>;

#[allow(clippy::too_many_arguments)]
fn prepare_edge(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &crate::ports::PortEnds<'_>,
    parents: &BTreeMap<&str, Option<&str>>,
    containers: &BTreeSet<&str>,
    spacing: Spacing,
    orientations: &BTreeMap<String, Orientation>,
    edge_id: &str,
    source: &str,
    target: &str,
    require_band: bool,
) -> Result<Prepared, Delegation> {
    prepare_edge_inner(
        graph,
        boxes,
        ports,
        parents,
        containers,
        spacing,
        orientations,
        edge_id,
        source,
        target,
        require_band,
        true,
        &BTreeMap::new(),
    )
}

#[allow(clippy::too_many_arguments)]
fn prepare_edge_inner(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &crate::ports::PortEnds<'_>,
    parents: &BTreeMap<&str, Option<&str>>,
    containers: &BTreeSet<&str>,
    spacing: Spacing,
    orientations: &BTreeMap<String, Orientation>,
    edge_id: &str,
    source: &str,
    target: &str,
    require_band: bool,
    prepare_obstacles: bool,
    lanes: &BTreeMap<(String, String), (f64, f64)>,
) -> Result<Prepared, Delegation> {
    let _ = containers;
    if !boxes.contains_key(source) || !boxes.contains_key(target) {
        return Err(Delegation::Unresolved);
    }
    // The areas the route passes: from each end up to the smallest container that holds both. An edge between containers
    // leaves its own containers through their borders and runs between them on the common level; it is one monotone path
    // when every area on the way flows along the same axis. A container is an endpoint like a leaf (its port is on its
    // border); an end that lies inside the other end is not such a path.
    let ancestors = |id: &str| -> Vec<Option<&str>> {
        let mut chain = Vec::new();
        let mut at = parents.get(id).copied().flatten();
        loop {
            chain.push(at);
            match at {
                Some(a) => at = parents.get(a).copied().flatten(),
                None => break,
            }
        }
        chain
    };
    let (s_chain, t_chain) = (ancestors(source), ancestors(target));
    if s_chain.contains(&Some(target)) || t_chain.contains(&Some(source)) {
        return Err(Delegation::CrossesArea);
    }
    let common = *s_chain
        .iter()
        .find(|a| t_chain.contains(a))
        .expect("the root is common to every chain");
    let passed: Vec<Option<&str>> = s_chain
        .iter()
        .take_while(|a| **a != common)
        .chain(t_chain.iter().take_while(|a| **a != common))
        .copied()
        .collect();
    let vertical = match flow_of(graph, common, orientations) {
        Some(vertical) => vertical,
        None => return Err(Delegation::NoFixedFlow),
    };
    // An edge between areas of different flow axes is built in the frame of the smallest area that holds both ends (plan
    // section 4a, step 6); the areas it passes need a fixed flow of either axis.
    if passed
        .iter()
        .any(|area| flow_of(graph, *area, orientations).is_none())
    {
        return Err(Delegation::CrossesArea);
    }
    let found = |endpoint: &str| ports.get(edge_id, endpoint).ok_or(Delegation::Unresolved);
    let (source_port, target_port) = (found("source")?, found("target")?);
    let (out_side, in_side) = if vertical {
        (PortSide::Bottom, PortSide::Top)
    } else {
        (PortSide::Right, PortSide::Left)
    };
    // A backward edge whose ports lie on the reversed flow sides is built from its target to its source, as a forward edge.
    // With port sides, every edge whose target box lies before its source box along the flow is.
    let centre = |id: &str| {
        let r = &boxes[id];
        frame(
            vertical,
            &Point {
                x: r.x + r.width / 2.,
                y: r.y + r.height / 2.,
            },
        )
        .0
    };
    let reversed = if spacing.port_sides {
        centre(target) < centre(source) - EPS
    } else {
        frame(vertical, &target_port.position).0 < frame(vertical, &source_port.position).0 - EPS
            && source_port.side == in_side
            && target_port.side == out_side
    };
    let (first, last, from_node, to_node) = if reversed {
        (target_port, source_port, target, source)
    } else {
        (source_port, target_port, source, target)
    };
    let (lead_first, first_at, first_side) = if spacing.port_sides {
        let planned = lanes
            .get(&(edge_id.to_string(), first.endpoint.clone()))
            .copied();
        let (lane, stub) = (
            planned.map(|p| p.0),
            planned.map_or(spacing.margin, |p| p.1),
        );
        let (lead, at) = port_lead(
            &boxes[from_node],
            first,
            &last.position,
            true,
            vertical,
            (spacing.margin, stub),
            lane,
        );
        (
            lead.clone(),
            at,
            if lead.is_empty() {
                first.side
            } else {
                out_side
            },
        )
    } else {
        (Vec::new(), first.position.clone(), first.side)
    };
    let (lead_last, last_at, last_side) = if spacing.port_sides {
        // The target's lead turns toward the lane the source's lead ends on.
        let planned = lanes
            .get(&(edge_id.to_string(), last.endpoint.clone()))
            .copied();
        let (lane, stub) = (
            planned.map(|p| p.0),
            planned.map_or(spacing.margin, |p| p.1),
        );
        let (mut lead, at) = port_lead(
            &boxes[to_node],
            last,
            &first_at,
            false,
            vertical,
            (spacing.margin, stub),
            lane,
        );
        lead.reverse();
        (
            lead.clone(),
            at,
            if lead.is_empty() { last.side } else { in_side },
        )
    } else {
        (Vec::new(), last.position.clone(), last.side)
    };
    let (s_main, s_across) = frame(vertical, &first_at);
    let (t_main, t_across) = frame(vertical, &last_at);
    // The flow runs in the positive direction of the frame's main axis.
    if t_main <= s_main + EPS {
        return Err(Delegation::NotForward);
    }
    // The direction is checked first, so that an edge running backwards is reported as such and not by its port side.
    if first_side != out_side || last_side != in_side {
        return Err(Delegation::PortSide);
    }
    let (s_box, t_box) = (&boxes[from_node], &boxes[to_node]);
    let (s_far, t_near) = if vertical {
        (s_box.y + s_box.height, t_box.y)
    } else {
        (s_box.x + s_box.width, t_box.x)
    };
    // The middle of the route lies beyond each endpoint's own box by the margin, so the part of the first and last
    // segment that the clearance does not trim stays clear of its own node.
    let band = (s_far + spacing.margin, t_near - spacing.margin);
    if require_band && band.1 <= band.0 + EPS {
        return Err(Delegation::NoFreeTrack);
    }
    // Obstacles: every box except the endpoints and the areas that contain them, inflated by the margin.
    // The containers of both ends are left or entered through their borders, so they are not obstacles.
    let skip = endpoint_exclusions(parents, source, target);
    let obstacles: Vec<Rect> = if prepare_obstacles {
        boxes
            .iter()
            .filter(|(id, _)| !skip.contains(id.as_str()))
            .map(|(_, r)| inflate(r, spacing.margin))
            .collect()
    } else {
        Vec::new()
    };
    Ok(Prepared {
        edge_id: edge_id.to_string(),
        from_node: from_node.to_string(),
        to_node: to_node.to_string(),
        reversed,
        vertical,
        from: (s_main, s_across),
        to: (t_main, t_across),
        band,
        obstacle_index: std::rc::Rc::new(BoxIndex::new(
            &obstacles
                .iter()
                .map(|r| [r.x, r.y, r.x + r.width, r.y + r.height])
                .collect::<Vec<_>>(),
        )),
        obstacles,
        start: first_at,
        end: last_at,
        lead: (lead_first, lead_last),
    })
}

/// The leads by (node, high side), each with its order key (`lead_key`, then whether it goes around the node).
type LeadGroups<'a> = BTreeMap<(&'a str, bool), Vec<((u8, u8, i64), &'a LeadEnd)>>;

/// One end of an edge at an explicit port, for `plan_leads`.
pub(crate) struct LeadEnd {
    pub node: String,
    pub edge: String,
    pub endpoint: String,
    pub side: PortSide,
    /// The port's place along the flow within its node, 0 at the near end and 1 at the far end.
    pub along: f64,
    /// The port's place across the flow within its node, 0 at the low side and 1 at the high side.
    pub across: f64,
    /// The route leaves the node along the flow here (a source end, or the target end of an edge built reversed).
    pub leaving: bool,
    pub vertical: bool,
}

/// Provisional (option `portSides`): the lead of every end at an explicit port that does not lie on its flow side, by
/// (edge, endpoint): its side across the flow (true for the high side) and its lane's number from the node outwards. A port
/// across the flow has its lead on its own side; a port on the side against the flow goes around its node on the side with
/// fewer leads of the first kind (the low one when even). On each side the leads are numbered by `lead_key`, so they do not
/// cross each other; leads around the node come last of their kind. The placement keeps that many lanes beside the side
/// (`explicit_lanes` in lib.rs) and the construction draws the leads on them (`lead_lanes`).
pub(crate) fn plan_leads(ends: &[LeadEnd]) -> Vec<(String, String, bool, usize, usize)> {
    let low_high = |e: &LeadEnd| match (e.side, e.vertical) {
        (PortSide::Top, false) | (PortSide::Left, true) => Some(false),
        (PortSide::Bottom, false) | (PortSide::Right, true) => Some(true),
        _ => None,
    };
    let far = |e: &LeadEnd| {
        matches!(
            (e.side, e.vertical),
            (PortSide::Right, false) | (PortSide::Bottom, true)
        )
    };
    let mut across: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for e in ends {
        if let Some(high) = low_high(e) {
            let c = across.entry(e.node.as_str()).or_default();
            if high {
                c.1 += 1;
            } else {
                c.0 += 1;
            }
        }
    }
    let mut groups: LeadGroups<'_> = BTreeMap::new();
    for e in ends {
        let (high, around) = match low_high(e) {
            Some(high) => (high, 0),
            // On a side along the flow: no lead where the route leaves by the far side or arrives by the near one.
            None if far(e) == e.leaving => continue,
            // Around the node by the side with fewer leads across the flow (the low one when even): the placement and the
            // construction decide alike (measured against the side toward the other end, which is known only in the
            // construction: fewer crossings, but more shared lines and bends; `elk-port-sides-20261108.md`).
            None => {
                let (low, high) = across.get(e.node.as_str()).copied().unwrap_or_default();
                (high < low, 1)
            }
        };
        let (kind, mut at) = lead_key(e.leaving, e.along);
        if around == 1 {
            // Around the node: the port nearest the side the lead goes to first, so that the leads around one end of the
            // node nest (the inner one's lane and way both nearer the node) instead of crossing.
            let toward = if high { 1. - e.across } else { e.across };
            at = (toward * 1000.).round() as i64;
        }
        groups
            .entry((e.node.as_str(), high))
            .or_default()
            .push(((kind, around, at), e));
    }
    let mut out = Vec::new();
    for ((_, high), mut list) in groups {
        list.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.edge.cmp(&b.1.edge))
                .then(a.1.endpoint.cmp(&b.1.endpoint))
        });
        for (i, (key, e)) in list.into_iter().enumerate() {
            out.push((
                e.edge.clone(),
                e.endpoint.clone(),
                high,
                i,
                usize::from(key.1 == 1),
            ));
        }
    }
    // The leads around one end of a node (its side against the flow) stand a track apart along the flow, the low side's
    // first, each side's in its lane order: the number replaces the marker set above.
    let mut around: BTreeMap<(&str, u8), Vec<usize>> = BTreeMap::new();
    for (k, (edge, endpoint, _, _, marked)) in out.iter().enumerate() {
        if *marked == 1 {
            let e = ends
                .iter()
                .find(|e| &e.edge == edge && &e.endpoint == endpoint)
                .expect("planned end");
            around
                .entry((e.node.as_str(), e.side as u8))
                .or_default()
                .push(k);
        }
    }
    let mut stubs = vec![0; out.len()];
    for list in around.values() {
        let mut list = list.clone();
        list.sort_by_key(|&k| (out[k].2, out[k].3));
        for (n, k) in list.into_iter().enumerate() {
            stubs[k] = n;
        }
    }
    for (k, entry) in out.iter_mut().enumerate() {
        entry.4 = stubs[k];
    }
    out
}

/// Provisional (option `portSides`): the lane across the flow of every lead (`plan_leads`), by (edge, endpoint), in the
/// drawing's coordinates (the margin beside its node's side, and a track further for each lead nearer the node), and how far
/// a lead around the node goes out along the flow before it turns (the margin, and a track further for each one inside it).
pub(crate) fn lead_lanes(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[arqlet_model::ResolvedPort],
    parents: &BTreeMap<&str, Option<&str>>,
    orientations: &BTreeMap<String, Orientation>,
    spacing: Spacing,
) -> BTreeMap<(String, String), (f64, f64)> {
    let mut out = BTreeMap::new();
    if !spacing.port_sides {
        return out;
    }
    let edges: BTreeMap<&str, &arqlet_model::Edge> =
        graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut ends = Vec::new();
    for port in ports.iter().filter(|p| p.explicit) {
        let Some(edge) = edges.get(port.edge_id.as_str()) else {
            continue;
        };
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        if edge.source == target {
            continue;
        }
        let (Some(node), Some(s_box), Some(t_box)) = (
            boxes.get(&port.node_id),
            boxes.get(&edge.source),
            boxes.get(target),
        ) else {
            continue;
        };
        let parent = parents.get(port.node_id.as_str()).copied().flatten();
        let Some(vertical) = flow_of(graph, parent, orientations) else {
            continue;
        };
        let centre = |r: &Rect| {
            if vertical {
                r.y + r.height / 2.
            } else {
                r.x + r.width / 2.
            }
        };
        let reversed = centre(t_box) < centre(s_box) - EPS;
        let (start, length) = if vertical {
            (node.y, node.height)
        } else {
            (node.x, node.width)
        };
        ends.push(LeadEnd {
            node: port.node_id.clone(),
            edge: port.edge_id.clone(),
            endpoint: port.endpoint.clone(),
            side: port.side,
            along: (frame(vertical, &port.position).0 - start) / length.max(EPS),
            across: {
                let (lo, size) = if vertical {
                    (node.x, node.width)
                } else {
                    (node.y, node.height)
                };
                (frame(vertical, &port.position).1 - lo) / size.max(EPS)
            },
            leaving: (port.endpoint == "source") != reversed,
            vertical,
        });
    }
    let plan = plan_leads(&ends);
    let end_of = |edge: &str, endpoint: &str| {
        ends.iter()
            .find(|e| e.edge == edge && e.endpoint == endpoint)
            .expect("planned end")
    };
    for (edge, endpoint, high, i, stub) in &plan {
        let (high, i) = (*high, *i);
        let stub = spacing.margin + *stub as f64 * spacing.track;
        let e = end_of(edge, endpoint);
        let r = &boxes[&e.node];
        let (lo, hi) = if e.vertical {
            (r.x, r.x + r.width)
        } else {
            (r.y, r.y + r.height)
        };
        let away = spacing.margin + i as f64 * spacing.track;
        out.insert(
            (edge.clone(), endpoint.clone()),
            (if high { hi + away } else { lo - away }, stub),
        );
    }
    out
}

/// The order of the leads on one side of a node (`plan_leads`): nearest the node first. `along` is the port's place along
/// the flow within its node.
pub(crate) fn lead_key(leaving: bool, along: f64) -> (u8, i64) {
    let at = (along * 1000.).round() as i64;
    if leaving {
        (0, -at)
    } else {
        (1, at)
    }
}

/// The lead of an edge end whose port does not lie on the side the route leaves (`leaving`: the far side along the flow) or
/// enters (the near side): the points from the port to the point the constructed route starts (ends) at, without that
/// point, and that point. A port on its flow side has no lead. A port across the flow goes straight out to a lane beside
/// the node: the other end's lane (`other`, across the flow) when that lies beyond the side, else the margin away. A port on
/// the side against the flow goes out by the margin and around the node to such a lane, on the side toward the other end.
/// The route then runs along that lane past the node, so it stays monotone along the flow.
fn port_lead(
    node: &Rect,
    port: &arqlet_model::ResolvedPort,
    other: &Point,
    leaving: bool,
    vertical: bool,
    (margin, stub): (f64, f64),
    assigned: Option<f64>,
) -> (Vec<Point>, Point) {
    let (main_lo, main_hi, across_lo, across_hi) = if vertical {
        (node.y, node.y + node.height, node.x, node.x + node.width)
    } else {
        (node.x, node.x + node.width, node.y, node.y + node.height)
    };
    let (p_main, p_across) = frame(vertical, &port.position);
    let o_across = frame(vertical, other).1;
    // The port's side in the frame: along the flow (+1 far, -1 near) or across (+1 high, -1 low).
    let (along, across) = match (port.side, vertical) {
        (PortSide::Right, false) | (PortSide::Bottom, true) => (1, 0),
        (PortSide::Left, false) | (PortSide::Top, true) => (-1, 0),
        (PortSide::Bottom, false) | (PortSide::Right, true) => (0, 1),
        (PortSide::Top, false) | (PortSide::Left, true) => (0, -1),
    };
    let flow = if leaving { 1 } else { -1 };
    if along == flow {
        return (Vec::new(), port.position.clone());
    }
    // The lane beside the node on side `dir` (-1 low, +1 high): the other end's lane when it lies beyond, else the margin.
    let lane = |dir: i32| {
        if let Some(lane) = assigned {
            return lane;
        }
        let edge = if dir < 0 {
            across_lo - margin
        } else {
            across_hi + margin
        };
        if (dir < 0 && o_across < edge) || (dir > 0 && o_across > edge) {
            o_across
        } else {
            edge
        }
    };
    if across != 0 {
        let at = to_point(vertical, (p_main, lane(across)));
        return (vec![port.position.clone()], at);
    }
    // Against the flow: out by the margin, then around the side of its planned lane (else the side nearer the other end).
    let toward = assigned.unwrap_or(o_across);
    let dir = if toward < (across_lo + across_hi) / 2. {
        -1
    } else {
        1
    };
    let out = if along < 0 {
        main_lo - stub
    } else {
        main_hi + stub
    };
    let stub = to_point(vertical, (out, p_across));
    let at = to_point(vertical, (out, lane(dir)));
    (vec![port.position.clone(), stub], at)
}

/// The route of a prepared edge: its tracks kept apart from `assigned`, and, where `desired` names a corridor, on the track
/// of its place in that corridor's order (or on any free track when that one is not free).
fn build_edge(
    p: &Prepared,
    assigned: &mut Vec<Segment>,
    track: f64,
    desired: &Desired,
    work: &RefCell<ConstructionCounters>,
) -> Result<(Route, usize), Delegation> {
    work.borrow_mut().edge_searches += 1;
    let (vertical, edge_id) = (p.vertical, p.edge_id.as_str());
    let search = |taken: &[Segment]| {
        monotone_path(
            edge_id,
            &p.obstacles,
            &p.obstacle_index,
            taken,
            vertical,
            p.from,
            p.to,
            p.band,
            track,
            desired,
            work,
        )
    };
    let corners = match search(assigned) {
        Some(corners) => corners,
        // The reason is named from the routes already built (another search without them would tell the two causes apart
        // exactly, at the cost of a whole search; the reason does not change the drawing).
        None if !assigned.is_empty() => return Err(Delegation::TrackTaken),
        None => return Err(Delegation::NoFreeTrack),
    };
    let points = planned_points(p, &corners);
    let segments = segments_of(&points);
    // The search already keeps its steps clear of the assigned segments; this final check keeps the record consistent.
    if segments
        .iter()
        .any(|s| assigned.iter().any(|a| conflicts(a, s, track)))
    {
        return Err(Delegation::NoFreeTrack);
    }
    assigned.extend(segments);
    let lane_changes = corners.len() / 2;
    let mut points = straighten(points);
    if p.reversed {
        points.reverse();
    }
    Ok((Route::new(edge_id, points), lane_changes))
}

/// The cheapest monotone path from `from` to `to` in the frame, as its corners in order, without the two ends. The path
/// leaves `from` and enters `to` along the flow and changes lane at most once in each corridor between the boxes. `None`
/// when no such path keeps clear of the obstacles, which are the inflated boxes. The corridors lie inside `band`.
///
/// `arrive[k][j]` is the cost of reaching corridor `k` on lane `j` from the left. `depart[k][j]` is the cost of leaving
/// it on lane `j`, after its one change of lane. The lanes are the free gaps across the flow, and both port lanes.
/// The free gaps of a search: along the flow (where the route may change lane) and across it (its lanes).
/// How many tracks the outermost lanes reach beyond the boxes.
const OUTER_TRACKS: f64 = 16.;

fn search_gaps(
    obstacles: &[Rect],
    vertical: bool,
    from: (f64, f64),
    to: (f64, f64),
    band: (f64, f64),
    track: f64,
) -> (Vec<(f64, f64)>, Vec<(f64, f64)>) {
    let x_gaps: Vec<(f64, f64)> = free_gaps(obstacles, vertical, band.0, band.1, track)
        .into_iter()
        .filter(|(lo, hi)| (lo + hi) / 2. > from.0 + EPS && (lo + hi) / 2. < to.0 - EPS)
        .collect();
    let (lo_across, hi_across) = obstacles.iter().fold((f64::MAX, f64::MIN), |(lo, hi), r| {
        let (a, b) = across_extent(r, vertical);
        (lo.min(a), hi.max(b))
    });
    // The lanes reach OUTER_TRACKS tracks beyond the outermost box, so routes can pass on either side of a row of boxes, and
    // long edges (backward edges above all) find a free track there.
    let y_gaps = free_gaps(
        obstacles,
        !vertical,
        lo_across - OUTER_TRACKS * track,
        hi_across + OUTER_TRACKS * track,
        track,
    );
    (x_gaps, y_gaps)
}

fn centres(gaps: &[(f64, f64)]) -> Vec<f64> {
    gaps.iter().map(|(lo, hi)| (lo + hi) / 2.).collect()
}

/// The gap that holds `v`.
fn gap_of(gaps: &[(f64, f64)], v: f64) -> Option<(f64, f64)> {
    gaps.iter()
        .copied()
        .find(|(lo, hi)| v > lo - EPS && v < hi + EPS)
}

/// The key of a corridor: its free interval along the flow at half-unit resolution, so that the edges that change lane in
/// the same corridor find the same key.
fn gap_key((lo, hi): (f64, f64)) -> (i64, i64) {
    ((lo * 2.).round() as i64, (hi * 2.).round() as i64)
}

/// The way of a prepared edge over the gaps' centres, regardless of the routes already built: its lane changes as
/// (corridor, lane before, lane after) in the flow's frame.
fn lane_changes(
    p: &Prepared,
    track: f64,
    work: &RefCell<ConstructionCounters>,
) -> Vec<((f64, f64), f64, f64)> {
    work.borrow_mut().lane_searches += 1;
    let (x_gaps, y_gaps) = search_gaps(&p.obstacles, p.vertical, p.from, p.to, p.band, track);
    let empty_index = BoxIndex::new(&[]);
    let bare = Lookup {
        obstacles: &p.obstacles,
        obstacle_index: &p.obstacle_index,
        assigned: &[],
        assigned_index: &empty_index,
        crossed: &[],
        crossed_index: &empty_index,
    };
    let Some(corners) = search_on(
        (&p.edge_id, "lane"),
        &bare,
        p.vertical,
        p.from,
        p.to,
        centres(&x_gaps),
        centres(&y_gaps),
        track,
        work,
    ) else {
        return Vec::new();
    };
    corners
        .chunks(2)
        .filter_map(|pair| {
            let (x, before) = pair[0];
            let after = pair.get(1)?.1;
            Some((gap_of(&x_gaps, x)?, before, after))
        })
        .collect()
}

/// The order of the lane changes in one corridor that crosses least. A change is (lane before, lane after); when `a` lies
/// before `b` along the flow, `a`'s outgoing lane crosses `b`'s change if it lies inside `b`'s span, and `b`'s incoming lane
/// crosses `a`'s change if it lies inside `a`'s span. Neighbours are swapped while that lowers the crossings (greedy switch),
/// from the order of the lanes' midpoints.
fn corridor_order(changes: &[(f64, f64)]) -> Vec<usize> {
    let inside = |v: f64, (a, b): (f64, f64)| v > a.min(b) + EPS && v < a.max(b) - EPS;
    let cost =
        |a: (f64, f64), b: (f64, f64)| usize::from(inside(a.1, b)) + usize::from(inside(b.0, a));
    let mut order: Vec<usize> = (0..changes.len()).collect();
    order.sort_by(|&i, &j| {
        let (a, b) = (changes[i], changes[j]);
        (a.0 + a.1).total_cmp(&(b.0 + b.1)).then(i.cmp(&j))
    });
    let mut improved = true;
    let mut rounds = 0;
    while improved && rounds <= changes.len() * changes.len() {
        improved = false;
        rounds += 1;
        for k in 0..order.len().saturating_sub(1) {
            let (a, b) = (changes[order[k]], changes[order[k + 1]]);
            if cost(b, a) < cost(a, b) {
                order.swap(k, k + 1);
                improved = true;
            }
        }
    }
    order
}

/// The order of the lane changes of one channel that keeps them apart and crosses least: placing `a` before `b` costs the
/// crossings of the two in that order (as in `corridor_order`), and far more when `a` arrives on a lane (closer than
/// `track`) that `b` leaves, since `a`'s new lane would run on `b`'s old one. The changes are taken greedily, each time the
/// one whose placing first costs least against all that remain; then every change moves to its cheapest place among the
/// others while that lowers the cost.
pub(crate) fn dependent_order(changes: &[(f64, f64)], track: f64) -> Vec<usize> {
    const CONFLICT: i64 = 1_000;
    let n = changes.len();
    let inside = |v: f64, (a, b): (f64, f64)| v > a.min(b) + EPS && v < a.max(b) - EPS;
    let first = |a: (f64, f64), b: (f64, f64)| -> i64 {
        i64::from(inside(a.1, b))
            + i64::from(inside(b.0, a))
            + if (a.1 - b.0).abs() < track - EPS {
                CONFLICT
            } else {
                0
            }
    };
    // cost[i][j]: `i` placed before `j`.
    let cost: Vec<Vec<i64>> = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| {
                    if i == j {
                        0
                    } else {
                        first(changes[i], changes[j])
                    }
                })
                .collect()
        })
        .collect();
    let mut left: Vec<usize> = corridor_order(changes);
    let mut balances: Vec<i64> = (0..n)
        .map(|i| (0..n).map(|j| cost[i][j] - cost[j][i]).sum())
        .collect();
    let mut order = Vec::with_capacity(n);
    while !left.is_empty() {
        let mut best = 0;
        for k in 1..left.len() {
            if balances[left[k]] < balances[left[best]] {
                best = k;
            }
        }
        let removed = left.remove(best);
        order.push(removed);
        for &i in &left {
            balances[i] -= cost[i][removed] - cost[removed][i];
        }
    }
    // Sifting: each change, taken out, goes to the place where it costs least against the others.
    let mut improved = true;
    let mut rounds = 0;
    while improved && rounds < 8 {
        improved = false;
        rounds += 1;
        for i in 0..n {
            let at = order.iter().position(|&x| x == i).expect("in the order");
            let mut rest = order.clone();
            rest.remove(at);
            // Cost of `i` at place p: before rest[p..], after rest[..p].
            let mut c: i64 = rest.iter().map(|&j| cost[i][j]).sum();
            let (mut best, mut best_place) = (c, 0);
            for (p, &j) in rest.iter().enumerate() {
                c += cost[j][i] - cost[i][j];
                if c < best {
                    best = c;
                    best_place = p + 1;
                }
            }
            let now: i64 = rest[..at].iter().map(|&j| cost[j][i]).sum::<i64>()
                + rest[at..].iter().map(|&j| cost[i][j]).sum::<i64>();
            if best < now {
                rest.insert(best_place, i);
                order = rest;
                improved = true;
            }
        }
    }
    order
}

/// The track slot of every lane change of one channel, in the channel's order (`order`, positions into `changes`): changes
/// whose spans across the flow overlap (with a track of clearance) keep the order on separate tracks; changes that do not
/// overlap may share a track. A change takes the slot after the highest slot of the overlapping changes before it in the
/// order. Returns the slot of each change (by its index in `changes`) and the number of slots.
pub(crate) fn track_slots(
    changes: &[(f64, f64)],
    order: &[usize],
    track: f64,
) -> (Vec<usize>, usize) {
    let span = |(a, b): (f64, f64)| (a.min(b), a.max(b));
    // Building the range index costs more than the prefix scan on short channels.
    if order.len() <= TRACK_SLOT_INDEX_THRESHOLD {
        let mut slots = vec![0; changes.len()];
        let mut count = 0;
        for (k, &i) in order.iter().enumerate() {
            let (lo, hi) = span(changes[i]);
            let slot = order[..k]
                .iter()
                .filter(|&&j| {
                    let (l, h) = span(changes[j]);
                    l < hi + track - EPS && lo < h + track - EPS
                })
                .map(|&j| slots[j] + 1)
                .max()
                .unwrap_or(0);
            slots[i] = slot;
            count = count.max(slot + 1);
        }
        return (slots, count);
    }
    let cmp = |a: &f64, b: &f64| {
        if a == b {
            std::cmp::Ordering::Equal
        } else {
            a.total_cmp(b)
        }
    };
    let mut lows: Vec<f64> = changes.iter().map(|&change| span(change).0).collect();
    lows.sort_by(cmp);
    lows.dedup_by(|a, b| *a == *b);

    let mut highs_by_low = vec![Vec::new(); lows.len() + 1];
    for &change in changes {
        let (lo, hi) = span(change);
        let index = lows.binary_search_by(|value| cmp(value, &lo)).unwrap() + 1;
        let mut node = index;
        while node < highs_by_low.len() {
            highs_by_low[node].push(hi);
            node += node & node.wrapping_neg();
        }
    }

    let mut maxima: Vec<Vec<usize>> = highs_by_low
        .iter_mut()
        .map(|highs| {
            highs.sort_by(cmp);
            highs.dedup_by(|a, b| *a == *b);
            vec![0; highs.len() + 1]
        })
        .collect();
    let mut slots = vec![0; changes.len()];
    let mut count = 0;
    for &i in order {
        let (lo, hi) = span(changes[i]);
        let max_low_count = lows.partition_point(|&l| l < hi + track - EPS);
        let mut node = max_low_count;
        let mut slot = 0;
        while node > 0 {
            let highs = &highs_by_low[node];
            let suffix_start = highs.partition_point(|&h| !(lo < h + track - EPS));
            let mut high_index = highs.len() - suffix_start;
            while high_index > 0 {
                slot = slot.max(maxima[node][high_index]);
                high_index &= high_index - 1;
            }
            node &= node - 1;
        }
        slots[i] = slot;
        count = count.max(slot + 1);
        let (lo, hi) = span(changes[i]);
        let low_index = lows.binary_search_by(|value| cmp(value, &lo)).unwrap() + 1;
        let mut node = low_index;
        while node < highs_by_low.len() {
            let highs = &highs_by_low[node];
            let high_position = highs.binary_search_by(|value| cmp(value, &hi)).unwrap();
            let mut high_index = highs.len() - high_position;
            while high_index < maxima[node].len() {
                maxima[node][high_index] = maxima[node][high_index].max(slot + 1);
                high_index += high_index & high_index.wrapping_neg();
            }
            node += node & node.wrapping_neg();
        }
    }
    (slots, count)
}

/// The tracks of `count` lane changes in a gap, in order along the flow: at the track spacing, centred in the gap; spread
/// evenly when the gap is too narrow for that.
fn ordered_tracks((lo, hi): (f64, f64), count: usize, track: f64) -> Vec<f64> {
    let n = count as f64;
    let span = (n - 1.) * track;
    if span < hi - lo - 2. * EPS {
        let first = (lo + hi) / 2. - span / 2.;
        (0..count).map(|i| first + i as f64 * track).collect()
    } else {
        (0..count)
            .map(|i| lo + (hi - lo) * (i as f64 + 1.) / (n + 1.))
            .collect()
    }
}

/// `ordered_tracks` that keeps the track spacing whenever the tracks fit (also exactly), centred in the gap.
fn spaced_tracks((lo, hi): (f64, f64), count: usize, track: f64) -> Vec<f64> {
    let span = count.saturating_sub(1) as f64 * track;
    if span <= hi - lo + EPS {
        let first = (lo + hi) / 2. - span / 2.;
        (0..count).map(|i| first + i as f64 * track).collect()
    } else {
        ordered_tracks((lo, hi), count, track)
    }
}

#[allow(clippy::too_many_arguments)]
fn monotone_path(
    edge_id: &str,
    obstacles: &[Rect],
    obstacle_index: &BoxIndex,
    assigned: &[Segment],
    vertical: bool,
    from: (f64, f64),
    to: (f64, f64),
    band: (f64, f64),
    track: f64,
    desired: &Desired,
    work: &RefCell<ConstructionCounters>,
) -> Option<Vec<(f64, f64)>> {
    let (x_gaps, y_gaps) = search_gaps(obstacles, vertical, from, to, band, track);
    // Every step of the search lies between the ends along the flow (`from` to `to` on the main axis); a segment whose box
    // (with the track spacing) misses that range cannot meet a step, so only the others are indexed. The answers are those
    // over all segments; an edit, whose kept routes all count as built, indexes far fewer.
    let (lo, hi) = (
        from.0.min(to.0) - track - EPS,
        from.0.max(to.0) + track + EPS,
    );
    let near: Vec<Segment> = assigned
        .iter()
        .filter(|s| {
            let b = segment_box(s, track);
            let (b0, b1) = if vertical { (b[1], b[3]) } else { (b[0], b[2]) };
            b0 <= hi && b1 >= lo
        })
        .copied()
        .collect();
    let assigned = near.as_slice();
    // First the way: which gaps the route passes, on the gaps' centres. The routes already built do not block it, but every
    // crossing with them costs (CROSS), so the way passes on the side of a row of boxes where it crosses least.
    // The segments already built, indexed once for every search of this edge (the obstacles' index is the edge's own).
    let assigned_index = BoxIndex::new(
        &assigned
            .iter()
            .map(|s| segment_box(s, track))
            .collect::<Vec<_>>(),
    );
    let empty_index = BoxIndex::new(&[]);
    let bare = Lookup {
        obstacles,
        obstacle_index,
        assigned: &[],
        assigned_index: &empty_index,
        crossed: assigned,
        crossed_index: &assigned_index,
    };
    let taken = Lookup {
        obstacles,
        obstacle_index,
        assigned,
        assigned_index: &assigned_index,
        crossed: assigned,
        crossed_index: &assigned_index,
    };
    let topology = search_on(
        (edge_id, "topology"),
        &bare,
        vertical,
        from,
        to,
        centres(&x_gaps),
        centres(&y_gaps),
        track,
        work,
    )?;
    // Then the tracks: every track of the gaps on a way, kept apart from the routes already built. Bound: in a corridor where
    // the corridor's order gives this edge a track, only that track.
    let tracks_on = |way: &[(f64, f64)], bound: bool| {
        let mut used_x: Vec<(f64, f64)> = way.iter().filter_map(|c| gap_of(&x_gaps, c.0)).collect();
        let mut used_y: Vec<(f64, f64)> = way.iter().filter_map(|c| gap_of(&y_gaps, c.1)).collect();
        used_x.dedup_by(|a, b| (a.0 - b.0).abs() < EPS);
        used_y.sort_by(|a, b| a.0.total_cmp(&b.0));
        used_y.dedup_by(|a, b| (a.0 - b.0).abs() < EPS);
        let xs: Vec<f64> = used_x
            .iter()
            .flat_map(|gap| match desired.get(&gap_key(*gap)).filter(|_| bound) {
                Some(x) => vec![*x],
                None => gap_tracks(gap.0, gap.1, track),
            })
            .collect();
        let ys: Vec<f64> = used_y
            .iter()
            .flat_map(|(lo, hi)| gap_tracks(*lo, *hi, track))
            .collect();
        search_on(
            (edge_id, if bound { "boundTracks" } else { "freeTracks" }),
            &taken,
            vertical,
            from,
            to,
            xs,
            ys,
            track,
            work,
        )
    };
    if !desired.is_empty() {
        if let Some(corners) = tracks_on(&topology, true) {
            return Some(corners);
        }
    }
    if let Some(corners) = tracks_on(&topology, false) {
        return Some(corners);
    }
    // The gaps of that way are full. A second way over the centres avoids the routes already built, so it passes other gaps;
    // its own centre lanes are free, and the tracks around them are tried first.
    let second = search_on(
        (edge_id, "alternateTopology"),
        &taken,
        vertical,
        from,
        to,
        centres(&x_gaps),
        centres(&y_gaps),
        track,
        work,
    )?;
    tracks_on(&second, false).or(Some(second))
}

/// What a search checks its steps against: the obstacles and the segments already built, each with its index.
struct Lookup<'a> {
    obstacles: &'a [Rect],
    obstacle_index: &'a BoxIndex,
    assigned: &'a [Segment],
    assigned_index: &'a BoxIndex,
    /// The segments whose crossings a step pays for (the segments already built), with their index.
    crossed: &'a [Segment],
    crossed_index: &'a BoxIndex,
}

/// The box a segment reaches with its track spacing: a segment closer than the spacing to it lies inside this box.
fn segment_box(s: &Segment, track: f64) -> [f64; 4] {
    if s.horizontal {
        [s.range.0, s.coord - track, s.range.1, s.coord + track]
    } else {
        [s.coord - track, s.range.0, s.coord + track, s.range.1]
    }
}

/// A uniform grid over boxes `[x0, y0, x1, y1]`: `any` tests only the boxes whose cells the query box touches, each once.
/// It finds every box that meets the query box (closed boxes), so a test over its candidates gives the same answer as a test
/// over all boxes.
struct BoxIndex {
    origin: (f64, f64),
    cell: f64,
    cols: usize,
    rows: usize,
    cells: Vec<Vec<u32>>,
    seen: std::cell::RefCell<(Vec<u32>, u32)>,
}

impl BoxIndex {
    fn new(boxes: &[[f64; 4]]) -> Self {
        if boxes.is_empty() {
            return BoxIndex {
                origin: (0., 0.),
                cell: 1.,
                cols: 0,
                rows: 0,
                cells: Vec::new(),
                seen: std::cell::RefCell::new((Vec::new(), 0)),
            };
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for b in boxes {
            x0 = x0.min(b[0]);
            y0 = y0.min(b[1]);
            x1 = x1.max(b[2]);
            y1 = y1.max(b[3]);
        }
        let side = ((boxes.len() as f64).sqrt().ceil() as usize).clamp(1, 64) as f64;
        let cell = ((x1 - x0) / side).max((y1 - y0) / side).max(1.);
        let cols = ((x1 - x0) / cell).floor() as usize + 1;
        let rows = ((y1 - y0) / cell).floor() as usize + 1;
        let mut index = BoxIndex {
            origin: (x0, y0),
            cell,
            cols,
            rows,
            cells: vec![Vec::new(); cols * rows],
            seen: std::cell::RefCell::new((vec![0; boxes.len()], 0)),
        };
        for (i, b) in boxes.iter().enumerate() {
            let (c0, c1, r0, r1) = index.cover(b);
            for r in r0..=r1 {
                for c in c0..=c1 {
                    index.cells[r * cols + c].push(i as u32);
                }
            }
        }
        index
    }

    /// The cell range a box covers, clamped to the grid.
    fn cover(&self, b: &[f64; 4]) -> (usize, usize, usize, usize) {
        let at =
            |v: f64, o: f64, n: usize| (((v - o) / self.cell).floor().max(0.) as usize).min(n - 1);
        (
            at(b[0], self.origin.0, self.cols),
            at(b[2], self.origin.0, self.cols),
            at(b[1], self.origin.1, self.rows),
            at(b[3], self.origin.1, self.rows),
        )
    }

    /// Whether `test` holds for a box whose cell the query box touches (each box is tested at most once).
    fn any(&self, query: [f64; 4], mut test: impl FnMut(usize) -> bool) -> bool {
        if self.cells.is_empty() {
            return false;
        }
        let (c0, c1, r0, r1) = self.cover(&query);
        let mut seen = self.seen.borrow_mut();
        let (stamps, epoch) = &mut *seen;
        *epoch = epoch.wrapping_add(1);
        if *epoch == 0 {
            stamps.iter_mut().for_each(|s| *s = 0);
            *epoch = 1;
        }
        for r in r0..=r1 {
            for c in c0..=c1 {
                for &i in &self.cells[r * self.cols + c] {
                    let i = i as usize;
                    if stamps[i] == *epoch {
                        continue;
                    }
                    stamps[i] = *epoch;
                    if test(i) {
                        return true;
                    }
                }
            }
        }
        false
    }
}

/// The cheapest monotone path from `from` to `to` over the given tracks along the flow (`xs`) and across it (`ys`; the two
/// port lanes are added). See `monotone_path`.
#[allow(clippy::too_many_arguments)]
fn search_on(
    context: (&str, &'static str),
    lookup: &Lookup,
    vertical: bool,
    from: (f64, f64),
    to: (f64, f64),
    xs: Vec<f64>,
    ys: Vec<f64>,
    track: f64,
    work: &RefCell<ConstructionCounters>,
) -> Option<Vec<(f64, f64)>> {
    let started = crate::stage_timer_start();
    let mut counted = ConstructionCounters::default();
    counted.searches = 1;
    let result = search_on_counted(lookup, vertical, from, to, xs, ys, track, &mut counted);
    counted.failed_searches = usize::from(result.is_none());
    counted.search_nanos = crate::elapsed_nanos(started);
    #[cfg(feature = "benchmark-timings")]
    counted
        .search_details
        .push(arqlet_routing::channel::ConstructionSearch {
            edge: context.0.to_string(),
            phase: context.1,
            failed: result.is_none(),
            states: counted.states,
            transitions: counted.transitions,
        });
    #[cfg(not(feature = "benchmark-timings"))]
    let _ = context;
    work.borrow_mut().add(&counted);
    result
}

#[allow(clippy::too_many_arguments)]
fn search_on_counted(
    lookup: &Lookup,
    vertical: bool,
    from: (f64, f64),
    to: (f64, f64),
    mut xs: Vec<f64>,
    mut ys: Vec<f64>,
    track: f64,
    work: &mut ConstructionCounters,
) -> Option<Vec<(f64, f64)>> {
    // A step is clear when it misses every obstacle and keeps the track spacing from every assigned segment. The indexes
    // hand over only the obstacles and segments whose boxes touch the step's box; the tests are the same as on all of them.
    let clear = |a: (f64, f64), b: (f64, f64)| {
        let (pa, pb) = (to_point(vertical, a), to_point(vertical, b));
        let reach = [
            pa.x.min(pb.x) - EPS,
            pa.y.min(pb.y) - EPS,
            pa.x.max(pb.x) + EPS,
            pa.y.max(pb.y) + EPS,
        ];
        if lookup.obstacle_index.any(reach, |i| {
            segment_hits(pa.clone(), pb.clone(), &lookup.obstacles[i])
        }) {
            return false;
        }
        segments_of(&[pa.clone(), pb.clone()]).iter().all(|step| {
            !lookup
                .assigned_index
                .any(reach, |i| conflicts(&lookup.assigned[i], step, track))
        })
    };
    xs.retain(|&x| x > from.0 + EPS && x < to.0 - EPS);
    xs.sort_by(f64::total_cmp);
    xs.dedup_by(|a, b| (*a - *b).abs() < EPS);
    ys.push(from.1);
    ys.push(to.1);
    ys.sort_by(f64::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < EPS);
    let lane_of = |y: f64| ys.iter().position(|&v| (v - y).abs() < EPS);
    let (start_lane, end_lane) = (lane_of(from.1)?, lane_of(to.1)?);
    if xs.is_empty() {
        // No corridor: one straight segment, when the two ports share a lane.
        return (start_lane == end_lane && clear(from, to)).then(Vec::new);
    }
    let n = xs.len();
    let m = ys.len();
    work.states += n * m;
    let inf = f64::INFINITY;
    // For each corridor column, the cross-flow coordinates of the built segments that run along the flow through it, sorted:
    // the crossings of a lane change there are those strictly between its two lanes.
    let column_crossings: Vec<Vec<f64>> = xs
        .iter()
        .map(|&x| {
            let (lo, hi) = (ys[0], ys[m - 1]);
            let (pa, pb) = (to_point(vertical, (x, lo)), to_point(vertical, (x, hi)));
            let reach = [
                pa.x.min(pb.x) - EPS,
                pa.y.min(pb.y) - EPS,
                pa.x.max(pb.x) + EPS,
                pa.y.max(pb.y) + EPS,
            ];
            let mut found = Vec::new();
            if let Some(column) = segments_of(&[pa.clone(), pb.clone()]).first() {
                lookup.crossed_index.any(reach, |i| {
                    let s = &lookup.crossed[i];
                    if s.horizontal != column.horizontal
                        && column.coord > s.range.0 + EPS
                        && column.coord < s.range.1 - EPS
                    {
                        found.push(s.coord);
                    }
                    false
                });
            }
            found.sort_by(f64::total_cmp);
            found
        })
        .collect();
    // Likewise for each lane: the coordinates along the flow of the built segments that run across it.
    let lane_crossings: Vec<Vec<f64>> = ys
        .iter()
        .map(|&y| {
            let (lo, hi) = (from.0.min(xs[0]), to.0.max(xs[n - 1]));
            let (pa, pb) = (to_point(vertical, (lo, y)), to_point(vertical, (hi, y)));
            let reach = [
                pa.x.min(pb.x) - EPS,
                pa.y.min(pb.y) - EPS,
                pa.x.max(pb.x) + EPS,
                pa.y.max(pb.y) + EPS,
            ];
            let mut found = Vec::new();
            if let Some(lane) = segments_of(&[pa.clone(), pb.clone()]).first() {
                lookup.crossed_index.any(reach, |i| {
                    let s = &lookup.crossed[i];
                    if s.horizontal != lane.horizontal
                        && lane.coord > s.range.0 + EPS
                        && lane.coord < s.range.1 - EPS
                    {
                        found.push(s.coord);
                    }
                    false
                });
            }
            found.sort_by(f64::total_cmp);
            found
        })
        .collect();
    let between = |sorted: &[f64], a: f64, b: f64| {
        let (lo, hi) = (a.min(b), a.max(b));
        let first = sorted.partition_point(|&c| c <= lo + EPS);
        let last = sorted.partition_point(|&c| c < hi - EPS);
        last.saturating_sub(first) as f64 * CROSS
    };
    let mut arrive = vec![vec![inf; m]; n];
    let mut depart = vec![vec![inf; m]; n];
    // The lane a corridor's departure lane was entered on (the same lane when there was no change).
    let mut entered = vec![vec![usize::MAX; m]; n];
    if clear(from, (xs[0], from.1)) {
        arrive[0][start_lane] =
            xs[0] - from.0 + between(&lane_crossings[start_lane], from.0, xs[0]);
    }
    for k in 0..n {
        for j in 0..m {
            if arrive[k][j] < inf {
                depart[k][j] = arrive[k][j];
                entered[k][j] = j;
            }
        }
        // The one change of lane inside corridor k: from a reached lane i, scan outwards while the span stays clear. A span
        // that meets an obstacle meets it again on every longer span, so the scan stops there; and a span is clear when each
        // piece between neighbouring lanes is, so only the new piece is tested.
        // Whether the piece between lane l and lane l + 1 is clear, tested once per corridor.
        let piece: Vec<bool> = if (0..m).any(|i| arrive[k][i] < inf) {
            (0..m - 1)
                .map(|l| clear((xs[k], ys[l]), (xs[k], ys[l + 1])))
                .collect()
        } else {
            Vec::new()
        };
        for i in 0..m {
            if arrive[k][i] == inf {
                continue;
            }
            let column = &column_crossings[k];
            for step in [-1isize, 1] {
                // The crossings strictly between lane i and the scanned lane, as `between` counts them on the column: the
                // bound at lane i stays, the other moves outwards with the scan, so its partition point only moves one way.
                let (mut first, mut last) = if step > 0 {
                    (column.partition_point(|&c| c <= ys[i] + EPS), 0)
                } else {
                    (column.len(), column.partition_point(|&c| c < ys[i] - EPS))
                };
                let mut j = i as isize + step;
                while j >= 0 && (j as usize) < m {
                    let lane = j as usize;
                    let previous = (j - step) as usize;
                    if !piece[lane.min(previous)] {
                        break;
                    }
                    if step > 0 {
                        while last < column.len() && column[last] < ys[lane] - EPS {
                            last += 1;
                        }
                    } else {
                        while first > 0 && column[first - 1] > ys[lane] + EPS {
                            first -= 1;
                        }
                    }
                    let cost = arrive[k][i]
                        + (ys[lane] - ys[i]).abs()
                        + BEND
                        + last.saturating_sub(first) as f64 * CROSS;
                    work.transitions += 1;
                    if cost < depart[k][lane] {
                        depart[k][lane] = cost;
                        entered[k][lane] = i;
                    }
                    j += step;
                }
            }
        }
        if k + 1 < n {
            for j in 0..m {
                if depart[k][j] < inf && clear((xs[k], ys[j]), (xs[k + 1], ys[j])) {
                    let cost = depart[k][j]
                        + (xs[k + 1] - xs[k])
                        + between(&lane_crossings[j], xs[k], xs[k + 1]);
                    if cost < arrive[k + 1][j] {
                        arrive[k + 1][j] = cost;
                    }
                }
            }
        }
    }
    let last = n - 1;
    if depart[last][end_lane] == inf || !clear((xs[last], to.1), to) {
        return None;
    }
    // Walk back from the end lane. Every change of lane is a corner at the corridor's column.
    let mut corners: Vec<(f64, f64)> = Vec::new();
    let mut lane = end_lane;
    for k in (0..n).rev() {
        let before = entered[k][lane];
        if before != lane {
            corners.push((xs[k], ys[lane]));
            corners.push((xs[k], ys[before]));
            lane = before;
        }
    }
    if lane != start_lane {
        return None;
    }
    corners.reverse();
    Some(corners)
}

/// The extent of a box across the flow: its y range for a left-to-right flow, its x range for a top-to-bottom flow.
fn across_extent(r: &Rect, vertical: bool) -> (f64, f64) {
    if vertical {
        (r.x, r.x + r.width)
    } else {
        (r.y, r.y + r.height)
    }
}

/// Whether a new segment runs too close to an assigned one, parallel to it, over an overlapping range.
fn conflicts(a: &Segment, b: &Segment, track: f64) -> bool {
    a.horizontal == b.horizontal
        && (a.coord - b.coord).abs() < track - EPS
        && a.range.0 < b.range.1
        && b.range.0 < a.range.1
}

/// Drops repeated points and the points that lie between two others on one line, so a route has a bend only where it turns.
fn straighten(points: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if let Some(last) = out.last() {
            if (last.x - p.x).abs() < EPS && (last.y - p.y).abs() < EPS {
                continue;
            }
        }
        out.push(p);
        while out.len() >= 3 {
            let n = out.len();
            let (a, b, c) = (&out[n - 3], &out[n - 2], &out[n - 1]);
            let on_line = ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs() < EPS;
            if !on_line {
                break;
            }
            out.remove(n - 2);
        }
    }
    out
}

/// The flow of an area: `Some(false)` for left-to-right, `Some(true)` for top-to-bottom, `None` for anything else. A free
/// or allowed direction takes the orientation the layout chose for the area (`orientations`, keyed like `area_orientations`).
pub(crate) fn flow_of(
    graph: &Graph,
    parent: Option<&str>,
    orientations: &BTreeMap<String, Orientation>,
) -> Option<bool> {
    let direction = match parent {
        Some(id) => graph
            .container_directions
            .get(id)
            .unwrap_or(&graph.default_direction),
        None => &graph.default_direction,
    };
    let orientation = match direction {
        DirectionConstraint::Fixed(orientation) => *orientation,
        DirectionConstraint::Free | DirectionConstraint::Allowed(_) => {
            *orientations.get(parent.unwrap_or("<root>"))?
        }
    };
    match orientation {
        Orientation::Lr => Some(false),
        Orientation::Tb => Some(true),
        _ => None,
    }
}

/// The ports of the edges that the construction can take, moved onto the sides the flow leaves and enters through: right
/// and left for a left-to-right area, bottom and top for a top-to-bottom one. The ports of one side are spread evenly over
/// it in the order of the other endpoints. Every other port is kept. This is a ceiling experiment: it replaces the
/// automatic ports, so it is valid only for drawings without explicit ports.
pub fn flow_ports(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    orientations: &BTreeMap<String, Orientation>,
) -> Vec<ResolvedPort> {
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    // (node, side) -> (position of the other endpoint across the flow, edge id, endpoint, side)
    let mut groups: BTreeMap<(&str, u8), Vec<(f64, &str, &str, PortSide)>> = BTreeMap::new();
    for edge in &graph.edges {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        if edge.source == target
            || containers.contains(edge.source.as_str())
            || containers.contains(target)
            || !boxes.contains_key(&edge.source)
            || !boxes.contains_key(target)
        {
            continue;
        }
        let parent = parents.get(edge.source.as_str()).copied().flatten();
        if parents.get(target).copied().flatten() != parent {
            continue;
        }
        let Some(vertical) = flow_of(graph, parent, orientations) else {
            continue;
        };
        let (out_side, in_side) = if vertical {
            (PortSide::Bottom, PortSide::Top)
        } else {
            (PortSide::Right, PortSide::Left)
        };
        for (node, other, side, endpoint) in [
            (edge.source.as_str(), target, out_side, "source"),
            (target, edge.source.as_str(), in_side, "target"),
        ] {
            let across = across_centre(&boxes[other], vertical);
            groups.entry((node, side_rank(side))).or_default().push((
                across,
                edge.id.as_str(),
                endpoint,
                side,
            ));
        }
    }
    let mut out = ports.to_vec();
    for ((node, _), mut members) in groups {
        members.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(b.1)));
        let r = &boxes[node];
        let n = members.len() as f64;
        for (i, (_, edge_id, endpoint, side)) in members.into_iter().enumerate() {
            let f = (i as f64 + 1.) / (n + 1.);
            let position = match side {
                PortSide::Right => Point {
                    x: r.x + r.width,
                    y: r.y + r.height * f,
                },
                PortSide::Left => Point {
                    x: r.x,
                    y: r.y + r.height * f,
                },
                PortSide::Bottom => Point {
                    x: r.x + r.width * f,
                    y: r.y + r.height,
                },
                PortSide::Top => Point {
                    x: r.x + r.width * f,
                    y: r.y,
                },
            };
            if let Some(port) = out
                .iter_mut()
                .find(|p| p.edge_id == edge_id && p.endpoint == endpoint)
            {
                port.side = side;
                port.position = position;
            }
        }
    }
    out
}

fn side_rank(side: PortSide) -> u8 {
    match side {
        PortSide::Left => 0,
        PortSide::Right => 1,
        PortSide::Top => 2,
        PortSide::Bottom => 3,
    }
}

/// The centre of a box across the flow: its y for a left-to-right flow, its x for a top-to-bottom one.
fn across_centre(r: &Rect, vertical: bool) -> f64 {
    if vertical {
        r.x + r.width / 2.
    } else {
        r.y + r.height / 2.
    }
}

/// The point's coordinates in the flow's frame: (main, across). Left-to-right keeps x as main; top-to-bottom swaps them.
fn frame(vertical: bool, p: &Point) -> (f64, f64) {
    if vertical {
        (p.y, p.x)
    } else {
        (p.x, p.y)
    }
}

fn to_point(vertical: bool, (main, across): (f64, f64)) -> Point {
    if vertical {
        Point { x: across, y: main }
    } else {
        Point { x: main, y: across }
    }
}

fn inflate(r: &Rect, by: f64) -> Rect {
    Rect {
        x: r.x - by,
        y: r.y - by,
        width: r.width + 2. * by,
        height: r.height + 2. * by,
    }
}

/// The free gaps between `from` and `to` on one axis: the intervals between the boxes' edges that are wide enough for a track.
fn free_gaps(
    obstacles: &[Rect],
    vertical: bool,
    from: f64,
    to: f64,
    track: f64,
) -> Vec<(f64, f64)> {
    let mut edges = vec![from, to];
    for r in obstacles {
        let (start, length) = if vertical {
            (r.y, r.height)
        } else {
            (r.x, r.width)
        };
        for e in [start, start + length] {
            if e > from + EPS && e < to - EPS {
                edges.push(e);
            }
        }
    }
    edges.sort_by(f64::total_cmp);
    edges.dedup_by(|a, b| (*a - *b).abs() < EPS);
    edges
        .windows(2)
        .filter(|w| w[1] - w[0] > 2. * track)
        .map(|w| (w[0], w[1]))
        .collect()
}

/// The most tracks one free gap offers. Bounds the search on very wide gaps; a gap rarely needs more.
const MAX_TRACKS_PER_GAP: usize = 32;

/// The tracks of one free gap between `lo` and `hi`: its centre first, then outwards at the track spacing on both sides, as
/// many as fit strictly inside the gap. One track per gap would let only one route pass a corridor that the placement widened
/// for several.
fn gap_tracks(lo: f64, hi: f64, track: f64) -> Vec<f64> {
    let centre = (lo + hi) / 2.;
    let mut tracks = vec![centre];
    let mut k = 1.;
    while tracks.len() < MAX_TRACKS_PER_GAP {
        let (below, above) = (centre - k * track, centre + k * track);
        let fits_below = below > lo + EPS;
        let fits_above = above < hi - EPS;
        if !fits_below && !fits_above {
            break;
        }
        if fits_below {
            tracks.push(below);
        }
        if fits_above && tracks.len() < MAX_TRACKS_PER_GAP {
            tracks.push(above);
        }
        k += 1.;
    }
    tracks
}

/// Whether an axis-aligned segment passes through the interior of `r`.
fn segment_hits(a: Point, b: Point, r: &Rect) -> bool {
    let (x0, x1) = (a.x.min(b.x), a.x.max(b.x));
    let (y0, y1) = (a.y.min(b.y), a.y.max(b.y));
    x1 > r.x + EPS && x0 < r.x + r.width - EPS && y1 > r.y + EPS && y0 < r.y + r.height - EPS
}

type Parents<'a> = BTreeMap<&'a str, Option<&'a str>>;

fn endpoint_exclusions<'a>(
    parents: &Parents<'a>,
    source: &'a str,
    target: &'a str,
) -> BTreeSet<&'a str> {
    let mut skip = BTreeSet::new();
    for endpoint in [source, target] {
        let mut at = Some(endpoint);
        while let Some(id) = at {
            skip.insert(id);
            at = parents.get(id).copied().flatten();
        }
    }
    skip
}

fn parents_of(graph: &Graph) -> Parents<'_> {
    graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect()
}

fn containers_of(graph: &Graph) -> BTreeSet<&str> {
    graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect()
}

/// The routes from the plans' sections, by slot, the slots whose sections cross an area, and the narrow channels.
type Sections = (
    BTreeMap<usize, Vec<(f64, f64)>>,
    BTreeSet<usize>,
    Vec<(String, usize, f64)>,
);

/// The first part of a construction: every edge of `indices` (in that order) checked and its ends prepared, then the routes
/// from the plans' sections.
#[allow(clippy::too_many_arguments)]
fn prepare_all(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    spacing: Spacing,
    orientations: &BTreeMap<String, Orientation>,
    indices: Vec<usize>,
    plans: Option<&LayeredPlans>,
    only: Option<&BTreeSet<String>>,
    (parents, containers): (&Parents<'_>, &BTreeSet<&str>),
    work: &RefCell<ConstructionCounters>,
) -> (Vec<(usize, Result<Prepared, Delegation>)>, Sections) {
    let started = crate::stage_timer_start();
    // Every edge is checked, then its way is planned over the gaps' centres. The lane changes of all edges in one corridor
    // are ordered so that they cross least, and every edge is bound to the track of its place in that order.
    let port_ends = crate::ports::PortEnds::new(ports);
    let prepared: Vec<(usize, Result<Prepared, Delegation>)> = indices
        .into_iter()
        .filter_map(|index| {
            let edge = &graph.edges[index];
            if only.is_some_and(|only| !only.contains(&edge.id)) {
                return None;
            }
            let target = edge.target.as_deref()?;
            let outcome = if edge.source == target {
                Err(Delegation::SelfLoop)
            } else {
                prepare_edge(
                    graph,
                    boxes,
                    &port_ends,
                    parents,
                    containers,
                    spacing,
                    orientations,
                    &edge.id,
                    &edge.source,
                    target,
                    true,
                )
            };
            Some((index, outcome))
        })
        .collect();
    // The routes from the sections of the plans (step 5), by slot, as corners in the flow's frame.
    let sections = plans
        .map(|plans| section_routes(graph, boxes, &prepared, plans, spacing))
        .unwrap_or_default();
    let mut counted = work.borrow_mut();
    counted.preparations += 1;
    counted.prepared_edges += prepared.len();
    counted.preparation_nanos = counted
        .preparation_nanos
        .saturating_add(crate::elapsed_nanos(started));
    (prepared, sections)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arqlet_model::{Edge, Node, NodeShape};

    const SPACING: Spacing = Spacing {
        margin: 12.,
        track: 6.,
        through_borders: false,
        probe: false,
        port_sides: false,
    };

    fn node(id: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            label: id.into(),
            kind: "symbol".into(),
            parent_id: parent.map(Into::into),
            shape: NodeShape::default(),
        }
    }

    fn graph(nodes: Vec<Node>, edges: Vec<(&str, &str, &str)>) -> Graph {
        Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes,
            edges: edges
                .into_iter()
                .map(|(id, s, t)| Edge {
                    id: id.into(),
                    kind: "calls".into(),
                    source: s.into(),
                    target: Some(t.into()),
                    weight: None,
                })
                .collect(),
            buses: Vec::new(),
        }
    }

    fn rect(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 40.,
            height: 40.,
        }
    }

    #[test]
    fn constructor_search_and_probe_work_are_counted_once() {
        let g = graph(
            vec![node("a", None), node("b", None)],
            vec![("e", "a", "b")],
        );
        let boxes = BTreeMap::from([("a".into(), rect(0., 0.)), ("b".into(), rect(160., 0.))]);
        let ports = ports("e", &boxes["a"], &boxes["b"]);
        let plans = LayeredPlans::new();
        let orientations = BTreeMap::new();
        let prepared = prepare_planned(&g, &boxes, &ports, SPACING, &orientations, &plans);
        assert_eq!(prepared.work.preparations, 1);
        assert_eq!(prepared.work.prepared_edges, 1);
        assert_eq!(prepared.work.searches, 0);
        let mut probe_work = prepared.work.clone();
        let reused = construct_planned_from(
            &g,
            &boxes,
            &ports,
            SPACING,
            &orientations,
            &plans,
            Some(prepared),
        );
        let full = construct_planned(&g, &boxes, &ports, SPACING, &orientations, &plans);
        assert_eq!(
            serde_json::to_vec(&reused.routes).unwrap(),
            serde_json::to_vec(&full.routes).unwrap()
        );
        assert_eq!(reused.work.preparations, 0);
        assert_eq!(full.work.edge_searches, 1);
        assert_eq!(full.work.lane_searches, 1);
        assert!(full.work.searches >= 2);
        assert!(full.work.states > 0);
        probe_work.add(&reused.work);
        let mut full_work = full.work.clone();
        for work in [&mut probe_work, &mut full_work] {
            work.preparation_nanos = 0;
            work.search_nanos = 0;
        }
        assert_eq!(probe_work, full_work);
    }

    #[test]
    fn monotone_search_counts_cells_transitions_and_failures() {
        let empty = BoxIndex::new(&[]);
        let lookup = Lookup {
            obstacles: &[],
            obstacle_index: &empty,
            assigned: &[],
            assigned_index: &empty,
            crossed: &[],
            crossed_index: &empty,
        };
        let work = RefCell::new(ConstructionCounters::default());
        assert!(search_on(
            ("test", "test"),
            &lookup,
            false,
            (0., 0.),
            (100., 10.),
            vec![50.],
            vec![],
            6.,
            &work,
        )
        .is_some());
        assert!(search_on(
            ("test", "test"),
            &lookup,
            false,
            (0., 0.),
            (100., 10.),
            vec![],
            vec![],
            6.,
            &work,
        )
        .is_none());
        let work = work.borrow();
        assert_eq!(work.searches, 2);
        assert_eq!(work.failed_searches, 1);
        assert_eq!(work.states, 2);
        assert_eq!(work.transitions, 1);
    }

    #[test]
    fn planned_failures_distinguish_obstacles_from_occupied_tracks() {
        let g = graph(
            vec![node("a", None), node("b", None)],
            vec![("e", "a", "b")],
        );
        let boxes = BTreeMap::from([("a".into(), rect(0., 0.)), ("b".into(), rect(160., 0.))]);
        let ports = ports("e", &boxes["a"], &boxes["b"]);
        let preparation = prepare_planned(
            &g,
            &boxes,
            &ports,
            SPACING,
            &BTreeMap::new(),
            &LayeredPlans::new(),
        );
        let mut p = preparation.prepared[0].1.as_ref().unwrap().clone();
        let work = RefCell::new(ConstructionCounters::default());
        let mut assigned = vec![Segment {
            horizontal: true,
            coord: p.start.y,
            range: (p.start.x, p.end.x),
        }];
        assert!(build_planned(&p, &[], &mut assigned, SPACING.track, &work).is_none());
        p.obstacles.push(rect(80., 0.));
        assert!(build_planned(&p, &[], &mut Vec::new(), SPACING.track, &work).is_none());
        assert_eq!(work.borrow().planned_attempts, 2);
        assert_eq!(work.borrow().planned_track_failures, 1);
        assert_eq!(work.borrow().planned_obstacle_failures, 1);
        assert_eq!(work.borrow().searches, 0);
    }

    fn reference_dependent_order(changes: &[(f64, f64)], track: f64) -> Vec<usize> {
        const CONFLICT: i64 = 1_000;
        let n = changes.len();
        let inside = |v: f64, (a, b): (f64, f64)| v > a.min(b) + EPS && v < a.max(b) - EPS;
        let first = |a: (f64, f64), b: (f64, f64)| -> i64 {
            i64::from(inside(a.1, b))
                + i64::from(inside(b.0, a))
                + if (a.1 - b.0).abs() < track - EPS {
                    CONFLICT
                } else {
                    0
                }
        };
        let cost: Vec<Vec<i64>> = (0..n)
            .map(|i| {
                (0..n)
                    .map(|j| {
                        if i == j {
                            0
                        } else {
                            first(changes[i], changes[j])
                        }
                    })
                    .collect()
            })
            .collect();
        let mut left = corridor_order(changes);
        let mut order = Vec::with_capacity(n);
        while !left.is_empty() {
            let (k, _) = left
                .iter()
                .enumerate()
                .map(|(k, &i)| {
                    let balance: i64 = left.iter().map(|&j| cost[i][j] - cost[j][i]).sum();
                    (k, balance)
                })
                .min_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)))
                .unwrap();
            order.push(left.remove(k));
        }
        let mut improved = true;
        let mut rounds = 0;
        while improved && rounds < 8 {
            improved = false;
            rounds += 1;
            for i in 0..n {
                let at = order.iter().position(|&x| x == i).unwrap();
                let mut rest = order.clone();
                rest.remove(at);
                let mut c: i64 = rest.iter().map(|&j| cost[i][j]).sum();
                let (mut best, mut best_place) = (c, 0);
                for (p, &j) in rest.iter().enumerate() {
                    c += cost[j][i] - cost[i][j];
                    if c < best {
                        best = c;
                        best_place = p + 1;
                    }
                }
                let now: i64 = rest[..at].iter().map(|&j| cost[j][i]).sum::<i64>()
                    + rest[at..].iter().map(|&j| cost[i][j]).sum::<i64>();
                if best < now {
                    rest.insert(best_place, i);
                    order = rest;
                    improved = true;
                }
            }
        }
        order
    }

    fn reference_track_slots(
        changes: &[(f64, f64)],
        order: &[usize],
        track: f64,
    ) -> (Vec<usize>, usize) {
        let span = |(a, b): (f64, f64)| (a.min(b), a.max(b));
        let mut slots = vec![0; changes.len()];
        let mut count = 0;
        for (k, &i) in order.iter().enumerate() {
            let (lo, hi) = span(changes[i]);
            let slot = order[..k]
                .iter()
                .filter(|&&j| {
                    let (l, h) = span(changes[j]);
                    l < hi + track - EPS && lo < h + track - EPS
                })
                .map(|&j| slots[j] + 1)
                .max()
                .unwrap_or(0);
            slots[i] = slot;
            count = count.max(slot + 1);
        }
        (slots, count)
    }

    fn next_random(state: &mut u64) -> u64 {
        *state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        *state
    }

    #[test]
    fn incremental_dependent_order_matches_the_previous_selection() {
        let mut state = 0x4d59_5df4_d0f3_3173;
        for n in 1..=64 {
            for _ in 0..4 {
                let changes: Vec<_> = (0..n)
                    .map(|_| {
                        let a = (next_random(&mut state) % 31) as f64 * 6.;
                        let b = (next_random(&mut state) % 31) as f64 * 6.;
                        (a, b)
                    })
                    .collect();
                assert_eq!(
                    dependent_order(&changes, 6.),
                    reference_dependent_order(&changes, 6.),
                    "changes: {changes:?}"
                );
            }
        }
    }

    #[test]
    fn indexed_track_slots_match_the_previous_interval_scan() {
        let mut state = 0xa076_1d64_78bd_642f;
        let tracks = [0., EPS, 6. - EPS, 6., 6. + EPS, 12.];
        let boundary_changes = [
            (0., 0.),
            (0., 10.),
            (10., 20.),
            (10. + EPS, 20. + EPS),
            (20., 10.),
            (10. + 6. - EPS, 30.),
            (30., 10. + 6. - EPS),
        ];
        let boundary_order: Vec<_> = (0..boundary_changes.len()).collect();
        for &track in &tracks {
            assert_eq!(
                track_slots(&boundary_changes, &boundary_order, track),
                reference_track_slots(&boundary_changes, &boundary_order, track)
            );
        }
        let mut large_boundaries = boundary_changes.to_vec();
        large_boundaries.extend((0..513 - large_boundaries.len()).map(|i| {
            let lo = (i % 41) as f64 * 3.;
            (lo, lo + (i % 7) as f64 * 6.)
        }));
        let large_order: Vec<_> = (0..large_boundaries.len()).rev().collect();
        for &track in &tracks {
            assert_eq!(
                track_slots(&large_boundaries, &large_order, track),
                reference_track_slots(&large_boundaries, &large_order, track)
            );
        }
        for n in (0..=96).chain([513, 640]) {
            for &track in &tracks {
                let changes: Vec<_> = (0..n)
                    .map(|_| {
                        let a = (next_random(&mut state) % 43) as f64 * 3.;
                        let b = (next_random(&mut state) % 43) as f64 * 3.;
                        (a, b)
                    })
                    .collect();
                let mut order: Vec<_> = (0..n).collect();
                for i in (1..n).rev() {
                    let j = next_random(&mut state) as usize % (i + 1);
                    order.swap(i, j);
                }
                assert_eq!(
                    track_slots(&changes, &order, track),
                    reference_track_slots(&changes, &order, track),
                    "track {track}, changes {changes:?}, order {order:?}"
                );
            }
        }
    }

    /// The ports of an edge at the middle of the facing sides of two boxes.
    fn ports(edge: &str, s: &Rect, t: &Rect) -> Vec<ResolvedPort> {
        let port = |endpoint: &str, side: PortSide, x: f64, y: f64| ResolvedPort {
            id: format!("{edge}:{endpoint}"),
            node_id: String::new(),
            edge_id: edge.into(),
            endpoint: endpoint.into(),
            side,
            position: Point { x, y },
            explicit: false,
        };
        vec![
            port(
                "source",
                PortSide::Right,
                s.x + s.width,
                s.y + s.height / 2.,
            ),
            port("target", PortSide::Left, t.x, t.y + t.height / 2.),
        ]
    }

    #[test]
    fn a_straight_edge_between_neighbours_is_one_segment() {
        let g = graph(
            vec![node("a", None), node("b", None)],
            vec![("e", "a", "b")],
        );
        let boxes = BTreeMap::from([("a".into(), rect(0., 0.)), ("b".into(), rect(100., 0.))]);
        let p = ports("e", &boxes["a"], &boxes["b"]);
        let c = construct(&g, &boxes, &p, SPACING, &BTreeMap::new());
        assert!(c.delegated.is_empty());
        assert_eq!(c.routes[0].points.len(), 2);
        assert_eq!(c.routes[0].points[0].x, 40.);
        assert_eq!(c.routes[0].points[1].x, 100.);
    }

    #[test]
    fn a_blocker_on_the_port_lane_is_gone_around_by_a_change_of_lane() {
        // The blocker covers the port lane between the nodes but leaves the space below it free, so the route changes
        // lane once, in the corridor between the nodes.
        let g = graph(
            vec![node("a", None), node("c", None), node("b", None)],
            vec![("e", "a", "b")],
        );
        let boxes = BTreeMap::from([
            ("a".into(), rect(0., 0.)),
            (
                "c".into(),
                Rect {
                    x: 90.,
                    y: -30.,
                    width: 20.,
                    height: 100.,
                },
            ),
            ("b".into(), rect(160., 0.)),
        ]);
        let p = ports("e", &boxes["a"], &boxes["b"]);
        let c = construct(&g, &boxes, &p, SPACING, &BTreeMap::new());
        assert!(c.delegated.is_empty(), "{:?}", c.delegated);
        assert_eq!(c.routes.len(), 1);
        assert!(c.lane_changes >= 1);
    }

    #[test]
    fn a_long_edge_keeps_clear_of_every_box_it_passes() {
        // Two ranks sit between the endpoints; each has a box on the port lane, at different heights, so the route changes
        // lane in each corridor.
        let g = graph(
            vec![
                node("a", None),
                node("m1", None),
                node("m2", None),
                node("b", None),
            ],
            vec![("e", "a", "b")],
        );
        let boxes = BTreeMap::from([
            ("a".into(), rect(0., 0.)),
            (
                "m1".into(),
                Rect {
                    x: 100.,
                    y: -40.,
                    width: 40.,
                    height: 60.,
                },
            ),
            (
                "m2".into(),
                Rect {
                    x: 200.,
                    y: 10.,
                    width: 40.,
                    height: 80.,
                },
            ),
            ("b".into(), rect(300., 0.)),
        ]);
        let p = ports("e", &boxes["a"], &boxes["b"]);
        let c = construct(&g, &boxes, &p, SPACING, &BTreeMap::new());
        assert!(c.delegated.is_empty(), "{:?}", c.delegated);
        let route = &c.routes[0];
        assert!(c.lane_changes >= 2, "{:?}", route.points);
        for w in route.points.windows(2) {
            for (id, r) in &boxes {
                if id == "a" || id == "b" {
                    continue;
                }
                assert!(
                    !segment_hits(w[0].clone(), w[1].clone(), r),
                    "{id} is crossed by {:?}",
                    route.points
                );
            }
        }
    }

    #[test]
    fn flow_ports_spread_the_edges_of_one_side_evenly() {
        let g = graph(
            vec![node("a", None), node("b", None), node("c", None)],
            vec![("e1", "a", "b"), ("e2", "a", "c")],
        );
        let boxes = BTreeMap::from([
            ("a".into(), rect(0., 0.)),
            ("b".into(), rect(100., -60.)),
            ("c".into(), rect(100., 60.)),
        ]);
        let mut p = ports("e1", &boxes["a"], &boxes["b"]);
        p.extend(ports("e2", &boxes["a"], &boxes["c"]));
        let flow = flow_ports(&g, &boxes, &p, &BTreeMap::new());
        let out_of_a: Vec<f64> = flow
            .iter()
            .filter(|q| q.node_id.is_empty() && q.endpoint == "source")
            .map(|q| q.position.y)
            .collect();
        assert_eq!(out_of_a.len(), 2);
        // The edge to the upper box leaves from the upper slot.
        let upper = flow
            .iter()
            .find(|q| q.edge_id == "e1" && q.endpoint == "source")
            .unwrap();
        let lower = flow
            .iter()
            .find(|q| q.edge_id == "e2" && q.endpoint == "source")
            .unwrap();
        assert!(upper.position.y < lower.position.y, "{upper:?} {lower:?}");
        assert!(upper.position.y > 0. && lower.position.y < 40.);
        assert_eq!(upper.side, PortSide::Right);
        let c = construct(&g, &boxes, &flow, SPACING, &BTreeMap::new());
        assert_eq!(c.routes.len(), 2, "{:?}", c.delegated);
    }

    #[test]
    fn a_free_area_takes_the_orientation_the_layout_chose() {
        let g = Graph {
            container_directions: BTreeMap::from([("g".to_string(), DirectionConstraint::Free)]),
            ..graph(
                vec![node("g", None), node("a", Some("g")), node("b", Some("g"))],
                vec![("e", "a", "b")],
            )
        };
        let boxes = BTreeMap::from([
            (
                "g".into(),
                Rect {
                    x: -20.,
                    y: -20.,
                    width: 200.,
                    height: 200.,
                },
            ),
            ("a".into(), rect(0., 0.)),
            (
                "b".into(),
                Rect {
                    x: 0.,
                    y: 100.,
                    width: 40.,
                    height: 40.,
                },
            ),
        ]);
        let p = vec![
            ResolvedPort {
                id: "e:source".into(),
                node_id: "a".into(),
                edge_id: "e".into(),
                endpoint: "source".into(),
                side: PortSide::Bottom,
                position: Point { x: 20., y: 40. },
                explicit: false,
            },
            ResolvedPort {
                id: "e:target".into(),
                node_id: "b".into(),
                edge_id: "e".into(),
                endpoint: "target".into(),
                side: PortSide::Top,
                position: Point { x: 20., y: 100. },
                explicit: false,
            },
        ];
        // Without an orientation the free area has no flow, so the edge is delegated.
        let none = construct(&g, &boxes, &p, SPACING, &BTreeMap::new());
        assert_eq!(
            none.delegated,
            vec![("e".to_string(), Delegation::NoFixedFlow)]
        );
        let tb = BTreeMap::from([("g".to_string(), Orientation::Tb)]);
        let c = construct(&g, &boxes, &p, SPACING, &tb);
        assert_eq!(c.routes.len(), 1, "{:?}", c.delegated);
    }

    /// Fixture: a narrow corridor. The target stands `gap` units after the source's right edge. Widening the corridor moves the
    /// target and everything beyond it by the extra width, as `corridor_gaps` does for a drawing.
    fn narrow_corridor(gap: f64) -> (Graph, BTreeMap<String, Rect>, Vec<ResolvedPort>) {
        let g = graph(
            vec![node("a", None), node("b", None)],
            vec![("e", "a", "b")],
        );
        let boxes = BTreeMap::from([
            ("a".into(), rect(0., 0.)),
            ("b".into(), rect(40. + gap, 0.)),
        ]);
        let p = ports("e", &boxes["a"], &boxes["b"]);
        (g, boxes, p)
    }

    #[test]
    fn a_narrow_corridor_needs_the_reserved_width_and_no_more() {
        // The two margins (12 each) must fit between the boxes, so a gap of 24 or less cannot carry the edge.
        let (g, boxes, p) = narrow_corridor(20.);
        let c = construct(&g, &boxes, &p, SPACING, &BTreeMap::new());
        assert_eq!(
            c.delegated,
            vec![("e".to_string(), Delegation::NoFreeTrack)]
        );
        // The smallest widening that lets the edge in: found by widening in steps of 0.5 units.
        let mut extra = 0.;
        let needed = loop {
            let (g, boxes, p) = narrow_corridor(20. + extra);
            if construct(&g, &boxes, &p, SPACING, &BTreeMap::new())
                .routes
                .len()
                == 1
            {
                break extra;
            }
            extra += 0.5;
            assert!(extra < 20., "the corridor never opens");
        };
        assert!((4.0..=4.5).contains(&needed), "needed widening {needed}");
        // Every wider corridor carries it too (the reservation is monotone).
        for wider in [needed, needed + 6., needed + 30.] {
            let (g, boxes, p) = narrow_corridor(20. + wider);
            assert_eq!(
                construct(&g, &boxes, &p, SPACING, &BTreeMap::new())
                    .routes
                    .len(),
                1
            );
        }
    }

    #[test]
    fn self_loops_and_backward_edges_delegate() {
        let g = graph(
            vec![node("a", None), node("b", None)],
            vec![("loop", "a", "a"), ("back", "b", "a")],
        );
        let boxes = BTreeMap::from([("a".into(), rect(0., 0.)), ("b".into(), rect(100., 0.))]);
        let mut p = ports("back", &boxes["b"], &boxes["a"]);
        p.extend(ports("loop", &boxes["a"], &boxes["a"]));
        let c = construct(&g, &boxes, &p, SPACING, &BTreeMap::new());
        assert_eq!(
            c.delegated,
            vec![
                ("loop".to_string(), Delegation::SelfLoop),
                ("back".to_string(), Delegation::NotForward),
            ]
        );
    }

    #[test]
    fn an_edge_leaving_its_container_in_the_same_flow_crosses_the_border() {
        let g = graph(
            vec![
                node("box", None),
                node("a", Some("box")),
                node("b", None),
                node("c", None),
            ],
            vec![("e", "a", "b")],
        );
        let boxes = BTreeMap::from([
            (
                "box".into(),
                Rect {
                    x: 0.,
                    y: -10.,
                    width: 60.,
                    height: 60.,
                },
            ),
            ("a".into(), rect(10., 0.)),
            // A sibling of the container below the straight line: an obstacle the route must keep clear of.
            (
                "c".into(),
                Rect {
                    x: 120.,
                    y: 60.,
                    width: 40.,
                    height: 40.,
                },
            ),
            ("b".into(), rect(200., 0.)),
        ]);
        let p = ports("e", &boxes["a"], &boxes["b"]);
        let c = construct(&g, &boxes, &p, SPACING, &BTreeMap::new());
        assert!(c.delegated.is_empty(), "{:?}", c.delegated);
        let route = &c.routes[0];
        // It leaves the container through its right border and stays clear of the sibling.
        assert!(route.points.iter().any(|q| q.x > 60.));
        for w in route.points.windows(2) {
            assert!(
                !segment_hits(w[0].clone(), w[1].clone(), &boxes["c"]),
                "{:?}",
                route.points
            );
        }
    }

    #[test]
    fn an_edge_out_of_a_container_with_another_flow_is_built_in_the_outer_frame() {
        let mut g = graph(
            vec![node("box", None), node("a", Some("box")), node("b", None)],
            vec![("e", "a", "b")],
        );
        g.container_directions
            .insert("box".into(), DirectionConstraint::Fixed(Orientation::Tb));
        let boxes = BTreeMap::from([
            (
                "box".into(),
                Rect {
                    x: 0.,
                    y: -10.,
                    width: 60.,
                    height: 60.,
                },
            ),
            ("a".into(), rect(10., 0.)),
            ("b".into(), rect(200., 0.)),
        ]);
        let p = ports("e", &boxes["a"], &boxes["b"]);
        let c = construct(&g, &boxes, &p, SPACING, &BTreeMap::new());
        // Step 6 of plan section 4a: the edge leaves the top-to-bottom container towards its border and is built in the
        // frame of the left-to-right area that holds both ends.
        assert!(c.delegated.is_empty(), "{:?}", c.delegated);
        assert_eq!(c.routes.len(), 1);
    }

    #[test]
    fn an_edge_through_a_container_without_a_fixed_flow_delegates() {
        let mut g = graph(
            vec![node("box", None), node("a", Some("box")), node("b", None)],
            vec![("e", "a", "b")],
        );
        g.container_directions
            .insert("box".into(), DirectionConstraint::Free);
        let boxes = BTreeMap::from([
            (
                "box".into(),
                Rect {
                    x: 0.,
                    y: -10.,
                    width: 60.,
                    height: 60.,
                },
            ),
            ("a".into(), rect(10., 0.)),
            ("b".into(), rect(200., 0.)),
        ]);
        let p = ports("e", &boxes["a"], &boxes["b"]);
        let c = construct(&g, &boxes, &p, SPACING, &BTreeMap::new());
        assert_eq!(
            c.delegated,
            vec![("e".to_string(), Delegation::CrossesArea)]
        );
    }
}

#[cfg(test)]
mod lead_tests {
    use super::*;

    fn end(node: &str, edge: &str, side: PortSide, along: f64, leaving: bool) -> LeadEnd {
        LeadEnd {
            node: node.into(),
            edge: edge.into(),
            endpoint: if leaving { "source" } else { "target" }.into(),
            side,
            along,
            across: 0.5,
            leaving,
            vertical: false,
        }
    }

    /// Two ends arriving at the far side of a node both go around it: the one whose port is nearer the side they go to
    /// takes the inner lane and turns first, so their leads nest.
    #[test]
    fn leads_around_one_end_of_a_node_nest() {
        let mut upper = end("n", "u", PortSide::Right, 1., false);
        upper.across = 0.25;
        let mut lower = end("n", "l", PortSide::Right, 1., false);
        lower.across = 0.75;
        let plan: BTreeMap<String, (bool, usize, usize)> = plan_leads(&[upper, lower])
            .into_iter()
            .map(|(edge, _, high, i, stub)| (edge, (high, i, stub)))
            .collect();
        // No leads across the flow: both go by the low side, the upper port nearer it.
        assert_eq!(plan["u"], (false, 0, 0));
        assert_eq!(plan["l"], (false, 1, 1));
    }

    /// On one side the leaving leads come first, the one nearest the far end nearest the node, then the arriving ones; a
    /// port on its flow side has no lead, one against the flow goes around by the side with fewer leads.
    #[test]
    fn leads_are_numbered_so_that_they_do_not_cross() {
        let ends = [
            end("n", "a", PortSide::Top, 0.25, true),
            end("n", "b", PortSide::Top, 0.75, true),
            end("n", "c", PortSide::Top, 0.5, false),
            end("n", "d", PortSide::Right, 0.5, true),
            end("n", "e", PortSide::Left, 0.5, true),
            end("n", "f", PortSide::Bottom, 0.5, false),
        ];
        let plan: BTreeMap<String, (bool, usize)> = plan_leads(&ends)
            .into_iter()
            .map(|(edge, _, high, i, _)| (edge, (high, i)))
            .collect();
        assert_eq!(plan["b"], (false, 0));
        assert_eq!(plan["a"], (false, 1));
        assert_eq!(plan["c"], (false, 2));
        assert!(!plan.contains_key("d"));
        // Around the node: the top has three leads, the bottom one, so it goes by the bottom, where it leaves before the
        // arriving lead of f (leads around the node come last of their kind, the leaving ones before the arriving ones).
        assert_eq!(plan["e"], (true, 0));
        assert_eq!(plan["f"], (true, 1));
    }

    /// A port across the flow leads straight out to its lane; a port against the flow out by the margin and around.
    #[test]
    fn port_leads_go_out_and_around() {
        let node = Rect {
            x: 100.,
            y: 100.,
            width: 80.,
            height: 40.,
        };
        let port = |side, x, y| arqlet_model::ResolvedPort {
            id: "p".into(),
            node_id: "n".into(),
            edge_id: "e".into(),
            endpoint: "source".into(),
            side,
            position: Point { x, y },
            explicit: true,
        };
        let other = Point { x: 400., y: 300. };
        let (lead, at) = port_lead(
            &node,
            &port(PortSide::Top, 140., 100.),
            &other,
            true,
            false,
            (12., 12.),
            Some(82.),
        );
        assert_eq!(lead, vec![Point { x: 140., y: 100. }]);
        assert_eq!(at, Point { x: 140., y: 82. });
        let (lead, at) = port_lead(
            &node,
            &port(PortSide::Left, 100., 120.),
            &other,
            true,
            false,
            (12., 12.),
            Some(152.),
        );
        assert_eq!(
            lead,
            vec![Point { x: 100., y: 120. }, Point { x: 88., y: 120. }]
        );
        assert_eq!(at, Point { x: 88., y: 152. });
        let (lead, at) = port_lead(
            &node,
            &port(PortSide::Right, 180., 120.),
            &other,
            true,
            false,
            (12., 12.),
            None,
        );
        assert!(lead.is_empty());
        assert_eq!(at, Point { x: 180., y: 120. });
    }
}
