//! Provisional (ADR 0022, first slice): the hull of a membership group, and the two numbers that judge it.
//!
//! A hull is drawn behind the nodes and is **invisible to the router**: it is derived from boxes the layout has already
//! produced, it is never an obstacle, and it never makes a drawing invalid. Nothing here moves a node, so a drawing with
//! hulls has the same node boxes and routes as the same drawing without them.
//!
//! The first slice uses a padded **convex** hull for every area. ADR 0022 §3 decides a rectilinear outline for layered
//! areas; that is a measured refinement of this step, and until then the drawing shows diagonal edges against orthogonal
//! geometry. The geometry here is comparisons, additions and multiplications only — no trigonometry, so no
//! `PortableMath` call and no cross-platform risk (`CLAUDE.md`).
use arqlet_model::{Point, Rect};

/// Orders two points by x, then y, with IEEE 754's total order (`total_cmp`). Scene coordinates are finite; a NaN would
/// still not break the sort: a positive NaN sorts after every number, a negative one before.
fn by_xy(a: &Point, b: &Point) -> std::cmp::Ordering {
    a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y))
}

/// Twice the signed area of the triangle `o, a, b`: positive when `o → a → b` turns counter-clockwise.
fn cross(o: &Point, a: &Point, b: &Point) -> f64 {
    (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x)
}

/// The convex hull of `points`, counter-clockwise, without repeating the first point (Andrew's monotone chain). Fewer
/// than three distinct points give those points as they are, so a group of one or two members still has an outline to
/// draw.
pub fn convex_hull(points: &[Point]) -> Vec<Point> {
    let mut sorted: Vec<Point> = points.to_vec();
    sorted.sort_by(by_xy);
    sorted.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if sorted.len() < 3 {
        return sorted;
    }
    let mut hull: Vec<Point> = Vec::with_capacity(sorted.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &Point>> = if pass == 0 {
            Box::new(sorted.iter())
        } else {
            Box::new(sorted.iter().rev())
        };
        for p in iter {
            while hull.len() >= start + 2
                && cross(&hull[hull.len() - 2], &hull[hull.len() - 1], p) <= 0.
            {
                hull.pop();
            }
            hull.push(p.clone());
        }
        hull.pop();
    }
    hull
}

/// The corners of `rect` grown by `padding` on every side.
fn padded_corners(rect: &Rect, padding: f64) -> [Point; 4] {
    let (x0, y0) = (rect.x - padding, rect.y - padding);
    let (x1, y1) = (
        rect.x + rect.width + padding,
        rect.y + rect.height + padding,
    );
    [
        Point { x: x0, y: y0 },
        Point { x: x1, y: y0 },
        Point { x: x1, y: y1 },
        Point { x: x0, y: y1 },
    ]
}

/// The hull of a group: the convex hull of its members' boxes grown by `padding`. Empty when the group has no box in the
/// view, so a group whose members are all hidden is not drawn (ADR 0022 §3).
pub fn group_hull(boxes: &[Rect], padding: f64) -> Vec<Point> {
    let corners: Vec<Point> = boxes
        .iter()
        .flat_map(|r| padded_corners(r, padding))
        .collect();
    if corners.is_empty() {
        return Vec::new();
    }
    convex_hull(&corners)
}

/// Whether `point` lies inside the polygon `hull` (ray casting; a point on the border counts as inside). A hull of fewer
/// than three points encloses nothing.
pub fn contains(hull: &[Point], point: &Point) -> bool {
    if hull.len() < 3 {
        return false;
    }
    let mut inside = false;
    for i in 0..hull.len() {
        let a = &hull[i];
        let b = &hull[(i + 1) % hull.len()];
        // On the segment: inside, and decided here so that a shared border is not a coin toss.
        if cross(a, b, point) == 0.
            && point.x >= a.x.min(b.x)
            && point.x <= a.x.max(b.x)
            && point.y >= a.y.min(b.y)
            && point.y <= a.y.max(b.y)
        {
            return true;
        }
        if (a.y > point.y) != (b.y > point.y) {
            let t = (point.y - a.y) / (b.y - a.y);
            if point.x < a.x + t * (b.x - a.x) {
                inside = !inside;
            }
        }
    }
    inside
}

/// The centre of `rect`, which is the point that decides whether a box counts as an intruder.
pub fn centre(rect: &Rect) -> Point {
    Point {
        x: rect.x + rect.width / 2.,
        y: rect.y + rect.height / 2.,
    }
}

