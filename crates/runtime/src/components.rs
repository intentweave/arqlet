//! A recursive prototype for independent components (`docs/recursive-optimization.md` §4.1): each weakly connected
//! component of a flat graph is laid out and refined on its own, with the strategies its own plan allows; the components
//! are composed side by side and the composition is evaluated as a whole. The composed result and the joint layout of the
//! whole graph compete under the shared conflict key, and the better one is returned. Containers and negotiated
//! boundaries are not handled here: a graph with containers is refused.

use crate::refine_snapshot_logged;
use arqlet_layout::structure;
use arqlet_model::{Graph, LayoutOptions, OptimizationReport, Point, Snapshot};
use std::collections::BTreeSet;

/// Space between two components in the composed drawing, in drawing units.
const COMPONENT_GAP: f64 = 48.;

/// What one component contributed: its nodes, the strategy its own refinement chose, and its own key.
#[derive(Clone, Debug, PartialEq)]
pub struct ComponentReport {
    pub nodes: Vec<String>,
    pub strategy: String,
    pub key: Vec<i64>,
}

/// The result of the component prototype: the chosen snapshot, which candidate it is, and both keys for the comparison.
#[derive(Clone, Debug)]
pub struct ComponentResult {
    pub snapshot: Snapshot,
    /// `composed` when the composition of the components wins the shared key, `joint` otherwise.
    pub chosen: &'static str,
    pub components: Vec<ComponentReport>,
    pub composed_key: Option<Vec<i64>>,
    pub joint_key: Option<Vec<i64>>,
}

