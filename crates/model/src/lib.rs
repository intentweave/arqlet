//! Portable graph and resumable scene contract for the first prototype.
pub mod math;
pub mod outline;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub parent_id: Option<String>,
    /// The outline of the node. `rect` (the default, omitted) uses its size as it is; `square` and `circle` use the larger
    /// of its two sizes on both axes. Collision and routing still use the bounding rectangle in this version.
    #[serde(default, skip_serializing_if = "NodeShape::is_default")]
    pub shape: NodeShape,
}

/// A row of a leaf node's content (ADR 0011): a piece of the content with its own size, stacked from the top in the order
/// given. The node's size follows from its rows. A row is layout geometry, not a graph node: it takes no part in the
/// arrangement of the diagram.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    /// The text the row shows. The engine does not read it; it is carried to the viewer.
    #[serde(default)]
    pub label: String,
    pub width: f64,
    pub height: f64,
}

fn is_zero(value: &f64) -> bool {
    *value == 0.
}

/// How row ports that share a row and a side are placed (ADR 0011). `peer` orders them by where their peers lie and spreads
/// them over the row; `middle` puts each at the middle of the row. The default is `peer`.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum RowPortOrder {
    #[default]
    Peer,
    Middle,
}

impl RowPortOrder {
    pub fn is_default(&self) -> bool {
        *self == RowPortOrder::Peer
    }
}

/// Inner padding of a node around its rows, and the gap between two rows.
pub const ROW_PADDING: f64 = 10.;
pub const ROW_GAP: f64 = 4.;

/// The outline of a leaf node (docs/...: step 1 of the shape support). Containers use the same names later.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum NodeShape {
    #[default]
    Rect,
    Square,
    Circle,
}

impl NodeShape {
    fn is_default(&self) -> bool {
        *self == Self::Rect
    }

    /// The size of a node of this shape from its requested size: square and circle take the larger side on both axes.
    pub fn fit(self, width: f64, height: f64) -> (f64, f64) {
        match self {
            Self::Rect => (width, height),
            Self::Square | Self::Circle => {
                let side = width.max(height);
                (side, side)
            }
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Edge {
    pub id: String,
    pub kind: String,
    pub source: String,
    pub target: Option<String>,
    /// Provisional: how many edges this edge stands for (a pre-aggregated relation), at least 1; 1 when left out. It
    /// counts where edges are counted: a bundled or aggregated route weighs the sum of its edges, and the overview and
    /// the communities pull by it. Omitted from output when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "number"))]
    pub weight: Option<u32>,
}

impl Edge {
    /// The number of edges this edge stands for (`weight`, 1 when left out).
    pub fn count(&self) -> usize {
        self.weight.map_or(1, |w| w.max(1) as usize)
    }
}
/// A graph as it is read: the legacy strategy values in the direction slots are translated here, so that earlier graphs keep
/// their meaning. Anything else is parsed as it is.
#[derive(Deserialize)]
struct GraphWire {
    #[serde(default, rename = "defaultDirection")]
    default_direction: Option<serde_json::Value>,
    #[serde(default, rename = "containerDirections")]
    container_directions: BTreeMap<String, serde_json::Value>,
    #[serde(default, rename = "defaultStrategy")]
    default_strategy: Option<serde_json::Value>,
    #[serde(default, rename = "containerStrategies")]
    container_strategies: BTreeMap<String, serde_json::Value>,
    #[serde(default, rename = "defaultRouting")]
    default_routing: Option<serde_json::Value>,
    #[serde(default, rename = "containerRouting")]
    container_routing: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    ports: Vec<Port>,
    #[serde(default, rename = "edgePorts")]
    edge_ports: Vec<EdgePorts>,
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    #[serde(default)]
    buses: Vec<Bus>,
}

/// Reads one direction slot: a direction, or a legacy strategy name, which gives a free direction and that strategy.
fn read_slot(
    value: &serde_json::Value,
) -> Result<(DirectionConstraint, Option<LayoutStrategy>), String> {
    // `hybrid` was the compact choice per area, which a free direction is now.
    if value.as_str() == Some("hybrid") {
        return Ok((DirectionConstraint::Free, None));
    }
    match legacy_strategy(value) {
        Some(strategy) => Ok((DirectionConstraint::Free, Some(strategy))),
        None => Ok((parse_direction(value)?, None)),
    }
}

impl TryFrom<GraphWire> for Graph {
    type Error = String;
    fn try_from(wire: GraphWire) -> Result<Self, Self::Error> {
        let (default_direction, legacy_root) = match &wire.default_direction {
            Some(v) => read_slot(v)?,
            None => (DirectionConstraint::default(), None),
        };
        let default_strategy = match (legacy_root, &wire.default_strategy) {
            (Some(_), Some(_)) => {
                return Err(
                    "the root has a legacy strategy in its direction and a defaultStrategy".into(),
                )
            }
            (Some(s), None) => StrategyConstraint::Fixed(s),
            (None, Some(v)) => parse_strategy(v)?,
            (None, None) => StrategyConstraint::default(),
        };
        let mut container_directions = BTreeMap::new();
        let mut container_strategies = BTreeMap::new();
        for (id, value) in &wire.container_directions {
            let (direction, legacy) = read_slot(value)?;
            container_directions.insert(id.clone(), direction);
            if let Some(strategy) = legacy {
                container_strategies.insert(id.clone(), StrategyConstraint::Fixed(strategy));
            }
        }
        // Earlier graphs had one slot per container: a direction there replaced the inherited strategy. A document without any
        // strategy field keeps that meaning: a container with a direction of its own and no strategy is plain.
        let earlier_document =
            wire.default_strategy.is_none() && wire.container_strategies.is_empty();
        if earlier_document {
            for id in container_directions.keys() {
                let legacy_slot = wire
                    .container_directions
                    .get(id)
                    .and_then(|v| v.as_str())
                    .is_some_and(|t| t == "hybrid" || t == "shortest");
                if legacy_slot {
                    continue;
                }
                container_strategies
                    .entry(id.clone())
                    .or_insert(StrategyConstraint::Fixed(LayoutStrategy::None));
            }
        }
        for (id, value) in &wire.container_strategies {
            if container_strategies.contains_key(id) {
                return Err(format!("container {id} has a legacy strategy in its direction and a containerStrategies entry"));
            }
            container_strategies.insert(id.clone(), parse_strategy(value)?);
        }
        let default_routing = match &wire.default_routing {
            Some(v) => Some(parse_routing(v)?),
            None => None,
        };
        let mut container_routing = BTreeMap::new();
        for (id, value) in &wire.container_routing {
            container_routing.insert(id.clone(), parse_routing(value)?);
        }
        Ok(Graph {
            default_direction,
            container_directions,
            default_strategy,
            container_strategies,
            default_routing,
            container_routing,
            ports: wire.ports,
            edge_ports: wire.edge_ports,
            nodes: wire.nodes,
            edges: wire.edges,
            buses: wire.buses,
        })
    }
}

/// A graph: the canonical data of a view. Direction and layout strategy are two constraints, each set for the root and for
/// containers and inherited by containers that declare nothing (docs/adr/0003-container-direction-constraints.md).
#[derive(Clone, Debug)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Graph {
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "\"lr\" | \"rl\" | \"tb\" | \"bt\" | \"auto\" | Array<\"lr\" | \"rl\" | \"tb\" | \"bt\">"
        )
    )]
    // Optional in the contract: a graph as it is read may leave these out (defaults), and the type serves input too.
    #[cfg_attr(feature = "ts", ts(rename = "defaultDirection"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub default_direction: DirectionConstraint,
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ [key in string]: \"lr\" | \"rl\" | \"tb\" | \"bt\" | \"auto\" | Array<\"lr\" | \"rl\" | \"tb\" | \"bt\"> }"
        )
    )]
    #[cfg_attr(feature = "ts", ts(rename = "containerDirections"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub container_directions: BTreeMap<String, DirectionConstraint>,
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "\"none\" | \"tree\" | \"radial\" | \"auto\" | Array<\"none\" | \"tree\" | \"radial\">"
        )
    )]
    #[cfg_attr(feature = "ts", ts(rename = "defaultStrategy"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub default_strategy: StrategyConstraint,
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ [key in string]: \"none\" | \"tree\" | \"radial\" | \"auto\" | Array<\"none\" | \"tree\" | \"radial\"> }"
        )
    )]
    #[cfg_attr(feature = "ts", ts(rename = "containerStrategies"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub container_strategies: BTreeMap<String, StrategyConstraint>,
    /// The edge routing of the root and its containers (docs/adr/0005-routing-constraints.md).
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "\"straight\" | \"orthogonal\" | \"roundedOrthogonal\" | \"polyline\" | \"roundedPolyline\" | \"octilinear\" | \"curve\" | \"bus\" | \"auto\" | Array<\"straight\" | \"orthogonal\" | \"roundedOrthogonal\" | \"polyline\" | \"roundedPolyline\" | \"octilinear\" | \"curve\" | \"bus\">"
        )
    )]
    #[cfg_attr(feature = "ts", ts(rename = "defaultRouting"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub default_routing: Option<RoutingConstraint>,
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ [key in string]: \"straight\" | \"orthogonal\" | \"roundedOrthogonal\" | \"polyline\" | \"roundedPolyline\" | \"octilinear\" | \"curve\" | \"bus\" | \"auto\" | Array<\"straight\" | \"orthogonal\" | \"roundedOrthogonal\" | \"polyline\" | \"roundedPolyline\" | \"octilinear\" | \"curve\" | \"bus\"> }"
        )
    )]
    #[cfg_attr(feature = "ts", ts(rename = "containerRouting"))]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub container_routing: BTreeMap<String, RoutingConstraint>,
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<Port>>", optional))]
    pub ports: Vec<Port>,
    #[cfg_attr(feature = "ts", ts(rename = "edgePorts"))]
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<EdgePorts>>", optional))]
    pub edge_ports: Vec<EdgePorts>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Provisional: explicit buses (ADR 0005): each groups edges drawn on one shared trunk with branches. Membership is never
    /// inferred. Omitted when empty, so earlier documents and drawings do not change.
    #[cfg_attr(feature = "ts", ts(as = "Option<Vec<Bus>>", optional))]
    pub buses: Vec<Bus>,
}

/// The graph as it is written. Each direction slot holds a direction, or the legacy `hybrid` or `shortest` where the graph
/// has that strategy with a free direction. The field order is the one of earlier graphs.
#[derive(Serialize)]
struct GraphOut<'a> {
    #[serde(rename = "defaultDirection")]
    default_direction: serde_json::Value,
    #[serde(rename = "containerDirections")]
    container_directions: BTreeMap<&'a str, serde_json::Value>,
    #[serde(rename = "defaultStrategy", skip_serializing_if = "Option::is_none")]
    default_strategy: Option<serde_json::Value>,
    #[serde(
        rename = "containerStrategies",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    container_strategies: BTreeMap<&'a str, serde_json::Value>,
    #[serde(rename = "defaultRouting", skip_serializing_if = "Option::is_none")]
    default_routing: Option<serde_json::Value>,
    #[serde(
        rename = "containerRouting",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    container_routing: BTreeMap<&'a str, serde_json::Value>,
    ports: &'a Vec<Port>,
    #[serde(rename = "edgePorts")]
    edge_ports: &'a Vec<EdgePorts>,
    nodes: &'a Vec<Node>,
    edges: &'a Vec<Edge>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    buses: &'a Vec<Bus>,
}

/// One direction slot and the strategy it leaves over: the legacy name where it applies, else the two values apart.
///
/// A strategy equal to the default (`none`) is left out, except when `keep_default` says the document sets strategies
/// somewhere: a container that is plain inside an organic root must keep its explicit `none`, or reading the document again
/// gives it the root's strategy.
fn written_slot(
    direction: &DirectionConstraint,
    strategy: Option<&StrategyConstraint>,
    keep_default: bool,
) -> (serde_json::Value, Option<serde_json::Value>) {
    let legacy = match (direction, strategy) {
        (
            DirectionConstraint::Free,
            Some(StrategyConstraint::Fixed(s @ LayoutStrategy::Shortest)),
        ) => Some(*s),
        _ => None,
    };
    match legacy {
        Some(s) => (serde_json::to_value(s).unwrap_or_default(), None),
        None => (
            serde_json::to_value(direction).unwrap_or_default(),
            strategy
                .filter(|st| keep_default || **st != StrategyConstraint::default())
                .map(|st| serde_json::to_value(st).unwrap_or_default()),
        ),
    }
}

impl Serialize for Graph {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let (default_direction, rest) =
            written_slot(&self.default_direction, Some(&self.default_strategy), false);
        // Whether the document sets a strategy anywhere. Then every container's entry is written, `none` included.
        let sets_strategy = self.default_strategy != StrategyConstraint::default()
            || self
                .container_strategies
                .values()
                .any(|s| *s != StrategyConstraint::default());
        let mut container_directions = BTreeMap::new();
        let mut container_strategies = BTreeMap::new();
        for (id, direction) in &self.container_directions {
            let (value, rest) =
                written_slot(direction, self.container_strategies.get(id), sets_strategy);
            container_directions.insert(id.as_str(), value);
            if let Some(rest) = rest {
                container_strategies.insert(id.as_str(), rest);
            }
        }
        for (id, strategy) in &self.container_strategies {
            if !self.container_directions.contains_key(id) {
                container_strategies.insert(
                    id.as_str(),
                    serde_json::to_value(strategy).unwrap_or_default(),
                );
            }
        }
        GraphOut {
            default_direction,
            container_directions,
            default_strategy: rest,
            container_strategies,
            default_routing: self
                .default_routing
                .as_ref()
                .map(|r| serde_json::to_value(r).unwrap_or_default()),
            container_routing: self
                .container_routing
                .iter()
                .map(|(id, r)| (id.as_str(), serde_json::to_value(r).unwrap_or_default()))
                .collect(),
            ports: &self.ports,
            edge_ports: &self.edge_ports,
            nodes: &self.nodes,
            edges: &self.edges,
            buses: &self.buses,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Graph {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = GraphWire::deserialize(deserializer)?;
        Graph::try_from(wire).map_err(serde::de::Error::custom)
    }
}

/// The phase of a move gesture: begin, a position update, finish (keep the pin, or release it), or cancel.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MovePhase {
    Begin,
    Update,
    Finish,
    Cancel,
}

/// One step of a move gesture. Positions are in the coordinate space of the node's parent (the world origin for a root
/// node). `seq` must be larger than the last step applied in the gesture; the position of a moved node is binding for the
/// layout, which places it at the pin.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MoveStep {
    pub gesture_id: String,
    pub seq: u64,
    pub phase: MovePhase,
    pub node: String,
    #[serde(default)]
    pub position: Option<Point>,
    /// On finish: release the pin (the layout may move the node again) instead of keeping it fixed.
    #[serde(default)]
    pub release: bool,
    /// The other nodes the gesture moves with `node` (a selection moved as a whole), with their positions in their own
    /// parents' frames. The set is fixed by the first step; an update, a finish and a cancel carry the same names.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub others: BTreeMap<String, Point>,
}

