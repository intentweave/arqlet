//! The container step of the recursive prototype (`docs/recursive-optimization.md` §4.2): one container of leaf children
//! under a fixed boundary. The boundary contract, stated explicitly:
//!
//! - Fixed by the interior: the container's size (its interior's bounding box plus the padding and the header) and the
//!   children's positions inside it. The interior is laid out and refined on its own.
//! - Anchor identity: an edge that crosses the border is one edge with its own id, and its ends are its two node ports.
//!   The container is not an anchor; the child is.
//! - Allowed area and direction: the seed starts each end at the node's port, leaving outward from the node's side; the
//!   router improves the route within the drawn boxes. No stub is stitched in afterwards.
//! - Responsibility: the outer layout places the container and the outside nodes; the composition fixes the interior; the
//!   router owns every route, including its border crossing.
//!
//! Not covered: negotiation of the boundary (alternative sizes or attachment sides), nested or several containers,
//! explicit ports. These are refused, not approximated.

use crate::refine_snapshot_logged;
use arqlet_model::{
    container_extent, Graph, LayoutOptions, NodeSize, OptimizationReport, Rect, Snapshot,
};
use std::collections::BTreeSet;

/// Padding between the interior and the container border, and the header above the interior. These are the geometry
/// rules' defaults, so the composed container passes the same validation as any laid-out container.
const PADDING: f64 = 12.;
const HEADER: f64 = 36.;

/// The container prototype's result: the chosen snapshot, which candidate it is, and both keys for the comparison.
#[derive(Clone, Debug)]
pub struct ContainerResult {
    pub snapshot: Snapshot,
    /// `composed` when the composition with the fixed boundary wins the shared key, `joint` otherwise.
    pub chosen: &'static str,
    pub container: String,
    pub interior_strategy: String,
    pub outer_strategy: String,
    pub composed_key: Option<Vec<i64>>,
    pub joint_key: Option<Vec<i64>>,
}

