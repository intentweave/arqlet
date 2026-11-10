//! Rotation systems: the cyclic order of neighbours around each vertex, derived from faces, and the same order read
//! from a drawing. A drawing realises an embedding when the two agree at every vertex, up to one global orientation.
//!
//! Exact arithmetic throughout: directions are integer vectors and angles are compared by half-plane and cross product.
use crate::TopologyError;

type P = (i64, i64);

/// Cyclic neighbour order of each vertex implied by triangular faces with consistent orientation: for a face
/// `(.., p, v, q, ..)` the neighbour after `p` around `v` is `q`. Fails if the faces do not form one cycle per vertex.
pub fn rotation_from_faces(
    n: usize,
    faces: &[Vec<usize>],
) -> Result<Vec<Vec<usize>>, TopologyError> {
    let mut succ: Vec<std::collections::BTreeMap<usize, usize>> = vec![Default::default(); n];
    for face in faces {
        let k = face.len();
        for i in 0..k {
            let prev = face[(i + k - 1) % k];
            let v = face[i];
            let next = face[(i + 1) % k];
            if succ[v].insert(prev, next).is_some() {
                return Err(TopologyError::Internal(format!(
                    "vertex {v} has two successors for {prev}"
                )));
            }
        }
    }
    let mut rotation = Vec::with_capacity(n);
    for (v, around) in succ.iter().enumerate() {
        let Some((&start, _)) = around.iter().next() else {
            rotation.push(Vec::new());
            continue;
        };
        let mut cycle = vec![start];
        let mut cur = around[&start];
        while cur != start {
            if cycle.len() > around.len() {
                return Err(TopologyError::Internal(format!(
                    "rotation at {v} does not close"
                )));
            }
            cycle.push(cur);
            cur = *around
                .get(&cur)
                .ok_or_else(|| TopologyError::Internal(format!("rotation at {v} is broken")))?;
        }
        if cycle.len() != around.len() {
            return Err(TopologyError::Internal(format!(
                "rotation at {v} has several cycles"
            )));
        }
        rotation.push(cycle);
    }
    Ok(rotation)
}

/// The neighbours of `v` (from `adj[v]`) in counterclockwise order around `v`, by exact angle.
pub fn angular_order(v: usize, adj: &[usize], positions: &[P]) -> Vec<usize> {
    let origin = positions[v];
    let mut neighbours: Vec<(usize, (i64, i64))> = adj
        .iter()
        .map(|&u| (u, (positions[u].0 - origin.0, positions[u].1 - origin.1)))
        .collect();
    neighbours.sort_by(|a, b| {
        let half = |d: (i64, i64)| {
            if d.1 > 0 || (d.1 == 0 && d.0 > 0) {
                0
            } else {
                1
            }
        };
        half(a.1).cmp(&half(b.1)).then_with(|| {
            let cross = a.1 .0 as i128 * b.1 .1 as i128 - a.1 .1 as i128 * b.1 .0 as i128;
            0.cmp(&cross)
        })
    });
    neighbours.into_iter().map(|(u, _)| u).collect()
}

/// Whether `a` and `b` are the same cyclic sequence (rotation allowed, reversal not).
fn same_cycle(a: &[usize], b: &[usize]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    if a.is_empty() {
        return true;
    }
    let shift = b.iter().position(|&x| x == a[0]);
    match shift {
        None => false,
        Some(s) => (0..a.len()).all(|i| a[i] == b[(s + i) % b.len()]),
    }
}

/// Checks that the drawing realises `rotation` at every vertex with one global orientation. Returns `Ok(true)` when the
/// drawing is counterclockwise relative to the rotation, `Ok(false)` when clockwise, and an error naming the first vertex
/// where the orders disagree.
pub fn realises(
    rotation: &[Vec<usize>],
    adj: &[Vec<usize>],
    positions: &[P],
) -> Result<bool, String> {
    let mut orientation: Option<bool> = None;
    for v in 0..rotation.len() {
        if rotation[v].len() < 2 {
            continue;
        }
        let drawn = angular_order(v, &adj[v], positions);
        let mut reversed = rotation[v].clone();
        reversed.reverse();
        let this = if same_cycle(&rotation[v], &drawn) {
            true
        } else if same_cycle(&reversed, &drawn) {
            false
        } else {
            return Err(format!(
                "vertex {v}: drawn order {drawn:?} differs from rotation {:?}",
                rotation[v]
            ));
        };
        match orientation {
            None => orientation = Some(this),
            Some(o) if o != this => {
                return Err(format!(
                    "vertex {v} is mirrored relative to earlier vertices"
                ))
            }
            Some(_) => {}
        }
    }
    Ok(orientation.unwrap_or(true))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_of_a_closed_triangulation_is_read_from_faces() {
        // The faces of K4 (a sphere): every vertex has three neighbours in one cycle.
        let faces = vec![vec![2, 1, 3], vec![2, 0, 1], vec![1, 0, 3], vec![3, 0, 2]];
        let rot = rotation_from_faces(4, &faces).unwrap();
        for (v, cycle) in rot.iter().enumerate() {
            let mut sorted = cycle.clone();
            sorted.sort_unstable();
            let expected: Vec<usize> = (0..4).filter(|&u| u != v).collect();
            assert_eq!(sorted, expected, "vertex {v}");
        }
    }

    #[test]
    fn open_fans_are_rejected_as_a_rotation() {
        // Two triangles sharing an edge: vertex 0 is on the boundary, so its neighbours do not form a closed cycle.
        let faces = vec![vec![0, 1, 2], vec![1, 0, 3]];
        assert!(rotation_from_faces(4, &faces).is_err());
    }

    #[test]
    fn angular_order_is_counterclockwise_from_the_east() {
        let positions = [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1), (1, -1)];
        let order = angular_order(0, &[1, 2, 3, 4, 5], &positions);
        assert_eq!(order, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn detects_a_mismatch_and_reports_orientation() {
        // Vertex 0 at the origin with neighbours east, north, west, south.
        let positions = [(0, 0), (1, 0), (0, 1), (-1, 0), (0, -1)];
        let adj = vec![vec![1, 2, 3, 4], vec![0], vec![0], vec![0], vec![0]];
        let ccw = vec![vec![1, 2, 3, 4], vec![0], vec![0], vec![0], vec![0]];
        assert_eq!(realises(&ccw, &adj, &positions), Ok(true));
        let cw = vec![vec![1, 4, 3, 2], vec![0], vec![0], vec![0], vec![0]];
        assert_eq!(realises(&cw, &adj, &positions), Ok(false));
        let wrong = vec![vec![1, 3, 2, 4], vec![0], vec![0], vec![0], vec![0]];
        assert!(realises(&wrong, &adj, &positions).is_err());
    }
}
