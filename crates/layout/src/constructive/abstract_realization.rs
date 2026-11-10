//! Provisional physical realization of the abstract routing plan on an existing drawing.
//! The realization functions do not search, mutate inputs or accept candidates.
//! The separate `completion` diagnostic completes their routes with explicit fallback.
use super::*;
use crate::abstract_routing::{self, EdgePlan, Plan, Unsupported};
mod chains;
pub mod completion;
pub mod construction;
mod continuity;
pub mod coordination;
pub mod growth;
mod joint;
pub mod planning;
pub mod regional;
pub use chains::{ChainCut, Summary as ChainSummary};
pub use continuity::{CapacityWitness, LaneReservation, Summary as ContinuitySummary};
pub use joint::{BendReservation, Summary as JointSummary};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Coordination {
    None,
    FixedTracks,
    Joint,
    Chains,
}

type BendKey = (String, String, usize);

#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    Structural(Unsupported),
    Preparation(Delegation),
    MissingGeometry {
        area: String,
        element: String,
    },
    InvalidGeometry {
        area: String,
        element: String,
    },
    LaneCapacity {
        area: String,
        element: String,
        required: f64,
        available: f64,
    },
    /// Capacity exhausted by the selected longitudinal lane order, not a proof of unroutability.
    LongitudinalCapacity {
        junction: usize,
        required: f64,
        available: f64,
    },
    JointOrder {
        section: usize,
    },
    JointCapacity {
        section: usize,
        required: f64,
        available: f64,
    },
    NarrowChannel {
        area: String,
        rank: usize,
    },
    NonMonotone,
    Obstacle,
    TrackConflict {
        other: String,
        along_flow: bool,
        separation: f64,
        overlap: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Channel {
    pub area: String,
    pub rank: usize,
    /// Full rank-to-rank space, including clearance on both sides.
    pub available: f64,
    pub required: f64,
    pub missing: f64,
    pub tracks: usize,
    pub assignments: Vec<(String, usize, f64)>,
}

#[derive(Debug, Default)]
pub struct Realization {
    pub routes: Vec<Route>,
    /// Complete edge partition: an edge without a route always has a reason here.
    pub delegated: Vec<(String, Failure)>,
    pub channels: Vec<Channel>,
    pub work: ConstructionCounters,
    pub continuity: Option<ContinuitySummary>,
    pub joint: Option<JointSummary>,
    pub chains: Option<ChainSummary>,
    pub times: construction::Times,
    pub coordination_work: coordination::Work,
}

#[derive(Clone)]
struct Trial {
    prepared: Prepared,
    lanes: Vec<f64>,
    corners: Vec<(f64, f64)>,
    failure: Option<Failure>,
}

struct Geometry<'a> {
    boxes: &'a BTreeMap<String, Rect>,
    plans: &'a LayeredPlans,
}

impl Geometry<'_> {
    fn rect(&self, area: &str, element: &str) -> Result<Rect, Failure> {
        let missing = || Failure::MissingGeometry {
            area: area.into(),
            element: element.into(),
        };
        let plan = self.plans.get(area).ok_or_else(missing)?;
        if let Some(rect) = plan.dummies.get(element) {
            let (x, y) = if area == "<root>" {
                (0., 0.)
            } else {
                let area = self.boxes.get(area).ok_or_else(missing)?;
                (area.x, area.y)
            };
            let rect = Rect {
                x: rect.x + x,
                y: rect.y + y,
                ..rect.clone()
            };
            if ![rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]
                .iter()
                .all(|v| v.is_finite())
            {
                return Err(Failure::InvalidGeometry {
                    area: area.into(),
                    element: element.into(),
                });
            }
            Ok(rect)
        } else {
            self.boxes.get(element).cloned().ok_or_else(missing)
        }
    }

    fn main_span(&self, area: &str, rank: usize) -> Result<(f64, f64), Failure> {
        let plan = &self.plans[area];
        let mut far = f64::NEG_INFINITY;
        let mut near = f64::INFINITY;
        for id in &plan.ranks[rank] {
            let r = self.rect(area, id)?;
            far = far.max(if plan.vertical {
                r.y + r.height
            } else {
                r.x + r.width
            });
        }
        for id in &plan.ranks[rank + 1] {
            let r = self.rect(area, id)?;
            near = near.min(if plan.vertical { r.y } else { r.x });
        }
        if !far.is_finite() || !near.is_finite() {
            return Err(Failure::InvalidGeometry {
                area: area.into(),
                element: format!("<channel:{rank}>"),
            });
        }
        Ok((far, near))
    }
}

fn lanes(
    edge: &EdgePlan,
    prepared: &Prepared,
    geometry: &Geometry<'_>,
    members: &BTreeMap<(String, String), Vec<String>>,
    track: f64,
) -> Result<Vec<f64>, Failure> {
    let mut lanes = vec![prepared.from.1];
    for pair in edge.sections.windows(2) {
        let (area, element) = junction(&pair[0], &pair[1]);
        let rect = geometry.rect(area, element)?;
        let (lo, size) = if prepared.vertical {
            (rect.x, rect.width)
        } else {
            (rect.y, rect.height)
        };
        let group = &members[&(area.clone(), element.clone())];
        let slot = group
            .binary_search(&edge.edge)
            .expect("edge is a member of its junction");
        let required = group.len().saturating_sub(1) as f64 * track;
        if !required.is_finite() {
            return Err(Failure::InvalidGeometry {
                area: area.clone(),
                element: element.clone(),
            });
        }
        if required > size + EPS {
            return Err(Failure::LaneCapacity {
                area: area.clone(),
                element: element.clone(),
                required,
                available: size,
            });
        }
        // Same absolute lane on both sides, including crossings between regional coordinate frames.
        let lane =
            lo + size / 2. + (slot as f64 - group.len().saturating_sub(1) as f64 / 2.) * track;
        lanes.push(lane);
    }
    lanes.push(prepared.to.1);
    Ok(lanes)
}

fn junction<'a>(
    a: &'a abstract_routing::Section,
    b: &'a abstract_routing::Section,
) -> (&'a String, &'a String) {
    if a.area == b.area || a.to != b.area {
        (&a.area, &a.to)
    } else {
        (&b.area, &b.from)
    }
}

fn lane_domain(
    edge: &EdgePlan,
    trial: &Trial,
    geometry: &Geometry<'_>,
    index: usize,
) -> Result<(String, (f64, f64)), Failure> {
    if index == 0 || index == edge.sections.len() {
        let area = if index == 0 {
            &edge.sections[0].area
        } else {
            &edge.sections[index - 1].area
        };
        Ok((area.clone(), (trial.lanes[index], trial.lanes[index])))
    } else {
        let (area, element) = junction(&edge.sections[index - 1], &edge.sections[index]);
        let rect = geometry.rect(area, element)?;
        Ok((
            area.clone(),
            if trial.prepared.vertical {
                (rect.x, rect.x + rect.width)
            } else {
                (rect.y, rect.y + rect.height)
            },
        ))
    }
}

/// Realize discrete channel order using physical lanes and exact interval spacing.
/// `boxes` must be absolute; dummy rectangles remain local to each area.
/// Accepted routes keep the supplied resolved endpoints and logical direction.
/// All routes are checked against the constructor's inflated obstacles and accepted
/// occupancy; no claim about label placement or full-layout acceptance is made.
/// Missing widths are reported, never compressed or silently applied to containers.
pub fn realize(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    plans: &LayeredPlans,
    spacing: Spacing,
) -> Result<Realization, String> {
    realize_impl(graph, boxes, ports, plans, spacing, Coordination::None)
}

/// Separate provisional trial with shared longitudinal reservations across all areas.
/// Junction lanes may move only inside their supplied rectangles; resolved ports stay fixed.
/// A deterministic lane order may reject otherwise routable edges. This is neither a
/// feasibility proof nor a replacement for the complete router.
pub fn realize_coordinated(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    plans: &LayeredPlans,
    spacing: Spacing,
) -> Result<Realization, String> {
    realize_impl(
        graph,
        boxes,
        ports,
        plans,
        spacing,
        Coordination::FixedTracks,
    )
}

