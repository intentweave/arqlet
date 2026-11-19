//! Provisional: a node of shape `busLine` (a physical bus: CAN, LIN, FlexRay, …) is drawn as a bar that runs past the nodes
//! attached to it, and each edge between the bus and one of them as a short tap onto the bar. Applied to the finished
//! layout: the placement keeps the bus node's box (`Snapshot::boxes`, so a later layout continues from the same place), and
//! the bar goes to `Snapshot::bus_lines`, which the scene draws instead of the box.
//!
//! The bar lies across the flow of its area (a fixed or chosen orientation) or, without one, along the axis on which the
//! attached nodes spread more. It runs from the first to the last attached node's centre on that axis, at least the bus
//! node's own length, through the bus node's centre across it, and stops short of any other box in its way. A tap leaves
//! its node on the side facing the bar and meets the bar square; a node beyond the bar's ends meets it at the nearer end
//! with one bend. Edges keep their logical direction.

use arqlet_model::{Bus, BusLine, BusTap, Edge, Graph, Node, NodeShape, Point, Rect, Snapshot};
use std::collections::{BTreeMap, BTreeSet};

/// How thick a bar is drawn.
pub(crate) const THICKNESS: f64 = 8.;
/// The gap a bar keeps from a box it stops short of.
const CLEARANCE: f64 = 12.;

