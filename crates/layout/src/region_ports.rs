//! Experimental (`regionPorts`, ADR 0020 step S1-2): border ports for regions of another strategy (organic, tree, radial)
//! in a layered parent. An edge that leaves such a region is laid out with the region's box as its end, so the parent's
//! routing (single pass where it runs) builds its outer leg to a port on the region's border, on the side its flow uses;
//! the inner leg from that port to the node is drawn afterwards by the region's own routing (straight for organic and
//! radial areas and straight routing, axis-parallel otherwise). The route is the two legs joined in logical direction;
//! the snapshot keeps the given graph.
use arqlet_model::math::PortableMath;
use arqlet_model::outline::Outline;
use arqlet_model::{Graph, Mode, Point, PortSide, Rect, ResolvedPort, RoutingStyle, Snapshot};
use arqlet_scene::geom;
use std::collections::{BTreeMap, BTreeSet};

/// The ends of an edge that were moved to a region's box: (region, node inside it) for the source and for the target.
#[derive(Clone, Debug, Default)]
pub(crate) struct Moved {
    pub source: Option<(String, String)>,
    pub target: Option<(String, String)>,
}

/// The graph whose edges end at the regions they leave, and which ends moved; `None` when no edge leaves a region (or the
/// graph has buses or explicit edge ports, whose ends are given on the nodes).
pub(crate) fn split(graph: &Graph) -> Option<(Graph, BTreeMap<String, Moved>)> {
    if !graph.buses.is_empty() || !graph.edge_ports.is_empty() {
        return None;
    }
    let parent: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let containers: BTreeSet<&str> = parent.values().flatten().copied().collect();
    // A region: a container placed by another strategy whose parent area has a fixed flow.
    let regions: BTreeSet<&str> = containers
        .iter()
        .copied()
        .filter(|c| {
            matches!(
                graph.area_mode(Some(c)),
                Ok(Mode::Organic | Mode::Radial | Mode::Tree(_))
            ) && matches!(
                graph.area_mode(parent.get(c).copied().flatten()),
                Ok(Mode::Orientation(_))
            )
        })
        .collect();
    if regions.is_empty() {
        return None;
    }
    let ancestors = |id: &str| -> Vec<&str> {
        let mut chain = Vec::new();
        let mut current = parent.get(id).copied().flatten();
        while let Some(c) = current {
            chain.push(c);
            current = parent.get(c).copied().flatten();
        }
        chain
    };
    let mut out = graph.clone();
    let mut moved = BTreeMap::new();
    for edge in &mut out.edges {
        let Some(target) = edge.target.clone() else {
            continue;
        };
        if edge.source == target {
            continue;
        }
        let (up_s, up_t) = (ancestors(&edge.source), ancestors(&target));
        // The outermost region below the edge's area that holds the end, if any.
        let lift = |chain: &[&str], other: &[&str], other_id: &str| -> Option<String> {
            chain
                .iter()
                .take_while(|c| !other.contains(c) && **c != other_id)
                .filter(|c| regions.contains(**c))
                .last()
                .map(|c| c.to_string())
        };
        let source_region = lift(&up_s, &up_t, &target);
        let target_region = lift(&up_t, &up_s, &edge.source);
        if source_region.is_none() && target_region.is_none() {
            continue;
        }
        let mut m = Moved::default();
        if let Some(r) = source_region {
            m.source = Some((r.clone(), edge.source.clone()));
            edge.source = r;
        }
        if let Some(r) = target_region {
            m.target = Some((r.clone(), target.clone()));
            edge.target = Some(r);
        }
        moved.insert(edge.id.clone(), m);
    }
    (!moved.is_empty()).then_some((out, moved))
}

