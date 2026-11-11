//! Organic placement of one area's children, in three stages that can each be inspected:
//!
//! 1. Initialization: pivot MDS on the squared graph distances (farthest-point pivots, double centring, the two leading
//!    eigenvectors by power iteration). Deterministic and non-degenerate: a tiny fixed offset breaks exact collinearity.
//! 2. Stress majorization (SMACOF, the weighted Guttman transform with weights 1/d²) towards target lengths `L·d`, where
//!    `L` is the typical centre-to-centre spacing of the real node sizes. The stress is measured before and after.
//! 3. Separation by the real sizes: overlapping boxes move apart, along the axis of smaller overlap, in index order.
//!
//! Pure and deterministic. The dense distance matrix is quadratic in the number of children, so an area above `MAX_NODES` is
//! refused before placement (`check_organic_areas`). With `sparseStress`, the stress uses only graph neighbours and pivots
//! (`SparseTargets`), and the larger bound `SPARSE_MAX_NODES` applies.

use crate::{spatial::SpatialHash, vpsc};
use arqlet_model::{Graph, Mode, Rect};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;

/// The largest number of children an organic area may have in the first version (the distance matrix is n² values).
pub const MAX_NODES: usize = 300;

/// The largest number of children of an area under `sparseStress`. The stress is O(pivots·n + m) per iteration, but the repair
/// stage still keeps a triangular table of pairs (n²/2 entries) and the import of 500 children took 82 s in one release run
/// (300 took 13.6 s). Larger areas are not measured, so they are not allowed yet.
pub const SPARSE_MAX_NODES: usize = 500;

/// The largest number of children of an area under `scalableStress` (see `scalable_rounds`). The repair still keeps a table
/// of node pairs and of (edge, node) items, quadratic in memory; the bound is the largest size measured
/// (`benchmarks/results/organic-scalable-20261109.md`).
pub const SCALABLE_MAX_NODES: usize = 2000;

/// The number of pivots of the sparse stress (the same farthest-point choice as the dense pivot MDS, capped at this).
const SPARSE_PIVOTS: usize = 50;

/// The iteration budget of the stress stage when the options do not set one.
pub const DEFAULT_ITERATIONS: usize = 300;

/// Spacing kept between two boxes that the separation step moves apart.
const GAP: f64 = 24.;

/// The positions after each stage, in the order of the ids, with the stress before and after the majorization. The stage
/// values are read by the tests and for diagnosis; the placement itself uses the separated positions.
#[derive(Clone, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub struct Stages {
    pub initial: Vec<(f64, f64)>,
    pub stress: Vec<(f64, f64)>,
    pub separated: Vec<(f64, f64)>,
    pub stress_initial: f64,
    pub stress_final: f64,
    pub iterations: usize,
    pub target_length: f64,
    pub stress_nanos: u64,
    pub separation_nanos: u64,
}

/// Refuses an organic area above the node bound (`SPARSE_MAX_NODES` under `sparse`). Areas placed otherwise are not checked here.
pub fn check_organic_areas(graph: &Graph, sparse: bool, scalable: bool) -> Result<(), String> {
    let bound = if scalable {
        SCALABLE_MAX_NODES
    } else if sparse {
        SPARSE_MAX_NODES
    } else {
        MAX_NODES
    };
    let mut areas: Vec<Option<&str>> = vec![None];
    areas.extend(
        graph
            .nodes
            .iter()
            .filter(|n| {
                graph
                    .nodes
                    .iter()
                    .any(|c| c.parent_id.as_deref() == Some(n.id.as_str()))
            })
            .map(|n| Some(n.id.as_str())),
    );
    for parent in areas {
        if graph.area_mode(parent)? != Mode::Organic {
            continue;
        }
        let count = graph
            .nodes
            .iter()
            .filter(|n| n.parent_id.as_deref() == parent)
            .count();
        if count > bound {
            return Err(format!(
                "organic placement is bounded to {bound} children per area; {} has {count}",
                parent.unwrap_or("the root")
            ));
        }
    }
    Ok(())
}

/// The distances of the last area that was asked for. A step of a drag lays out the same area again with the same children and
/// links, so the breadth-first searches are not repeated; a hit gives the value a new search would give, so the output does
/// not depend on the cache. Kept per thread, one entry, compared by content.
#[derive(Default)]
struct DistanceMemo {
    key: Option<(Vec<String>, BTreeSet<(String, String)>)>,
    value: Rc<Vec<f64>>,
    hits: usize,
    misses: usize,
}

thread_local! {
    static DISTANCES: std::cell::RefCell<DistanceMemo> = std::cell::RefCell::new(DistanceMemo::default());
}

/// How often the distances were taken from the memo and how often they were computed, on this thread.
#[cfg(test)]
pub fn distance_memo_stats() -> (usize, usize) {
    DISTANCES.with(|memo| {
        let memo = memo.borrow();
        (memo.hits, memo.misses)
    })
}

fn distances(ids: &[String], links: &BTreeSet<(String, String)>) -> Rc<Vec<f64>> {
    DISTANCES.with(|memo| {
        let mut memo = memo.borrow_mut();
        if memo
            .key
            .as_ref()
            .is_some_and(|(i, l)| i.as_slice() == ids && l == links)
        {
            memo.hits += 1;
            return Rc::clone(&memo.value);
        }
        let value = Rc::new(compute_distances(ids, links));
        memo.misses += 1;
        memo.key = Some((ids.to_vec(), links.clone()));
        memo.value = Rc::clone(&value);
        value
    })
}

/// Graph distances between the children over their undirected links. Pairs in different components get one more than the
/// largest finite distance, so that components drift apart without an infinite value.
fn compute_distances(ids: &[String], links: &BTreeSet<(String, String)>) -> Vec<f64> {
    let index: BTreeMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let n = ids.len();
    let mut adjacent = vec![Vec::new(); n];
    for (a, b) in links {
        let (i, j) = (index[a.as_str()], index[b.as_str()]);
        adjacent[i].push(j);
        adjacent[j].push(i);
    }
    let mut d = vec![f64::INFINITY; n * n];
    let mut largest = 1.0f64;
    for source in 0..n {
        d[source * n + source] = 0.;
        let mut queue = VecDeque::from([source]);
        while let Some(v) = queue.pop_front() {
            for &w in &adjacent[v] {
                let index = source * n + w;
                if d[index].is_infinite() {
                    d[index] = d[source * n + v] + 1.;
                    largest = largest.max(d[index]);
                    queue.push_back(w);
                }
            }
        }
    }
    for value in &mut d {
        if value.is_infinite() {
            *value = largest + 1.;
        }
    }
    d
}

/// Pivot MDS: farthest-point pivots (the first is index 0), the squared distances to them double-centred, and the two
/// leading eigenvectors by power iteration. Returns coordinates in hop units.
fn pivot_mds(d: &[f64], n: usize) -> Vec<(f64, f64)> {
    if n == 1 {
        return vec![(0., 0.)];
    }
    let k = n.min(50);
    let mut pivots = vec![0usize];
    let mut nearest: Vec<f64> = (0..n).map(|i| d[i]).collect();
    while pivots.len() < k {
        let next = (0..n)
            .filter(|i| !pivots.contains(i))
            .fold(None, |best: Option<usize>, i| match best {
                Some(b) if nearest[b] >= nearest[i] => Some(b),
                _ => Some(i),
            });
        let Some(next) = next else { break };
        pivots.push(next);
        for i in 0..n {
            nearest[i] = nearest[i].min(d[next * n + i]);
        }
    }
    pivot_coordinates(n, &pivots, |i, q| d[i * n + pivots[q]])
}

/// Pivot coordinates from the distances of every node to the pivots: `dist(i, q)` is the distance from node `i` to pivot `q`.
fn pivot_coordinates(
    n: usize,
    pivots: &[usize],
    dist: impl Fn(usize, usize) -> f64,
) -> Vec<(f64, f64)> {
    if n == 1 {
        return vec![(0., 0.)];
    }
    let k = pivots.len();
    // A[i][q] = squared distance from node i to pivot q; B = double-centred -A/2.
    let a: Vec<Vec<f64>> = (0..n)
        .map(|i| (0..k).map(|q| dist(i, q) * dist(i, q)).collect())
        .collect();
    let row_mean: Vec<f64> = a.iter().map(|r| r.iter().sum::<f64>() / k as f64).collect();
    let col_mean: Vec<f64> = (0..k)
        .map(|q| a.iter().map(|r| r[q]).sum::<f64>() / n as f64)
        .collect();
    let grand = row_mean.iter().sum::<f64>() / n as f64;
    let b: Vec<Vec<f64>> = (0..n)
        .map(|i| {
            (0..k)
                .map(|q| -0.5 * (a[i][q] - row_mean[i] - col_mean[q] + grand))
                .collect()
        })
        .collect();
    // M = BᵀB (k×k); its two leading eigenvectors give the coordinates.
    let mut m = vec![vec![0f64; k]; k];
    for row in &b {
        for q in 0..k {
            for r in 0..k {
                m[q][r] += row[q] * row[r];
            }
        }
    }
    let v1 = power(&m, None);
    let v2 = power(&m, Some(&v1));
    let mut coords: Vec<(f64, f64)> = b
        .iter()
        .map(|row| {
            let x: f64 = (0..k).map(|q| row[q] * v1[q]).sum();
            let y: f64 = (0..k).map(|q| row[q] * v2[q]).sum();
            (x, y)
        })
        .collect();
    // Non-degeneracy: a path has one dominant axis; a fixed, index-based offset keeps the second axis from collapsing.
    let scale = coords
        .iter()
        .map(|c| c.0.abs().max(c.1.abs()))
        .fold(1e-9f64, f64::max);
    for (i, c) in coords.iter_mut().enumerate() {
        let offset = ((i * 2_654_435_761) % 1000) as f64 / 1000. - 0.5;
        c.1 += 0.05 * scale * offset;
    }
    coords
}

/// The leading eigenvector of a symmetric matrix by power iteration, optionally orthogonal to `against`.
fn power(m: &[Vec<f64>], against: Option<&[f64]>) -> Vec<f64> {
    let k = m.len();
    let mut v: Vec<f64> = (0..k).map(|q| 1. + q as f64 * 0.01).collect();
    for _ in 0..200 {
        if let Some(u) = against {
            let dot: f64 = v.iter().zip(u).map(|(a, b)| a * b).sum();
            for (a, b) in v.iter_mut().zip(u) {
                *a -= dot * b;
            }
        }
        let mut next: Vec<f64> = (0..k)
            .map(|q| (0..k).map(|r| m[q][r] * v[r]).sum())
            .collect();
        if let Some(u) = against {
            let dot: f64 = next.iter().zip(u).map(|(a, b)| a * b).sum();
            for (a, b) in next.iter_mut().zip(u) {
                *a -= dot * b;
            }
        }
        let norm = next.iter().map(|a| a * a).sum::<f64>().sqrt();
        if norm < 1e-12 {
            break;
        }
        v = next.iter().map(|a| a / norm).collect();
    }
    v
}

