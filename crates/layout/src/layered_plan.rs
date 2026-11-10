//! Provisional (plan section 4a, experimental option `layeredConstruction`): the derived layout graph of a layered area.
//!
//! One value per layered area, built by the placement and kept for the steps after it, so that ordering, placement, port
//! assignment and routing work on the same sections instead of guessing them again from the geometry. It is never part of
//! the canonical graph or the view.
//!
//! An area's plan holds its ranks in flow order, each in its order across the flow, with the area's children and the
//! long-edge dummies; the chain of dummies of every link; the links the cycle break reversed; and the place of every dummy.

use arqlet_model::{Rect, StoredAreaPlan, StoredChain, StoredLink, STORED_PLAN_FORMAT};
use std::collections::{BTreeMap, BTreeSet};

/// The plans of the layered areas of one layout pass, by area (`"<root>"` for the top level).
pub type LayeredPlans = BTreeMap<String, AreaPlan>;

/// The derived layout graph of one layered area (provisional).
#[derive(Clone, Debug, Default)]
pub struct AreaPlan {
    /// The flow runs along y (top to bottom or bottom to top).
    pub vertical: bool,
    /// The flow runs against the axis (right to left or bottom to top); the ranks are mirrored on the axis.
    pub reversed: bool,
    /// The ranks in flow order, each in its order across the flow: children of the area and long-edge dummies.
    pub ranks: Vec<Vec<String>>,
    /// The links between the area's children in flow direction (after the cycle break), each with the dummies it passes in
    /// order; a link between neighbouring ranks has none.
    pub chains: BTreeMap<(String, String), Vec<String>>,
    /// The links the cycle break reversed, as (source, target) of the logical direction.
    pub reversed_links: BTreeSet<(String, String)>,
    /// The place of every dummy, in the area's frame (relative to the area's box, as the children's boxes are).
    pub dummies: BTreeMap<String, Rect>,
    /// Border nodes (step 6): by edge, the virtual node where an edge between the inside and the outside of this area (a
    /// container) crosses its border, in an extra rank at the border it leaves or enters by. It is a dummy of the plan.
    pub borders: BTreeMap<String, String>,
    /// Side nodes (single pass, ADR 0017): by edge, the virtual node where an edge leaves or enters this area on a side its
    /// parent's flow uses, when the parent's flow runs across this area's. It stands in the rank after (leaving) or before
    /// (entering) its child, last or first across the flow, and is a dummy like a border node.
    pub sides: BTreeMap<String, String>,
    /// The order of the ranks is given (by the hierarchical order of all plans): the placement takes it as it is. Only for
    /// the passes of one layout; not kept with the snapshot.
    pub fixed: bool,
    /// The order of the border ranks is given (by the hierarchical order of all plans, single pass): the placement follows
    /// the given order everywhere and improves only the inner ranks. Only for the passes of one layout.
    pub fixed_borders: bool,
    /// Experimental (consistent layered pipeline): the place of a port of a leaf child along its node side, from the
    /// side's start, by (edge, source end): the ports of a side in the order of the neighbours in the next rank. The
    /// placement counts the tracks from these places, and the ports take them (`port_lanes`). Not kept with the snapshot.
    pub port_offsets: BTreeMap<(String, bool), f64>,
    /// Experimental (consistent layered pipeline): where an edge crosses a dummy of this plan, by (dummy, edge), from the
    /// dummy's start across the flow: the slots of a chain's edges keep the pattern of their lanes where the chain begins.
    /// Not kept with the snapshot.
    pub lane_offsets: BTreeMap<(String, String), f64>,
}

impl AreaPlan {
    /// The connections between neighbouring ranks, as (upper, lower) of the flow: each link split at its dummies.
    pub fn sections(&self) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for ((a, b), dummies) in &self.chains {
            let mut previous = a.clone();
            for d in dummies {
                out.push((previous, d.clone()));
                previous = d.clone();
            }
            out.push((previous, b.clone()));
        }
        out
    }

    /// The crossings of the sections between neighbouring ranks with the current order (bilayer crossings).
    pub fn crossings(&self) -> usize {
        order_crossings(&self.ranks, &self.sections())
    }

    /// The plan as kept with a snapshot (`Snapshot::layered_plans`).
    pub fn to_stored(&self) -> StoredAreaPlan {
        let link = |(source, target): &(String, String)| StoredLink {
            source: source.clone(),
            target: target.clone(),
        };
        StoredAreaPlan {
            format: STORED_PLAN_FORMAT,
            vertical: self.vertical,
            reversed: self.reversed,
            ranks: self.ranks.clone(),
            chains: self
                .chains
                .iter()
                .map(|((source, target), dummies)| StoredChain {
                    source: source.clone(),
                    target: target.clone(),
                    dummies: dummies.clone(),
                })
                .collect(),
            reversed_links: self.reversed_links.iter().map(link).collect(),
            dummies: self.dummies.clone(),
            borders: self.borders.clone(),
            sides: self.sides.clone(),
        }
    }

    /// The plan kept with a snapshot, or `None` when it was written in another format.
    pub fn from_stored(stored: &StoredAreaPlan) -> Option<AreaPlan> {
        (stored.format == STORED_PLAN_FORMAT).then(|| AreaPlan {
            vertical: stored.vertical,
            reversed: stored.reversed,
            ranks: stored.ranks.clone(),
            chains: stored
                .chains
                .iter()
                .map(|c| ((c.source.clone(), c.target.clone()), c.dummies.clone()))
                .collect(),
            reversed_links: stored
                .reversed_links
                .iter()
                .map(|l| (l.source.clone(), l.target.clone()))
                .collect(),
            dummies: stored.dummies.clone(),
            borders: stored.borders.clone(),
            sides: stored.sides.clone(),
            fixed: false,
            fixed_borders: false,
            port_offsets: BTreeMap::new(),
            lane_offsets: BTreeMap::new(),
        })
    }
}

