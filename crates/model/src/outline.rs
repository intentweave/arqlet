//! The real outline of a node, for collision and clearance. A rectangle is its box; a circle is the disc inscribed in its
//! (square) box. Pure geometry: no renderer, no I/O.

use crate::math::PortableMath;
use crate::{NodeShape, Rect};

/// Where a point lies against an outline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Position {
    Inside,
    Boundary,
    Outside,
}

/// The outline a node occupies in the drawing.
#[derive(Clone, Debug)]
pub enum Outline {
    Rect(Rect),
    Disc { x: f64, y: f64, radius: f64 },
}

impl Outline {
    /// The outline of a node of `shape` in the box `rect`. A circle is the disc inscribed in the box.
    pub fn of(shape: NodeShape, rect: &Rect) -> Outline {
        match shape {
            NodeShape::Rect | NodeShape::Square => Outline::Rect(rect.clone()),
            NodeShape::Circle => Outline::Disc {
                x: rect.x + rect.width / 2.,
                y: rect.y + rect.height / 2.,
                radius: rect.width.min(rect.height) / 2.,
            },
        }
    }
}

impl Outline {
    /// The box around the outline: the rectangle itself, or the square around a disc.
    pub fn bounds(&self) -> Rect {
        match self {
            Outline::Rect(r) => r.clone(),
            Outline::Disc { x, y, radius } => Rect {
                x: x - radius,
                y: y - radius,
                width: 2. * radius,
                height: 2. * radius,
            },
        }
    }

    /// Classifies a point against the outline: inside, on the boundary (within `tolerance`), or outside.
    pub fn classify(&self, x: f64, y: f64, tolerance: f64) -> Position {
        let signed = match self {
            Outline::Disc {
                x: cx,
                y: cy,
                radius,
            } => (x - cx).p_hypot(y - cy) - radius,
            Outline::Rect(r) => {
                // Signed distance to the box: negative inside, the larger of the two per-axis overhangs.
                let dx = (r.x - x).max(x - (r.x + r.width));
                let dy = (r.y - y).max(y - (r.y + r.height));
                if dx <= 0. && dy <= 0. {
                    dx.max(dy)
                } else {
                    dx.max(0.).p_hypot(dy.max(0.))
                }
            }
        };
        if signed.abs() <= tolerance {
            Position::Boundary
        } else if signed < 0. {
            Position::Inside
        } else {
            Position::Outside
        }
    }

    /// How far the outline reaches beyond the disc with centre `(cx, cy)` and `radius`; zero when it lies inside. The farthest
    /// corner of a rectangle, or the far side of a disc, decides.
    pub fn overshoot_beyond_disc(&self, cx: f64, cy: f64, radius: f64) -> f64 {
        let reach = match self {
            Outline::Disc { x, y, radius: r } => (x - cx).p_hypot(y - cy) + r,
            Outline::Rect(r) => [
                (r.x, r.y),
                (r.x + r.width, r.y),
                (r.x, r.y + r.height),
                (r.x + r.width, r.y + r.height),
            ]
            .iter()
            .map(|(x, y)| (x - cx).p_hypot(y - cy))
            .fold(0., f64::max),
        };
        (reach - radius).max(0.)
    }

    /// The box grown by `margin` on every side: a conservative bound for broad-phase tests with a clearance.
    pub fn bounds_with_margin(&self, margin: f64) -> Rect {
        let b = self.bounds();
        Rect {
            x: b.x - margin,
            y: b.y - margin,
            width: b.width + 2. * margin,
            height: b.height + 2. * margin,
        }
    }

    /// The point of the outline's boundary that a connection toward `(x, y)` leaves from: a disc's boundary on the ray from
    /// its centre; a rectangle's boundary where the ray from its centre leaves it. `None` when the target is the centre.
    pub fn attachment_toward(&self, x: f64, y: f64) -> Option<(f64, f64)> {
        match self {
            Outline::Disc {
                x: cx,
                y: cy,
                radius,
            } => {
                let (dx, dy) = (x - cx, y - cy);
                let length = dx.p_hypot(dy);
                (length > 0.).then(|| (cx + radius * dx / length, cy + radius * dy / length))
            }
            Outline::Rect(r) => {
                let (cx, cy) = (r.x + r.width / 2., r.y + r.height / 2.);
                let (dx, dy) = (x - cx, y - cy);
                if dx == 0. && dy == 0. {
                    return None;
                }
                // The scale at which the ray leaves the box on its nearer axis.
                let tx = if dx == 0. {
                    f64::INFINITY
                } else {
                    (r.width / 2.) / dx.abs()
                };
                let ty = if dy == 0. {
                    f64::INFINITY
                } else {
                    (r.height / 2.) / dy.abs()
                };
                let t = tx.min(ty);
                Some((cx + t * dx, cy + t * dy))
            }
        }
    }

