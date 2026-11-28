//! Provisional (#237, ADR 0018 addendum of 2026-11-28): Leiden community detection, beside Louvain.
//!
//! Implemented from the description in V. A. Traag, L. Waltman and N. J. van Eck, "From Louvain to Leiden: guaranteeing
//! well-connected communities", Scientific Reports 9:5233 (2019), its section on the Leiden algorithm and its
//! Algorithms A.1–A.3. No code of any implementation was read or copied.
//!
//! Three phases per level: fast local moving of nodes (a queue), refinement of every community into well-connected
//! subcommunities, and aggregation on the refined partition, with the moving phase's partition as the aggregate's start.
//! The quality is modularity with a resolution γ, as Louvain here optimises it.
//!
//! Deterministic, without randomness: the queue starts in index order (the paper: random order), and the refinement is
//! greedy (the paper's limit θ → 0 of its randomised choice): a node joins the well-connected subcommunity with the
//! largest gain, ties to the smaller one. The guarantee of connected communities holds all the same; it rests on merging
//! nodes still alone into well-connected subsets with a positive gain, not on the randomness. Work is counted in `u64`.
use super::Meter;
use std::collections::{BTreeMap, VecDeque};

/// Below this a gain is no gain (as in Louvain).
const EPS: f64 = 1e-12;

/// The graph of one level: adjacency with self-loops kept (the inside of an aggregated community), degrees (a self-loop
/// counts twice) and twice the total weight.
struct Level {
    adjacent: Vec<Vec<(usize, f64)>>,
    degree: Vec<f64>,
    total: f64,
    /// Units of one pass over the nodes and their adjacency.
    sweep: u64,
}

impl Level {
    fn new(nodes: usize, edges: &BTreeMap<(usize, usize), f64>) -> Self {
        let mut adjacent: Vec<Vec<(usize, f64)>> = vec![Vec::new(); nodes];
        let mut degree = vec![0f64; nodes];
        let mut total = 0.;
        for (&(a, b), &w) in edges {
            if a == b {
                adjacent[a].push((a, w));
                degree[a] += 2. * w;
            } else {
                adjacent[a].push((b, w));
                adjacent[b].push((a, w));
                degree[a] += w;
                degree[b] += w;
            }
            total += 2. * w;
        }
        let sweep = nodes as u64 + adjacent.iter().map(|a| a.len() as u64).sum::<u64>();
        Self {
            adjacent,
            degree,
            total,
            sweep,
        }
    }

    fn nodes(&self) -> usize {
        self.degree.len()
    }
}

/// Leiden on `k` nodes with weighted links, at resolution `gamma`: the community of every node, level by level
/// (`result[l][c]`: the community at level l + 1 of community c at level l), as `louvain` returns them. Each level is a
/// refined partition (the last one is the final partition), so every community of every level is connected in the
/// links. Communities are numbered in order of their first node. Nodes without links stay alone. `None` when the meter
/// stops it (units: per level the nodes and links read and aggregated; per node taken from the queue 1 plus its links;
/// per refinement two passes over the nodes and their links).
pub(super) fn leiden(
    k: usize,
    links: &[(usize, usize, f64)],
    gamma: f64,
    meter: &mut dyn Meter,
) -> Option<Vec<Vec<usize>>> {
    let mut result = Vec::new();
    let mut nodes = k;
    let mut edges: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for &(a, b, w) in links {
        *edges.entry((a.min(b), a.max(b))).or_default() += w;
    }
    // The start of each level's moving phase: every node alone, then the communities carried over the aggregation.
    let mut start: Vec<usize> = (0..nodes).collect();
    for _ in 0..32 {
        if !meter.spend(nodes as u64 + edges.len() as u64) {
            return None;
        }
        let g = Level::new(nodes, &edges);
        if g.total == 0. {
            break;
        }
        let mut community = start;
        move_nodes_fast(&g, &mut community, gamma, meter)?;
        let (part, count) = renumber(&community);
        // Every community is one node: the level before is the final partition.
        if count == nodes {
            break;
        }
        let (refined, refined_count) = renumber(&refine(&g, &part, count, gamma, meter)?);
        if refined_count == nodes {
            // The refinement merged nothing, so aggregating would not shrink the graph: the moving phase's communities,
            // split into their connected parts, are the last level (no link joins two parts, so modularity only rises).
            if !meter.spend(g.sweep) {
                return None;
            }
            let (parts, parts_count) = renumber(&connected_parts(&g, &part));
            if parts_count < nodes {
                result.push(parts);
            }
            break;
        }
        let mut aggregated: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        for (&(a, b), &w) in &edges {
            let (ca, cb) = (refined[a], refined[b]);
            *aggregated.entry((ca.min(cb), ca.max(cb))).or_default() += w;
        }
        // Each aggregate node starts in the moving phase's community of its members (all in one: refined communities
        // lie inside them). Those numbers are below `count` ≤ `refined_count`, the aggregate's nodes.
        let mut next = vec![0usize; refined_count];
        for i in 0..nodes {
            next[refined[i]] = part[i];
        }
        result.push(refined);
        edges = aggregated;
        nodes = refined_count;
        start = next;
    }
    Some(result)
}

