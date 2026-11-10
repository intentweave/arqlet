//! Bounded fixed-order flow-axis realization. Does not run placement or search.
use super::*;
use arqlet_model::{AxisSize, LayoutOptions};

pub struct Drawing {
    pub boxes: BTreeMap<String, Rect>,
    pub plans: LayeredPlans,
    pub ports: Vec<ResolvedPort>,
}

/// Check support for growth without changing ranks, cross coordinates or port order.
/// Only non-flattened, uniformly positive-flow hierarchies without pins, fixed
/// container sizes, placement grids or extent bounds are supported.
pub fn check_capability(
    graph: &Graph,
    plans: &LayeredPlans,
    options: &LayoutOptions,
) -> Result<(), String> {
    crate::validate(graph)?;
    if !options.pins.is_empty() {
        return Err("Fixed-order channel growth does not support pinned nodes".into());
    }
    if options.placement_grid != 0. || options.max_extent.is_some() {
        return Err(
            "Fixed-order channel growth does not support a placement grid or maximum extent".into(),
        );
    }
    if options.container_sizing.values().any(|s| {
        matches!(s.width, Some(AxisSize::Fixed(_))) || matches!(s.height, Some(AxisSize::Fixed(_)))
    }) {
        return Err("Fixed-order channel growth does not support fixed container sizes".into());
    }
    // Without `singlePassMixed` every area needs a plan; with it, an area without one (organic, tree, radial) keeps its
    // children where its own placement put them, and they move apart by what they grow (`widen_planned`).
    if !options.single_pass_mixed && !plans.contains_key("<root>") {
        return Err("Fixed-order channel growth requires a root plan".into());
    }
    // Areas may differ in axis (their border is handled by the growth below), not in sense.
    if plans.values().any(|p| p.reversed) {
        return Err("Fixed-order channel growth requires positive flow directions".into());
    }
    let parents: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref().unwrap_or("<root>")))
        .collect();
    let mut ranked = BTreeSet::new();
    for (area, plan) in plans {
        for id in plan.ranks.iter().flatten() {
            if let Some(parent) = parents.get(id.as_str()) {
                if *parent != area || !ranked.insert(id.as_str()) {
                    return Err(
                        "Fixed-order channel growth does not support flattened membership".into(),
                    );
                }
            } else if !plan.dummies.contains_key(id) {
                return Err(format!(
                    "Fixed-order channel growth: missing dummy geometry {id}"
                ));
            }
        }
    }
    let unplanned = |n: &&arqlet_model::Node| {
        options.single_pass_mixed && !plans.contains_key(n.parent_id.as_deref().unwrap_or("<root>"))
    };
    if graph
        .nodes
        .iter()
        .any(|n| !ranked.contains(n.id.as_str()) && !unplanned(&n))
    {
        return Err("Fixed-order channel growth requires every node in its parent plan".into());
    }
    Ok(())
}

