//! Step 3 of the recursive prototype (`docs/recursive-optimization.md` §4.2): several and deeply nested containers under
//! fixed boundaries. The same procedure as the single container, applied level by level from the inside out. A container's
//! children are laid out as leaves of their own level, with every container child at the fixed size its own interior gives
//! it; that fixes the size the container presents to its parent level. Boxes stay relative to their parent, so a level
//! shifts only its own direct children. The whole drawing is then composed, its edges are routed by the regular router, and
//! the composition competes with the joint layout under the shared key.
//!
//! Shape alternatives (step 4 of the plan): a container's interior is laid out in its own direction (its explicit direction,
//! or the graph's default). One pass over the containers lets each free container try the other direction (`lr` against
//! `tb`); the change is kept only when the whole composition improves its shared key. Every alternative is a complete
//! interior layout with its own size, and the outer layout is recomputed for it. Containers with an explicit direction
//! keep it.
//!
//! The boundary contract is the one of the single container (see `containers`): fixed size and fixed child positions per
//! container, router-owned routes. No negotiation of boundary attachments.

use crate::refine_snapshot_logged;
use arqlet_model::{
    container_extent, DirectionConstraint, Graph, LayoutOptions, LayoutStrategy, NodeSize,
    OptimizationReport, Orientation, Rect, Snapshot, StrategyConstraint,
};
use std::collections::{BTreeMap, BTreeSet};

/// Padding between a container's interior and its border, and the header above it (the geometry rules' defaults).
const PADDING: f64 = 12.;
const HEADER: f64 = 36.;

/// The result of the hierarchical refinement.
#[derive(Clone, Debug)]
pub struct HierarchyResult {
    pub snapshot: Snapshot,
    /// `composed` when the composition wins the shared key, `joint` otherwise.
    pub chosen: &'static str,
    /// The containers, in the order their sizes were fixed (innermost first), each with the strategy its interior chose.
    pub containers: Vec<(String, String)>,
    pub composed_key: Option<Vec<i64>>,
    pub joint_key: Option<Vec<i64>>,
    /// The composed drawing before the comparison, whether or not it won: for diagnosis, not for output.
    pub composed_candidate: Snapshot,
    /// The joint layout before the comparison.
    pub joint_candidate: Snapshot,
    /// The composition with every container in its own direction, before the shape search.
    pub baseline_key: Option<Vec<i64>>,
    /// The composition before the shape search, for the before-and-after comparison.
    pub baseline_candidate: Snapshot,
    /// The direction each container's interior was laid out in, after the shape search.
    pub shapes: Vec<(String, String)>,
    /// The layout strategy each level was laid out with, after the search (key "root" is the root).
    pub strategies: Vec<(String, String)>,
    /// Compositions evaluated, including the baseline.
    pub compositions: usize,
}

/// A composition after routing, with its key when it is valid.
struct Scored {
    snapshot: Snapshot,
    key: Option<Vec<i64>>,
    containers: Vec<(String, String)>,
    shapes: Vec<(String, String)>,
    strategies: Vec<(String, String)>,
}

