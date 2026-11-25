//! Provisional (ADR 0022 M3b, option `holdMemberships`): holding a soft membership group together inside its owner.
//!
//! A group whose shown members all have one container in the view is laid out as a container nested in that owner, which is
//! taken out of the result again: the members keep their owner, and only the group's hull is drawn. Order alone does not
//! hold a group (`benchmarks/results/memberships-20261125-hold-inside.md`): what separates its members mostly stands in
//! other ranks. Each group is tried on its own layout, in the order of the priority, and kept when it is then drawn whole
//! and its cost stays in the budget; otherwise it is released, and the display stages of ADR 0022 §3 draw it.
//!
//! Only a complete layout searches. One that continues an earlier drawing (an edit of a part of it) holds the groups that
//! were held, in the areas it lays out again, and tries no others.

use crate::{compute_counted_unclocked, hard_violations, published_metrics, SearchCounters};
use arqlet_model::{
    grouping::{self, PARENT},
    Graph, HeldGroup, HoldingCost, HoldingReport, LayoutOptions, Mode, Node, Orientation, Rect,
    ReleaseReason, ReleasedGroup, Snapshot, HOLDING_PREFIX as PREFIX,
};
use std::collections::{BTreeMap, BTreeSet};

/// Whether `id` is a container that holds a group during the layout. None of them leaves this module.
pub(crate) fn is_holding(id: &str) -> bool {
    id.starts_with(PREFIX)
}

/// At most this many groups are tried in one layout, since each costs a layout. A placeholder until M3 measures it.
pub const TRIALS: usize = 64;
/// The budget of one group on rank 2, halved with every further rank: at most the larger of the fixed number and the share
/// of the drawing without holding. Placeholders until M3d measures them.
const CROSSINGS: usize = 2;
const BENDS: usize = 4;
const SHARE: usize = 20;
/// How far a hull reaches beyond its boxes (as the scene draws it).
const HULL_PADDING: f64 = 10.;

/// Whether the options and the graph ask for holding at all.
pub(crate) fn applies(graph: &Graph, options: &LayoutOptions) -> bool {
    options.hold_memberships
        && options.grouping_priority.len() > 1
        && !graph.memberships.dimensions.is_empty()
}

/// A soft group that could be held: its rank in the priority (from 1), and its members that the view shows, by id.
struct Candidate {
    rank: usize,
    held: HeldGroup,
    members: Vec<String>,
}

impl Candidate {
    /// The id of the container that holds it during the layout.
    fn container(&self) -> String {
        format!("{PREFIX}{}/{}", self.held.dimension, self.held.group)
    }

    fn released(&self, reason: ReleaseReason) -> ReleasedGroup {
        ReleasedGroup {
            dimension: self.held.dimension.clone(),
            group: self.held.group.clone(),
            reason,
        }
    }
}

/// The groups of the soft dimensions with at least two shown members, by rank and in their declared order. A moved-down
/// parent hierarchy is not held, nor a group that is still a box of the view (it owns nodes there).
fn candidates(graph: &Graph, options: &LayoutOptions) -> Vec<Candidate> {
    let shown: BTreeSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut out = Vec::new();
    for (rank, criterion) in options.grouping_priority.iter().enumerate().skip(1) {
        if criterion.by == PARENT {
            continue;
        }
        let Some(dimension) = graph.memberships.dimension(&criterion.by) else {
            continue;
        };
        // Membership in a nested group implies its ancestors (ADR 0022 §1).
        let parent: BTreeMap<&str, Option<&str>> = dimension
            .groups
            .iter()
            .map(|g| (g.id.as_str(), g.parent.as_deref()))
            .collect();
        let mut enclosed: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for g in &dimension.groups {
            let mut at = Some(g.id.as_str());
            for _ in 0..=dimension.groups.len() {
                let Some(a) = at else { break };
                enclosed
                    .entry(a)
                    .or_default()
                    .extend(g.members.iter().map(String::as_str));
                at = parent.get(a).copied().flatten();
            }
        }
        for g in &dimension.groups {
            let own = grouping::container_id(&dimension.id, &g.id);
            if !grouping::considers(criterion, &g.id) || shown.contains(own.as_str()) {
                continue;
            }
            let members: Vec<String> = enclosed
                .get(g.id.as_str())
                .into_iter()
                .flatten()
                .filter(|m| shown.contains(*m))
                .map(|m| m.to_string())
                .collect();
            if members.len() > 1 {
                out.push(Candidate {
                    rank,
                    held: HeldGroup {
                        dimension: dimension.id.clone(),
                        group: g.id.clone(),
                    },
                    members,
                });
            }
        }
    }
    out
}