/// Refines a flat graph component by component and compares the composition with the joint layout.
pub fn refine_by_components(
    graph: &Graph,
    options: &LayoutOptions,
) -> Result<ComponentResult, String> {
    if graph.nodes.iter().any(|n| n.parent_id.is_some()) {
        return Err("the component prototype needs a graph without containers".into());
    }
    if !graph.ports.is_empty() || !graph.edge_ports.is_empty() {
        return Err("the component prototype does not handle explicit ports yet".into());
    }
    // A pin or an authored size is an explicit requirement the prototype does not read; it refuses instead of moving or resizing.
    if !options.pins.is_empty() || !options.container_sizing.is_empty() {
        return Err(
            "the component prototype does not handle pins or authored container sizes yet".into(),
        );
    }
    let index: std::collections::BTreeMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let pairs: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .filter_map(|e| {
            let t = e.target.as_deref()?;
            Some((index[e.source.as_str()], index[t]))
        })
        .collect();
    let parts = structure::analyse(graph.nodes.len(), &pairs).components;

    let no_dirty = BTreeSet::new();
    let mut pieces = Vec::new();
    let mut reports = Vec::new();
    let mut cursor = 0.;
    for part in &parts {
        let members: BTreeSet<&str> = part.iter().map(|&i| graph.nodes[i].id.as_str()).collect();
        let sub = Graph {
            default_direction: graph.default_direction.clone(),
            container_directions: graph.container_directions.clone(),
            default_strategy: graph.default_strategy.clone(),
            container_strategies: graph.container_strategies.clone(),
            default_routing: graph.default_routing.clone(),
            container_routing: graph.container_routing.clone(),
            nodes: part.iter().map(|&i| graph.nodes[i].clone()).collect(),
            edges: graph
                .edges
                .iter()
                .filter(|e| members.contains(e.source.as_str()))
                .cloned()
                .collect(),
            ports: vec![],
            edge_ports: vec![],
            buses: Vec::new(),
        };
        let laid = arqlet_layout::compute(&sub, None, &no_dirty, options)?;
        let (mut refined, _, _) =
            refine_snapshot_logged(&laid, laid.revision, laid.layout_version, 1, None)?;
        let strategy = refined
            .optimization
            .as_ref()
            .map_or("keep-existing".to_string(), |o| o.strategy.clone());
        let key = arqlet_layout::published_key(&refined).unwrap_or_default();
        // Shift the component so it starts at the cursor, and place the next one after it.
        let min_x = refined
            .boxes
            .values()
            .map(|b| b.x)
            .fold(f64::INFINITY, f64::min);
        let max_x = refined
            .boxes
            .values()
            .map(|b| b.x + b.width)
            .fold(f64::NEG_INFINITY, f64::max);
        let dx = cursor - min_x;
        translate(&mut refined, dx, 0.);
        cursor = max_x + dx + COMPONENT_GAP;
        reports.push(ComponentReport {
            nodes: members.iter().map(|s| s.to_string()).collect(),
            strategy,
            key,
        });
        pieces.push(refined);
    }

    // The joint layout of the whole graph is the comparison, and it supplies the fields the composition shares with it.
    let joint = arqlet_layout::compute(graph, None, &no_dirty, options)?;
    let (joint, _, _) =
        refine_snapshot_logged(&joint, joint.revision, joint.layout_version, 1, None)?;
    let joint_key = arqlet_layout::published_key(&joint).ok();

    let mut composed = joint.clone();
    composed.boxes = pieces.iter().flat_map(|p| p.boxes.clone()).collect();
    composed.routes = pieces.iter().flat_map(|p| p.routes.clone()).collect();
    composed.resolved_ports = pieces
        .iter()
        .flat_map(|p| p.resolved_ports.clone())
        .collect();
    composed.labels = pieces.iter().flat_map(|p| p.labels.clone()).collect();
    composed.routing_issues = pieces
        .iter()
        .flat_map(|p| p.routing_issues.clone())
        .collect();
    composed.geometry_issues = arqlet_scene::validate_geometry(
        &composed.graph,
        &composed.boxes,
        arqlet_scene::GeometryRules::default(),
    );
    let composed_key = if composed.geometry_issues.is_empty() {
        arqlet_layout::published_key(&composed).ok()
    } else {
        None
    };
    composed.optimization = Some(OptimizationReport {
        before: arqlet_layout::quality_with(&joint, &joint, &options.objectives),
        after: arqlet_layout::quality_with(&composed, &joint, &options.objectives),
        candidates_evaluated: reports.len(),
        interrupted: false,
        work_limited: false,
        stop_reason: None,
        strategy: format!(
            "components: {}",
            reports
                .iter()
                .map(|r| r.strategy.as_str())
                .collect::<Vec<_>>()
                .join("|")
        ),
    });

    let composed_wins = match (&composed_key, &joint_key) {
        (Some(c), Some(j)) => arqlet_layout::improves(c, j),
        (Some(_), None) => true,
        _ => false,
    };
    let (snapshot, chosen) = if composed_wins {
        (composed, "composed")
    } else {
        (joint, "joint")
    };
    Ok(ComponentResult {
        snapshot,
        chosen,
        components: reports,
        composed_key,
        joint_key,
    })
}