    /// Whether a segment from `(ax, ay)` to `(bx, by)` enters the outline grown by `margin` (the interior, for margin 0). A
    /// segment that only touches the boundary does not enter it. This is the test a route uses against a node.
    pub fn segment_enters(&self, ax: f64, ay: f64, bx: f64, by: f64, margin: f64) -> bool {
        match self {
            Outline::Disc { x, y, radius } => {
                let (dx, dy) = (bx - ax, by - ay);
                let len2 = dx * dx + dy * dy;
                let t = if len2 <= 0. {
                    0.
                } else {
                    (((x - ax) * dx + (y - ay) * dy) / len2).clamp(0., 1.)
                };
                // The same tiny inset as the rectangle: a port that lies on the outline up to rounding does not enter it.
                (ax + t * dx - x).p_hypot(ay + t * dy - y) < radius + margin - 1e-9
            }
            Outline::Rect(r) => {
                // Liang–Barsky clipping against the rectangle grown by the margin, shrunk by a tiny inset so that a
                // segment along its boundary does not count as entering.
                let inset = 1e-9;
                let (left, right) = (r.x - margin + inset, r.x + r.width + margin - inset);
                let (top, bottom) = (r.y - margin + inset, r.y + r.height + margin - inset);
                let (mut t0, mut t1) = (0f64, 1f64);
                for (p, q) in [
                    (-(bx - ax), ax - left),
                    (bx - ax, right - ax),
                    (-(by - ay), ay - top),
                    (by - ay, bottom - ay),
                ] {
                    if p == 0. {
                        if q < 0. {
                            return false;
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
                t0 < t1
            }
        }
    }
}

/// Whether two rectangles overlap in their interiors.
fn rects_collide(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height
}

/// The closest point of a rectangle to a point.
fn closest_on_rect(rect: &Rect, x: f64, y: f64) -> (f64, f64) {
    (
        x.clamp(rect.x, rect.x + rect.width),
        y.clamp(rect.y, rect.y + rect.height),
    )
}

/// Whether two outlines overlap in their interiors.
pub fn collides(a: &Outline, b: &Outline) -> bool {
    match (a, b) {
        (Outline::Rect(p), Outline::Rect(q)) => rects_collide(p, q),
        (
            Outline::Disc { x, y, radius },
            Outline::Disc {
                x: u,
                y: v,
                radius: s,
            },
        ) => (x - u).p_hypot(y - v) < radius + s,
        (Outline::Disc { x, y, radius }, Outline::Rect(r))
        | (Outline::Rect(r), Outline::Disc { x, y, radius }) => {
            let (qx, qy) = closest_on_rect(r, *x, *y);
            (x - qx).p_hypot(y - qy) < *radius
        }
    }
}

/// Whether two outlines come closer than `gap` (their clearance band overlaps). Two rectangles are closer when both their
/// axis gaps are below `gap`; discs and rectangles use the true distance between the outlines.
pub fn within(a: &Outline, b: &Outline, gap: f64) -> bool {
    match (a, b) {
        // Axis overlaps: negative values are separations. Two rectangles are within the gap when both axes are within it.
        (Outline::Rect(p), Outline::Rect(q)) => {
            let overlap_x = (p.x + p.width).min(q.x + q.width) - p.x.max(q.x);
            let overlap_y = (p.y + p.height).min(q.y + q.height) - p.y.max(q.y);
            overlap_x > -gap && overlap_y > -gap
        }
        (
            Outline::Disc { x, y, radius },
            Outline::Disc {
                x: u,
                y: v,
                radius: s,
            },
        ) => (x - u).p_hypot(y - v) < radius + s + gap,
        (Outline::Disc { x, y, radius }, Outline::Rect(r))
        | (Outline::Rect(r), Outline::Disc { x, y, radius }) => {
            let (qx, qy) = closest_on_rect(r, *x, *y);
            (x - qx).p_hypot(y - qy) < radius + gap
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect {
            x,
            y,
            width: w,
            height: h,
        }
    }

    /// A circle is the disc inscribed in its box: a box corner is outside the circle, so two circles in neighbouring corners
    /// do not collide where their boxes would.
    #[test]
    fn a_circle_is_its_inscribed_disc_not_its_box() {
        let a = Outline::of(NodeShape::Circle, &rect(0., 0., 100., 100.));
        let b = Outline::of(NodeShape::Circle, &rect(90., 90., 100., 100.));
        assert!(!collides(&a, &b), "the boxes overlap, the discs do not");
        let box_a = Outline::Rect(rect(0., 0., 100., 100.));
        let box_b = Outline::Rect(rect(90., 90., 100., 100.));
        assert!(collides(&box_a, &box_b));
    }

    /// A disc and a rectangle collide by their true distance; a label in the corner of the box is clear of the disc.
    #[test]
    fn a_label_in_the_box_corner_is_clear_of_the_disc() {
        let disc = Outline::of(NodeShape::Circle, &rect(0., 0., 100., 100.));
        let corner = Outline::Rect(rect(92., 92., 60., 20.));
        assert!(!collides(&disc, &corner));
        assert!(!within(&disc, &corner, 4.));
        let inside = Outline::Rect(rect(40., 40., 20., 20.));
        assert!(collides(&disc, &inside));
    }

    /// A route through the empty corner of a circle's box does not enter the disc; a route through its centre does.
    #[test]
    fn a_segment_enters_a_disc_only_through_the_disc() {
        let disc = Outline::of(NodeShape::Circle, &rect(0., 0., 100., 100.));
        assert!(disc.segment_enters(0., 50., 100., 50., 0.));
        assert!(!disc.segment_enters(80., 100., 100., 80., 0.));
        assert!(disc.segment_enters(80., 100., 100., 80., 30.));
    }

    /// A rectangle is entered by a segment across it, not by one along its edge.
    #[test]
    fn a_segment_enters_a_rectangle_across_its_interior_only() {
        let r = Outline::Rect(rect(0., 0., 100., 100.));
        assert!(r.segment_enters(-10., 50., 110., 50., 0.));
        assert!(!r.segment_enters(-10., 0., 110., 0., 0.));
        assert!(!r.segment_enters(120., 50., 130., 50., 0.));
        assert!(r.segment_enters(120., 50., 130., 50., 30.));
    }

    /// Clearance: a rectangle just beyond the disc is within a larger gap but not a smaller one.
    #[test]
    fn clearance_compares_the_true_distance() {
        let disc = Outline::of(NodeShape::Circle, &rect(0., 0., 100., 100.));
        let near = Outline::Rect(rect(105., 40., 20., 20.));
        assert!(within(&disc, &near, 10.));
        assert!(!within(&disc, &near, 4.));
    }

    /// Classification: inside, on the boundary within the tolerance, and outside, for a disc and a box.
    #[test]
    fn a_point_is_classified_against_the_outline() {
        let disc = Outline::of(NodeShape::Circle, &rect(0., 0., 100., 100.));
        assert_eq!(disc.classify(50., 50., 1e-6), Position::Inside);
        assert_eq!(disc.classify(100., 50., 1e-6), Position::Boundary);
        assert_eq!(
            disc.classify(95., 95., 1e-6),
            Position::Outside,
            "a box corner is outside the disc"
        );
        let box_ = Outline::Rect(rect(0., 0., 100., 100.));
        assert_eq!(box_.classify(95., 95., 1e-6), Position::Inside);
        assert_eq!(box_.classify(0., 50., 1e-6), Position::Boundary);
        assert_eq!(box_.classify(120., 50., 1e-6), Position::Outside);
    }

    /// The attachment toward a target lies on the outline, on the ray from the centre; a disc and a box both.
    #[test]
    fn an_attachment_lies_on_the_outline_toward_the_target() {
        let disc = Outline::of(NodeShape::Circle, &rect(0., 0., 100., 100.));
        let (x, y) = disc.attachment_toward(300., 50.).unwrap();
        assert!((x - 100.).abs() < 1e-9 && (y - 50.).abs() < 1e-9);
        let box_ = Outline::Rect(rect(0., 0., 100., 50.));
        // Centre (50, 25); the ray to (300, 75) leaves the box through its right side at (100, 35).
        let (x, y) = box_.attachment_toward(300., 75.).unwrap();
        assert!(
            (x - 100.).abs() < 1e-9 && (y - 35.).abs() < 1e-9,
            "{x}, {y}"
        );
        assert!(
            (x - 100.).abs() < 1e-9 || (y - 50.).abs() < 1e-9,
            "on the boundary: {x}, {y}"
        );
        assert!(box_.attachment_toward(50., 25.).is_none());
    }

    /// A rectangle reaches beyond a disc at its farthest corner, a disc at its far side; inside is zero.
    #[test]
    fn the_overshoot_beyond_a_disc_is_the_reach_of_the_farthest_corner() {
        let box_ = Outline::Rect(rect(40., 40., 20., 20.));
        assert_eq!(box_.overshoot_beyond_disc(50., 50., 50.), 0.);
        // The corner (100, 100) of this box is 50 * sqrt(2) from the centre of a disc of radius 50.
        let corner = Outline::Rect(rect(80., 80., 20., 20.));
        let expected = 50. * 2f64.sqrt() - 50.;
        assert!((corner.overshoot_beyond_disc(50., 50., 50.) - expected).abs() < 1e-9);
        let disc = Outline::Disc {
            x: 80.,
            y: 50.,
            radius: 30.,
        };
        assert!((disc.overshoot_beyond_disc(50., 50., 50.) - 10.).abs() < 1e-9);
    }
}
