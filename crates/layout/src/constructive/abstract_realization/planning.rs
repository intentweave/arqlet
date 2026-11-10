//! Provisional run-local, exact-dependency planning. Optional construction and
//! repair reuse validate their own geometry/obstacle/occupancy dependencies.
use super::*;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Work {
    pub ordinal_computations: usize,
    pub ordinal_reuses: usize,
    pub lane_computations: usize,
    pub lane_reuses: usize,
    pub channel_computations: usize,
    pub channel_reuses: usize,
    pub preferred_changes_before: usize,
    pub preferred_changes_after: usize,
}

impl Work {
    pub fn since(&self, before: &Self) -> Self {
        Self {
            ordinal_computations: self.ordinal_computations - before.ordinal_computations,
            ordinal_reuses: self.ordinal_reuses - before.ordinal_reuses,
            lane_computations: self.lane_computations - before.lane_computations,
            lane_reuses: self.lane_reuses - before.lane_reuses,
            channel_computations: self.channel_computations - before.channel_computations,
            channel_reuses: self.channel_reuses - before.channel_reuses,
            preferred_changes_before: self.preferred_changes_before
                - before.preferred_changes_before,
            preferred_changes_after: self.preferred_changes_after - before.preferred_changes_after,
        }
    }
}
type OrdinalEntry = (Vec<abstract_routing::Assignment>, abstract_routing::Channel);
type LaneEntry = (Vec<(u64, u64)>, Vec<f64>);
type TrackEntry = (Vec<(String, usize, u64, u64)>, u64, bool, Vec<usize>, usize);

/// One entry per current edge/channel, replaced on dependency changes. Scope is
/// one graph/candidate sequence; callers may discard it at any time.
#[derive(Clone, Default)]
pub struct Session {
    ordinal: BTreeMap<(String, usize), OrdinalEntry>,
    lanes: BTreeMap<String, LaneEntry>,
    tracks: BTreeMap<(String, usize), TrackEntry>,
    pub work: Work,
    repair: Option<arqlet_routing::fixed_tracks::Session>,
    pub(super) construction: Option<construction::Session>,
    pub(super) coordination: bool,
    /// Channel slots follow the physical lane order (public `abstractRouting`) instead of the ordinal plan order.
    pub(super) physical_tracks: bool,
    /// Edges with explicit ports are planned (single pass); see `abstract_routing::build_with`.
    pub(super) explicit_ports: bool,
}

impl Session {
    /// Share immutable grown-geometry projections across the chain probe and
    /// final passes. Solver constraints/results and occupancy stay fresh.
    pub fn with_coordination_reuse() -> Self {
        Self {
            coordination: true,
            ..Self::with_construction_reuse()
        }
    }

    /// Opt in to fixed-path geometry/obstacle reuse and one shared obstacle
    /// index. Track acceptance, coordination and final evaluation remain fresh.
    pub fn with_construction_reuse() -> Self {
        Self {
            construction: Some(Default::default()),
            ..Default::default()
        }
    }

    /// Opt in to exact fixed-track decision reuse. Not used by ordinary layout
    /// or by the historical planning-only diagnostic.
    pub fn with_repair_reuse() -> Self {
        Self {
            repair: Some(Default::default()),
            ..Default::default()
        }
    }

    pub(super) fn repair(
        &mut self,
        graph: &Graph,
        boxes: &BTreeMap<String, Rect>,
        moving: &[Route],
        fixed: &[Route],
        params: arqlet_routing::Params,
        stop: &dyn Fn() -> bool,
    ) -> Result<arqlet_routing::fixed_tracks::Repair, String> {
        match &mut self.repair {
            Some(session) => session.repair(graph, boxes, moving, fixed, params, stop),
            None => arqlet_routing::fixed_tracks::repair(graph, boxes, moving, fixed, params, stop),
        }
    }
}

