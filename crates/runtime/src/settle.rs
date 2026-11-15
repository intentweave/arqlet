//! The settling of an organic area after a drag: a few more damped steps of the stress placement from the positions the drag left,
//! with every pin held, until no node moves more than `CONVERGED_BELOW` or the steps are used. Each call is bounded
//! (`CONTINUATION_STEPS`), so a host calls it again only while the result is not converged and nothing newer has happened.
//! This is a continuation of the placement from its positions, not of a solver state: no solver state is kept. Provisional.
use super::{check_current, recompute_layout, uses_organic};
use arqlet_model::math::PortableMath;
use arqlet_model::{EditReport, Gesture, Settle, Snapshot};

/// The damped steps a call runs at most (each is one complete recomputation of the view from the positions before it).
pub const CONTINUATION_STEPS: usize = 3;
/// A node that moves less than this in a step (layout units) counts as settled.
pub const CONVERGED_BELOW: f64 = 0.5;
/// The id of the preview gesture the steps of a settle run under (`organicForces`); never in a published snapshot.
const SETTLE_GESTURE: &str = "settle";
/// The share of the step the next call of a settling keeps (with `organicForces`): 0.9 per call, a tenth after 22 calls.
pub const SETTLE_COOLING: f64 = 0.9;

/// The largest distance any node's box moved between two snapshots of the same view.
fn residual(before: &Snapshot, after: &Snapshot) -> f64 {
    after
        .boxes
        .iter()
        .filter_map(|(id, b)| {
            let a = before.boxes.get(id)?;
            Some((a.x - b.x).p_hypot(a.y - b.y))
        })
        .fold(0., f64::max)
}

/// Lets the free nodes of the organic areas settle. A view without force-directed behaviour or without an organic area has
/// nothing to settle: it is returned as it is, converged, without a new layout version.
pub fn settle_snapshot(
    snapshot: &Snapshot,
    expected: u64,
    layout: u64,
) -> Result<Snapshot, String> {
    check_current(snapshot, expected, layout)?;
    let source = snapshot.source_graph.as_ref().unwrap_or(&snapshot.graph);
    if !snapshot.options.force_directed || !uses_organic(source) {
        let mut same = snapshot.clone();
        same.edit_report = Some(EditReport {
            settle: Some(Settle {
                steps: 0,
                residual: 0.,
                converged: true,
                round: 0,
            }),
            ..Default::default()
        });
        return Ok(same);
    }
    let mut current = snapshot.clone();
    current.gesture = None;
    // With `organicForces` the steps are previews, as the steps of a force-directed drag: the forces move the nodes and the
    // repair does not run, so the two do not work against each other from step to step (with a repair in every step the
    // drawing swung by 80 units and more without converging). The step that converges is repaired, as the end of a drag.
    let forces = snapshot.options.organic_forces;
    let preview = |ending: bool| Gesture {
        id: SETTLE_GESTURE.into(),
        seq: 0,
        node: String::new(),
        active: true,
        previous: None,
        previous_others: Default::default(),
        ending,
        origin: None,
    };
    // The calls of one settling: each continues the round of the report it finds, and with the forces takes a smaller step
    // than the one before (`SETTLE_COOLING` per call), so that the settling converges instead of swinging.
    let round = snapshot
        .edit_report
        .as_ref()
        .and_then(|r| r.settle.as_ref())
        .filter(|s| !s.converged)
        .map_or(0, |s| s.round.saturating_add(1));
    let mut options = snapshot.options.clone();
    if forces {
        options.settle_cooling = Some(SETTLE_COOLING.p_powf(f64::from(round)));
    }
    let (mut steps, mut moved) = (0, f64::INFINITY);
    while steps < CONTINUATION_STEPS {
        if forces {
            current.gesture = Some(preview(false));
        }
        let next = recompute_layout(&current, &options, None)?;
        moved = residual(&current, &next);
        current = next;
        steps += 1;
        if moved < CONVERGED_BELOW {
            break;
        }
    }
    // The forces have converged: the repair ends the settling, as the drop ends a drag. Its moves are not the forces' and
    // would start them again (the repaired drawing is not their equilibrium), so the settling counts as converged and the
    // host stops asking; the residual reports the forces' last step.
    if forces && moved < CONVERGED_BELOW {
        current.gesture = Some(preview(true));
        current = recompute_layout(&current, &options, None)?;
    }
    current.gesture = None;
    current.options.settle_cooling = None;
    current.edit_report = Some(EditReport {
        settle: Some(Settle {
            steps,
            residual: moved,
            converged: moved < CONVERGED_BELOW,
            round,
        }),
        ..Default::default()
    });
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{configure_snapshot, import_view};
    use arqlet_model::Point;

    fn organic() -> Snapshot {
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/fixtures/structure/mixed-organic.json"),
        )
        .unwrap();
        let base = import_view(&text, 200).unwrap();
        configure_snapshot(
            &base,
            base.revision,
            base.layout_version,
            &serde_json::json!({"forceDirected": true}),
        )
        .unwrap()
    }

    #[test]
    fn a_view_without_force_directed_behaviour_has_nothing_to_settle() {
        let base = import_view(
            &std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../examples/fixtures/small.json"),
            )
            .unwrap(),
            100,
        )
        .unwrap();
        let same = settle_snapshot(&base, base.revision, base.layout_version).unwrap();
        assert_eq!(same.layout_version, base.layout_version);
        let report = same.edit_report.unwrap().settle.unwrap();
        assert!(report.converged && report.steps == 0);
    }

    #[test]
    fn settling_is_bounded_holds_the_pins_and_reports_the_residual() {
        let base = organic();
        let node = base
            .graph
            .nodes
            .iter()
            .find(|n| n.parent_id.is_some())
            .unwrap()
            .id
            .clone();
        let at = base.boxes[&node].clone();
        let pinned = configure_snapshot(
            &base,
            base.revision,
            base.layout_version,
            &serde_json::json!({"pins": {node.clone(): Point { x: at.x + 20., y: at.y }}}),
        )
        .unwrap();
        let settled = settle_snapshot(&pinned, pinned.revision, pinned.layout_version).unwrap();
        let report = settled.edit_report.clone().unwrap().settle.unwrap();
        assert!(
            report.steps >= 1 && report.steps <= CONTINUATION_STEPS,
            "{report:?}"
        );
        assert_eq!(report.converged, report.residual < CONVERGED_BELOW);
        // The pinned node did not move.
        let after = &settled.boxes[&node];
        assert!((after.x - (at.x + 20.)).abs() < 1e-6 && (after.y - at.y).abs() < 1e-6);
        // Stale requests are refused, and the same call gives the same result.
        assert!(settle_snapshot(&pinned, pinned.revision, pinned.layout_version + 1).is_err());
        let again = settle_snapshot(&pinned, pinned.revision, pinned.layout_version).unwrap();
        assert_eq!(
            serde_json::to_string(&again.boxes).unwrap(),
            serde_json::to_string(&settled.boxes).unwrap()
        );
    }
}
