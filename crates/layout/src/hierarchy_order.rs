//! Provisional (plan section 4a, consistent layered pipeline, step 1; option `hierarchicalOrdering`): one order for the
//! plans of all areas together.
//!
//! Every container keeps its own ranks. A container with border nodes stands in its parent's plan as one element; the
//! edges that cross its border leave or enter it at the places of their border nodes in its first or last rank (its
//! "sub-places"). A sweep over an area that reaches a container orders the container's border rank by the places of the
//! edges outside, sweeps the container in the same direction, and then lets the parent count the container's edges at the
//! sub-places the container's order gives them. The crossings are counted over all plans with these places, and the best
//! order of all plans together is kept. Deterministic.

use crate::layered_plan::LayeredPlans;
use std::collections::BTreeMap;

/// Where the edges of a container meet its parent's chains: by container, by its neighbour in the parent's plan (the
/// element next to it on a chain, before or after it), the border nodes of the edges that run through that neighbour.
#[derive(Debug, Default)]
struct Ports {
    /// By edge, the element at the other end of its chain in the parent's plan (where the edge comes from or goes to).
    far: BTreeMap<String, String>,
    /// Neighbours after the container (its edges leave by its last rank): neighbour → (edge, border node).
    leaving: BTreeMap<String, Vec<(String, String)>>,
    /// Neighbours before the container (its edges enter by its first rank): neighbour → (edge, border node).
    entering: BTreeMap<String, Vec<(String, String)>>,
}

/// The containers of every area that stand in it with border nodes, and how their edges meet the area's chains.
fn ports_of(
    plans: &LayeredPlans,
    parent_of: &BTreeMap<String, String>,
    edges: &BTreeMap<String, (String, String)>,
) -> BTreeMap<String, Ports> {
    let mut out: BTreeMap<String, Ports> = BTreeMap::new();
    for (container, inner) in plans {
        // A container of another axis meets its parent at its side nodes (ADR 0017) instead of border nodes.
        let sides = !inner.sides.is_empty();
        if inner.borders.is_empty() && !sides {
            continue;
        }
        let Some(area) = parent_of.get(container) else {
            continue;
        };
        let Some(outer) = plans.get(area) else {
            continue;
        };
        if !outer.ranks.iter().flatten().any(|id| id == container) {
            continue;
        }
        let first = inner.ranks.first();
        let mut ports = Ports::default();
        let nodes = if sides { &inner.sides } else { &inner.borders };
        for (edge, border) in nodes {
            let Some((source, target)) = edges.get(edge) else {
                continue;
            };
            let entering = if sides {
                border.starts_with(crate::SIDE_IN_PREFIX)
            } else {
                first.is_some_and(|rank| rank.contains(border))
            };
            // The end outside the container (a backward edge enters the container that holds its source).
            let inside =
                |node: &str| node == container || child_of(container, node, parent_of).is_some();
            let other = if inside(source) { target } else { source };
            // The other end's element in the area: the child that holds it, or the area's own border node for the edge.
            let element =
                child_of(area, other, parent_of).or_else(|| outer.borders.get(edge).cloned());
            let Some(element) = element else { continue };
            let neighbour = outer.chains.iter().find_map(|((a, b), dummies)| {
                if a == container && *b == element {
                    Some(dummies.first().unwrap_or(b).clone())
                } else if *a == element && b == container {
                    Some(dummies.last().unwrap_or(a).clone())
                } else {
                    None
                }
            });
            let Some(neighbour) = neighbour else { continue };
            ports.far.insert(edge.clone(), element.clone());
            let side = if entering {
                &mut ports.entering
            } else {
                &mut ports.leaving
            };
            side.entry(neighbour)
                .or_default()
                .push((edge.clone(), border.clone()));
        }
        out.insert(container.clone(), ports);
    }
    out
}

/// The child of `area` that holds `node` (or `node` itself when it is a child), if any.
fn child_of(area: &str, node: &str, parent_of: &BTreeMap<String, String>) -> Option<String> {
    let mut at = node.to_string();
    loop {
        let up = parent_of.get(&at)?;
        if up == area {
            return Some(at);
        }
        at = up.clone();
    }
}

/// `child_of` without copying: the child of `area` that holds `node`.
fn child_ref<'a>(
    area: &str,
    node: &'a str,
    parent_of: &'a BTreeMap<String, String>,
) -> Option<&'a str> {
    let mut at = node;
    loop {
        let up = parent_of.get(at)?;
        if up == area {
            return Some(at);
        }
        at = up;
    }
}

