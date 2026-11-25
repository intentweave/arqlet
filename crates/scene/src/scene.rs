//! Scene v1: renderer-neutral absolute geometry derived from an engine snapshot.
//!
//! The scene only projects a snapshot. It runs no layout, does not alter graph semantics and does
//! not invent routes. Container coordinates are flattened once: consumers must not apply a parent
//! transform to routes or labels again.
use crate::{geom, realize};
use arqlet_model::math::PortableMath;
use arqlet_model::*;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Scene {
    pub schema_version: u32,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub layout_version: u64,
    pub nodes: Vec<SceneNode>,
    pub routes: Vec<SceneRoute>,
    pub ports: Vec<ResolvedPort>,
    pub labels: Vec<PlacedLabel>,
    pub crossings: Vec<BoundaryCrossing>,
    pub show_ports: bool,
    /// What does not hold in the drawing, with every party of it: overlaps and children that do not fit a container, authored
    /// sizes below their content, routes through nodes. Omitted when there is none, so a clean drawing is unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts: Vec<SceneConflict>,
    /// Explicit buses (ADR 0005), each as its edges' ids, for a renderer and for the shared conflict queries (members of one
    /// bus may share lines). Omitted when there is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buses: Vec<Vec<String>>,
    /// Where the lines of a bus branch: points at which its drawn routes run off in three or more directions. A renderer marks
    /// them as junction dots. Omitted when there is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub junctions: Vec<Point>,
    /// Provisional: the bus lines of the view (a bus kept as a node of shape `busLine` in the source graph): each a bar with
    /// its taps, one per edge of the bus, in the edge's direction. Omitted when there is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bus_lines: Vec<SceneBusLine>,
    /// Provisional (`hubRefs`): edges of hubs drawn as stubs at their other end, named after the hub. Omitted when none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hub_refs: Vec<SceneHubRef>,
    /// Provisional (ADR 0022, the soft criteria of `groupingPriority`): a hull per membership group, to be drawn behind the
    /// nodes. In the declared order of the dimensions and of their groups. Omitted when there is none, so every drawing
    /// without the option is unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub membership_hulls: Vec<SceneHull>,
    /// Provisional (ADR 0022 §5): one entry per dimension the options name, with its counts and one summary line.
    /// Omitted when there is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub membership_report: Vec<SceneMembershipReport>,
    /// Provisional (ADR 0022 §3, M2b): the members of the soft groups that fell into too many pieces to be drawn as hulls,
    /// marked one by one instead (a colour or a badge on the box, no polygon). Omitted when there is none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub membership_marks: Vec<SceneMark>,
    /// Extent of everything drawn, with a 30-unit margin. May start at negative coordinates.
    pub bounds: Rect,
}

/// Provisional (ADR 0022 §3, M2b): one member of a marked group — the box to mark, and the group it belongs to.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneMark {
    pub dimension: String,
    pub dimension_label: String,
    pub group: String,
    /// What to write in the mark's tooltip or legend: the group's label, or its id where it has none.
    pub label: String,
    /// The box that carries the mark: the member's own box, or the box that stands for it.
    pub node: String,
    /// The group's own box, when the view still draws it and this member left it: what the mark points back to.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub own_box: Option<String>,
}

/// Provisional (ADR 0022, first slice): the hull of one membership group, drawn behind the nodes. `points` is a closed
/// polygon (the last point joins the first) and is empty for no group: a group whose members the view does not show is
/// left out instead. The hull is **not an obstacle**: the router never saw it, and routes may cross it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneHull {
    /// The dimension this group belongs to, and its label for a legend.
    pub dimension: String,
    pub dimension_label: String,
    pub group: String,
    /// What to write on the hull: the group's label, or its id where it has none.
    pub label: String,
    pub points: Vec<Point>,
    /// The boxes the hull was built from: a member's own box, or the box that stands for it (a collapsed container).
    pub boxes: Vec<String>,
    /// Leaf boxes inside the hull that are not members of the group. The honest cost of drawing a hull around nodes the
    /// layout kept apart, and one of the two numbers a later default depends on (ADR 0022 §6). Empty once a group is split
    /// (M2b): a group is split exactly so that no piece encloses a foreign box.
    pub intruders: Vec<String>,
    /// Provisional (ADR 0022 §3, M2b): this hull is piece `piece` (from 0) of `pieces` of its group. A group whose whole
    /// hull would enclose a foreign box is drawn in pieces — one per owner, then by distance within an owner.
    pub piece: usize,
    pub pieces: usize,
}

/// Provisional (ADR 0022 §5): what one membership dimension did in this drawing. Typed, because one line of text per
/// group would be thousands of lines on a real graph; `summary` is the single line a host can simply print.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneMembershipReport {
    pub dimension: String,
    /// The mode the options asked for (`overlay`, `hierarchy`, `lanes`).
    pub mode: String,
    /// Whether this drawing applied it. `hierarchy` and `lanes` are accepted and not yet applied (ADR 0022, M1 and M4).
    pub applied: bool,
    /// Groups the dimension declares, and how many of them were drawn.
    pub groups: usize,
    pub drawn: usize,
    /// Leaf boxes inside a hull they do not belong to, summed over the dimension's groups.
    pub intruders: usize,
    /// Pairs of this dimension's hulls that overlap. Hulls of *different* dimensions are expected to overlap — that is
    /// what cutting across each other means — so only pairs within one dimension are counted.
    pub overlapping_pairs: usize,
    /// Member ids the source graph does not have. Information, not a conflict (ADR 0022 §1).
    pub unknown_members: usize,
    /// Members the view does not show and for which no box stands in (filtered out, or outside the view).
    pub hidden_members: usize,
    /// Nodes this criterion owns: it is the first that assigns them (ADR 0022 §2, rule 1).
    pub owned: usize,
    /// Of those, the nodes in several of its groups that are not one path: the value order chose (rule 2).
    pub multiple_owners: usize,
    /// Nodes no listed criterion assigns (rule 3), placed at the root. Counted on the first criterion's entry only.
    pub unassigned: usize,
    /// Provisional (ADR 0022 §3, M2b): groups drawn in more than one piece (`grouping-split`), and groups drawn as marks
    /// because they fell into more pieces than `MARKS_ABOVE_PIECES` (`grouping-marked`).
    pub split: usize,
    pub marked: usize,
    /// The foreign leaf boxes the groups' hulls would enclose if each group were drawn whole, as the first slice drew them.
    /// Kept beside `intruders` for one version: splitting lowers `intruders` by construction, which is not a layout gain.
    pub intruders_whole: usize,
    /// Groups the view still draws as a box that lost some members to another criterion: only those members are drawn,
    /// joined to the box where that is compact and encloses nothing foreign, else marked (ADR 0022 §3). The box shows the rest.
    pub kept_box: usize,
    /// Provisional (ADR 0022 M3b): groups the layout holds together inside their owner (`LayoutOptions::hold_memberships`).
    #[serde(skip_serializing_if = "is_zero")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<usize>"))]
    pub held: usize,
    pub summary: String,
}

/// Provisional: one bus line of a scene (`Scene::bus_lines`).
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneBusLine {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub bounds: Rect,
    pub taps: Vec<SceneTap>,
}

/// Provisional: an edge drawn as a reference to a hub (`Scene::hub_refs`): a short stub at `node` (out of its right side for
/// an edge to the hub, into its left side for one from it), in the edge's direction, and the hub's name (`text`) in
/// `label` beside the stub's free end.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneHubRef {
    pub edge: String,
    pub node: String,
    pub hub: String,
    pub outgoing: bool,
    pub text: String,
    pub points: Vec<Point>,
    pub label: Rect,
}

