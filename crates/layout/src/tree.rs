//! Tree and radial-tree placement of one area's children. Pure and deterministic. A tree is placed along an orientation
//! (`lr`, `rl`, `tb`, `bt`): the left-to-right arrangement is computed and transformed, so every orientation is the same
//! arrangement, mirrored or transposed. A radial tree is placed on rings and has no orientation. The children must form a
//! forest: `check_tree_areas` refuses an area whose links contain a cycle, two parents for one node, or a parallel or
//! antiparallel pair. A self-loop on a child is not part of the tree skeleton: the check and the arrangement ignore it, and
//! the router draws it. Nothing is removed silently. Routes are not placed here; they are routed like any other edge of
//! the drawing.

use arqlet_model::math::PortableMath;
use arqlet_model::{Graph, Mode, Orientation, Rect};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The children of `parent` with their directed links (edges between two children, mapped to the children that own their
/// endpoints), as `place` computes them. A self-loop on a child is kept by this function; the check ignores it.
fn area_links(graph: &Graph, parent: Option<&str>) -> (Vec<String>, Vec<(String, String, bool)>) {
    let mut children: Vec<String> = graph
        .nodes
        .iter()
        .filter(|n| n.parent_id.as_deref() == parent)
        .map(|n| n.id.clone())
        .collect();
    children.sort();
    let child_set: BTreeSet<&str> = children.iter().map(String::as_str).collect();
    let parents: BTreeMap<&str, Option<&str>> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref()))
        .collect();
    let owner = |id: &str| -> Option<String> {
        let mut current = Some(id);
        while let Some(c) = current {
            if child_set.contains(c) {
                return Some(c.to_string());
            }
            current = *parents.get(c)?;
        }
        None
    };
    let mut links = Vec::new();
    for e in &graph.edges {
        let Some(target) = e.target.as_deref() else {
            continue;
        };
        if let (Some(a), Some(b)) = (owner(&e.source), owner(target)) {
            if a == b && e.source == target {
                links.push((a.clone(), b, true));
            } else if a != b {
                links.push((a, b, false));
            }
        }
    }
    (children, links)
}

/// Refuses every area placed as a tree or radial tree whose children do not form a forest. Areas placed otherwise are not
/// checked here; an area that cannot take its constraint is refused by `Graph::area_mode`.
pub fn check_tree_areas(graph: &Graph) -> Result<(), String> {
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
        let mode = graph.area_mode(parent)?;
        let kind = match mode {
            Mode::Tree(_) => "tree",
            Mode::Radial => "radial",
            _ => continue,
        };
        let name = parent.unwrap_or("the root");
        let (children, links) = area_links(graph, parent);
        // A self-loop does not change the tree: the arrangement ignores it, and the router draws it (ADR 0011).
        let links: Vec<(String, String, bool)> = links
            .into_iter()
            .filter(|(_, _, selfloop)| !*selfloop)
            .collect();
        let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
        for (a, b, _) in &links {
            let pair = if a < b {
                (a.clone(), b.clone())
            } else {
                (b.clone(), a.clone())
            };
            if !pairs.insert(pair) {
                return Err(format!(
                    "the {kind} arrangement of {name} refuses parallel or antiparallel links: the children do not form a tree"
                ));
            }
        }
        let mut in_links: BTreeMap<&str, usize> = BTreeMap::new();
        for (_, b, _) in &links {
            *in_links.entry(b.as_str()).or_default() += 1;
        }
        if in_links.values().any(|&n| n > 1) {
            return Err(format!(
                "the {kind} arrangement of {name} refuses a node with two parents: the children do not form a tree"
            ));
        }
        // A forest has exactly as many undirected links as nodes minus components.
        let index: BTreeMap<&str, usize> = children
            .iter()
            .enumerate()
            .map(|(i, c)| (c.as_str(), i))
            .collect();
        let mut parent_of: Vec<usize> = (0..children.len()).collect();
        fn find(parent_of: &mut [usize], mut x: usize) -> usize {
            while parent_of[x] != x {
                parent_of[x] = parent_of[parent_of[x]];
                x = parent_of[x];
            }
            x
        }
        let mut components = children.len();
        for (a, b) in &pairs {
            let (ra, rb) = (
                find(&mut parent_of, index[a.as_str()]),
                find(&mut parent_of, index[b.as_str()]),
            );
            if ra != rb {
                parent_of[ra] = rb;
                components -= 1;
            }
        }
        if pairs.len() + components != children.len() {
            return Err(format!(
                "the {kind} arrangement of {name} refuses a cycle: the children do not form a tree or forest"
            ));
        }
    }
    Ok(())
}

/// The forest of one area: the roots in order, the children of each node in id order, and the depth of each node.
struct Forest {
    roots: Vec<String>,
    children: BTreeMap<String, Vec<String>>,
    depth: BTreeMap<String, usize>,
}

