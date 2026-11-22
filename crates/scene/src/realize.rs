//! The geometry a route is drawn with, computed once so that the scene, the exports and the renderers show the same path.
//! Pure and deterministic. Provisional: the corner rule below is the one the viewer used to apply itself.
use crate::geom::{self, Rect};
use arqlet_model::Point;

/// The largest radius of a rounded corner (layout units).
pub const CORNER_RADIUS: f64 = 12.;
/// Points per rounded corner: the arc is a quadratic curve sampled at `ARC_STEPS + 1` points.
pub const ARC_STEPS: usize = 6;
/// Points per piece of a curve at most, and the length one point stands for (layout units).
pub const CURVE_STEPS: usize = 16;
const CURVE_STEP_LENGTH: f64 = 8.;

/// A smooth curve through the points of a route (a centripetal Catmull-Rom spline: it passes through every point, does
/// not loop or overshoot at uneven spacing), sampled into points: per piece between two route points as many as its
/// length needs (at most `CURVE_STEPS`). A piece whose samples would enter an obstacle stays the straight segment, which
/// the caller has kept clear. The ends and the route points stay where they are.
pub fn smooth_curve(points: &[Point], obstacles: &[Rect]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let n = points.len();
    let at = |i: isize| -> &Point { &points[i.clamp(0, n as isize - 1) as usize] };
    let mut out = vec![points[0].clone()];
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (
            at(i as isize - 1),
            at(i as isize),
            at(i as isize + 1),
            at(i as isize + 2),
        );
        let length = geom::dist(p1, p2);
        let steps = ((length / CURVE_STEP_LENGTH).ceil() as usize).clamp(2, CURVE_STEPS);
        let piece: Vec<Point> = (1..=steps)
            .map(|k| catmull_rom(p0, p1, p2, p3, k as f64 / steps as f64))
            .collect();
        let mut previous = p1.clone();
        let clear = piece.iter().all(|q| {
            let ok = !obstacles.iter().any(|o| geom::enters(&previous, q, o));
            previous = q.clone();
            ok
        });
        if clear {
            out.extend(piece);
        } else {
            out.push(p2.clone());
        }
    }
    out
}

// The centripetal Catmull-Rom point at `t` in 0..1 between p1 and p2 (Barry and Goldman's form, alpha 0.5). Equal
// neighbours (the ends) are handled by the knot spacing's floor.
fn catmull_rom(p0: &Point, p1: &Point, p2: &Point, p3: &Point, t: f64) -> Point {
    let knot = |a: &Point, b: &Point| geom::dist(a, b).sqrt().max(1e-6);
    let (t0, t1) = (0., knot(p0, p1));
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    let u = t1 + (t2 - t1) * t;
    let lerp = |a: &Point, b: &Point, ta: f64, tb: f64| {
        let w = (u - ta) / (tb - ta);
        Point {
            x: a.x + (b.x - a.x) * w,
            y: a.y + (b.y - a.y) * w,
        }
    };
    let a1 = lerp(p0, p1, t0, t1);
    let a2 = lerp(p1, p2, t1, t2);
    let a3 = lerp(p2, p3, t2, t3);
    let b1 = lerp(&a1, &a2, t0, t2);
    let b2 = lerp(&a2, &a3, t1, t3);
    lerp(&b1, &b2, t1, t2)
}

/// The longest cut of an octilinear corner along each of its two segments (layout units): the diagonal is this times √2.
pub const CHAMFER: f64 = 12.;

/// Cuts every right-angled corner of an orthogonal route at 45°: the corner point gives way to two points `cut` before and
/// after it on its segments, at most half of each segment, so the diagonal between them runs at 45°. When the diagonal
/// would enter an obstacle, the cut of that corner shrinks (halving, down to 0.5) until it stays clear; if none does, the
/// corner stays as the route has it. A corner that is not a right angle (a straight or diagonal segment meets it) and the
/// ends stay where they are.
pub fn chamfer_corners(points: &[Point], cut: f64, obstacles: &[Rect]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    const EPS: f64 = 1e-9;
    let axis = |a: &Point, b: &Point| (a.x - b.x).abs() < EPS || (a.y - b.y).abs() < EPS;
    let mut out = vec![points[0].clone()];
    for i in 1..points.len() - 1 {
        let (a, b, c) = (&points[i - 1], &points[i], &points[i + 1]);
        let (ab, bc) = (geom::dist(a, b), geom::dist(b, c));
        let turns = ((a.x - b.x) * (c.y - b.y) - (a.y - b.y) * (c.x - b.x)).abs() > EPS;
        let right = turns && axis(a, b) && axis(b, c) && ab > EPS && bc > EPS;
        if !right {
            out.push(b.clone());
            continue;
        }
        let mut d = cut.min(ab / 2.).min(bc / 2.);
        let cutting = loop {
            if d < 0.5 {
                break None;
            }
            let from = Point {
                x: b.x + (a.x - b.x) * d / ab,
                y: b.y + (a.y - b.y) * d / ab,
            };
            let to = Point {
                x: b.x + (c.x - b.x) * d / bc,
                y: b.y + (c.y - b.y) * d / bc,
            };
            if !obstacles.iter().any(|o| geom::enters(&from, &to, o)) {
                break Some([from, to]);
            }
            d /= 2.;
        };
        // Two cuts that meet in the middle of a short segment give one point, not two.
        let mut push = |q: Point| {
            if out
                .last()
                .is_none_or(|l| (l.x - q.x).abs() > EPS || (l.y - q.y).abs() > EPS)
            {
                out.push(q);
            }
        };
        match cutting {
            Some([from, to]) => {
                push(from);
                push(to);
            }
            None => push(b.clone()),
        }
    }
    out.push(points[points.len() - 1].clone());
    out
}

