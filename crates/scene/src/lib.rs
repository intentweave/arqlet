//! Geometry constraints independent of layout and rendering.
use arqlet_model::*;
use std::collections::{BTreeMap, BTreeSet};
pub mod geom;
pub mod metrics;
pub mod quality;
pub mod realize;
mod scene;
pub use scene::{build_corner_scene, build_scene, Scene, SceneConflict, SceneNode, SceneRoute};

#[derive(Clone, Copy)]
pub struct GeometryRules {
    pub gap: f64,
    pub padding: f64,
    pub header: f64,
}
impl Default for GeometryRules {
    fn default() -> Self {
        Self {
            gap: 12.,
            padding: 12.,
            header: 36.,
        }
    }
}
fn issue(kind: &str, ids: Vec<String>, message: &str) -> GeometryIssue {
    GeometryIssue {
        kind: kind.into(),
        node_ids: ids,
        message: message.into(),
    }
}
/// All unrelated nodes are tested, not just siblings. Ancestor containment is intentional.
pub fn validate_geometry(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    rules: GeometryRules,
) -> Vec<GeometryIssue> {
    let mut issues = Vec::new();
    if [rules.gap, rules.padding, rules.header]
        .iter()
        .any(|v| !v.is_finite() || *v < 0.)
    {
        return vec![issue(
            "invalid_rules",
            vec![],
            "Geometry rules must be finite and nonnegative",
        )];
    }
    if let Err(e) = validate(graph) {
        return vec![issue("invalid_graph", vec![], &e)];
    }
    for n in &graph.nodes {
        match boxes.get(&n.id) {
            None => issues.push(issue(
                "missing_rectangle",
                vec![n.id.clone()],
                "Node has no rectangle",
            )),
            Some(r)
                if ![r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite())
                    || r.width <= 0.
                    || r.height <= 0. =>
            {
                issues.push(issue(
                    "invalid_rectangle",
                    vec![n.id.clone()],
                    "Rectangle must be finite and have positive size",
                ))
            }
            _ => {}
        }
    }
    let known: BTreeSet<&str> = graph.nodes.iter().map(|n| n.id.as_str()).collect();
    for id in boxes.keys() {
        if !known.contains(id.as_str()) {
            issues.push(issue(
                "unknown_rectangle",
                vec![id.clone()],
                "Rectangle has no graph node",
            ));
        }
    }
    if issues
        .iter()
        .any(|i| i.kind == "missing_rectangle" || i.kind == "invalid_rectangle")
    {
        return issues;
    }
    let nodes: BTreeMap<_, _> = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut absolute = BTreeMap::new();
    let mut ancestors = BTreeMap::new();
    for n in &graph.nodes {
        let r = &boxes[&n.id];
        let mut a = r.clone();
        let mut chain = BTreeSet::new();
        let mut parent = n.parent_id.as_deref();
        while let Some(id) = parent {
            chain.insert(id.to_string());
            a.x += boxes[id].x;
            a.y += boxes[id].y;
            parent = nodes[id].parent_id.as_deref();
        }
        if ![a.x, a.y, a.x + a.width, a.y + a.height]
            .iter()
            .all(|v| v.is_finite())
        {
            issues.push(issue(
                "coordinate_overflow",
                vec![n.id.clone()],
                "Absolute coordinates overflow",
            ));
        }
        absolute.insert(n.id.as_str(), a);
        ancestors.insert(n.id.as_str(), chain);
        if let Some(parent) = &n.parent_id {
            let container = &boxes[parent];
            // A circle container holds its children inside its disc (less the padding), below the header.
            let outside = if nodes[parent.as_str()].shape == arqlet_model::NodeShape::Circle {
                r.y < rules.header - 1e-6
                    || arqlet_model::outline::Outline::of(n.shape, r).overshoot_beyond_disc(
                        container.width / 2.,
                        container.height / 2.,
                        container.width.min(container.height) / 2. - rules.padding,
                    ) > 1e-6
            } else {
                r.x < rules.padding - 1e-6
                    || r.y < rules.header - 1e-6
                    || r.x + r.width > container.width - rules.padding + 1e-6
                    || r.y + r.height > container.height - rules.padding + 1e-6
            };
            if outside {
                issues.push(issue(
                    "container_bounds",
                    vec![n.id.clone(), parent.clone()],
                    "Child intrudes into header/padding or extends beyond its container",
                ));
            }
        }
    }
    if issues.iter().any(|i| i.kind == "coordinate_overflow") {
        return issues;
    }
    // The outline of each node: a rectangle, or the disc of a circle, which can come closer to a neighbour in its box corners.
    // By node index; the ancestry is looked up only for the pairs that come too close.
    let outlines: Vec<arqlet_model::outline::Outline> = graph
        .nodes
        .iter()
        .map(|n| arqlet_model::outline::Outline::of(n.shape, &absolute[n.id.as_str()]))
        .collect();
    for (i, a) in graph.nodes.iter().enumerate() {
        for (j, b) in graph.nodes.iter().enumerate().skip(i + 1) {
            let (x, y) = (&outlines[i], &outlines[j]);
            let collides = arqlet_model::outline::collides(x, y);
            if !collides && !arqlet_model::outline::within(x, y, rules.gap - 1e-6) {
                continue;
            }
            if ancestors[a.id.as_str()].contains(&b.id) || ancestors[b.id.as_str()].contains(&a.id)
            {
                continue;
            }
            if collides {
                issues.push(issue(
                    "node_overlap",
                    vec![a.id.clone(), b.id.clone()],
                    "Unrelated node rectangles overlap",
                ));
            } else if arqlet_model::outline::within(x, y, rules.gap - 1e-6) {
                issues.push(issue(
                    "minimum_gap",
                    vec![a.id.clone(), b.id.clone()],
                    "Node separation is smaller than the required clearance",
                ));
            }
        }
    }
    issues
}
#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            label: id.into(),
            kind: "node".into(),
            parent_id: parent.map(str::to_string),
            shape: Default::default(),
        }
    }
    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }
    #[test]
    fn valid_nesting_is_not_collision() {
        let g = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![node("root", None), node("child", Some("root"))],
            edges: vec![],
            buses: Vec::new(),
        };
        let b = BTreeMap::from([
            ("root".into(), rect(0., 0., 200., 150.)),
            ("child".into(), rect(20., 40., 50., 50.)),
        ]);
        assert!(validate_geometry(&g, &b, GeometryRules::default()).is_empty());
    }
    #[test]
    fn catches_cross_branch_overlap() {
        let g = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![
                node("a", None),
                node("b", None),
                node("x", Some("a")),
                node("y", Some("b")),
            ],
            edges: vec![],
            buses: Vec::new(),
        };
        let b = BTreeMap::from([
            ("a".into(), rect(0., 0., 200., 200.)),
            ("b".into(), rect(10., 0., 200., 200.)),
            ("x".into(), rect(20., 40., 50., 50.)),
            ("y".into(), rect(20., 40., 50., 50.)),
        ]);
        assert!(validate_geometry(&g, &b, GeometryRules::default())
            .iter()
            .any(|i| i.kind == "node_overlap" && i.node_ids == vec!["x", "y"]));
    }
    #[test]
    fn reports_header_and_clearance() {
        let g = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![
                node("root", None),
                node("a", Some("root")),
                node("b", Some("root")),
            ],
            edges: vec![],
            buses: Vec::new(),
        };
        let b = BTreeMap::from([
            ("root".into(), rect(0., 0., 300., 200.)),
            ("a".into(), rect(20., 20., 50., 50.)),
            ("b".into(), rect(75., 40., 50., 50.)),
        ]);
        let issues = validate_geometry(&g, &b, GeometryRules::default());
        assert!(issues.iter().any(|i| i.kind == "container_bounds"));
        assert!(issues.iter().any(|i| i.kind == "minimum_gap"));
    }
    #[test]
    fn exact_clearance_is_valid() {
        let g = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![node("a", None), node("b", None)],
            edges: vec![],
            buses: Vec::new(),
        };
        let b = BTreeMap::from([
            ("a".into(), rect(0., 0., 50., 50.)),
            ("b".into(), rect(62., 0., 50., 50.)),
        ]);
        assert!(validate_geometry(&g, &b, GeometryRules::default()).is_empty());
    }
    #[test]
    fn missing_and_nonfinite_are_diagnostic() {
        let g = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![node("a", None), node("b", None)],
            edges: vec![],
            buses: Vec::new(),
        };
        let b = BTreeMap::from([("a".into(), rect(f64::NAN, 0., 50., 50.))]);
        assert_eq!(validate_geometry(&g, &b, GeometryRules::default()).len(), 2);
    }
}