/// The plan an edge between `source` and `target` is drawn in, and the elements of it that stand for the two ends: the
/// smallest area holding both ends whose plan has an element on each end's way up (the end itself, or the container that
/// holds it); with global layering a flattened container has no plan, and its children are elements of its parent's.
/// `parent` gives every node's area (`"<root>"` for the top level).
pub fn plan_ends(
    plans: &LayeredPlans,
    parent: &BTreeMap<&str, &str>,
    source: &str,
    target: &str,
) -> Option<(String, String, String)> {
    let ancestors = |id: &str| -> Vec<String> {
        let mut out = Vec::new();
        let mut at = id;
        while let Some(&up) = parent.get(at) {
            out.push(up.to_string());
            if up == "<root>" {
                break;
            }
            at = up;
        }
        out
    };
    let (up_s, up_t) = (ancestors(source), ancestors(target));
    for area in up_s.iter().filter(|a| up_t.contains(a)) {
        let Some(plan) = plans.get(area) else {
            continue;
        };
        // A scan, not a set: an edge looks up only the few ids on its way up.
        let member = |end: &str| -> Option<String> {
            let mut at = end;
            while at != area {
                if plan.ranks.iter().flatten().any(|m| m == at) {
                    return Some(at.to_string());
                }
                at = parent.get(at)?;
            }
            None
        };
        if let (Some(s), Some(t)) = (member(source), member(target)) {
            return Some((area.clone(), s, t));
        }
    }
    None
}

/// The crossings between neighbouring ranks of `ranks` (each in order) for the sections `links` (upper, lower). A section
/// whose ends are not in neighbouring ranks is not counted. Two sections that share an end do not cross.
pub fn order_crossings(ranks: &[Vec<String>], links: &[(String, String)]) -> usize {
    let mut place: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for (r, ids) in ranks.iter().enumerate() {
        for (i, id) in ids.iter().enumerate() {
            place.insert(id.as_str(), (r, i));
        }
    }
    let mut by_rank: Vec<Vec<(usize, usize)>> = vec![Vec::new(); ranks.len()];
    for (a, b) in links {
        let (Some(&(ra, ia)), Some(&(rb, ib))) = (place.get(a.as_str()), place.get(b.as_str()))
        else {
            continue;
        };
        if rb == ra + 1 {
            by_rank[ra].push((ia, ib));
        } else if ra == rb + 1 {
            by_rank[rb].push((ib, ia));
        }
    }
    by_rank.iter_mut().map(|pairs| inversions(pairs)).sum()
}

/// The pairs `(upper, lower)` of places that cross (strictly opposite orders on the two ranks), in O(n log n) with a
/// Fenwick tree over the lower places.
fn inversions(pairs: &mut [(usize, usize)]) -> usize {
    if pairs.len() < 2 {
        return 0;
    }
    pairs.sort_unstable();
    let size = pairs.iter().map(|p| p.1).max().unwrap_or(0) + 2;
    let mut tree = vec![0usize; size];
    let (mut total, mut inserted) = (0usize, 0usize);
    let mut i = 0;
    while i < pairs.len() {
        let mut j = i;
        while j < pairs.len() && pairs[j].0 == pairs[i].0 {
            j += 1;
        }
        for &(_, b) in &pairs[i..j] {
            // The earlier pairs with a greater lower place.
            let mut k = b + 1;
            let mut at_most = 0;
            while k > 0 {
                at_most += tree[k];
                k -= k & k.wrapping_neg();
            }
            total += inserted - at_most;
        }
        for &(_, b) in &pairs[i..j] {
            let mut k = b + 1;
            while k < size {
                tree[k] += 1;
                k += k & k.wrapping_neg();
            }
            inserted += 1;
        }
        i = j;
    }
    total
}

/// The ranks `ranks` in the order of an earlier plan's ranks `previous` (reuse): every id that `previous` holds goes to its
/// relative place in its earlier rank; an id it does not hold follows the id before it in `ranks`. `group` keeps the blocks
/// of `improve_order`. Deterministic.
pub fn follow_order(
    ranks: &[Vec<String>],
    previous: &[Vec<String>],
    group: &dyn Fn(&str) -> usize,
) -> Vec<Vec<String>> {
    let mut place: BTreeMap<&str, f64> = BTreeMap::new();
    for ids in previous {
        for (i, id) in ids.iter().enumerate() {
            place.insert(id.as_str(), (i as f64 + 0.5) / ids.len() as f64);
        }
    }
    ranks
        .iter()
        .map(|ids| {
            let mut last = 0.;
            let mut keyed: Vec<(usize, f64, usize, &String)> = ids
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    if let Some(&p) = place.get(id.as_str()) {
                        last = p;
                    }
                    (group(id), last, i, id)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
            keyed.into_iter().map(|k| k.3.clone()).collect()
        })
        .collect()
}