/// The weight of every section of every area: the number of edges that run through it (a chain between two containers
/// carries all the edges between their insides, and two such chains that cross are as many crossings as the product of
/// their edges).
fn weights_of(
    plans: &LayeredPlans,
    parent_of: &BTreeMap<String, String>,
    edges: &BTreeMap<String, (String, String)>,
) -> BTreeMap<String, BTreeMap<(String, String), usize>> {
    // An edge counts only in the areas that hold one of its ends: elsewhere both ends fall back to the edge's border node
    // (or to nothing), and an edge whose ends are one element is no section.
    let mut per_chain: BTreeMap<&str, BTreeMap<&(String, String), usize>> = BTreeMap::new();
    let mut areas: Vec<&str> = Vec::new();
    for (edge, (source, target)) in edges {
        areas.clear();
        for node in [source, target] {
            let mut at = node.as_str();
            while let Some(up) = parent_of.get(at) {
                if !areas.contains(&up.as_str()) {
                    areas.push(up);
                }
                at = up;
            }
        }
        for &area in &areas {
            let Some(plan) = plans.get(area) else {
                continue;
            };
            let border = plan.borders.get(edge).map(String::as_str);
            let a = child_ref(area, source, parent_of).or(border);
            let b = child_ref(area, target, parent_of).or(border);
            let (Some(a), Some(b)) = (a, b) else {
                continue;
            };
            if a == b {
                continue;
            }
            // The first chain in key order that joins the two, either way round.
            let key = [(a, b), (b, a)]
                .into_iter()
                .filter_map(|(x, y)| plan.chains.get_key_value(&(x.to_string(), y.to_string())))
                .map(|(key, _)| key)
                .min();
            if let Some(key) = key {
                *per_chain.entry(area).or_default().entry(key).or_default() += 1;
            }
        }
    }
    let mut out: BTreeMap<String, BTreeMap<(String, String), usize>> = BTreeMap::new();
    for (area, plan) in plans {
        let per_chain = per_chain.remove(area.as_str()).unwrap_or_default();
        let weights = out.entry(area.clone()).or_default();
        for ((a, b), dummies) in &plan.chains {
            let w = per_chain
                .get(&(a.clone(), b.clone()))
                .copied()
                .unwrap_or(1)
                .max(1);
            let mut previous = a.clone();
            for d in dummies.iter().chain(std::iter::once(b)) {
                weights.insert((previous.clone(), d.clone()), w);
                previous = d.clone();
            }
        }
    }
    out
}

/// One area in numbers (see `Model`): its elements by index, their ranks and places, its sections with their weights, and
/// where a container's edges meet its parent.
struct Area {
    /// The elements' ids; an element's index is its place here.
    elems: Vec<String>,
    /// By element: the index of the area it stands for, when it is a container with border nodes.
    container: Vec<Option<usize>>,
    /// The ranks, each in its current order.
    ranks: Vec<Vec<u32>>,
    /// By element: its rank and its place in it.
    rank_of: Vec<u32>,
    pos: Vec<u32>,
    /// The sections (upper element, lower element, weight), in the order of the plan's chains.
    sections: Vec<(u32, u32, f64)>,
    /// By element: the sections it ends (for sweeps along the flow) and begins (against it), as indices into `sections`.
    ends: Vec<Vec<u32>>,
    begins: Vec<Vec<u32>>,
    /// By rank `r`: the sections between rank `r` and `r + 1`.
    between: Vec<Vec<u32>>,
    /// As a container in its parent: by the neighbour (an element of the parent), the edges that leave by the last rank
    /// or enter by the first, each with its border node here; and by edge, the element at the other end of its chain in
    /// the parent.
    leaving: BTreeMap<u32, Vec<(u32, u32)>>,
    entering: BTreeMap<u32, Vec<(u32, u32)>>,
    far: BTreeMap<u32, u32>,
    /// By element: whether it is a side node entering (`Some(false)`) or leaving (`Some(true)`) the area (ADR 0017).
    side: Vec<Option<bool>>,
    /// By side node: the child it is linked to.
    side_child: Vec<Option<u32>>,
    /// The area meets its parent at side nodes instead of border nodes.
    side_ports: bool,
}

impl Area {
    /// The place (0..1) along the area's flow of the port of side node `b`: its rank, then its depth among the side nodes of
    /// its kind in that rank, the first across the flow deepest (as `prepare_leg` staggers the ports).
    fn side_place(&self, b: u32) -> f64 {
        let r = self.rank_of[b as usize] as usize;
        let block = self.side_block(r, self.side[b as usize]);
        let j = block.iter().position(|&x| x == b).unwrap_or(0) as f64;
        let n = block.len().max(1) as f64;
        (r as f64 + (n - 1. - j + 0.5) / n) / self.ranks.len().max(1) as f64
    }

    /// The side nodes of `kind` in rank `r`, in the order the placement gives them: by their children's places.
    fn side_block(&self, r: usize, kind: Option<bool>) -> Vec<u32> {
        let mut block: Vec<u32> = self.ranks[r]
            .iter()
            .copied()
            .filter(|&x| self.side[x as usize] == kind)
            .collect();
        block.sort_by_key(|&x| {
            let child = self.side_child[x as usize];
            (child.map_or(u32::MAX, |c| self.pos[c as usize]), x)
        });
        block
    }

