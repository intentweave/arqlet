//! Planar embedding of biconnected graphs by the Demoucron–Malgrange–Pertuiset face-splitting method (1964).
//!
//! Start from a cycle, which splits the plane into two faces. Repeatedly take a fragment of the graph not yet
//! embedded (an unembedded edge between embedded vertices, or a connected group of unembedded vertices with its
//! attachments), choose a face containing all its attachments, and split that face along a path through the fragment.
//! A fragment without an admissible face proves nonplanarity. Quadratic work per step; intended for analysis of
//! moderate graphs, not as a scalable planarity test.
use crate::graph::Graph;
use crate::TopologyError;
use std::collections::BTreeSet;

/// Faces of a planar embedding, each a simple cycle of vertices. Every graph edge lies on exactly two faces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Embedding {
    pub faces: Vec<Vec<usize>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Planarity {
    Planar(Embedding),
    NonPlanar,
}

/// Embeds a biconnected graph, or reports that it is nonplanar.
pub fn embed(graph: &Graph) -> Result<Planarity, TopologyError> {
    if !graph.is_biconnected() {
        return Err(TopologyError::NotBiconnected);
    }
    let n = graph.n();
    let cycle = find_cycle(graph);
    let mut faces = vec![
        cycle.clone(),
        cycle.iter().rev().copied().collect::<Vec<_>>(),
    ];
    let mut inside_v = vec![false; n];
    let mut inside_e: BTreeSet<(usize, usize)> = BTreeSet::new();
    for &v in &cycle {
        inside_v[v] = true;
    }
    for i in 0..cycle.len() {
        inside_e.insert(key(cycle[i], cycle[(i + 1) % cycle.len()]));
    }

    while inside_e.len() < graph.m() {
        let fragments = fragments(graph, &inside_v, &inside_e);
        let mut choice: Option<(&Fragment, usize)> = None;
        let mut best = usize::MAX;
        for fragment in &fragments {
            let admissible: Vec<usize> = (0..faces.len())
                .filter(|&f| fragment.attachments.iter().all(|a| faces[f].contains(a)))
                .collect();
            if admissible.is_empty() {
                return Ok(Planarity::NonPlanar);
            }
            if admissible.len() < best {
                best = admissible.len();
                choice = Some((fragment, admissible[0]));
            }
        }
        let (fragment, face) = choice
            .ok_or_else(|| TopologyError::Internal("no fragment although edges remain".into()))?;
        let path = fragment_path(graph, fragment, &inside_v)?;
        split_face(&mut faces, face, &path)?;
        for pair in path.windows(2) {
            inside_e.insert(key(pair[0], pair[1]));
        }
        for &v in &path {
            inside_v[v] = true;
        }
    }
    Ok(Planarity::Planar(Embedding { faces }))
}

fn key(a: usize, b: usize) -> (usize, usize) {
    (a.min(b), a.max(b))
}

/// A cycle through a non-tree edge of a BFS tree, with tree paths up to their lowest common ancestor.
fn find_cycle(graph: &Graph) -> Vec<usize> {
    let n = graph.n();
    let mut parent = vec![usize::MAX; n];
    let mut depth = vec![0usize; n];
    let mut seen = vec![false; n];
    let mut queue = std::collections::VecDeque::from([0usize]);
    seen[0] = true;
    while let Some(v) = queue.pop_front() {
        for &u in graph.adj(v) {
            if !seen[u] {
                seen[u] = true;
                parent[u] = v;
                depth[u] = depth[v] + 1;
                queue.push_back(u);
            }
        }
    }
    let (u, v) = graph
        .edges()
        .into_iter()
        .find(|&(a, b)| parent[a] != b && parent[b] != a)
        .expect("a biconnected graph with three vertices has a non-tree edge");
    let (mut x, mut y) = (u, v);
    let mut up_x = vec![x];
    let mut up_y = vec![y];
    while x != y {
        if depth[x] >= depth[y] {
            x = parent[x];
            up_x.push(x);
        } else {
            y = parent[y];
            up_y.push(y);
        }
    }
    let mut cycle = up_x;
    cycle.extend(up_y[..up_y.len() - 1].iter().rev());
    cycle
}

#[derive(Debug, Clone)]
struct Fragment {
    /// Embedded vertices the fragment touches, sorted and unique.
    attachments: Vec<usize>,
    /// Unembedded vertices of the fragment; empty for a bridge (an unembedded edge between embedded vertices).
    interior: Vec<usize>,
}

fn fragments(
    graph: &Graph,
    inside_v: &[bool],
    inside_e: &BTreeSet<(usize, usize)>,
) -> Vec<Fragment> {
    let n = graph.n();
    let mut out = Vec::new();
    for (a, b) in graph.edges() {
        if inside_v[a] && inside_v[b] && !inside_e.contains(&(a, b)) {
            out.push(Fragment {
                attachments: vec![a, b],
                interior: Vec::new(),
            });
        }
    }
    let mut seen = vec![false; n];
    for start in 0..n {
        if inside_v[start] || seen[start] {
            continue;
        }
        let mut interior = vec![start];
        seen[start] = true;
        let mut i = 0;
        while i < interior.len() {
            let v = interior[i];
            i += 1;
            for &u in graph.adj(v) {
                if !inside_v[u] && !seen[u] {
                    seen[u] = true;
                    interior.push(u);
                }
            }
        }
        let mut attachments: BTreeSet<usize> = BTreeSet::new();
        for &v in &interior {
            for &u in graph.adj(v) {
                if inside_v[u] {
                    attachments.insert(u);
                }
            }
        }
        out.push(Fragment {
            attachments: attachments.into_iter().collect(),
            interior,
        });
    }
    out
}

