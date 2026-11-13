use super::{absolute_all, absolute_with};
use arqlet_model::math::PortableMath;
use arqlet_model::*;
use std::collections::{BTreeMap, BTreeSet};

/// The resolved port of an edge's end, instead of a scan of all ports per edge, which grows with the square of the edges and
/// runs in every realization (benchmarks/results/single-pass-levers-20261106.md). The first of several wins, as in a scan.
pub(crate) struct PortEnds<'a>(BTreeMap<(&'a str, &'a str), &'a ResolvedPort>);

impl<'a> PortEnds<'a> {
    pub(crate) fn new(ports: &'a [ResolvedPort]) -> Self {
        let mut ends = BTreeMap::new();
        for port in ports {
            ends.entry((port.edge_id.as_str(), port.endpoint.as_str()))
                .or_insert(port);
        }
        Self(ends)
    }

    pub(crate) fn get(&self, edge: &str, endpoint: &str) -> Option<&'a ResolvedPort> {
        self.0.get(&(edge, endpoint)).copied()
    }
}

/// Ports for a graph and its boxes. `placement` decides where automatic ports sit along their side; `min_spacing`
/// is the minimum distance between neighbouring ports, which the optimized placement keeps.
pub fn resolve_with(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    placement: PortPlacement,
    min_spacing: f64,
    node_rows: &BTreeMap<String, Vec<arqlet_model::Row>>,
    row_order: arqlet_model::RowPortOrder,
) -> Vec<ResolvedPort> {
    PortContext::new(graph).resolve_with(boxes, placement, min_spacing, node_rows, row_order)
}

/// Immutable lookup tables shared by every port resolution for one graph.
#[derive(Clone)]
pub(crate) struct PortContext<'g> {
    graph: &'g Graph,
    node_ids: Vec<&'g str>,
    node_index: BTreeMap<&'g str, usize>,
    /// The indices of `node_ids` in the order of their ids, to read a box map in one pass.
    by_id: Vec<usize>,
    /// The parent of every node, by index.
    parent_index: Vec<Option<usize>>,
    /// The node indices of every edge's source and target, in the order of `edges` (`None` for an end outside the graph).
    edge_ends: Vec<[Option<usize>; 2]>,
    explicit: BTreeMap<&'g str, &'g Port>,
    bindings: BTreeMap<&'g str, &'g EdgePorts>,
    shapes: BTreeMap<&'g str, NodeShape>,
    parents: BTreeMap<&'g str, Option<&'g str>>,
    edges: Vec<&'g Edge>,
    family: BTreeMap<(&'g str, &'g str), Vec<&'g str>>,
    peers: BTreeMap<&'g str, (&'g str, &'g str)>,
    loop_indices: BTreeMap<&'g str, usize>,
    loop_counts: BTreeMap<&'g str, usize>,
    /// Experimental (plan section 4a, `layeredConstruction`): an automatic port of an edge that runs forward through areas
    /// of one fixed flow sits on the side the flow leaves (source) or enters (target). Off by default.
    flow_sides: bool,
    /// Experimental (plan section 4a, step 3): order keys of automatic ports by (edge, source end), from the layered plan
    /// (`layered_plan::port_keys`). A node side whose ports all have a key is ordered by them instead of by the geometry.
    order_keys: BTreeMap<(String, bool), f64>,
    /// Provisional (single pass, ADR 0017): the side of an automatic port by (edge, source end), for an end inside an area
    /// that the edge leaves or enters through a side node: the side the area's own flow leaves or enters by
    /// (`layered_plan::end_sides`). It wins over the flow-side rule, which follows the smallest area holding both ends.
    end_sides: BTreeMap<(String, bool), PortSide>,
    /// Experimental (consistent layered pipeline): absolute places along the side of automatic ports by (edge, source end),
    /// with whether the flow they were planned for is vertical (`layered_plan::port_lanes`). A side across that flow (left
    /// and right for a horizontal flow) whose ports all have one takes them.
    lanes: BTreeMap<(String, bool), (f64, bool)>,
}

#[derive(Default)]
pub(crate) struct PortResolveScratch {
    rects: Vec<Rect>,
    /// The boxes of `node_ids` as given (relative to their parents), read in one pass over the boxes.
    relative: Vec<Option<Rect>>,
    /// Whether a node (by index) is one of the nodes asked for (`only`).
    selected: Vec<bool>,
    row_ports: Vec<(usize, String, String, PortSide, f64, String)>,
}