/// Provisional: one tap of a bus line: the edge it stands for, the node it attaches, and its points.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneTap {
    pub edge: String,
    pub node: String,
    pub points: Vec<Point>,
}

/// One conflict of the drawing. A renderer marks every node and route it names and shows `reason` for them.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneConflict {
    /// The kind: a geometry issue's kind (`node_overlap`, `container_bounds`, `minimum_gap`, `container-fixed-below-content`, ...)
    /// or `route-through-node`.
    pub kind: String,
    /// The nodes it involves, and the routes (edge IDs).
    pub nodes: Vec<String>,
    pub routes: Vec<String>,
    pub reason: String,
}

/// A row of a node's content, with its absolute bounds (ADR 0011). The viewer draws the content inside it.
#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneRow {
    pub id: String,
    pub label: String,
    pub bounds: Rect,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneNode {
    #[serde(flatten)]
    pub node: Node,
    /// Absolute bounds in scene coordinates.
    pub bounds: Rect,
    pub container: bool,
    pub aggregate: bool,
    pub collapsed: bool,
    pub invalid: bool,
    /// The rows of the node's content, top to bottom (empty for a node without rows).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rows: Vec<SceneRow>,
    /// Provisional: for a container, how many children it holds in the view (the communities and groups it adds count
    /// as children; a partly shown container counts the children in its rest too). Omitted for a leaf.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub child_count: Option<usize>,
    /// Provisional: of those, how many are not drawn: all of them for a collapsed container, those in its rest (`+N more`)
    /// for a partly shown one. Omitted when every child is drawn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub hidden_count: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct SceneRoute {
    pub id: String,
    /// The edge's kind: for a relationship, its type (the verb), drawn as the route's label.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub kind: String,
    pub source: String,
    pub target: String,
    pub points: Vec<Point>,
    pub weight: usize,
    /// Provisional (communities): the route stands for collapsed content in both directions; this many edges run the
    /// other way (from `target` to `source`), `weight` in its own. Present only on such routes, 0 when every one of them
    /// runs one way; a renderer draws an arrow at the start too when it is above 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub back_weight: Option<usize>,
    /// True when the engine reported a structured conflict for this route.
    pub invalid: bool,
    /// True when the route is drawn with rounded corners (styles `roundedOrthogonal` and `roundedPolyline`). Its points
    /// already contain the arcs, sampled by the engine (`realize`); a renderer draws them as they are. Omitted otherwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub rounded: bool,
    /// Edges this drawn route stands for (a bundle leader's members). Absent when the route is one edge.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub members: Vec<String>,
}

/// How far a polyline route keeps from the nodes it passes (layout units).
const POLYLINE_CLEARANCE: f64 = 4.;

/// The same for a rounded polyline: the arcs cut the corners, so the bends keep further from the nodes.
const ROUNDED_POLYLINE_CLEARANCE: f64 = 10.;

/// Builds scene v1 from a snapshot. Bundle members are omitted: they share their leader's route.
/// The junctions of the buses (`Scene::junctions`): the points of their members' routes at which the bus's lines run off in
/// three or more directions, in a fixed order.
fn junctions_of(snapshot: &Snapshot, routes: &[SceneRoute]) -> Vec<Point> {
    const EPS: f64 = 1e-6;
    let mut out: Vec<Point> = Vec::new();
    for bus in snapshot.graph.effective_buses().iter() {
        let lines: Vec<&Vec<Point>> = routes
            .iter()
            .filter(|r| bus.edges.contains(&r.id))
            .map(|r| &r.points)
            .collect();
        let segments: Vec<(&Point, &Point)> = lines
            .iter()
            .flat_map(|p| p.windows(2).map(|w| (&w[0], &w[1])))
            .collect();
        for p in lines.iter().flat_map(|p| p.iter()) {
            // The directions in which the bus's segments leave `p` (a segment through it leaves both ways).
            let mut directions: Vec<(f64, f64)> = Vec::new();
            for &(a, b) in &segments {
                let on = |q: &Point| (q.x - p.x).abs() < EPS && (q.y - p.y).abs() < EPS;
                let toward = |q: &Point| {
                    let (dx, dy) = (q.x - p.x, q.y - p.y);
                    let l = dx.p_hypot(dy).max(EPS);
                    (dx / l, dy / l)
                };
                let inside = {
                    let (x0, x1) = (a.x.min(b.x) - EPS, a.x.max(b.x) + EPS);
                    let (y0, y1) = (a.y.min(b.y) - EPS, a.y.max(b.y) + EPS);
                    let cross = (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);
                    cross.abs() < EPS && p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1
                };
                if !inside {
                    continue;
                }
                for q in [a, b] {
                    if !on(q) {
                        let d = toward(q);
                        if !directions
                            .iter()
                            .any(|e| (e.0 - d.0).abs() < 1e-3 && (e.1 - d.1).abs() < 1e-3)
                        {
                            directions.push(d);
                        }
                    }
                }
            }
            if directions.len() >= 3
                && !out
                    .iter()
                    .any(|q| (q.x - p.x).abs() < EPS && (q.y - p.y).abs() < EPS)
            {
                out.push(p.clone());
            }
        }
    }
    out.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    out
}

pub fn build_scene(snapshot: &Snapshot) -> Result<Scene, String> {
    build_scene_with(snapshot, true)
}

/// The scene with the corner points of every route: pulled taut where the style asks for it, but without the sampled arcs
/// of rounded styles. The metrics measure this geometry (bends are corners, not samples of an arc).
pub fn build_corner_scene(snapshot: &Snapshot) -> Result<Scene, String> {
    build_scene_with(snapshot, false)
}

