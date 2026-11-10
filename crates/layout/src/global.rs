//! Provisional (plan section 4a, global layering, iteration 3; option `globalLayering`): a layered area lays out the
//! containers inside it whose flow runs the same axis and way as its own together with it, down to the leaves, in one set
//! of ranks. Every such container is a block that stays contiguous in each rank it spans and keeps one place among its
//! siblings in all of them (`layered_plan::hierarchical_order`), so that it is drawn as one rectangle.

use super::{direction_for, GraphIndex, Mode};
use arqlet_model::{Graph, LayoutOptions, Node, Rect};
use std::collections::{BTreeMap, BTreeSet};

/// The containers flattened into an area, with their place in each other.
#[derive(Debug, Default)]
pub(crate) struct Flattening {
    /// By flattened container, the flattened container it lies in (`None`: directly in the area).
    pub parent: BTreeMap<String, Option<String>>,
    /// By flattened container, its children (units or flattened containers), in graph order.
    pub children: BTreeMap<String, Vec<String>>,
    /// By unit (a node that takes part in the area's ranks), the flattened container it lies in directly.
    pub container_of: BTreeMap<String, String>,
}

impl Flattening {
    /// The containers of `area` (and inside them, down to the leaves) whose flow runs along the same axis and the same way
    /// as the area's (both fixed, neither reversed), with children, not pinned, without an authored size, not a circle,
    /// and with no pinned child. Empty without the option or when the area has no fixed, not reversed flow.
    pub fn of(
        area: Option<&str>,
        graph: &Graph,
        index: &GraphIndex<'_>,
        options: &LayoutOptions,
        children: &[&Node],
    ) -> Flattening {
        let mut out = Flattening::default();
        if !(options.global_layering && options.layered_construction) {
            return out;
        }
        let Mode::Orientation(outer) = direction_for(graph, area) else {
            return out;
        };
        if outer.reversed() {
            return out;
        }
        let flattenable = |node: &Node| -> Option<Vec<String>> {
            let inside: Vec<String> = index
                .children_of(Some(&node.id))
                .iter()
                .map(|&i| index.nodes[i].id.clone())
                .collect();
            if inside.is_empty()
                || options.pins.contains_key(&node.id)
                || options.container_sizing.contains_key(&node.id)
                || node.shape == arqlet_model::NodeShape::Circle
                || inside.iter().any(|id| options.pins.contains_key(id))
            {
                return None;
            }
            match direction_for(graph, Some(&node.id)) {
                Mode::Orientation(inner)
                    if inner.vertical() == outer.vertical() && !inner.reversed() =>
                {
                    Some(inside)
                }
                _ => None,
            }
        };
        let mut stack: Vec<(&Node, Option<String>)> = children.iter().map(|c| (*c, None)).collect();
        while let Some((node, parent)) = stack.pop() {
            match flattenable(node) {
                Some(inside) => {
                    out.parent.insert(node.id.clone(), parent);
                    out.children.insert(node.id.clone(), inside.clone());
                    for id in &inside {
                        if let Some(child) = index.node(id) {
                            stack.push((child, Some(node.id.clone())));
                        }
                    }
                }
                None => {
                    if let Some(p) = parent {
                        out.container_of.insert(node.id.clone(), p);
                    }
                }
            }
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// The units that replace `children` in the area's arrangement: each child, or, for a flattened container, its units in
    /// turn.
    pub fn units<'a>(&self, index: &GraphIndex<'a>, children: Vec<&'a Node>) -> Vec<&'a Node> {
        let mut out = Vec::new();
        for child in children {
            match self.children.get(&child.id) {
                Some(inside) => {
                    let nodes: Vec<&Node> = inside.iter().filter_map(|id| index.node(id)).collect();
                    out.extend(self.units(index, nodes));
                }
                None => out.push(child),
            }
        }
        out
    }

    /// The flattened containers around a unit or a container, outermost first (the container itself not included).
    pub fn path(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut at = self
            .container_of
            .get(id)
            .cloned()
            .or_else(|| self.parent.get(id).cloned().flatten());
        while let Some(c) = at {
            at = self.parent.get(&c).cloned().flatten();
            out.push(c);
        }
        out.reverse();
        out
    }

    /// Nesting depth of a flattened container (1 directly in the area).
    fn depth(&self, container: &str) -> usize {
        self.path(container).len() + 1
    }
}

/// The margins of a flattened container: (before, after) along the flow and (before, after) across it. The header lies on
/// top, so it is before along a vertical flow and before across a horizontal one.
pub(crate) fn margins(vertical: bool, (header, padding): (f64, f64)) -> ((f64, f64), (f64, f64)) {
    if vertical {
        ((header, padding), (padding, padding))
    } else {
        ((padding, padding), (header, padding))
    }
}

/// Packs the elements of the ranks across the flow, block by block: the children of the area (and of every block) in the
/// order of their keys; an element takes the next free place of its rank; a block starts after everything placed before it
/// in every rank it spans, packs its own children inside (a flattened container after its header or padding), and ends
/// after its padding. Returns the far end across the flow.
#[allow(clippy::too_many_arguments)]
pub(crate) fn skyline(
    levels: &[&Vec<String>],
    placed: &mut BTreeMap<String, Rect>,
    path: &dyn Fn(&str) -> Vec<String>,
    keys: &crate::layered_plan::HierarchyKeys,
    containers: &BTreeSet<String>,
    vertical: bool,
    gap: f64,
    start: f64,
    rules: (f64, f64),
) -> f64 {
    #[derive(Default)]
    struct Block {
        blocks: Vec<String>,
        elements: Vec<String>,
        first: usize,
        last: usize,
    }
    let mut rank_of: BTreeMap<&str, usize> = BTreeMap::new();
    let mut tree: BTreeMap<String, Block> = BTreeMap::new();
    let root = String::new();
    tree.insert(
        root.clone(),
        Block {
            first: 0,
            last: levels.len().saturating_sub(1),
            ..Default::default()
        },
    );
    for (k, ids) in levels.iter().enumerate() {
        for id in ids.iter() {
            rank_of.insert(id.as_str(), k);
            let p = path(id);
            let mut parent = root.clone();
            for b in &p {
                let new = !tree.contains_key(b);
                let entry = tree.entry(b.clone()).or_insert(Block {
                    first: k,
                    last: k,
                    ..Default::default()
                });
                entry.first = entry.first.min(k);
                entry.last = entry.last.max(k);
                if new {
                    tree.get_mut(&parent)
                        .expect("parent block")
                        .blocks
                        .push(b.clone());
                }
                parent = b.clone();
            }
            tree.get_mut(&parent)
                .expect("block")
                .elements
                .push(id.clone());
        }
    }
    let ((_, _), (before, after)) = margins(vertical, rules);
    let across = |r: &Rect| if vertical { r.width } else { r.height };
    fn pack(
        block: &str,
        tree: &BTreeMap<String, Block>,
        free: &mut [f64],
        placed: &mut BTreeMap<String, Rect>,
        rank_of: &BTreeMap<&str, usize>,
        order: &dyn Fn(&str, bool) -> f64,
        containers: &BTreeSet<String>,
        across: &dyn Fn(&Rect) -> f64,
        vertical: bool,
        gap: f64,
        (before, after): (f64, f64),
    ) {
        let b = &tree[block];
        let mut items: Vec<(f64, &String, bool)> = b
            .blocks
            .iter()
            .map(|x| (order(x, true), x, true))
            .chain(b.elements.iter().map(|x| (order(x, false), x, false)))
            .collect();
        items.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(b.1)));
        for (_, id, is_block) in items {
            if is_block {
                let inner = &tree[id.as_str()];
                let (first, last) = (inner.first, inner.last);
                let (lead, trail) = if containers.contains(id) {
                    (before, after)
                } else {
                    (0., 0.)
                };
                let begin = free[first..=last].iter().fold(f64::MIN, |a, &f| a.max(f));
                let mut inside: Vec<f64> = free.to_vec();
                for f in &mut inside[first..=last] {
                    *f = begin + lead;
                }
                pack(
                    id,
                    tree,
                    &mut inside,
                    placed,
                    rank_of,
                    order,
                    containers,
                    across,
                    vertical,
                    gap,
                    (before, after),
                );
                let end = inside[first..=last]
                    .iter()
                    .fold(begin + lead, |a, &f| a.max(f - gap))
                    .max(begin + lead)
                    + trail;
                for f in &mut free[first..=last] {
                    *f = end + gap;
                }
            } else if let (Some(&k), Some(r)) = (rank_of.get(id.as_str()), placed.get_mut(id)) {
                if vertical {
                    r.x = free[k];
                } else {
                    r.y = free[k];
                }
                free[k] += across(r) + gap;
            }
        }
    }
    let order = |id: &str, is_block: bool| {
        if is_block {
            keys.blocks.get(id).copied().unwrap_or(0.5)
        } else {
            keys.elements.get(id).copied().unwrap_or(0.5)
        }
    };
    let mut free = vec![start; levels.len()];
    pack(
        &root,
        &tree,
        &mut free,
        placed,
        &rank_of,
        &order,
        containers,
        &across,
        vertical,
        gap,
        (before, after),
    );
    free.into_iter().fold(start, f64::max) - gap
}