fn parents(graph: &Graph) -> BTreeMap<&str, Option<&str>> {
    graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect()
}

/// The container all members share, when they share one: `Some(None)` is the top level.
fn common_parent<'a>(
    parents: &BTreeMap<&'a str, Option<&'a str>>,
    members: &[String],
) -> Option<Option<&'a str>> {
    let mut found = members.iter().map(|m| parents.get(m.as_str()).copied());
    let first = found.next()??;
    found.all(|p| p == Some(first)).then_some(first)
}

/// Whether the hull of the members encloses no foreign leaf: the first display stage (ADR 0022 §3).
fn whole(snapshot: &Snapshot, options: &LayoutOptions, candidate: &Candidate) -> bool {
    let parent = parents(&snapshot.graph);
    let containers: BTreeSet<&str> = parent.values().flatten().copied().collect();
    let members: BTreeSet<&str> = candidate.members.iter().map(String::as_str).collect();
    let boxes = crate::absolute_all(&snapshot.graph, &snapshot.boxes);
    let rects: Vec<Rect> = members.iter().map(|m| boxes[*m].clone()).collect();
    let shape = options.grouping_priority[candidate.rank]
        .hull
        .unwrap_or(options.membership_hull);
    let hull = arqlet_scene::hull::group_shape(&rects, HULL_PADDING, shape);
    let inside_member = |id: &str| {
        let mut at = Some(id);
        for _ in 0..=parent.len() {
            match at {
                Some(x) if members.contains(x) => return true,
                Some(x) => at = parent.get(x).copied().flatten(),
                None => return false,
            }
        }
        false
    };
    snapshot.graph.nodes.iter().all(|n| {
        let id = n.id.as_str();
        containers.contains(id)
            || inside_member(id)
            || !arqlet_scene::hull::contains(&hull, &arqlet_scene::hull::centre(&boxes[id]))
    })
}

/// Takes the holding containers out of a layout: their children move to the owner they have in `graph`, at the same place.
/// `None` when the layout says something about such a container that cannot be taken out.
fn dissolve(mut snapshot: Snapshot, graph: &Graph) -> Option<Snapshot> {
    let held = |id: &str| id.starts_with(PREFIX);
    if snapshot
        .geometry_issues
        .iter()
        .any(|i| i.node_ids.iter().any(|n| held(n)))
        || snapshot.route_conflicts.iter().any(|c| held(&c.node_id))
    {
        return None;
    }
    let working: BTreeMap<String, Option<String>> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.parent_id.clone()))
        .collect();
    // The areas whose plans name a holding container: the owners of one, and the containers themselves.
    let mut areas: BTreeSet<String> = BTreeSet::new();
    for (id, parent) in &working {
        if held(id) {
            areas.insert(id.clone());
            areas.insert(parent.clone().unwrap_or_else(|| "<root>".into()));
        }
    }
    let mut moved = BTreeMap::new();
    for (id, parent) in &working {
        let (mut x, mut y) = (0., 0.);
        let mut at = parent.as_deref();
        while let Some(p) = at.filter(|p| held(p)) {
            let r = snapshot.boxes.get(p)?;
            x += r.x;
            y += r.y;
            at = working.get(p)?.as_deref();
        }
        if !held(id) && (x, y) != (0., 0.) {
            moved.insert(id.clone(), (x, y));
        }
    }
    for (id, (x, y)) in moved {
        let r = snapshot.boxes.get_mut(&id)?;
        r.x += x;
        r.y += y;
    }
    snapshot.boxes.retain(|id, _| !held(id));
    snapshot.recomputed_containers.retain(|id| !held(id));
    // Findings that name a holding container are about an area the result does not have.
    if snapshot.routing_issues.iter().any(|i| i.contains(PREFIX)) {
        return None;
    }
    snapshot.layout_diagnostics.retain(|d| !d.contains(PREFIX));
    snapshot
        .boundary_crossings
        .retain(|c| !held(&c.container_id));
    // The crossings of an edge are numbered along it: without the ones taken out, numbered again.
    let mut next: BTreeMap<String, usize> = BTreeMap::new();
    for c in &mut snapshot.boundary_crossings {
        let n = next.entry(c.edge_id.clone()).or_default();
        c.ordinal = *n;
        *n += 1;
    }
    snapshot
        .corridor_gaps
        .retain(|g| !held(&g.before) && !held(&g.after));
    snapshot.area_orientations.retain(|id, _| !held(id));
    snapshot.area_strategies.retain(|id, _| !held(id));
    snapshot.layered_plans.retain(|id, _| !areas.contains(id));
    let original = parents(graph);
    snapshot.graph.nodes.retain(|n| !held(&n.id));
    for n in &mut snapshot.graph.nodes {
        n.parent_id = original.get(n.id.as_str()).copied()?.map(str::to_string);
    }
    Some(snapshot)
}

