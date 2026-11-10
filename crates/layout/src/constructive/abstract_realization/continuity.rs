//! Global longitudinal reservations with bounded lanes and a fixed, deterministic order.
//! No offsets are probed and no path is searched. Capacity failures are order-specific.
use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub reservations: usize,
    pub overlapping_pairs: usize,
    pub constraints: usize,
    pub cross_area_constraints: usize,
    pub shifted: usize,
    pub rejected_edges: usize,
    pub lanes: Vec<LaneReservation>,
    pub separation_constraints: Vec<(usize, usize)>,
    pub capacity_witnesses: Vec<CapacityWitness>,
}

/// Propagated longest path when an edge exceeds its selected-order domain.
/// Edge removals may already have relaxed part of this historical path;
/// it certifies the traced bounds, not graph infeasibility or an active minimum.
#[derive(Clone, Debug, PartialEq)]
pub struct CapacityWitness {
    pub edge: String,
    pub junction: usize,
    pub chain: Vec<usize>,
    pub lower: f64,
    pub upper: f64,
    pub required: f64,
    pub deficit: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LaneReservation {
    pub edge: String,
    pub junction: usize,
    pub area: String,
    pub main: (f64, f64),
    pub bounds: (f64, f64),
    pub preferred: f64,
    /// None means this entire edge was rejected by the selected lane order.
    pub coordinate: Option<f64>,
}

struct Reservation {
    edge: String,
    junction: usize,
    area: String,
    main: (f64, f64),
    bounds: (f64, f64),
    preferred: f64,
    vertical: bool,
}

/// Minimal feasible values for the selected order. A failed edge is explicitly
/// removed, including its earlier reservations; there is no retry/reordering.
fn solve(
    reservations: &[Reservation],
    predecessors: &[Vec<usize>],
    track: f64,
) -> (Vec<f64>, BTreeMap<String, Failure>, Vec<CapacityWitness>) {
    let mut values = vec![0.; reservations.len()];
    let mut failed = BTreeMap::new();
    let mut witnesses = Vec::new();
    let mut cause = vec![None; reservations.len()];
    for (i, r) in reservations.iter().enumerate() {
        if failed.contains_key(&r.edge) {
            continue;
        }
        let mut value = r.bounds.0;
        for &j in &predecessors[i] {
            if !failed.contains_key(&reservations[j].edge) && values[j] + track > value {
                value = values[j] + track;
                cause[i] = Some(j);
            }
        }
        if !value.is_finite() || value > r.bounds.1 + EPS {
            let mut chain = vec![i];
            let mut at = i;
            while let Some(previous) = cause[at] {
                chain.push(previous);
                at = previous;
            }
            chain.reverse();
            let lower = reservations[at].bounds.0;
            let required = chain.len().saturating_sub(1) as f64 * track;
            witnesses.push(CapacityWitness {
                edge: r.edge.clone(),
                junction: r.junction,
                chain,
                lower,
                upper: r.bounds.1,
                required,
                deficit: value - r.bounds.1,
            });
            failed.insert(
                r.edge.clone(),
                Failure::LongitudinalCapacity {
                    junction: r.junction,
                    required: value - r.bounds.0,
                    available: r.bounds.1 - r.bounds.0,
                },
            );
        } else {
            values[i] = value.min(r.bounds.1);
        }
    }
    // Removing a failed edge can only relax these constraints. Propagate upper
    // bounds backwards, then choose the nearest preferred value in fixed order.
    let mut upper: Vec<_> = reservations.iter().map(|r| r.bounds.1).collect();
    for i in (0..reservations.len()).rev() {
        if failed.contains_key(&reservations[i].edge) {
            continue;
        }
        for &j in &predecessors[i] {
            if !failed.contains_key(&reservations[j].edge) {
                upper[j] = upper[j].min(upper[i] - track);
            }
        }
    }
    for (i, r) in reservations.iter().enumerate() {
        if failed.contains_key(&r.edge) {
            continue;
        }
        let lower = predecessors[i]
            .iter()
            .filter(|&&j| !failed.contains_key(&reservations[j].edge))
            .fold(r.bounds.0, |v, &j| v.max(values[j] + track));
        values[i] = r.preferred.min(upper[i]).max(lower).min(r.bounds.1);
    }
    (values, failed, witnesses)
}

pub(super) fn assign(
    plan: &Plan,
    geometry: &coordination::Context<'_, '_>,
    trials: &mut BTreeMap<String, Trial>,
    spacing: Spacing,
    channels: &mut Vec<Channel>,
    supplied_bends: Option<&BTreeMap<BendKey, f64>>,
) -> Result<Summary, String> {
    // A channel per area and rank instead of a scan of all channels per section; the first of a pair wins, as in a scan.
    let mut channel_of = BTreeMap::<String, BTreeMap<usize, usize>>::new();
    for (i, c) in channels.iter().enumerate() {
        channel_of
            .entry(c.area.clone())
            .or_default()
            .entry(c.rank)
            .or_insert(i);
    }
    let mut bends = BTreeMap::new();
    for edge in &plan.edges {
        if !trials.get(&edge.edge).is_some_and(|t| t.failure.is_none()) {
            continue;
        }
        for s in &edge.sections {
            if let Some(supplied) = supplied_bends {
                let key = (edge.edge.clone(), s.area.clone(), s.channel);
                let at = supplied
                    .get(&key)
                    .ok_or_else(|| format!("Missing joint bend for {key:?}"))?;
                bends.insert(key, *at);
                continue;
            }
            let existing = channel_of
                .get(&s.area)
                .and_then(|ranks| ranks.get(&s.channel))
                .map(|&i| &channels[i]);
            let coordinate = existing.and_then(|c| {
                c.assignments
                    .iter()
                    .find(|(id, _, _)| id == &edge.edge)
                    .map(|a| a.2)
                    .or_else(|| c.assignments.first().map(|a| a.2))
            });
            let at = match coordinate {
                Some(at) => at,
                None => {
                    let (far, near) = geometry
                        .main_span(&s.area, s.channel)
                        .map_err(|f| format!("Longitudinal bend geometry: {f:?}"))?;
                    far / 2. + near / 2.
                }
            };
            bends.insert((edge.edge.clone(), s.area.clone(), s.channel), at);
        }
    }
    let mut reservations = Vec::new();
    for edge in &plan.edges {
        let Some(trial) = trials.get_mut(&edge.edge) else {
            continue;
        };
        if trial.failure.is_some() {
            continue;
        }
        let tracks: Vec<_> = edge
            .sections
            .iter()
            .map(|s| bends[&(edge.edge.clone(), s.area.clone(), s.channel)])
            .collect();
        let start = reservations.len();
        for i in 0..trial.lanes.len() {
            let main = (
                if i == 0 {
                    trial.prepared.from.0
                } else {
                    tracks[i - 1]
                },
                if i == tracks.len() {
                    trial.prepared.to.0
                } else {
                    tracks[i]
                },
            );
            let (area, bounds) = geometry
                .domain(edge, trial, i)
                .map_err(|f| format!("Longitudinal reservations: {f:?}"))?;
            if ![main.0, main.1, bounds.0, bounds.1]
                .iter()
                .all(|v| v.is_finite())
            {
                return Err(format!(
                    "Nonfinite longitudinal reservation for {}",
                    edge.edge
                ));
            }
            if main.0 > main.1 + EPS {
                trial.failure = Some(Failure::NonMonotone);
                break;
            }
            reservations.push(Reservation {
                edge: edge.edge.clone(),
                junction: i,
                area,
                main,
                bounds,
                preferred: trial.lanes[i],
                vertical: trial.prepared.vertical,
            });
        }
        if trial.failure.is_some() {
            reservations.truncate(start);
        }
    }
    // This total order makes the difference-constraint graph acyclic. Equal
    // nominal lanes use logical edge identity, never scheduling or map insertion.
    reservations.sort_by(|a, b| {
        a.preferred
            .total_cmp(&b.preferred)
            .then(a.edge.cmp(&b.edge))
            .then(a.junction.cmp(&b.junction))
    });
    let mut summary = Summary {
        reservations: reservations.len(),
        ..Summary::default()
    };
    let mut predecessors = vec![Vec::new(); reservations.len()];
    let mut sweep: Vec<_> = (0..reservations.len()).collect();
    sweep.sort_by(|&a, &b| {
        reservations[a]
            .main
            .0
            .total_cmp(&reservations[b].main.0)
            .then(a.cmp(&b))
    });
    let mut active: Vec<usize> = Vec::new();
    for i in sweep {
        let a = &reservations[i];
        active.retain(|&j| reservations[j].main.1 > a.main.0 + EPS);
        if a.main.1 - a.main.0 <= EPS {
            continue;
        }
        for &j in &active {
            let b = &reservations[j];
            if a.edge == b.edge || a.vertical != b.vertical {
                continue;
            }
            summary.overlapping_pairs += 1;
            if a.bounds.0 >= b.bounds.1 + spacing.track - EPS
                || b.bounds.0 >= a.bounds.1 + spacing.track - EPS
            {
                continue;
            }
            let (from, to) = if i < j { (i, j) } else { (j, i) };
            predecessors[to].push(from);
            summary.constraints += 1;
            summary.cross_area_constraints += usize::from(a.area != b.area);
        }
        active.push(i);
    }
    let (values, failed, witnesses) = solve(&reservations, &predecessors, spacing.track);
    if failed.values().any(
        |f| matches!(f, Failure::LongitudinalCapacity { required, .. } if !required.is_finite()),
    ) {
        return Err("Nonfinite longitudinal capacity requirement".into());
    }
    summary.rejected_edges = failed.len();
    if witnesses.iter().any(|w| {
        ![w.lower, w.upper, w.required, w.deficit]
            .iter()
            .all(|v| v.is_finite())
    }) {
        return Err("Nonfinite longitudinal capacity witness".into());
    }
    summary.capacity_witnesses = witnesses;
    for (i, r) in reservations.iter().enumerate() {
        summary.lanes.push(LaneReservation {
            edge: r.edge.clone(),
            junction: r.junction,
            area: r.area.clone(),
            main: r.main,
            bounds: r.bounds,
            preferred: r.preferred,
            coordinate: (!failed.contains_key(&r.edge)).then_some(values[i]),
        });
        summary
            .separation_constraints
            .extend(predecessors[i].iter().map(|&j| (j, i)));
        if !failed.contains_key(&r.edge) {
            summary.shifted += usize::from((values[i] - r.preferred).abs() > EPS);
            trials.get_mut(&r.edge).unwrap().lanes[r.junction] = values[i];
        }
    }
    for (edge, failure) in failed {
        trials.get_mut(&edge).unwrap().failure = Some(failure);
    }
    // Keep every existing bend coordinate fixed. Newly non-straight sections use
    // a reserved channel track, or its centre if that channel was entirely straight.
    for edge in &plan.edges {
        let Some(trial) = trials.get_mut(&edge.edge) else {
            continue;
        };
        if trial.failure.is_some() {
            continue;
        }
        for (i, s) in edge.sections.iter().enumerate() {
            if (trial.lanes[i] - trial.lanes[i + 1]).abs() <= EPS {
                continue;
            }
            let at = bends[&(edge.edge.clone(), s.area.clone(), s.channel)];
            let index = if let Some(&index) = channel_of
                .get(&s.area)
                .and_then(|ranks| ranks.get(&s.channel))
            {
                index
            } else {
                let (far, near) = geometry
                    .main_span(&s.area, s.channel)
                    .map_err(|f| format!("Longitudinal channel geometry: {f:?}"))?;
                let required = 2. * spacing.margin;
                let available = near - far;
                let missing = (required - available).max(0.);
                if ![required, available, missing].iter().all(|v| v.is_finite()) {
                    return Err(format!(
                        "Nonfinite longitudinal channel space in {}:{}",
                        s.area, s.channel
                    ));
                }
                channels.push(Channel {
                    area: s.area.clone(),
                    rank: s.channel,
                    available,
                    required,
                    missing,
                    tracks: 1,
                    assignments: Vec::new(),
                });
                channel_of
                    .entry(s.area.clone())
                    .or_default()
                    .insert(s.channel, channels.len() - 1);
                channels.len() - 1
            };
            let c = &mut channels[index];
            if c.missing > EPS {
                trial.failure = Some(Failure::NarrowChannel {
                    area: s.area.clone(),
                    rank: s.channel,
                });
            }
            if !c.assignments.iter().any(|a| a.0 == edge.edge) {
                let slot = c.assignments.first().map_or(0, |a| a.1);
                c.assignments.push((edge.edge.clone(), slot, at));
            }
        }
    }
    channels.sort_by(|a, b| a.area.cmp(&b.area).then(a.rank.cmp(&b.rank)));
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reservation(edge: &str, preferred: f64, bounds: (f64, f64)) -> Reservation {
        Reservation {
            edge: edge.into(),
            junction: 1,
            area: "<root>".into(),
            main: (0., 100.),
            bounds,
            preferred,
            vertical: false,
        }
    }

    #[test]
    fn propagates_fixed_port_bounds_backwards_without_search_or_compression() {
        let reservations = [
            reservation("a", 9., (0., 10.)),
            reservation("b", 10., (10., 10.)),
        ];
        let (values, failures, witnesses) = solve(&reservations, &[vec![], vec![0]], 6.);
        assert!(failures.is_empty());
        assert!(witnesses.is_empty());
        assert_eq!(values, [4., 10.]);
        let impossible = [
            reservation("a", 5., (5., 5.)),
            reservation("b", 10., (10., 10.)),
        ];
        let (_, failures, witnesses) = solve(&impossible, &[vec![], vec![0]], 6.);
        assert_eq!(witnesses[0].chain, [0, 1]);
        assert_eq!(witnesses[0].deficit, 1.);
        assert!(matches!(
            failures["b"],
            Failure::LongitudinalCapacity { .. }
        ));
    }

    #[test]
    fn traces_a_transitive_deficit_even_when_every_adjacent_pair_fits() {
        let reservations = [
            reservation("a", 0., (0., 0.)),
            reservation("b", 3., (0., 6.)),
            reservation("c", 6., (6., 6.)),
        ];
        let (_, failed, witnesses) = solve(&reservations, &[vec![], vec![0], vec![1]], 6.);
        assert_eq!(failed.len(), 1);
        assert_eq!(
            witnesses,
            [CapacityWitness {
                edge: "c".into(),
                junction: 1,
                chain: vec![0, 1, 2],
                lower: 0.,
                upper: 6.,
                required: 12.,
                deficit: 6.,
            }]
        );
        for pair in reservations.windows(2) {
            assert!(pair[1].bounds.1 - pair[0].bounds.0 >= 6.);
        }
    }

    #[test]
    fn generated_ordered_systems_keep_all_feasible_edges_and_every_retained_constraint() {
        let mut state = 17_u64;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (state >> 32) as usize
        };
        for repeated_edges in [false, true] {
            for _ in 0..256 {
                let mut reservations = Vec::new();
                let mut predecessors = Vec::new();
                let mut minimal = Vec::<f64>::new();
                let mut feasible = true;
                for i in 0..12 {
                    let lo = (next() % 30) as f64;
                    let hi = lo + (next() % 20) as f64;
                    let previous: Vec<_> = (0..i).filter(|_| next() % 4 == 0).collect();
                    let need = previous.iter().fold(lo, |v, &j| v.max(minimal[j] + 2.));
                    feasible &= need <= hi;
                    minimal.push(need);
                    reservations.push(reservation(
                        &format!("e{}", if repeated_edges { i % 4 } else { i }),
                        lo + (hi - lo) / 2.,
                        (lo, hi),
                    ));
                    predecessors.push(previous);
                }
                let (values, failed, witnesses) = solve(&reservations, &predecessors, 2.);
                assert_eq!(witnesses.len(), failed.len());
                for w in witnesses {
                    assert!(w.deficit > EPS);
                    assert!((w.lower + w.required - w.upper - w.deficit).abs() < EPS);
                    for pair in w.chain.windows(2) {
                        assert!(predecessors[pair[1]].contains(&pair[0]));
                    }
                }
                if feasible {
                    assert!(failed.is_empty());
                } else if !repeated_edges {
                    assert!(!failed.is_empty());
                }
                for (i, r) in reservations.iter().enumerate() {
                    if failed.contains_key(&r.edge) {
                        continue;
                    }
                    assert!(values[i] >= r.bounds.0 - EPS && values[i] <= r.bounds.1 + EPS);
                    for &j in &predecessors[i] {
                        if !failed.contains_key(&reservations[j].edge) {
                            assert!(values[i] - values[j] >= 2. - EPS);
                        }
                    }
                }
            }
        }
    }
}
