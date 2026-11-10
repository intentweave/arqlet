//! Exact reuse of a region's local coordinates after its children have been realized.
use crate::{layered_plan, placement_structure, CapacitySeed};
use arqlet_model::{Node, Orientation, Rect};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Borders {
    ranks: Vec<Vec<String>>,
    borders: BTreeMap<String, String>,
    geometry: BTreeMap<String, [u64; 4]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Input {
    structure: placement_structure::Input,
    levels: BTreeMap<usize, Vec<String>>,
    sizes: BTreeMap<String, [u64; 2]>,
    gaps: BTreeMap<(String, String), u64>,
    children: BTreeMap<String, Borders>,
    fixed: bool,
    optimize: usize,
}

impl Input {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        area: &str,
        structure: placement_structure::Input,
        levels: &BTreeMap<usize, Vec<String>>,
        sizes: &BTreeMap<String, (f64, f64)>,
        gaps: &BTreeMap<(String, String), f64>,
        children: &[&Node],
        plans: &layered_plan::LayeredPlans,
        optimize: usize,
    ) -> Self {
        let elements: BTreeSet<&str> = levels.values().flatten().map(String::as_str).collect();
        Self {
            structure,
            levels: levels.clone(),
            sizes: sizes
                .iter()
                .map(|(id, (w, h))| (id.clone(), [w.to_bits(), h.to_bits()]))
                .collect(),
            gaps: gaps
                .iter()
                .filter(|((a, _), _)| a == area || elements.contains(a.as_str()))
                .map(|(key, extra)| (key.clone(), extra.to_bits()))
                .collect(),
            children: children
                .iter()
                .filter_map(|child| {
                    let plan = plans.get(&child.id)?;
                    Some((
                        child.id.clone(),
                        Borders {
                            ranks: plan.ranks.clone(),
                            borders: plan.borders.clone(),
                            geometry: plan
                                .borders
                                .values()
                                .filter_map(|id| {
                                    let r = plan.dummies.get(id)?;
                                    Some((
                                        id.clone(),
                                        [
                                            r.x.to_bits(),
                                            r.y.to_bits(),
                                            r.width.to_bits(),
                                            r.height.to_bits(),
                                        ],
                                    ))
                                })
                                .collect(),
                        },
                    ))
                })
                .collect(),
            fixed: plans.get(area).is_some_and(|p| p.fixed),
            optimize,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Realization {
    pub boxes: BTreeMap<String, Rect>,
    pub size: (f64, f64),
    pub plan: layered_plan::AreaPlan,
    pub capacity_seeds: Vec<CapacitySeed>,
    pub orientation: Option<Orientation>,
}

/// Bound to the same immutable graph/options and one compute_once as the structural memo.
#[derive(Default)]
pub(crate) struct Memo {
    areas: BTreeMap<String, (Input, Realization)>,
    #[cfg(test)]
    pub disabled: bool,
}

impl Memo {
    pub fn get(&self, area: &str, input: &Input) -> Option<Realization> {
        #[cfg(test)]
        if self.disabled {
            return None;
        }
        self.areas
            .get(area)
            .filter(|(previous, _)| previous == input)
            .map(|(_, result)| result.clone())
    }

    pub fn insert(&mut self, area: &str, input: Input, result: Realization) {
        #[cfg(test)]
        if self.disabled {
            return;
        }
        self.areas.insert(area.to_string(), (input, result));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::GraphIndex;
    use arqlet_model::{Graph, LayoutOptions};

    #[test]
    fn channel_growth_reuses_unaffected_region_but_realizes_its_ancestors() {
        let graph: Graph = serde_json::from_value(serde_json::json!({
            "nodes": [
                {"id":"left","kind":"group","label":"Left"},
                {"id":"right","kind":"group","label":"Right"},
                {"id":"a","kind":"node","label":"A","parentId":"left"},
                {"id":"b","kind":"node","label":"B","parentId":"left"},
                {"id":"c","kind":"node","label":"C","parentId":"right"},
                {"id":"d","kind":"node","label":"D","parentId":"right"}
            ],
            "edges": [
                {"id":"ab","kind":"link","source":"a","target":"b"},
                {"id":"cd","kind":"link","source":"c","target":"d"}
            ]
        }))
        .unwrap();
        let index = GraphIndex::new(&graph);
        let options = LayoutOptions::default();
        let mut cached = crate::placement_order::Memo::default();
        let mut uncached = crate::placement_order::Memo::default();
        uncached.structure.disabled = true;
        uncached.regions.disabled = true;
        let mut cached_plans = layered_plan::LayeredPlans::new();
        let mut uncached_plans = layered_plan::LayeredPlans::new();
        let mut gaps = BTreeMap::new();
        for round in 0..4 {
            if round == 3 {
                gaps.insert(crate::channel_key("left", 0), 80.);
            }
            let mut work = crate::SearchCounters::default();
            let mut reference_work = crate::SearchCounters::default();
            let mut boxes = BTreeMap::new();
            let mut reference_boxes = BTreeMap::new();
            let size = crate::place(
                None,
                &graph,
                &index,
                &mut boxes,
                true,
                &BTreeSet::new(),
                &mut Vec::new(),
                &mut BTreeMap::new(),
                3,
                &options,
                &BTreeMap::new(),
                &gaps,
                &mut work,
                &mut cached_plans,
                &mut cached,
                &mut Vec::new(),
            );
            let reference_size = crate::place(
                None,
                &graph,
                &index,
                &mut reference_boxes,
                true,
                &BTreeSet::new(),
                &mut Vec::new(),
                &mut BTreeMap::new(),
                3,
                &options,
                &BTreeMap::new(),
                &gaps,
                &mut reference_work,
                &mut uncached_plans,
                &mut uncached,
                &mut Vec::new(),
            );
            assert_eq!(size, reference_size);
            assert_eq!(
                serde_json::to_vec(&boxes).unwrap(),
                serde_json::to_vec(&reference_boxes).unwrap()
            );
            assert_eq!(format!("{cached_plans:?}"), format!("{uncached_plans:?}"));
            if round == 2 {
                assert_eq!(work.region_realizations, 0);
                assert_eq!(work.reused_region_realizations, 3);
            }
            if round == 3 {
                assert_eq!(work.structural_plans, 0);
                assert_eq!(work.reused_structural_plans, 3);
                assert_eq!(work.region_realizations, 2);
                assert_eq!(work.reused_region_realizations, 1);
                assert!(boxes["left"].width > boxes["right"].width);
            }
        }
    }

    #[test]
    fn local_dependencies_invalidate_but_unrelated_channel_growth_does_not() {
        let graph: Graph =
            serde_json::from_str(include_str!("../../../examples/fixtures/small.json")).unwrap();
        let index = GraphIndex::new(&graph);
        let children: Vec<_> = index
            .children_of(None)
            .iter()
            .map(|&i| index.nodes[i])
            .collect();
        let options = LayoutOptions::default();
        let mut plans = layered_plan::LayeredPlans::new();
        plans.insert(
            "root".into(),
            layered_plan::AreaPlan {
                borders: BTreeMap::from([("ab".into(), "border".into())]),
                dummies: BTreeMap::from([(
                    "border".into(),
                    Rect {
                        x: 24.,
                        y: 48.,
                        width: 12.,
                        height: 12.,
                    },
                )]),
                ranks: vec![vec!["border".into()]],
                ..Default::default()
            },
        );
        let structure = placement_structure::Input::new(
            None,
            &children,
            &graph,
            &index,
            &options,
            &plans,
            &BTreeMap::new(),
        );
        let levels = BTreeMap::from([(0, vec!["root".into()])]);
        let sizes = BTreeMap::from([("root".into(), (200., 100.))]);
        let mut gaps = BTreeMap::new();
        let make = |gaps: &BTreeMap<_, _>, plans: &layered_plan::LayeredPlans| {
            Input::new(
                "<root>",
                structure.clone(),
                &levels,
                &sizes,
                gaps,
                &children,
                plans,
                3,
            )
        };
        let initial = make(&gaps, &plans);
        gaps.insert(crate::channel_key("unrelated", 0), 50.);
        assert_eq!(initial, make(&gaps, &plans));
        gaps.insert(crate::channel_key("<root>", 0), 50.);
        assert_ne!(initial, make(&gaps, &plans));
        gaps.clear();
        plans
            .get_mut("root")
            .unwrap()
            .dummies
            .get_mut("border")
            .unwrap()
            .x += 1.;
        assert_ne!(initial, make(&gaps, &plans));
        let mut changed = initial.clone();
        changed.sizes.get_mut("root").unwrap()[0] = 201_f64.to_bits();
        assert_ne!(initial, changed);
        let mut changed = initial.clone();
        changed.fixed = true;
        assert_ne!(initial, changed);
        let mut memo = Memo::default();
        memo.insert(
            "<root>",
            initial.clone(),
            Realization {
                boxes: BTreeMap::new(),
                size: (200., 100.),
                plan: Default::default(),
                capacity_seeds: Vec::new(),
                orientation: None,
            },
        );
        assert!(memo.get("<root>", &initial).is_some());
        assert!(memo.get("<root>", &changed).is_none());
        assert!(memo.get("other", &initial).is_none());
    }
}