/// A move gesture on one leaf node. `seq` is the last step applied; a step must carry a larger one. `previous` is the pin the
/// node had before the gesture, which a cancel restores.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Gesture {
    pub id: String,
    pub seq: u64,
    pub node: String,
    pub active: bool,
    pub previous: Option<Point>,
    /// The pins the other moved nodes had before the gesture; a cancel restores them. Omitted for a single node.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub previous_others: BTreeMap<String, Option<Point>>,
    /// Set only on the copy of the snapshot that the last step of a gesture (the finish or the cancel) continues from, so
    /// that the layout repairs that step moderately instead of locally. Never present in a published snapshot.
    #[serde(default, skip_serializing_if = "is_false")]
    pub ending: bool,
    /// Provisional: the position the node had when a gesture on a single-pass drawing began; its steps move only the node,
    /// and a cancel puts it back here. Omitted otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Point>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Rect {
    // Coordinates relative to the parent container.
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Point {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Route {
    pub id: String,
    pub points: Vec<Point>,
    /// Number of edges drawn along this route. One unless the route is a bundle leader.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub weight: usize,
    /// Leader edge id when this edge is a bundled member; members are not drawn separately.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_of: Option<String>,
}
impl Snapshot {
    /// Provisional: the weight of every route when an edge of the view has a `weight` (else `None`, and the routes keep
    /// their own): for a route that stands for collapsed content the edges it stands for in its own direction
    /// (`aggregate_weights`, at least 1), else its edge's `weight` plus those of the routes bundled into it. Saturating:
    /// `usize` is 32-bit in WASM.
    pub fn weighted_routes(&self) -> Option<BTreeMap<String, usize>> {
        if !self.graph.edges.iter().any(|e| e.weight.is_some()) {
            return None;
        }
        let count: BTreeMap<&str, usize> = self
            .graph
            .edges
            .iter()
            .map(|e| (e.id.as_str(), e.count()))
            .collect();
        let of = |id: &str| count.get(id).copied().unwrap_or(1);
        let mut weights: BTreeMap<String, usize> = BTreeMap::new();
        for route in &self.routes {
            if route.bundle_of.is_none() {
                let own = match self.options.aggregate_weights.get(&route.id) {
                    Some(w) => w.forward.max(1),
                    None => of(&route.id),
                };
                weights.insert(route.id.clone(), own);
            }
        }
        for route in &self.routes {
            if let Some(leader) = &route.bundle_of {
                if let Some(w) = weights.get_mut(leader) {
                    *w = w.saturating_add(of(&route.id));
                }
            }
        }
        Some(weights)
    }
}

impl Route {
    pub fn new(id: impl Into<String>, points: Vec<Point>) -> Self {
        Self {
            id: id.into(),
            points,
            weight: 1,
            bundle_of: None,
        }
    }
}
fn one() -> usize {
    1
}
fn is_one(value: &usize) -> bool {
    *value == 1
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    #[serde(default)]
    pub labels: Vec<PlacedLabel>,
    #[serde(default)]
    pub routing_issues: Vec<String>,
    /// Why the selected layout pipeline fell back from the requested default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layout_diagnostics: Vec<String>,
    /// The corridors the layout widened for the routes that run through them, one entry per pair of neighbouring siblings.
    /// A later edit that lays out only part of the drawing starts its areas with these widths instead of from nothing.
    /// Omitted when no corridor was widened.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corridor_gaps: Vec<CorridorGap>,
    /// What an edit that laid out only part of the drawing did. Absent for a complete layout, so default output is unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit_report: Option<EditReport>,
    /// The orientation a free direction (`auto`) chose for each area, keyed by the container's ID (`<root>` for the view). Only areas
    /// with a free direction appear; omitted when there are none, so a layout with declared directions is unchanged.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub area_orientations: BTreeMap<String, Orientation>,
    /// The strategy the structured optimisation chose for each area whose strategy is not fixed (`auto` or an allowed set), keyed like
    /// `area_orientations`. Only a result of the hierarchical composition carries it; a plain layout treats `auto` as `none` and records
    /// nothing, so the entry never outlives the drawing it describes.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub area_strategies: BTreeMap<String, LayoutStrategy>,
    /// Structured counterpart of the routing diagnostics that name a passed-through node.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub route_conflicts: Vec<RouteConflict>,
    pub schema_version: u32,
    pub engine_version: String,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub revision: u64,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub layout_version: u64,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub base_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "LayoutOptions::is_default")]
    pub options: LayoutOptions,
    pub graph: Graph,
    #[serde(default)]
    pub source_graph: Option<Graph>,
    #[serde(default)]
    pub collapsed_ids: Vec<String>,
    pub boxes: BTreeMap<String, Rect>,
    pub routes: Vec<Route>,
    pub recomputed_containers: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimization: Option<OptimizationReport>,
    pub omitted_unresolved_edges: usize,
    #[serde(default)]
    pub geometry_issues: Vec<GeometryIssue>,
    #[serde(default)]
    pub resolved_ports: Vec<ResolvedPort>,
    #[serde(default)]
    pub boundary_crossings: Vec<BoundaryCrossing>,
    /// The edges drawn as straight segments (the others are orthogonal). Empty when every edge is orthogonal.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub straight_edges: Vec<String>,
    /// Provisional: where the layout placed every backbone bus (`Graph::buses`) as a virtual node, by that node's id, relative
    /// to its parent like `boxes`. The bus is drawn on a line through it; a later layout continues from it. Omitted when the
    /// graph has none.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub bus_boxes: BTreeMap<String, Rect>,
    /// The move gesture in progress or the last one finished, for sequence checks. Omitted when there was none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gesture: Option<Gesture>,
    /// Provisional (experimental option `layeredConstruction`): the derived layout graph of every layered area, by area
    /// (`"<root>"` for the top level), kept so that a later pass reuses the plans of the areas it does not place again.
    /// Empty (and omitted) without the option.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layered_plans: BTreeMap<String, StoredAreaPlan>,
}
/// The format of `StoredAreaPlan`; a plan of another format is not reused.
pub const STORED_PLAN_FORMAT: u32 = 1;
/// Provisional: the derived layout graph of one layered area as stored with a snapshot (see `Snapshot::layered_plans`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct StoredAreaPlan {
    /// `STORED_PLAN_FORMAT` of the engine that wrote it.
    pub format: u32,
    /// The flow runs along y.
    pub vertical: bool,
    /// The flow runs against the axis.
    pub reversed: bool,
    /// The ranks in flow order, each in its order across the flow: children of the area, long-edge dummies, border nodes.
    pub ranks: Vec<Vec<String>>,
    /// The links between the area's children in flow direction, each with the dummies it passes in order.
    pub chains: Vec<StoredChain>,
    /// The links the cycle break reversed, as source and target of the logical direction.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reversed_links: Vec<StoredLink>,
    /// The place of every dummy in the area's frame.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub dummies: BTreeMap<String, Rect>,
    /// By edge, the border node where the edge crosses this area's border.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub borders: BTreeMap<String, String>,
    /// Provisional (single pass, ADR 0017): by edge, the side node where an edge leaves or enters this area on a side its
    /// parent's flow uses, when the parent's flow runs across this area's.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sides: BTreeMap<String, String>,
}
/// A link of a stored plan with the dummies it passes (see `StoredAreaPlan::chains`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct StoredChain {
    pub source: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dummies: Vec<String>,
}
/// A link of a stored plan (see `StoredAreaPlan::reversed_links`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct StoredLink {
    pub source: String,
    pub target: String,
}
/// A drawn route that passes through a node it must not cross.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct RouteConflict {
    pub edge_id: String,
    pub node_id: String,
}
pub fn validate(graph: &Graph) -> Result<(), String> {
    let mut nodes = BTreeMap::new();
    for n in &graph.nodes {
        if n.id.is_empty() || nodes.insert(n.id.as_str(), n).is_some() {
            return Err("Empty or duplicate node ID".into());
        }
    }
    for n in &graph.nodes {
        let mut seen = BTreeSet::from([n.id.as_str()]);
        let mut parent = n.parent_id.as_deref();
        while let Some(id) = parent {
            if !seen.insert(id) {
                return Err(format!("Containment cycle: {id}"));
            }
            parent = nodes
                .get(id)
                .ok_or_else(|| format!("Missing parent: {id}"))?
                .parent_id
                .as_deref();
        }
    }
    let mut edges = BTreeSet::new();
    for e in &graph.edges {
        if !edges.insert(&e.id) {
            return Err(format!("Duplicate edge: {}", e.id));
        }
        if e.weight == Some(0) {
            return Err(format!("Edge weight must be at least 1: {}", e.id));
        }
        if !nodes.contains_key(e.source.as_str()) {
            return Err(format!("Missing source: {}", e.id));
        }
        if let Some(t) = &e.target {
            if !nodes.contains_key(t.as_str()) {
                return Err(format!("Missing target: {}", e.id));
            }
        }
    }
    let mut ports = BTreeMap::new();
    for port in &graph.ports {
        if port.id.is_empty() || ports.insert(port.id.as_str(), port).is_some() {
            return Err("Duplicate or empty port ID".into());
        }
        if !nodes.contains_key(port.node_id.as_str())
            || !port.offset.is_finite()
            || !(0.0..=1.0).contains(&port.offset)
        {
            return Err(format!("Invalid port: {}", port.id));
        }
    }
    let mut bindings = BTreeSet::new();
    for binding in &graph.edge_ports {
        if !bindings.insert(&binding.edge_id) {
            return Err("Duplicate edge port binding".into());
        }
        let edge = graph
            .edges
            .iter()
            .find(|e| e.id == binding.edge_id)
            .ok_or("Port binding references unknown edge")?;
        for (id, node) in [
            (binding.source_port.as_ref(), Some(&edge.source)),
            (binding.target_port.as_ref(), edge.target.as_ref()),
        ] {
            if let Some(id) = id {
                let port = ports.get(id.as_str()).ok_or("Unknown endpoint port")?;
                if Some(&port.node_id) != node {
                    return Err("Port belongs to another node".into());
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    pub sibling_overlaps: usize,
    pub projected_crossings: usize,
    #[serde(default)]
    pub routed_crossings: usize,
    #[serde(default)]
    pub edge_node_hits: usize,
    #[serde(default)]
    pub shared_route_pairs: usize,
    pub route_length: f64,
    pub area: f64,
    pub movement: f64,
    /// Sum of endpoint-center distances over all edges. Computed only when an objective needs it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub end_to_end: f64,
    /// Direction changes over all drawn routes. Computed only when an objective needs it.
    #[serde(default, skip_serializing_if = "is_zero_count")]
    pub bends: usize,
    /// Drawn segments that come within the node margin of a foreign node without entering it.
    /// Computed only when an objective needs it.
    #[serde(default, skip_serializing_if = "is_zero_count")]
    pub near_node_segments: usize,
}
fn is_symmetric(value: &PortPlacement) -> bool {
    *value == PortPlacement::Symmetric
}
/// The engine version a snapshot carries. 0.2: single pass is the default (ADR 0016); a snapshot of another version is
/// rejected explicitly rather than read with another meaning.
pub const ENGINE_VERSION: &str = "0.2.0-prototype";

fn is_false(value: &bool) -> bool {
    !*value
}
fn is_true(value: &bool) -> bool {
    *value
}

fn default_true() -> bool {
    true
}

/// The clearance that applies when a request names none (ADR 0005). Provisional: the value follows the track spacing.
pub const DEFAULT_REQUIRED_CLEARANCE: f64 = 6.;

fn default_required_clearance() -> Option<f64> {
    Some(DEFAULT_REQUIRED_CLEARANCE)
}

fn is_default_required_clearance(value: &Option<f64>) -> bool {
    *value == default_required_clearance()
}

fn is_zero_seed(value: &u32) -> bool {
    *value == 0
}
fn is_zero_count(value: &usize) -> bool {
    *value == 0
}
/// Why refinement stopped before it completed. The work budget is deterministic; the time budget and a cancellation
/// depend on the host.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum StopReason {
    WorkBudget,
    TimeBudget,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OptimizationReport {
    pub before: Quality,
    pub after: Quality,
    pub candidates_evaluated: usize,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub interrupted: bool,
    /// A deterministic work budget dropped at least one candidate job or global round.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub work_limited: bool,
    /// Why refinement stopped early. Absent when it completed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    pub strategy: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct GeometryIssue {
    pub kind: String,
    pub node_ids: Vec<String>,
    pub message: String,
}

/// Whether a geometry issue is an authored conflict (ADR 0010): a child that does not fit a container with a fixed axis,
/// or a fixed axis below its children. Such a conflict is reported and kept; it is the authored size that is required,
/// so it does not make a layout ineligible.
pub fn is_authored_conflict(
    issue: &GeometryIssue,
    sizing: &BTreeMap<String, ContainerSizing>,
) -> bool {
    conflict_container(issue)
        .and_then(|id| sizing.get(id))
        .is_some_and(|s| {
            matches!(s.width, Some(AxisSize::Fixed(_)))
                || matches!(s.height, Some(AxisSize::Fixed(_)))
        })
}

/// The container an authored conflict is about: a fixed size below its children, or a child that does not fit it.
fn conflict_container(issue: &GeometryIssue) -> Option<&str> {
    match issue.kind.as_str() {
        "container-fixed-below-content" => issue.node_ids.first().map(String::as_str),
        "container_bounds" => issue.node_ids.get(1).map(String::as_str),
        _ => None,
    }
}

/// Whether the geometry issues make a layout ineligible (ADR 0010). An authored conflict does not, and neither does an
/// issue that involves a node inside a container whose fixed size is in conflict: when the box is too small, its
/// children are pushed out of it, so they overlap their neighbours. Those overlaps are consequences of the authored
/// size. Every other issue makes the layout ineligible. The layout search and the refinement both use this rule.
pub fn is_ineligible_geometry(
    issues: &[GeometryIssue],
    graph: &Graph,
    sizing: &BTreeMap<String, ContainerSizing>,
) -> bool {
    let conflicted: Vec<&str> = issues
        .iter()
        .filter(|issue| is_authored_conflict(issue, sizing))
        .filter_map(conflict_container)
        .collect();
    if conflicted.is_empty() {
        return issues
            .iter()
            .any(|issue| !is_authored_conflict(issue, sizing));
    }
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    // Whether a node is a conflicted container or one of its descendants. The parent map is built once per call.
    let inside_conflict = |node: &str| {
        let mut cursor = Some(node);
        while let Some(id) = cursor {
            if conflicted.contains(&id) {
                return true;
            }
            cursor = parents.get(id).copied().flatten();
        }
        false
    };
    issues.iter().any(|issue| {
        !is_authored_conflict(issue, sizing) && !issue.node_ids.iter().any(|id| inside_conflict(id))
    })
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum PortSide {
    Left,
    Right,
    Top,
    Bottom,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Port {
    pub id: String,
    pub node_id: String,
    pub side: PortSide,
    pub offset: f64,
    /// Binds the port to a row of its node (ADR 0011). The port then sits on `side` (left or right) at the middle of the
    /// row, and `offset` is not used. Without a row the port is on the node's box, as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<String>,
}

/// The rectangles of a node's rows, stacked from the top inside the padding, as wide as the node's inner width. The
/// scene and the port placement both use this, so a row port meets the row the viewer draws.
pub fn row_bounds(node: &Rect, rows: &[Row]) -> Vec<(String, Rect)> {
    let mut y = node.y + ROW_PADDING;
    rows.iter()
        .map(|row| {
            let bounds = Rect {
                x: node.x + ROW_PADDING,
                y,
                width: node.width - 2. * ROW_PADDING,
                height: row.height,
            };
            y += row.height + ROW_GAP;
            (row.id.clone(), bounds)
        })
        .collect()
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct EdgePorts {
    pub edge_id: String,
    pub source_port: Option<String>,
    pub target_port: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ResolvedPort {
    pub id: String,
    pub node_id: String,
    pub edge_id: String,
    pub endpoint: String,
    pub side: PortSide,
    pub position: Point,
    pub explicit: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct BoundaryCrossing {
    pub edge_id: String,
    pub container_id: String,
    pub ordinal: usize,
    pub side: PortSide,
    pub position: Point,
    pub derived: bool,
}

/// A layout orientation: where the ranks run. `Rl` and `Bt` are the mirror images of `Lr` and `Tb`: the same arrangement with
/// the ranks reversed. Edges keep their logical direction in every orientation.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum Orientation {
    #[default]
    Lr,
    Rl,
    Tb,
    Bt,
}

impl Orientation {
    /// Ranks run vertically (top to bottom or bottom to top).
    pub fn vertical(self) -> bool {
        matches!(self, Self::Tb | Self::Bt)
    }

    /// Ranks run against the forward axis: the arrangement is the mirror image of the forward one.
    pub fn reversed(self) -> bool {
        matches!(self, Self::Rl | Self::Bt)
    }
}

/// A layout strategy: a rule about how an area is arranged. `none` is the plain arrangement by direction; `hybrid` and
/// `shortest` choose an orientation per area; `tree` places a tree or forest along the area's direction; `radial` places a
/// tree or forest on rings. Compatible with the values `hybrid` and `shortest` of earlier graphs.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum LayoutStrategy {
    /// The plain arrangement by direction.
    #[default]
    None,
    /// Per area, the orientation with the smallest sum of endpoint distances across its links.
    Shortest,
    /// The children form a tree or forest, placed along the area's direction.
    Tree,
    /// The children form a tree or forest drawn on rings around a centre; the area's direction must be free.
    Radial,
    /// The children are placed by stress majorization on their graph distances, then separated by their real sizes. The
    /// area's direction must be free: a ranked layout cannot be replaced silently by a stress placement.
    Organic,
}

/// The two ways a free direction chooses an orientation per area: by compactness (the default of a free direction), or by the
/// smallest end-to-end path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chooser {
    Compact,
    Shortest,
}

/// What a layout uses for an area: an orientation, a chooser that picks one, a tree along an orientation, or a radial
/// tree around a centre.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Orientation(Orientation),
    Chooser(Chooser),
    Tree(Orientation),
    Radial,
    /// The children placed by stress majorization; no orientation applies.
    Organic,
}

impl Mode {
    /// The orientation of the mode, when it has one.
    pub fn orientation(self) -> Option<Orientation> {
        match self {
            Self::Orientation(o) | Self::Tree(o) => Some(o),
            Self::Chooser(_) | Self::Radial | Self::Organic => None,
        }
    }
}

/// A direction constraint, the same type for the root and for every container (docs/adr/0003-container-direction-constraints.md).
/// Written in the graph as `"lr"`, `"rl"`, `"tb"`, `"bt"` (fixed), `"auto"` (free: the layout chooses) or a list such as
/// `["lr", "tb"]` (allowed). A missing root entry is free. A missing container entry inherits the nearest ancestor's.
#[derive(Clone, Debug, PartialEq)]
pub enum DirectionConstraint {
    Fixed(Orientation),
    Free,
    Allowed(Vec<Orientation>),
}

impl Default for DirectionConstraint {
    fn default() -> Self {
        Self::Fixed(Orientation::Lr)
    }
}

/// A layout strategy constraint. Written as a strategy name (fixed), `"auto"` (best wins: the search picks among the
/// eligible strategies by the shared key) or a list of strategy names (allowed). A missing entry inherits as for direction;
/// a missing root entry is `"none"`.
#[derive(Clone, Debug, PartialEq)]
pub enum StrategyConstraint {
    Fixed(LayoutStrategy),
    Auto,
    Allowed(Vec<LayoutStrategy>),
}

impl Default for StrategyConstraint {
    fn default() -> Self {
        Self::Fixed(LayoutStrategy::None)
    }
}

/// The shape of an edge route. The first four are drawn; the others are named so that a graph can ask for them and be refused
/// with the reason, rather than drawn some other way.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RoutingStyle {
    /// One segment per edge, between its ports.
    Straight,
    /// Axis-parallel segments between ports.
    #[default]
    Orthogonal,
    /// The orthogonal route, with its corners drawn as arcs. The route keeps its axis-parallel points; the scene marks it
    /// and a renderer rounds the corners.
    RoundedOrthogonal,
    /// The orthogonal route pulled taut: bends the route does not need are dropped, so segments run at any angle and
    /// still stay clear of the other nodes. The scene carries the pulled route; the layout keeps the orthogonal one.
    Polyline,
    /// The polyline, with its corners drawn as arcs (see `RoundedOrthogonal`). The scene keeps a larger distance to
    /// the nodes, so the arcs do not cut into them.
    RoundedPolyline,
    Octilinear,
    Curve,
    Bus,
}

impl RoutingStyle {
    /// Whether the layout draws this style.
    pub fn supported(self) -> bool {
        matches!(
            self,
            Self::Straight
                | Self::Orthogonal
                | Self::RoundedOrthogonal
                | Self::Polyline
                | Self::RoundedPolyline
        )
    }

    /// The contract name of the style.
    pub fn name(self) -> &'static str {
        match self {
            Self::Straight => "straight",
            Self::Orthogonal => "orthogonal",
            Self::RoundedOrthogonal => "roundedOrthogonal",
            Self::Polyline => "polyline",
            Self::RoundedPolyline => "roundedPolyline",
            Self::Octilinear => "octilinear",
            Self::Curve => "curve",
            Self::Bus => "bus",
        }
    }
}

/// A routing constraint, with the same three forms as direction: a fixed style, `"auto"` (the layout chooses among the drawn
/// styles), or a list of allowed styles (the layout chooses among the drawn members; their order is not a preference).
/// An explicit value is kept as written, including `orthogonal`.
#[derive(Clone, Debug, PartialEq)]
pub enum RoutingConstraint {
    Fixed(RoutingStyle),
    Auto,
    Allowed(Vec<RoutingStyle>),
}

impl RoutingConstraint {
    /// Whether the constraint admits `style`: a fixed style admits only itself, auto every style, a list its members.
    pub fn admits(&self, style: RoutingStyle) -> bool {
        match self {
            Self::Fixed(s) => *s == style,
            Self::Auto => true,
            Self::Allowed(list) => list.contains(&style),
        }
    }

    /// Whether the non-straight edges of the area are drawn in `style`. A fixed style is drawn as asked. A list is drawn in
    /// its plainest non-straight member (orthogonal, rounded orthogonal, polyline, rounded polyline); its order is no preference. `auto`
    /// keeps the orthogonal baseline.
    pub fn picks(&self, style: RoutingStyle) -> bool {
        match self {
            Self::Fixed(fixed) => *fixed == style,
            Self::Auto => false,
            Self::Allowed(list) => {
                list.iter().filter(|s| **s != RoutingStyle::Straight).min() == Some(&style)
            }
        }
    }

    /// Whether every edge of the area is drawn on its orthogonal route: a fixed `orthogonal` or `roundedOrthogonal`, or a list
    /// of only these two. Rounded corners are drawn by the scene on the same route, so a layout that only builds orthogonal
    /// routes (single pass) can take both. `auto` and lists with other styles may draw some edges otherwise.
    pub fn orthogonal_only(&self) -> bool {
        let orthogonal = |s: &RoutingStyle| {
            matches!(
                s,
                RoutingStyle::Orthogonal | RoutingStyle::RoundedOrthogonal
            )
        };
        match self {
            Self::Fixed(style) => orthogonal(style),
            Self::Auto => false,
            Self::Allowed(list) => !list.is_empty() && list.iter().all(orthogonal),
        }
    }

    /// Whether the non-straight edges of the area are drawn with rounded corners (see `picks`).
    pub fn rounds(&self) -> bool {
        self.picks(RoutingStyle::RoundedOrthogonal) || self.picks(RoutingStyle::RoundedPolyline)
    }
}

impl Serialize for RoutingConstraint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Fixed(style) => style.serialize(serializer),
            Self::Auto => serializer.serialize_str("auto"),
            Self::Allowed(list) => list.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for RoutingConstraint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        parse_routing(&value).map_err(serde::de::Error::custom)
    }
}

/// Parses a routing constraint from its contract value: a style name, `"auto"`, or a list of style names.
pub fn parse_routing(value: &serde_json::Value) -> Result<RoutingConstraint, String> {
    let style = |v: &serde_json::Value| -> Result<RoutingStyle, String> {
        serde_json::from_value(v.clone()).map_err(|_| format!("unknown routing style {v}"))
    };
    match value {
        serde_json::Value::String(text) if text == "auto" => Ok(RoutingConstraint::Auto),
        serde_json::Value::String(_) => style(value).map(RoutingConstraint::Fixed),
        serde_json::Value::Array(items) => {
            let list: Vec<RoutingStyle> = items.iter().map(style).collect::<Result<_, _>>()?;
            if list.is_empty() {
                return Err("an allowed routing list is empty".into());
            }
            for (i, d) in list.iter().enumerate() {
                if list[..i].contains(d) {
                    return Err(format!("routing style {} is listed twice", d.name()));
                }
            }
            Ok(if list.len() == 1 {
                RoutingConstraint::Fixed(list[0])
            } else {
                RoutingConstraint::Allowed(list)
            })
        }
        other => Err(format!(
            "a routing constraint must be a name or a list, not {other}"
        )),
    }
}

fn orientation_from_name(name: &str) -> Option<Orientation> {
    serde_json::from_value(serde_json::Value::String(name.into())).ok()
}

pub fn strategy_from_name(name: &str) -> Option<LayoutStrategy> {
    serde_json::from_value(serde_json::Value::String(name.into())).ok()
}

/// The shortest end-to-end path is an optimization criterion, not a layout strategy. It stays readable in the legacy
/// direction slot until it has its own parameter; the strategy fields refuse it.
const CRITERION_NAMES: &[&str] = &["shortest"];

/// Parses a direction constraint from its contract value (a name, `"auto"`, or a list of orientations).
pub fn parse_direction(value: &serde_json::Value) -> Result<DirectionConstraint, String> {
    match value {
        serde_json::Value::String(text) if text == "auto" => Ok(DirectionConstraint::Free),
        serde_json::Value::String(text) => orientation_from_name(text)
            .map(DirectionConstraint::Fixed)
            .ok_or_else(|| format!("unknown direction \"{text}\"")),
        serde_json::Value::Array(items) => {
            let list: Vec<Orientation> = items
                .iter()
                .map(|v| {
                    v.as_str()
                        .and_then(orientation_from_name)
                        .ok_or_else(|| format!("unknown direction {v} in a list"))
                })
                .collect::<Result<_, _>>()?;
            if list.is_empty() {
                return Err("an allowed direction list is empty".into());
            }
            for (i, d) in list.iter().enumerate() {
                if list[..i].contains(d) {
                    return Err(format!("direction {d:?} is listed twice"));
                }
            }
            Ok(if list.len() == 1 {
                DirectionConstraint::Fixed(list[0])
            } else {
                DirectionConstraint::Allowed(list)
            })
        }
        other => Err(format!("a direction must be a name or a list, not {other}")),
    }
}

/// Parses a layout strategy constraint from its contract value (a name, `"auto"`, or a list of strategy names).
pub fn parse_strategy(value: &serde_json::Value) -> Result<StrategyConstraint, String> {
    match value {
        serde_json::Value::String(text) if text == "auto" => Ok(StrategyConstraint::Auto),
        serde_json::Value::String(text) if CRITERION_NAMES.contains(&text.as_str()) => Err(format!(
            "\"{text}\" is an optimization criterion, not a layout strategy; it is not a strategy value"
        )),
        serde_json::Value::String(text) => strategy_from_name(text)
            .map(StrategyConstraint::Fixed)
            .ok_or_else(|| format!("unknown layout strategy \"{text}\"")),
        serde_json::Value::Array(items) => {
            let list: Vec<LayoutStrategy> = items
                .iter()
                .map(|v| {
                    v.as_str()
                        .filter(|t| !CRITERION_NAMES.contains(t))
                        .and_then(strategy_from_name)
                        .ok_or_else(|| format!("unknown layout strategy {v} in a list"))
                })
                .collect::<Result<_, _>>()?;
            if list.is_empty() {
                return Err("an allowed strategy list is empty".into());
            }
            for (i, d) in list.iter().enumerate() {
                if list[..i].contains(d) {
                    return Err(format!("strategy {d:?} is listed twice"));
                }
            }
            Ok(if list.len() == 1 {
                StrategyConstraint::Fixed(list[0])
            } else {
                StrategyConstraint::Allowed(list)
            })
        }
        other => Err(format!(
            "a layout strategy must be a name or a list, not {other}"
        )),
    }
}

/// Values of a direction field as earlier graphs wrote them: `hybrid` or `shortest` in the direction slot. Such a value is
/// read as the strategy (fixed) with a free direction, which is what it always meant, and written back the same way.
pub fn legacy_strategy(value: &serde_json::Value) -> Option<LayoutStrategy> {
    match value {
        serde_json::Value::String(text) if text == "shortest" => strategy_from_name(text),
        _ => None,
    }
}

impl Serialize for DirectionConstraint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Fixed(o) => o.serialize(serializer),
            Self::Free => serializer.serialize_str("auto"),
            Self::Allowed(list) => list.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for DirectionConstraint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        parse_direction(&value).map_err(serde::de::Error::custom)
    }
}