/// Renumbers communities in order of their first node; returns the numbers and how many there are.
fn renumber(community: &[usize]) -> (Vec<usize>, usize) {
    let mut number: BTreeMap<usize, usize> = BTreeMap::new();
    let renamed = community
        .iter()
        .map(|&c| {
            let next = number.len();
            *number.entry(c).or_insert(next)
        })
        .collect();
    (renamed, number.len())
}

/// The fast local moving phase (Algorithm A.2 of the paper, `MoveNodesFast`): a queue of every node in index order; a
/// node taken from it moves to the community with the largest gain (the empty community included, gain 0) if the gain
/// is real, ties to the smaller community, and its neighbours outside the community it joined that are not queued join
/// the end of the queue. At most 64 × nodes takes (Louvain: at most 64 sweeps).
fn move_nodes_fast(
    g: &Level,
    community: &mut [usize],
    gamma: f64,
    meter: &mut dyn Meter,
) -> Option<()> {
    let n = g.nodes();
    let mut tot = vec![0f64; n];
    let mut size = vec![0usize; n];
    for i in 0..n {
        tot[community[i]] += g.degree[i];
        size[community[i]] += 1;
    }
    // Unused community numbers, the smallest on top.
    let mut empty: Vec<usize> = (0..n).rev().filter(|&c| size[c] == 0).collect();
    let mut queue: VecDeque<usize> = (0..n).collect();
    let mut queued = vec![true; n];
    let mut weight_to = vec![0f64; n];
    let mut touched: Vec<usize> = Vec::new();
    let limit = 64u64.saturating_mul(n as u64);
    let mut taken = 0u64;
    while let Some(i) = queue.pop_front() {
        queued[i] = false;
        taken += 1;
        if taken > limit {
            break;
        }
        if !meter.spend(1 + g.adjacent[i].len() as u64) {
            return None;
        }
        let own = community[i];
        tot[own] -= g.degree[i];
        size[own] -= 1;
        for &(j, w) in &g.adjacent[i] {
            if j == i {
                continue;
            }
            let c = community[j];
            if weight_to[c] == 0. {
                touched.push(c);
            }
            weight_to[c] += w;
        }
        // The gain of joining c, less what is common to all choices: k_i,in(c) - γ tot(c) k_i / 2m.
        let gain = |c: usize, w: f64| w - gamma * tot[c] * g.degree[i] / g.total;
        let mut best = own;
        let mut best_gain = gain(own, weight_to[own]);
        touched.sort_unstable();
        for &c in &touched {
            if c == own {
                continue;
            }
            let x = gain(c, weight_to[c]);
            if x > best_gain + EPS {
                best = c;
                best_gain = x;
            }
        }
        // The empty community (gain 0), when the node does not form one already. One is free: the other n - 1 nodes
        // fill at most n - 1 numbers, and its own is taken.
        if size[own] > 0 && 0. > best_gain + EPS {
            if let Some(&free) = empty.last() {
                best = free;
                empty.pop();
            }
        }
        for &c in &touched {
            weight_to[c] = 0.;
        }
        touched.clear();
        tot[best] += g.degree[i];
        size[best] += 1;
        if best != own {
            community[i] = best;
            if size[own] == 0 {
                empty.push(own);
            }
            for &(j, _) in &g.adjacent[i] {
                if j != i && !queued[j] && community[j] != best {
                    queued[j] = true;
                    queue.push_back(j);
                }
            }
        }
    }
    Some(())
}