/// Whether two convex hulls overlap (the separating-axis test: they are apart when some edge of either one separates
/// them). Touching along a border is not an overlap.
pub fn overlap(a: &[Point], b: &[Point]) -> bool {
    if a.len() < 3 || b.len() < 3 {
        return false;
    }
    !separated(a, b) && !separated(b, a)
}

/// Whether some edge of `hull` has all of `other` strictly outside it.
fn separated(hull: &[Point], other: &[Point]) -> bool {
    for i in 0..hull.len() {
        let p = &hull[i];
        let q = &hull[(i + 1) % hull.len()];
        // The hull is counter-clockwise, so a point outside this edge has a non-positive cross product.
        if other.iter().all(|o| cross(p, q, o) < 0.) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64) -> Rect {
        Rect {
            x,
            y,
            width: 10.,
            height: 10.,
        }
    }

    #[test]
    fn the_hull_of_one_box_is_that_box_grown_by_the_padding() {
        let hull = group_hull(&[rect(0., 0.)], 4.);
        assert_eq!(hull.len(), 4, "a rectangle has four corners: {hull:?}");
        let xs: Vec<f64> = hull.iter().map(|p| p.x).collect();
        let ys: Vec<f64> = hull.iter().map(|p| p.y).collect();
        assert_eq!(xs.iter().cloned().fold(f64::MAX, f64::min), -4.);
        assert_eq!(xs.iter().cloned().fold(f64::MIN, f64::max), 14.);
        assert_eq!(ys.iter().cloned().fold(f64::MAX, f64::min), -4.);
        assert_eq!(ys.iter().cloned().fold(f64::MIN, f64::max), 14.);
    }

    #[test]
    fn the_hull_turns_counter_clockwise_and_drops_interior_points() {
        // Four boxes at the corners of a square, plus one in the middle whose corners must not appear on the hull.
        let boxes = [
            rect(0., 0.),
            rect(100., 0.),
            rect(100., 100.),
            rect(0., 100.),
            rect(50., 50.),
        ];
        let hull = group_hull(&boxes, 0.);
        assert_eq!(hull.len(), 4, "the middle box is inside: {hull:?}");
        let mut area = 0.;
        for i in 0..hull.len() {
            let a = &hull[i];
            let b = &hull[(i + 1) % hull.len()];
            area += a.x * b.y - b.x * a.y;
        }
        assert!(area > 0., "counter-clockwise means a positive area");
    }

    #[test]
    fn a_box_between_the_members_is_inside_the_hull() {
        let hull = group_hull(&[rect(0., 0.), rect(100., 0.)], 4.);
        assert!(contains(&hull, &centre(&rect(50., 0.))), "between them");
        assert!(!contains(&hull, &centre(&rect(50., 100.))), "far below");
    }

    #[test]
    fn hulls_overlap_only_when_they_really_do() {
        let left = group_hull(&[rect(0., 0.), rect(40., 0.)], 4.);
        let right = group_hull(&[rect(30., 0.), rect(70., 0.)], 4.);
        let far = group_hull(&[rect(300., 300.), rect(340., 300.)], 4.);
        assert!(overlap(&left, &right), "they share the span 30..50");
        assert!(!overlap(&left, &far));
        assert!(!overlap(&far, &left), "the test is symmetric");
    }

    #[test]
    fn a_group_of_one_or_two_members_still_has_an_outline() {
        assert_eq!(group_hull(&[rect(0., 0.)], 2.).len(), 4);
        let two = group_hull(&[rect(0., 0.), rect(0., 40.)], 2.);
        assert!(two.len() >= 4, "a column of two boxes: {two:?}");
        assert!(group_hull(&[], 2.).is_empty(), "nothing to draw");
    }

    #[test]
    fn the_hull_does_not_depend_on_the_order_of_the_boxes() {
        let a = group_hull(&[rect(0., 0.), rect(50., 20.), rect(20., 60.)], 3.);
        let b = group_hull(&[rect(20., 60.), rect(0., 0.), rect(50., 20.)], 3.);
        assert_eq!(a.len(), b.len());
        assert!(
            a.iter()
                .zip(b.iter())
                .all(|(p, q)| p.x == q.x && p.y == q.y),
            "the same boxes must give the same hull, whatever their order"
        );
    }
}
