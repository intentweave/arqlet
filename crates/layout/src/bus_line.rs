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
    // What a bar or a tap must not pass: boxes that are neither a bus, a member of this bus, nor a container around the
    // bus or around that member.
    let ancestors = |id: &str| {
        let mut out = BTreeSet::new();
        let mut at = parent.get(id).copied().flatten();
        while let Some(p) = at {
            out.insert(p.to_string());
            at = parent.get(p).copied().flatten();
        }
        out
    };
    let mut placed_bars: Vec<Rect> = Vec::new();
    for (i, plan) in plans.iter().enumerate() {
        let members: BTreeSet<&str> = plan.attached.iter().map(|(_, m, _)| m.as_str()).collect();
        let around_bus = ancestors(&plan.bus);
        let blocks = |id: &str, extra: &BTreeSet<String>| {
            !bus_set.contains(id)
                && !members.contains(id)
                && !around_bus.contains(id)
                && !extra.contains(id)
        };
        let bar_at = |across: f64| {
            if plan.horizontal {
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
            }
        };
        let none = BTreeSet::new();
        let bar_free = |r: &Rect| {
            absolute
                .iter()
                .all(|(id, b)| !blocks(id, &none) || !overlaps(r, b))
                && placed_bars.iter().all(|b| !overlaps(r, b))
        };
        // On its lane, or the nearest free one beside it (away from its nodes first).
        let base = plan.across + offset[i] - THICKNESS / 2.;
        let step = THICKNESS + CLEARANCE;
        let across = (0..=40)
            .flat_map(|k| {
                let d = k as f64 * step;
                [base + d * plan.away, base - d * plan.away]
            })
            .find(|&a| bar_free(&bar_at(a)))
            .unwrap_or(base);
        placed_bars.push(bar_at(across));
        let across = across;
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
            // The straight tap, or one beside it along the stretch the node and the bar share that passes no box.
            // A tap passes no box but its own member's (another member of the bus too is in its way).
            let around_member = ancestors(member);
            let tap_blocks = |id: &str| {
                id != member.as_str()
                    && !bus_set.contains(id)
                    && !around_bus.contains(id)
                    && !around_member.contains(id)
            };
            let clear = |points: &[Point]| {
                absolute
                    .iter()
                    .all(|(id, b)| !tap_blocks(id) || !path_hits(points, b))
            };
            let mut points = tap(&absolute[member], &bar, plan.horizontal);
            if !clear(&points) {
                let obstacles: Vec<Rect> = absolute
                    .iter()
                    .filter(|(id, _)| tap_blocks(id))
                    .map(|(_, r)| r.clone())
                    .collect();
                if let Some(better) = tap_beside(&absolute[member], &bar, plan.horizontal, &clear)
                    .or_else(|| tap_around(&absolute[member], &bar, plan.horizontal, &clear))
                    .or_else(|| tap_routed(&absolute[member], &bar, plan.horizontal, &obstacles))
                {
                    points = better;
                }
            }
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

/// Whether two boxes overlap by more than half a unit.
fn overlaps(a: &Rect, b: &Rect) -> bool {
    a.x + a.width > b.x + 0.5
        && a.x < b.x + b.width - 0.5
        && a.y + a.height > b.y + 0.5
        && a.y < b.y + b.height - 0.5
}

/// Whether a polyline of axis-parallel segments passes through a box (by more than half a unit).
fn path_hits(points: &[Point], b: &Rect) -> bool {
    points.windows(2).any(|w| {
        let (x0, x1) = (w[0].x.min(w[1].x), w[0].x.max(w[1].x));
        let (y0, y1) = (w[0].y.min(w[1].y), w[0].y.max(w[1].y));
        x1 > b.x + 0.5 && x0 < b.x + b.width - 0.5 && y1 > b.y + 0.5 && y0 < b.y + b.height - 0.5
    })
}

/// A straight tap from `member` to `bar` at another place along the stretch they share, nearest the member's centre first,
/// that `clear` accepts. `None` when the stretch has no such place.
fn tap_beside(
    member: &Rect,
    bar: &Rect,
    horizontal: bool,
    clear: &dyn Fn(&[Point]) -> bool,
) -> Option<Vec<Point>> {
    let (m_along, m_across) = axes(member, horizontal);
    let (b_along, b_across) = axes(bar, horizontal);
    let (lo, hi) = (
        m_along.0.max(b_along.0) + CLEARANCE / 2.,
        m_along.1.min(b_along.1) - CLEARANCE / 2.,
    );
    if lo > hi {
        return None;
    }
    let above = (m_across.0 + m_across.1) / 2. < (b_across.0 + b_across.1) / 2.;
    let (leave, meet) = if above {
        (m_across.1, b_across.0)
    } else {
        (m_across.0, b_across.1)
    };
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
    let c = ((m_along.0 + m_along.1) / 2.).clamp(lo, hi);
    let steps = ((hi - lo) / 4.).ceil() as usize;
    (0..=steps)
        .flat_map(|k| [c + k as f64 * 4., c - k as f64 * 4.])
        .filter(|&at| at >= lo && at <= hi)
        .map(|at| vec![point(at, leave), point(at, meet)])
        .find(|points| clear(points))
}

/// A tap from `member` to `bar` that bends around what is in the way, the shortest that `clear` accepts: an L (out of the
/// member's side along the bar, then square onto the bar) or a Z (square out of the member's side facing the bar, along,
/// then square onto the bar), at places along the bar every 8 units within its span and, for a Z, at a few distances from
/// the member. `None` when none is clear.
fn tap_around(
    member: &Rect,
    bar: &Rect,
    horizontal: bool,
    clear: &dyn Fn(&[Point]) -> bool,
) -> Option<Vec<Point>> {
    let (m_along, m_across) = axes(member, horizontal);
    let (b_along, b_across) = axes(bar, horizontal);
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
    let mid_m = (m_across.0 + m_across.1) / 2.;
    let above = mid_m < (b_across.0 + b_across.1) / 2.;
    let (leave, meet) = if above {
        (m_across.1, b_across.0)
    } else {
        (m_across.0, b_across.1)
    };
    let c = (m_along.0 + m_along.1) / 2.;
    let (lo, hi) = (b_along.0 + CLEARANCE / 2., b_along.1 - CLEARANCE / 2.);
    if lo > hi {
        return None;
    }
    let length = |pts: &[Point]| -> f64 {
        pts.windows(2)
            .map(|w| (w[0].x - w[1].x).abs() + (w[0].y - w[1].y).abs())
            .sum()
    };
    let mut candidates: Vec<Vec<Point>> = Vec::new();
    let steps = ((hi - lo) / 8.).ceil() as usize;
    for k in 0..=steps {
        let x = lo + k as f64 * 8.;
        if x > hi {
            break;
        }
        // L: out of the member's side towards x, along to x, square onto the bar.
        if x < m_along.0 - 0.5 || x > m_along.1 + 0.5 {
            let side = if x < m_along.0 { m_along.0 } else { m_along.1 };
            candidates.push(vec![point(side, mid_m), point(x, mid_m), point(x, meet)]);
        }
        // Z: square out of the member's side facing the bar, along to x at a few depths, square onto the bar.
        let gap = meet - leave;
        for share in [0.5, 0.25, 0.75, 0.1, 0.9] {
            let turn = leave + gap * share;
            let start = c.clamp(m_along.0 + 2., m_along.1 - 2.);
            candidates.push(vec![
                point(start, leave),
                point(start, turn),
                point(x, turn),
                point(x, meet),
            ]);
        }
    }
    candidates.sort_by(|a, b| length(a).total_cmp(&length(b)));
    candidates.into_iter().find(|pts| clear(pts))
}

/// A tap found by a search: the shortest orthogonal way (each bend costs as much as 24 units) on a grid of the edges of
/// the boxes near the member and the bar, from the middle of a side of the member to the bar's face towards it, passing no
/// box of `obstacles`, the member or the bar. `None` when there is none within the region around both.
fn tap_routed(
    member: &Rect,
    bar: &Rect,
    horizontal: bool,
    obstacles: &[Rect],
) -> Option<Vec<Point>> {
    const MARGIN: f64 = 6.;
    const BEND: f64 = 24.;
    // Work along the bar (a) and across it (c).
    let to_ac = |r: &Rect| {
        let (a, c) = axes(r, horizontal);
        (a.0, a.1, c.0, c.1)
    };
    let (ma0, ma1, mc0, mc1) = to_ac(member);
    let (ba0, ba1, bc0, bc1) = to_ac(bar);
    let above = (mc0 + mc1) / 2. < (bc0 + bc1) / 2.;
    let meet = if above { bc0 } else { bc1 };
    // The region: around the member and the nearest stretch of the bar (not the whole bar, which may be long).
    let pad = 240.;
    let near = ((ma0 + ma1) / 2.).clamp(ba0, ba1);
    let (ra0, ra1) = (ma0.min(near) - pad, ma1.max(near) + pad);
    let (rc0, rc1) = (mc0.min(bc0) - pad, mc1.max(bc1) + pad);
    let mut walls: Vec<(f64, f64, f64, f64)> = obstacles
        .iter()
        .map(to_ac)
        .filter(|&(a0, a1, c0, c1)| a1 > ra0 && a0 < ra1 && c1 > rc0 && c0 < rc1)
        .collect();
    walls.push((ma0, ma1, mc0, mc1));
    walls.push((ba0, ba1, bc0, bc1));
    let mut a_lines = vec![(ma0 + ma1) / 2., ma0 - MARGIN, ma1 + MARGIN, ra0, ra1];
    let mut c_lines = vec![(mc0 + mc1) / 2., mc0 - MARGIN, mc1 + MARGIN, meet, rc0, rc1];
    for &(a0, a1, c0, c1) in &walls {
        a_lines.extend([a0 - MARGIN, a1 + MARGIN]);
        c_lines.extend([c0 - MARGIN, c1 + MARGIN]);
    }
    let mut a_lines: Vec<f64> = a_lines
        .into_iter()
        .filter(|a| *a >= ra0 && *a <= ra1)
        .collect();
    let mut c_lines: Vec<f64> = c_lines
        .into_iter()
        .filter(|c| *c >= rc0 && *c <= rc1)
        .collect();
    a_lines.sort_by(f64::total_cmp);
    a_lines.dedup_by(|x, y| (*x - *y).abs() < 0.5);
    c_lines.sort_by(f64::total_cmp);
    c_lines.dedup_by(|x, y| (*x - *y).abs() < 0.5);
    if a_lines.len() * c_lines.len() > 40_000 {
        return None;
    }
    // A segment is open when its inside meets no wall's inside (a point on a face, as the start and the goal, is open).
    let open = |p: (f64, f64), q: (f64, f64)| {
        let (a0, a1) = (p.0.min(q.0), p.0.max(q.0));
        let (c0, c1) = (p.1.min(q.1), p.1.max(q.1));
        walls.iter().all(|&(wa0, wa1, wc0, wc1)| {
            !(a1 > wa0 + 0.5 && a0 < wa1 - 0.5 && c1 > wc0 + 0.5 && c0 < wc1 - 0.5)
                || (a1 - a0 < 1e-9 && (a0 <= wa0 + 0.5 || a0 >= wa1 - 0.5))
                || (c1 - c0 < 1e-9 && (c0 <= wc0 + 0.5 || c0 >= wc1 - 0.5))
        })
    };
    let na = a_lines.len();
    let nc = c_lines.len();
    let at = |i: usize, j: usize| (a_lines[i], c_lines[j]);
    // Starts: the middle of every side of the member, on the grid.
    let ia = |v: f64| a_lines.iter().position(|x| (x - v).abs() < 0.5);
    let ic = |v: f64| c_lines.iter().position(|x| (x - v).abs() < 0.5);
    let mid_a = (ma0 + ma1) / 2.;
    let mid_c = (mc0 + mc1) / 2.;
    // A start lies just outside the member (a margin off its side), reached by a stub from the side.
    let starts: Vec<((usize, usize), (f64, f64))> = [
        (ia(mid_a), ic(mc0 - MARGIN), (mid_a, mc0)),
        (ia(mid_a), ic(mc1 + MARGIN), (mid_a, mc1)),
        (ia(ma0 - MARGIN), ic(mid_c), (ma0, mid_c)),
        (ia(ma1 + MARGIN), ic(mid_c), (ma1, mid_c)),
    ]
    .into_iter()
    .filter_map(|(i, j, side)| Some(((i?, j?), side)))
    .collect();
    let jm = ic(meet)?;
    // Dijkstra over (cell, direction: 0 along, 1 across).
    let idx = |i: usize, j: usize, d: usize| (i * nc + j) * 2 + d;
    let mut dist = vec![f64::INFINITY; na * nc * 2];
    let mut prev: Vec<Option<usize>> = vec![None; na * nc * 2];
    let mut heap = std::collections::BinaryHeap::new();
    #[derive(PartialEq)]
    struct Item(f64, usize);
    impl Eq for Item {}
    impl PartialOrd for Item {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for Item {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            o.0.total_cmp(&self.0)
        }
    }
    for &((i, j), _) in &starts {
        for d in 0..2 {
            let k = idx(i, j, d);
            dist[k] = 0.;
            heap.push(Item(0., k));
        }
    }
    let mut goal = None;
    while let Some(Item(cost, k)) = heap.pop() {
        if cost > dist[k] {
            continue;
        }
        let d = k % 2;
        let cell = k / 2;
        let (i, j) = (cell / nc, cell % nc);
        if j == jm && a_lines[i] >= ba0 + CLEARANCE / 2. && a_lines[i] <= ba1 - CLEARANCE / 2. {
            goal = Some(k);
            break;
        }
        let mut step = |ni: usize, nj: usize, nd: usize| {
            if !open(at(i, j), at(ni, nj)) {
                return;
            }
            let (p, q) = (at(i, j), at(ni, nj));
            let c = cost + (p.0 - q.0).abs() + (p.1 - q.1).abs() + if nd == d { 0. } else { BEND };
            let nk = idx(ni, nj, nd);
            if c < dist[nk] {
                dist[nk] = c;
                prev[nk] = Some(k);
                heap.push(Item(c, nk));
            }
        };
        if i > 0 {
            step(i - 1, j, 0);
        }
        if i + 1 < na {
            step(i + 1, j, 0);
        }
        if j > 0 {
            step(i, j - 1, 1);
        }
        if j + 1 < nc {
            step(i, j + 1, 1);
        }
    }
    let mut k = goal?;
    let mut cells = vec![k / 2];
    while let Some(p) = prev[k] {
        k = p;
        if *cells.last().expect("a cell") != k / 2 {
            cells.push(k / 2);
        }
    }
    cells.reverse();
    let first = cells[0];
    let (fi, fj) = (first / nc, first % nc);
    let side = starts
        .iter()
        .find(|((i, j), _)| *i == fi && *j == fj)
        .map(|(_, s)| *s)?;
    let mut ac: Vec<(f64, f64)> = vec![side];
    ac.extend(cells.iter().map(|&c| at(c / nc, c % nc)));
    // Only the corners.
    let mut corners: Vec<(f64, f64)> = Vec::new();
    for p in ac {
        if corners.len() >= 2 {
            let (a, b) = (corners[corners.len() - 2], corners[corners.len() - 1]);
            let straight = ((a.0 - b.0).abs() < 1e-9 && (b.0 - p.0).abs() < 1e-9)
                || ((a.1 - b.1).abs() < 1e-9 && (b.1 - p.1).abs() < 1e-9);
            if straight {
                corners.pop();
            }
        }
        if corners.last() != Some(&p) {
            corners.push(p);
        }
    }
    Some(
        corners
            .into_iter()
            .map(|(a, c)| {
                if horizontal {
                    Point { x: a, y: c }
                } else {
                    Point { x: c, y: a }
                }
            })
            .collect(),
    )
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
/// nearest, or above `EXACT_MIDDLE_MAX` members the middle of a longest path found by a second search from `far`. A root
/// `kept` from an earlier layout of the network wins while it is still a member.
fn root_of<'a>(
    network: &BTreeMap<&'a str, usize>,
    far: &'a str,
    buses: &BTreeSet<&str>,
    adjacent: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    order: &BTreeMap<&str, usize>,
    kept: &[String],
) -> &'a str {
    if let Some((root, _)) = kept.iter().find_map(|k| network.get_key_value(k.as_str())) {
        return root;
    }
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

/// `orient`'s answer: the graph with the turned edges, their ids, the root of every network, and every member's level.
pub(crate) type Oriented = (
    Graph,
    BTreeSet<String>,
    Vec<String>,
    BTreeMap<String, usize>,
);

pub(crate) fn orient(graph: &Graph, kept: &[String]) -> Option<Oriented> {
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
    // An edge between two members of a bus network that joins no bus follows the tree too: a collapsed container that
    // holds a bus stands for it, and the edges to that bus end at the container.
    let members: BTreeSet<&str> = adjacent.keys().copied().collect();
    let joining: Vec<(usize, &str, &str)> = graph
        .edges
        .iter()
        .enumerate()
        .filter_map(|(k, e)| {
            let (s, t) = (e.source.as_str(), e.target.as_deref()?);
            (s != t
                && !buses.contains(s)
                && !buses.contains(t)
                && members.contains(s)
                && members.contains(t))
            .then_some((k, s, t))
        })
        .collect();
    for (k, s, t) in joining {
        adjacent.entry(s).or_default().insert(t);
        adjacent.entry(t).or_default().insert(s);
        bus_edges.push(k);
    }
    // Breadth-first levels from one root per network (`root_of`).
    let mut starts: Vec<&str> = adjacent.keys().copied().collect();
    starts.sort_by_key(|id| order[id]);
    let mut level: BTreeMap<&str, usize> = BTreeMap::new();
    let mut roots = Vec::new();
    for start in starts {
        if level.contains_key(start) {
            continue;
        }
        let (network, _, far) = bfs(start, &adjacent, &order);
        let root = root_of(&network, far, &buses, &adjacent, &order, kept);
        roots.push(root.to_string());
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
    for binding in &mut oriented.edge_ports {
        if flipped.contains(&binding.edge_id) {
            std::mem::swap(&mut binding.source_port, &mut binding.target_port);
        }
    }
    let levels = level.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    Some((oriented, flipped, roots, levels))
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
            // One tap per attached node (edges of collapsed members end at their container, several at one).
            taps: {
                let mut seen = BTreeSet::new();
                let mut list = taps.remove(node.id.as_str()).unwrap_or_default();
                list.retain(|t| seen.insert(t.node.clone()));
                list
            },
            kind: Some(node.kind.clone()),
            parent_id: node.parent_id.clone(),
        });
    }
    Some(view)
}

