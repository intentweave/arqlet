//! Provisional (#209): the places of an earlier overview given by node id (`OverviewStart::nodes`), read onto the
//! hierarchy of the graph as it is now, so that the overview of a changed graph continues without a session.
//!
//! - A node of the graph continues from its entry when that lies in the earlier disc of its container (the nearest
//!   container from the graph; every disc of the top level). Without an entry, in a container without one, or outside
//!   its container's disc (it changed its container), it is placed anew.
//! - A cluster the overview added has an id that is a position in a clustering and may name other members on another
//!   revision, so its place follows from its members: the earlier cluster disc that fits them best says which of them
//!   stayed (the others are placed anew), and that disc is taken as it was when it still fits them exactly (or has the
//!   same id and holds them). Otherwise the cluster's disc is the one around its members' earlier places.
//!
//! Everything runs in index or id order, so the result is the same on every platform.
use super::{enclosing_centre, Container, Hierarchy, OverviewOptions, Previous, COMMUNITY_PREFIX};
use arqlet_model::{OverviewContinuation, OverviewPlace};
use std::collections::{BTreeMap, BTreeSet};

/// How far off a place of a previous answer may be: its numbers are rounded to hundredths.
const TOLERANCE: f64 = 0.05;

#[derive(Clone, Copy)]
struct Disc {
    x: f64,
    y: f64,
    r: f64,
}

impl Disc {
    /// Whether the point lies in the disc. Discs of one container do not overlap, so a node's centre lies in the discs
    /// of the clusters that hold it and in no other.
    fn holds(&self, (x, y): (f64, f64)) -> bool {
        let (dx, dy) = (x - self.x, y - self.y);
        (dx * dx + dy * dy).sqrt() <= self.r + TOLERANCE
    }
}

/// Discs by the cell of their centre, in grids of doubling cell size, to find those that hold a point without looking
/// at all of them.
#[derive(Default)]
struct Discs {
    discs: Vec<Disc>,
    /// Per level: the cell size, and the discs per cell.
    grids: BTreeMap<u32, (f64, BTreeMap<(i64, i64), Vec<usize>>)>,
}

fn cell_of(x: f64, y: f64, size: f64) -> (i64, i64) {
    ((x / size).floor() as i64, (y / size).floor() as i64)
}

impl Discs {
    fn insert(&mut self, disc: Disc) {
        // The cell is at least as wide as the disc reaches, so a point it holds is in a neighbouring cell at most.
        let (mut level, mut size) = (0u32, 1f64);
        while size < disc.r + TOLERANCE && size.is_finite() {
            size *= 2.;
            level += 1;
        }
        let index = self.discs.len();
        self.discs.push(disc);
        self.grids
            .entry(level)
            .or_insert_with(|| (size, BTreeMap::new()))
            .1
            .entry(cell_of(disc.x, disc.y, size))
            .or_default()
            .push(index);
    }

