//! Placement's coordinate-free ordering phase. Reuse is local to one layout pass.
use crate::layered_plan::{self, AreaPlan};
use arqlet_model::OrderHint;
use std::collections::{BTreeMap, BTreeSet};

type Levels = BTreeMap<usize, Vec<String>>;
type Links = BTreeSet<(String, String)>;

#[derive(Clone, PartialEq, Eq)]
struct Input {
    levels: Levels,
    links: Links,
    components: BTreeMap<String, usize>,
    optimize: usize,
    layered: bool,
    earlier: Option<(Vec<Vec<String>>, bool, bool)>,
    hints: BTreeMap<String, OrderHint>,
}

#[derive(Default)]
pub(crate) struct Memo {
    areas: BTreeMap<String, (Input, Levels)>,
    /// The links the cycle break reverses in every area, when they are known before the first placement (single pass):
    /// a crossing edge then takes the border sides of its final placement from the start.
    pub reversals: std::rc::Rc<BTreeMap<String, Links>>,
    /// A pass that only orders (single pass): every area records its plan from the structure and the order, without
    /// coordinates, for the hierarchical ordering that the one placement with coordinates follows.
    pub order_only: bool,
    pub coordinates: std::cell::RefCell<crate::placement_geometry::Memo>,
    pub structure: crate::placement_structure::Memo,
    pub regions: crate::placement_region::Memo,
    #[cfg(test)]
    pub disabled: bool,
}

impl Memo {
    #[allow(clippy::too_many_arguments)]
    pub fn order(
        &mut self,
        area: &str,
        levels: &mut Levels,
        links: &Links,
        components: &BTreeMap<String, usize>,
        optimize: usize,
        layered: bool,
        earlier: Option<&AreaPlan>,
        hints: &BTreeMap<String, OrderHint>,
    ) -> bool {
        #[cfg(test)]
        if self.disabled {
            compute(levels, links, components, optimize, layered, earlier, hints);
            return false;
        }
        // Only the hints of this area's elements are part of its input.
        let hints: BTreeMap<String, OrderHint> = hints
            .iter()
            .filter(|(id, _)| levels.values().any(|ids| ids.contains(id)))
            .map(|(id, hint)| (id.clone(), hint.clone()))
            .collect();
        if let Some((previous, ordered)) = self.areas.get(area) {
            let earlier = earlier
                .filter(|_| layered)
                .map(|p| (&p.ranks, p.fixed, p.fixed_borders));
            if previous.levels == *levels
                && previous.links == *links
                && previous.components == *components
                && previous.optimize == optimize
                && previous.layered == layered
                && previous
                    .earlier
                    .as_ref()
                    .map(|(ranks, fixed, borders)| (ranks, *fixed, *borders))
                    == earlier
                && previous.hints == hints
            {
                levels.clone_from(ordered);
                return true;
            }
        }
        let input = Input {
            levels: levels.clone(),
            links: links.clone(),
            components: components.clone(),
            optimize,
            layered,
            earlier: earlier
                .filter(|_| layered)
                .map(|p| (p.ranks.clone(), p.fixed, p.fixed_borders)),
            hints: hints.clone(),
        };
        compute(
            levels, links, components, optimize, layered, earlier, &hints,
        );
        self.areas.insert(area.to_string(), (input, levels.clone()));
        false
    }
}