/// Joins the inner legs to the outer routes and gives the snapshot the given graph back. An inner leg runs from the port on
/// the region's border to the node's outline: one segment where the region draws straight edges, otherwise axis-parallel
/// (straight on where the node lies across from the port, else one bend at the node's centre line).
pub(crate) fn join(snapshot: &mut Snapshot, graph: &Graph, moved: &BTreeMap<String, Moved>) {
    let absolute = crate::absolute_all(graph, &snapshot.boxes);
    let shape: BTreeMap<&str, arqlet_model::NodeShape> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.shape))
        .collect();
    let straight_region = |region: &str| straight_region(graph, region);
    let parent: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let mut ports: Vec<ResolvedPort> = Vec::new();
    for route in &mut snapshot.routes {
        let Some(m) = moved.get(&route.id) else {
            continue;
        };
        if route.points.len() < 2 {
            continue;
        }
        if let Some((region, node)) = &m.target {
            let port = route.points.last().unwrap().clone();
            let leg = inner_leg(
                &port,
                node,
                region,
                &absolute,
                &parent,
                &shape,
                straight_region(region),
            );
            ports.push(resolved(
                &route.id,
                node,
                "target",
                leg.last().unwrap(),
                &absolute[node],
            ));
            route.points.extend(leg.into_iter().skip(1));
        }
        if let Some((region, node)) = &m.source {
            let port = route.points[0].clone();
            let mut leg = inner_leg(
                &port,
                node,
                region,
                &absolute,
                &parent,
                &shape,
                straight_region(region),
            );
            ports.push(resolved(
                &route.id,
                node,
                "source",
                leg.last().unwrap(),
                &absolute[node],
            ));
            leg.reverse();
            leg.pop();
            leg.extend(route.points.drain(..));
            route.points = leg;
        }
    }
    // The ports of the moved ends are now on the nodes.
    snapshot.resolved_ports.retain(|p| {
        let Some(m) = moved.get(&p.edge_id) else {
            return true;
        };
        !(p.endpoint == "source" && m.source.is_some()
            || p.endpoint == "target" && m.target.is_some())
    });
    snapshot.resolved_ports.extend(ports);
    snapshot.graph = graph.clone();
    let straight: BTreeSet<String> = snapshot.straight_edges.iter().cloned().collect();
    let drawn: Vec<arqlet_model::Route> = snapshot
        .routes
        .iter()
        .filter(|r| r.bundle_of.is_none())
        .cloned()
        .collect();
    snapshot.boundary_crossings =
        crate::ports::boundaries(&snapshot.graph, &snapshot.boxes, &drawn, &straight);
}

/// The distance an inner leg keeps from the other nodes of its region where it has to go around them.
const LEG_MARGIN: f64 = 8.;

/// The inner leg from `port` on the region's border to the outline of `node`, starting at the port: the simple leg (one
/// segment where the region draws straight edges, else axis-parallel with at most one bend) when it enters no other node
/// of the region, otherwise the shortest polyline around those nodes (a visibility graph over their corners, `LEG_MARGIN`
/// away), and the simple leg when there is none.
fn inner_leg(
    port: &Point,
    node: &str,
    region: &str,
    absolute: &BTreeMap<String, Rect>,
    parent: &BTreeMap<&str, Option<&str>>,
    shape: &BTreeMap<&str, arqlet_model::NodeShape>,
    straight: bool,
) -> Vec<Point> {
    let rect = &absolute[node];
    let node_shape = shape[node];
    // The other nodes of the region the leg must not enter: every node inside the region except the node and the
    // containers that hold it.
    let holding: BTreeSet<&str> =
        std::iter::successors(Some(node), |id| parent.get(id).copied().flatten()).collect();
    let inside = |id: &str| {
        std::iter::successors(parent.get(id).copied().flatten(), |c| {
            parent.get(c).copied().flatten()
        })
        .any(|c| c == region)
    };
    let obstacles: Vec<geom::Rect> = absolute
        .iter()
        .filter(|(id, _)| !holding.contains(id.as_str()) && inside(id))
        .map(|(_, r)| geom::Rect {
            x0: r.x,
            y0: r.y,
            x1: r.x + r.width,
            y1: r.y + r.height,
        })
        .collect();
    let clear = |points: &[Point]| {
        points
            .windows(2)
            .all(|w| !obstacles.iter().any(|o| geom::enters(&w[0], &w[1], o)))
    };
    let simple = simple_leg(port, rect, node_shape, straight);
    if clear(&simple) {
        return simple;
    }
    let area = &absolute[region];
    match around(port, rect, node_shape, &obstacles, area) {
        Some(path) => path,
        None => simple,
    }
}

