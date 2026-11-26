//! Provisional (#209): the places of an earlier overview given by node id (`OverviewStart::nodes`), read onto the graph
//! as it is now, so that the overview of a changed graph continues without a session.
//!
//! - The clusters the overview added continue: their ids are positions in a clustering, and a changed graph clustered
//!   anew would group much of it differently. Discs of one container do not overlap, so the earlier discs tell which
//!   nodes each cluster held (`clusters_by_id`). A node without such a place joins the cluster of the nodes it is linked
//!   to most. Only a container without earlier clusters is clustered as a new graph is.
//! - A node continues from its entry when that lies in the earlier disc of its container (the nearest container from
//!   the graph; every disc of the top level). Without an entry, in a container without one, outside its container's disc
//!   (it changed its container) or in a cluster made now, it is placed anew (`continue_by_id`).
//!
//! Everything runs in index or id order, so the result is the same on every platform.
use super::{
    build, with_communities_where, with_satellites_where, Container, Hierarchy, Hubs, Previous,
    COMMUNITY_PREFIX,
};
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
    fn of(p: &OverviewPlace) -> Self {
        Disc {
            x: p.x,
            y: p.y,
            r: p.r,
        }
    }

    fn distance(&self, (x, y): (f64, f64)) -> f64 {
        let (dx, dy) = (x - self.x, y - self.y);
        (dx * dx + dy * dy).sqrt()
    }

    /// Whether the point lies in the disc.
    fn holds(&self, p: (f64, f64)) -> bool {
        self.distance(p) <= self.r + TOLERANCE
    }

    /// Whether the other disc lies in this one whole.
    fn holds_disc(&self, other: &Disc) -> bool {
        self.distance((other.x, other.y)) + other.r <= self.r + TOLERANCE
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

    /// The smallest disc (ties: the one inserted first) that holds the point and passes `also`.
    fn smallest(&self, p: (f64, f64), also: impl Fn(usize, &Disc) -> bool) -> Option<usize> {
        let mut best: Option<usize> = None;
        for (size, cells) in self.grids.values() {
            let (cx, cy) = cell_of(p.0, p.1, *size);
            for dx in -1..=1 {
                for dy in -1..=1 {
                    // Saturating: cells are clamped at the ends of the number range.
                    let key = (cx.saturating_add(dx), cy.saturating_add(dy));
                    for &i in cells.get(&key).map_or(&[][..], Vec::as_slice) {
                        let d = &self.discs[i];
                        let smaller = best.is_none_or(|b| {
                            d.r.total_cmp(&self.discs[b].r).then(i.cmp(&b)).is_lt()
                        });
                        if smaller && d.holds(p) && also(i, d) {
                            best = Some(i);
                        }
                    }
                }
            }
        }
        best
    }
}

/// The nearest container from the graph above a node: the clusters the overview added are passed over.
fn owner_of(h: &Hierarchy, i: usize) -> Option<usize> {
    let mut at = h.parent[i];
    while let Some(p) = at.filter(|&p| p >= h.real) {
        at = h.parent[p];
    }
    at
}