/// Orders the ranks so that the sections cross less: sweeps down (each node at the barycentre of its neighbours in the rank
/// above) and up (of the rank below) alternate, each followed by transpositions of neighbours that lower the crossings with
/// both neighbouring ranks. The best order seen is kept; the search stops after four sweeps without improvement. `group`
/// keeps nodes of different groups (components) in separate blocks of every rank. Deterministic.
pub fn improve_order(
    ranks: &mut [Vec<String>],
    links: &[(String, String)],
    group: &dyn Fn(&str) -> usize,
) {
    improve_order_around(ranks, links, group, &[]);
}

/// `improve_order` that keeps the ranks marked in `frozen` (by rank index; missing marks are false) in their order.
pub fn improve_order_around(
    ranks: &mut [Vec<String>],
    links: &[(String, String)],
    group: &dyn Fn(&str) -> usize,
    frozen: &[bool],
) {
    let frozen = |r: usize| frozen.get(r).copied().unwrap_or(false);
    if ranks.len() < 2 {
        return;
    }
    // The ranks in numbers: elements by index, their rank and place, their neighbours above and below (in the order of the
    // links), their group.
    let mut ids: Vec<String> = Vec::new();
    let mut index: BTreeMap<String, u32> = BTreeMap::new();
    let mut order: Vec<Vec<u32>> = Vec::with_capacity(ranks.len());
    let (mut rank_of, mut pos) = (Vec::new(), Vec::new());
    for (r, rank) in ranks.iter().enumerate() {
        let mut row = Vec::with_capacity(rank.len());
        for (i, id) in rank.iter().enumerate() {
            let k = ids.len() as u32;
            ids.push(id.clone());
            index.insert(id.clone(), k);
            rank_of.push(r);
            pos.push(i);
            row.push(k);
        }
        order.push(row);
    }
    let n = ids.len();
    let (mut above, mut below) = (vec![Vec::new(); n], vec![Vec::new(); n]);
    for (a, b) in links {
        let (Some(&ka), Some(&kb)) = (index.get(a.as_str()), index.get(b.as_str())) else {
            continue;
        };
        let (ra, rb) = (rank_of[ka as usize], rank_of[kb as usize]);
        if rb == ra + 1 {
            below[ka as usize].push(kb);
            above[kb as usize].push(ka);
        } else if ra == rb + 1 {
            below[kb as usize].push(ka);
            above[ka as usize].push(kb);
        }
    }
    let groups: Vec<usize> = ids.iter().map(|id| group(id)).collect();
    let rank_count = order.len();
    // The sections between neighbouring ranks, as (upper, lower) elements, for counting crossings.
    let sections: Vec<(u32, u32)> = (0..n as u32)
        .flat_map(|a| below[a as usize].iter().map(move |&b| (a, b)))
        .collect();
    let count = |pos: &[usize]| -> usize {
        let mut by_rank: Vec<Vec<(usize, usize)>> = vec![Vec::new(); rank_count];
        for &(a, b) in &sections {
            by_rank[rank_of[a as usize]].push((pos[a as usize], pos[b as usize]));
        }
        by_rank.iter_mut().map(|pairs| inversions(pairs)).sum()
    };
    let mut best = order.clone();
    let mut best_crossings = order_crossings(ranks, links);
    let mut idle = 0;
    let mut sweep = 0;
    while best_crossings > 0 && idle < 4 && sweep < 32 {
        let down = sweep % 2 == 0;
        sweep += 1;
        let sequence: Vec<usize> = if down {
            (1..order.len()).collect()
        } else {
            (0..order.len() - 1).rev().collect()
        };
        for r in sequence {
            if frozen(r) {
                continue;
            }
            let fixed = if down { r - 1 } else { r + 1 };
            let neighbours = if down { &above } else { &below };
            let mut keyed: Vec<(usize, f64, usize, u32)> = order[r]
                .iter()
                .map(|&id| {
                    let ps: Vec<f64> = neighbours[id as usize]
                        .iter()
                        .filter(|&&x| rank_of[x as usize] == fixed)
                        .map(|&x| pos[x as usize] as f64)
                        .collect();
                    let current = pos[id as usize];
                    let key = if ps.is_empty() {
                        current as f64
                    } else {
                        ps.iter().sum::<f64>() / ps.len() as f64
                    };
                    (groups[id as usize], key, current, id)
                })
                .collect();
            keyed.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| a.1.total_cmp(&b.1))
                    .then_with(|| a.2.cmp(&b.2))
                    .then_with(|| ids[a.3 as usize].cmp(&ids[b.3 as usize]))
            });
            order[r] = keyed.into_iter().map(|k| k.3).collect();
            for (i, &id) in order[r].iter().enumerate() {
                pos[id as usize] = i;
            }
        }
        transpose(&mut order, &mut pos, &above, &below, &groups, &frozen);
        let crossings = count(&pos);
        if crossings < best_crossings {
            best_crossings = crossings;
            best = order.clone();
            idle = 0;
        } else {
            idle += 1;
        }
    }
    for (rank, row) in ranks.iter_mut().zip(&best) {
        *rank = row.iter().map(|&k| ids[k as usize].clone()).collect();
    }
}

/// The keys of a hierarchical order (`hierarchical_order`): of every block, and of every element.
#[derive(Clone, Debug, Default)]
pub struct HierarchyKeys {
    pub blocks: BTreeMap<String, f64>,
    pub elements: BTreeMap<String, f64>,
}

