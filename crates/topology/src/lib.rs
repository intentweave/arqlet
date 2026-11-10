//! Planarity analysis and straight-line grid drawings of undirected graphs (A4b, stages 1 and 4).
//!
//! Pure computation: no host APIs, no I/O, no threads, no dependencies. Provisional scaffold: the API may change.
//!
//! Pipeline for a biconnected simple graph:
//! 1. [`planarity::embed`] computes a planar embedding (faces as vertex cycles) or reports nonplanarity, using the
//!    Demoucron–Malgrange–Pertuiset face-splitting method.
//! 2. [`triangulate::triangulate`] makes every face a triangle: diagonals where a fan avoids existing edges, one dummy
//!    vertex per face otherwise.
//! 3. [`ordering::canonical_order`] computes a canonical ordering of the triangulation (de Fraysseix, Pach, Pollack).
//! 4. [`grid::positions`] assigns grid coordinates with the Chrobak–Payne construction; dummy vertices are dropped.
//!
//! [`check::validate`] verifies any drawing with exact integer geometry, independently of the construction.
//!
//! Not yet supported: graphs that are not biconnected (block-wise analysis is the next step), witnesses for
//! nonplanarity, containers, and rectangular nodes with ports. The embedding method is Demoucron–Malgrange–
//! Pertuiset, which is quadratic in the number of edges; it is not a linear-time planarity test.

pub mod check;
#[cfg(test)]
mod corpus;
pub mod graph;
pub mod grid;
pub mod ordering;
pub mod planarity;
pub mod rotation;
pub mod triangulate;

use std::fmt;

/// Errors of the topology pipeline. Nonplanarity is a result, not an error; see [`Outcome`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopologyError {
    /// Fewer than three vertices, or an edge endpoint outside the vertex range.
    InvalidInput(String),
    /// The graph is not connected or has a cut vertex; block-wise handling is not implemented yet.
    NotBiconnected,
    /// The graph has no planar embedding; drawing is impossible.
    NonPlanar,
    /// An internal invariant failed. This indicates a bug, not an input problem.
    Internal(String),
}

impl fmt::Display for TopologyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TopologyError::InvalidInput(message) => write!(f, "invalid input: {message}"),
            TopologyError::NotBiconnected => write!(f, "graph is not biconnected"),
            TopologyError::NonPlanar => write!(f, "graph is nonplanar"),
            TopologyError::Internal(message) => write!(f, "internal error: {message}"),
        }
    }
}

impl std::error::Error for TopologyError {}

/// A straight-line drawing of the original vertices. `positions[v]` is the grid point of vertex `v`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drawing {
    pub positions: Vec<(i64, i64)>,
    /// Largest x and y of the original vertices; the visible extent. Coordinates are nonnegative.
    pub width: i64,
    pub height: i64,
    /// Largest x and y over all triangulated vertices, dummies included: the extent of the construction grid.
    pub grid_width: i64,
    pub grid_height: i64,
    /// Dummy vertices added by triangulation (not drawn).
    pub dummies: usize,
    /// Diagonals added inside faces (not drawn).
    pub diagonals: usize,
}

/// The triangulation of a planar graph and the grid position of every triangulated vertex, dummies included.
pub(crate) fn draw_triangulation(
    graph: &graph::Graph,
) -> Result<(triangulate::Triangulation, Vec<(i64, i64)>), TopologyError> {
    let embedding = match planarity::embed(graph)? {
        planarity::Planarity::Planar(embedding) => embedding,
        planarity::Planarity::NonPlanar => return Err(TopologyError::NonPlanar),
    };
    let triangulation = triangulate::triangulate(graph.n(), &embedding);
    let order = ordering::canonical_order(&triangulation)?;
    let all = grid::positions(triangulation.n, &order)?;
    Ok((triangulation, all))
}

/// Draws a biconnected simple graph on a grid. Returns [`TopologyError::NonPlanar`] when no embedding exists.
pub fn draw(graph: &graph::Graph) -> Result<Drawing, TopologyError> {
    let (triangulation, all) = draw_triangulation(graph)?;
    let positions: Vec<(i64, i64)> = all[..graph.n()].to_vec();
    let extent = |points: &[(i64, i64)]| {
        (
            points.iter().map(|p| p.0).max().unwrap_or(0),
            points.iter().map(|p| p.1).max().unwrap_or(0),
        )
    };
    let (width, height) = extent(&positions);
    let (grid_width, grid_height) = extent(&all);
    Ok(Drawing {
        positions,
        width,
        height,
        grid_width,
        grid_height,
        dummies: triangulation.dummies,
        diagonals: triangulation.diagonals,
    })
}

