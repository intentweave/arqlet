//! Provisional: the cluster overview (ADR 0018). A light layout of a whole graph as points, for graphs far larger than the
//! box pipeline handles: every node is a disc, the containment hierarchy is the cluster hierarchy, and each container's
//! children are arranged by a force simulation inside the container's disc, level by level from the leaves up. No boxes,
//! ports, routes, labels or repair.
//!
//! The work splits into independent jobs, so a host may run them in parallel: [`hierarchy`] reads the graph once, [`levels`]
//! groups the containers by height (every container of one level depends only on lower levels), [`arrange`] lays out one
//! container's children in its own frame, and [`assemble`] places every frame. [`overview`] runs them in order.
//!
//! Deterministic: every pass runs in index order and nothing depends on scheduling, so the result is the same for any
//! order in which the jobs of one level run.
use crate::organic::QuadTree;
use arqlet_model::math::PortableMath;
use arqlet_model::Graph;
use std::collections::BTreeMap;

/// Provisional options of the overview.
#[derive(Clone, Debug, PartialEq)]
pub struct OverviewOptions {
    /// The radius of a leaf's disc.
    pub node_radius: f64,
    /// The free space kept between two discs of one container.
    pub gap: f64,
    /// The free space between a container's border and its children.
    pub padding: f64,
    /// The ticks of each container's force simulation.
    pub ticks: usize,
    /// The many-body repulsion per unit of a disc's area (d3's charge, scaled by area), and the pull towards the
    /// container's centre (d3's `forceX`/`forceY` strength). Weak repulsion and strong gravity pack the discs tightly;
    /// the collision keeps them apart.
    pub charge: f64,
    pub gravity: f64,
    /// Containers with more children than this are split into communities (`with_communities`); 0 never.
    pub communities: usize,
}

impl Default for OverviewOptions {
    fn default() -> Self {
        Self {
            node_radius: 4.,
            gap: 2.,
            padding: 6.,
            ticks: 300,
            charge: 0.05,
            gravity: 0.1,
            communities: 120,
        }
    }
}

/// What a host gives the overview besides the graph (ADR 0018, addendum): radii it sets for some nodes (a structured
/// cluster needs the room of its layered layout), and the positions of a previous overview of the same graph to start
/// from, so that a change moves the drawing as little as the forces allow.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OverviewHints {
    /// Radii by node index, in place of the leaf radius or the container's arrangement.
    pub radius: BTreeMap<usize, f64>,
    /// A previous overview of the same graph to start from: absolute centres and radii by node index.
    pub start: Option<Previous>,
}

/// The places of a previous overview (its `x`, `y` and `radius`), by node index. A container none of whose children
/// changed its radius keeps its arrangement exactly; in any other, the children keep their places and only make room
/// (`make_room`): no forces run and the container is not centred again.
#[derive(Clone, Debug, PartialEq)]
pub struct Previous {
    pub centre: Vec<(f64, f64)>,
    pub radius: Vec<f64>,
    /// In a container whose children changed radius, settle instead of only making room: a short, cool force
    /// simulation from the previous places (`SETTLE_ALPHA`) closes the gaps a shrinking child leaves, and the container
    /// is centred again. Meant for a host that animates the change; without it, moving only what must move is calmer.
    pub settle: bool,
}

/// The starting temperature of a settling simulation: the forces only settle what changed.
const SETTLE_ALPHA: f64 = 0.3;

/// Two radii are the same when they differ by at most this much. A host passes back a previous response, whose numbers are
/// rounded to hundredths, and an unchanged container measures its radius again from rounded places.
const SAME_RADIUS: f64 = 0.02;

/// A container whose children are arranged together: a node with children, or the root, whose children are the nodes
/// without a parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Container {
    Root,
    Node(usize),
}

/// The containment hierarchy of a graph, with its edges aggregated per container.
#[derive(Clone, Debug)]
pub struct Hierarchy {
    /// Node ids in input order; every index below refers to this order.
    pub ids: Vec<String>,
    pub parent: Vec<Option<usize>>,
    /// The children of every node, and of the root, in input order.
    pub children: Vec<Vec<usize>>,
    pub roots: Vec<usize>,
    /// The resolved edges as node index pairs, in input order (self-loops and parallel edges kept).
    pub edges: Vec<(usize, usize)>,
    /// Per container (the root last, at index `ids.len()`): the links between its children that the edges below it make,
    /// as (child, child, number of edges), each pair once with the smaller child index first, in index order. An edge
    /// counts in the container that is the lowest common ancestor of its ends; an edge between a container and its own
    /// descendant, and a self-loop, count nowhere.
    pub links: Vec<Vec<(usize, usize, f64)>>,
    /// The number of nodes from the graph: the nodes after them are clusters the overview added (`with_communities`).
    pub real: usize,
}

impl Hierarchy {
    fn slot(&self, container: Container) -> usize {
        match container {
            Container::Root => self.ids.len(),
            Container::Node(i) => i,
        }
    }

    pub fn members(&self, container: Container) -> &[usize] {
        match container {
            Container::Root => &self.roots,
            Container::Node(i) => &self.children[i],
        }
    }

    pub fn container_links(&self, container: Container) -> &[(usize, usize, f64)] {
        &self.links[self.slot(container)]
    }
}

/// Reads the hierarchy of a graph that `arqlet_model::validate` accepted. Unresolved edges (no target) are left out.
pub fn hierarchy(graph: &Graph) -> Hierarchy {
    let index: BTreeMap<&str, usize> = graph
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.as_str(), i))
        .collect();
    let parent: Vec<Option<usize>> = graph
        .nodes
        .iter()
        .map(|node| {
            node.parent_id
                .as_deref()
                .and_then(|p| index.get(p).copied())
        })
        .collect();
    let edges: Vec<(usize, usize)> = graph
        .edges
        .iter()
        .filter_map(|e| {
            let target = e.target.as_deref()?;
            Some((*index.get(e.source.as_str())?, *index.get(target)?))
        })
        .collect();
    let ids = graph.nodes.iter().map(|node| node.id.clone()).collect();
    build(ids, parent, edges, graph.nodes.len())
}