impl HierarchyKeys {
    /// The order of two elements: along their paths of blocks (outermost first), at the first place where they differ, by
    /// the key of the block (or of the element itself, where one path ends), then by id.
    pub fn compare(
        &self,
        path: &dyn Fn(&str) -> Vec<String>,
        a: &str,
        b: &str,
    ) -> std::cmp::Ordering {
        let (pa, pb) = (path(a), path(b));
        for level in 0.. {
            let xa = pa.get(level).map_or(a, String::as_str);
            let xb = pb.get(level).map_or(b, String::as_str);
            if xa != xb {
                let key = |x: &str, is_block: bool| {
                    if is_block {
                        self.blocks.get(x).copied().unwrap_or(0.5)
                    } else {
                        self.elements.get(x).copied().unwrap_or(0.5)
                    }
                };
                return key(xa, level < pa.len())
                    .total_cmp(&key(xb, level < pb.len()))
                    .then(xa.cmp(xb));
            }
            if level >= pa.len() && level >= pb.len() {
                break;
            }
        }
        std::cmp::Ordering::Equal
    }
}

/// Global layering (iteration 3): orders the ranks so that the sections cross less while every block (a flattened
/// container, nested in others, or the chain of a long edge) stays contiguous and keeps one place among its siblings in
/// every rank it spans, so that it can be drawn as one rectangle. `path` gives an element's blocks, outermost first (empty
/// for an element directly in the area). Sweeps down and up give every element the mean relative place of its neighbours
/// in the rank before (after); a block's key is the mean of its elements' keys over all its ranks; every rank is sorted
/// along the paths by these keys. The best order seen is kept; four sweeps without improvement end it. Deterministic.
/// Returns the keys of the order kept.
pub fn hierarchical_order(
    ranks: &mut [Vec<String>],
    links: &[(String, String)],
    path: &dyn Fn(&str) -> Vec<String>,
    start: Option<&HierarchyKeys>,
) -> HierarchyKeys {
    let mut above: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut below: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut rank_of: BTreeMap<String, usize> = BTreeMap::new();
    for (r, ids) in ranks.iter().enumerate() {
        for id in ids {
            rank_of.insert(id.clone(), r);
        }
    }
    for (a, b) in links {
        let (Some(&ra), Some(&rb)) = (rank_of.get(a), rank_of.get(b)) else {
            continue;
        };
        if rb == ra + 1 {
            below.entry(a.clone()).or_default().push(b.clone());
            above.entry(b.clone()).or_default().push(a.clone());
        } else if ra == rb + 1 {
            below.entry(b.clone()).or_default().push(a.clone());
            above.entry(a.clone()).or_default().push(b.clone());
        }
    }
    let relative = |ranks: &[Vec<String>]| -> BTreeMap<String, f64> {
        ranks
            .iter()
            .flat_map(|ids| {
                let n = ids.len() as f64;
                ids.iter()
                    .enumerate()
                    .map(move |(i, id)| (id.clone(), (i as f64 + 0.5) / n))
            })
            .collect()
    };
    let block_keys = |elements: &BTreeMap<String, f64>| -> BTreeMap<String, f64> {
        let mut sums: BTreeMap<String, (f64, usize)> = BTreeMap::new();
        for (id, key) in elements {
            for block in path(id) {
                let entry = sums.entry(block).or_insert((0., 0));
                entry.0 += key;
                entry.1 += 1;
            }
        }
        sums.into_iter()
            .map(|(b, (sum, n))| (b, sum / n as f64))
            .collect()
    };
    let sort_all = |ranks: &mut [Vec<String>], keys: &HierarchyKeys| {
        for ids in ranks.iter_mut() {
            ids.sort_by(|a, b| keys.compare(path, a, b));
        }
    };
    // The start: the given keys (an order found before, such as the one per area), where they name an element or a block,
    // else the ranks' order as it is.
    let mut keys = HierarchyKeys {
        elements: relative(ranks),
        ..Default::default()
    };
    keys.blocks = block_keys(&keys.elements);
    if let Some(start) = start {
        keys.elements
            .extend(start.elements.iter().map(|(k, v)| (k.clone(), *v)));
        keys.blocks
            .extend(start.blocks.iter().map(|(k, v)| (k.clone(), *v)));
    }
    sort_all(ranks, &keys);
    let mut best = ranks.to_vec();
    let mut best_keys = keys.clone();
    let mut best_crossings = order_crossings(ranks, links);
    let (mut idle, mut sweep) = (0, 0);
    while best_crossings > 0 && idle < 4 && sweep < 32 && ranks.len() > 1 {
        let down = sweep % 2 == 0;
        sweep += 1;
        let sequence: Vec<usize> = if down {
            (1..ranks.len()).collect()
        } else {
            (0..ranks.len() - 1).rev().collect()
        };
        for r in sequence {
            let fixed = if down { r - 1 } else { r + 1 };
            let n = ranks[fixed].len() as f64;
            let position: BTreeMap<&str, f64> = ranks[fixed]
                .iter()
                .enumerate()
                .map(|(i, id)| (id.as_str(), (i as f64 + 0.5) / n))
                .collect();
            let neighbours = if down { &above } else { &below };
            for id in ranks[r].clone() {
                let ps: Vec<f64> = neighbours
                    .get(&id)
                    .map(|ns| {
                        ns.iter()
                            .filter_map(|x| position.get(x.as_str()).copied())
                            .collect()
                    })
                    .unwrap_or_default();
                if !ps.is_empty() {
                    keys.elements
                        .insert(id, ps.iter().sum::<f64>() / ps.len() as f64);
                }
            }
            keys.blocks = block_keys(&keys.elements);
            let mut rank = std::mem::take(&mut ranks[r]);
            rank.sort_by(|a, b| keys.compare(path, a, b));
            ranks[r] = rank;
        }
        // The blocks' keys changed with every rank: every rank is sorted again, so that each block keeps one place.
        sort_all(ranks, &keys);
        let crossings = order_crossings(ranks, links);
        if crossings < best_crossings {
            best_crossings = crossings;
            best = ranks.to_vec();
            best_keys = keys.clone();
            idle = 0;
        } else {
            idle += 1;
        }
    }
    ranks.clone_from_slice(&best);
    best_keys
}