/// The shortest polyline from `port` to the node in `rect` around `obstacles`, inside `area`: Dijkstra over the corners of
/// the obstacles pushed `LEG_MARGIN` out, the port, and the node's centre (the last segment cut at its outline).
fn around(
    port: &Point,
    rect: &Rect,
    shape: arqlet_model::NodeShape,
    obstacles: &[geom::Rect],
    area: &Rect,
) -> Option<Vec<Point>> {
    let centre = Point {
        x: rect.x + rect.width / 2.,
        y: rect.y + rect.height / 2.,
    };
    let within = |p: &Point| {
        p.x >= area.x && p.x <= area.x + area.width && p.y >= area.y && p.y <= area.y + area.height
    };
    let mut points = vec![port.clone(), centre.clone()];
    for o in obstacles {
        for (x, y) in [(o.x0, o.y0), (o.x1, o.y0), (o.x0, o.y1), (o.x1, o.y1)] {
            let p = Point {
                x: x + if x == o.x0 { -LEG_MARGIN } else { LEG_MARGIN },
                y: y + if y == o.y0 { -LEG_MARGIN } else { LEG_MARGIN },
            };
            if within(&p) && !obstacles.iter().any(|q| inside_rect(&p, q)) {
                points.push(p);
            }
        }
    }
    // The node's own box is no obstacle for the segment into its centre; every other segment must not enter it either.
    let own = geom::Rect {
        x0: rect.x,
        y0: rect.y,
        x1: rect.x + rect.width,
        y1: rect.y + rect.height,
    };
    let free = |a: usize, b: usize| {
        let (p, q) = (&points[a], &points[b]);
        !obstacles.iter().any(|o| geom::enters(p, q, o))
            && (a == 1 || b == 1 || !geom::enters(p, q, &own))
    };
    let n = points.len();
    let mut dist = vec![f64::INFINITY; n];
    let mut from = vec![usize::MAX; n];
    let mut done = vec![false; n];
    dist[0] = 0.;
    for _ in 0..n {
        let Some(u) = (0..n)
            .filter(|&i| !done[i] && dist[i].is_finite())
            .min_by(|&a, &b| dist[a].total_cmp(&dist[b]).then(a.cmp(&b)))
        else {
            break;
        };
        done[u] = true;
        if u == 1 {
            break;
        }
        for v in 0..n {
            if done[v] || !free(u, v) {
                continue;
            }
            let d = dist[u] + (points[u].x - points[v].x).p_hypot(points[u].y - points[v].y);
            if d < dist[v] {
                dist[v] = d;
                from[v] = u;
            }
        }
    }
    if !dist[1].is_finite() {
        return None;
    }
    let mut path = vec![1];
    while *path.last().unwrap() != 0 {
        path.push(from[*path.last().unwrap()]);
    }
    path.reverse();
    let mut out: Vec<Point> = path.iter().map(|&i| points[i].clone()).collect();
    let before = out[out.len() - 2].clone();
    *out.last_mut().unwrap() = towards(&before, &centre, rect, shape);
    Some(out)
}

fn inside_rect(p: &Point, r: &geom::Rect) -> bool {
    p.x > r.x0 && p.x < r.x1 && p.y > r.y0 && p.y < r.y1
}

/// The simple leg: one segment to the outline where the region draws straight edges; otherwise axis-parallel, straight on
/// where the node lies across from the port, else one bend at the node's centre line.
fn simple_leg(
    port: &Point,
    rect: &Rect,
    shape: arqlet_model::NodeShape,
    straight: bool,
) -> Vec<Point> {
    let centre = Point {
        x: rect.x + rect.width / 2.,
        y: rect.y + rect.height / 2.,
    };
    if straight {
        return vec![port.clone(), towards(port, &centre, rect, shape)];
    }
    let across_x = port.y > rect.y && port.y < rect.y + rect.height;
    let across_y = port.x > rect.x && port.x < rect.x + rect.width;
    if across_x {
        let x = if port.x < centre.x {
            rect.x
        } else {
            rect.x + rect.width
        };
        return vec![port.clone(), Point { x, y: port.y }];
    }
    if across_y {
        let y = if port.y < centre.y {
            rect.y
        } else {
            rect.y + rect.height
        };
        return vec![port.clone(), Point { x: port.x, y }];
    }
    let bend = Point {
        x: centre.x,
        y: port.y,
    };
    let y = if port.y < centre.y {
        rect.y
    } else {
        rect.y + rect.height
    };
    vec![port.clone(), bend, Point { x: centre.x, y }]
}

/// Where the line from `from` to the node's centre meets its outline.
fn towards(from: &Point, centre: &Point, rect: &Rect, shape: arqlet_model::NodeShape) -> Point {
    let (dx, dy) = (from.x - centre.x, from.y - centre.y);
    let length = (dx * dx + dy * dy).sqrt();
    if length < 1e-9 {
        return centre.clone();
    }
    match Outline::of(shape, rect) {
        Outline::Disc { x, y, radius } => Point {
            x: x + dx / length * radius,
            y: y + dy / length * radius,
        },
        Outline::Rect(_) => {
            // The smaller scale that reaches a side of the box.
            let sx = if dx.abs() > 1e-9 {
                rect.width / 2. / dx.abs()
            } else {
                f64::INFINITY
            };
            let sy = if dy.abs() > 1e-9 {
                rect.height / 2. / dy.abs()
            } else {
                f64::INFINITY
            };
            let s = sx.min(sy).min(1.);
            Point {
                x: centre.x + dx * s,
                y: centre.y + dy * s,
            }
        }
    }
}

fn resolved(edge: &str, node: &str, endpoint: &str, at: &Point, rect: &Rect) -> ResolvedPort {
    let eps = 1e-6;
    let side = if (at.x - rect.x).abs() < eps {
        PortSide::Left
    } else if (at.x - rect.x - rect.width).abs() < eps {
        PortSide::Right
    } else if (at.y - rect.y).abs() < eps {
        PortSide::Top
    } else if (at.y - rect.y - rect.height).abs() < eps {
        PortSide::Bottom
    } else {
        // On a circle: the side its direction from the centre points to.
        let (dx, dy) = (
            at.x - rect.x - rect.width / 2.,
            at.y - rect.y - rect.height / 2.,
        );
        if dx.abs() >= dy.abs() {
            if dx < 0. {
                PortSide::Left
            } else {
                PortSide::Right
            }
        } else if dy < 0. {
            PortSide::Top
        } else {
            PortSide::Bottom
        }
    };
    ResolvedPort {
        id: format!("auto:{edge}:{endpoint}"),
        node_id: node.into(),
        edge_id: edge.into(),
        endpoint: endpoint.into(),
        side,
        position: at.clone(),
        explicit: false,
    }
}