    /// Puts the side nodes of rank `r` on their sides, the entering ones first and the leaving ones last, each block in the
    /// order of their children (as the placement does), around the other elements in their new order.
    fn keep_sides(&self, r: usize, order: Vec<u32>) -> Vec<u32> {
        let inside = order
            .into_iter()
            .filter(|&x| self.side[x as usize].is_none());
        self.side_block(r, Some(false))
            .into_iter()
            .chain(inside)
            .chain(self.side_block(r, Some(true)))
            .collect()
    }
    fn set_rank(&mut self, r: usize, order: Vec<u32>) {
        for (i, &e) in order.iter().enumerate() {
            self.pos[e as usize] = i as u32;
        }
        self.ranks[r] = order;
    }
}

/// All plans in numbers, so that the sweeps and counts look nothing up by name.
struct Model {
    areas: Vec<Area>,
}

impl Model {
    /// The sub-place (0..1) of a container's edges towards `neighbour`: the mean place of their border nodes in the
    /// container's first (entering) or last (leaving) rank; 0.5 when the container has none.
    fn sub_place(&self, c: usize, neighbour: u32, leaving: bool) -> f64 {
        let inner = &self.areas[c];
        let side = if leaving {
            &inner.leaving
        } else {
            &inner.entering
        };
        let Some(borders) = side.get(&neighbour) else {
            return 0.5;
        };
        if inner.side_ports {
            let sum: f64 = borders.iter().map(|(_, b)| inner.side_place(*b)).sum();
            return sum / borders.len().max(1) as f64;
        }
        let Some(r) = (if leaving {
            inner.ranks.len().checked_sub(1)
        } else {
            (!inner.ranks.is_empty()).then_some(0)
        }) else {
            return 0.5;
        };
        let len = inner.ranks[r].len() as f64;
        let (mut sum, mut count) = (0., 0usize);
        for (_, b) in borders {
            if inner.rank_of[*b as usize] as usize == r {
                sum += (inner.pos[*b as usize] as f64 + 0.5) / len;
                count += 1;
            }
        }
        if count == 0 {
            0.5
        } else {
            sum / count as f64
        }
    }