/// The graph the layout sees for a view with bus lines: every bus line as a node of shape `busLine` again, with one edge per
/// tap, after the other nodes and edges. `None` when the view has no bus line.
pub(crate) fn expand(view: &Graph) -> Option<Graph> {
    if !view
        .buses
        .iter()
        .any(|b| !b.taps.is_empty() && b.edges.is_empty())
    {
        return None;
    }
    let present: BTreeSet<&str> = view.nodes.iter().map(|n| n.id.as_str()).collect();
    let mut graph = view.clone();
    graph
        .buses
        .retain(|b| b.taps.is_empty() || !b.edges.is_empty());
    for bus in view
        .buses
        .iter()
        .filter(|b| !b.taps.is_empty() && b.edges.is_empty())
    {
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
    let line_buses: Vec<&Bus> = view
        .buses
        .iter()
        .filter(|b| !b.taps.is_empty() && b.edges.is_empty())
        .collect();
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
    // The taps are drawn from the bar's geometry, not from their routes: issues found on those routes do not hold for the
    // drawing (they named every tap of an organic area, a few thousand times, though none is drawn so).
    snapshot.routing_issues.retain(|issue| {
        issue
            .strip_prefix("Edge ")
            .and_then(|rest| rest.split(' ').next())
            .is_none_or(|id| !taps.contains(id))
    });
    snapshot
        .route_conflicts
        .retain(|c| !taps.contains(c.edge_id.as_str()));
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
    for bus in snapshot
        .graph
        .buses
        .iter()
        .filter(|b| !b.taps.is_empty() && b.edges.is_empty())
    {
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
    turn_trunks(snapshot, false);
    let Some(graph) = expand(&snapshot.graph) else {
        return false;
    };
    for bus in snapshot
        .graph
        .buses
        .iter()
        .filter(|b| !b.taps.is_empty() && b.edges.is_empty())
    {
        if let Some(r) = snapshot.bus_boxes.remove(&bus.id) {
            snapshot.boxes.insert(bus.id.clone(), r);
        }
    }
    let kept = std::mem::take(&mut snapshot.bus_taps);
    snapshot.bus_taps.roots = kept.roots;
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
    turn_trunks(snapshot, true);
}

/// Provisional (`busStyle: trunk`): the view of a graph with bus lines drawn as shared lines. Every bus line (`Bus::taps`)
/// with a member above it in its bus tree (`levels` from `orient`) gets that member as its head (the nearest to the root,
/// then the one on the most buses, then the earliest in the graph) and one edge from the head to every other member, with the
/// id and kind of that member's tap; the bus lists those edges (an explicit bus, ADR 0005), so the layout gives it no rank
/// and draws one trunk with branches. A bus at the root of its tree keeps its bar. The taps keep the edges' own
/// direction. `None` when no bus gets a head.
pub(crate) fn trunks(
    view: &Graph,
    levels: &BTreeMap<String, usize>,
    always: bool,
    skip: &dyn Fn(&Bus) -> bool,
) -> Option<Graph> {
    if !view
        .buses
        .iter()
        .any(|b| !b.taps.is_empty() && b.edges.is_empty())
    {
        return None;
    }
    let present: BTreeSet<&str> = view.nodes.iter().map(|n| n.id.as_str()).collect();
    let order: BTreeMap<&str, usize> = view
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.id.as_str(), i))
        .collect();
    let mut on_buses: BTreeMap<&str, usize> = BTreeMap::new();
    for bus in &view.buses {
        for tap in &bus.taps {
            *on_buses.entry(tap.node.as_str()).or_default() += 1;
        }
    }
    let mut graph = view.clone();
    for bus in graph
        .buses
        .iter_mut()
        .filter(|b| !b.taps.is_empty() && b.edges.is_empty() && !skip(b))
    {
        let members: Vec<&BusTap> = bus
            .taps
            .iter()
            .filter(|t| present.contains(t.node.as_str()))
            .collect();
        // The head: the member above the bus in its tree (the nearest to the root, then the one on the most buses, then
        // the earliest). A bus at the root of its tree has none and keeps its bar.
        let own = levels.get(&bus.id).copied().unwrap_or(0);
        let head = members
            .iter()
            .filter(|t| levels.get(&t.node).is_some_and(|&l| l < own))
            .min_by_key(|t| {
                (
                    levels[&t.node],
                    std::cmp::Reverse(on_buses.get(t.node.as_str()).copied().unwrap_or(0)),
                    order[t.node.as_str()],
                )
            })
            .map(|t| t.node.clone())
            // `always` (`busStyle: lines`): without a member above, the member on the most buses, then the earliest.
            .or_else(|| {
                always
                    .then(|| {
                        members
                            .iter()
                            .min_by_key(|t| {
                                (
                                    std::cmp::Reverse(
                                        on_buses.get(t.node.as_str()).copied().unwrap_or(0),
                                    ),
                                    order[t.node.as_str()],
                                )
                            })
                            .map(|t| t.node.clone())
                    })
                    .flatten()
            });
        let Some(head) = head else { continue };
        for tap in members.iter().filter(|t| t.node != head) {
            view_edge(&mut graph.edges, tap, &head);
            bus.edges.push(tap.edge.clone());
        }
    }
    Some(graph)
}