/// The refinement (Algorithm A.3 of the paper, `RefinePartition` and `MergeNodesSubset`), greedy: within every community
/// `S` of `part` (`count` of them), every node starts alone; in index order, a node still alone and well connected to
/// `S` joins the well-connected refined community of a neighbour in `S` with the largest real gain (ties to the smaller
/// one). Well connected: `E(X, S − X) ≥ γ K_X (K_S − K_X) / 2m`. Returns the refined community of every node.
fn refine(
    g: &Level,
    part: &[usize],
    count: usize,
    gamma: f64,
    meter: &mut dyn Meter,
) -> Option<Vec<usize>> {
    let n = g.nodes();
    // First pass: the total degree of every community, and every node's links to the rest of its community.
    if !meter.spend(g.sweep) {
        return None;
    }
    let mut k_s = vec![0f64; count];
    let mut inside = vec![0f64; n];
    for i in 0..n {
        k_s[part[i]] += g.degree[i];
        for &(j, w) in &g.adjacent[i] {
            if j != i && part[j] == part[i] {
                inside[i] += w;
            }
        }
    }
    // Second pass: the merging.
    if !meter.spend(g.sweep) {
        return None;
    }
    let well = |links: f64, k: f64, k_set: f64| links >= gamma * k * (k_set - k) / g.total - EPS;
    let mut refined: Vec<usize> = (0..n).collect();
    let mut tot = g.degree.clone();
    let mut outside = inside.clone();
    let mut size = vec![1usize; n];
    let mut weight_to = vec![0f64; n];
    let mut touched: Vec<usize> = Vec::new();
    for i in 0..n {
        let s = part[i];
        if size[refined[i]] != 1 || !well(inside[i], g.degree[i], k_s[s]) {
            continue;
        }
        for &(j, w) in &g.adjacent[i] {
            if j == i || part[j] != s {
                continue;
            }
            let c = refined[j];
            if weight_to[c] == 0. {
                touched.push(c);
            }
            weight_to[c] += w;
        }
        touched.sort_unstable();
        let mut best = None;
        let mut best_gain = 0.;
        for &c in &touched {
            if !well(outside[c], tot[c], k_s[s]) {
                continue;
            }
            let x = weight_to[c] - gamma * tot[c] * g.degree[i] / g.total;
            if x > best_gain + EPS {
                best = Some(c);
                best_gain = x;
            }
        }
        if let Some(c) = best {
            let own = refined[i];
            // E(C + v, S − C − v) = E(C, S − C) + E(v, S − v) − 2 w(v, C).
            outside[c] += inside[i] - 2. * weight_to[c];
            tot[c] += g.degree[i];
            size[c] += 1;
            tot[own] = 0.;
            size[own] = 0;
            refined[i] = c;
        }
        for &c in &touched {
            weight_to[c] = 0.;
        }
        touched.clear();
    }
    Some(refined)
}

