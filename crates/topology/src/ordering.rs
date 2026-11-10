//! Canonical ordering of a triangulated plane graph (de Fraysseix, Pach, Pollack).
//!
//! The outer face is a triangle `(v1, v2, v3)`; the contour runs from `v1` over the top to `v2`. Vertices are removed
//! from the contour one at a time: a vertex that is not `v1` or `v2` and has no chord to the contour is removed,
//! and its interior neighbours, in order from left to right, become the new contour piece. Reversing the removal
//! order gives the canonical ordering. Each entry carries the contour neighbours it had when it was added, which
//! drive the grid construction.
use crate::triangulate::Triangulation;
use crate::TopologyError;
use std::collections::BTreeMap;

/// `(vertex, contour_neighbours)` for every vertex in insertion order. The first three entries are `v1, v2, v3` and
/// carry no contour neighbours; for `k >= 4` the neighbours are ordered from left to right on the contour.
pub type CanonicalOrder = Vec<(usize, Vec<usize>)>;

pub fn canonical_order(t: &Triangulation) -> Result<CanonicalOrder, TopologyError> {
    if t.faces.is_empty() {
        return Err(TopologyError::Internal("no faces to order".into()));
    }
    let total = t.n;
    let mut faces_of = vec![Vec::new(); total];
    for (f, face) in t.faces.iter().enumerate() {
        for &v in face {
            faces_of[v].push(f);
        }
    }
    let outer = t.faces[0];
    let (v1, v2, v3) = (outer[0], outer[1], outer[2]);
    let mut alive = vec![true; total];
    let mut on_contour = vec![false; total];
    let mut contour = vec![v1, v3, v2];
    for &v in &contour {
        on_contour[v] = true;
    }
    let mut alive_count = total;
    let mut removed: Vec<(usize, Vec<usize>)> = Vec::new();

    while alive_count > 3 {
        let position = (1..contour.len() - 1).find(|&i| {
            let x = contour[i];
            let (left, right) = (contour[i - 1], contour[i + 1]);
            !t.adj[x]
                .iter()
                .any(|&u| alive[u] && on_contour[u] && u != left && u != right)
        });
        let i = position.ok_or_else(|| {
            TopologyError::Internal("no chordless contour vertex; not a triangulation".into())
        })?;
        let x = contour[i];
        let path = link_path(t, &faces_of, &alive, x, contour[i - 1], contour[i + 1])?;
        for &u in &path[1..path.len() - 1] {
            on_contour[u] = true;
        }
        on_contour[x] = false;
        alive[x] = false;
        alive_count -= 1;
        contour.splice(i..=i, path[1..path.len() - 1].iter().copied());
        removed.push((x, path));
    }

    if contour.len() != 3 {
        return Err(TopologyError::Internal(
            "contour did not reduce to a triangle".into(),
        ));
    }
    // The contour runs from v1 to v2 over the top vertex, so the base pair is (v1, v2) and the top vertex is third.
    let mut order: CanonicalOrder =
        vec![(v1, Vec::new()), (v2, Vec::new()), (contour[1], Vec::new())];
    for (x, path) in removed.into_iter().rev() {
        order.push((x, path));
    }
    if order.len() != total {
        return Err(TopologyError::Internal("ordering misses vertices".into()));
    }
    Ok(order)
}

/// The interior neighbours of `x` from `left` to `right`, following the triangles of the current graph that contain
/// `x` (the outer face is excluded). Both ends are included.
fn link_path(
    t: &Triangulation,
    faces_of: &[Vec<usize>],
    alive: &[bool],
    x: usize,
    left: usize,
    right: usize,
) -> Result<Vec<usize>, TopologyError> {
    let mut link: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &f in &faces_of[x] {
        if f == 0 {
            continue;
        }
        let face = t.faces[f];
        if !face.iter().all(|&v| alive[v]) {
            continue;
        }
        let others: Vec<usize> = face.iter().copied().filter(|&v| v != x).collect();
        link.entry(others[0]).or_default().push(others[1]);
        link.entry(others[1]).or_default().push(others[0]);
    }
    let mut path = vec![left];
    let mut prev = usize::MAX;
    let mut cur = left;
    while cur != right {
        let next = link
            .get(&cur)
            .and_then(|ns| ns.iter().copied().find(|&y| y != prev))
            .ok_or_else(|| TopologyError::Internal("broken link path".into()))?;
        if path.len() > t.n {
            return Err(TopologyError::Internal(
                "link path does not terminate".into(),
            ));
        }
        path.push(next);
        prev = cur;
        cur = next;
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::planarity::{embed, Planarity};
    use crate::triangulate::triangulate;

    fn order_of(n: usize, edges: &[(usize, usize)]) -> (Triangulation, CanonicalOrder) {
        let (g, _) = Graph::from_edges(n, edges).unwrap();
        let Planarity::Planar(e) = embed(&g).unwrap() else {
            panic!("planar");
        };
        let t = triangulate(n, &e);
        let order = canonical_order(&t).unwrap();
        (t, order)
    }

    #[test]
    fn every_vertex_appears_once_and_contours_have_two_or_more_entries() {
        let (t, order) = order_of(4, &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]);
        let mut seen: Vec<usize> = order.iter().map(|e| e.0).collect();
        seen.sort_unstable();
        assert_eq!(seen, (0..t.n).collect::<Vec<_>>());
        for (_, contour) in order.iter().skip(3) {
            assert!(contour.len() >= 2, "{contour:?}");
        }
    }

    #[test]
    fn contour_neighbours_are_adjacent_to_the_vertex() {
        let (t, order) = order_of(6, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0), (0, 3)]);
        for (x, contour) in order.iter().skip(3) {
            for u in contour {
                assert!(t.adj[*x].contains(u), "{x} not adjacent to {u}");
            }
        }
    }
}