pub(crate) fn compute(
    levels: &mut Levels,
    links: &Links,
    components: &BTreeMap<String, usize>,
    optimize: usize,
    layered: bool,
    earlier: Option<&AreaPlan>,
    hints: &BTreeMap<String, OrderHint>,
) {
    if optimize > 0 {
        let mut before: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
        let mut after: BTreeMap<&String, Vec<&String>> = BTreeMap::new();
        let ranks: BTreeMap<&String, usize> = levels
            .iter()
            .flat_map(|(&rank, ids)| ids.iter().map(move |id| (id, rank)))
            .collect();
        for (a, b) in links {
            if ranks[a] < ranks[b] {
                before.entry(b).or_default().push(a);
                after.entry(a).or_default().push(b);
            }
        }
        for sweep in 0..4 {
            let mut positions: BTreeMap<String, f64> = levels
                .values()
                .flat_map(|ids| ids.iter().enumerate().map(|(i, id)| (id.clone(), i as f64)))
                .collect();
            let backward = matches!(optimize, 2 | 4) && sweep % 2 == 0;
            let mut order: Vec<_> = levels.keys().copied().collect();
            if backward {
                order.reverse();
            }
            for rank in order {
                let ids = levels.get_mut(&rank).unwrap();
                let neighbours = if backward { &after } else { &before };
                let score = |id: &String| {
                    let mut ns: Vec<f64> = neighbours
                        .get(id)
                        .into_iter()
                        .flatten()
                        .filter_map(|n| positions.get(*n).copied())
                        .collect();
                    if ns.is_empty() {
                        positions[id]
                    } else if optimize >= 3 {
                        ns.sort_by(f64::total_cmp);
                        (ns[(ns.len() - 1) / 2] + ns[ns.len() / 2]) / 2.
                    } else {
                        ns.iter().sum::<f64>() / ns.len() as f64
                    }
                };
                let keys: Vec<(usize, f64, f64)> = ids
                    .iter()
                    .map(|id| {
                        (
                            components.get(id).copied().unwrap_or(0),
                            score(id),
                            positions[id],
                        )
                    })
                    .collect();
                let mut sorted: Vec<usize> = (0..ids.len()).collect();
                sorted.sort_by(|&a, &b| {
                    keys[a]
                        .0
                        .cmp(&keys[b].0)
                        .then_with(|| keys[a].1.total_cmp(&keys[b].1))
                        .then_with(|| keys[a].2.total_cmp(&keys[b].2))
                        .then_with(|| ids[a].cmp(&ids[b]))
                });
                *ids = sorted.iter().map(|&i| ids[i].clone()).collect();
                for (i, id) in ids.iter().enumerate() {
                    positions.insert(id.clone(), i as f64);
                }
            }
        }
    }
    if layered {
        let mut ordered: Vec<Vec<String>> = levels.values().cloned().collect();
        let sections: Vec<(String, String)> = links.iter().cloned().collect();
        let group = |id: &str| components.get(id).copied().unwrap_or(0);
        let given = earlier
            .filter(|p| p.fixed)
            .map(|p| layered_plan::follow_order(&ordered, &p.ranks, &|_| 0));
        let is_given = given.is_some();
        let mut frozen = Vec::new();
        if let Some(given) = given {
            ordered = given;
        } else if let Some(earlier) = earlier.filter(|p| p.fixed_borders) {
            // Start from the given order and keep the ranks of border nodes, which agree with the plans outside.
            ordered = layered_plan::follow_order(&ordered, &earlier.ranks, &|_| 0);
            let borders: BTreeSet<&String> = earlier.borders.values().collect();
            frozen = ordered
                .iter()
                .map(|ids| !ids.is_empty() && ids.iter().all(|id| borders.contains(id)))
                .collect();
        } else if let Some(earlier) = earlier {
            let kept = layered_plan::follow_order(&ordered, &earlier.ranks, &group);
            if layered_plan::order_crossings(&kept, &sections)
                <= layered_plan::order_crossings(&ordered, &sections)
            {
                ordered = kept;
            }
        }
        if !is_given {
            layered_plan::improve_order_around(&mut ordered, &sections, &group, &frozen);
        }
        for (ids, better) in levels.values_mut().zip(ordered) {
            *ids = better;
        }
        follow_hints(levels, hints);
    }
}

