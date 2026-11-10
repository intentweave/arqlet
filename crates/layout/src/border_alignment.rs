//! Provisional (single-pass mode): border alignment from the top down.
//!
//! Every area is placed on its own, from the innermost out: a container's border nodes take their places from the inside
//! (they follow their neighbours in the container), and the parent then places the container as one element. An edge
//! that crosses several borders can therefore change its lane once in every area it passes, two bends each, where a
//! flat layout changes lane once. Measured on the uniform views (`benchmarks/results/single-pass-bends-20261107.md`):
//! the routes that bend in more than one area hold almost all bends single pass draws beyond elk-rs.
//!
//! After the placement, this pass visits the containers from the outside in. A border node whose edge changes lane both
//! outside the container (between the border and the edge's neighbour in the parent area) and inside it (between the
//! border and its neighbour in the container) moves onto the outside lane, or else onto the inside lane: one of the two
//! changes disappears, the other stays where it was. A node moves only within the gaps of its border rank (the order of
//! the rank stays, and its lane stays a track from its neighbours' lanes, as the lanes of a channel do) and the
//! container's margins, so nothing else moves. The container's port follows its border node
//! (`layered_plan::port_lanes`), and the realization grows the channels whose lane changes moved.

use std::collections::BTreeMap;

use arqlet_model::{Graph, Rect};

use crate::layered_plan::{AreaPlan, LayeredPlans};

/// Lanes closer than this are one lane.
const SAME_LANE: f64 = 0.5;

/// Moves border nodes onto the lane of their edge outside or inside the container, where both sides change lane. Returns
/// the number of border nodes moved.
pub(crate) fn align_borders(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    plans: &mut LayeredPlans,
    track: f64,
) -> usize {
    let parent: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref().unwrap_or("<root>")))
        .collect();
    let ends: BTreeMap<&str, (&str, Option<&str>)> = graph
        .edges
        .iter()
        .map(|e| (e.id.as_str(), (e.source.as_str(), e.target.as_deref())))
        .collect();
    let depth = |id: &str| {
        let mut d = 0;
        let mut up = parent.get(id).copied();
        while let Some(p) = up {
            d += 1;
            up = parent.get(p).copied();
        }
        d
    };
    // The containers with a plan and border nodes, from the outside in.
    let mut containers: Vec<String> = plans
        .iter()
        .filter(|(id, plan)| id.as_str() != "<root>" && !plan.borders.is_empty())
        .map(|(id, _)| id.clone())
        .collect();
    containers.sort_by_key(|id| (depth(id), id.clone()));
    let mut moved = 0;
    for container in containers {
        let area = parent.get(container.as_str()).copied().unwrap_or("<root>");
        let (Some(outer), Some(inner)) = (plans.get(area), plans.get(&container)) else {
            continue;
        };
        if outer.vertical != inner.vertical {
            continue;
        }
        let vertical = inner.vertical;
        let Some(frame) = boxes.get(&container) else {
            continue;
        };
        let origin = absolute(&container, boxes, &parent, vertical);
        let width = if vertical { frame.width } else { frame.height };
        let margin = if vertical { 24. } else { 48. };
        let mut targets: Vec<(String, f64)> = Vec::new();
        for (edge, border) in &inner.borders {
            let Some(rect) = inner.dummies.get(border) else {
                continue;
            };
            let (start, size) = across(rect, vertical);
            let here = origin + start + size / 2.;
            let entering = inner.ranks.first().is_some_and(|r| r.contains(border));
            let lane = |plan: &AreaPlan, area: &str, element: &str| {
                lane_at(
                    plan, area, element, edge, boxes, plans, &parent, &ends, vertical,
                )
            };
            let (Some(outside), Some(inside)) = (
                neighbour(outer, &container, edge, entering, &parent, &ends, area)
                    .and_then(|n| lane(outer, area, &n)),
                neighbour(inner, border, edge, !entering, &parent, &ends, &container)
                    .and_then(|n| lane(inner, &container, &n)),
            ) else {
                continue;
            };
            if (outside - here).abs() <= SAME_LANE || (inside - here).abs() <= SAME_LANE {
                continue;
            }
            targets.push((border.clone(), outside - origin - size / 2.));
            targets.push((border.clone(), inside - origin - size / 2.));
        }
        let plan = plans.get_mut(&container).expect("container plan");
        let mut done = std::collections::BTreeSet::new();
        for (border, start) in targets {
            if done.contains(&border) {
                continue;
            }
            let Some(rank) = plan.ranks.iter().find(|r| r.contains(&border)) else {
                continue;
            };
            let i = rank.iter().position(|x| x == &border).expect("in rank");
            let size = across(&plan.dummies[&border], vertical).1;
            let low = match i.checked_sub(1).and_then(|j| plan.dummies.get(&rank[j])) {
                Some(r) => {
                    let (s, w) = across(r, vertical);
                    s + w.max(track)
                }
                None => margin,
            };
            let high = match plan.dummies.get(rank.get(i + 1).map_or("", String::as_str)) {
                Some(r) => across(r, vertical).0 - size.max(track),
                None => width - margin - size,
            };
            if start < low - 1e-9 || start > high + 1e-9 {
                continue;
            }
            let rect = plan.dummies.get_mut(&border).expect("border rect");
            if vertical {
                rect.x = start;
            } else {
                rect.y = start;
            }
            done.insert(border);
            moved += 1;
        }
    }
    moved
}