/// The stress of a placement: Σ over pairs of (1/d²)(‖xᵢ−xⱼ‖ − L·dᵢⱼ)².
fn stress(x: &[(f64, f64)], d: &[f64], length: f64, pad: &[f64]) -> f64 {
    let n = x.len();
    let mut total = 0.;
    for i in 0..n {
        for j in i + 1..n {
            let dij = d[i * n + j];
            let norm = ((x[i].0 - x[j].0).powi(2) + (x[i].1 - x[j].1).powi(2)).sqrt();
            total += (norm - length * dij - pair_pad(pad, i, j)).powi(2) / (dij * dij);
        }
    }
    total
}

/// The targets of the sparse stress: a child's graph neighbours at distance 1 and its distance to every pivot. A pivot has a
/// target for every child, so the pairs that are not listed are the remote pairs between two non-pivots. The pivots stand for
/// them, as in Ortmann, Klimenta and Brandes (GD 2016).
struct SparseTargets {
    pivots: Vec<usize>,
    /// `pivot_dist[q][i]`: the hop distance from pivot `q` to child `i`, with unreachable pairs one more than the largest.
    pivot_dist: Vec<Vec<f64>>,
    /// Per child, `(j, d_ij)` in ascending `j`. Symmetric: `j` lists `i` exactly when `i` lists `j`.
    neighbours: Vec<Vec<(usize, f64)>>,
}

/// Breadth-first hop distances from one source; unreachable children stay infinite.
fn hop_distances(adjacent: &[Vec<usize>], source: usize) -> Vec<f64> {
    let mut d = vec![f64::INFINITY; adjacent.len()];
    d[source] = 0.;
    let mut queue = VecDeque::from([source]);
    while let Some(v) = queue.pop_front() {
        for &w in &adjacent[v] {
            if d[w].is_infinite() {
                d[w] = d[v] + 1.;
                queue.push_back(w);
            }
        }
    }
    d
}

/// The sparse targets of one area: `SPARSE_PIVOTS` farthest-point pivots (the first is child 0), one breadth-first search each.
fn sparse_targets(ids: &[String], links: &BTreeSet<(String, String)>) -> SparseTargets {
    let n = ids.len();
    let index: BTreeMap<&str, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let mut adjacent = vec![Vec::new(); n];
    for (a, b) in links {
        let (i, j) = (index[a.as_str()], index[b.as_str()]);
        adjacent[i].push(j);
        adjacent[j].push(i);
    }
    let k = n.min(SPARSE_PIVOTS);
    let mut pivots = Vec::with_capacity(k);
    let mut pivot_dist: Vec<Vec<f64>> = Vec::with_capacity(k);
    let mut nearest = vec![f64::INFINITY; n];
    let mut next = (k > 0).then_some(0);
    while let Some(p) = next {
        pivots.push(p);
        let dist = hop_distances(&adjacent, p);
        for (near, &d) in nearest.iter_mut().zip(&dist) {
            *near = near.min(d);
        }
        pivot_dist.push(dist);
        next = if pivots.len() < k {
            (0..n)
                .filter(|i| !pivots.contains(i))
                .fold(None, |best: Option<usize>, i| match best {
                    Some(b) if nearest[b] >= nearest[i] => Some(b),
                    _ => Some(i),
                })
        } else {
            None
        };
    }
    // Unreachable pairs get one more than the largest finite distance, as in the dense matrix.
    let largest = pivot_dist
        .iter()
        .flatten()
        .copied()
        .filter(|d| d.is_finite())
        .fold(1f64, f64::max);
    for dist in &mut pivot_dist {
        for d in dist.iter_mut().filter(|d| d.is_infinite()) {
            *d = largest + 1.;
        }
    }
    let mut lists: Vec<BTreeMap<usize, f64>> = vec![BTreeMap::new(); n];
    for (i, adj) in adjacent.iter().enumerate() {
        for &j in adj {
            lists[i].insert(j, 1.);
        }
    }
    for (q, &p) in pivots.iter().enumerate() {
        for i in (0..n).filter(|&i| i != p) {
            lists[i].insert(p, pivot_dist[q][i]);
            lists[p].insert(i, pivot_dist[q][i]);
        }
    }
    SparseTargets {
        pivots,
        pivot_dist,
        neighbours: lists
            .into_iter()
            .map(|list| list.into_iter().collect())
            .collect(),
    }
}

/// The sparse stress of a placement: the terms of `stress` over the pairs listed in `targets`, each pair once.
fn sparse_stress(x: &[(f64, f64)], targets: &SparseTargets, length: f64, pad: &[f64]) -> f64 {
    let mut total = 0.;
    for (i, list) in targets.neighbours.iter().enumerate() {
        for &(j, dij) in list.iter().filter(|(j, _)| *j > i) {
            let norm = ((x[i].0 - x[j].0).powi(2) + (x[i].1 - x[j].1).powi(2)).sqrt();
            total += (norm - length * dij - pair_pad(pad, i, j)).powi(2) / (dij * dij);
        }
    }
    total
}

/// The extra target length of a pair under the size-aware stress: the sum of the two children's `pad` (their half-extents).
/// Zero when the stress is not size-aware, so the default targets are unchanged.
fn pair_pad(pad: &[f64], i: usize, j: usize) -> f64 {
    if pad.is_empty() {
        0.
    } else {
        pad[i] + pad[j]
    }
}

/// The position that child `i` moves to in one majorization step: the average of the positions its targets ask for, weighted
/// by 1/d². `None` when there are no targets.
fn majorized_position(
    x: &[(f64, f64)],
    i: usize,
    targets: impl Iterator<Item = (usize, f64)>,
    length: f64,
    pad: &[f64],
) -> Option<(f64, f64)> {
    let (mut sx, mut sy, mut total) = (0., 0., 0.);
    for (j, dij) in targets {
        let w = 1. / (dij * dij);
        let target = length * dij + pair_pad(pad, i, j);
        let (dx, dy) = (x[i].0 - x[j].0, x[i].1 - x[j].1);
        let norm = (dx * dx + dy * dy).sqrt().max(1e-9);
        sx += w * (x[j].0 + target * dx / norm);
        sy += w * (x[j].1 + target * dy / norm);
        total += w;
    }
    (total > 0.).then(|| (sx / total, sy / total))
}

/// The scalable placement (`scalableStress`) from the pivot initialization in `x`; returns the steps it took.
///
/// 1. Weighted sparse stress. Each child moves towards its graph neighbours (target 1, weight 1) and towards every pivot. A
///    pivot term stands for the remote pairs it replaces: its weight is `s / d²`, where `s` counts the children of the pivot's
///    region (the children nearest to it) that lie at most half the distance from the pivot (Ortmann, Klimenta and Brandes,
///    "A sparse stress model", GD 2016). Without that weight the remote pairs have almost no pull, and remote children fall
///    onto each other. Stops when the stress improves by less than 1e-5 relative or after `iterations` steps.
/// 2. A force refinement (`force_refine`): the stress at the node spacing crowds graphs whose neighbourhoods grow fast (trees
///    with hubs), and a separation afterwards tears the drawing apart; springs, repulsion and box collision spread the
///    crowded regions from the stress layout instead.
///
/// Every step is a Jacobi step in index order, so the result is deterministic. Cost per step: O(n·(pivots + degree)).
fn scalable_rounds(
    x: &mut [(f64, f64)],
    targets: &SparseTargets,
    extent: &[(f64, f64)],
    fixed: &[bool],
    length: f64,
    iterations: usize,
) -> usize {
    let n = x.len();
    let k = targets.pivots.len();
    // The region of every child: its nearest pivot (the first on ties).
    let region: Vec<usize> = (0..n)
        .map(|i| {
            (0..k)
                .min_by(|&a, &b| {
                    targets.pivot_dist[a][i]
                        .total_cmp(&targets.pivot_dist[b][i])
                        .then(a.cmp(&b))
                })
                .unwrap_or(0)
        })
        .collect();
    // Per pivot, the sorted distances of its region's children to it.
    let mut members: Vec<Vec<f64>> = vec![Vec::new(); k];
    for i in 0..n {
        if k > 0 {
            members[region[i]].push(targets.pivot_dist[region[i]][i]);
        }
    }
    for list in &mut members {
        list.sort_by(f64::total_cmp);
    }
    // The terms of every child: (other, distance, weight), in ascending `other`.
    let pivot_of: BTreeMap<usize, usize> = targets
        .pivots
        .iter()
        .enumerate()
        .map(|(q, &p)| (p, q))
        .collect();
    let terms: Vec<Vec<(usize, f64, f64)>> = (0..n)
        .map(|i| {
            let mut list: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
            for &(j, dij) in &targets.neighbours[i] {
                if dij == 1. {
                    list.insert(j, (1., 1.));
                }
            }
            // Children two steps apart (siblings of one parent, above all): without this term they have the same targets and
            // fall onto one point.
            for &(j, dij) in &targets.neighbours[i] {
                if dij != 1. {
                    continue;
                }
                for &(m, dm) in &targets.neighbours[j] {
                    if dm == 1. && m != i {
                        list.entry(m).or_insert((2., 0.25));
                    }
                }
            }
            for (&p, &q) in &pivot_of {
                if p == i || list.contains_key(&p) {
                    continue;
                }
                let d = targets.pivot_dist[q][i];
                let half = d / 2.;
                let s = members[q].partition_point(|&m| m <= half).max(1) as f64;
                list.insert(p, (d, s / (d * d)));
            }
            list.into_iter().map(|(j, (d, w))| (j, d, w)).collect()
        })
        .collect();
    let stress_of = |x: &[(f64, f64)]| {
        let mut total = 0.;
        for (i, list) in terms.iter().enumerate() {
            for &(j, d, w) in list {
                let norm = ((x[i].0 - x[j].0).powi(2) + (x[i].1 - x[j].1).powi(2)).sqrt();
                total += w * (norm - length * d).powi(2);
            }
        }
        total
    };
    let step = |x: &[(f64, f64)]| -> Vec<(f64, f64)> {
        (0..n)
            .map(|i| {
                if fixed[i] {
                    return x[i];
                }
                let (mut sx, mut sy, mut total) = (0., 0., 0.);
                for &(j, d, w) in &terms[i] {
                    let (dx, dy) = (x[i].0 - x[j].0, x[i].1 - x[j].1);
                    let norm = (dx * dx + dy * dy).sqrt().max(1e-9);
                    let target = length * d;
                    sx += w * (x[j].0 + target * dx / norm);
                    sy += w * (x[j].1 + target * dy / norm);
                    total += w;
                }
                if total > 0. {
                    (sx / total, sy / total)
                } else {
                    x[i]
                }
            })
            .collect()
    };
    let mut used = 0;
    let mut previous = stress_of(x);
    for _ in 0..iterations {
        used += 1;
        let next = step(x);
        x.copy_from_slice(&next);
        let current = stress_of(x);
        if previous - current <= 1e-5 * previous.max(1e-12) {
            break;
        }
        previous = current;
    }
    let edges: Vec<(usize, usize)> = (0..n)
        .flat_map(|i| {
            targets.neighbours[i]
                .iter()
                .filter(move |&&(j, d)| d == 1. && j > i)
                .map(move |&(j, _)| (i, j))
        })
        .collect();
    used += force_refine(x, &edges, extent, fixed, length);
    used
}