/// Refines a flat graph with one container of leaf children, using a fixed boundary.
pub fn refine_by_fixed_boundary(
    graph: &Graph,
    options: &LayoutOptions,
) -> Result<ContainerResult, String> {
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    if containers.len() != 1 {
        return Err("the container prototype needs exactly one container".into());
    }
    let container = containers.iter().next().unwrap().to_string();
    if graph
        .nodes
        .iter()
        .any(|n| n.id == container && n.parent_id.is_some())
    {
        return Err("the container must be top-level".into());
    }
    let children: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == Some(container.as_str()))
        .map(|n| n.id.as_str())
        .collect();
    if graph
        .nodes
        .iter()
        .any(|n| n.parent_id.as_deref().is_some_and(|p| children.contains(p)))
    {
        return Err("nested containers are not handled".into());
    }
    if !graph.ports.is_empty() || !graph.edge_ports.is_empty() {
        return Err("the container prototype does not handle explicit ports yet".into());
    }
    // A pin or an authored size is an explicit requirement the prototype does not read; it refuses instead of moving or resizing.
    if !options.pins.is_empty() || !options.container_sizing.is_empty() {
        return Err(
            "the container prototype does not handle pins or authored container sizes yet".into(),
        );
    }

    // The interior: the children and the edges between them, laid out flat.
    let interior = Graph {
        default_direction: graph.default_direction.clone(),
        container_directions: graph.container_directions.clone(),
        default_strategy: graph.default_strategy.clone(),
        container_strategies: graph.container_strategies.clone(),
        default_routing: graph.default_routing.clone(),
        container_routing: graph.container_routing.clone(),
        nodes: graph
            .nodes
            .iter()
            .filter(|n| children.contains(n.id.as_str()))
            .map(|n| {
                let mut n = n.clone();
                n.parent_id = None;
                n
            })
            .collect(),
        edges: graph
            .edges
            .iter()
            .filter(|e| {
                children.contains(e.source.as_str())
                    && e.target.as_deref().is_some_and(|t| children.contains(t))
            })
            .cloned()
            .collect(),
        ports: vec![],
        edge_ports: vec![],
        buses: Vec::new(),
    };
    let no_dirty = BTreeSet::new();
    let laid = arqlet_layout::compute(&interior, None, &no_dirty, options)?;
    let (interior_snap, _, _) =
        refine_snapshot_logged(&laid, laid.revision, laid.layout_version, 1, None)?;
    let interior_strategy = strategy_of(&interior_snap);
    let min_x = interior_snap
        .boxes
        .values()
        .map(|b| b.x)
        .fold(f64::INFINITY, f64::min);
    let min_y = interior_snap
        .boxes
        .values()
        .map(|b| b.y)
        .fold(f64::INFINITY, f64::min);
    let max_x = interior_snap
        .boxes
        .values()
        .map(|b| b.x + b.width)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = interior_snap
        .boxes
        .values()
        .map(|b| b.y + b.height)
        .fold(f64::NEG_INFINITY, f64::max);
    // The boundary: the container's size follows from its interior, or from its authored size (ADR 0010). A fixed axis
    // below its content is reported by the geometry validator.
    let content = NodeSize {
        width: (max_x - min_x) + 2. * PADDING,
        height: (max_y - min_y) + HEADER + PADDING,
    };
    let (size, _) = container_extent(options.container_sizing.get(&container), content);

    // The outer graph: the container is a leaf of the fixed size, and edges into the container attach to its border.
    let outer = Graph {
        default_direction: graph.default_direction.clone(),
        container_directions: graph.container_directions.clone(),
        default_strategy: graph.default_strategy.clone(),
        container_strategies: graph.container_strategies.clone(),
        default_routing: graph.default_routing.clone(),
        container_routing: graph.container_routing.clone(),
        nodes: graph
            .nodes
            .iter()
            .filter(|n| !children.contains(n.id.as_str()))
            .map(|n| {
                let mut n = n.clone();
                n.parent_id = None;
                n
            })
            .collect(),
        edges: graph
            .edges
            .iter()
            .filter(|e| {
                !(children.contains(e.source.as_str())
                    && e.target.as_deref().is_some_and(|t| children.contains(t)))
            })
            .map(|e| {
                let mut e = e.clone();
                if children.contains(e.source.as_str()) {
                    e.source = container.clone();
                }
                if let Some(t) = e.target.as_deref() {
                    if children.contains(t) {
                        e.target = Some(container.clone());
                    }
                }
                e
            })
            .collect(),
        ports: vec![],
        edge_ports: vec![],
        buses: Vec::new(),
    };
    let mut outer_options = options.clone();
    outer_options
        .node_sizes
        .insert(container.clone(), size.clone());
    let laid_outer = arqlet_layout::compute(&outer, None, &no_dirty, &outer_options)?;
    let (outer_snap, _, _) = refine_snapshot_logged(
        &laid_outer,
        laid_outer.revision,
        laid_outer.layout_version,
        1,
        None,
    )?;
    let outer_strategy = strategy_of(&outer_snap);
    let frame = outer_snap
        .boxes
        .get(&container)
        .ok_or("the outer layout has no container box")?
        .clone();

    // The composition, in the coordinates of the whole graph: child boxes are relative to the container.
    let mut composed = arqlet_layout::compute(graph, None, &no_dirty, options)?;
    composed.boxes = outer_snap
        .boxes
        .iter()
        .filter(|(id, _)| **id != container)
        .map(|(id, r)| (id.clone(), r.clone()))
        .collect();
    composed.boxes.insert(container.clone(), frame);
    for (id, r) in &interior_snap.boxes {
        composed.boxes.insert(
            id.clone(),
            Rect {
                x: r.x - min_x + PADDING,
                y: r.y - min_y + HEADER,
                width: r.width,
                height: r.height,
            },
        );
    }
    // The boundary contract: the boxes are fixed (the container's size, and the children's positions inside it). Routes
    // are not stitched together: every edge, including each one that crosses the border, starts from the regular seed of
    // the composed drawing and is improved by the normal router, with ports at both ends.
    let (seed, seed_ports) = arqlet_layout::seed_drawing(graph, &composed.boxes, options);
    composed.routes = seed;
    composed.resolved_ports = seed_ports;
    let (rerouted, _) = arqlet_layout::reroute_counted(&composed, true, &|| false);
    composed = rerouted.map_err(|_| "the boundary routing was interrupted".to_string())?;
    composed.labels = vec![];
    composed.geometry_issues = arqlet_scene::validate_geometry(
        &composed.graph,
        &composed.boxes,
        arqlet_scene::GeometryRules::default(),
    );
    composed.geometry_issues.extend(crate::fixed_size_conflicts(
        &composed.graph,
        &composed.boxes,
        &options.container_sizing,
    ));
    let composed_key = if composed.geometry_issues.is_empty() {
        arqlet_layout::published_key(&composed).ok()
    } else {
        None
    };
    // The joint layout of the whole graph is the comparison.
    let joint = arqlet_layout::compute(graph, None, &no_dirty, options)?;
    let (joint, _, _) =
        refine_snapshot_logged(&joint, joint.revision, joint.layout_version, 1, None)?;
    let joint_key = arqlet_layout::published_key(&joint).ok();

    // Authored container sizes are requirements (ADR 0010): only the composition honors them, so it wins when one is set.
    let composed_wins = !options.container_sizing.is_empty()
        || match (&composed_key, &joint_key) {
            (Some(c), Some(j)) => arqlet_layout::improves(c, j),
            (Some(_), None) => true,
            _ => false,
        };
    let (mut snapshot, chosen) = if composed_wins {
        (composed, "composed")
    } else {
        (joint, "joint")
    };
    let before = arqlet_layout::quality_with(
        &joint_for_report(graph, options)?,
        &joint_for_report(graph, options)?,
        &options.objectives,
    );
    snapshot.optimization = Some(OptimizationReport {
        before,
        after: arqlet_layout::quality_with(&snapshot, &snapshot, &options.objectives),
        candidates_evaluated: 2,
        interrupted: false,
        work_limited: false,
        stop_reason: None,
        strategy: format!(
            "fixed boundary: interior {interior_strategy}, outer {outer_strategy}, chosen {chosen}"
        ),
    });
    Ok(ContainerResult {
        snapshot,
        chosen,
        container,
        interior_strategy,
        outer_strategy,
        composed_key,
        joint_key,
    })
}