    /// By edge, the place (0..1) of its border node in a container's last (`leaving`) or first rank, for the edges between
    /// the container and `neighbour`; `None` when it has none.
    fn edge_places(&self, c: usize, neighbour: u32, leaving: bool) -> Option<EdgePlaces<'_>> {
        let inner = &self.areas[c];
        let side = if leaving {
            &inner.leaving
        } else {
            &inner.entering
        };
        let list = side.get(&neighbour)?;
        if inner.side_ports {
            return Some(EdgePlaces {
                area: inner,
                list,
                rank: 0,
                len: 1.,
            });
        }
        let r = if leaving {
            inner.ranks.len().checked_sub(1)?
        } else {
            (!inner.ranks.is_empty()).then_some(0)?
        };
        Some(EdgePlaces {
            area: inner,
            list,
            rank: r,
            len: inner.ranks[r].len() as f64,
        })
    }

    /// The place of `element` in its rank of `area` as seen from `neighbour`: its index plus the sub-place of a container,
    /// relative to the rank's length.
    fn place(&self, area: usize, element: u32, neighbour: u32, leaving: bool) -> f64 {
        let a = &self.areas[area];
        let len = a.ranks[a.rank_of[element as usize] as usize].len() as f64;
        let sub = match a.container[element as usize] {
            Some(c) => self.sub_place(c, neighbour, leaving),
            None => 0.5,
        };
        (a.pos[element as usize] as f64 + sub) / len
    }

    /// One sweep over `area` along the flow (`down`) or against it, descending into its containers.
    fn sweep(&mut self, area: usize, down: bool) {
        let n = self.areas[area].ranks.len();
        if n == 0 {
            return;
        }
        // The first rank of the sweep is fixed (a border rank set by the parent, or the start); its containers are
        // swept before the next rank looks at them.
        let first = if down { 0 } else { n - 1 };
        self.descend(area, first, down);
        let order: Vec<usize> = if down {
            (1..n).collect()
        } else {
            (0..n - 1).rev().collect()
        };
        for r in order {
            let fixed = if down { r - 1 } else { r + 1 };
            let rank = self.areas[area].ranks[r].clone();
            let len = rank.len() as f64;
            let mut keyed: Vec<(f64, u32, u32)> = Vec::with_capacity(rank.len());
            for (i, &id) in rank.iter().enumerate() {
                let a = &self.areas[area];
                let list = if down {
                    &a.ends[id as usize]
                } else {
                    &a.begins[id as usize]
                };
                let (mut total, mut weighted, mut any) = (0., 0., false);
                for &k in list {
                    let (u, l, w) = a.sections[k as usize];
                    let x = if down { u } else { l };
                    if a.rank_of[x as usize] as usize != fixed {
                        continue;
                    }
                    // A neighbour container is seen at the sub-place of its edges towards this element (it leaves towards
                    // it in a sweep along the flow, enters from it against); a section counts with its edges.
                    total += w;
                    weighted += self.place(area, x, id, down) * w;
                    any = true;
                }
                let key = if any {
                    weighted / total
                } else {
                    (i as f64 + 0.5) / len
                };
                keyed.push((key, i as u32, id));
            }
            // Every rank after the sweep's first follows its neighbours, the border rank at the sweep's far end too (the
            // parent only sets the border rank the sweep starts from, in `descend`), so that the next container along the
            // sweep sees the order this one ends with.
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            let mut order: Vec<u32> = keyed.into_iter().map(|k| k.2).collect();
            if self.areas[area].side_ports {
                order = self.areas[area].keep_sides(r, order);
            }
            self.areas[area].set_rank(r, order);
            self.descend(area, r, down);
        }
    }

    /// The containers of rank `r` of `area`: their border rank on the sweep's side is ordered by the places of their
    /// edges' neighbours outside, and they are swept in the same direction.
    fn descend(&mut self, area: usize, r: usize, down: bool) {
        let rank = self.areas[area].ranks[r].clone();
        for element in rank {
            let Some(c) = self.areas[area].container[element as usize] else {
                continue;
            };
            // By border node: the place of its edge's neighbour outside, then (for the edges of one chain, which share the
            // neighbour) the place of the edge at the chain's other end, so that a bundle keeps one order along its chain.
            let mut key: BTreeMap<u32, (f64, f64)> = BTreeMap::new();
            {
                let inner = &self.areas[c];
                let side = if down {
                    &inner.entering
                } else {
                    &inner.leaving
                };
                for (&neighbour, borders) in side {
                    let p = self.place(area, neighbour, element, !down);
                    for &(edge, b) in borders {
                        let far = inner
                            .far
                            .get(&edge)
                            .and_then(|&f| self.areas[area].container[f as usize]);
                        let other = far.and_then(|fc| {
                            self.edge_places(fc, neighbour, down)
                                .and_then(|m| m.place(edge))
                                .or_else(|| {
                                    // The far end is the neighbour's own chain end: seen from a container there, at the
                                    // place of the edge's border node on that container's side towards us.
                                    let fa = &self.areas[fc];
                                    let list = if down { &fa.leaving } else { &fa.entering };
                                    let fr = if down {
                                        fa.ranks.len().checked_sub(1)?
                                    } else {
                                        (!fa.ranks.is_empty()).then_some(0)?
                                    };
                                    let (_, border) =
                                        list.values().flatten().find(|(e, _)| *e == edge)?;
                                    (fa.rank_of[*border as usize] as usize == fr).then(|| {
                                        (fa.pos[*border as usize] as f64 + 0.5)
                                            / fa.ranks[fr].len() as f64
                                    })
                                })
                        });
                        key.insert(b, (p, other.unwrap_or(0.5)));
                    }
                }
            }
            let inner = &mut self.areas[c];
            if inner.side_ports {
                // The children with side nodes on the sweep's side, by the place of their edges' neighbours outside (the mean
                // over their side nodes), within the places those children hold in their rank: their side nodes follow them,
                // so the ports follow the neighbours' order, as a border rank does for a container of the same axis.
                let kind = Some(!down);
                let mut child_keys: BTreeMap<u32, (f64, usize)> = BTreeMap::new();
                for (x, child) in inner.side_child.iter().enumerate() {
                    let (Some(child), true) = (child, inner.side[x] == kind) else {
                        continue;
                    };
                    let Some(&(p, _)) = key.get(&(x as u32)) else {
                        continue;
                    };
                    let entry = child_keys.entry(*child).or_insert((0., 0));
                    entry.0 += p;
                    entry.1 += 1;
                }
                for r in 0..inner.ranks.len() {
                    let rank = inner.ranks[r].clone();
                    let mut keyed: Vec<(f64, usize, u32)> = rank
                        .iter()
                        .enumerate()
                        .filter_map(|(i, x)| {
                            child_keys.get(x).map(|&(sum, n)| (sum / n as f64, i, *x))
                        })
                        .collect();
                    if keyed.len() < 2 {
                        continue;
                    }
                    keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
                    let mut sorted = keyed.iter().map(|k| k.2);
                    let order: Vec<u32> = rank
                        .iter()
                        .map(|x| {
                            if child_keys.contains_key(x) {
                                sorted.next().unwrap()
                            } else {
                                *x
                            }
                        })
                        .collect();
                    inner.set_rank(r, order);
                }
                for r in 0..inner.ranks.len() {
                    let order = inner.keep_sides(r, inner.ranks[r].clone());
                    inner.set_rank(r, order);
                }
                self.sweep(c, down);
                continue;
            }
            let at = if down {
                0
            } else {
                inner.ranks.len().saturating_sub(1)
            };
            if at < inner.ranks.len() {
                // The elements of a rank are distinct, so the current place breaks every tie.
                let mut keyed: Vec<((f64, f64), usize, u32)> = inner.ranks[at]
                    .iter()
                    .enumerate()
                    .map(|(i, &x)| (key.get(&x).copied().unwrap_or((2., 0.)), i, x))
                    .collect();
                keyed.sort_by(|a, b| {
                    a.0 .0
                        .total_cmp(&b.0 .0)
                        .then(a.0 .1.total_cmp(&b.0 .1))
                        .then(a.1.cmp(&b.1))
                });
                inner.set_rank(at, keyed.into_iter().map(|k| k.2).collect());
            }
            self.sweep(c, down);
        }
    }

    /// The crossings of all plans: between neighbouring ranks of every area, a container's edges counted at their
    /// sub-places.
    fn crossings(&self) -> usize {
        let mut scratch = Scratch::default();
        (0..self.areas.len())
            .map(|a| {
                (0..self.areas[a].ranks.len().saturating_sub(1))
                    .map(|r| self.pair_crossings(a, r, &mut scratch))
                    .sum::<usize>()
            })
            .sum()
    }

    /// The crossings of `area` between its ranks `r` and `r + 1`, a container's edges counted edge by edge at their border
    /// nodes and any other section with its weight.
    fn pair_crossings(&self, area: usize, r: usize, scratch: &mut Scratch) -> usize {
        let a = &self.areas[area];
        let (na, nb) = (a.ranks[r].len() as f64, a.ranks[r + 1].len() as f64);
        let Scratch {
            pairs,
            above,
            below,
            lows,
            tree,
            ranks,
        } = scratch;
        pairs.clear();
        for &k in &a.between[r] {
            let (u, l, w) = a.sections[k as usize];
            let sa = a.container[u as usize].and_then(|c| self.edge_places(c, l, true));
            let sb = a.container[l as usize].and_then(|c| self.edge_places(c, u, false));
            if sa.is_none() && sb.is_none() {
                pairs.push((
                    self.place(area, u, l, true),
                    self.place(area, l, u, false),
                    w as usize,
                ));
                continue;
            }
            let (ia, ib) = (a.pos[u as usize] as f64, a.pos[l as usize] as f64);
            // Every edge of either side once, in the order of its index, at its first place on each side (0.5 on a
            // side without it).
            for (m, out) in [(&sa, &mut *above), (&sb, &mut *below)] {
                out.clear();
                if let Some(m) = m {
                    m.places(out);
                }
            }
            let (mut i, mut j) = (0, 0);
            while i < above.len() || j < below.len() {
                let e = match (above.get(i), below.get(j)) {
                    (Some(x), Some(y)) => x.0.min(y.0),
                    (Some(x), None) => x.0,
                    (None, Some(y)) => y.0,
                    (None, None) => unreachable!(),
                };
                let at = |list: &[(u32, f64)], k: &mut usize| match list.get(*k) {
                    Some(&(x, p)) if x == e => {
                        *k += 1;
                        p
                    }
                    _ => 0.5,
                };
                let (p, q) = (at(above, &mut i), at(below, &mut j));
                pairs.push(((ia + p) / na, (ib + q) / nb, 1));
            }
        }
        weighted_inversions(pairs, lows, tree, ranks)
    }
}