fn across(r: &Rect, vertical: bool) -> (f64, f64) {
    if vertical {
        (r.x, r.width)
    } else {
        (r.y, r.height)
    }
}

/// The absolute start across the flow of an area's frame (`"<root>"`: 0).
fn absolute(
    area: &str,
    boxes: &BTreeMap<String, Rect>,
    parent: &BTreeMap<&str, &str>,
    vertical: bool,
) -> f64 {
    let mut at = area;
    let mut sum = 0.;
    while let Some(r) = boxes.get(at) {
        sum += across(r, vertical).0;
        match parent.get(at) {
            Some(&p) => at = p,
            None => break,
        }
    }
    sum
}

/// The element of `area` that holds `node`, or `None` when `node` is outside it.
fn child_of<'a>(node: &'a str, area: &str, parent: &BTreeMap<&'a str, &'a str>) -> Option<&'a str> {
    let mut at = node;
    loop {
        let up = *parent.get(at)?;
        if up == area {
            return Some(at);
        }
        at = up;
    }
}

/// The neighbour of `element` on the chain of `edge` in `plan` (the plan of `area`): upstream (`before`) or downstream.
fn neighbour<'a>(
    plan: &AreaPlan,
    element: &str,
    edge: &str,
    before: bool,
    parent: &BTreeMap<&'a str, &'a str>,
    ends: &BTreeMap<&'a str, (&'a str, Option<&'a str>)>,
    area: &str,
) -> Option<String> {
    let &(source, target) = ends.get(edge)?;
    let target = target?;
    // The other element of the edge in this area: the child holding the end that is not `element`'s, or the area's own
    // border node for the edge.
    let mut others: Vec<String> = [source, target]
        .iter()
        .filter_map(|end| child_of(end, area, parent))
        .filter(|c| *c != element)
        .map(str::to_string)
        .collect();
    if let Some(b) = plan.borders.get(edge) {
        if b != element {
            others.push(b.clone());
        }
    }
    for other in others {
        let (key, upstream) = if before {
            ((other.clone(), element.to_string()), true)
        } else {
            ((element.to_string(), other.clone()), false)
        };
        if let Some(dummies) = plan.chains.get(&key) {
            let next = if upstream {
                dummies.last()
            } else {
                dummies.first()
            };
            return Some(next.cloned().unwrap_or(other));
        }
    }
    None
}