/// Room along the flow for the margins of the flattened containers: before the rank where a container begins, its margin
/// before (and those of the containers around it that begin there too); after the rank where it ends, its margin after
/// (likewise). Later ranks move along the flow. Returns the total shift.
pub(crate) fn main_margins(
    levels: &[&Vec<String>],
    placed: &mut BTreeMap<String, Rect>,
    path: &dyn Fn(&str) -> Vec<String>,
    containers: &BTreeSet<String>,
    vertical: bool,
    rules: (f64, f64),
) -> f64 {
    let ((before, after), _) = margins(vertical, rules);
    let mut span: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for (k, ids) in levels.iter().enumerate() {
        for id in ids.iter() {
            for c in path(id).into_iter().filter(|c| containers.contains(c)) {
                let e = span.entry(c).or_insert((k, k));
                e.0 = e.0.min(k);
                e.1 = e.1.max(k);
            }
        }
    }
    // A container's margins add to those of the containers around it that begin (end) in the same rank.
    let mut lead = vec![0f64; levels.len() + 1];
    let mut trail = vec![0f64; levels.len() + 1];
    for (c, &(first, last)) in &span {
        let around: Vec<&(usize, usize)> = path_of_container(c, path, levels)
            .iter()
            .filter_map(|o| span.get(o))
            .collect();
        let starting = 1 + around.iter().filter(|s| s.0 == first).count();
        let ending = 1 + around.iter().filter(|s| s.1 == last).count();
        lead[first] = lead[first].max(starting as f64 * before);
        trail[last + 1] = trail[last + 1].max(ending as f64 * after);
    }
    let mut shift = 0.;
    for (k, ids) in levels.iter().enumerate() {
        shift += lead[k] + trail[k];
        for id in ids.iter() {
            if let Some(r) = placed.get_mut(id) {
                if vertical {
                    r.y += shift;
                } else {
                    r.x += shift;
                }
            }
        }
    }
    shift + trail[levels.len()]
}