impl Serialize for StrategyConstraint {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Fixed(s) => s.serialize(serializer),
            Self::Auto => serializer.serialize_str("auto"),
            Self::Allowed(list) => list.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for StrategyConstraint {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        parse_strategy(&value).map_err(serde::de::Error::custom)
    }
}

impl DirectionConstraint {
    /// The orientation a layout uses without a search: the fixed one, `lr` for free, the first listed for an allowed set.
    pub fn default_choice(&self) -> Orientation {
        match self {
            Self::Fixed(o) => *o,
            Self::Free => Orientation::Lr,
            Self::Allowed(list) => list.first().copied().unwrap_or_default(),
        }
    }

    /// The orientations a search may choose among: `lr` and `tb` for free, the listed ones for an allowed set.
    pub fn candidates(&self) -> Vec<Orientation> {
        match self {
            Self::Fixed(_) => Vec::new(),
            Self::Free => vec![Orientation::Lr, Orientation::Tb],
            Self::Allowed(list) => list.clone(),
        }
    }

    /// The constraint in words, for reports.
    pub fn describe(&self) -> String {
        let name = |o: &Orientation| format!("{o:?}").to_lowercase();
        match self {
            Self::Fixed(o) => format!("fixed {}", name(o)),
            Self::Free => "free".into(),
            Self::Allowed(list) => format!(
                "allowed {}",
                list.iter().map(name).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

impl StrategyConstraint {
    /// The strategy a layout uses without a search: the fixed one, `none` for auto, the first listed for an allowed set.
    pub fn default_choice(&self) -> LayoutStrategy {
        match self {
            Self::Fixed(s) => *s,
            Self::Auto => LayoutStrategy::None,
            Self::Allowed(list) => list.first().copied().unwrap_or_default(),
        }
    }

    /// The strategies a search may choose among.
    pub fn candidates(&self) -> Vec<LayoutStrategy> {
        match self {
            Self::Fixed(_) => Vec::new(),
            Self::Auto => vec![
                LayoutStrategy::None,
                LayoutStrategy::Tree,
                LayoutStrategy::Radial,
                LayoutStrategy::Organic,
            ],
            Self::Allowed(list) => list.clone(),
        }
    }

    /// The constraint in words, for reports.
    pub fn describe(&self) -> String {
        let name = |s: &LayoutStrategy| format!("{s:?}").to_lowercase();
        match self {
            Self::Fixed(s) => format!("fixed {}", name(s)),
            Self::Auto => "auto".into(),
            Self::Allowed(list) => format!(
                "allowed {}",
                list.iter().map(name).collect::<Vec<_>>().join(", ")
            ),
        }
    }
}

impl Graph {
    /// Whether every area draws its edges on their orthogonal routes, plain or with rounded corners
    /// (`RoutingConstraint::orthogonal_only`); no routing declared is orthogonal.
    pub fn routing_orthogonal_only(&self) -> bool {
        self.default_routing
            .as_ref()
            .is_none_or(RoutingConstraint::orthogonal_only)
            && self
                .container_routing
                .values()
                .all(RoutingConstraint::orthogonal_only)
    }

    /// The nearest declared entry of a container map, starting at `parent` and walking up, else the root's value.
    fn nearest<'a, T: Clone>(
        &'a self,
        parent: Option<&str>,
        entries: &'a BTreeMap<String, T>,
        root: &T,
    ) -> T {
        let mut current = parent;
        while let Some(id) = current {
            if let Some(value) = entries.get(id) {
                return value.clone();
            }
            current = self
                .nodes
                .iter()
                .find(|n| n.id == id)
                .and_then(|n| n.parent_id.as_deref());
        }
        root.clone()
    }

    /// The direction constraint in force for the interior of `parent` (`None` for the root).
    pub fn container_constraint(&self, parent: Option<&str>) -> DirectionConstraint {
        self.nearest(parent, &self.container_directions, &self.default_direction)
    }

    /// The strategy constraint in force for the interior of `parent` (`None` for the root).
    pub fn strategy_constraint(&self, parent: Option<&str>) -> StrategyConstraint {
        self.nearest(parent, &self.container_strategies, &self.default_strategy)
    }

    /// The routing constraint authored for the area `parent` (`None` for the root): the nearest declaration on the way up,
    /// the root included. `None` when nothing is declared, so the layout's fallback applies.
    pub fn routing_constraint(&self, parent: Option<&str>) -> Option<RoutingConstraint> {
        let mut current = parent;
        while let Some(id) = current {
            if let Some(value) = self.container_routing.get(id) {
                return Some(value.clone());
            }
            current = self
                .nodes
                .iter()
                .find(|n| n.id == id)
                .and_then(|n| n.parent_id.as_deref());
        }
        self.default_routing.clone()
    }

    /// The containers strictly above `id`, nearest first.
    fn strict_ancestors(&self, id: &str) -> Vec<&str> {
        let mut chain = Vec::new();
        let mut current = self
            .nodes
            .iter()
            .find(|n| n.id == id)
            .and_then(|n| n.parent_id.as_deref());
        while let Some(parent) = current {
            chain.push(parent);
            current = self
                .nodes
                .iter()
                .find(|n| n.id == parent)
                .and_then(|n| n.parent_id.as_deref());
        }
        chain
    }

    /// The area an edge is drawn in: the deepest container that holds both endpoints (`None` for the root). An edge from a
    /// container to its own child is drawn in the container's parent.
    pub fn edge_area(&self, source: &str, target: &str) -> Option<String> {
        let target_chain = self.strict_ancestors(target);
        self.strict_ancestors(source)
            .into_iter()
            .find(|area| target_chain.contains(area))
            .map(str::to_string)
    }

    /// The layout mode of the area whose children are `parent`'s children, from its two constraints. The strategy `auto`
    /// has no search here, so it takes `none`; the composition's search chooses among the candidates. A strategy that
    /// cannot take its direction is refused with the reason: `radial` needs a free direction, and `hybrid` and `shortest`
    /// choose the orientation themselves, so they need a free or allowed direction.
    pub fn area_mode(&self, parent: Option<&str>) -> Result<Mode, String> {
        let area = parent.unwrap_or("the root");
        let direction = self.container_constraint(parent);
        let strategy = self.strategy_constraint(parent).default_choice();
        let check = |who: &str| -> Result<(), String> {
            match &direction {
                DirectionConstraint::Free => Ok(()),
                other => Err(format!("{who} cannot take the {} direction of {area}: set the direction to auto or an allowed set", other.describe())),
            }
        };
        match strategy {
            LayoutStrategy::None => Ok(match direction {
                // A free direction chooses per area by compactness.
                DirectionConstraint::Free => Mode::Chooser(Chooser::Compact),
                other => Mode::Orientation(other.default_choice()),
            }),
            LayoutStrategy::Tree => Ok(Mode::Tree(direction.default_choice())),
            // A stress placement is not a ranked layout, so a direction does not apply to it. The strategy an area is given
            // wins: an organic area ignores the direction it has or inherits (the direction stays in the document and applies
            // again when the strategy changes). A strategy that is only inherited yields to a direction the container
            // declares itself, so a container with its own direction stays layered in an organic parent.
            LayoutStrategy::Organic => {
                let inherited_organic =
                    parent.is_some_and(|id| !self.container_strategies.contains_key(id));
                let own_direction =
                    parent.is_some_and(|id| self.container_directions.contains_key(id));
                if inherited_organic && own_direction && direction != DirectionConstraint::Free {
                    Ok(Mode::Orientation(direction.default_choice()))
                } else {
                    Ok(Mode::Organic)
                }
            }
            LayoutStrategy::Radial => {
                if let DirectionConstraint::Fixed(_) = direction {
                    return Err(format!(
                        "radial cannot take the fixed direction of {area}: set the direction to auto, or use the tree strategy"
                    ));
                }
                if let DirectionConstraint::Allowed(_) = direction {
                    return Err(format!(
                        "radial cannot take the allowed directions of {area}: set the direction to auto, or use the tree strategy"
                    ));
                }
                Ok(Mode::Radial)
            }
            LayoutStrategy::Shortest => {
                check("shortest")?;
                Ok(Mode::Chooser(Chooser::Shortest))
            }
        }
    }
}

/// Refuses `initialCollapse`, renamed to `levelOfDetail` in engine 0.4.
fn renamed_to_level_of_detail<'de, D: serde::Deserializer<'de>>(_: D) -> Result<(), D::Error> {
    Err(serde::de::Error::custom(
        "initialCollapse was renamed to levelOfDetail in engine 0.4",
    ))
}

/// The level of detail (provisional): which containers a view shows collapsed at first, by depth and by kind; opening
/// them is up to the user (`toggle`, `reveal`, or a click in the cluster overview). The same value for the layered view
/// (`LayoutOptions::level_of_detail`) and the cluster overview (`OverviewConfig::level_of_detail`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct LevelOfDetail {
    /// Containers at this depth or deeper (the top level is 0).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<usize>,
    /// Containers of these kinds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
}

/// What a view hides (provisional, view state set by the `filter` operation): nodes of these kinds, these nodes (each with
/// everything below it) and edges of these kinds. The layout keeps their room, so what stays keeps its place; the scene
/// leaves them out, and an edge that stands for collapsed content counts only the edges still shown.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct Hidden {
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub node_kinds: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub nodes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub edge_kinds: Vec<String>,
    /// Lay the view out again without what is hidden, so that the rest closes up (positions change); off: the hidden keep
    /// their room and nothing else moves. Omitted when off.
    #[serde(skip_serializing_if = "is_false")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<bool>"))]
    pub compact: bool,
}

impl Hidden {
    pub fn is_empty(&self) -> bool {
        self.node_kinds.is_empty() && self.nodes.is_empty() && self.edge_kinds.is_empty()
    }