/// Whether the region draws its edges straight: organic and radial areas, and areas with straight routing.
fn straight_region(graph: &Graph, region: &str) -> bool {
    matches!(
        graph.area_mode(Some(region)),
        Ok(Mode::Organic | Mode::Radial)
    ) || graph
        .routing_constraint(Some(region))
        .is_some_and(|c| c.picks(RoutingStyle::Straight))
}

/// The distance between two ports on one side of a region, and from the side's ends.
const PORT_GAP: f64 = 16.;

/// The split graph with an explicit port for every moved end on its region's box: on the side the first layout gave it, at
/// the height (or, on a top or bottom side, the place) of the node inside, so that the inner leg can run straight on;
/// ports of one side keep the order of their nodes and `PORT_GAP` apart.
pub(crate) fn with_ports(
    split: &Graph,
    moved: &BTreeMap<String, Moved>,
    first: &Snapshot,
) -> Graph {
    let absolute = crate::absolute_all(split, &first.boxes);
    // Per region and side: the moved ends with the places of their nodes along the side.
    let mut sides: BTreeMap<(String, u8), Vec<Entry>> = BTreeMap::new();
    for (edge, m) in moved {
        for (end, endpoint) in [(&m.source, "source"), (&m.target, "target")] {
            let Some((region, node)) = end else { continue };
            let Some(port) = first
                .resolved_ports
                .iter()
                .find(|p| &p.edge_id == edge && p.endpoint == endpoint)
            else {
                continue;
            };
            let (Some(n), Some(_)) = (absolute.get(node), absolute.get(region)) else {
                continue;
            };
            // Where the region routes axis-parallel (a tree), its own edges meet a node at the middle of its sides; the leg
            // then meets it a quarter of the node beside the middle, so that the two do not share a line.
            let beside = if straight_region(split, region) {
                0.
            } else {
                0.25
            };
            let along = match port.side {
                PortSide::Left | PortSide::Right => n.y + n.height * (0.5 + beside),
                PortSide::Top | PortSide::Bottom => n.x + n.width * (0.5 + beside),
            };
            sides
                .entry((region.clone(), port.side as u8))
                .or_default()
                .push(Entry {
                    along,
                    edge: edge.clone(),
                    endpoint,
                });
        }
    }
    let side_of = |code: u8| match code {
        c if c == PortSide::Left as u8 => PortSide::Left,
        c if c == PortSide::Right as u8 => PortSide::Right,
        c if c == PortSide::Top as u8 => PortSide::Top,
        _ => PortSide::Bottom,
    };
    let mut out = split.clone();
    let mut edge_ports: BTreeMap<String, arqlet_model::EdgePorts> = BTreeMap::new();
    for ((region, code), list) in sides {
        let side = side_of(code);
        let r = &absolute[&region];
        let (start, length) = match side {
            PortSide::Left | PortSide::Right => (r.y, r.height),
            PortSide::Top | PortSide::Bottom => (r.x, r.width),
        };
        // The places: the nodes' places in their order, pushed apart forwards and then backwards within the side.
        let mut places: Vec<f64> = list.iter().map(|e| e.along).collect();
        places.sort_by(f64::total_cmp);
        let (lo, hi) = (start + PORT_GAP, start + length - PORT_GAP);
        let mut at: Vec<f64> = places.iter().map(|d| d.clamp(lo, hi.max(lo))).collect();
        for i in 1..at.len() {
            at[i] = at[i].max(at[i - 1] + PORT_GAP);
        }
        if let Some(last) = at.last_mut() {
            *last = last.min(hi.max(lo));
        }
        for i in (0..at.len().saturating_sub(1)).rev() {
            at[i] = at[i].min(at[i + 1] - PORT_GAP);
        }
        // In the order of the nodes inside, so that the inner legs run straight on and do not cross; the order of the
        // parent's elements follows these ports (`hierarchical_sweep` sees the region at them), so the routes outside do
        // not cross for them either where the parent's order allows.
        let mut order: Vec<usize> = (0..list.len()).collect();
        order.sort_by(|&a, &b| {
            list[a]
                .along
                .total_cmp(&list[b].along)
                .then_with(|| list[a].edge.cmp(&list[b].edge))
                .then(list[a].endpoint.cmp(list[b].endpoint))
        });
        for (e, place) in order.iter().map(|&k| &list[k]).zip(at) {
            let (edge, endpoint) = (&e.edge, &e.endpoint);
            let offset = if length > 0. {
                ((place - start) / length).clamp(0., 1.)
            } else {
                0.5
            };
            let id = format!("region-port:{edge}:{endpoint}");
            out.ports.push(arqlet_model::Port {
                id: id.clone(),
                node_id: region.clone(),
                side,
                offset,
                row: None,
            });
            let entry = edge_ports
                .entry(edge.clone())
                .or_insert_with(|| arqlet_model::EdgePorts {
                    edge_id: edge.clone(),
                    source_port: None,
                    target_port: None,
                });
            if *endpoint == "source" {
                entry.source_port = Some(id);
            } else {
                entry.target_port = Some(id);
            }
        }
    }
    out.edge_ports.extend(edge_ports.into_values());
    out
}