impl<'g> PortContext<'g> {
    pub(crate) fn new(graph: &'g Graph) -> Self {
        let node_ids: Vec<&str> = graph.nodes.iter().map(|node| node.id.as_str()).collect();
        let node_index: BTreeMap<&str, usize> = node_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (*id, index))
            .collect();
        let explicit = graph.ports.iter().map(|p| (p.id.as_str(), p)).collect();
        let bindings = graph
            .edge_ports
            .iter()
            .map(|p| (p.edge_id.as_str(), p))
            .collect();
        let shapes = graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.shape))
            .collect();
        let parents = graph
            .nodes
            .iter()
            .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
            .collect();
        let mut edges: Vec<_> = graph.edges.iter().filter(|e| e.target.is_some()).collect();
        edges.sort_by(|a, b| a.id.cmp(&b.id));

        let mut family: BTreeMap<(&str, &str), Vec<&str>> = BTreeMap::new();
        let mut peers = BTreeMap::new();
        for edge in &edges {
            let target = edge.target.as_deref().unwrap_or_default();
            peers.insert(edge.id.as_str(), (edge.source.as_str(), target));
            if edge.source != target {
                let key = if edge.source.as_str() < target {
                    (edge.source.as_str(), target)
                } else {
                    (target, edge.source.as_str())
                };
                family.entry(key).or_default().push(edge.id.as_str());
            }
        }
        let mut loop_counts: BTreeMap<&str, usize> = BTreeMap::new();
        let mut loop_indices = BTreeMap::new();
        for edge in &graph.edges {
            if edge.target.as_deref() == Some(edge.source.as_str()) {
                let index = loop_counts.entry(edge.source.as_str()).or_default();
                loop_indices.insert(edge.id.as_str(), *index);
                *index += 1;
            }
        }
        let mut by_id: Vec<usize> = (0..node_ids.len()).collect();
        by_id.sort_by(|&a, &b| node_ids[a].cmp(node_ids[b]));
        let parent_index: Vec<Option<usize>> = graph
            .nodes
            .iter()
            .map(|n| {
                n.parent_id
                    .as_deref()
                    .and_then(|p| node_index.get(p).copied())
            })
            .collect();
        let edge_ends = edges
            .iter()
            .map(|e| {
                [
                    node_index.get(e.source.as_str()).copied(),
                    e.target.as_deref().and_then(|t| node_index.get(t).copied()),
                ]
            })
            .collect();
        Self {
            graph,
            node_ids,
            node_index,
            by_id,
            parent_index,
            edge_ends,
            explicit,
            bindings,
            shapes,
            parents,
            edges,
            family,
            peers,
            loop_indices,
            loop_counts,
            flow_sides: false,
            order_keys: BTreeMap::new(),
            end_sides: BTreeMap::new(),
            lanes: BTreeMap::new(),
        }
    }

    /// The context with the lanes of the layered plans' border nodes (see `lanes`).
    pub(crate) fn with_lanes(mut self, lanes: BTreeMap<(String, bool), (f64, bool)>) -> Self {
        self.lanes = lanes;
        self
    }

    /// The context with the sides of ends inside areas of another axis (see `end_sides`).
    pub(crate) fn with_end_sides(mut self, sides: BTreeMap<(String, bool), PortSide>) -> Self {
        self.end_sides = sides;
        self
    }

    /// The context with the order keys of the layered plan (see `order_keys`).
    pub(crate) fn with_order_keys(mut self, keys: BTreeMap<(String, bool), f64>) -> Self {
        self.order_keys = keys;
        self
    }

    /// The context with the flow-side rule switched on or off (see `flow_sides`).
    pub(crate) fn with_flow_sides(mut self, on: bool) -> Self {
        self.flow_sides = on;
        self
    }

    pub(crate) fn resolve_with(
        &self,
        boxes: &BTreeMap<String, Rect>,
        placement: PortPlacement,
        min_spacing: f64,
        node_rows: &BTreeMap<String, Vec<arqlet_model::Row>>,
        row_order: arqlet_model::RowPortOrder,
    ) -> Vec<ResolvedPort> {
        self.resolve_nodes(boxes, placement, min_spacing, node_rows, row_order, None)
    }

    pub(crate) fn resolve_nodes(
        &self,
        boxes: &BTreeMap<String, Rect>,
        placement: PortPlacement,
        min_spacing: f64,
        node_rows: &BTreeMap<String, Vec<arqlet_model::Row>>,
        row_order: arqlet_model::RowPortOrder,
        only: Option<&[&str]>,
    ) -> Vec<ResolvedPort> {
        let mut scratch = PortResolveScratch::default();
        let mut result = Vec::new();
        self.resolve_nodes_into(
            boxes,
            placement,
            min_spacing,
            node_rows,
            row_order,
            only,
            &mut scratch,
            &mut result,
        );
        result
    }

    pub(crate) fn resolve_nodes_into(
        &self,
        boxes: &BTreeMap<String, Rect>,
        placement: PortPlacement,
        min_spacing: f64,
        node_rows: &BTreeMap<String, Vec<arqlet_model::Row>>,
        row_order: arqlet_model::RowPortOrder,
        only: Option<&[&str]>,
        scratch: &mut PortResolveScratch,
        result: &mut Vec<ResolvedPort>,
    ) {
        result.clear();
        let graph = self.graph;
        let explicit = &self.explicit;
        let bindings = &self.bindings;
        let parents = &self.parents;
        // The absolute box of every node, as `absolute_with` gives it (its own box, then each parent's offset, in that order),
        // with the boxes read in one pass in id order instead of a map lookup per node and per parent.
        scratch.relative.clear();
        scratch.relative.resize(self.node_ids.len(), None);
        {
            let mut entries = boxes.iter().peekable();
            for &i in &self.by_id {
                let id = self.node_ids[i];
                while entries.peek().is_some_and(|(key, _)| key.as_str() < id) {
                    entries.next();
                }
                if let Some((key, rect)) = entries.peek() {
                    if key.as_str() == id {
                        scratch.relative[i] = Some((*rect).clone());
                    }
                }
            }
        }
        scratch.rects.clear();
        scratch.rects.reserve(self.node_ids.len());
        for i in 0..self.node_ids.len() {
            let Some(own) = scratch.relative[i].clone() else {
                // A node without a box: the same failure as before.
                scratch
                    .rects
                    .push(absolute_with(parents, boxes, self.node_ids[i]));
                continue;
            };
            let mut r = own;
            let mut parent = self.parent_index[i];
            while let Some(p) = parent {
                let Some(pr) = scratch.relative[p].as_ref() else {
                    r = absolute_with(parents, boxes, self.node_ids[i]);
                    break;
                };
                r.x += pr.x;
                r.y += pr.y;
                parent = self.parent_index[p];
            }
            scratch.rects.push(r);
        }
        let rects = &scratch.rects;
        let rect_for = |id: &str| &rects[self.node_index[id]];
        let shapes = &self.shapes;
        let edges = &self.edges;
        let family = &self.family;
        let peers = &self.peers;
        let loop_indices = &self.loop_indices;
        // The sides of an edge's automatic ports, by the flow-side rule (when on) or the facing sides.
        let automatic_sides = |edge: &Edge| -> (PortSide, PortSide) {
            let src = rect_for(edge.source.as_str());
            let dst = rect_for(edge.target.as_deref().unwrap());
            let parent = parents[edge.source.as_str()];
            let self_loop = edge.source == edge.target.as_deref().unwrap();
            let sides = self
                .flow_sides
                .then(|| {
                    (!self_loop)
                        .then(|| {
                            flow_side_pair(
                                graph,
                                parents,
                                edge.source.as_str(),
                                edge.target.as_deref().unwrap(),
                                src,
                                dst,
                            )
                        })
                        .flatten()
                })
                .flatten()
                .unwrap_or_else(|| facing_sides(src, dst, direction_for(graph, parent), self_loop));
            let (source, target) = sides;
            (
                *self
                    .end_sides
                    .get(&(edge.id.clone(), true))
                    .unwrap_or(&source),
                *self
                    .end_sides
                    .get(&(edge.id.clone(), false))
                    .unwrap_or(&target),
            )
        };
        // Experimental (flow sides): the loops of a node take the corner whose two sides carry the fewest other edges (ties:
        // the usual loop sides), so that the loops do not lie across the node's other edges.
        let mut loop_corner: BTreeMap<&str, (PortSide, PortSide)> = BTreeMap::new();
        if self.flow_sides && !self.loop_counts.is_empty() {
            let mut load: BTreeMap<(&str, u8), usize> = BTreeMap::new();
            for edge in edges {
                let target = edge.target.as_deref().unwrap();
                if edge.source == target {
                    continue;
                }
                let (a, b) = automatic_sides(edge);
                *load.entry((edge.source.as_str(), a as u8)).or_default() += 1;
                *load.entry((target, b as u8)).or_default() += 1;
            }
            const CORNERS: [(PortSide, PortSide); 4] = [
                (PortSide::Right, PortSide::Top),
                (PortSide::Top, PortSide::Left),
                (PortSide::Left, PortSide::Bottom),
                (PortSide::Bottom, PortSide::Right),
            ];
            for &node in self.loop_counts.keys() {
                let r = rect_for(node);
                let usual = facing_sides(r, r, direction_for(graph, parents[node]), true);
                let count = |(a, b): (PortSide, PortSide)| {
                    load.get(&(node, a as u8)).copied().unwrap_or(0)
                        + load.get(&(node, b as u8)).copied().unwrap_or(0)
                };
                let best = CORNERS
                    .iter()
                    .copied()
                    .enumerate()
                    .min_by_key(|&(i, corner)| (count(corner), corner != usual, i))
                    .map(|(_, corner)| corner)
                    .unwrap_or(usual);
                loop_corner.insert(node, best);
            }
        }
        // Row ports, with their node, row, side, the peer's height and the edge (see the ordering below).
        scratch.row_ports.clear();
        let row_ports = &mut scratch.row_ports;
        // The nodes asked for, as a mask by index: an end outside the graph is never asked for (it is not among the ids).
        scratch.selected.clear();
        if let Some(nodes) = only {
            scratch.selected.resize(self.node_ids.len(), false);
            for node in nodes {
                if let Some(&i) = self.node_index.get(node) {
                    scratch.selected[i] = true;
                }
            }
        }
        let selected = &scratch.selected;
        for (k, edge) in edges.iter().enumerate() {
            for (side_index, (endpoint, node)) in [
                ("source", edge.source.as_str()),
                ("target", edge.target.as_deref().unwrap()),
            ]
            .into_iter()
            .enumerate()
            {
                if only.is_some() && !self.edge_ends[k][side_index].is_some_and(|i| selected[i]) {
                    continue;
                }
                let configured = bindings
                    .get(edge.id.as_str())
                    .and_then(|b| {
                        if endpoint == "source" {
                            b.source_port.as_deref()
                        } else {
                            b.target_port.as_deref()
                        }
                    })
                    .and_then(|id| explicit.get(id).copied());
                let self_loop = edge.source == edge.target.as_deref().unwrap();
                let (source_side, target_side) = match loop_corner.get(edge.source.as_str()) {
                    Some(&corner) if self_loop => corner,
                    _ => automatic_sides(edge),
                };
                let side = configured
                    .map(|p| p.side)
                    .unwrap_or(if endpoint == "source" {
                        source_side
                    } else {
                        target_side
                    });
                // Automatic endpoints are distributed together per physical side below.
                let offset = configured.map(|p| p.offset).unwrap_or(0.5);
                let r = rect_for(node);
                let shape = shapes.get(node).copied().unwrap_or_default();
                if shape == NodeShape::Circle {
                    // A circle port lies on the outline. An automatic port faces its peer; a configured side and offset
                    // map to the arc of that side (offset 0.5 is the middle of the side).
                    let peer_id = if endpoint == "source" {
                        edge.target.as_deref().unwrap()
                    } else {
                        edge.source.as_str()
                    };
                    let peer = rect_for(peer_id);
                    let (cx, cy) = (r.x + r.width / 2., r.y + r.height / 2.);
                    let radius = r.width.min(r.height) / 2.;
                    let (px, py) = (peer.x + peer.width / 2., peer.y + peer.height / 2.);
                    // An automatic port is the outline's attachment toward the peer; a configured one is an arc of its side.
                    let outline = arqlet_model::outline::Outline::of(NodeShape::Circle, r);
                    let (position, side) = match configured {
                        Some(p) => {
                            let base = match p.side {
                                PortSide::Right => 0.,
                                PortSide::Bottom => std::f64::consts::FRAC_PI_2,
                                PortSide::Left => std::f64::consts::PI,
                                PortSide::Top => 3. * std::f64::consts::FRAC_PI_2,
                            };
                            // The arc of a side spans 90 degrees: a port never leaves tangentially, so its exit is valid.
                            let angle = base + (offset - 0.5) * std::f64::consts::FRAC_PI_2;
                            (
                                Point {
                                    x: cx + radius * angle.p_cos(),
                                    y: cy + radius * angle.p_sin(),
                                },
                                p.side,
                            )
                        }
                        None if edge.target.as_deref() == Some(edge.source.as_str()) => {
                            // A loop has no peer to face: its ends lie on the sides the loop rule picks (`facing_sides`), and the
                            // loops of one node are turned against each other so that their ends differ.
                            let nth = loop_indices.get(edge.id.as_str()).copied().unwrap_or(0);
                            let base = match side {
                                PortSide::Right => 0.,
                                PortSide::Bottom => std::f64::consts::FRAC_PI_2,
                                PortSide::Left => std::f64::consts::PI,
                                PortSide::Top => 3. * std::f64::consts::FRAC_PI_2,
                            };
                            let angle = match loop_corner.get(node) {
                                // Experimental (flow sides): the ends lie on the arc of their side next to the loops' corner, the
                                // first loop nearest it, all of them within the side's arc.
                                Some(&(a, b)) => {
                                    let (na, nb) = (side_normal(a), side_normal(b));
                                    let corner = (na.1 + nb.1).p_atan2(na.0 + nb.0);
                                    let mut toward = base - corner;
                                    while toward > std::f64::consts::PI {
                                        toward -= 2. * std::f64::consts::PI;
                                    }
                                    while toward <= -std::f64::consts::PI {
                                        toward += 2. * std::f64::consts::PI;
                                    }
                                    let count = self.loop_counts.get(node).copied().unwrap_or(1);
                                    let step = std::f64::consts::FRAC_PI_4 / (count as f64 + 1.);
                                    corner + toward.signum() * (nth as f64 + 1.) * step
                                }
                                None => base + nth as f64 * LOOP_TURN,
                            };
                            (
                                Point {
                                    x: cx + radius * angle.p_cos(),
                                    y: cy + radius * angle.p_sin(),
                                },
                                side,
                            )
                        }
                        None => {
                            let (mut ax, mut ay) = outline
                                .attachment_toward(px, py)
                                .unwrap_or((cx + radius, cy));
                            // One of several edges between the same two nodes: the port moves along the circle, to the side of
                            // the line between the nodes that the edge's place among its siblings gives it. Both ends use the
                            // same side, so the segments run side by side and do not cross.
                            let source_id = edge.source.as_str();
                            let target_id = edge.target.as_deref().unwrap_or_default();
                            let key = if source_id < target_id {
                                (source_id, target_id)
                            } else {
                                (target_id, source_id)
                            };
                            if let Some(ids) = family.get(&key).filter(|ids| ids.len() > 1) {
                                let place = ids.iter().position(|i| *i == edge.id).unwrap_or(0);
                                let spacing = min_spacing.max(8.);
                                let off = (place as f64 - (ids.len() as f64 - 1.) / 2.) * spacing;
                                let (a, b) = (rect_for(key.0), rect_for(key.1));
                                let (wx, wy) = (
                                    (b.x + b.width / 2.) - (a.x + a.width / 2.),
                                    (b.y + b.height / 2.) - (a.y + a.height / 2.),
                                );
                                let length = wx.p_hypot(wy);
                                if length > 0. && radius > 0. {
                                    // The edges are spread along the axis on which a rectangle spreads its ports on the side
                                    // that faces the circle (ports are ordered by increasing coordinate), so that a circle and a
                                    // rectangle agree on which sibling lies where. Between two circles the longer axis decides.
                                    let peer_is_box =
                                        shapes.get(peer_id).copied().unwrap_or_default()
                                            != NodeShape::Circle;
                                    let peer_side = if endpoint == "source" {
                                        target_side
                                    } else {
                                        source_side
                                    };
                                    let along_y = if peer_is_box {
                                        matches!(peer_side, PortSide::Left | PortSide::Right)
                                    } else {
                                        wx.abs() >= wy.abs()
                                    };
                                    let (nx, ny) = if along_y { (0., 1.) } else { (1., 0.) };
                                    let lateral = (off / radius).clamp(-0.9, 0.9);
                                    let (bx, by) = ((ax - cx) / radius, (ay - cy) / radius);
                                    let (dx, dy) = (bx + nx * lateral, by + ny * lateral);
                                    let norm = dx.p_hypot(dy);
                                    if norm > 0. {
                                        ax = cx + radius * dx / norm;
                                        ay = cy + radius * dy / norm;
                                    }
                                }
                            }
                            let (dx, dy) = (ax - cx, ay - cy);
                            let side = if dx.abs() >= dy.abs() {
                                if dx >= 0. {
                                    PortSide::Right
                                } else {
                                    PortSide::Left
                                }
                            } else if dy >= 0. {
                                PortSide::Bottom
                            } else {
                                PortSide::Top
                            };
                            (Point { x: ax, y: ay }, side)
                        }
                    };
                    result.push(ResolvedPort {
                        id: configured
                            .map(|p| p.id.clone())
                            .unwrap_or(format!("auto:{}:{}", edge.id, endpoint)),
                        node_id: node.into(),
                        edge_id: edge.id.clone(),
                        endpoint: endpoint.into(),
                        side,
                        position,
                        explicit: configured.is_some(),
                    });
                    continue;
                }
                if let Some(row_id) = configured.and_then(|p| p.row.as_deref()) {
                    let row = node_rows
                        .get(node)
                        .and_then(|rows| {
                            arqlet_model::row_bounds(r, rows)
                                .into_iter()
                                .find(|(id, _)| id == row_id)
                        })
                        .map(|(_, bounds)| bounds);
                    // Validated by the layout: the row exists and the side is left or right.
                    if let Some(row) = row {
                        let y = row.y + row.height / 2.;
                        let x = if side == PortSide::Left {
                            r.x
                        } else {
                            r.x + r.width
                        };
                        let peer = if endpoint == "source" {
                            edge.target.as_deref().unwrap()
                        } else {
                            edge.source.as_str()
                        };
                        let peer_rect = rect_for(peer);
                        let peer_y = peer_rect.y + peer_rect.height / 2.;
                        row_ports.push((
                            result.len(),
                            node.to_string(),
                            row_id.to_string(),
                            side,
                            peer_y,
                            edge.id.clone(),
                        ));
                        result.push(ResolvedPort {
                            id: configured.map(|p| p.id.clone()).unwrap_or_default(),
                            node_id: node.into(),
                            edge_id: edge.id.clone(),
                            endpoint: endpoint.into(),
                            side,
                            position: Point { x, y },
                            explicit: true,
                        });
                        continue;
                    }
                }
                let position = match side {
                    PortSide::Left => Point {
                        x: r.x,
                        y: r.y + r.height * offset,
                    },
                    PortSide::Right => Point {
                        x: r.x + r.width,
                        y: r.y + r.height * offset,
                    },
                    PortSide::Top => Point {
                        x: r.x + r.width * offset,
                        y: r.y,
                    },
                    PortSide::Bottom => Point {
                        x: r.x + r.width * offset,
                        y: r.y + r.height,
                    },
                };
                result.push(ResolvedPort {
                    id: configured
                        .map(|p| p.id.clone())
                        .unwrap_or(format!("auto:{}:{}", edge.id, endpoint)),
                    node_id: node.into(),
                    edge_id: edge.id.clone(),
                    endpoint: endpoint.into(),
                    side,
                    position,
                    explicit: configured.is_some(),
                });
            }
        }
        // Row ports on one row and side share the row's height (ADR 0011). They are ordered by where their peers lie, so the
        // routes do not cross, and spread evenly over the row. A single port keeps the middle of its row.
        let mut shared: BTreeMap<(String, String, String), Vec<(f64, String, usize)>> =
            BTreeMap::new();
        for (index, node, row_id, side, peer_y, edge_id) in row_ports.iter() {
            shared
                .entry((node.clone(), row_id.clone(), format!("{side:?}")))
                .or_default()
                .push((*peer_y, edge_id.clone(), *index));
        }
        for ((node, row_id, _), mut ports) in shared {
            if row_order == arqlet_model::RowPortOrder::Middle {
                break;
            }
            ports.sort_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
            let bounds = node_rows
                .get(&node)
                .and_then(|rows| {
                    arqlet_model::row_bounds(rect_for(node.as_str()), rows)
                        .into_iter()
                        .find(|(id, _)| *id == row_id)
                })
                .map(|(_, bounds)| bounds);
            let Some(bounds) = bounds else {
                continue;
            };
            let count = ports.len() as f64;
            for (i, (_, _, index)) in ports.iter().enumerate() {
                result[*index].position.y =
                    bounds.y + bounds.height * (i as f64 + 1.) / (count + 1.);
            }
        }
        let mut groups: BTreeMap<(String, u8), Vec<usize>> = BTreeMap::new();
        // Circle ports keep their place on the outline; the rectangle sides below do not move them.
        for (i, port) in result.iter().enumerate().filter(|(_, p)| {
            !p.explicit
                && shapes.get(p.node_id.as_str()).copied().unwrap_or_default() != NodeShape::Circle
        }) {
            groups
                .entry((port.node_id.clone(), port.side as u8))
                .or_default()
                .push(i);
        }
        // Ports on one side keep the order of their peers along that side, so that edges do not cross
        // at the node. Ties (same peer position) fall back to the edge id.
        let peer_key = |i: usize| -> f64 {
            let port = &result[i];
            let (source, target) = peers[port.edge_id.as_str()];
            // Experimental (flow sides): a self-loop's port goes to the end of its side at the corner it shares with the
            // loop's other side, so that the loops of a node lie at that corner, nested, and other edges do not pass them.
            if self.flow_sides && source == target {
                let other_side = result
                    .iter()
                    .find(|p| p.edge_id == port.edge_id && p.endpoint != port.endpoint)
                    .map(|p| p.side);
                match other_side {
                    Some(PortSide::Top | PortSide::Left) => return f64::NEG_INFINITY,
                    Some(PortSide::Bottom | PortSide::Right) => return f64::INFINITY,
                    None => {}
                }
            }
            let other = if port.endpoint == "source" {
                target
            } else {
                source
            };
            let c = rect_for(other);
            if placement == PortPlacement::Angular {
                // Slope of the peer seen from the node centre, along the side: increasing slope moves the port down or
                // right, which is the order of the directions around the node.
                let own = rect_for(port.node_id.as_str());
                let dx = (c.x + c.width / 2.) - (own.x + own.width / 2.);
                let dy = (c.y + c.height / 2.) - (own.y + own.height / 2.);
                return match port.side {
                    PortSide::Left | PortSide::Right => dy / dx.abs().max(1e-9),
                    PortSide::Top | PortSide::Bottom => dx / dy.abs().max(1e-9),
                };
            }
            match port.side {
                PortSide::Top | PortSide::Bottom => c.x + c.width / 2.,
                PortSide::Left | PortSide::Right => c.y + c.height / 2.,
            }
        };
        let order_key = |i: usize| {
            let port = &result[i];
            self.order_keys
                .get(&(port.edge_id.clone(), port.endpoint == "source"))
                .copied()
        };
        for indices in groups.values_mut() {
            // The order of the layered plan, when it gives every port of the side a key.
            if !self.order_keys.is_empty() && indices.iter().all(|&i| order_key(i).is_some()) {
                indices.sort_by(|a, b| {
                    order_key(*a)
                        .unwrap()
                        .total_cmp(&order_key(*b).unwrap())
                        .then_with(|| result[*a].edge_id.cmp(&result[*b].edge_id))
                });
                continue;
            }
            indices.sort_by(|a, b| {
                peer_key(*a)
                    .total_cmp(&peer_key(*b))
                    .then_with(|| result[*a].edge_id.cmp(&result[*b].edge_id))
            });
        }
        let keys: Vec<f64> = (0..result.len()).map(peer_key).collect();
        for indices in groups.values() {
            let Some(&first) = indices.first() else {
                continue;
            };
            let r = rect_for(result[first].node_id.as_str());
            let side = result[first].side;
            let (origin, length) = match side {
                PortSide::Left | PortSide::Right => (r.y, r.height),
                PortSide::Top | PortSide::Bottom => (r.x, r.width),
            };
            // Optimized: each port at its peer's position, kept apart by the minimum spacing, inside the side.
            let optimized = (placement == PortPlacement::Optimized).then(|| {
                let spacing = min_spacing.max(OPTIMIZED_MIN_SPACING);
                let margin = (spacing / 2.).max(OPTIMIZED_MARGIN);
                let desired: Vec<f64> = indices.iter().map(|i| keys[*i] - origin).collect();
                spread_to_peers(&desired, margin, length - margin, spacing)
            });
            // A planned place holds only on a side across the flow it was planned for.
            let lane = |i: usize| {
                let (value, vertical) = self
                    .lanes
                    .get(&(result[i].edge_id.clone(), result[i].endpoint == "source"))
                    .copied()?;
                let across = match result[i].side {
                    PortSide::Left | PortSide::Right => !vertical,
                    PortSide::Top | PortSide::Bottom => vertical,
                };
                across.then_some(value)
            };
            let lanes: Option<Vec<f64>> = (!self.lanes.is_empty())
                .then(|| {
                    indices
                        .iter()
                        .map(|&i| lane(i))
                        .collect::<Option<Vec<f64>>>()
                })
                .flatten();
            for (ordinal, index) in indices.iter().enumerate() {
                if let Some(lanes) = &lanes {
                    let port = &mut result[*index];
                    match port.side {
                        PortSide::Left | PortSide::Right => port.position.y = lanes[ordinal],
                        PortSide::Top | PortSide::Bottom => port.position.x = lanes[ordinal],
                    }
                    continue;
                }
                let port = &mut result[*index];
                let along = match optimized.as_ref().and_then(|o| o.as_ref()) {
                    Some(positions) => origin + positions[ordinal],
                    None => {
                        // Same operation order as the prototype, so the default stays bit-identical.
                        let offset = (ordinal + 1) as f64 / (indices.len() + 1) as f64;
                        origin + length * offset
                    }
                };
                match port.side {
                    PortSide::Left | PortSide::Right => port.position.y = along,
                    PortSide::Top | PortSide::Bottom => port.position.x = along,
                }
            }
        }
    }
}

