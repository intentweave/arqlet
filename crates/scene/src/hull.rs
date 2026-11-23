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

/// The rectangle of a group: the bounding box of its members' boxes grown by `padding`, as a counter-clockwise polygon of
/// four points, so every query here (containment, overlap) treats it like any other hull. Empty when the group has no box.
pub fn group_rectangle(boxes: &[Rect], padding: f64) -> Vec<Point> {
    let Some(first) = boxes.first() else {
        return Vec::new();
    };
    let (mut x0, mut y0) = (first.x, first.y);
    let (mut x1, mut y1) = (first.x + first.width, first.y + first.height);
    for r in &boxes[1..] {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x + r.width);
        y1 = y1.max(r.y + r.height);
    }
    let bounds = Rect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    };
    // Through the convex hull, so the orientation and the order of the corners are those of every other hull.
    convex_hull(&padded_corners(&bounds, padding))
}

/// The hull of a group in the given shape.
pub fn group_shape(boxes: &[Rect], padding: f64, shape: arqlet_model::HullShape) -> Vec<Point> {
    match shape {
        arqlet_model::HullShape::Rectangle => group_rectangle(boxes, padding),
        arqlet_model::HullShape::Polygon => group_hull(boxes, padding),
    }
}

/// One member of a group as the splitting sees it: its box and the container that holds the box in the view.
pub struct Member<'a> {
    pub rect: Rect,
    pub owner: Option<&'a str>,
}

/// How many members a piece may have for the splitting by distance to be tried; a larger piece that is not clean falls
/// apart into its members (and so, as a rule, into marks). Bounds the work: the merging is cubic in this number.
pub const SPLIT_BY_DISTANCE_UP_TO: usize = 64;

/// Provisional (ADR 0022 §3, M2b): the pieces a group is drawn in, as lists of member indices, so that no piece's hull
/// encloses a foreign point (the centre of a leaf box that is not a member). Drawing only: the members stay where the
/// layout put them. In order:
///
/// 1. the whole group, if its hull is clean;
/// 2. else one piece per owner (the container that holds the member's box), in the order the owners first appear;
/// 3. an owner's piece that is still not clean is split by distance: its members start apart, and the two closest pieces
///    whose joint hull is clean are joined, again and again, until no join is clean.
///
/// Deterministic: ties are broken by index, and the result depends only on the members' order and boxes.
pub fn pieces(
    members: &[Member],
    foreign: &[Point],
    padding: f64,
    shape: arqlet_model::HullShape,
) -> Vec<Vec<usize>> {
    let clean = |piece: &[usize]| -> bool {
        let rects: Vec<Rect> = piece.iter().map(|&i| members[i].rect.clone()).collect();
        let hull = group_shape(&rects, padding, shape);
        !foreign.iter().any(|p| contains(&hull, p))
    };
    let all: Vec<usize> = (0..members.len()).collect();
    if all.is_empty() || clean(&all) {
        return if all.is_empty() {
            Vec::new()
        } else {
            vec![all]
        };
    }
    let mut owners: Vec<Option<&str>> = Vec::new();
    for m in members {
        if !owners.contains(&m.owner) {
            owners.push(m.owner);
        }
    }
    let mut out = Vec::new();
    for owner in owners {
        let piece: Vec<usize> = (0..members.len())
            .filter(|&i| members[i].owner == owner)
            .collect();
        if clean(&piece) {
            out.push(piece);
        } else if piece.len() > SPLIT_BY_DISTANCE_UP_TO {
            out.extend(piece.into_iter().map(|i| vec![i]));
        } else {
            out.extend(split_by_distance(members, &piece, &clean));
        }
    }
    out
}

/// The bounding box of some members.
fn bounds_of(members: &[Member], piece: &[usize]) -> (f64, f64, f64, f64) {
    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &i in piece {
        let r = &members[i].rect;
        b = (
            b.0.min(r.x),
            b.1.min(r.y),
            b.2.max(r.x + r.width),
            b.3.max(r.y + r.height),
        );
    }
    b
}