/// The children, roots and links of a hierarchy given by its parents. `real` is the number of nodes from the graph; the
/// nodes after them are clusters the overview added (communities).
fn build(
    ids: Vec<String>,
    parent: Vec<Option<usize>>,
    edges: Vec<(usize, usize)>,
    real: usize,
) -> Hierarchy {
    let n = ids.len();
    let mut children = vec![Vec::new(); n];
    let mut roots = Vec::new();
    for (i, p) in parent.iter().enumerate() {
        match p {
            Some(p) => children[*p].push(i),
            None => roots.push(i),
        }
    }
    let mut depth = vec![0usize; n];
    for i in 0..n {
        let mut d = 0;
        let mut at = parent[i];
        while let Some(p) = at {
            d += 1;
            at = parent[p];
        }
        depth[i] = d;
    }
    let mut counted: Vec<BTreeMap<(usize, usize), f64>> = vec![BTreeMap::new(); n + 1];
    for &(a, b) in &edges {
        if a == b {
            continue;
        }
        // Climb from the deeper end until both ends are children of one container.
        let (mut u, mut v) = (a, b);
        while depth[u] > depth[v] {
            u = parent[u].unwrap();
        }
        while depth[v] > depth[u] {
            v = parent[v].unwrap();
        }
        if u == v {
            // One end is an ancestor of the other.
            continue;
        }
        while parent[u] != parent[v] {
            u = parent[u].unwrap();
            v = parent[v].unwrap();
        }
        let slot = parent[u].unwrap_or(n);
        *counted[slot].entry((u.min(v), u.max(v))).or_default() += 1.;
    }
    let links = counted
        .into_iter()
        .map(|m| m.into_iter().map(|((a, b), w)| (a, b, w)).collect())
        .collect();
    Hierarchy {
        ids,
        parent,
        children,
        roots,
        edges,
        links,
        real,
    }
}

/// The fewest nodes a community needs to become a cluster of its own: smaller ones would only add rings around a
/// handful of points.
const MIN_COMMUNITY: usize = 8;

/// The prefix of the ids of the clusters `with_communities` adds.
pub const COMMUNITY_PREFIX: &str = "community:";

/// Splits every container with more than `threshold` children (the root included; a flat graph is one such container)
/// into communities of its children, found by Louvain on the links between them (Blondel, Guillaume, Lambiotte and
/// Lefebvre, "Fast unfolding of communities in large networks", 2008; reimplemented, deterministic: nodes in index order,
/// ties to the smaller community). Every level of the Louvain hierarchy becomes a level of clusters, so a flat graph gets
/// the nested clusters the overview and the drill-down need. A community of fewer than `MIN_COMMUNITY` nodes is no
/// cluster; children without links are gathered in one cluster. Added clusters come after the graph's nodes, with ids `community:<container>:<level>.<k>`.
/// `threshold` 0 leaves the hierarchy as it is.
pub fn with_communities(h: Hierarchy, threshold: usize) -> Result<Hierarchy, String> {
    if threshold == 0 {
        return Ok(h);
    }
    let n = h.ids.len();
    let mut ids = h.ids.clone();
    let mut parent = h.parent.clone();
    let taken: std::collections::BTreeSet<String> = h.ids.iter().cloned().collect();
    let containers: Vec<Container> = (0..n)
        .filter(|&i| h.children[i].len() > threshold)
        .map(Container::Node)
        .chain((h.roots.len() > threshold).then_some(Container::Root))
        .collect();
    for container in containers {
        let members = h.members(container).to_vec();
        let owner = match container {
            Container::Root => None,
            Container::Node(i) => Some(i),
        };
        let name = owner.map_or("#".to_string(), |i| h.ids[i].clone());
        let local: BTreeMap<usize, usize> =
            members.iter().enumerate().map(|(k, &m)| (m, k)).collect();
        let links: Vec<(usize, usize, f64)> = h
            .container_links(container)
            .iter()
            .map(|&(a, b, w)| (local[&a], local[&b], w))
            .collect();
        let mut linked = vec![false; members.len()];
        for &(a, b, _) in &links {
            linked[a] = true;
            linked[b] = true;
        }
        // A container whose children are mostly without links (a file of type declarations, say) has no communities
        // to find: splitting it would only add a level that gathers the unlinked ones.
        if linked.iter().filter(|l| **l).count() * 2 < members.len() {
            continue;
        }
        // The levels of Louvain, from the members up: `levels[l][c]` is the community at level l + 1 of community c at
        // level l (level 0: the members).
        let levels = louvain(members.len(), &links);
        // Clusters per level, top down, so that every cluster's parent exists when it is made.
        let mut cluster_of: Vec<Vec<Option<usize>>> = Vec::new();
        let mut sizes: Vec<Vec<usize>> = vec![vec![1; members.len()]];
        for level in &levels {
            let count = level.iter().copied().max().map_or(0, |m| m + 1);
            let mut size = vec![0usize; count];
            for (c, &up) in level.iter().enumerate() {
                size[up] += sizes.last().unwrap()[c];
            }
            sizes.push(size);
        }
        // How many communities of the level below each community holds: a community holding one is no cluster.
        let mut parts: Vec<Vec<usize>> = Vec::new();
        for level in &levels {
            let count = level.iter().copied().max().map_or(0, |m| m + 1);
            let mut part = vec![0usize; count];
            for &up in level {
                part[up] += 1;
            }
            parts.push(part);
        }
        for (l, part) in parts.iter().enumerate().rev() {
            let mut made = vec![None; part.len()];
            for c in 0..part.len() {
                // A community smaller than `MIN_COMMUNITY` is no cluster (its members join the cluster above); nor one
                // that only repeats its child.
                if sizes[l + 1][c] < MIN_COMMUNITY || part[c] < 2 {
                    continue;
                }
                let under = match cluster_of.last() {
                    // The cluster of the level above that holds it, else the container.
                    Some(above) => {
                        let up = levels[l + 1][c];
                        above[up].or(owner)
                    }
                    None => owner,
                };
                // The top level holding everything is the container itself.
                if l + 1 == levels.len() && part.len() == 1 {
                    continue;
                }
                made[c] = Some(add_cluster(
                    &mut ids,
                    &mut parent,
                    &taken,
                    &name,
                    &format!("{}.{c}", l + 1),
                    under,
                )?);
            }
            // A community without a cluster passes its parent down.
            if let Some(above) = cluster_of.last() {
                for c in 0..part.len() {
                    if made[c].is_none() {
                        made[c] = above[levels[l + 1][c]];
                    }
                }
            }
            cluster_of.push(made);
        }
        // Members: under their community's cluster at the lowest level, if any; members without links together.
        let lowest = cluster_of.last();
        let mut unlinked = None;
        let alone = linked.iter().filter(|l| !**l).count();
        for (k, &m) in members.iter().enumerate() {
            if !linked[k] {
                if alone >= 2 {
                    if unlinked.is_none() {
                        unlinked = Some(add_cluster(
                            &mut ids,
                            &mut parent,
                            &taken,
                            &name,
                            "unlinked",
                            owner,
                        )?);
                    }
                    parent[m] = unlinked;
                }
                continue;
            }
            if let (Some(lowest), Some(first)) = (lowest, levels.first()) {
                if let Some(cluster) = lowest[first[k]] {
                    parent[m] = Some(cluster);
                }
            }
        }
    }
    Ok(build(ids, parent, h.edges, h.real))
}

