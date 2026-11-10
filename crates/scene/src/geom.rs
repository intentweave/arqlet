//! Pure segment and rectangle geometry shared by the metrics (and, later, the layout and routing).
//! Tolerances are absolute; every function is deterministic and free of host dependencies.
use arqlet_model::{Point, PortSide};

pub const EPS: f64 = 1e-6;

#[derive(Clone, Copy)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

impl Rect {
    pub fn inflate(self, by: f64) -> Self {
        Rect {
            x0: self.x0 - by,
            y0: self.y0 - by,
            x1: self.x1 + by,
            y1: self.y1 + by,
        }
    }
}

pub fn dist(a: &Point, b: &Point) -> f64 {
    (b.x - a.x).hypot(b.y - a.y)
}

pub fn same(a: &Point, b: &Point) -> bool {
    (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS
}

pub fn ends(points: &[Point]) -> Vec<&Point> {
    match (points.first(), points.last()) {
        (Some(a), Some(b)) => vec![a, b],
        _ => vec![],
    }
}

pub fn dedupe(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for p in points {
        if out.last().is_none_or(|q| !same(q, p)) {
            out.push(p.clone());
            while out.len() >= 3 {
                let n = out.len();
                let (a, b, c) = (&out[n - 3], &out[n - 2], &out[n - 1]);
                let cross = (b.x - a.x) * (c.y - b.y) - (b.y - a.y) * (c.x - b.x);
                let dot = (b.x - a.x) * (c.x - b.x) + (b.y - a.y) * (c.y - b.y);
                if cross.abs() < EPS && dot > 0. {
                    out.remove(n - 2);
                } else {
                    break;
                }
            }
        }
    }
    out
}

/// Liang–Barsky clipping of a segment to a closed rectangle: the parameter interval inside it.
pub fn clip(a: &Point, b: &Point, r: &Rect) -> Option<(f64, f64)> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut t0, mut t1) = (0f64, 1f64);
    for (p, q) in [
        (-dx, a.x - r.x0),
        (dx, r.x1 - a.x),
        (-dy, a.y - r.y0),
        (dy, r.y1 - a.y),
    ] {
        if p.abs() < EPS {
            if q < -EPS {
                return None;
            }
        } else {
            let t = q / p;
            if p < 0. {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    (t1 - t0 > EPS).then_some((t0, t1))
}

pub fn clip_length(a: &Point, b: &Point, r: &Rect) -> f64 {
    clip(a, b, r).map_or(0., |(t0, t1)| (t1 - t0) * dist(a, b))
}

/// True when the segment passes through the open interior of the node (not along its border).
pub fn enters(a: &Point, b: &Point, r: &Rect) -> bool {
    let Some((t0, t1)) = clip(a, b, r) else {
        return false;
    };
    let t = (t0 + t1) / 2.;
    let x = a.x + (b.x - a.x) * t;
    let y = a.y + (b.y - a.y) * t;
    x > r.x0 + EPS && x < r.x1 - EPS && y > r.y0 + EPS && y < r.y1 - EPS
}

/// Pulls a route taut: from each kept point, the farthest later point of the route that the direct segment reaches without
/// entering an obstacle. The ends stay; every kept point is a point of the route, and the next one is always accepted, so
/// a route that is clear stays clear. The result is a polyline whose segments may run at any angle.
pub fn pull_taut(points: &[Point], obstacles: &[Rect]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let clear = |a: &Point, b: &Point| !obstacles.iter().any(|r| enters(a, b, r));
    let mut out = vec![points[0].clone()];
    let mut at = 0;
    while at < points.len() - 1 {
        let next = (at + 2..points.len())
            .rev()
            .find(|&j| clear(&points[at], &points[j]))
            .unwrap_or(at + 1);
        out.push(points[next].clone());
        at = next;
    }
    out
}

/// Where a polyline of any angle crosses the border of a rectangle, in route order: the side it crosses and the point. A
/// segment that runs along a border does not cross it, and a crossing at a segment's start belongs to the segment before.
pub fn border_crossings(points: &[Point], r: &Rect) -> Vec<(PortSide, Point)> {
    let mut found: Vec<(f64, PortSide, Point)> = Vec::new();
    for (i, w) in points.windows(2).enumerate() {
        let (a, b) = (&w[0], &w[1]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let along_vertical =
            dx.abs() <= EPS && ((a.x - r.x0).abs() <= EPS || (a.x - r.x1).abs() <= EPS);
        let along_horizontal =
            dy.abs() <= EPS && ((a.y - r.y0).abs() <= EPS || (a.y - r.y1).abs() <= EPS);
        if along_vertical || along_horizontal {
            continue;
        }
        let mut here: Vec<(f64, PortSide, Point)> = Vec::new();
        if dx.abs() > EPS {
            for (x, side) in [(r.x0, PortSide::Left), (r.x1, PortSide::Right)] {
                let t = (x - a.x) / dx;
                let y = a.y + dy * t;
                if t > EPS && t <= 1. + EPS && y > r.y0 - EPS && y < r.y1 + EPS {
                    here.push((i as f64 + t, side, Point { x, y }));
                }
            }
        }
        if dy.abs() > EPS {
            for (y, side) in [(r.y0, PortSide::Top), (r.y1, PortSide::Bottom)] {
                let t = (y - a.y) / dy;
                let x = a.x + dx * t;
                if t > EPS && t <= 1. + EPS && x > r.x0 - EPS && x < r.x1 + EPS {
                    here.push((i as f64 + t, side, Point { x, y }));
                }
            }
        }
        here.sort_by(|p, q| p.0.total_cmp(&q.0));
        found.extend(here);
    }
    found.into_iter().map(|(_, side, p)| (side, p)).collect()
}

pub struct Overlap {
    pub lo: f64,
    pub hi: f64,
    pub length: f64,
}

/// Overlap of two collinear segments, measured along the first segment's direction.
pub fn collinear_overlap(a: &Point, b: &Point, c: &Point, d: &Point) -> Option<Overlap> {
    let len = dist(a, b);
    if len < EPS || offset(a, b, c).abs() > EPS || offset(a, b, d).abs() > EPS {
        return None;
    }
    let (tc, td) = (along(a, b, c)?, along(a, b, d)?);
    let lo = tc.min(td).max(0.);
    let hi = tc.max(td).min(len);
    (hi - lo > EPS).then_some(Overlap {
        lo,
        hi,
        length: hi - lo,
    })
}

/// Signed distance of `p` from the infinite line through a and b (0 for a degenerate segment).
pub fn offset(a: &Point, b: &Point, p: &Point) -> f64 {
    let len = dist(a, b);
    if len < EPS {
        return 0.;
    }
    ((b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)) / len
}

/// Position of `p` along the segment a→b, in length units from `a`.
pub fn along(a: &Point, b: &Point, p: &Point) -> Option<f64> {
    let len = dist(a, b);
    if len < EPS {
        return None;
    }
    Some(((p.x - a.x) * (b.x - a.x) + (p.y - a.y) * (b.y - a.y)) / len)
}

pub fn proper_crossing(a: &Point, b: &Point, c: &Point, d: &Point) -> bool {
    let orient =
        |p: &Point, q: &Point, r: &Point| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (o1, o2) = (orient(a, b, c), orient(a, b, d));
    let (o3, o4) = (orient(c, d, a), orient(c, d, b));
    ((o1 > EPS && o2 < -EPS) || (o1 < -EPS && o2 > EPS))
        && ((o3 > EPS && o4 < -EPS) || (o3 < -EPS && o4 > EPS))
}

/// Endpoints of `segs` lying strictly inside a segment of `others`, where the two are not collinear.
pub fn touches(segs: &[(Point, Point)], others: &[(Point, Point)]) -> usize {
    // A point that touches a segment lies within EPS of it; a looser box test skips the far segments.
    const NEAR: f64 = 1000. * EPS;
    let mut count = 0;
    for (a, b) in segs {
        for p in [a, b] {
            for (c, d) in others {
                if p.x < c.x.min(d.x) - NEAR
                    || p.x > c.x.max(d.x) + NEAR
                    || p.y < c.y.min(d.y) - NEAR
                    || p.y > c.y.max(d.y) + NEAR
                {
                    continue;
                }
                let on = offset(c, d, p).abs() < EPS;
                let within = along(c, d, p).is_some_and(|t| t > EPS && t < dist(c, d) - EPS);
                // Parallel segments on one line are overlaps, not touches.
                let parallel = offset(c, d, a).abs() < EPS && offset(c, d, b).abs() < EPS;
                if on && within && !parallel {
                    count += 1;
                }
            }
        }
    }
    count
}

#[cfg(test)]
mod pull_taut_tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn a_clear_route_loses_its_bends_and_keeps_its_ends() {
        let route = [p(0., 0.), p(50., 0.), p(50., 40.), p(100., 40.)];
        assert_eq!(pull_taut(&route, &[]), vec![p(0., 0.), p(100., 40.)]);
    }

    #[test]
    fn a_bend_that_goes_around_an_obstacle_stays() {
        let block = Rect {
            x0: 40.,
            y0: 10.,
            x1: 60.,
            y1: 30.,
        };
        let route = [p(0., 0.), p(50., 0.), p(50., 40.), p(100., 40.)];
        let taut = pull_taut(&route, &[block]);
        assert_eq!(taut.first(), route.first());
        assert_eq!(taut.last(), route.last());
        for w in taut.windows(2) {
            assert!(!enters(&w[0], &w[1], &block), "{taut:?}");
        }
    }

    #[test]
    fn a_diagonal_route_crosses_a_border_where_it_meets_it() {
        let r = Rect {
            x0: 0.,
            y0: 0.,
            x1: 100.,
            y1: 50.,
        };
        let out = border_crossings(&[p(50., 25.), p(150., 125.)], &r);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, PortSide::Bottom);
        assert!((out[0].1.x - 75.).abs() < 1e-9 && (out[0].1.y - 50.).abs() < 1e-9);
        // Along a border is no crossing, and a route that stays inside crosses nothing.
        assert!(border_crossings(&[p(0., -10.), p(0., 60.)], &r).is_empty());
        assert!(border_crossings(&[p(10., 10.), p(90., 40.)], &r).is_empty());
    }

    #[test]
    fn short_routes_are_unchanged() {
        let route = [p(0., 0.), p(10., 10.)];
        assert_eq!(pull_taut(&route, &[]), route.to_vec());
    }
}