    /// Whether the view leaves the hidden out of its layout (`compact` with something hidden).
    pub fn compacts(&self) -> bool {
        self.compact && !self.is_empty()
    }
}

/// Provisional: the children that `communities` leaves out of Louvain, so that one hub does not pull a whole container
/// into one community: those with at least `min_links` links in their container (by edge weight, as Louvain counts them)
/// and those named in `nodes`. A hub stays a direct child of its container. Ids the graph lacks are ignored.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct CommunityHubs {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "number"))]
    pub min_links: Option<u32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Vec<String>>"))]
    pub nodes: Vec<String>,
}

impl CommunityHubs {
    pub fn is_empty(&self) -> bool {
        self.min_links.is_none() && self.nodes.is_empty()
    }
}

/// Experimental (`LayoutOptions::pack_components`): how the components of a container are packed.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct PackComponents {
    /// The width over the height the packed components aim at (default 1: square).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub aspect: Option<f64>,
}

/// The edges one aggregated edge stands for, by direction (see `LayoutOptions::aggregate_weights`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct AggregateWeight {
    pub forward: usize,
    pub backward: usize,
}

/// View configuration. Part of the portable snapshot, so backend and browser compute identically.
/// The default value reproduces the prototype's behavior and is omitted from serialized snapshots.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default)]
pub struct LayoutOptions {
    /// Identity of the config set that produced these options (`<presetId>@<version>`). Absent for custom
    /// options. Every option change that does not come from a config set clears it (see runtime).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_id: Option<String>,
    pub appearance: AppearanceConfig,
    pub edge_labels: BTreeMap<String, LabelSpec>,
    pub port_labels: BTreeMap<String, LabelSpec>,
    /// Ordering strategies to evaluate, by mode number 1..=4 (see layout::CANDIDATES).
    pub candidates: Vec<usize>,
    /// Whether the routing-only candidate is evaluated.
    pub routing_candidate: bool,
    pub objectives: Vec<Objective>,
    pub bundling: Bundling,
    pub hybrid_by: HybridCriterion,
    /// Measured leaf sizes reported by the visualization. Absent nodes use the default size.
    pub node_sizes: BTreeMap<String, NodeSize>,
    /// The revision of the latest geometry measurement applied to each element (see `Measurement`). A measurement with
    /// an older revision is refused. Omitted when empty, so default output does not change.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub geometry_revisions: BTreeMap<String, u64>,
    /// Where the user put labels: the top-left corner in scene coordinates, keyed by the label's id (an edge or a port). A
    /// label is placed there while the place is free and reported as a routing issue otherwise. Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub label_positions: BTreeMap<String, Point>,
    /// Rows of leaf nodes (ADR 0011). A node with rows takes its size from them instead of from `node_sizes`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub node_rows: BTreeMap<String, Vec<Row>>,
    /// Extra room, in layout units, for an edge that leaves a node and returns to it. Zero keeps the standard loop.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub self_loop_clearance: f64,
    /// How row ports on one row and side are placed (ADR 0011). The default `peer` is omitted from the output.
    #[serde(default, skip_serializing_if = "RowPortOrder::is_default")]
    pub row_port_order: RowPortOrder,
    /// Minimum distance between neighbouring ports on one node side. Zero keeps the prototype sizing.
    /// When set, leaf sizes grow until every side has room for its ports.
    pub min_port_spacing: f64,
    /// Placement grid for leaf sizes and positions. Zero means free placement.
    pub placement_grid: f64,
    /// Where automatic ports sit along their side (see `PortPlacement`).
    #[serde(default, skip_serializing_if = "is_symmetric")]
    pub port_placement: PortPlacement,
    /// Experimental flat-region candidate (A4b); see `LayoutConfig::flat_region`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flat_region: bool,
    /// Experimental (P1): the channel router's later rounds also re-route edges whose routes cross another route or
    /// hug a node margin, and refinement adds a candidate that uses this wider selection. Off, nothing changes.
    #[serde(default, skip_serializing_if = "is_false")]
    pub extended_conflict_selection: bool,
    /// Minimum distance between routes and nodes they do not attach to. Zero keeps the prototype.
    pub routing_clearance: f64,
    /// Experimental: routes never enter the clearance band of a foreign node (channel engine). Edges that cannot be
    /// routed that way fall back to a direct line, which the routing issues report when it passes through a node.
    /// Omitted when off.
    #[serde(default, skip_serializing_if = "is_false")]
    pub strict_clearance: bool,
    /// Places each weakly connected component of a container's children as its own block along the cross axis of every
    /// level, ordering components separately. The portfolio also tries the unpacked layout, and the published key
    /// decides between them. Omitted when off, so the default output does not change.
    #[serde(default, skip_serializing_if = "is_false")]
    pub component_packing: bool,
    /// Experimental: the weakly connected components of a container's children (a lone child is one) keep their own
    /// arrangement and are packed as boxes in rows towards an aspect ratio (`aspect`, width over height, default 1),
    /// largest first, instead of all sharing the ranks of one long drawing. Orders the components in blocks as
    /// `component_packing` does. Not for circle containers, containers with a pinned child, or flattened areas. Omitted
    /// when off, so the default output does not change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub pack_components: Option<PackComponents>,
    /// Experimental: the children of a layered area that have no edge to another child (a file of unrelated functions)
    /// are spread over the ranks as a grid instead of all standing in the first rank, one long column. They fill the
    /// ranks the connected children use, and more when there are many of them, so the grid comes out about square.
    /// Omitted when off, so the default output does not change.
    #[serde(default, skip_serializing_if = "is_false")]
    pub isolated_grid: bool,
    /// How edges are drawn: orthogonal routes (the default, omitted) or straight segments between the port positions.
    /// Straight routing never bends: an edge that would cross a foreign node is reported as a routing issue.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edge_routing: Option<EdgeRouting>,
    /// Force-directed behaviour of organic areas: a layout that continues an earlier one does not hold the nodes where they
    /// are but relaxes the stress for a few steps from their positions. A node that is pinned (or is being dragged) is held,
    /// the others follow it like on springs, and a node that is let go settles. Omitted when off, so the default output does
    /// not change.
    #[serde(default, skip_serializing_if = "is_false")]
    pub force_directed: bool,
    /// Iteration budget of the organic (stress) placement. Omitted when unset; the default is 300.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organic_iterations: Option<usize>,
    /// Experimental minimum-displacement overlap constraints for organic placement and repair. Off by default.
    #[serde(default, skip_serializing_if = "is_false")]
    pub vpsc_overlap_removal: bool,
    /// Experimental conflict-directed organic repair, with the exhaustive search as a fallback. Off by default.
    #[serde(default, skip_serializing_if = "is_false")]
    pub directed_repair: bool,
    /// Experimental bounded repulsion from projected edge chords at the end of organic stress. Off by default.
    #[serde(default, skip_serializing_if = "is_false")]
    pub node_edge_repulsion: bool,
    /// Experimental sparse stress for organic placement: exact targets for graph neighbours and pivots instead of the full
    /// distance matrix, so an area may hold more children than the dense bound. Off by default.
    #[serde(default, skip_serializing_if = "is_false")]
    pub sparse_stress: bool,
    /// Experimental scalable organic placement: pivot initialization, sparse stress whose pivot terms are weighted by the pivot
    /// regions (Ortmann, Klimenta and Brandes, GD 2016), then rounds that alternate a stress step with a separation of
    /// overlapping boxes. An area may hold up to `SCALABLE_MAX_NODES` children. Off by default; takes precedence over
    /// `sparseStress`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub scalable_stress: bool,
    /// Experimental size-aware stress targets for organic placement: a pair's target length adds the half-extents of both
    /// children, so large nodes are kept apart by the stress itself. Off by default.
    #[serde(default, skip_serializing_if = "is_false")]
    pub size_aware_stress: bool,
    /// Use the hierarchical layered pipeline where the graph's area modes support it. Set false to use the previous
    /// layered layout for comparison. The pipeline does not change explicit tree, radial or organic area strategies.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub layered_construction: bool,
    /// Provisional opt-in complete abstract routing after placement, instead of the
    /// legacy constructor/probe loop. Requires a uniformly positive-flow layered
    /// hierarchy, default orthogonal routing, no bundling, pins or fixed sizes.
    /// Unsupported requests fail explicitly. Refinement keeps its incumbent and
    /// existing candidate portfolio; this option is not output-equivalent.
    /// On by default since engine 0.2, with `single_pass` and `single_pass_auto` (ADR 0016); written as `false` when a layout
    /// fell back to the other pipeline.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub abstract_routing: bool,
    /// Provisional opt-in (requires `abstract_routing`): construct once and do not correct afterwards. Abstract
    /// completion keeps plan, growth and construction, but draws edges the constructor delegates with their simple
    /// port-to-port seed route instead of searching or repairing them, and counts them in the layout diagnostics.
    /// The hierarchical order is chosen after the backward-edge border correction, and the placement keeps the border
    /// ranks it chose. Refinement plans no candidates. On by default since engine 0.2 (ADR 0016), accepted by its quality gate;
    /// written as `false` when a layout fell back to the other pipeline.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub single_pass: bool,
    /// Use single pass where it works, and the other pipeline, with a diagnostic, where it fails: on by default since engine
    /// 0.2 (ADR 0016; omitted when on, written as `false` when off). An import or a `configure` with this option on turns on
    /// `abstract_routing` and `single_pass`. A layout that fails with them turns them off, so these two options say which
    /// pipeline made the drawing, and later edits and the refinement stay on it until the next import or `configure`, which
    /// keeps an edit from changing pipeline twice.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub single_pass_auto: bool,
    /// Provisional, for measurement: the seed of the shuffled starts of the hierarchical order (runs from the fourth on).
    /// 0 (the default, omitted) keeps the order that was used before the option existed. Quality gates average over seeds
    /// (ADR 0016); a drawing made with one seed is as valid as with any other.
    #[serde(default, skip_serializing_if = "is_zero_seed")]
    pub order_seed: u32,
    /// Experimental (plan section 4a, global layering, iteration 1): the most visible nodes the first view of an import lays
    /// out. Containers are collapsed to stay within it, the outermost first, and expanded level by level while it holds.
    /// 0 (the default, omitted) lays out every node.
    #[serde(default, skip_serializing_if = "is_zero_count")]
    pub visible_node_budget: usize,
    /// Provisional: containers with more children than this (the root included) are divided into communities of their
    /// children, found from the edges between them (the cluster overview's Louvain), shown as containers of kind
    /// `community`, collapsed at first and opened with `toggle`. They exist only in the view: the source graph is
    /// unchanged. 0 (the default, omitted) adds none.
    #[serde(default, skip_serializing_if = "is_zero_count")]
    pub communities: usize,
    /// Provisional: groups given by the data or its domain, by node id: the path of groups the node belongs to inside its
    /// container (`["Billing", "Invoices"]`). They become containers of kind `group` in the view only, nested along the path,
    /// before `communities` divides what is still large. Ids the graph does not have are ignored (a mapping may cover more
    /// than the imported view). Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub groups: BTreeMap<String, Vec<String>>,
    /// Provisional: the level of detail, which containers an import shows collapsed: those at `depth` or deeper (0: every
    /// one; the top level is depth 0) and those of the `kinds` listed. Unset (the default, omitted): none, except the
    /// containers that `communities` and `groups` add, which start collapsed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level_of_detail: Option<LevelOfDetail>,
    /// Provisional (view state, set by `filter`): what the view hides, keeping the place of the rest. Omitted when empty.
    #[serde(default, skip_serializing_if = "Hidden::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<Hidden>"))]
    pub hidden: Hidden,
    /// Provisional: the children `communities` leaves out of Louvain (hubs). Omitted when empty.
    #[serde(default, skip_serializing_if = "CommunityHubs::is_empty")]
    #[cfg_attr(feature = "ts", ts(optional, as = "Option<CommunityHubs>"))]
    pub community_hubs: CommunityHubs,
    /// The name before engine 0.4 (`initialCollapse`), refused with a message that names the new one.
    #[serde(
        default,
        rename = "initialCollapse",
        skip_serializing,
        deserialize_with = "renamed_to_level_of_detail"
    )]
    #[cfg_attr(feature = "ts", ts(skip))]
    pub renamed_initial_collapse: (),
    /// Provisional (view state, set by `reveal`): containers shown in part. The children listed for a container are shown;
    /// the others are gathered in a collapsed container `rest:<id>` of kind `rest` ("+N more"), which a `toggle` dissolves.
    /// Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub partial: BTreeMap<String, Vec<String>>,
    /// Experimental (plan section 4a, global layering, iteration 2; needs `layered_construction`): in a layered area with a
    /// fixed flow, an expanded child container of the same flow axis takes part with its children in the area's ranks, as
    /// a contiguous block, instead of being laid out on its own. Off by default; omitted when off.
    #[serde(default, skip_serializing_if = "is_false")]
    pub global_layering: bool,
    /// Find a shared order for hierarchical areas and route through their planned sections. Enabled with the default
    /// layered pipeline; set false to compare against the earlier per-area ordering.
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub hierarchical_ordering: bool,
    /// Experimental (needs `hierarchical_ordering`): the ranks of every layered area minimise the summed spans of its links,
    /// weighted by their edges (network simplex), instead of the longest paths. Off by default; omitted when off.
    #[serde(default, skip_serializing_if = "is_false")]
    pub simplex_ranks: bool,
    /// Experimental (derived by the runtime, not set by a user): the size a collapsed container takes, estimated from its
    /// content (inside out), so that expanding it moves the outer picture little. Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub collapsed_sizes: BTreeMap<String, NodeSize>,
    /// Provisional (derived by the runtime with `communities`, not set by a user): what an edge that stands for collapsed
    /// content represents, by edge id: the edges between its two visible ends in its own direction (`forward`) and in the
    /// other (`backward`). The route takes `forward` as its weight; the scene reports `backward` as `backWeight`, so a
    /// renderer can draw both directions on one route. Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub aggregate_weights: BTreeMap<String, AggregateWeight>,
    /// Optional limit on the drawing's larger extent, in units. Not a constraint the search enforces: when the result
    /// spans more, a routing issue reports it (the search is not extent-aware, so this proves nothing about feasibility).
    /// Omitted when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_extent: Option<f64>,
    /// Experimental. Re-routes every ordering candidate before the key compares it, so an ordering's layout competes with
    /// its own routing. Off by default; on the benchmark corpus the effect is mixed (see
    /// `benchmarks/results/portfolio-coordinator-20261023.md`), so it is not a recommended setting.
    #[serde(default, skip_serializing_if = "is_false")]
    pub routed_candidates: bool,
    /// Authored container sizes by container ID (ADR 0010). An axis without a value follows its content. Omitted when
    /// empty, so default documents do not change.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub container_sizing: BTreeMap<String, ContainerSizing>,
    /// Experimental. The number of archive entries after the incumbent that each get one routing pass after the first
    /// stage, in archive order, whether or not they were published. Off (0) by default; the default portfolio is unchanged.
    #[serde(default, skip_serializing_if = "is_zero_count")]
    pub archive_refinement: usize,
    /// Spacing between parallel routes. The prototype uses 6.
    pub track_spacing: f64,
    /// Provisional: the least gap between two neighbours of a rank (across the flow), before label room is added. `None` (the
    /// default, omitted) keeps 24.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_spacing: Option<f64>,
    /// Provisional: the least gap between two ranks (along the flow), before label room is added and channels widen for their
    /// tracks. `None` (the default, omitted) keeps 64.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer_spacing: Option<f64>,
    /// Provisional (single pass): explicit ports on any side. An edge end whose port lies across the flow or on the side
    /// against it is constructed through a short lead around its node instead of being left to the fallback router. Off by
    /// default (omitted), so default drawings do not change.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub port_sides: bool,
    /// Provisional (single pass): areas without a fixed flow (organic, tree, radial) and straight routing are allowed in a
    /// single-pass layout. Those areas keep their own placement and their edges go to the router that completes single pass;
    /// a layered child that grows moves its siblings apart. Off by default (omitted): such graphs use the pipeline before
    /// single pass, as before.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub single_pass_mixed: bool,
    /// The clearance routes keep from nodes, their own nodes included beyond the stub at their port. It is a hard
    /// constraint (ADR 0005): the default is `DEFAULT_REQUIRED_CLEARANCE`, so it always applies. An explicit `null` is
    /// the opt-out: proximity is then a weighted quality term with the fixed `HUG_MARGIN` (docs/adr/0002-quality-model.md).
    #[serde(
        default = "default_required_clearance",
        skip_serializing_if = "is_default_required_clearance"
    )]
    pub required_clearance: Option<f64>,
    /// Experimental: during clearance widening, re-place affected areas and reuse routes whose geometry is unchanged.
    /// Candidate selection and geometry validation still use the common rules; off by default.
    #[serde(default, skip_serializing_if = "is_false")]
    pub incremental_clearance_widening: bool,
    /// Adds corridor centres as candidate tracks for routes.
    pub routing_corridors: bool,
    /// Spreads parallel route segments to the track spacing after routing.
    pub routing_track_separation: bool,
    /// Orthogonal routing engine.
    pub routing_engine: RoutingEngine,
    /// Lets nodes slide inside their layer towards the centre of their links (free positioning).
    pub free_placement: bool,
    /// Evaluations for the routing-aware placement search during refinement. Zero disables it.
    pub placement_search: usize,
    /// Node positions fixed by the curator, keyed by leaf node id: the top-left corner relative to the direct
    /// parent (the world origin for root nodes). Stored, validated and honoured by placement; the editor offers them
    /// as controls. Omitted when empty, so default output does not change.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub pins: BTreeMap<String, Point>,
    /// Provisional: orders the user chose by a drag in a layered area, keyed by node id. The node stands in its rank right
    /// after the named sibling, or first. A hint whose sibling is in another rank has no effect. Omitted when empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub order_hints: BTreeMap<String, OrderHint>,
}

