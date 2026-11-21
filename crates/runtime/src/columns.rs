//! Provisional (ADR 0019, step 4): the scene of a session as columns instead of objects. Every node and route gets a dense
//! `u32` index (its position in the scene's order); the string ids are kept once in a table, and the geometry is struct
//! of arrays: `f32` rectangles and route points, `u32` indices and offsets. A host in the browser reads the columns as
//! typed-array views into the engine's memory; they stay valid until the session changes.
//!
//! The columns are filled in the scene's order, so they are identical for any thread count and on every platform.
//! Coordinates are narrowed to `f32` (drawing precision, as the overview's buffers); the scene's JSON keeps `f64`.
use arqlet_scene::Scene;
use std::collections::HashMap;

/// Marks "no node" in an index column (a route end that is not a node of the scene, a node without a parent).
pub const NONE: u32 = u32::MAX;

/// Bits of the `nodeFlags` column.
pub const FLAG_CONTAINER: u32 = 1;
pub const FLAG_AGGREGATE: u32 = 2;
pub const FLAG_COLLAPSED: u32 = 4;
pub const FLAG_INVALID: u32 = 8;
/// Bits of the `routeFlags` column.
pub const ROUTE_INVALID: u32 = 1;
pub const ROUTE_ROUNDED: u32 = 2;
/// Bits of `portFlags`: the port was authored, and it is the target end (the source end otherwise).
pub const PORT_EXPLICIT: u32 = 1;
pub const PORT_TARGET: u32 = 2;
/// Values of the `nodeShape` column.
pub const SHAPE_RECT: u32 = 0;
pub const SHAPE_SQUARE: u32 = 1;
pub const SHAPE_CIRCLE: u32 = 2;
pub const SHAPE_BUS_LINE: u32 = 3;
/// Values of the side columns (`portSide`, `crossingSide`).
pub const SIDE_LEFT: u32 = 0;
pub const SIDE_RIGHT: u32 = 1;
pub const SIDE_TOP: u32 = 2;
pub const SIDE_BOTTOM: u32 = 3;

fn side_code(side: arqlet_model::PortSide) -> u32 {
    match side {
        arqlet_model::PortSide::Left => SIDE_LEFT,
        arqlet_model::PortSide::Right => SIDE_RIGHT,
        arqlet_model::PortSide::Top => SIDE_TOP,
        arqlet_model::PortSide::Bottom => SIDE_BOTTOM,
    }
}

/// Strings kept once, numbered in the order they first occur.
#[derive(Default)]
struct Table {
    items: Vec<String>,
    index: HashMap<String, u32>,
}

impl Table {
    fn id(&mut self, text: &str) -> u32 {
        if let Some(&i) = self.index.get(text) {
            return i;
        }
        let i = self.items.len() as u32;
        self.items.push(text.to_string());
        self.index.insert(text.to_string(), i);
        i
    }
}