/// Refines a graph with containers of any depth by the hierarchical composition, and compares it with the joint layout.
pub fn refine_hierarchical(
    graph: &Graph,
    options: &LayoutOptions,
) -> Result<HierarchyResult, String> {
    if !graph.ports.is_empty() || !graph.edge_ports.is_empty() {
        return Err("the hierarchical prototype does not handle explicit ports yet".into());
    }
    // A pin or an authored size is an explicit requirement the prototype does not read; it refuses instead of moving or resizing.
    if !options.pins.is_empty() || !options.container_sizing.is_empty() {
        return Err(
            "the hierarchical prototype does not handle pins or authored container sizes yet"
                .into(),
        );
    }
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let no_dirty = BTreeSet::new();
    let base = arqlet_layout::compute(graph, None, &no_dirty, options)?;

    let mut shapes: BTreeMap<String, Orientation> = BTreeMap::new();
    let mut strategies: BTreeMap<String, LayoutStrategy> = BTreeMap::new();
    let mut best = compose(graph, &parents, options, &base, &shapes, &strategies)?;
    let baseline_key = best.key.clone();
    let baseline_candidate = best.snapshot.clone();
    let mut compositions = 1;
    // One pass over the levels, the root first (key ""). Each level tries the alternatives its constraints allow: first its
    // strategy (for `auto` and allowed sets), then its orientation where the strategy leaves it free (plain and tree). A
    // candidate that the graph cannot take is refused by the layout, and is skipped; a change is kept only when the whole
    // composition gets a better key.
    let mut levels: Vec<Option<String>> = vec![None];
    levels.extend(best.containers.iter().map(|(id, _)| Some(id.clone())));
    for level in levels {
        let key = level.clone().unwrap_or_default();
        let strategy_constraint = match &level {
            None => graph.default_strategy.clone(),
            Some(id) => graph.strategy_constraint(Some(id)),
        };
        let current_strategy =
            level_strategy(graph, &strategies, level.as_deref()).default_choice();
        for alternative in strategy_constraint
            .candidates()
            .into_iter()
            .filter(|s| *s != current_strategy)
        {
            let mut trial = strategies.clone();
            trial.insert(key.clone(), alternative);
            compositions += 1;
            let Ok(scored) = compose(graph, &parents, options, &base, &shapes, &trial) else {
                continue;
            };
            if better(&scored.key, &best.key) {
                strategies = trial;
                best = scored;
            }
        }
        let chosen = level_strategy(graph, &strategies, level.as_deref()).default_choice();
        if !matches!(chosen, LayoutStrategy::None | LayoutStrategy::Tree) {
            continue;
        }
        let direction = match &level {
            None => graph.default_direction.clone(),
            Some(id) => graph.container_constraint(Some(id)),
        };
        let current = level_direction(graph, &shapes, level.as_deref()).default_choice();
        for alternative in direction.candidates().into_iter().filter(|o| *o != current) {
            let mut trial = shapes.clone();
            trial.insert(key.clone(), alternative);
            let scored = compose(graph, &parents, options, &base, &trial, &strategies)?;
            compositions += 1;
            if better(&scored.key, &best.key) {
                shapes = trial;
                best = scored;
            }
        }
    }

    let (joint, _, _) = refine_snapshot_logged(&base, base.revision, base.layout_version, 1, None)?;
    let joint_key = arqlet_layout::published_key(&joint).ok();
    // Authored container sizes are requirements (ADR 0010). Only a composition honors them, so a set size makes the
    // composition win; a conflict is then reported by its geometry issues rather than hidden by the joint layout.
    let composed_wins = !options.container_sizing.is_empty()
        || match (&best.key, &joint_key) {
            (Some(c), Some(j)) => arqlet_layout::improves(c, j),
            (Some(_), None) => true,
            _ => false,
        };
    let composed_key = best.key.clone();
    let composed_candidate = best.snapshot.clone();
    let (mut snapshot, chosen) = if composed_wins {
        (best.snapshot, "composed")
    } else {
        (joint.clone(), "joint")
    };
    snapshot.optimization = Some(OptimizationReport {
        before: arqlet_layout::quality_with(&joint, &joint, &options.objectives),
        after: arqlet_layout::quality_with(&snapshot, &joint, &options.objectives),
        candidates_evaluated: compositions + 1,
        interrupted: false,
        work_limited: false,
        stop_reason: None,
        strategy: format!(
            "hierarchy: {} containers, chosen {chosen}",
            best.containers.len()
        ),
    });
    Ok(HierarchyResult {
        snapshot,
        chosen,
        containers: best.containers,
        composed_key,
        joint_key,
        composed_candidate,
        joint_candidate: joint,
        baseline_key,
        baseline_candidate,
        shapes: best.shapes,
        strategies: best.strategies,
        compositions,
    })
}

/// Whether a candidate's key is valid and better than the incumbent's (a valid key beats none).
fn better(candidate: &Option<Vec<i64>>, incumbent: &Option<Vec<i64>>) -> bool {
    match (candidate, incumbent) {
        (Some(c), Some(i)) => arqlet_layout::improves(c, i),
        (Some(_), None) => true,
        _ => false,
    }
}