/// The bars and taps of the `busLine` nodes of a laid-out graph (`boxes` relative to their parents, as a snapshot keeps
/// them), in absolute coordinates; the taps in each edge's own direction. Empty without such a node.
pub(crate) fn draw(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    orientations: &BTreeMap<String, arqlet_model::Orientation>,
) -> BTreeMap<String, BusLine> {
    let buses: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| n.shape == NodeShape::BusLine && boxes.contains_key(&n.id))
        .map(|n| n.id.clone())
        .collect();
    if buses.is_empty() {
        return BTreeMap::new();
    }
    let absolute = crate::absolute_all(graph, boxes);
    let parent: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let bus_set: BTreeSet<&str> = buses.iter().map(String::as_str).collect();
    // First every bar's plan: its axis, its span along it, the line across it, and the side away from its nodes.
    struct Plan {
        bus: String,
        horizontal: bool,
        lo: f64,
        hi: f64,
        across: f64,
        /// +1 when the bar's further lanes lie on the higher side across (its nodes lie on the lower one), else -1.
        away: f64,
        attached: Vec<(String, String, bool)>,
    }
    let mut plans: Vec<Plan> = Vec::new();
    for bus in &buses {
        let own = &absolute[bus];
        // The edges between the bus and another visible node, with that node.
        let attached: Vec<(String, String, bool)> = graph
            .edges
            .iter()
            .filter_map(|e| {
                let target = e.target.as_deref()?;
                let (other, from_member) = if e.source == *bus && target != bus {
                    (target, false)
                } else if target == bus && e.source != *bus {
                    (e.source.as_str(), true)
                } else {
                    return None;
                };
                absolute
                    .contains_key(other)
                    .then(|| (e.id.clone(), other.to_string(), from_member))
            })
            .collect();
        if attached.is_empty() {
            continue;
        }
        let members: BTreeSet<&str> = attached.iter().map(|(_, m, _)| m.as_str()).collect();
        let horizontal = horizontal_bar(
            graph,
            orientations,
            parent[bus.as_str()],
            &members,
            &absolute,
        );
        let (along, across) = axes(own, horizontal);
        let centre_across = (across.0 + across.1) / 2.;
        let centre = |r: &Rect| {
            let (a, _) = axes(r, horizontal);
            (a.0 + a.1) / 2.
        };
        let centre_of_across = |r: &Rect| {
            let (_, c) = axes(r, horizontal);
            (c.0 + c.1) / 2.
        };
        let mut lo = along.0;
        let mut hi = along.1;
        let mut lower = 0usize;
        let band = (
            centre_across - THICKNESS / 2.,
            centre_across + THICKNESS / 2.,
        );
        // A node on the bar's own line bounds the bar: it stops a gap short of it, and the node's tap runs along the line.
        let (mut cap_lo, mut cap_hi) = (f64::MIN, f64::MAX);
        for m in &members {
            let r = &absolute[*m];
            let (a, c) = axes(r, horizontal);
            if c.1 > band.0 && c.0 < band.1 {
                if centre(r) >= centre(own) {
                    cap_hi = cap_hi.min(a.0 - CLEARANCE);
                    hi = hi.max(a.0 - CLEARANCE);
                } else {
                    cap_lo = cap_lo.max(a.1 + CLEARANCE);
                    lo = lo.min(a.1 + CLEARANCE);
                }
                continue;
            }
            lo = lo.min(centre(r));
            hi = hi.max(centre(r));
            if centre_of_across(r) < centre_across {
                lower += 1;
            }
        }
        hi = hi.min(cap_hi);
        lo = lo.max(cap_lo);
        // Stop short of the boxes in the bar's way that are neither a bus, an attached node, nor one that holds the bus.
        let bus_centre = centre(own);
        let holds_bus = |id: &str| {
            let mut at = parent.get(bus.as_str()).copied().flatten();
            while let Some(p) = at {
                if p == id {
                    return true;
                }
                at = parent.get(p).copied().flatten();
            }
            false
        };
        for (id, r) in &absolute {
            if bus_set.contains(id.as_str()) || members.contains(id.as_str()) || holds_bus(id) {
                continue;
            }
            let (a, c) = axes(r, horizontal);
            if c.1 <= band.0 || c.0 >= band.1 || a.1 <= lo || a.0 >= hi {
                continue;
            }
            if a.0 >= bus_centre {
                hi = hi.min(a.0 - CLEARANCE).max(along.1);
            } else if a.1 <= bus_centre {
                lo = lo.max(a.1 + CLEARANCE).min(along.0);
            }
        }
        plans.push(Plan {
            bus: bus.clone(),
            horizontal,
            lo,
            hi,
            across: centre_across,
            away: if 2 * lower >= members.len().max(1) {
                1.
            } else {
                -1.
            },
            attached,
        });
    }
    // Bars on one line, axis and side that overlap take lanes beside each other, away from their nodes: in the order of
    // their starts, each the first lane free at its start (a schematic of parallel bus lines).
    // A group: the bars of one area, axis and side whose lines lie closer than a lane apart (in the order of their lines).
    let mut by_side: BTreeMap<(Option<&str>, bool, i64), Vec<usize>> = BTreeMap::new();
    for (i, plan) in plans.iter().enumerate() {
        by_side
            .entry((parent[plan.bus.as_str()], plan.horizontal, plan.away as i64))
            .or_default()
            .push(i);
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    for mut side in by_side.into_values() {
        side.sort_by(|&a, &b| {
            plans[a]
                .across
                .total_cmp(&plans[b].across)
                .then(plans[a].bus.cmp(&plans[b].bus))
        });
        let mut start = f64::MIN;
        for i in side {
            if plans[i].across - start > THICKNESS + CLEARANCE || groups.is_empty() {
                groups.push(Vec::new());
                start = plans[i].across;
            }
            groups.last_mut().expect("a group").push(i);
        }
    }
    let mut offset = vec![0f64; plans.len()];
    for members in groups.iter_mut() {
        members.sort_by(|&a, &b| {
            plans[a]
                .lo
                .total_cmp(&plans[b].lo)
                .then(plans[a].hi.total_cmp(&plans[b].hi))
                .then(plans[a].bus.cmp(&plans[b].bus))
        });
        // Every bar of the group on the line of its first, then lanes away from their nodes.
        let line = members
            .iter()
            .map(|&i| plans[i].across)
            .fold(f64::MAX, f64::min);
        let mut lanes: Vec<f64> = Vec::new();
        for &i in members.iter() {
            let lane = match lanes.iter().position(|&end| end + CLEARANCE < plans[i].lo) {
                Some(free) => free,
                None => {
                    lanes.push(f64::MIN);
                    lanes.len() - 1
                }
            };
            lanes[lane] = plans[i].hi;
            let base = if plans[i].away > 0. {
                line
            } else {
                members
                    .iter()
                    .map(|&j| plans[j].across)
                    .fold(f64::MIN, f64::max)
            };
            offset[i] =
                base - plans[i].across + lane as f64 * (THICKNESS + CLEARANCE) * plans[i].away;
        }
    }
    let mut lines: BTreeMap<String, BusLine> = BTreeMap::new();
    for (i, plan) in plans.iter().enumerate() {
        let across = plan.across + offset[i] - THICKNESS / 2.;
        let bar = if plan.horizontal {
            Rect {
                x: plan.lo,
                y: across,
                width: plan.hi - plan.lo,
                height: THICKNESS,
            }
        } else {
            Rect {
                x: across,
                y: plan.lo,
                width: THICKNESS,
                height: plan.hi - plan.lo,
            }
        };
        let mut taps = BTreeMap::new();
        for (edge, member, from_member) in &plan.attached {
            let mut points = tap(&absolute[member], &bar, plan.horizontal);
            if !from_member {
                points.reverse();
            }
            taps.insert(edge.clone(), points);
        }
        lines.insert(plan.bus.clone(), BusLine { bar, taps });
    }
    lines
}

