//! The geometry a route is drawn with, computed once so that the scene, the exports and the renderers show the same path.
//! Pure and deterministic. Provisional: the corner rule below is the one the viewer used to apply itself.
use crate::geom::{self, Rect};
use arqlet_model::Point;

/// The largest radius of a rounded corner (layout units).
pub const CORNER_RADIUS: f64 = 12.;
/// Points per rounded corner: the arc is a quadratic curve sampled at `ARC_STEPS + 1` points.
pub const ARC_STEPS: usize = 6;

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