/// A moved end on one side of a region: the place of its node along the side, and its edge and endpoint.
#[derive(Clone, Debug)]
struct Entry {
    along: f64,
    edge: String,
    endpoint: &'static str,
}

/// Experimental (`regionChildPorts`, ADR 0020 S1-3): the straight edges of an organic or radial area that end inside one
/// of its containers with a fixed flow run inside that container along its ranks to the border node its plan gave the
/// edge, and leave it there: the inner leg passes the dummies of the edge's chain (each on its lane, changing lanes in the
/// channels between ranks), and the area's leg runs straight from the container's border to the other end. An edge keeps
/// its route where an end has no border node or the straight leg would enter a node it does not end in.
pub(crate) fn child_legs(snapshot: &mut Snapshot) {
    let graph = snapshot.graph.clone();
    let parent: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let absolute = crate::absolute_all(&graph, &snapshot.boxes);
    let shape: BTreeMap<&str, arqlet_model::NodeShape> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.shape))
        .collect();
    let straight: BTreeSet<&str> = snapshot.straight_edges.iter().map(String::as_str).collect();
    let ancestors = |id: &str| -> Vec<&str> {
        std::iter::successors(parent.get(id).copied().flatten(), |c| {
            parent.get(c).copied().flatten()
        })
        .collect()
    };
    let mut changed: BTreeMap<String, Vec<Point>> = BTreeMap::new();
    let mut new_ports: Vec<ResolvedPort> = Vec::new();
    for edge in &graph.edges {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        if edge.source == target || !straight.contains(edge.id.as_str()) {
            continue;
        }
        let Some(route) = snapshot.routes.iter().find(|r| r.id == edge.id) else {
            continue;
        };
        if route.points.len() < 2 {
            continue;
        }
        let (up_s, up_t) = (ancestors(&edge.source), ancestors(target));
        // The outermost container with a fixed flow inside an organic or radial area, below the edge's area, for each end.
        let lift = |chain: &[&str], other: &[&str], other_id: &str| -> Option<String> {
            chain
                .iter()
                .take_while(|c| !other.contains(c) && **c != other_id)
                .filter(|c| layered_child(&graph, c, &parent).is_some())
                .last()
                .map(|c| c.to_string())
        };
        let ends = [
            (edge.source.as_str(), lift(&up_s, &up_t, target), true),
            (target, lift(&up_t, &up_s, &edge.source), false),
        ];
        if ends.iter().all(|(_, c, _)| c.is_none()) {
            continue;
        }
        // The legs inside, each from the node to the container's border.
        let mut legs: Vec<Option<Vec<Point>>> = Vec::new();
        let mut ok = true;
        for (node, container, _) in &ends {
            let Some(c) = container else {
                legs.push(None);
                continue;
            };
            match chain_leg(snapshot, &edge.id, node, c, &absolute, &parent, &shape) {
                Some(leg) => legs.push(Some(leg)),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        // An end without a leg inside is met on its outline towards the other end's border point (its earlier port faced
        // the earlier line).
        let border_s = legs[0].as_ref().map(|l| l.last().unwrap().clone());
        let border_t = legs[1].as_ref().map(|l| l.last().unwrap().clone());
        let meet = |id: &str, towards_point: &Point| {
            let r = &absolute[id];
            let centre = Point {
                x: r.x + r.width / 2.,
                y: r.y + r.height / 2.,
            };
            towards(towards_point, &centre, r, shape[id])
        };
        let start = match (&border_s, &border_t) {
            (Some(p), _) => p.clone(),
            (None, Some(q)) => meet(&edge.source, q),
            (None, None) => route.points[0].clone(),
        };
        let end = match (&border_t, &border_s) {
            (Some(q), _) => q.clone(),
            (None, Some(p)) => meet(target, p),
            (None, None) => route.points.last().unwrap().clone(),
        };
        // The straight leg must enter no node, the edge's own ends and their containers included: it starts on a
        // container's border (or an end's outline) and leaves it. Only the containers that hold an end without a leg
        // inside may be crossed, as by any straight edge.
        let holding: BTreeSet<&str> = ends
            .iter()
            .zip(&legs)
            .filter(|(_, leg)| leg.is_none())
            .flat_map(|((id, _, _), _)| {
                std::iter::successors(parent.get(id).copied().flatten(), |c| {
                    parent.get(c).copied().flatten()
                })
            })
            .collect();
        let free = |a: &Point, b: &Point| {
            absolute.iter().all(|(id, r)| {
                holding.contains(id.as_str())
                    || !Outline::of(shape[id.as_str()], r).segment_enters(a.x, a.y, b.x, b.y, 0.)
            })
        };
        // Straight where that is free; else around the corners of the containers the edge leaves (the other end may lie
        // across the container's flow, or on the side the abstract placement did not foresee).
        let lifted: Vec<&Rect> = ends
            .iter()
            .zip(&legs)
            .filter_map(|((_, c, _), leg)| leg.as_ref().and(c.as_ref()))
            .map(|c| &absolute[c])
            .collect();
        let Some(middle) = around_boxes(&start, &end, &lifted, &free) else {
            continue;
        };
        let mut points: Vec<Point> = match &legs[0] {
            Some(l) => l.clone(),
            None => vec![start.clone()],
        };
        points.extend(middle);
        match &legs[1] {
            Some(l) => points.extend(l.iter().rev().cloned()),
            None => points.push(end.clone()),
        }
        points.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
        for ((node, container, _), leg) in ends.iter().zip(&legs) {
            if let (Some(_), Some(l)) = (container, leg) {
                new_ports.push(resolved(
                    &edge.id,
                    node,
                    if *node == edge.source.as_str() {
                        "source"
                    } else {
                        "target"
                    },
                    &l[0],
                    &absolute[*node],
                ));
            }
        }
        changed.insert(edge.id.clone(), points);
    }
    if changed.is_empty() {
        return;
    }
    for route in &mut snapshot.routes {
        if let Some(points) = changed.remove(&route.id) {
            route.points = points;
        }
    }
    snapshot.resolved_ports.retain(|p| {
        !new_ports
            .iter()
            .any(|n| n.edge_id == p.edge_id && n.endpoint == p.endpoint)
    });
    snapshot.resolved_ports.extend(new_ports);
    let straight: BTreeSet<String> = snapshot.straight_edges.iter().cloned().collect();
    let drawn: Vec<arqlet_model::Route> = snapshot
        .routes
        .iter()
        .filter(|r| r.bundle_of.is_none())
        .cloned()
        .collect();
    snapshot.boundary_crossings =
        crate::ports::boundaries(&snapshot.graph, &snapshot.boxes, &drawn, &straight);
}

/// The leg of `edge` from `node` to the border of `container` along the container's plan: from the node (or the child of the
/// container that holds it) through the dummies of the chain to the edge's border node, changing lanes in the channels, and
/// on to the container's border. Starts on the node's outline. `None` when the plan has no border node for the edge.
#[allow(clippy::too_many_arguments)]
fn chain_leg(
    snapshot: &Snapshot,
    edge: &str,
    node: &str,
    container: &str,
    absolute: &BTreeMap<String, Rect>,
    parent: &BTreeMap<&str, Option<&str>>,
    shape: &BTreeMap<&str, arqlet_model::NodeShape>,
) -> Option<Vec<Point>> {
    let plan = snapshot.layered_plans.get(container)?;
    if plan.reversed {
        return None;
    }
    let border = plan.borders.get(edge)?;
    // The leg runs to the end of the flow when the border node stands in the last rank, else to its start (the side the
    // abstract placement chose, `child_sides`, or the flow's own).
    let leaving = plan.ranks.last().is_some_and(|rank| rank.contains(border));
    let c = &absolute[container];
    let at = |r: &Rect| Rect {
        x: c.x + r.x,
        y: c.y + r.y,
        width: r.width,
        height: r.height,
    };
    let border_rect = at(plan.dummies.get(border)?);
    // The child of the container that holds the node (the node itself when it is a child).
    let top = std::iter::successors(Some(node), |id| parent.get(id).copied().flatten())
        .find(|id| parent.get(id).copied().flatten() == Some(container))?;
    let chain = plan.chains.iter().find(|ch| {
        (ch.source == top && ch.target == *border) || (ch.source == *border && ch.target == top)
    })?;
    // In the frame of the flow: `u` along it, `v` across it.
    let vertical = plan.vertical;
    let to_u = |r: &Rect| {
        if vertical {
            (r.y, r.y + r.height)
        } else {
            (r.x, r.x + r.width)
        }
    };
    let to_v = |r: &Rect| {
        if vertical {
            (r.x, r.x + r.width)
        } else {
            (r.y, r.y + r.height)
        }
    };
    let point = |uu: f64, vv: f64| {
        if vertical {
            Point { x: vv, y: uu }
        } else {
            Point { x: uu, y: vv }
        }
    };
    // The elements from the top child to the border node, in the order away from the child.
    let mut lanes: Vec<Rect> = chain
        .dummies
        .iter()
        .filter_map(|d| plan.dummies.get(d).map(at))
        .collect();
    if chain.source == *border {
        lanes.reverse();
    }
    lanes.push(border_rect.clone());
    let lane = |r: &Rect| {
        let (a, b) = to_v(r);
        (a + b) / 2.
    };
    let top_rect = &absolute[top];
    // The start: on the node's side towards the border, at the first lane where it lies across from the node, else at the
    // node's middle with a change of lane in the first channel.
    let n = &absolute[node];
    let (nu0, nu1) = to_u(n);
    let (nv0, nv1) = to_v(n);
    let first_lane = lane(&lanes[0]);
    let start_v = if first_lane > nv0 + 2. && first_lane < nv1 - 2. {
        first_lane
    } else {
        (nv0 + nv1) / 2.
    };
    let start_u = if leaving { nu1 } else { nu0 };
    let (c0, c1) = to_u(c);
    let border_u = if leaving { c1 } else { c0 };
    // A straight lane from the node to the container's border, free of every other node inside the container: the one
    // closest to the border node's lane, without changes of lane.
    if top == node {
        let holding: BTreeSet<&str> =
            std::iter::successors(Some(node), |id| parent.get(id).copied().flatten()).collect();
        let inside = |id: &str| {
            std::iter::successors(parent.get(id).copied().flatten(), |x| {
                parent.get(x).copied().flatten()
            })
            .any(|x| x == container)
        };
        let obstacles: Vec<geom::Rect> = absolute
            .iter()
            .filter(|(id, _)| !holding.contains(id.as_str()) && inside(id))
            .map(|(_, r)| geom::Rect {
                x0: r.x,
                y0: r.y,
                x1: r.x + r.width,
                y1: r.y + r.height,
            })
            .collect();
        let goal = lane(lanes.last().unwrap());
        let mut candidates: Vec<f64> = Vec::new();
        let mut t = nv0 + 4.;
        while t <= nv1 - 4. + 1e-9 {
            candidates.push(t);
            t += 2.;
        }
        candidates.sort_by(|a, b| {
            (a - goal)
                .abs()
                .total_cmp(&(b - goal).abs())
                .then(a.total_cmp(b))
        });
        if let Some(v) = candidates.into_iter().find(|&v| {
            let (p, q) = (point(start_u, v), point(border_u, v));
            !obstacles.iter().any(|o| geom::enters(&p, &q, o))
        }) {
            return Some(vec![point(start_u, v), point(border_u, v)]);
        }
    }
    let mut points = vec![point(start_u, start_v)];
    let mut previous_u = if leaving {
        to_u(top_rect).1
    } else {
        to_u(top_rect).0
    };
    let mut current_v = start_v;
    // A nested child: from the node straight to the child's side along the flow first (inside the child, around its nodes).
    if top != node {
        let side_u = previous_u;
        let side = point(side_u, current_v);
        let inner = inner_leg(&side, node, top, absolute, parent, shape, false);
        points = inner.into_iter().rev().collect();
    }
    // The extent along the flow of every rank: its children's boxes and its dummies, so that a change of lane happens in
    // the free channel between two ranks, clear of the widest element of each.
    let extent = |rank: &Vec<String>| -> Option<(f64, f64)> {
        rank.iter()
            .filter_map(|id| {
                plan.dummies
                    .get(id)
                    .map(at)
                    .or_else(|| absolute.get(id).cloned())
            })
            .map(|r| to_u(&r))
            .reduce(|(a0, a1), (b0, b1)| (a0.min(b0), a1.max(b1)))
    };
    let rank_of = |id: &str| {
        plan.ranks
            .iter()
            .position(|rank| rank.iter().any(|x| x == id))
    };
    let mut previous_rank = rank_of(top);
    let mut elements: Vec<&str> = chain.dummies.iter().map(String::as_str).collect();
    if chain.source == *border {
        elements.reverse();
    }
    elements.push(border.as_str());
    for (r, id) in lanes.iter().zip(&elements) {
        let (r0, r1) = to_u(r);
        let (near, far) = if leaving { (r0, r1) } else { (r1, r0) };
        let here = rank_of(id);
        let channel = match (
            previous_rank.and_then(|p| extent(&plan.ranks[p])),
            here.and_then(|h| extent(&plan.ranks[h])),
        ) {
            (Some((p0, p1)), Some((h0, h1))) => {
                if leaving {
                    (p1 + h0) / 2.
                } else {
                    (p0 + h1) / 2.
                }
            }
            _ => (previous_u + near) / 2.,
        };
        let target_v = lane(r);
        if (target_v - current_v).abs() > 1e-6 {
            points.push(point(channel, current_v));
            points.push(point(channel, target_v));
            current_v = target_v;
        }
        previous_u = far;
        previous_rank = here;
    }
    // On to the container's border.
    points.push(point(border_u, current_v));
    Some(points)
}

/// Whether `c` has a fixed flow and its parent area is organic or radial (`regionChildPorts`); its flow's axis, if so.
fn layered_child(graph: &Graph, c: &str, parent: &BTreeMap<&str, Option<&str>>) -> Option<bool> {
    match (
        graph.area_mode(Some(c)),
        graph.area_mode(parent.get(c).copied().flatten()),
    ) {
        (Ok(Mode::Orientation(o)), Ok(Mode::Organic | Mode::Radial)) if !o.reversed() => {
            Some(o.vertical())
        }
        _ => None,
    }
}

/// For every edge that leaves a container with a fixed flow inside an organic or radial area towards another child of that
/// area, whether its border node stands at the end of the container's flow: when the other end's child lies after the
/// container along the container's axis in the area's abstract placement (the pivot MDS of the area's children over their
/// links, where the organic placement starts). Keyed `"{container}\u{1}{edge}"`.
pub(crate) fn child_sides(graph: &Graph) -> BTreeMap<String, bool> {
    let parent: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let mut out = BTreeMap::new();
    let mut areas: BTreeSet<Option<&str>> = BTreeSet::new();
    for n in &graph.nodes {
        if layered_child(graph, &n.id, &parent).is_some() {
            areas.insert(n.parent_id.as_deref());
        }
    }
    for area in areas {
        let ids: Vec<String> = graph
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == area)
            .map(|n| n.id.clone())
            .collect();
        // The child of the area that holds a node (the node itself when it is a child).
        let child_of = |id: &str| -> Option<String> {
            std::iter::successors(Some(id), |x| parent.get(x).copied().flatten())
                .find(|x| parent.get(x).copied().flatten() == area)
                .map(str::to_string)
        };
        let mut links: BTreeSet<(String, String)> = BTreeSet::new();
        for e in &graph.edges {
            let Some(t) = e.target.as_deref() else {
                continue;
            };
            if let (Some(a), Some(b)) = (child_of(&e.source), child_of(t)) {
                if a != b {
                    links.insert((a, b));
                }
            }
        }
        let positions: BTreeMap<&str, (f64, f64)> = ids
            .iter()
            .map(String::as_str)
            .zip(crate::organic::abstract_positions(&ids, &links))
            .collect();
        for e in &graph.edges {
            let Some(t) = e.target.as_deref() else {
                continue;
            };
            let (Some(a), Some(b)) = (child_of(&e.source), child_of(t)) else {
                continue;
            };
            if a == b {
                continue;
            }
            for (c, other) in [(&a, &b), (&b, &a)] {
                let Some(vertical) = layered_child(graph, c, &parent) else {
                    continue;
                };
                let (pc, po) = (positions[c.as_str()], positions[other.as_str()]);
                let after = if vertical { po.1 >= pc.1 } else { po.0 >= pc.0 };
                out.insert(format!("{c}\u{1}{}", e.id), after);
            }
        }
    }
    out
}