/// Crossings (with those at shared ends), bends and hard violations of a drawing, with its geometry issues.
fn cost(snapshot: &Snapshot) -> Result<(usize, usize, usize), String> {
    let m = published_metrics(snapshot)?;
    Ok((
        m.crossings + m.shared_endpoint_crossings,
        m.bends,
        hard_violations(&m, false) + snapshot.geometry_issues.len(),
    ))
}

/// The complete layout of `graph` with the groups held that the budget allows.
pub(crate) fn hold(
    graph: &Graph,
    options: &LayoutOptions,
) -> Result<(Snapshot, SearchCounters), String> {
    let plain = LayoutOptions {
        hold_memberships: false,
        ..options.clone()
    };
    let lay = |g: &Graph| compute_counted_unclocked(g, None, &BTreeSet::new(), &plain);
    let (mut best, stages) = lay(graph)?;
    let found = candidates(graph, options);
    let finish = |mut snapshot: Snapshot,
                  held: Vec<HeldGroup>,
                  released: Vec<ReleasedGroup>,
                  cost: Option<HoldingCost>| {
        snapshot.options.hold_memberships = true;
        snapshot.held_groups = held;
        snapshot.holding = Some(HoldingReport { released, cost });
        Ok((snapshot, stages))
    };
    // A bus is drawn on lines its members share; its layout rewrites the graph, so nothing is held beside one.
    if !graph.buses.is_empty() || graph.nodes.iter().any(|n| is_holding(&n.id)) {
        best.layout_diagnostics
            .push("holdMemberships: not applied to a graph with buses or ids `held:`".into());
        let released = found
            .iter()
            .map(|c| c.released(ReleaseReason::Buses))
            .collect();
        return finish(best, Vec::new(), released, None);
    }
    let mut released = Vec::new();
    let mut all: Vec<Candidate> = Vec::new();
    for c in found {
        let reason = match fixed(graph, options, &c) {
            Some(reason) => reason,
            None => {
                all.push(c);
                continue;
            }
        };
        if !whole(&best, options, &c) {
            released.push(c.released(reason));
        }
    }
    let flow_before = against_flow(&best);
    let (base_crossings, base_bends, _) = cost(&best)?;
    let (mut crossings, mut bends, mut hard) = cost(&best)?;
    let mut is_whole: Vec<bool> = all.iter().map(|c| whole(&best, options, c)).collect();
    let mut working = graph.clone();
    let mut held = Vec::new();
    let mut trials = 0;
    for (i, candidate) in all.iter().enumerate() {
        if is_whole[i] {
            continue;
        }
        // Members that another held group took apart cannot share a container any more.
        let parent = parents(&working);
        if common_parent(&parent, &candidate.members).is_none() {
            released.push(candidate.released(ReleaseReason::OverlapsHeld));
            continue;
        }
        if trials == TRIALS {
            released.push(candidate.released(ReleaseReason::Trials));
            continue;
        }
        trials += 1;
        let mut trial = working.clone();
        enclose(&mut trial, candidate);
        // A layout the container makes impossible (an arrangement that refuses it) releases the group.
        let Some(drawn) = lay(&trial).ok().and_then(|(s, _)| dissolve(s, graph)) else {
            released.push(candidate.released(ReleaseReason::Refused));
            continue;
        };
        let (c, b, h) = cost(&drawn)?;
        let allowed = |floor: usize, base: usize| {
            floor.max(base / SHARE) >> (candidate.rank - 1).min(usize::BITS as usize - 1)
        };
        let now: Vec<bool> = all.iter().map(|c| whole(&drawn, options, c)).collect();
        let keeps = is_whole
            .iter()
            .zip(&now)
            .all(|(before, after)| !before || *after);
        let reason = if !now[i] {
            ReleaseReason::NotWhole
        } else if !keeps {
            ReleaseReason::BreaksOther
        } else if h > hard {
            ReleaseReason::Violation
        } else if c > crossings + allowed(CROSSINGS, base_crossings)
            || b > bends + allowed(BENDS, base_bends)
        {
            ReleaseReason::Budget
        } else {
            (best, crossings, bends, hard, is_whole) = (drawn, c, b, h, now);
            working = trial;
            held.push(candidate.held.clone());
            continue;
        };
        released.push(candidate.released(reason));
    }
    // A group that a later one made whole is not released after all.
    released.retain(|r| {
        !all.iter()
            .zip(&is_whole)
            .any(|(c, whole)| *whole && c.held.dimension == r.dimension && c.held.group == r.group)
    });
    let cost = (!held.is_empty()).then(|| HoldingCost {
        crossings_before: base_crossings,
        crossings,
        bends_before: base_bends,
        bends,
        against_flow_before: flow_before,
        against_flow: against_flow(&best),
    });
    finish(best, held, released, cost)
}