/// Whether a bus's bar lies along x: across the flow of its area when the area has one (chosen or fixed), else along the
/// axis its attached nodes spread more on.
fn horizontal_bar(
    graph: &Graph,
    orientations: &BTreeMap<String, arqlet_model::Orientation>,
    parent: Option<&str>,
    members: &BTreeSet<&str>,
    absolute: &BTreeMap<String, Rect>,
) -> bool {
    let chosen = orientations
        .get(parent.unwrap_or("<root>"))
        .copied()
        .or_else(|| arqlet_model::direction_for(graph, parent).orientation());
    if let Some(orientation) = chosen {
        return orientation.vertical();
    }
    let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for m in members {
        let r = &absolute[*m];
        let (cx, cy) = (r.x + r.width / 2., r.y + r.height / 2.);
        x0 = x0.min(cx);
        x1 = x1.max(cx);
        y0 = y0.min(cy);
        y1 = y1.max(cy);
    }
    x1 - x0 >= y1 - y0
}

/// The tap from `member` onto `bar`: from the side of the member that faces the bar, square onto it, or with one bend to
/// the nearer end when the member lies beyond the bar.
fn tap(member: &Rect, bar: &Rect, horizontal: bool) -> Vec<Point> {
    let point = |along: f64, across: f64| {
        if horizontal {
            Point {
                x: along,
                y: across,
            }
        } else {
            Point {
                x: across,
                y: along,
            }
        }
    };
    let (m_along, m_across) = axes(member, horizontal);
    let (b_along, b_across) = axes(bar, horizontal);
    let c = (m_along.0 + m_along.1) / 2.;
    let mid = (b_across.0 + b_across.1) / 2.;
    // A node on the bar's line: from its side facing the bar along the line to the bar's end.
    if m_across.1 > b_across.0 && m_across.0 < b_across.1 {
        return if c >= b_along.1 {
            vec![point(m_along.0, mid), point(b_along.1, mid)]
        } else if c <= b_along.0 {
            vec![point(m_along.1, mid), point(b_along.0, mid)]
        } else {
            // Across the bar (it was moved to a lane over the node): square from the nearer side.
            let side = if (m_across.0 + m_across.1) / 2. < mid {
                m_across.1
            } else {
                m_across.0
            };
            vec![point(c, side), point(c, mid)]
        };
    }
    // The member's side towards the bar, and the bar's side towards the member.
    let above = m_across.1 <= b_across.0 + 1e-9
        || (m_across.0 + m_across.1) / 2. < (b_across.0 + b_across.1) / 2.;
    let (leave, meet) = if above {
        (m_across.1, b_across.0)
    } else {
        (m_across.0, b_across.1)
    };
    // Square onto the bar where the node and the bar share a stretch along it: at the node's centre when the bar reaches
    // it, else as near to it as the shared stretch allows.
    let (shared_lo, shared_hi) = (m_along.0.max(b_along.0), m_along.1.min(b_along.1));
    if c >= b_along.0 && c <= b_along.1 {
        vec![point(c, leave), point(c, meet)]
    } else if shared_lo + 2. * CLEARANCE <= shared_hi {
        let at = c.clamp(shared_lo + CLEARANCE, shared_hi - CLEARANCE);
        vec![point(at, leave), point(at, meet)]
    } else {
        let end = if c < b_along.0 { b_along.0 } else { b_along.1 };
        vec![point(c, leave), point(c, mid), point(end, mid)]
    }
}

