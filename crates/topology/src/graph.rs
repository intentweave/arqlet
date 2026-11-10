//! Simple undirected graphs on vertices `0..n`.
use crate::TopologyError;
use std::collections::BTreeSet;

/// A simple undirected graph. Self-loops and parallel edges are not represented; [`Graph::from_edges`] counts them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Graph {
    n: usize,
    adj: Vec<Vec<usize>>,
}

/// What [`Graph::from_edges`] left out, reported so that analysis can state the scope honestly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Skeleton {
    pub dropped_loops: usize,
    pub dropped_parallel: usize,
}

impl Graph {
    /// Builds the simple skeleton of `edges` on `n` vertices. Endpoints must lie in `0..n`.
    pub fn from_edges(
        n: usize,
        edges: &[(usize, usize)],
    ) -> Result<(Graph, Skeleton), TopologyError> {
        let mut seen = BTreeSet::new();
        let mut skeleton = Skeleton::default();
        for &(a, b) in edges {
            if a >= n || b >= n {
                return Err(TopologyError::InvalidInput(format!(
                    "edge ({a}, {b}) outside 0..{n}"
                )));
            }
            if a == b {
                skeleton.dropped_loops += 1;
            } else if !seen.insert((a.min(b), a.max(b))) {
                skeleton.dropped_parallel += 1;
            }
        }
        let mut adj = vec![Vec::new(); n];
        for (a, b) in seen {
            adj[a].push(b);
            adj[b].push(a);
        }
        for list in &mut adj {
            list.sort_unstable();
        }
        Ok((Graph { n, adj }, skeleton))
    }

    pub fn n(&self) -> usize {
        self.n
    }

    pub fn m(&self) -> usize {
        self.adj.iter().map(Vec::len).sum::<usize>() / 2
    }

    pub fn adj(&self, v: usize) -> &[usize] {
        &self.adj[v]
    }

    /// All edges as `(a, b)` with `a < b`, sorted.
    pub fn edges(&self) -> Vec<(usize, usize)> {
        let mut out = Vec::with_capacity(self.m());
        for (a, list) in self.adj.iter().enumerate() {
            out.extend(list.iter().filter(|&&b| b > a).map(|&b| (a, b)));
        }
        out
    }

    /// Connectivity of the vertices not in `skip` (`skip` may be `None`).
    fn connected_without(&self, skip: Option<usize>) -> bool {
        let start = match (0..self.n).find(|&v| Some(v) != skip) {
            Some(start) => start,
            None => return true,
        };
        let mut seen = vec![false; self.n];
        let mut stack = vec![start];
        seen[start] = true;
        let mut count = 1;
        while let Some(v) = stack.pop() {
            for &u in &self.adj[v] {
                if Some(u) != skip && !seen[u] {
                    seen[u] = true;
                    count += 1;
                    stack.push(u);
                }
            }
        }
        let expected = self.n - usize::from(skip.is_some());
        count == expected
    }

    pub fn is_connected(&self) -> bool {
        self.connected_without(None)
    }

    /// At least three vertices, connected, and no cut vertex.
    pub fn is_biconnected(&self) -> bool {
        self.n >= 3
            && self.connected_without(None)
            && (0..self.n).all(|v| self.connected_without(Some(v)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_loops_and_parallel_edges_and_drops_them() {
        let (g, skeleton) =
            Graph::from_edges(3, &[(0, 1), (1, 0), (1, 1), (1, 2), (2, 0)]).unwrap();
        assert_eq!(g.m(), 3);
        assert_eq!(
            skeleton,
            Skeleton {
                dropped_loops: 1,
                dropped_parallel: 1
            }
        );
        assert_eq!(g.edges(), vec![(0, 1), (0, 2), (1, 2)]);
    }

    #[test]
    fn rejects_endpoints_outside_the_vertex_range() {
        assert!(matches!(
            Graph::from_edges(2, &[(0, 2)]),
            Err(TopologyError::InvalidInput(_))
        ));
    }

    #[test]
    fn biconnectivity_needs_no_cut_vertex() {
        let (cycle, _) = Graph::from_edges(4, &[(0, 1), (1, 2), (2, 3), (3, 0)]).unwrap();
        assert!(cycle.is_biconnected());
        let (path, _) = Graph::from_edges(3, &[(0, 1), (1, 2)]).unwrap();
        assert!(!path.is_biconnected());
        let (two_triangles, _) =
            Graph::from_edges(5, &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)]).unwrap();
        assert!(!two_triangles.is_biconnected());
    }
}