fn view_edge(edges: &mut Vec<Edge>, tap: &BusTap, head: &str) {
    edges.push(Edge {
        id: tap.edge.clone(),
        kind: tap.kind.clone(),
        source: head.to_string(),
        target: Some(tap.node.clone()),
        weight: None,
    });
}

/// Provisional (`busStyle: trunk`): turns the edges of trunk buses between the layout's direction (from the head to the
/// member, so the tree grows down) and their own (`logical`: a tap from the member to the bus runs from the member to the
/// head), with their routes, ports and port bindings. Applying it in one direction twice changes nothing.
pub(crate) fn turn_trunks(snapshot: &mut Snapshot, logical: bool) {
    let mut turned: BTreeSet<String> = BTreeSet::new();
    let index: BTreeMap<&str, usize> = snapshot
        .graph
        .edges
        .iter()
        .enumerate()
        .map(|(i, e)| (e.id.as_str(), i))
        .collect();
    let mut flip = Vec::new();
    for bus in snapshot
        .graph
        .buses
        .iter()
        .filter(|b| !b.taps.is_empty() && !b.edges.is_empty())
    {
        for tap in bus.taps.iter().filter(|t| t.to_bus) {
            let Some(&i) = index.get(tap.edge.as_str()) else {
                continue;
            };
            let e = &snapshot.graph.edges[i];
            // In the layout's direction the member is the target; in its own, the source.
            let at_member = if logical {
                e.target.as_deref() == Some(tap.node.as_str())
            } else {
                e.source == tap.node
            };
            if at_member {
                flip.push(i);
            }
        }
    }
    for i in flip {
        let e = &mut snapshot.graph.edges[i];
        let target = e.target.take().expect("a trunk edge has a target");
        e.target = Some(std::mem::replace(&mut e.source, target));
        turned.insert(e.id.clone());
    }
    if turned.is_empty() {
        return;
    }
    for route in &mut snapshot.routes {
        if turned.contains(&route.id) {
            route.points.reverse();
        }
    }
    for port in &mut snapshot.resolved_ports {
        if turned.contains(&port.edge_id) {
            port.endpoint = if port.endpoint == "source" {
                "target".into()
            } else {
                "source".into()
            };
        }
    }
    for binding in &mut snapshot.graph.edge_ports {
        if turned.contains(&binding.edge_id) {
            std::mem::swap(&mut binding.source_port, &mut binding.target_port);
        }
    }
}

