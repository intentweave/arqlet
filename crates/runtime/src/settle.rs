//! The settling of an organic area after a drag: a few more damped steps of the stress placement from the positions the drag left,
//! with every pin held, until no node moves more than `CONVERGED_BELOW` or the steps are used. Each call is bounded
//! (`CONTINUATION_STEPS`), so a host calls it again only while the result is not converged and nothing newer has happened.
//! This is a continuation of the placement from its positions, not of a solver state: no solver state is kept. Provisional.
use super::{check_current, recompute_layout, uses_organic};
use arqlet_model::{EditReport, Settle, Snapshot};

/// The damped steps a call runs at most (each is one complete recomputation of the view from the positions before it).
pub const CONTINUATION_STEPS: usize = 3;
/// A node that moves less than this in a step (layout units) counts as settled.
pub const CONVERGED_BELOW: f64 = 0.5;

/// The largest distance any node's box moved between two snapshots of the same view.
fn residual(before: &Snapshot, after: &Snapshot) -> f64 {
    after
        .boxes
        .iter()
        .filter_map(|(id, b)| {
            let a = before.boxes.get(id)?;
            Some((a.x - b.x).hypot(a.y - b.y))
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
            }),
            ..Default::default()
        });
        return Ok(same);
    }
    let mut current = snapshot.clone();
    current.gesture = None;
    let (mut steps, mut moved) = (0, f64::INFINITY);
    while steps < CONTINUATION_STEPS {
        let next = recompute_layout(&current, &snapshot.options, None)?;
        moved = residual(&current, &next);
        current = next;
        steps += 1;
        if moved < CONVERGED_BELOW {
            break;
        }
    }
    current.edit_report = Some(EditReport {
        settle: Some(Settle {
            steps,
            residual: moved,
            converged: moved < CONVERGED_BELOW,
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