/// Why a group cannot be tried at all: its members are in several containers, or one has a place or an order the user fixed,
/// which is relative to the owner and which holding would move.
fn fixed(graph: &Graph, options: &LayoutOptions, c: &Candidate) -> Option<ReleaseReason> {
    if common_parent(&parents(graph), &c.members).is_none() {
        return Some(ReleaseReason::AcrossContainers);
    }
    c.members
        .iter()
        .any(|m| options.pins.contains_key(m) || options.order_hints.contains_key(m))
        .then_some(ReleaseReason::FixedPlace)
}

/// Puts the members of a group into a container of their own inside the one they share. Nothing where they share none.
fn enclose(graph: &mut Graph, candidate: &Candidate) -> bool {
    let owner = match common_parent(&parents(graph), &candidate.members) {
        Some(owner) => owner.map(str::to_string),
        None => return false,
    };
    let id = candidate.container();
    graph.nodes.push(Node {
        id: id.clone(),
        kind: "membership".into(),
        label: String::new(),
        parent_id: owner,
        shape: Default::default(),
    });
    let members: BTreeSet<&str> = candidate.members.iter().map(String::as_str).collect();
    for n in &mut graph.nodes {
        if members.contains(n.id.as_str()) {
            n.parent_id = Some(id.clone());
        }
    }
    true
}

/// The edges whose target does not lie ahead of their source along the flow of the area both are in. Areas without a flow
/// (organic, radial) count none.
fn against_flow(snapshot: &Snapshot) -> usize {
    let graph = &snapshot.graph;
    let parent = parents(graph);
    let boxes = crate::absolute_all(graph, &snapshot.boxes);
    let path = |id: &str| {
        let mut out = Vec::new();
        let mut at = parent.get(id).copied().flatten();
        while let Some(p) = at {
            if out.len() > parent.len() {
                break;
            }
            out.push(p.to_string());
            at = parent.get(p).copied().flatten();
        }
        out
    };
    graph
        .edges
        .iter()
        .filter(|e| {
            let Some(target) = e.target.as_deref().filter(|t| *t != e.source) else {
                return false;
            };
            let (a, b) = (path(&e.source), path(target));
            let area = a.iter().find(|p| b.contains(p)).map(String::as_str);
            let orientation = match snapshot.area_orientations.get(area.unwrap_or("<root>")) {
                Some(o) => *o,
                None => match graph.area_mode(area) {
                    Ok(Mode::Orientation(o)) => o,
                    _ => return false,
                },
            };
            let (Some(s), Some(t)) = (boxes.get(e.source.as_str()), boxes.get(target)) else {
                return false;
            };
            let (s, t) = (arqlet_scene::hull::centre(s), arqlet_scene::hull::centre(t));
            match orientation {
                Orientation::Lr => t.x < s.x,
                Orientation::Rl => t.x > s.x,
                Orientation::Tb => t.y < s.y,
                Orientation::Bt => t.y > s.y,
            }
        })
        .count()
}