/// The ticks of the force refinement of the scalable placement, and its starting temperature (d3-force starts at 1; the stress
/// layout is already close, so the refinement starts cooler).
const FORCE_TICKS: usize = 300;
const FORCE_ALPHA: f64 = 0.5;

/// A force refinement in the manner of d3-force (springs, many-body repulsion, collision; the model is reimplemented here and
/// no code is copied): springs on the edges, a many-body repulsion by a Barnes–Hut quadtree, and a collision of the boxes with
/// the gap `GAP`, with velocities that decay and a temperature that cools over `FORCE_TICKS` ticks. The spring length and the
/// repulsion follow the node spacing `length`. Starting from the stress layout, it keeps the global shape and spreads crowded
/// regions until the boxes fit. Deterministic: every pass runs in index order, and coincident points are skipped, not
/// jiggled. Returns the ticks taken.
fn force_refine(
    x: &mut [(f64, f64)],
    edges: &[(usize, usize)],
    extent: &[(f64, f64)],
    fixed: &[bool],
    length: f64,
) -> usize {
    let n = x.len();
    if n < 2 {
        return 0;
    }
    let distance = length * std::f64::consts::SQRT_2;
    let charge = -distance * distance / 44.;
    let mut degree = vec![0usize; n];
    for &(a, b) in edges {
        degree[a] += 1;
        degree[b] += 1;
    }
    let mut v = vec![(0f64, 0f64); n];
    let decay = 1. - 0.001f64.powf(1. / FORCE_TICKS as f64);
    let mut alpha = FORCE_ALPHA;
    let max_extent = extent.iter().map(|(w, h)| w.max(*h)).fold(0., f64::max);
    for tick in 0..FORCE_TICKS {
        alpha -= alpha * decay;
        // Springs (as d3's link force: strength 1 / smaller degree, biased towards the endpoint of smaller degree).
        for &(a, b) in edges {
            let dx = x[b].0 + v[b].0 - x[a].0 - v[a].0;
            let dy = x[b].1 + v[b].1 - x[a].1 - v[a].1;
            let l = (dx * dx + dy * dy).sqrt();
            if l == 0. {
                continue;
            }
            let strength = 1. / degree[a].min(degree[b]).max(1) as f64;
            let k = (l - distance) / l * alpha * strength;
            let (fx, fy) = (dx * k, dy * k);
            let bias = degree[a] as f64 / (degree[a] + degree[b]) as f64;
            v[b].0 -= fx * bias;
            v[b].1 -= fy * bias;
            v[a].0 += fx * (1. - bias);
            v[a].1 += fy * (1. - bias);
        }
        // Many-body repulsion.
        let tree = QuadTree::new(x);
        for (i, vi) in v.iter_mut().enumerate() {
            let (fx, fy) = tree.force(x, i, charge * alpha);
            vi.0 += fx;
            vi.1 += fy;
        }
        // Collision of the boxes (with the gap) on the predicted positions: part of the overlap each, along the smaller axis.
        let predicted: Vec<(f64, f64)> =
            (0..n).map(|i| (x[i].0 + v[i].0, x[i].1 + v[i].1)).collect();
        let spatial = SpatialHash::new(&predicted, (max_extent + GAP).max(1.));
        let mut near = Vec::new();
        for i in 0..n {
            // The candidates after `i` within one cell on each axis, in index order (the hash does not change in this pass).
            spatial.neighbors_after(predicted[i], i, &mut near);
            for &j in &near {
                let dx = predicted[j].0 - predicted[i].0;
                let dy = predicted[j].1 - predicted[i].1;
                let ox = (extent[i].0 + extent[j].0) / 2. + GAP - dx.abs();
                let oy = (extent[i].1 + extent[j].1) / 2. + GAP - dy.abs();
                if ox <= 0. || oy <= 0. {
                    continue;
                }
                let (si, sj) = match (fixed[i], fixed[j]) {
                    (true, true) => continue,
                    (true, false) => (0., 1.),
                    (false, true) => (1., 0.),
                    (false, false) => (0.5, 0.5),
                };
                if ox <= oy {
                    let sign = if dx < 0. || (dx == 0. && i > j) {
                        -1.
                    } else {
                        1.
                    };
                    v[i].0 -= sign * ox * si * COLLIDE_STRENGTH;
                    v[j].0 += sign * ox * sj * COLLIDE_STRENGTH;
                } else {
                    let sign = if dy < 0. || (dy == 0. && i > j) {
                        -1.
                    } else {
                        1.
                    };
                    v[i].1 -= sign * oy * si * COLLIDE_STRENGTH;
                    v[j].1 += sign * oy * sj * COLLIDE_STRENGTH;
                }
            }
        }
        if tick >= FORCE_TICKS - EDGE_TICKS {
            edge_push(x, &mut v, edges, extent, fixed, max_extent);
        }
        for i in 0..n {
            if fixed[i] {
                v[i] = (0., 0.);
                continue;
            }
            v[i].0 *= 1. - VELOCITY_DECAY;
            v[i].1 *= 1. - VELOCITY_DECAY;
            x[i].0 += v[i].0;
            x[i].1 += v[i].1;
        }
    }
    FORCE_TICKS
}

/// The last ticks of the force refinement, in which a node whose box (with half the gap) a straight edge crosses is pushed off
/// that edge, and the edge's ends the other way.
const EDGE_TICKS: usize = 150;
/// The share of the way off an edge that one tick moves the node (the edge's ends move a quarter of it each).
const EDGE_PUSH: f64 = 0.5;

/// One pass of the edge push of `force_refine`, on the predicted positions (`x + v`), in edge order, the candidates found by
/// a spatial hash over the box centres.
fn edge_push(
    x: &[(f64, f64)],
    v: &mut [(f64, f64)],
    edges: &[(usize, usize)],
    extent: &[(f64, f64)],
    fixed: &[bool],
    max_extent: f64,
) {
    let n = x.len();
    let predicted: Vec<(f64, f64)> = (0..n).map(|i| (x[i].0 + v[i].0, x[i].1 + v[i].1)).collect();
    let cell = (max_extent + GAP).max(1.);
    let spatial = SpatialHash::new(&predicted, cell);
    let reach = max_extent / 2. + GAP;
    let mut candidates = Vec::new();
    for &(a, b) in edges {
        let (p, q) = (predicted[a], predicted[b]);
        let min = (p.0.min(q.0) - reach, p.1.min(q.1) - reach);
        let max = (p.0.max(q.0) + reach, p.1.max(q.1) + reach);
        spatial.query_bounds(min, max, &mut candidates);
        let (dx, dy) = (q.0 - p.0, q.1 - p.1);
        let length2 = dx * dx + dy * dy;
        if length2 == 0. {
            continue;
        }
        for &k in &candidates {
            if k == a || k == b || fixed[k] && fixed[a] && fixed[b] {
                continue;
            }
            let c = predicted[k];
            let (hw, hh) = (extent[k].0 / 2. + GAP / 2., extent[k].1 / 2. + GAP / 2.);
            if !segment_hits_box(p, q, c, hw, hh) {
                continue;
            }
            // The closest point of the edge to the centre, and the way from it to the centre (perpendicular when the
            // centre lies on the edge).
            let t = (((c.0 - p.0) * dx + (c.1 - p.1) * dy) / length2).clamp(0., 1.);
            let closest = (p.0 + t * dx, p.1 + t * dy);
            let (mut nx, mut ny) = (c.0 - closest.0, c.1 - closest.1);
            let mut distance = (nx * nx + ny * ny).sqrt();
            if distance < 1e-9 {
                let length = length2.sqrt();
                (nx, ny, distance) = (-dy / length, dx / length, 0.);
            } else {
                nx /= distance;
                ny /= distance;
            }
            // How far the box reaches towards the edge along that way, less the distance it has.
            let depth = hw * nx.abs() + hh * ny.abs() - distance;
            if depth <= 0. {
                continue;
            }
            let step = depth * EDGE_PUSH;
            let (share_node, share_end) = match (fixed[k], fixed[a] && fixed[b]) {
                (true, _) => (0., 0.5),
                (false, true) => (1., 0.),
                (false, false) => (1., 0.25),
            };
            v[k].0 += nx * step * share_node;
            v[k].1 += ny * step * share_node;
            for end in [a, b] {
                if !fixed[end] {
                    v[end].0 -= nx * step * share_end;
                    v[end].1 -= ny * step * share_end;
                }
            }
        }
    }
}