/// One column, borrowed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Column<'a> {
    F32(&'a [f32]),
    U32(&'a [u32]),
    I32(&'a [i32]),
    F64(&'a [f64]),
}

impl Column<'_> {
    /// Number of elements.
    pub fn len(&self) -> usize {
        match self {
            Column::F32(c) => c.len(),
            Column::U32(c) => c.len(),
            Column::I32(c) => c.len(),
            Column::F64(c) => c.len(),
        }
    }

    /// Size of the column's memory in bytes.
    pub fn bytes(&self) -> usize {
        self.len()
            * match self {
                Column::F32(_) | Column::U32(_) | Column::I32(_) => 4,
                Column::F64(_) => 8,
            }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Address of the first element and the element type (`"f32"`, `"u32"`, `"i32"` or `"f64"`), for a host that views
    /// the memory.
    pub fn raw(&self) -> (*const u8, &'static str) {
        match self {
            Column::F32(c) => (c.as_ptr() as *const u8, "f32"),
            Column::U32(c) => (c.as_ptr() as *const u8, "u32"),
            Column::I32(c) => (c.as_ptr() as *const u8, "i32"),
            Column::F64(c) => (c.as_ptr() as *const u8, "f64"),
        }
    }
}

/// The scene of one revision and layout version, as columns.
#[derive(Clone, Debug, Default)]
pub struct SceneColumns {
    pub schema_version: u32,
    pub revision: u64,
    pub layout_version: u64,
    /// Node ids by node index.
    pub node_ids: Vec<String>,
    /// Absolute bounds, four per node: x, y, width, height.
    pub node_bounds: Vec<f32>,
    /// Index of the parent node, `NONE` for a root (or a parent outside the scene).
    pub node_parent: Vec<u32>,
    pub node_flags: Vec<u32>,
    /// Each node's label, its kind (index into `node_kinds`) and its shape (`SHAPE_*`).
    pub node_labels: Vec<String>,
    pub node_kinds: Vec<String>,
    pub node_kind: Vec<u32>,
    pub node_shape: Vec<u32>,
    /// Provisional: each node's `SceneNode::child_count` and `hidden_count`, `NONE` when absent.
    pub node_child_count: Vec<u32>,
    pub node_hidden_count: Vec<u32>,
    /// Route ids by route index.
    pub route_ids: Vec<String>,
    /// Node indices of each route's ends, `NONE` when the end is not a node of the scene.
    pub route_source: Vec<u32>,
    pub route_target: Vec<u32>,
    pub route_weight: Vec<u32>,
    pub route_flags: Vec<u32>,
    /// Each route's kind (index into `route_kinds`; "" when it has none), the edges it stands for in the other direction
    /// (`NONE` when it is not a two-way route), and the edges of a bundle (indices into `edge_ids`, behind
    /// `route_member_offsets`, route count + 1).
    pub route_kinds: Vec<String>,
    pub route_kind: Vec<u32>,
    pub route_back_weight: Vec<u32>,
    pub route_member_offsets: Vec<u32>,
    pub route_members: Vec<u32>,
    /// `route_count + 1` offsets into `route_points`, counted in points: route `i` is points `offsets[i]..offsets[i+1]`.
    pub route_offsets: Vec<u32>,
    /// All route points in one array, x and y alternating.
    pub route_points: Vec<f32>,
    /// Port ids by port index; the other port columns are indexed alike.
    pub port_ids: Vec<String>,
    /// The port's node (index, `NONE` if not in the scene) and its edge (index into `edge_ids`).
    pub port_node: Vec<u32>,
    pub port_edge: Vec<u32>,
    /// Bits of `PORT_*`, and the side (`SIDE_*`).
    pub port_flags: Vec<u32>,
    pub port_side: Vec<u32>,
    /// x and y alternating.
    pub port_position: Vec<f32>,
    /// The edge ids that ports and boundary crossings name, each once, in the order they first occur.
    pub edge_ids: Vec<String>,
    /// Boundary crossings: edge (into `edge_ids`), container (node index), ordinal, side, flags (bit 0: derived), and
    /// position (x and y alternating).
    pub crossing_edge: Vec<u32>,
    pub crossing_container: Vec<u32>,
    pub crossing_ordinal: Vec<u32>,
    pub crossing_side: Vec<u32>,
    pub crossing_flags: Vec<u32>,
    pub crossing_position: Vec<f32>,
    /// Placed labels: id and text by label index, the kind (index into `label_kinds`), and absolute bounds (four per label).
    pub label_ids: Vec<String>,
    pub label_texts: Vec<String>,
    pub label_kinds: Vec<String>,
    pub label_kind: Vec<u32>,
    pub label_bounds: Vec<f32>,
    /// Rows of the nodes' content, node by node: id and label by row index, the node (index), absolute bounds (four per row).
    pub row_ids: Vec<String>,
    pub row_labels: Vec<String>,
    pub row_node: Vec<u32>,
    pub row_bounds: Vec<f32>,
    /// Conflicts: kind (index into `conflict_kinds`) and reason by conflict index; the nodes and routes each names as
    /// indices (`NONE` for one outside the scene), as offsets (conflict count + 1) into the index arrays.
    pub conflict_kinds: Vec<String>,
    pub conflict_reasons: Vec<String>,
    pub conflict_kind: Vec<u32>,
    pub conflict_node_offsets: Vec<u32>,
    pub conflict_nodes: Vec<u32>,
    pub conflict_route_offsets: Vec<u32>,
    pub conflict_routes: Vec<u32>,
    /// Where bus lines branch: x and y alternating.
    pub junctions: Vec<f32>,
    /// The extent of everything drawn (x, y, width, height) and whether ports are shown, as in the scene.
    pub bounds: [f64; 4],
    pub show_ports: bool,
    /// The explicit buses, each as its edges' ids.
    pub buses: Vec<Vec<String>>,
    /// Provisional: the bus lines, as the scene has them (few, so kept as they are).
    pub bus_lines: Vec<arqlet_scene::SceneBusLine>,
    /// Provisional: the hub references, as the scene has them.
    pub hub_refs: Vec<arqlet_scene::SceneHubRef>,
}

impl SceneColumns {
    pub fn from_scene(scene: &Scene) -> Self {
        let index: HashMap<&str, u32> = scene
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.node.id.as_str(), i as u32))
            .collect();
        let at = |id: &str| index.get(id).copied().unwrap_or(NONE);
        let mut c = SceneColumns {
            schema_version: scene.schema_version,
            revision: scene.revision,
            layout_version: scene.layout_version,
            ..Default::default()
        };
        let mut node_kinds = Table::default();
        for n in &scene.nodes {
            c.node_ids.push(n.node.id.clone());
            c.node_labels.push(n.node.label.clone());
            c.node_kind.push(node_kinds.id(&n.node.kind));
            c.node_shape.push(match n.node.shape {
                arqlet_model::NodeShape::Rect => SHAPE_RECT,
                arqlet_model::NodeShape::Square => SHAPE_SQUARE,
                arqlet_model::NodeShape::Circle => SHAPE_CIRCLE,
                arqlet_model::NodeShape::BusLine => SHAPE_BUS_LINE,
            });
            let count = |v: Option<usize>| v.map_or(NONE, |v| u32::try_from(v).unwrap_or(NONE - 1));
            c.node_child_count.push(count(n.child_count));
            c.node_hidden_count.push(count(n.hidden_count));
            let b = &n.bounds;
            c.node_bounds
                .extend([b.x as f32, b.y as f32, b.width as f32, b.height as f32]);
            c.node_parent
                .push(n.node.parent_id.as_deref().map_or(NONE, at));
            c.node_flags.push(
                (n.container as u32 * FLAG_CONTAINER)
                    | (n.aggregate as u32 * FLAG_AGGREGATE)
                    | (n.collapsed as u32 * FLAG_COLLAPSED)
                    | (n.invalid as u32 * FLAG_INVALID),
            );
        }
        c.node_kinds = node_kinds.items;
        c.route_offsets.push(0);
        let mut route_kinds = Table::default();
        // Counted in u64: `usize` is 32 bits in WASM.
        let mut points = 0u64;
        for r in &scene.routes {
            c.route_ids.push(r.id.clone());
            c.route_source.push(at(&r.source));
            c.route_target.push(at(&r.target));
            c.route_weight
                .push(u32::try_from(r.weight).unwrap_or(u32::MAX));
            c.route_flags
                .push((r.invalid as u32 * ROUTE_INVALID) | (r.rounded as u32 * ROUTE_ROUNDED));
            c.route_kind.push(route_kinds.id(&r.kind));
            c.route_back_weight.push(
                r.back_weight
                    .map_or(NONE, |w| u32::try_from(w).unwrap_or(u32::MAX - 1)),
            );
            for p in &r.points {
                c.route_points.extend([p.x as f32, p.y as f32]);
            }
            points += r.points.len() as u64;
            c.route_offsets
                .push(u32::try_from(points).unwrap_or(u32::MAX));
        }
        let route_index: HashMap<&str, u32> = scene
            .routes
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id.as_str(), i as u32))
            .collect();
        let mut edges = Table::default();
        for p in &scene.ports {
            c.port_ids.push(p.id.clone());
            c.port_node.push(at(&p.node_id));
            c.port_edge.push(edges.id(&p.edge_id));
            c.port_flags.push(
                (p.explicit as u32 * PORT_EXPLICIT)
                    | ((p.endpoint == "target") as u32 * PORT_TARGET),
            );
            c.port_side.push(side_code(p.side));
            c.port_position
                .extend([p.position.x as f32, p.position.y as f32]);
        }
        for x in &scene.crossings {
            c.crossing_edge.push(edges.id(&x.edge_id));
            c.crossing_container.push(at(&x.container_id));
            c.crossing_ordinal
                .push(u32::try_from(x.ordinal).unwrap_or(u32::MAX));
            c.crossing_side.push(side_code(x.side));
            c.crossing_flags.push(x.derived as u32);
            c.crossing_position
                .extend([x.position.x as f32, x.position.y as f32]);
        }
        // The edges of bundles, after those of ports and crossings.
        c.route_member_offsets.push(0);
        for r in &scene.routes {
            for member in &r.members {
                let id = edges.id(member);
                c.route_members.push(id);
            }
            c.route_member_offsets.push(c.route_members.len() as u32);
        }
        c.route_kinds = route_kinds.items;
        c.edge_ids = edges.items;
        let mut kinds = Table::default();
        for l in &scene.labels {
            c.label_ids.push(l.id.clone());
            c.label_texts.push(l.text.clone());
            c.label_kind.push(kinds.id(&l.kind));
            let b = &l.bounds;
            c.label_bounds
                .extend([b.x as f32, b.y as f32, b.width as f32, b.height as f32]);
        }
        c.label_kinds = kinds.items;
        for (i, n) in scene.nodes.iter().enumerate() {
            for row in &n.rows {
                c.row_ids.push(row.id.clone());
                c.row_labels.push(row.label.clone());
                c.row_node.push(i as u32);
                let b = &row.bounds;
                c.row_bounds
                    .extend([b.x as f32, b.y as f32, b.width as f32, b.height as f32]);
            }
        }
        let mut conflict_kinds = Table::default();
        c.conflict_node_offsets.push(0);
        c.conflict_route_offsets.push(0);
        for k in &scene.conflicts {
            c.conflict_kind.push(conflict_kinds.id(&k.kind));
            c.conflict_reasons.push(k.reason.clone());
            c.conflict_nodes.extend(k.nodes.iter().map(|id| at(id)));
            c.conflict_routes.extend(
                k.routes
                    .iter()
                    .map(|id| route_index.get(id.as_str()).copied().unwrap_or(NONE)),
            );
            c.conflict_node_offsets.push(c.conflict_nodes.len() as u32);
            c.conflict_route_offsets
                .push(c.conflict_routes.len() as u32);
        }
        c.conflict_kinds = conflict_kinds.items;
        for p in &scene.junctions {
            c.junctions.extend([p.x as f32, p.y as f32]);
        }
        let b = &scene.bounds;
        c.bounds = [b.x, b.y, b.width, b.height];
        c.show_ports = scene.show_ports;
        c.buses = scene.buses.clone();
        c.bus_lines = scene.bus_lines.clone();
        c.hub_refs = scene.hub_refs.clone();
        c
    }

    /// The numeric columns in a fixed order, with the names a host knows them by. The position is the index that
    /// `lw_session_column` takes.
    pub fn columns(&self) -> Vec<(&'static str, Column<'_>)> {
        vec![
            ("nodeBounds", Column::F32(&self.node_bounds)),
            ("nodeParent", Column::U32(&self.node_parent)),
            ("nodeFlags", Column::U32(&self.node_flags)),
            ("nodeKind", Column::U32(&self.node_kind)),
            ("nodeShape", Column::U32(&self.node_shape)),
            ("routeSource", Column::U32(&self.route_source)),
            ("routeTarget", Column::U32(&self.route_target)),
            ("routeWeight", Column::U32(&self.route_weight)),
            ("routeFlags", Column::U32(&self.route_flags)),
            ("routeOffsets", Column::U32(&self.route_offsets)),
            ("routePoints", Column::F32(&self.route_points)),
            ("routeKind", Column::U32(&self.route_kind)),
            ("routeBackWeight", Column::U32(&self.route_back_weight)),
            (
                "routeMemberOffsets",
                Column::U32(&self.route_member_offsets),
            ),
            ("routeMembers", Column::U32(&self.route_members)),
            ("portNode", Column::U32(&self.port_node)),
            ("portEdge", Column::U32(&self.port_edge)),
            ("portFlags", Column::U32(&self.port_flags)),
            ("portSide", Column::U32(&self.port_side)),
            ("portPosition", Column::F32(&self.port_position)),
            ("crossingEdge", Column::U32(&self.crossing_edge)),
            ("crossingContainer", Column::U32(&self.crossing_container)),
            ("crossingOrdinal", Column::U32(&self.crossing_ordinal)),
            ("crossingSide", Column::U32(&self.crossing_side)),
            ("crossingFlags", Column::U32(&self.crossing_flags)),
            ("crossingPosition", Column::F32(&self.crossing_position)),
            ("labelKind", Column::U32(&self.label_kind)),
            ("labelBounds", Column::F32(&self.label_bounds)),
            ("rowNode", Column::U32(&self.row_node)),
            ("rowBounds", Column::F32(&self.row_bounds)),
            ("conflictKind", Column::U32(&self.conflict_kind)),
            (
                "conflictNodeOffsets",
                Column::U32(&self.conflict_node_offsets),
            ),
            ("conflictNodes", Column::U32(&self.conflict_nodes)),
            (
                "conflictRouteOffsets",
                Column::U32(&self.conflict_route_offsets),
            ),
            ("conflictRoutes", Column::U32(&self.conflict_routes)),
            ("junctions", Column::F32(&self.junctions)),
            ("nodeChildCount", Column::U32(&self.node_child_count)),
            ("nodeHiddenCount", Column::U32(&self.node_hidden_count)),
        ]
    }

    /// What a host needs to read the columns: versions, the id tables, and each column's name, type and length in the
    /// order of `columns()`.
    pub fn describe(&self) -> serde_json::Value {
        let columns: Vec<_> = self
            .columns()
            .iter()
            .map(
                |(name, c)| serde_json::json!({"name": name, "type": c.raw().1, "length": c.len()}),
            )
            .collect();
        serde_json::json!({
            "schemaVersion": self.schema_version,
            "revision": self.revision,
            "layoutVersion": self.layout_version,
            "nodeIds": self.node_ids,
            "nodeLabels": self.node_labels,
            "nodeKinds": self.node_kinds,
            "routeIds": self.route_ids,
            "routeKinds": self.route_kinds,
            "portIds": self.port_ids,
            "edgeIds": self.edge_ids,
            "labelIds": self.label_ids,
            "labelTexts": self.label_texts,
            "labelKinds": self.label_kinds,
            "rowIds": self.row_ids,
            "rowLabels": self.row_labels,
            "conflictKinds": self.conflict_kinds,
            "conflictReasons": self.conflict_reasons,
            "buses": self.buses,
            "busLines": self.bus_lines,
            "hubRefs": self.hub_refs,
            "bounds": self.bounds,
            "showPorts": self.show_ports,
            "columns": columns,
        })
    }
}