/// Provisional: a view whose bus lines are drawn as trunks (`trunks`) turned back into one with bars: the trunks' edges
/// leave, the taps stay. `None` without a trunk.
pub(crate) fn untrunk(view: &Graph) -> Option<Graph> {
    let edges: BTreeSet<&str> = view
        .buses
        .iter()
        .filter(|b| !b.taps.is_empty() && !b.edges.is_empty())
        .flat_map(|b| b.edges.iter().map(String::as_str))
        .collect();
    if edges.is_empty() {
        return None;
    }
    let mut graph = view.clone();
    graph.edges.retain(|e| !edges.contains(e.id.as_str()));
    graph
        .edge_ports
        .retain(|b| !edges.contains(b.edge_id.as_str()));
    for bus in graph.buses.iter_mut().filter(|b| !b.taps.is_empty()) {
        bus.edges.clear();
    }
    Some(graph)
}

/// Provisional (`busStyle: trunk`): the edges of a trunk view that are neither a trunk's nor a bar's tap but join two
/// members of a bus tree (as to a collapsed zone that holds a bus), turned so they point down the tree (`levels`), with
/// their ids; `restore` gives their direction back after the layout.
pub(crate) fn orient_joining(
    view: &Graph,
    levels: &BTreeMap<String, usize>,
) -> (Graph, BTreeSet<String>) {
    let trunk: BTreeSet<&str> = view
        .buses
        .iter()
        .filter(|b| !b.taps.is_empty())
        .flat_map(|b| b.edges.iter().map(String::as_str))
        .collect();
    let mut graph = view.clone();
    let mut flipped = BTreeSet::new();
    for e in graph.edges.iter_mut() {
        let Some(t) = e.target.clone() else { continue };
        if trunk.contains(e.id.as_str()) || e.source == t {
            continue;
        }
        if let (Some(a), Some(b)) = (levels.get(&e.source), levels.get(&t)) {
            if a > b {
                e.target = Some(std::mem::replace(&mut e.source, t));
                flipped.insert(e.id.clone());
            }
        }
    }
    for binding in &mut graph.edge_ports {
        if flipped.contains(&binding.edge_id) {
            std::mem::swap(&mut binding.source_port, &mut binding.target_port);
        }
    }
    (graph, flipped)
}