/// Reused buffers of the crossing count.
#[derive(Default)]
struct Scratch {
    pairs: Vec<(f64, f64, usize)>,
    above: Vec<(u32, f64)>,
    below: Vec<(u32, f64)>,
    lows: Vec<f64>,
    tree: Vec<usize>,
    ranks: Vec<usize>,
}

/// A container's edges towards one neighbour that have their border node in its first (entering) or last (leaving)
/// rank, with their places (0..1) there.
struct EdgePlaces<'a> {
    area: &'a Area,
    list: &'a [(u32, u32)],
    rank: usize,
    len: f64,
}

impl EdgePlaces<'_> {
    fn on_rank(&self) -> impl Iterator<Item = (u32, f64)> + '_ {
        let sides = self.area.side_ports;
        self.list
            .iter()
            .filter(move |(_, b)| sides || self.area.rank_of[*b as usize] as usize == self.rank)
            .map(move |&(e, b)| {
                if sides {
                    (e, self.area.side_place(b))
                } else {
                    (e, (self.area.pos[b as usize] as f64 + 0.5) / self.len)
                }
            })
    }

    fn place(&self, edge: u32) -> Option<f64> {
        self.on_rank().find(|(e, _)| *e == edge).map(|(_, p)| p)
    }

    /// By edge index, each edge once at its first place.
    fn places(&self, out: &mut Vec<(u32, f64)>) {
        out.extend(self.on_rank());
        out.sort_by_key(|&(e, _)| e);
        out.dedup_by_key(|&mut (e, _)| e);
    }
}