/// A box's extent along the bar and across it.
fn axes(r: &Rect, horizontal: bool) -> ((f64, f64), (f64, f64)) {
    if horizontal {
        ((r.x, r.x + r.width), (r.y, r.y + r.height))
    } else {
        ((r.y, r.y + r.height), (r.x, r.x + r.width))
    }
}

/// Provisional: the edges of the `busLine` nodes, turned for the layering so that the bus network reads as a tree: in every
/// connected network of nodes and buses one root (`root_of`: a gateway, else the middle of the network), a breadth-first
/// search goes node → bus → node from it, and every bus edge points from the end reached first to the end
/// reached later. A gateway then stands above its buses, a node on one bus below it, a node on two buses below both. Only
/// the layout sees the turned edges (`restore` gives the graph back); `None` when the graph has no bus line.
/// The largest bus network whose middle is found exactly (a search from every member, quadratic); a larger one takes the
/// middle of a longest path found by two searches.
const EXACT_MIDDLE_MAX: usize = 2000;

/// The root of one bus network (`network`: its members; `far`: the member a search from its first member reached last): the
/// node on the most buses when it is on three or more (a gateway; the earliest in the graph on a tie), else the middle of the
/// network, which may be a bus (a trunk without gateways, the backbone of zones): the member whose farthest member is
/// nearest, or above `EXACT_MIDDLE_MAX` members the middle of a longest path found by a second search from `far`.
fn root_of<'a>(
    network: &BTreeMap<&'a str, usize>,
    far: &'a str,
    buses: &BTreeSet<&str>,
    adjacent: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    order: &BTreeMap<&str, usize>,
) -> &'a str {
    let gateway = network
        .keys()
        .copied()
        .filter(|id| !buses.contains(id) && adjacent[id].len() >= 3)
        .min_by(|a, b| {
            adjacent[b]
                .len()
                .cmp(&adjacent[a].len())
                .then(order[a].cmp(&order[b]))
        });
    if let Some(g) = gateway {
        return g;
    }
    // The exact middle: the member whose farthest member is nearest, then the one on more edges, then the earliest.
    if network.len() <= EXACT_MIDDLE_MAX {
        return network
            .keys()
            .copied()
            .map(|id| {
                let reach = bfs(id, adjacent, order).0;
                (reach.values().copied().max().unwrap_or(0), id)
            })
            .min_by(|(ea, a), (eb, b)| {
                ea.cmp(eb)
                    .then(adjacent[b].len().cmp(&adjacent[a].len()))
                    .then(order[a].cmp(&order[b]))
            })
            .map(|(_, id)| id)
            .expect("a member");
    }
    let (_, parent, end) = bfs(far, adjacent, order);
    let mut path = vec![end];
    while let Some(&p) = parent.get(path.last().expect("a node")) {
        path.push(p);
    }
    path[path.len() / 2]
}

