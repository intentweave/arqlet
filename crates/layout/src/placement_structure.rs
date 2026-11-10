//! Region structure is independent of node sizes, coordinates and channel gaps.
use crate::{direction_for, layered_plan, GraphIndex};
use arqlet_model::{Graph, LayoutOptions, Mode, Node};
use std::collections::{BTreeMap, BTreeSet};

type Links = BTreeSet<(String, String)>;

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Plan {
    pub links: Links,
    pub levels: BTreeMap<usize, Vec<String>>,
    pub order_links: Links,
    pub chains: BTreeMap<(String, String), Vec<String>>,
    pub reversed_links: Links,
    pub borders: BTreeMap<String, String>,
    pub sides: BTreeMap<String, String>,
    pub link_edges: BTreeMap<(String, String), Vec<String>>,
    pub virtual_sizes: BTreeMap<String, (f64, f64)>,
    pub components: BTreeMap<String, usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Input {
    children: Vec<String>,
    layered: bool,
    hierarchical: bool,
    simplex: bool,
    components: bool,
    track: u64,
    grid: u64,
    /// Only ancestor reversal decisions can change which border a crossing edge uses.
    ancestors: Vec<Links>,
    /// Single pass (ADR 0017, amendment A3): what decides which edges cross the parent beside this area (`through_edges`):
    /// whether this area stands first or last in its rank in the parent's plan, and the parent's side nodes.
    through: Option<(bool, bool, Vec<String>)>,
}

impl Input {
    pub fn new(
        parent: Option<&str>,
        children: &[&Node],
        graph: &Graph,
        index: &GraphIndex<'_>,
        options: &LayoutOptions,
        plans: &layered_plan::LayeredPlans,
        reversals: &BTreeMap<String, Links>,
    ) -> Self {
        let layered = options.layered_construction
            && matches!(
                direction_for(graph, parent),
                Mode::Orientation(_) | Mode::Chooser(_)
            );
        let same_axis = parent.is_some_and(|p| {
            let outer = index.node(p).and_then(|n| n.parent_id.as_deref());
            match (direction_for(graph, parent), direction_for(graph, outer)) {
                (Mode::Orientation(inner), Mode::Orientation(outer)) => {
                    inner.vertical() == outer.vertical() && !inner.reversed() && !outer.reversed()
                }
                _ => false,
            }
        });
        let mut ancestors = Vec::new();
        if layered && options.hierarchical_ordering && same_axis {
            let mut up = parent
                .and_then(|p| index.node(p))
                .and_then(|n| n.parent_id.as_deref());
            loop {
                ancestors.push(
                    layered_plan::reversed_links(plans, reversals, up.unwrap_or("<root>"))
                        .cloned()
                        .unwrap_or_default(),
                );
                up = match up {
                    Some(id) => index.node(id).and_then(|n| n.parent_id.as_deref()),
                    None => break,
                };
            }
        }
        let through = (options.single_pass)
            .then(|| {
                let middle = parent
                    .and_then(|p| index.node(p))
                    .and_then(|n| n.parent_id.as_deref())?;
                // Only an area of the other axis than its parent and the axis of its parent's parent can have such edges.
                let above = index.node(middle).and_then(|n| n.parent_id.as_deref());
                let axis = |id: Option<&str>| match direction_for(graph, id) {
                    Mode::Orientation(o) if !o.reversed() => Some(o.vertical()),
                    _ => None,
                };
                let (own, mid) = (axis(parent), axis(Some(middle)));
                if own.is_none() || own == mid || own != axis(above) {
                    return None;
                }
                let plan = plans.get(middle)?;
                let area = parent?;
                let is_side = |id: &str| {
                    id.starts_with(crate::SIDE_IN_PREFIX) || id.starts_with(crate::SIDE_OUT_PREFIX)
                };
                let rank = plan.ranks.iter().find(|r| r.iter().any(|x| x == area))?;
                let inner: Vec<&String> = rank.iter().filter(|x| !is_side(x)).collect();
                Some((
                    inner.first().is_some_and(|x| x.as_str() == area),
                    inner.last().is_some_and(|x| x.as_str() == area),
                    plan.sides.keys().cloned().collect(),
                ))
            })
            .flatten();
        Self {
            through,
            children: children.iter().map(|n| n.id.clone()).collect(),
            layered,
            hierarchical: options.hierarchical_ordering,
            simplex: options.simplex_ranks,
            components: options.component_packing,
            track: options.track_spacing.to_bits(),
            grid: options.placement_grid.to_bits(),
            ancestors,
        }
    }
}

/// One graph/options context per compute_once; scratch flattening has its own memo.
#[derive(Default)]
pub(crate) struct Memo {
    areas: BTreeMap<String, (Input, Plan)>,
    #[cfg(test)]
    pub disabled: bool,
}

impl Memo {
    pub fn plan(
        &mut self,
        area: &str,
        input: Input,
        compute: impl FnOnce() -> Plan,
    ) -> (Plan, bool) {
        #[cfg(test)]
        if self.disabled {
            return (compute(), false);
        }
        if let Some((previous, plan)) = self.areas.get(area) {
            if previous == &input {
                return (plan.clone(), true);
            }
        }
        let plan = compute();
        self.areas.insert(area.to_string(), (input, plan.clone()));
        (plan, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{compute_mode, Run};
    use arqlet_model::{DirectionConstraint, NodeSize, Orientation};

    #[test]
    fn only_ancestor_reversals_change_boundary_structure() {
        let graph: Graph =
            serde_json::from_str(include_str!("../../../examples/fixtures/small.json")).unwrap();
        let index = GraphIndex::new(&graph);
        let children: Vec<_> = index
            .children_of(Some("root"))
            .iter()
            .map(|&i| index.nodes[i])
            .collect();
        let options = LayoutOptions {
            // The pipeline before single pass (ADR 0016), whose reuse these tests exercise.
            single_pass_auto: false,
            single_pass: false,
            abstract_routing: false,
            ..LayoutOptions::default()
        };
        let mut plans = layered_plan::LayeredPlans::new();
        let input = Input::new(
            Some("root"),
            &children,
            &graph,
            &index,
            &options,
            &plans,
            &BTreeMap::new(),
        );
        plans.insert(
            "root".into(),
            layered_plan::AreaPlan {
                reversed_links: BTreeSet::from([("a".into(), "b".into())]),
                ..Default::default()
            },
        );
        plans.insert("unrelated".into(), plans["root"].clone());
        assert_eq!(
            input,
            Input::new(
                Some("root"),
                &children,
                &graph,
                &index,
                &options,
                &plans,
                &BTreeMap::new()
            )
        );
        plans.insert("<root>".into(), plans["root"].clone());
        let changed = Input::new(
            Some("root"),
            &children,
            &graph,
            &index,
            &options,
            &plans,
            &BTreeMap::new(),
        );
        assert_ne!(input, changed);
        let mut memo = Memo::default();
        assert!(!memo.plan("root", input.clone(), Plan::default).1);
        assert!(
            memo.plan("root", input, || panic!("exact structural reuse"))
                .1
        );
        assert!(!memo.plan("root", changed, Plan::default).1);
    }

    #[test]
    fn rank_and_border_plans_match_uncached_for_modes_options_and_directions() {
        let source: Graph =
            serde_json::from_str(include_str!("../../../examples/fixtures/small.json")).unwrap();
        let mut computations = 0;
        for variant in 0..6 {
            let mut graph = source.clone();
            graph.container_directions.clear();
            graph.default_direction = match variant {
                1 => DirectionConstraint::Fixed(Orientation::Tb),
                2 => DirectionConstraint::Fixed(Orientation::Rl),
                3 => DirectionConstraint::Fixed(Orientation::Bt),
                5 => DirectionConstraint::Free,
                _ => DirectionConstraint::Fixed(Orientation::Lr),
            };
            let options = LayoutOptions {
                simplex_ranks: variant == 4,
                component_packing: variant == 5,
                node_sizes: graph
                    .nodes
                    .iter()
                    .filter(|n| index_is_leaf(&graph, n))
                    .map(|n| {
                        (
                            n.id.clone(),
                            NodeSize {
                                width: 137.5,
                                height: 61.25,
                            },
                        )
                    })
                    .collect(),
                single_pass_auto: false,
                single_pass: false,
                abstract_routing: false,
                ..LayoutOptions::default()
            };
            for mode in [0, 3, 4] {
                let mut cached = Run::new(&|| false);
                let mut uncached = Run::new(&|| false);
                uncached.disable_structure_reuse = true;
                uncached.disable_region_reuse = true;
                let a = compute_mode(&graph, None, &BTreeSet::new(), mode, &options, &mut cached)
                    .unwrap();
                let b = compute_mode(
                    &graph,
                    None,
                    &BTreeSet::new(),
                    mode,
                    &options,
                    &mut uncached,
                )
                .unwrap();
                assert_eq!(
                    serde_json::to_vec(&a).unwrap(),
                    serde_json::to_vec(&b).unwrap(),
                    "variant={variant}, mode={mode}"
                );
                assert_eq!(
                    cached.search.placement_passes,
                    uncached.search.placement_passes
                );
                assert_eq!(
                    cached.search.structural_plans + cached.search.reused_structural_plans,
                    uncached.search.structural_plans
                );
                assert_eq!(
                    cached.search.region_realizations + cached.search.reused_region_realizations,
                    uncached.search.region_realizations
                );
                assert_eq!(
                    cached.search.construction.states,
                    uncached.search.construction.states
                );
                assert_eq!(
                    cached.search.construction.transitions,
                    uncached.search.construction.transitions
                );
                computations += cached.search.structural_plans;
            }
        }
        assert!(computations > 0);
    }

    #[test]
    fn frozen_width_feedback_matches_without_structure_or_region_reuse() {
        let graph: Graph = serde_json::from_str(include_str!(
            "../../../examples/fixtures/phase-width-feedback.json"
        ))
        .unwrap();
        let options = LayoutOptions {
            // The pipeline before single pass (ADR 0016), whose reuse these tests exercise.
            single_pass_auto: false,
            single_pass: false,
            abstract_routing: false,
            ..LayoutOptions::default()
        };
        let mut reuses = 0;
        for mode in 0..=4 {
            let mut cached = Run::new(&|| false);
            let mut uncached = Run::new(&|| false);
            uncached.disable_structure_reuse = true;
            uncached.disable_region_reuse = true;
            let a =
                compute_mode(&graph, None, &BTreeSet::new(), mode, &options, &mut cached).unwrap();
            let b = compute_mode(
                &graph,
                None,
                &BTreeSet::new(),
                mode,
                &options,
                &mut uncached,
            )
            .unwrap();
            assert_eq!(
                serde_json::to_vec(&a).unwrap(),
                serde_json::to_vec(&b).unwrap(),
                "mode={mode}"
            );
            assert_eq!(
                cached.search.structural_plans + cached.search.reused_structural_plans,
                uncached.search.structural_plans
            );
            assert_eq!(
                cached.search.region_realizations + cached.search.reused_region_realizations,
                uncached.search.region_realizations
            );
            assert_eq!(
                cached.search.construction.states,
                uncached.search.construction.states
            );
            assert_eq!(
                cached.search.construction.transitions,
                uncached.search.construction.transitions
            );
            reuses += cached.search.reused_region_realizations;
        }
        assert!(reuses > 0);
    }

    fn index_is_leaf(graph: &Graph, node: &Node) -> bool {
        !graph
            .nodes
            .iter()
            .any(|n| n.parent_id.as_deref() == Some(&node.id))
    }
}