/// Puts every hinted node right after its sibling, or first, in its rank: the order a user chose wins over the one the
/// ordering found, after all of the ordering. A hint whose sibling is in another rank, or not in this area, has no effect.
fn follow_hints(levels: &mut Levels, hints: &BTreeMap<String, OrderHint>) {
    for ids in levels.values_mut() {
        for (node, hint) in hints {
            let Some(from) = ids.iter().position(|id| id == node) else {
                continue;
            };
            let after = match &hint.after {
                None => None,
                Some(sibling) => match ids.iter().position(|id| id == sibling) {
                    Some(at) => Some(at),
                    None => continue,
                },
            };
            let id = ids.remove(from);
            let at = match after {
                None => 0,
                Some(at) if at > from => at,
                Some(at) => at + 1,
            };
            ids.insert(at, id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuse_requires_all_ordering_inputs_but_not_coordinates() {
        let levels: Levels = [
            (0, vec!["a".into(), "b".into()]),
            (1, vec!["c".into(), "d".into()]),
        ]
        .into_iter()
        .collect();
        let links = BTreeSet::from([("a".into(), "d".into()), ("b".into(), "c".into())]);
        let components = BTreeMap::new();
        let mut memo = Memo::default();
        let none = BTreeMap::new();
        for mode in 0..=4 {
            let mut first = levels.clone();
            assert!(!memo.order(
                "area",
                &mut first,
                &links,
                &components,
                mode,
                true,
                None,
                &none
            ));
            let mut second = levels.clone();
            assert!(memo.order(
                "area",
                &mut second,
                &links,
                &components,
                mode,
                true,
                None,
                &none
            ));
            assert_eq!(first, second);
        }
        let mut previous = AreaPlan::default();
        previous.ranks = vec![vec!["b".into(), "a".into()], vec!["d".into(), "c".into()]];
        for fixed in [false, true] {
            previous.fixed = fixed;
            let mut first = levels.clone();
            assert!(!memo.order(
                "area",
                &mut first,
                &links,
                &components,
                4,
                true,
                Some(&previous),
                &none
            ));
            previous.dummies.insert(
                "dummy".into(),
                arqlet_model::Rect {
                    x: 42.,
                    y: 1.,
                    width: 10.,
                    height: 20.,
                },
            );
            let mut second = levels.clone();
            assert!(memo.order(
                "area",
                &mut second,
                &links,
                &components,
                4,
                true,
                Some(&previous),
                &none
            ));
            assert_eq!(first, second);
        }
        for changed in 0..5 {
            let mut l = levels.clone();
            let mut e = links.clone();
            let mut c = components.clone();
            let mut p = previous.clone();
            match changed {
                0 => l.get_mut(&0).unwrap().reverse(),
                1 => {
                    e.remove(&("a".into(), "d".into()));
                }
                2 => {
                    c.insert("a".into(), 1);
                }
                3 => p.ranks[0].reverse(),
                _ => {}
            }
            assert!(!memo.order("area", &mut l, &e, &c, 4, changed != 4, Some(&p), &none));
        }
    }

    #[test]
    fn a_hint_puts_its_node_after_its_sibling_and_is_part_of_the_reused_input() {
        let levels: Levels = [(0, vec!["a".into(), "b".into(), "c".into(), "d".into()])]
            .into_iter()
            .collect();
        let hint = |after: Option<&str>| OrderHint {
            after: after.map(str::to_string),
        };
        for (node, after, expected) in [
            ("d", Some("a"), ["a", "d", "b", "c"]),
            ("a", Some("c"), ["b", "c", "a", "d"]),
            ("c", None, ["c", "a", "b", "d"]),
            ("b", Some("missing"), ["a", "b", "c", "d"]),
        ] {
            let mut ordered = levels.clone();
            follow_hints(
                &mut ordered,
                &BTreeMap::from([(node.to_string(), hint(after))]),
            );
            assert_eq!(ordered[&0], expected, "{node} after {after:?}");
        }
        // Through the ordering: the hint holds whatever the ordering chose, and a changed hint is not served from memory.
        let links = BTreeSet::new();
        let mut memo = Memo::default();
        for (after, first) in [(Some("a"), "a"), (None, "d")] {
            let mut ordered = levels.clone();
            let hints = BTreeMap::from([("d".to_string(), hint(after))]);
            memo.order(
                "area",
                &mut ordered,
                &links,
                &BTreeMap::new(),
                4,
                true,
                None,
                &hints,
            );
            assert_eq!(ordered[&0][0], first);
            assert!(ordered[&0].contains(&"d".to_string()));
        }
    }

    #[test]
    fn full_drawings_match_uncached_ordering_with_feedback() {
        use crate::{compute_mode, Run};
        use arqlet_model::{Graph, LayoutOptions};
        let mut reuses = 0;
        for text in [
            include_str!("../../../examples/fixtures/small.json"),
            include_str!("../../../benchmarks/datasets/deployment-cluster.json"),
            include_str!("../../../benchmarks/datasets/org-chart.json"),
            include_str!("../../../benchmarks/datasets/petri-net.json"),
        ] {
            let graph: Graph = serde_json::from_str(text).unwrap();
            for mode in 0..=4 {
                for global in [false, true] {
                    let options = LayoutOptions {
                        global_layering: global,
                        single_pass_auto: false,
                        single_pass: false,
                        abstract_routing: false,
                        ..LayoutOptions::default()
                    };
                    let mut cached = Run::new(&|| false);
                    let mut uncached = Run::new(&|| false);
                    uncached.disable_phase_reuse = true;
                    let a =
                        compute_mode(&graph, None, &BTreeSet::new(), mode, &options, &mut cached)
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
                        "mode={mode}, global={global}"
                    );
                    assert_eq!(
                        cached.search.placement_passes,
                        uncached.search.placement_passes
                    );
                    cached.search.construction.preparation_nanos = 0;
                    cached.search.construction.search_nanos = 0;
                    uncached.search.construction.preparation_nanos = 0;
                    uncached.search.construction.search_nanos = 0;
                    assert!(
                        cached.search.construction.searches
                            <= uncached.search.construction.searches
                    );
                    assert_eq!(
                        cached.search.ordering_passes + cached.search.reused_orderings,
                        uncached.search.ordering_passes
                    );
                    reuses += cached.search.reused_orderings;
                }
            }
        }
        assert!(
            reuses > 0,
            "fixtures must exercise exact reuse across coordinate passes"
        );
    }
}