fn build_scene_with(snapshot: &Snapshot, arcs: bool) -> Result<Scene, String> {
    if snapshot.schema_version != 1 {
        return Err("Unsupported snapshot schema".into());
    }
    let graph = &snapshot.graph;
    let by_id: BTreeMap<&str, &Node> = graph.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let source = snapshot.source_graph.as_deref().unwrap_or(graph);
    let aggregates: BTreeSet<&str> = source
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let closed: BTreeSet<&str> = snapshot.collapsed_ids.iter().map(String::as_str).collect();
    let invalid: BTreeSet<&str> = snapshot
        .geometry_issues
        .iter()
        .flat_map(|issue| issue.node_ids.iter().map(String::as_str))
        .collect();
    let conflicted: BTreeSet<&str> = snapshot
        .route_conflicts
        .iter()
        .map(|c| c.edge_id.as_str())
        .collect();

    let mut positions: BTreeMap<&str, Rect> = BTreeMap::new();
    let mut nodes = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let bounds = absolute(
            &node.id,
            &by_id,
            snapshot,
            &mut positions,
            &mut BTreeSet::new(),
        )?;
        let rows = node_rows(snapshot, &node.id, &bounds);
        nodes.push(SceneNode {
            node: node.clone(),
            bounds,
            container: containers.contains(node.id.as_str()),
            aggregate: aggregates.contains(node.id.as_str()),
            collapsed: closed.contains(node.id.as_str()),
            invalid: invalid.contains(node.id.as_str()),
            rows,
            child_count: None,
            hidden_count: None,
        });
    }

    let straight: BTreeSet<&str> = snapshot.straight_edges.iter().map(String::as_str).collect();
    let edges: BTreeMap<&str, &Edge> = graph.edges.iter().map(|e| (e.id.as_str(), e)).collect();
    let mut routes = Vec::new();
    let weighted = snapshot.weighted_routes();
    let mut pulled_ids: BTreeSet<String> = BTreeSet::new();
    for route in snapshot.routes.iter().filter(|r| r.bundle_of.is_none()) {
        let edge = edges
            .get(route.id.as_str())
            .ok_or_else(|| format!("Missing route endpoints: {}", route.id))?;
        let target = edge
            .target
            .as_deref()
            .filter(|t| by_id.contains_key(edge.source.as_str()) && by_id.contains_key(t))
            .ok_or_else(|| format!("Missing route endpoints: {}", route.id))?;
        if route
            .points
            .iter()
            .any(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Err(format!("Invalid route: {}", route.id));
        }
        let constraint = (!straight.contains(route.id.as_str()))
            .then(|| graph.routing_constraint(graph.edge_area(&edge.source, target).as_deref()))
            .flatten();
        let pulled = |c: &RoutingConstraint| {
            [
                RoutingStyle::Polyline,
                RoutingStyle::RoundedPolyline,
                RoutingStyle::Curve,
            ]
            .into_iter()
            .find(|s| c.picks(*s))
        };
        let style = constraint.as_ref().and_then(pulled);
        let rounds = constraint.as_ref().is_some_and(|c| c.rounds());
        let chamfers = constraint.as_ref().is_some_and(|c| c.chamfers());
        let curves = constraint.as_ref().is_some_and(|c| c.curves());
        // A container that holds an endpoint is traversable for the edge; every other node is an obstacle, except the two
        // endpoints, which the route may leave and reach along their border. A pulled route keeps a distance; a rounded
        // orthogonal route only must not run into a node.
        let obstacles = |clearance: f64| -> Vec<geom::Rect> {
            let holding: BTreeSet<&str> = [edge.source.as_str(), target]
                .into_iter()
                .flat_map(|id| {
                    std::iter::successors(by_id.get(id).copied(), |n| {
                        n.parent_id.as_deref().and_then(|p| by_id.get(p).copied())
                    })
                })
                .map(|n| n.id.as_str())
                .collect();
            positions
                .iter()
                .filter(|(id, _)| !holding.contains(**id) || **id == edge.source || **id == target)
                .map(|(id, r)| {
                    let rect = geom::Rect {
                        x0: r.x,
                        y0: r.y,
                        x1: r.x + r.width,
                        y1: r.y + r.height,
                    };
                    if *id == edge.source || *id == target {
                        rect
                    } else {
                        rect.inflate(clearance)
                    }
                })
                .collect()
        };
        let clearance = match style {
            // A curve bulges past its route points: it keeps the larger distance of the rounded polyline.
            Some(RoutingStyle::RoundedPolyline | RoutingStyle::Curve) => ROUNDED_POLYLINE_CLEARANCE,
            Some(_) => POLYLINE_CLEARANCE,
            None => 0.,
        };
        let mut points = route.points.clone();
        if style.is_some() || rounds || chamfers {
            let blocks = obstacles(clearance);
            if style.is_some() {
                pulled_ids.insert(route.id.clone());
                points = geom::pull_taut(&points, &blocks);
            }
            if rounds && arcs {
                points = realize::round_corners(&points, realize::CORNER_RADIUS, &blocks);
            }
            // Octilinear: the corners cut at 45°. Like the arcs, only in the drawn scene: the metrics count the corners of
            // the orthogonal route (`build_corner_scene`).
            if chamfers && arcs {
                points = realize::chamfer_corners(&points, realize::CHAMFER, &blocks);
            }
            // A curve through the pulled route's points; the metrics count those points' corners.
            if curves && arcs {
                points = realize::smooth_curve(&points, &blocks);
            }
        }
        routes.push(SceneRoute {
            id: route.id.clone(),
            kind: edge.kind.clone(),
            source: edge.source.clone(),
            target: target.to_string(),
            points,
            // An edge that stands for collapsed content: its weights come from the view (`aggregate_weights`), whichever
            // path computed the route.
            weight: match &weighted {
                // With input weights: the edges it stands for (`Snapshot::weighted_routes`).
                Some(w) => w.get(&route.id).copied().unwrap_or(route.weight),
                None => snapshot
                    .options
                    .aggregate_weights
                    .get(&route.id)
                    .map_or(route.weight, |w| w.forward.max(1)),
            },
            back_weight: snapshot
                .options
                .aggregate_weights
                .get(&route.id)
                .map(|w| w.backward),
            invalid: conflicted.contains(route.id.as_str()),
            rounded: rounds,
            members: Vec::new(),
        });
    }
    // Bundle members are not drawn; their leader's route names them so a selection can show what it carries.
    for member in snapshot
        .routes
        .iter()
        .filter_map(|r| r.bundle_of.as_ref().map(|leader| (leader, &r.id)))
    {
        if let Some(leader) = routes.iter_mut().find(|r| &r.id == member.0) {
            leader.members.push(member.1.clone());
        }
    }

    let labels = snapshot.labels.clone();
    let mut left: f64 = 0.;
    let mut top: f64 = 0.;
    let mut right: f64 = 1.;
    let mut bottom: f64 = 1.;
    let mut include = |r: &Rect| {
        left = left.min(r.x);
        top = top.min(r.y);
        right = right.max(r.x + r.width);
        bottom = bottom.max(r.y + r.height);
    };
    for node in &nodes {
        include(&node.bounds);
    }
    // Provisional: the bus lines of the view (`Bus::taps`), each with its bar and taps.
    let bus_lines: Vec<SceneBusLine> = snapshot
        .graph
        .buses
        .iter()
        .filter_map(|bus| {
            let line = snapshot.bus_lines.get(&bus.id)?;
            Some(SceneBusLine {
                id: bus.id.clone(),
                label: bus.label.clone().unwrap_or_else(|| bus.id.clone()),
                kind: bus.kind.clone().unwrap_or_default(),
                bounds: line.bar.clone(),
                taps: bus
                    .taps
                    .iter()
                    .filter_map(|tap| {
                        Some(SceneTap {
                            edge: tap.edge.clone(),
                            node: tap.node.clone(),
                            points: line.taps.get(&tap.edge)?.clone(),
                        })
                    })
                    .collect(),
            })
        })
        .collect();
    for line in &bus_lines {
        include(&line.bounds);
    }
    // Provisional (ADR 0022): the membership hulls, which are derived from the boxes above and so cannot move anything.
    // Their points still widen the extent, because a hull reaches a padding beyond its outermost members.
    let (membership_hulls, membership_report, membership_marks) =
        membership_overlays(snapshot, &positions, &containers);
    for hull in &membership_hulls {
        for p in &hull.points {
            include(&Rect {
                x: p.x,
                y: p.y,
                width: 0.,
                height: 0.,
            });
        }
    }
    let hub_refs = hub_refs_of(snapshot, &positions);
    for r in &hub_refs {
        include(&r.label);
    }
    for label in &labels {
        include(&label.bounds);
    }
    for route in &routes {
        for p in &route.points {
            include(&Rect {
                x: p.x,
                y: p.y,
                width: 0.,
                height: 0.,
            });
        }
    }

    // The crossings of the snapshot are those of its orthogonal routes. A pulled route crosses the borders elsewhere, so
    // its crossings are found again on the drawn points.
    let mut crossings: Vec<BoundaryCrossing> = snapshot
        .boundary_crossings
        .iter()
        .filter(|c| !pulled_ids.contains(&c.edge_id))
        .cloned()
        .collect();
    let mut container_ids: Vec<&str> = containers.iter().copied().collect();
    container_ids.sort_unstable();
    for route in routes.iter().filter(|r| pulled_ids.contains(&r.id)) {
        for id in &container_ids {
            let Some(r) = positions.get(id) else { continue };
            let border = geom::Rect {
                x0: r.x,
                y0: r.y,
                x1: r.x + r.width,
                y1: r.y + r.height,
            };
            for (ordinal, (side, position)) in geom::border_crossings(&route.points, &border)
                .into_iter()
                .enumerate()
            {
                crossings.push(BoundaryCrossing {
                    edge_id: route.id.clone(),
                    container_id: id.to_string(),
                    ordinal,
                    side,
                    position,
                    derived: true,
                });
            }
        }
    }

    let junctions = junctions_of(snapshot, &routes);
    // Bridges (`appearance.bridges`): the later of two crossing routes hops over the other; members of one explicit bus
    // share their lines and never hop over each other. Only in the drawn scene, as the arcs.
    let mut routes = routes;
    if arcs && snapshot.options.appearance.bridges {
        let effective = snapshot.graph.effective_buses();
        let bus_of: BTreeMap<&str, Vec<usize>> =
            effective
                .iter()
                .enumerate()
                .fold(BTreeMap::new(), |mut m, (b, bus)| {
                    for e in &bus.edges {
                        m.entry(e.as_str()).or_insert_with(Vec::new).push(b);
                    }
                    m
                });
        let buses: Vec<Vec<usize>> = routes
            .iter()
            .map(|r| bus_of.get(r.id.as_str()).cloned().unwrap_or_default())
            .collect();
        let mut points: Vec<Vec<Point>> = routes.iter().map(|r| r.points.clone()).collect();
        realize::add_bridges(&mut points, realize::BRIDGE_RADIUS, &|i, j| {
            buses[i].iter().any(|b| buses[j].contains(b))
        });
        for (route, p) in routes.iter_mut().zip(points) {
            route.points = p;
        }
    }
    Ok(Scene {
        schema_version: 1,
        revision: snapshot.revision,
        layout_version: snapshot.layout_version,
        nodes,
        routes,
        ports: snapshot.resolved_ports.clone(),
        labels,
        crossings,
        show_ports: snapshot.options.appearance.show_ports,
        conflicts: conflicts_of(snapshot),
        buses: snapshot
            .graph
            .effective_buses()
            .iter()
            .map(|b| b.edges.clone())
            .collect(),
        junctions,
        bus_lines,
        hub_refs,
        membership_hulls,
        membership_report,
        membership_marks,
        bounds: Rect {
            x: left - 30.,
            y: top - 30.,
            width: right - left + 60.,
            height: bottom - top + 60.,
        },
    })
}