/// Rounds the corners of a route. A corner takes `radius`, at most half of each adjacent segment. When the arc would enter
/// an obstacle, the radius of that corner shrinks (halving, down to 0.5) until it stays clear; if none does, the corner stays
/// sharp, as the route has it. The style never changes: a rounded route stays rounded where there is room. The ends and the
/// straight runs stay where the route has them.
pub fn round_corners(points: &[Point], radius: f64, obstacles: &[Rect]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let mut out = vec![points[0].clone()];
    for i in 1..points.len() - 1 {
        let (a, b, c) = (&points[i - 1], &points[i], &points[i + 1]);
        let (ab, bc) = (geom::dist(a, b), geom::dist(b, c));
        let mut r = radius.min(ab / 2.).min(bc / 2.);
        let arc = loop {
            if r < 0.5 {
                break None;
            }
            let arc = corner(a, b, c, ab, bc, r);
            let clear = arc
                .windows(2)
                .all(|w| !obstacles.iter().any(|o| geom::enters(&w[0], &w[1], o)));
            if clear {
                break Some(arc);
            }
            r /= 2.;
        };
        match arc {
            Some(arc) => out.extend(arc),
            None => out.push(b.clone()),
        }
    }
    out.push(points[points.len() - 1].clone());
    out
}

fn corner(a: &Point, b: &Point, c: &Point, ab: f64, bc: f64, r: f64) -> Vec<Point> {
    let from = Point {
        x: b.x + (a.x - b.x) * r / ab,
        y: b.y + (a.y - b.y) * r / ab,
    };
    let to = Point {
        x: b.x + (c.x - b.x) * r / bc,
        y: b.y + (c.y - b.y) * r / bc,
    };
    (0..=ARC_STEPS)
        .map(|k| {
            let t = k as f64 / ARC_STEPS as f64;
            let u = 1. - t;
            Point {
                x: u * u * from.x + 2. * u * t * b.x + t * t * to.x,
                y: u * u * from.y + 2. * u * t * b.y + t * t * to.y,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn a_curve_passes_through_the_route_points_and_keeps_the_ends() {
        let route = [p(0., 0.), p(100., 0.), p(100., 100.), p(200., 100.)];
        let out = smooth_curve(&route, &[]);
        assert_eq!(out.first(), route.first());
        assert_eq!(out.last(), route.last());
        assert!(out.len() > route.len() * 3, "{}", out.len());
        // It passes through every route point.
        for q in &route {
            assert!(out.iter().any(|o| geom::dist(o, q) < 1e-9), "{q:?}");
        }
        // It is smooth: no turn between two samples is sharper than 45°.
        for w in out.windows(3) {
            let (a, b, c) = (&w[0], &w[1], &w[2]);
            let (u, v) = ((b.x - a.x, b.y - a.y), (c.x - b.x, c.y - b.y));
            let cos = (u.0 * v.0 + u.1 * v.1) / ((u.0.hypot(u.1)) * (v.0.hypot(v.1))).max(1e-12);
            assert!(cos > 0.70, "{cos} at {b:?}");
        }
    }

    #[test]
    fn a_piece_that_would_touch_a_node_stays_straight() {
        let route = [p(0., 0.), p(100., 0.), p(100., 100.)];
        let free = smooth_curve(&route, &[]);
        // The free curve bulges out beyond (100, 0); a node there keeps that piece straight.
        assert!(free.iter().any(|q| q.x > 100. + 1e-6 || q.y < -1e-6));
        let block = Rect {
            x0: 100.5,
            y0: -20.,
            x1: 130.,
            y1: 60.,
        };
        let kept = smooth_curve(&route, &[block]);
        for w in kept.windows(2) {
            assert!(!geom::enters(&w[0], &w[1], &block));
        }
        assert!(kept.iter().any(|q| q == &p(100., 0.)));
    }

    #[test]
    fn an_octilinear_corner_is_cut_at_45_degrees_and_the_ends_stay() {
        let route = [p(0., 0.), p(100., 0.), p(100., 100.)];
        let out = chamfer_corners(&route, CHAMFER, &[]);
        assert_eq!(
            out,
            vec![
                p(0., 0.),
                p(100. - CHAMFER, 0.),
                p(100., CHAMFER),
                p(100., 100.)
            ]
        );
        // The diagonal runs at 45°.
        let (dx, dy) = (out[2].x - out[1].x, out[2].y - out[1].y);
        assert!((dx.abs() - dy.abs()).abs() < 1e-9);
    }

    #[test]
    fn a_short_segment_shortens_the_cut_and_an_obstacle_keeps_the_corner() {
        // A 10-unit step: each cut at most half of it.
        let step = [p(0., 0.), p(50., 0.), p(50., 10.), p(100., 10.)];
        let out = chamfer_corners(&step, CHAMFER, &[]);
        assert_eq!(out[1], p(45., 0.));
        assert_eq!(out[2], p(50., 5.));
        // A node right inside the corner: the cut shrinks until it is clear.
        let corner = [p(0., 0.), p(100., 0.), p(100., 100.)];
        let block = Rect {
            x0: 93.,
            y0: 1.,
            x1: 99.,
            y1: 7.,
        };
        let cut = chamfer_corners(&corner, CHAMFER, &[block]);
        assert_eq!(cut.len(), 4);
        let d = 100. - cut[1].x;
        assert!(d < CHAMFER && d >= 0.5, "{d}");
        assert!(!geom::enters(&cut[1], &cut[2], &block));
        // A corner that is not a right angle stays.
        let slanted = [p(0., 0.), p(50., 50.), p(100., 50.)];
        assert_eq!(chamfer_corners(&slanted, CHAMFER, &[]), slanted.to_vec());
    }

    #[test]
    fn a_corner_is_rounded_and_the_ends_stay() {
        let route = [p(0., 0.), p(100., 0.), p(100., 100.)];
        let out = round_corners(&route, CORNER_RADIUS, &[]);
        assert_eq!(out.len(), 2 + ARC_STEPS + 1);
        assert_eq!(out.first(), route.first());
        assert_eq!(out.last(), route.last());
        // The arc starts and ends on the two segments, at the radius from the corner.
        assert!((out[1].x - 88.).abs() < 1e-9 && out[1].y.abs() < 1e-9);
        assert!(
            (out[ARC_STEPS + 1].x - 100.).abs() < 1e-9 && (out[ARC_STEPS + 1].y - 12.).abs() < 1e-9
        );
    }

    #[test]
    fn a_short_segment_limits_the_radius() {
        let out = round_corners(&[p(0., 0.), p(10., 0.), p(10., 40.)], CORNER_RADIUS, &[]);
        assert_eq!(out[1], p(5., 0.));
    }

    #[test]
    fn a_route_with_fewer_than_three_points_is_unchanged() {
        let route = [p(0., 0.), p(10., 10.)];
        assert_eq!(round_corners(&route, CORNER_RADIUS, &[]), route.to_vec());
    }

    #[test]
    fn an_arc_never_enters_an_obstacle_and_the_radius_shrinks_first() {
        // A block sits in the inside of the corner: the full arc would cut into it.
        let block = Rect {
            x0: 86.,
            y0: 2.,
            x1: 98.,
            y1: 14.,
        };
        let route = [p(0., 0.), p(100., 0.), p(100., 100.)];
        let out = round_corners(&route, CORNER_RADIUS, &[block]);
        for w in out.windows(2) {
            assert!(!geom::enters(&w[0], &w[1], &block), "{out:?}");
        }
        // The corner is still rounded (more points than the sharp route), just with a smaller radius.
        assert!(out.len() > route.len());
        assert!(out[1].x > 88.);
    }

    #[test]
    fn a_corner_that_has_no_room_stays_sharp() {
        let block = Rect {
            x0: 98.,
            y0: 1.,
            x1: 99.9,
            y1: 3.,
        };
        // Even the smallest arc passes through the block.
        let out = round_corners(
            &[p(0., 0.), p(100., 0.), p(100., 100.)],
            CORNER_RADIUS,
            &[block],
        );
        for w in out.windows(2) {
            assert!(!geom::enters(&w[0], &w[1], &block), "{out:?}");
        }
    }
}