/// Distance between neighbouring port slots on a side: the minimum spacing, never less than the smallest one used.
pub fn slot_pitch(min_spacing: f64) -> f64 {
    min_spacing.max(OPTIMIZED_MIN_SPACING)
}

/// Sides whose automatic ports do not fit their slots (`count` slots `pitch` apart, plus half a pitch reserved at each
/// corner). Returned as `(node id, side)`; empty when every side has room.
pub fn overflowing_sides(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    min_spacing: f64,
    node_rows: &BTreeMap<String, Vec<arqlet_model::Row>>,
) -> Vec<(String, PortSide)> {
    let rects = absolute_all(graph, boxes);
    let pitch = slot_pitch(min_spacing);
    let mut counts: BTreeMap<(String, u8), (usize, PortSide)> = BTreeMap::new();
    for port in resolve_with(
        graph,
        boxes,
        PortPlacement::Angular,
        min_spacing,
        node_rows,
        arqlet_model::RowPortOrder::Peer,
    )
    .iter()
    .filter(|p| !p.explicit)
    {
        let entry = counts
            .entry((port.node_id.clone(), port.side as u8))
            .or_insert((0, port.side));
        entry.0 += 1;
    }
    counts
        .into_iter()
        .filter_map(|((node, _), (count, side))| {
            let r = &rects[node.as_str()];
            let length = match side {
                PortSide::Left | PortSide::Right => r.height,
                PortSide::Top | PortSide::Bottom => r.width,
            };
            let needed = (count as f64 - 1.) * pitch + pitch;
            (needed > length).then_some((node, side))
        })
        .collect()
}