/// Provisional (ADR 0022 M4a): the lanes of the first criterion that do not lie behind the ones before them, across the flow of
/// the top level. `None` where the top level has no flow.
fn lane_breaks(
    snapshot: &Snapshot,
    positions: &BTreeMap<&str, Rect>,
    by: &str,
    ownership: &arqlet_model::grouping::Ownership,
) -> Option<usize> {
    let orientation = match snapshot.area_orientations.get("<root>") {
        Some(o) => *o,
        None => match snapshot.graph.area_mode(None) {
            Ok(Mode::Orientation(o)) => o,
            _ => return None,
        },
    };
    let source = snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph);
    let dimension = source.memberships.dimension(by)?;
    let mut extent: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for (id, r) in positions {
        if let Some(arqlet_model::grouping::Owner::Group {
            dimension: d,
            group,
        }) = ownership.owners.get(*id)
        {
            if d == by {
                let (lo, hi) = if orientation.vertical() {
                    (r.x, r.x + r.width)
                } else {
                    (r.y, r.y + r.height)
                };
                let e = extent.entry(group.as_str()).or_insert((lo, hi));
                *e = (e.0.min(lo), e.1.max(hi));
            }
        }
    }
    let (mut end, mut breaks) = (f64::NEG_INFINITY, 0);
    for g in &dimension.groups {
        if let Some(&(lo, hi)) = extent.get(g.id.as_str()) {
            breaks += usize::from(lo < end - 1e-6);
            end = end.max(hi);
        }
    }
    Some(breaks)
}

fn is_zero(value: &usize) -> bool {
    *value == 0
}

/// How far a hull reaches beyond the boxes it encloses, in layout units.
const HULL_PADDING: f64 = 10.;

/// Provisional (ADR 0022 §3, M2b): a group that falls into more pieces than this is marked member by member instead of
/// drawn as hulls. A placeholder until M3 measures where the switch lies.
pub const MARKS_ABOVE_PIECES: usize = 3;

/// Provisional (ADR 0022 §3): a hull that joins a group's own box to the members that left it may add at most this many
/// times the area of those members; beyond it the members are marked. A placeholder until M3 measures it.
const JOINED_GROWTH_LIMIT: f64 = 4.;