/// The direction a level is laid out under: the orientation the search chose for it (key "" is the root), else the constraint
/// its parent container declares or inherits, else the root's.
fn level_direction(
    graph: &Graph,
    shapes: &BTreeMap<String, Orientation>,
    parent: Option<&str>,
) -> DirectionConstraint {
    match shapes.get(parent.unwrap_or("")) {
        Some(orientation) => DirectionConstraint::Fixed(*orientation),
        None => match parent {
            None => graph.default_direction.clone(),
            Some(id) => graph.container_constraint(Some(id)),
        },
    }
}

/// The strategy a level is laid out under, with the same precedence as the direction.
fn level_strategy(
    graph: &Graph,
    strategies: &BTreeMap<String, LayoutStrategy>,
    parent: Option<&str>,
) -> StrategyConstraint {
    match strategies.get(parent.unwrap_or("")) {
        Some(strategy) => StrategyConstraint::Fixed(*strategy),
        None => match parent {
            None => graph.default_strategy.clone(),
            Some(id) => graph.strategy_constraint(Some(id)),
        },
    }
}

fn name_of<T: std::fmt::Debug>(value: T) -> String {
    format!("{value:?}").to_lowercase()
}

/// Composes the drawing for the given shape choices, routes it and scores it.
fn compose(
    graph: &Graph,
    parents: &BTreeMap<&str, Option<&str>>,
    options: &LayoutOptions,
    base: &Snapshot,
    shapes: &BTreeMap<String, Orientation>,
    strategies: &BTreeMap<String, LayoutStrategy>,
) -> Result<Scored, String> {
    let mut containers = Vec::new();
    let (boxes, _) = layout_level(
        graph,
        parents,
        None,
        options,
        shapes,
        strategies,
        &mut containers,
    )?;

    // The composed drawing: all boxes from the levels, the regular seed of the whole graph, and the router's improvement.
    let (seed, seed_ports) = arqlet_layout::seed_drawing(graph, &boxes, options);
    let mut composed = base.clone();
    composed.boxes = boxes;
    composed.routes = seed;
    composed.resolved_ports = seed_ports;
    composed.labels = vec![];
    let (rerouted, _) = arqlet_layout::reroute_counted(&composed, true, &|| false);
    composed =
        rerouted.map_err(|_| "the routing of the composed drawing was interrupted".to_string())?;
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
    // A straight drawing with routing issues is not conflict-free: it has no key, so it cannot win by the comparison.
    let straight_conflicts = composed
        .routing_issues
        .iter()
        .any(|issue| issue.starts_with("straight edge"));
    let key = if composed.geometry_issues.is_empty() && !straight_conflicts {
        arqlet_layout::published_key(&composed).ok()
    } else {
        None
    };
    let orientation_of =
        |parent: Option<&str>| name_of(level_direction(graph, shapes, parent).default_choice());
    let strategy_of =
        |parent: Option<&str>| name_of(level_strategy(graph, strategies, parent).default_choice());
    let mut shapes_out = vec![("root".to_string(), orientation_of(None))];
    shapes_out.extend(
        containers
            .iter()
            .map(|(id, _)| (id.clone(), orientation_of(Some(id)))),
    );
    let mut strategies_out = vec![("root".to_string(), strategy_of(None))];
    strategies_out.extend(
        containers
            .iter()
            .map(|(id, _)| (id.clone(), strategy_of(Some(id)))),
    );
    Ok(Scored {
        snapshot: composed,
        key,
        containers,
        shapes: shapes_out,
        strategies: strategies_out,
    })
}