/// The weighted crossings of segments between two parallel lines given by their places on each (a pair of segments crosses
/// when their order differs on the two lines; equal places do not cross), in O(n log n): by place on the first line, a
/// Fenwick tree over the places on the second sums the weights of the earlier segments that lie after on the second.
fn weighted_inversions(
    pairs: &mut [(f64, f64, usize)],
    lows: &mut Vec<f64>,
    tree: &mut Vec<usize>,
    ranks: &mut Vec<usize>,
) -> usize {
    if pairs.len() < 2 {
        return 0;
    }
    lows.clear();
    lows.extend(pairs.iter().map(|p| p.1));
    lows.sort_by(f64::total_cmp);
    lows.dedup_by(|a, b| (*a - *b).abs() <= 1e-12);
    let rank_of = |v: f64| lows.partition_point(|&x| x < v - 1e-12);
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
    tree.clear();
    tree.resize(lows.len() + 1, 0);
    let add = |tree: &mut Vec<usize>, mut i: usize, w: usize| {
        i += 1;
        while i < tree.len() {
            tree[i] += w;
            i += i & i.wrapping_neg();
        }
    };
    let sum = |tree: &Vec<usize>, mut i: usize| {
        // The weights at places 0..i.
        let mut s = 0;
        while i > 0 {
            s += tree[i];
            i -= i & i.wrapping_neg();
        }
        s
    };
    let (mut total, mut inserted) = (0usize, 0usize);
    let mut i = 0;
    while i < pairs.len() {
        // Segments with one place on the first line do not cross each other: they are counted before any is inserted.
        let mut j = i;
        while j < pairs.len() && (pairs[j].0 - pairs[i].0).abs() <= 1e-12 {
            j += 1;
        }
        ranks.clear();
        ranks.extend(pairs[i..j].iter().map(|p| rank_of(p.1)));
        for (p, &r) in pairs[i..j].iter().zip(ranks.iter()) {
            // The earlier segments placed strictly after on the second line.
            total += p.2 * (inserted - sum(tree, r + 1));
        }
        for (p, &r) in pairs[i..j].iter().zip(ranks.iter()) {
            add(tree, r, p.2);
            inserted += p.2;
        }
        i = j;
    }
    total
}

/// The plans in numbers (`Model`), with the containers' ports and the sections' weights; the area indices follow the
/// plans' order.
fn model_of(
    plans: &LayeredPlans,
    ports: &BTreeMap<String, Ports>,
    weights: &BTreeMap<String, BTreeMap<(String, String), usize>>,
    edges: &BTreeMap<String, (String, String)>,
) -> Model {
    let area_index: BTreeMap<&String, usize> =
        plans.keys().enumerate().map(|(i, a)| (a, i)).collect();
    let edge_index: BTreeMap<&String, u32> = edges
        .keys()
        .enumerate()
        .map(|(i, e)| (e, i as u32))
        .collect();
    let mut areas: Vec<Area> = Vec::with_capacity(plans.len());
    let mut elem_index: Vec<BTreeMap<String, u32>> = Vec::with_capacity(plans.len());
    for (name, plan) in plans {
        let mut elems: Vec<String> = Vec::new();
        let mut index: BTreeMap<String, u32> = BTreeMap::new();
        let mut ranks: Vec<Vec<u32>> = Vec::with_capacity(plan.ranks.len());
        let (mut rank_of, mut pos) = (Vec::new(), Vec::new());
        for (r, rank) in plan.ranks.iter().enumerate() {
            let mut ids = Vec::with_capacity(rank.len());
            for (i, id) in rank.iter().enumerate() {
                let k = elems.len() as u32;
                elems.push(id.clone());
                index.insert(id.clone(), k);
                rank_of.push(r as u32);
                pos.push(i as u32);
                ids.push(k);
            }
            ranks.push(ids);
        }
        let n = elems.len();
        let side: Vec<Option<bool>> = elems
            .iter()
            .map(|id| {
                if id.starts_with(crate::SIDE_IN_PREFIX) {
                    Some(false)
                } else if id.starts_with(crate::SIDE_OUT_PREFIX) {
                    Some(true)
                } else {
                    None
                }
            })
            .collect();
        let w = weights.get(name);
        let weight = |a: &str, b: &str| -> f64 {
            w.and_then(|w| w.get(&(a.to_string(), b.to_string())))
                .or_else(|| w.and_then(|w| w.get(&(b.to_string(), a.to_string()))))
                .copied()
                .unwrap_or(1) as f64
        };
        let mut sections = Vec::new();
        let (mut ends, mut begins) = (vec![Vec::new(); n], vec![Vec::new(); n]);
        let mut between = vec![Vec::new(); plan.ranks.len().saturating_sub(1)];
        for (a, b) in plan.sections() {
            let (Some(&u), Some(&l)) = (index.get(&a), index.get(&b)) else {
                continue;
            };
            let k = sections.len() as u32;
            sections.push((u, l, weight(&a, &b)));
            ends[l as usize].push(k);
            begins[u as usize].push(k);
            let (ru, rl) = (rank_of[u as usize] as usize, rank_of[l as usize] as usize);
            if rl == ru + 1 {
                between[ru].push(k);
            }
        }
        let side_child = {
            let mut side_child = vec![None; n];
            for &(u, l, _) in &sections {
                if side[u as usize].is_some() {
                    side_child[u as usize] = Some(l);
                }
                if side[l as usize].is_some() {
                    side_child[l as usize] = Some(u);
                }
            }
            side_child
        };
        areas.push(Area {
            container: vec![None; n],
            elems,
            ranks,
            rank_of,
            pos,
            sections,
            ends,
            begins,
            between,
            leaving: BTreeMap::new(),
            entering: BTreeMap::new(),
            far: BTreeMap::new(),
            side_child,
            side,
            side_ports: !plan.sides.is_empty(),
        });
        elem_index.push(index);
    }
    // The containers' ports, in the numbers of both areas.
    for (container, p) in ports {
        let (Some(&c), Some(parent)) = (
            area_index.get(container),
            parents_of(plans, container, &area_index),
        ) else {
            continue;
        };
        let Some(&ce) = elem_index[parent].get(container) else {
            continue;
        };
        areas[parent].container[ce as usize] = Some(c);
        let convert =
            |side: &BTreeMap<String, Vec<(String, String)>>| -> BTreeMap<u32, Vec<(u32, u32)>> {
                side.iter()
                    .filter_map(|(neighbour, list)| {
                        let n = *elem_index[parent].get(neighbour)?;
                        let list = list
                            .iter()
                            .filter_map(|(e, b)| {
                                Some((*edge_index.get(e)?, *elem_index[c].get(b)?))
                            })
                            .collect();
                        Some((n, list))
                    })
                    .collect()
            };
        areas[c].leaving = convert(&p.leaving);
        areas[c].entering = convert(&p.entering);
        areas[c].far = p
            .far
            .iter()
            .filter_map(|(e, f)| Some((*edge_index.get(e)?, *elem_index[parent].get(f)?)))
            .collect();
    }
    Model { areas }
}

