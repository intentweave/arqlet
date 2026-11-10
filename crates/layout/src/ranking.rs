//! Provisional (consistent layered pipeline, option `hierarchicalOrdering`): ranks that minimise the summed weighted spans
//! of the links (network simplex, after Gansner, Koutsofios, North and Vo, "A Technique for Drawing Directed Graphs",
//! IEEE TSE 1993). Pure and deterministic; the graph must be acyclic (the caller orients its links by the cycle break).

/// Ranks of the nodes `0..n` for the links `(from, to, weight)`: every link spans at least one rank (`rank[to] >=
/// rank[from] + 1`), and the sum of `weight * (rank[to] - rank[from])` is least. Each weakly connected component starts at
/// rank 0. `links` must not contain a cycle; a link from a node to itself is ignored.
pub fn network_simplex(n: usize, links: &[(usize, usize, f64)]) -> Vec<usize> {
    let links: Vec<(usize, usize, f64)> = links.iter().copied().filter(|l| l.0 != l.1).collect();
    let mut rank = longest_path(n, &links);
    let mut component = vec![usize::MAX; n];
    for start in 0..n {
        if component[start] != usize::MAX {
            continue;
        }
        // The nodes of this component, by a search over the links in both directions.
        let mut members = vec![start];
        component[start] = start;
        let mut i = 0;
        while i < members.len() {
            let v = members[i];
            i += 1;
            for &(a, b, _) in &links {
                let other = if a == v {
                    b
                } else if b == v {
                    a
                } else {
                    continue;
                };
                if component[other] == usize::MAX {
                    component[other] = start;
                    members.push(other);
                }
            }
        }
        members.sort_unstable();
        let inner: Vec<(usize, usize, f64)> = links
            .iter()
            .copied()
            .filter(|l| component[l.0] == start)
            .collect();
        solve(&members, &inner, &mut rank);
        let low = members.iter().map(|&v| rank[v]).min().unwrap_or(0);
        for &v in &members {
            rank[v] -= low;
        }
    }
    rank.into_iter().map(|r| r as usize).collect()
}

/// The longest path ranks: a node one rank after its latest predecessor (a feasible start).
fn longest_path(n: usize, links: &[(usize, usize, f64)]) -> Vec<i64> {
    let mut indegree = vec![0usize; n];
    for &(_, b, _) in links {
        indegree[b] += 1;
    }
    let mut rank = vec![0i64; n];
    let mut ready: Vec<usize> = (0..n).filter(|&v| indegree[v] == 0).collect();
    ready.reverse();
    while let Some(v) = ready.pop() {
        for &(a, b, _) in links {
            if a == v {
                rank[b] = rank[b].max(rank[v] + 1);
                indegree[b] -= 1;
                if indegree[b] == 0 {
                    ready.push(b);
                }
            }
        }
    }
    rank
}

/// Network simplex on one connected component (`members`, its `links`), improving `rank` in place.
fn solve(members: &[usize], links: &[(usize, usize, f64)], rank: &mut [i64]) {
    if members.len() < 2 || links.is_empty() {
        return;
    }
    let slack = |rank: &[i64], l: &(usize, usize, f64)| rank[l.1] - rank[l.0] - 1;
    // A feasible spanning tree of tight links: grow it from the first member over tight links; when it stops, move the tree
    // along the link of least slack that has one end in it, which makes that link tight.
    let mut in_tree = vec![false; rank.len()];
    let mut tree: Vec<usize> = Vec::new();
    in_tree[members[0]] = true;
    let mut size = 1;
    while size < members.len() {
        let mut grew = true;
        while grew {
            grew = false;
            for (k, l) in links.iter().enumerate() {
                if in_tree[l.0] != in_tree[l.1] && slack(rank, l) == 0 {
                    in_tree[l.0] = true;
                    in_tree[l.1] = true;
                    tree.push(k);
                    size += 1;
                    grew = true;
                }
            }
        }
        if size == members.len() {
            break;
        }
        let Some((_, k)) = links
            .iter()
            .enumerate()
            .filter(|(_, l)| in_tree[l.0] != in_tree[l.1])
            .map(|(k, l)| (slack(rank, l), k))
            .min()
        else {
            break;
        };
        let l = links[k];
        // Moving the tree towards the outside end closes the slack.
        let delta = if in_tree[l.0] {
            slack(rank, &l)
        } else {
            -slack(rank, &l)
        };
        for &v in members {
            if in_tree[v] {
                rank[v] += delta;
            }
        }
    }
    if tree.len() + 1 != members.len() {
        return;
    }
    // Exchanges: a tree link with a negative cut value leaves; the non-tree link of least slack that crosses the cut the
    // other way enters. Bounded, and deterministic by the order of the links.
    let limit = 4 * links.len() * members.len() + 16;
    let mut in_tree_link = vec![false; links.len()];
    for &k in &tree {
        in_tree_link[k] = true;
    }
    for _ in 0..limit {
        let shape = Shape::of(members, links, &tree, rank.len());
        // The cut value of the tree link above every node (by the node below it): every link adds its weight on the tree
        // path between its ends, positive where it runs from the tree link's tail side to its head side.
        let mut cut = vec![0f64; rank.len()];
        for l in links {
            let (a, b) = (l.0, l.1);
            let mut x = a;
            while !shape.within(x, b) {
                // `x`'s subtree holds `a`, not `b`: the link leaves it.
                let t = links[shape.up_link[x]];
                cut[x] += if shape.within(x, t.0) { l.2 } else { -l.2 };
                x = shape.parent[x];
            }
            let mut y = b;
            while !shape.within(y, a) {
                // `y`'s subtree holds `b`, not `a`: the link enters it.
                let t = links[shape.up_link[y]];
                cut[y] += if shape.within(y, t.0) { -l.2 } else { l.2 };
                y = shape.parent[y];
            }
        }
        // The first tree link (in the order of the members) with a negative cut value leaves.
        let Some(&below) = members.iter().find(|&&v| v != shape.root && cut[v] < -1e-9) else {
            break;
        };
        let leaving = shape.up_link[below];
        let tail_below = shape.within(below, links[leaving].0);
        let tail_side = |v: usize| shape.within(below, v) == tail_below;
        // The entering link runs from the head side to the tail side, with the least slack.
        let Some((_, k)) = links
            .iter()
            .enumerate()
            .filter(|(k, l)| !in_tree_link[*k] && !tail_side(l.0) && tail_side(l.1))
            .map(|(k, l)| (slack(rank, l), k))
            .min()
        else {
            break;
        };
        let delta = slack(rank, &links[k]);
        // The head side moves towards the tail side by the slack, which makes the entering link tight.
        for &v in members {
            if !tail_side(v) {
                rank[v] += delta;
            }
        }
        let pos = tree
            .iter()
            .position(|&t| t == leaving)
            .expect("a tree link");
        tree[pos] = k;
        in_tree_link[leaving] = false;
        in_tree_link[k] = true;
    }
}

