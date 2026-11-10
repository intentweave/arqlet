//! Triangulation of an embedding. Each face longer than three is triangulated by diagonals from one of its vertices
//! (a fan) whenever no diagonal would duplicate an existing edge. A face where every fan is blocked receives one new
//! dummy vertex joined to all its vertices instead. Diagonals and dummy edges are auxiliary: the caller draws the
//! original graph and never outputs them.
use crate::planarity::Embedding;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Triangulation {
    /// Original vertices plus dummies.
    pub n: usize,
    /// Triangular faces.
    pub faces: Vec<[usize; 3]>,
    /// Adjacency of the triangulated graph (sorted), including auxiliary edges.
    pub adj: Vec<Vec<usize>>,
    /// Number of dummy vertices (`n - original vertex count`).
    pub dummies: usize,
    /// Number of diagonals added inside faces.
    pub diagonals: usize,
}

/// Triangulates the faces of `embedding`, which has `original` vertices.
pub fn triangulate(original: usize, embedding: &Embedding) -> Triangulation {
    let mut edges: BTreeSet<(usize, usize)> = BTreeSet::new();
    for face in &embedding.faces {
        for i in 0..face.len() {
            edges.insert(key(face[i], face[(i + 1) % face.len()]));
        }
    }
    let mut faces = Vec::new();
    let mut next = original;
    let mut diagonals = 0;
    for face in &embedding.faces {
        if face.len() == 3 {
            faces.push([face[0], face[1], face[2]]);
            continue;
        }
        match fan(face, &edges) {
            Some((centre, chords)) => {
                for &(a, b) in &chords {
                    edges.insert(key(a, b));
                }
                diagonals += chords.len();
                let k = face.len();
                for i in 0..k {
                    let (a, b) = (face[i], face[(i + 1) % k]);
                    if a != centre && b != centre {
                        faces.push([a, b, centre]);
                    }
                }
            }
            None => {
                let dummy = next;
                next += 1;
                for i in 0..face.len() {
                    let (a, b) = (face[i], face[(i + 1) % face.len()]);
                    edges.insert(key(a, dummy));
                    faces.push([a, b, dummy]);
                }
            }
        }
    }
    let mut adj = vec![Vec::new(); next];
    for (a, b) in edges {
        adj[a].push(b);
        adj[b].push(a);
    }
    for list in &mut adj {
        list.sort_unstable();
    }
    Triangulation {
        n: next,
        faces,
        adj,
        dummies: next - original,
        diagonals,
    }
}

fn key(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

/// Diagonals of a fan from one vertex of `face` that duplicate no existing edge, trying the vertices in order. `None`
/// when every fan is blocked. Diagonals of a fan from a vertex of a simple cycle lie inside the face.
fn fan(face: &[usize], edges: &BTreeSet<(usize, usize)>) -> Option<(usize, Vec<(usize, usize)>)> {
    let k = face.len();
    (0..k).find_map(|c| {
        let centre = face[c];
        let mut chords = Vec::new();
        for offset in 2..k - 1 {
            let k2 = key(centre, face[(c + offset) % k]);
            if edges.contains(&k2) {
                return None;
            }
            chords.push(k2);
        }
        Some((centre, chords))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::planarity::{embed, Planarity};

    #[test]
    fn a_square_gets_two_triangles_and_no_dummy() {
        let embedding = Embedding {
            faces: vec![vec![0, 1, 2, 3], vec![3, 2, 1, 0]],
        };
        let t = triangulate(4, &embedding);
        assert_eq!(t.dummies, 0);
        assert_eq!(t.diagonals, 2, "each of the two faces takes one diagonal");
        let edges: usize = t.adj.iter().map(Vec::len).sum::<usize>() / 2;
        assert_eq!(edges, 3 * t.n - 6, "a triangulation has 3V - 6 edges");
    }

    #[test]
    fn triangulated_input_is_unchanged() {
        let (g, _) =
            Graph::from_edges(4, &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]).unwrap();
        let Planarity::Planar(e) = embed(&g).unwrap() else {
            panic!("planar");
        };
        let t = triangulate(4, &e);
        assert_eq!((t.n, t.faces.len(), t.diagonals), (4, 4, 0));
    }

    #[test]
    fn blocked_fans_fall_back_to_a_dummy() {
        // A hexagon whose every fan diagonal already exists as an edge outside the face.
        let hexagon = vec![0, 1, 2, 3, 4, 5];
        let mut edges = BTreeSet::new();
        for i in 0..6 {
            edges.insert(key(hexagon[i], hexagon[(i + 1) % 6]));
        }
        for (a, b) in [
            (0, 2),
            (0, 3),
            (0, 4),
            (1, 3),
            (1, 4),
            (1, 5),
            (2, 4),
            (2, 5),
            (3, 5),
        ] {
            edges.insert(key(a, b));
        }
        assert!(fan(&hexagon, &edges).is_none());
        assert!(fan(&hexagon, &BTreeSet::new()).is_some());
    }
}
