//! Conflict proposals only. The real geometry scan decides whether a proposal is admissible.

/// Closest point on a segment, and its interpolation parameter. Degenerate segments are points.
pub(crate) fn closest(point: (f64, f64), a: (f64, f64), b: (f64, f64)) -> ((f64, f64), f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let norm = dx * dx + dy * dy;
    let t = if norm <= 1e-18 {
        0.
    } else {
        (((point.0 - a.0) * dx + (point.1 - a.1) * dy) / norm).clamp(0., 1.)
    };
    ((a.0 + t * dx, a.1 + t * dy), t)
}

/// Conservative box-support displacement away from a segment; a proposal, not a feasibility test.
/// At a point exactly on the segment, the oriented left normal breaks the tie deterministically.
pub(crate) fn segment_push(
    point: (f64, f64),
    a: (f64, f64),
    b: (f64, f64),
    extent: (f64, f64),
    clearance: f64,
) -> ((f64, f64), f64) {
    let (near, t) = closest(point, a, b);
    let (dx, dy) = (point.0 - near.0, point.1 - near.1);
    let distance = dx.hypot(dy);
    let normal = if distance > 1e-9 {
        (dx / distance, dy / distance)
    } else {
        let (vx, vy) = (b.0 - a.0, b.1 - a.1);
        let length = vx.hypot(vy);
        if length > 1e-9 {
            (-vy / length, vx / length)
        } else {
            (0., 1.)
        }
    };
    let reach = extent.0 / 2. * normal.0.abs() + extent.1 / 2. * normal.1.abs() + clearance;
    let shortfall = (reach - distance).max(0.);
    ((normal.0 * shortfall, normal.1 * shortfall), t)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn on_segment_and_degenerate_proposals_are_finite_and_stable() {
        assert_eq!(
            segment_push((50., 0.), (0., 0.), (100., 0.), (20., 40.), 10.),
            ((0., 30.), 0.5)
        );
        assert_eq!(
            segment_push((0., 0.), (0., 0.), (0., 0.), (20., 40.), 10.),
            ((0., 30.), 0.)
        );
        assert_eq!(
            segment_push((50., 100.), (0., 0.), (100., 0.), (20., 40.), 10.).0,
            (0., 0.)
        );
    }
}