fn forest(ids: &[String], links: &BTreeSet<(String, String)>) -> Forest {
    let mut neighbours: BTreeMap<&str, Vec<&str>> =
        ids.iter().map(|i| (i.as_str(), Vec::new())).collect();
    let mut indegree: BTreeMap<&str, usize> = ids.iter().map(|i| (i.as_str(), 0)).collect();
    for (a, b) in links {
        neighbours.get_mut(a.as_str()).unwrap().push(b.as_str());
        neighbours.get_mut(b.as_str()).unwrap().push(a.as_str());
        *indegree.get_mut(b.as_str()).unwrap() += 1;
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut roots = Vec::new();
    let mut children: BTreeMap<String, Vec<String>> =
        ids.iter().map(|i| (i.clone(), Vec::new())).collect();
    let mut depth: BTreeMap<String, usize> = BTreeMap::new();
    for start in ids {
        if seen.contains(start.as_str()) {
            continue;
        }
        // The component's root: a node without incoming links, else the smallest id.
        let mut component = vec![start.as_str()];
        let mut stack = vec![start.as_str()];
        seen.insert(start.as_str());
        while let Some(v) = stack.pop() {
            for w in neighbours[v].clone() {
                if seen.insert(w) {
                    component.push(w);
                    stack.push(w);
                }
            }
        }
        component.sort();
        let root = component
            .iter()
            .find(|c| indegree[*c] == 0)
            .copied()
            .unwrap_or(component[0]);
        roots.push(root.to_string());
        depth.insert(root.to_string(), 0);
        let mut queue = VecDeque::from([root]);
        let mut visited: BTreeSet<&str> = BTreeSet::from([root]);
        while let Some(v) = queue.pop_front() {
            for w in neighbours[v].clone() {
                if visited.insert(w) {
                    children.get_mut(v).unwrap().push(w.to_string());
                    depth.insert(w.to_string(), depth[v] + 1);
                    queue.push_back(w);
                }
            }
        }
    }
    for list in children.values_mut() {
        list.sort();
    }
    Forest {
        roots,
        children,
        depth,
    }
}

/// Left-to-right tree placement: depths form the columns, and each subtree takes the cross width of its node or of its
/// children, whichever is larger, so that a node is never narrower than the subtree it heads. Children are centred inside
/// their parent's slot. The margins are those of the other arrangements.
fn lr_tree(
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    sizes: &BTreeMap<String, (f64, f64)>,
    primary_gap: f64,
    cross_gap: f64,
    grid: f64,
) -> (BTreeMap<String, Rect>, (f64, f64)) {
    let f = forest(ids, links);
    let snap = |v: f64| {
        if grid > 0. {
            (v / grid).round() * grid
        } else {
            v
        }
    };
    fn width(node: &str, f: &Forest, sizes: &BTreeMap<String, (f64, f64)>, cross_gap: f64) -> f64 {
        let own = sizes[node].1;
        let kids = &f.children[node];
        if kids.is_empty() {
            return own;
        }
        let children: f64 = kids
            .iter()
            .map(|k| width(k, f, sizes, cross_gap))
            .sum::<f64>()
            + cross_gap * (kids.len() - 1) as f64;
        own.max(children)
    }
    fn place(
        node: &str,
        start: f64,
        f: &Forest,
        sizes: &BTreeMap<String, (f64, f64)>,
        cross_gap: f64,
        centre: &mut BTreeMap<String, f64>,
    ) {
        let slot = width(node, f, sizes, cross_gap);
        centre.insert(node.to_string(), start + slot / 2.);
        let kids = f.children[node].clone();
        if kids.is_empty() {
            return;
        }
        let used: f64 = kids
            .iter()
            .map(|k| width(k, f, sizes, cross_gap))
            .sum::<f64>()
            + cross_gap * (kids.len() - 1) as f64;
        let mut cursor = start + (slot - used) / 2.;
        for k in &kids {
            place(k, cursor, f, sizes, cross_gap, centre);
            cursor += width(k, f, sizes, cross_gap) + cross_gap;
        }
    }
    // Roots follow each other across the top margin of 48.
    let mut cursor = 48.;
    let mut cross_centre: BTreeMap<String, f64> = BTreeMap::new();
    for root in &f.roots {
        place(root, cursor, &f, sizes, cross_gap, &mut cross_centre);
        cursor += width(root, &f, sizes, cross_gap) + cross_gap;
    }
    let max_depth = f.depth.values().copied().max().unwrap_or(0);
    let mut band = vec![0f64; max_depth + 1];
    for id in ids {
        band[f.depth[id]] = band[f.depth[id]].max(sizes[id].0);
    }
    let mut primary_start = vec![0f64; max_depth + 1];
    let mut primary = snap(24.);
    for d in 0..=max_depth {
        primary_start[d] = primary;
        primary += band[d] + primary_gap;
    }
    let mut placed = BTreeMap::new();
    for id in ids {
        let (w, h) = sizes[id];
        placed.insert(
            id.clone(),
            Rect {
                x: primary_start[f.depth[id]],
                y: snap(cross_centre[id] - h / 2.),
                width: w,
                height: h,
            },
        );
    }
    let cross_extent = snap((cursor - cross_gap + 24.).max(90.));
    let size = (snap(primary - primary_gap + 24.), cross_extent);
    (placed, size)
}

/// Tree placement along an orientation. The left-to-right arrangement is computed on the transposed sizes for the vertical
/// orientations, transposed back, and mirrored on the rank axis for the reversed ones. Returns the boxes and the area size.
pub fn tree_arrange(
    orientation: Orientation,
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    sizes: &BTreeMap<String, (f64, f64)>,
    primary_gap: f64,
    cross_gap: f64,
    grid: f64,
) -> (BTreeMap<String, Rect>, (f64, f64)) {
    let vertical = orientation.vertical();
    let turned: BTreeMap<String, (f64, f64)> = sizes
        .iter()
        .map(|(id, &(w, h))| (id.clone(), if vertical { (h, w) } else { (w, h) }))
        .collect();
    let (lr, lr_size) = lr_tree(ids, links, &turned, primary_gap, cross_gap, grid);
    let mut placed: BTreeMap<String, Rect> = lr
        .into_iter()
        .map(|(id, r)| {
            // The left-to-right frame keeps 48 at its top for the container's header and 24 elsewhere; transposed, that room
            // would lie on the left and the children would start in the header. They move down by 24 and left by 24, so a
            // vertical tree keeps the same margins (nested-tree-radial: 24 children in their containers' headers).
            let rect = if vertical {
                Rect {
                    x: r.y - 24.,
                    y: r.x + 24.,
                    width: r.height,
                    height: r.width,
                }
            } else {
                r
            };
            (id, rect)
        })
        .collect();
    if orientation.reversed() && !placed.is_empty() {
        let lo_x = placed.values().map(|r| r.x).fold(f64::INFINITY, f64::min);
        let hi_x = placed
            .values()
            .map(|r| r.x + r.width)
            .fold(f64::NEG_INFINITY, f64::max);
        let lo_y = placed.values().map(|r| r.y).fold(f64::INFINITY, f64::min);
        let hi_y = placed
            .values()
            .map(|r| r.y + r.height)
            .fold(f64::NEG_INFINITY, f64::max);
        for r in placed.values_mut() {
            if vertical {
                r.y = lo_y + hi_y - r.y - r.height;
            } else {
                r.x = lo_x + hi_x - r.x - r.width;
            }
        }
    }
    let size = if vertical {
        (lr_size.1 - 24., lr_size.0 + 24.)
    } else {
        lr_size
    };
    (placed, size)
}

/// Radial placement: the root (or a virtual centre for several roots) at the middle, each depth on a ring, and every node
/// centred on the middle of its angular interval. The interval of a node is proportional to its number of leaves. A ring
/// is far enough out that every node on it fits in its interval, and the next ring is far enough out that the boxes of
/// consecutive depths do not touch; so unequal node sizes do not overlap. Returns the boxes and the area size.
pub fn radial_arrange(
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    sizes: &BTreeMap<String, (f64, f64)>,
    grid: f64,
) -> (BTreeMap<String, Rect>, (f64, f64)) {
    let f = forest(ids, links);
    let snap = |v: f64| {
        if grid > 0. {
            (v / grid).round() * grid
        } else {
            v
        }
    };
    const GAP: f64 = 48.;
    let tau = 2. * std::f64::consts::PI;
    fn count(node: &str, f: &Forest, leaves: &mut BTreeMap<String, usize>) -> usize {
        let kids = f.children[node].clone();
        let n = if kids.is_empty() {
            1
        } else {
            kids.iter().map(|k| count(k, f, leaves)).sum()
        };
        leaves.insert(node.to_string(), n);
        n
    }
    let mut leaves: BTreeMap<String, usize> = BTreeMap::new();
    for root in &f.roots {
        count(root, &f, &mut leaves);
    }
    let total: usize = f.roots.iter().map(|r| leaves[r]).sum();
    let single = f.roots.len() == 1;
    // Angular intervals: a single root spans the circle; several roots share it in proportion to their leaves.
    let mut interval: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    fn spread(
        node: &str,
        lo: f64,
        hi: f64,
        f: &Forest,
        leaves: &BTreeMap<String, usize>,
        interval: &mut BTreeMap<String, (f64, f64)>,
    ) {
        interval.insert(node.to_string(), (lo, hi));
        let kids = f.children[node].clone();
        let sum: usize = kids.iter().map(|k| leaves[k]).sum();
        let mut start = lo;
        for k in &kids {
            let share = (hi - lo) * leaves[k] as f64 / sum.max(1) as f64;
            spread(k, start, start + share, f, leaves, interval);
            start += share;
        }
    }
    let mut lo = 0.;
    for root in &f.roots {
        let share = tau * leaves[root] as f64 / total.max(1) as f64;
        let (a, b) = if single { (0., tau) } else { (lo, lo + share) };
        spread(root, a, b, &f, &leaves, &mut interval);
        lo += share;
    }
    // Rings: ring index = depth + offset (a single root is ring 0, the centre; several roots start on ring 1).
    let offset = if single { 0 } else { 1 };
    let max_depth = f.depth.values().copied().max().unwrap_or(0);
    let rings = max_depth + offset + 1;
    let diagonal_half = |id: &str| {
        let (w, h) = sizes[id];
        (w * w + h * h).sqrt() / 2.
    };
    let mut half = vec![0f64; rings];
    let mut needed = vec![0f64; rings];
    for id in ids {
        let level = f.depth[id] + offset;
        half[level] = half[level].max(diagonal_half(id));
        let (lo, hi) = interval[id];
        let wedge = (hi - lo).max(1e-9);
        needed[level] = needed[level].max((2. * diagonal_half(id) + GAP) / wedge);
    }
    let mut radius = vec![0f64; rings];
    for level in offset..rings {
        if single && level == 0 {
            continue;
        }
        let previous = if level == offset {
            0.
        } else {
            radius[level - 1]
        };
        let previous_half = if level == offset { 0. } else { half[level - 1] };
        radius[level] = needed[level].max(previous + previous_half + half[level] + GAP);
    }
    let mut centre: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    for id in ids {
        let r = radius[f.depth[id] + offset];
        let (lo, hi) = interval[id];
        let t = (lo + hi) / 2.;
        centre.insert(id.clone(), (r * t.p_cos(), r * t.p_sin()));
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for id in ids {
        let (cx, cy) = centre[id];
        let (w, h) = sizes[id];
        min_x = min_x.min(cx - w / 2.);
        min_y = min_y.min(cy - h / 2.);
        max_x = max_x.max(cx + w / 2.);
        max_y = max_y.max(cy + h / 2.);
    }
    let mut placed = BTreeMap::new();
    for id in ids {
        let (cx, cy) = centre[id];
        let (w, h) = sizes[id];
        placed.insert(
            id.clone(),
            Rect {
                x: snap(cx - w / 2. - min_x + 24.),
                y: snap(cy - h / 2. - min_y + 48.),
                width: w,
                height: h,
            },
        );
    }
    let size = (snap(max_x - min_x + 48.), snap(max_y - min_y + 72.));
    (placed, size)
}

/// The eight compass directions on the lattice, counter-clockwise from east (y grows downwards).
const COMPASS: [(i64, i64); 8] = [
    (1, 0),
    (1, -1),
    (0, -1),
    (-1, -1),
    (-1, 0),
    (-1, 1),
    (0, 1),
    (1, 1),
];

/// Octilinear star placement, for buses or metro lines: per connected group of the area's children (links taken in
/// either direction), the node with the most links (ties by id) is the middle; a breadth-first spanning tree from it
/// decides the branches. The middle's branches leave in distinct compass directions, the largest first and on the main
/// axes; below it, a node's largest branch goes straight on in its parent's direction and the others bend off by 45
/// degrees, then by 90 (alternating sides). Nodes sit on a square lattice whose step fits the largest box plus a gap, so a
/// straight chain lies on one row, column or diagonal; a node whose cell is taken moves further out along its direction.
/// Groups are placed side by side. Links outside the spanning tree are left to the router.
///
/// `lines` names the line of a link (key: the two ids in order; an explicit bus or an edge kind): a line runs straight
/// through the nodes it passes, so at a node the branch on the line it came by goes straight on, and at the middle two
/// branches of one line leave in opposite directions (as a metro line passes through an interchange). With `refine`
/// (provisional, option `octilinearRefine`), `refine_octilinear` then moves nodes and branches on the lattice so that the
/// links outside the spanning tree are octilinear too and the lines run straighter and shorter. Returns the boxes and the
/// area size.
pub fn octilinear_arrange(
    ids: &[String],
    links: &BTreeSet<(String, String)>,
    sizes: &BTreeMap<String, (f64, f64)>,
    grid: f64,
    lines: &BTreeMap<(String, String), String>,
    refine: bool,
) -> (BTreeMap<String, Rect>, (f64, f64)) {
    let line_of = |a: &str, b: &str| -> Option<&String> {
        let key = if a < b {
            (a.to_string(), b.to_string())
        } else {
            (b.to_string(), a.to_string())
        };
        lines.get(&key)
    };
    let snap = |v: f64| {
        if grid > 0. {
            (v / grid).round() * grid
        } else {
            v
        }
    };
    const GAP_X: f64 = 48.;
    const GAP_Y: f64 = 36.;
    let mut neighbours: BTreeMap<&str, BTreeSet<&str>> =
        ids.iter().map(|i| (i.as_str(), BTreeSet::new())).collect();
    for (a, b) in links {
        if a != b && neighbours.contains_key(a.as_str()) && neighbours.contains_key(b.as_str()) {
            neighbours.get_mut(a.as_str()).unwrap().insert(b.as_str());
            neighbours.get_mut(b.as_str()).unwrap().insert(a.as_str());
        }
    }
    // A square lattice, so that a step along a diagonal is one at 45 degrees; its side fits the largest box plus a gap.
    let step = ids.iter().fold(0f64, |m, id| {
        let (bw, bh) = sizes[id];
        m.max(bw + GAP_X).max(bh + GAP_Y)
    });
    let (step_x, step_y) = (step, step);
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut groups: Vec<BTreeMap<&str, (i64, i64)>> = Vec::new();
    // The sum of the distances from a node to the nodes it reaches (smaller: more central).
    let spread = |from: &str| -> usize {
        let mut depth: BTreeMap<&str, usize> = BTreeMap::from([(from, 0)]);
        let mut queue = std::collections::VecDeque::from([from]);
        while let Some(node) = queue.pop_front() {
            for &n in &neighbours[node] {
                if !depth.contains_key(n) {
                    depth.insert(n, depth[node] + 1);
                    queue.push_back(n);
                }
            }
        }
        depth.values().sum()
    };
    // The middles in order: the most linked node of what is left, the more central of equals, then the smaller id;
    // again and again for the groups left.
    loop {
        let middle = ids
            .iter()
            .map(String::as_str)
            .filter(|i| !seen.contains(i))
            .min_by(|a, b| {
                neighbours[b]
                    .len()
                    .cmp(&neighbours[a].len())
                    .then_with(|| spread(a).cmp(&spread(b)))
                    .then(a.cmp(b))
            });
        let Some(middle) = middle else { break };
        // The breadth-first spanning tree: each node's children in id order.
        let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        let mut order = vec![middle];
        seen.insert(middle);
        let mut k = 0;
        while k < order.len() {
            let node = order[k];
            k += 1;
            let mut kids = Vec::new();
            for &n in &neighbours[node] {
                if seen.insert(n) {
                    kids.push(n);
                    order.push(n);
                }
            }
            children.insert(node, kids);
        }
        fn size_of<'a>(
            node: &'a str,
            children: &BTreeMap<&'a str, Vec<&'a str>>,
            memo: &mut BTreeMap<&'a str, usize>,
        ) -> usize {
            if let Some(&n) = memo.get(node) {
                return n;
            }
            let n = 1 + children[node]
                .iter()
                .map(|c| size_of(c, children, memo))
                .sum::<usize>();
            memo.insert(node, n);
            n
        }
        let mut weight: BTreeMap<&str, usize> = BTreeMap::new();
        size_of(middle, &children, &mut weight);
        // Largest branch first; ties by id.
        let ranked = |node: &str| -> Vec<&str> {
            let mut kids = children[node].clone();
            kids.sort_by(|a, b| weight[b].cmp(&weight[a]).then(a.cmp(b)));
            kids
        };
        let mut cell: BTreeMap<&str, (i64, i64)> = BTreeMap::new();
        let mut taken: BTreeSet<(i64, i64)> = BTreeSet::new();
        cell.insert(middle, (0, 0));
        taken.insert((0, 0));
        // A node's place: one step from its parent in its direction, further while the cell is taken.
        let place = |from: (i64, i64), d: (i64, i64), taken: &mut BTreeSet<(i64, i64)>| {
            let mut at = (from.0 + d.0, from.1 + d.1);
            while taken.contains(&at) {
                at = (at.0 + d.0, at.1 + d.1);
            }
            taken.insert(at);
            at
        };
        // The middle's branches: east, west, north, south, then the diagonals. Two branches of one line take a pair of
        // opposite directions (largest line first), the others the free directions in that order.
        const FIRST: [usize; 8] = [0, 4, 2, 6, 1, 5, 3, 7];
        let mut stack: Vec<(&str, usize)> = Vec::new();
        let kids = ranked(middle);
        let mut used = [false; 8];
        let mut assigned: BTreeMap<&str, usize> = BTreeMap::new();
        let mut pairs: Vec<(&str, &str)> = Vec::new();
        for (i, &a) in kids.iter().enumerate() {
            if assigned.contains_key(a) || pairs.iter().any(|p| p.0 == a || p.1 == a) {
                continue;
            }
            let Some(line) = line_of(middle, a) else {
                continue;
            };
            if let Some(&b) = kids[i + 1..].iter().find(|&&b| {
                line_of(middle, b) == Some(line) && !pairs.iter().any(|p| p.0 == b || p.1 == b)
            }) {
                pairs.push((a, b));
            }
        }
        for (a, b) in pairs {
            if let Some(&d) = FIRST.iter().find(|&&d| !used[d] && !used[(d + 4) % 8]) {
                used[d] = true;
                used[(d + 4) % 8] = true;
                assigned.insert(a, d);
                assigned.insert(b, (d + 4) % 8);
            }
        }
        let mut next = FIRST.iter().copied().cycle();
        for kid in kids {
            let d = match assigned.get(kid) {
                Some(&d) => d,
                None => {
                    let free = FIRST.iter().copied().find(|&d| !used[d]);
                    let d = free.unwrap_or_else(|| next.next().unwrap());
                    used[d] = true;
                    d
                }
            };
            let at = place((0, 0), COMPASS[d], &mut taken);
            cell.insert(kid, at);
            stack.push((kid, d));
        }
        // Then breadth first: straight on (the branch on the line the node came by first), then 45 degrees to either
        // side, then 90.
        const TURNS: [i64; 5] = [0, 1, -1, 2, -2];
        let parent_of: BTreeMap<&str, &str> = children
            .iter()
            .flat_map(|(p, ks)| ks.iter().map(move |k| (*k, *p)))
            .collect();
        let mut queue: std::collections::VecDeque<(&str, usize)> = stack.into_iter().collect();
        while let Some((node, d)) = queue.pop_front() {
            let mut kids = ranked(node);
            if let Some(came) = parent_of.get(node).and_then(|p| line_of(p, node)) {
                if let Some(i) = kids.iter().position(|k| line_of(node, k) == Some(came)) {
                    let k = kids.remove(i);
                    kids.insert(0, k);
                }
            }
            for (i, kid) in kids.into_iter().enumerate() {
                let turn = TURNS[i % TURNS.len()];
                let nd = (d as i64 + turn).rem_euclid(8) as usize;
                let at = place(cell[node], COMPASS[nd], &mut taken);
                cell.insert(kid, at);
                queue.push_back((kid, nd));
            }
        }
        if refine {
            refine_octilinear(&mut cell, &neighbours, &line_of, &children);
        }
        groups.push(cell);
    }
    // Groups side by side, each from its own left edge.
    let mut centre: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut offset = 0.;
    for cell in &groups {
        let min_i = cell.values().map(|c| c.0).min().unwrap_or(0);
        let max_i = cell.values().map(|c| c.0).max().unwrap_or(0);
        for (id, (i, j)) in cell {
            centre.insert(
                id.to_string(),
                (offset + (i - min_i) as f64 * step_x, *j as f64 * step_y),
            );
        }
        offset += (max_i - min_i + 1) as f64 * step_x;
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for id in ids {
        let (cx, cy) = centre[id];
        let (w, h) = sizes[id];
        min_x = min_x.min(cx - w / 2.);
        min_y = min_y.min(cy - h / 2.);
        max_x = max_x.max(cx + w / 2.);
        max_y = max_y.max(cy + h / 2.);
    }
    let mut placed = BTreeMap::new();
    for id in ids {
        let (cx, cy) = centre[id];
        let (w, h) = sizes[id];
        placed.insert(
            id.clone(),
            Rect {
                x: snap(cx - w / 2. - min_x + 24.),
                y: snap(cy - h / 2. - min_y + 48.),
                width: w,
                height: h,
            },
        );
    }
    let size = (snap(max_x - min_x + 48.), snap(max_y - min_y + 72.));
    (placed, size)
}

/// Provisional (option `octilinearRefine`): a deterministic local search over one group's lattice cells after the star
/// placement, as hand-drawn metro maps are schematised. A move puts one node, or a node with its branch of the spanning
/// tree (`children`), on another free cell within a few steps; a move is taken when it lowers the cost, the best move per
/// node in id order, round after round until none helps (at most `ROUNDS`). The cost ranks, each far above the next: a
/// node lying on a link it does not end (a line–node hit), crossings and overlaps of links, links that are not octilinear
/// (not along a row, a column or a diagonal), bends of a line where it passes a node (45 degrees one unit, 90 two, …),
/// then the links' lengths in lattice steps (so bends count before length). All in integers but the angle classes, which
/// compare a cosine from `sqrt` (exactly rounded) with fixed bounds, so the result is the same on every platform. Groups
/// above `MAX_NODES` nodes are left as placed.
fn refine_octilinear<'a, 'l>(
    cell: &mut BTreeMap<&'a str, (i64, i64)>,
    neighbours: &BTreeMap<&'a str, BTreeSet<&'a str>>,
    line_of: &dyn Fn(&str, &str) -> Option<&'l String>,
    children: &BTreeMap<&'a str, Vec<&'a str>>,
) {
    const ROUNDS: usize = 30;
    const MAX_NODES: usize = 400;
    const RADIUS: i64 = 3;
    const BRANCH_RADIUS: i64 = 2;
    const HIT: i64 = 1_000_000_000;
    const CROSS: i64 = 1_000_000;
    const SKEW: i64 = 10_000;
    const BEND: i64 = 100;
    let nodes: Vec<&str> = cell.keys().copied().collect();
    let n = nodes.len();
    if !(3..=MAX_NODES).contains(&n) {
        return;
    }
    let index: BTreeMap<&str, usize> = nodes.iter().enumerate().map(|(i, &id)| (id, i)).collect();
    let mut pos: Vec<(i64, i64)> = nodes.iter().map(|id| cell[id]).collect();
    // The links between the group's nodes, each once (smaller index first), with their line.
    let mut line_ids: BTreeMap<&String, usize> = BTreeMap::new();
    let mut edges: Vec<(usize, usize, Option<usize>)> = Vec::new();
    for (i, &a) in nodes.iter().enumerate() {
        for &b in &neighbours[a] {
            let Some(&j) = index.get(b) else { continue };
            if i < j {
                let line = line_of(a, b).map(|l| {
                    let next = line_ids.len();
                    *line_ids.entry(l).or_insert(next)
                });
                edges.push((i, j, line));
            }
        }
    }
    let mut incident: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (e, &(a, b, _)) in edges.iter().enumerate() {
        incident[a].push(e);
        incident[b].push(e);
    }
    let other = |e: usize, v: usize| {
        if edges[e].0 == v {
            edges[e].1
        } else {
            edges[e].0
        }
    };
    // At each node, the pairs of its links on one line: where the line passes through it.
    let mut passes: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (v, list) in incident.iter().enumerate() {
        for (k, &e) in list.iter().enumerate() {
            for &f in &list[k + 1..] {
                if edges[e].2.is_some() && edges[e].2 == edges[f].2 {
                    passes[v].push((e, f));
                }
            }
        }
    }
    // Each node's branch of the spanning tree (itself included), sorted.
    let mut branch: Vec<Vec<usize>> = vec![Vec::new(); n];
    for (i, &id) in nodes.iter().enumerate() {
        let mut stack = vec![id];
        while let Some(at) = stack.pop() {
            branch[i].push(index[at]);
            if let Some(kids) = children.get(at) {
                stack.extend(kids.iter().copied().filter(|k| index.contains_key(k)));
            }
        }
        branch[i].sort_unstable();
    }
    let cross = |o: (i64, i64), a: (i64, i64), b: (i64, i64)| {
        (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
    };
    // Whether p lies on the segment a–b strictly between its ends.
    let inside = |p: (i64, i64), a: (i64, i64), b: (i64, i64)| {
        p != a
            && p != b
            && cross(a, b, p) == 0
            && (p.0 - a.0) * (p.0 - b.0) <= 0
            && (p.1 - a.1) * (p.1 - b.1) <= 0
    };
    // Two links: a proper crossing, or an overlap (collinear and sharing more than an end point).
    let pair = |pos: &[(i64, i64)], e: usize, f: usize| -> i64 {
        let (a, b, _) = edges[e];
        let (c, d, _) = edges[f];
        let (pa, pb, pc, pd) = (pos[a], pos[b], pos[c], pos[d]);
        if a == c || a == d || b == c || b == d {
            // Two links from one node in the same direction lie on each other.
            let (v, x, y) = if a == c {
                (pa, pb, pd)
            } else if a == d {
                (pa, pb, pc)
            } else if b == c {
                (pb, pa, pd)
            } else {
                (pb, pa, pc)
            };
            let (ux, uy, wx, wy) = (x.0 - v.0, x.1 - v.1, y.0 - v.0, y.1 - v.1);
            return if ux * wy - uy * wx == 0 && ux * wx + uy * wy > 0 {
                CROSS
            } else {
                0
            };
        }
        let (d1, d2) = (cross(pc, pd, pa), cross(pc, pd, pb));
        let (d3, d4) = (cross(pa, pb, pc), cross(pa, pb, pd));
        if ((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0))
        {
            return CROSS;
        }
        if inside(pa, pc, pd) || inside(pb, pc, pd) || inside(pc, pa, pb) || inside(pd, pa, pb) {
            return CROSS;
        }
        0
    };
    // A link on its own: not octilinear, and its length beyond one step.
    let own = |pos: &[(i64, i64)], e: usize| -> i64 {
        let (a, b, _) = edges[e];
        let (dx, dy) = ((pos[b].0 - pos[a].0).abs(), (pos[b].1 - pos[a].1).abs());
        let skew = if dx == 0 || dy == 0 || dx == dy {
            0
        } else {
            SKEW
        };
        skew + dx.max(dy) - 1
    };
    // The bend where a line passes v by links e and f, in 45-degree units (0: straight on).
    let bend = |pos: &[(i64, i64)], v: usize, e: usize, f: usize| -> i64 {
        let (x, y) = (pos[other(e, v)], pos[other(f, v)]);
        let (ux, uy) = ((x.0 - pos[v].0) as f64, (x.1 - pos[v].1) as f64);
        let (wx, wy) = ((y.0 - pos[v].0) as f64, (y.1 - pos[v].1) as f64);
        let norm = ((ux * ux + uy * uy) * (wx * wx + wy * wy)).sqrt();
        if norm == 0. {
            return 4;
        }
        let c = (ux * wx + uy * wy) / norm;
        // Bounds halfway between the classes: cos 157.5°, 112.5°, 67.5°, 22.5°.
        [
            -0.923_879_532_5,
            -0.382_683_432_4,
            0.382_683_432_4,
            0.923_879_532_5,
        ]
        .iter()
        .filter(|&&bound| c > bound)
        .count() as i64
    };
    // The cost of everything that involves a node of `moved` (sorted): the terms without one cancel when two placements of
    // the same set are compared.
    let local = |pos: &[(i64, i64)], moved: &[usize]| -> i64 {
        let in_moved = |v: usize| moved.binary_search(&v).is_ok();
        let touched: BTreeSet<usize> = moved
            .iter()
            .flat_map(|&v| incident[v].iter().copied())
            .collect();
        let mut total = 0;
        for &e in &touched {
            total += own(pos, e);
            let (a, b, _) = edges[e];
            for f in 0..edges.len() {
                if f == e || (touched.contains(&f) && f < e) {
                    continue;
                }
                total += pair(pos, e, f);
            }
            for (k, &p) in pos.iter().enumerate() {
                if k != a && k != b && inside(p, pos[a], pos[b]) {
                    total += HIT;
                }
            }
        }
        for &v in moved {
            for (f, &(a, b, _)) in edges.iter().enumerate() {
                if !touched.contains(&f) && inside(pos[v], pos[a], pos[b]) {
                    total += HIT;
                }
            }
        }
        let mut around: BTreeSet<usize> = moved.iter().copied().collect();
        for &v in moved {
            around.extend(incident[v].iter().map(|&e| other(e, v)));
        }
        for &v in &around {
            for &(e, f) in &passes[v] {
                if in_moved(v) || in_moved(other(e, v)) || in_moved(other(f, v)) {
                    total += BEND * bend(pos, v, e, f);
                }
            }
        }
        total
    };
    // The moves within r steps, nearest first.
    let offsets = |r: i64| -> Vec<(i64, i64)> {
        let mut list: Vec<(i64, i64)> = (-r..=r)
            .flat_map(|dx| (-r..=r).map(move |dy| (dx, dy)))
            .filter(|&d| d != (0, 0))
            .collect();
        list.sort_by_key(|&(dx, dy)| (dx.abs().max(dy.abs()), dx.abs() + dy.abs(), dx, dy));
        list
    };
    let (near, far) = (offsets(RADIUS), offsets(BRANCH_RADIUS));
    for _ in 0..ROUNDS {
        let mut improved = false;
        for v in 0..n {
            // The node alone, then its branch when that holds more than the node and less than the group.
            let alone = [v];
            let mut sets: Vec<(&[usize], &[(i64, i64)])> = vec![(&alone[..], &near[..])];
            if branch[v].len() > 1 && branch[v].len() < n {
                sets.push((&branch[v][..], &far[..]));
            }
            for (moved, moves) in sets {
                let taken: BTreeSet<(i64, i64)> = (0..n)
                    .filter(|k| moved.binary_search(k).is_err())
                    .map(|k| pos[k])
                    .collect();
                let before = local(&pos, moved);
                let mut best: Option<(i64, (i64, i64))> = None;
                let mut trial = pos.clone();
                for &(dx, dy) in moves {
                    if moved
                        .iter()
                        .any(|&k| taken.contains(&(pos[k].0 + dx, pos[k].1 + dy)))
                    {
                        continue;
                    }
                    for &k in moved {
                        trial[k] = (pos[k].0 + dx, pos[k].1 + dy);
                    }
                    let delta = local(&trial, moved) - before;
                    if delta < 0 && best.is_none_or(|(d, _)| delta < d) {
                        best = Some((delta, (dx, dy)));
                    }
                }
                if let Some((_, (dx, dy))) = best {
                    for &k in moved {
                        pos[k] = (pos[k].0 + dx, pos[k].1 + dy);
                    }
                    improved = true;
                }
            }
        }
        if !improved {
            break;
        }
    }
    for (i, id) in nodes.iter().enumerate() {
        cell.insert(id, pos[i]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arqlet_model::LayoutOptions;

    fn fixture(name: &str) -> Graph {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        serde_json::from_str(
            &std::fs::read_to_string(root.join("examples/fixtures/structure").join(name)).unwrap(),
        )
        .unwrap()
    }

    fn ids(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn sizes(names: &[&str]) -> BTreeMap<String, (f64, f64)> {
        names.iter().map(|n| (n.to_string(), (100., 40.))).collect()
    }

    #[test]
    fn an_octilinear_star_puts_the_hub_in_the_middle_and_chains_straight_out() {
        // A gateway on four buses (chains in either link direction), one bus with a branch at its second node.
        let names = ["gw", "a1", "a2", "a3", "b1", "b2", "c1", "c2", "d1", "x"];
        let links = links_of(&[
            ("a1", "gw"),
            ("a2", "a1"),
            ("a3", "a2"),
            ("gw", "b1"),
            ("b1", "b2"),
            ("c1", "gw"),
            ("c2", "c1"),
            ("gw", "d1"),
            ("a1", "x"),
        ]);
        let (placed, size) = octilinear_arrange(
            &ids(&names),
            &links,
            &sizes(&names),
            0.,
            &BTreeMap::new(),
            false,
        );
        let c = |id: &str| {
            let r = &placed[id];
            (r.x + r.width / 2., r.y + r.height / 2.)
        };
        assert!(size.0 > 0. && size.1 > 0.);
        // The largest branch (a, with x) leaves east, the next west: a chain lies on one row.
        let gw = c("gw");
        assert!(c("a1").0 > gw.0 && c("a2").0 > c("a1").0 && c("a3").0 > c("a2").0);
        assert!((c("a1").1 - gw.1).abs() < 1e-9 && (c("a3").1 - gw.1).abs() < 1e-9);
        assert!(c("b1").0 < gw.0 || c("c1").0 < gw.0);
        // The branch at a1 bends off by 45 degrees: one step along and one across.
        let (dx, dy) = (c("x").0 - c("a1").0, c("x").1 - c("a1").1);
        assert!(dx > 0. && dy.abs() > 0.);
        // No two boxes overlap.
        let all: Vec<&Rect> = placed.values().collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(
                    a.x + a.width <= b.x
                        || b.x + b.width <= a.x
                        || a.y + a.height <= b.y
                        || b.y + b.height <= a.y
                );
            }
        }
    }

    #[test]
    fn a_line_runs_straight_through_the_middle_and_the_nodes_it_passes() {
        // Two lines through a hub h: red a-h-b, blue c-h-d-e; d also starts a branch f on another line.
        let names = ["h", "a", "b", "c", "d", "e", "f"];
        let links = links_of(&[
            ("a", "h"),
            ("h", "b"),
            ("c", "h"),
            ("h", "d"),
            ("d", "e"),
            ("d", "f"),
        ]);
        let mut lines = BTreeMap::new();
        for (a, b, l) in [
            ("a", "h", "red"),
            ("b", "h", "red"),
            ("c", "h", "blue"),
            ("d", "h", "blue"),
            ("d", "e", "blue"),
            ("d", "f", "green"),
        ] {
            let key = if a < b {
                (a.to_string(), b.to_string())
            } else {
                (b.to_string(), a.to_string())
            };
            lines.insert(key, l.to_string());
        }
        let (placed, _) =
            octilinear_arrange(&ids(&names), &links, &sizes(&names), 0., &lines, false);
        let c = |id: &str| {
            let r = &placed[id];
            (r.x + r.width / 2., r.y + r.height / 2.)
        };
        let collinear = |p: (f64, f64), q: (f64, f64), r: (f64, f64)| {
            ((q.0 - p.0) * (r.1 - p.1) - (q.1 - p.1) * (r.0 - p.0)).abs() < 1e-6
        };
        // Each line passes straight through the hub, and blue goes on straight through d.
        assert!(collinear(c("a"), c("h"), c("b")), "red");
        assert!(collinear(c("c"), c("h"), c("d")), "blue");
        assert!(collinear(c("h"), c("d"), c("e")), "blue through d");
        assert!(!collinear(c("h"), c("d"), c("f")), "green bends off");
    }

    #[test]
    fn of_two_equally_linked_nodes_the_more_central_is_the_middle() {
        // A chain a-b-c-d-e: b, c and d have two links each; c is the centre.
        let names = ["a", "b", "c", "d", "e"];
        let links = links_of(&[("a", "b"), ("b", "c"), ("c", "d"), ("d", "e")]);
        let (placed, _) = octilinear_arrange(
            &ids(&names),
            &links,
            &sizes(&names),
            0.,
            &BTreeMap::new(),
            false,
        );
        let x = |id: &str| placed[id].x;
        // c in the middle, the chain straight east and west of it.
        assert!(
            x("a") < x("b") && x("b") < x("c") && x("c") < x("d") && x("d") < x("e")
                || x("e") < x("d") && x("d") < x("c") && x("c") < x("b") && x("b") < x("a")
        );
        assert!(placed.values().all(|r| (r.y - placed["c"].y).abs() < 1e-9));
    }

    #[test]
    fn an_octilinear_star_takes_a_cycle_and_separate_groups() {
        let names = ["a", "b", "c", "d", "e"];
        let links = links_of(&[("a", "b"), ("b", "c"), ("c", "a"), ("d", "e")]);
        let (placed, _) = octilinear_arrange(
            &ids(&names),
            &links,
            &sizes(&names),
            0.,
            &BTreeMap::new(),
            false,
        );
        assert_eq!(placed.len(), 5);
        // The second group sits right of the first.
        let right = |id: &str| placed[id].x + placed[id].width;
        assert!(
            placed["d"].x.min(placed["e"].x)
                >= ["a", "b", "c"].iter().map(|i| right(i)).fold(0., f64::max)
        );
    }

    /// Metro lines as (line, stations in order): the links between neighbours and the line of each.
    fn metro(
        lines: &[(&str, &[&str])],
    ) -> (
        Vec<String>,
        BTreeSet<(String, String)>,
        BTreeMap<(String, String), String>,
    ) {
        let mut names: BTreeSet<String> = BTreeSet::new();
        let mut links = BTreeSet::new();
        let mut line = BTreeMap::new();
        for (id, stations) in lines {
            for pair in stations.windows(2) {
                names.insert(pair[0].to_string());
                names.insert(pair[1].to_string());
                links.insert((pair[0].to_string(), pair[1].to_string()));
                let key = if pair[0] < pair[1] {
                    (pair[0].to_string(), pair[1].to_string())
                } else {
                    (pair[1].to_string(), pair[0].to_string())
                };
                line.entry(key).or_insert(id.to_string());
            }
        }
        (names.into_iter().collect(), links, line)
    }

    /// The made-up city of the playground's example metro-map (benchmarks/generate.py): five lines, cycles between them.
    const CITY: &[(&str, &[&str])] = &[
        (
            "red",
            &[
                "westfield",
                "mill-lane",
                "market",
                "old-town",
                "harbour",
                "eastgate",
            ],
        ),
        (
            "blue",
            &[
                "north-park",
                "hill-road",
                "market",
                "station-street",
                "riverside",
                "south-end",
            ],
        ),
        (
            "green",
            &[
                "lakeview",
                "college",
                "old-town",
                "station-street",
                "docklands",
            ],
        ),
        (
            "yellow",
            &["west-cross", "mill-lane", "hill-road", "college"],
        ),
        (
            "purple",
            &["airport", "docklands", "harbour", "eastgate", "sea-view"],
        ),
    ];
    /// The example vehicle-metro: five buses through a central gateway, a tree.
    const VEHICLE: &[(&str, &[&str])] = &[
        (
            "pt",
            &["engine", "transmission", "gateway", "battery", "thermal"],
        ),
        (
            "chassis",
            &["brake", "steering", "gateway", "suspension", "airbag"],
        ),
        (
            "body",
            &["gateway", "body", "climate", "door-fl", "door-fr", "seat"],
        ),
        ("lin", &["body", "window-rl", "window-rr", "rain", "mirror"]),
        (
            "eth",
            &[
                "obd",
                "gateway",
                "head-unit",
                "cluster",
                "telematics",
                "adas",
                "camera",
                "radar",
            ],
        ),
    ];

    /// What a schematic metro map is judged by, from the placed boxes' centres: links not octilinear, bends of the lines
    /// where they pass a station (45-degree units), crossings and overlaps of links, stations lying on a link they do not
    /// end, and the links' total length in lattice steps (the largest box plus its gap is one step).
    #[derive(Debug, PartialEq)]
    struct Schematic {
        skewed: usize,
        bends: usize,
        crossings: usize,
        hits: usize,
        length: usize,
    }

    fn judge(
        placed: &BTreeMap<String, Rect>,
        links: &BTreeSet<(String, String)>,
        line: &BTreeMap<(String, String), String>,
    ) -> Schematic {
        let step = 100. + 48.;
        let c = |id: &str| {
            let r = &placed[id];
            (
                ((r.x + r.width / 2.) / step).round() as i64,
                ((r.y + r.height / 2.) / step).round() as i64,
            )
        };
        let edges: Vec<(&str, &str)> = links
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        let cross = |o: (i64, i64), a: (i64, i64), b: (i64, i64)| {
            (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0)
        };
        let inside = |p: (i64, i64), a: (i64, i64), b: (i64, i64)| {
            p != a
                && p != b
                && cross(a, b, p) == 0
                && (p.0 - a.0) * (p.0 - b.0) <= 0
                && (p.1 - a.1) * (p.1 - b.1) <= 0
        };
        let mut s = Schematic {
            skewed: 0,
            bends: 0,
            crossings: 0,
            hits: 0,
            length: 0,
        };
        for (k, &(a, b)) in edges.iter().enumerate() {
            let (pa, pb) = (c(a), c(b));
            let (dx, dy) = ((pb.0 - pa.0).abs(), (pb.1 - pa.1).abs());
            if !(dx == 0 || dy == 0 || dx == dy) {
                s.skewed += 1;
            }
            s.length += dx.max(dy) as usize;
            s.hits += placed.keys().filter(|id| inside(c(id), pa, pb)).count();
            for &(x, y) in &edges[k + 1..] {
                if [a, b].contains(&x) || [a, b].contains(&y) {
                    continue;
                }
                let (px, py) = (c(x), c(y));
                let (d1, d2, d3, d4) = (
                    cross(px, py, pa),
                    cross(px, py, pb),
                    cross(pa, pb, px),
                    cross(pa, pb, py),
                );
                if (d1 * d2 < 0 && d3 * d4 < 0)
                    || inside(px, pa, pb)
                    || inside(py, pa, pb)
                    || inside(pa, px, py)
                    || inside(pb, px, py)
                {
                    s.crossings += 1;
                }
            }
        }
        // Bends: at each station, each pair of its links on one line.
        let key = |a: &str, b: &str| {
            if a < b {
                (a.to_string(), b.to_string())
            } else {
                (b.to_string(), a.to_string())
            }
        };
        for v in placed.keys() {
            let around: Vec<&str> = edges
                .iter()
                .filter_map(|&(a, b)| {
                    if a == v {
                        Some(b)
                    } else if b == v {
                        Some(a)
                    } else {
                        None
                    }
                })
                .collect();
            for (i, &x) in around.iter().enumerate() {
                for &y in &around[i + 1..] {
                    if line.get(&key(v, x)) != line.get(&key(v, y)) {
                        continue;
                    }
                    let (pv, px, py) = (c(v), c(x), c(y));
                    let (ux, uy, wx, wy) = (
                        (px.0 - pv.0) as f64,
                        (px.1 - pv.1) as f64,
                        (py.0 - pv.0) as f64,
                        (py.1 - pv.1) as f64,
                    );
                    let cos =
                        (ux * wx + uy * wy) / ((ux * ux + uy * uy) * (wx * wx + wy * wy)).sqrt();
                    s.bends += [-0.9238795325, -0.3826834324, 0.3826834324, 0.9238795325]
                        .iter()
                        .filter(|&&b| cos > b)
                        .count();
                }
            }
        }
        s
    }

    #[test]
    fn the_octilinear_refinement_schematises_a_metro_map() {
        for (name, lines) in [("city", CITY), ("vehicle", VEHICLE)] {
            let (names, links, line) = metro(lines);
            let sizes: BTreeMap<String, (f64, f64)> =
                names.iter().map(|n| (n.clone(), (100., 40.))).collect();
            let (star, _) = octilinear_arrange(&names, &links, &sizes, 0., &line, false);
            let (refined, _) = octilinear_arrange(&names, &links, &sizes, 0., &line, true);
            let (before, after) = (judge(&star, &links, &line), judge(&refined, &links, &line));
            eprintln!("{name}: star {before:?}, refined {after:?}");
            // Never worse on what ranks first: hits, crossings, skewed links, bends.
            assert!(
                after.hits <= before.hits && after.crossings <= before.crossings,
                "{name}: {after:?}"
            );
            assert!(
                after.skewed <= before.skewed && after.bends <= before.bends,
                "{name}: {after:?}"
            );
            if name == "city" {
                // The star leaves a closing link skewed, three crossings, a station on a line and eight bends; the
                // refinement removes all but three bends, for two lattice steps more.
                assert_eq!(
                    before,
                    Schematic {
                        skewed: 1,
                        bends: 8,
                        crossings: 3,
                        hits: 1,
                        length: 22
                    }
                );
                assert_eq!(
                    after,
                    Schematic {
                        skewed: 0,
                        bends: 3,
                        crossings: 0,
                        hits: 0,
                        length: 24
                    }
                );
            } else {
                // A tree already drawn straight stays as it is.
                assert_eq!(after, before);
            }
            // Deterministic: the same input gives the same boxes.
            assert_eq!(
                refined,
                octilinear_arrange(&names, &links, &sizes, 0., &line, true).0
            );
        }
    }

    fn links_of(pairs: &[(&str, &str)]) -> BTreeSet<(String, String)> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    /// A parent is centred over its children, and depths are columns: the chain goes left to right.
    #[test]
    fn tree_places_depths_as_columns_and_parents_over_their_children() {
        let names = ["r", "a", "b", "c", "d"];
        let links = links_of(&[("r", "a"), ("r", "b"), ("a", "c"), ("a", "d")]);
        let (placed, size) = tree_arrange(
            Orientation::Lr,
            &ids(&names),
            &links,
            &sizes(&names),
            64.,
            24.,
            0.,
        );
        assert!(placed["a"].x > placed["r"].x && placed["c"].x > placed["a"].x);
        let centre_a = placed["a"].y + placed["a"].height / 2.;
        let centre_of_children = (placed["c"].y + placed["d"].y + placed["d"].height) / 2.;
        assert!(
            (centre_a - centre_of_children).abs() < 1e-6,
            "a centred over c and d"
        );
        assert!(size.0 > 0. && size.1 > 0.);
    }

    /// Every orientation keeps the container's margins: the header (36) above the children and the padding (12) on the
    /// other sides, in a vertical tree too (it was computed left to right and transposed, which put the header room on
    /// the left).
    #[test]
    fn a_tree_of_every_orientation_leaves_the_header_free() {
        let names = ["r", "a", "b", "c"];
        let links = links_of(&[("r", "a"), ("r", "b"), ("a", "c")]);
        let rules = arqlet_scene::GeometryRules::default();
        for o in [
            Orientation::Lr,
            Orientation::Rl,
            Orientation::Tb,
            Orientation::Bt,
        ] {
            let (placed, size) =
                tree_arrange(o, &ids(&names), &links, &sizes(&names), 64., 24., 0.);
            for (id, r) in &placed {
                assert!(
                    r.y >= rules.header - 1e-6,
                    "{o:?}: {id} below the header: {r:?}"
                );
                assert!(
                    r.x >= rules.padding - 1e-6,
                    "{o:?}: {id} inside the padding: {r:?}"
                );
                assert!(
                    r.x + r.width <= size.0 - rules.padding + 1e-6,
                    "{o:?}: {id} within the width"
                );
                assert!(
                    r.y + r.height <= size.1 - rules.padding + 1e-6,
                    "{o:?}: {id} within the height"
                );
            }
        }
    }

    /// Every orientation is the same arrangement: tb puts the depths in rows, and the reversed ones point the other way.
    #[test]
    fn tree_takes_every_orientation_by_transformation() {
        let names = ["r", "a", "b"];
        let links = links_of(&[("r", "a"), ("a", "b")]);
        let place = |o| tree_arrange(o, &ids(&names), &links, &sizes(&names), 64., 24., 0.).0;
        let lr = place(Orientation::Lr);
        let rl = place(Orientation::Rl);
        let tb = place(Orientation::Tb);
        let bt = place(Orientation::Bt);
        assert!(lr["b"].x > lr["a"].x && lr["a"].x > lr["r"].x);
        assert!(rl["b"].x < rl["a"].x && rl["a"].x < rl["r"].x);
        assert!(tb["b"].y > tb["a"].y && tb["a"].y > tb["r"].y);
        assert!(bt["b"].y < bt["a"].y && bt["a"].y < bt["r"].y);
        // The vertical arrangement keeps the node extents: the box keeps its width and height.
        assert_eq!(tb["a"].width, 100.);
        assert_eq!(tb["a"].height, 40.);
    }

    /// The radial root sits at the centre, and each depth lies on its own ring.
    #[test]
    fn radial_puts_the_root_at_the_centre_and_depths_on_rings() {
        let names = ["r", "a", "b", "c"];
        let links = links_of(&[("r", "a"), ("r", "b"), ("a", "c")]);
        let (placed, _) = radial_arrange(&ids(&names), &links, &sizes(&names), 0.);
        let centre = |id: &str| {
            let r = &placed[id];
            (r.x + r.width / 2., r.y + r.height / 2.)
        };
        let mid = centre("r");
        let dist = |id: &str| {
            let c = centre(id);
            ((c.0 - mid.0).powi(2) + (c.1 - mid.1).powi(2)).sqrt()
        };
        assert!(
            dist("a") > 1.0 && (dist("a") - dist("b")).abs() < 1e-6,
            "a and b share the first ring"
        );
        assert!(
            dist("c") > dist("a"),
            "the second depth lies outside the first"
        );
    }

    /// Unequal node sizes do not overlap, neither across a ring nor between siblings: the radial and the tree placement
    /// are checked with a wide node among narrow ones.
    #[test]
    fn unequal_sizes_do_not_overlap_in_either_arrangement() {
        let names = ["r", "a", "b", "c", "d", "e"];
        let links = links_of(&[("r", "a"), ("r", "b"), ("a", "c"), ("a", "d"), ("b", "e")]);
        let mut uneven = sizes(&names);
        uneven.insert("a".into(), (320., 180.));
        uneven.insert("c".into(), (60., 200.));
        let overlaps = |placed: &BTreeMap<String, Rect>| {
            let mut count = 0;
            let items: Vec<&Rect> = placed.values().collect();
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
        };
        let (radial, _) = radial_arrange(&ids(&names), &links, &uneven, 0.);
        assert_eq!(
            overlaps(&radial),
            0,
            "radial rings keep unequal boxes apart"
        );
        for o in [Orientation::Lr, Orientation::Tb] {
            let (tree, _) = tree_arrange(o, &ids(&names), &links, &uneven, 64., 24., 0.);
            assert_eq!(overlaps(&tree), 0, "tree {o:?} keeps unequal boxes apart");
        }
    }

    /// Cycles, two parents, parallel links and antiparallel links are refused by name, never removed. A self-loop is allowed.
    #[test]
    fn cyclic_or_non_tree_areas_are_refused_not_repaired() {
        let mut cycle = fixture("tree-cycle.json");
        cycle.default_strategy =
            arqlet_model::StrategyConstraint::Fixed(arqlet_model::LayoutStrategy::Tree);
        let err = check_tree_areas(&cycle).unwrap_err();
        assert!(err.contains("cycle") && err.contains("the root"), "{err}");
        // The default layout refuses the same graph; it does not lay it out with another arrangement.
        let refused = crate::compute(&cycle, None, &BTreeSet::new(), &LayoutOptions::default());
        assert!(refused.unwrap_err().contains("cycle"));
        // A diamond: two parents for one node, acyclic as a directed graph but not a tree.
        let mut diamond = fixture("tree.json");
        diamond.edges.push(arqlet_model::Edge {
            id: "diamond".into(),
            kind: "flow".into(),
            source: "b".into(),
            target: Some("d".into()),
            weight: None,
        });
        assert!(check_tree_areas(&diamond)
            .unwrap_err()
            .contains("two parents"));
        let mut parallel = fixture("tree.json");
        parallel.edges.push(parallel.edges[0].clone());
        parallel.edges.last_mut().unwrap().id = "dup".into();
        assert!(check_tree_areas(&parallel)
            .unwrap_err()
            .contains("parallel"));
        let mut looped = fixture("tree.json");
        looped.edges.push(arqlet_model::Edge {
            id: "loop".into(),
            kind: "flow".into(),
            source: "a".into(),
            target: Some("a".into()),
            weight: None,
        });
        // A self-loop is allowed in a tree area: the arrangement ignores it, and the router draws it.
        assert!(check_tree_areas(&looped).is_ok());
    }

    /// A tree root composes a complete, valid drawing, for every direction the tree can take.
    #[test]
    fn a_tree_root_composes_a_complete_valid_drawing_in_every_orientation() {
        use arqlet_model::{DirectionConstraint, LayoutStrategy, Orientation, StrategyConstraint};
        for orientation in [
            Orientation::Lr,
            Orientation::Rl,
            Orientation::Tb,
            Orientation::Bt,
        ] {
            let mut graph = fixture("tree.json");
            graph.default_direction = DirectionConstraint::Fixed(orientation);
            graph.default_strategy = StrategyConstraint::Fixed(LayoutStrategy::Tree);
            let snapshot =
                crate::compute(&graph, None, &BTreeSet::new(), &LayoutOptions::default()).unwrap();
            assert_eq!(snapshot.routes.len(), graph.edges.len(), "{orientation:?}");
            assert!(
                snapshot.geometry_issues.is_empty(),
                "{orientation:?}: {:?}",
                snapshot.geometry_issues
            );
        }
    }
}