/// Provisional (`busStyle: trunk`/`lines` in an area that is not layered): the head of every bus of `heads` (bus id ->
/// head) drawn as a bar becomes shared lines along the bar's middle: for every other member one route, from the member down
/// its tap to the bar's middle, along it to the head's foot, and up the head's tap to the head (orthogonal throughout, as
/// the bar and its taps are), in the tap's direction; the bus lists those edges (an explicit bus, junction dots where they
/// branch) and its bar goes. The bar geometry already keeps clear of the boxes.
pub(crate) fn bars_to_lines(snapshot: &mut Snapshot, heads: &BTreeMap<String, String>) {
    for bus in snapshot.graph.buses.iter_mut() {
        let Some(head) = heads.get(&bus.id) else {
            continue;
        };
        let Some(line) = snapshot.bus_lines.get(&bus.id) else {
            continue;
        };
        let b = &line.bar;
        let horizontal = b.width >= b.height;
        let middle = if horizontal {
            b.y + b.height / 2.
        } else {
            b.x + b.width / 2.
        };
        let onto = |p: &Point| {
            if horizontal {
                Point { x: p.x, y: middle }
            } else {
                Point { x: middle, y: p.y }
            }
        };
        // A member's way from its box to the bar's middle.
        let to_bar = |tap: &BusTap| -> Option<Vec<Point>> {
            let mut points = line.taps.get(&tap.edge)?.clone();
            if !tap.to_bus {
                points.reverse();
            }
            // Down to the bar's middle: square from the last point (a tap may end with a stretch along the bar's face).
            let last = points.last()?.clone();
            let before = points.len().checked_sub(2).map(|k| points[k].clone());
            let along_face = before.is_some_and(|b| {
                if horizontal {
                    (b.y - last.y).abs() < 1e-9
                } else {
                    (b.x - last.x).abs() < 1e-9
                }
            });
            if along_face {
                points.push(onto(&last));
            } else {
                points.pop();
                points.push(onto(&last));
            }
            Some(points)
        };
        let Some(head_tap) = bus.taps.iter().find(|t| &t.node == head) else {
            continue;
        };
        let Some(head_way) = to_bar(head_tap) else {
            continue;
        };
        let mut routes = Vec::new();
        for tap in bus.taps.iter().filter(|t| &t.node != head) {
            let Some(mut points) = to_bar(tap) else {
                continue;
            };
            points.extend(head_way.iter().rev().cloned());
            points.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
            // In the edge's direction: from the member to the head for a tap to the bus, else back.
            let (source, target) = if tap.to_bus {
                (tap.node.clone(), head.clone())
            } else {
                points.reverse();
                (head.clone(), tap.node.clone())
            };
            routes.push((tap, source, target, points));
        }
        for (tap, source, target, points) in routes {
            snapshot.graph.edges.push(Edge {
                id: tap.edge.clone(),
                kind: tap.kind.clone(),
                source,
                target: Some(target),
                weight: None,
            });
            snapshot.routes.push(arqlet_model::Route {
                id: tap.edge.clone(),
                points,
                weight: 1,
                bundle_of: None,
            });
            bus.edges.push(tap.edge.clone());
        }
        snapshot.bus_lines.remove(&bus.id);
    }
    // The new lines checked against the boxes they pass (not their own ends or those ends' containers).
    let absolute = crate::absolute_all(&snapshot.graph, &snapshot.boxes);
    let parent: BTreeMap<&str, Option<&str>> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let around = |id: &str| {
        let mut out = BTreeSet::new();
        let mut at = Some(id);
        while let Some(n) = at {
            out.insert(n.to_string());
            at = parent.get(n).copied().flatten();
        }
        out
    };
    let lines: BTreeSet<&str> = heads
        .keys()
        .flat_map(|bus| {
            snapshot
                .graph
                .buses
                .iter()
                .filter(move |b| &b.id == bus)
                .flat_map(|b| b.edges.iter().map(String::as_str))
        })
        .collect();
    let mut found = Vec::new();
    for e in snapshot
        .graph
        .edges
        .iter()
        .filter(|e| lines.contains(e.id.as_str()))
    {
        let Some(route) = snapshot.routes.iter().find(|r| r.id == e.id) else {
            continue;
        };
        let mut skip = around(&e.source);
        if let Some(t) = e.target.as_deref() {
            skip.extend(around(t));
        }
        for (id, r) in &absolute {
            if skip.contains(id) {
                continue;
            }
            let hit = route.points.windows(2).any(|w| {
                let (x0, x1) = (w[0].x.min(w[1].x), w[0].x.max(w[1].x));
                let (y0, y1) = (w[0].y.min(w[1].y), w[0].y.max(w[1].y));
                x1 > r.x + 0.5
                    && x0 < r.x + r.width - 0.5
                    && y1 > r.y + 0.5
                    && y0 < r.y + r.height - 0.5
            });
            if hit {
                found.push(arqlet_model::RouteConflict {
                    edge_id: e.id.clone(),
                    node_id: id.clone(),
                });
            }
        }
    }
    snapshot.route_conflicts.extend(found);
}