/// An area without a plan (`singlePassMixed`): when a child grew, the siblings that now overlap move apart (with the gap the
/// placement keeps), so the placement's arrangement stays as far as it can; the area then grows by how far its content
/// reaches beyond where it did.
fn spread_unplanned(
    drawing: &mut Drawing,
    before: &BTreeMap<String, Rect>,
    area: &str,
    children: &BTreeMap<&str, Vec<&str>>,
    shift: &dyn Fn(&mut Drawing, &str, &str, bool, f64) -> Result<(), String>,
) -> Result<(), String> {
    let kids: Vec<&str> = children.get(area).cloned().unwrap_or_default();
    // Two siblings that overlap (with the gap the placement keeps) move
    // apart, the later one in (x, y) order along the axis it overlaps least, with everything inside it. Repeated until no
    // pair overlaps, a bounded number of times; overlaps that remain are reported by the geometry check as before.
    const GAP: f64 = 24.;
    // Siblings that overlap before any growth (the placement of the area left them so) move apart too.
    {
        for _ in 0..4 * kids.len().max(1) {
            let mut order: Vec<&str> = kids.clone();
            order.sort_by(|a, b| {
                let (ra, rb) = (&drawing.boxes[*a], &drawing.boxes[*b]);
                ra.x.total_cmp(&rb.x)
                    .then(ra.y.total_cmp(&rb.y))
                    .then(a.cmp(b))
            });
            let mut moved = false;
            for i in 0..order.len() {
                for j in i + 1..order.len() {
                    let (a, b) = (
                        drawing.boxes[order[i]].clone(),
                        drawing.boxes[order[j]].clone(),
                    );
                    let dx = (a.x + a.width + GAP - b.x).min(b.x + b.width + GAP - a.x);
                    let dy = (a.y + a.height + GAP - b.y).min(b.y + b.height + GAP - a.y);
                    if dx <= EPS || dy <= EPS {
                        continue;
                    }
                    // Away from `a`: right or down when `b` starts after it, else left or up.
                    if dx <= dy {
                        let sign = if b.x + b.width / 2. >= a.x + a.width / 2. {
                            1.
                        } else {
                            -1.
                        };
                        shift(drawing, area, order[j], false, sign * dx)?;
                    } else {
                        let sign = if b.y + b.height / 2. >= a.y + a.height / 2. {
                            1.
                        } else {
                            -1.
                        };
                        shift(drawing, area, order[j], true, sign * dy)?;
                    }
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
    }
    if area != "<root>" && !kids.is_empty() {
        let reach = |boxes: &BTreeMap<String, Rect>, along_y: bool| {
            kids.iter()
                .map(|c| {
                    let r = &boxes[*c];
                    if along_y {
                        r.y + r.height
                    } else {
                        r.x + r.width
                    }
                })
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let growth = [
            (reach(&drawing.boxes, false) - reach(before, false)).max(0.),
            (reach(&drawing.boxes, true) - reach(before, true)).max(0.),
        ];
        let rect = drawing
            .boxes
            .get_mut(area)
            .ok_or_else(|| format!("Missing area geometry {area}"))?;
        rect.width += growth[0];
        rect.height += growth[1];
    }
    Ok(())
}

/// Produce an independent widened drawing; recompute demand from these exact inputs.
pub fn widen(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    plans: &LayeredPlans,
    options: &LayoutOptions,
) -> Result<Drawing, String> {
    let plan = abstract_routing::build(graph, plans)?;
    widen_planned(graph, boxes, ports, plans, options, &plan, None, false)
}

/// Demand is scheduled before final flow coordinates, without constructing
/// provisional routes or running joint/continuity planning. Cross coordinates,
/// endpoint domains and the selected abstract order are still supplied inputs.
pub(super) fn widen_planned(
    graph: &Graph,
    boxes: &BTreeMap<String, Rect>,
    ports: &[ResolvedPort],
    plans: &LayeredPlans,
    options: &LayoutOptions,
    plan: &Plan,
    session: Option<&mut planning::Session>,
    joint_demand: bool,
) -> Result<Drawing, String> {
    check_capability(graph, plans, options)?;
    let parents: BTreeMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.parent_id.as_deref().unwrap_or("<root>")))
        .collect();
    // A shifted container moves everything below it. The walk down this list replaces a scan of every node and its parent
    // chain per shifted container; the output is identical, and the saving shows on views that shift many containers
    // (benchmarks/results/single-pass-levers-20261106.md).
    let mut children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for n in &graph.nodes {
        if let Some(parent) = n.parent_id.as_deref() {
            children.entry(parent).or_default().push(n.id.as_str());
        } else if options.single_pass_mixed {
            // The top level, for an unplanned root (`spread_unplanned`); no node has this id.
            children.entry("<root>").or_default().push(n.id.as_str());
        }
    }
    let spacing = crate::constructive_spacing(options);
    validate_inputs(graph, boxes, ports, plans, spacing)?;
    let report = realize_plan(
        graph,
        boxes,
        ports,
        plans,
        spacing,
        plan,
        if joint_demand {
            Coordination::Joint
        } else {
            Coordination::None
        },
        true,
        session,
    )?;
    let mut drawing = Drawing {
        boxes: boxes.clone(),
        plans: plans.clone(),
        ports: ports.to_vec(),
    };
    let requirements: BTreeMap<(&str, usize), f64> = report
        .channels
        .iter()
        .map(|c| ((c.area.as_str(), c.rank), c.required))
        .collect();
    let original = Geometry { boxes, plans };
    // Children finish before their parents read the grown outlines. With `singlePassMixed` the areas without a plan take part
    // too (their children move apart by what they grew).
    let mut area_names: BTreeSet<String> = plans.keys().cloned().collect();
    if options.single_pass_mixed {
        area_names.insert("<root>".into());
        area_names.extend(children.keys().map(|c| c.to_string()));
    }
    let mut areas: Vec<_> = area_names
        .iter()
        .map(|area| {
            let mut depth = 0;
            let mut at = area.as_str();
            while let Some(parent) = parents.get(at) {
                depth += 1;
                at = parent;
            }
            (depth, area.as_str())
        })
        .collect();
    areas.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
    // Moves an element of an area, a node with everything below it or a dummy, by `shift` along x or y.
    let shift_element = |drawing: &mut Drawing,
                         area: &str,
                         id: &str,
                         along_y: bool,
                         shift: f64|
     -> Result<(), String> {
        let moved = |rect: &mut Rect| {
            if along_y {
                rect.y += shift;
            } else {
                rect.x += shift;
            }
        };
        if parents.contains_key(id) {
            let mut descendants = vec![id];
            let mut next = 0;
            while let Some(&at) = descendants.get(next) {
                next += 1;
                if let Some(below) = children.get(at) {
                    descendants.extend(below);
                }
            }
            for descendant in descendants {
                moved(
                    drawing
                        .boxes
                        .get_mut(descendant)
                        .ok_or_else(|| format!("Missing node geometry {descendant}"))?,
                );
            }
        } else {
            moved(
                drawing
                    .plans
                    .get_mut(area)
                    .unwrap()
                    .dummies
                    .get_mut(id)
                    .unwrap(),
            );
        }
        Ok(())
    };
    for (_, area) in areas {
        if !plans.contains_key(area) {
            spread_unplanned(&mut drawing, boxes, area, &children, &shift_element)?;
            continue;
        }
        let ranks = plans[area].ranks.clone();
        // Each area grows along its own flow. Children finish first, and a child area whose flow runs across this one's has
        // grown across it: the elements after it in its rank move on by that growth (the border between two areas of
        // different axes; with one axis everywhere nothing grows across a flow and nothing moves here).
        let vertical = plans[area].vertical;
        for rank in &ranks {
            let mut across = 0.;
            for id in rank {
                if across > EPS {
                    shift_element(&mut drawing, area, id, !vertical, across)?;
                }
                if children.contains_key(id.as_str()) {
                    let (before, after) = (&boxes[id.as_str()], &drawing.boxes[id.as_str()]);
                    across += if vertical {
                        after.width - before.width
                    } else {
                        after.height - before.height
                    }
                    .max(0.);
                }
            }
        }
        for rank in 1..ranks.len() {
            let (old_far, old_near) = original
                .main_span(area, rank - 1)
                .map_err(|f| format!("Fixed-order channel growth: {f:?}"))?;
            let current = Geometry {
                boxes: &drawing.boxes,
                plans: &drawing.plans,
            };
            let (far, near) = current
                .main_span(area, rank - 1)
                .map_err(|f| format!("Fixed-order channel growth: {f:?}"))?;
            let need = (old_near - old_far)
                .max(0.)
                .max(requirements.get(&(area, rank - 1)).copied().unwrap_or(0.));
            let shift = (far + need - near).max(0.);
            if !shift.is_finite() {
                return Err(format!(
                    "Fixed-order channel growth: nonfinite shift in {area}"
                ));
            }
            if shift <= EPS {
                continue;
            }
            for id in &ranks[rank] {
                shift_element(&mut drawing, area, id, vertical, shift)?;
            }
        }
        if area != "<root>" && ranks.iter().any(|r| !r.is_empty()) {
            // The area grows by what its content grew, along its flow and across it.
            let end = |geometry: &Geometry<'_>, along_y: bool| -> Result<f64, String> {
                ranks
                    .iter()
                    .flatten()
                    .map(|id| {
                        geometry
                            .rect(area, id)
                            .map(|r| {
                                if along_y {
                                    r.y + r.height
                                } else {
                                    r.x + r.width
                                }
                            })
                            .map_err(|f| format!("Fixed-order channel growth: {f:?}"))
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map(|v| v.into_iter().fold(f64::NEG_INFINITY, f64::max))
            };
            let grown = Geometry {
                boxes: &drawing.boxes,
                plans: &drawing.plans,
            };
            let mut growth = [0.; 2];
            for (axis, along_y) in [false, true].into_iter().enumerate() {
                growth[axis] = (end(&grown, along_y)? - end(&original, along_y)?).max(0.);
            }
            let rect = drawing
                .boxes
                .get_mut(area)
                .ok_or_else(|| format!("Missing area geometry {area}"))?;
            rect.width += growth[0];
            rect.height += growth[1];
        }
    }
    for port in &mut drawing.ports {
        let before = boxes
            .get(&port.node_id)
            .ok_or_else(|| format!("Missing port node {}", port.node_id))?;
        let after = &drawing.boxes[&port.node_id];
        let fraction =
            |at: f64, start: f64, size: f64| if size > 0. { (at - start) / size } else { 0.5 };
        let (x, y) = (
            fraction(port.position.x, before.x, before.width),
            fraction(port.position.y, before.y, before.height),
        );
        port.position = Point {
            x: if after.width == before.width {
                port.position.x + (after.x - before.x)
            } else {
                after.x + x * after.width
            },
            y: if after.height == before.height {
                port.position.y + (after.y - before.y)
            } else {
                after.y + y * after.height
            },
        };
    }
    if drawing
        .boxes
        .values()
        .chain(drawing.plans.values().flat_map(|p| p.dummies.values()))
        .any(|r| {
            ![r.x, r.y, r.width, r.height, r.x + r.width, r.y + r.height]
                .iter()
                .all(|v| v.is_finite())
        })
        || drawing
            .ports
            .iter()
            .any(|p| !p.position.x.is_finite() || !p.position.y.is_finite())
    {
        return Err("Fixed-order channel growth produced nonfinite geometry".into());
    }
    Ok(drawing)
}