/// Stage 1 result for one graph: what was analysed, the planarity outcome and, when drawn, the drawing check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Analysis {
    pub vertices: usize,
    pub edges: usize,
    pub dropped_loops: usize,
    pub dropped_parallel: usize,
    pub biconnected: bool,
    /// `None` when planarity was not decided (not biconnected).
    pub planar: Option<bool>,
    /// Faces of the embedding (planar graphs only).
    pub faces: Option<usize>,
    /// Dummy vertices added by triangulation.
    pub dummies: Option<usize>,
    /// Visible extent (width, height) of the drawn original vertices.
    pub extent: Option<(i64, i64)>,
    /// Extent (width, height) of the construction grid over all triangulated vertices.
    pub grid: Option<(i64, i64)>,
    /// Whether the drawing passed [`check::validate`].
    pub drawing_valid: Option<bool>,
}

/// Analyses an edge list (self-loops and parallel edges are counted, not drawn). Never fails: undecided cases are
/// reported as `None`.
pub fn analyze(vertices: usize, edges: &[(usize, usize)]) -> Result<Analysis, TopologyError> {
    let (g, skeleton) = graph::Graph::from_edges(vertices, edges)?;
    let mut out = Analysis {
        vertices,
        edges: g.m(),
        dropped_loops: skeleton.dropped_loops,
        dropped_parallel: skeleton.dropped_parallel,
        biconnected: g.is_biconnected(),
        planar: None,
        faces: None,
        dummies: None,
        extent: None,
        grid: None,
        drawing_valid: None,
    };
    if !out.biconnected {
        return Ok(out);
    }
    match planarity::embed(&g)? {
        planarity::Planarity::NonPlanar => out.planar = Some(false),
        planarity::Planarity::Planar(embedding) => {
            out.planar = Some(true);
            out.faces = Some(embedding.faces.len());
            let drawing = draw(&g)?;
            out.dummies = Some(drawing.dummies);
            out.extent = Some((drawing.width, drawing.height));
            out.grid = Some((drawing.grid_width, drawing.grid_height));
            out.drawing_valid = Some(check::validate(&g, &drawing.positions).is_ok());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use graph::Graph;

    fn graph(n: usize, edges: &[(usize, usize)]) -> Graph {
        Graph::from_edges(n, edges).unwrap().0
    }

    /// Draws `g`, checks the drawing independently, and returns the grid size of the triangulation.
    fn drawn(g: &Graph) -> Drawing {
        let d = draw(g).unwrap();
        check::validate(g, &d.positions).unwrap_or_else(|e| panic!("invalid drawing: {e}"));
        d
    }

    #[test]
    fn small_planar_fixtures_draw_validly() {
        let k4 = graph(4, &[(0, 1), (0, 2), (0, 3), (1, 2), (1, 3), (2, 3)]);
        drawn(&k4);
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
        drawn(&wheel);
        let chorded = graph(6, &[(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (5, 0), (0, 3)]);
        drawn(&chorded);
        let square = graph(4, &[(0, 1), (1, 2), (2, 3), (3, 0)]);
        drawn(&square);
    }

    #[test]
    fn nonplanar_graphs_are_reported_not_drawn() {
        let mut k5 = Vec::new();
        for a in 0..5 {
            for b in a + 1..5 {
                k5.push((a, b));
            }
        }
        assert_eq!(draw(&graph(5, &k5)), Err(TopologyError::NonPlanar));
    }

    /// The visible edges of `benchmarks/inputs/state-tcp.json` (skeleton; nodes in file order).
    #[test]
    fn state_tcp_draws_validly() {
        let edges = [
            (0, 9),
            (0, 1),
            (0, 2),
            (0, 10),
            (7, 4),
            (7, 9),
            (8, 5),
            (8, 10),
            (4, 5),
            (4, 3),
            (4, 2),
            (5, 6),
            (5, 3),
            (6, 10),
            (1, 3),
            (1, 2),
            (3, 2),
        ];
        let g = graph(11, &edges);
        let d = drawn(&g);
        let total = 11 + d.dummies;
        assert!(
            d.grid_width <= 2 * total as i64 - 4 && d.grid_height <= total as i64 - 2,
            "grid"
        );
    }

    /// Stacked triangulations (every new vertex sits in a face): planar, 3-connected, and of random shape.
    fn stacked_triangulation(n: usize, seed: u64) -> Graph {
        let mut state = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut next = move |m: usize| {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as usize % m
        };
        let mut faces: Vec<[usize; 3]> = vec![[0, 1, 2], [0, 2, 1]];
        let mut edges = vec![(0, 1), (1, 2), (0, 2)];
        for v in 3..n {
            let i = next(faces.len());
            let [a, b, c] = faces.swap_remove(i);
            faces.push([a, b, v]);
            faces.push([b, c, v]);
            faces.push([c, a, v]);
            edges.extend([(a, v), (b, v), (c, v)]);
        }
        graph(n, &edges)
    }

    #[test]
    fn random_stacked_triangulations_draw_validly_on_the_bounded_grid() {
        for seed in 0..40u64 {
            for n in [4usize, 5, 7, 10, 16, 25] {
                let g = stacked_triangulation(n, seed);
                let d = drawn(&g);
                let total = (n + d.dummies) as i64;
                assert!(
                    d.grid_width <= 2 * total - 4 && d.grid_height <= total - 2,
                    "seed {seed} n {n}: {}x{} exceeds (2N-4)x(N-2) with N = {total}",
                    d.grid_width,
                    d.grid_height
                );
            }
        }
    }

    /// Stage 1 record for state-tcp, printed with `--nocapture`; the values are copied into the algorithm document.
    #[test]
    fn state_tcp_stage_one_record() {
        let edges = [
            (0, 9),
            (0, 1),
            (0, 2),
            (0, 10),
            (7, 4),
            (7, 9),
            (8, 5),
            (8, 10),
            (4, 5),
            (4, 3),
            (4, 2),
            (5, 6),
            (5, 3),
            (6, 10),
            (1, 3),
            (1, 2),
            (3, 2),
        ];
        let a = analyze(11, &edges).unwrap();
        eprintln!("STAGE1 state-tcp {a:?}");
        assert_eq!(a.planar, Some(true));
        assert_eq!(a.drawing_valid, Some(true));
    }

    /// Planarity answers agree with networkx on 303 biconnected graphs, also after relabelling the vertices and
    /// reversing the edge list. Every planar answer comes with an embedding satisfying Euler's formula and a valid
    /// drawing; the corpus contains both planar and nonplanar graphs, so negative answers are tested too.
    #[test]
    fn planarity_agrees_with_networkx_on_the_corpus() {
        let (mut planar_seen, mut nonplanar_seen) = (0, 0);
        for (n, edges, expected) in corpus::CORPUS {
            for shift in [0, 3] {
                let relabelled: Vec<(usize, usize)> = edges
                    .iter()
                    .rev()
                    .map(|&(a, b)| ((a + shift) % n, (b + shift) % n))
                    .collect();
                let g = graph(*n, &relabelled);
                match planarity::embed(&g).unwrap_or_else(|e| panic!("{n} vertices: {e}")) {
                    planarity::Planarity::Planar(embedding) => {
                        assert!(*expected, "{n} vertices: planar, networkx says nonplanar");
                        let faces = embedding.faces.len() as i64;
                        assert_eq!(*n as i64 - g.m() as i64 + faces, 2, "Euler");
                        drawn(&g);
                        if shift == 0 {
                            planar_seen += 1;
                        }
                    }
                    planarity::Planarity::NonPlanar => {
                        assert!(!*expected, "{n} vertices: nonplanar, networkx says planar");
                        if shift == 0 {
                            nonplanar_seen += 1;
                        }
                    }
                }
            }
        }
        assert!(
            planar_seen > 100 && nonplanar_seen > 100,
            "{planar_seen} planar, {nonplanar_seen} nonplanar"
        );
    }

    /// Every planar corpus drawing realises the rotation system of its triangulation at every vertex, with one global
    /// orientation: the drawn edge order is the embedding's, so routes and ports can be ordered from the embedding.
    #[test]
    fn drawings_realise_the_embedding_rotation() {
        let mut checked = 0;
        for (n, edges, expected) in corpus::CORPUS {
            if !expected {
                continue;
            }
            let g = graph(*n, edges);
            let (t, all) = draw_triangulation(&g).unwrap();
            let faces: Vec<Vec<usize>> = t.faces.iter().map(|f| f.to_vec()).collect();
            let rot = rotation::rotation_from_faces(t.n, &faces).unwrap();
            if let Err(e) = rotation::realises(&rot, &t.adj, &all) {
                panic!("{n} vertices: {e}");
            }
            checked += 1;
        }
        assert!(checked > 100, "{checked} planar corpus graphs checked");
    }
}