/// An explicit bus (`Graph::buses`): edges drawn with one shared trunk. The edges that share an end (all one source,
/// or all one target) leave (enter) that node by one port and share the way to where they branch off.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Bus {
    pub id: String,
    pub edges: Vec<String>,
    /// What the bus is called in the drawing ("CAN 500 kbit/s"). Placed like an edge label when the options give its size
    /// under `edge_labels["bus:<id>"]`; omitted when none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub label: Option<String>,
}

/// Where a node stands in its rank, chosen by the user (`LayoutOptions::order_hints`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct OrderHint {
    /// The sibling it follows; none puts it first.
    pub after: Option<String>,
}
impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            config_id: None,
            appearance: AppearanceConfig::default(),
            edge_labels: BTreeMap::new(),
            port_labels: BTreeMap::new(),
            candidates: vec![1, 2, 3, 4],
            routing_candidate: true,
            objectives: DEFAULT_OBJECTIVES.to_vec(),
            bundling: Bundling::None,
            hybrid_by: HybridCriterion::Compact,
            node_sizes: BTreeMap::new(),
            geometry_revisions: BTreeMap::new(),
            label_positions: BTreeMap::new(),
            node_rows: BTreeMap::new(),
            min_port_spacing: 0.,
            self_loop_clearance: 0.,
            row_port_order: RowPortOrder::Peer,
            placement_grid: 0.,
            port_placement: PortPlacement::Symmetric,
            flat_region: false,
            routing_clearance: 0.,
            strict_clearance: false,
            component_packing: false,
            pack_components: None,
            isolated_grid: false,
            force_directed: false,
            edge_routing: None,
            organic_iterations: None,
            vpsc_overlap_removal: false,
            directed_repair: false,
            node_edge_repulsion: false,
            sparse_stress: false,
            scalable_stress: false,
            size_aware_stress: false,
            layered_construction: true,
            abstract_routing: true,
            single_pass: true,
            single_pass_auto: true,
            order_seed: 0,
            visible_node_budget: 0,
            communities: 0,
            groups: BTreeMap::new(),
            level_of_detail: None,
            hidden: Hidden::default(),
            community_hubs: CommunityHubs::default(),
            renamed_initial_collapse: (),
            partial: BTreeMap::new(),
            global_layering: false,
            hierarchical_ordering: true,
            simplex_ranks: false,
            collapsed_sizes: BTreeMap::new(),
            aggregate_weights: BTreeMap::new(),
            max_extent: None,
            routed_candidates: false,
            container_sizing: BTreeMap::new(),
            archive_refinement: 0,
            track_spacing: 6.,
            node_spacing: None,
            layer_spacing: None,
            port_sides: false,
            single_pass_mixed: false,
            required_clearance: default_required_clearance(),
            incremental_clearance_widening: false,
            routing_corridors: false,
            routing_track_separation: false,
            routing_engine: RoutingEngine::Legacy,
            extended_conflict_selection: false,
            free_placement: false,
            placement_search: 0,
            pins: BTreeMap::new(),
            order_hints: BTreeMap::new(),
        }
    }
}
impl LayoutOptions {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Drops container entries that follow their content, so that they are stored only when they differ from content.
    pub fn normalize_container_sizing(&mut self) {
        self.container_sizing
            .retain(|_, sizing| !sizing.is_content());
    }