/// Provisional endpoint-aware bend ordering followed by bounded lane compaction.
/// The selected disjunctive order is heuristic, not an infeasibility proof.
/// Neither node geometry nor resolved ports is changed; final route checks apply.
pub fn realize_joint(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    plans: &LayeredPlans,
    spacing: Spacing,
) -> Result<Realization, String> {
    realize_impl(graph, boxes, ports, plans, spacing, Coordination::Joint)
}

/// Bounded two-pass trial: trace joint-plan capacity chains, derive one temporal
/// cut per witness, then replan once from the original drawing and lane preferences.
/// Cuts are selected heuristically; output may lose previously realized edges.
pub fn realize_chains(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    plans: &LayeredPlans,
    spacing: Spacing,
) -> Result<Realization, String> {
    realize_impl(graph, boxes, ports, plans, spacing, Coordination::Chains)
}

fn realize_impl(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    plans: &LayeredPlans,
    spacing: Spacing,
    coordination: Coordination,
) -> Result<Realization, String> {
    let plan = abstract_routing::build(graph, plans)?;
    validate_inputs(graph, boxes, ports, plans, spacing)?;
    realize_plan(
        graph,
        boxes,
        ports,
        plans,
        spacing,
        &plan,
        coordination,
        false,
        None,
    )
}

fn validate_inputs(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    plans: &LayeredPlans,
    spacing: Spacing,
) -> Result<(), String> {
    for node in &graph.nodes {
        if !boxes.contains_key(&node.id) {
            return Err(format!(
                "Abstract realization: missing node geometry {}",
                node.id
            ));
        }
    }
    if !spacing.margin.is_finite()
        || spacing.margin < 0.
        || !spacing.track.is_finite()
        || spacing.track <= 0.
    {
        return Err(
            "Abstract realization requires finite nonnegative clearance and positive track spacing"
                .into(),
        );
    }
    for (id, rect) in boxes
        .iter()
        .chain(plans.values().flat_map(|p| p.dummies.iter()))
    {
        if ![
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            rect.x + rect.width,
            rect.y + rect.height,
        ]
        .iter()
        .all(|v| v.is_finite())
            || rect.width < 0.
            || rect.height < 0.
        {
            return Err(format!("Abstract realization: invalid rectangle {id}"));
        }
    }
    let mut port_ends = BTreeSet::new();
    for port in ports {
        if !port_ends.insert((&port.edge_id, &port.endpoint)) {
            return Err(format!(
                "Abstract realization: duplicate resolved endpoint {}:{}",
                port.edge_id, port.endpoint
            ));
        }
        if !port.position.x.is_finite() || !port.position.y.is_finite() {
            return Err(format!(
                "Abstract realization: invalid resolved port {}",
                port.id
            ));
        }
    }
    Ok(())
}