/// Adds a cluster `community:<container>:<label>` under `under`; refused when the graph has a node of that id.
fn add_cluster(
    ids: &mut Vec<String>,
    parent: &mut Vec<Option<usize>>,
    taken: &std::collections::BTreeSet<String>,
    container: &str,
    label: &str,
    under: Option<usize>,
) -> Result<usize, String> {
    let id = format!("{COMMUNITY_PREFIX}{container}:{label}");
    if taken.contains(&id) {
        return Err(format!(
            "A node of the graph has the id of a community: {id}"
        ));
    }
    ids.push(id);
    parent.push(under);
    Ok(ids.len() - 1)
}

/// Louvain on `k` nodes with weighted links: the community of every node, level by level (`result[l][c]`: the community
/// at level l + 1 of community c at level l), until a level merges nothing. Communities are numbered in order of their
/// first node, so the result depends only on the input. Nodes without links stay alone.
fn louvain(k: usize, links: &[(usize, usize, f64)]) -> Vec<Vec<usize>> {
    let mut result = Vec::new();
    let mut nodes = k;
    let mut edges: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    for &(a, b, w) in links {
        *edges.entry((a.min(b), a.max(b))).or_default() += w;
    }
    for _ in 0..32 {
        // Adjacency with self-loops kept (the inside of an aggregated community), and degrees.
        let mut adjacent: Vec<Vec<(usize, f64)>> = vec![Vec::new(); nodes];
        let mut degree = vec![0f64; nodes];
        let mut total = 0.;
        for (&(a, b), &w) in &edges {
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
        if total == 0. {
            break;
        }
        let mut community: Vec<usize> = (0..nodes).collect();
        let mut tot = degree.clone();
        let mut weight_to = vec![0f64; nodes];
        let mut touched: Vec<usize> = Vec::new();
        for _ in 0..64 {
            let mut moved = false;
            for i in 0..nodes {
                let own = community[i];
                tot[own] -= degree[i];
                for &(j, w) in &adjacent[i] {
                    if j == i {
                        continue;
                    }
                    let c = community[j];
                    if weight_to[c] == 0. {
                        touched.push(c);
                    }
                    weight_to[c] += w;
                }
                // The gain of joining c, less what is common to all choices: k_i,in(c) - tot(c) k_i / 2m.
                let gain = |c: usize, w: f64| w - tot[c] * degree[i] / total;
                let mut best = own;
                let mut best_gain = gain(own, weight_to[own]);
                touched.sort_unstable();
                for &c in &touched {
                    let g = gain(c, weight_to[c]);
                    // Only a real gain moves a node; candidates come in ascending order, so a tie keeps the smaller.
                    if g > best_gain + 1e-12 {
                        best = c;
                        best_gain = g;
                    }
                }
                for &c in &touched {
                    weight_to[c] = 0.;
                }
                weight_to[own] = 0.;
                touched.clear();
                tot[best] += degree[i];
                if best != own {
                    community[i] = best;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        // Number the communities in order of their first node.
        let mut number: BTreeMap<usize, usize> = BTreeMap::new();
        let mut renamed = vec![0usize; nodes];
        for i in 0..nodes {
            let next = number.len();
            renamed[i] = *number.entry(community[i]).or_insert(next);
        }
        let count = number.len();
        if count == nodes {
            break;
        }
        let mut aggregated: BTreeMap<(usize, usize), f64> = BTreeMap::new();
        for (&(a, b), &w) in &edges {
            let (ca, cb) = (renamed[a], renamed[b]);
            *aggregated.entry((ca.min(cb), ca.max(cb))).or_default() += w;
        }
        result.push(renamed);
        edges = aggregated;
        nodes = count;
    }
    result
}

/// The containers grouped by height: level 0 holds the containers whose children are all leaves, and every container
/// comes after all containers below it. The root is alone on the last level. Within a level, containers are in index order.
pub fn levels(h: &Hierarchy) -> Vec<Vec<Container>> {
    let n = h.ids.len();
    let mut height = vec![0usize; n];
    // Children have larger depth; visiting nodes from the deepest makes every child's height known before its parent's.
    let mut order: Vec<usize> = (0..n).collect();
    let depth: Vec<usize> = (0..n)
        .map(|i| {
            let (mut d, mut at) = (0, h.parent[i]);
            while let Some(p) = at {
                d += 1;
                at = h.parent[p];
            }
            d
        })
        .collect();
    order.sort_by(|a, b| depth[*b].cmp(&depth[*a]).then(a.cmp(b)));
    for &i in &order {
        if !h.children[i].is_empty() {
            height[i] = 1 + h.children[i].iter().map(|&c| height[c]).max().unwrap_or(0);
        }
    }
    let top = height.iter().copied().max().unwrap_or(0);
    let mut result: Vec<Vec<Container>> = vec![Vec::new(); top];
    for i in 0..n {
        if height[i] > 0 {
            result[height[i] - 1].push(Container::Node(i));
        }
    }
    result.retain(|level| !level.is_empty());
    result.push(vec![Container::Root]);
    result
}

/// One container laid out in its own frame: its children's centres (in the order of `Hierarchy::members`) relative to the
/// centre of its disc, and the disc's radius.
#[derive(Clone, Debug, PartialEq)]
pub struct Arrangement {
    pub positions: Vec<(f64, f64)>,
    pub radius: f64,
    /// Pairs of children whose discs (with the gap) still overlap after the final separation. Normally 0.
    pub overlaps: usize,
}

/// The share of the velocity lost per tick (d3-force: 0.4). The model follows d3-force; no code is copied.
const VELOCITY_DECAY: f64 = 0.4;
/// The share of an overlap that one tick of the collision removes, and the rounds of the final separation.
const COLLIDE_STRENGTH: f64 = 0.7;
const SEPARATION_ROUNDS: usize = 600;
const STRETCH_EVERY: usize = 12;
const STRETCH: f64 = 1.02;

/// Lays out the children of `container`, whose discs have the radii `radius` (indexed by node; leaves carry the leaf
/// radius, containers the radius their own arrangement gave). Starts from a phyllotaxis spiral in the order of a
/// breadth-first walk over the links (so linked children start near each other), then runs a force simulation: springs on
/// the links, a Barnes–Hut many-body repulsion weighted by each disc's area, a gravity towards the centre and a collision
/// of the discs. A final separation removes what overlap is left. With `start` (absolute centres by node, see
/// `OverviewHints::start`) the children start from their places relative to the container's previous centre, cooler; and
/// when no child's radius changed, the previous arrangement is kept as it was.
pub fn arrange(
    h: &Hierarchy,
    container: Container,
    radius: &[f64],
    options: &OverviewOptions,
    start: Option<&Previous>,
) -> Arrangement {
    let members = h.members(container);
    let k = members.len();
    let local: BTreeMap<usize, usize> = members.iter().enumerate().map(|(i, &m)| (m, i)).collect();
    let r: Vec<f64> = members.iter().map(|&m| radius[m]).collect();
    if k == 0 {
        return Arrangement {
            positions: Vec::new(),
            radius: options.node_radius,
            overlaps: 0,
        };
    }
    if k == 1 {
        return Arrangement {
            positions: vec![(0., 0.)],
            radius: r[0] + options.padding,
            overlaps: 0,
        };
    }
    let links: Vec<(usize, usize, f64)> = h
        .container_links(container)
        .iter()
        .map(|&(a, b, w)| (local[&a], local[&b], w))
        .collect();
    let mut x = match start {
        Some(previous) => {
            let origin = match container {
                Container::Root => (0., 0.),
                Container::Node(c) => previous.centre[c],
            };
            let x: Vec<(f64, f64)> = members
                .iter()
                .map(|&m| {
                    (
                        previous.centre[m].0 - origin.0,
                        previous.centre[m].1 - origin.1,
                    )
                })
                .collect();
            let changed = members
                .iter()
                .any(|&m| (radius[m] - previous.radius[m]).abs() > SAME_RADIUS);
            if !changed {
                // Kept as it was: only the radius is measured again (it may have been set by a host before).
                let reach = x
                    .iter()
                    .zip(&r)
                    .map(|(p, ri)| (p.0 * p.0 + p.1 * p.1).sqrt() + ri)
                    .fold(0., f64::max);
                return Arrangement {
                    positions: x,
                    radius: reach + options.padding,
                    overlaps: 0,
                };
            }
            if previous.settle {
                // The forces settle the container again, from where its children are.
                x
            } else {
                // A child changed its radius: the others make room, and nothing else moves (no forces, no new centre), so
                // that a change the user made does not shift what is around it more than it must.
                let mut x = x;
                let overlaps = make_room(&mut x, &r, options.gap);
                let reach = x
                    .iter()
                    .zip(&r)
                    .map(|(p, ri)| (p.0 * p.0 + p.1 * p.1).sqrt() + ri)
                    .fold(0., f64::max);
                return Arrangement {
                    positions: x,
                    radius: reach + options.padding,
                    overlaps,
                };
            }
        }
        None => spiral(&r, &links, options.gap),
    };
    let alpha = if start.is_some() { SETTLE_ALPHA } else { 1. };
    simulate(&mut x, &r, &links, options, alpha);
    let overlaps = separate(&mut x, &r, options.gap);
    // Centre the disc on (nearly) the smallest circle around the children, and take the radius that holds them.
    let (cx, cy) = enclosing_centre(&x, &r);
    for p in &mut x {
        p.0 -= cx;
        p.1 -= cy;
    }
    let reach = x
        .iter()
        .zip(&r)
        .map(|(p, ri)| (p.0 * p.0 + p.1 * p.1).sqrt() + ri)
        .fold(0., f64::max);
    Arrangement {
        positions: x,
        radius: reach + options.padding,
        overlaps,
    }
}

/// Start positions on a phyllotaxis spiral (as d3-force places nodes without a position), each child at the distance that
/// leaves the area of the discs before it free, in the order of a breadth-first walk over the links from the most linked
/// child (ties: the smaller index); unlinked children follow in index order.
fn spiral(r: &[f64], links: &[(usize, usize, f64)], gap: f64) -> Vec<(f64, f64)> {
    let k = r.len();
    let mut adjacent: Vec<Vec<usize>> = vec![Vec::new(); k];
    let mut weight = vec![0f64; k];
    for &(a, b, w) in links {
        adjacent[a].push(b);
        adjacent[b].push(a);
        weight[a] += w;
        weight[b] += w;
    }
    let mut seeds: Vec<usize> = (0..k).collect();
    seeds.sort_by(|a, b| weight[*b].total_cmp(&weight[*a]).then(a.cmp(b)));
    let mut order = Vec::with_capacity(k);
    let mut seen = vec![false; k];
    for seed in seeds {
        if seen[seed] {
            continue;
        }
        seen[seed] = true;
        let mut queue = std::collections::VecDeque::from([seed]);
        while let Some(i) = queue.pop_front() {
            order.push(i);
            for &j in &adjacent[i] {
                if !seen[j] {
                    seen[j] = true;
                    queue.push_back(j);
                }
            }
        }
    }
    let golden = std::f64::consts::PI * (3. - 5f64.sqrt());
    let mut x = vec![(0., 0.); k];
    let mut area = 0.;
    for (slot, &i) in order.iter().enumerate() {
        let size = r[i] + gap;
        area += size * size;
        let distance = area.sqrt() * 1.2;
        let angle = slot as f64 * golden;
        x[i] = (distance * angle.p_cos(), distance * angle.p_sin());
    }
    x
}

/// The force simulation of `arrange`.
fn simulate(
    x: &mut [(f64, f64)],
    r: &[f64],
    links: &[(usize, usize, f64)],
    options: &OverviewOptions,
    start_alpha: f64,
) {
    let k = x.len();
    let ticks = options.ticks.max(1);
    let mut degree = vec![0f64; k];
    for &(a, b, w) in links {
        degree[a] += w;
        degree[b] += w;
    }
    // A disc repels by its area, so a large cluster keeps more room around it.
    let mass: Vec<f64> = r.iter().map(|ri| (ri + options.gap).powi(2)).collect();
    let mut v = vec![(0f64, 0f64); k];
    let decay = 1. - 0.001f64.p_powf(1. / ticks as f64);
    let mut alpha = start_alpha;
    for _ in 0..ticks {
        alpha -= alpha * decay;
        // Springs, d3's link force: the rest length leaves the gap between the discs, the strength is the share of the
        // link in the smaller end's links, and the move is biased towards the end with fewer links.
        for &(a, b, w) in links {
            let dx = x[b].0 + v[b].0 - x[a].0 - v[a].0;
            let dy = x[b].1 + v[b].1 - x[a].1 - v[a].1;
            let l = (dx * dx + dy * dy).sqrt();
            if l == 0. {
                continue;
            }
            let rest = r[a] + r[b] + 2. * options.gap;
            let strength = (w / degree[a].min(degree[b])).min(1.);
            let f = (l - rest) / l * alpha * strength;
            let (fx, fy) = (dx * f, dy * f);
            let bias = degree[a] / (degree[a] + degree[b]);
            v[b].0 -= fx * bias;
            v[b].1 -= fy * bias;
            v[a].0 += fx * (1. - bias);
            v[a].1 += fy * (1. - bias);
        }
        // Many-body repulsion, and gravity towards the centre.
        let tree = QuadTree::new_weighted(x, &mass);
        for i in 0..k {
            let (fx, fy) = tree.force(x, i, -options.charge * alpha);
            v[i].0 += fx - x[i].0 * options.gravity * alpha;
            v[i].1 += fy - x[i].1 * options.gravity * alpha;
        }
        collide(x, &mut v, r, options.gap, COLLIDE_STRENGTH);
        for i in 0..k {
            v[i].0 *= 1. - VELOCITY_DECAY;
            v[i].1 *= 1. - VELOCITY_DECAY;
            x[i].0 += v[i].0;
            x[i].1 += v[i].1;
        }
    }
}

/// Pushes overlapping discs (with the gap) apart on the predicted positions `x + v`: each pair loses `strength` of its
/// overlap along the line between the centres, the smaller disc moving more. Candidates come from a sweep over the discs
/// sorted by their left edge. Returns the number of overlapping pairs found.
fn collide(x: &[(f64, f64)], v: &mut [(f64, f64)], r: &[f64], gap: f64, strength: f64) -> usize {
    let k = x.len();
    let p: Vec<(f64, f64)> = (0..k).map(|i| (x[i].0 + v[i].0, x[i].1 + v[i].1)).collect();
    let mut order: Vec<usize> = (0..k).collect();
    order.sort_by(|a, b| {
        (p[*a].0 - r[*a])
            .total_cmp(&(p[*b].0 - r[*b]))
            .then(a.cmp(b))
    });
    let mut found = 0;
    for (s, &i) in order.iter().enumerate() {
        let right = p[i].0 + r[i] + gap;
        for &j in &order[s + 1..] {
            if p[j].0 - r[j] >= right {
                break;
            }
            let (dx, dy) = (p[j].0 - p[i].0, p[j].1 - p[i].1);
            let reach = r[i] + r[j] + gap;
            let l2 = dx * dx + dy * dy;
            if l2 >= reach * reach {
                continue;
            }
            found += 1;
            let l = l2.sqrt();
            // Coincident centres part along a fixed direction that depends only on the pair.
            let (ux, uy) = if l > 1e-9 {
                (dx / l, dy / l)
            } else {
                let angle = (i.min(j) * 31 + i.max(j) * 17) as f64;
                (angle.p_cos(), angle.p_sin())
            };
            let overlap = (reach - l) * strength;
            let (mi, mj) = (r[i] * r[i], r[j] * r[j]);
            let (si, sj) = (mj / (mi + mj), mi / (mi + mj));
            v[i].0 -= ux * overlap * si;
            v[i].1 -= uy * overlap * si;
            v[j].0 += ux * overlap * sj;
            v[j].1 += uy * overlap * sj;
        }
    }
    found
}

/// Rounds of full collision steps until no discs overlap. When a stretch of `STRETCH_EVERY` rounds leaves overlaps (a dense
/// pack relaxes slowly pair by pair), the positions are stretched by `STRETCH` about their centre, which lengthens every
/// distance; so the separation ends, after at most `SEPARATION_ROUNDS` rounds. Returns the overlapping pairs left.
fn separate(x: &mut [(f64, f64)], r: &[f64], gap: f64) -> usize {
    let k = x.len();
    for round in 0..SEPARATION_ROUNDS {
        let mut v = vec![(0f64, 0f64); k];
        // A little more than the gap, so that rounding does not leave touching discs counted as overlaps.
        if collide(x, &mut v, r, gap * 1.01, 1.) == 0 {
            return 0;
        }
        for i in 0..k {
            x[i].0 += v[i].0;
            x[i].1 += v[i].1;
        }
        if round % STRETCH_EVERY == STRETCH_EVERY - 1 {
            let (cx, cy) = centre(x, r);
            for p in x.iter_mut() {
                p.0 = cx + (p.0 - cx) * STRETCH;
                p.1 = cy + (p.1 - cy) * STRETCH;
            }
        }
    }
    let mut v = vec![(0f64, 0f64); k];
    collide(x, &mut v, r, gap, 0.)
}

/// The rounds of the approximation of the smallest enclosing circle.
const ENCLOSING_ROUNDS: usize = 256;

/// Nearly the centre of the smallest circle around the discs: from the weighted centre, each round moves the centre a
/// shrinking share (1 / (round + 2)) of the way to the farthest point of the disc that reaches farthest (Bădoiu and
/// Clarkson, "Smaller core-sets for balls", SODA 2003, for points; the farthest point of a disc stands for the disc).
/// Only the centre is approximate: the radius is always measured from it, so every disc is held.
fn enclosing_centre(x: &[(f64, f64)], r: &[f64]) -> (f64, f64) {
    let mut c = centre(x, r);
    let reach =
        |c: (f64, f64), i: usize| ((x[i].0 - c.0).powi(2) + (x[i].1 - c.1).powi(2)).sqrt() + r[i];
    let worst = |c: (f64, f64)| (0..x.len()).map(|i| reach(c, i)).fold(0., f64::max);
    let (mut best, mut best_reach) = (c, worst(c));
    for round in 0..ENCLOSING_ROUNDS {
        let far = (0..x.len())
            .max_by(|&a, &b| reach(c, a).total_cmp(&reach(c, b)).then(b.cmp(&a)))
            .unwrap();
        let (dx, dy) = (x[far].0 - c.0, x[far].1 - c.1);
        let l = (dx * dx + dy * dy).sqrt();
        if l < 1e-12 {
            break;
        }
        // The point of the farthest disc farthest from the centre.
        let point = (x[far].0 + dx / l * r[far], x[far].1 + dy / l * r[far]);
        let step = 1. / (round as f64 + 2.);
        c = (c.0 + (point.0 - c.0) * step, c.1 + (point.1 - c.1) * step);
        let w = worst(c);
        if w < best_reach {
            (best, best_reach) = (c, w);
        }
    }
    best
}

/// Removes the overlaps of discs that start from their previous places, moving them as little as it can: rounds of
/// full collision steps without any other force (the larger disc of a pair moves less), up to `SEPARATION_ROUNDS`; only
/// if that does not end it, `separate` (which may stretch) finishes. Returns the overlapping pairs left.
fn make_room(x: &mut [(f64, f64)], r: &[f64], gap: f64) -> usize {
    let k = x.len();
    for _ in 0..SEPARATION_ROUNDS {
        let mut v = vec![(0f64, 0f64); k];
        if collide(x, &mut v, r, gap * 1.01, 1.) == 0 {
            return 0;
        }
        for i in 0..k {
            x[i].0 += v[i].0;
            x[i].1 += v[i].1;
        }
    }
    separate(x, r, gap)
}

/// The centre of discs weighted by their area.
fn centre(x: &[(f64, f64)], r: &[f64]) -> (f64, f64) {
    let mass: f64 = r.iter().map(|ri| ri * ri).sum();
    (
        x.iter().zip(r).map(|(p, ri)| p.0 * ri * ri).sum::<f64>() / mass,
        x.iter().zip(r).map(|(p, ri)| p.1 * ri * ri).sum::<f64>() / mass,
    )
}

/// The overview of a whole graph: every node's centre and radius in one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Overview {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    pub radius: Vec<f64>,
    /// The radius of the root's disc, which is centred on the origin.
    pub root_radius: f64,
    /// Overlapping pairs left over all containers (normally 0).
    pub overlaps: usize,
}

/// Places every container's frame: the root's children at their arranged positions, every other node at its container's
/// centre plus its arranged position. `arrangements` holds one entry per container of `levels` (in any order).
pub fn assemble(
    h: &Hierarchy,
    arrangements: &BTreeMap<Container, Arrangement>,
    options: &OverviewOptions,
    hints: &OverviewHints,
) -> Overview {
    let n = h.ids.len();
    let mut x = vec![0f64; n];
    let mut y = vec![0f64; n];
    let radius = radii(h, arrangements, options, hints);
    let mut stack: Vec<(Container, (f64, f64))> = vec![(Container::Root, (0., 0.))];
    while let Some((container, origin)) = stack.pop() {
        let Some(arrangement) = arrangements.get(&container) else {
            continue;
        };
        for (&m, p) in h.members(container).iter().zip(&arrangement.positions) {
            x[m] = origin.0 + p.0;
            y[m] = origin.1 + p.1;
            if !h.children[m].is_empty() {
                stack.push((Container::Node(m), (x[m], y[m])));
            }
        }
    }
    Overview {
        x,
        y,
        radius,
        root_radius: arrangements.get(&Container::Root).map_or(0., |a| a.radius),
        overlaps: arrangements.values().map(|a| a.overlaps).sum(),
    }
}

/// The radii the containers of one level need from the levels below: leaves have the leaf radius, containers the radius
/// of their arrangement, and a node with a radius in `hints` that radius.
pub fn radii(
    h: &Hierarchy,
    arrangements: &BTreeMap<Container, Arrangement>,
    options: &OverviewOptions,
    hints: &OverviewHints,
) -> Vec<f64> {
    let mut radius = vec![options.node_radius; h.ids.len()];
    for (container, arrangement) in arrangements {
        if let Container::Node(i) = container {
            radius[*i] = arrangement.radius;
        }
    }
    for (&i, &r) in &hints.radius {
        radius[i] = r;
    }
    radius
}

/// The whole overview, serially. A host that runs the jobs of one level in parallel gets the same result.
pub fn overview(
    graph: &Graph,
    options: &OverviewOptions,
    hints: &OverviewHints,
) -> (Hierarchy, Overview) {
    let h = with_communities(hierarchy(graph), options.communities)
        .unwrap_or_else(|_| hierarchy(graph));
    let mut arrangements = BTreeMap::new();
    for level in levels(&h) {
        let radius = radii(&h, &arrangements, options, hints);
        for container in level {
            let start = hints.start.as_ref();
            arrangements.insert(container, arrange(&h, container, &radius, options, start));
        }
    }
    let result = assemble(&h, &arrangements, options, hints);
    (h, result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use arqlet_model::{Edge, Node};

    fn node(id: &str, parent: Option<&str>) -> Node {
        Node {
            id: id.into(),
            kind: "n".into(),
            label: id.into(),
            parent_id: parent.map(Into::into),
            shape: Default::default(),
        }
    }

    fn edge(id: &str, s: &str, t: Option<&str>) -> Edge {
        Edge {
            id: id.into(),
            kind: "e".into(),
            source: s.into(),
            target: t.map(Into::into),
        }
    }

    fn graph(nodes: Vec<Node>, edges: Vec<Edge>) -> Graph {
        let mut g: Graph = serde_json::from_str(r#"{"nodes":[],"edges":[]}"#).unwrap();
        g.nodes = nodes;
        g.edges = edges;
        g
    }

    /// Two clusters of three leaves, linked inside, one edge between them, a cycle, a self-loop, a parallel edge, an
    /// unresolved edge and an edge from a container to its own child.
    fn sample() -> Graph {
        graph(
            vec![
                node("a", None),
                node("a1", Some("a")),
                node("a2", Some("a")),
                node("a3", Some("a")),
                node("b", None),
                node("b1", Some("b")),
                node("b2", Some("b")),
                node("b3", Some("b")),
                node("solo", None),
            ],
            vec![
                edge("e1", "a1", Some("a2")),
                edge("e2", "a2", Some("a3")),
                edge("e3", "a3", Some("a1")),
                edge("e4", "b1", Some("b2")),
                edge("e5", "b1", Some("b2")),
                edge("e6", "a1", Some("b3")),
                edge("self", "b3", Some("b3")),
                edge("open", "b2", None),
                edge("down", "a", Some("a2")),
            ],
        )
    }

    #[test]
    fn edges_are_aggregated_at_their_lowest_common_container() {
        let h = hierarchy(&sample());
        // Unresolved edges are left out; the rest keep input order, self-loop and parallel edge included.
        assert_eq!(h.edges.len(), 8);
        let a = &h.links[0];
        assert_eq!(a, &vec![(1, 2, 1.), (1, 3, 1.), (2, 3, 1.)]);
        assert_eq!(h.links[4], vec![(5, 6, 2.)]);
        // The root sees one link between the clusters; the self-loop and the edge into its own container count nowhere.
        assert_eq!(h.container_links(Container::Root), &[(0, 4, 1.)]);
        assert_eq!(
            levels(&h),
            vec![
                vec![Container::Node(0), Container::Node(4)],
                vec![Container::Root]
            ]
        );
    }

    #[test]
    fn discs_do_not_overlap_and_children_stay_inside_their_container() {
        let options = OverviewOptions::default();
        let (h, o) = overview(&sample(), &options, &OverviewHints::default());
        assert_eq!(o.overlaps, 0);
        for i in 0..h.ids.len() {
            assert!(o.x[i].is_finite() && o.y[i].is_finite());
            if let Some(p) = h.parent[i] {
                let d = ((o.x[i] - o.x[p]).powi(2) + (o.y[i] - o.y[p]).powi(2)).sqrt();
                assert!(
                    d + o.radius[i] <= o.radius[p] + 1e-9,
                    "{} leaves {}",
                    h.ids[i],
                    h.ids[p]
                );
            }
            for j in i + 1..h.ids.len() {
                if h.parent[i] == h.parent[j] {
                    let d = ((o.x[i] - o.x[j]).powi(2) + (o.y[i] - o.y[j]).powi(2)).sqrt();
                    assert!(
                        d >= o.radius[i] + o.radius[j] + options.gap,
                        "{} meets {}",
                        h.ids[i],
                        h.ids[j]
                    );
                }
            }
        }
        assert!(o.root_radius > o.radius[0]);
    }

    #[test]
    fn linked_children_end_closer_than_unlinked_ones() {
        // One container of 30 leaves: a ring of 10 linked leaves and 20 isolated ones.
        let mut nodes = vec![node("c", None)];
        let mut edges = Vec::new();
        for i in 0..30 {
            nodes.push(node(&format!("n{i}"), Some("c")));
        }
        for i in 0..10 {
            edges.push(edge(
                &format!("e{i}"),
                &format!("n{i}"),
                Some(&format!("n{}", (i + 1) % 10)),
            ));
        }
        let (_, o) = overview(
            &graph(nodes, edges),
            &OverviewOptions::default(),
            &OverviewHints::default(),
        );
        let distance =
            |a: usize, b: usize| ((o.x[a] - o.x[b]).powi(2) + (o.y[a] - o.y[b]).powi(2)).sqrt();
        let linked: f64 = (0..10)
            .map(|i| distance(1 + i, 1 + (i + 1) % 10))
            .sum::<f64>()
            / 10.;
        let unlinked: f64 = (10..29).map(|i| distance(1 + i, 2 + i)).sum::<f64>() / 19.;
        assert!(linked < unlinked, "linked {linked} unlinked {unlinked}");
    }

    #[test]
    fn the_result_does_not_depend_on_the_order_of_the_jobs_of_a_level() {
        let g = sample();
        let options = OverviewOptions::default();
        let (h, serial) = overview(&g, &options, &OverviewHints::default());
        let mut arrangements = BTreeMap::new();
        for level in levels(&h) {
            let radius = radii(&h, &arrangements, &options, &OverviewHints::default());
            for container in level.into_iter().rev() {
                arrangements.insert(container, arrange(&h, container, &radius, &options, None));
            }
        }
        assert_eq!(
            assemble(&h, &arrangements, &options, &OverviewHints::default()),
            serial
        );
    }

    #[test]
    fn a_flat_graph_is_one_container() {
        let nodes = (0..50).map(|i| node(&format!("n{i}"), None)).collect();
        let edges = (0..49)
            .map(|i| {
                edge(
                    &format!("e{i}"),
                    &format!("n{i}"),
                    Some(&format!("n{}", i + 1)),
                )
            })
            .collect();
        let (h, o) = overview(
            &graph(nodes, edges),
            &OverviewOptions::default(),
            &OverviewHints::default(),
        );
        assert_eq!(levels(&h), vec![vec![Container::Root]]);
        assert_eq!(o.overlaps, 0);
    }

    fn previous(o: &Overview) -> Previous {
        Previous {
            centre: o.x.iter().copied().zip(o.y.iter().copied()).collect(),
            radius: o.radius.clone(),
            settle: false,
        }
    }

    #[test]
    fn a_larger_radius_makes_room_and_a_warm_start_keeps_the_rest_in_place() {
        let options = OverviewOptions::default();
        let g = sample();
        let (h, before) = overview(&g, &options, &OverviewHints::default());
        // Cluster a (index 0) is shown structured and needs three times its room.
        let hints = OverviewHints {
            radius: BTreeMap::from([(0, before.radius[0] * 3.)]),
            start: Some(previous(&before)),
        };
        let (_, after) = overview(&g, &options, &hints);
        assert_eq!(after.radius[0], before.radius[0] * 3.);
        assert_eq!(after.overlaps, 0);
        // The top-level discs still keep their gap.
        let d = ((after.x[0] - after.x[4]).powi(2) + (after.y[0] - after.y[4]).powi(2)).sqrt();
        assert!(d >= after.radius[0] + after.radius[4] + options.gap);
        // Inside cluster b nothing changed: its children keep their places relative to its centre.
        for c in [5, 6, 7] {
            let rel = |o: &Overview| (o.x[c] - o.x[4], o.y[c] - o.y[4]);
            let (p, q) = (rel(&before), rel(&after));
            assert!(
                (p.0 - q.0).abs() < 1e-6 && (p.1 - q.1).abs() < 1e-6,
                "{} moved inside b",
                h.ids[c]
            );
        }
        // Switching back: without the radius, the cluster shrinks to its own arrangement again.
        let back = OverviewHints {
            radius: BTreeMap::new(),
            start: Some(previous(&after)),
        };
        let (_, restored) = overview(&g, &options, &back);
        assert!((restored.radius[0] - before.radius[0]).abs() < 1e-6);
        assert_eq!(restored.overlaps, 0);
        // A warm start from its own result keeps everything.
        let again = OverviewHints {
            radius: BTreeMap::new(),
            start: Some(previous(&before)),
        };
        let (_, settled) = overview(&g, &options, &again);
        let moved = (0..h.ids.len())
            .map(|i| {
                ((settled.x[i] - before.x[i]).powi(2) + (settled.y[i] - before.y[i]).powi(2)).sqrt()
            })
            .fold(0., f64::max);
        assert!(moved < 1e-6, "moved {moved}");
    }

    #[test]
    fn a_flat_graph_gets_its_communities_as_clusters() {
        // Three dense groups of 50 nodes (each connected), a bridge between each two, and 10 nodes without edges.
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for g in 0..3 {
            for i in 0..50 {
                nodes.push(node(&format!("g{g}n{i}"), None));
                for j in 0..i {
                    if (i + j) % 3 != 0 {
                        edges.push(edge(
                            &format!("g{g}e{i}-{j}"),
                            &format!("g{g}n{i}"),
                            Some(&format!("g{g}n{j}")),
                        ));
                    }
                }
            }
            edges.push(edge(
                &format!("bridge{g}"),
                &format!("g{g}n0"),
                Some(&format!("g{}n1", (g + 1) % 3)),
            ));
        }
        for i in 0..10 {
            nodes.push(node(&format!("alone{i}"), None));
        }
        let g = graph(nodes, edges);
        let h = with_communities(hierarchy(&g), 120).unwrap();
        assert_eq!(h.real, 160);
        assert!(h.ids.len() > 160, "no communities added");
        // Every clique ends in one cluster that holds nothing of the other cliques.
        let top = |mut i: usize| {
            while let Some(p) = h.parent[i] {
                if h.parent[p].is_none() {
                    return p;
                }
                i = p;
            }
            i
        };
        for c in 0..3 {
            let cluster = top(c * 50);
            assert!(
                h.ids[cluster].starts_with(COMMUNITY_PREFIX),
                "{}",
                h.ids[cluster]
            );
            for i in 0..50 {
                assert_eq!(top(c * 50 + i), cluster, "clique {c} split");
            }
            for other in (0..3).filter(|o| *o != c) {
                assert_ne!(top(other * 50), cluster);
            }
        }
        // The ten nodes without edges are gathered in one cluster.
        let unlinked = h.parent[150].unwrap();
        assert!(h.ids[unlinked].ends_with(":unlinked"));
        assert!((150..160).all(|i| h.parent[i] == Some(unlinked)));
        // Deterministic, and laid out without overlaps; a threshold of 0 leaves the hierarchy alone.
        assert_eq!(with_communities(hierarchy(&g), 120).unwrap().ids, h.ids);
        let (_, o) = overview(&g, &OverviewOptions::default(), &OverviewHints::default());
        assert_eq!(o.overlaps, 0);
        assert_eq!(with_communities(hierarchy(&g), 0).unwrap().ids.len(), 160);
    }

    #[test]
    fn growing_without_touching_anyone_moves_nothing_else() {
        let options = OverviewOptions::default();
        let g = sample();
        let (h, before) = overview(&g, &options, &OverviewHints::default());
        // Cluster a grows by one hundredth of the gap: it touches nobody, so nobody moves, not even a's own children.
        let hints = OverviewHints {
            radius: BTreeMap::from([(0, before.radius[0] + options.gap / 100.)]),
            start: Some(previous(&before)),
        };
        let (_, after) = overview(&g, &options, &hints);
        for i in 0..h.ids.len() {
            assert_eq!(
                (after.x[i], after.y[i]),
                (before.x[i], before.y[i]),
                "{} moved",
                h.ids[i]
            );
        }
    }

    #[test]
    fn settling_closes_the_gap_a_shrinking_cluster_leaves() {
        let options = OverviewOptions::default();
        let g = sample();
        let (_, before) = overview(&g, &options, &OverviewHints::default());
        // Cluster a grows to five times its room, then is let go again: once only making room, once settling.
        let grow = OverviewHints {
            radius: BTreeMap::from([(0, before.radius[0] * 5.)]),
            start: Some(previous(&before)),
        };
        let (_, grown) = overview(&g, &options, &grow);
        let back = |settle| OverviewHints {
            radius: BTreeMap::new(),
            start: Some(Previous {
                settle,
                ..previous(&grown)
            }),
        };
        let (_, kept) = overview(&g, &options, &back(false));
        let (_, settled) = overview(&g, &options, &back(true));
        assert_eq!(kept.overlaps + settled.overlaps, 0);
        // Only making room leaves the gap: the root stays as large as with the grown cluster; settling closes it.
        assert!(
            kept.root_radius > before.root_radius * 1.5,
            "{} {}",
            kept.root_radius,
            before.root_radius
        );
        assert!(
            settled.root_radius < kept.root_radius * 0.8,
            "{} {}",
            settled.root_radius,
            kept.root_radius
        );
        // Deterministic.
        assert_eq!(overview(&g, &options, &back(true)).1, settled);
    }

    #[test]
    fn a_container_of_mostly_unlinked_children_is_not_split() {
        // 150 top-level nodes, of which only 20 are linked (a chain): no communities, no `unlinked` cluster.
        let nodes = (0..150).map(|i| node(&format!("n{i}"), None)).collect();
        let edges = (0..19)
            .map(|i| {
                edge(
                    &format!("e{i}"),
                    &format!("n{i}"),
                    Some(&format!("n{}", i + 1)),
                )
            })
            .collect();
        let h = with_communities(hierarchy(&graph(nodes, edges)), 120).unwrap();
        assert_eq!(h.ids.len(), 150);
    }
}