/// The containers around a flattened container, outermost first, read from the path of any element inside it.
fn path_of_container(
    container: &str,
    path: &dyn Fn(&str) -> Vec<String>,
    levels: &[&Vec<String>],
) -> Vec<String> {
    for ids in levels {
        for id in ids.iter() {
            let p = path(id);
            if let Some(i) = p.iter().position(|c| c == container) {
                return p[..i].to_vec();
            }
        }
    }
    Vec::new()
}

/// The rectangles of the flattened containers, innermost first: each around its children (units and inner containers)
/// with its header on top and its padding around; then every unit and container is given in the frame of the container it
/// lies in. `placed` holds the units in the area's frame on entry.
pub(crate) fn frames(
    flattening: &Flattening,
    placed: &mut BTreeMap<String, Rect>,
    (header, padding): (f64, f64),
) {
    let mut order: Vec<&String> = flattening.children.keys().collect();
    order.sort_by_key(|c| std::cmp::Reverse(flattening.depth(c)));
    let mut absolute: BTreeMap<String, Rect> = BTreeMap::new();
    for c in order {
        let rects: Vec<Rect> = flattening.children[c]
            .iter()
            .filter_map(|id| absolute.get(id).or_else(|| placed.get(id)).cloned())
            .collect();
        if rects.is_empty() {
            continue;
        }
        let x0 = rects.iter().map(|r| r.x).fold(f64::MAX, f64::min);
        let y0 = rects.iter().map(|r| r.y).fold(f64::MAX, f64::min);
        let x1 = rects.iter().map(|r| r.x + r.width).fold(f64::MIN, f64::max);
        let y1 = rects
            .iter()
            .map(|r| r.y + r.height)
            .fold(f64::MIN, f64::max);
        absolute.insert(
            c.clone(),
            Rect {
                x: x0 - padding,
                y: y0 - header,
                width: x1 - x0 + 2. * padding,
                height: y1 - y0 + header + padding,
            },
        );
    }
    let frame_of =
        |c: &Option<String>| c.as_ref().and_then(|c| absolute.get(c)).map(|r| (r.x, r.y));
    for (unit, c) in &flattening.container_of {
        if let (Some(r), Some((x, y))) = (placed.get_mut(unit), frame_of(&Some(c.clone()))) {
            r.x -= x;
            r.y -= y;
        }
    }
    for (c, rect) in &absolute {
        let mut r = rect.clone();
        if let Some((x, y)) = frame_of(&flattening.parent[c]) {
            r.x -= x;
            r.y -= y;
        }
        placed.insert(c.clone(), r);
    }
}