/// Lays out the children of `parent` (`None` for the root level), recursing into every container child first. Appends the
/// containers in the order their sizes are fixed, and records each fixed size. The interior of a container is laid out in
/// its effective direction.
fn layout_level(
    graph: &Graph,
    parents: &BTreeMap<&str, Option<&str>>,
    parent: Option<&str>,
    options: &LayoutOptions,
    shapes: &BTreeMap<String, Orientation>,
    strategies: &BTreeMap<String, LayoutStrategy>,
    containers: &mut Vec<(String, String)>,
) -> Result<(BTreeMap<String, Rect>, String), String> {
    let children: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == parent)
        .map(|n| n.id.as_str())
        .collect();
    let mut below: BTreeMap<String, Rect> = BTreeMap::new();
    let mut level_sizes = options.node_sizes.clone();
    for child in &children {
        let has_children = parents.values().any(|p| *p == Some(child));
        if !has_children {
            continue;
        }
        // The container's interior first: its children, laid out as their own level.
        let (inner, inner_strategy) = layout_level(
            graph,
            parents,
            Some(child),
            options,
            shapes,
            strategies,
            containers,
        )?;
        let direct: Vec<&str> = children_of(graph, Some(child));
        let (min_x, min_y, max_x, max_y) = bounds(&inner, &direct);
        // The size of the container, and the shift that puts its interior inside the padding and below the header. An
        // authored size (ADR 0010) sets the extent; a fixed axis below its content is reported by the geometry validator.
        let content = NodeSize {
            width: (max_x - min_x) + 2. * PADDING,
            height: (max_y - min_y) + HEADER + PADDING,
        };
        let (size, _) = container_extent(options.container_sizing.get(*child), content);
        let shift = (PADDING - min_x, HEADER - min_y);
        for (id, r) in inner {
            let r = if direct.contains(&id.as_str()) {
                Rect {
                    x: r.x + shift.0,
                    y: r.y + shift.1,
                    width: r.width,
                    height: r.height,
                }
            } else {
                r
            };
            below.insert(id, r);
        }
        level_sizes.insert((*child).to_string(), size);
        containers.push(((*child).to_string(), inner_strategy));
    }

    // This level as a flat graph: its children are leaves, and an edge between two of them is an edge between the two
    // children that contain its endpoints. The level is laid out in the direction of its parent container.
    let direction = level_direction(graph, shapes, parent);
    let strategy = level_strategy(graph, strategies, parent);
    let is_child: BTreeSet<&str> = children.iter().copied().collect();
    let owner = |id: &str| -> Option<String> {
        let mut current = id;
        loop {
            if is_child.contains(current) {
                return Some(current.to_string());
            }
            current = (*parents.get(current)?)?;
        }
    };
    let level_graph = Graph {
        default_direction: direction,
        container_directions: BTreeMap::new(),
        default_strategy: strategy,
        container_strategies: Default::default(),
        default_routing: Default::default(),
        container_routing: Default::default(),
        nodes: children
            .iter()
            .map(|id| {
                let mut n = graph.nodes.iter().find(|n| n.id == *id).unwrap().clone();
                n.parent_id = None;
                n
            })
            .collect(),
        edges: graph
            .edges
            .iter()
            .filter_map(|e| {
                let target = e.target.as_deref()?;
                let (a, b) = (owner(&e.source)?, owner(target)?);
                (a != b).then(|| {
                    let mut e = e.clone();
                    e.source = a;
                    e.target = Some(b);
                    e
                })
            })
            .collect(),
        ports: vec![],
        edge_ports: vec![],
        buses: Vec::new(),
    };
    let mut level_options = options.clone();
    level_options.node_sizes = level_sizes;
    let laid = arqlet_layout::compute(&level_graph, None, &BTreeSet::new(), &level_options)?;
    // The level is refined by its own strategies, as the single container's interior is.
    let (refined, _, _) =
        refine_snapshot_logged(&laid, laid.revision, laid.layout_version, 1, None)?;
    let strategy = refined
        .optimization
        .as_ref()
        .map_or("keep-existing".to_string(), |o| o.strategy.clone());
    for id in &children {
        let r = refined
            .boxes
            .get(*id)
            .ok_or_else(|| format!("the level layout has no box for {id}"))?;
        below.insert((*id).to_string(), r.clone());
    }
    Ok((below, strategy))
}

fn children_of<'a>(graph: &'a Graph, parent: Option<&str>) -> Vec<&'a str> {
    graph
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == parent)
        .map(|n| n.id.as_str())
        .collect()
}

