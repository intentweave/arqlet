//! Provisional shared geometry for the chain probe and final coordination.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Work {
    pub span_computations: usize,
    pub span_reuses: usize,
    pub domain_computations: usize,
    pub domain_reuses: usize,
}

/// One context belongs to one immutable grown drawing. Never reuse these
/// projections after placement, growth, endpoint or plan changes.
pub(super) struct Context<'a, 'g> {
    geometry: &'a Geometry<'g>,
    enabled: bool,
    spans: RefCell<BTreeMap<(String, usize), Result<(f64, f64), Failure>>>,
    domains: RefCell<BTreeMap<(String, String), Result<Rect, Failure>>>,
    work: RefCell<Work>,
}

#[cfg(test)]
mod tests {
    use super::super::tests::{area, graph, ports, rect, SPACING};
    use super::*;

    fn trial(vertical: bool) -> Trial {
        Trial {
            prepared: Prepared {
                edge_id: "e".into(),
                from_node: "a".into(),
                to_node: "b".into(),
                reversed: false,
                vertical,
                from: (20., 15.),
                to: (200., 40.),
                band: (30., 190.),
                obstacles: Vec::new(),
                obstacle_index: std::rc::Rc::new(BoxIndex::new(&[])),
                start: to_point(vertical, (20., 15.)),
                end: to_point(vertical, (200., 40.)),
                lead: (Vec::new(), Vec::new()),
            },
            lanes: vec![15., 30., 40.],
            corners: Vec::new(),
            failure: None,
        }
    }

    fn edge() -> EdgePlan {
        EdgePlan {
            edge: "e".into(),
            source: "a".into(),
            target: Some("b".into()),
            reversed: false,
            sections: vec![
                abstract_routing::Section {
                    area: "group".into(),
                    channel: 0,
                    from: "a".into(),
                    to: "dummy".into(),
                },
                abstract_routing::Section {
                    area: "group".into(),
                    channel: 1,
                    from: "dummy".into(),
                    to: "b".into(),
                },
            ],
            boundaries: Vec::new(),
            unsupported: None,
            leg: None,
        }
    }

    #[test]
    fn projections_match_direct_geometry_and_endpoint_domains_remain_fresh() {
        let edge = edge();
        for i in 0..128 {
            let vertical = i % 2 == 1;
            let mut plans =
                LayeredPlans::from([("group".into(), area(&[&["a"], &["dummy"], &["b"]], &[]))]);
            plans.get_mut("group").unwrap().vertical = vertical;
            plans
                .get_mut("group")
                .unwrap()
                .dummies
                .insert("dummy".into(), rect(60. + i as f64 / 4., 80., 30., 40.));
            let boxes = BTreeMap::from([
                ("group".into(), rect(100. + i as f64, 200., 300., 400.)),
                ("a".into(), rect(120., 220., 10., 10.)),
                ("b".into(), rect(300. + i as f64, 500., 10., 10.)),
            ]);
            let geometry = Geometry {
                boxes: &boxes,
                plans: &plans,
            };
            let shared = Context::new(&geometry, true);
            let direct = Context::new(&geometry, false);
            let mut trial = trial(vertical);
            for pass in 0..4 {
                for rank in [0, 1] {
                    let a = shared.main_span("group", rank);
                    let b = direct.main_span("group", rank);
                    assert_eq!(a, b);
                    assert_eq!(a.unwrap().0.to_bits(), b.unwrap().0.to_bits());
                }
                trial.prepared.vertical = if pass % 2 == 0 { vertical } else { !vertical };
                trial.lanes[0] += 1.;
                trial.lanes[2] += 2.;
                for index in 0..3 {
                    assert_eq!(
                        shared.domain(&edge, &trial, index),
                        direct.domain(&edge, &trial, index)
                    );
                }
            }
            assert_eq!(
                shared.work(),
                Work {
                    span_computations: 2,
                    span_reuses: 6,
                    domain_computations: 1,
                    domain_reuses: 3
                }
            );
            assert_eq!(direct.work().span_computations, 8);
            assert_eq!(direct.work().domain_computations, 4);
        }
    }

    #[test]
    fn failed_projections_replay_exact_errors_and_a_new_context_sees_new_geometry() {
        let edge = edge();
        let mut plans =
            LayeredPlans::from([("group".into(), area(&[&["a"], &["dummy"], &["b"]], &[]))]);
        let mut boxes = BTreeMap::from([
            ("a".into(), rect(0., 0., 20., 20.)),
            ("b".into(), rect(200., 0., 20., 20.)),
        ]);
        {
            let geometry = Geometry {
                boxes: &boxes,
                plans: &plans,
            };
            let shared = Context::new(&geometry, true);
            for _ in 0..2 {
                assert_eq!(shared.main_span("group", 0), geometry.main_span("group", 0));
                assert!(shared.main_span("group", 0).is_err());
                assert_eq!(
                    shared.domain(&edge, &trial(false), 1),
                    lane_domain(&edge, &trial(false), &geometry, 1)
                );
                assert!(shared.domain(&edge, &trial(false), 1).is_err());
            }
            assert_eq!(shared.work().span_computations, 1);
            assert_eq!(shared.work().domain_computations, 1);
        }
        plans
            .get_mut("group")
            .unwrap()
            .dummies
            .insert("dummy".into(), rect(50., 60., 20., 30.));
        boxes.insert("group".into(), rect(10., 20., 300., 100.));
        let geometry = Geometry {
            boxes: &boxes,
            plans: &plans,
        };
        let shared = Context::new(&geometry, true);
        assert_eq!(shared.main_span("group", 0).unwrap(), (20., 60.));
        assert_eq!(
            shared.domain(&edge, &trial(false), 1).unwrap().1,
            (80., 110.)
        );
        assert_eq!(shared.work().span_computations, 1);
        assert_eq!(shared.work().domain_computations, 1);
    }