/// The squared gap between two bounding boxes (0 when they touch or overlap). Squared, so no square root is needed to
/// compare: comparison geometry only.
fn gap(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> f64 {
    let dx = (b.0 - a.2).max(a.0 - b.2).max(0.);
    let dy = (b.1 - a.3).max(a.1 - b.3).max(0.);
    dx * dx + dy * dy
}

/// Stage 3 of `pieces`: join the closest pieces as long as the joint hull stays clean.
fn split_by_distance(
    members: &[Member],
    piece: &[usize],
    clean: &dyn Fn(&[usize]) -> bool,
) -> Vec<Vec<usize>> {
    let mut parts: Vec<Vec<usize>> = piece.iter().map(|&i| vec![i]).collect();
    loop {
        let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
        for i in 0..parts.len() {
            for j in (i + 1)..parts.len() {
                let d = gap(bounds_of(members, &parts[i]), bounds_of(members, &parts[j]));
                pairs.push((d, i, j));
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
        let joined = pairs.into_iter().find_map(|(_, i, j)| {
            let mut joint = parts[i].clone();
            joint.extend(parts[j].iter().copied());
            joint.sort_unstable();
            clean(&joint).then_some((i, j, joint))
        });
        match joined {
            Some((i, j, joint)) => {
                parts[i] = joint;
                parts.remove(j);
            }
            None => return parts,
        }
    }
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
    fn the_rectangle_is_the_padded_bounding_box_and_turns_like_every_hull() {
        let boxes = [rect(0., 0.), rect(100., 30.), rect(40., 80.)];
        let r = group_rectangle(&boxes, 5.);
        assert_eq!(r.len(), 4, "a rectangle has four corners: {r:?}");
        let xs: Vec<f64> = r.iter().map(|p| p.x).collect();
        let ys: Vec<f64> = r.iter().map(|p| p.y).collect();
        assert_eq!(xs.iter().cloned().fold(f64::MAX, f64::min), -5.);
        assert_eq!(xs.iter().cloned().fold(f64::MIN, f64::max), 115.);
        assert_eq!(ys.iter().cloned().fold(f64::MAX, f64::min), -5.);
        assert_eq!(ys.iter().cloned().fold(f64::MIN, f64::max), 95.);
        // Every edge is axis-parallel.
        for i in 0..4 {
            let (a, b) = (&r[i], &r[(i + 1) % 4]);
            assert!(
                a.x == b.x || a.y == b.y,
                "edge {a:?} → {b:?} is not axis-parallel"
            );
        }
        // The same orientation as the convex hull, so containment and overlap work on it unchanged.
        let polygon = group_hull(&boxes, 5.);
        assert!(contains(&r, &centre(&rect(40., 80.))));
        assert!(overlap(&r, &polygon));
        // The rectangle encloses the convex hull: it is never the tighter of the two.
        assert!(polygon.iter().all(|p| contains(&r, p)));
        assert!(group_rectangle(&[], 5.).is_empty());
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

    fn member(x: f64, y: f64, owner: Option<&str>) -> Member<'_> {
        Member {
            rect: rect(x, y),
            owner,
        }
    }

    const R: arqlet_model::HullShape = arqlet_model::HullShape::Rectangle;

    #[test]
    fn a_clean_group_is_one_piece() {
        let members = [member(0., 0., Some("a")), member(40., 0., Some("b"))];
        let far = [Point { x: 500., y: 500. }];
        assert_eq!(pieces(&members, &far, 4., R), [vec![0, 1]]);
        assert!(pieces(&[], &far, 4., R).is_empty());
    }

    #[test]
    fn a_foreign_node_between_owners_splits_the_group_per_owner() {
        // Two members in container a, one in b, and a foreign node between a and b.
        let members = [
            member(0., 0., Some("a")),
            member(0., 30., Some("a")),
            member(200., 0., Some("b")),
        ];
        let between = [Point { x: 100., y: 5. }];
        assert_eq!(pieces(&members, &between, 4., R), [vec![0, 1], vec![2]]);
    }

    #[test]
    fn inside_one_owner_the_closest_clean_members_are_joined() {
        // Four members in one container in a row, a foreign node between the second and the third.
        let members = [
            member(0., 0., Some("a")),
            member(20., 0., Some("a")),
            member(100., 0., Some("a")),
            member(120., 0., Some("a")),
        ];
        let foreign = [Point { x: 60., y: 5. }];
        assert_eq!(pieces(&members, &foreign, 4., R), [vec![0, 1], vec![2, 3]]);
        // The polygon shape splits the same way here: the foreign node lies on the line between the pairs.
        let polygon = pieces(&members, &foreign, 4., arqlet_model::HullShape::Polygon);
        assert_eq!(polygon, [vec![0, 1], vec![2, 3]]);
    }

    #[test]
    fn no_piece_encloses_a_foreign_point_and_every_member_is_in_one() {
        // A scattered group around foreign nodes: whatever the split, the invariants hold.
        let members: Vec<Member> = (0..9)
            .map(|i| member((i % 3) as f64 * 60., (i / 3) as f64 * 60., Some("a")))
            .collect();
        let foreign = [Point { x: 35., y: 35. }, Point { x: 95., y: 95. }];
        let parts = pieces(&members, &foreign, 4., R);
        let mut seen: Vec<usize> = parts.iter().flatten().copied().collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..9).collect::<Vec<_>>(), "each member exactly once");
        for part in &parts {
            let rects: Vec<Rect> = part.iter().map(|&i| members[i].rect.clone()).collect();
            let hull = group_rectangle(&rects, 4.);
            assert!(
                !foreign.iter().any(|p| contains(&hull, p)),
                "{part:?} encloses a foreign point"
            );
        }
        // Deterministic: the same input gives the same pieces.
        assert_eq!(parts, pieces(&members, &foreign, 4., R));
    }
}
