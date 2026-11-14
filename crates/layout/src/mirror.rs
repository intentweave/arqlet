//! Experimental (`singlePassReversed`, ADR 0020 steps S1-0 and S1-0b): a drawing whose areas all flow right to left (`rl`)
//! or bottom to top (`bt`) is laid out with the positive flow of that axis, where single pass runs, and mirrored on that
//! axis area by area, a container's header staying at its top. Logical
//! edge direction is kept: only coordinates are mirrored. Mirroring twice gives the drawing back, so a later layout starts
//! from the unmirrored earlier one.
use arqlet_model::{
    DirectionConstraint, Graph, LayoutOptions, Mode, Orientation, Point, PortSide, Rect, Snapshot,
};
use std::collections::{BTreeMap, BTreeSet};

/// The reversed orientation (`rl` or `bt`) every area of the graph has, if the drawing can be mirrored: no pins, label
/// places, explicit ports or buses (their places are given in the reversed frame), and every area a fixed flow.
pub(crate) fn uniform_reversal(graph: &Graph, options: &LayoutOptions) -> Option<Orientation> {
    if !options.pins.is_empty()
        || !options.label_positions.is_empty()
        || !graph.ports.is_empty()
        || !graph.edge_ports.is_empty()
        || !graph.buses.is_empty()
    {
        return None;
    }
    let containers: BTreeSet<&str> = graph
        .nodes
        .iter()
        .filter_map(|n| n.parent_id.as_deref())
        .collect();
    let mut seen = None;
    for area in std::iter::once(None).chain(containers.into_iter().map(Some)) {
        match graph.area_mode(area) {
            Ok(Mode::Orientation(o)) if o.reversed() => match seen {
                None => seen = Some(o),
                Some(s) if s == o => {}
                _ => return None,
            },
            _ => return None,
        }
    }
    seen
}

/// The positive orientation of the same axis.
pub(crate) fn positive(o: Orientation) -> Orientation {
    match o {
        Orientation::Rl => Orientation::Lr,
        Orientation::Bt => Orientation::Tb,
        other => other,
    }
}

/// The graph with every reversed direction turned positive.
pub(crate) fn positive_graph(graph: &Graph, reversed: Orientation) -> Graph {
    let turn = |d: &DirectionConstraint| match d {
        DirectionConstraint::Fixed(o) if *o == reversed => {
            DirectionConstraint::Fixed(positive(reversed))
        }
        other => other.clone(),
    };
    let mut out = graph.clone();
    out.default_direction = turn(&graph.default_direction);
    for d in out.container_directions.values_mut() {
        *d = turn(d);
    }
    out
}