/// Which nodes of the graph have no place to continue from in their container: those without an entry, those in a
/// container without one, and those whose entry lies outside their container's disc.
fn without_place(h: &Hierarchy, frame: &[Option<Disc>]) -> Vec<bool> {
    let mut anew: Vec<bool> = frame[..h.real].iter().map(Option::is_none).collect();
    // Nodes with an entry, by their container.
    let mut owned: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in (0..h.real).filter(|&i| frame[i].is_some()) {
        if let Some(owner) = owner_of(h, i) {
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
    anew
}

/// The label of a cluster the overview added.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Label {
    /// `<level>.<k>`, a community.
    Community(u32, u32),
    /// `unlinked`, the children without links.
    Unlinked,
    /// `sat.<k>`, a hub with its satellites.
    Satellites(u32),
}

/// `community:<container>:<label>` as (container, label).
fn parse(id: &str) -> Option<(&str, Label)> {
    let (container, label) = id.strip_prefix(COMMUNITY_PREFIX)?.rsplit_once(':')?;
    let label = if label == "unlinked" {
        Label::Unlinked
    } else {
        let (a, b) = label.split_once('.')?;
        if a == "sat" {
            Label::Satellites(b.parse().ok()?)
        } else {
            Label::Community(a.parse().ok()?, b.parse().ok()?)
        }
    };
    Some((container, label))
}

/// The hierarchy of a graph whose earlier places are given by id: the clusters of those places with the nodes they
/// held, new nodes joined to the cluster of the nodes they are linked to most (of its container), and in containers
/// without earlier clusters new ones as `with_communities_except` and `with_satellites` make them. An earlier cluster
/// with fewer than two children is dissolved. `communities` or `satellites` 0 keeps no cluster of that sort. Returns
/// the hierarchy and the indices of the clusters kept. For an unchanged graph and options it is the hierarchy those
/// two make, in the same order.
pub fn clusters_by_id(
    base: Hierarchy,
    places: &BTreeMap<String, OverviewPlace>,
    communities: usize,
    hubs: &Hubs,
    satellites: f64,
) -> Result<(Hierarchy, BTreeSet<usize>), String> {
    let n = base.ids.len();
    let index: BTreeMap<&str, usize> = base
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    let frame: Vec<Option<Disc>> = base
        .ids
        .iter()
        .map(|id| places.get(id).map(Disc::of))
        .collect();
    let anew = without_place(&base, &frame);

    // The earlier clusters of containers the graph still has, in id order: (id, disc, container).
    let mut old: Vec<(&str, Disc, Option<usize>)> = Vec::new();
    for (id, place) in places {
        if index.contains_key(id.as_str()) {
            continue;
        }
        let Some((mut container, label)) = parse(id) else {
            continue;
        };
        let wanted = match label {
            Label::Satellites(_) => satellites > 0.,
            _ => communities > 0,
        };
        // A cluster inside a cluster names it; the container from the graph is at the end of that chain.
        while !index.contains_key(container) {
            match parse(container) {
                Some((outer, _)) => container = outer,
                None => break,
            }
        }
        let owner = index.get(container).copied();
        let known = match owner {
            Some(o) => frame[o].is_some() && !base.children[o].is_empty(),
            None => container == "#",
        };
        if wanted && known {
            old.push((id, Disc::of(place), owner));
        }
    }
    let mut grids: BTreeMap<Option<usize>, (Discs, Vec<usize>)> = BTreeMap::new();
    for (c, (_, disc, owner)) in old.iter().enumerate() {
        let (grid, list) = grids.entry(*owner).or_default();
        grid.insert(*disc);
        list.push(c);
    }
    // The smallest cluster of the container whose disc holds this one whole and passes `also`.
    let around = |owner: Option<usize>, disc: &Disc, also: &dyn Fn(&Disc) -> bool| {
        let (grid, list) = grids.get(&owner)?;
        grid.smallest((disc.x, disc.y), |_, d| d.holds_disc(disc) && also(d))
            .map(|k| list[k])
    };
    // The cluster around each cluster, and the cluster of each node that stayed.
    let mut outer: Vec<Option<usize>> = old
        .iter()
        .map(|(_, disc, owner)| around(*owner, disc, &|d| d.r > disc.r))
        .collect();
    let mut cluster: Vec<Option<usize>> = (0..n)
        .map(|i| {
            let disc = frame[i].filter(|_| !anew[i])?;
            around(base.parent[i], &disc, &|_| true)
        })
        .collect();

    // New nodes (and nodes new to their container) join where the nodes they are linked to most are.
    for (&owner, (_, list)) in &grids {
        let container = owner.map_or(Container::Root, Container::Node);
        let members = base.members(container);
        if !members.iter().any(|&m| anew[m]) {
            continue;
        }
        let mut adjacent: BTreeMap<usize, Vec<(usize, f64)>> = BTreeMap::new();
        for &(a, b, w) in base.container_links(container) {
            adjacent.entry(a).or_default().push((b, w));
            adjacent.entry(b).or_default().push((a, w));
        }
        let mut done: BTreeSet<usize> = members.iter().copied().filter(|&m| !anew[m]).collect();
        let linked_to = |m: usize| adjacent.get(&m).map_or(&[][..], Vec::as_slice);
        let mut queue: std::collections::VecDeque<usize> = members
            .iter()
            .copied()
            .filter(|&m| anew[m] && linked_to(m).iter().any(|(o, _)| done.contains(o)))
            .collect();
        while let Some(m) = queue.pop_front() {
            if done.contains(&m) {
                continue;
            }
            // The weight per cluster among the linked nodes that have their place (no cluster counts as one choice);
            // a tie goes to no cluster, else to the earlier one.
            let mut weight: BTreeMap<Option<usize>, f64> = BTreeMap::new();
            for &(o, w) in linked_to(m) {
                if done.contains(&o) {
                    *weight.entry(cluster[o]).or_default() += w;
                }
            }
            let mut best: Option<(Option<usize>, f64)> = None;
            for (c, w) in weight {
                if best.is_none_or(|(_, most)| w > most) {
                    best = Some((c, w));
                }
            }
            cluster[m] = best.and_then(|(c, _)| c);
            done.insert(m);
            queue.extend(
                linked_to(m)
                    .iter()
                    .map(|&(o, _)| o)
                    .filter(|o| !done.contains(o)),
            );
        }
        // Without any link: with the unlinked ones, where there is such a cluster.
        let unlinked = list
            .iter()
            .copied()
            .find(|&c| outer[c].is_none() && matches!(parse(old[c].0), Some((_, Label::Unlinked))));
        for &m in members {
            if !done.contains(&m) && linked_to(m).is_empty() {
                cluster[m] = unlinked;
            }
        }
    }

    // Dissolve, from the smallest up, every cluster left with fewer than two children.
    let mut children = vec![0usize; old.len()];
    for c in cluster.iter().flatten().chain(outer.iter().flatten()) {
        children[*c] += 1;
    }
    let mut order: Vec<usize> = (0..old.len()).collect();
    order.sort_by(|&a, &b| old[a].1.r.total_cmp(&old[b].1.r).then(a.cmp(&b)));
    let mut alive = vec![true; old.len()];
    for c in order {
        if children[c] < 2 {
            alive[c] = false;
            if let Some(p) = outer[c] {
                // The cluster around it loses it and gains what it held (a cluster is dissolved after all in it).
                children[p] = children[p] + children[c] - 1;
            }
        }
    }
    let surviving = |mut c: Option<usize>| {
        while let Some(k) = c.filter(|&k| !alive[k]) {
            c = outer[k];
        }
        c
    };
    for c in &mut cluster {
        *c = surviving(*c);
    }
    outer = outer.iter().map(|&p| surviving(p)).collect();

    // The hierarchy with the clusters kept after the graph's nodes.
    let mut at = vec![None; old.len()];
    let mut ids = base.ids.clone();
    for c in (0..old.len()).filter(|&c| alive[c]) {
        at[c] = Some(ids.len());
        ids.push(old[c].0.to_string());
    }
    let mut parent = base.parent.clone();
    for i in 0..n {
        if let Some(c) = cluster[i] {
            parent[i] = at[c];
        }
    }
    let mut settled: BTreeSet<Container> = BTreeSet::new();
    for c in (0..old.len()).filter(|&c| alive[c]) {
        parent.push(outer[c].map_or(old[c].2, |p| at[p]));
        // Neither the kept cluster nor its container is clustered again.
        settled.extend(at[c].map(Container::Node));
        settled.insert(old[c].2.map_or(Container::Root, Container::Node));
    }
    let kept = ids.len() - n;
    let h = build(ids, parent, base.edges, base.weights, base.real);
    let h = with_communities_where(h, communities, hubs, &|c| !settled.contains(&c))?;
    let h = with_satellites_where(h, satellites, &|c| !settled.contains(&c))?;
    Ok(in_cluster_order(h, n..n + kept))
}

/// The hierarchy with its added clusters in the order `with_communities` and then `with_satellites` make them in
/// (communities by container, higher levels first, then the unlinked ones; satellite groups by container), so that a
/// continuation answers in the same order whichever clusters were kept. Returns it with the new indices of `kept`.
fn in_cluster_order(h: Hierarchy, kept: std::ops::Range<usize>) -> (Hierarchy, BTreeSet<usize>) {
    let n = h.ids.len();
    let index: BTreeMap<&str, usize> = h
        .ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    // The container the id names, as its index; the top level last.
    let named = |c: usize| {
        let (container, label) = parse(&h.ids[c])?;
        let at = if container == "#" {
            usize::MAX
        } else {
            index.get(container).copied().unwrap_or(usize::MAX - 1)
        };
        Some((at, label))
    };
    let mut communities: Vec<((usize, u32, u32), usize)> = Vec::new();
    for c in h.real..n {
        match named(c) {
            Some((at, Label::Community(level, k))) => {
                communities.push(((at, u32::MAX - level.min(u32::MAX - 1) - 1, k), c));
            }
            Some((at, Label::Unlinked)) => communities.push(((at, u32::MAX, 0), c)),
            _ => {}
        }
    }
    communities.sort_unstable();
    let mut order: Vec<usize> = (0..h.real).collect();
    order.extend(communities.iter().map(|&(_, c)| c));
    // Satellite groups name their container by its index once the communities are in place.
    let place: BTreeMap<usize, usize> =
        order.iter().enumerate().map(|(new, &c)| (c, new)).collect();
    let mut groups: Vec<((usize, u32), usize)> = (h.real..n)
        .filter_map(|c| match named(c) {
            Some((at, Label::Satellites(k))) => {
                Some(((place.get(&at).copied().unwrap_or(at), k), c))
            }
            _ => None,
        })
        .collect();
    groups.sort_unstable();
    order.extend(groups.iter().map(|&(_, c)| c));
    // Every added cluster has such an id; any other would keep its place at the end.
    let listed: BTreeSet<usize> = order.iter().copied().collect();
    order.extend((h.real..n).filter(|c| !listed.contains(c)));
    let mut new = vec![0usize; n];
    for (i, &c) in order.iter().enumerate() {
        new[c] = i;
    }
    let ids = order.iter().map(|&c| h.ids[c].clone()).collect();
    let parent = order.iter().map(|&c| h.parent[c].map(|p| new[p])).collect();
    let edges = h.edges.iter().map(|&(a, b)| (new[a], new[b])).collect();
    let kept = kept.map(|c| new[c]).collect();
    (build(ids, parent, edges, h.weights, h.real), kept)
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
                if a.distance((b.x, b.y)) >= a.r + b.r - TOLERANCE {
                    continue;
                }
                changed.insert(container);
                for (outer, inner, node) in [(a, b, i), (b, a, j)] {
                    if outer.holds_disc(&inner) && !h.children[node].is_empty() {
                        changed.insert(Container::Node(node));
                    }
                }
            }
        }
    }
}