/// Smallest spacing the optimized placement uses, even when the options ask for none, so two stubs never coincide.
const OPTIMIZED_MIN_SPACING: f64 = 8.;
/// Distance of the first and last port from a node corner.
const OPTIMIZED_MARGIN: f64 = 6.;

/// Positions for ports that want `desired` (ascending, relative to the side start), inside `[lo, hi]` and at least
/// `spacing` apart. `None` when the side cannot hold them; the caller then keeps the symmetric placement.
fn spread_to_peers(desired: &[f64], lo: f64, hi: f64, spacing: f64) -> Option<Vec<f64>> {
    let n = desired.len();
    if n == 0 {
        return Some(Vec::new());
    }
    if hi < lo || (n - 1) as f64 * spacing > hi - lo + 1e-9 {
        return None;
    }
    let mut pos: Vec<f64> = desired.iter().map(|d| d.clamp(lo, hi)).collect();
    for i in 1..n {
        pos[i] = pos[i].max(pos[i - 1] + spacing);
    }
    if pos[n - 1] > hi {
        pos[n - 1] = hi;
        for i in (0..n - 1).rev() {
            pos[i] = pos[i].min(pos[i + 1] - spacing);
        }
    }
    (pos[0] >= lo - 1e-9 && pos[n - 1] <= hi + 1e-9).then_some(pos)
}

