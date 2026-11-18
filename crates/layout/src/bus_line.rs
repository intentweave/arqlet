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

use arqlet_model::{NodeShape, Point, Rect, Snapshot};
use std::collections::{BTreeMap, BTreeSet};

/// How thick a bar is drawn.
pub(crate) const THICKNESS: f64 = 8.;
/// The gap a bar keeps from a box it stops short of.
const CLEARANCE: f64 = 12.;

/// Draws the `busLine` nodes of `snapshot` as bars, with their edges as taps. Nothing changes without such a node.
pub(crate) fn apply(snapshot: &mut Snapshot) {
    let buses: Vec<String> = snapshot
        .graph
        .nodes
        .iter()
        .filter(|n| n.shape == NodeShape::BusLine && snapshot.boxes.contains_key(&n.id))
        .map(|n| n.id.clone())
        .collect();
    snapshot.bus_lines.clear();
    if buses.is_empty() {
        return;
    }
    let absolute = crate::absolute_all(&snapshot.graph, &snapshot.boxes);
    let parent: BTreeMap<&str, Option<&str>> = snapshot
        .graph
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
        let attached: Vec<(String, String, bool)> = snapshot
            .graph
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
        let horizontal = horizontal_bar(snapshot, parent[bus.as_str()], &members, &absolute);
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
        for m in &members {
            let r = &absolute[*m];
            lo = lo.min(centre(r));
            hi = hi.max(centre(r));
            if centre_of_across(r) < centre_across {
                lower += 1;
            }
        }
        // Stop short of the boxes in the bar's way that are neither a bus, an attached node, nor one that holds the bus.
        let bus_centre = centre(own);
        let band = (
            centre_across - THICKNESS / 2.,
            centre_across + THICKNESS / 2.,
        );
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
            away: if 2 * lower >= members.len() { 1. } else { -1. },
            attached,
        });
    }
    // Bars on one line, axis and side that overlap take lanes beside each other, away from their nodes: in the order of
    // their starts, each the first lane free at its start (a schematic of parallel bus lines).
    let mut groups: BTreeMap<(Option<&str>, bool, i64, i64), Vec<usize>> = BTreeMap::new();
    for (i, plan) in plans.iter().enumerate() {
        let key = (
            parent[plan.bus.as_str()],
            plan.horizontal,
            (plan.across * 2.).round() as i64,
            plan.away as i64,
        );
        groups.entry(key).or_default().push(i);
    }
    let mut offset = vec![0f64; plans.len()];
    for members in groups.values_mut() {
        members.sort_by(|&a, &b| {
            plans[a]
                .lo
                .total_cmp(&plans[b].lo)
                .then(plans[a].hi.total_cmp(&plans[b].hi))
                .then(plans[a].bus.cmp(&plans[b].bus))
        });
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
            offset[i] = lane as f64 * (THICKNESS + CLEARANCE) * plans[i].away;
        }
    }
    let mut taps: BTreeMap<String, Vec<Point>> = BTreeMap::new();
    let mut lines: BTreeMap<String, Rect> = BTreeMap::new();
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
        for (edge, member, from_member) in &plan.attached {
            let mut points = tap(&absolute[member], &bar, plan.horizontal);
            if !from_member {
                points.reverse();
            }
            taps.insert(edge.clone(), points);
        }
        lines.insert(plan.bus.clone(), bar);
    }
    // The bars relative to the bus node's parent, as the boxes are.
    for (bus, bar) in lines {
        let origin = parent[bus.as_str()]
            .and_then(|p| absolute.get(p))
            .map_or((0., 0.), |p| (p.x, p.y));
        snapshot.bus_lines.insert(
            bus,
            Rect {
                x: bar.x - origin.0,
                y: bar.y - origin.1,
                ..bar
            },
        );
    }
    for route in &mut snapshot.routes {
        if route.bundle_of.is_some() {
            continue;
        }
        if let Some(points) = taps.remove(&route.id) {
            route.points = points;
        }
    }
    // The automatic ports at the taps' ends.
    let ends: BTreeMap<&str, (Point, Point)> = snapshot
        .routes
        .iter()
        .filter_map(|r| {
            Some((
                r.id.as_str(),
                (r.points.first()?.clone(), r.points.last()?.clone()),
            ))
        })
        .collect();
    for port in snapshot.resolved_ports.iter_mut().filter(|p| !p.explicit) {
        if let Some((first, last)) = ends.get(port.edge_id.as_str()) {
            port.position = if port.endpoint == "source" {
                first.clone()
            } else {
                last.clone()
            };
        }
    }
}

/// Whether a bus's bar lies along x: across the flow of its area when the area has one (chosen or fixed), else along the
/// axis its attached nodes spread more on.
fn horizontal_bar(
    snapshot: &Snapshot,
    parent: Option<&str>,
    members: &BTreeSet<&str>,
    absolute: &BTreeMap<String, Rect>,
) -> bool {
    let chosen = snapshot
        .area_orientations
        .get(parent.unwrap_or("<root>"))
        .copied()
        .or_else(|| arqlet_model::direction_for(&snapshot.graph, parent).orientation());
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
    // The member's side towards the bar, and the bar's side towards the member.
    let above = m_across.1 <= b_across.0 + 1e-9
        || (m_across.0 + m_across.1) / 2. < (b_across.0 + b_across.1) / 2.;
    let (leave, meet) = if above {
        (m_across.1, b_across.0)
    } else {
        (m_across.0, b_across.1)
    };
    if c >= b_along.0 && c <= b_along.1 {
        vec![point(c, leave), point(c, meet)]
    } else {
        let end = if c < b_along.0 { b_along.0 } else { b_along.1 };
        let mid = (b_across.0 + b_across.1) / 2.;
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
