//! Cross-flow lane planning, independent of flow-axis sizes and channel widths.
use arqlet_model::Rect;
use std::collections::{BTreeMap, BTreeSet};

type Levels = BTreeMap<usize, Vec<String>>;
type Links = BTreeSet<(String, String)>;
type Chains = BTreeMap<(String, String), Vec<String>>;
type Lanes = BTreeMap<(String, String), f64>;
type Across = BTreeMap<String, (f64, f64)>;

pub(crate) struct Input<'a> {
    pub levels: &'a Levels,
    pub links: &'a Links,
    pub chains: &'a Chains,
    pub edges: &'a Chains,
    pub lanes: &'a Lanes,
    pub across: Across,
    pub extent: f64,
    pub gap: f64,
    pub track: f64,
    pub free: bool,
    /// Port leads by element, low and high side (option `portSides`).
    pub room: Across,
}

#[derive(Clone)]
struct Key {
    levels: Levels,
    links: Links,
    chains: Chains,
    edges: Chains,
    lanes: Lanes,
    across: Across,
    extent: f64,
    gap: f64,
    track: f64,
    free: bool,
    room: Across,
}

impl Key {
    fn matches(&self, input: &Input<'_>) -> bool {
        self.levels == *input.levels
            && self.links == *input.links
            && self.chains == *input.chains
            && self.edges == *input.edges
            && self.lanes.len() == input.lanes.len()
            && self
                .lanes
                .iter()
                .zip(input.lanes)
                .all(|((ka, a), (kb, b))| ka == kb && a.to_bits() == b.to_bits())
            && self.across.len() == input.across.len()
            && self
                .across
                .iter()
                .zip(&input.across)
                .all(|((ka, a), (kb, b))| {
                    ka == kb && a.0.to_bits() == b.0.to_bits() && a.1.to_bits() == b.1.to_bits()
                })
            && self.extent.to_bits() == input.extent.to_bits()
            && self.gap.to_bits() == input.gap.to_bits()
            && self.track.to_bits() == input.track.to_bits()
            && self.free == input.free
            && self.room == input.room
    }

    fn new(input: Input<'_>) -> Self {
        Self {
            levels: input.levels.clone(),
            links: input.links.clone(),
            chains: input.chains.clone(),
            edges: input.edges.clone(),
            lanes: input.lanes.clone(),
            across: input.across,
            extent: input.extent,
            gap: input.gap,
            track: input.track,
            free: input.free,
            room: input.room,
        }
    }
}

#[derive(Clone)]
pub(crate) struct CrossPlan {
    pub across: Across,
    pub extent: f64,
    pub border_orders: Levels,
    pub slots: Lanes,
    pub tracks: Vec<usize>,
}

#[derive(Default)]
pub(crate) struct Memo {
    areas: BTreeMap<(String, bool), (Key, CrossPlan)>,
    #[cfg(test)]
    pub disabled: bool,
}

impl Memo {
    pub fn plan(
        &mut self,
        area: &str,
        vertical: bool,
        input: Input<'_>,
        compute: impl FnOnce() -> CrossPlan,
    ) -> (CrossPlan, bool) {
        #[cfg(test)]
        if self.disabled {
            return (compute(), false);
        }
        let area = (area.to_string(), vertical);
        if let Some((key, result)) = self.areas.get(&area) {
            if key.matches(&input) {
                return (result.clone(), true);
            }
        }
        let result = compute();
        self.areas.insert(area, (Key::new(input), result.clone()));
        (result, false)
    }
}

pub(crate) fn project(placed: &BTreeMap<String, Rect>, vertical: bool) -> Across {
    placed
        .iter()
        .map(|(id, r)| {
            (
                id.clone(),
                if vertical {
                    (r.x, r.width)
                } else {
                    (r.y, r.height)
                },
            )
        })
        .collect()
}