/// Mirrors a drawing on the x axis (`vertical` false) or the y axis, area by area: every area's children are mirrored
/// within its box (the root's within the drawing's extent), and a container's content is moved back by the difference of
/// its two margins on that axis, so that a header on the near side stays where it was (a container keeps its header at its
/// top in both senses). The routes, labels, ports and border crossings (absolute) follow the area that holds each point,
/// a horizontal segment that passes a container's side changing its height there; the dummies of the stored plans follow
/// their areas; port and crossing sides swap. The operation is its own inverse.
pub(crate) fn mirror(snapshot: &mut Snapshot, vertical: bool) {
    let along = |r: &Rect| {
        if vertical {
            (r.y, r.height)
        } else {
            (r.x, r.width)
        }
    };
    let graph = snapshot.graph.clone();
    let parent: BTreeMap<String, Option<String>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.parent_id.clone()))
        .collect();
    let containers: BTreeSet<String> = parent.values().flatten().cloned().collect();
    let before = crate::absolute_all(&graph, &snapshot.boxes);
    // The root's extent on the axis: everything drawn.
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for r in before.values() {
        let (p, l) = along(r);
        lo = lo.min(p);
        hi = hi.max(p + l);
    }
    for route in &snapshot.routes {
        for p in &route.points {
            let v = if vertical { p.y } else { p.x };
            lo = lo.min(v);
            hi = hi.max(v);
        }
    }
    for label in &snapshot.labels {
        let (p, l) = along(&label.bounds);
        lo = lo.min(p);
        hi = hi.max(p + l);
    }
    if !(hi >= lo) {
        return;
    }
    let total = lo + hi;
    // By container: its length on the axis and the shift of its content (near margin less far margin).
    let mut frame: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for c in &containers {
        let Some(b) = snapshot.boxes.get(c) else {
            continue;
        };
        let length = along(b).1;
        let (mut near, mut far) = (f64::INFINITY, f64::NEG_INFINITY);
        for (id, p) in &parent {
            if p.as_deref() == Some(c.as_str()) {
                if let Some(r) = snapshot.boxes.get(id) {
                    let (q, l) = along(r);
                    near = near.min(q);
                    far = far.max(q + l);
                }
            }
        }
        let shift = if near.is_finite() {
            near - (length - far)
        } else {
            0.
        };
        frame.insert(c.clone(), (length, shift));
    }
    // A coordinate relative to an area (`None`: the root, absolute) of an element of `extent` along the axis, mirrored.
    let flip = |area: Option<&str>, v: f64, extent: f64| -> f64 {
        match area.and_then(|a| frame.get(a)) {
            Some(&(length, shift)) => length - v - extent + shift,
            None => total - v - extent,
        }
    };
    // Boxes, relative to their parents.
    let ids: Vec<String> = snapshot.boxes.keys().cloned().collect();
    for id in &ids {
        let area = parent.get(id).cloned().flatten();
        let r = snapshot.boxes.get_mut(id).unwrap();
        if vertical {
            r.y = flip(area.as_deref(), r.y, r.height);
        } else {
            r.x = flip(area.as_deref(), r.x, r.width);
        }
    }
    let after = crate::absolute_all(&graph, &snapshot.boxes);
    // The innermost container whose interior holds an absolute point (before the mirror), if any.
    let holder = |x: f64, y: f64| -> Option<&String> {
        containers
            .iter()
            .filter(|c| {
                before.get(*c).is_some_and(|r| {
                    x > r.x + 1e-6
                        && x < r.x + r.width - 1e-6
                        && y > r.y + 1e-6
                        && y < r.y + r.height - 1e-6
                })
            })
            .max_by(|a, b| {
                let depth = |c: &str| {
                    std::iter::successors(Some(c.to_string()), |x| parent.get(x).cloned().flatten())
                        .count()
                };
                depth(a).cmp(&depth(b)).then(b.cmp(a))
            })
    };
    // An absolute point mirrored in the frame of the area that holds the point `context` (the point itself by default).
    let map = |x: f64, y: f64, context: (f64, f64)| -> (f64, f64) {
        let area = holder(context.0, context.1);
        let v = if vertical { y } else { x };
        let w = match area {
            Some(c) => {
                let (o0, o1) = (along(&before[c]).0, along(&after[c]).0);
                o1 + flip(Some(c), v - o0, 0.)
            }
            None => total - v,
        };
        if vertical {
            (x, w)
        } else {
            (w, y)
        }
    };
    let point = |p: &Point| {
        let (x, y) = map(p.x, p.y, (p.x, p.y));
        Point { x, y }
    };
    // The sides of containers a segment along the other axis passes (where it changes from one area to another).
    let crossings_of = |a: &Point, b: &Point| -> Vec<f64> {
        let mut out: Vec<f64> = Vec::new();
        for c in &containers {
            let Some(r) = before.get(c) else { continue };
            let (u0, u1, v, v0, v1, w0, w1) = if vertical {
                // A horizontal segment (along x) passes the left and right sides.
                (
                    a.x.min(b.x),
                    a.x.max(b.x),
                    a.y,
                    r.y,
                    r.y + r.height,
                    r.x,
                    r.x + r.width,
                )
            } else {
                (
                    a.y.min(b.y),
                    a.y.max(b.y),
                    a.x,
                    r.x,
                    r.x + r.width,
                    r.y,
                    r.y + r.height,
                )
            };
            if v <= v0 + 1e-6 || v >= v1 - 1e-6 {
                continue;
            }
            for side in [w0, w1] {
                if side > u0 + 1e-6 && side < u1 - 1e-6 {
                    out.push(side);
                }
            }
        }
        out
    };
    for route in &mut snapshot.routes {
        let mut out: Vec<Point> = Vec::with_capacity(route.points.len());
        for (k, p) in route.points.iter().enumerate() {
            if k > 0 {
                let a = &route.points[k - 1];
                let across = if vertical {
                    (a.y - p.y).abs() < 1e-9
                } else {
                    (a.x - p.x).abs() < 1e-9
                };
                if across {
                    let mut sides = crossings_of(a, p);
                    let forward = if vertical { p.x > a.x } else { p.y > a.y };
                    sides.sort_by(|s, t| {
                        if forward {
                            s.total_cmp(t)
                        } else {
                            t.total_cmp(s)
                        }
                    });
                    sides.dedup();
                    for side in sides {
                        let step = if forward { 1e-3 } else { -1e-3 };
                        let (x, y) = if vertical { (side, a.y) } else { (a.x, side) };
                        let (before_ctx, after_ctx) = if vertical {
                            ((side - step, y), (side + step, y))
                        } else {
                            ((x, side - step), (x, side + step))
                        };
                        let (x1, y1) = map(x, y, before_ctx);
                        let (x2, y2) = map(x, y, after_ctx);
                        out.push(Point { x: x1, y: y1 });
                        if (x1 - x2).abs() > 1e-9 || (y1 - y2).abs() > 1e-9 {
                            out.push(Point { x: x2, y: y2 });
                        }
                    }
                }
            }
            out.push(point(p));
        }
        out.dedup_by(|a, b| (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9);
        route.points = out;
    }
    let flip_side = |s: &mut PortSide| {
        *s = match (*s, vertical) {
            (PortSide::Left, false) => PortSide::Right,
            (PortSide::Right, false) => PortSide::Left,
            (PortSide::Top, true) => PortSide::Bottom,
            (PortSide::Bottom, true) => PortSide::Top,
            (other, _) => other,
        }
    };
    for label in &mut snapshot.labels {
        let b = label.bounds.clone();
        let centre = Point {
            x: b.x + b.width / 2.,
            y: b.y + b.height / 2.,
        };
        let moved = point(&centre);
        label.bounds.x = moved.x - b.width / 2.;
        label.bounds.y = moved.y - b.height / 2.;
    }
    for port in &mut snapshot.resolved_ports {
        port.position = point(&port.position);
        flip_side(&mut port.side);
    }
    for crossing in &mut snapshot.boundary_crossings {
        crossing.position = point(&crossing.position);
        flip_side(&mut crossing.side);
    }
    for (area, plan) in snapshot.layered_plans.iter_mut() {
        let area = (area != "<root>").then_some(area.as_str());
        for r in plan.dummies.values_mut() {
            if vertical {
                r.y = flip(area, r.y, r.height);
            } else {
                r.x = flip(area, r.x, r.width);
            }
        }
    }
}