    /// Checks every authored container size (ADR 0010). Invalid values are rejected before any layout runs.
    pub fn validate_container_sizing(&self) -> Result<(), String> {
        for (id, sizing) in &self.container_sizing {
            sizing
                .validate()
                .map_err(|e| format!("container {id}: {e}"))?;
        }
        Ok(())
    }
    /// Named presets. The switch point between presets is set by measurements, not in advance.
    /// `best` currently equals `balanced`: no further strategy exists beyond the four orderings.
    pub fn preset(name: &str) -> Option<Self> {
        match name {
            "fast" => Some(Self {
                candidates: vec![1],
                ..Self::default()
            }),
            "balanced" | "best" => Some(Self::default()),
            _ => None,
        }
    }
}
/// The mode the layout uses for an area. `layout::compute` checks every area before placing it, so an area that cannot
/// take its constraints is refused there; this reads the mode of a graph that has been checked.
pub fn direction_for(graph: &Graph, parent: Option<&str>) -> Mode {
    graph
        .area_mode(parent)
        .unwrap_or(Mode::Orientation(Orientation::Lr))
}

/// Quality objectives in priority order. Earlier objectives dominate later ones.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum Objective {
    SiblingOverlaps,
    EdgeNodeHits,
    SharedRoutePairs,
    RoutedCrossings,
    ProjectedCrossings,
    /// Weighted tail: route length, movement and area, compared with a small tolerance.
    Cost,
    /// Sum of endpoint-center distances; a second-order criterion for compact paths.
    EndToEnd,
    /// Segments that run along or near a foreign node's border, within the node margin.
    NearNodeSegments,
    /// Direction changes over all drawn routes: each bend slows the eye along a line.
    Bends,
}

/// Default priority: node geometry, then no line through a node, no overlapping lines, no lines hugging
/// node borders, fewer crossings, shortest lines last.
pub const DEFAULT_OBJECTIVES: [Objective; 8] = [
    Objective::SiblingOverlaps,
    Objective::EdgeNodeHits,
    Objective::SharedRoutePairs,
    Objective::RoutedCrossings,
    Objective::NearNodeSegments,
    Objective::ProjectedCrossings,
    Objective::Bends,
    Objective::Cost,
];

/// How the edges of a drawing are drawn. Orthogonal routing is the default; straight segments are an option of their own,
/// usable with any arrangement.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum EdgeRouting {
    #[default]
    Orthogonal,
    Straight,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum Bundling {
    #[default]
    None,
    /// Edges with the same source and the same target share one drawn route.
    Complete,
}

/// Where automatic ports sit along their node side. `Symmetric` spaces them evenly, in the order of their peers.
/// `Optimized` moves each port to its peer's position along the side (so the route can run straight) and keeps the
/// minimum port spacing; a side without room for that falls back to symmetric. `Angular` orders the ports of a side by
/// the direction of their peers (slope from the node centre), so the port order around a node follows the drawn
/// direction of its neighbours; with a drawing that realises a rotation system, that is the rotation.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum PortPlacement {
    #[default]
    Symmetric,
    Optimized,
    Angular,
}

/// Orthogonal routing engine. `Legacy` is the candidate-track search; `Channel` is the grid router with
/// negotiated congestion (see routing::channel).
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum RoutingEngine {
    #[default]
    Legacy,
    Channel,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum HybridCriterion {
    #[default]
    Compact,
    EndToEnd,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NodeSize {
    pub width: f64,
    pub height: f64,
}

/// The outline a consumer reports for an element it has drawn, in layout units. It is the same description the engine uses
/// for a node's shape: a `rect` with its width and height, or a `circle` with its diameter. The consumer owns colours,
/// borders, icons and content; the engine reads only this geometry.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Contour {
    Rect { width: f64, height: f64 },
    Circle { diameter: f64 },
}

impl Contour {
    /// The box the outline needs: a circle's diameter on both axes.
    pub fn size(&self) -> NodeSize {
        match *self {
            Self::Rect { width, height } => NodeSize { width, height },
            Self::Circle { diameter } => NodeSize {
                width: diameter,
                height: diameter,
            },
        }
    }

    /// Whether every extent is finite and positive.
    pub fn is_valid(&self) -> bool {
        let size = self.size();
        [size.width, size.height]
            .iter()
            .all(|v| v.is_finite() && *v > 0.)
    }

    /// Whether the outline is the kind of outline a node of `shape` has: a circle for a circle, a rectangle for a
    /// rectangle or a square.
    pub fn fits(&self, shape: NodeShape) -> bool {
        matches!(
            (self, shape),
            (Self::Circle { .. }, NodeShape::Circle)
                | (Self::Rect { .. }, NodeShape::Rect | NodeShape::Square)
        )
    }
}

/// A geometry observation for one element: the realized outline and the revision of the measurement. The revision rises with
/// every new measurement of that element, so that an old one is recognised when it arrives late.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub contour: Contour,
    pub geometry_revision: u64,
}

/// What the user decided about a view, as one value: the part of a snapshot that an undo or a redo restores. It holds the
/// decisions (pins, sizes, shapes, collapsed containers, direction, strategy and routing requirements), never the geometry
/// the layout derived from them. The engine reads it from a snapshot and restores it in one recomputation (`intent`,
/// `restore`); a host keeps a stack of such values and treats them as opaque. Provisional.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Intent {
    pub pins: BTreeMap<String, Point>,
    pub container_sizing: BTreeMap<String, ContainerSizing>,
    pub node_sizes: BTreeMap<String, NodeSize>,
    pub collapsed_ids: Vec<String>,
    /// The shape of every node that is not the default rectangle.
    pub shapes: BTreeMap<String, NodeShape>,
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "\"lr\" | \"rl\" | \"tb\" | \"bt\" | \"auto\" | Array<\"lr\" | \"rl\" | \"tb\" | \"bt\">"
        )
    )]
    pub default_direction: DirectionConstraint,
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ [key in string]: \"lr\" | \"rl\" | \"tb\" | \"bt\" | \"auto\" | Array<\"lr\" | \"rl\" | \"tb\" | \"bt\"> }"
        )
    )]
    pub container_directions: BTreeMap<String, DirectionConstraint>,
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "\"none\" | \"tree\" | \"radial\" | \"auto\" | Array<\"none\" | \"tree\" | \"radial\">"
        )
    )]
    pub default_strategy: StrategyConstraint,
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ [key in string]: \"none\" | \"tree\" | \"radial\" | \"auto\" | Array<\"none\" | \"tree\" | \"radial\"> }"
        )
    )]
    pub container_strategies: BTreeMap<String, StrategyConstraint>,
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "\"straight\" | \"orthogonal\" | \"roundedOrthogonal\" | \"polyline\" | \"roundedPolyline\" | \"octilinear\" | \"curve\" | \"bus\" | \"auto\" | Array<\"straight\" | \"orthogonal\" | \"roundedOrthogonal\" | \"polyline\" | \"roundedPolyline\" | \"octilinear\" | \"curve\" | \"bus\">"
        )
    )]
    pub default_routing: Option<RoutingConstraint>,
    #[cfg_attr(
        feature = "ts",
        ts(
            type = "{ [key in string]: \"straight\" | \"orthogonal\" | \"roundedOrthogonal\" | \"polyline\" | \"roundedPolyline\" | \"octilinear\" | \"curve\" | \"bus\" | \"auto\" | Array<\"straight\" | \"orthogonal\" | \"roundedOrthogonal\" | \"polyline\" | \"roundedPolyline\" | \"octilinear\" | \"curve\" | \"bus\"> }"
        )
    )]
    pub container_routing: BTreeMap<String, RoutingConstraint>,
    /// The ports the graph declares and the edge ends bound to them, which includes those a user placed by hand.
    pub ports: Vec<Port>,
    pub edge_ports: Vec<EdgePorts>,
    /// Where the user put labels.
    pub label_positions: BTreeMap<String, Point>,
    /// The orders the user chose by a drag.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub order_hints: BTreeMap<String, OrderHint>,
}

/// The authored size of one axis of a container (ADR 0010). Absent means content: the extent follows the children,
/// padding and header.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum AxisSize {
    /// At least this many layout units. The container grows with its children and never shrinks below it.
    Min(f64),
    /// Exactly this many layout units. Content that does not fit is a geometry conflict, never a silent resize.
    Fixed(f64),
}

impl AxisSize {
    fn value(self) -> f64 {
        match self {
            Self::Min(value) | Self::Fixed(value) => value,
        }
    }
}

/// The authored size of one container, per axis (ADR 0010). Each axis is content, a minimum or a fixed size.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ContainerSizing {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<AxisSize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<AxisSize>,
}

impl ContainerSizing {
    /// Whether both axes follow their content. Such an entry is equal to having none, so it is not stored.
    pub fn is_content(&self) -> bool {
        self.width.is_none() && self.height.is_none()
    }

    /// Every value that is set must be a positive, finite number of layout units.
    pub fn validate(&self) -> Result<(), String> {
        for axis in [self.width, self.height].into_iter().flatten() {
            let value = axis.value();
            if !(value.is_finite() && value > 0.) {
                return Err(format!(
                    "a container size must be a positive finite number, got {value}"
                ));
            }
        }
        Ok(())
    }
}

/// The extent of a container for content of `content` units (ADR 0010), and whether a fixed axis is smaller than its
/// content. A fixed axis keeps its value in both cases, so the conflict is reported and the container does not change.
pub fn container_extent(sizing: Option<&ContainerSizing>, content: NodeSize) -> (NodeSize, bool) {
    let resolve = |axis: Option<AxisSize>, content: f64| match axis {
        None => (content, false),
        Some(AxisSize::Min(min)) => (content.max(min), false),
        Some(AxisSize::Fixed(fixed)) => (fixed, content > fixed + 1e-9),
    };
    let (width, width_conflict) = resolve(sizing.and_then(|s| s.width), content.width);
    let (height, height_conflict) = resolve(sizing.and_then(|s| s.height), content.height);
    (
        NodeSize { width, height },
        width_conflict || height_conflict,
    )
}

/// Measured text rectangle. Hosts may supply exact font measurements.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LabelSpec {
    pub text: String,
    pub width: f64,
    pub height: f64,
}
/// The work of an edit that laid out and routed only part of the drawing (see `Snapshot::edit_report`).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct EditReport {
    /// Routes that were carried over from the earlier drawing, and routes that were made again.
    pub routes_kept: usize,
    pub routes_rerouted: usize,
    /// Routes that run through a node after the edit, and before it.
    pub routes_through_nodes: usize,
    pub routes_through_nodes_before: usize,
    /// The local result had more routes through nodes than the drawing before, and the complete layout was taken instead.
    #[serde(default, skip_serializing_if = "is_false")]
    pub complete_fallback: bool,
    /// A step of a drag: the result is a preview and may still have routes through nodes; the step that ends the gesture
    /// decides (and falls back to the complete layout if it must).
    #[serde(default, skip_serializing_if = "is_false")]
    pub preview: bool,
    /// After a drag in an organic area: the damped steps run to let the other nodes settle (`settle`), the largest distance
    /// any node moved in the last step, and whether that was below the threshold. Absent otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settle: Option<Settle>,
}

/// The outcome of the settling steps after a drag (see `EditReport::settle`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct Settle {
    pub steps: usize,
    pub residual: f64,
    pub converged: bool,
}

/// What a streamed publication of a refinement job is: a preview (a step of an edit that is not final), the best layout found so
/// far, or the final result of the job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "lowercase")]
pub enum PublicationKind {
    Preview,
    Best,
    Final,
}

/// The input a job read: the graph revision, the layout version it started from, and the canonical options.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PublicationInput {
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub graph_revision: u64,
    #[cfg_attr(feature = "ts", ts(type = "number"))]
    pub base_layout_version: u64,
    pub options_key: String,
}

/// The measured quality of a publication's snapshot (tiers of the shadow model; see `scene::quality`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PublicationScore {
    pub quality_version: u32,
    pub hard_violations: Option<usize>,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub shadow_total: Option<i64>,
}

/// One entry of the bounded archive of measured candidates at a publication (metadata only; the snapshot is not kept).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct ArchiveSummary {
    pub index: usize,
    pub strategy: String,
    #[cfg_attr(feature = "ts", ts(type = "Array<number>"))]
    pub key: Vec<i64>,
    pub geometry_fingerprint: String,
}

/// The typed envelope of a streamed publication. A publication belongs to one job and one input: a host shows it only while the
/// job is the one it waits for and the input is still current, takes the newest, and drops the rest as stale. The archive (the
/// search state) is kept apart from the snapshot that is shown.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub struct PublicationEnvelope {
    pub job_id: String,
    /// Position in the job's publications, starting at 1.
    pub sequence: usize,
    pub kind: PublicationKind,
    pub strategy: String,
    #[cfg_attr(feature = "ts", ts(type = "Array<number>"))]
    pub key: Vec<i64>,
    pub candidates_evaluated: usize,
    pub input: PublicationInput,
    pub snapshot: Snapshot,
    pub publication_version: u32,
    pub score: PublicationScore,
    pub archive: Vec<ArchiveSummary>,
}

/// The extra width of the corridor between two neighbouring siblings (see `Snapshot::corridor_gaps`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CorridorGap {
    pub before: String,
    pub after: String,
    pub extra: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PlacedLabel {
    pub id: String,
    pub kind: String,
    pub text: String,
    pub bounds: Rect,
}

/// Renderer settings never change graph topology or explicit port contracts.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct AppearanceConfig {
    pub show_ports: bool,
}
impl Default for AppearanceConfig {
    fn default() -> Self {
        Self { show_ports: true }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigSet {
    pub schema_version: u32,
    pub id: String,
    pub layout: LayoutConfig,
    pub routing: RoutingConfig,
    /// Display preference. Absent means the config set leaves the current display unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<AppearanceConfig>,
    /// Provisional (ADR 0018): how the cluster overview shows the data's clusters. Absent: no rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overview: Option<OverviewConfig>,
}

/// Provisional (ADR 0018, addendum): how a container of the cluster overview is shown.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
pub enum Representation {
    /// One node; its edges aggregated.
    Collapsed,
    /// Its children as discs, by force layout.
    Organic,
    /// Its children as boxes, by the box pipeline (layered, tree, radial).
    Structured,
}

/// Provisional (ADR 0018, addendum): the rules of the data for the cluster overview, the second step of the order
/// user, rules, zoom.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverviewConfig {
    /// The representation by node kind.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(
        feature = "ts",
        ts(optional, as = "Option<BTreeMap<String, Representation>>")
    )]
    pub rules: BTreeMap<String, Representation>,
    /// Whether the zoom decides what neither the user nor a rule decided. Absent: yes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub auto: Option<bool>,
    /// What is collapsed at first, after the user's decisions and the rules, before the zoom (the same value as
    /// `LayoutOptions::level_of_detail`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub level_of_detail: Option<LevelOfDetail>,
}