fn pair_cost(a: &abstract_routing::Assignment, b: &abstract_routing::Assignment) -> usize {
    let between =
        |v: &abstract_routing::Lane, a: &abstract_routing::Lane, b: &abstract_routing::Lane| {
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            lo < v && v < hi
        };
    usize::from(between(&a.to, &b.from, &b.to)) + usize::from(between(&b.from, &a.from, &a.to))
}

fn quality_order(mut channel: abstract_routing::Channel) -> abstract_routing::Channel {
    // Swapping adjacent entries affects exactly their pair cost, not any other
    // pair. Two fixed sweeps, no retry loop or cubic rescoring.
    let n = channel.order.len();
    for reverse in [false, true] {
        for k in 0..n.saturating_sub(1) {
            let i = if reverse { n - 2 - k } else { k };
            let (a, b) = (channel.order[i], channel.order[i + 1]);
            if pair_cost(&channel.assignments[b], &channel.assignments[a])
                < pair_cost(&channel.assignments[a], &channel.assignments[b])
            {
                channel.order.swap(i, i + 1);
            }
        }
    }
    let mut position = vec![0; n];
    channel.tracks = 0;
    for (at, &i) in channel.order.iter().enumerate() {
        position[i] = at;
        let a = &channel.assignments[i];
        let (lo, hi) = if a.from < a.to {
            (&a.from, &a.to)
        } else {
            (&a.to, &a.from)
        };
        let slot = channel.order[..at]
            .iter()
            .filter_map(|&j| {
                let b = &channel.assignments[j];
                let (l, h) = if b.from < b.to {
                    (&b.from, &b.to)
                } else {
                    (&b.to, &b.from)
                };
                (lo <= h && l <= hi).then_some(b.slot + 1)
            })
            .max()
            .unwrap_or(0);
        channel.assignments[i].slot = slot;
        channel.tracks = channel.tracks.max(slot + 1);
    }
    channel.broken_dependencies = channel
        .dependencies
        .iter()
        .copied()
        .filter(|&(a, b)| position[a] > position[b])
        .collect();
    channel
}

impl Session {
    /// Validate topology/ranks afresh, then reuse only channels with exactly
    /// identical ordinal assignments. No geometry participates in this phase.
    pub fn plan(&mut self, graph: &Graph, plans: &LayeredPlans) -> Result<Plan, String> {
        let mut active = BTreeSet::new();
        let plan = abstract_routing::build_with(
            graph,
            plans,
            self.explicit_ports,
            |area, rank, assignments| {
                let key = (area.clone(), rank);
                active.insert(key.clone());
                if let Some((before, channel)) = self.ordinal.get(&key) {
                    if *before == assignments {
                        self.work.ordinal_reuses += 1;
                        return channel.clone();
                    }
                }
                let channel =
                    quality_order(abstract_routing::channel(area, rank, assignments.clone()));
                self.work.ordinal_computations += 1;
                self.ordinal.insert(key, (assignments, channel.clone()));
                channel
            },
        )?;
        self.ordinal.retain(|key, _| active.contains(key));
        self.tracks.retain(|key, _| active.contains(key));
        let edges: BTreeSet<_> = plan.edges.iter().map(|e| e.edge.as_str()).collect();
        self.lanes.retain(|key, _| edges.contains(key.as_str()));
        Ok(plan)
    }

    fn lanes(&mut self, edge: &str, domains: &[(f64, f64)]) -> Vec<f64> {
        let key: Vec<_> = domains
            .iter()
            .map(|(a, b)| (a.to_bits(), b.to_bits()))
            .collect();
        if let Some((before, values)) = self.lanes.get(edge) {
            if *before == key {
                self.work.lane_reuses += 1;
                return values.clone();
            }
        }
        // Partition the ordered domains into maximal runs with a common
        // intersection. One constant lane per run minimizes changes for these
        // domains alone; inter-edge constraints remain the coordinator's job.
        let mut values = vec![0.; domains.len()];
        let mut start = 0;
        while start < domains.len() {
            let (mut lo, mut hi) = domains[start];
            let mut end = start + 1;
            while end < domains.len() {
                let (l, h) = domains[end];
                if lo.max(l) > hi.min(h) {
                    break;
                }
                lo = lo.max(l);
                hi = hi.min(h);
                end += 1;
            }
            let preferred = lo / 2. + hi / 2.;
            values[start..end].fill(preferred.max(lo).min(hi));
            start = end;
        }
        self.work.lane_computations += 1;
        self.lanes.insert(edge.into(), (key, values.clone()));
        values
    }

