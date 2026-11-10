//! One diagnostic probe followed by one chain-derived ordering pass. No retry loop.
use super::*;

#[derive(Clone, Debug, PartialEq)]
pub struct ChainCut {
    pub witness: usize,
    /// Indices into the probe's longitudinal reservations; first ends before second starts.
    pub pair: Option<(usize, usize)>,
    pub slack: Option<f64>,
    pub displacement: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub planning_passes: usize,
    pub probe_joint: JointSummary,
    pub probe_continuity: ContinuitySummary,
    pub cuts: Vec<ChainCut>,
    pub orders: Vec<(String, usize, String, usize)>,
    pub uncut_witnesses: usize,
}

fn endpoint_bound(
    lane: &LaneReservation,
    start: bool,
    upper: bool,
    joint: &JointSummary,
    trials: &BTreeMap<String, Trial>,
) -> Result<f64, String> {
    let trial = trials
        .get(&lane.edge)
        .ok_or_else(|| format!("Missing chain edge {}", lane.edge))?;
    if start && lane.junction == 0 {
        return Ok(trial.prepared.from.0);
    }
    if !start && lane.junction == trial.lanes.len() - 1 {
        return Ok(trial.prepared.to.0);
    }
    let section = if start {
        lane.junction - 1
    } else {
        lane.junction
    };
    let bend = joint
        .bends
        .iter()
        .find(|b| b.edge == lane.edge && b.section == section)
        .ok_or_else(|| format!("Missing chain bend {}:{section}", lane.edge))?;
    Ok(if upper { bend.bounds.1 } else { bend.bounds.0 })
}

fn select(
    joint: JointSummary,
    continuity: ContinuitySummary,
    trials: &BTreeMap<String, Trial>,
) -> Result<Summary, String> {
    let mut summary = Summary {
        planning_passes: 2,
        probe_joint: joint,
        probe_continuity: continuity,
        ..Summary::default()
    };
    let mut orders = BTreeSet::new();
    for (index, witness) in summary
        .probe_continuity
        .capacity_witnesses
        .iter()
        .enumerate()
    {
        let mut best: Option<(usize, usize, f64, f64)> = None;
        // Every adjacent constraint contributes one full track to this deficit.
        // Cut the feasible one requiring least nominal movement, then most slack.
        for pair in witness.chain.windows(2) {
            for (a, b) in [(pair[0], pair[1]), (pair[1], pair[0])] {
                let first = &summary.probe_continuity.lanes[a];
                let second = &summary.probe_continuity.lanes[b];
                if first.edge == second.edge {
                    continue;
                }
                let earliest_end =
                    endpoint_bound(first, false, false, &summary.probe_joint, trials)?;
                let latest_start =
                    endpoint_bound(second, true, true, &summary.probe_joint, trials)?;
                let slack = latest_start - earliest_end;
                let displacement = (first.main.1 - second.main.0).max(0.);
                if !slack.is_finite() || !displacement.is_finite() {
                    return Err("Nonfinite chain-cut feasibility".into());
                }
                if slack < -EPS {
                    continue;
                }
                if best.is_none_or(|(_, _, old_slack, old_displacement)| {
                    displacement < old_displacement
                        || (displacement == old_displacement && slack > old_slack)
                }) {
                    best = Some((a, b, slack, displacement));
                }
            }
        }
        let mut cut = ChainCut {
            witness: index,
            pair: None,
            slack: None,
            displacement: None,
        };
        if let Some((a, b, slack, displacement)) = best {
            let first = &summary.probe_continuity.lanes[a];
            let second = &summary.probe_continuity.lanes[b];
            orders.insert((
                first.edge.clone(),
                first.junction,
                second.edge.clone(),
                second.junction,
            ));
            cut.pair = Some((a, b));
            cut.slack = Some(slack);
            cut.displacement = Some(displacement);
        } else {
            summary.uncut_witnesses += 1;
        }
        summary.cuts.push(cut);
    }
    summary.orders = orders.into_iter().collect();
    Ok(summary)
}

/// The probe's coordinated state, the result of the ordering pass when the probe derives no chain orders.
pub(super) struct Probe {
    pub trials: BTreeMap<String, Trial>,
    pub channels: Vec<Channel>,
    pub joint: JointSummary,
    pub continuity: ContinuitySummary,
}

pub(super) fn assign(
    plan: &Plan,
    geometry: &coordination::Context<'_, '_>,
    trials: &BTreeMap<String, Trial>,
    spacing: Spacing,
    channels: &[Channel],
) -> Result<(Summary, Option<Probe>), String> {
    let mut probe_trials = trials.clone();
    let mut probe_channels = channels.to_vec();
    let (joint, bends) = joint::assign(
        plan,
        geometry,
        &mut probe_trials,
        spacing,
        &mut probe_channels,
        &[],
    )?;
    let continuity = continuity::assign(
        plan,
        geometry,
        &mut probe_trials,
        spacing,
        &mut probe_channels,
        Some(&bends),
    )?;
    let summary = select(joint, continuity, trials)?;
    let probe = summary.orders.is_empty().then(|| Probe {
        trials: probe_trials,
        channels: probe_channels,
        joint: summary.probe_joint.clone(),
        continuity: summary.probe_continuity.clone(),
    });
    Ok((summary, probe))
}
