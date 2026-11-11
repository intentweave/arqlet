//! Evaluation modes of curated references (section 9.1 of the curated-references design). Provisional (C3):
//! - `constrained`: the engine lays out the reference's input with the curator's pins; pins are checked.
//! - `unconstrained`: the same input without pins, the algorithm's own result.
//! - `repair`: a recorded node move is applied to the curated geometry, the affected routes are rerouted, and the
//!   result is measured together with the movement outside the edited nodes.

use std::collections::{BTreeMap, BTreeSet};

use arqlet_model::{Point, Rect, Snapshot};
use arqlet_reference::{create, reroute, validate, ReferenceDocument};
use serde_json::{json, Value};

/// Tolerance for comparing a placed position with its pin.
const EPS: f64 = 1e-9;

/// Distance within which a route counts as affected by a moved node box.
const AFFECTED_MARGIN: f64 = 12.;

/// Lays out the reference's input and measures the result. `constrained` keeps the curator's pins, `unconstrained`
/// drops them. A structurally invalid or tampered reference is an error, not a result.
pub fn reproduce(value: &Value) -> Result<Value, String> {
    let document = parse(value)?;
    let curated = validate(&value["reference"])?;
    let constrained = match value["mode"].as_str().unwrap_or("constrained") {
        "constrained" => true,
        "unconstrained" => false,
        other => return Err(format!("Unknown reproduction mode: {other}")),
    };
    let snapshot = laid_out(&document, constrained)?;
    let pins = if constrained {
        pin_outcome(&snapshot.boxes, &document.input.options.pins)
    } else {
        Value::Null
    };
    let candidate = create(&snapshot, &format!("{}-candidate", document.reference_id))?;
    let measured = validate(&serde_json::to_value(&candidate).map_err(|e| e.to_string())?)?;
    Ok(json!({
        "variant": if constrained { "constrained" } else { "unconstrained" },
        "inputHash": document.input_hash,
        "pins": pins,
        "curated": {"valid": curated.valid, "hardViolations": curated.hard_violations, "metrics": curated.metrics},
        "candidate": {"valid": measured.valid, "hardViolations": measured.hard_violations, "metrics": measured.metrics},
        "candidateReference": candidate,
    }))
}

/// Applies recorded node moves to the curated geometry, reroutes the affected routes and measures the result.
/// `edits` is a list of `{nodeId, dx, dy}`; a moved node keeps its size and the move is not clamped.
pub fn repair(value: &Value) -> Result<Value, String> {
    let document = parse(value)?;
    let before = validate(&value["reference"])?;
    let edits = edits_of(value)?;
    if edits.is_empty() {
        return Err("A repair needs at least one recorded edit".into());
    }
    let original = document.curated.nodes.clone();
    let mut nodes = original.clone();
    let mut pending = BTreeSet::new();
    let mut edited = BTreeSet::new();
    for edit in &edits {
        let current = nodes
            .get(&edit.node_id)
            .ok_or_else(|| format!("Unknown node {}", edit.node_id))?
            .clone();
        let moved = Rect {
            x: current.x + edit.dx,
            y: current.y + edit.dy,
            width: current.width,
            height: current.height,
        };
        for (route_id, points) in &document.curated.routes {
            let attached = document.input.graph.edges.iter().any(|edge| {
                edge.id == *route_id
                    && (edge.source == edit.node_id
                        || edge.target.as_deref() == Some(edit.node_id.as_str()))
            });
            if attached || affected(points, &[&current, &moved]) {
                pending.insert(route_id.clone());
            }
        }
        nodes.insert(edit.node_id.clone(), moved);
        edited.insert(edit.node_id.clone());
    }
    let moved_elsewhere: Vec<String> = nodes
        .iter()
        .filter(|(id, rect)| {
            !edited.contains(*id) && original.get(*id).is_some_and(|old| !same(old, rect))
        })
        .map(|(id, _)| id.clone())
        .collect();
    let mut draft = document;
    draft.curated.nodes = nodes;
    draft.curated.pending_routes = pending.iter().cloned().collect();
    let repaired = if pending.is_empty() {
        draft
    } else {
        reroute(&serde_json::to_value(&draft).map_err(|e| e.to_string())?)?
    };
    let after = validate(&serde_json::to_value(&repaired).map_err(|e| e.to_string())?)?;
    Ok(json!({
        "variant": "repair",
        "edits": edits.len(),
        "pendingRoutes": pending,
        "movedElsewhere": moved_elsewhere,
        "before": {"valid": before.valid, "hardViolations": before.hard_violations, "metrics": before.metrics},
        "after": {"valid": after.valid, "hardViolations": after.hard_violations, "metrics": after.metrics},
        "repairedReference": repaired,
    }))
}

/// One recorded move: a node id and its offset.
struct Edit {
    node_id: String,
    dx: f64,
    dy: f64,
}