/// Provisional (ADR 0022 §2–§3, M1 and M2b): the hulls and marks of the soft criteria of the grouping priority, and the
/// report of every criterion it lists. The first criterion owns: its groups are the view's containers
/// (`with_grouping_priority`), so it draws no hull. Every later criterion holds its members softly — the parent hierarchy
/// too, once it is moved down, around the children of each source container — and draws each group in the first form
/// that encloses no foreign box (`hull::pieces`): one hull, else one per owner, else pieces by distance; a group in more
/// than `MARKS_ABOVE_PIECES` pieces is marked instead. A hull that would only trace a box the view already draws (every
/// member owned by that very group) is left out.
///
/// Everything here is derived from boxes that are already placed, so this function cannot move a node: a drawing with
/// hulls has the same node boxes and the same routes as the same drawing without them.
///
/// A member is resolved to the box that stands for it: its own box when the view shows it, else the container its owner
/// gives it in the view, else the nearest ancestor of the source the view shows (a collapsed container, as bus taps attach
/// to one). A member with no such box is counted as hidden, and one the source graph does not have at all as unknown.
fn membership_overlays(
    snapshot: &Snapshot,
    positions: &BTreeMap<&str, Rect>,
    containers: &BTreeSet<&str>,
) -> (Vec<SceneHull>, Vec<SceneMembershipReport>, Vec<SceneMark>) {
    use arqlet_model::grouping::{self, Owner, PARENT};
    let priority = &snapshot.options.grouping_priority;
    if grouping::is_default(priority) {
        return (Vec::new(), Vec::new(), Vec::new());
    }
    let source = snapshot.source_graph.as_deref().unwrap_or(&snapshot.graph);
    let ownership = grouping::owners(source, priority);
    // The container of every box of the view: the owner a group is split by (M2b).
    let view_parent: BTreeMap<&str, Option<&str>> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let parent_of: BTreeMap<&str, Option<&str>> = source
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let label_of: BTreeMap<&str, &str> = source
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.label.as_str()))
        .collect();
    let visible = |id: &str| positions.get_key_value(id).map(|(k, _)| *k);
    // The box that stands for `member`.
    let box_for = |member: &str| -> Option<&str> {
        if let Some(k) = visible(member) {
            return Some(k);
        }
        if let Some(Owner::Group { dimension, group }) = ownership.owners.get(member) {
            let id = grouping::container_id(dimension, group);
            if let Some(k) = visible(&id) {
                return Some(k);
            }
        }
        let mut at = *parent_of.get(member)?;
        while let Some(id) = at {
            if let Some(k) = visible(id) {
                return Some(k);
            }
            at = parent_of.get(id).copied().flatten();
        }
        None
    };
    // The children of every source container, for the parent hierarchy as a soft criterion.
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for n in &source.nodes {
        if let Some(p) = n.parent_id.as_deref() {
            children.entry(p).or_default().push(n.id.as_str());
        }
    }
    let mut hulls = Vec::new();
    let mut marks = Vec::new();
    let mut reports = Vec::new();
    for (rank, criterion) in priority.iter().enumerate() {
        let owning = rank == 0;
        let lanes = criterion.render == GroupingRendering::Lanes;
        // M4a: the first criterion's lanes, when the top level has a flow: how many lanes do not lie behind the one before.
        let lane_breaks = (lanes && owning)
            .then(|| lane_breaks(snapshot, positions, &criterion.by, &ownership))
            .flatten();
        let shape = criterion.hull.unwrap_or(snapshot.options.membership_hull);
        let owned = ownership.rank.values().filter(|r| **r == rank).count();
        let multiple = ownership
            .multiple
            .iter()
            .filter(|n| ownership.rank.get(n.as_str()) == Some(&rank))
            .count();
        // The groups of the criterion: (id, label, members, whether a member owned by this group is already its box).
        let mut groups: Vec<(String, String, Vec<String>)> = Vec::new();
        let mut unknown = 0usize;
        let mut dimension_label = String::new();
        if criterion.by == PARENT {
            dimension_label = "Parent".into();
            for n in &source.nodes {
                // Only the children, not every descendant: a nested hierarchy would repeat one hull per level around the
                // same nodes. A child container the projection dropped has no box; its children are its own group.
                if let Some(of) = children.get(n.id.as_str()) {
                    let members = of
                        .iter()
                        .filter(|c| !children.contains_key(**c) || visible(c).is_some())
                        .map(|c| c.to_string())
                        .collect();
                    groups.push((n.id.clone(), n.label.clone(), members));
                }
            }
        } else if let Some(d) = source.memberships.dimension(&criterion.by) {
            dimension_label = d.label.clone();
            // Membership in a nested group implies its ancestors (§1): a group encloses the members of every group below.
            // One walk up from each group, by a map of the parents, gives every group what it encloses; groups are taken in
            // their declared order, so each list keeps the order of the groups its members come from.
            let group_parent: BTreeMap<&str, Option<&str>> = d
                .groups
                .iter()
                .map(|g| (g.id.as_str(), g.parent.as_deref()))
                .collect();
            let mut enclosed: BTreeMap<&str, Vec<String>> = BTreeMap::new();
            for g in &d.groups {
                let mut at = Some(g.id.as_str());
                let mut walked = 0usize;
                while let Some(a) = at {
                    enclosed
                        .entry(a)
                        .or_default()
                        .extend(g.members.iter().cloned());
                    walked += 1;
                    if walked > d.groups.len() {
                        break; // a cycle, which validation refuses
                    }
                    at = group_parent.get(a).copied().flatten();
                }
            }
            for group in &d.groups {
                // A group the criterion does not consider (`consider`) is neither drawn nor counted.
                if !grouping::considers(criterion, &group.id) {
                    continue;
                }
                for m in &group.members {
                    if !parent_of.contains_key(m.as_str()) {
                        unknown += 1;
                    }
                }
                let members: Vec<String> = enclosed.remove(group.id.as_str()).unwrap_or_default();
                let label = if group.label.is_empty() {
                    group.id.clone()
                } else {
                    group.label.clone()
                };
                groups.push((group.id.clone(), label, members));
            }
        }
        let mut hidden = 0usize;
        let mut split = 0usize;
        let mut marked = 0usize;
        let mut intruders_whole = 0usize;
        let mut kept_box = 0usize;
        let mut drawn = 0usize;
        let first = hulls.len();
        for (group, label, members) in &groups {
            let mut boxes: Vec<&str> = Vec::new();
            let mut all_owned_here = !members.is_empty();
            for member in members {
                if !parent_of.contains_key(member.as_str()) {
                    continue;
                }
                // Already inside the group's own box: owned by this group, or — for the parent hierarchy, whose groups hold
                // their descendants — owned by the parent hierarchy through a container at or below the group.
                let here = match ownership.owners.get(member) {
                    Some(Owner::Parent(Some(p))) => {
                        criterion.by == PARENT && {
                            let mut at = Some(p.as_str());
                            let mut walked = 0usize;
                            let mut found = false;
                            while let Some(c) = at {
                                if c == group.as_str() {
                                    found = true;
                                    break;
                                }
                                // The parent hierarchy of the source is a tree; the bound only guards against a broken one.
                                walked += 1;
                                if walked > parent_of.len() {
                                    break;
                                }
                                at = parent_of.get(c).copied().flatten();
                            }
                            found
                        }
                    }
                    Some(Owner::Group {
                        dimension,
                        group: g,
                    }) => *dimension == criterion.by && g == group,
                    _ => false,
                };
                all_owned_here &= here;
                match box_for(member) {
                    Some(id) if !boxes.contains(&id) => boxes.push(id),
                    Some(_) => {}
                    None => hidden += 1,
                }
            }
            // The owning criterion draws boxes, not hulls; a soft group whose members all sit in its own box needs none.
            if owning || all_owned_here || boxes.is_empty() {
                continue;
            }
            let shown = if criterion.by == PARENT {
                label_of
                    .get(group.as_str())
                    .map(|l| l.to_string())
                    .unwrap_or_else(|| label.clone())
            } else {
                label.clone()
            };
            // A foreign box is a leaf of the view that is not one of the group's boxes. A container that encloses members
            // is not one: it is the drawing's own structure, not a node the hull swallowed. Nor is a leaf inside one of the
            // group's boxes: a member may itself be a container (an interface and its properties), and its box holds its
            // children wherever its hull is drawn — no split can take them out.
            // The group's own box, if the view still draws it: a source container that kept some of its members (parent),
            // or the container of a group that owns some nodes (a dimension). Its box already shows the members inside it.
            let own_box: Option<&str> = if criterion.by == PARENT {
                visible(group)
            } else {
                visible(&grouping::container_id(&criterion.by, group))
            };
            let within_member = |id: &str| -> bool {
                let mut at = view_parent.get(id).copied().flatten();
                let mut walked = 0usize;
                while let Some(p) = at {
                    if boxes.contains(&p) || own_box == Some(p) {
                        return true;
                    }
                    walked += 1;
                    if walked > view_parent.len() {
                        return false; // the view's tree is acyclic; the bound only guards a broken one
                    }
                    at = view_parent.get(p).copied().flatten();
                }
                false
            };
            let foreign: Vec<(&str, Point)> = positions
                .iter()
                .filter(|(id, _)| {
                    !containers.contains(**id) && !boxes.contains(*id) && !within_member(id)
                })
                .map(|(id, rect)| (*id, crate::hull::centre(rect)))
                .collect();
            let inside = |points: &[Point]| -> Vec<String> {
                foreign
                    .iter()
                    .filter(|(_, c)| crate::hull::contains(points, c))
                    .map(|(id, _)| id.to_string())
                    .collect()
            };
            let rects: Vec<Rect> = boxes.iter().map(|id| positions[*id].clone()).collect();
            // What the first slice drew: the whole group, and what it would enclose.
            intruders_whole += inside(&crate::hull::group_shape(&rects, HULL_PADDING, shape)).len();
            // A group that is still a box draws only the members that left it: the box shows the others. A member box is
            // inside when it is the own box or lies below it in the view.
            let in_own_box = |id: &str| -> bool {
                let Some(own) = own_box else {
                    return false;
                };
                let mut at = Some(id);
                let mut walked = 0usize;
                while let Some(c) = at {
                    if c == own {
                        return true;
                    }
                    walked += 1;
                    if walked > view_parent.len() {
                        return false;
                    }
                    at = view_parent.get(c).copied().flatten();
                }
                false
            };
            let (boxes, rects): (Vec<&str>, Vec<Rect>) = if own_box.is_some() {
                boxes
                    .iter()
                    .zip(&rects)
                    .filter(|(id, _)| !in_own_box(id))
                    .map(|(id, r)| (*id, r.clone()))
                    .unzip()
            } else {
                (boxes.clone(), rects.clone())
            };
            if boxes.is_empty() {
                continue;
            }
            let centres: Vec<Point> = foreign.iter().map(|(_, c)| c.clone()).collect();
            drawn += 1;
            // Joined to the box: one hull over the box and the members that left, where that encloses nothing foreign and
            // stays compact. Members far away would blow the hull up until it says nothing, and a hull around them alone
            // does not show the box they left: they are marked instead, and the mark names the box.
            if let Some(own) = own_box {
                kept_box += 1;
                let mut joined = rects.clone();
                joined.push(positions[own].clone());
                let points = crate::hull::group_shape(&joined, HULL_PADDING, shape);
                let padded =
                    |r: &Rect| (r.width + 2. * HULL_PADDING) * (r.height + 2. * HULL_PADDING);
                let left: f64 = rects.iter().map(padded).sum();
                let corners = crate::hull::group_shape(&joined, HULL_PADDING, HullShape::Rectangle);
                let side = |of: fn(&Point) -> f64| {
                    let all = corners.iter().map(of);
                    all.clone().fold(f64::MIN, f64::max) - all.fold(f64::MAX, f64::min)
                };
                let growth = side(|p| p.x) * side(|p| p.y) - padded(&positions[own]);
                if points.len() >= 3
                    && growth <= JOINED_GROWTH_LIMIT * left
                    && inside(&points).is_empty()
                {
                    let mut listed: Vec<String> = boxes.iter().map(|id| id.to_string()).collect();
                    listed.push(own.to_string());
                    hulls.push(SceneHull {
                        dimension: criterion.by.clone(),
                        dimension_label: dimension_label.clone(),
                        group: group.clone(),
                        label: shown.clone(),
                        points,
                        boxes: listed,
                        intruders: Vec::new(),
                        piece: 0,
                        pieces: 1,
                    });
                } else {
                    marked += 1;
                    for id in &boxes {
                        marks.push(SceneMark {
                            dimension: criterion.by.clone(),
                            dimension_label: dimension_label.clone(),
                            group: group.clone(),
                            label: shown.clone(),
                            node: id.to_string(),
                            own_box: Some(own.to_string()),
                        });
                    }
                }
                continue;
            }
            // The pieces (M2b): whole if clean, else per owner, else by distance within an owner.
            let members: Vec<crate::hull::Member> = boxes
                .iter()
                .zip(&rects)
                .map(|(id, rect)| crate::hull::Member {
                    rect: rect.clone(),
                    owner: view_parent.get(id).copied().flatten(),
                })
                .collect();
            let pieces = crate::hull::pieces(&members, &centres, HULL_PADDING, shape);
            if pieces.len() > MARKS_ABOVE_PIECES {
                // Too scattered for hulls: every member is marked instead, and no polygon claims a shape.
                marked += 1;
                for id in &boxes {
                    marks.push(SceneMark {
                        dimension: criterion.by.clone(),
                        dimension_label: dimension_label.clone(),
                        group: group.clone(),
                        label: shown.clone(),
                        node: id.to_string(),
                        own_box: None,
                    });
                }
                continue;
            }
            if pieces.len() > 1 {
                split += 1;
            }
            let count = pieces.len();
            for (index, piece) in pieces.into_iter().enumerate() {
                let piece_rects: Vec<Rect> = piece.iter().map(|&i| rects[i].clone()).collect();
                let points = crate::hull::group_shape(&piece_rects, HULL_PADDING, shape);
                if points.len() < 3 {
                    continue;
                }
                let intruders = inside(&points);
                hulls.push(SceneHull {
                    dimension: criterion.by.clone(),
                    dimension_label: dimension_label.clone(),
                    group: group.clone(),
                    label: shown.clone(),
                    points,
                    boxes: piece.iter().map(|&i| boxes[i].to_string()).collect(),
                    intruders,
                    piece: index,
                    pieces: count,
                });
            }
        }
        let intruders: usize = hulls[first..].iter().map(|h| h.intruders.len()).sum();
        // Pairs of hulls of two different groups of this criterion that overlap; the pieces of one group are not a pair.
        let mut overlapping = 0usize;
        for i in first..hulls.len() {
            for j in (i + 1)..hulls.len() {
                if hulls[i].group != hulls[j].group
                    && crate::hull::overlap(&hulls[i].points, &hulls[j].points)
                {
                    overlapping += 1;
                }
            }
        }
        let unassigned = if owning {
            ownership.unassigned.len()
        } else {
            0
        };
        let mut summary = if owning {
            format!(
                "{}: owns {owned} nodes in {} groups",
                criterion.by,
                groups.len()
            )
        } else {
            format!(
                "{}: holds softly, {drawn} of {} groups drawn ({split} split, {marked} marked, {kept_box} drawn by the members that left their box), {intruders} foreign boxes \
                 inside a hull ({intruders_whole} if drawn whole), {overlapping} overlapping pairs",
                criterion.by,
                groups.len()
            )
        };
        if multiple > 0 {
            summary.push_str(&format!(
                ", {multiple} in several groups (value order chose)"
            ));
        }
        if unassigned > 0 {
            summary.push_str(&format!(", {unassigned} nodes no criterion assigns"));
        }
        let held = snapshot
            .held_groups
            .iter()
            .filter(|h| h.dimension == criterion.by)
            .count();
        if held > 0 {
            summary.push_str(&format!(", {held} held together by the layout"));
        }
        // Why the others are not, by reason (`Snapshot::holding` names the groups).
        let mut reasons: BTreeMap<ReleaseReason, usize> = BTreeMap::new();
        for r in snapshot.holding.iter().flat_map(|h| &h.released) {
            if r.dimension == criterion.by {
                *reasons.entry(r.reason).or_default() += 1;
            }
        }
        if !reasons.is_empty() {
            let list: Vec<String> = reasons
                .iter()
                .map(|(r, n)| format!("{n} {}", r.text()))
                .collect();
            summary.push_str(&format!(", not held: {}", list.join(", ")));
        }
        if let Some(breaks) = lane_breaks {
            summary.push_str(&format!(" as lanes, {breaks} out of place"));
        } else if lanes {
            summary.push_str(", lanes not applied yet (ADR 0022, M4)");
        }
        reports.push(SceneMembershipReport {
            dimension: criterion.by.clone(),
            mode: if owning { "owner" } else { "soft" }.to_string(),
            applied: !lanes || lane_breaks == Some(0),
            groups: groups.len(),
            drawn,
            intruders,
            overlapping_pairs: overlapping,
            unknown_members: unknown,
            hidden_members: hidden,
            owned,
            multiple_owners: multiple,
            unassigned,
            split,
            marked,
            intruders_whole,
            kept_box,
            held,
            summary,
        });
    }
    (hulls, reports, marks)
}