/// Extends the border attachments of the outer routes to the children they belong to: an edge into the container ends at
/// its border, so the last segment to the child's centre is added on that side.

/// The joint layout again, for the report's `before`: the quality of the layout without the composition.
fn joint_for_report(graph: &Graph, options: &LayoutOptions) -> Result<Snapshot, String> {
    let laid = arqlet_layout::compute(graph, None, &BTreeSet::new(), options)?;
    let (refined, _, _) =
        refine_snapshot_logged(&laid, laid.revision, laid.layout_version, 1, None)?;
    Ok(refined)
}

fn strategy_of(snapshot: &Snapshot) -> String {
    snapshot
        .optimization
        .as_ref()
        .map_or("keep-existing".to_string(), |o| o.strategy.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arqlet_model::Point;

    fn fixture(name: &str) -> Graph {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure").join(name)).unwrap(),
        )
        .unwrap()
    }

    /// The container's size follows from its interior; the composition is valid, every node and edge is present, the
    /// children lie inside the container, and the chosen candidate is the better key.
    #[test]
    fn a_fixed_boundary_container_is_composed_and_evaluated() {
        let graph = fixture("container.json");
        let result = refine_by_fixed_boundary(&graph, &LayoutOptions::default()).unwrap();
        let snapshot = &result.snapshot;
        assert!(
            snapshot.geometry_issues.is_empty(),
            "{:?}",
            snapshot.geometry_issues
        );
        assert_eq!(snapshot.boxes.len(), graph.nodes.len());
        assert_eq!(snapshot.routes.len(), graph.edges.len());
        let frame = snapshot.boxes["box"].clone();
        for child in ["c1", "c2", "c3"] {
            let b = snapshot.boxes[child].clone();
            assert!(
                b.x >= 0. && b.y >= 0.,
                "{child} lies inside the container frame"
            );
            assert!(
                b.x + b.width <= frame.width && b.y + b.height <= frame.height,
                "{child} fits the container"
            );
        }
        match (&result.composed_key, &result.joint_key) {
            (Some(c), Some(j)) => {
                let best = if arqlet_layout::improves(c, j) { c } else { j };
                assert_eq!(&arqlet_layout::published_key(snapshot).unwrap(), best);
            }
            _ => panic!("both candidates are measurable on this fixture"),
        }
    }

    /// Every edge is a complete route between its two nodes, including the edges that cross the container border: each end
    /// lies at its node's box in the whole drawing (children are positioned relative to the container).
    #[test]
    fn every_route_connects_its_nodes_across_the_border() {
        let graph = fixture("container.json");
        let result = refine_by_fixed_boundary(&graph, &LayoutOptions::default()).unwrap();
        let snapshot = &result.snapshot;
        let frame = snapshot.boxes["box"].clone();
        let absolute = |id: &str| {
            let r = snapshot.boxes[id].clone();
            let inside = graph
                .nodes
                .iter()
                .any(|n| n.id == id && n.parent_id.as_deref() == Some("box"));
            let (ox, oy) = if inside { (frame.x, frame.y) } else { (0., 0.) };
            Rect {
                x: r.x + ox,
                y: r.y + oy,
                width: r.width,
                height: r.height,
            }
        };
        let near = |p: &Point, r: &Rect| {
            let d = 25.;
            p.x >= r.x - d
                && p.x <= r.x + r.width + d
                && p.y >= r.y - d
                && p.y <= r.y + r.height + d
        };
        for edge in &graph.edges {
            let route = snapshot
                .routes
                .iter()
                .find(|r| r.id == edge.id)
                .expect("every edge has a route");
            let start = route.points.first().unwrap();
            let end = route.points.last().unwrap();
            assert!(
                near(start, &absolute(&edge.source)),
                "{} starts at {}",
                edge.id,
                edge.source
            );
            assert!(
                near(end, &absolute(edge.target.as_deref().unwrap())),
                "{} ends at its target",
                edge.id
            );
        }
    }

    /// Nested containers and several containers are refused, not composed silently.
    #[test]
    fn unsupported_container_structures_are_refused() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let pools: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("benchmarks/inputs/bpmn-pools.json")).unwrap(),
        )
        .unwrap();
        assert!(refine_by_fixed_boundary(&pools, &LayoutOptions::default()).is_err());
    }
}