    #[test]
    fn shared_coordination_preserves_solver_witnesses_channels_and_failures() {
        let graph = graph(
            &[("a", None), ("b", None), ("c", None), ("d", None)],
            &[
                ("ac", "a", "c"),
                ("ad", "a", "d"),
                ("bc", "b", "c"),
                ("bd", "b", "d"),
                ("parallel", "a", "c"),
            ],
        );
        let plan = area(&[&["a", "b"], &["c", "d"]], &[]);
        let mut plans = LayeredPlans::from([("<root>".into(), plan)]);
        for vertical in [false, true] {
            plans.get_mut("<root>").unwrap().vertical = vertical;
            let mut graph = graph.clone();
            if vertical {
                graph.default_direction = DirectionConstraint::Fixed(Orientation::Tb);
            }
            for gap in [12., 24., 100.] {
                let mut boxes = BTreeMap::from([
                    ("a".into(), rect(0., 0., 20., 20.)),
                    ("b".into(), rect(0., 30., 20., 20.)),
                    ("c".into(), rect(20. + gap, 5., 20., 20.)),
                    ("d".into(), rect(20. + gap, 35., 20., 20.)),
                ]);
                let mut ports = ports(&graph, &boxes);
                if vertical {
                    for r in boxes.values_mut() {
                        std::mem::swap(&mut r.x, &mut r.y);
                        std::mem::swap(&mut r.width, &mut r.height);
                    }
                    for p in &mut ports {
                        std::mem::swap(&mut p.position.x, &mut p.position.y);
                        p.side = if p.endpoint == "source" {
                            PortSide::Bottom
                        } else {
                            PortSide::Top
                        };
                    }
                }
                let plan = abstract_routing::build(&graph, &plans).unwrap();
                for mode in [
                    Coordination::FixedTracks,
                    Coordination::Joint,
                    Coordination::Chains,
                ] {
                    let mut baseline = planning::Session::with_construction_reuse();
                    let plain = realize_plan(
                        &graph,
                        &boxes,
                        &ports,
                        &plans,
                        SPACING,
                        &plan,
                        mode,
                        false,
                        Some(&mut baseline),
                    )
                    .unwrap();
                    let mut session = planning::Session::with_coordination_reuse();
                    let shared = realize_plan(
                        &graph,
                        &boxes,
                        &ports,
                        &plans,
                        SPACING,
                        &plan,
                        mode,
                        false,
                        Some(&mut session),
                    )
                    .unwrap();
                    assert_eq!(
                        serde_json::to_value(&plain.routes).unwrap(),
                        serde_json::to_value(&shared.routes).unwrap()
                    );
                    assert_eq!(plain.delegated, shared.delegated);
                    assert_eq!(plain.channels, shared.channels);
                    assert_eq!(plain.joint, shared.joint);
                    assert_eq!(plain.chains, shared.chains);
                    assert_eq!(plain.continuity, shared.continuity);
                    let mut old = plain.work;
                    let mut new = shared.work;
                    old.preparation_nanos = 0;
                    new.preparation_nanos = 0;
                    assert_eq!(old, new);
                }
            }
        }
    }
}

impl<'a, 'g> Context<'a, 'g> {
    pub(super) fn new(geometry: &'a Geometry<'g>, enabled: bool) -> Self {
        Self {
            geometry,
            enabled,
            spans: Default::default(),
            domains: Default::default(),
            work: Default::default(),
        }
    }

    pub(super) fn work(&self) -> Work {
        self.work.borrow().clone()
    }

    pub(super) fn main_span(&self, area: &str, rank: usize) -> Result<(f64, f64), Failure> {
        if !self.enabled {
            self.work.borrow_mut().span_computations += 1;
            return self.geometry.main_span(area, rank);
        }
        let key = (area.to_string(), rank);
        if let Some(span) = self.spans.borrow().get(&key) {
            self.work.borrow_mut().span_reuses += 1;
            return span.clone();
        }
        let span = self.geometry.main_span(area, rank);
        self.work.borrow_mut().span_computations += 1;
        self.spans.borrow_mut().insert(key, span.clone());
        span
    }

    pub(super) fn domain(
        &self,
        edge: &EdgePlan,
        trial: &Trial,
        index: usize,
    ) -> Result<(String, (f64, f64)), Failure> {
        if !self.enabled || index == 0 || index == edge.sections.len() {
            if index > 0 && index < edge.sections.len() {
                self.work.borrow_mut().domain_computations += 1;
            }
            return lane_domain(edge, trial, self.geometry, index);
        }
        let (area, element) = junction(&edge.sections[index - 1], &edge.sections[index]);
        let key = (area.clone(), element.clone());
        let cached = self.domains.borrow().get(&key).cloned();
        let rect = if let Some(rect) = cached {
            self.work.borrow_mut().domain_reuses += 1;
            rect
        } else {
            let rect = self.geometry.rect(area, element);
            self.work.borrow_mut().domain_computations += 1;
            self.domains.borrow_mut().insert(key, rect.clone());
            rect
        }?;
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