fn realize_plan(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    plans: &LayeredPlans,
    spacing: Spacing,
    plan: &Plan,
    coordination: Coordination,
    demand_only: bool,
    mut session: Option<&mut planning::Session>,
) -> Result<Realization, String> {
    let preparation_started = crate::stage_timer_start();
    let geometry = Geometry { boxes, plans };
    let parents = parents_of(graph);
    let containers = containers_of(graph);
    let edges: BTreeMap<&str, _> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut members: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for edge in &plan.edges {
        for pair in edge.sections.windows(2) {
            let (area, element) = junction(&pair[0], &pair[1]);
            let key = (area.clone(), element.clone());
            members.entry(key).or_default().push(edge.edge.clone());
        }
    }
    for group in members.values_mut() {
        group.sort();
        group.dedup();
    }
    // The flow of every planned area as its plan has it: an area that chooses its direction (`auto`, allowed sets) has
    // chosen it in the placement, and its edges are prepared in that flow.
    let orientations: BTreeMap<String, arqlet_model::Orientation> = plans
        .iter()
        .map(|(area, plan)| {
            use arqlet_model::Orientation;
            let orientation = match (plan.vertical, plan.reversed) {
                (false, false) => Orientation::Lr,
                (false, true) => Orientation::Rl,
                (true, false) => Orientation::Tb,
                (true, true) => Orientation::Bt,
            };
            (area.clone(), orientation)
        })
        .collect();
    let mut report = Realization::default();
    let mut trials = BTreeMap::new();
    let mut prepared_edges = 0;
    let port_ends = crate::ports::PortEnds::new(ports);
    let mut obstacle_preparations_avoided = 0;
    let reuse_construction = session.as_ref().is_some_and(|s| s.construction.is_some());
    let lead_lanes = lead_lanes(graph, boxes, ports, &parents, &orientations, spacing);
    for edge in &plan.edges {
        if let Some(reason) = &edge.unsupported {
            report
                .delegated
                .push((edge.edge.clone(), Failure::Structural(reason.clone())));
            continue;
        }
        let logical = edges[edge
            .leg
            .as_ref()
            .map_or(edge.edge.as_str(), |l| l.edge.as_str())];
        prepared_edges += 1;
        // A leg from a leaf to a side node beside it (ADR 0017, amendment A) is one straight segment across the area's flow,
        // from the leaf's port to the side port; it needs no lanes, tracks or corners.
        if let (Some(leg), true) = (&edge.leg, edge.sections.is_empty()) {
            match lateral_leg(edge, leg, &port_ends, boxes, plans, spacing) {
                Ok(route) => report.routes.push(route),
                Err(reason) => report
                    .delegated
                    .push((edge.edge.clone(), Failure::Preparation(reason))),
            }
            continue;
        }
        let prepared = if let Some(leg) = &edge.leg {
            prepare_leg(
                edge,
                leg,
                &port_ends,
                boxes,
                &geometry,
                &parents,
                spacing,
                !reuse_construction,
            )
        } else {
            prepare_edge_inner(
                graph,
                boxes,
                &port_ends,
                &parents,
                &containers,
                spacing,
                &orientations,
                &logical.id,
                &logical.source,
                logical
                    .target
                    .as_deref()
                    .expect("a planned edge has a target"),
                false,
                !reuse_construction,
                &lead_lanes,
            )
        };
        let prepared = match prepared {
            Ok(prepared) => {
                if reuse_construction {
                    obstacle_preparations_avoided += 1;
                }
                prepared
            }
            Err(reason) => {
                report
                    .delegated
                    .push((edge.edge.clone(), Failure::Preparation(reason)));
                continue;
            }
        };
        if edge.sections.iter().any(|s| {
            let area = &plans[&s.area];
            area.vertical != prepared.vertical || area.reversed
        }) {
            report.delegated.push((
                edge.edge.clone(),
                Failure::Preparation(Delegation::NoFixedFlow),
            ));
            continue;
        }
        if edge.leg.is_none() && prepared.reversed != edge.reversed {
            report
                .delegated
                .push((edge.edge.clone(), Failure::NonMonotone));
            continue;
        }
        match lanes(edge, &prepared, &geometry, &members, spacing.track) {
            Ok(lanes) => {
                let mut trial = Trial {
                    prepared,
                    lanes,
                    corners: Vec::new(),
                    failure: None,
                };
                if let Some(session) = session.as_deref_mut() {
                    if let Err(failure) = session.align(edge, &mut trial, &geometry) {
                        report.delegated.push((edge.edge.clone(), failure));
                        continue;
                    }
                }
                trials.insert(edge.edge.clone(), trial);
            }
            Err(reason) => report.delegated.push((edge.edge.clone(), reason)),
        }
    }
    let sections: BTreeMap<(&str, &str, usize), usize> = plan
        .edges
        .iter()
        .flat_map(|edge| {
            edge.sections
                .iter()
                .enumerate()
                .map(move |(i, s)| ((edge.edge.as_str(), s.area.as_str(), s.channel), i))
        })
        .collect();
    report.times.edge_preparation = crate::elapsed_nanos(preparation_started);
    let channels_started = crate::stage_timer_start();
    for channel in &plan.channels {
        let mut changes = Vec::new();
        for &i in &channel.order {
            let assignment = &channel.assignments[i];
            let Some(trial) = trials.get(&assignment.edge) else {
                continue;
            };
            let index = sections[&(
                assignment.edge.as_str(),
                channel.area.as_str(),
                channel.rank,
            )];
            let (from, to) = (trial.lanes[index], trial.lanes[index + 1]);
            if (from - to).abs() > EPS {
                changes.push((assignment.edge.as_str(), index, from, to));
            }
        }
        if changes.is_empty() {
            continue;
        }
        let (far, near) = match geometry.main_span(&channel.area, channel.rank) {
            Ok(span) => span,
            Err(reason) => {
                for (id, _, _, _) in changes {
                    trials.get_mut(id).unwrap().failure = Some(reason.clone());
                }
                continue;
            }
        };
        // The public completion (with a session) orders the channel on the physical lanes (`Session::tracks`); the
        // diagnostic trials keep the ordinal plan order their coordination mechanisms are built to correct.
        let (slots, count) = if let Some(session) = session.as_deref_mut() {
            session.tracks(&channel.area, channel.rank, &changes, spacing.track)
        } else {
            let spans: Vec<_> = changes.iter().map(|c| (c.2, c.3)).collect();
            let order: Vec<usize> = (0..spans.len()).collect();
            track_slots(&spans, &order, spacing.track)
        };
        let required = 2. * spacing.margin + count.saturating_sub(1) as f64 * spacing.track;
        let available = near - far;
        if !required.is_finite() || !available.is_finite() {
            return Err(format!(
                "Abstract realization: nonfinite channel space in {}:{}",
                channel.area, channel.rank
            ));
        }
        let missing = (required - available).max(0.);
        if !missing.is_finite() {
            return Err(format!(
                "Abstract realization: nonfinite channel deficit in {}:{}",
                channel.area, channel.rank
            ));
        }
        let first = far / 2. + near / 2. - count.saturating_sub(1) as f64 * spacing.track / 2.;
        let mut assignments = Vec::new();
        for (i, &(id, _, _, _)) in changes.iter().enumerate() {
            let at = first + slots[i] as f64 * spacing.track;
            if !at.is_finite() {
                return Err(format!(
                    "Abstract realization: nonfinite track in {}:{}",
                    channel.area, channel.rank
                ));
            }
            assignments.push((id.to_string(), slots[i], at));
            let trial = trials.get_mut(id).unwrap();
            if missing > EPS {
                trial.failure = Some(Failure::NarrowChannel {
                    area: channel.area.clone(),
                    rank: channel.rank,
                });
            }
        }
        report.channels.push(Channel {
            area: channel.area.clone(),
            rank: channel.rank,
            available,
            required,
            missing,
            tracks: count,
            assignments,
        });
    }
    report.times.channels = crate::elapsed_nanos(channels_started);
    if let Some(reuse) = session.as_deref_mut().and_then(|s| s.construction.as_mut()) {
        reuse.work.obstacle_preparations_avoided += obstacle_preparations_avoided;
    }
    if demand_only {
        if coordination == Coordination::Joint {
            // The joint order may need more tracks than the channel's interval slots; growth reserves them up front.
            let context = coordination::Context::new(
                &geometry,
                session.as_ref().is_some_and(|s| s.coordination),
            );
            let joint = joint::demand(plan, &context, &trials, spacing, &report.channels)?;
            for channel in &mut report.channels {
                if let Some(&width) = joint.get(&(channel.area.clone(), channel.rank)) {
                    channel.required = channel.required.max(width);
                }
            }
        }
        report.work = ConstructionCounters {
            preparations: 1,
            prepared_edges,
            preparation_nanos: crate::elapsed_nanos(preparation_started),
            ..Default::default()
        };
        return Ok(report);
    }
    let coordination_started = crate::stage_timer_start();
    let coordination_geometry =
        coordination::Context::new(&geometry, session.as_ref().is_some_and(|s| s.coordination));
    let mut probe = None;
    if coordination == Coordination::Chains {
        let (summary, reusable) = chains::assign(
            plan,
            &coordination_geometry,
            &trials,
            spacing,
            &report.channels,
        )?;
        report.chains = Some(summary);
        probe = reusable;
    }
    if let Some(probe) = probe {
        // Without chain orders the ordering pass repeats the probe on the same input: its result is the probe's.
        trials = probe.trials;
        report.channels = probe.channels;
        report.joint = Some(probe.joint);
        report.continuity = Some(probe.continuity);
    } else if coordination != Coordination::None {
        let bends = if matches!(coordination, Coordination::Joint | Coordination::Chains) {
            let orders = report
                .chains
                .as_ref()
                .map_or(&[][..], |c| c.orders.as_slice());
            let (summary, bends) = joint::assign(
                plan,
                &coordination_geometry,
                &mut trials,
                spacing,
                &mut report.channels,
                orders,
            )?;
            report.joint = Some(summary);
            Some(bends)
        } else {
            None
        };
        report.continuity = Some(continuity::assign(
            plan,
            &coordination_geometry,
            &mut trials,
            spacing,
            &mut report.channels,
            bends.as_ref(),
        )?);
    }
    let tracks: BTreeMap<(&str, &str, usize), f64> = report
        .channels
        .iter()
        .flat_map(|c| {
            c.assignments
                .iter()
                .map(move |(id, _, at)| ((id.as_str(), c.area.as_str(), c.rank), *at))
        })
        .collect();
    for edge in &plan.edges {
        let Some(trial) = trials.get_mut(&edge.edge) else {
            continue;
        };
        if trial.failure.is_some() {
            continue;
        }
        for (i, s) in edge.sections.iter().enumerate() {
            if (trial.lanes[i] - trial.lanes[i + 1]).abs() <= EPS {
                continue;
            }
            let at = tracks[&(edge.edge.as_str(), s.area.as_str(), s.channel)];
            trial.corners.push((at, trial.lanes[i]));
            trial.corners.push((at, trial.lanes[i + 1]));
        }
    }
    report.times.coordination = crate::elapsed_nanos(coordination_started);
    report.coordination_work = coordination_geometry.work();
    let materialization_started = crate::stage_timer_start();
    let work = RefCell::new(ConstructionCounters {
        preparations: 1,
        prepared_edges,
        preparation_nanos: crate::elapsed_nanos(preparation_started),
        ..ConstructionCounters::default()
    });
    let mut assigned = Vec::new();
    let mut occupied = construction::Occupancy::new(spacing.track);
    let mut owners = Vec::new();
    let obstacles = reuse_construction.then(|| construction::Obstacles::new(boxes, spacing.margin));
    if let Some(reuse) = session.as_deref_mut().and_then(|s| s.construction.as_mut()) {
        reuse.work.shared_indexes += 1;
    }
    let mut visited = BTreeSet::new();
    // The legs of an edge across a border of another axis, in order (ADR 0017); they are built where the edge would be.
    let mut legs_of: BTreeMap<&str, Vec<(usize, &str)>> = BTreeMap::new();
    for e in &plan.edges {
        if let Some(leg) = &e.leg {
            legs_of
                .entry(leg.edge.as_str())
                .or_default()
                .push((leg.index, e.edge.as_str()));
        }
    }
    // Keep graph-input order, as the existing constructor does; never let thread scheduling select occupancy.
    for (edge, id) in graph.edges.iter().flat_map(|edge| {
        let ids: Vec<String> = match legs_of.get(edge.id.as_str()) {
            Some(legs) => {
                let mut legs = legs.clone();
                legs.sort();
                legs.into_iter().map(|(_, id)| id.to_string()).collect()
            }
            None => vec![edge.id.clone()],
        };
        ids.into_iter().map(move |id| (edge, id))
    }) {
        let Some(trial) = trials.remove(&id) else {
            continue;
        };
        let reason = if let Some(reason) = trial.failure {
            Some(reason)
        } else if !monotone(&trial.prepared, &trial.corners) {
            Some(Failure::NonMonotone)
        } else {
            let before_obstacle = work.borrow().planned_obstacle_failures;
            let built =
                if let Some(reuse) = session.as_deref_mut().and_then(|s| s.construction.as_mut()) {
                    let skip = endpoint_exclusions(
                        &parents,
                        &edge.source,
                        edge.target.as_deref().expect("a planned edge has a target"),
                    );
                    visited.insert(id.clone());
                    reuse.build(
                        &trial.prepared,
                        &trial.corners,
                        obstacles.as_ref().expect("shared construction obstacles"),
                        &skip,
                        &mut assigned,
                        &mut occupied,
                        &work,
                    )
                } else {
                    build_planned(
                        &trial.prepared,
                        &trial.corners,
                        &mut assigned,
                        spacing.track,
                        &work,
                    )
                };
            match built {
                Some((route, _)) => {
                    owners.resize(assigned.len(), id.clone());
                    report.routes.push(route);
                    None
                }
                None if work.borrow().planned_obstacle_failures > before_obstacle => {
                    Some(Failure::Obstacle)
                }
                None => {
                    let points = planned_points(&trial.prepared, &trial.corners);
                    let (step, index) = segments_of(&points)
                        .into_iter()
                        .find_map(|s| {
                            assigned
                                .iter()
                                .position(|a| conflicts(a, &s, spacing.track))
                                .map(|i| (s, i))
                        })
                        .expect("a planned track failure has a conflicting segment");
                    let other = &assigned[index];
                    Some(Failure::TrackConflict {
                        other: owners[index].clone(),
                        along_flow: trial.prepared.vertical != step.horizontal,
                        separation: (step.coord - other.coord).abs(),
                        overlap: step.range.1.min(other.range.1) - step.range.0.max(other.range.0),
                    })
                }
            }
        };
        if let Some(reason) = reason {
            report.delegated.push((id.clone(), reason));
        }
    }
    join_legs(&mut report);
    report.delegated.sort_by(|a, b| a.0.cmp(&b.0));
    if let Some(reuse) = session.and_then(|s| s.construction.as_mut()) {
        reuse.retain(&visited);
    }
    report.work = work.into_inner();
    report.times.materialization = crate::elapsed_nanos(materialization_started);
    Ok(report)
}

