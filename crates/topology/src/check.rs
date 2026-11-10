//! Validation of straight-line drawings with exact integer arithmetic, independent of how the drawing was made.
//!
//! A drawing is valid when all vertices have distinct positions, no vertex lies on an edge it is not an endpoint of,
//! no two edges without a common endpoint touch, and two edges with a common endpoint do not overlap.
use crate::graph::Graph;

type P = (i64, i64);

/// Coordinates are limited to this magnitude, so every difference and product below fits in `i128`.
pub const MAX_COORDINATE: i64 = 1 << 30;

/// `(a - o)` as exact `i128` components.
fn diff(a: P, o: P) -> (i128, i128) {
    (a.0 as i128 - o.0 as i128, a.1 as i128 - o.1 as i128)
}

fn cross(o: P, a: P, b: P) -> i128 {
    let (ax, ay) = diff(a, o);
    let (bx, by) = diff(b, o);
    ax * by - ay * bx
}

fn dot(o: P, a: P, b: P) -> i128 {
    let (ax, ay) = diff(a, o);
    let (bx, by) = diff(b, o);
    ax * bx + ay * by
}

fn within(p: P, a: P, b: P) -> bool {
    p.0 >= a.0.min(b.0) && p.0 <= a.0.max(b.0) && p.1 >= a.1.min(b.1) && p.1 <= a.1.max(b.1)
}

/// `p` lies on the closed segment `ab`.
fn on_segment(p: P, a: P, b: P) -> bool {
    cross(a, b, p) == 0 && within(p, a, b)
}

/// The closed segments `ab` and `cd` have a common point.
fn touch(a: P, b: P, c: P, d: P) -> bool {
    let d1 = cross(a, b, c).signum();
    let d2 = cross(a, b, d).signum();
    let d3 = cross(c, d, a).signum();
    let d4 = cross(c, d, b).signum();
    if d1 * d2 < 0 && d3 * d4 < 0 {
        return true;
    }
    on_segment(c, a, b) || on_segment(d, a, b) || on_segment(a, c, d) || on_segment(b, c, d)
}

/// Checks a drawing of `graph`. The error names the first violation found.
pub fn validate(graph: &Graph, positions: &[P]) -> Result<(), String> {
    let n = graph.n();
    if positions.len() != n {
        return Err(format!("{} positions for {n} vertices", positions.len()));
    }
    if let Some((v, p)) = positions
        .iter()
        .enumerate()
        .find(|(_, p)| p.0.abs() > MAX_COORDINATE || p.1.abs() > MAX_COORDINATE)
    {
        return Err(format!("vertex {v} at {p:?} exceeds the coordinate limit"));
    }
    for a in 0..n {
        for b in a + 1..n {
            if positions[a] == positions[b] {
                return Err(format!("vertices {a} and {b} share a position"));
            }
        }
    }
    let edges = graph.edges();
    for &(a, b) in &edges {
        for w in 0..n {
            if w != a && w != b && on_segment(positions[w], positions[a], positions[b]) {
                return Err(format!("vertex {w} lies on edge ({a}, {b})"));
            }
        }
    }
    for (i, &(a, b)) in edges.iter().enumerate() {
        for &(c, d) in &edges[i + 1..] {
            let shared = [a == c, a == d, b == c, b == d]
                .iter()
                .filter(|&&s| s)
                .count();
            let (pa, pb, pc, pd) = (positions[a], positions[b], positions[c], positions[d]);
            match shared {
                0 => {
                    if touch(pa, pb, pc, pd) {
                        return Err(format!("edges ({a}, {b}) and ({c}, {d}) touch"));
                    }
                }
                1 => {
                    // Shared endpoint `s` with other endpoints `p` and `q`: overlap means q lies on the ray from s through p.
                    let (s, p, q) = if a == c {
                        (pa, pb, pd)
                    } else if a == d {
                        (pa, pb, pc)
                    } else if b == c {
                        (pb, pa, pd)
                    } else {
                        (pb, pa, pc)
                    };
                    if cross(s, p, q) == 0 && dot(s, p, q) > 0 {
                        return Err(format!("edges ({a}, {b}) and ({c}, {d}) overlap"));
                    }
                }
                _ => return Err(format!("parallel edges ({a}, {b}) and ({c}, {d})")),
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(n: usize, edges: &[(usize, usize)]) -> Graph {
        Graph::from_edges(n, edges).unwrap().0
    }

    #[test]
    fn accepts_a_square_and_rejects_crossing_diagonals() {
        let square = graph(4, &[(0, 1), (1, 2), (2, 3), (3, 0)]);
        assert!(validate(&square, &[(0, 0), (2, 0), (2, 2), (0, 2)]).is_ok());
        let crossing = graph(4, &[(0, 2), (1, 3)]);
        assert!(validate(&crossing, &[(0, 0), (2, 0), (2, 2), (0, 2)]).is_err());
    }

    #[test]
    fn rejects_a_vertex_on_an_edge_and_overlapping_edges() {
        let path = graph(3, &[(0, 2)]);
        assert!(validate(&path, &[(0, 0), (1, 0), (2, 0)]).is_err());
        let fan = graph(3, &[(0, 1), (0, 2)]);
        assert!(validate(&fan, &[(0, 0), (2, 0), (1, 0)]).is_err());
        assert!(validate(&fan, &[(0, 0), (1, 0), (0, 1)]).is_ok());
    }

    #[test]
    fn rejects_coordinates_beyond_the_exact_range_without_overflowing() {
        let g = graph(3, &[(0, 1), (1, 2), (2, 0)]);
        let huge = i64::MAX / 2;
        assert!(validate(&g, &[(0, 0), (huge, 0), (0, huge)]).is_err());
        assert!(validate(&g, &[(-MAX_COORDINATE, 0), (MAX_COORDINATE, 0), (0, 1)]).is_ok());
    }

    #[test]
    fn rejects_shared_positions() {
        let g = graph(2, &[(0, 1)]);
        assert!(validate(&g, &[(1, 1), (1, 1)]).is_err());
    }
}