/// Swaps neighbours in a rank while that lowers their crossings with both neighbouring ranks (the transposition step of the
/// layered method), never across a group boundary. On the ranks in numbers (`improve_order`): `order` by rank, `pos` by
/// element.
fn transpose(
    order: &mut [Vec<u32>],
    pos: &mut [usize],
    above: &[Vec<u32>],
    below: &[Vec<u32>],
    groups: &[usize],
    frozen: &dyn Fn(usize) -> bool,
) {
    // Crossings of the sections of `u` with those of `v` when `u` lies before `v`, against one neighbouring rank.
    let pair = |u: u32, v: u32, side: &[Vec<u32>], pos: &[usize]| -> usize {
        let ev = &side[v as usize];
        side[u as usize]
            .iter()
            .map(|&a| {
                ev.iter()
                    .filter(|&&b| pos[b as usize] < pos[a as usize])
                    .count()
            })
            .sum()
    };
    for _ in 0..order.len() * 4 {
        let mut improved = false;
        for r in 0..order.len() {
            if frozen(r) {
                continue;
            }
            for i in 0..order[r].len().saturating_sub(1) {
                let (u, v) = (order[r][i], order[r][i + 1]);
                if groups[u as usize] != groups[v as usize] {
                    continue;
                }
                let cost = |u: u32, v: u32| pair(u, v, above, pos) + pair(u, v, below, pos);
                if cost(v, u) < cost(u, v) {
                    order[r].swap(i, i + 1);
                    pos[u as usize] = i + 1;
                    pos[v as usize] = i;
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
}

/// Experimental (consistent layered pipeline): the absolute place along its side of the port of every edge that ends at a
/// container with a border node for it in the container's plan (the lane of that node), and of every port of a leaf whose
/// place its area's plan set (`AreaPlan::port_offsets`); each with whether the plan's flow is vertical.
pub fn port_lanes(
    plans: &LayeredPlans,
    graph: &arqlet_model::Graph,
    rects: &BTreeMap<String, arqlet_model::Rect>,
) -> BTreeMap<(String, bool), (f64, bool)> {
    let mut lanes = BTreeMap::new();
    for edge in &graph.edges {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        for (end, source) in [(edge.source.as_str(), true), (target, false)] {
            let (Some(plan), Some(r)) = (plans.get(end), rects.get(end)) else {
                continue;
            };
            let Some(d) = plan.borders.get(&edge.id).and_then(|b| plan.dummies.get(b)) else {
                continue;
            };
            let lane = if plan.vertical {
                r.x + d.x + d.width / 2.
            } else {
                r.y + d.y + d.height / 2.
            };
            lanes.insert((edge.id.clone(), source), (lane, plan.vertical));
        }
    }
    // The ports of leaves at the places their area's plan gave them. Only the plan of the area that holds the leaf: an edge
    // built in legs (ADR 0017) is in the plans of several areas, and another area's offset is for its own element of the edge.
    let ends: BTreeMap<&str, (&str, Option<&str>)> = graph
        .edges
        .iter()
        .map(|e| (e.id.as_str(), (e.source.as_str(), e.target.as_deref())))
        .collect();
    let area_of: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref().unwrap_or("<root>")))
        .collect();
    for (area, plan) in plans {
        for ((edge, source), offset) in &plan.port_offsets {
            let Some(&(s, t)) = ends.get(edge.as_str()) else {
                continue;
            };
            let node = if *source { Some(s) } else { t };
            if node.and_then(|n| area_of.get(n)) != Some(&area.as_str()) {
                continue;
            }
            let Some(r) = node.and_then(|n| rects.get(n)) else {
                continue;
            };
            let start = if plan.vertical { r.x } else { r.y };
            lanes.insert((edge.clone(), *source), (start + offset, plan.vertical));
        }
    }
    // A leaf beside its side node (amendment A) has its port across from the side port, so the inner leg is straight.
    for (area, plan) in plans {
        let Some(origin) = rects.get(area.as_str()) else {
            continue;
        };
        for (edge, side) in &plan.sides {
            let Some(&(s, t)) = ends.get(edge.as_str()) else {
                continue;
            };
            for (node, source) in [(Some(s), true), (t, false)] {
                let Some(node) = node else { continue };
                if area_of.get(node) != Some(&area.as_str()) || !lateral(plan, side, node) {
                    continue;
                }
                if let Some(port) = side_port(plan, (origin.x, origin.y), side, None) {
                    let lane = if plan.vertical { port.y } else { port.x };
                    lanes.insert((edge.clone(), source), (lane, !plan.vertical));
                }
            }
        }
    }
    lanes
}

/// The order keys of the automatic ports of the edges inside the planned areas (plan section 4a, step 3): by (edge, the
/// port is the source end), the place, in its rank, of the next element of the edge's chain beyond the port's node: the first
/// dummy or the other end in the neighbouring rank. The ports of one node side are then ordered as the sections they start
/// leave or enter, so that they do not cross at the node. Only edges whose ends are children of one planned area get keys;
/// self-loops get none.
/// The side of an automatic port by (edge, source end) for an end inside an area that the edge leaves or enters through a
/// side node (single pass, ADR 0017): the side that area's own flow leaves by (side-out) or enters by (side-in). The area
/// nearest the end decides; the areas between it and the end share its axis. Empty without side nodes.
/// ADR 0017, amendment A: whether the side node `side` of this plan stands beside `end` (in its rank, without a chain), so that
/// the edge's inner leg is one straight segment across the flow.
pub fn lateral(plan: &AreaPlan, side: &str, end: &str) -> bool {
    !plan.chains.keys().any(|(a, b)| a == side || b == side)
        && plan
            .ranks
            .iter()
            .any(|rank| rank.iter().any(|id| id == side) && rank.iter().any(|id| id == end))
}

/// ADR 0017, amendment A3: the lane, across the area's flow, of a side node beside a child area that leaves by a border node
/// of its own for the edge: that border node's lane, so that the edge crosses the area straight. `rects` are absolute.
pub fn lateral_child_lane(
    plans: &LayeredPlans,
    rects: &BTreeMap<String, arqlet_model::Rect>,
    area: &str,
    side: &str,
    edge: &str,
) -> Option<f64> {
    let plan = plans.get(area)?;
    if plan.chains.keys().any(|(a, b)| a == side || b == side) {
        return None;
    }
    let rank = plan.ranks.iter().find(|r| r.iter().any(|x| x == side))?;
    rank.iter().find_map(|child| {
        let inner = plans.get(child)?;
        let d = inner.dummies.get(inner.borders.get(edge)?)?;
        let r = rects.get(child)?;
        Some(if plan.vertical {
            r.y + d.y + d.height / 2.
        } else {
            r.x + d.x + d.width / 2.
        })
    })
}

/// The border port of a side node (ADR 0017, decision 3): on the side node's own lane, staggered a track apart along the
/// area's flow, the first side node of its kind in the rank farthest along. `origin` is the area's absolute corner; `track`
/// the track spacing (`None`: the side node's length over the number of its peers, which is the same).
pub fn side_port(
    plan: &AreaPlan,
    origin: (f64, f64),
    side: &str,
    track: Option<f64>,
) -> Option<arqlet_model::Point> {
    let rect = plan.dummies.get(side)?;
    let kind = if side.starts_with(crate::SIDE_OUT_PREFIX) {
        crate::SIDE_OUT_PREFIX
    } else {
        crate::SIDE_IN_PREFIX
    };
    let peers: Vec<&String> = plan
        .ranks
        .iter()
        .find(|rank| rank.iter().any(|id| id == side))?
        .iter()
        .filter(|id| id.starts_with(kind))
        .collect();
    let n = peers.len().max(1);
    let k = peers.iter().position(|id| *id == side).unwrap_or(0);
    let depth = (n.saturating_sub(1 + k)) as f64 + 0.5;
    let (x, y) = (origin.0 + rect.x, origin.1 + rect.y);
    Some(if plan.vertical {
        let track = track.unwrap_or(rect.height / n as f64);
        arqlet_model::Point {
            x: x + rect.width / 2.,
            y: y + depth * track,
        }
    } else {
        let track = track.unwrap_or(rect.width / n as f64);
        arqlet_model::Point {
            x: x + depth * track,
            y: y + rect.height / 2.,
        }
    })
}

pub fn end_sides(
    plans: &LayeredPlans,
    graph: &arqlet_model::Graph,
    chosen_flows: bool,
) -> BTreeMap<(String, bool), arqlet_model::PortSide> {
    use arqlet_model::PortSide;
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    // The areas above a node, nearest first.
    let above = |id: &str| -> Vec<&str> {
        let mut out = Vec::new();
        let mut at = parents.get(id).copied().flatten();
        while let Some(area) = at {
            out.push(area);
            at = parents.get(area).copied().flatten();
        }
        out
    };
    let mut sides = BTreeMap::new();
    for edge in &graph.edges {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        for (end, is_source) in [(edge.source.as_str(), true), (target, false)] {
            // The search ends at an area of the other axis than its parent that the edge leaves by a border node of its own
            // flow (amendment A3): the end's port then follows the ordinary flow rule. A border node of an area of its
            // parent's axis is the ordinary case and does not end it.
            let found = above(end)
                .into_iter()
                .take_while(|area| {
                    let Some(plan) = plans.get(*area) else {
                        return true;
                    };
                    let up = parents.get(area).copied().flatten().unwrap_or("<root>");
                    let across = plans.get(up).is_some_and(|p| p.vertical != plan.vertical);
                    !(across && plan.borders.contains_key(&edge.id))
                })
                .find_map(|area| {
                    let plan = plans.get(area)?;
                    let node = plan.sides.get(&edge.id)?;
                    let leaving = node.starts_with(crate::SIDE_OUT_PREFIX);
                    // A side node beside its leaf (amendment A): the port faces the side, across the area's own flow.
                    let across = lateral(plan, node, end);
                    Some(match (plan.vertical != across, leaving) {
                        (true, true) => PortSide::Bottom,
                        (true, false) => PortSide::Top,
                        (false, true) => PortSide::Right,
                        (false, false) => PortSide::Left,
                    })
                });
            if let Some(side) = found {
                sides.insert((edge.id.clone(), is_source), side);
            }
        }
    }
    // With `chosen_flows` (single pass): an edge whose ends are children of an area that chose its direction (`auto`, allowed
    // sets) leaves and enters by the sides of the flow the area chose, as an edge of a fixed flow does; the ports would
    // otherwise face each other across the free direction, and the construction delegates them (`Delegation::PortSide`).
    if chosen_flows {
        for edge in &graph.edges {
            let Some(target) = edge.target.as_deref() else {
                continue;
            };
            if edge.source == target {
                continue;
            }
            let (Some(a), Some(b)) = (parents.get(edge.source.as_str()), parents.get(target))
            else {
                continue;
            };
            if a != b {
                continue;
            }
            let area = a.unwrap_or("<root>");
            let chooses = !matches!(graph.area_mode(*a), Ok(arqlet_model::Mode::Orientation(_)));
            let Some(plan) = plans.get(area).filter(|_| chooses) else {
                continue;
            };
            let reversed = plan
                .reversed_links
                .contains(&(edge.source.clone(), target.to_string()));
            let (out, into) = if plan.vertical {
                (PortSide::Bottom, PortSide::Top)
            } else {
                (PortSide::Right, PortSide::Left)
            };
            let (source_side, target_side) = if reversed { (into, out) } else { (out, into) };
            sides.entry((edge.id.clone(), true)).or_insert(source_side);
            sides.entry((edge.id.clone(), false)).or_insert(target_side);
        }
    }
    sides
}

pub fn port_keys(
    plans: &LayeredPlans,
    graph: &arqlet_model::Graph,
) -> BTreeMap<(String, bool), f64> {
    let parent: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref().unwrap_or("<root>")))
        .collect();
    let mut keys = BTreeMap::new();
    for edge in &graph.edges {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        if edge.source == target {
            continue;
        }
        // The smallest area that holds both ends, and the children of it that hold them (step 6: an end may lie inside a
        // child container; its port then follows the chain between the containers).
        let Some((area, source, target)) = plan_ends(plans, &parent, &edge.source, target) else {
            continue;
        };
        if source == target {
            continue;
        }
        let Some(plan) = plans.get(&area) else {
            continue;
        };
        // The chain in flow direction, and whether the edge runs with it.
        let logical = (source.to_string(), target.to_string());
        let (chain_ends, forward) = if plan.reversed_links.contains(&logical) {
            ((target.to_string(), source.to_string()), false)
        } else {
            (logical.clone(), true)
        };
        let Some(dummies) = plan.chains.get(&chain_ends) else {
            continue;
        };
        let mut chain: Vec<&str> = vec![chain_ends.0.as_str()];
        chain.extend(dummies.iter().map(String::as_str));
        chain.push(chain_ends.1.as_str());
        let place = |id: &str| -> Option<f64> {
            plan.ranks
                .iter()
                .find_map(|rank| rank.iter().position(|x| x == id))
                .map(|i| i as f64)
        };
        // The flow-start end looks at the element after it, the flow-end end at the element before it.
        let (start_key, end_key) = (place(chain[1]), place(chain[chain.len() - 2]));
        let (source_key, target_key) = if forward {
            (start_key, end_key)
        } else {
            (end_key, start_key)
        };
        if let Some(k) = source_key {
            keys.insert((edge.id.clone(), true), k);
        }
        if let Some(k) = target_key {
            keys.insert((edge.id.clone(), false), k);
        }
    }
    // An end in a container that has a border node for the edge (step 6) follows the chain to that node inside the
    // container instead.
    for edge in &graph.edges {
        let Some(target) = edge.target.as_deref() else {
            continue;
        };
        for (end, is_source) in [(edge.source.as_str(), true), (target, false)] {
            let Some(plan) = parent.get(end).and_then(|area| plans.get(*area)) else {
                continue;
            };
            let Some(border) = plan.borders.get(&edge.id) else {
                continue;
            };
            let key = if is_source {
                (end.to_string(), border.clone())
            } else {
                (border.clone(), end.to_string())
            };
            let Some(dummies) = plan.chains.get(&key) else {
                continue;
            };
            let next = if is_source {
                dummies.first().unwrap_or(border)
            } else {
                dummies.last().unwrap_or(border)
            };
            if let Some(i) = plan
                .ranks
                .iter()
                .find_map(|rank| rank.iter().position(|x| x == next))
            {
                keys.insert((edge.id.clone(), is_source), i as f64);
            }
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn links(v: &[(&str, &str)]) -> Vec<(String, String)> {
        v.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn hierarchical_order_keeps_nested_blocks_contiguous_in_one_order_in_every_rank() {
        // Blocks: C holds c1 (rank 0), c2 (rank 1) and the block D; D holds d1 (rank 0), d2 (rank 1). x and y are outside.
        let mut ranks = vec![ids(&["d1", "x", "c1"]), ids(&["y", "c2", "d2"])];
        let path = |id: &str| -> Vec<String> {
            match id {
                "c1" | "c2" => ids(&["C"]),
                "d1" | "d2" => ids(&["C", "D"]),
                _ => vec![],
            }
        };
        // x links to d2 and y to c1, which pulls the blocks apart in the plain order.
        let l = links(&[("c1", "c2"), ("d1", "d2"), ("x", "d2"), ("c1", "y")]);
        let keys = hierarchical_order(&mut ranks, &l, &path, None);
        for rank in &ranks {
            // The members of C (c*, d*) are contiguous, and inside C the members of D are contiguous.
            let in_c: Vec<usize> = rank
                .iter()
                .enumerate()
                .filter(|(_, id)| !path(id).is_empty())
                .map(|(i, _)| i)
                .collect();
            assert!(in_c.windows(2).all(|w| w[1] == w[0] + 1), "{rank:?}");
        }
        // Every rank follows the one order of the keys, so blocks that share ranks keep one order in all of them.
        for rank in &ranks {
            let mut sorted = rank.clone();
            sorted.sort_by(|a, b| keys.compare(&path, a, b));
            assert_eq!(&sorted, rank);
        }
        assert!(keys.blocks.contains_key("C") && keys.blocks.contains_key("D"));
    }

    #[test]
    fn follow_order_keeps_the_earlier_order_and_places_new_ids_after_their_predecessor() {
        let ranks = vec![ids(&["a", "b", "c"]), ids(&["x", "new", "y", "z"])];
        let previous = vec![ids(&["c", "a", "b"]), ids(&["z", "y", "x"])];
        let followed = follow_order(&ranks, &previous, &|_| 0);
        assert_eq!(followed[0], ids(&["c", "a", "b"]));
        // "new" stood after "x" and follows it to the end of the rank.
        assert_eq!(followed[1], ids(&["z", "y", "x", "new"]));
        // Groups stay in separate blocks whatever the earlier order.
        let grouped = follow_order(&ranks[..1], &previous[..1], &|id| usize::from(id == "c"));
        assert_eq!(grouped[0], ids(&["a", "b", "c"]));
    }

    #[test]
    fn crossings_are_counted_between_neighbouring_ranks() {
        let ranks = vec![ids(&["a", "b"]), ids(&["c", "d"])];
        assert_eq!(
            order_crossings(&ranks, &links(&[("a", "d"), ("b", "c")])),
            1
        );
        assert_eq!(
            order_crossings(&ranks, &links(&[("a", "c"), ("b", "d")])),
            0
        );
        // Sections that share an end do not cross.
        assert_eq!(
            order_crossings(&ranks, &links(&[("a", "c"), ("a", "d")])),
            0
        );
    }

    #[test]
    fn the_improved_order_removes_avoidable_crossings_and_keeps_the_best() {
        // Two chains drawn crossed: a -> d -> f and b -> c -> e. Untangling them is crossing free.
        let mut ranks = vec![ids(&["a", "b"]), ids(&["c", "d"]), ids(&["e", "f"])];
        let l = links(&[("a", "d"), ("d", "f"), ("b", "c"), ("c", "e")]);
        assert!(order_crossings(&ranks, &l) > 0);
        improve_order(&mut ranks, &l, &|_| 0);
        assert_eq!(order_crossings(&ranks, &l), 0);
    }

    #[test]
    fn the_improved_order_keeps_frozen_ranks_and_untangles_the_others() {
        // The crossed chains of the test above, with the first and last rank frozen: only the middle rank may move.
        let mut ranks = vec![ids(&["a", "b"]), ids(&["c", "d"]), ids(&["f", "e"])];
        let l = links(&[("a", "d"), ("d", "f"), ("b", "c"), ("c", "e")]);
        assert!(order_crossings(&ranks, &l) > 0);
        improve_order_around(&mut ranks, &l, &|_| 0, &[true, false, true]);
        assert_eq!(ranks[0], ids(&["a", "b"]));
        assert_eq!(ranks[2], ids(&["f", "e"]));
        assert_eq!(ranks[1], ids(&["d", "c"]));
        assert_eq!(order_crossings(&ranks, &l), 0);
    }

    #[test]
    fn the_improved_order_never_raises_the_crossings() {
        // A complete bipartite pair of ranks: every order has the same crossings; the order must not get worse.
        let mut ranks = vec![ids(&["a", "b", "c"]), ids(&["d", "e", "f"])];
        let l = links(&[
            ("a", "d"),
            ("a", "e"),
            ("a", "f"),
            ("b", "d"),
            ("b", "e"),
            ("c", "f"),
        ]);
        let before = order_crossings(&ranks, &l);
        improve_order(&mut ranks, &l, &|_| 0);
        assert!(order_crossings(&ranks, &l) <= before);
    }

    #[test]
    fn groups_stay_in_separate_blocks() {
        let mut ranks = vec![ids(&["a", "x"]), ids(&["y", "b"])];
        let l = links(&[("a", "b"), ("x", "y")]);
        let group = |id: &str| usize::from(id == "x" || id == "y");
        improve_order(&mut ranks, &l, &group);
        for rank in &ranks {
            let groups: Vec<usize> = rank.iter().map(|id| group(id)).collect();
            let mut sorted = groups.clone();
            sorted.sort();
            assert_eq!(groups, sorted);
        }
    }
}

/// The links the cycle break reversed in an area: those known before the placement (`reversals`), else those of its plan.
pub(crate) fn reversed_links<'a>(
    plans: &'a LayeredPlans,
    reversals: &'a BTreeMap<String, BTreeSet<(String, String)>>,
    area: &str,
) -> Option<&'a BTreeSet<(String, String)>> {
    reversals
        .get(area)
        .or_else(|| plans.get(area).map(|plan| &plan.reversed_links))
}