/// The outward normal of a side, in screen coordinates (y grows downwards).
fn side_normal(side: PortSide) -> (f64, f64) {
    match side {
        PortSide::Left => (-1., 0.),
        PortSide::Right => (1., 0.),
        PortSide::Top => (0., -1.),
        PortSide::Bottom => (0., 1.),
    }
}

/// The angle, in radians, between the ends of one node's successive loops on a circle.
const LOOP_TURN: f64 = 0.35;

/// Prefer exposed, mutually facing sides; layout policy only breaks diagonal ties.
/// The flow sides of an edge (experimental, see `PortContext::flow_sides`): when every area from each end up to the smallest
/// container holding both has a fixed left-to-right or top-to-bottom flow, and the target lies after the source in it, the
/// source leaves on the right (bottom) and the target is entered on the left (top). `None` otherwise.
fn flow_side_pair(
    graph: &Graph,
    parents: &BTreeMap<&str, Option<&str>>,
    source: &str,
    target: &str,
    src: &Rect,
    dst: &Rect,
) -> Option<(PortSide, PortSide)> {
    let chain = |id: &str| -> Vec<Option<&str>> {
        let mut out = Vec::new();
        let mut at = parents.get(id).copied().flatten();
        loop {
            out.push(at);
            match at {
                Some(a) => at = parents.get(a).copied().flatten(),
                None => break,
            }
        }
        out
    };
    let (s_chain, t_chain) = (chain(source), chain(target));
    if s_chain.contains(&Some(target)) || t_chain.contains(&Some(source)) {
        return None;
    }
    let common = *s_chain.iter().find(|a| t_chain.contains(a))?;
    let axis = |area: Option<&str>| -> Option<bool> {
        match area
            .and_then(|id| graph.container_directions.get(id))
            .unwrap_or(&graph.default_direction)
        {
            DirectionConstraint::Fixed(Orientation::Lr) => Some(false),
            DirectionConstraint::Fixed(Orientation::Tb) => Some(true),
            _ => None,
        }
    };
    let vertical = axis(common)?;
    let passed = s_chain
        .iter()
        .take_while(|a| **a != common)
        .chain(t_chain.iter().take_while(|a| **a != common));
    // The sides follow the flow of the smallest area that holds both ends (plan section 4a, step 6): an end inside a
    // container of another flow axis leaves it towards the container's border where the edge goes on. A passed area needs a
    // fixed flow of either axis.
    if passed.copied().any(|area| axis(area).is_none()) {
        return None;
    }
    let forward = if vertical {
        dst.y >= src.y + src.height
    } else {
        dst.x >= src.x + src.width
    };
    // A backward edge (its target lies wholly before its source) is drawn as a forward edge in reverse, as in the layered
    // method's cycle break: it leaves its source on the incoming side and enters its target on the outgoing side.
    let backward = if vertical {
        src.y >= dst.y + dst.height
    } else {
        src.x >= dst.x + dst.width
    };
    let (out_side, in_side) = if vertical {
        (PortSide::Bottom, PortSide::Top)
    } else {
        (PortSide::Right, PortSide::Left)
    };
    if forward {
        Some((out_side, in_side))
    } else if backward {
        Some((in_side, out_side))
    } else {
        None
    }
}