    pub(super) fn align(
        &mut self,
        edge: &EdgePlan,
        trial: &mut Trial,
        geometry: &Geometry<'_>,
    ) -> Result<(), Failure> {
        let domains: Vec<_> = (0..trial.lanes.len())
            .map(|i| lane_domain(edge, trial, geometry, i).map(|(_, bounds)| bounds))
            .collect::<Result<_, _>>()?;
        self.work.preferred_changes_before += trial
            .lanes
            .windows(2)
            .filter(|w| (w[0] - w[1]).abs() > EPS)
            .count();
        trial.lanes = self.lanes(&edge.edge, &domains);
        self.work.preferred_changes_after += trial
            .lanes
            .windows(2)
            .filter(|w| (w[0] - w[1]).abs() > EPS)
            .count();
        Ok(())
    }

    pub(super) fn tracks(
        &mut self,
        area: &str,
        rank: usize,
        changes: &[(&str, usize, f64, f64)],
        track: f64,
    ) -> (Vec<usize>, usize) {
        let key = (area.to_string(), rank);
        let inputs: Vec<_> = changes
            .iter()
            .map(|(id, i, a, b)| (id.to_string(), *i, a.to_bits(), b.to_bits()))
            .collect();
        if let Some((before, bits, physical, slots, count)) = self.tracks.get(&key) {
            if *before == inputs && *bits == track.to_bits() && *physical == self.physical_tracks {
                self.work.channel_reuses += 1;
                return (slots.clone(), *count);
            }
        }
        // Public abstract routing orders on the physical lanes: ordinal lanes of edges that share a link (container ports,
        // bundles) only know their edge ids, so their plan order is not crossing-aware. The prototype trials keep the
        // ordinal order their repair and regional mechanisms are measured on.
        let spans: Vec<_> = changes.iter().map(|c| (c.2, c.3)).collect();
        let order = if self.physical_tracks {
            dependent_order(&spans, track)
        } else {
            (0..spans.len()).collect()
        };
        let (slots, count) = track_slots(&spans, &order, track);
        self.work.channel_computations += 1;
        self.tracks.insert(
            key,
            (
                inputs,
                track.to_bits(),
                self.physical_tracks,
                slots.clone(),
                count,
            ),
        );
        (slots, count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_runs_minimize_lane_changes_for_generated_domains() {
        let mut seed = 41_u64;
        let mut session = Session::default();
        for _ in 0..512 {
            let mut next = || {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed >> 32) as usize % 9
            };
            let domains: Vec<_> = (0..7)
                .map(|_| {
                    let (a, b) = (next(), next());
                    (a.min(b) as f64, a.max(b) as f64)
                })
                .collect();
            let result = session.lanes("edge", &domains);
            for (&value, &(lo, hi)) in result.iter().zip(&domains) {
                assert!(value >= lo && value <= hi);
            }
            let mut costs = [0_usize; 9];
            for (i, &(lo, hi)) in domains.iter().enumerate() {
                let previous = costs;
                for (value, cost) in costs.iter_mut().enumerate() {
                    *cost = if (value as f64) < lo || value as f64 > hi {
                        1000
                    } else if i == 0 {
                        0
                    } else {
                        previous
                            .iter()
                            .enumerate()
                            .map(|(before, c)| c + usize::from(before != value))
                            .min()
                            .unwrap()
                    };
                }
            }
            let changes = result.windows(2).filter(|w| w[0] != w[1]).count();
            assert_eq!(changes, *costs.iter().min().unwrap());
            assert_eq!(session.lanes("edge", &domains), result);
        }
        assert_eq!(session.work.lane_computations, 512);
        assert_eq!(session.work.lane_reuses, 512);
    }

    #[test]
    fn lane_and_channel_dependencies_invalidate_only_affected_entries() {
        let mut s = Session::default();
        let domains = [(0., 0.), (0., 10.), (10., 10.)];
        let a = s.lanes("a", &domains);
        let b = s.lanes("b", &domains);
        let before = s.work.clone();
        assert_eq!(s.lanes("b", &domains), b);
        assert_ne!(s.lanes("a", &[(0., 0.), (0., 20.), (20., 20.)]), a);
        let delta = s.work.since(&before);
        assert_eq!((delta.lane_computations, delta.lane_reuses), (1, 1));
        let changes = [("a", 0, 0., 10.), ("b", 0, 4., 14.)];
        let original = s.tracks("area", 0, &changes, 6.);
        s.tracks("other", 0, &changes, 6.);
        let before = s.work.clone();
        assert_eq!(s.tracks("area", 0, &changes, 6.), original);
        s.tracks("other", 0, &changes, 8.);
        let delta = s.work.since(&before);
        assert_eq!((delta.channel_computations, delta.channel_reuses), (1, 1));
        let before = s.work.clone();
        s.tracks("area", 0, &[("a", 0, 0., 10.), ("b", 0, 4., 15.)], 6.);
        s.tracks("other", 0, &changes, 8.);
        assert_eq!(
            (
                s.work.since(&before).channel_computations,
                s.work.since(&before).channel_reuses
            ),
            (1, 1)
        );
    }

    #[test]
    fn ordinal_adjacent_sweeps_never_worsen_the_pair_proxy() {
        let mut seed = 97_u64;
        for _ in 0..128 {
            let mut next = || {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                (seed >> 32) as usize % 12
            };
            let assignments = (0..12)
                .map(|i| abstract_routing::Assignment {
                    edge: format!("e{i}"),
                    from: abstract_routing::Lane {
                        element: next(),
                        port: i,
                    },
                    to: abstract_routing::Lane {
                        element: next(),
                        port: i,
                    },
                    slot: 0,
                })
                .collect();
            let base = abstract_routing::channel("area".into(), 0, assignments);
            let quality = quality_order(base.clone());
            let cost = |channel: &abstract_routing::Channel| {
                channel
                    .order
                    .iter()
                    .enumerate()
                    .map(|(i, &a)| {
                        channel.order[i + 1..]
                            .iter()
                            .map(|&b| pair_cost(&channel.assignments[a], &channel.assignments[b]))
                            .sum::<usize>()
                    })
                    .sum::<usize>()
            };
            assert!(cost(&quality) <= cost(&base));
            let position: BTreeMap<_, _> = quality
                .order
                .iter()
                .enumerate()
                .map(|(i, &a)| (a, i))
                .collect();
            for (at, &a) in quality.order.iter().enumerate() {
                let a = &quality.assignments[a];
                let (lo, hi) = (
                    a.from.clone().min(a.to.clone()),
                    a.from.clone().max(a.to.clone()),
                );
                for &b in &quality.order[..at] {
                    let b = &quality.assignments[b];
                    let (l, h) = (
                        b.from.clone().min(b.to.clone()),
                        b.from.clone().max(b.to.clone()),
                    );
                    if lo <= h && l <= hi {
                        assert!(a.slot > b.slot);
                    }
                }
            }
            for &(a, b) in &quality.dependencies {
                assert_eq!(
                    quality.broken_dependencies.contains(&(a, b)),
                    position[&a] > position[&b]
                );
            }
        }
    }
}