/// Whether the segment p–q enters the box around `c` with half extents `hw`, `hh` (Liang–Barsky clipping).
fn segment_hits_box(p: (f64, f64), q: (f64, f64), c: (f64, f64), hw: f64, hh: f64) -> bool {
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let (mut t0, mut t1) = (0f64, 1f64);
    for (dp, dq) in [
        (-dx, p.0 - (c.0 - hw)),
        (dx, (c.0 + hw) - p.0),
        (-dy, p.1 - (c.1 - hh)),
        (dy, (c.1 + hh) - p.1),
    ] {
        if dp == 0. {
            if dq < 0. {
                return false;
            }
        } else {
            let r = dq / dp;
            if dp < 0. {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
        }
    }
    t0 < t1
}

/// The share of a box overlap that one tick of the collision removes, and the share of the velocity lost per tick (d3: 0.4).
const COLLIDE_STRENGTH: f64 = 0.7;
const VELOCITY_DECAY: f64 = 0.4;

/// A Barnes–Hut quadtree over points: every cell holds its mass (the point count, or the sum of the points' masses) and its
/// centre of mass.
pub(crate) struct QuadTree {
    cells: Vec<QuadCell>,
}

struct QuadCell {
    /// The cell's square: left, top, side.
    x: f64,
    y: f64,
    side: f64,
    count: f64,
    cx: f64,
    cy: f64,
    /// The four children (all 0 for a leaf), and the point of a leaf.
    children: [usize; 4],
    point: Option<usize>,
}

impl QuadTree {
    fn new(points: &[(f64, f64)]) -> Self {
        Self::build(points, None)
    }

    /// A quadtree whose points have the masses `mass` (the overview weights a cluster by its size). With every mass 1 it is
    /// the tree of `new`, in the same arithmetic.
    pub(crate) fn new_weighted(points: &[(f64, f64)], mass: &[f64]) -> Self {
        Self::build(points, Some(mass))
    }

    fn build(points: &[(f64, f64)], mass: Option<&[f64]>) -> Self {
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for p in points {
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0);
            y1 = y1.max(p.1);
        }
        let side = (x1 - x0).max(y1 - y0).max(1.) * (1. + 1e-9);
        let mut tree = Self {
            cells: vec![QuadCell::new(x0, y0, side)],
        };
        for i in 0..points.len() {
            tree.insert(i, points, mass.map_or(1., |m| m[i]));
        }
        tree.aggregate(0, points);
        tree
    }

    fn insert(&mut self, i: usize, points: &[(f64, f64)], mass: f64) {
        let mut cell = 0;
        let mut depth = 0;
        loop {
            if self.cells[cell].children == [0; 4] {
                match self.cells[cell].point {
                    None => {
                        self.cells[cell].point = Some(i);
                        self.cells[cell].count = mass;
                        return;
                    }
                    // Coincident points (or too deep to tell apart) share the leaf, counted in its mass.
                    Some(_) if depth > 48 => {
                        self.cells[cell].count += mass;
                        return;
                    }
                    Some(other) => {
                        let count = self.cells[cell].count;
                        self.cells[cell].point = None;
                        self.cells[cell].count = 0.;
                        self.split(cell);
                        let q = self.quadrant(cell, points[other]);
                        let child = self.cells[cell].children[q];
                        self.cells[child].point = Some(other);
                        self.cells[child].count = count;
                    }
                }
            }
            let q = self.quadrant(cell, points[i]);
            cell = self.cells[cell].children[q];
            depth += 1;
        }
    }

    fn quadrant(&self, cell: usize, p: (f64, f64)) -> usize {
        let c = &self.cells[cell];
        let half = c.side / 2.;
        usize::from(p.0 >= c.x + half) + 2 * usize::from(p.1 >= c.y + half)
    }

    fn split(&mut self, cell: usize) {
        let (x, y, half) = {
            let c = &self.cells[cell];
            (c.x, c.y, c.side / 2.)
        };
        for q in 0..4 {
            let index = self.cells.len();
            self.cells.push(QuadCell::new(
                x + if q & 1 == 1 { half } else { 0. },
                y + if q & 2 == 2 { half } else { 0. },
                half,
            ));
            self.cells[cell].children[q] = index;
        }
    }

    /// The count and the centre of mass of every cell, children first.
    fn aggregate(&mut self, cell: usize, points: &[(f64, f64)]) -> f64 {
        let children = self.cells[cell].children;
        let (count, sx, sy) = if children == [0; 4] {
            match self.cells[cell].point {
                Some(i) => {
                    let c = self.cells[cell].count;
                    (c, points[i].0 * c, points[i].1 * c)
                }
                None => (0., 0., 0.),
            }
        } else {
            let mut total = (0., 0., 0.);
            for child in children {
                let c = self.aggregate(child, points);
                let (cx, cy) = (self.cells[child].cx, self.cells[child].cy);
                total = (total.0 + c, total.1 + cx * c, total.2 + cy * c);
            }
            total
        };
        let c = &mut self.cells[cell];
        c.count = count;
        if count > 0. {
            c.cx = sx / count;
            c.cy = sy / count;
        }
        count
    }

    /// The repulsion on point `i`: `strength · count / l²` along the direction to the cell (a negative strength repels). A
    /// cell counts as one body while its side is less than 0.9 of its distance (d3's θ) and it does not hold the point.
    pub(crate) fn force(&self, points: &[(f64, f64)], i: usize, strength: f64) -> (f64, f64) {
        let mut stack = vec![0usize];
        let (mut fx, mut fy) = (0., 0.);
        let p = points[i];
        while let Some(cell) = stack.pop() {
            let c = &self.cells[cell];
            if c.count == 0. {
                continue;
            }
            let (dx, dy) = (c.cx - p.0, c.cy - p.1);
            let l2 = dx * dx + dy * dy;
            let leaf = c.children == [0; 4];
            // A cell that holds the point itself is never one body: the point would push itself.
            let holds = p.0 >= c.x && p.0 <= c.x + c.side && p.1 >= c.y && p.1 <= c.y + c.side;
            if leaf || (!holds && c.side * c.side / 0.81 < l2) {
                if l2 == 0. {
                    continue;
                }
                let w = strength * c.count / l2.max(1.);
                fx += dx * w;
                fy += dy * w;
            } else {
                for &child in c.children.iter().rev() {
                    stack.push(child);
                }
            }
        }
        (fx, fy)
    }
}

impl QuadCell {
    fn new(x: f64, y: f64, side: f64) -> Self {
        Self {
            x,
            y,
            side,
            count: 0.,
            cx: 0.,
            cy: 0.,
            children: [0; 4],
            point: None,
        }
    }
}

/// The share of the way to the relaxed positions that a force-directed step takes, and the farthest it may go, in node spacings.
const FORCE_DAMPING: f64 = 0.3;
const FORCE_MAX_STEP: f64 = 0.25;

fn separate_overlaps(positions: &mut [(f64, f64)], extent: &[(f64, f64)], pinned: &[bool]) {
    separate_overlaps_pairwise(positions, extent, pinned);
}

fn separate_overlaps_with_option(
    positions: &mut [(f64, f64)],
    extent: &[(f64, f64)],
    pinned: &[bool],
    use_vpsc: bool,
) {
    if use_vpsc && separate_overlaps_vpsc(positions, extent, pinned) {
        return;
    }
    separate_overlaps(positions, extent, pinned);
}

fn separate_overlaps_vpsc(
    positions: &mut [(f64, f64)],
    extent: &[(f64, f64)],
    pinned: &[bool],
) -> bool {
    if positions.len() != extent.len() || positions.len() != pinned.len() {
        return false;
    }
    let targets = positions.to_vec();
    let bounds = vec![vpsc::PositionBounds::default(); positions.len()];
    let movable: Vec<bool> = pinned.iter().map(|&is_pinned| !is_pinned).collect();
    let mut current = targets.clone();
    let mut constraints = BTreeMap::new();
    for _ in 0..vpsc::MAX_CONSTRAINT_ROUNDS {
        let Some(pairs) = vpsc::scanline_pairs(&current, extent, GAP) else {
            return false;
        };
        if pairs.is_empty() {
            positions.copy_from_slice(&current);
            return true;
        }
        let previous_count = constraints.len();
        for (a, b) in pairs {
            constraints.entry((a, b)).or_insert_with(|| {
                vpsc::constraint_for_pair(a, b, &targets, &current, extent, GAP)
            });
        }
        if constraints.len() == previous_count || constraints.values().any(Option::is_none) {
            return false;
        }
        let separations: Vec<_> = constraints.values().filter_map(|c| *c).collect();
        let Some(next) = vpsc::solve(&targets, &movable, &bounds, &separations) else {
            return false;
        };
        if next == current {
            return false;
        }
        current = next;
    }
    if vpsc::scanline_pairs(&current, extent, GAP).is_some_and(|pairs| pairs.is_empty()) {
        positions.copy_from_slice(&current);
        true
    } else {
        false
    }
}

fn separate_overlaps_pairwise(
    positions: &mut [(f64, f64)],
    extent: &[(f64, f64)],
    pinned: &[bool],
) {
    let max_extent = extent
        .iter()
        .map(|(width, height)| (*width).max(*height))
        .fold(0., f64::max);
    let mut spatial = SpatialHash::new(positions, (max_extent + GAP).max(1.));
    for _ in 0..200 {
        let mut any = false;
        for i in 0..positions.len() {
            let mut next = i + 1;
            while let Some(j) = spatial.next_neighbor(positions[i], next, i) {
                next = j + 1;
                let (fi, fj) = (pinned[i], pinned[j]);
                if fi && fj {
                    continue;
                }
                let (share_i, share_j) = match (fi, fj) {
                    (true, _) => (0., 1.),
                    (_, true) => (1., 0.),
                    _ => (0.5, 0.5),
                };
                let (wi, hi) = extent[i];
                let (wj, hj) = extent[j];
                let dx = positions[j].0 - positions[i].0;
                let dy = positions[j].1 - positions[i].1;
                let ox = (wi + wj) / 2. + GAP - dx.abs();
                let oy = (hi + hj) / 2. + GAP - dy.abs();
                if ox > 0. && oy > 0. {
                    any = true;
                    if ox <= oy {
                        let sign = if dx < 0. || (dx == 0. && i > j) {
                            -1.
                        } else {
                            1.
                        };
                        positions[i].0 -= sign * ox * share_i;
                        positions[j].0 += sign * ox * share_j;
                    } else {
                        let sign = if dy < 0. || (dy == 0. && i > j) {
                            -1.
                        } else {
                            1.
                        };
                        positions[i].1 -= sign * oy * share_i;
                        positions[j].1 += sign * oy * share_j;
                    }
                    spatial.update(i, positions[i]);
                    spatial.update(j, positions[j]);
                }
            }
        }
        if !any {
            break;
        }
    }
}