/// Breadth-first search over the bus network from `from`, neighbours in graph order (so it does not depend on the ids): the
/// level and the parent of every member, and the member reached last.
type Search<'a> = (
    BTreeMap<&'a str, usize>,
    BTreeMap<&'a str, &'a str>,
    &'a str,
);
fn bfs<'a>(
    from: &'a str,
    adjacent: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    order: &BTreeMap<&str, usize>,
) -> Search<'a> {
    let mut level: BTreeMap<&str, usize> = BTreeMap::from([(from, 0)]);
    let mut parent: BTreeMap<&str, &str> = BTreeMap::new();
    let mut queue = std::collections::VecDeque::from([from]);
    let mut last = from;
    while let Some(at) = queue.pop_front() {
        last = at;
        let mut next: Vec<&str> = adjacent[at].iter().copied().collect();
        next.sort_by_key(|id| order[id]);
        for n in next {
            if !level.contains_key(n) {
                level.insert(n, level[at] + 1);
                parent.insert(n, at);
                queue.push_back(n);
            }
        }
    }
    (level, parent, last)
}

pub(crate) fn orient(graph: &Graph) -> Option<(Graph, BTreeSet<String>)> {
    let buses: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.shape == NodeShape::BusLine)
        .map(|n| n.id.as_str())
        .collect();
    if buses.is_empty() {
        return None;
    }
    let order: BTreeMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    // The bus edges: between a bus and a node that is not a bus.
    let mut adjacent: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut bus_edges: Vec<usize> = Vec::new();
    for (k, e) in graph.edges.iter().enumerate() {
        let Some(t) = e.target.as_deref() else {
            continue;
        };
        let (s, t) = (e.source.as_str(), t);
        if s == t
            || buses.contains(s) == buses.contains(t)
            || !order.contains_key(s)
            || !order.contains_key(t)
        {
            continue;
        }
        adjacent.entry(s).or_default().insert(t);
        adjacent.entry(t).or_default().insert(s);
        bus_edges.push(k);
    }
    if bus_edges.is_empty() {
        return None;
    }
    // Breadth-first levels from one root per network (`root_of`).
    let mut starts: Vec<&str> = adjacent.keys().copied().collect();
    starts.sort_by_key(|id| order[id]);
    let mut level: BTreeMap<&str, usize> = BTreeMap::new();
    for start in starts {
        if level.contains_key(start) {
            continue;
        }
        let (network, _, far) = bfs(start, &adjacent, &order);
        let root = root_of(&network, far, &buses, &adjacent, &order);
        level.extend(bfs(root, &adjacent, &order).0);
    }
    let mut oriented = graph.clone();
    let mut flipped = BTreeSet::new();
    for k in bus_edges {
        let e = &mut oriented.edges[k];
        let t = e.target.clone().expect("a bus edge has a target");
        if level.get(e.source.as_str()) > level.get(t.as_str()) {
            e.target = Some(std::mem::replace(&mut e.source, t));
            flipped.insert(e.id.clone());
        }
    }
    if flipped.is_empty() {
        return None;
    }
    for binding in &mut oriented.edge_ports {
        if flipped.contains(&binding.edge_id) {
            std::mem::swap(&mut binding.source_port, &mut binding.target_port);
        }
    }
    Some((oriented, flipped))
}

/// The snapshot of a layout of `orient`'s graph, given back on `graph`: the turned edges' routes run in their own direction
/// again and their ports name their own ends (the taps are drawn anew by `apply`).
pub(crate) fn restore(snapshot: &mut Snapshot, graph: &Graph, flipped: &BTreeSet<String>) {
    snapshot.graph = graph.clone();
    for route in &mut snapshot.routes {
        if flipped.contains(&route.id) {
            route.points.reverse();
        }
    }
    for port in &mut snapshot.resolved_ports {
        if flipped.contains(&port.edge_id) {
            port.endpoint = if port.endpoint == "source" {
                "target".into()
            } else {
                "source".into()
            };
        }
    }
}

