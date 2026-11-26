//! Provisional (ADR 0022 M4a): the groups of the first criterion as lanes (`as: lanes`), bands across the flow of the top
//! level. The projection makes no containers of them (`communities::with_grouping_priority`); every rank keeps the lanes as
//! consecutive blocks in their declared order, dummies of long edges included, and after the placement across the flow each
//! lane is moved behind the one before it, so that no element of a lane stands in the band of another.
//!
//! M4b: the groups of the second criterion `as: lanes` are phases along the flow of the top level. The ranking takes the phases
//! one after the other (an edge from a later phase to an earlier one is turned for the layout and keeps its direction), and a
//! phase begins in the rank after the last of the one before.

use arqlet_model::{
    grouping::{self, Owner, PARENT},
    Graph, GroupingRendering, LayoutOptions, Rect,
};
use std::collections::BTreeMap;

/// Room between two bands, across the flow.
const BETWEEN: f64 = 40.;

/// Whether the first criterion is drawn as lanes.
pub(crate) fn applies(options: &LayoutOptions) -> bool {
    options
        .grouping_priority
        .first()
        .is_some_and(|c| c.by != PARENT && c.render == GroupingRendering::Lanes)
}

/// Provisional (ADR 0022 M4b): the phase of every node at the top level, when the first criterion is drawn as lanes and the
/// second too: the
/// earliest group of its dimension, in the declared order, that names the node or a node below it as a member; a nested group
/// is a phase of its own. A node of no group comes after all phases. Empty without such a criterion.
pub(crate) fn phases(graph: &Graph, options: &LayoutOptions) -> BTreeMap<String, usize> {
    // Phases make a grid with the lanes of the first criterion; below another first criterion they would be lanes inside each
    // owner, which is not built (M4).
    if !applies(options) {
        return BTreeMap::new();
    }
    let Some(criterion) = options
        .grouping_priority
        .get(1)
        .filter(|c| c.by != PARENT && c.render == GroupingRendering::Lanes)
    else {
        return BTreeMap::new();
    };
    let Some(dimension) = graph.memberships.dimension(&criterion.by) else {
        return BTreeMap::new();
    };
    let mut phase = grouping::top_level_phases(graph, &criterion.by);
    for n in graph.nodes.iter().filter(|n| n.parent_id.is_none()) {
        phase.entry(n.id.clone()).or_insert(dimension.groups.len());
    }
    phase
}

/// Provisional (ADR 0022 M4d): whether the second criterion is drawn as lanes inside every area, behind a first criterion that
/// is not: the containers it gives (the parent hierarchy, or a dimension's boxes) are cut into bands.
pub(crate) fn inner(options: &LayoutOptions) -> bool {
    options
        .grouping_priority
        .first()
        .is_some_and(|c| c.render != GroupingRendering::Lanes || c.by == PARENT)
        && options
            .grouping_priority
            .get(1)
            .is_some_and(|c| c.by != PARENT && c.render == GroupingRendering::Lanes)
}

/// The lanes of the children of `area`, where it has any: the first criterion's at the top level (M4a), or the second's in
/// every area (M4d); a child of no lane comes after all of them.
pub(crate) fn of_area(
    graph: &Graph,
    options: &LayoutOptions,
    area: Option<&str>,
) -> Option<BTreeMap<String, usize>> {
    if area.is_none() && applies(options) {
        return Some(of_nodes(graph, options));
    }
    if !inner(options) {
        return None;
    }
    let by = &options.grouping_priority[1].by;
    let count = graph.memberships.dimension(by)?.groups.len();
    let mut lanes = grouping::area_groups(graph, by, area);
    for n in graph
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == area)
    {
        lanes.entry(n.id.clone()).or_insert(count);
    }
    Some(lanes)
}

/// The lane of every node at the top level, by its group's place in the dimension; a node of no group comes after all of them.
pub(crate) fn of_nodes(graph: &Graph, options: &LayoutOptions) -> BTreeMap<String, usize> {
    let Some(dimension) = options
        .grouping_priority
        .first()
        .and_then(|c| graph.memberships.dimension(&c.by))
    else {
        return BTreeMap::new();
    };
    let place: BTreeMap<&str, usize> = dimension
        .groups
        .iter()
        .enumerate()
        .map(|(i, g)| (g.id.as_str(), i))
        .collect();
    let ownership = grouping::owners(graph, &options.grouping_priority);
    graph
        .nodes
        .iter()
        .filter(|n| n.parent_id.is_none())
        .map(|n| {
            let lane = match ownership.owners.get(&n.id) {
                Some(Owner::Group {
                    dimension: d,
                    group,
                }) if *d == dimension.id => place.get(group.as_str()).copied(),
                _ => None,
            };
            (n.id.clone(), lane.unwrap_or(dimension.groups.len()))
        })
        .collect()
}

/// The lanes of the dummies of every chain: the earlier of its ends' lanes. Provisional: of six rules it drew the fewest
/// crossings on twelve views, closely before the source's lane (`benchmarks/results/memberships-20261125-lanes-engine.md`).
pub(crate) fn extend(
    lanes: &mut BTreeMap<String, usize>,
    chains: &BTreeMap<(String, String), Vec<String>>,
) {
    for ((a, b), dummies) in chains {
        let (Some(&from), Some(&to)) = (lanes.get(a), lanes.get(b)) else {
            continue;
        };
        for dummy in dummies {
            lanes.insert(dummy.clone(), from.min(to));
        }
    }
}

/// Moves each lane across the flow behind the one before it. Every lane moves as a whole and at least as far as the one
/// before, so the order and the gaps of a rank stay. Returns how far the last one reaches.
pub(crate) fn separate(
    placed: &mut BTreeMap<String, Rect>,
    lanes: &BTreeMap<String, usize>,
    vertical: bool,
) -> f64 {
    let mut extent: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
    for (id, r) in placed.iter() {
        if let Some(&lane) = lanes.get(id) {
            let (lo, hi) = if vertical {
                (r.x, r.x + r.width)
            } else {
                (r.y, r.y + r.height)
            };
            let e = extent.entry(lane).or_insert((lo, hi));
            *e = (e.0.min(lo), e.1.max(hi));
        }
    }
    let mut shift: BTreeMap<usize, f64> = BTreeMap::new();
    let (mut last, mut end) = (0_f64, f64::NEG_INFINITY);
    for (&lane, &(lo, hi)) in &extent {
        last = last.max(end + BETWEEN - lo);
        shift.insert(lane, last);
        end = hi + last;
    }
    for (id, r) in placed.iter_mut() {
        if let Some(by) = lanes.get(id).and_then(|l| shift.get(l)) {
            if vertical {
                r.x += by;
            } else {
                r.y += by;
            }
        }
    }
    end.max(0.)
}
