//! The common entry of the recursive prototype (`docs/recursive-optimization.md` §3, P05): one operation that reads the
//! structure of a graph and decides which processing applies. A flat graph with independent components takes the
//! component path; one container of leaf children takes the container path; several or nested containers take the
//! hierarchical composition; everything else takes the joint layout. Each path that does not apply is recorded with its
//! reason. The per-path operations remain as diagnostics.
//!
//! Every composition is compared with the joint layout under the shared key. When the composition does not win, the
//! result is the joint layout, and the decision says so: the route is `joint`, with the reason.

use crate::components::refine_by_components;
use crate::containers::refine_by_fixed_boundary;
use crate::hierarchy::refine_hierarchical;
use crate::refine_snapshot_logged;
use arqlet_layout::structure;
use arqlet_model::{Graph, LayoutOptions, Snapshot};
use std::collections::{BTreeMap, BTreeSet};

/// The processing a view received, the paths that did not apply (each with its reason), both candidates' keys, and the
/// strategy each part of the drawing used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    /// `components`, `container`, `hierarchy` or `joint`.
    pub route: &'static str,
    /// Which path did not apply to this graph, or did not win, and why.
    pub not_applied: Vec<(&'static str, String)>,
    /// The composed candidate's shared key, when a composition was evaluated.
    pub composed_key: Option<Vec<i64>>,
    /// The joint layout's shared key.
    pub joint_key: Option<Vec<i64>>,
    /// Per part of the drawing (a component's nodes, a container's id): the strategy its own refinement chose.
    pub details: Vec<(String, String)>,
    /// The strategy the hierarchical composition chose for the root (`root`) and each container, when that route won.
    pub strategies: Vec<(String, String)>,
}

/// Reads the structure of a graph and refines it by the path that applies. Returns the chosen snapshot and the decision.
pub fn refine_structured(
    graph: &Graph,
    options: &LayoutOptions,
) -> Result<(Snapshot, Decision), String> {
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let nested = graph.nodes.iter().any(|n| {
        n.parent_id.as_deref().is_some_and(|p| {
            graph
                .nodes
                .iter()
                .any(|q| q.id == p && q.parent_id.is_some())
        })
    });
    let index: BTreeMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let pairs: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .filter_map(|e| Some((index[e.source.as_str()], index[e.target.as_deref()?])))
        .collect();
    let components = structure::analyse(graph.nodes.len(), &pairs)
        .components
        .len();

    let mut not_applied = Vec::new();
    if containers.is_empty() {
        not_applied.push(("container", "the graph has no container".to_string()));
        if components > 1 {
            match refine_by_components(graph, options) {
                Ok(result) => {
                    let details = result
                        .components
                        .iter()
                        .map(|c| (c.nodes.join(", "), c.strategy.clone()))
                        .collect();
                    return Ok(judged(
                        "components",
                        result.chosen,
                        result.snapshot,
                        not_applied,
                        result.composed_key,
                        result.joint_key,
                        details,
                    ));
                }
                Err(reason) => not_applied.push(("components", reason)),
            }
        } else {
            not_applied.push((
                "components",
                "the graph is one connected component".to_string(),
            ));
        }
    } else {
        not_applied.push((
            "components",
            "the components path needs a graph without containers".to_string(),
        ));
        if containers.len() == 1 && !nested {
            match refine_by_fixed_boundary(graph, options) {
                Ok(result) => {
                    let details = vec![(
                        result.container.clone(),
                        format!(
                            "interior {}, outer {}",
                            result.interior_strategy, result.outer_strategy
                        ),
                    )];
                    return Ok(judged(
                        "container",
                        result.chosen,
                        result.snapshot,
                        not_applied,
                        result.composed_key,
                        result.joint_key,
                        details,
                    ));
                }
                Err(reason) => not_applied.push(("container", reason)),
            }
        }
        match refine_hierarchical(graph, options) {
            Ok(result) => {
                let strategies = result.strategies.clone();
                let (snapshot, mut decision) = judged(
                    "hierarchy",
                    result.chosen,
                    result.snapshot,
                    not_applied,
                    result.composed_key,
                    result.joint_key,
                    result.containers,
                );
                if decision.route == "hierarchy" {
                    decision.strategies = strategies;
                }
                return Ok((snapshot, decision));
            }
            Err(reason) => not_applied.push(("hierarchy", reason)),
        }
    }

    let laid = arqlet_layout::compute(graph, None, &BTreeSet::new(), options)?;
    let (joint, _, _) = refine_snapshot_logged(&laid, laid.revision, laid.layout_version, 1, None)?;
    let joint_key = arqlet_layout::published_key(&joint).ok();
    Ok((
        joint,
        Decision {
            route: "joint",
            not_applied,
            composed_key: None,
            joint_key,
            details: Vec::new(),
            strategies: Vec::new(),
        },
    ))
}