/// Joins the legs of every edge across a border of another axis (ADR 0017) into the edge's route, so that the realization
/// reports edges, not legs. The legs meet at their border ports; a backward edge's legs come out in logical direction each,
/// and meet the other way round. An edge with a leg that could not be built is delegated as a whole, with that leg's reason.
fn join_legs(report: &mut Realization) {
    let separator = "\u{1}leg";
    let logical = |id: &str| -> Option<(String, usize)> {
        let (edge, index) = id.split_once(separator)?;
        Some((edge.to_string(), index.parse().ok()?))
    };
    let mut legs: BTreeMap<String, Vec<(usize, Route)>> = BTreeMap::new();
    report.routes.retain(|route| match logical(&route.id) {
        Some((edge, index)) => {
            legs.entry(edge).or_default().push((index, route.clone()));
            false
        }
        None => true,
    });
    let mut failed: BTreeMap<String, Failure> = BTreeMap::new();
    report.delegated.retain(|(id, failure)| match logical(id) {
        Some((edge, _)) => {
            failed.entry(edge).or_insert_with(|| failure.clone());
            false
        }
        None => true,
    });
    for (edge, mut built) in legs {
        if failed.contains_key(&edge) {
            continue;
        }
        built.sort_by_key(|(index, _)| *index);
        let meets = |a: &Point, b: &Point| (a.x - b.x).abs() <= EPS && (a.y - b.y).abs() <= EPS;
        // The legs in one order (index order, or its reverse for a backward edge), each joined at the last point so far. A
        // straight leg of two points (a lateral segment, amendment A) may run either way; it is turned to fit.
        let chain = |order: &[&Route]| -> Option<Vec<Point>> {
            let mut points: Vec<Point> = Vec::new();
            for (i, route) in order.iter().enumerate() {
                let mut leg = route.points.clone();
                let next = order.get(i + 1).map(|r| &r.points);
                match points.last() {
                    None => {
                        // The first leg: turned when only its first point meets the next leg.
                        if leg.len() == 2 {
                            if let Some(next) = next {
                                let fits = |p: &Point| {
                                    next.first().is_some_and(|n| meets(p, n))
                                        || (next.len() == 2
                                            && next.last().is_some_and(|n| meets(p, n)))
                                };
                                if !fits(leg.last()?) && fits(&leg[0]) {
                                    leg.reverse();
                                }
                            }
                        }
                        points.extend(leg);
                    }
                    Some(last) => {
                        if leg.len() == 2 && !meets(last, &leg[0]) && meets(last, &leg[1]) {
                            leg.reverse();
                        }
                        if !meets(last, leg.first()?) {
                            return None;
                        }
                        points.extend(leg.into_iter().skip(1));
                    }
                }
            }
            Some(points)
        };
        let forward: Vec<&Route> = built.iter().map(|(_, r)| r).collect();
        let backward: Vec<&Route> = built.iter().rev().map(|(_, r)| r).collect();
        let joined_points = chain(&forward).or_else(|| chain(&backward));
        let joined = joined_points.is_some();
        let points = joined_points.unwrap_or_default();
        if joined {
            report.routes.push(Route::new(&edge, straighten(points)));
        } else {
            failed.insert(edge, Failure::NonMonotone);
        }
    }
    report.delegated.extend(failed);
}

/// The route of a leg without sections: from a leaf's port straight across its area's flow to the side port beside it
/// (ADR 0017, amendment A), in construction order (the edge's, reversed for a backward edge).
fn lateral_leg(
    edge: &EdgePlan,
    leg: &abstract_routing::Leg,
    ports: &crate::ports::PortEnds<'_>,
    boxes: &BTreeMap<String, Rect>,
    plans: &LayeredPlans,
    spacing: Spacing,
) -> Result<Route, Delegation> {
    let (first_end, last_end) = if edge.reversed {
        ("target", "source")
    } else {
        ("source", "target")
    };
    let point = |at: &abstract_routing::LegEnd,
                 endpoint: &str|
     -> Result<(Point, Option<bool>), Delegation> {
        match at {
            abstract_routing::LegEnd::End => Ok((
                ports
                    .get(&leg.edge, endpoint)
                    .ok_or(Delegation::Unresolved)?
                    .position
                    .clone(),
                None,
            )),
            abstract_routing::LegEnd::Port { area, side } => {
                let plan = plans.get(area).ok_or(Delegation::Unresolved)?;
                let origin = boxes.get(area).ok_or(Delegation::Unresolved)?;
                let port = crate::layered_plan::side_port(
                    plan,
                    (origin.x, origin.y),
                    side,
                    Some(spacing.track),
                )
                .ok_or(Delegation::Unresolved)?;
                Ok((port, Some(plan.vertical)))
            }
        }
    };
    let (start, a) = point(&leg.start, first_end)?;
    let (end, b) = point(&leg.end, last_end)?;
    let vertical = a.or(b).ok_or(Delegation::Unresolved)?;
    // Across a vertical flow the segment is horizontal: both ends on one line.
    let off = if vertical {
        (start.y - end.y).abs()
    } else {
        (start.x - end.x).abs()
    };
    if off > EPS {
        return Err(Delegation::PortSide);
    }
    Ok(Route::new(&edge.edge, vec![start, end]))
}