/// The index of the area that holds `container` as an element.
fn parents_of(
    plans: &LayeredPlans,
    container: &str,
    area_index: &BTreeMap<&String, usize>,
) -> Option<usize> {
    plans
        .iter()
        .find(|(_, plan)| plan.ranks.iter().flatten().any(|id| id == container))
        .and_then(|(a, _)| area_index.get(a).copied())
}

/// The runs of the hierarchical sweep.
pub const RUNS: usize = 8;
/// The runs of the hierarchical sweep in single pass.
pub const RUNS_SINGLE_PASS: usize = 5;
/// Sweeps without improvement that end a run.
pub const IDLE_SWEEPS: usize = 4;
/// Sweeps without improvement that end a run in single pass: as many as `IDLE_SWEEPS`. One was chosen on 2026-11-06
/// (crossings -4.3 % on 34 uniform views, benchmarks/results/single-pass-levers-20261106.md); with the borders and side nodes of
/// 2026-11-08, four draw fewer crossings: on the 31 uniform views 9093 -> 8566 over five seeds (bends +1.6 %), on the 11
/// mixed datasets every view better than the refined default (uml-class 8.6 -> 6.6), the frozen views 3944 -> 3848
/// (benchmarks/results/single-pass-idle-20261108.md). Time not measured on a quiet machine.
pub const IDLE_SWEEPS_SINGLE_PASS: usize = IDLE_SWEEPS;

