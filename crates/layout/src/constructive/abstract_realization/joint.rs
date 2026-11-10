//! One selected disjunctive order: separate longitudinal lanes, or order their bends.
//! This is bounded compaction of an existing drawing, not path or offset search.
use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub longitudinal_pairs: usize,
    pub temporal_constraints: usize,
    /// First longitudinal interval must end before the second starts.
    pub temporal_orders: Vec<(String, usize, String, usize)>,
    pub unseparable_pairs: usize,
    pub cross_flow_constraints: usize,
    /// Edges whose orders closed a cycle. A first selection that closes one is repeated with a cycle check
    /// (`reserve_once`), which keeps the cycle to the one edge whose order it cannot place.
    pub blocked_edges: usize,
    pub capacity_edges: usize,
    pub shifted: usize,
    pub bends: Vec<BendReservation>,
    /// Absolute flow-coordinate difference constraints, including zero-distance ordering.
    pub precedences: Vec<(usize, usize, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BendReservation {
    pub edge: String,
    pub section: usize,
    pub area: String,
    pub rank: usize,
    pub bounds: (f64, f64),
    pub preferred: f64,
    pub coordinate: Option<f64>,
}

struct Bend {
    reservation: BendReservation,
    origin: f64,
    cross: (f64, f64),
    /// Room the channel needs beyond the bend's upper bound for a port lead there (`lead_segments`): the demand adds it.
    high_reserve: f64,
}

/// The segments across the flow of the port leads (option `portSides`, `Prepared::lead`), by edge: (edge, vertical flow,
/// place along the flow, range across it). A lead around its node stands in the channel beside it; the bends there keep a
/// track from it.
fn lead_segments(trials: &BTreeMap<String, Trial>) -> Vec<(&str, bool, f64, (f64, f64))> {
    let mut out = Vec::new();
    for (edge, trial) in trials {
        let p = &trial.prepared;
        if p.lead.0.is_empty() && p.lead.1.is_empty() {
            continue;
        }
        let first: Vec<&Point> = p.lead.0.iter().chain(std::iter::once(&p.start)).collect();
        let last: Vec<&Point> = std::iter::once(&p.end).chain(p.lead.1.iter()).collect();
        for part in [first, last] {
            for w in part.windows(2) {
                let (a, b) = (frame(p.vertical, w[0]), frame(p.vertical, w[1]));
                if (a.0 - b.0).abs() < EPS && (a.1 - b.1).abs() > EPS {
                    out.push((edge.as_str(), p.vertical, a.0, (a.1.min(b.1), a.1.max(b.1))));
                }
            }
        }
    }
    out
}

#[derive(Clone, Copy)]
enum End {
    Fixed(f64),
    Bend(usize),
}

struct Longitudinal {
    edge: String,
    junction: usize,
    vertical: bool,
    cross: (f64, f64),
    preferred: f64,
    main: (f64, f64),
    start: End,
    end: End,
}

fn end_bound(end: End, bends: &[Bend], upper: bool) -> f64 {
    match end {
        End::Fixed(at) => at,
        End::Bend(i) => {
            if upper {
                bends[i].reservation.bounds.1
            } else {
                bends[i].reservation.bounds.0
            }
        }
    }
}

fn precedence(
    from: End,
    to: End,
    bends: &mut [Bend],
    constraints: &mut BTreeMap<(usize, usize), f64>,
) {
    match (from, to) {
        (End::Bend(a), End::Bend(b)) => {
            constraints.entry((a, b)).or_insert(0.);
        }
        (End::Bend(a), End::Fixed(at)) => {
            bends[a].reservation.bounds.1 = bends[a].reservation.bounds.1.min(at);
        }
        (End::Fixed(at), End::Bend(b)) => {
            bends[b].reservation.bounds.0 = bends[b].reservation.bounds.0.max(at);
        }
        (End::Fixed(_), End::Fixed(_)) => {}
    }
}

fn topological(bends: &[Bend], constraints: &BTreeMap<(usize, usize), f64>) -> Vec<usize> {
    let mut incoming = vec![0; bends.len()];
    let mut outgoing = vec![Vec::new(); bends.len()];
    for &(a, b) in constraints.keys() {
        incoming[b] += 1;
        outgoing[a].push(b);
    }
    let mut preference: Vec<_> = (0..bends.len()).collect();
    preference.sort_by(|&a, &b| {
        bends[a]
            .reservation
            .preferred
            .total_cmp(&bends[b].reservation.preferred)
            .then(a.cmp(&b))
    });
    let mut priority = vec![0; bends.len()];
    for (i, &v) in preference.iter().enumerate() {
        priority[v] = i;
    }
    let mut ready: BTreeSet<_> = (0..bends.len())
        .filter(|&i| incoming[i] == 0)
        .map(|i| (priority[i], i))
        .collect();
    let mut order = Vec::new();
    while let Some((_, i)) = ready.pop_first() {
        order.push(i);
        for &j in &outgoing[i] {
            incoming[j] -= 1;
            if incoming[j] == 0 {
                ready.insert((priority[j], j));
            }
        }
    }
    order
}

fn snap(value: f64, origin: f64, track: f64, up: bool) -> f64 {
    let units = (value - origin) / track;
    origin
        + if up {
            (units - EPS / track).ceil()
        } else {
            (units + EPS / track).floor()
        } * track
}

fn compact(
    bends: &[Bend],
    constraints: &BTreeMap<(usize, usize), f64>,
    order: &[usize],
    failed: &mut BTreeMap<String, Failure>,
    track: f64,
) -> Result<Vec<f64>, String> {
    let mut previous = vec![Vec::new(); bends.len()];
    for (&(a, b), &distance) in constraints {
        if !failed.contains_key(&bends[a].reservation.edge)
            && !failed.contains_key(&bends[b].reservation.edge)
        {
            previous[b].push((a, distance));
        }
    }
    let mut values = vec![0.; bends.len()];
    for &i in order {
        let b = &bends[i];
        if failed.contains_key(&b.reservation.edge) {
            continue;
        }
        let need = previous[i]
            .iter()
            .filter(|&&(j, _)| !failed.contains_key(&bends[j].reservation.edge))
            .fold(b.reservation.bounds.0, |v, &(j, distance)| {
                v.max(values[j] + distance)
            });
        let at = snap(need, b.origin, track, true);
        if !at.is_finite() {
            return Err("Nonfinite joint bend compaction".into());
        }
        if at > b.reservation.bounds.1 + EPS {
            failed.insert(
                b.reservation.edge.clone(),
                Failure::JointCapacity {
                    section: b.reservation.section,
                    required: at - b.reservation.bounds.0,
                    available: b.reservation.bounds.1 - b.reservation.bounds.0,
                },
            );
        } else {
            values[i] = at;
        }
    }
    if failed.values().any(|f| {
        matches!(f, Failure::JointCapacity { required, available, .. }
        if !required.is_finite() || !available.is_finite())
    }) {
        return Err("Nonfinite joint capacity requirement".into());
    }
    let mut upper: Vec<_> = bends
        .iter()
        .map(|b| snap(b.reservation.bounds.1, b.origin, track, false))
        .collect();
    for &i in order.iter().rev() {
        if failed.contains_key(&bends[i].reservation.edge) {
            continue;
        }
        for &(j, distance) in &previous[i] {
            if !failed.contains_key(&bends[j].reservation.edge) {
                upper[j] = upper[j].min(snap(upper[i] - distance, bends[j].origin, track, false));
            }
        }
    }
    for &i in order {
        let b = &bends[i];
        if failed.contains_key(&b.reservation.edge) {
            continue;
        }
        let lower = previous[i]
            .iter()
            .filter(|&&(j, _)| !failed.contains_key(&bends[j].reservation.edge))
            .fold(b.reservation.bounds.0, |v, &(j, distance)| {
                v.max(values[j] + distance)
            });
        let lower = snap(lower, b.origin, track, true);
        values[i] = snap(b.reservation.preferred, b.origin, track, true)
            .min(upper[i])
            .max(lower);
        if !values[i].is_finite()
            || values[i] < b.reservation.bounds.0 - EPS
            || values[i] > b.reservation.bounds.1 + EPS
        {
            return Err("Invalid joint bend compaction bounds".into());
        }
    }
    Ok(values)
}

struct Reserved {
    bends: Vec<Bend>,
    constraints: BTreeMap<(usize, usize), f64>,
    order: Vec<usize>,
    failed: BTreeMap<String, Failure>,
    summary: Summary,
}

/// The bends of every unfailed edge with their precedences, in one topological order. When the selected orders close a
/// cycle (which would block every edge on it and downstream of it), the orders are selected again with a cycle check
/// (`reserve_once` with `careful`).
fn reserve(
    plan: &Plan,
    geometry: &coordination::Context<'_, '_>,
    trials: &BTreeMap<String, Trial>,
    spacing: Spacing,
    channels: &[Channel],
    additional_orders: &[(String, usize, String, usize)],
    narrow: bool,
) -> Result<Reserved, String> {
    let first = reserve_once(
        plan,
        geometry,
        trials,
        spacing,
        channels,
        additional_orders,
        narrow,
        false,
    )?;
    if first.summary.blocked_edges == 0 {
        return Ok(first);
    }
    reserve_once(
        plan,
        geometry,
        trials,
        spacing,
        channels,
        additional_orders,
        narrow,
        true,
    )
}

fn outgoing_of(n: usize, constraints: &BTreeMap<(usize, usize), f64>) -> Vec<Vec<usize>> {
    let mut outgoing = vec![Vec::new(); n];
    for &(a, b) in constraints.keys() {
        outgoing[a].push(b);
    }
    outgoing
}

/// Whether `to` reaches `from` along the constraints: an order `from` before `to` would close a cycle.
fn closes_cycle(from: End, to: End, outgoing: &[Vec<usize>]) -> bool {
    let (End::Bend(from), End::Bend(to)) = (from, to) else {
        return false;
    };
    let mut seen = vec![false; outgoing.len()];
    let mut stack = vec![to];
    while let Some(i) = stack.pop() {
        if i == from {
            return true;
        }
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        stack.extend(outgoing[i].iter().copied());
    }
    false
}

/// The given orders (chain cuts) as precedences. With `careful`, an order that would close a cycle is left out and the edge
/// of its second interval fails with `JointOrder`.
fn given_orders(
    additional_orders: &[(String, usize, String, usize)],
    longitudinal: &[Longitudinal],
    bends: &mut [Bend],
    constraints: &mut BTreeMap<(usize, usize), f64>,
    cut: &mut BTreeMap<String, Failure>,
    summary: &mut Summary,
    careful: bool,
) -> Result<(), String> {
    for (first_edge, first_junction, second_edge, second_junction) in additional_orders {
        let first = longitudinal
            .iter()
            .find(|r| r.edge == *first_edge && r.junction == *first_junction)
            .ok_or_else(|| format!("Missing chain-cut interval {first_edge}:{first_junction}"))?;
        let second = longitudinal
            .iter()
            .find(|r| r.edge == *second_edge && r.junction == *second_junction)
            .ok_or_else(|| format!("Missing chain-cut interval {second_edge}:{second_junction}"))?;
        if first.edge == second.edge || first.vertical != second.vertical {
            return Err("Invalid chain-cut interval pair".into());
        }
        if careful
            && closes_cycle(
                first.end,
                second.start,
                &outgoing_of(bends.len(), constraints),
            )
        {
            if let End::Bend(i) = second.start {
                cut.entry(bends[i].reservation.edge.clone())
                    .or_insert(Failure::JointOrder {
                        section: bends[i].reservation.section,
                    });
            }
            continue;
        }
        precedence(first.end, second.start, bends, constraints);
        summary.temporal_constraints += 1;
        summary.temporal_orders.push((
            first.edge.clone(),
            first.junction,
            second.edge.clone(),
            second.junction,
        ));
    }
    Ok(())
}

/// One selection of the orders. With `careful`, an order that would close a cycle is replaced by the opposite order when
/// the bounds allow both, and otherwise left out; the edge of its second interval then fails with `JointOrder`, alone,
/// instead of every edge on the cycle and downstream of it.
#[allow(clippy::too_many_arguments)]
fn reserve_once(
    plan: &Plan,
    geometry: &coordination::Context<'_, '_>,
    trials: &BTreeMap<String, Trial>,
    spacing: Spacing,
    channels: &[Channel],
    additional_orders: &[(String, usize, String, usize)],
    narrow: bool,
    careful: bool,
) -> Result<Reserved, String> {
    let mut cut: BTreeMap<String, Failure> = BTreeMap::new();
    let mut bends = Vec::new();
    let mut longitudinal = Vec::new();
    let mut constraints = BTreeMap::<(usize, usize), f64>::new();
    let mut summary = Summary::default();
    // A channel per (area, rank) instead of a scan of all channels per section; the first of a pair wins, as in a scan.
    let mut channel_of = BTreeMap::<(&str, usize), &Channel>::new();
    for c in channels {
        channel_of.entry((c.area.as_str(), c.rank)).or_insert(c);
    }
    let leads = lead_segments(trials);
    for edge in &plan.edges {
        // Growth demand (`narrow`) includes the edges whose channel is too narrow before it grows.
        let Some(trial) = trials.get(&edge.edge).filter(|t| match &t.failure {
            None => true,
            Some(Failure::NarrowChannel { .. }) => narrow,
            Some(_) => false,
        }) else {
            continue;
        };
        let first = bends.len();
        for (i, s) in edge.sections.iter().enumerate() {
            let (far, near) = geometry
                .main_span(&s.area, s.channel)
                .map_err(|f| format!("Joint channel geometry: {f:?}"))?;
            let origin = far + spacing.margin;
            let mut bounds = (
                origin.max(trial.prepared.from.0),
                (near - spacing.margin).min(trial.prepared.to.0),
            );
            // A port lead of another edge across this bend's range in the channel: the bend keeps a track from it, on the
            // channel's inner side (the lead stands a margin from the node it belongs to).
            let cross = (
                trial.lanes[i].min(trial.lanes[i + 1]),
                trial.lanes[i].max(trial.lanes[i + 1]),
            );
            let mut high_reserve: f64 = 0.;
            for &(owner, vertical, at, range) in &leads {
                if owner == edge.edge
                    || vertical != trial.prepared.vertical
                    || at < far - EPS
                    || at > near + EPS
                    || range.0 >= cross.1 + spacing.track - EPS
                    || cross.0 >= range.1 + spacing.track - EPS
                {
                    continue;
                }
                if at - far <= near - at {
                    bounds.0 = bounds.0.max(at + spacing.track);
                } else {
                    bounds.1 = bounds.1.min(at - spacing.track);
                    high_reserve = spacing.track;
                }
            }
            let preferred = channel_of
                .get(&(s.area.as_str(), s.channel))
                .and_then(|c| c.assignments.iter().find(|a| a.0 == edge.edge))
                .map_or(far / 2. + near / 2., |a| a.2);
            if ![origin, bounds.0, bounds.1, preferred]
                .iter()
                .all(|v| v.is_finite())
            {
                return Err(format!("Nonfinite joint bend for {}", edge.edge));
            }
            bends.push(Bend {
                reservation: BendReservation {
                    edge: edge.edge.clone(),
                    section: i,
                    area: s.area.clone(),
                    rank: s.channel,
                    bounds,
                    preferred,
                    coordinate: None,
                },
                origin,
                cross,
                high_reserve,
            });
            if i > 0 {
                constraints.insert((first + i - 1, first + i), 0.);
            }
        }
        for i in 0..trial.lanes.len() {
            let (_, cross) = geometry
                .domain(edge, trial, i)
                .map_err(|f| format!("Joint lane domain: {f:?}"))?;
            longitudinal.push(Longitudinal {
                edge: edge.edge.clone(),
                junction: i,
                vertical: trial.prepared.vertical,
                cross,
                preferred: trial.lanes[i],
                main: (
                    if i == 0 {
                        trial.prepared.from.0
                    } else {
                        bends[first + i - 1].reservation.preferred
                    },
                    if i == edge.sections.len() {
                        trial.prepared.to.0
                    } else {
                        bends[first + i].reservation.preferred
                    },
                ),
                start: if i == 0 {
                    End::Fixed(trial.prepared.from.0)
                } else {
                    End::Bend(first + i - 1)
                },
                end: if i == edge.sections.len() {
                    End::Fixed(trial.prepared.to.0)
                } else {
                    End::Bend(first + i)
                },
            });
        }
    }
    longitudinal.sort_by(|a, b| a.main.0.total_cmp(&b.main.0).then(a.edge.cmp(&b.edge)));
    // With `careful`, the given orders (chain cuts) come first, so that a selected order gives way to them, not the reverse.
    if careful {
        given_orders(
            additional_orders,
            &longitudinal,
            &mut bends,
            &mut constraints,
            &mut cut,
            &mut summary,
            careful,
        )?;
    }
    let mut active: Vec<usize> = Vec::new();
    for (i, a) in longitudinal.iter().enumerate() {
        active.retain(|&j| longitudinal[j].main.1 > a.main.0 + EPS);
        if a.main.1 - a.main.0 <= EPS {
            continue;
        }
        for &j in &active {
            let b = &longitudinal[j];
            if a.vertical != b.vertical {
                continue;
            }
            let (lower, upper) =
                if a.preferred < b.preferred || (a.preferred == b.preferred && a.edge < b.edge) {
                    (a, b)
                } else {
                    (b, a)
                };
            // The string comparison comes last: most pairs of this scan end at the numbers.
            if upper.cross.1 - lower.cross.0 >= spacing.track - EPS || a.edge == b.edge {
                continue;
            }
            summary.longitudinal_pairs += 1;
            // If lane domains cannot realize the preferred separation, select
            // non-overlap along flow instead. Prefer the earlier nominal interval.
            let ab = end_bound(a.end, &bends, false) <= end_bound(b.start, &bends, true) + EPS;
            let ba = end_bound(b.end, &bends, false) <= end_bound(a.start, &bends, true) + EPS;
            let mut selected = match (ab, ba) {
                (true, false) => Some((a, b)),
                (false, true) => Some((b, a)),
                (true, true) => {
                    if a.main.0 / 2. + a.main.1 / 2. < b.main.0 / 2. + b.main.1 / 2. {
                        Some((a, b))
                    } else {
                        Some((b, a))
                    }
                }
                (false, false) => None,
            };
            if careful {
                if let Some((first, second)) = selected {
                    let outgoing = outgoing_of(bends.len(), &constraints);
                    if closes_cycle(first.end, second.start, &outgoing) {
                        selected = if ab && ba && !closes_cycle(second.end, first.start, &outgoing)
                        {
                            Some((second, first))
                        } else {
                            if let End::Bend(i) = second.start {
                                cut.entry(bends[i].reservation.edge.clone()).or_insert(
                                    Failure::JointOrder {
                                        section: bends[i].reservation.section,
                                    },
                                );
                            } else if let End::Bend(i) = first.end {
                                cut.entry(bends[i].reservation.edge.clone()).or_insert(
                                    Failure::JointOrder {
                                        section: bends[i].reservation.section,
                                    },
                                );
                            }
                            None
                        };
                    }
                }
            }
            if let Some((first, second)) = selected {
                precedence(first.end, second.start, &mut bends, &mut constraints);
                summary.temporal_constraints += 1;
                summary.temporal_orders.push((
                    first.edge.clone(),
                    first.junction,
                    second.edge.clone(),
                    second.junction,
                ));
            } else {
                summary.unseparable_pairs += 1;
            }
        }
        active.push(i);
    }
    if !careful {
        given_orders(
            additional_orders,
            &longitudinal,
            &mut bends,
            &mut constraints,
            &mut cut,
            &mut summary,
            careful,
        )?;
    }
    let order = topological(&bends, &constraints);
    let mut failed = cut;
    let visited: BTreeSet<_> = order.iter().copied().collect();
    for (i, b) in bends.iter().enumerate() {
        if !visited.contains(&i) {
            failed
                .entry(b.reservation.edge.clone())
                .or_insert(Failure::JointOrder {
                    section: b.reservation.section,
                });
        }
    }
    summary.blocked_edges = failed.len();
    let mut by_channel: BTreeMap<(&str, usize), Vec<usize>> = BTreeMap::new();
    for &i in &order {
        let b = &bends[i];
        if !failed.contains_key(&b.reservation.edge) {
            by_channel
                .entry((&b.reservation.area, b.reservation.rank))
                .or_default()
                .push(i);
        }
    }
    for members in by_channel.values() {
        for (k, &a) in members.iter().enumerate() {
            for &b in &members[k + 1..] {
                if bends[a].cross.0 < bends[b].cross.1 + spacing.track - EPS
                    && bends[b].cross.0 < bends[a].cross.1 + spacing.track - EPS
                {
                    constraints
                        .entry((a, b))
                        .and_modify(|v| *v = v.max(spacing.track))
                        .or_insert(spacing.track);
                    summary.cross_flow_constraints += 1;
                }
            }
        }
    }
    Ok(Reserved {
        bends,
        constraints,
        order,
        failed,
        summary,
    })
}

/// The channel widths the joint order needs: its compaction from the lower bounds, without the channel ends
/// as upper bounds. Channel growth reserves them, so the realization does not fail on capacity.
pub(super) fn demand(
    plan: &Plan,
    geometry: &coordination::Context<'_, '_>,
    trials: &BTreeMap<String, Trial>,
    spacing: Spacing,
    channels: &[Channel],
) -> Result<BTreeMap<(String, usize), f64>, String> {
    let Reserved {
        bends,
        constraints,
        order,
        failed,
        ..
    } = reserve(plan, geometry, trials, spacing, channels, &[], true)?;
    let mut previous = vec![Vec::new(); bends.len()];
    for (&(a, b), &distance) in &constraints {
        previous[b].push((a, distance));
    }
    let mut values = vec![0.; bends.len()];
    let mut required = BTreeMap::new();
    for &i in &order {
        let b = &bends[i];
        if failed.contains_key(&b.reservation.edge) {
            continue;
        }
        let need = previous[i]
            .iter()
            .filter(|&&(j, _)| !failed.contains_key(&bends[j].reservation.edge))
            .fold(b.reservation.bounds.0, |v, &(j, distance)| {
                v.max(values[j] + distance)
            });
        values[i] = snap(need, b.origin, spacing.track, true);
        if !values[i].is_finite() {
            return Err("Nonfinite joint demand".into());
        }
        let width = values[i] - b.origin + 2. * spacing.margin + b.high_reserve;
        let entry = required
            .entry((b.reservation.area.clone(), b.reservation.rank))
            .or_insert(0f64);
        *entry = entry.max(width);
    }
    Ok(required)
}

pub(super) fn assign(
    plan: &Plan,
    geometry: &coordination::Context<'_, '_>,
    trials: &mut BTreeMap<String, Trial>,
    spacing: Spacing,
    channels: &mut Vec<Channel>,
    additional_orders: &[(String, usize, String, usize)],
) -> Result<(Summary, BTreeMap<BendKey, f64>), String> {
    let Reserved {
        mut bends,
        constraints,
        order,
        mut failed,
        mut summary,
    } = reserve(
        plan,
        geometry,
        trials,
        spacing,
        channels,
        additional_orders,
        false,
    )?;
    let values = compact(&bends, &constraints, &order, &mut failed, spacing.track)?;
    summary.capacity_edges = failed.len() - summary.blocked_edges;
    let mut result = BTreeMap::new();
    channels.clear();
    let mut channel_of = BTreeMap::<String, BTreeMap<usize, usize>>::new();
    for (i, b) in bends.iter_mut().enumerate() {
        if !failed.contains_key(&b.reservation.edge) {
            let at = values[i];
            b.reservation.coordinate = Some(at);
            summary.shifted += usize::from((at - b.reservation.preferred).abs() > EPS);
            result.insert(
                (
                    b.reservation.edge.clone(),
                    b.reservation.area.clone(),
                    b.reservation.rank,
                ),
                at,
            );
            let index = channel_of
                .get(&b.reservation.area)
                .and_then(|ranks| ranks.get(&b.reservation.rank))
                .copied();
            let index = match index {
                Some(i) => i,
                None => {
                    let (far, near) = geometry
                        .main_span(&b.reservation.area, b.reservation.rank)
                        .map_err(|f| format!("Joint channel report: {f:?}"))?;
                    channels.push(Channel {
                        area: b.reservation.area.clone(),
                        rank: b.reservation.rank,
                        available: near - far,
                        required: 2. * spacing.margin,
                        missing: 0.,
                        tracks: 1,
                        assignments: Vec::new(),
                    });
                    channel_of
                        .entry(b.reservation.area.clone())
                        .or_default()
                        .insert(b.reservation.rank, channels.len() - 1);
                    channels.len() - 1
                }
            };
            let slot = ((at - b.origin) / spacing.track).round();
            if !slot.is_finite() || slot < 0. || slot >= usize::MAX as f64 {
                return Err("Invalid joint bend slot".into());
            }
            let c = &mut channels[index];
            c.tracks = c.tracks.max(slot as usize + 1);
            c.required = 2. * spacing.margin + c.tracks.saturating_sub(1) as f64 * spacing.track;
            c.missing = (c.required - c.available).max(0.);
            if ![c.required, c.available, c.missing]
                .iter()
                .all(|v| v.is_finite())
            {
                return Err("Nonfinite joint channel demand".into());
            }
            c.assignments
                .push((b.reservation.edge.clone(), slot as usize, at));
        }
        summary.bends.push(b.reservation.clone());
    }
    summary.precedences = constraints
        .into_iter()
        .map(|((a, b), d)| (a, b, d))
        .collect();
    for (edge, failure) in failed {
        trials.get_mut(&edge).unwrap().failure = Some(failure);
    }
    Ok((summary, result))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bend(edge: &str, preferred: f64) -> Bend {
        Bend {
            reservation: BendReservation {
                edge: edge.into(),
                section: 0,
                area: "<root>".into(),
                rank: 0,
                bounds: (12., 78.),
                preferred,
                coordinate: None,
            },
            origin: 12.,
            cross: (0., 40.),
            high_reserve: 0.,
        }
    }

    #[test]
    fn endpoint_precedence_overrides_preferences_and_reports_cycle_blockage() {
        let mut bends = [
            bend("a", 60.),
            bend("b", 30.),
            bend("downstream", 50.),
            bend("independent", 40.),
        ];
        let mut constraints = BTreeMap::new();
        precedence(End::Bend(0), End::Bend(1), &mut bends, &mut constraints);
        assert_eq!(topological(&bends, &constraints), [3, 2, 0, 1]);
        precedence(End::Bend(1), End::Bend(0), &mut bends, &mut constraints);
        precedence(End::Bend(1), End::Bend(2), &mut bends, &mut constraints);
        assert_eq!(topological(&bends, &constraints), [3]);
        precedence(End::Fixed(24.), End::Bend(3), &mut bends, &mut constraints);
        precedence(End::Bend(3), End::Fixed(66.), &mut bends, &mut constraints);
        assert_eq!(bends[3].reservation.bounds, (24., 66.));
    }

    #[test]
    fn an_order_that_closes_a_cycle_is_recognized_before_it_is_added() {
        let mut bends = [bend("a", 10.), bend("b", 20.), bend("c", 30.)];
        let mut constraints = BTreeMap::new();
        precedence(End::Bend(0), End::Bend(1), &mut bends, &mut constraints);
        precedence(End::Bend(1), End::Bend(2), &mut bends, &mut constraints);
        let outgoing = outgoing_of(bends.len(), &constraints);
        // c before a would close a -> b -> c -> a; a before c follows the path.
        assert!(closes_cycle(End::Bend(2), End::Bend(0), &outgoing));
        assert!(!closes_cycle(End::Bend(0), End::Bend(2), &outgoing));
        // A fixed end adds no constraint between bends, so it never closes one.
        assert!(!closes_cycle(End::Fixed(0.), End::Bend(0), &outgoing));
        assert_eq!(topological(&bends, &constraints).len(), 3);
    }

    #[test]
    fn channel_grid_keeps_exact_bounds_and_nonintegral_origins() {
        for origin in [0., 0.25, -31.5, 12000.25] {
            assert!((snap(origin + 12., origin, 6., true) - origin - 12.).abs() < EPS);
            assert!((snap(origin + 12., origin, 6., false) - origin - 12.).abs() < EPS);
            assert!((snap(origin + 12.1, origin, 6., true) - origin - 18.).abs() < EPS);
            assert!((snap(origin + 11.9, origin, 6., false) - origin - 6.).abs() < EPS);
        }
    }

    #[test]
    fn compaction_propagates_upper_bounds_and_reports_exact_grid_capacity() {
        let mut bends = [bend("a", 60.), bend("b", 30.)];
        bends[1].reservation.bounds.1 = 30.;
        let constraints = BTreeMap::from([((0, 1), 6.)]);
        let mut failed = BTreeMap::new();
        let values = compact(&bends, &constraints, &[0, 1], &mut failed, 6.).unwrap();
        assert!(failed.is_empty());
        assert_eq!(values, [24., 30.]);
        bends[1].reservation.bounds.1 = 17.;
        compact(&bends, &constraints, &[0, 1], &mut failed, 6.).unwrap();
        assert_eq!(
            failed["b"],
            Failure::JointCapacity {
                section: 0,
                required: 6.,
                available: 5.
            }
        );
        bends[0].reservation.bounds = (f64::MAX, f64::MAX);
        assert!(compact(
            &bends,
            &constraints,
            &[0, 1],
            &mut BTreeMap::new(),
            f64::MIN_POSITIVE
        )
        .is_err());
    }

    #[test]
    fn generated_grid_dags_preserve_every_retained_difference_constraint() {
        let mut state = 23_u64;
        let mut next = || {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            (state >> 32) as usize
        };
        for repeated_edges in [false, true] {
            for _ in 0..256 {
                let mut bends = Vec::new();
                let mut constraints = BTreeMap::new();
                let mut minimum = Vec::<f64>::new();
                let mut feasible = true;
                for i in 0..12 {
                    let mut b = bend(&format!("e{}", if repeated_edges { i % 4 } else { i }), 40.);
                    b.origin = (next() % 3) as f64 / 4.;
                    b.reservation.bounds = (
                        b.origin + (next() % 8) as f64 * 2.,
                        b.origin + 24. + (next() % 8) as f64 * 2.,
                    );
                    let mut need = b.reservation.bounds.0;
                    for j in 0..i {
                        if next() % 4 == 0 {
                            let distance = (next() % 2) as f64 * 2.;
                            constraints.insert((j, i), distance);
                            need = need.max(minimum[j] + distance);
                        }
                    }
                    let at = snap(need, b.origin, 2., true);
                    feasible &= at <= b.reservation.bounds.1;
                    minimum.push(at);
                    bends.push(b);
                }
                let order = topological(&bends, &constraints);
                assert_eq!(order.len(), bends.len());
                let mut failed = BTreeMap::new();
                let values = compact(&bends, &constraints, &order, &mut failed, 2.).unwrap();
                if feasible {
                    assert!(failed.is_empty());
                }
                for (i, b) in bends.iter().enumerate() {
                    if failed.contains_key(&b.reservation.edge) {
                        continue;
                    }
                    assert!(
                        values[i] >= b.reservation.bounds.0 - EPS
                            && values[i] <= b.reservation.bounds.1 + EPS
                    );
                    assert!(
                        (((values[i] - b.origin) / 2.).round() - (values[i] - b.origin) / 2.).abs()
                            < EPS
                    );
                }
                for (&(a, b), &distance) in &constraints {
                    if !failed.contains_key(&bends[a].reservation.edge)
                        && !failed.contains_key(&bends[b].reservation.edge)
                    {
                        assert!(values[b] - values[a] >= distance - EPS);
                    }
                }
            }
        }
    }
}
