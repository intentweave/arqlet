//! Provisional (plan section 4a, global layering, iteration 1): the first view of an import within a node budget.
//!
//! Containers are collapsed so that the visible nodes stay within the budget: the outermost first, then they are expanded
//! level by level (in id order within a level) while the budget holds. A collapsed container takes a size estimated from
//! its content, inside out, so that expanding it later moves the outer picture little.

use arqlet_model::{DirectionConstraint, Graph, NodeSize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// The size of a leaf without a measured size (as the layout's default).
const LEAF: (f64, f64) = (180., 54.);
/// Gaps of the estimate: between ranks along the flow, between neighbours across it, and the padding of a container.
const RANK_GAP: f64 = 60.;
const CROSS_GAP: f64 = 24.;
const PADDING: f64 = 24.;
/// A collapsed container is drawn as a miniature of its estimated size, never smaller than a leaf.
const MINIATURE: f64 = 0.25;

/// The containers to collapse so that at most `budget` nodes are visible (leaves and collapsed containers; an expanded
/// container is a frame, not counted). When the top level alone exceeds the budget, every top-level container stays
/// collapsed.
pub fn initial_collapse(graph: &Graph, budget: usize) -> BTreeSet<String> {
    let children = children_of(graph);
    let is_container = |id: &str| children.contains_key(id);
    let roots: Vec<&str> = graph
        .nodes
        .iter()
        .filter(|n| n.parent_id.is_none())
        .map(|n| n.id.as_str())
        .collect();
    let mut collapsed: BTreeSet<String> = roots
        .iter()
        .filter(|id| is_container(id))
        .map(|id| id.to_string())
        .collect();
    let mut visible = roots.len();
    // Level by level: a container is expanded when its children fit; its child containers start collapsed.
    let mut queue: VecDeque<String> = collapsed.iter().cloned().collect();
    while let Some(id) = queue.pop_front() {
        let inside = &children[id.as_str()];
        let after = visible - 1 + inside.len();
        if after > budget {
            continue;
        }
        visible = after;
        collapsed.remove(&id);
        for child in inside {
            if is_container(child) {
                collapsed.insert(child.to_string());
                queue.push_back(child.to_string());
            }
        }
    }
    collapsed
}

/// The drawn size of every collapsed container: a miniature of the size its content would take laid out in its flow
/// (ranks along the flow from the longest path over the edges between its children, the largest rank across it), with
/// child containers estimated the same way.
pub fn estimated_sizes(graph: &Graph, collapsed: &BTreeSet<String>) -> BTreeMap<String, NodeSize> {
    let children = children_of(graph);
    let parent: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .filter_map(|n| Some((n.id.as_str(), n.parent_id.as_deref()?)))
        .collect();
    let mut memo: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let mut out = BTreeMap::new();
    for id in collapsed {
        let (width, height) = content_size(graph, &children, &parent, id, &mut memo);
        out.insert(
            id.clone(),
            NodeSize {
                width: (width * MINIATURE).max(LEAF.0).round(),
                height: (height * MINIATURE).max(LEAF.1).round(),
            },
        );
    }
    out
}

fn children_of(graph: &Graph) -> BTreeMap<&str, Vec<&str>> {
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for n in &graph.nodes {
        if let Some(p) = n.parent_id.as_deref() {
            children.entry(p).or_default().push(n.id.as_str());
        }
    }
    children
}

/// The full (not miniature) size of a container's content.
fn content_size(
    graph: &Graph,
    children: &BTreeMap<&str, Vec<&str>>,
    parent: &BTreeMap<&str, &str>,
    id: &str,
    memo: &mut BTreeMap<String, (f64, f64)>,
) -> (f64, f64) {
    if let Some(size) = memo.get(id) {
        return *size;
    }
    let Some(inside) = children.get(id) else {
        return LEAF;
    };
    let sizes: Vec<(f64, f64)> = inside
        .iter()
        .map(|c| content_size(graph, children, parent, c, memo))
        .collect();
    // The child of `id` that holds a node, if any.
    let owner = |node: &str| -> Option<usize> {
        let mut at = node;
        loop {
            let up = *parent.get(at)?;
            if up == id {
                return inside.iter().position(|c| *c == at);
            }
            at = up;
        }
    };
    let mut links: BTreeSet<(usize, usize)> = BTreeSet::new();
    for e in &graph.edges {
        let Some(target) = e.target.as_deref() else {
            continue;
        };
        if let (Some(a), Some(b)) = (owner(&e.source), owner(target)) {
            if a != b {
                links.insert((a, b));
            }
        }
    }
    // Longest path ranks, bounded by the number of children (a cycle stops growing there).
    let n = inside.len();
    let mut rank = vec![0usize; n];
    for _ in 0..n {
        let mut changed = false;
        for &(a, b) in &links {
            if rank[b] < rank[a] + 1 && rank[a] + 1 < n {
                rank[b] = rank[a] + 1;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let direction = graph
        .container_directions
        .get(id)
        .unwrap_or(&graph.default_direction);
    let vertical = match direction {
        DirectionConstraint::Fixed(o) => Some(o.vertical()),
        _ => None,
    };
    let size = match vertical {
        Some(vertical) => {
            // Along the flow: the deepest child of each rank; across: the widest rank.
            let ranks = rank.iter().max().map_or(0, |r| r + 1);
            let mut along = vec![0f64; ranks];
            let mut across = vec![0f64; ranks];
            for (i, &(w, h)) in sizes.iter().enumerate() {
                let (main, cross) = if vertical { (h, w) } else { (w, h) };
                along[rank[i]] = along[rank[i]].max(main);
                across[rank[i]] += cross + CROSS_GAP;
            }
            let main = along.iter().sum::<f64>() + RANK_GAP * ranks.saturating_sub(1) as f64;
            let cross = across.iter().fold(0f64, |a, b| a.max(*b)) - CROSS_GAP;
            let (w, h) = if vertical {
                (cross, main)
            } else {
                (main, cross)
            };
            (w + 2. * PADDING, h + 2. * PADDING)
        }
        None => {
            // No fixed flow: a square grid of the children.
            let columns = (n as f64).sqrt().ceil().max(1.) as usize;
            let rows = n.div_ceil(columns);
            let w = sizes.iter().map(|s| s.0).fold(0f64, f64::max);
            let h = sizes.iter().map(|s| s.1).fold(0f64, f64::max);
            (
                columns as f64 * (w + CROSS_GAP) + 2. * PADDING,
                rows as f64 * (h + CROSS_GAP) + 2. * PADDING,
            )
        }
    };
    memo.insert(id.to_string(), size);
    size
}

#[cfg(test)]
mod tests {
    use super::*;
    use arqlet_model::LayoutOptions;
    use serde_json::json;

    fn graph(nodes: serde_json::Value, edges: serde_json::Value) -> Graph {
        serde_json::from_value(json!({
            "defaultDirection": "lr",
            "containerDirections": {},
            "defaultStrategy": "none",
            "containerStrategies": {},
            "nodes": nodes,
            "edges": edges,
        }))
        .unwrap()
    }

    fn sample() -> Graph {
        let node = |id: &str, parent: Option<&str>| json!({"id": id, "kind": "k", "label": id, "parentId": parent});
        graph(
            json!([
                node("A", None),
                node("B", None),
                node("a1", Some("A")),
                node("a2", Some("A")),
                node("A1", Some("A")),
                node("x", Some("A1")),
                node("y", Some("A1")),
                node("b1", Some("B")),
                node("b2", Some("B")),
                node("b3", Some("B")),
            ]),
            json!([
                {"id": "e1", "kind": "k", "source": "a1", "target": "a2"},
                {"id": "e2", "kind": "k", "source": "a2", "target": "x"},
            ]),
        )
    }

    #[test]
    fn the_outer_layer_is_collapsed_and_expanded_level_by_level_within_the_budget() {
        let g = sample();
        // Two top-level containers: both collapsed.
        assert_eq!(
            initial_collapse(&g, 2),
            BTreeSet::from(["A".to_string(), "B".to_string()])
        );
        // A (3 children) fits: 2 - 1 + 3 = 4; B (3 children) does not fit then (4 - 1 + 3 = 6 > 5); A1 (2) fits: 4 - 1 + 2 = 5.
        assert_eq!(initial_collapse(&g, 5), BTreeSet::from(["B".to_string()]));
        // Everything fits.
        assert!(initial_collapse(&g, 100).is_empty());
    }

    #[test]
    fn aggregated_edges_are_drawn_once_per_pair_and_parallel_edges_between_visible_nodes_stay() {
        let node = |id: &str, parent: Option<&str>| json!({"id": id, "kind": "k", "label": id, "parentId": parent});
        let edge =
            |id: &str, s: &str, t: &str| json!({"id": id, "kind": "k", "source": s, "target": t});
        let g = graph(
            json!([
                node("C", None),
                node("c1", Some("C")),
                node("c2", Some("C")),
                node("x", None),
                node("y", None)
            ]),
            json!([
                edge("e1", "c1", "x"),
                edge("e2", "c2", "x"),
                edge("e3", "x", "c1"),
                edge("e4", "x", "y"),
                edge("e5", "x", "y"),
            ]),
        );
        let projected = arqlet_view::project(&g, &BTreeSet::from(["C".to_string()])).unwrap();
        let merged = arqlet_view::merge_aggregated(&g, projected);
        let ids: Vec<&str> = merged.edges.iter().map(|e| e.id.as_str()).collect();
        // e1 and e2 both run C -> x: one is drawn; e3 runs the other way and stays; the parallel e4 and e5 both stay.
        assert_eq!(ids, ["e1", "e3", "e4", "e5"]);
    }

    #[test]
    fn expanding_within_a_budget_keeps_the_places_around_and_pushes_what_lies_after() {
        let node = |id: &str, parent: Option<&str>| json!({"id": id, "kind": "k", "label": id, "parentId": parent});
        let edge =
            |id: &str, s: &str, t: &str| json!({"id": id, "kind": "k", "source": s, "target": t});
        // x -> C -> D: C and D are collapsed in the first view (budget 3), C lies after x and before D.
        let g = graph(
            json!([
                node("x", None),
                node("C", None),
                node("c1", Some("C")),
                node("c2", Some("C")),
                node("c3", Some("C")),
                node("D", None),
                node("d1", Some("D")),
                node("d2", Some("D")),
            ]),
            json!([
                edge("e1", "x", "c1"),
                edge("e2", "c1", "c2"),
                edge("e3", "c2", "c3"),
                edge("e4", "c3", "d1"),
                edge("e5", "d1", "d2"),
            ]),
        );
        let options = LayoutOptions {
            visible_node_budget: 3,
            ..LayoutOptions::default()
        };
        let base = crate::import_graph_with(g, 100, &options).unwrap();
        assert_eq!(base.collapsed_ids, ["C", "D"]);
        let expanded =
            crate::toggle_container(&base, base.revision, base.layout_version, "C").unwrap();
        assert!(
            expanded.geometry_issues.is_empty(),
            "{:?}",
            expanded.geometry_issues
        );
        assert_eq!(expanded.collapsed_ids, ["D"]);
        // x lies before C: it keeps its place; D lies wholly after C along the flow: it moves on by C's growth.
        let (x0, x1) = (&base.boxes["x"], &expanded.boxes["x"]);
        assert!(
            (x0.x - x1.x).abs() < 1e-9 && (x0.y - x1.y).abs() < 1e-9,
            "{x0:?} {x1:?}"
        );
        let grown = expanded.boxes["C"].width - base.boxes["C"].width;
        assert!(grown > 0.);
        assert!((expanded.boxes["D"].x - base.boxes["D"].x - grown).abs() < 1e-6);
        // C's children are laid out inside it.
        for c in ["c1", "c2", "c3"] {
            let (r, f) = (&expanded.boxes[c], &expanded.boxes["C"]);
            assert!(
                r.x >= 0. && r.y >= 0. && r.x + r.width <= f.width && r.y + r.height <= f.height
            );
        }
    }

    #[test]
    fn a_collapsed_container_is_a_miniature_of_its_content_in_its_flow() {
        let g = sample();
        let sizes = estimated_sizes(&g, &BTreeSet::from(["A".to_string(), "B".to_string()]));
        // B: three unlinked leaves in one rank, stacked across a left-to-right flow: taller than wide before the miniature.
        let b = &sizes["B"];
        assert!(b.width >= LEAF.0 && b.height >= LEAF.1);
        // A: a1 -> a2 -> A1 make three ranks along the flow: wider than B.
        assert!(sizes["A"].width > b.width, "{sizes:?}");
    }
}
