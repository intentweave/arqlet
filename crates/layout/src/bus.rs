//! Provisional: explicit buses (ADR 0005, `LayoutOptions::buses`). The edges of a bus that share an end (all one source, or
//! all one target) are drawn on one trunk: they leave (enter) that node by one port, share the stub and the way across the
//! first channel, and branch off there onto the lanes their own routes take. Edges with several sources and several targets
//! share a backbone across one channel they all pass (`backbone`). Applied to the finished routes, so it changes
//! neither the placement nor the routing of other edges. A member whose new way would enter a node or share a line with an
//! edge outside the bus keeps its own route, and that is reported. Membership is never inferred.

use arqlet_model::{Bus, Graph, Point, Rect, Route};
use std::collections::BTreeMap;

const EPS: f64 = 1e-6;

/// Draws the buses on `routes` (absolute coordinates, as `rects`). Returns one diagnostic per bus or member that is not
/// drawn as asked.
pub(crate) fn draw_buses(
    graph: &Graph,
    rects: &BTreeMap<String, Rect>,
    routes: &mut [Route],
    buses: &[Bus],
    margin: f64,
    trunks: &BTreeMap<String, f64>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let ends: BTreeMap<&str, (&str, Option<&str>)> = graph
        .edges
        .iter()
        .map(|e| (e.id.as_str(), (e.source.as_str(), e.target.as_deref())))
        .collect();
    let explicit: BTreeMap<(&str, bool), Option<&str>> = graph
        .edge_ports
        .iter()
        .flat_map(|p| {
            [
                ((p.edge_id.as_str(), true), p.source_port.as_deref()),
                ((p.edge_id.as_str(), false), p.target_port.as_deref()),
            ]
        })
        .collect();
    for bus in buses {
        let members: Vec<usize> = bus
            .edges
            .iter()
            .filter_map(|id| {
                routes
                    .iter()
                    .position(|r| &r.id == id && r.bundle_of.is_none() && r.points.len() >= 2)
            })
            .filter(|&i| {
                ends.get(routes[i].id.as_str())
                    .is_some_and(|(s, t)| Some(*s) != *t)
            })
            .collect();
        if members.len() < 2 {
            diagnostics.push(format!(
                "Bus {}: fewer than two of its edges are drawn; nothing to share",
                bus.id
            ));
            continue;
        }
        let end_of = |i: usize| ends[routes[i].id.as_str()];
        let (source, target) = end_of(members[0]);
        let fan_out = members.iter().all(|&i| end_of(i).0 == source);
        let fan_in = members.iter().all(|&i| end_of(i).1 == target);
        let (anchor, from_source) = match (fan_out, fan_in, target) {
            (true, _, _) => (source, true),
            (false, true, Some(t)) => (t, false),
            _ => {
                // Several sources and several targets: a backbone across one channel they all pass.
                let ends_of: Vec<(&str, Option<&str>)> =
                    members.iter().map(|&i| end_of(i)).collect();
                diagnostics.extend(backbone(
                    bus,
                    &members,
                    &ends_of,
                    routes,
                    rects,
                    &parents,
                    trunks.get(&bus.id).copied(),
                ));
                continue;
            }
        };
        // Explicit ports at the shared end are kept only when every member has the same one (an ELK hyperedge from a port).
        let port_at = |i: usize| {
            explicit
                .get(&(routes[i].id.as_str(), from_source))
                .cloned()
                .flatten()
        };
        let first_port = port_at(members[0]);
        if members.iter().any(|&i| port_at(i).is_some())
            && members.iter().any(|&i| port_at(i) != first_port)
        {
            diagnostics.push(format!(
                "Bus {}: its edges have different explicit ports at the shared end {anchor}; they are drawn apart",
                bus.id
            ));
            continue;
        }
        let Some(node) = rects.get(anchor) else {
            continue;
        };
        // Every member from the shared end on.
        let paths: Vec<Vec<Point>> = members
            .iter()
            .map(|&i| {
                let mut p = routes[i].points.clone();
                if !from_source {
                    p.reverse();
                }
                p
            })
            .collect();
        // The members that leave the node along the same axis and to the same side as most do: the trunk's.
        let way = |p: &[Point]| -> Option<(bool, i8)> {
            let (a, b) = (&p[0], &p[1]);
            if (a.y - b.y).abs() < EPS && (a.x - b.x).abs() > EPS {
                Some((true, if b.x > a.x { 1 } else { -1 }))
            } else if (a.x - b.x).abs() < EPS && (a.y - b.y).abs() > EPS {
                Some((false, if b.y > a.y { 1 } else { -1 }))
            } else {
                None
            }
        };
        let mut count: BTreeMap<(bool, i8), usize> = BTreeMap::new();
        for p in &paths {
            if let Some(w) = way(p) {
                *count.entry(w).or_default() += 1;
            }
        }
        let Some((&(horizontal, sign), _)) = count.iter().max_by_key(|(w, n)| (**n, **w)) else {
            // Straight edges have no channel to share (a star through a junction was measured and not kept: longer lines,
            // more bends and lines through their own targets, `benchmarks/results/bus-20261108.md`).
            diagnostics.push(format!(
                "Bus {}: its edges do not leave {anchor} along an axis (only orthogonal routes are drawn as buses); they are drawn apart",
                bus.id
            ));
            continue;
        };
        let along = |p: &Point| if horizontal { p.x } else { p.y };
        let across = |p: &Point| if horizontal { p.y } else { p.x };
        let point = |main: f64, cross: f64| {
            if horizontal {
                Point { x: main, y: cross }
            } else {
                Point { x: cross, y: main }
            }
        };
        let side = if horizontal {
            if sign > 0 {
                node.x + node.width
            } else {
                node.x
            }
        } else if sign > 0 {
            node.y + node.height
        } else {
            node.y
        };
        let in_trunk: Vec<usize> = (0..paths.len())
            .filter(|&k| way(&paths[k]) == Some((horizontal, sign)))
            .collect();
        if in_trunk.len() < 2 {
            diagnostics.push(format!(
                "Bus {}: fewer than two of its edges leave {anchor} to the same side; they are drawn apart",
                bus.id
            ));
            continue;
        }
        // The trunk's way across: the first corner nearest the node among the members that turn, else a margin out.
        let s = f64::from(sign);
        // The shared port: the middle one of the ports of the members that run straight from the node (they stay straight),
        // else of all members' ports, so the trunk reaches both ways from it. (Choosing it with the trunk by the score below
        // crossed more on the corpus: `benchmarks/results/bus-20261108.md`.)
        let straight: Vec<f64> = in_trunk
            .iter()
            .filter(|&&k| paths[k].len() == 2)
            .map(|&k| across(&paths[k][0]))
            .collect();
        let mut ports: Vec<f64> = if straight.is_empty() {
            in_trunk.iter().map(|&k| across(&paths[k][0])).collect()
        } else {
            straight
        };
        ports.sort_by(f64::total_cmp);
        let port = ports[(ports.len() - 1) / 2];
        let start = along(&paths[in_trunk[0]][0]);
        let outside: Vec<&Route> = routes
            .iter()
            .filter(|r| r.bundle_of.is_none() && !bus.edges.contains(&r.id))
            .collect();
        // The members' routes with the trunk at `trunk` across the flow: every member that can branch off there, with
        // whether its new way would enter a node other than its ends (and the areas around them) or share a line with an
        // edge outside the bus, in which case it keeps its own route.
        let build = |port: f64, trunk: f64| -> Vec<(usize, Vec<Point>, Option<&'static str>)> {
            let mut out = Vec::new();
            for &k in &in_trunk {
                let p = &paths[k];
                // The lane the member branches onto, and the rest of its own route from there.
                let (lane, rest): (f64, &[Point]) = if p.len() >= 3 {
                    (across(&p[2]), &p[3..])
                } else {
                    (across(&p[0]), &p[1..])
                };
                let Some(next) = rest.first().map(along) else {
                    continue;
                };
                if s * next < s * trunk - EPS {
                    continue;
                }
                let mut points = vec![point(start, port), point(trunk, port), point(trunk, lane)];
                points.extend(rest.iter().cloned());
                let points = tidy(points);
                let i = members[k];
                let (s_id, t_id) = end_of(i);
                let candidate = Route::new(routes[i].id.clone(), points.clone());
                let enters = rects.iter().any(|(id, r)| {
                    id != s_id
                        && Some(id.as_str()) != t_id
                        && !contains(r, &rects[s_id])
                        && !t_id.is_some_and(|t| contains(r, &rects[t]))
                        && arqlet_routing::hits(&candidate, r)
                });
                let shares = outside
                    .iter()
                    .any(|o| arqlet_routing::shares_segment(&candidate, o));
                let failure = if enters {
                    Some("enter a node")
                } else if shares {
                    Some("share a line with another edge")
                } else {
                    None
                };
                out.push((k, points, failure));
            }
            out
        };
        // The trunk's way across: at one of the members' first corners (or a margin out when none turns), the one whose
        // drawing keeps the most members on the bus, then crosses the edges outside it least, then bends least, then lies
        // nearest the node.
        let mut candidates: Vec<f64> = in_trunk
            .iter()
            .filter(|&&k| paths[k].len() >= 3)
            .map(|&k| along(&paths[k][1]))
            .collect();
        if candidates.is_empty() {
            candidates.push(side + s * margin);
        }
        candidates.sort_by(|a, b| (s * a).total_cmp(&(s * b)));
        candidates.dedup_by(|a, b| (*a - *b).abs() < EPS);
        let score = |drawn: &[(usize, Vec<Point>, Option<&'static str>)]| {
            let on_bus: Vec<&Vec<Point>> = drawn
                .iter()
                .filter(|d| d.2.is_none())
                .map(|d| &d.1)
                .collect();
            let crossings: usize = on_bus
                .iter()
                .map(|p| {
                    outside
                        .iter()
                        .map(|o| crossings(p, &o.points))
                        .sum::<usize>()
                })
                .sum();
            let bends: usize = on_bus.iter().map(|p| p.len().saturating_sub(2)).sum();
            (std::cmp::Reverse(on_bus.len()), crossings, bends)
        };
        let Some(drawn) = candidates
            .iter()
            .map(|&t| build(port, t))
            .min_by_key(|d| score(d))
        else {
            continue;
        };
        let mut kept = Vec::new();
        for (k, points, failure) in drawn {
            let i = members[k];
            if let Some(why) = failure {
                diagnostics.push(format!(
                    "Bus {}: edge {} keeps its own route (the shared way would {why})",
                    bus.id, routes[i].id
                ));
                continue;
            }
            kept.push((i, points));
        }
        if kept.len() < 2 {
            diagnostics.push(format!(
                "Bus {}: fewer than two of its edges can share the trunk; they are drawn apart",
                bus.id
            ));
            continue;
        }
        for (i, mut points) in kept {
            if !from_source {
                points.reverse();
            }
            routes[i].points = points;
        }
    }
    diagnostics
}

/// A bus whose edges share no end (`draw_buses`): every member leaves its source and enters its target along the flow, and
/// all of them pass one channel between the last source and the first target. The trunk is one line across the flow in that
/// channel; each member runs from its source's port straight to it, along it to its target's port lane, and straight on into
/// the target. The members at one node share one port there (the middle one of theirs), so each node has one stub. When no
/// channel is common to all, the members are split into the fewest groups that each share one, each with its own backbone. The trunk's place is chosen as for a fan (members kept, crossings with other edges, bends, then the middle of
/// the channel); members whose way would enter a node or share a line with another edge keep their routes.
fn backbone(
    bus: &Bus,
    members: &[usize],
    ends: &[(&str, Option<&str>)],
    routes: &mut [Route],
    rects: &BTreeMap<String, Rect>,
    parents: &BTreeMap<&str, Option<&str>>,
    trunk: Option<f64>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    // A member whose end lies inside a container (a software component in an ECU) keeps its way inside: the bus starts where
    // the route leaves the outermost container of its source that does not hold its target, and ends where it enters the
    // corresponding container of its target. `outer` is that stretch, `inner` the parts before and after it.
    let mut outer: BTreeMap<usize, Vec<Point>> = BTreeMap::new();
    let mut inner: BTreeMap<usize, (Vec<Point>, Vec<Point>)> = BTreeMap::new();
    let mut leaf_end: BTreeMap<(usize, bool), bool> = BTreeMap::new();
    for (k, &i) in members.iter().enumerate() {
        let (source, target) = ends[k];
        let Some(target) = target else { continue };
        let chain = |id: &str| -> Vec<String> {
            let mut out = vec![id.to_string()];
            let mut at = id;
            while let Some(Some(up)) = parents.get(at) {
                out.push(up.to_string());
                at = up;
            }
            out
        };
        let (cs, ct) = (chain(source), chain(target));
        // The outermost ancestor-or-self of each end that the other end's chain does not contain.
        let element = |c: &Vec<String>, other: &Vec<String>| -> String {
            c.iter()
                .rev()
                .find(|a| !other.contains(a))
                .cloned()
                .unwrap_or_else(|| c[0].clone())
        };
        let (es, et) = (element(&cs, &ct), element(&ct, &cs));
        let points = &routes[i].points;
        let crossing = |r: &Rect, a: &Point, b: &Point| -> Point {
            // Where the axis-parallel segment from `a` (inside) to `b` (outside) leaves `r`.
            let clamp = |v: f64, lo: f64, hi: f64| v.max(lo).min(hi);
            if (a.x - b.x).abs() < EPS {
                Point {
                    x: a.x,
                    y: if b.y > a.y { r.y + r.height } else { r.y }
                        .clamp(a.y.min(b.y), a.y.max(b.y)),
                }
            } else {
                Point {
                    x: clamp(
                        if b.x > a.x { r.x + r.width } else { r.x },
                        a.x.min(b.x),
                        a.x.max(b.x),
                    ),
                    y: a.y,
                }
            }
        };
        let inside = |r: &Rect, p: &Point| {
            p.x >= r.x - EPS
                && p.x <= r.x + r.width + EPS
                && p.y >= r.y - EPS
                && p.y <= r.y + r.height + EPS
        };
        let (Some(rs), Some(rt)) = (rects.get(&es), rects.get(&et)) else {
            continue;
        };
        let leaf_s = es == source;
        let leaf_t = et == target;
        let js = if leaf_s {
            Some(0)
        } else {
            (0..points.len() - 1).find(|&j| inside(rs, &points[j]) && !inside(rs, &points[j + 1]))
        };
        let jt = if leaf_t {
            Some(points.len() - 2)
        } else {
            (0..points.len() - 1)
                .rev()
                .find(|&j| !inside(rt, &points[j]) && inside(rt, &points[j + 1]))
        };
        let (Some(js), Some(jt)) = (js, jt) else {
            continue;
        };
        if jt < js {
            continue;
        }
        let bs = if leaf_s {
            points[0].clone()
        } else {
            crossing(rs, &points[js], &points[js + 1])
        };
        let bt = if leaf_t {
            points[points.len() - 1].clone()
        } else {
            crossing(rt, &points[jt + 1], &points[jt])
        };
        let mut middle = vec![bs];
        middle.extend(points[js + 1..=jt].iter().cloned());
        middle.push(bt);
        let middle = tidy(middle);
        if middle.len() < 2 {
            continue;
        }
        let mut middle = middle;
        let mut before = if leaf_s {
            Vec::new()
        } else {
            points[..=js].to_vec()
        };
        let mut after = if leaf_t {
            Vec::new()
        } else {
            points[jt + 1..].to_vec()
        };
        // Inside a container the way between its border and the component's port is straight when nothing lies between:
        // the crossing moves onto the port's line (no jog just inside the border).
        let straight = |r: &Rect,
                        port: &Point,
                        next: &Point,
                        crossing: &Point,
                        owner: &str|
         -> Option<Point> {
            let on_horizontal_side =
                (crossing.y - r.y).abs() < EPS || (crossing.y - r.y - r.height).abs() < EPS;
            // Only where the route leaves the port toward that border (a vertical first piece for a top or bottom border).
            let leaves_vertically = (port.x - next.x).abs() < EPS;
            if on_horizontal_side != leaves_vertically {
                return None;
            }
            let c = if on_horizontal_side {
                Point {
                    x: port.x,
                    y: crossing.y,
                }
            } else {
                Point {
                    x: crossing.x,
                    y: port.y,
                }
            };
            if !(c.x >= r.x - EPS
                && c.x <= r.x + r.width + EPS
                && c.y >= r.y - EPS
                && c.y <= r.y + r.height + EPS)
            {
                return None;
            }
            let way = Route::new("way", vec![c.clone(), port.clone()]);
            let blocked = rects.iter().any(|(id, o)| {
                id != owner && !contains(o, &rects[owner]) && arqlet_routing::hits(&way, o)
            });
            (!blocked).then_some(c)
        };
        let set_across = |p: &mut Point, like: &Point, vertical_segment: bool| {
            // Move a point of the stretch across to `like`'s line, keeping its place along the segment's axis.
            if vertical_segment {
                p.x = like.x;
            } else {
                p.y = like.y;
            }
        };
        if !leaf_t {
            let port = points.last().expect("two points").clone();
            let n = middle.len();
            let next = points[points.len() - 2].clone();
            if let Some(c) = straight(rt, &port, &next, &middle[n - 1], target) {
                let vertical = (middle[n - 2].x - middle[n - 1].x).abs() < EPS;
                let mut trial = middle.clone();
                if n > 2 {
                    set_across(&mut trial[n - 2], &c, vertical);
                }
                trial[n - 1] = c;
                // Only when the stretch stays axis-parallel at that end.
                let (a, b) = (&trial[n - 2], &trial[n - 1]);
                if (a.x - b.x).abs() < EPS || (a.y - b.y).abs() < EPS {
                    middle = trial;
                    after = vec![port];
                }
            }
        }
        if !leaf_s {
            let port = points[0].clone();
            if let Some(c) = straight(rs, &port, &points[1], &middle[0], source) {
                let vertical = (middle[0].x - middle[1].x).abs() < EPS;
                let mut trial = middle.clone();
                if trial.len() > 2 {
                    set_across(&mut trial[1], &c, vertical);
                }
                trial[0] = c;
                let (a, b) = (&trial[0], &trial[1]);
                if (a.x - b.x).abs() < EPS || (a.y - b.y).abs() < EPS {
                    middle = trial;
                    before = vec![port];
                }
            }
        }
        outer.insert(i, middle);
        inner.insert(i, (before, after));
        leaf_end.insert((i, true), leaf_s);
        leaf_end.insert((i, false), leaf_t);
    }
    if members.iter().any(|i| !outer.contains_key(i)) {
        diagnostics.push(format!(
            "Bus {}: an edge does not leave its source's container or enter its target's along the flow; they are drawn apart",
            bus.id
        ));
        return diagnostics;
    }
    // Members that end at one node inside a container share its way in, and members that start at one node inside a
    // container share its way out, so a component has one line to the bus: the way of the member whose crossing runs in the
    // direction most of them take (the middle one of those), adopted with that direction.
    let seg_way = |a: &Point, b: &Point| -> Option<(bool, i8)> {
        if (a.y - b.y).abs() < EPS && (a.x - b.x).abs() > EPS {
            Some((true, if b.x > a.x { 1 } else { -1 }))
        } else if (a.x - b.x).abs() < EPS && (a.y - b.y).abs() > EPS {
            Some((false, if b.y > a.y { 1 } else { -1 }))
        } else {
            None
        }
    };
    let mut adopted: BTreeMap<(usize, bool), Option<(bool, i8)>> = BTreeMap::new();
    // The flow most of the members' stretches start and end in.
    let bus_flow = {
        let mut count: BTreeMap<(bool, i8), usize> = BTreeMap::new();
        for p in outer.values() {
            for w in [
                seg_way(&p[0], &p[1]),
                seg_way(&p[p.len() - 2], &p[p.len() - 1]),
            ]
            .into_iter()
            .flatten()
            {
                *count.entry(w).or_default() += 1;
            }
        }
        count
            .into_iter()
            .max_by_key(|(w, n)| (*n, *w))
            .map(|(w, _)| w)
    };
    for from_start in [true, false] {
        let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (k, &i) in members.iter().enumerate() {
            let node = if from_start {
                Some(ends[k].0)
            } else {
                ends[k].1
            };
            if let (Some(node), Some(false)) = (node, leaf_end.get(&(i, from_start)).copied()) {
                groups.entry(node).or_default().push(i);
            }
        }
        for list in groups.values().filter(|l| l.len() > 1) {
            let own = |i: usize| {
                let p = &outer[&i];
                if from_start {
                    seg_way(&p[0], &p[1])
                } else {
                    seg_way(&p[p.len() - 2], &p[p.len() - 1])
                }
            };
            let mut count: BTreeMap<(bool, i8), usize> = BTreeMap::new();
            for &i in list {
                if let Some(w) = own(i) {
                    *count.entry(w).or_default() += 1;
                }
            }
            // The bus's own flow wins when any member of the group has it.
            let Some(common) = bus_flow.filter(|f| count.contains_key(f)).or_else(|| {
                count
                    .iter()
                    .max_by_key(|(w, n)| (**n, **w))
                    .map(|(w, _)| *w)
            }) else {
                continue;
            };
            let place = |i: usize| {
                let p = &outer[&i];
                let q = if from_start {
                    &p[0]
                } else {
                    p.last().expect("two points")
                };
                (q.x, q.y)
            };
            let mut sorted: Vec<usize> = list
                .iter()
                .copied()
                .filter(|&i| own(i) == Some(common))
                .collect();
            sorted.sort_by(|&a, &b| {
                place(a)
                    .partial_cmp(&place(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            let rep = sorted[(sorted.len() - 1) / 2];
            let (crossing, way) = if from_start {
                (outer[&rep][0].clone(), inner[&rep].0.clone())
            } else {
                (
                    outer[&rep].last().expect("two points").clone(),
                    inner[&rep].1.clone(),
                )
            };
            for &i in list {
                if from_start {
                    outer.get_mut(&i).expect("outer")[0] = crossing.clone();
                    inner.get_mut(&i).expect("inner").0 = way.clone();
                } else {
                    *outer
                        .get_mut(&i)
                        .expect("outer")
                        .last_mut()
                        .expect("two points") = crossing.clone();
                    inner.get_mut(&i).expect("inner").1 = way.clone();
                }
                adopted.insert((i, from_start), Some(common));
            }
        }
    }
    // The flow every member leaves and enters by: axis and sign of its first and last segments.
    let way = |a: &Point, b: &Point| -> Option<(bool, i8)> {
        if (a.y - b.y).abs() < EPS && (a.x - b.x).abs() > EPS {
            Some((true, if b.x > a.x { 1 } else { -1 }))
        } else if (a.x - b.x).abs() < EPS && (a.y - b.y).abs() > EPS {
            Some((false, if b.y > a.y { 1 } else { -1 }))
        } else {
            None
        }
    };
    let flows: Vec<Option<(bool, i8)>> = members
        .iter()
        .map(|&i| {
            let p = &outer[&i];
            let first = adopted
                .get(&(i, true))
                .copied()
                .unwrap_or_else(|| way(&p[0], &p[1]));
            let last = adopted
                .get(&(i, false))
                .copied()
                .unwrap_or_else(|| way(&p[p.len() - 2], &p[p.len() - 1]));
            (first.is_some() && first == last)
                .then_some(first)
                .flatten()
        })
        .collect();
    // The flow most members take; a member that leaves or enters across it keeps its own route.
    let mut count: BTreeMap<(bool, i8), usize> = BTreeMap::new();
    for f in flows.iter().flatten() {
        *count.entry(*f).or_default() += 1;
    }
    let Some((&(horizontal, sign), &most)) = count.iter().max_by_key(|(f, n)| (**n, **f)) else {
        diagnostics.push(format!(
            "Bus {}: its edges do not leave and enter along the flow (only orthogonal routes are drawn as buses); they are drawn apart",
            bus.id
        ));
        return diagnostics;
    };
    if most < members.len() {
        if most < 2 {
            diagnostics.push(format!(
                "Bus {}: fewer than two of its edges leave and enter along one flow; they are drawn apart",
                bus.id
            ));
            return diagnostics;
        }
        let keep: Vec<usize> = (0..members.len())
            .filter(|&k| flows[k] == Some((horizontal, sign)))
            .collect();
        for k in (0..members.len()).filter(|k| !keep.contains(k)) {
            diagnostics.push(format!(
                "Bus {}: edge {} leaves or enters across the flow and keeps its own route",
                bus.id, routes[members[k]].id
            ));
        }
        let sub: Vec<usize> = keep.iter().map(|&k| members[k]).collect();
        let sub_ends: Vec<(&str, Option<&str>)> = keep.iter().map(|&k| ends[k]).collect();
        diagnostics.extend(backbone(
            bus, &sub, &sub_ends, routes, rects, parents, trunk,
        ));
        return diagnostics;
    }
    let along = |p: &Point| if horizontal { p.x } else { p.y };
    let across = |p: &Point| if horizontal { p.y } else { p.x };
    let point = |main: f64, cross: f64| {
        if horizontal {
            Point { x: main, y: cross }
        } else {
            Point { x: cross, y: main }
        }
    };
    let s = f64::from(sign);
    // The channel every member passes: after the farthest source port, before the nearest target port.
    let first_free = members
        .iter()
        .map(|&i| s * along(&outer[&i][0]))
        .fold(f64::MIN, f64::max);
    let last_free = members
        .iter()
        .map(|&i| s * along(outer[&i].last().expect("two points")))
        .fold(f64::MAX, f64::min);
    if last_free - first_free < 2. * EPS {
        // No channel every member passes (sources and targets interleave along the flow): the fewest groups that each pass
        // one channel together, by stabbing the members' free spans greedily from the nearest end; each group of two or more
        // gets its own backbone, a member alone keeps its route.
        let span = |i: usize| {
            (
                s * along(&outer[&i][0]),
                s * along(outer[&i].last().expect("two points")),
            )
        };
        let mut order: Vec<usize> = (0..members.len()).collect();
        order.sort_by(|&a, &b| {
            span(members[a])
                .1
                .total_cmp(&span(members[b]).1)
                .then(a.cmp(&b))
        });
        let mut groups: Vec<Vec<usize>> = Vec::new();
        let mut placed = vec![false; members.len()];
        for &k in &order {
            if placed[k] {
                continue;
            }
            let stab = span(members[k]).1 - EPS;
            let group: Vec<usize> = order
                .iter()
                .copied()
                .filter(|&j| {
                    !placed[j] && span(members[j]).0 < stab - EPS && span(members[j]).1 > stab - EPS
                })
                .collect();
            let group = if group.is_empty() { vec![k] } else { group };
            for &j in &group {
                placed[j] = true;
            }
            groups.push(group);
        }
        if groups.len() == members.len() {
            diagnostics.push(format!(
                "Bus {}: no two of its edges pass a channel together; they are drawn apart",
                bus.id
            ));
            return diagnostics;
        }
        for group in groups {
            if group.len() < 2 {
                diagnostics.push(format!(
                    "Bus {}: edge {} passes no channel with another member and keeps its own route",
                    bus.id, routes[members[group[0]]].id
                ));
                continue;
            }
            let sub: Vec<usize> = group.iter().map(|&k| members[k]).collect();
            let sub_ends: Vec<(&str, Option<&str>)> = group.iter().map(|&k| ends[k]).collect();
            diagnostics.extend(backbone(
                bus, &sub, &sub_ends, routes, rects, parents, trunk,
            ));
        }
        return diagnostics;
    }
    // Candidates: the members' corners inside that channel, and its middle.
    let mut candidates: Vec<f64> = members
        .iter()
        .flat_map(|&i| outer[&i].iter().map(along))
        .filter(|&m| s * m > first_free + EPS && s * m < last_free - EPS)
        .collect();
    candidates.push(s * (first_free + last_free) / 2.);
    // A bus the layout placed (`bus_layout`) has its line where its node lies, when that is inside the channel.
    if let Some(t) = trunk.filter(|t| s * t > first_free + EPS && s * t < last_free - EPS) {
        candidates = vec![t];
    }
    candidates.sort_by(f64::total_cmp);
    candidates.dedup_by(|a, b| (*a - *b).abs() < EPS);
    let ids: Vec<&str> = members.iter().map(|&i| routes[i].id.as_str()).collect();
    let outside: Vec<Route> = routes
        .iter()
        .filter(|r| r.bundle_of.is_none() && !bus.edges.contains(&r.id))
        .cloned()
        .collect();
    let middle = s * (first_free + last_free) / 2.;
    // One stub per node: the members at one node meet the trunk from the middle one of their ports there.
    let mut at_node: BTreeMap<(&str, bool), Vec<f64>> = BTreeMap::new();
    for (k, &i) in members.iter().enumerate() {
        let p = &outer[&i];
        at_node
            .entry((ends[k].0, true))
            .or_default()
            .push(across(&p[0]));
        if let Some(t) = ends[k].1 {
            at_node
                .entry((t, false))
                .or_default()
                .push(across(p.last().expect("two points")));
        }
    }
    let shared_port: BTreeMap<(&str, bool), f64> = at_node
        .into_iter()
        .map(|(key, mut list)| {
            list.sort_by(f64::total_cmp);
            (key, list[(list.len() - 1) / 2])
        })
        .collect();
    let build = |trunk: f64| -> Vec<(usize, Vec<Point>, Option<&'static str>)> {
        members
            .iter()
            .enumerate()
            .map(|(k, &i)| {
                let p = &outer[&i];
                let (start, end) = (&p[0], p.last().expect("two points"));
                // One stub per node where the end is the node itself; an end inside a container keeps its border crossing.
                let from = if leaf_end[&(i, true)] {
                    shared_port[&(ends[k].0, true)]
                } else {
                    across(start)
                };
                let to = if leaf_end[&(i, false)] {
                    ends[k].1.map_or(across(end), |t| shared_port[&(t, false)])
                } else {
                    across(end)
                };
                let points = tidy(vec![
                    point(along(start), from),
                    point(trunk, from),
                    point(trunk, to),
                    point(along(end), to),
                ]);
                let (s_id, t_id) = ends[k];
                let candidate = Route::new(ids[k], points.clone());
                let enters = rects.iter().any(|(id, r)| {
                    id != s_id
                        && Some(id.as_str()) != t_id
                        && !contains(r, &rects[s_id])
                        && !t_id.is_some_and(|t| contains(r, &rects[t]))
                        && arqlet_routing::hits(&candidate, r)
                });
                let shares = outside
                    .iter()
                    .any(|o| arqlet_routing::shares_segment(&candidate, o));
                let failure = if enters {
                    Some("enter a node")
                } else if shares {
                    Some("share a line with another edge")
                } else {
                    None
                };
                (i, points, failure)
            })
            .collect()
    };
    let score = |drawn: &[(usize, Vec<Point>, Option<&'static str>)], trunk: f64| {
        let on: Vec<&Vec<Point>> = drawn
            .iter()
            .filter(|d| d.2.is_none())
            .map(|d| &d.1)
            .collect();
        let crossings: usize = on
            .iter()
            .map(|p| {
                outside
                    .iter()
                    .map(|o| crossings(p, &o.points))
                    .sum::<usize>()
            })
            .sum();
        let bends: usize = on.iter().map(|p| p.len().saturating_sub(2)).sum();
        (
            std::cmp::Reverse(on.len()),
            crossings,
            bends,
            ((trunk - middle).abs() * 1000.).round() as i64,
        )
    };
    let Some((drawn, _)) = candidates
        .iter()
        .map(|&t| (build(t), t))
        .min_by_key(|(d, t)| score(d, *t))
    else {
        return diagnostics;
    };
    let kept: Vec<(usize, Vec<Point>)> = drawn
        .into_iter()
        .filter_map(|(i, points, failure)| match failure {
            Some(why) => {
                diagnostics.push(format!(
                    "Bus {}: edge {} keeps its own route (the shared way would {why})",
                    bus.id, routes[i].id
                ));
                None
            }
            None => Some((i, points)),
        })
        .collect();
    if kept.len() < 2 {
        diagnostics.push(format!(
            "Bus {}: fewer than two of its edges can share the trunk; they are drawn apart",
            bus.id
        ));
        return diagnostics;
    }
    for (i, points) in kept {
        let (before, after) = &inner[&i];
        let mut all = before.clone();
        all.extend(points);
        all.extend(after.iter().cloned());
        routes[i].points = tidy(all);
    }
    diagnostics
}

/// The proper crossings of two orthogonal polylines: a horizontal and a vertical segment that pass through each other's
/// insides.
fn crossings(a: &[Point], b: &[Point]) -> usize {
    let mut n = 0;
    for p in a.windows(2) {
        for q in b.windows(2) {
            let cross = |h: &[Point], v: &[Point]| {
                (h[0].y - h[1].y).abs() < EPS
                    && (v[0].x - v[1].x).abs() < EPS
                    && v[0].x > h[0].x.min(h[1].x) + EPS
                    && v[0].x < h[0].x.max(h[1].x) - EPS
                    && h[0].y > v[0].y.min(v[1].y) + EPS
                    && h[0].y < v[0].y.max(v[1].y) - EPS
            };
            if cross(p, q) || cross(q, p) {
                n += 1;
            }
        }
    }
    n
}

/// The points without repeats and without points in the middle of a straight run.
fn tidy(points: Vec<Point>) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if out
            .last()
            .is_some_and(|q| (q.x - p.x).abs() < EPS && (q.y - p.y).abs() < EPS)
        {
            continue;
        }
        if out.len() >= 2 {
            let (a, b) = (&out[out.len() - 2], &out[out.len() - 1]);
            let collinear = ((a.x - b.x).abs() < EPS && (b.x - p.x).abs() < EPS)
                || ((a.y - b.y).abs() < EPS && (b.y - p.y).abs() < EPS);
            if collinear {
                out.pop();
            }
        }
        out.push(p);
    }
    out
}

fn contains(outer: &Rect, inner: &Rect) -> bool {
    outer.x <= inner.x + EPS
        && outer.y <= inner.y + EPS
        && inner.x + inner.width <= outer.x + outer.width + EPS
        && inner.y + inner.height <= outer.y + outer.height + EPS
}

#[cfg(test)]
mod tests {
    use super::*;
    use arqlet_model::{Edge, Node};

    fn rect(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 40.,
            height: 20.,
        }
    }

    fn graph() -> Graph {
        let mut g: Graph = serde_json::from_value(serde_json::json!({
            "nodes": [], "edges": []
        }))
        .unwrap();
        for id in ["s", "a", "b", "c"] {
            let n: Node =
                serde_json::from_value(serde_json::json!({"id": id, "kind": "node", "label": id}))
                    .unwrap();
            g.nodes.push(n);
        }
        for t in ["a", "b", "c"] {
            let e: Edge = serde_json::from_value(
                serde_json::json!({"id": format!("s{t}"), "kind": "e", "source": "s", "target": t}),
            )
            .unwrap();
            g.edges.push(e);
        }
        g
    }

    /// Three edges from one source, each on its own port and corner, become one trunk: one port, one way across, and a
    /// branch onto each edge's own lane.
    #[test]
    fn a_fan_out_shares_one_trunk() {
        let g = graph();
        let rects: BTreeMap<String, Rect> = [
            ("s", rect(0., 40.)),
            ("a", rect(200., 0.)),
            ("b", rect(200., 40.)),
            ("c", rect(200., 80.)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let p = |x: f64, y: f64| Point { x, y };
        let mut routes = vec![
            Route::new(
                "sa",
                vec![p(40., 45.), p(100., 45.), p(100., 10.), p(200., 10.)],
            ),
            Route::new("sb", vec![p(40., 50.), p(200., 50.)]),
            Route::new(
                "sc",
                vec![p(40., 55.), p(120., 55.), p(120., 90.), p(200., 90.)],
            ),
        ];
        let bus = Bus {
            id: "bus".into(),
            edges: vec!["sa".into(), "sb".into(), "sc".into()],
            label: None,
        };
        let diagnostics = draw_buses(&g, &rects, &mut routes, &[bus], 12., &BTreeMap::new());
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        assert_eq!(
            routes[0].points,
            vec![p(40., 50.), p(100., 50.), p(100., 10.), p(200., 10.)]
        );
        assert_eq!(routes[1].points, vec![p(40., 50.), p(200., 50.)]);
        assert_eq!(
            routes[2].points,
            vec![p(40., 50.), p(100., 50.), p(100., 90.), p(200., 90.)]
        );
        // The logical direction stays: every route starts at the source.
        assert!(routes.iter().all(|r| r.points[0] == p(40., 50.)));
    }

    #[test]
    fn edges_without_a_shared_end_are_drawn_apart() {
        let g = graph();
        let rects: BTreeMap<String, Rect> = [
            ("s", rect(0., 0.)),
            ("a", rect(100., 0.)),
            ("b", rect(100., 40.)),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect();
        let p = |x: f64, y: f64| Point { x, y };
        let mut routes = vec![
            Route::new("sa", vec![p(40., 10.), p(100., 10.)]),
            Route::new("ab", vec![p(120., 20.), p(120., 40.)]),
        ];
        let mut g = g;
        let e: Edge = serde_json::from_value(
            serde_json::json!({"id": "ab", "kind": "e", "source": "a", "target": "b"}),
        )
        .unwrap();
        g.edges.push(e);
        let before = routes.clone();
        let bus = Bus {
            id: "x".into(),
            edges: vec!["sa".into(), "ab".into()],
            label: None,
        };
        let diagnostics = draw_buses(&g, &rects, &mut routes, &[bus], 12., &BTreeMap::new());
        assert!(routes
            .iter()
            .zip(&before)
            .all(|(a, b)| a.points == b.points));
        assert!(!diagnostics.is_empty(), "{diagnostics:?}");
    }
}