/// The distance a leg keeps from the corner of a container it goes around.
const CORNER_MARGIN: f64 = 12.;

/// The corners between `start` and `end` (both excluded) of the shortest free way with the fewest corners: none (the
/// straight line), one or two of the corners of `boxes`, each `CORNER_MARGIN` out; `None` when none of these is free.
fn around_boxes(
    start: &Point,
    end: &Point,
    boxes: &[&Rect],
    free: &dyn Fn(&Point, &Point) -> bool,
) -> Option<Vec<Point>> {
    if free(start, end) {
        return Some(vec![]);
    }
    let corners: Vec<Point> = boxes
        .iter()
        .flat_map(|r| {
            let (x0, y0) = (r.x - CORNER_MARGIN, r.y - CORNER_MARGIN);
            let (x1, y1) = (
                r.x + r.width + CORNER_MARGIN,
                r.y + r.height + CORNER_MARGIN,
            );
            [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
        })
        .map(|(x, y)| Point { x, y })
        .collect();
    let length = |path: &[&Point]| -> f64 {
        path.windows(2)
            .map(|w| (w[0].x - w[1].x).p_hypot(w[0].y - w[1].y))
            .sum()
    };
    let mut best: Option<(f64, Vec<Point>)> = None;
    for c in &corners {
        if free(start, c) && free(c, end) {
            let l = length(&[start, c, end]);
            if best.as_ref().is_none_or(|(b, _)| l < *b) {
                best = Some((l, vec![c.clone()]));
            }
        }
    }
    if best.is_some() {
        return best.map(|(_, p)| p);
    }
    for c in &corners {
        if !free(start, c) {
            continue;
        }
        for d in &corners {
            if std::ptr::eq(c, d) || !free(c, d) || !free(d, end) {
                continue;
            }
            let l = length(&[start, c, d, end]);
            if best.as_ref().is_none_or(|(b, _)| l < *b) {
                best = Some((l, vec![c.clone(), d.clone()]));
            }
        }
    }
    best.map(|(_, p)| p)
}