/// Absolute rectangle of a node, computed once per node. Cycles and invalid rectangles are errors.
/// The stubs of `Snapshot::hub_refs` at their nodes' boxes: per node, the references to hubs out of its right side and
/// those from hubs into its left side, spread evenly along the side in the order of the references.
fn hub_refs_of(snapshot: &Snapshot, positions: &BTreeMap<&str, Rect>) -> Vec<SceneHubRef> {
    const STUB: f64 = 18.;
    let labels: BTreeMap<&str, &str> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.label.as_str()))
        .collect();
    let mut count: BTreeMap<(&str, bool), usize> = BTreeMap::new();
    for r in &snapshot.hub_refs {
        *count.entry((r.node.as_str(), r.outgoing)).or_default() += 1;
    }
    let mut seen: BTreeMap<(&str, bool), usize> = BTreeMap::new();
    let mut out = Vec::new();
    for r in &snapshot.hub_refs {
        let Some(b) = positions.get(r.node.as_str()) else {
            continue;
        };
        let key = (r.node.as_str(), r.outgoing);
        let k = *seen.entry(key).and_modify(|k| *k += 1).or_insert(0);
        let n = count[&key];
        let y = b.y + b.height * (k + 1) as f64 / (n + 1) as f64;
        let text = labels
            .get(r.hub.as_str())
            .copied()
            .unwrap_or(r.hub.as_str())
            .to_string();
        let width = text.chars().count().min(32) as f64 * 6.5 + 8.;
        let (points, label) = if r.outgoing {
            let (x0, x1) = (b.x + b.width, b.x + b.width + STUB);
            (
                vec![Point { x: x0, y }, Point { x: x1, y }],
                Rect {
                    x: x1 + 3.,
                    y: y - 7.,
                    width,
                    height: 14.,
                },
            )
        } else {
            let (x0, x1) = (b.x - STUB, b.x);
            (
                vec![Point { x: x0, y }, Point { x: x1, y }],
                Rect {
                    x: x0 - 3. - width,
                    y: y - 7.,
                    width,
                    height: 14.,
                },
            )
        };
        out.push(SceneHubRef {
            edge: r.edge.clone(),
            node: r.node.clone(),
            hub: r.hub.clone(),
            outgoing: r.outgoing,
            text,
            points,
            label,
        });
    }
    out
}