/// The communities of `part` split into their connected parts (by the links between members of one community), each
/// part numbered by its first node.
fn connected_parts(g: &Level, part: &[usize]) -> Vec<usize> {
    let n = g.nodes();
    let mut out = vec![usize::MAX; n];
    let mut stack = Vec::new();
    for first in 0..n {
        if out[first] != usize::MAX {
            continue;
        }
        out[first] = first;
        stack.push(first);
        while let Some(i) = stack.pop() {
            for &(j, _) in &g.adjacent[i] {
                if out[j] == usize::MAX && part[j] == part[i] {
                    out[j] = first;
                    stack.push(j);
                }
            }
        }
    }
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::overview::Unlimited;

    /// Whether every community of every level holds members connected by the links among them.
    pub(crate) fn connected_levels(
        k: usize,
        links: &[(usize, usize, f64)],
        levels: &[Vec<usize>],
    ) -> bool {
        // The community of every member at each level.
        let mut of: Vec<usize> = (0..k).collect();
        for level in levels {
            for c in of.iter_mut() {
                *c = level[*c];
            }
            let parts = {
                let mut adjacent = vec![Vec::new(); k];
                for &(a, b, _) in links {
                    if of[a] == of[b] {
                        adjacent[a].push(b);
                        adjacent[b].push(a);
                    }
                }
                let mut seen = vec![false; k];
                let mut communities_seen = std::collections::BTreeSet::new();
                let mut ok = true;
                for first in 0..k {
                    if seen[first] {
                        continue;
                    }
                    if !communities_seen.insert(of[first]) {
                        ok = false;
                    }
                    seen[first] = true;
                    let mut stack = vec![first];
                    while let Some(i) = stack.pop() {
                        for &j in &adjacent[i] {
                            if !seen[j] {
                                seen[j] = true;
                                stack.push(j);
                            }
                        }
                    }
                }
                ok
            };
            if !parts {
                return false;
            }
        }
        true
    }

    #[test]
    fn two_cliques_and_a_bridge_are_two_communities() {
        let mut links = Vec::new();
        for (base, size) in [(0, 6), (6, 6)] {
            for a in 0..size {
                for b in a + 1..size {
                    links.push((base + a, base + b, 1.));
                }
            }
        }
        links.push((5, 6, 1.));
        let levels = leiden(12, &links, 1., &mut Unlimited).unwrap();
        let mut of: Vec<usize> = (0..12).collect();
        for level in &levels {
            for c in of.iter_mut() {
                *c = level[*c];
            }
        }
        assert!(of[..6].iter().all(|&c| c == of[0]));
        assert!(of[6..].iter().all(|&c| c == of[6]));
        assert_ne!(of[0], of[6]);
        assert!(connected_levels(12, &links, &levels));
    }

    /// Found by a deterministic search over generated graphs with planted groups and shrunk link by link while Louvain
    /// still left a community in parts: at its first level Louvain puts 2, 11, 22 and 26 into one community, but no
    /// link joins {2, 11} to {22, 26}.
    const LOUVAIN_IN_PARTS: [(usize, usize); 43] = [
        (0, 35),
        (1, 15),
        (2, 11),
        (2, 15),
        (2, 31),
        (2, 32),
        (3, 8),
        (3, 16),
        (3, 20),
        (4, 13),
        (4, 22),
        (4, 34),
        (5, 6),
        (5, 23),
        (7, 31),
        (8, 12),
        (8, 20),
        (8, 28),
        (8, 32),
        (9, 12),
        (9, 21),
        (10, 24),
        (10, 30),
        (10, 31),
        (11, 16),
        (12, 20),
        (12, 32),
        (13, 33),
        (14, 24),
        (15, 19),
        (15, 25),
        (16, 20),
        (16, 26),
        (16, 28),
        (17, 23),
        (18, 23),
        (20, 28),
        (22, 26),
        (23, 29),
        (27, 31),
        (29, 35),
        (32, 35),
        (33, 34),
    ];

    #[test]
    fn leiden_keeps_connected_what_louvain_leaves_in_parts() {
        let links: Vec<(usize, usize, f64)> =
            LOUVAIN_IN_PARTS.iter().map(|&(a, b)| (a, b, 1.)).collect();
        let louvain = super::super::louvain(36, &links, 1., &mut Unlimited).unwrap();
        assert!(
            !connected_levels(36, &links, &louvain),
            "the case still shows Louvain's flaw"
        );
        assert_eq!(
            louvain[0][2], louvain[0][22],
            "2 and 22 share a Louvain community"
        );
        let levels = leiden(36, &links, 1., &mut Unlimited).unwrap();
        assert!(connected_levels(36, &links, &levels));
        assert!(!levels.is_empty());
    }

    /// Generated graphs with planted groups (a fixed xorshift stream), as the search above used.
    fn planted(count: usize) -> Vec<(usize, Vec<(usize, usize, f64)>)> {
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..count)
            .map(|_| {
                let k = 20 + (next() % 100) as usize;
                let groups = 2 + (next() % 6) as usize;
                let m = k * (2 + (next() % 3) as usize);
                let mut seen = std::collections::BTreeSet::new();
                let mut links = Vec::new();
                for _ in 0..m {
                    let a = (next() % k as u64) as usize;
                    let b = if next() % 10 < 7 {
                        let r = (next() % (k as u64 / groups as u64 + 1)) as usize * groups
                            + a % groups;
                        r.min(k - 1)
                    } else {
                        (next() % k as u64) as usize
                    };
                    if a != b && seen.insert((a.min(b), a.max(b))) {
                        links.push((a.min(b), a.max(b), 1. + (next() % 3) as f64));
                    }
                }
                (k, links)
            })
            .collect()
    }

    #[test]
    fn every_level_of_leiden_is_connected_on_generated_graphs() {
        for (k, links) in planted(300) {
            for gamma in [0.5, 1., 2.] {
                let levels = leiden(k, &links, gamma, &mut Unlimited).unwrap();
                assert!(
                    connected_levels(k, &links, &levels),
                    "{k} nodes, γ {gamma}: {links:?}"
                );
            }
        }
    }

    /// Modularity (resolution 1) of the final partition.
    fn modularity(k: usize, links: &[(usize, usize, f64)], levels: &[Vec<usize>]) -> f64 {
        let mut of: Vec<usize> = (0..k).collect();
        for level in levels {
            for c in of.iter_mut() {
                *c = level[*c];
            }
        }
        let (mut degree, mut inside, mut total) = (vec![0.; k], vec![0.; k], 0.);
        for &(a, b, w) in links {
            degree[a] += w;
            degree[b] += w;
            total += 2. * w;
            if of[a] == of[b] {
                inside[of[a]] += 2. * w;
            }
        }
        let mut tot = vec![0.; k];
        for i in 0..k {
            tot[of[i]] += degree[i];
        }
        (0..k)
            .map(|c| inside[c] / total - (tot[c] / total) * (tot[c] / total))
            .sum()
    }

    #[test]
    fn leiden_finds_about_the_modularity_of_louvain() {
        // Not a quality claim (that is the measurement's): only that the greedy refinement does not lose the partition.
        let (mut louvain_sum, mut leiden_sum) = (0., 0.);
        for (k, links) in planted(100) {
            louvain_sum += modularity(
                k,
                &links,
                &super::super::louvain(k, &links, 1., &mut Unlimited).unwrap(),
            );
            leiden_sum += modularity(k, &links, &leiden(k, &links, 1., &mut Unlimited).unwrap());
        }
        assert!(
            leiden_sum >= 0.97 * louvain_sum,
            "Leiden {leiden_sum} against Louvain {louvain_sum}"
        );
    }

    #[test]
    fn a_higher_resolution_gives_more_communities() {
        let count = |gamma: f64| {
            let (k, links) = planted(1).remove(0);
            let levels = leiden(k, &links, gamma, &mut Unlimited).unwrap();
            let mut of: Vec<usize> = (0..k).collect();
            for level in &levels {
                for c in of.iter_mut() {
                    *c = level[*c];
                }
            }
            of.iter().collect::<std::collections::BTreeSet<_>>().len()
        };
        assert!(count(0.3) < count(1.));
        assert!(count(1.) < count(4.));
    }

    #[test]
    fn the_meter_stops_leiden_and_counts_like_louvain() {
        for (k, links) in planted(20) {
            let mut full = crate::overview::Limit::new(u64::MAX);
            let levels = leiden(k, &links, 1., &mut full).unwrap();
            // The same result whenever it fits, and none when one unit less is left.
            let mut exact = crate::overview::Limit::new(full.used);
            assert_eq!(leiden(k, &links, 1., &mut exact), Some(levels));
            assert_eq!(exact.used, full.used);
            let mut short = crate::overview::Limit::new(full.used - 1);
            assert_eq!(leiden(k, &links, 1., &mut short), None);
            // Comparable to Louvain's: within a small factor on the same links.
            let mut louvain = crate::overview::Limit::new(u64::MAX);
            super::super::louvain(k, &links, 1., &mut louvain).unwrap();
            assert!(
                full.used <= 4 * louvain.used && louvain.used <= 4 * full.used,
                "{} against {}",
                full.used,
                louvain.used
            );
        }
    }

    #[test]
    fn nodes_without_links_stay_alone_and_no_links_give_no_levels() {
        assert!(leiden(5, &[], 1., &mut Unlimited).unwrap().is_empty());
        let levels = leiden(4, &[(0, 1, 1.)], 1., &mut Unlimited).unwrap();
        let mut of: Vec<usize> = (0..4).collect();
        for level in &levels {
            for c in of.iter_mut() {
                *c = level[*c];
            }
        }
        assert_eq!(of[0], of[1]);
        assert!(of[2] != of[3] && of[2] != of[0] && of[3] != of[0]);
    }
}