/// Orders the plans of all areas together (see the module documentation): sweeps along the flow and against it over every
/// area that does not stand with border nodes in a parent's plan, descending into the containers; the best order of all
/// plans (fewest crossings, counted with the sub-places) is kept; `idle_limit` sweeps without improvement end a run. Returns the
/// crossings before and after. `parent_of` gives every node's area (`"<root>"` for the top level); `edges` every edge's
/// source and target. The work runs on the plans in numbers (`Model`).
pub fn hierarchical_sweep(
    plans: &mut LayeredPlans,
    parent_of: &BTreeMap<String, String>,
    edges: &BTreeMap<String, (String, String)>,
    runs: usize,
    idle_limit: usize,
    seed: u32,
) -> (usize, usize) {
    let ports = ports_of(plans, parent_of, edges);
    let weights = weights_of(plans, parent_of, edges);
    let mut model = model_of(plans, &ports, &weights, edges);
    let names: Vec<String> = plans.keys().cloned().collect();
    let tops: Vec<usize> = names
        .iter()
        .enumerate()
        .filter(|(_, area)| !ports.contains_key(*area))
        .map(|(i, _)| i)
        .collect();
    let snapshot =
        |m: &Model| -> Vec<Vec<Vec<u32>>> { m.areas.iter().map(|a| a.ranks.clone()).collect() };
    let restore = |m: &mut Model, s: &[Vec<Vec<u32>>]| {
        for (a, ranks) in m.areas.iter_mut().zip(s) {
            for (r, order) in ranks.iter().enumerate() {
                a.set_rank(r, order.clone());
            }
        }
    };
    let before = model.crossings();
    let start = snapshot(&model);
    let mut best = start.clone();
    let mut best_crossings = before;
    // Several runs (step 3): from the given order sweeping along the flow first, against it first, from the top areas'
    // ranks reversed, and from shuffled orders; eight in all (measured on nine views against three: fewer crossings and
    // bends in sum). Switches of neighbours that look at both ranks lowered these counts but not the drawn crossings, and
    // were left out. The fewest crossings win; ties keep the earlier run. `runs` is the number of runs: eight, or five in
    // single pass (measured on 34 views over five shuffle seeds: as few crossings and bends as eight, `RUNS_SINGLE_PASS`).
    for run in 0..runs {
        restore(&mut model, &start);
        if run == 2 {
            for &a in &tops {
                for r in 0..model.areas[a].ranks.len() {
                    let mut order = model.areas[a].ranks[r].clone();
                    order.reverse();
                    model.areas[a].set_rank(r, order);
                }
            }
        }
        // Further runs start from every rank of every area shuffled (deterministically, by the run's number and the option
        // `order_seed`; seed 0 gives the shuffles used before the option existed).
        if run >= 3 {
            let mut seed: u64 = 0x9e37_79b9_7f4a_7c15 ^ run as u64 ^ (u64::from(seed) << 32);
            let mut next = move || {
                seed ^= seed << 13;
                seed ^= seed >> 7;
                seed ^= seed << 17;
                seed
            };
            for a in model.areas.iter_mut() {
                for r in 0..a.ranks.len() {
                    let mut order = a.ranks[r].clone();
                    for i in (1..order.len()).rev() {
                        let j = (next() % (i as u64 + 1)) as usize;
                        order.swap(i, j);
                    }
                    a.set_rank(r, order);
                }
            }
        }
        let mut run_best = snapshot(&model);
        let mut run_crossings = model.crossings();
        let (mut idle, mut sweep) = (0, 0);
        while run_crossings > 0 && idle < idle_limit && sweep < 16 {
            let down = (sweep + usize::from(run == 1)) % 2 == 0;
            sweep += 1;
            for &a in &tops {
                model.sweep(a, down);
            }
            let crossings = model.crossings();
            if crossings < run_crossings {
                run_crossings = crossings;
                run_best = snapshot(&model);
                idle = 0;
            } else {
                idle += 1;
            }
        }
        if run_crossings < best_crossings {
            best_crossings = run_crossings;
            best = run_best;
        }
    }
    // The best orders back into the plans.
    for ((name, plan), (area, ranks)) in plans.iter_mut().zip(model.areas.iter().zip(&best)) {
        let _ = name;
        plan.ranks = ranks
            .iter()
            .map(|order| {
                order
                    .iter()
                    .map(|&e| area.elems[e as usize].clone())
                    .collect()
            })
            .collect();
    }
    (before, best_crossings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layered_plan::AreaPlan;
    use std::collections::BTreeSet;

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// A root area with a container C (two inner nodes c1, c2 leaving by border nodes) and two targets x, y: the border
    /// order inside C first disagrees with the order of x and y outside.
    #[test]
    fn the_sweep_agrees_a_containers_border_order_with_the_order_outside() {
        let mut plans = LayeredPlans::new();
        // Root: rank 0 C, rank 1 x y; chains C -> x (edge e1) and C -> y (edge e2).
        plans.insert(
            "<root>".into(),
            AreaPlan {
                ranks: vec![ids(&["C"]), ids(&["x", "y"])],
                chains: BTreeMap::from([
                    (("C".into(), "x".into()), vec![]),
                    (("C".into(), "y".into()), vec![]),
                ]),
                ..Default::default()
            },
        );
        // C: rank 0 c2 c1, rank 1 its leaving border nodes; e1 leaves from c1, e2 from c2. The border rank starts in the
        // order b1 b2, which crosses inside (c2 above c1) or outside (x above y).
        plans.insert(
            "C".into(),
            AreaPlan {
                ranks: vec![ids(&["c2", "c1"]), ids(&["b1", "b2"])],
                chains: BTreeMap::from([
                    (("c1".into(), "b1".into()), vec![]),
                    (("c2".into(), "b2".into()), vec![]),
                ]),
                borders: BTreeMap::from([("e1".into(), "b1".into()), ("e2".into(), "b2".into())]),
                reversed_links: BTreeSet::new(),
                ..Default::default()
            },
        );
        let parent_of: BTreeMap<String, String> = [
            ("C", "<root>"),
            ("x", "<root>"),
            ("y", "<root>"),
            ("c1", "C"),
            ("c2", "C"),
        ]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .collect();
        let edges: BTreeMap<String, (String, String)> = [("e1", "c1", "x"), ("e2", "c2", "y")]
            .into_iter()
            .map(|(e, s, t)| (e.to_string(), (s.to_string(), t.to_string())))
            .collect();
        let (before, after) =
            hierarchical_sweep(&mut plans, &parent_of, &edges, RUNS, IDLE_SWEEPS, 0);
        assert_eq!(before, 1, "the border order crosses inside C");
        assert_eq!(after, 0, "{plans:?}");
    }
}