/// Provisional (ADR 0018, addendum; ADR 0015): the user's decisions in the cluster overview as one value, by node id.
/// The first step of the order user, rules, zoom; it always holds.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverviewIntent {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    #[cfg_attr(
        feature = "ts",
        ts(optional, as = "Option<BTreeMap<String, Representation>>")
    )]
    pub decisions: BTreeMap<String, Representation>,
}
/// Provisional (ADR 0018): the answer of the `overview` operation, `{ok, overview}`. Arrays are indexed by node: the
/// graph's nodes in input order, then the clusters the overview added (communities, satellite groups).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Overview {
    /// 1.
    pub schema_version: u32,
    /// The values of the order user, rules, zoom, as the engine checked them.
    pub representation: OverviewRepresentation,
    pub ids: Vec<String>,
    /// Each node's label from the data; an added cluster is named after its most connected member (`≈ <name>` for a
    /// community, `<name> +<n>` for a hub with its satellites).
    pub labels: Vec<String>,
    /// The parent's index, or -1 at the top level.
    #[cfg_attr(feature = "ts", ts(type = "number[]"))]
    pub parent: Vec<i64>,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// Disc radii: leaves share the leaf radius, a container holds its children.
    pub r: Vec<f64>,
    /// The radius of the disc around everything, centred on the origin.
    pub root_radius: f64,
    /// Node kinds in order of first use (`community` for added clusters), and each node's kind as an index into them.
    pub kinds: Vec<String>,
    #[cfg_attr(feature = "ts", ts(type = "number[]"))]
    pub kind: Vec<u32>,
    /// Resolved edges as index pairs, source first, in input order (flat).
    #[cfg_attr(feature = "ts", ts(type = "number[]"))]
    pub edges: Vec<u32>,
    /// Per container, the links between its children: child, child, number of edges (flat triples).
    pub links: Vec<f64>,
    pub stats: OverviewStats,
}

/// Provisional: what of some source-graph nodes and edges the current view shows (`trace`, `neighbourhood`): the visible
/// boxes that hold the nodes, and the routes that carry the edges.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VisibleParts {
    pub nodes: Vec<String>,
    pub routes: Vec<String>,
}

/// Provisional: the answer of `trace` (besides `ok`): the nodes and edges on the paths between two nodes of the source
/// graph, and what of them the view shows. `reversed`: no path from `from` to `to`, the paths are those back; `length`
/// is null without a path.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceAnswer {
    pub found: bool,
    pub reversed: bool,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub length: Option<usize>,
    pub nodes: Vec<String>,
    pub edges: Vec<String>,
    pub visible: VisibleParts,
    /// Hops of every visible node from the path through the drawn edges (for fading); unreachable nodes are left out.
    pub distance: BTreeMap<String, usize>,
}

/// Provisional: the answer of `neighbourhood` (besides `ok`): the nodes within some hops of nodes, the edges they were
/// reached by, each node's hops, and what of them the view shows (as for `trace`).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NeighbourhoodAnswer {
    pub nodes: Vec<String>,
    pub edges: Vec<String>,
    pub hops: BTreeMap<String, usize>,
    pub visible: VisibleParts,
    pub distance: BTreeMap<String, usize>,
}

/// Provisional (sessions): the part of an overview that changes while the clusters stay the same, answered for a request
/// with `positionsOnly` laid out on the clusters of the session's last overview. The host keeps ids, labels, parents,
/// kinds, edges and links from that answer.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverviewPositions {
    pub schema_version: u32,
    /// Always true: tells this answer from a whole `Overview`.
    pub positions_only: bool,
    pub representation: OverviewRepresentation,
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub r: Vec<f64>,
    pub root_radius: f64,
    pub stats: OverviewStats,
}

impl OverviewPositions {
    /// The moving part of a whole overview.
    pub fn of(overview: Overview) -> Self {
        Self {
            schema_version: overview.schema_version,
            positions_only: true,
            representation: overview.representation,
            x: overview.x,
            y: overview.y,
            r: overview.r,
            root_radius: overview.root_radius,
            stats: overview.stats,
        }
    }
}

/// Provisional: the paths between two nodes of a graph (`overviewTrace`), without a view: the overview's viewer maps them
/// to its discs. `reversed`: no path from `from` to `to`, the paths are those back; `length` is null without a path.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphTrace {
    pub found: bool,
    pub reversed: bool,
    #[cfg_attr(feature = "ts", ts(type = "number | null"))]
    pub length: Option<usize>,
    pub nodes: Vec<String>,
    pub edges: Vec<String>,
}

/// Provisional: the nodes within some hops of nodes of a graph (`overviewNeighbourhood`), without a view.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GraphNeighbourhood {
    pub nodes: Vec<String>,
    pub edges: Vec<String>,
    pub hops: BTreeMap<String, usize>,
}

/// Provisional: the counts of an overview.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverviewStats {
    pub nodes: u32,
    pub containers: u32,
    pub communities: u32,
    pub levels: u32,
    pub edges: u32,
    pub unresolved_edges: u32,
    pub overlaps: u32,
}

/// Provisional (ADR 0018, addendum): the representation values of an overview request, as the engine checked them.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverviewRepresentation {
    /// The user's decisions by node id.
    pub decisions: BTreeMap<String, Representation>,
    /// The rules by node kind.
    pub rules: BTreeMap<String, Representation>,
    /// Whether the zoom decides what neither a decision nor a rule does.
    pub auto: bool,
    /// Rule kinds the graph does not have (reported, not refused: a config set serves many graphs).
    pub unknown_kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub level_of_detail: Option<LevelOfDetail>,
}

/// Provisional (ADR 0018): a request of the `overview` operation. In a session `graph` may be left out after the
/// first request, and `start` without positions continues from the last overview.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverviewRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub graph: Option<Graph>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub options: Option<OverviewLayoutOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub intent: Option<OverviewIntent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub config_set: Option<ConfigSet>,
    /// Radii by node id, for clusters a host shows structured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub radii: Option<BTreeMap<String, f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub start: Option<OverviewStart>,
    /// Provisional (sessions): answer only what moves (`OverviewPositions`) when the clusters are those of the session's
    /// last overview; otherwise the whole overview.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub positions_only: Option<bool>,
    /// Worker threads (native only); the answer is the same for any number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "number"))]
    pub threads: Option<u32>,
}

/// Provisional: the layout options of an overview (defaults in brackets).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverviewLayoutOptions {
    /// The radius of a leaf (4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub node_radius: Option<f64>,
    /// The gap between discs (2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub gap: Option<f64>,
    /// The padding inside a container (6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub padding: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub charge: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub gravity: Option<f64>,
    /// Children above which a container is divided into communities (120; 0: never).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "number"))]
    pub communities: Option<u32>,
    /// The share of a hub's links a satellite must have to be gathered with it (0: never; at most 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub satellites: Option<f64>,
    /// Chains of single children share one disc (false).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub chains: Option<bool>,
    /// Force steps per container (1..=10000).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional, type = "number"))]
    pub ticks: Option<u32>,
    /// Groups the data gives: node id → path of group names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub groups: Option<BTreeMap<String, Vec<String>>>,
    /// Children left out of Louvain (hubs), as `LayoutOptions::community_hubs`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub hubs: Option<CommunityHubs>,
}

/// Provisional: where an overview continues from. Positions (`x`, `y`, `r`) of a previous overview of the same
/// clusters, or none in a session (its last overview); `settle` lets the changed containers settle again.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OverviewStart {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub x: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub y: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub r: Option<Vec<f64>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional))]
    pub settle: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LayoutConfig {
    pub algorithm: String,
    pub candidates: Vec<usize>,
    pub hybrid_by: HybridCriterion,
    #[serde(default)]
    pub min_port_spacing: f64,
    #[serde(default)]
    pub placement_grid: f64,
    #[serde(default)]
    pub free_placement: bool,
    #[serde(default)]
    pub placement_search: usize,
    #[serde(default)]
    pub port_placement: PortPlacement,
    /// Experimental flat-region candidate (A4b): nodes placed on the drawing grid of their planar skeleton. Only for
    /// flat, biconnected views; off by default and omitted when off.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flat_region: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RoutingConfig {
    pub style: String,
    pub bundling: Bundling,
    pub routing_candidate: bool,
    #[serde(default)]
    pub clearance: f64,
    #[serde(default = "default_track_spacing")]
    pub track_spacing: f64,
    #[serde(default)]
    pub corridors: bool,
    #[serde(default)]
    pub track_separation: bool,
    #[serde(default)]
    pub engine: RoutingEngine,
}
fn default_track_spacing() -> f64 {
    6.
}
impl ConfigSet {
    pub fn from_options(id: String, options: &LayoutOptions) -> Self {
        Self {
            schema_version: 1,
            id,
            layout: LayoutConfig {
                algorithm: "hierarchical".into(),
                candidates: options.candidates.clone(),
                hybrid_by: options.hybrid_by,
                min_port_spacing: options.min_port_spacing,
                placement_grid: options.placement_grid,
                free_placement: options.free_placement,
                placement_search: options.placement_search,
                port_placement: options.port_placement,
                flat_region: options.flat_region,
            },
            routing: RoutingConfig {
                style: "orthogonal".into(),
                bundling: options.bundling,
                routing_candidate: options.routing_candidate,
                clearance: options.routing_clearance,
                track_spacing: options.track_spacing,
                corridors: options.routing_corridors,
                track_separation: options.routing_track_separation,
                engine: options.routing_engine,
            },
            appearance: Some(options.appearance.clone()),
            overview: None,
        }
    }
    pub fn apply(&self, options: &mut LayoutOptions) -> Result<(), String> {
        let config_id = self.id.clone();
        self.apply_values(options)?;
        options.config_id = Some(config_id);
        Ok(())
    }
    fn apply_values(&self, options: &mut LayoutOptions) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("Unsupported config set version".into());
        }
        if self.id.trim().is_empty() {
            return Err("Config set id must not be empty".into());
        }
        if self.layout.algorithm != "hierarchical" {
            return Err("Unsupported layout algorithm".into());
        }
        if self.routing.style != "orthogonal" {
            return Err("Unsupported routing style".into());
        }
        let unique: BTreeSet<_> = self.layout.candidates.iter().collect();
        if unique.len() != self.layout.candidates.len()
            || self.layout.candidates.iter().any(|m| !(1..=4).contains(m))
        {
            return Err("Candidate modes must be unique and within 1..=4".into());
        }
        options.candidates = self.layout.candidates.clone();
        options.hybrid_by = self.layout.hybrid_by;
        for value in [self.layout.min_port_spacing, self.layout.placement_grid] {
            if !value.is_finite() || !(0.0..=1000.0).contains(&value) {
                return Err("Spacing and grid must be finite values between 0 and 1000".into());
            }
        }
        options.free_placement = self.layout.free_placement;
        if self.layout.placement_search > 1000 {
            return Err("Placement search budget must be at most 1000".into());
        }
        options.placement_search = self.layout.placement_search;
        options.port_placement = self.layout.port_placement;
        options.flat_region = self.layout.flat_region;
        options.min_port_spacing = self.layout.min_port_spacing;
        options.placement_grid = self.layout.placement_grid;
        if !self.routing.clearance.is_finite()
            || !(0.0..=1000.0).contains(&self.routing.clearance)
            || !self.routing.track_spacing.is_finite()
            || !(0.0..=1000.0).contains(&self.routing.track_spacing)
        {
            return Err(
                "Clearance and track spacing must be finite values between 0 and 1000".into(),
            );
        }
        options.routing_clearance = self.routing.clearance;
        options.track_spacing = self.routing.track_spacing;
        options.routing_corridors = self.routing.corridors;
        options.routing_track_separation = self.routing.track_separation;
        options.routing_engine = self.routing.engine;
        options.bundling = self.routing.bundling;
        options.routing_candidate = self.routing.routing_candidate;
        if let Some(appearance) = &self.appearance {
            options.appearance = appearance.clone();
        }
        Ok(())
    }
}

#[cfg(test)]
mod container_direction_tests {
    use super::*;

    fn graph(json: &str) -> Result<Graph, serde_json::Error> {
        serde_json::from_str(json)
    }