/// A layout that continues `previous`: the groups held there are held again in the areas it lays out, without a search. Where
/// that is not possible, the layout goes on without holding, and their owners, which have no plan to keep, are placed again.
pub(crate) fn resume(
    graph: &Graph,
    previous: &Snapshot,
    dirty: &BTreeSet<String>,
    options: &LayoutOptions,
    plain: &LayoutOptions,
) -> Result<(Snapshot, SearchCounters), String> {
    let found = candidates(graph, options);
    let again: Vec<&Candidate> = previous
        .held_groups
        .iter()
        .filter_map(|h| found.iter().find(|c| c.held == *h))
        .filter(|c| fixed(graph, options, c).is_none())
        .collect();
    let (mut working, mut earlier) = (graph.clone(), previous.clone());
    // Laid out again: what the edit asks for, the owners of held groups, and the containers of the groups themselves.
    let mut redo = dirty.clone();
    redo.extend(owners(previous));
    let mut boxes = crate::absolute_all(&previous.graph, &previous.boxes);
    let mut enclosed = Vec::new();
    for c in again {
        if enclose(&mut working, c) {
            redo.insert(c.container());
            enclosed.push(c);
        }
    }
    // The earlier drawing with the same containers around the members it shows; an inner one was enclosed after the outer.
    for c in enclosed.iter().rev() {
        let of: Vec<&Rect> = working
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == Some(c.container().as_str()))
            .filter_map(|n| boxes.get(n.id.as_str()))
            .collect();
        if of.is_empty() {
            continue;
        }
        let low = |v: &dyn Fn(&Rect) -> f64| of.iter().map(|r| v(r)).fold(f64::MAX, f64::min);
        let high = |v: &dyn Fn(&Rect) -> f64| of.iter().map(|r| v(r)).fold(f64::MIN, f64::max);
        let (x, y) = (low(&|r| r.x) - 24., low(&|r| r.y) - 24.);
        let rect = Rect {
            x,
            y,
            width: high(&|r| r.x + r.width) + 24. - x,
            height: high(&|r| r.y + r.height) + 24. - y,
        };
        boxes.insert(c.container(), rect);
    }
    let parent = parents(&working);
    for n in &mut earlier.graph.nodes {
        if let Some(p) = parent.get(n.id.as_str()).copied().flatten() {
            if is_holding(p) {
                n.parent_id = Some(p.to_string());
            }
        }
    }
    for c in &enclosed {
        let id = c.container();
        if let (Some(n), true) = (
            working.nodes.iter().find(|n| n.id == id),
            boxes.contains_key(&id),
        ) {
            earlier.graph.nodes.push(n.clone());
        }
    }
    earlier.boxes = earlier
        .graph
        .nodes
        .iter()
        .filter_map(|n| {
            let mut r = boxes.get(&n.id)?.clone();
            if let Some(p) = n.parent_id.as_ref().and_then(|p| boxes.get(p)) {
                r.x -= p.x;
                r.y -= p.y;
            }
            Some((n.id.clone(), r))
        })
        .collect();
    let held = (!enclosed.is_empty())
        .then(|| compute_counted_unclocked(&working, Some(&earlier), &redo, plain).ok())
        .flatten()
        .and_then(|(s, stages)| Some((dissolve(s, graph)?, stages)));
    let (mut snapshot, stages) = match held {
        Some(done) => done,
        None => {
            let mut redo = dirty.clone();
            redo.extend(owners(previous));
            compute_counted_unclocked(graph, Some(previous), &redo, plain)?
        }
    };
    kept(&mut snapshot, options, &previous.held_groups);
    // What was released stays so; a group the edit released says it.
    let held = &snapshot.held_groups;
    let released = found
        .iter()
        .filter(|c| !held.contains(&c.held) && !whole(&snapshot, options, c))
        .map(|c| {
            let before = previous.holding.iter().flat_map(|h| &h.released);
            let before = before
                .filter(|r| r.dimension == c.held.dimension && r.group == c.held.group)
                .map(|r| r.reason)
                .next();
            let reason = fixed(graph, options, c).or(before);
            c.released(reason.unwrap_or(ReleaseReason::Edit))
        })
        .collect();
    snapshot.holding = Some(HoldingReport {
        released,
        cost: None,
    });
    Ok((snapshot, stages))
}

/// The areas a layout that continues `earlier` has to place again: the owners of held groups have no plan to keep.
pub(crate) fn owners(earlier: &Snapshot) -> Vec<String> {
    let parent = parents(&earlier.graph);
    let all = candidates(&earlier.graph, &earlier.options);
    earlier
        .held_groups
        .iter()
        .filter_map(|h| all.iter().find(|c| c.held == *h))
        .filter_map(|c| common_parent(&parent, &c.members)?.map(str::to_string))
        .collect()
}

/// A layout that continues an earlier one, with the groups held there that are still whole.
pub(crate) fn kept(snapshot: &mut Snapshot, options: &LayoutOptions, earlier: &[HeldGroup]) {
    snapshot.options.hold_memberships = true;
    let all = candidates(&snapshot.graph, options);
    snapshot.held_groups = earlier
        .iter()
        .filter(|h| {
            all.iter().any(|c| {
                c.held == **h
                    && common_parent(&parents(&snapshot.graph), &c.members).is_some()
                    && whole(snapshot, options, c)
            })
        })
        .cloned()
        .collect();
}