fn bounds(boxes: &BTreeMap<String, Rect>, ids: &[&str]) -> (f64, f64, f64, f64) {
    let rects: Vec<&Rect> = ids.iter().filter_map(|id| boxes.get(*id)).collect();
    (
        rects.iter().map(|r| r.x).fold(f64::INFINITY, f64::min),
        rects.iter().map(|r| r.y).fold(f64::INFINITY, f64::min),
        rects
            .iter()
            .map(|r| r.x + r.width)
            .fold(f64::NEG_INFINITY, f64::max),
        rects
            .iter()
            .map(|r| r.y + r.height)
            .fold(f64::NEG_INFINITY, f64::max),
    )
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

    /// Nested containers are composed level by level: the outer container contains a child container, and both containers
    /// have a valid fixed size. Every node is boxed, every edge has a route, and the chosen candidate is the better key.
    #[test]
    fn nested_containers_are_composed_level_by_level() {
        let graph = fixture("nested.json");
        let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        let snapshot = &result.snapshot;
        assert!(
            snapshot.geometry_issues.is_empty(),
            "{:?}",
            snapshot.geometry_issues
        );
        assert_eq!(snapshot.boxes.len(), graph.nodes.len());
        assert_eq!(snapshot.routes.len(), graph.edges.len());
        assert_eq!(
            result.containers.len(),
            2,
            "the inner and the outer container get fixed sizes"
        );
        match (&result.composed_key, &result.joint_key) {
            (Some(c), Some(j)) => {
                let best = if arqlet_layout::improves(c, j) { c } else { j };
                assert_eq!(&arqlet_layout::published_key(snapshot).unwrap(), best);
            }
            _ => panic!("both candidates are measurable on this fixture"),
        }
    }

    /// Absolute rectangle of a node: boxes are parent-relative, so the offsets of its ancestors are added.
    fn absolute(graph: &Graph, snapshot: &Snapshot, id: &str) -> Rect {
        let mut r = snapshot.boxes[id].clone();
        let mut parent = graph
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .parent_id
            .clone();
        while let Some(p) = parent {
            let pr = &snapshot.boxes[&p];
            r.x += pr.x;
            r.y += pr.y;
            parent = graph
                .nodes
                .iter()
                .find(|n| n.id == p)
                .unwrap()
                .parent_id
                .clone();
        }
        r
    }

    /// Distance from a point to the border of a rectangle (zero on the border, positive outside or inside).
    fn border_distance(p: &Point, r: &Rect) -> f64 {
        let (dx, dy) = (
            (p.x - r.x).abs().min((p.x - r.x - r.width).abs()),
            (p.y - r.y).abs().min((p.y - r.y - r.height).abs()),
        );
        let inside = p.x >= r.x && p.x <= r.x + r.width && p.y >= r.y && p.y <= r.y + r.height;
        if inside {
            dx.min(dy)
        } else {
            let ox = (r.x - p.x).max(0.).max(p.x - r.x - r.width);
            let oy = (r.y - p.y).max(0.).max(p.y - r.y - r.height);
            (ox * ox + oy * oy).sqrt()
        }
    }

    /// Every route starts on its source's border and ends on its target's border in the whole drawing, across every
    /// container boundary; the strict check the composition is held to.
    #[test]
    fn every_route_starts_and_ends_on_its_nodes_across_all_levels() {
        for name in ["nested.json", "two-containers.json"] {
            let graph = fixture(name);
            let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
            let snapshot = &result.snapshot;
            for edge in &graph.edges {
                let route = snapshot
                    .routes
                    .iter()
                    .find(|r| r.id == edge.id)
                    .expect("every edge has a route");
                let start = route.points.first().unwrap();
                let end = route.points.last().unwrap();
                let source = absolute(&graph, snapshot, &edge.source);
                let target = absolute(&graph, snapshot, edge.target.as_deref().unwrap());
                assert!(
                    border_distance(start, &source) <= 1.0,
                    "{name}: edge {} starts off {}",
                    edge.id,
                    edge.source
                );
                assert!(
                    border_distance(end, &target) <= 1.0,
                    "{name}: edge {} ends off {}",
                    edge.id,
                    edge.target.as_deref().unwrap()
                );
            }
        }
    }

    /// Several sibling containers are composed in one drawing; the result is valid and complete.
    #[test]
    fn several_containers_are_composed_in_one_drawing() {
        let graph = fixture("two-containers.json");
        let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        assert!(
            result.snapshot.geometry_issues.is_empty(),
            "{:?}",
            result.snapshot.geometry_issues
        );
        assert_eq!(result.snapshot.boxes.len(), graph.nodes.len());
        assert_eq!(result.snapshot.routes.len(), graph.edges.len());
        assert_eq!(result.containers.len(), 2);
    }

    /// The shape search never accepts a worse composition: the chosen composition's key is no worse than the baseline's,
    /// and the number of compositions stays within one per container plus the baseline.
    #[test]
    fn the_shape_search_never_returns_a_worse_composition() {
        for name in ["nested.json", "two-containers.json"] {
            let graph = fixture(name);
            let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
            if let (Some(chosen), Some(baseline)) = (&result.composed_key, &result.baseline_key) {
                assert!(
                    !arqlet_layout::improves(baseline, chosen),
                    "{name}: the composition is worse than its baseline"
                );
            }
            assert!(result.compositions <= result.containers.len() + 1, "{name}");
        }
    }

    /// An allowed set is searched only within its members: whatever the search chooses for the container is one of them.
    #[test]
    fn an_allowed_set_is_searched_only_within_its_members() {
        let mut graph = fixture("nested.json");
        graph.container_directions.insert(
            "outer".into(),
            arqlet_model::DirectionConstraint::Allowed(vec![Orientation::Lr, Orientation::Tb]),
        );
        let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        let outer = result.shapes.iter().find(|(id, _)| id == "outer").unwrap();
        assert!(outer.1 == "lr" || outer.1 == "tb", "{outer:?}");
        assert!(result.snapshot.geometry_issues.is_empty());
    }

    /// A fixed direction on a container is inherited by its child containers that declare nothing: the child's interior
    /// follows it, and the search does not change it.
    #[test]
    fn an_inherited_fixed_direction_is_followed_and_not_searched() {
        let mut graph = fixture("nested.json");
        graph.container_directions.insert(
            "outer".into(),
            arqlet_model::DirectionConstraint::Fixed(Orientation::Tb),
        );
        let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        let inner = result.shapes.iter().find(|(id, _)| id == "inner").unwrap();
        assert_eq!(inner.1, "tb");
    }

    /// The acceptance case of the portfolio: an outer hierarchical level, a tree-shaped container, a network container, both
    /// on `auto`. The automatic search picks per container among the eligible strategies; the tree is never chosen for the
    /// network (it has a cycle), and the drawing is complete and valid.
    #[test]
    fn automatic_portfolio_chooses_per_container_among_eligible_strategies() {
        let graph = fixture("portfolio-nested.json");
        let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        assert!(
            result.snapshot.geometry_issues.is_empty(),
            "{:?}",
            result.snapshot.geometry_issues
        );
        assert_eq!(result.snapshot.routes.len(), graph.edges.len());
        let chosen = |id: &str| {
            result
                .strategies
                .iter()
                .find(|(k, _)| k == id)
                .unwrap()
                .1
                .clone()
        };
        assert_ne!(
            chosen("N"),
            "tree",
            "a network with a cycle cannot be a tree"
        );
        assert!(
            ["none", "tree", "organic"].contains(&chosen("T").as_str()),
            "{}",
            chosen("T")
        );
        if let (Some(c), Some(b)) = (&result.composed_key, &result.baseline_key) {
            assert!(!arqlet_layout::improves(b, c), "worse than the baseline");
        }
    }

    /// Explicit strategies are binding: a fixed tree and a fixed organic container are reported as set, and the search does
    /// not replace them.
    #[test]
    fn explicit_strategies_are_not_replaced_by_the_search() {
        let mut graph = fixture("portfolio-nested.json");
        graph.container_strategies.insert(
            "T".into(),
            arqlet_model::StrategyConstraint::Fixed(arqlet_model::LayoutStrategy::Tree),
        );
        graph.container_strategies.insert(
            "N".into(),
            arqlet_model::StrategyConstraint::Fixed(arqlet_model::LayoutStrategy::Organic),
        );
        let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        let chosen = |id: &str| {
            result
                .strategies
                .iter()
                .find(|(k, _)| k == id)
                .unwrap()
                .1
                .clone()
        };
        assert_eq!(chosen("T"), "tree");
        assert_eq!(chosen("N"), "organic");
        assert!(result.snapshot.geometry_issues.is_empty());
    }

    /// A strategy `auto` is searched like the other choices: the chosen composition is never worse than the one without the
    /// search, and the strategy each level took is reported. A tree whose children are not a forest is skipped, not forced.
    #[test]
    fn an_auto_strategy_is_searched_and_never_worse_than_the_baseline() {
        let mut graph = fixture("mixed-tree-radial.json");
        graph
            .container_strategies
            .insert("A".into(), arqlet_model::StrategyConstraint::Auto);
        graph
            .container_strategies
            .insert("B".into(), arqlet_model::StrategyConstraint::Auto);
        let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        assert!(
            result.snapshot.geometry_issues.is_empty(),
            "{:?}",
            result.snapshot.geometry_issues
        );
        assert_eq!(result.snapshot.routes.len(), graph.edges.len());
        assert!(result.compositions > 1, "the auto strategies were tried");
        if let (Some(chosen), Some(baseline)) = (&result.composed_key, &result.baseline_key) {
            assert!(
                !arqlet_layout::improves(baseline, chosen),
                "worse than the baseline"
            );
        }
        assert!(result
            .strategies
            .iter()
            .any(|(id, name)| id == "A" && !name.is_empty()));
    }

    /// A mixed drawing: an outer hierarchical level, one container laid out as a tree and another as a radial tree, both
    /// composed by the same recursive procedure. The drawing is complete and valid.
    #[test]
    fn a_tree_container_and_a_radial_container_compose_in_one_drawing() {
        let graph = fixture("mixed-tree-radial.json");
        let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        assert!(
            result.snapshot.geometry_issues.is_empty(),
            "{:?}",
            result.snapshot.geometry_issues
        );
        assert_eq!(result.snapshot.boxes.len(), graph.nodes.len());
        assert_eq!(result.snapshot.routes.len(), graph.edges.len());
        assert_eq!(result.containers.len(), 2);
    }

    /// Regression: an authored container direction changes the drawing of its interior, not only the report. A chain of
    /// three children inside one container is stacked along the y axis under `tb` and along the x axis under `lr`.
    #[test]
    fn an_authored_container_direction_changes_the_interior_drawing() {
        let mut graph = fixture("container.json");
        graph.container_directions.insert(
            "box".into(),
            arqlet_model::DirectionConstraint::Fixed(Orientation::Tb),
        );
        let tb = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        graph.container_directions.insert(
            "box".into(),
            arqlet_model::DirectionConstraint::Fixed(Orientation::Lr),
        );
        let lr = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        let chain = |s: &Snapshot| {
            let (c1, c2, c3) = (&s.boxes["c1"], &s.boxes["c2"], &s.boxes["c3"]);
            (
                (c2.x - c1.x).abs(),
                (c2.y - c1.y).abs(),
                (c3.x - c2.x).abs(),
                (c3.y - c2.y).abs(),
            )
        };
        let (tb_dx, tb_dy, _, _) = chain(&tb.snapshot);
        let (_, _, lr_dx, lr_dy) = chain(&lr.snapshot);
        assert!(
            tb_dy > tb_dx,
            "tb stacks the chain vertically: dx {tb_dx}, dy {tb_dy}"
        );
        assert!(
            lr_dx > lr_dy,
            "lr stacks the chain horizontally: dx {lr_dx}, dy {lr_dy}"
        );
    }

    /// A container with an explicit direction keeps it: the search never changes it.
    #[test]
    fn an_explicit_container_direction_is_not_searched() {
        let mut graph = fixture("nested.json");
        graph.container_directions.insert(
            "outer".into(),
            arqlet_model::DirectionConstraint::Fixed(Orientation::Tb),
        );
        let result = refine_hierarchical(&graph, &LayoutOptions::default()).unwrap();
        let outer = result.shapes.iter().find(|(id, _)| id == "outer").unwrap();
        assert_eq!(outer.1, "tb");
    }
}