/// Prepares a leg of an edge (ADR 0017) from its two ends instead of two graph nodes, in the frame of the area its sections
/// are in. An end of the edge is its resolved port, which has to lie on the frame's flow side; a border port lies on the side
/// of the child area its side node stands for, at the side node's place along that area's flow. A leg is built in the edge's
/// construction direction and marked like the edge, so its points come out in logical direction.
#[allow(clippy::too_many_arguments)]
fn prepare_leg(
    edge: &EdgePlan,
    leg: &abstract_routing::Leg,
    ports: &crate::ports::PortEnds<'_>,
    boxes: &BTreeMap<String, Rect>,
    geometry: &Geometry<'_>,
    parents: &Parents<'_>,
    spacing: Spacing,
    prepare_obstacles: bool,
) -> Result<Prepared, Delegation> {
    let area = &edge.sections.first().ok_or(Delegation::Unresolved)?.area;
    let vertical = geometry
        .plans
        .get(area)
        .ok_or(Delegation::Unresolved)?
        .vertical;
    let target = edge.target.as_deref().ok_or(Delegation::Unresolved)?;
    let (out_side, in_side) = if vertical {
        (PortSide::Bottom, PortSide::Top)
    } else {
        (PortSide::Right, PortSide::Left)
    };
    let (first_end, last_end) = if edge.reversed {
        ("target", "source")
    } else {
        ("source", "target")
    };
    // An end: its point, and how far its own box reaches along the flow (the border port has no box of its own).
    let end = |at: &abstract_routing::LegEnd, endpoint: &str, side: PortSide, first: bool| match at
    {
        abstract_routing::LegEnd::End => {
            let port = ports
                .get(&leg.edge, endpoint)
                .ok_or(Delegation::Unresolved)?;
            if port.side != side {
                return Err(Delegation::PortSide);
            }
            let node = boxes.get(&port.node_id).ok_or(Delegation::Unresolved)?;
            let reach = match (vertical, first) {
                (true, true) => node.y + node.height,
                (true, false) => node.y,
                (false, true) => node.x + node.width,
                (false, false) => node.x,
            };
            Ok((port.position.clone(), port.node_id.clone(), reach))
        }
        abstract_routing::LegEnd::Port { area, side } => {
            let inner = geometry.plans.get(area).ok_or(Delegation::Unresolved)?;
            // The port is the side node's place on its own lane (`layered_plan::side_port`), so that no two legs meet on the
            // area's border; beside a child area that leaves by a border node of its own (amendment A3), on that node's lane.
            let origin = geometry.boxes.get(area).ok_or(Delegation::Unresolved)?;
            let mut point = crate::layered_plan::side_port(
                inner,
                (origin.x, origin.y),
                side,
                Some(spacing.track),
            )
            .ok_or(Delegation::Unresolved)?;
            if let Some(lane) = crate::layered_plan::lateral_child_lane(
                geometry.plans,
                geometry.boxes,
                area,
                side,
                &leg.edge,
            ) {
                if inner.vertical {
                    point.y = lane;
                } else {
                    point.x = lane;
                }
            }
            let reach = frame(vertical, &point).0;
            Ok((point, area.clone(), reach))
        }
    };
    let (start, from_node, s_far) = end(&leg.start, first_end, out_side, true)?;
    let (end, to_node, t_near) = end(&leg.end, last_end, in_side, false)?;
    let (s_main, s_across) = frame(vertical, &start);
    let (t_main, t_across) = frame(vertical, &end);
    if t_main <= s_main + EPS {
        return Err(Delegation::NotForward);
    }
    let skip = endpoint_exclusions(parents, &edge.source, target);
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
        edge_id: edge.edge.clone(),
        from_node,
        to_node,
        reversed: edge.reversed,
        vertical,
        from: (s_main, s_across),
        to: (t_main, t_across),
        band: (s_far + spacing.margin, t_near - spacing.margin),
        obstacle_index: std::rc::Rc::new(BoxIndex::new(
            &obstacles
                .iter()
                .map(|r| [r.x, r.y, r.x + r.width, r.y + r.height])
                .collect::<Vec<_>>(),
        )),
        obstacles,
        start,
        end,
        lead: (Vec::new(), Vec::new()),
    })
}