/// The `Previous` of `h` (as `clusters_by_id` made it, with the clusters it `kept`) from earlier places by node id, and
/// what became of them.
pub fn continue_by_id(
    h: &Hierarchy,
    places: &BTreeMap<String, OverviewPlace>,
    kept: &BTreeSet<usize>,
) -> (Previous, OverviewContinuation) {
    let n = h.ids.len();
    // The disc of every node as it was; a cluster made now has none, whatever its id.
    let frame: Vec<Option<Disc>> = (0..n)
        .map(|i| {
            (i < h.real || kept.contains(&i))
                .then(|| places.get(&h.ids[i]).map(Disc::of))
                .flatten()
        })
        .collect();
    let mut anew = without_place(h, &frame);
    anew.extend((h.real..n).map(|c| frame[c].is_none()));
    // What a cluster made now holds has no place in it.
    for i in 0..h.real {
        if h.parent[i].is_some_and(|p| p >= h.real && frame[p].is_none()) {
            anew[i] = true;
        }
    }

    // A container a node left (removed, or moved elsewhere) has changed, though no radius in it may have: the smallest
    // disc that held the node's place.
    let mut changed: BTreeSet<Container> = BTreeSet::new();
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
        let left = containers.smallest((place.x, place.y), |k, _| !inside(nodes[k]));
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