/// The view of a graph with `busLine` nodes (ADR 0019 view model): every such node and its edges leave the graph and become
/// a bus line (`Bus::taps`, one tap per edge, the node's id, label, kind and parent). Edges between two bus nodes, and loops
/// on one, are left out. `None` when the graph has no `busLine` node.
pub(crate) fn project(graph: &Graph) -> Option<Graph> {
    let buses: Vec<&Node> = graph
        .nodes
        .iter()
        .filter(|n| n.shape == NodeShape::BusLine)
        .collect();
    if buses.is_empty() {
        return None;
    }
    let ids: BTreeSet<&str> = buses.iter().map(|n| n.id.as_str()).collect();
    let mut taps: BTreeMap<&str, Vec<BusTap>> = BTreeMap::new();
    for e in &graph.edges {
        let (s, t) = (e.source.as_str(), e.target.as_deref());
        match (ids.contains(s), t.map(|t| ids.contains(t))) {
            (true, Some(false)) => taps.entry(s).or_default().push(BusTap {
                edge: e.id.clone(),
                node: t.expect("a target").to_string(),
                to_bus: false,
                kind: e.kind.clone(),
            }),
            (false, Some(true)) => taps.entry(t.expect("a target")).or_default().push(BusTap {
                edge: e.id.clone(),
                node: s.to_string(),
                to_bus: true,
                kind: e.kind.clone(),
            }),
            _ => {}
        }
    }
    let touches = |e: &Edge| {
        ids.contains(e.source.as_str()) || e.target.as_deref().is_some_and(|t| ids.contains(t))
    };
    let dropped: BTreeSet<&str> = graph
        .edges
        .iter()
        .filter(|e| touches(e))
        .map(|e| e.id.as_str())
        .collect();
    let mut view = graph.clone();
    view.nodes.retain(|n| !ids.contains(n.id.as_str()));
    view.edges.retain(|e| !touches(e));
    view.edge_ports
        .retain(|b| !dropped.contains(b.edge_id.as_str()));
    view.ports.retain(|p| !ids.contains(p.node_id.as_str()));
    for node in buses {
        view.buses.push(Bus {
            id: node.id.clone(),
            edges: Vec::new(),
            label: Some(node.label.clone()),
            taps: taps.remove(node.id.as_str()).unwrap_or_default(),
            kind: Some(node.kind.clone()),
            parent_id: node.parent_id.clone(),
        });
    }
    Some(view)
}

/// The graph the layout sees for a view with bus lines: every bus line as a node of shape `busLine` again, with one edge per
/// tap, after the other nodes and edges. `None` when the view has no bus line.
pub(crate) fn expand(view: &Graph) -> Option<Graph> {
    if !view.buses.iter().any(|b| !b.taps.is_empty()) {
        return None;
    }
    let present: BTreeSet<&str> = view.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut graph = view.clone();
    graph.buses.retain(|b| b.taps.is_empty());
    for bus in view.buses.iter().filter(|b| !b.taps.is_empty()) {
        graph.nodes.push(Node {
            id: bus.id.clone(),
            kind: bus.kind.clone().unwrap_or_else(|| "bus".into()),
            label: bus.label.clone().unwrap_or_else(|| bus.id.clone()),
            parent_id: bus
                .parent_id
                .clone()
                .filter(|p| present.contains(p.as_str())),
            shape: NodeShape::BusLine,
        });
        for tap in bus
            .taps
            .iter()
            .filter(|t| present.contains(t.node.as_str()))
        {
            let (source, target) = if tap.to_bus {
                (tap.node.clone(), bus.id.clone())
            } else {
                (bus.id.clone(), tap.node.clone())
            };
            graph.edges.push(Edge {
                id: tap.edge.clone(),
                kind: tap.kind.clone(),
                source,
                target: Some(target),
                weight: None,
            });
        }
    }
    Some(graph)
}