/// Provisional (ADR 0019, step 4): the numbers of an overview as columns, kept by an overview session after a request with
/// `"columns": true`. The answer's `overview` is the overview without these arrays plus `columns` (name, type, length of
/// each, in the order of `column`); the strings (`ids`, `labels`, `kinds`) and the scalars stay in it as JSON.
///
/// The positions stay `f64`, the values the JSON answer carries (rounded to hundredths): a viewer that finds chains by
/// comparing discs gets the same ones from either path, which `f32` could not promise at large coordinates.
#[derive(Clone, Debug, Default)]
pub struct OverviewColumns {
    x: Vec<f64>,
    y: Vec<f64>,
    r: Vec<f64>,
    /// Empty when only positions were asked for (`positions_only`); the host keeps those of its last whole overview.
    parent: Vec<i32>,
    kind: Vec<u32>,
    edges: Vec<u32>,
    links: Vec<f64>,
    positions_only: bool,
    meta: serde_json::Value,
}

impl OverviewColumns {
    /// The columns of an overview. With `positions_only` (a continuation on the same clusters) only `x`, `y` and `r`.
    pub fn of(overview: arqlet_model::Overview, positions_only: bool) -> Self {
        let mut meta = if positions_only {
            let mut meta = serde_json::json!({
                "schemaVersion": overview.schema_version,
                "positionsOnly": true,
                "representation": overview.representation,
                "rootRadius": overview.root_radius,
                "stats": overview.stats,
            });
            if !overview.bus_lines.is_empty() {
                meta["busLines"] = serde_json::to_value(&overview.bus_lines).unwrap_or_default();
            }
            meta
        } else {
            let mut whole = serde_json::to_value(&overview).unwrap_or_default();
            if let Some(o) = whole.as_object_mut() {
                for key in ["x", "y", "r", "parent", "kind", "edges", "links"] {
                    o.remove(key);
                }
            }
            whole
        };
        let mut c = OverviewColumns {
            x: overview.x,
            y: overview.y,
            r: overview.r,
            positions_only,
            ..Default::default()
        };
        if !positions_only {
            c.parent = overview.parent.iter().map(|&p| p as i32).collect();
            c.kind = overview.kind;
            c.edges = overview.edges;
            c.links = overview.links;
        }
        let listed: Vec<_> = c
            .columns()
            .iter()
            .map(|(name, col)| serde_json::json!({"name": name, "type": col.raw().1, "length": col.len()}))
            .collect();
        meta["columns"] = serde_json::Value::Array(listed);
        c.meta = meta;
        c
    }