pub(crate) fn facing_sides(
    src: &Rect,
    dst: &Rect,
    policy: Mode,
    self_loop: bool,
) -> (PortSide, PortSide) {
    if self_loop {
        return if matches!(policy, Mode::Orientation(o) if o.vertical()) {
            (PortSide::Bottom, PortSide::Right)
        } else {
            (PortSide::Right, PortSide::Top)
        };
    }
    let dx = dst.x + dst.width / 2. - src.x - src.width / 2.;
    let dy = dst.y + dst.height / 2. - src.y - src.height / 2.;
    let x_gap = dx.abs() - (src.width + dst.width) / 2.;
    let y_gap = dy.abs() - (src.height + dst.height) / 2.;
    let vertical = if x_gap <= 0. && y_gap > 0. {
        true
    } else if y_gap <= 0. && x_gap > 0. {
        false
    } else if (x_gap - y_gap).abs() < 1e-6 {
        matches!(policy, Mode::Orientation(o) if o.vertical())
    } else {
        y_gap > x_gap
    };
    if vertical {
        if dy >= 0. {
            (PortSide::Bottom, PortSide::Top)
        } else {
            (PortSide::Top, PortSide::Bottom)
        }
    } else if dx >= 0. {
        (PortSide::Right, PortSide::Left)
    } else {
        (PortSide::Left, PortSide::Right)
    }
}

pub fn outward(port: &ResolvedPort, distance: f64) -> Point {
    let mut p = port.position.clone();
    match port.side {
        PortSide::Left => p.x -= distance,
        PortSide::Right => p.x += distance,
        PortSide::Top => p.y -= distance,
        PortSide::Bottom => p.y += distance,
    };
    p
}
/// Where the segment from `a` to `b` meets the disc inscribed in `r`: the parameters along the segment (zero excluded, one
/// included, as for a box side) and the points, in the order along the segment.
fn segment_disc_crossings(a: &Point, b: &Point, r: &Rect) -> Vec<(f64, Point)> {
    let (cx, cy) = (r.x + r.width / 2., r.y + r.height / 2.);
    let radius = r.width.min(r.height) / 2.;
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (fx, fy) = (a.x - cx, a.y - cy);
    let qa = dx * dx + dy * dy;
    if qa <= 0. {
        return vec![];
    }
    let qb = 2. * (fx * dx + fy * dy);
    let qc = fx * fx + fy * fy - radius * radius;
    let discriminant = qb * qb - 4. * qa * qc;
    if discriminant <= 0. {
        return vec![];
    }
    let root = discriminant.sqrt();
    let mut found = Vec::new();
    for t in [(-qb - root) / (2. * qa), (-qb + root) / (2. * qa)] {
        if t > 0. && t <= 1. {
            found.push((
                t,
                Point {
                    x: a.x + t * dx,
                    y: a.y + t * dy,
                },
            ));
        }
    }
    found
}

/// Where an oblique segment from `a` to `b` meets the sides of the box `r`, strictly inside each side's range.
fn segment_box_crossings(a: &Point, b: &Point, r: &Rect) -> Vec<(f64, PortSide, Point)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let mut found = Vec::new();
    for (x, side) in [(r.x, PortSide::Left), (r.x + r.width, PortSide::Right)] {
        if dx.abs() > 1e-6 {
            let t = (x - a.x) / dx;
            let y = a.y + t * dy;
            if t > 0. && t <= 1. && y > r.y + 1e-6 && y < r.y + r.height - 1e-6 {
                found.push((t, side, Point { x, y }));
            }
        }
    }
    for (y, side) in [(r.y, PortSide::Top), (r.y + r.height, PortSide::Bottom)] {
        if dy.abs() > 1e-6 {
            let t = (y - a.y) / dy;
            let x = a.x + t * dx;
            if t > 0. && t <= 1. && x > r.x + 1e-6 && x < r.x + r.width - 1e-6 {
                found.push((t, side, Point { x, y }));
            }
        }
    }
    found
}

/// Derived route/container intersections, not fixed routing constraints.
///
/// A segment that is neither horizontal nor vertical meets a box container only for the edges in `straight` (drawn as one
/// segment): other routes are orthogonal and keep the crossings they always had. A circle container's border is its disc for
/// every segment.
pub fn boundaries(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    routes: &[Route],
    straight: &BTreeSet<String>,
) -> Vec<BoundaryCrossing> {
    if routes.is_empty() {
        return Vec::new();
    }
    let rects = absolute_all(graph, boxes);
    let parent_ids: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let mut containers: Vec<_> = graph
        .nodes
        .iter()
        .filter(|n| parent_ids.contains(n.id.as_str()))
        .map(|n| (n, &rects[n.id.as_str()]))
        .collect();
    containers.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    let mut result = Vec::new();
    for route in routes {
        let reach = route_reach(route);
        for (container, r) in &containers {
            // A crossing lies on the route and on the container's border, so a container out of the route's reach has none.
            // Without this every route walks every container (benchmarks/results/single-pass-levers-20261106.md).
            if !reaches(reach, r) {
                continue;
            }
            let found =
                container_crossings(route, container.shape == NodeShape::Circle, r, straight);
            for (ordinal, (_, side, position)) in found.into_iter().enumerate() {
                result.push(BoundaryCrossing {
                    edge_id: route.id.clone(),
                    container_id: container.id.clone(),
                    ordinal,
                    side,
                    position,
                    derived: true,
                });
            }
        }
    }
    result
}

/// The bounding box of a route, as `[min x, min y, max x, max y]`, one tolerance wider than the crossings' rounding.
fn route_reach(route: &Route) -> [f64; 4] {
    let mut reach = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in &route.points {
        reach[0] = reach[0].min(p.x - 1e-6);
        reach[1] = reach[1].min(p.y - 1e-6);
        reach[2] = reach[2].max(p.x + 1e-6);
        reach[3] = reach[3].max(p.y + 1e-6);
    }
    reach
}

fn reaches(reach: [f64; 4], r: &Rect) -> bool {
    reach[0] <= r.x + r.width && reach[2] >= r.x && reach[1] <= r.y + r.height && reach[3] >= r.y
}