impl CrossPlan {
    pub fn apply(&self, placed: &mut BTreeMap<String, Rect>, vertical: bool) {
        for (id, r) in placed {
            let &(start, size) = self.across.get(id).expect("same cross-plan elements");
            if vertical {
                r.x = start;
                r.width = size;
            } else {
                r.y = start;
                r.height = size;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn input(key: &Key) -> Input<'_> {
        Input {
            levels: &key.levels,
            links: &key.links,
            chains: &key.chains,
            edges: &key.edges,
            lanes: &key.lanes,
            across: key.across.clone(),
            extent: key.extent,
            gap: key.gap,
            track: key.track,
            free: key.free,
            room: key.room.clone(),
        }
    }

    #[test]
    fn every_cross_input_invalidates_and_areas_axes_are_isolated() {
        let key = Key {
            levels: BTreeMap::from([(0, vec!["a".into()]), (1, vec!["b".into()])]),
            links: BTreeSet::from([("a".into(), "b".into())]),
            chains: BTreeMap::from([(("a".into(), "b".into()), Vec::new())]),
            edges: BTreeMap::from([(("a".into(), "b".into()), vec!["e".into()])]),
            lanes: BTreeMap::from([(("a".into(), "e".into()), 10.)]),
            across: BTreeMap::from([("a".into(), (24., 20.)), ("b".into(), (24., 30.))]),
            extent: 90.,
            gap: 24.,
            track: 12.,
            room: BTreeMap::new(),
            free: true,
        };
        let compute = || CrossPlan {
            across: key.across.clone(),
            extent: 90.,
            border_orders: BTreeMap::new(),
            slots: BTreeMap::new(),
            tracks: vec![1],
        };
        let mut memo = Memo::default();
        assert!(!memo.plan("area", false, input(&key), compute).1);
        assert!(
            memo.plan("area", false, input(&key), || panic!("exact reuse"))
                .1
        );
        assert!(!memo.plan("other", false, input(&key), compute).1);
        assert!(!memo.plan("area", true, input(&key), compute).1);
        for field in 0..11 {
            let mut changed = key.clone();
            match field {
                0 => changed.levels.get_mut(&0).unwrap().push("c".into()),
                1 => changed.links.clear(),
                2 => changed.chains.clear(),
                3 => changed.edges.clear(),
                4 => changed.lanes.clear(),
                5 => changed.across.get_mut("a").unwrap().0 += 1.,
                6 => changed.extent += 1.,
                7 => changed.gap += 1.,
                8 => changed.track += 1.,
                9 => changed.free = false,
                _ => changed.across.get_mut("a").unwrap().1 += 1.,
            }
            assert!(!key.matches(&input(&changed)), "field={field}");
        }
    }

    #[test]
    fn cross_projection_and_application_preserve_new_flow_coordinates_and_sizes() {
        for vertical in [false, true] {
            let original = BTreeMap::from([(
                "a".into(),
                Rect {
                    x: 24.,
                    y: 48.,
                    width: 120.,
                    height: 54.,
                },
            )]);
            let mut grown = original.clone();
            let r = grown.get_mut("a").unwrap();
            if vertical {
                r.y += 53.75;
                r.height += 12.25;
            } else {
                r.x += 53.75;
                r.width += 12.25;
            }

            assert_eq!(project(&original, vertical), project(&grown, vertical));
            let flow_before = grown.clone();
            let cross = CrossPlan {
                across: BTreeMap::from([("a".into(), (100., 80.))]),
                extent: 200.,
                border_orders: BTreeMap::new(),
                slots: BTreeMap::new(),
                tracks: Vec::new(),
            };
            cross.apply(&mut grown, vertical);
            let (before, after) = (&flow_before["a"], &grown["a"]);
            if vertical {
                assert_eq!((after.y, after.height), (before.y, before.height));
                assert_eq!((after.x, after.width), (100., 80.));
            } else {
                assert_eq!((after.x, after.width), (before.x, before.width));
                assert_eq!((after.y, after.height), (100., 80.));
            }
        }
    }

    #[test]
    fn signed_zero_is_not_an_exact_lane_match() {
        let empty = BTreeMap::new();
        let lanes = BTreeMap::from([(("a".into(), "e".into()), 0.)]);
        let mut i = Input {
            levels: &empty,
            links: &BTreeSet::new(),
            chains: &BTreeMap::new(),
            edges: &BTreeMap::new(),
            lanes: &lanes,
            across: BTreeMap::new(),
            extent: 0.,
            gap: 0.,
            track: 12.,
            free: false,
            room: BTreeMap::new(),
        };
        let key = Key::new(Input {
            across: BTreeMap::new(),
            room: BTreeMap::new(),
            ..i
        });
        i.extent = -0.;
        assert!(!key.matches(&i));
        i.extent = 0.;
        let changed = BTreeMap::from([(("a".into(), "e".into()), -0.)]);
        i.lanes = &changed;
        assert!(!key.matches(&i));
    }

    #[test]
    fn directions_sizes_and_cross_axis_options_match_uncached_drawings() {
        use crate::{compute_mode, Run};
        use arqlet_model::{DirectionConstraint, Graph, LayoutOptions, NodeSize, Orientation};
        let source: Graph =
            serde_json::from_str(include_str!("../../../examples/fixtures/small.json")).unwrap();
        for direction in [
            DirectionConstraint::Fixed(Orientation::Lr),
            DirectionConstraint::Fixed(Orientation::Rl),
            DirectionConstraint::Fixed(Orientation::Tb),
            DirectionConstraint::Fixed(Orientation::Bt),
            DirectionConstraint::Free,
            DirectionConstraint::Allowed(vec![Orientation::Lr, Orientation::Tb]),
        ] {
            let mut graph = source.clone();
            graph.default_direction = direction;
            for free in [false, true] {
                let options = LayoutOptions {
                    free_placement: free,
                    node_sizes: BTreeMap::from([
                        (
                            "a".into(),
                            NodeSize {
                                width: 83.5,
                                height: 67.25,
                            },
                        ),
                        (
                            "b".into(),
                            NodeSize {
                                width: 129.25,
                                height: 35.5,
                            },
                        ),
                    ]),
                    required_clearance: Some(11.25),
                    single_pass_auto: false,
                    single_pass: false,
                    abstract_routing: false,
                    ..LayoutOptions::default()
                };
                for mode in [0, 3, 4] {
                    let mut cached = Run::new(&|| false);
                    let mut uncached = Run::new(&|| false);
                    uncached.disable_coordinate_reuse = true;
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
                        serde_json::to_vec(&b).unwrap()
                    );
                }
            }
        }
    }
    #[test]
    fn width_feedback_drawings_match_with_cross_planning_disabled() {
        use crate::{compute_mode, Run};
        use arqlet_model::{Graph, LayoutOptions};
        let graph: Graph = serde_json::from_str(include_str!(
            "../../../examples/fixtures/phase-width-feedback.json"
        ))
        .unwrap();
        let mut reused = 0;
        for mode in 0..=4 {
            let mut cached = Run::new(&|| false);
            let mut uncached = Run::new(&|| false);
            uncached.disable_coordinate_reuse = true;
            let options = LayoutOptions {
                // The pipeline before single pass (ADR 0016), whose reuse these tests exercise.
                single_pass_auto: false,
                single_pass: false,
                abstract_routing: false,
                ..LayoutOptions::default()
            };
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
                cached.search.placement_passes,
                uncached.search.placement_passes
            );
            assert_eq!(
                cached.search.cross_plans + cached.search.reused_cross_plans,
                uncached.search.cross_plans
            );
            assert_eq!(
                cached.search.construction.states,
                uncached.search.construction.states
            );
            assert_eq!(
                cached.search.construction.transitions,
                uncached.search.construction.transitions
            );
            reused += cached.search.reused_cross_plans;
        }
        assert!(
            reused > 0,
            "the frozen view must exercise cross-flow plan reuse"
        );
    }
}