    /// The columns in a fixed order; a positions-only set has the first three.
    pub fn columns(&self) -> Vec<(&'static str, Column<'_>)> {
        let mut all = vec![
            ("x", Column::F64(&self.x)),
            ("y", Column::F64(&self.y)),
            ("r", Column::F64(&self.r)),
        ];
        if !self.positions_only {
            all.push(("parent", Column::I32(&self.parent)));
            all.push(("kind", Column::U32(&self.kind)));
            all.push(("edges", Column::U32(&self.edges)));
            all.push(("links", Column::F64(&self.links)));
        }
        all
    }

    /// Column `index` of `columns()`, for a host that views the memory.
    pub fn column(&self, index: usize) -> Option<Column<'_>> {
        self.columns().into_iter().nth(index).map(|(_, c)| c)
    }

    /// The overview without its numeric arrays, plus the list of columns.
    pub fn describe(&self) -> serde_json::Value {
        self.meta.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arqlet_model::{
        BoundaryCrossing, Node, NodeShape, PlacedLabel, Point, PortSide, Rect, ResolvedPort,
    };
    use arqlet_scene::{SceneConflict, SceneNode, SceneRoute, SceneRow};

    fn node(id: &str, parent: Option<&str>, kind: &str, shape: NodeShape) -> Node {
        Node {
            id: id.into(),
            kind: kind.into(),
            label: format!("label {id}"),
            parent_id: parent.map(String::from),
            shape,
        }
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    fn row(id: &str, label: &str, bounds: Rect) -> SceneRow {
        SceneRow {
            id: id.into(),
            label: label.into(),
            bounds,
        }
    }

    fn label(id: &str, kind: &str, text: &str, bounds: Rect) -> PlacedLabel {
        PlacedLabel {
            id: id.into(),
            kind: kind.into(),
            text: text.into(),
            bounds,
        }
    }

    /// One scene with everything in it: a container, two nodes of different shapes with rows, routes (one leaves the
    /// scene), ports, a crossing, labels, conflicts that name a node and a route outside the scene, a junction and a bus.
    fn scene() -> Scene {
        let a = SceneNode {
            node: node("a", None, "group", NodeShape::Rect),
            bounds: rect(0., 0., 200., 100.),
            container: true,
            aggregate: false,
            collapsed: false,
            invalid: false,
            rows: vec![],
            child_count: None,
            hidden_count: None,
        };
        let b = SceneNode {
            node: node("b", Some("a"), "service", NodeShape::Circle),
            bounds: rect(10.5, 20.25, 40., 40.),
            container: false,
            aggregate: false,
            collapsed: false,
            invalid: true,
            rows: vec![
                row("r0", "first", rect(11., 21., 38., 10.)),
                row("r1", "second", rect(11., 31., 38., 10.)),
            ],
            child_count: None,
            hidden_count: None,
        };
        let c = SceneNode {
            node: node("c", Some("a"), "service", NodeShape::Square),
            bounds: rect(100., 20., 40., 40.),
            container: false,
            aggregate: true,
            collapsed: true,
            invalid: false,
            rows: vec![row("r2", "only", rect(101., 21., 38., 10.))],
            child_count: None,
            hidden_count: None,
        };
        let route = |id: &str, target: &str, n: usize| SceneRoute {
            id: id.into(),
            kind: if id == "e2" {
                "calls".into()
            } else {
                String::new()
            },
            source: "b".into(),
            target: target.into(),
            points: (0..n)
                .map(|i| Point {
                    x: i as f64 + 0.5,
                    y: 2. * i as f64,
                })
                .collect(),
            weight: 1,
            back_weight: (id == "e2").then_some(2),
            invalid: false,
            rounded: id == "e1",
            members: if id == "e2" {
                vec!["m1".into(), "e0".into()]
            } else {
                vec![]
            },
        };
        let port =
            |id: &str, edge: &str, endpoint: &str, side: PortSide, explicit: bool| ResolvedPort {
                id: id.into(),
                node_id: "b".into(),
                edge_id: edge.into(),
                endpoint: endpoint.into(),
                side,
                position: Point { x: 50.5, y: 40.25 },
                explicit,
            };
        Scene {
            schema_version: 1,
            revision: 3,
            layout_version: 4,
            nodes: vec![a, b, c],
            routes: vec![
                route("e0", "c", 3),
                route("e1", "gone", 0),
                route("e2", "c", 2),
            ],
            ports: vec![
                port("p0", "e0", "source", PortSide::Right, true),
                port("p1", "e0", "target", PortSide::Top, false),
                port("p2", "e2", "source", PortSide::Bottom, false),
            ],
            labels: vec![
                label("e0", "edge", "calls", rect(60., 30., 30., 10.)),
                label("p0", "port", "out", rect(52., 38., 10., 8.)),
                label("e2", "edge", "uses", rect(70., 50., 30., 10.)),
            ],
            crossings: vec![BoundaryCrossing {
                edge_id: "e2".into(),
                container_id: "a".into(),
                ordinal: 7,
                side: PortSide::Left,
                position: Point { x: 0., y: 33.5 },
                derived: true,
            }],
            show_ports: true,
            conflicts: vec![
                SceneConflict {
                    kind: "node_overlap".into(),
                    nodes: vec!["b".into(), "c".into()],
                    routes: vec![],
                    reason: "b and c overlap".into(),
                },
                SceneConflict {
                    kind: "route-through-node".into(),
                    nodes: vec!["c".into(), "missing".into()],
                    routes: vec!["e2".into(), "unknown".into()],
                    reason: "e2 runs through c".into(),
                },
            ],
            buses: vec![vec!["e0".into(), "e2".into()]],
            junctions: vec![Point { x: 5., y: 6.5 }],
            bus_lines: Vec::new(),
            hub_refs: Vec::new(),
            bounds: rect(-30., -30., 260., 160.),
        }
    }

    fn numbers(c: &SceneColumns, name: &str) -> Vec<f64> {
        match c.columns().into_iter().find(|(n, _)| *n == name).unwrap().1 {
            Column::F32(v) => v.iter().map(|&x| x as f64).collect(),
            Column::U32(v) => v.iter().map(|&x| x as f64).collect(),
            Column::I32(v) => v.iter().map(|&x| x as f64).collect(),
            Column::F64(v) => v.to_vec(),
        }
    }

    #[test]
    fn every_part_of_the_scene_has_its_columns() {
        let c = SceneColumns::from_scene(&scene());
        let none = NONE as f64;
        // Nodes: label, kind (a table), shape, flags, parent.
        assert_eq!(c.node_labels, ["label a", "label b", "label c"]);
        assert_eq!(c.node_kinds, ["group", "service"]);
        assert_eq!(numbers(&c, "nodeKind"), [0., 1., 1.]);
        assert_eq!(numbers(&c, "nodeShape"), [0., 2., 1.]);
        assert_eq!(numbers(&c, "nodeFlags"), [1., 8., 2. + 4.]);
        assert_eq!(numbers(&c, "nodeParent"), [none, 0., 0.]);
        // Ports: node, edge (a table of the edges that ports and crossings name), flags, side, position.
        assert_eq!(c.port_ids, ["p0", "p1", "p2"]);
        assert_eq!(
            c.edge_ids,
            ["e0", "e2", "m1"],
            "ports and crossings first, then bundle members"
        );
        assert_eq!(numbers(&c, "portNode"), [1., 1., 1.]);
        assert_eq!(numbers(&c, "portEdge"), [0., 0., 1.]);
        assert_eq!(numbers(&c, "portFlags"), [1., 2., 0.]);
        assert_eq!(numbers(&c, "portSide"), [1., 2., 3.]);
        assert_eq!(
            numbers(&c, "portPosition"),
            [50.5, 40.25, 50.5, 40.25, 50.5, 40.25]
        );
        // The crossing names an edge already in the table.
        assert_eq!(numbers(&c, "crossingEdge"), [1.]);
        assert_eq!(numbers(&c, "crossingContainer"), [0.]);
        assert_eq!(numbers(&c, "crossingOrdinal"), [7.]);
        assert_eq!(numbers(&c, "crossingSide"), [0.]);
        assert_eq!(numbers(&c, "crossingFlags"), [1.]);
        assert_eq!(numbers(&c, "crossingPosition"), [0., 33.5]);
        // Labels: kinds as a table.
        assert_eq!(c.label_texts, ["calls", "out", "uses"]);
        assert_eq!(c.label_kinds, ["edge", "port"]);
        assert_eq!(numbers(&c, "labelKind"), [0., 1., 0.]);
        assert_eq!(numbers(&c, "labelBounds")[..4], [60., 30., 30., 10.]);
        // Rows: node by node, each with its node.
        assert_eq!(c.row_ids, ["r0", "r1", "r2"]);
        assert_eq!(c.row_labels, ["first", "second", "only"]);
        assert_eq!(numbers(&c, "rowNode"), [1., 1., 2.]);
        assert_eq!(numbers(&c, "rowBounds")[4..8], [11., 31., 38., 10.]);
        // Conflicts: kinds, and nodes and routes as indices behind offsets; what is outside the scene is NONE.
        assert_eq!(c.conflict_kinds, ["node_overlap", "route-through-node"]);
        assert_eq!(numbers(&c, "conflictKind"), [0., 1.]);
        assert_eq!(numbers(&c, "conflictNodeOffsets"), [0., 2., 4.]);
        assert_eq!(numbers(&c, "conflictNodes"), [1., 2., 2., none]);
        assert_eq!(numbers(&c, "conflictRouteOffsets"), [0., 0., 2.]);
        assert_eq!(numbers(&c, "conflictRoutes"), [2., none]);
        assert_eq!(c.conflict_reasons, ["b and c overlap", "e2 runs through c"]);
        // Routes keep their order, offsets and flags; the rest of the scene is in the description.
        assert_eq!(numbers(&c, "routeOffsets"), [0., 3., 3., 5.]);
        assert_eq!(numbers(&c, "routeFlags"), [0., 2., 0.]);
        // Kind (a table), the edges of the other direction (NONE: not a two-way route) and bundle members (in the edge table
        // after those of ports and crossings; "e0" is there already).
        assert_eq!(c.route_kinds, ["", "calls"]);
        assert_eq!(numbers(&c, "routeKind"), [0., 0., 1.]);
        assert_eq!(numbers(&c, "routeBackWeight"), [none, none, 2.]);
        assert_eq!(c.edge_ids, ["e0", "e2", "m1"]);
        assert_eq!(numbers(&c, "routeMemberOffsets"), [0., 0., 0., 2.]);
        assert_eq!(numbers(&c, "routeMembers"), [2., 0.]);
        assert_eq!(c.describe()["schemaVersion"], 1);
        assert_eq!(numbers(&c, "junctions"), [5., 6.5]);
        let described = c.describe();
        assert_eq!(
            described["bounds"],
            serde_json::json!([-30., -30., 260., 160.])
        );
        assert_eq!(described["showPorts"], true);
        assert_eq!(described["buses"], serde_json::json!([["e0", "e2"]]));
        // Every column is listed with its length, in the order of `columns()`.
        let listed = described["columns"].as_array().unwrap();
        assert_eq!(listed.len(), c.columns().len());
        for (entry, (name, column)) in listed.iter().zip(c.columns()) {
            assert_eq!(entry["name"], name);
            assert_eq!(entry["length"], column.len());
        }
    }
}