    /// The discs that hold the point, in the order they were inserted.
    fn holding(&self, p: (f64, f64)) -> Vec<usize> {
        let mut out = Vec::new();
        for (size, cells) in self.grids.values() {
            let (cx, cy) = cell_of(p.0, p.1, *size);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    let key = (cx.saturating_add(dx), cy.saturating_add(dy));
                    for &i in cells.get(&key).map_or(&[][..], Vec::as_slice) {
                        if self.discs[i].holds(p) {
                            out.push(i);
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        // Cells clamped at the ends of the number range may have been read twice.
        out.dedup();
        out
    }
}

/// The distance from `centre` to the farthest point of the discs.
fn reach(centre: (f64, f64), discs: &[Disc]) -> f64 {
    discs
        .iter()
        .map(|d| {
            let (dx, dy) = (d.x - centre.0, d.y - centre.1);
            (dx * dx + dy * dy).sqrt() + d.r
        })
        .fold(0., f64::max)
}

/// Marks the containers whose children, at their continued places, no longer fit together (a node that moved up out
/// of a container still lies in its disc), and a child's own container when its disc holds a sibling's whole.
fn mark_overlaps(
    h: &Hierarchy,
    frame: &[Option<Disc>],
    anew: &[bool],
    changed: &mut BTreeSet<Container>,
) {
    let containers = std::iter::once(Container::Root).chain((0..h.ids.len()).map(Container::Node));
    for container in containers {
        let mut discs: Vec<(Disc, usize)> = h
            .members(container)
            .iter()
            .filter(|&&m| !anew[m])
            .filter_map(|&m| Some((frame[m]?, m)))
            .collect();
        discs.sort_by(|a, b| {
            (a.0.x - a.0.r)
                .total_cmp(&(b.0.x - b.0.r))
                .then(a.1.cmp(&b.1))
        });
        for (k, &(a, i)) in discs.iter().enumerate() {
            for &(b, j) in &discs[k + 1..] {
                if b.x - b.r >= a.x + a.r {
                    break;
                }
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let apart = (dx * dx + dy * dy).sqrt();
                if apart >= a.r + b.r - TOLERANCE {
                    continue;
                }
                changed.insert(container);
                for (outer, inner, node) in [(a, b, i), (b, a, j)] {
                    if apart + inner.r <= outer.r + TOLERANCE && !h.children[node].is_empty() {
                        changed.insert(Container::Node(node));
                    }
                }
            }
        }
    }
}

/// The `Previous` of `h` from earlier places by node id, and what became of them.
pub fn continue_by_id(
    h: &Hierarchy,
    places: &BTreeMap<String, OverviewPlace>,
    options: &OverviewOptions,
) -> (Previous, OverviewContinuation) {
    let n = h.ids.len();
    let disc_of = |p: &OverviewPlace| Disc {
        x: p.x,
        y: p.y,
        r: p.r,
    };
    // The disc of every node as it was: a graph node's entry; an added cluster's is set below.
    let mut frame: Vec<Option<Disc>> = (0..n)
        .map(|i| {
            (i < h.real)
                .then(|| places.get(&h.ids[i]).map(disc_of))
                .flatten()
        })
        .collect();
    let mut anew: Vec<bool> = frame.iter().map(Option::is_none).collect();

    // The nearest container from the graph above a node: the clusters the overview added are passed over.
    let owner_of = |i: usize| {
        let mut at = h.parent[i];
        while let Some(p) = at.filter(|&p| p >= h.real) {
            at = h.parent[p];
        }
        at
    };
    // Graph nodes with an entry, by their container.
    let mut owned: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in (0..h.real).filter(|&i| frame[i].is_some()) {
        if let Some(owner) = owner_of(i) {
            owned.entry(owner).or_default().push(i);
        }
    }
    for (owner, members) in owned {
        let Some(disc) = frame[owner] else {
            // A new container: what it holds is placed anew.
            for m in members {
                anew[m] = true;
            }
            continue;
        };
        let centre = |m: usize| frame[m].map_or((0., 0.), |d| (d.x, d.y));
        let outside: Vec<usize> = members
            .iter()
            .copied()
            .filter(|&m| !disc.holds(centre(m)))
            .collect();
        // A disc smaller than what it holds (a host set its radius, `radii`) has its members around it: it does not
        // tell who is new. It does when it holds half of them, or when they gather somewhere else.
        let count = members.len() as f64;
        let middle = members.iter().fold((0., 0.), |sum, &m| {
            (sum.0 + centre(m).0 / count, sum.1 + centre(m).1 / count)
        });
        if 2 * (members.len() - outside.len()) >= members.len() || !disc.holds(middle) {
            for m in outside {
                anew[m] = true;
            }
        }
    }

    // The earlier discs of clusters the overview added, by id; whatever their ids name now.
    let real: BTreeSet<&str> = h.ids[..h.real].iter().map(String::as_str).collect();
    let mut old = Discs::default();
    let mut old_ids: Vec<&str> = Vec::new();
    for (id, place) in places {
        if id.starts_with(COMMUNITY_PREFIX) && !real.contains(id.as_str()) {
            old.insert(disc_of(place));
            old_ids.push(id);
        }
    }
    // For every graph node that stayed, the earlier cluster discs it lay in whole (a disc inside a container may hold
    // the container's centre, never the container); and per container, how many of its nodes each of those held.
    let held: Vec<Vec<usize>> = (0..h.real)
        .map(|i| match frame[i] {
            Some(d) if !anew[i] => old
                .holding((d.x, d.y))
                .into_iter()
                .filter(|&k| {
                    reach((old.discs[k].x, old.discs[k].y), &[d]) <= old.discs[k].r + TOLERANCE
                })
                .collect(),
            _ => Vec::new(),
        })
        .collect();
    let mut content: BTreeMap<(usize, Option<usize>), u64> = BTreeMap::new();
    for (i, list) in held.iter().enumerate() {
        for &d in list {
            *content.entry((d, owner_of(i))).or_default() += 1;
        }
    }

    let mut changed: BTreeSet<Container> = BTreeSet::new();
    // Added clusters from the deepest up, so that every cluster's members have their discs.
    let depth = |mut i: usize| {
        let mut d = 0usize;
        while let Some(p) = h.parent[i] {
            d += 1;
            i = p;
        }
        d
    };
    let mut order: Vec<(usize, usize)> = (h.real..n).map(|c| (depth(c), c)).collect();
    order.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    // The graph nodes that stayed below each added cluster (through added clusters only).
    let mut below: Vec<Vec<usize>> = vec![Vec::new(); n - h.real];
    for (_, c) in order {
        let mut under = Vec::new();
        for &m in &h.children[c] {
            if m >= h.real {
                under.extend_from_slice(&below[m - h.real]);
            } else if !anew[m] {
                under.push(m);
            }
        }
        let mut shared: BTreeMap<usize, u64> = BTreeMap::new();
        for &m in &under {
            for &d in &held[m] {
                *shared.entry(d).or_default() += 1;
            }
        }
        // The earlier cluster that fits best (its home): at least half of these nodes lay in it, and the largest share of
        // what is in either, counting the nodes of this container (Jaccard); ties go to the smaller disc, then the same
        // id, then the earlier id.
        let size = under.len() as u64;
        let owner = owner_of(c);
        let same = |d: usize| old_ids[d] == h.ids[c];
        let home = shared
            .iter()
            .filter(|&(_, &both)| 2 * both >= size)
            .map(|(&d, &both)| (d, both, size + content[&(d, owner)] - both))
            .min_by(|a, b| {
                (b.1 * a.2)
                    .cmp(&(a.1 * b.2))
                    .then(old.discs[a.0].r.total_cmp(&old.discs[b.0].r))
                    .then(same(b.0).cmp(&same(a.0)))
                    .then(a.0.cmp(&b.0))
            })
            .map(|(d, _, _)| d);
        let same_id = home.is_some_and(same);
        let home = home.map(|d| old.discs[d]);
        let mut kept = Vec::new();
        for &m in &h.children[c] {
            let Some(d) = frame[m].filter(|_| !anew[m]) else {
                continue;
            };
            if home.is_some_and(|home| !home.holds((d.x, d.y))) {
                anew[m] = true;
            } else {
                kept.push(d);
            }
        }
        below[c - h.real] = under;
        if kept.is_empty() {
            anew[c] = true;
            continue;
        }
        let padding = if kept.len() == 1 && options.chains {
            0.
        } else {
            options.padding
        };
        // The home as it was, when it has the same id or is the disc around exactly these members.
        let as_it_was = home
            .filter(|d| same_id || (reach((d.x, d.y), &kept) + padding - d.r).abs() <= TOLERANCE);
        anew[c] = false;
        frame[c] = as_it_was.or_else(|| {
            let x: Vec<(f64, f64)> = kept.iter().map(|d| (d.x, d.y)).collect();
            let r: Vec<f64> = kept.iter().map(|d| d.r).collect();
            let at = enclosing_centre(&x, &r);
            // A disc that was in no earlier answer: its container looks for overlaps.
            changed.insert(h.parent[c].map_or(Container::Root, Container::Node));
            Some(Disc {
                x: at.0,
                y: at.1,
                r: reach(at, &kept) + padding,
            })
        });
    }

    // A container a node left (removed, or moved elsewhere) has changed, though no radius in it may have: the smallest
    // disc that held the node's place.
    let mut containers = Discs::default();
    let mut nodes: Vec<usize> = Vec::new();
    for i in (0..n).filter(|&i| !h.children[i].is_empty()) {
        if let Some(d) = frame[i] {
            containers.insert(d);
            nodes.push(i);
        }
    }
    let mut ignored = Vec::new();
    let index: BTreeMap<&str, usize> = h
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    for (id, place) in places {
        let node = index.get(id.as_str()).copied();
        if node.is_none() && id.starts_with(COMMUNITY_PREFIX) {
            continue;
        }
        if node.is_none() {
            ignored.push(id.clone());
        }
        if node.is_some_and(|i| i >= h.real || !anew[i]) {
            continue;
        }
        // The node itself, and what it holds, is no container it left.
        let inside = |mut c: usize| loop {
            if Some(c) == node {
                break true;
            }
            match h.parent[c] {
                Some(p) => c = p,
                None => break false,
            }
        };
        let left = containers
            .holding((place.x, place.y))
            .into_iter()
            .filter(|&k| !inside(nodes[k]))
            .min_by(|&a, &b| {
                containers.discs[a]
                    .r
                    .total_cmp(&containers.discs[b].r)
                    .then(a.cmp(&b))
            });
        changed.insert(left.map_or(Container::Root, |k| Container::Node(nodes[k])));
    }

    mark_overlaps(h, &frame, &anew, &mut changed);

    let continued = anew[..h.real].iter().filter(|a| !**a).count();
    let previous = Previous {
        centre: frame
            .iter()
            .map(|d| d.map_or((0., 0.), |d| (d.x, d.y)))
            .collect(),
        radius: frame.iter().map(|d| d.map_or(0., |d| d.r)).collect(),
        anew: (0..n).filter(|&i| anew[i]).collect(),
        changed,
        ..Previous::default()
    };
    let report = OverviewContinuation {
        continued: continued as u32,
        anew: (h.real - continued) as u32,
        ignored,
    };
    (previous, report)
}