/// A laid-out snapshot of `expand`'s graph, given back on the view: the bus nodes' boxes move to `bus_boxes` (where the next
/// layout finds them), their taps' routes, ports and border crossings leave, and the bars and taps go to `bus_lines`.
pub(crate) fn collapse(snapshot: &mut Snapshot, view: &Graph) {
    let lines = draw(
        &snapshot.graph,
        &snapshot.boxes,
        &snapshot.area_orientations,
    );
    let line_buses: Vec<&Bus> = view.buses.iter().filter(|b| !b.taps.is_empty()).collect();
    let taps: BTreeSet<&str> = line_buses
        .iter()
        .flat_map(|b| b.taps.iter().map(|t| t.edge.as_str()))
        .collect();
    for bus in &line_buses {
        if let Some(r) = snapshot.boxes.remove(&bus.id) {
            snapshot.bus_boxes.insert(bus.id.clone(), r);
        }
    }
    // The taps' layout state stays apart from the view's (`Snapshot::bus_taps`).
    let (routes, kept) = std::mem::take(&mut snapshot.routes)
        .into_iter()
        .partition(|r| !taps.contains(r.id.as_str()));
    snapshot.routes = routes;
    snapshot.bus_taps.routes = kept;
    let (ports, kept) = std::mem::take(&mut snapshot.resolved_ports)
        .into_iter()
        .partition(|p| !taps.contains(p.edge_id.as_str()));
    snapshot.resolved_ports = ports;
    snapshot.bus_taps.resolved_ports = kept;
    let (crossings, kept) = std::mem::take(&mut snapshot.boundary_crossings)
        .into_iter()
        .partition(|c| !taps.contains(c.edge_id.as_str()));
    snapshot.boundary_crossings = crossings;
    snapshot.bus_taps.boundary_crossings = kept;
    snapshot.graph = view.clone();
    snapshot.bus_lines = lines;
}

/// The bars and taps of a view's bus lines drawn anew from the snapshot's boxes and the bus boxes the layout left
/// (`bus_boxes`), for an operation that finishes a snapshot without the layout's last step. Nothing changes without a bus
/// line.
pub(crate) fn redraw(snapshot: &mut Snapshot) {
    let Some(graph) = expand(&snapshot.graph) else {
        snapshot.bus_lines.clear();
        return;
    };
    let mut boxes = snapshot.boxes.clone();
    for bus in snapshot.graph.buses.iter().filter(|b| !b.taps.is_empty()) {
        if let Some(r) = snapshot.bus_boxes.get(&bus.id) {
            boxes.insert(bus.id.clone(), r.clone());
        }
    }
    snapshot.bus_lines = draw(&graph, &boxes, &snapshot.area_orientations);
}

/// A snapshot of a view with bus lines, turned back into the snapshot of the graph the layout sees (`expand`): the bus nodes
/// and their edges in the graph, their boxes, and the taps' routes, ports and crossings as the layout left them. An
/// operation that continues a layout works on this; `collapse_snapshot` gives the view back. Nothing changes (and `false`)
/// without a bus line.
pub(crate) fn expand_snapshot(snapshot: &mut Snapshot) -> bool {
    let Some(graph) = expand(&snapshot.graph) else {
        return false;
    };
    for bus in snapshot.graph.buses.iter().filter(|b| !b.taps.is_empty()) {
        if let Some(r) = snapshot.bus_boxes.remove(&bus.id) {
            snapshot.boxes.insert(bus.id.clone(), r);
        }
    }
    let kept = std::mem::take(&mut snapshot.bus_taps);
    snapshot.routes.extend(kept.routes);
    snapshot.resolved_ports.extend(kept.resolved_ports);
    snapshot.boundary_crossings.extend(kept.boundary_crossings);
    snapshot.bus_lines.clear();
    snapshot.graph = graph;
    true
}

/// The view of a snapshot whose graph has nodes of shape `busLine` (from `expand_snapshot`, or an operation that finished
/// one): every such node becomes a bus line, drawn from the boxes. A snapshot of a view already is its own view: its bus
/// lines are drawn anew. Applying it again changes nothing.
pub(crate) fn collapse_snapshot(snapshot: &mut Snapshot) {
    match project(&snapshot.graph) {
        Some(view) => collapse(snapshot, &view),
        None => redraw(snapshot),
    }
}