fn absolute<'a>(
    id: &'a str,
    by_id: &BTreeMap<&'a str, &'a Node>,
    snapshot: &'a Snapshot,
    positions: &mut BTreeMap<&'a str, Rect>,
    visiting: &mut BTreeSet<&'a str>,
) -> Result<Rect, String> {
    if let Some(known) = positions.get(id) {
        return Ok(known.clone());
    }
    if !visiting.insert(id) {
        return Err(format!("Cyclic scene containment: {id}"));
    }
    let node = by_id
        .get(id)
        .ok_or_else(|| format!("Missing scene geometry: {id}"))?;
    let mut rect = snapshot
        .boxes
        .get(id)
        .ok_or_else(|| format!("Missing scene geometry: {id}"))?
        .clone();
    if let Some(parent) = node.parent_id.as_deref() {
        let p = absolute(parent, by_id, snapshot, positions, visiting)?;
        rect.x += p.x;
        rect.y += p.y;
    }
    if ![rect.x, rect.y, rect.width, rect.height]
        .iter()
        .all(|v| v.is_finite())
        || rect.width <= 0.
        || rect.height <= 0.
    {
        return Err(format!("Invalid rectangle: {id}"));
    }
    visiting.remove(id);
    positions.insert(id, rect.clone());
    Ok(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(overrides: serde_json::Value) -> Snapshot {
        let mut value = json!({
            "schemaVersion": 1,
            "engineVersion": arqlet_model::ENGINE_VERSION,
            "revision": 3,
            "layoutVersion": 5,
            "graph": {
                "nodes": [
                    {"id": "group", "kind": "container", "label": "Group", "parentId": null},
                    {"id": "child", "kind": "symbol", "label": "Child", "parentId": "group"},
                    {"id": "other", "kind": "symbol", "label": "Other", "parentId": null}
                ],
                "edges": [{"id": "e", "kind": "calls", "source": "child", "target": "other"}]
            },
            "boxes": {
                "group": {"x": 100, "y": 100, "width": 300, "height": 200},
                "child": {"x": 20, "y": 50, "width": 100, "height": 50},
                "other": {"x": 450, "y": 150, "width": 160, "height": 70}
            },
            "routes": [{"id": "e", "points": [{"x": 220, "y": 175}, {"x": 450, "y": 175}]}],
            "recomputedContainers": [],
            "omittedUnresolvedEdges": 0
        });
        for (key, extra) in overrides.as_object().unwrap() {
            value[key] = extra.clone();
        }
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn nested_geometry_is_flattened_once_and_bounds_include_labels() {
        let mut s = snapshot(json!({"labels": [{
            "id": "e", "kind": "edge", "text": "calls", "bounds": {"x": -40, "y": -60, "width": 80, "height": 22}
        }]}));
        s.options.appearance.show_ports = false;
        let scene = build_scene(&s).unwrap();
        let child = scene.nodes.iter().find(|n| n.node.id == "child").unwrap();
        assert_eq!((child.bounds.x, child.bounds.y), (120., 150.));
        assert!(
            scene
                .nodes
                .iter()
                .find(|n| n.node.id == "group")
                .unwrap()
                .container
        );
        assert!(!scene.show_ports);
        assert_eq!(scene.routes.len(), 1);
        assert_eq!(scene.routes[0].source, "child");
        assert_eq!(scene.routes[0].target, "other");
        // Label reaches x = -40; bounds add the 30-unit margin on each side.
        assert_eq!(scene.bounds.x, -70.);
        assert_eq!(scene.bounds.y, -90.);
        assert_eq!(scene.revision, 3);
        assert_eq!(scene.layout_version, 5);
    }

    #[test]
    fn a_rounded_route_carries_its_arcs_and_the_style_stays() {
        let bend = json!([{"x": 220, "y": 175}, {"x": 335, "y": 175}, {"x": 335, "y": 185}, {"x": 450, "y": 185}]);
        let mut s = snapshot(json!({"routes": [{"id": "e", "points": bend}]}));
        s.graph.default_routing = Some(RoutingConstraint::Fixed(RoutingStyle::RoundedOrthogonal));
        let scene = build_scene(&s).unwrap();
        let route = &scene.routes[0];
        assert!(route.rounded);
        // Two corners of a 10-unit step: each is rounded with a radius limited by the step, so the route has the arc points.
        assert!(route.points.len() > 4, "{:?}", route.points);
        assert_eq!(route.points.first().map(|p| p.x), Some(220.));
        assert_eq!(route.points.last().map(|p| p.x), Some(450.));
        // The style is not given up where there is room: the same route with the orthogonal style keeps its corners.
        s.graph.default_routing = Some(RoutingConstraint::Fixed(RoutingStyle::Orthogonal));
        let plain = build_scene(&s).unwrap();
        assert!(!plain.routes[0].rounded);
        assert_eq!(plain.routes[0].points.len(), 4);
    }

    #[test]
    fn an_octilinear_route_cuts_its_corners_at_45_degrees_and_the_metrics_keep_the_corners() {
        let bend = json!([{"x": 220, "y": 175}, {"x": 335, "y": 175}, {"x": 335, "y": 185}, {"x": 450, "y": 185}]);
        let mut s = snapshot(json!({"routes": [{"id": "e", "points": bend}]}));
        s.graph.default_routing = Some(RoutingConstraint::Fixed(RoutingStyle::Octilinear));
        let route = &build_scene(&s).unwrap().routes[0];
        // Two corners of a 10-unit step, each cut by half the step (the cuts meet in its middle): the diagonals at 45°.
        let p: Vec<(f64, f64)> = route.points.iter().map(|p| (p.x, p.y)).collect();
        assert_eq!(
            p,
            vec![
                (220., 175.),
                (330., 175.),
                (335., 180.),
                (340., 185.),
                (450., 185.)
            ]
        );
        assert!(!route.rounded);
        // The corner scene, which the metrics measure, keeps the orthogonal corners.
        assert_eq!(build_corner_scene(&s).unwrap().routes[0].points.len(), 4);
    }

    #[test]
    fn a_curve_route_is_the_pulled_route_smoothed_and_the_metrics_keep_its_corners() {
        let bend = json!([{"x": 220, "y": 175}, {"x": 335, "y": 175}, {"x": 335, "y": 185}, {"x": 450, "y": 185}]);
        let mut s = snapshot(json!({"routes": [{"id": "e", "points": bend}]}));
        s.graph.default_routing = Some(RoutingConstraint::Fixed(RoutingStyle::Polyline));
        let polyline = build_scene(&s).unwrap().routes[0].points.clone();
        s.graph.default_routing = Some(RoutingConstraint::Fixed(RoutingStyle::Curve));
        let curve = build_scene(&s).unwrap().routes[0].points.clone();
        // The curve passes through the polyline's points and keeps its ends.
        assert_eq!(curve.first(), polyline.first());
        assert_eq!(curve.last(), polyline.last());
        for q in &polyline {
            assert!(
                curve.iter().any(|c| (c.x - q.x).hypot(c.y - q.y) < 1e-9),
                "{q:?}"
            );
        }
        assert!(curve.len() >= polyline.len());
        // The corner scene, which the metrics measure, has the polyline's corners.
        assert_eq!(build_corner_scene(&s).unwrap().routes[0].points, polyline);
    }

    #[test]
    fn bridges_are_drawn_only_when_asked_and_the_metrics_keep_the_plain_routes() {
        let mut s = snapshot(json!({
            "graph": {
                "nodes": [
                    {"id": "group", "kind": "container", "label": "Group", "parentId": null},
                    {"id": "child", "kind": "symbol", "label": "Child", "parentId": "group"},
                    {"id": "other", "kind": "symbol", "label": "Other", "parentId": null}
                ],
                "edges": [
                    {"id": "e", "kind": "calls", "source": "child", "target": "other"},
                    {"id": "f", "kind": "calls", "source": "other", "target": "child"}
                ]
            },
            "routes": [
                {"id": "e", "points": [{"x": 220, "y": 175}, {"x": 450, "y": 175}]},
                {"id": "f", "points": [{"x": 335, "y": 100}, {"x": 335, "y": 260}]}
            ]
        }));
        let plain = build_scene(&s).unwrap();
        let count = |sc: &Scene| sc.routes.iter().map(|r| r.points.len()).sum::<usize>();
        s.options.appearance.bridges = true;
        let bridged = build_scene(&s).unwrap();
        assert!(
            count(&bridged) > count(&plain),
            "a bridge adds the half circle's points"
        );
        assert_eq!(
            count(&build_corner_scene(&s).unwrap()),
            count(&plain),
            "the metrics see no bridge"
        );
        // Off, the option is not written.
        s.options.appearance.bridges = false;
        assert!(!serde_json::to_string(&s.options)
            .unwrap()
            .contains("bridges"));
    }

    #[test]
    fn the_scene_names_every_party_of_each_conflict() {
        let clean = build_scene(&snapshot(json!({}))).unwrap();
        assert!(clean.conflicts.is_empty());
        assert!(
            serde_json::to_value(&clean)
                .unwrap()
                .get("conflicts")
                .is_none(),
            "a clean drawing carries no conflicts field"
        );
        let s = snapshot(json!({
            "geometryIssues": [{"kind": "node_overlap", "nodeIds": ["child", "other"], "message": "Unrelated node rectangles overlap"}],
            "routeConflicts": [{"edgeId": "e", "nodeId": "other"}]
        }));
        let scene = build_scene(&s).unwrap();
        assert_eq!(scene.conflicts.len(), 2);
        let overlap = &scene.conflicts[0];
        assert_eq!(overlap.kind, "node_overlap");
        assert_eq!(overlap.nodes, ["child", "other"]);
        assert!(overlap.routes.is_empty());
        let through = &scene.conflicts[1];
        assert_eq!(through.kind, "route-through-node");
        assert_eq!(
            (through.routes.as_slice(), through.nodes.as_slice()),
            (&["e".to_string()][..], &["other".to_string()][..])
        );
        assert!(through.reason.contains("passes through"));
    }

    #[test]
    fn bundle_members_are_omitted_and_conflicts_are_flagged() {
        let s = snapshot(json!({
            "routes": [
                {"id": "e", "points": [{"x": 220, "y": 175}, {"x": 450, "y": 175}], "weight": 2},
                {"id": "f", "points": [{"x": 220, "y": 175}, {"x": 450, "y": 175}], "bundle_of": "e"}
            ],
            "routeConflicts": [{"edgeId": "e", "nodeId": "other"}]
        }));
        let scene = build_scene(&s).unwrap();
        assert_eq!(scene.routes.len(), 1);
        assert_eq!(scene.routes[0].weight, 2);
        assert!(scene.routes[0].invalid);
    }

    #[test]
    fn containment_cycles_and_invalid_rectangles_fail_explicitly() {
        let mut cyclic = snapshot(json!({}));
        cyclic.graph.nodes[0].parent_id = Some("child".into());
        assert!(build_scene(&cyclic)
            .unwrap_err()
            .starts_with("Cyclic scene containment"));

        let flat = snapshot(json!({"boxes": {
            "group": {"x": 100, "y": 100, "width": 0, "height": 200},
            "child": {"x": 20, "y": 50, "width": 100, "height": 50},
            "other": {"x": 450, "y": 150, "width": 160, "height": 70}
        }}));
        assert!(build_scene(&flat)
            .unwrap_err()
            .starts_with("Invalid rectangle: group"));

        let mut missing_target = snapshot(json!({}));
        missing_target.graph.edges[0].target = Some("nowhere".into());
        assert!(build_scene(&missing_target)
            .unwrap_err()
            .starts_with("Missing route endpoints"));

        let schema = snapshot(json!({"schemaVersion": 2}));
        assert!(build_scene(&schema).is_err());
    }
}

/// The rows of a node, stacked from the top inside its padding (ADR 0011), as the model places them.
fn node_rows(snapshot: &Snapshot, id: &str, bounds: &Rect) -> Vec<SceneRow> {
    let Some(rows) = snapshot.options.node_rows.get(id) else {
        return Vec::new();
    };
    arqlet_model::row_bounds(bounds, rows)
        .into_iter()
        .zip(rows)
        .map(|((_, bounds), row)| SceneRow {
            id: row.id.clone(),
            label: row.label.clone(),
            bounds,
        })
        .collect()
}

/// The conflicts of a snapshot as the scene reports them: the geometry issues, then the routes through nodes. Order follows the
/// snapshot, so the same snapshot gives the same list.
fn conflicts_of(snapshot: &Snapshot) -> Vec<SceneConflict> {
    let issues = snapshot.geometry_issues.iter().map(|issue| SceneConflict {
        kind: issue.kind.clone(),
        nodes: issue.node_ids.clone(),
        routes: Vec::new(),
        reason: issue.message.clone(),
    });
    let routes = snapshot.route_conflicts.iter().map(|c| SceneConflict {
        kind: "route-through-node".into(),
        nodes: vec![c.node_id.clone()],
        routes: vec![c.edge_id.clone()],
        reason: format!("Edge {} passes through node {}", c.edge_id, c.node_id),
    });
    issues.chain(routes).collect()
}