/// The absolute lane across the flow at which `edge` crosses `element` of `plan` (the plan of `area`).
#[allow(clippy::too_many_arguments)]
fn lane_at(
    plan: &AreaPlan,
    area: &str,
    element: &str,
    edge: &str,
    boxes: &BTreeMap<String, Rect>,
    plans: &LayeredPlans,
    parent: &BTreeMap<&str, &str>,
    ends: &BTreeMap<&str, (&str, Option<&str>)>,
    vertical: bool,
) -> Option<f64> {
    let origin = if area == "<root>" {
        0.
    } else {
        absolute(area, boxes, parent, vertical)
    };
    if let Some(d) = plan.dummies.get(element) {
        let (start, size) = across(d, vertical);
        let offset = plan
            .lane_offsets
            .get(&(element.to_string(), edge.to_string()))
            .copied()
            .unwrap_or(size / 2.);
        return Some(origin + start + offset);
    }
    let r = boxes.get(element)?;
    let (start, size) = across(r, vertical);
    let base = origin + start;
    if let Some(inner) = plans.get(element) {
        if inner.vertical != vertical {
            return None;
        }
        let b = inner.borders.get(edge)?;
        let d = inner.dummies.get(b)?;
        let (s, w) = across(d, vertical);
        return Some(base + s + w / 2.);
    }
    let &(source, _) = ends.get(edge)?;
    let offset = plan
        .port_offsets
        .get(&(edge.to_string(), source == element))
        .copied()
        .unwrap_or(size / 2.);
    Some(base + offset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rect(x: f64, width: f64) -> Rect {
        Rect {
            x,
            y: 0.,
            width,
            height: 6.,
        }
    }

    /// Top to bottom: `s` in the root above container `c`, which holds `t`; edge `e` from `s` to `t` enters `c` by the border
    /// node `in-e`, which lies on neither its outside lane (the middle of `s`, 120) nor its inside lane (the middle of `t`,
    /// 170). `extra` adds a second border node `in-f` at 70 within `c`.
    fn case(extra: bool) -> (Graph, BTreeMap<String, Rect>, LayeredPlans) {
        let graph: Graph = serde_json::from_value(json!({
            "defaultDirection": "tb", "containerDirections": {},
            "defaultStrategy": "none", "containerStrategies": {},
            "nodes": [
                {"id": "s", "kind": "node", "label": "s"},
                {"id": "c", "kind": "node", "label": "c"},
                {"id": "t", "kind": "node", "label": "t", "parentId": "c"},
            ],
            "edges": [
                {"id": "e", "kind": "edge", "source": "s", "target": "t"},
                {"id": "f", "kind": "edge", "source": "s", "target": "t"},
            ],
        }))
        .unwrap();
        let boxes = BTreeMap::from([
            ("s".to_string(), rect(100., 40.)),
            ("c".to_string(), rect(50., 200.)),
            ("t".to_string(), rect(100., 40.)),
        ]);
        let root = AreaPlan {
            vertical: true,
            ranks: vec![vec!["s".into()], vec!["c".into()]],
            chains: BTreeMap::from([(("s".into(), "c".into()), Vec::new())]),
            ..AreaPlan::default()
        };
        let mut c = AreaPlan {
            vertical: true,
            ranks: vec![vec!["in-e".into()], vec!["t".into()]],
            chains: BTreeMap::from([(("in-e".into(), "t".into()), Vec::new())]),
            ..AreaPlan::default()
        };
        c.borders.insert("e".into(), "in-e".into());
        c.dummies.insert("in-e".into(), rect(20., 6.));
        if extra {
            c.ranks[0].push("in-f".into());
            c.borders.insert("f".into(), "in-f".into());
            c.dummies.insert("in-f".into(), rect(70., 6.));
        }
        let plans = BTreeMap::from([("<root>".to_string(), root), ("c".to_string(), c)]);
        (graph, boxes, plans)
    }

    #[test]
    fn a_border_node_takes_the_outside_lane_when_both_sides_change_lane() {
        let (graph, boxes, mut plans) = case(false);
        assert_eq!(align_borders(&graph, &boxes, &mut plans, 6.), 1);
        // The outside lane 120, in the frame of `c` (from 50), less half the node.
        assert_eq!(plans["c"].dummies["in-e"].x, 67.);
    }

    #[test]
    fn a_border_node_keeps_its_place_when_its_rank_has_no_room() {
        let (graph, boxes, mut plans) = case(true);
        // `in-f` at 70 leaves `in-e` room up to 64: neither lane fits, and `in-f` has no chain inside.
        assert_eq!(align_borders(&graph, &boxes, &mut plans, 6.), 0);
        assert_eq!(plans["c"].dummies["in-e"].x, 20.);
    }
}