/// A path's result, judged by its own comparison: a winning composition keeps its route; a lost one is the joint layout,
/// and the reason is recorded.
fn judged(
    route: &'static str,
    chosen: &str,
    snapshot: Snapshot,
    mut not_applied: Vec<(&'static str, String)>,
    composed_key: Option<Vec<i64>>,
    joint_key: Option<Vec<i64>>,
    details: Vec<(String, String)>,
) -> (Snapshot, Decision) {
    let route = if chosen == "composed" {
        route
    } else {
        not_applied.push((
            route,
            "the composition did not win the shared key".to_string(),
        ));
        "joint"
    };
    (
        snapshot,
        Decision {
            route,
            not_applied,
            composed_key,
            joint_key,
            details,
            strategies: Vec::new(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Graph {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure").join(name)).unwrap(),
        )
        .unwrap()
    }

    fn reason<'a>(decision: &'a Decision, path: &str) -> Option<&'a str> {
        decision
            .not_applied
            .iter()
            .find(|(p, _)| *p == path)
            .map(|(_, r)| r.as_str())
    }

    /// Independent components take the component path, or the joint layout when the composition loses; the container
    /// path is recorded as not applying.
    #[test]
    fn independent_components_take_the_component_path() {
        let (snapshot, decision) =
            refine_structured(&fixture("disconnected.json"), &LayoutOptions::default()).unwrap();
        assert!(
            matches!(decision.route, "components" | "joint"),
            "{}",
            decision.route
        );
        assert!(reason(&decision, "container").is_some());
        assert!(snapshot.geometry_issues.is_empty());
        assert!(decision.joint_key.is_some());
    }

    /// One container of leaf children takes the container path, or the joint layout when the composition loses.
    #[test]
    fn one_container_takes_the_container_path() {
        let (snapshot, decision) =
            refine_structured(&fixture("container.json"), &LayoutOptions::default()).unwrap();
        assert!(
            matches!(decision.route, "container" | "joint"),
            "{}",
            decision.route
        );
        assert_eq!(
            reason(&decision, "components"),
            Some("the components path needs a graph without containers")
        );
        assert!(snapshot.geometry_issues.is_empty());
        assert!(decision.joint_key.is_some());
    }

    /// A connected flat graph takes the joint layout with its reason, and has no composition to compare.
    #[test]
    fn a_connected_flat_graph_takes_the_joint_layout_with_its_reason() {
        let (_, single) =
            refine_structured(&fixture("singleton.json"), &LayoutOptions::default()).unwrap();
        assert_eq!(single.route, "joint");
        assert_eq!(
            reason(&single, "components"),
            Some("the graph is one connected component")
        );
        assert_eq!(single.composed_key, None);
    }

    /// Acceptance for step 3: several and nested containers go through the common entry; every edge is routed, the
    /// geometry is valid, and the result is never worse than the joint layout under the shared key.
    #[test]
    fn nested_and_several_containers_go_through_the_common_entry() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pools: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/bpmn-pools.json")).unwrap(),
        )
        .unwrap();
        for (name, graph) in [
            ("nested.json", fixture("nested.json")),
            ("two-containers.json", fixture("two-containers.json")),
            ("bpmn-pools.json", pools),
        ] {
            let (snapshot, decision) =
                refine_structured(&graph, &LayoutOptions::default()).unwrap();
            assert!(
                matches!(decision.route, "hierarchy" | "joint"),
                "{name}: {}",
                decision.route
            );
            assert!(
                snapshot.geometry_issues.is_empty(),
                "{name}: {:?}",
                snapshot.geometry_issues
            );
            assert_eq!(snapshot.boxes.len(), graph.nodes.len());
            assert_eq!(snapshot.routes.len(), graph.edges.len());
            let joint =
                arqlet_layout::compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default())
                    .unwrap();
            let (joint, _, _) =
                refine_snapshot_logged(&joint, joint.revision, joint.layout_version, 1, None)
                    .unwrap();
            let chosen = arqlet_layout::published_key(&snapshot).unwrap();
            let baseline = arqlet_layout::published_key(&joint).unwrap();
            assert!(
                !arqlet_layout::improves(&baseline, &chosen),
                "{name}: worse than the joint layout"
            );
            assert_eq!(
                decision.joint_key.as_ref(),
                Some(&baseline),
                "{name}: the joint key is reported"
            );
            if decision.route == "hierarchy" {
                assert!(
                    !decision.details.is_empty(),
                    "{name}: every container reports its strategy"
                );
            } else {
                assert!(
                    reason(&decision, "hierarchy").is_some(),
                    "{name}: a lost composition is reported"
                );
            }
        }
    }
}