/// Moves every geometric value of a snapshot by the given offset.
fn translate(s: &mut Snapshot, dx: f64, dy: f64) {
    let shift = |p: &mut Point| {
        p.x += dx;
        p.y += dy;
    };
    for rect in s.boxes.values_mut() {
        rect.x += dx;
        rect.y += dy;
    }
    for route in &mut s.routes {
        route.points.iter_mut().for_each(shift);
    }
    for port in &mut s.resolved_ports {
        shift(&mut port.position);
    }
    for label in &mut s.labels {
        label.bounds.x += dx;
        label.bounds.y += dy;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arqlet_model::Graph;

    fn fixture(name: &str) -> Graph {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure").join(name)).unwrap(),
        )
        .unwrap()
    }

    /// The components of a disconnected graph are laid out separately, composed side by side, and the composition is
    /// evaluated under the shared key: the result is valid, every node and edge is present, and the chosen candidate's key
    /// is the better of the two.
    #[test]
    fn components_are_refined_separately_composed_and_evaluated() {
        let graph = fixture("disconnected.json");
        let result = refine_by_components(&graph, &LayoutOptions::default()).unwrap();
        assert_eq!(
            result.components.len(),
            3,
            "two chains and an isolated node"
        );
        let snapshot = &result.snapshot;
        assert!(
            snapshot.geometry_issues.is_empty(),
            "{:?}",
            snapshot.geometry_issues
        );
        assert_eq!(snapshot.boxes.len(), graph.nodes.len());
        assert_eq!(snapshot.routes.len(), graph.edges.len());
        assert!(result.components.iter().all(|c| !c.strategy.is_empty()));
        let chosen_key = arqlet_layout::published_key(snapshot).unwrap();
        match (&result.composed_key, &result.joint_key) {
            (Some(composed), Some(joint)) => {
                let best = if arqlet_layout::improves(composed, joint) {
                    composed
                } else {
                    joint
                };
                assert_eq!(&chosen_key, best, "the chosen candidate is the better key");
            }
            _ => panic!("both candidates are measurable on this fixture"),
        }
    }

    /// Components of one graph may choose different strategies: a corpus graph (state-tcp, flat) and the disconnected fixture
    /// are combined with their ids kept apart. The corpus component is refined with a strategy of its own, the chains keep
    /// their input, and the result is valid with every node and edge present.
    #[test]
    fn components_of_one_graph_can_use_different_strategies() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let corpus: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/state-tcp.json")).unwrap(),
        )
        .unwrap();
        let chains = fixture("disconnected.json");
        let prefixed = |g: &Graph, p: &str| -> (Vec<_>, Vec<_>) {
            let nodes = g
                .nodes
                .iter()
                .map(|n| {
                    let mut n = n.clone();
                    n.id = format!("{p}{}", n.id);
                    n
                })
                .collect();
            let edges = g
                .edges
                .iter()
                .map(|e| {
                    let mut e = e.clone();
                    e.id = format!("{p}{}", e.id);
                    e.source = format!("{p}{}", e.source);
                    e.target = e.target.map(|t| format!("{p}{t}"));
                    e
                })
                .collect();
            (nodes, edges)
        };
        let (mut nodes, mut edges) = prefixed(&corpus, "t_");
        let (more_nodes, more_edges) = prefixed(&chains, "c_");
        nodes.extend(more_nodes);
        edges.extend(more_edges);
        let graph = Graph {
            default_direction: corpus.default_direction.clone(),
            container_directions: corpus.container_directions.clone(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            nodes,
            edges,
            ports: vec![],
            edge_ports: vec![],
            buses: Vec::new(),
        };
        // The refinement portfolio of the pipeline before single pass (ADR 0016): single pass plans no candidates, so every
        // component would keep its input.
        let legacy = LayoutOptions {
            single_pass_auto: false,
            single_pass: false,
            abstract_routing: false,
            ..LayoutOptions::default()
        };
        let result = refine_by_components(&graph, &legacy).unwrap();
        assert!(
            result.snapshot.geometry_issues.is_empty(),
            "{:?}",
            result.snapshot.geometry_issues
        );
        assert_eq!(result.snapshot.boxes.len(), graph.nodes.len());
        assert_eq!(result.snapshot.routes.len(), graph.edges.len());
        let strategies: BTreeSet<&str> = result
            .components
            .iter()
            .map(|c| c.strategy.as_str())
            .collect();
        assert!(
            strategies.len() > 1,
            "the components chose different strategies: {strategies:?}"
        );
    }

    /// The component prototype refuses what it does not handle, rather than composing it silently.
    #[test]
    fn a_graph_with_containers_is_refused() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/bpmn-pools.json")).unwrap(),
        )
        .unwrap();
        let error = refine_by_components(&graph, &LayoutOptions::default()).unwrap_err();
        assert_eq!(
            error,
            "the component prototype needs a graph without containers"
        );
    }
}
