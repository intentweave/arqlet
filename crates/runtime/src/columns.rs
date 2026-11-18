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
    pub revision: u64,
    pub layout_version: u64,
    /// Node ids by node index.
    pub node_ids: Vec<String>,
    /// Absolute bounds, four per node: x, y, width, height.
    pub node_bounds: Vec<f32>,
    /// Index of the parent node, `NONE` for a root (or a parent outside the scene).
    pub node_parent: Vec<u32>,
    pub node_flags: Vec<u32>,
    /// Route ids by route index.
    pub route_ids: Vec<String>,
    /// Node indices of each route's ends, `NONE` when the end is not a node of the scene.
    pub route_source: Vec<u32>,
    pub route_target: Vec<u32>,
    pub route_weight: Vec<u32>,
    pub route_flags: Vec<u32>,
    /// `route_count + 1` offsets into `route_points`, counted in points: route `i` is points `offsets[i]..offsets[i+1]`.
    pub route_offsets: Vec<u32>,
    /// All route points in one array, x and y alternating.
    pub route_points: Vec<f32>,
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
            revision: scene.revision,
            layout_version: scene.layout_version,
            ..Default::default()
        };
        for n in &scene.nodes {
            c.node_ids.push(n.node.id.clone());
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
        c.route_offsets.push(0);
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
            for p in &r.points {
                c.route_points.extend([p.x as f32, p.y as f32]);
            }
            points += r.points.len() as u64;
            c.route_offsets
                .push(u32::try_from(points).unwrap_or(u32::MAX));
        }
        c
    }

    /// The numeric columns in a fixed order, with the names a host knows them by. The position is the index that
    /// `lw_session_column` takes.
    pub fn columns(&self) -> [(&'static str, Column<'_>); 9] {
        [
            ("nodeBounds", Column::F32(&self.node_bounds)),
            ("nodeParent", Column::U32(&self.node_parent)),
            ("nodeFlags", Column::U32(&self.node_flags)),
            ("routeSource", Column::U32(&self.route_source)),
            ("routeTarget", Column::U32(&self.route_target)),
            ("routeWeight", Column::U32(&self.route_weight)),
            ("routeFlags", Column::U32(&self.route_flags)),
            ("routeOffsets", Column::U32(&self.route_offsets)),
            ("routePoints", Column::F32(&self.route_points)),
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
            "revision": self.revision,
            "layoutVersion": self.layout_version,
            "nodeIds": self.node_ids,
            "routeIds": self.route_ids,
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
            serde_json::json!({
                "schemaVersion": overview.schema_version,
                "positionsOnly": true,
                "representation": overview.representation,
                "rootRadius": overview.root_radius,
                "stats": overview.stats,
            })
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