    fn chain(extra: &str) -> Graph {
        graph(&format!(
            r#"{{{extra},
                "nodes":[{{"id":"outer","kind":"container","label":"o","parentId":null}},
                         {{"id":"mid","kind":"container","label":"m","parentId":"outer"}},
                         {{"id":"leaf","kind":"symbol","label":"l","parentId":"mid"}}],
                "edges":[]}}"#
        ))
        .unwrap()
    }

    /// Direction and strategy forms parse from their contract spellings and serialize back to them.
    #[test]
    fn the_forms_round_trip_through_the_contract_format() {
        let g = chain(
            r#""defaultDirection":"rl","containerDirections":{"outer":"tb","mid":["lr","bt"]},"defaultStrategy":"auto","containerStrategies":{"outer":"tree","mid":["tree","radial"]}"#,
        );
        assert_eq!(
            g.default_direction,
            DirectionConstraint::Fixed(Orientation::Rl)
        );
        assert_eq!(
            g.container_directions["mid"],
            DirectionConstraint::Allowed(vec![Orientation::Lr, Orientation::Bt])
        );
        assert_eq!(g.default_strategy, StrategyConstraint::Auto);
        assert_eq!(
            g.container_strategies["outer"],
            StrategyConstraint::Fixed(LayoutStrategy::Tree)
        );
        assert_eq!(
            serde_json::to_string(&g.container_strategies).unwrap(),
            r#"{"mid":["tree","radial"],"outer":"tree"}"#
        );
    }

    /// Legacy values in the direction slot keep their meaning: a strategy name there is a free direction with that strategy.
    #[test]
    fn legacy_strategy_names_in_the_direction_slot_are_read_as_strategies() {
        let g = chain(
            r#""defaultDirection":"hybrid","containerDirections":{"outer":"shortest","mid":"tb"}"#,
        );
        assert_eq!(g.default_direction, DirectionConstraint::Free);
        assert_eq!(g.default_strategy, StrategyConstraint::default());
        assert_eq!(g.container_directions["outer"], DirectionConstraint::Free);
        assert_eq!(
            g.container_strategies["outer"],
            StrategyConstraint::Fixed(LayoutStrategy::Shortest)
        );
        assert_eq!(
            g.container_directions["mid"],
            DirectionConstraint::Fixed(Orientation::Tb)
        );
    }

    /// Unknown values, empty lists and duplicates are refused, not replaced.
    #[test]
    fn malformed_constraints_are_refused() {
        for bad in [
            r#"{"defaultDirection":"sideways","nodes":[],"edges":[]}"#,
            r#"{"containerDirections":{"a":[]},"nodes":[],"edges":[]}"#,
            r#"{"containerDirections":{"a":["tb","tb"]},"nodes":[],"edges":[]}"#,
            r#"{"defaultStrategy":"sideways","nodes":[],"edges":[]}"#,
            r#"{"containerStrategies":{"a":["tree","tree"]},"nodes":[],"edges":[]}"#,
            r#"{"containerDirections":{"a":"tb"},"containerStrategies":{"a":"tree"},"nodes":[],"edges":[]}"#,
        ] {
            assert!(graph(bad).is_err() || bad.contains("\"a\":\"tb\""), "{bad}");
        }
    }

    /// Each axis inherits separately; the nearest declared entry on each axis wins.
    #[test]
    fn each_axis_inherits_separately() {
        let g =
            chain(r#""containerDirections":{"outer":"tb"},"containerStrategies":{"mid":"tree"}"#);
        assert_eq!(
            g.container_constraint(Some("leaf")),
            DirectionConstraint::Fixed(Orientation::Tb)
        );
        assert_eq!(
            g.strategy_constraint(Some("leaf")),
            StrategyConstraint::Fixed(LayoutStrategy::Tree)
        );
        assert_eq!(
            g.strategy_constraint(Some("outer")),
            StrategyConstraint::default()
        );
    }

    /// The mode follows both axes: a tree takes the direction, radial needs a free direction, and a chooser needs a free
    /// or allowed direction.
    #[test]
    fn the_mode_combines_direction_and_strategy_and_refuses_incompatible_pairs() {
        let tree =
            chain(r#""containerDirections":{"outer":"tb"},"containerStrategies":{"mid":"tree"}"#);
        assert_eq!(tree.area_mode(Some("mid")), Ok(Mode::Tree(Orientation::Tb)));
        let radial_fixed =
            chain(r#""containerDirections":{"outer":"tb"},"containerStrategies":{"mid":"radial"}"#);
        assert!(radial_fixed
            .area_mode(Some("mid"))
            .unwrap_err()
            .contains("fixed direction"));
        let radial_free = chain(
            r#""containerDirections":{"outer":"auto"},"containerStrategies":{"mid":"radial"}"#,
        );
        assert_eq!(radial_free.area_mode(Some("mid")), Ok(Mode::Radial));
        let chooser = chain(r#""defaultDirection":"auto""#);
        assert_eq!(
            chooser.area_mode(Some("mid")),
            Ok(Mode::Chooser(Chooser::Compact))
        );
        let chooser_fixed =
            chain(r#""containerDirections":{"mid":"lr"},"containerStrategies":{"mid":"radial"}"#);
        assert!(chooser_fixed.area_mode(Some("mid")).is_err());
        let plain = chain(r#""containerDirections":{"outer":"tb"}"#);
        assert_eq!(
            plain.area_mode(Some("mid")),
            Ok(Mode::Orientation(Orientation::Tb))
        );
    }

    /// Auto strategy has no search in the default layout (`none`); it offers every strategy as a candidate to the search.
    #[test]
    fn auto_strategy_is_the_default_layout_with_every_candidate_for_the_search() {
        assert_eq!(
            StrategyConstraint::Auto.default_choice(),
            LayoutStrategy::None
        );
        assert_eq!(StrategyConstraint::Auto.candidates().len(), 4);
        assert_eq!(DirectionConstraint::Free.default_choice(), Orientation::Lr);
        assert_eq!(
            DirectionConstraint::Free.candidates(),
            vec![Orientation::Lr, Orientation::Tb]
        );
        assert!(DirectionConstraint::Fixed(Orientation::Tb)
            .candidates()
            .is_empty());
    }

    /// The reversed orientations mirror the forward ones on their own axis.
    #[test]
    fn reversed_orientations_are_the_mirror_of_the_forward_ones() {
        assert!(Orientation::Rl.reversed() && !Orientation::Rl.vertical());
        assert!(Orientation::Bt.reversed() && Orientation::Bt.vertical());
        assert!(!Orientation::Lr.reversed() && !Orientation::Tb.reversed());
    }
}

#[cfg(test)]
mod routing_constraint_tests {
    use super::*;

    fn graph(json: &str) -> Graph {
        serde_json::from_str(json).unwrap()
    }

    const NESTED: &str = r#"{"nodes":[
        {"id":"root","kind":"group","label":"R","parentId":null},
        {"id":"box","kind":"group","label":"B","parentId":"root"},
        {"id":"a","kind":"node","label":"A","parentId":"box"},
        {"id":"b","kind":"node","label":"B","parentId":"box"},
        {"id":"c","kind":"node","label":"C","parentId":"root"}],
        "edges":[{"id":"ab","kind":"flow","source":"a","target":"b"},{"id":"ac","kind":"flow","source":"a","target":"c"}],
        "defaultRouting":"straight",
        "containerRouting":{"box":"orthogonal"}}"#;

    /// An edge is drawn in the deepest container that holds both endpoints; an edge from a container to its own child is
    /// drawn in the container's parent.
    #[test]
    fn an_edge_is_drawn_in_the_deepest_common_container() {
        let g = graph(NESTED);
        assert_eq!(g.edge_area("a", "b").as_deref(), Some("box"));
        assert_eq!(g.edge_area("a", "c").as_deref(), Some("root"));
        assert_eq!(g.edge_area("box", "a").as_deref(), Some("root"));
    }

    /// An explicit root `orthogonal` is a declaration: it is written, and it stays declared after a round trip.
    #[test]
    fn an_explicit_default_style_survives_a_round_trip() {
        let g = graph(
            r#"{"nodes":[{"id":"a","kind":"node","label":"A","parentId":null}],"edges":[],"defaultRouting":"orthogonal"}"#,
        );
        assert_eq!(
            g.default_routing,
            Some(RoutingConstraint::Fixed(RoutingStyle::Orthogonal))
        );
        let text = serde_json::to_string(&g).unwrap();
        assert!(text.contains("\"defaultRouting\":\"orthogonal\""), "{text}");
        let back: Graph = serde_json::from_str(&text).unwrap();
        assert_eq!(back.default_routing, g.default_routing);
    }

    /// A document that declares nothing writes nothing; a missing root entry stays missing.
    #[test]
    fn omission_remains_omission() {
        let plain =
            graph(r#"{"nodes":[{"id":"a","kind":"node","label":"A","parentId":null}],"edges":[]}"#);
        assert_eq!(plain.default_routing, None);
        let text = serde_json::to_string(&plain).unwrap();
        assert!(!text.contains("Routing"), "{text}");
    }

    /// A container inherits the nearest authored routing; a container's own entry overrides; nothing authored is `None`.
    #[test]
    fn containers_inherit_the_nearest_authored_routing() {
        let g = graph(NESTED);
        assert_eq!(
            g.routing_constraint(Some("box")),
            Some(RoutingConstraint::Fixed(RoutingStyle::Orthogonal))
        );
        assert_eq!(
            g.routing_constraint(Some("other")),
            Some(RoutingConstraint::Fixed(RoutingStyle::Straight))
        );
        assert_eq!(
            g.routing_constraint(None),
            Some(RoutingConstraint::Fixed(RoutingStyle::Straight))
        );
        let bare = graph(
            r#"{"nodes":[{"id":"g","kind":"group","label":"G","parentId":null},{"id":"a","kind":"node","label":"A","parentId":"g"}],"edges":[]}"#,
        );
        assert_eq!(bare.routing_constraint(Some("g")), None);
    }

    /// Malformed values are refused by the parser: unknown names, empty and repeated lists.
    #[test]
    fn malformed_routing_values_are_refused() {
        assert!(parse_routing(&serde_json::json!("zigzag")).is_err());
        assert!(parse_routing(&serde_json::json!([])).is_err());
        assert!(parse_routing(&serde_json::json!(["straight", "straight"])).is_err());
    }

    /// Admission: a fixed style admits only itself, auto admits all, a list its members.
    #[test]
    fn admission_follows_the_form() {
        assert!(RoutingConstraint::Fixed(RoutingStyle::Orthogonal).admits(RoutingStyle::Orthogonal));
        assert!(!RoutingConstraint::Fixed(RoutingStyle::Orthogonal).admits(RoutingStyle::Straight));
        assert!(RoutingConstraint::Auto.admits(RoutingStyle::Straight));
        assert!(
            RoutingConstraint::Allowed(vec![RoutingStyle::Straight, RoutingStyle::Orthogonal])
                .admits(RoutingStyle::Straight)
        );
    }
}

#[cfg(test)]
mod container_sizing_tests {
    use super::*;

    fn content(width: f64, height: f64) -> NodeSize {
        NodeSize { width, height }
    }

    #[test]
    fn default_options_do_not_store_container_sizing() {
        let value = serde_json::to_value(LayoutOptions::default()).unwrap();
        assert!(value.get("containerSizing").is_none(), "{value}");
    }

    #[test]
    fn vpsc_overlap_removal_is_opt_in_and_round_trips() {
        let defaults = serde_json::to_value(LayoutOptions::default()).unwrap();
        assert!(defaults.get("vpscOverlapRemoval").is_none(), "{defaults}");

        let options: LayoutOptions =
            serde_json::from_value(serde_json::json!({"vpscOverlapRemoval": true})).unwrap();
        assert!(options.vpsc_overlap_removal);
        assert_eq!(
            serde_json::to_value(options).unwrap()["vpscOverlapRemoval"],
            true
        );
    }

    #[test]
    fn order_seed_is_omitted_at_zero_and_round_trips() {
        let defaults = serde_json::to_value(LayoutOptions::default()).unwrap();
        assert!(defaults.get("orderSeed").is_none(), "{defaults}");
        let options: LayoutOptions =
            serde_json::from_value(serde_json::json!({"orderSeed": 3})).unwrap();
        assert_eq!(options.order_seed, 3);
        assert_eq!(serde_json::to_value(options).unwrap()["orderSeed"], 3);
    }

    #[test]
    fn size_aware_stress_is_opt_in_and_round_trips() {
        let defaults = serde_json::to_value(LayoutOptions::default()).unwrap();
        assert!(defaults.get("sizeAwareStress").is_none(), "{defaults}");
        let options: LayoutOptions =
            serde_json::from_value(serde_json::json!({"sizeAwareStress": true})).unwrap();
        assert!(options.size_aware_stress);
        assert_eq!(
            serde_json::to_value(options).unwrap()["sizeAwareStress"],
            true
        );
    }

    #[test]
    fn scalable_stress_is_opt_in_and_round_trips() {
        let defaults = serde_json::to_value(LayoutOptions::default()).unwrap();
        assert!(defaults.get("scalableStress").is_none(), "{defaults}");
        let options: LayoutOptions =
            serde_json::from_value(serde_json::json!({"scalableStress": true})).unwrap();
        assert!(options.scalable_stress);
        assert_eq!(
            serde_json::to_value(options).unwrap()["scalableStress"],
            true
        );
    }

    #[test]
    fn sparse_stress_is_opt_in_and_round_trips() {
        let defaults = serde_json::to_value(LayoutOptions::default()).unwrap();
        assert!(defaults.get("sparseStress").is_none(), "{defaults}");

        let options: LayoutOptions =
            serde_json::from_value(serde_json::json!({"sparseStress": true})).unwrap();
        assert!(options.sparse_stress);
        assert_eq!(serde_json::to_value(options).unwrap()["sparseStress"], true);
    }

    #[test]
    fn a_stored_minimum_round_trips_per_axis() {
        let json = r#"{"width":{"min":240},"height":{"fixed":300}}"#;
        let sizing: ContainerSizing = serde_json::from_str(json).unwrap();
        assert_eq!(sizing.width, Some(AxisSize::Min(240.)));
        assert_eq!(sizing.height, Some(AxisSize::Fixed(300.)));
        assert_eq!(
            serde_json::to_value(&sizing).unwrap(),
            serde_json::json!({"width": {"min": 240.}, "height": {"fixed": 300.}})
        );
    }

    #[test]
    fn an_absent_axis_follows_its_content() {
        let sizing = ContainerSizing {
            width: Some(AxisSize::Min(240.)),
            height: None,
        };
        let (extent, conflict) = container_extent(Some(&sizing), content(100., 80.));
        assert_eq!((extent.width, extent.height, conflict), (240., 80., false));
    }

    #[test]
    fn a_minimum_grows_but_never_shrinks_the_container() {
        let min = ContainerSizing {
            width: Some(AxisSize::Min(150.)),
            height: Some(AxisSize::Min(50.)),
        };
        let (grown, conflict) = container_extent(Some(&min), content(100., 80.));
        assert_eq!((grown.width, grown.height, conflict), (150., 80., false));
    }

    #[test]
    fn a_fixed_extent_is_kept_and_a_small_one_is_a_conflict() {
        let fixed = ContainerSizing {
            width: Some(AxisSize::Fixed(150.)),
            height: Some(AxisSize::Fixed(50.)),
        };
        let (extent, conflict) = container_extent(Some(&fixed), content(100., 80.));
        assert_eq!((extent.width, extent.height), (150., 50.));
        assert!(conflict, "the height is below the content");
        let (fits, no_conflict) = container_extent(Some(&fixed), content(100., 40.));
        assert_eq!((fits.width, fits.height, no_conflict), (150., 50., false));
    }

    #[test]
    fn invalid_sizes_are_rejected_and_content_entries_are_dropped() {
        for bad in [0., -1., f64::INFINITY, f64::NAN] {
            let sizing = ContainerSizing {
                width: Some(AxisSize::Min(bad)),
                height: None,
            };
            assert!(sizing.validate().is_err(), "{bad} must be rejected");
        }
        let mut options = LayoutOptions::default();
        options
            .container_sizing
            .insert("content".into(), ContainerSizing::default());
        options.container_sizing.insert(
            "fixed".into(),
            ContainerSizing {
                width: Some(AxisSize::Fixed(10.)),
                height: None,
            },
        );
        options.normalize_container_sizing();
        assert_eq!(
            options.container_sizing.keys().collect::<Vec<_>>(),
            vec!["fixed"]
        );
        assert!(options.validate_container_sizing().is_ok());
    }
}

#[cfg(test)]
mod eligibility_tests {
    use super::*;

    fn issue(kind: &str, node_ids: &[&str]) -> GeometryIssue {
        GeometryIssue {
            kind: kind.into(),
            node_ids: node_ids.iter().map(|id| id.to_string()).collect(),
            message: String::new(),
        }
    }

    #[test]
    fn overlaps_inside_a_conflicted_fixed_container_are_authored_and_others_are_not() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure/nested.json")).unwrap(),
        )
        .unwrap();
        // `inner` holds `a` and `b`; `m` is its sibling in `outer`.
        let sizing = BTreeMap::from([(
            "inner".to_string(),
            ContainerSizing {
                width: Some(AxisSize::Fixed(20.)),
                height: None,
            },
        )]);
        let conflict = issue("container-fixed-below-content", &["inner"]);
        // A child of the small box is pushed out of it and overlaps its neighbour: a consequence of the fixed size.
        assert!(!is_ineligible_geometry(
            &[conflict.clone(), issue("node_overlap", &["m", "a"])],
            &graph,
            &sizing,
        ));
        // Two nodes outside the conflicted container are not.
        assert!(is_ineligible_geometry(
            &[conflict, issue("node_overlap", &["x", "y"])],
            &graph,
            &sizing,
        ));
        // Without any authored conflict, every issue makes the layout ineligible.
        assert!(is_ineligible_geometry(
            &[issue("node_overlap", &["m", "a"])],
            &graph,
            &BTreeMap::new(),
        ));
    }
}