fn monotone(prepared: &Prepared, corners: &[(f64, f64)]) -> bool {
    let points: Vec<_> = std::iter::once(prepared.from)
        .chain(corners.iter().copied())
        .chain(std::iter::once(prepared.to))
        .collect();
    points.windows(2).all(|p| {
        p[1].0 >= p[0].0 - EPS && ((p[0].0 - p[1].0).abs() <= EPS || (p[0].1 - p[1].1).abs() <= EPS)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layered_plan::AreaPlan;
    use arqlet_model::{Edge, Node};

    pub(super) const SPACING: Spacing = Spacing {
        margin: 12.,
        track: 6.,
        through_borders: true,
        probe: false,
        port_sides: false,
    };

    pub(super) fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    pub(super) fn graph(nodes: &[(&str, Option<&str>)], edges: &[(&str, &str, &str)]) -> Graph {
        Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: BTreeMap::new(),
            default_routing: None,
            container_routing: BTreeMap::new(),
            ports: Vec::new(),
            edge_ports: Vec::new(),
            nodes: nodes
                .iter()
                .map(|(id, parent)| Node {
                    id: id.to_string(),
                    kind: "node".into(),
                    label: id.to_string(),
                    parent_id: parent.map(str::to_string),
                    shape: Default::default(),
                })
                .collect(),
            edges: edges
                .iter()
                .map(|(id, from, to)| Edge {
                    id: id.to_string(),
                    kind: "edge".into(),
                    source: from.to_string(),
                    target: Some(to.to_string()),
                })
                .collect(),
            buses: Vec::new(),
        }
    }

    pub(super) fn area(ranks: &[&[&str]], links: &[(&str, &str, &[&str])]) -> AreaPlan {
        AreaPlan {
            ranks: ranks
                .iter()
                .map(|r| r.iter().map(|id| id.to_string()).collect())
                .collect(),
            chains: links
                .iter()
                .map(|(a, b, d)| {
                    (
                        (a.to_string(), b.to_string()),
                        d.iter().map(|id| id.to_string()).collect(),
                    )
                })
                .collect(),
            ..AreaPlan::default()
        }
    }

    pub(super) fn ports(graph: &Graph, boxes: &BTreeMap<String, Rect>) -> Vec<ResolvedPort> {
        graph
            .edges
            .iter()
            .flat_map(|edge| {
                let a = &boxes[&edge.source];
                let b = &boxes[edge.target.as_ref().unwrap()];
                [
                    (
                        "source",
                        &edge.source,
                        PortSide::Right,
                        a.x + a.width,
                        a.y + a.height / 2.,
                    ),
                    (
                        "target",
                        edge.target.as_ref().unwrap(),
                        PortSide::Left,
                        b.x,
                        b.y + b.height / 2.,
                    ),
                ]
                .map(|(end, node, side, x, y)| ResolvedPort {
                    id: format!("{}:{end}", edge.id),
                    node_id: node.clone(),
                    edge_id: edge.id.clone(),
                    endpoint: end.into(),
                    side,
                    position: Point { x, y },
                    explicit: false,
                })
            })
            .collect()
    }

    #[test]
    fn reports_exact_gap_and_realizes_without_search_after_only_that_growth() {
        let graph = graph(&[("a", None), ("b", None)], &[("e", "a", "b")]);
        let mut boxes = BTreeMap::from([
            ("a".into(), rect(0., 0., 40., 40.)),
            ("b".into(), rect(60., 100., 40., 40.)),
        ]);
        let plans =
            BTreeMap::from([("<root>".into(), area(&[&["a"], &["b"]], &[("a", "b", &[])]))]);
        let narrow = realize(&graph, &boxes, &ports(&graph, &boxes), &plans, SPACING).unwrap();
        assert!(narrow.routes.is_empty());
        assert_eq!(narrow.channels.len(), 1);
        assert_eq!(narrow.channels[0].available, 20.);
        assert_eq!(narrow.channels[0].required, 24.);
        assert_eq!(narrow.channels[0].missing, 4.);
        assert!(matches!(
            narrow.delegated[0].1,
            Failure::NarrowChannel { .. }
        ));
        boxes.get_mut("b").unwrap().x += 4.;
        let resolved = ports(&graph, &boxes);
        let grown = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert!(grown.delegated.is_empty(), "{:?}", grown.delegated);
        assert_eq!(grown.routes.len(), 1);
        let points = &grown.routes[0].points;
        assert_eq!(points[0].x, 40.);
        assert_eq!(points[0].y, 20.);
        assert_eq!(points.last().unwrap().x, 64.);
        assert_eq!(points.last().unwrap().y, 120.);
        assert!(points
            .windows(2)
            .all(|p| p[0].x == p[1].x || p[0].y == p[1].y));
        assert_eq!(grown.work.searches, 0);
        assert_eq!(grown.work.transitions, 0);
    }

    #[test]
    fn preserves_long_parallel_lanes_and_refuses_undersized_dummies() {
        let graph = graph(
            &[("a", None), ("b", None)],
            &[("p", "a", "b"), ("q", "a", "b")],
        );
        let boxes = BTreeMap::from([
            ("a".into(), rect(0., 0., 40., 40.)),
            ("b".into(), rect(320., 80., 40., 40.)),
        ]);
        let mut root = area(&[&["a"], &["dummy"], &["b"]], &[("a", "b", &["dummy"])]);
        root.dummies
            .insert("dummy".into(), rect(160., 40., 10., 6.));
        let mut plans = BTreeMap::from([("<root>".into(), root)]);
        let mut resolved = ports(&graph, &boxes);
        for port in &mut resolved {
            port.position.y += if port.edge_id == "p" { -3. } else { 3. };
        }
        let report = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(report.routes.len(), 2, "{:?}", report.delegated);
        assert_eq!(report.work.searches, 0);
        let y_at = |route: &Route, x: f64| {
            route
                .points
                .windows(2)
                .find_map(|p| (p[0].y == p[1].y && p[0].x <= x && p[1].x >= x).then_some(p[0].y))
                .unwrap()
        };
        assert_eq!(
            y_at(&report.routes[1], 165.) - y_at(&report.routes[0], 165.),
            6.
        );
        for r in &report.routes {
            let endpoint = resolved
                .iter()
                .find(|p| p.edge_id == r.id && p.endpoint == "source")
                .unwrap();
            assert_eq!(r.points[0].y, endpoint.position.y);
        }
        plans
            .get_mut("<root>")
            .unwrap()
            .dummies
            .get_mut("dummy")
            .unwrap()
            .height = 5.;
        let narrow = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(narrow.delegated.len(), 2);
        assert!(narrow.delegated.iter().all(|(_, f)| matches!(
            f,
            Failure::LaneCapacity {
                required: 6.,
                available: 5.,
                ..
            }
        )));
    }

    #[test]
    fn coordinates_distinct_junctions_on_shared_longitudinal_space() {
        let mut graph = graph(
            &[("a", None), ("b", None), ("c", None), ("d", None)],
            &[("p", "a", "b"), ("q", "c", "d")],
        );
        let mut boxes = BTreeMap::from([
            ("a".into(), rect(0., 0., 40., 40.)),
            ("b".into(), rect(320., 0., 40., 40.)),
            ("c".into(), rect(0., 80., 40., 40.)),
            ("d".into(), rect(320., 80., 40., 40.)),
        ]);
        let mut root = area(
            &[&["a", "c"], &["u", "v"], &["b", "d"]],
            &[("a", "b", &["u"]), ("c", "d", &["v"])],
        );
        for id in ["u", "v"] {
            root.dummies.insert(id.into(), rect(160., 40., 10., 12.));
        }
        let mut plans = BTreeMap::from([("<root>".into(), root)]);
        let mut resolved = ports(&graph, &boxes);
        let plain = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(plain.routes.len(), 1);
        let coordinated = realize_coordinated(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(coordinated.routes.len(), 2, "{:?}", coordinated.delegated);
        let c = coordinated.continuity.as_ref().unwrap();
        assert!(c.shifted > 0);
        assert_eq!(c.rejected_edges, 0);
        let junctions: Vec<_> = c.lanes.iter().filter(|r| r.junction == 1).collect();
        assert!(
            (junctions[1].coordinate.unwrap() - junctions[0].coordinate.unwrap() - 6.).abs() < EPS
        );
        assert_eq!(coordinated.work.searches, 0);
        let repeat = realize_coordinated(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(coordinated.continuity, repeat.continuity);
        assert_eq!(
            serde_json::to_vec(&coordinated.routes).unwrap(),
            serde_json::to_vec(&repeat.routes).unwrap()
        );
        let horizontal_joint = realize_joint(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(horizontal_joint.routes.len(), 2);
        let horizontal_chains = realize_chains(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(horizontal_chains.routes.len(), 2);
        assert!(horizontal_chains.chains.as_ref().unwrap().orders.is_empty());

        graph.default_direction = DirectionConstraint::Fixed(Orientation::Tb);
        for r in boxes
            .values_mut()
            .chain(plans.get_mut("<root>").unwrap().dummies.values_mut())
        {
            std::mem::swap(&mut r.x, &mut r.y);
            std::mem::swap(&mut r.width, &mut r.height);
        }
        plans.get_mut("<root>").unwrap().vertical = true;
        for port in &mut resolved {
            std::mem::swap(&mut port.position.x, &mut port.position.y);
            port.side = if port.endpoint == "source" {
                PortSide::Bottom
            } else {
                PortSide::Top
            };
        }
        let vertical = realize_coordinated(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(vertical.routes.len(), 2);
        assert_eq!(vertical.continuity, coordinated.continuity);
        for (a, b) in coordinated.routes.iter().zip(&vertical.routes) {
            for (p, q) in a.points.iter().zip(&b.points) {
                assert_eq!((p.x, p.y), (q.y, q.x));
            }
        }
        let joint = realize_joint(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(joint.routes.len(), 2, "{:?}", joint.delegated);
        assert_eq!(joint.work.searches, 0);
        assert_eq!(joint.joint, horizontal_joint.joint);
        for (a, b) in horizontal_joint.routes.iter().zip(&joint.routes) {
            assert_eq!(a.points.len(), b.points.len());
            for (p, q) in a.points.iter().zip(&b.points) {
                assert_eq!((p.x, p.y), (q.y, q.x));
            }
            let vertical_chains =
                realize_chains(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
            assert_eq!(vertical_chains.chains, horizontal_chains.chains);
            assert_eq!(vertical_chains.routes.len(), horizontal_chains.routes.len());
            for (a, b) in horizontal_chains.routes.iter().zip(&vertical_chains.routes) {
                assert_eq!(a.points.len(), b.points.len());
                for (p, q) in a.points.iter().zip(&b.points) {
                    assert_eq!((p.x, p.y), (q.y, q.x));
                }
            }
        }
    }

    #[test]
    fn joint_order_separates_close_source_and_target_stubs_by_bend_precedence() {
        let graph = graph(
            &[("a", None), ("b", None), ("c", None), ("d", None)],
            &[("z", "a", "b"), ("a", "c", "d")],
        );
        let boxes = BTreeMap::from([
            ("a".into(), rect(0., 0., 40., 40.)),
            ("b".into(), rect(200., 100., 40., 40.)),
            ("c".into(), rect(0., 200., 40., 40.)),
            ("d".into(), rect(200., 0., 40., 40.)),
        ]);
        let plans = BTreeMap::from([(
            "<root>".into(),
            area(
                &[&["c", "a"], &["b", "d"]],
                &[("a", "b", &[]), ("c", "d", &[])],
            ),
        )]);
        let resolved = ports(&graph, &boxes);
        let fixed = realize_coordinated(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(fixed.routes.len(), 1);
        let joint = realize_joint(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(joint.routes.len(), 2, "{:?}", joint.delegated);
        let report = joint.joint.as_ref().unwrap();
        assert!(report.temporal_constraints > 0);
        let p = report
            .bends
            .iter()
            .find(|b| b.edge == "z")
            .unwrap()
            .coordinate
            .unwrap();
        let q = report
            .bends
            .iter()
            .find(|b| b.edge == "a")
            .unwrap()
            .coordinate
            .unwrap();
        assert!(p + SPACING.track <= q + EPS);
        assert_eq!(joint.work.searches, 0);
        for route in &joint.routes {
            let source = resolved
                .iter()
                .find(|p| p.edge_id == route.id && p.endpoint == "source")
                .unwrap();
            let target = resolved
                .iter()
                .find(|p| p.edge_id == route.id && p.endpoint == "target")
                .unwrap();
            assert_eq!(
                serde_json::to_value(&route.points[0]).unwrap(),
                serde_json::to_value(&source.position).unwrap()
            );
            assert_eq!(
                serde_json::to_value(route.points.last().unwrap()).unwrap(),
                serde_json::to_value(&target.position).unwrap()
            );
        }
    }

    #[test]
    fn boundary_lanes_use_absolute_inner_origins_and_keep_logical_cycle_direction() {
        let mut graph = graph(
            &[("c", None), ("a", Some("c")), ("b", None)],
            &[("e", "a", "b")],
        );
        let boxes = BTreeMap::from([
            ("c".into(), rect(100., 200., 200., 160.)),
            ("a".into(), rect(120., 220., 40., 40.)),
            ("b".into(), rect(500., 300., 40., 40.)),
        ]);
        let mut inner = area(&[&["a"], &["exit"]], &[("a", "exit", &[])]);
        inner.dummies.insert("exit".into(), rect(190., 40., 0., 0.));
        inner.borders.insert("e".into(), "exit".into());
        let mut plans = BTreeMap::from([
            ("c".into(), inner),
            ("<root>".into(), area(&[&["c"], &["b"]], &[("c", "b", &[])])),
        ]);
        let resolved = ports(&graph, &boxes);
        let report = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(report.routes.len(), 1, "{:?}", report.delegated);
        // The whole exit from the leaf through the container border stays on y=240, not local y=40.
        assert!(report.routes[0]
            .points
            .windows(2)
            .any(|p| p[0].y == 240. && p[1].y == 240. && p[0].x < 300. && p[1].x > 300.));
        graph.edges[0].source = "b".into();
        graph.edges[0].target = Some("a".into());
        plans
            .get_mut("<root>")
            .unwrap()
            .reversed_links
            .insert(("b".into(), "c".into()));
        let mut reversed = resolved.clone();
        for p in &mut reversed {
            p.endpoint = if p.endpoint == "source" {
                "target"
            } else {
                "source"
            }
            .into();
        }
        let reverse = realize(&graph, &boxes, &reversed, &plans, SPACING).unwrap();
        assert_eq!(reverse.routes.len(), 1);
        let points = &reverse.routes[0].points;
        assert_eq!((points[0].x, points[0].y), (500., 320.));
        assert_eq!(
            (points.last().unwrap().x, points.last().unwrap().y),
            (160., 240.)
        );
        let coordinated = realize_coordinated(&graph, &boxes, &reversed, &plans, SPACING).unwrap();
        assert_eq!(
            serde_json::to_vec(&coordinated.routes).unwrap(),
            serde_json::to_vec(&reverse.routes).unwrap()
        );
        let border = coordinated
            .continuity
            .unwrap()
            .lanes
            .into_iter()
            .find(|r| r.junction == 1)
            .unwrap();
        assert_eq!(border.area, "c");
        assert_eq!(border.bounds, (240., 240.));
        assert_eq!(border.coordinate, Some(240.));
        let joint = realize_joint(&graph, &boxes, &reversed, &plans, SPACING).unwrap();
        assert_eq!(joint.routes.len(), 1, "{:?}", joint.delegated);
        let points = &joint.routes[0].points;
        assert_eq!((points[0].x, points[0].y), (500., 320.));
        assert_eq!(
            (points.last().unwrap().x, points.last().unwrap().y),
            (160., 240.)
        );
        let chains = realize_chains(&graph, &boxes, &reversed, &plans, SPACING).unwrap();
        assert_eq!(
            serde_json::to_vec(&chains.routes).unwrap(),
            serde_json::to_vec(&joint.routes).unwrap()
        );
    }

    #[test]
    fn rejects_obstacles_and_occupied_tracks_without_search_or_success_fallbacks() {
        let graph = graph(
            &[("a", None), ("b", None), ("block", None)],
            &[("p", "a", "b"), ("q", "a", "b")],
        );
        let mut boxes = BTreeMap::from([
            ("a".into(), rect(0., 0., 40., 40.)),
            ("b".into(), rect(200., 0., 40., 40.)),
            ("block".into(), rect(90., 100., 40., 40.)),
        ]);
        let plans =
            BTreeMap::from([("<root>".into(), area(&[&["a"], &["b"]], &[("a", "b", &[])]))]);
        let report = realize(&graph, &boxes, &ports(&graph, &boxes), &plans, SPACING).unwrap();
        assert_eq!(report.routes.len(), 1);
        assert!(matches!(&report.delegated[0], (id, Failure::TrackConflict {
            other, along_flow: true, separation: 0., ..
        }) if id == "q" && other == "p"));
        let chains =
            realize_chains(&graph, &boxes, &ports(&graph, &boxes), &plans, SPACING).unwrap();
        assert_eq!(chains.routes.len(), 1);
        let c = chains.chains.as_ref().unwrap();
        assert_eq!(c.planning_passes, 2);
        assert_eq!(c.probe_continuity.capacity_witnesses.len(), 1);
        assert_eq!(
            c.uncut_witnesses, 1,
            "Overlapping fixed-endpoint spans cannot be temporally separated"
        );
        assert!(c.orders.is_empty());
        assert_eq!(c.cuts[0].pair, None);
        assert_eq!(chains.work.searches, 0);
        boxes.get_mut("block").unwrap().y = 0.;
        let blocked = realize(&graph, &boxes, &ports(&graph, &boxes), &plans, SPACING).unwrap();
        assert!(blocked.routes.is_empty());
        assert!(blocked
            .delegated
            .iter()
            .all(|(_, f)| *f == Failure::Obstacle));
        assert_eq!(blocked.work.planned_obstacle_failures, 2);
        assert_eq!(blocked.work.searches, 0);
    }

    #[test]
    fn changed_axes_are_equivalent_and_reversed_flow_is_explicitly_delegated() {
        let mut graph = graph(&[("a", None), ("b", None)], &[("e", "a", "b")]);
        let mut boxes = BTreeMap::from([
            ("a".into(), rect(0., 10., 40., 40.)),
            ("b".into(), rect(200., 110., 40., 40.)),
        ]);
        let mut plans =
            BTreeMap::from([("<root>".into(), area(&[&["a"], &["b"]], &[("a", "b", &[])]))]);
        let mut resolved = ports(&graph, &boxes);
        let horizontal = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        plans.get_mut("<root>").unwrap().vertical = true;
        let mismatched = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(
            mismatched.delegated[0].1,
            Failure::Preparation(Delegation::NoFixedFlow)
        );
        graph.default_direction = DirectionConstraint::Fixed(Orientation::Tb);
        plans.get_mut("<root>").unwrap().vertical = true;
        for rect in boxes.values_mut() {
            std::mem::swap(&mut rect.x, &mut rect.y);
            std::mem::swap(&mut rect.width, &mut rect.height);
        }
        for p in &mut resolved {
            std::mem::swap(&mut p.position.x, &mut p.position.y);
            p.side = if p.endpoint == "source" {
                PortSide::Bottom
            } else {
                PortSide::Top
            };
        }
        let vertical = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(vertical.routes.len(), 1);
        assert!(vertical.routes[0]
            .points
            .iter()
            .zip(&horizontal.routes[0].points)
            .all(|(a, b)| a.x == b.y && a.y == b.x));
        graph.default_direction = DirectionConstraint::Fixed(Orientation::Bt);
        let unsupported = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(
            unsupported.delegated[0].1,
            Failure::Preparation(Delegation::NoFixedFlow)
        );
    }

    #[test]
    fn demand_handoff_matches_full_channel_plan_without_materializing_routes() {
        let graph = graph(&[("a", None), ("b", None)], &[("e", "a", "b")]);
        let boxes = BTreeMap::from([
            ("a".into(), rect(0., 10., 40., 40.)),
            ("b".into(), rect(200., 110., 40., 40.)),
        ]);
        let plans =
            BTreeMap::from([("<root>".into(), area(&[&["a"], &["b"]], &[("a", "b", &[])]))]);
        let resolved = ports(&graph, &boxes);
        let plan = abstract_routing::build(&graph, &plans).unwrap();
        let full = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert_eq!(full.routes.len(), 1);
        let demand = realize_plan(
            &graph,
            &boxes,
            &resolved,
            &plans,
            SPACING,
            &plan,
            Coordination::None,
            true,
            None,
        )
        .unwrap();
        assert_eq!(demand.channels, full.channels);
        assert!(demand.routes.is_empty());
        assert!(demand.continuity.is_none() && demand.joint.is_none() && demand.chains.is_none());
        assert_eq!(demand.work.searches, 0);
        assert_eq!(demand.work.planned_obstacle_failures, 0);
    }

    #[test]
    fn invalid_numbers_are_errors_and_incomplete_geometry_is_explicit() {
        let graph = graph(&[("a", None), ("b", None)], &[("e", "a", "b")]);
        let boxes = BTreeMap::from([
            ("a".into(), rect(0., 0., 40., 40.)),
            ("b".into(), rect(200., 100., 40., 40.)),
        ]);
        let plans = BTreeMap::from([(
            "<root>".into(),
            area(&[&["a"], &["missing"], &["b"]], &[("a", "b", &["missing"])]),
        )]);
        let resolved = ports(&graph, &boxes);
        let report = realize(&graph, &boxes, &resolved, &plans, SPACING).unwrap();
        assert!(matches!(
            report.delegated[0].1,
            Failure::MissingGeometry { .. }
        ));
        for value in [f64::NAN, f64::INFINITY, 0., -1.] {
            assert!(realize(
                &graph,
                &boxes,
                &resolved,
                &plans,
                Spacing {
                    track: value,
                    ..SPACING
                }
            )
            .is_err());
        }
        let mut duplicate = resolved.clone();
        duplicate.push(resolved[0].clone());
        assert!(realize(&graph, &boxes, &duplicate, &plans, SPACING)
            .unwrap_err()
            .contains("duplicate resolved"));
        let mut invalid = boxes;
        invalid.get_mut("a").unwrap().width = f64::NAN;
        assert!(realize(&graph, &invalid, &resolved, &plans, SPACING).is_err());
        invalid.remove("a");
        assert!(realize(&graph, &invalid, &resolved, &plans, SPACING)
            .unwrap_err()
            .contains("missing node"));
    }

    #[test]
    fn fixed_order_growth_propagates_child_extent_and_moves_neighbour_subtrees() {
        let graph = graph(
            &[
                ("c", None),
                ("a", Some("c")),
                ("b", None),
                ("d", None),
                ("z", Some("d")),
            ],
            &[("e", "a", "b")],
        );
        let boxes = BTreeMap::from([
            ("c".into(), rect(100., 200., 100., 160.)),
            ("a".into(), rect(120., 220., 40., 40.)),
            ("b".into(), rect(220., 300., 40., 40.)),
            ("d".into(), rect(400., 400., 100., 100.)),
            ("z".into(), rect(420., 420., 40., 40.)),
        ]);
        let mut inner = area(&[&["a"], &["exit"]], &[("a", "exit", &[])]);
        inner.dummies.insert("exit".into(), rect(80., 60., 0., 0.));
        inner.borders.insert("e".into(), "exit".into());
        let plans = BTreeMap::from([
            ("c".into(), inner),
            ("d".into(), area(&[&["z"]], &[])),
            (
                "<root>".into(),
                area(&[&["c"], &["b", "d"]], &[("c", "b", &[])]),
            ),
        ]);
        let resolved = ports(&graph, &boxes);
        let mut options = arqlet_model::LayoutOptions::default();
        // Match the exact spacing used in this fixture, independently of production clearance defaults.
        options.track_spacing = SPACING.track;
        options.required_clearance = Some(SPACING.margin - SPACING.track);
        let actual_spacing = crate::constructive_spacing(&options);
        assert_eq!(actual_spacing.margin, SPACING.margin);
        let before = realize(&graph, &boxes, &resolved, &plans, actual_spacing).unwrap();
        assert_eq!(before.channels.iter().map(|c| c.missing).sum::<f64>(), 8.);
        let grown = growth::widen(&graph, &boxes, &resolved, &plans, &options).unwrap();
        assert_eq!(grown.boxes["c"].width, 104.);
        assert_eq!(grown.boxes["a"].x, 120.);
        assert_eq!(grown.boxes["b"].x, 228.);
        assert_eq!(grown.boxes["d"].x, 408.);
        assert_eq!(grown.boxes["z"].x, 428.);
        assert_eq!(grown.plans["c"].dummies["exit"].x, 84.);
        for (id, rect) in &boxes {
            assert_eq!(grown.boxes[id].y, rect.y);
            assert_eq!(grown.boxes[id].height, rect.height);
        }
        for (area, p) in &plans {
            assert_eq!(grown.plans[area].ranks, p.ranks);
        }
        let after = realize(
            &graph,
            &grown.boxes,
            &grown.ports,
            &grown.plans,
            actual_spacing,
        )
        .unwrap();
        assert_eq!(after.routes.len(), 1, "{:?}", after.delegated);
        assert!(after.channels.iter().all(|c| c.missing <= EPS));
        assert_eq!(after.work.searches, 0);
        assert_eq!(boxes["c"].width, 100., "do not mutate the source drawing");
    }

    #[test]
    fn channel_growth_refuses_pins_fixed_sizes_flattened_and_reversed_plans() {
        use arqlet_model::{AxisSize, ContainerSizing, LayoutOptions};
        let graph = graph(
            &[("c", None), ("a", Some("c")), ("b", None)],
            &[("e", "a", "b")],
        );
        let mut plans = BTreeMap::from([
            ("c".into(), area(&[&["a"]], &[])),
            ("<root>".into(), area(&[&["c"], &["b"]], &[("c", "b", &[])])),
        ]);
        let mut options = LayoutOptions::default();
        assert!(growth::check_capability(&graph, &plans, &options).is_ok());
        options.pins.insert("a".into(), Point { x: 0., y: 0. });
        assert!(growth::check_capability(&graph, &plans, &options)
            .unwrap_err()
            .contains("pinned"));
        options.pins.clear();
        options.container_sizing.insert(
            "c".into(),
            ContainerSizing {
                width: Some(AxisSize::Fixed(100.)),
                height: None,
            },
        );
        assert!(growth::check_capability(&graph, &plans, &options)
            .unwrap_err()
            .contains("fixed"));
        options.container_sizing.clear();
        options.placement_grid = 8.;
        assert!(growth::check_capability(&graph, &plans, &options)
            .unwrap_err()
            .contains("grid"));
        options.placement_grid = 0.;
        options.max_extent = Some(100.);
        assert!(growth::check_capability(&graph, &plans, &options)
            .unwrap_err()
            .contains("extent"));
        options.max_extent = None;
        // Areas of different axes are allowed, reversed ones are not.
        plans.get_mut("c").unwrap().vertical = true;
        assert!(growth::check_capability(&graph, &plans, &options).is_ok());
        plans.get_mut("c").unwrap().reversed = true;
        assert!(growth::check_capability(&graph, &plans, &options)
            .unwrap_err()
            .contains("positive flow"));
        plans.remove("c");
        plans.get_mut("<root>").unwrap().ranks[0] = vec!["a".into()];
        assert!(growth::check_capability(&graph, &plans, &options)
            .unwrap_err()
            .contains("flattened"));
    }
}