/// The spanning tree rooted at the first member, numbered in postorder: a node's subtree holds the nodes numbered from its
/// `low` to its `lim`.
struct Shape {
    root: usize,
    parent: Vec<usize>,
    up_link: Vec<usize>,
    low: Vec<usize>,
    lim: Vec<usize>,
}

impl Shape {
    fn of(members: &[usize], links: &[(usize, usize, f64)], tree: &[usize], n: usize) -> Shape {
        let mut adjacent: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
        for &k in tree {
            let l = links[k];
            adjacent[l.0].push((l.1, k));
            adjacent[l.1].push((l.0, k));
        }
        let root = members[0];
        let (mut parent, mut up_link) = (vec![usize::MAX; n], vec![usize::MAX; n]);
        let (mut low, mut lim) = (vec![0; n], vec![0; n]);
        let mut seen = vec![false; n];
        let mut counter = 0;
        // Iterative depth-first search: (node, next neighbour index).
        let mut stack = vec![(root, 0usize)];
        seen[root] = true;
        low[root] = counter;
        while let Some(&mut (v, ref mut next)) = stack.last_mut() {
            if *next < adjacent[v].len() {
                let (w, k) = adjacent[v][*next];
                *next += 1;
                if !seen[w] {
                    seen[w] = true;
                    parent[w] = v;
                    up_link[w] = k;
                    low[w] = counter;
                    stack.push((w, 0));
                }
            } else {
                lim[v] = counter;
                counter += 1;
                stack.pop();
            }
        }
        Shape {
            root,
            parent,
            up_link,
            low,
            lim,
        }
    }

    /// Whether `v` lies in the subtree of `x`.
    fn within(&self, x: usize, v: usize) -> bool {
        self.low[x] <= self.lim[v] && self.lim[v] <= self.lim[x]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn total(rank: &[usize], links: &[(usize, usize, f64)]) -> f64 {
        links
            .iter()
            .map(|l| l.2 * (rank[l.1] as f64 - rank[l.0] as f64))
            .sum()
    }

    #[test]
    fn a_node_with_one_late_successor_moves_down_to_it() {
        // 0 -> 1 -> 2 -> 3, and 4 -> 3: the longest path puts 4 at rank 0 (span 3); the least spans put it at rank 2.
        let links = [(0, 1, 1.), (1, 2, 1.), (2, 3, 1.), (4, 3, 1.)];
        let rank = network_simplex(5, &links);
        assert_eq!(rank, vec![0, 1, 2, 3, 2]);
        assert_eq!(total(&rank, &links), 4.);
    }

    #[test]
    fn heavy_links_stay_short() {
        // 0 -> 1 -> 2 and 0 -> 2 heavy, 3 -> 2: every link spans at least one rank, and the result is never worse than the
        // longest path.
        let links = [(0, 1, 1.), (1, 2, 1.), (0, 2, 5.), (3, 2, 1.)];
        let rank = network_simplex(4, &links);
        for l in &links {
            assert!(rank[l.1] > rank[l.0]);
        }
        let start: Vec<usize> = longest_path(4, &links)
            .into_iter()
            .map(|r| r as usize)
            .collect();
        assert!(total(&rank, &links) <= total(&start, &links));
    }

    #[test]
    fn components_start_at_rank_zero() {
        let links = [(0, 1, 1.), (2, 3, 1.)];
        assert_eq!(network_simplex(5, &links), vec![0, 1, 0, 1, 0]);
    }
}