fn edits_of(value: &Value) -> Result<Vec<Edit>, String> {
    let items = value["edits"].as_array().ok_or("edits must be a list")?;
    items
        .iter()
        .map(|item| {
            Ok(Edit {
                node_id: item["nodeId"]
                    .as_str()
                    .ok_or("an edit needs a nodeId")?
                    .to_string(),
                dx: item["dx"].as_f64().ok_or("an edit needs a numeric dx")?,
                dy: item["dy"].as_f64().ok_or("an edit needs a numeric dy")?,
            })
        })
        .collect()
}

fn parse(value: &Value) -> Result<ReferenceDocument, String> {
    serde_json::from_value(value["reference"].clone()).map_err(|e| e.to_string())
}

/// The engine's layout of the reference's input, with or without its pins.
fn laid_out(document: &ReferenceDocument, keep_pins: bool) -> Result<Snapshot, String> {
    let input = &document.input;
    let graph = crate::project_view(
        &input.graph,
        &input.collapsed_ids,
        &input.options,
        crate::abstracts(&input.options),
    )?
    .projected;
    let mut options = input.options.clone();
    if !keep_pins {
        options.pins.clear();
    }
    let mut snapshot = arqlet_layout::compute(&graph, None, &BTreeSet::new(), &options)?;
    snapshot.source_graph = Some(input.graph.clone());
    snapshot.collapsed_ids = input.collapsed_ids.clone();
    Ok(snapshot)
}

/// Compares each pin with the position the engine placed. Pins on nodes outside the visible view are unplaced.
fn pin_outcome(boxes: &BTreeMap<String, Rect>, pins: &BTreeMap<String, Point>) -> Value {
    let mut honoured = Vec::new();
    let mut broken = Vec::new();
    let mut unplaced = Vec::new();
    for (id, pin) in pins {
        match boxes.get(id) {
            None => unplaced.push(id.clone()),
            Some(placed) if (placed.x - pin.x).abs() < EPS && (placed.y - pin.y).abs() < EPS => {
                honoured.push(id.clone())
            }
            Some(_) => broken.push(id.clone()),
        }
    }
    json!({
        "requested": pins.len(),
        "honoured": honoured,
        "broken": broken,
        "unplaced": unplaced,
    })
}

fn same(a: &Rect, b: &Rect) -> bool {
    (a.x - b.x).abs() < EPS
        && (a.y - b.y).abs() < EPS
        && (a.width - b.width).abs() < EPS
        && (a.height - b.height).abs() < EPS
}

/// Whether a route has a segment whose bounding box comes within the margin of one of the rectangles. Curated routes
/// are orthogonal, so the bounding box of a segment is the segment.
fn affected(points: &[Point], rects: &[&Rect]) -> bool {
    points.windows(2).any(|pair| {
        let (min_x, max_x) = (pair[0].x.min(pair[1].x), pair[0].x.max(pair[1].x));
        let (min_y, max_y) = (pair[0].y.min(pair[1].y), pair[0].y.max(pair[1].y));
        rects.iter().any(|rect| {
            min_x <= rect.x + rect.width + AFFECTED_MARGIN
                && max_x >= rect.x - AFFECTED_MARGIN
                && min_y <= rect.y + rect.height + AFFECTED_MARGIN
                && max_y >= rect.y - AFFECTED_MARGIN
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_is_honoured_when_the_placed_position_matches() {
        let boxes = BTreeMap::from([(
            "a".to_string(),
            Rect {
                x: 10.,
                y: 20.,
                width: 180.,
                height: 54.,
            },
        )]);
        let pins = BTreeMap::from([
            ("a".to_string(), Point { x: 10., y: 20. }),
            ("b".to_string(), Point { x: 0., y: 0. }),
            ("c".to_string(), Point { x: 0., y: 0. }),
        ]);
        let outcome = pin_outcome(&boxes, &pins);
        assert_eq!(outcome["honoured"], json!(["a"]));
        assert_eq!(outcome["unplaced"], json!(["b", "c"]));
        assert_eq!(outcome["broken"], json!([]));
    }

    #[test]
    fn a_moved_pin_is_reported_as_broken() {
        let boxes = BTreeMap::from([(
            "a".to_string(),
            Rect {
                x: 11.,
                y: 20.,
                width: 180.,
                height: 54.,
            },
        )]);
        let pins = BTreeMap::from([("a".to_string(), Point { x: 10., y: 20. })]);
        assert_eq!(pin_outcome(&boxes, &pins)["broken"], json!(["a"]));
    }

    #[test]
    fn a_route_far_from_the_box_is_not_affected() {
        let rect = Rect {
            x: 0.,
            y: 0.,
            width: 100.,
            height: 100.,
        };
        let far = [Point { x: 300., y: 300. }, Point { x: 400., y: 300. }];
        let near = [Point { x: 105., y: 50. }, Point { x: 200., y: 50. }];
        assert!(!affected(&far, &[&rect]));
        assert!(affected(&near, &[&rect]));
    }
}