/// Bounded end-of-stress correction on projected centre chords. Exact port/shape repair follows later.
/// Uses the spatial broad phase; fixed nodes and self-loops are left to real-geometry repair.
fn repel_node_edges(
    positions: &mut [(f64, f64)],
    edges: &[(usize, usize)],
    extents: &[(f64, f64)],
    pinned: &[bool],
    max_step: f64,
) {
    let padding = extents.iter().map(|(w, h)| w.max(*h)).fold(0., f64::max) + GAP;
    let mut spatial = SpatialHash::new(positions, padding.max(1.));
    let mut candidates = Vec::new();
    let mut delta = vec![(0., 0.); positions.len()];
    for _ in 0..8 {
        delta.fill((0., 0.));
        for &(a, b) in edges {
            if a == b {
                continue;
            }
            let (p, q) = (positions[a], positions[b]);
            spatial.query_bounds(
                (p.0.min(q.0) - padding, p.1.min(q.1) - padding),
                (p.0.max(q.0) + padding, p.1.max(q.1) + padding),
                &mut candidates,
            );
            for &k in &candidates {
                if k == a || k == b || pinned[k] {
                    continue;
                }
                let (push, _) = crate::directed::segment_push(positions[k], p, q, extents[k], GAP);
                delta[k].0 += push.0;
                delta[k].1 += push.1;
            }
        }
        let mut changed = false;
        for k in 0..positions.len() {
            let length = delta[k].0.hypot(delta[k].1);
            if !pinned[k] && length > 1e-9 && length.is_finite() {
                let scale = (max_step / length).min(0.5);
                positions[k].0 += delta[k].0 * scale;
                positions[k].1 += delta[k].1 * scale;
                spatial.update(k, positions[k]);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Runs the three stages for the children of one area. `start` gives the centres of a warm start; it replaces the pivot
/// initialization when it names every child.
#[cfg(test)]
pub fn organic_stages(
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    sizes: &BTreeMap<String, (f64, f64)>,
    start: Option<&BTreeMap<String, (f64, f64)>>,
    iterations: usize,
    fixed: &BTreeMap<String, (f64, f64)>,
    relax: usize,
) -> Stages {
    organic_stages_with_vpsc(
        ids, links, sizes, start, iterations, fixed, relax, false, false, false, false,
    )
}

#[allow(clippy::too_many_arguments)]
#[cfg_attr(not(test), allow(dead_code))]
fn organic_stages_with_vpsc(
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    sizes: &BTreeMap<String, (f64, f64)>,
    start: Option<&BTreeMap<String, (f64, f64)>>,
    iterations: usize,
    fixed: &BTreeMap<String, (f64, f64)>,
    relax: usize,
    use_vpsc: bool,
    repel_edges: bool,
    sparse: bool,
    size_aware: bool,
) -> Stages {
    organic_stages_full(
        ids,
        links,
        sizes,
        start,
        iterations,
        fixed,
        relax,
        use_vpsc,
        repel_edges,
        sparse,
        size_aware,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn organic_stages_full(
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    sizes: &BTreeMap<String, (f64, f64)>,
    start: Option<&BTreeMap<String, (f64, f64)>>,
    iterations: usize,
    fixed: &BTreeMap<String, (f64, f64)>,
    relax: usize,
    use_vpsc: bool,
    repel_edges: bool,
    sparse: bool,
    size_aware: bool,
    scalable: bool,
) -> Stages {
    // The scalable placement takes the sparse targets (pivots and neighbours) and weights them itself.
    let sparse = sparse || scalable;
    let n = ids.len();
    // The size-aware targets add each child's half-extent (a quarter of its width plus height) to both ends of a pair.
    let pad: Vec<f64> = if size_aware {
        ids.iter()
            .map(|id| (sizes[id].0 + sizes[id].1) / 4.)
            .collect()
    } else {
        Vec::new()
    };
    let fixed_flags: Vec<bool> = ids.iter().map(|id| fixed.contains_key(id)).collect();
    // A warm start that holds the positions needs neither the graph distances nor the stress: the distances (a breadth-first
    // search from every child) are computed only when a stage uses them. The stress values are then 0.
    let warm_hold = start.is_some_and(|s| ids.iter().all(|id| s.contains_key(id)))
        && iterations.min(relax) == 0;
    let no_stress = warm_hold && relax == 0;
    // The sparse stress takes its targets from a few breadth-first searches; the dense stress takes the whole matrix.
    let sparse_targets = (sparse && !no_stress).then(|| sparse_targets(ids, links));
    let d = if no_stress || sparse {
        Rc::new(Vec::new())
    } else {
        distances(ids, links)
    };
    let stress_of = |x: &[(f64, f64)], length: f64| match (&sparse_targets, d.is_empty()) {
        (Some(targets), _) => sparse_stress(x, targets, length, &pad),
        (None, true) => 0.,
        (None, false) => stress(x, d.as_slice(), length, &pad),
    };
    // The typical centre-to-centre spacing of neighbours: the mean node size plus the gap.
    let length = if n == 0 {
        1.
    } else {
        ids.iter()
            .map(|id| (sizes[id].0 + sizes[id].1) / 2. + GAP)
            .sum::<f64>()
            / n as f64
    };
    // A warm start names every child. It keeps the positions as they are, runs no stress iterations and does not compute
    // the pivot initialization again: a change reaches the neighbours through the pins, the separation and the repair, and
    // unchanged nodes do not drift. Without a complete start the stress runs from the pivot initialization as before.
    let warm = start.filter(|s| ids.iter().all(|id| s.contains_key(id)));
    // With `relax` steps a warm start is not held: the stress is relaxed from the positions for that many steps, which is the
    // force-directed behaviour (pinned children stay, the others move toward their targets).
    let iterations = if warm.is_some() { relax } else { iterations };
    let mut x: Vec<(f64, f64)> = match warm {
        Some(start) => ids.iter().map(|id| start[id]).collect(),
        None => {
            let coords = match &sparse_targets {
                Some(targets) => {
                    pivot_coordinates(n, &targets.pivots, |i, q| targets.pivot_dist[q][i])
                }
                None => pivot_mds(d.as_slice(), n),
            };
            coords
                .into_iter()
                .map(|(a, b)| (a * length, b * length))
                .collect()
        }
    };
    if warm.is_none() && !fixed.is_empty() {
        // Free children start around the centre of the pins, so that they are not placed on the far side of them.
        let (cx, cy) = fixed
            .values()
            .fold((0., 0.), |(sx, sy), p| (sx + p.0, sy + p.1));
        let count = fixed.len() as f64;
        for p in x.iter_mut() {
            *p = (p.0 + cx / count, p.1 + cy / count);
        }
    }
    // A pinned child keeps its centre in every step below.
    for (i, id) in ids.iter().enumerate() {
        if fixed_flags[i] {
            x[i] = fixed[id];
        }
    }
    let initial = x.clone();
    let stress_started = crate::stage_timer_start();
    let stress_initial = stress_of(&x, length);
    // Stress majorization: each step moves every node to the weighted average of the positions its targets ask for.
    let mut previous = stress_initial;
    let mut used = 0;
    let scalable_targets = sparse_targets
        .as_ref()
        .filter(|_| scalable && iterations > 0);
    if let Some(targets) = scalable_targets {
        let extent: Vec<(f64, f64)> = ids.iter().map(|id| sizes[id]).collect();
        used = scalable_rounds(&mut x, targets, &extent, &fixed_flags, length, iterations);
    }
    let iterations = if scalable_targets.is_some() {
        0
    } else {
        iterations
    };
    for _ in 0..iterations {
        used += 1;
        let mut next = x.clone();
        for i in 0..n {
            if fixed_flags[i] {
                continue;
            }
            let moved = match &sparse_targets {
                Some(targets) => {
                    majorized_position(&x, i, targets.neighbours[i].iter().copied(), length, &pad)
                }
                None => majorized_position(
                    &x,
                    i,
                    (0..n).filter(|&j| j != i).map(|j| (j, d[i * n + j])),
                    length,
                    &pad,
                ),
            };
            if let Some(position) = moved {
                next[i] = position;
            }
        }
        x = next;
        let current = stress_of(&x, length);
        if previous - current <= 1e-7 * previous.max(1e-12) {
            break;
        }
        previous = current;
    }
    if repel_edges && iterations > 0 {
        let index: BTreeMap<&str, usize> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i))
            .collect();
        let edges: Vec<(usize, usize)> = links
            .iter()
            .map(|(a, b)| (index[a.as_str()], index[b.as_str()]))
            .collect();
        let extents: Vec<_> = ids.iter().map(|id| sizes[id]).collect();
        repel_node_edges(&mut x, &edges, &extents, &fixed_flags, length * 0.1);
    }
    // A force-directed continuation moves toward the relaxed positions by a fraction, and by a bounded distance: each step of a
    // drag nudges the neighbours the way a spring would, instead of jumping to the equilibrium (which is far off after a big
    // drag and made the drawing jump and swim).
    if let (Some(start), true) = (warm, relax > 0) {
        for (i, id) in ids.iter().enumerate() {
            if fixed_flags[i] {
                continue;
            }
            let (dx, dy) = (x[i].0 - start[id].0, x[i].1 - start[id].1);
            let moved = dx.hypot(dy) * FORCE_DAMPING;
            let limit = length * FORCE_MAX_STEP;
            let scale = if moved > limit && moved > 0. {
                limit / (dx.hypot(dy))
            } else {
                FORCE_DAMPING
            };
            x[i] = (start[id].0 + dx * scale, start[id].1 + dy * scale);
        }
    }
    let stressed = x.clone();
    let stress_final = stress_of(&stressed, length);
    let stress_nanos = crate::elapsed_nanos(stress_started);
    // Separation by the real sizes, in index order. A pinned child takes no share of the move: its partner moves all of it.
    let separation_started = crate::stage_timer_start();
    let extent: Vec<(f64, f64)> = ids.iter().map(|id| sizes[id]).collect();
    separate_overlaps_with_option(&mut x, &extent, &fixed_flags, use_vpsc);
    let separation_nanos = crate::elapsed_nanos(separation_started);
    Stages {
        initial,
        stress: stressed,
        separated: x,
        stress_initial,
        stress_final,
        iterations: used,
        target_length: length,
        stress_nanos,
        separation_nanos,
    }
}

/// Places the children of one area: the three stages, and the boxes relative to the area with the area size as the other
/// arrangements do. With pins the frame is the area's own: pinned children keep their pinned places, and nothing is
/// shifted to the origin. The geometric repair of the root runs afterwards on the drawing (see `repair`).
#[cfg(test)]
pub fn organic_arrange(
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    sizes: &BTreeMap<String, (f64, f64)>,
    start: Option<&BTreeMap<String, (f64, f64)>>,
    iterations: usize,
    grid: f64,
    fixed: &BTreeMap<String, (f64, f64)>,
    relax: usize,
) -> (BTreeMap<String, Rect>, (f64, f64)) {
    organic_arrange_inner(
        ids, links, sizes, start, iterations, grid, fixed, relax, false, false, false, false,
        false, None,
    )
}

pub(crate) fn organic_arrange_counted(
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    sizes: &BTreeMap<String, (f64, f64)>,
    start: Option<&BTreeMap<String, (f64, f64)>>,
    iterations: usize,
    grid: f64,
    fixed: &BTreeMap<String, (f64, f64)>,
    relax: usize,
    use_vpsc: bool,
    repel_edges: bool,
    sparse: bool,
    size_aware: bool,
    scalable: bool,
    counters: &mut crate::SearchCounters,
) -> (BTreeMap<String, Rect>, (f64, f64)) {
    organic_arrange_inner(
        ids,
        links,
        sizes,
        start,
        iterations,
        grid,
        fixed,
        relax,
        use_vpsc,
        repel_edges,
        sparse,
        size_aware,
        scalable,
        Some(counters),
    )
}

fn organic_arrange_inner(
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    sizes: &BTreeMap<String, (f64, f64)>,
    start: Option<&BTreeMap<String, (f64, f64)>>,
    iterations: usize,
    grid: f64,
    fixed: &BTreeMap<String, (f64, f64)>,
    relax: usize,
    use_vpsc: bool,
    repel_edges: bool,
    sparse: bool,
    size_aware: bool,
    scalable: bool,
    mut counters: Option<&mut crate::SearchCounters>,
) -> (BTreeMap<String, Rect>, (f64, f64)) {
    let snap = |v: f64| {
        if grid > 0. {
            (v / grid).round() * grid
        } else {
            v
        }
    };
    if ids.is_empty() {
        return (BTreeMap::new(), (snap(48.), snap(72.)));
    }
    let stages = organic_stages_full(
        ids,
        links,
        sizes,
        start,
        iterations,
        fixed,
        relax,
        use_vpsc,
        repel_edges,
        sparse,
        size_aware,
        scalable,
    );
    if let Some(counters) = counters.as_deref_mut() {
        counters.organic_stress_nanos = counters
            .organic_stress_nanos
            .saturating_add(stages.stress_nanos);
        counters.separation_nanos = counters
            .separation_nanos
            .saturating_add(stages.separation_nanos);
    }
    let x = stages.separated;
    if !fixed.is_empty() {
        // The area keeps its frame: pinned children stay exactly where they are pinned.
        let mut placed = BTreeMap::new();
        for (i, id) in ids.iter().enumerate() {
            let (w, h) = sizes[id];
            let (left, top) = (x[i].0 - w / 2., x[i].1 - h / 2.);
            let (left, top) = if fixed.contains_key(id) {
                (left, top)
            } else {
                (snap(left), snap(top))
            };
            placed.insert(
                id.clone(),
                Rect {
                    x: left,
                    y: top,
                    width: w,
                    height: h,
                },
            );
        }
        let right = placed.values().map(|r| r.x + r.width).fold(0., f64::max);
        let bottom = placed.values().map(|r| r.y + r.height).fold(0., f64::max);
        return (placed, (snap(right + 24.), snap(bottom + 24.)));
    }
    let min_x = ids
        .iter()
        .enumerate()
        .map(|(i, id)| x[i].0 - sizes[id].0 / 2.)
        .fold(f64::INFINITY, f64::min);
    let min_y = ids
        .iter()
        .enumerate()
        .map(|(i, id)| x[i].1 - sizes[id].1 / 2.)
        .fold(f64::INFINITY, f64::min);
    let max_x = ids
        .iter()
        .enumerate()
        .map(|(i, id)| x[i].0 + sizes[id].0 / 2.)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = ids
        .iter()
        .enumerate()
        .map(|(i, id)| x[i].1 + sizes[id].1 / 2.)
        .fold(f64::NEG_INFINITY, f64::max);
    // A continuation keeps the frame of the earlier layout: nothing is translated unless a node would start before the margin.
    // Translating everything to the origin on each layout made the whole drawing jump when the node at the corner moved.
    if start.is_some_and(|s| ids.iter().all(|id| s.contains_key(id))) {
        let (shift_x, shift_y) = ((24. - min_x).max(0.), (48. - min_y).max(0.));
        let mut placed = BTreeMap::new();
        for (i, id) in ids.iter().enumerate() {
            let (w, h) = sizes[id];
            placed.insert(
                id.clone(),
                Rect {
                    x: snap(x[i].0 - w / 2. + shift_x),
                    y: snap(x[i].1 - h / 2. + shift_y),
                    width: w,
                    height: h,
                },
            );
        }
        return (
            placed,
            (snap(max_x + shift_x + 24.), snap(max_y + shift_y + 24.)),
        );
    }
    let mut placed = BTreeMap::new();
    for (i, id) in ids.iter().enumerate() {
        let (w, h) = sizes[id];
        placed.insert(
            id.clone(),
            Rect {
                x: snap(x[i].0 - w / 2. - min_x + 24.),
                y: snap(x[i].1 - h / 2. - min_y + 48.),
                width: w,
                height: h,
            },
        );
    }
    (
        placed,
        (snap(max_x - min_x + 48.), snap(max_y - min_y + 72.)),
    )
}

/// The extents of a set of points along its two principal axes (major, minor), for shape checks.
#[cfg(test)]
fn principal_extents(points: &[(f64, f64)]) -> (f64, f64) {
    let n = points.len() as f64;
    let (mx, my) = (
        points.iter().map(|p| p.0).sum::<f64>() / n,
        points.iter().map(|p| p.1).sum::<f64>() / n,
    );
    let (mut sxx, mut syy, mut sxy) = (0., 0., 0.);
    for p in points {
        let (dx, dy) = (p.0 - mx, p.1 - my);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let trace = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let disc = (trace * trace / 4. - det).max(0.).sqrt();
    let (l1, l2) = (trace / 2. + disc, (trace / 2. - disc).max(0.));
    let (major, minor) = (l1.max(0.).sqrt(), l2.sqrt());
    (major, minor)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("n{i}")).collect()
    }

    fn uniform(names: &[String], w: f64, h: f64) -> BTreeMap<String, (f64, f64)> {
        names.iter().map(|n| (n.clone(), (w, h))).collect()
    }

    fn links(pairs: &[(usize, usize)], names: &[String]) -> BTreeSet<(String, String)> {
        pairs
            .iter()
            .map(|&(a, b)| (names[a].clone(), names[b].clone()))
            .collect()
    }

    fn overlaps(placed: &BTreeMap<String, Rect>) -> usize {
        let items: Vec<&Rect> = placed.values().collect();
        let mut count = 0;
        for i in 0..items.len() {
            for j in i + 1..items.len() {
                let (a, b) = (items[i], items[j]);
                if a.x < b.x + b.width
                    && b.x < a.x + a.width
                    && a.y < b.y + b.height
                    && b.y < a.y + a.height
                {
                    count += 1;
                }
            }
        }
        count
    }

    fn path(n: usize) -> Vec<(usize, usize)> {
        (0..n - 1).map(|i| (i, i + 1)).collect()
    }

    #[test]
    fn distance_matrix_is_flat_row_major() {
        let names = ids(4);
        let matrix = compute_distances(&names, &links(&[(0, 1), (1, 2)], &names));
        assert_eq!(
            matrix,
            vec![
                0., 1., 2., 3., //
                1., 0., 1., 3., //
                2., 1., 0., 3., //
                3., 3., 3., 0.,
            ]
        );
    }

    #[test]
    fn node_edge_repulsion_moves_only_free_blockers_and_is_deterministic() {
        let start = vec![(0., 0.), (240., 0.), (120., 0.), (120., 500.)];
        let extent = vec![(40., 40.); 4];
        let pins = [true, true, false, false];
        let mut moved = start.clone();
        repel_node_edges(&mut moved, &[(0, 1), (0, 0)], &extent, &pins, 10.);
        assert_eq!(&moved[..2], &start[..2]);
        assert_eq!(moved[3], start[3]);
        assert!(
            moved[2].1 > 20.,
            "the blocker leaves its body radius: {moved:?}"
        );
        let mut again = start.clone();
        repel_node_edges(&mut again, &[(0, 1), (0, 0)], &extent, &pins, 10.);
        assert_eq!(again, moved);
        let mut fixed = start.clone();
        repel_node_edges(&mut fixed, &[(0, 1)], &extent, &[true; 4], 10.);
        assert_eq!(fixed, start);
    }

    #[test]
    fn enabling_repulsion_does_not_relax_a_held_warm_start() {
        let names = ids(3);
        let sizes = uniform(&names, 40., 40.);
        let links = links(&[(0, 1)], &names);
        let start: BTreeMap<_, _> = names
            .iter()
            .cloned()
            .zip([(0., 0.), (240., 0.), (120., 0.)])
            .collect();
        let old = organic_stages_with_vpsc(
            &names,
            &links,
            &sizes,
            Some(&start),
            30,
            &BTreeMap::new(),
            0,
            false,
            false,
            false,
            false,
        );
        let new = organic_stages_with_vpsc(
            &names,
            &links,
            &sizes,
            Some(&start),
            30,
            &BTreeMap::new(),
            0,
            false,
            true,
            false,
            false,
        );
        assert_eq!(old.separated, new.separated);
        assert_eq!(old.stress, new.stress);
    }

    #[test]
    fn spatial_separation_matches_the_original_pair_order() {
        for seed in 0..24 {
            let count = 48;
            let extent: Vec<(f64, f64)> = (0..count)
                .map(|i| {
                    (
                        20. + ((i * 37 + seed * 11) % 140) as f64,
                        18. + ((i * 53 + seed * 7) % 115) as f64,
                    )
                })
                .collect();
            let pinned: Vec<bool> = (0..count).map(|i| (i + seed) % 13 == 0).collect();
            let mut initial: Vec<(f64, f64)> = (0..count)
                .map(|i| {
                    (
                        (((i * 73 + seed * 19) % 137) as f64 - 68.) * 8.,
                        (((i * 47 + seed * 23) % 101) as f64 - 50.) * 7.,
                    )
                })
                .collect();
            initial[0] = (0., 0.);
            initial[1] = (0., 0.);

            let mut expected = initial.clone();
            for _ in 0..200 {
                let mut any = false;
                for i in 0..count {
                    for j in i + 1..count {
                        let (fi, fj) = (pinned[i], pinned[j]);
                        if fi && fj {
                            continue;
                        }
                        let (share_i, share_j) = match (fi, fj) {
                            (true, _) => (0., 1.),
                            (_, true) => (1., 0.),
                            _ => (0.5, 0.5),
                        };
                        let (wi, hi) = extent[i];
                        let (wj, hj) = extent[j];
                        let dx = expected[j].0 - expected[i].0;
                        let dy = expected[j].1 - expected[i].1;
                        let ox = (wi + wj) / 2. + GAP - dx.abs();
                        let oy = (hi + hj) / 2. + GAP - dy.abs();
                        if ox > 0. && oy > 0. {
                            any = true;
                            if ox <= oy {
                                let sign = if dx < 0. || (dx == 0. && i > j) {
                                    -1.
                                } else {
                                    1.
                                };
                                expected[i].0 -= sign * ox * share_i;
                                expected[j].0 += sign * ox * share_j;
                            } else {
                                let sign = if dy < 0. || (dy == 0. && i > j) {
                                    -1.
                                } else {
                                    1.
                                };
                                expected[i].1 -= sign * oy * share_i;
                                expected[j].1 += sign * oy * share_j;
                            }
                        }
                    }
                }
                if !any {
                    break;
                }
            }

            let mut actual = initial;
            super::separate_overlaps(&mut actual, &extent, &pinned);
            assert_eq!(actual, expected, "seed {seed}");
        }
    }

    #[test]
    fn constraint_separation_minimizes_motion_and_keeps_pins() {
        let initial = [(0., 0.), (20., 0.)];
        let extent = [(40., 40.); 2];
        let pinned = [true, false];
        let mut actual = initial;
        separate_overlaps_with_option(&mut actual, &extent, &pinned, true);
        assert_eq!(actual[0], initial[0]);
        assert!((actual[1].0 - (64. + 1e-7)).abs() < 1e-7, "{actual:?}");
        assert_eq!(actual[1].1, initial[1].1);
        assert!(vpsc::scanline_pairs(&actual, &extent, GAP)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn organic_arrangement_can_opt_into_constraint_separation() {
        let names = ids(2);
        let sizes = uniform(&names, 40., 40.);
        let start = BTreeMap::from([(names[0].clone(), (0., 0.)), (names[1].clone(), (20., 0.))]);
        let mut counters = crate::SearchCounters::default();
        let (placed, _) = organic_arrange_counted(
            &names,
            &BTreeSet::new(),
            &sizes,
            Some(&start),
            0,
            0.,
            &BTreeMap::new(),
            0,
            true,
            false,
            false,
            false,
            false,
            &mut counters,
        );
        assert_eq!(overlaps(&placed), 0);
    }

    #[cfg(feature = "benchmark-timings")]
    #[test]
    fn organic_stage_timings_are_collected() {
        let names = ids(8);
        let sizes = uniform(&names, 60., 30.);
        let links = links(&path(8), &names);
        let mut counters = crate::SearchCounters::default();
        organic_arrange_counted(
            &names,
            &links,
            &sizes,
            None,
            20,
            0.,
            &BTreeMap::new(),
            0,
            false,
            false,
            false,
            false,
            &mut counters,
        );
        assert!(counters.organic_stress_nanos > 0);
        assert!(counters.separation_nanos > 0);
    }

    /// The same area asked for again (a step of a drag) takes its distances from the memo and gets the same result; other links
    /// or other children are computed anew.
    #[test]
    fn the_distances_of_the_same_area_are_taken_from_the_memo_with_the_same_result() {
        let names = ids(12);
        let sizes = uniform(&names, 60., 30.);
        let l = links(&path(12), &names);
        let empty = BTreeMap::new();
        let first = organic_stages(&names, &l, &sizes, None, 30, &empty, 0);
        let (hits, misses) = distance_memo_stats();
        let again = organic_stages(&names, &l, &sizes, None, 30, &empty, 0);
        let (hits_after, misses_after) = distance_memo_stats();
        assert_eq!((hits_after, misses_after), (hits + 1, misses));
        assert_eq!(first.separated, again.separated);
        assert_eq!(first.stress_final, again.stress_final);
        let memoized = distances(&names, &l);
        let reused = distances(&names, &l);
        assert!(Rc::ptr_eq(&memoized, &reused));
        // Other links: computed anew.
        let other = links(&[(0, 5), (5, 11)], &names);
        organic_stages(&names, &other, &sizes, None, 30, &empty, 0);
        assert_eq!(distance_memo_stats().1, misses + 1);
    }

    /// The stress stage lowers the stress of the initialization, and the reported stresses are the measured ones.
    #[test]
    fn stress_is_measured_before_and_after_and_does_not_increase() {
        let names = ids(10);
        let l = links(
            &[
                (0, 1),
                (0, 2),
                (0, 3),
                (1, 4),
                (2, 5),
                (3, 6),
                (4, 7),
                (5, 8),
                (6, 9),
            ],
            &names,
        );
        let sizes = uniform(&names, 120., 54.);
        let s = organic_stages(&names, &l, &sizes, None, 300, &BTreeMap::new(), 0);
        assert!(
            s.stress_final <= s.stress_initial + 1e-9,
            "{} > {}",
            s.stress_final,
            s.stress_initial
        );
        assert!(s.iterations >= 1);
    }

    /// A path keeps its order along its main axis after the stress stage: it may become straight, but not scrambled.
    #[test]
    fn a_path_stays_in_order_along_its_main_axis() {
        let names = ids(8);
        let sizes = uniform(&names, 120., 54.);
        let s = organic_stages(
            &names,
            &links(&path(8), &names),
            &sizes,
            None,
            300,
            &BTreeMap::new(),
            0,
        );
        let (mx, my) = (
            s.stress.iter().map(|p| p.0).sum::<f64>() / 8.,
            s.stress.iter().map(|p| p.1).sum::<f64>() / 8.,
        );
        let (sxx, syy, sxy) = s.stress.iter().fold((0., 0., 0.), |acc, p| {
            let (dx, dy) = (p.0 - mx, p.1 - my);
            (acc.0 + dx * dx, acc.1 + dy * dy, acc.2 + dx * dy)
        });
        // The main axis: the eigenvector of the larger eigenvalue of the covariance.
        let theta = 0.5 * (2. * sxy).atan2(sxx - syy);
        let projected: Vec<f64> = s
            .stress
            .iter()
            .map(|p| (p.0 - mx) * theta.cos() + (p.1 - my) * theta.sin())
            .collect();
        let increasing = projected.windows(2).all(|w| w[1] >= w[0] - 1e-6);
        let decreasing = projected.windows(2).all(|w| w[1] <= w[0] + 1e-6);
        assert!(
            increasing || decreasing,
            "the path is scrambled: {projected:?}"
        );
    }

    /// A star does not collapse to a line in the stress stage: its leaves spread in two dimensions.
    #[test]
    fn a_star_spreads_in_two_dimensions_in_the_stress_stage() {
        let names = ids(9);
        let pairs: Vec<(usize, usize)> = (1..9).map(|i| (0, i)).collect();
        let sizes = uniform(&names, 120., 54.);
        let s = organic_stages(
            &names,
            &links(&pairs, &names),
            &sizes,
            None,
            300,
            &BTreeMap::new(),
            0,
        );
        let (major, minor) = principal_extents(&s.stress);
        assert!(
            minor / major > 0.5,
            "the star collapsed: major {major:.1}, minor {minor:.1}"
        );
    }

    /// A cycle keeps its ring shape in the stress stage: distances from the centre vary little.
    #[test]
    fn a_cycle_keeps_a_ring_shape_in_the_stress_stage() {
        let names = ids(10);
        let pairs: Vec<(usize, usize)> = (0..10).map(|i| (i, (i + 1) % 10)).collect();
        let sizes = uniform(&names, 60., 40.);
        let s = organic_stages(
            &names,
            &links(&pairs, &names),
            &sizes,
            None,
            300,
            &BTreeMap::new(),
            0,
        );
        let cx = s.stress.iter().map(|p| p.0).sum::<f64>() / 10.;
        let cy = s.stress.iter().map(|p| p.1).sum::<f64>() / 10.;
        let radii: Vec<f64> = s
            .stress
            .iter()
            .map(|p| ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt())
            .collect();
        let mean = radii.iter().sum::<f64>() / 10.;
        let spread = (radii.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / 10.).sqrt() / mean;
        assert!(
            spread < 0.25,
            "the cycle lost its ring: relative spread {spread:.3}"
        );
    }

    /// The placement is deterministic and unequal sizes do not overlap after the separation stage.
    #[test]
    fn placement_is_deterministic_and_unequal_sizes_do_not_overlap() {
        let names = ids(10);
        let pairs: Vec<(usize, usize)> = (1..10).map(|i| (0, i)).collect();
        let mut sizes = uniform(&names, 80., 50.);
        sizes.insert("n1".into(), (320., 160.));
        sizes.insert("n4".into(), (50., 200.));
        let a = organic_arrange(
            &names,
            &links(&pairs, &names),
            &sizes,
            None,
            300,
            0.,
            &BTreeMap::new(),
            0,
        )
        .0;
        let b = organic_arrange(
            &names,
            &links(&pairs, &names),
            &sizes,
            None,
            300,
            0.,
            &BTreeMap::new(),
            0,
        )
        .0;
        assert_eq!(format!("{a:?}"), format!("{b:?}"));
        assert_eq!(overlaps(&a), 0);
    }

    /// The node bound is checked before placement: a larger area is refused with its size.
    #[test]
    fn an_area_above_the_bound_is_refused_before_placement() {
        let mut graph = Graph {
            default_direction: arqlet_model::DirectionConstraint::Free,
            container_directions: BTreeMap::new(),
            default_strategy: arqlet_model::StrategyConstraint::Fixed(
                arqlet_model::LayoutStrategy::Organic,
            ),
            container_strategies: BTreeMap::new(),
            default_routing: Default::default(),
            container_routing: Default::default(),
            ports: vec![],
            edge_ports: vec![],
            nodes: vec![],
            edges: vec![],
            buses: Vec::new(),
        };
        for i in 0..(MAX_NODES + 1) {
            graph.nodes.push(arqlet_model::Node {
                id: format!("n{i}"),
                kind: "symbol".into(),
                label: "n".into(),
                parent_id: None,
                shape: Default::default(),
            });
        }
        let err = check_organic_areas(&graph, false, false).unwrap_err();
        assert!(err.contains("bounded to 300"), "{err}");
        // The sparse stress accepts this area, and refuses one more child than its own bound.
        assert!(check_organic_areas(&graph, true, false).is_ok());
        for i in (MAX_NODES + 1)..SPARSE_MAX_NODES {
            graph.nodes.push(arqlet_model::Node {
                id: format!("n{i}"),
                kind: "symbol".into(),
                label: "n".into(),
                parent_id: None,
                shape: Default::default(),
            });
        }
        assert!(check_organic_areas(&graph, true, false).is_ok());
        graph.nodes.push(arqlet_model::Node {
            id: "overflow".into(),
            kind: "symbol".into(),
            label: "n".into(),
            parent_id: None,
            shape: Default::default(),
        });
        let err = check_organic_areas(&graph, true, false).unwrap_err();
        assert!(err.contains("bounded to 500"), "{err}");
    }

    #[test]
    fn size_aware_targets_add_both_half_extents_and_default_adds_nothing() {
        assert_eq!(pair_pad(&[], 0, 1), 0.);
        assert_eq!(pair_pad(&[1., 2.], 0, 1), 3.);
    }

    #[test]
    fn sparse_targets_are_symmetric_and_carry_the_hop_distances_to_pivots() {
        let names = ids(7);
        // A path of six children and one isolated child.
        let links = links(&path(6), &names);
        let targets = sparse_targets(&names, &links);
        assert!(targets.pivots.contains(&6), "{:?}", targets.pivots);
        for (i, list) in targets.neighbours.iter().enumerate() {
            for &(j, d) in list {
                assert!(
                    targets.neighbours[j].iter().any(|&(k, e)| k == i && e == d),
                    "{i} lists {j} but not back"
                );
            }
        }
        for (q, &p) in targets.pivots.iter().enumerate() {
            if p == 6 {
                assert_eq!(
                    targets.pivot_dist[q][0], 6.,
                    "isolated pivot, unreachable pairs"
                );
            } else {
                assert_eq!(
                    targets.pivot_dist[q][6], 6.,
                    "unreachable pairs get largest + 1"
                );
            }
        }
        let from_zero = targets.pivots.iter().position(|&p| p == 0).unwrap();
        assert_eq!(targets.pivot_dist[from_zero][5], 5.);
        assert_eq!(targets.neighbours[1][0], (0, 1.));
    }

    #[test]
    fn sparse_stress_is_the_dense_stress_when_every_child_is_a_pivot() {
        // Up to SPARSE_PIVOTS children every child is a pivot, so the listed pairs are all pairs and the sums run in the same
        // order: the sparse and dense stages must agree bit for bit.
        let names = ids(9);
        let mut pairs = path(9);
        pairs.push((0, 8));
        let links = links(&pairs, &names);
        let sizes = uniform(&names, 60., 30.);
        let dense = organic_stages_with_vpsc(
            &names,
            &links,
            &sizes,
            None,
            40,
            &BTreeMap::new(),
            0,
            false,
            false,
            false,
            false,
        );
        let sparse = organic_stages_with_vpsc(
            &names,
            &links,
            &sizes,
            None,
            40,
            &BTreeMap::new(),
            0,
            false,
            false,
            true,
            false,
        );
        assert_eq!(dense.separated, sparse.separated);
        assert_eq!(dense.stress_final.to_bits(), sparse.stress_final.to_bits());
    }

    #[test]
    fn sparse_stress_places_a_larger_area_without_increasing_its_stress() {
        let count = 400;
        let names = ids(count);
        let mut pairs = path(count);
        pairs.extend((0..count - 7).step_by(7).map(|i| (i, i + 7)));
        let links = links(&pairs, &names);
        let sizes = uniform(&names, 60., 30.);
        let stages = organic_stages_with_vpsc(
            &names,
            &links,
            &sizes,
            None,
            60,
            &BTreeMap::new(),
            0,
            false,
            false,
            true,
            false,
        );
        assert!(stages.stress_final <= stages.stress_initial);
        assert!(stages
            .separated
            .iter()
            .all(|p| p.0.is_finite() && p.1.is_finite()));
        assert_eq!(stages.separated.len(), count);
    }

    fn asymmetric() -> Graph {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure/asymmetric-tree.json"))
                .unwrap(),
        )
        .unwrap()
    }

    /// An organic drawing is complete, valid and free of node overlaps, with the default orthogonal routing.
    #[test]
    fn an_organic_root_composes_a_complete_valid_drawing() {
        let mut graph = asymmetric();
        graph.default_direction = arqlet_model::DirectionConstraint::Free;
        graph.default_strategy =
            arqlet_model::StrategyConstraint::Fixed(arqlet_model::LayoutStrategy::Organic);
        let snapshot = crate::compute(
            &graph,
            None,
            &BTreeSet::new(),
            &arqlet_model::LayoutOptions::default(),
        )
        .unwrap();
        assert_eq!(snapshot.routes.len(), graph.edges.len());
        assert!(
            snapshot.geometry_issues.is_empty(),
            "{:?}",
            snapshot.geometry_issues
        );
        assert_eq!(overlaps(&snapshot.boxes), 0);
    }

    /// Organic is not a ranked layout, so the strategy of an area wins over its direction: a fixed direction does not stop an
    /// organic area, and the direction stays in the document.
    #[test]
    fn organic_below_a_fixed_direction_is_allowed_and_the_direction_is_not_used() {
        let mut graph = asymmetric();
        graph.default_direction =
            arqlet_model::DirectionConstraint::Fixed(arqlet_model::Orientation::Tb);
        graph.default_strategy =
            arqlet_model::StrategyConstraint::Fixed(arqlet_model::LayoutStrategy::Organic);
        assert_eq!(graph.area_mode(None), Ok(Mode::Organic));
        let snapshot = crate::compute(
            &graph,
            None,
            &BTreeSet::new(),
            &arqlet_model::LayoutOptions::default(),
        )
        .expect("an organic area with a fixed direction is laid out");
        assert_eq!(snapshot.boxes.len(), graph.nodes.len());
    }

    /// Straight routing: each edge is one segment between its ports. A segment through a foreign node is reported, and
    /// the route does not bend around it.
    #[test]
    fn straight_edges_never_bend_and_report_what_they_cross() {
        let mut graph = asymmetric();
        graph.default_direction =
            arqlet_model::DirectionConstraint::Fixed(arqlet_model::Orientation::Lr);
        graph.default_strategy =
            arqlet_model::StrategyConstraint::Fixed(arqlet_model::LayoutStrategy::Tree);
        let options = arqlet_model::LayoutOptions {
            edge_routing: Some(arqlet_model::EdgeRouting::Straight),
            ..Default::default()
        };
        let snapshot = crate::compute(&graph, None, &BTreeSet::new(), &options).unwrap();
        assert_eq!(snapshot.routes.len(), graph.edges.len());
        assert!(
            snapshot.routes.iter().all(|r| r.points.len() == 2),
            "no bends"
        );
    }

    /// Under straight routing a self-loop follows the loop rule: it has no second end for a segment, so it is drawn as a loop
    /// while the ordinary edges stay straight. Parallel edges are drawn, each with its own ports, as one segment each.
    #[test]
    fn straight_routing_draws_self_loops_as_loops_and_parallel_edges_straight() {
        let options = arqlet_model::LayoutOptions {
            edge_routing: Some(arqlet_model::EdgeRouting::Straight),
            ..Default::default()
        };
        let mut looped = asymmetric();
        looped.edges.push(arqlet_model::Edge {
            id: "loop".into(),
            kind: "flow".into(),
            source: "a".into(),
            target: Some("a".into()),
        });
        let drawn = crate::compute(&looped, None, &BTreeSet::new(), &options).unwrap();
        let looping = drawn.routes.iter().find(|r| r.id == "loop").unwrap();
        assert!(
            looping.points.len() > 2,
            "a loop has bends: {:?}",
            looping.points
        );
        assert!(!drawn.straight_edges.contains(&"loop".to_string()));
        for route in drawn.routes.iter().filter(|r| r.id != "loop") {
            assert!(drawn.straight_edges.contains(&route.id), "{}", route.id);
            assert_eq!(route.points.len(), 2, "{}", route.id);
        }
        let mut parallel = asymmetric();
        parallel.edges.push(parallel.edges[0].clone());
        parallel.edges.last_mut().unwrap().id = "dup".into();
        parallel.default_direction = arqlet_model::DirectionConstraint::Free;
        parallel.default_strategy =
            arqlet_model::StrategyConstraint::Fixed(arqlet_model::LayoutStrategy::Organic);
        let snapshot = crate::compute(&parallel, None, &BTreeSet::new(), &options).unwrap();
        let first = &parallel.edges[0].id;
        let (one, two) = (
            snapshot.routes.iter().find(|r| &r.id == first).unwrap(),
            snapshot.routes.iter().find(|r| r.id == "dup").unwrap(),
        );
        assert_eq!((one.points.len(), two.points.len()), (2, 2), "no bends");
        assert_ne!(
            (one.points[0].x, one.points[0].y),
            (two.points[0].x, two.points[0].y),
            "each edge has its own port"
        );
    }

    /// A mixed drawing: an organic container next to a plain one, in an automatic root. Both are laid out and the drawing is
    /// complete and valid.
    #[test]
    fn an_organic_container_composes_with_a_plain_one() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let graph: Graph = serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure/mixed-organic.json"))
                .unwrap(),
        )
        .unwrap();
        let snapshot = crate::compute(
            &graph,
            None,
            &BTreeSet::new(),
            &arqlet_model::LayoutOptions::default(),
        )
        .unwrap();
        assert_eq!(snapshot.routes.len(), graph.edges.len());
        assert!(
            snapshot.geometry_issues.is_empty(),
            "{:?}",
            snapshot.geometry_issues
        );
    }
}

#[cfg(test)]
mod scalable_tests {
    use super::*;

    /// A random recursive tree with chords (like `benchmarks/eval/organic.mjs`'s synthetic graphs): hubs, many leaves per parent,
    /// neighbourhoods that grow fast. Seeded, so the same graph every run.
    fn synthetic(n: usize) -> (Vec<String>, BTreeSet<(String, String)>) {
        let mut s: u32 = 20261109 + n as u32;
        let mut random = || {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            s as f64 / 4294967296.
        };
        let ids: Vec<String> = (0..n).map(|i| format!("n{i:05}")).collect();
        let mut links = BTreeSet::new();
        for i in 1..n {
            let p = (random() * i as f64) as usize;
            links.insert((ids[p].clone(), ids[i].clone()));
        }
        for _ in 0..(n * 3 / 10) {
            let (a, b) = (
                (random() * n as f64) as usize,
                (random() * n as f64) as usize,
            );
            if a != b {
                links.insert((ids[a.min(b)].clone(), ids[a.max(b)].clone()));
            }
        }
        (ids, links)
    }

    fn stages(
        ids: &[String],
        links: &BTreeSet<(String, String)>,
        fixed: &BTreeMap<String, (f64, f64)>,
        scalable: bool,
    ) -> Stages {
        let sizes: BTreeMap<String, (f64, f64)> =
            ids.iter().map(|id| (id.clone(), (180., 54.))).collect();
        organic_stages_full(
            ids, links, &sizes, None, 300, fixed, 0, false, false, !scalable, false, scalable,
        )
    }

    fn overlapping_pairs(x: &[(f64, f64)]) -> usize {
        let mut count = 0;
        for i in 0..x.len() {
            for j in i + 1..x.len() {
                if (x[i].0 - x[j].0).abs() < 180. && (x[i].1 - x[j].1).abs() < 54. {
                    count += 1;
                }
            }
        }
        count
    }

    #[test]
    fn the_scalable_placement_leaves_no_overlaps_where_the_sparse_stress_stacks_siblings() {
        let (ids, links) = synthetic(600);
        let sparse = stages(&ids, &links, &BTreeMap::new(), false);
        let scalable = stages(&ids, &links, &BTreeMap::new(), true);
        assert!(overlapping_pairs(&sparse.separated) > 100);
        assert_eq!(overlapping_pairs(&scalable.separated), 0);
        assert!(scalable
            .separated
            .iter()
            .all(|p| p.0.is_finite() && p.1.is_finite()));
    }

    #[test]
    fn the_scalable_placement_is_deterministic_and_keeps_pins() {
        let (ids, links) = synthetic(300);
        let fixed = BTreeMap::from([(ids[0].clone(), (0., 0.)), (ids[7].clone(), (900., 300.))]);
        let first = stages(&ids, &links, &fixed, true);
        let second = stages(&ids, &links, &fixed, true);
        assert_eq!(first.separated, second.separated);
        assert_eq!(first.separated[0], (0., 0.));
        assert_eq!(first.separated[7], (900., 300.));
    }

    #[test]
    fn the_scalable_bound_admits_its_size_and_refuses_one_more() {
        let make = |n: usize| -> Graph {
            serde_json::from_value(serde_json::json!({
                "defaultStrategy": "organic",
                "nodes": (0..n).map(|i| serde_json::json!({"id": format!("n{i}"), "kind": "node", "label": "n"})).collect::<Vec<_>>(),
                "edges": [],
            }))
            .unwrap()
        };
        assert!(check_organic_areas(&make(SCALABLE_MAX_NODES), false, true).is_ok());
        assert!(check_organic_areas(&make(SCALABLE_MAX_NODES), true, false).is_err());
        assert!(check_organic_areas(&make(SCALABLE_MAX_NODES + 1), false, true).is_err());
    }

    #[test]
    fn a_weighted_quadtree_with_unit_masses_is_the_unweighted_tree() {
        // The overview (ADR 0018) weights the tree; with every mass 1 the forces must be the same bits as before.
        let points: Vec<(f64, f64)> = (0..200)
            .map(|i| {
                let a = i as f64 * 2.399963;
                (
                    (i as f64).sqrt() * a.cos() * 9.,
                    (i as f64).sqrt() * a.sin() * 9.,
                )
            })
            .chain([(3., 3.), (3., 3.)])
            .collect();
        let plain = QuadTree::new(&points);
        let weighted = QuadTree::new_weighted(&points, &vec![1.; points.len()]);
        for i in 0..points.len() {
            let (a, b) = (
                plain.force(&points, i, -0.7),
                weighted.force(&points, i, -0.7),
            );
            assert_eq!(
                (a.0.to_bits(), a.1.to_bits()),
                (b.0.to_bits(), b.1.to_bits())
            );
        }
    }

    #[test]
    fn a_quadtree_sums_the_repulsion_of_every_point() {
        // Far points are taken as one body: the force on a point from a distant cluster is close to the exact sum.
        let mut points = vec![(0., 0.)];
        for i in 0..20 {
            points.push((1000. + (i % 5) as f64, 1000. + (i / 5) as f64));
        }
        let tree = QuadTree::new(&points);
        let (fx, fy) = tree.force(&points, 0, -1.);
        let (mut ex, mut ey) = (0., 0.);
        for p in &points[1..] {
            let l2 = p.0 * p.0 + p.1 * p.1;
            ex -= p.0 / l2;
            ey -= p.1 / l2;
        }
        assert!((fx - ex).abs() < 1e-3 * ex.abs(), "{fx} {ex}");
        assert!((fy - ey).abs() < 1e-3 * ey.abs(), "{fy} {ey}");
        // Coincident points do not push each other (no direction).
        let same = vec![(5., 5.), (5., 5.), (5., 5.)];
        assert_eq!(QuadTree::new(&same).force(&same, 1, -1.), (0., 0.));
    }
}