/// A path from one attachment to another through the fragment's interior (just the edge for a bridge).
fn fragment_path(
    graph: &Graph,
    fragment: &Fragment,
    inside_v: &[bool],
) -> Result<Vec<usize>, TopologyError> {
    if fragment.interior.is_empty() {
        return Ok(fragment.attachments.clone());
    }
    let n = graph.n();
    let start = fragment.attachments[0];
    let mut in_interior = vec![false; n];
    for &v in &fragment.interior {
        in_interior[v] = true;
    }
    let mut parent = vec![usize::MAX; n];
    let mut queue = std::collections::VecDeque::new();
    for &u in graph.adj(start) {
        if in_interior[u] && parent[u] == usize::MAX {
            parent[u] = start;
            queue.push_back(u);
        }
    }
    while let Some(x) = queue.pop_front() {
        for &y in graph.adj(x) {
            if inside_v[y] {
                if y != start {
                    let mut chain = Vec::new();
                    let mut cur = x;
                    loop {
                        chain.push(cur);
                        if parent[cur] == start {
                            break;
                        }
                        cur = parent[cur];
                    }
                    chain.reverse();
                    let mut path = vec![start];
                    path.extend(chain);
                    path.push(y);
                    return Ok(path);
                }
            } else if in_interior[y] && parent[y] == usize::MAX {
                parent[y] = x;
                queue.push_back(y);
            }
        }
    }
    Err(TopologyError::Internal(
        "fragment of a biconnected graph has a single attachment".into(),
    ))
}

/// Splits face `f` along `path` (first and last vertices lie on the face) into two faces.
fn split_face(faces: &mut Vec<Vec<usize>>, f: usize, path: &[usize]) -> Result<(), TopologyError> {
    let face = faces[f].clone();
    let a = path[0];
    let b = path[path.len() - 1];
    let i = face
        .iter()
        .position(|&v| v == a)
        .ok_or_else(|| TopologyError::Internal("path start not on face".into()))?;
    let j = face
        .iter()
        .position(|&v| v == b)
        .ok_or_else(|| TopologyError::Internal("path end not on face".into()))?;
    let interior = &path[1..path.len() - 1];
    let mut first = cyclic_range(&face, i, j);
    first.extend(interior.iter().rev());
    let mut second = cyclic_range(&face, j, i);
    second.extend(interior.iter());
    faces[f] = first;
    faces.push(second);
    Ok(())
}

/// Vertices of the cycle `face` from position `from` forward to position `to`, both included.
fn cyclic_range(face: &[usize], from: usize, to: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut k = from;
    loop {
        out.push(face[k]);
        if k == to {
            break;
        }
        k = (k + 1) % face.len();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(n: usize, edges: &[(usize, usize)]) -> Graph {
        Graph::from_edges(n, edges).unwrap().0
    }

    /// Euler's formula for a connected plane graph: V - E + F = 2, and every edge lies on exactly two faces.
    fn assert_valid_embedding(g: &Graph, e: &Embedding) {
        assert_eq!(
            g.n() as i64 - g.m() as i64 + e.faces.len() as i64,
            2,
            "Euler"
        );
        let mut count = std::collections::BTreeMap::new();
        for face in &e.faces {
            for i in 0..face.len() {
                *count
                    .entry(key(face[i], face[(i + 1) % face.len()]))
                    .or_insert(0) += 1;
            }
        }
        assert_eq!(count.len(), g.m());
        assert!(count.values().all(|&c| c == 2), "each edge on two faces");
    }

    fn planar_faces(g: &Graph) -> Embedding {
        match embed(g).unwrap() {
            Planarity::Planar(e) => {
                assert_valid_embedding(g, &e);
                e
            }
            Planarity::NonPlanar => panic!("expected a planar graph"),
        }
    }

    #[test]
    fn complete_graph_on_four_vertices_has_four_faces() {
        let k4 = graph(4, &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]);
        assert_eq!(planar_faces(&k4).faces.len(), 4);
    }

    #[test]
    fn wheel_and_cube_are_planar() {
        let wheel = graph(
            6,
            &[
                (0, 1),
                (0, 2),
                (0, 3),
                (0, 4),
                (0, 5),
                (1, 2),
                (2, 3),
                (3, 4),
                (4, 5),
                (5, 1),
            ],
        );
        assert_eq!(planar_faces(&wheel).faces.len(), 6);
        let cube = graph(
            8,
            &[
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0),
                (4, 5),
                (5, 6),
                (6, 7),
                (7, 4),
                (0, 4),
                (1, 5),
                (2, 6),
                (3, 7),
            ],
        );
        assert_eq!(planar_faces(&cube).faces.len(), 6);
    }

    #[test]
    fn k5_and_k33_are_nonplanar() {
        let mut k5 = Vec::new();
        for a in 0..5 {
            for b in a + 1..5 {
                k5.push((a, b));
            }
        }
        assert_eq!(embed(&graph(5, &k5)).unwrap(), Planarity::NonPlanar);
        let mut k33 = Vec::new();
        for a in 0..3 {
            for b in 3..6 {
                k33.push((a, b));
            }
        }
        assert_eq!(embed(&graph(6, &k33)).unwrap(), Planarity::NonPlanar);
    }

    #[test]
    fn chorded_cycle_is_planar() {
        let g = graph(6, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0), (0, 3)]);
        assert_eq!(planar_faces(&g).faces.len(), 3);
    }

    #[test]
    fn rejects_graphs_that_are_not_biconnected() {
        let path = graph(3, &[(0, 1), (1, 2)]);
        assert_eq!(embed(&path), Err(TopologyError::NotBiconnected));
    }
}
