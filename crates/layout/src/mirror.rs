//! Experimental (`singlePassReversed`, ADR 0020 step S1-0): a drawing whose areas all flow right to left (`rl`) is laid
//! out left to right, where single pass runs, and mirrored on the horizontal axis (padding is the same on both sides). Logical
//! edge direction is kept: only coordinates are mirrored. Mirroring twice gives the drawing back, so a later layout starts
//! from the unmirrored earlier one.
use arqlet_model::{
    DirectionConstraint, Graph, LayoutOptions, Mode, Orientation, PortSide, Rect, Snapshot,
};
use std::collections::{BTreeMap, BTreeSet};

/// The reversed orientation `rl`, if every area of the graph has it and the drawing can be mirrored: no pins, label places,
/// explicit ports or buses (their places are given in the reversed frame), and every area a fixed flow. Not `bt`: a
/// container's header is at its top in both senses, so a mirror on the vertical axis would move children into it and shift
/// the axis by a header per level of nesting; `bt` needs a layout of its own (ADR 0020).
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
            Ok(Mode::Orientation(o)) if o == Orientation::Rl => match seen {
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

/// Mirrors a drawing on the x axis (`vertical` false) or the y axis, about the middle of its extent: boxes within their
/// parents (relative), routes, labels, ports and border crossings (absolute), and the dummies of the stored plans within
/// their areas; port and crossing sides swap.
pub(crate) fn mirror(snapshot: &mut Snapshot, vertical: bool) {
    let along = |r: &Rect| {
        if vertical {
            (r.y, r.height)
        } else {
            (r.x, r.width)
        }
    };
    let absolute = crate::absolute_all(&snapshot.graph, &snapshot.boxes);
    // The extent of everything drawn, on the mirrored axis.
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut include = |a: f64, b: f64| {
        lo = lo.min(a);
        hi = hi.max(b);
    };
    for r in absolute.values() {
        let (p, l) = along(r);
        include(p, p + l);
    }
    for route in &snapshot.routes {
        for p in &route.points {
            let v = if vertical { p.y } else { p.x };
            include(v, v);
        }
    }
    for label in &snapshot.labels {
        let (p, l) = along(&label.bounds);
        include(p, p + l);
    }
    if !(hi >= lo) {
        return;
    }
    let total = lo + hi;
    let flip_rect = |r: &mut Rect, extent: f64| {
        if vertical {
            r.y = extent - r.y - r.height;
        } else {
            r.x = extent - r.x - r.width;
        }
    };
    let flip_side = |s: &mut PortSide| {
        *s = match (*s, vertical) {
            (PortSide::Left, false) => PortSide::Right,
            (PortSide::Right, false) => PortSide::Left,
            (PortSide::Top, true) => PortSide::Bottom,
            (PortSide::Bottom, true) => PortSide::Top,
            (other, _) => other,
        }
    };
    // Boxes: within their parent's box, the root's children within the extent.
    let parent: BTreeMap<String, Option<String>> = snapshot
        .graph
        .nodes
        .iter()
        .map(|n| (n.id.clone(), n.parent_id.clone()))
        .collect();
    let size = |id: Option<&String>| -> f64 {
        match id.and_then(|p| snapshot.boxes.get(p)) {
            Some(r) => along(r).1,
            None => total,
        }
    };
    let extents: BTreeMap<String, f64> = snapshot
        .boxes
        .keys()
        .map(|id| (id.clone(), size(parent.get(id).and_then(|p| p.as_ref()))))
        .collect();
    for (id, r) in snapshot.boxes.iter_mut() {
        flip_rect(r, extents[id]);
    }
    for route in &mut snapshot.routes {
        for p in &mut route.points {
            if vertical {
                p.y = total - p.y;
            } else {
                p.x = total - p.x;
            }
        }
    }
    for label in &mut snapshot.labels {
        flip_rect(&mut label.bounds, total);
    }
    for port in &mut snapshot.resolved_ports {
        if vertical {
            port.position.y = total - port.position.y;
        } else {
            port.position.x = total - port.position.x;
        }
        flip_side(&mut port.side);
    }
    for crossing in &mut snapshot.boundary_crossings {
        if vertical {
            crossing.position.y = total - crossing.position.y;
        } else {
            crossing.position.x = total - crossing.position.x;
        }
        flip_side(&mut crossing.side);
    }
    for (area, plan) in snapshot.layered_plans.iter_mut() {
        let extent = if area == "<root>" {
            total
        } else {
            snapshot.boxes.get(area).map_or(total, |r| along(r).1)
        };
        for r in plan.dummies.values_mut() {
            flip_rect(r, extent);
        }
    }
}