/// Where a route crosses a container's border, in the order along the route.
fn container_crossings(
    route: &Route,
    round: bool,
    r: &Rect,
    straight: &BTreeSet<String>,
) -> Vec<(f64, PortSide, Point)> {
    let mut found: Vec<(f64, PortSide, Point)> = Vec::new();
    for (i, w) in route.points.windows(2).enumerate() {
        let a = &w[0];
        let b = &w[1];
        let axis_aligned = (a.y - b.y).abs() < 1e-6 || (a.x - b.x).abs() < 1e-6;
        if round {
            // A circle container's border is its disc, whatever the direction of the segment.
            for (t, position) in segment_disc_crossings(a, b, r) {
                let (cx, cy) = (r.x + r.width / 2., r.y + r.height / 2.);
                let (dx, dy) = (position.x - cx, position.y - cy);
                let side = if dx.abs() >= dy.abs() {
                    if dx >= 0. {
                        PortSide::Right
                    } else {
                        PortSide::Left
                    }
                } else if dy >= 0. {
                    PortSide::Bottom
                } else {
                    PortSide::Top
                };
                found.push((i as f64 + t, side, position));
            }
        } else if !axis_aligned && straight.contains(&route.id) {
            // An oblique segment (a straight edge) crosses the box where it meets a side.
            for (t, side, position) in segment_box_crossings(a, b, r) {
                found.push((i as f64 + t, side, position));
            }
        } else if (a.y - b.y).abs() < 1e-6
            && (a.x - b.x).abs() > 1e-6
            && a.y > r.y + 1e-6
            && a.y < r.y + r.height - 1e-6
        {
            for (x, side) in [(r.x, PortSide::Left), (r.x + r.width, PortSide::Right)] {
                let t = (x - a.x) / (b.x - a.x);
                if t > 0. && t <= 1. {
                    found.push((i as f64 + t, side, Point { x, y: a.y }));
                }
            }
        } else if (a.x - b.x).abs() < 1e-6
            && (a.y - b.y).abs() > 1e-6
            && a.x > r.x + 1e-6
            && a.x < r.x + r.width - 1e-6
        {
            for (y, side) in [(r.y, PortSide::Top), (r.y + r.height, PortSide::Bottom)] {
                let t = (y - a.y) / (b.y - a.y);
                if t > 0. && t <= 1. {
                    found.push((i as f64 + t, side, Point { x: a.x, y }));
                }
            }
        }
    }
    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    found
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_container_out_of_a_routes_reach_has_no_crossing() {
        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed % n) as f64
        };
        let r = Rect {
            x: 100.,
            y: 100.,
            width: 80.,
            height: 40.,
        };
        let straight: BTreeSet<String> = ["e".to_string()].into();
        let (mut outside, mut crossing) = (0, 0);
        for i in 0..20000 {
            // Points on a coarse grid that holds the box's sides, and points one tolerance beside them.
            let point = |next: &mut dyn FnMut(u64) -> f64| Point {
                x: 20. * next(16) + [0., 1e-7, -1e-7, 2e-6][next(4) as usize],
                y: 20. * next(16) + [0., 1e-7, -1e-7, 2e-6][next(4) as usize],
            };
            let mut points = vec![point(&mut next), point(&mut next)];
            // Half of the routes are orthogonal.
            if i % 2 == 0 {
                points[1].y = points[0].y;
                let mut third = point(&mut next);
                third.x = points[1].x;
                points.push(third);
            }
            let route = Route::new(if i % 4 < 2 { "e" } else { "f" }, points);
            for round in [false, true] {
                let found = container_crossings(&route, round, &r, &straight);
                if reaches(route_reach(&route), &r) {
                    crossing += usize::from(!found.is_empty());
                } else {
                    outside += 1;
                    assert!(found.is_empty(), "{:?} round {round}", route.points);
                }
            }
        }
        assert!(
            outside > 1000 && crossing > 1000,
            "{outside} outside, {crossing} crossing"
        );
    }

    #[test]
    fn automatic_ports_face_neighbors_in_all_directions() {
        let graph: Graph = serde_json::from_str(r#"{"nodes":[{"id":"a","kind":"symbol","label":"A","parentId":null},{"id":"b","kind":"symbol","label":"B","parentId":null}],"edges":[{"id":"ab","kind":"calls","source":"a","target":"b"},{"id":"ba","kind":"calls","source":"b","target":"a"}]}"#).unwrap();
        for policy in [
            Mode::Orientation(Orientation::Lr),
            Mode::Orientation(Orientation::Tb),
            Mode::Orientation(Orientation::Rl),
            Mode::Orientation(Orientation::Bt),
            Mode::Chooser(Chooser::Compact),
        ] {
            for (x, y, expected) in [
                (0., 200., PortSide::Top),
                (0., -200., PortSide::Bottom),
                (300., 0., PortSide::Left),
                (-300., 0., PortSide::Right),
            ] {
                let mut g = graph.clone();
                match policy {
                    Mode::Orientation(o) => g.default_direction = DirectionConstraint::Fixed(o),
                    Mode::Chooser(Chooser::Compact) => {
                        g.default_direction = DirectionConstraint::Free
                    }
                    Mode::Chooser(Chooser::Shortest) => {
                        g.default_direction = DirectionConstraint::Free;
                        g.default_strategy = StrategyConstraint::Fixed(LayoutStrategy::Shortest);
                    }
                    _ => {}
                }
                let boxes = BTreeMap::from([
                    (
                        "a".into(),
                        Rect {
                            x: 0.,
                            y: 0.,
                            width: 100.,
                            height: 60.,
                        },
                    ),
                    (
                        "b".into(),
                        Rect {
                            x,
                            y,
                            width: 100.,
                            height: 60.,
                        },
                    ),
                ]);
                let ports = resolve_with(
                    &g,
                    &boxes,
                    PortPlacement::Symmetric,
                    0.,
                    &BTreeMap::new(),
                    arqlet_model::RowPortOrder::Peer,
                );
                let target = ports
                    .iter()
                    .find(|p| p.edge_id == "ab" && p.endpoint == "target")
                    .unwrap();
                assert_eq!(target.side, expected);
                // Incoming and outgoing automatic ports on the same side stay distinct.
                let outgoing = ports
                    .iter()
                    .find(|p| p.edge_id == "ba" && p.endpoint == "source")
                    .unwrap();
                assert_eq!(target.side, outgoing.side);
                assert!(
                    target.position.x != outgoing.position.x
                        || target.position.y != outgoing.position.y
                );
                let context = PortContext::new(&g);
                let routes = crate::route(
                    &g,
                    &boxes,
                    &LayoutOptions::default(),
                    &mut Default::default(),
                    &context,
                );
                let improved = arqlet_routing::improve(&g, &boxes, &routes);
                for route in routes.iter().chain(improved.iter()) {
                    assert!(route
                        .points
                        .windows(2)
                        .all(|w| w[0].x == w[1].x || w[0].y == w[1].y));
                    assert!(!arqlet_routing::hits(route, &boxes["a"]));
                    assert!(!arqlet_routing::hits(route, &boxes["b"]));
                    let end = ports
                        .iter()
                        .find(|p| p.edge_id == route.id && p.endpoint == "target")
                        .unwrap();
                    let before = &route.points[route.points.len() - 2];
                    match end.side {
                        PortSide::Bottom => assert!(before.y > end.position.y),
                        PortSide::Top => assert!(before.y < end.position.y),
                        PortSide::Left => assert!(before.x < end.position.x),
                        PortSide::Right => assert!(before.x > end.position.x),
                    }
                }
            }
        }
    }

    #[test]
    fn explicit_port_side_and_offset_drive_route() {
        let g = Graph {
            default_direction: DirectionConstraint::Fixed(Orientation::Lr),
            container_directions: BTreeMap::new(),
            default_strategy: Default::default(),
            container_strategies: Default::default(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![Port {
                id: "out".into(),
                node_id: "a".into(),
                side: PortSide::Bottom,
                offset: 0.25,
                row: None,
            }],
            edge_ports: vec![EdgePorts {
                edge_id: "ab".into(),
                source_port: Some("out".into()),
                target_port: None,
            }],
            nodes: ["a", "b"]
                .iter()
                .map(|id| Node {
                    id: (*id).into(),
                    kind: "symbol".into(),
                    label: (*id).into(),
                    parent_id: None,
                    shape: Default::default(),
                })
                .collect(),
            edges: vec![Edge {
                id: "ab".into(),
                kind: "calls".into(),
                source: "a".into(),
                target: Some("b".into()),
                weight: None,
            }],
            buses: Vec::new(),
        };
        let s = crate::compute(&g, None, &Default::default(), &LayoutOptions::default()).unwrap();
        let port = s.resolved_ports.iter().find(|p| p.id == "out").unwrap();
        let rect = &s.boxes["a"];
        assert_eq!(port.position.x, rect.x + rect.width * 0.25);
        assert_eq!(port.position.y, rect.y + rect.height);
        assert_eq!(s.routes[0].points[0].y, port.position.y);
        assert!(s.routes[0].points[1].y > port.position.y);
        let improved = crate::reroute(&s);
        assert_eq!(improved.routes[0].points[0].y, port.position.y);
        assert!(improved.routes[0].points[1].y > port.position.y);
    }
    #[test]
    fn invalid_port_reference_rejected() {
        let g:Graph=serde_json::from_str(r#"{"nodes":[{"id":"a","kind":"symbol","label":"A","parentId":null}],"edges":[{"id":"aa","kind":"calls","source":"a","target":"a"}],"edgePorts":[{"edgeId":"aa","sourcePort":"missing","targetPort":null}]}"#).unwrap();
        assert!(validate(&g).is_err());
    }
    #[test]
    fn identifies_container_exit_and_entry() {
        let graph:Graph=serde_json::from_str(r#"{"nodes":[{"id":"left","kind":"container","label":"Left","parentId":null},{"id":"right","kind":"container","label":"Right","parentId":null},{"id":"a","kind":"symbol","label":"A","parentId":"left"},{"id":"b","kind":"symbol","label":"B","parentId":"right"}],"edges":[{"id":"ab","kind":"calls","source":"a","target":"b"}]}"#).unwrap();
        let rect = |x, y, w, h| Rect {
            x,
            y,
            width: w,
            height: h,
        };
        let boxes = BTreeMap::from([
            ("left".into(), rect(0., 0., 100., 100.)),
            ("right".into(), rect(200., 0., 100., 100.)),
            ("a".into(), rect(20., 40., 20., 20.)),
            ("b".into(), rect(20., 40., 20., 20.)),
        ]);
        let route = Route::new(
            "ab",
            vec![Point { x: 40., y: 50. }, Point { x: 220., y: 50. }],
        );
        let crossings = boundaries(&graph, &boxes, &[route], &BTreeSet::new());
        assert_eq!(crossings.len(), 2);
        assert_eq!(crossings[0].container_id, "left");
        assert_eq!(crossings[0].side, PortSide::Right);
        assert_eq!(crossings[0].position.x, 100.);
        assert_eq!(crossings[1].container_id, "right");
        assert_eq!(crossings[1].side, PortSide::Left);
        assert_eq!(crossings[1].position.x, 200.);
    }

    #[test]
    fn spread_keeps_order_spacing_and_the_side() {
        // Two ports wanting the same place are pushed apart; the spread stays inside the side.
        assert_eq!(
            spread_to_peers(&[20., 20.], 6., 94., 8.),
            Some(vec![20., 28.])
        );
        // A port wanting the far end is pulled back inside the margin, and the one before it follows.
        assert_eq!(
            spread_to_peers(&[90., 95.], 6., 94., 8.),
            Some(vec![86., 94.])
        );
        // Three ports need 16 units of room: a side of 20 units (margin 6) cannot hold them.
        assert_eq!(spread_to_peers(&[10., 10., 10.], 6., 14., 8.), None);
    }

    #[test]
    fn optimized_ports_meet_their_peer_when_the_side_has_room() {
        // Peer C sits straight to the right of the source: the source port lands on the peer's centre line.
        let graph: Graph = serde_json::from_value(serde_json::json!({
            "nodes": [
                {"id": "a", "kind": "x", "label": "a", "parentId": null},
                {"id": "c", "kind": "x", "label": "c", "parentId": null}
            ],
            "edges": [{"id": "e", "kind": "calls", "source": "a", "target": "c"}]
        }))
        .unwrap();
        let boxes = BTreeMap::from([
            (
                "a".to_string(),
                Rect {
                    x: 0.,
                    y: 0.,
                    width: 180.,
                    height: 200.,
                },
            ),
            (
                "c".to_string(),
                Rect {
                    x: 400.,
                    y: 120.,
                    width: 180.,
                    height: 54.,
                },
            ),
        ]);
        let symmetric = resolve_with(
            &graph,
            &boxes,
            PortPlacement::Symmetric,
            0.,
            &BTreeMap::new(),
            arqlet_model::RowPortOrder::Peer,
        );
        let optimized = resolve_with(
            &graph,
            &boxes,
            PortPlacement::Optimized,
            0.,
            &BTreeMap::new(),
            arqlet_model::RowPortOrder::Peer,
        );
        let source = |ports: &[ResolvedPort]| {
            ports
                .iter()
                .find(|p| p.endpoint == "source")
                .unwrap()
                .position
                .y
        };
        assert_eq!(
            source(&symmetric),
            100.,
            "the symmetric port sits at the middle of the side"
        );
        assert_eq!(
            source(&optimized),
            147.,
            "the optimized port sits at the peer's centre line"
        );
    }
}

#[cfg(test)]
mod row_order_tests {
    use super::*;

    #[test]
    fn row_ports_on_one_row_and_side_follow_their_peers() {
        // Two edges leave the same row on the right: the one to the node above takes the upper place of the row, the one
        // to the node below the lower place. Their routes then do not cross (ADR 0011).
        let graph: Graph = serde_json::from_value(serde_json::json!({
            "nodes": [
                {"id": "order", "kind": "entity", "label": "Order", "parentId": null},
                {"id": "below", "kind": "node", "label": "Below", "parentId": null},
                {"id": "above", "kind": "node", "label": "Above", "parentId": null}
            ],
            "edges": [
                {"id": "to-below", "kind": "x", "source": "order", "target": "below"},
                {"id": "to-above", "kind": "x", "source": "order", "target": "above"}
            ],
            "ports": [
                {"id": "p-below", "nodeId": "order", "side": "right", "offset": 0.5, "row": "r1"},
                {"id": "p-above", "nodeId": "order", "side": "right", "offset": 0.5, "row": "r1"}
            ],
            "edgePorts": [
                {"edgeId": "to-below", "sourcePort": "p-below", "targetPort": null},
                {"edgeId": "to-above", "sourcePort": "p-above", "targetPort": null}
            ]
        }))
        .unwrap();
        let rect = |x: f64, y: f64, width: f64, height: f64| arqlet_model::Rect {
            x,
            y,
            width,
            height,
        };
        let boxes = BTreeMap::from([
            ("order".to_string(), rect(0., 0., 200., 120.)),
            ("below".to_string(), rect(300., 300., 100., 50.)),
            ("above".to_string(), rect(300., -200., 100., 50.)),
        ]);
        let node_rows = BTreeMap::from([(
            "order".to_string(),
            vec![arqlet_model::Row {
                id: "r1".into(),
                label: "r1".into(),
                width: 80.,
                height: 20.,
            }],
        )]);
        let resolved = resolve_with(
            &graph,
            &boxes,
            PortPlacement::Symmetric,
            0.,
            &node_rows,
            arqlet_model::RowPortOrder::Peer,
        );
        let y = |edge: &str| {
            resolved
                .iter()
                .find(|p| p.edge_id == edge && p.endpoint == "source")
                .unwrap()
                .position
                .y
        };
        // The row is 10 + 20 + 10 = 40 units in the box: the upper port is above the lower one, both inside the row.
        assert!(
            y("to-above") < y("to-below"),
            "above {} below {}",
            y("to-above"),
            y("to-below")
        );
        assert!(
            (y("to-above") - (10. + 20. / 3.)).abs() < 1e-6,
            "{}",
            y("to-above")
        );
        assert!(
            (y("to-below") - (10. + 40. / 3.)).abs() < 1e-6,
            "{}",
            y("to-below")
        );
    }
}
