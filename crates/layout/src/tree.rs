//! Tree and radial-tree placement of one area's children. Pure and deterministic. A tree is placed along an orientation
//! (`lr`, `rl`, `tb`, `bt`): the left-to-right arrangement is computed and transformed, so every orientation is the same
//! arrangement, mirrored or transposed. A radial tree is placed on rings and has no orientation. The children must form a
//! forest: `check_tree_areas` refuses an area whose links contain a cycle, two parents for one node, or a parallel or
//! antiparallel pair. A self-loop on a child is not part of the tree skeleton: the check and the arrangement ignore it, and
//! the router draws it. Nothing is removed silently. Routes are not placed here; they are routed like any other edge of
//! the drawing.

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
            let rect = if vertical {
                Rect {
                    x: r.y,
                    y: r.x,
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
        (lr_size.1, lr_size.0)
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
        centre.insert(id.clone(), (r * t.cos(), r * t.sin()));
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