/// The head every bus line of `view` (`Bus::taps`, no edges) that `pick` accepts takes as `trunks` chooses it, by bus id.
pub(crate) fn heads(
    view: &Graph,
    levels: &BTreeMap<String, usize>,
    always: bool,
    pick: &dyn Fn(&Bus) -> bool,
) -> BTreeMap<String, String> {
    let Some(graph) = trunks(view, levels, always, &|b| !pick(b)) else {
        return BTreeMap::new();
    };
    let ends: BTreeMap<&str, (&str, &str)> = graph
        .edges
        .iter()
        .filter_map(|e| Some((e.id.as_str(), (e.source.as_str(), e.target.as_deref()?))))
        .collect();
    graph
        .buses
        .iter()
        .filter(|b| !b.taps.is_empty() && pick(b))
        .filter_map(|b| {
            // `trunks` draws an edge from the head to every other member.
            let first = b.edges.first()?;
            let (source, _) = ends.get(first.as_str())?;
            Some((b.id.clone(), source.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod geometry_tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    #[test]
    fn a_bar_moves_off_a_foreign_box_and_a_tap_goes_around_one() {
        // Flow top to bottom: the bus box at y 100, members a (left) and b (right) below it. A foreign box c lies on the
        // bar's line between them; a foreign box d stands between a and the bar.
        let graph: Graph = serde_json::from_value(serde_json::json!({
            "defaultDirection": "tb",
            "nodes": [
                {"id": "bus", "kind": "bus", "label": "bus", "shape": "busLine"},
                {"id": "a", "kind": "k", "label": "a"}, {"id": "b", "kind": "k", "label": "b"},
                {"id": "c", "kind": "k", "label": "c"}, {"id": "d", "kind": "k", "label": "d"}],
            "edges": [{"id": "ta", "kind": "k", "source": "a", "target": "bus"},
                {"id": "tb", "kind": "k", "source": "b", "target": "bus"}]
        }))
        .unwrap();
        let boxes = BTreeMap::from([
            ("bus".to_string(), rect(150., 96., 100., 8.)),
            ("a".to_string(), rect(0., 220., 100., 50.)),
            ("b".to_string(), rect(300., 220., 100., 50.)),
            ("c".to_string(), rect(170., 90., 60., 20.)),
            ("d".to_string(), rect(20., 150., 60., 40.)),
        ]);
        let lines = draw(&graph, &boxes, &BTreeMap::new());
        let line = &lines["bus"];
        assert!(
            !overlaps(&line.bar, &boxes["c"]),
            "bar {:?} over c",
            line.bar
        );
        for (edge, points) in &line.taps {
            for foreign in ["c", "d"] {
                assert!(
                    !path_hits(points, &boxes[foreign]),
                    "{edge} {points:?} through {foreign}"
                );
            }
        }
        // The taps still run from their member to the bar.
        let first = &line.taps["ta"][0];
        assert!(first.x >= 0. && first.x <= 100. && first.y >= 220. - 1e-6 && first.y <= 270.);
    }
}
