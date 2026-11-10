//! Provisional execution identities for refinement jobs (P0). Internal to the runtime, not a stable ABI.
//!
//! Identities describe what a job read and which candidates it produced, never how it was scheduled. A job ID,
//! thread count or completion order must not change geometry, scoring or candidate selection.

/// Candidate identity, fixed before dispatch. Completion order and thread IDs never change it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CandidateId {
    /// Fixed strategy name (`ordering`, `routing`, `routing-last`, `search`, `flat-region`, `other-port-placement`).
    pub strategy: &'static str,
    /// Stable numeric parameter (the ordering mode); `0` for strategies without one.
    pub parameter: usize,
    /// Position among entries with the same strategy and parameter in the configured candidate list.
    pub occurrence: usize,
}

/// Identity of the input a job reads. Equal identities mean equal inputs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputIdentity {
    pub graph_revision: u64,
    pub base_layout_version: u64,
    /// Canonical serialized layout options. Field order is fixed by serde; a preset ID alone is not used.
    pub options_key: String,
}

impl InputIdentity {
    pub(crate) fn new(
        graph_revision: u64,
        base_layout_version: u64,
        options: &arqlet_model::LayoutOptions,
    ) -> Result<Self, String> {
        Ok(Self {
            graph_revision,
            base_layout_version,
            options_key: serde_json::to_string(options).map_err(|e| e.to_string())?,
        })
    }
}

/// Work actually performed by one refinement job. Counts completed operations, not reserved capacity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkCounters {
    /// Candidates whose evaluation was entered before any stop signal was seen.
    pub candidates_started: usize,
    /// Candidates whose evaluation returned (accepted or rejected by geometry).
    pub candidates_completed: usize,
    /// Global routing rounds that ran to completion.
    pub routing_rounds: usize,
    /// Channel A* expansions: routing candidates, global rounds, ordering candidates (channel engine), flat-region
    /// compaction and the placement search's re-routes. The alternative port placement's own pipeline is not counted.
    pub astar_expansions: usize,
    /// A* searches completed (one per routed edge per routing pass).
    pub astar_searches: usize,
    /// The largest number of expansions any single A* search needed.
    pub largest_astar_search: usize,
    /// Placement trials evaluated by the placement search, including rejected ones.
    pub placement_trials: usize,
    /// Legacy track router: candidate routes evaluated (each a scan of the obstacles and earlier routes). Evaluations of
    /// this router only; not comparable with `astar_expansions`.
    pub legacy_candidates: usize,
    /// Legacy repair router: grid-search heap pops. Grid steps of this router only; not comparable with
    /// `astar_expansions`.
    pub legacy_grid_pops: usize,
    /// The grid pops of the legacy repair by route id (attribution of the repair work to edges).
    pub legacy_grid_pops_by_route: std::collections::BTreeMap<String, usize>,
    /// Number of corridor gap evaluations and widening rounds across candidates.
    pub corridor_gap_calculations: usize,
    pub clearance_widening_rounds: usize,
    pub clearance_widening_noop_rounds: usize,
    /// Layered construction: routes taken from the plans' sections, and edges handed to the fallback router.
    pub constructed_routes: usize,
    pub delegated_routes: usize,
    pub abstract_routing_passes: usize,
    /// Fixed-track repair segment projections materialized, including refreshes after accepted moves.
    pub fixed_track_segment_projections_computed: usize,
    /// Route-pair checks made using the cached fixed-track segment projections.
    pub fixed_track_projection_pair_checks: usize,
    /// Actual layered-construction fallback reasons, recorded per route and construction pass.
    #[cfg(feature = "benchmark-timings")]
    pub delegation_reasons: Vec<(String, String)>,
    /// Candidates whose result was their input, byte for byte, and took over its measurement.
    pub reused_evaluations: usize,
    /// Complete layout passes and the placement passes within them (`SearchCounters`).
    pub layout_passes: usize,
    pub placement_passes: usize,
    /// Root placement pass counts by reason.
    pub placement_pass_reasons: std::collections::BTreeMap<String, usize>,
    /// Constructions started from a probe's preparation (`SearchCounters::reused_preparations`).
    pub reused_preparations: usize,
    pub ordering_passes: usize,
    pub reused_orderings: usize,
    pub cross_plans: usize,
    pub reused_cross_plans: usize,
    pub structural_plans: usize,
    pub reused_structural_plans: usize,
    pub region_realizations: usize,
    pub reused_region_realizations: usize,
    pub deferred_routing_passes: usize,
    pub construction: arqlet_layout::ConstructionCounters,
    /// Cumulative stage times in nanoseconds. Native benchmark builds populate them; default and
    /// WASM builds leave them zero. These are not deterministic work units, and concurrent candidate
    /// totals may exceed refinement wall time.
    pub placement_nanos: u64,
    pub organic_stress_nanos: u64,
    pub separation_nanos: u64,
    pub repair_nanos: u64,
    pub port_resolution_nanos: u64,
    pub routing_nanos: u64,
    pub corridor_gap_nanos: u64,
    pub scoring_nanos: u64,
    /// The fallback router on the delegated edges of the layered construction.
    pub delegated_nanos: u64,
    /// Timed subphases of abstract completion, nested within `routing_nanos`.
    pub abstract_completion: arqlet_layout::AbstractCompletionTimes,
    /// The whole evaluation of the candidate's job: its construction, routing and scoring.
    pub job_nanos: u64,
    /// The Optimize-time crossing step after the candidates (`relieve_crossings`), with its quality when it is accepted.
    /// Not part of any candidate; set on the job's report only.
    pub relieve_nanos: u64,
}

impl WorkCounters {
    /// Adds the work of one evaluated candidate. Only the per-candidate work fields are summed here.
    pub(crate) fn add_work(&mut self, other: &WorkCounters) {
        self.astar_expansions += other.astar_expansions;
        self.astar_searches += other.astar_searches;
        self.largest_astar_search = self.largest_astar_search.max(other.largest_astar_search);
        self.placement_trials += other.placement_trials;
        self.legacy_candidates += other.legacy_candidates;
        self.legacy_grid_pops += other.legacy_grid_pops;
        for (route, pops) in &other.legacy_grid_pops_by_route {
            *self
                .legacy_grid_pops_by_route
                .entry(route.clone())
                .or_default() += pops;
        }
        self.corridor_gap_calculations += other.corridor_gap_calculations;
        self.clearance_widening_rounds += other.clearance_widening_rounds;
        self.clearance_widening_noop_rounds += other.clearance_widening_noop_rounds;
        self.placement_nanos = self.placement_nanos.saturating_add(other.placement_nanos);
        self.organic_stress_nanos = self
            .organic_stress_nanos
            .saturating_add(other.organic_stress_nanos);
        self.separation_nanos = self.separation_nanos.saturating_add(other.separation_nanos);
        self.repair_nanos = self.repair_nanos.saturating_add(other.repair_nanos);
        self.port_resolution_nanos = self
            .port_resolution_nanos
            .saturating_add(other.port_resolution_nanos);
        self.routing_nanos = self.routing_nanos.saturating_add(other.routing_nanos);
        self.corridor_gap_nanos = self
            .corridor_gap_nanos
            .saturating_add(other.corridor_gap_nanos);
        self.scoring_nanos = self.scoring_nanos.saturating_add(other.scoring_nanos);
        self.constructed_routes += other.constructed_routes;
        self.delegated_routes += other.delegated_routes;
        self.abstract_routing_passes += other.abstract_routing_passes;
        self.fixed_track_segment_projections_computed +=
            other.fixed_track_segment_projections_computed;
        self.fixed_track_projection_pair_checks += other.fixed_track_projection_pair_checks;
        self.abstract_completion.add(&other.abstract_completion);
        #[cfg(feature = "benchmark-timings")]
        self.delegation_reasons
            .extend(other.delegation_reasons.iter().cloned());
        self.reused_evaluations += other.reused_evaluations;
        self.layout_passes += other.layout_passes;
        self.placement_passes += other.placement_passes;
        for (reason, count) in &other.placement_pass_reasons {
            *self
                .placement_pass_reasons
                .entry(reason.clone())
                .or_default() += count;
        }
        self.reused_preparations += other.reused_preparations;
        self.ordering_passes += other.ordering_passes;
        self.reused_orderings += other.reused_orderings;
        self.cross_plans += other.cross_plans;
        self.reused_cross_plans += other.reused_cross_plans;
        self.structural_plans += other.structural_plans;
        self.reused_structural_plans += other.reused_structural_plans;
        self.region_realizations += other.region_realizations;
        self.reused_region_realizations += other.reused_region_realizations;
        self.deferred_routing_passes += other.deferred_routing_passes;
        self.construction.add(&other.construction);
        self.delegated_nanos = self.delegated_nanos.saturating_add(other.delegated_nanos);
        self.job_nanos = self.job_nanos.saturating_add(other.job_nanos);
    }

    /// Adds the channel router's search work of one routing call.
    pub(crate) fn absorb_search(&mut self, search: &arqlet_layout::SearchCounters) {
        self.astar_expansions += search.expansions;
        self.astar_searches += search.searches;
        self.largest_astar_search = self.largest_astar_search.max(search.largest_search);
        self.legacy_candidates += search.legacy_candidates;
        self.legacy_grid_pops += search.legacy_grid_pops;
        for (route, pops) in &search.legacy_grid_pops_by_route {
            *self
                .legacy_grid_pops_by_route
                .entry(route.clone())
                .or_default() += pops;
        }
        self.corridor_gap_calculations += search.corridor_gap_calculations;
        self.clearance_widening_rounds += search.clearance_widening_rounds;
        self.clearance_widening_noop_rounds += search.clearance_widening_noop_rounds;
        self.constructed_routes += search.constructed_routes;
        self.delegated_routes += search.delegated_routes;
        self.abstract_routing_passes += search.abstract_routing_passes;
        self.fixed_track_segment_projections_computed +=
            search.fixed_track_segment_projections_computed;
        self.fixed_track_projection_pair_checks += search.fixed_track_projection_pair_checks;
        #[cfg(feature = "benchmark-timings")]
        self.delegation_reasons
            .extend(search.delegation_reasons.iter().cloned());
        self.layout_passes += search.layout_passes;
        self.placement_passes += search.placement_passes;
        for (reason, count) in &search.placement_pass_reasons {
            *self
                .placement_pass_reasons
                .entry(reason.clone())
                .or_default() += count;
        }
        self.reused_preparations += search.reused_preparations;
        self.ordering_passes += search.ordering_passes;
        self.reused_orderings += search.reused_orderings;
        self.cross_plans += search.cross_plans;
        self.reused_cross_plans += search.reused_cross_plans;
        self.structural_plans += search.structural_plans;
        self.reused_structural_plans += search.reused_structural_plans;
        self.region_realizations += search.region_realizations;
        self.reused_region_realizations += search.reused_region_realizations;
        self.deferred_routing_passes += search.deferred_routing_passes;
        self.construction.add(&search.construction);
        self.delegated_nanos = self.delegated_nanos.saturating_add(search.delegated_nanos);
        self.abstract_completion.add(&search.abstract_completion);
        self.placement_nanos = self.placement_nanos.saturating_add(search.placement_nanos);
        self.organic_stress_nanos = self
            .organic_stress_nanos
            .saturating_add(search.organic_stress_nanos);
        self.separation_nanos = self
            .separation_nanos
            .saturating_add(search.separation_nanos);
        self.repair_nanos = self.repair_nanos.saturating_add(search.repair_nanos);
        self.port_resolution_nanos = self
            .port_resolution_nanos
            .saturating_add(search.port_resolution_nanos);
        self.routing_nanos = self.routing_nanos.saturating_add(search.routing_nanos);
        self.corridor_gap_nanos = self
            .corridor_gap_nanos
            .saturating_add(search.corridor_gap_nanos);
    }
}

/// What one refinement job did. Returned next to the snapshot; it is execution metadata, not geometry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JobReport {
    /// Host-supplied opaque invocation ID. Echoed only; never used for scoring or geometry.
    pub job_id: String,
    pub input: InputIdentity,
    pub counters: WorkCounters,
    /// Per-candidate counters from the initial refinement batch, in configured reduction order.
    pub candidate_work: Vec<CandidateWork>,
    /// Completed candidates in fixed configured order.
    pub completed: Vec<CandidateId>,
    /// Strategies the planner did not plan for this view, each with the reason (see `plan_first_stage`).
    pub excluded: Vec<Exclusion>,
}

/// Work and timing for one independently computed initial candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateWork {
    pub candidate: CandidateId,
    pub counters: WorkCounters,
}

/// A strategy the planner did not plan, and the reason, from its declared capabilities or the view's structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exclusion {
    pub strategy: &'static str,
    pub reason: String,
}

/// A complete, validated improvement over the input, published while refinement runs. The host applies it only while
/// its `input` identity still matches the current state; a newer edit, view or configuration makes it stale.
#[derive(Clone, Debug)]
pub struct Publication {
    /// The host's opaque ID of the job that produced this publication.
    pub job_id: String,
    /// Position in this job's publication sequence, starting at 1.
    pub sequence: usize,
    pub strategy: String,
    /// The shared conflict key of `snapshot`, in the configured objective order. Keys strictly decrease along a job.
    pub key: Vec<i64>,
    /// Candidates evaluated when this improvement was found.
    pub candidates_evaluated: usize,
    /// Identity of the input this publication was computed from.
    pub input: InputIdentity,
    pub snapshot: arqlet_model::Snapshot,
    /// The bounded archive at this publication: the best measured candidates so far, best first (see `Archive`).
    pub archive: Vec<ArchiveEntry>,
    /// Tier 1 of the shadow model for `snapshot`: node overlaps and edge-node hits (and near-node pairs under a
    /// required clearance). `None` when the snapshot cannot be measured.
    pub hard_violations: Option<usize>,
    /// The shadow weighted total (tier 2) of `snapshot`, in millionths of 0..100. Reported, not used for acceptance.
    pub shadow_total: Option<i64>,
}

/// One entry of the bounded archive: a measured candidate, by key, with its strategy and geometry identity. Metadata
/// only; the snapshot is not kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveEntry {
    /// Position of the candidate among all candidates of this refinement, in reduction order.
    pub index: usize,
    pub strategy: String,
    pub key: Vec<i64>,
    /// Hash of the drawn geometry; equal values mean identical geometry.
    pub geometry_fingerprint: String,
}

/// Version of the streamed publication JSON (the typed `PublicationEnvelope`). Version 2 adds `publicationVersion` and `score`;
/// version 3 adds `archive`; version 4 adds `kind`.
/// Version 1 has none of them.
pub const PUBLICATION_VERSION: u32 = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_ids_order_by_strategy_then_parameter_then_occurrence() {
        let a = CandidateId {
            strategy: "ordering",
            parameter: 1,
            occurrence: 0,
        };
        let b = CandidateId {
            strategy: "ordering",
            parameter: 1,
            occurrence: 1,
        };
        let c = CandidateId {
            strategy: "routing",
            parameter: 0,
            occurrence: 0,
        };
        assert!(a < b && b < c);
    }

    #[test]
    fn input_identity_changes_only_with_its_inputs() {
        let options = arqlet_model::LayoutOptions::default();
        let base = InputIdentity::new(3, 7, &options).unwrap();
        assert_eq!(base, InputIdentity::new(3, 7, &options).unwrap());
        assert_ne!(base, InputIdentity::new(4, 7, &options).unwrap());
        assert_ne!(base, InputIdentity::new(3, 8, &options).unwrap());
        let mut other = options.clone();
        other.routing_candidate = !other.routing_candidate;
        assert_ne!(base, InputIdentity::new(3, 7, &other).unwrap());
    }
}

/// How a candidate ended. A job that produced no candidate, or whose candidate failed the geometry rules, is `NoResult`:
/// the two are not separated, because the job does not report which rule failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandidateOutcome {
    NoResult,
    Unmeasurable,
    Measured,
}

/// One complete candidate of a refinement, recorded for offline comparison. Diagnostics only, not a stable ABI.
///
/// Scope: the first-stage jobs and the routing jobs of each global round. Internal routing rounds, A* searches and
/// placement trials are not separate records; their totals are in `work`, which is shared by every candidate of its
/// job. Stopped candidates are not recorded. Records are in the fixed reduction order, so the log is identical for any
/// thread count.
#[derive(Clone, Debug)]
pub struct CandidateRecord {
    /// Position in the log.
    pub index: usize,
    pub input: InputIdentity,
    /// `candidate` for first-stage jobs, `round` for global routing rounds.
    pub phase: &'static str,
    /// The 1-based global round, for `round` records.
    pub round: Option<usize>,
    /// The strategy identity, for first-stage jobs.
    pub id: Option<CandidateId>,
    /// The strategy name the job produced (`ordering` names, `routing-only`, ...); `None` for `NoResult`.
    pub strategy: Option<String>,
    /// `input`, or the strategy of the incumbent a round started from.
    pub parent: String,
    pub outcome: CandidateOutcome,
    pub metrics: Option<arqlet_scene::metrics::Metrics>,
    /// Tier 1 of the shadow model: node overlaps and edge-node hits.
    pub hard_violations: Option<usize>,
    /// The published conflict key, the current acceptance measure.
    pub key: Option<Vec<i64>>,
    /// The shadow weighted total (tier 2), reported only.
    pub shadow_total: Option<i64>,
    /// The search work of the job that produced this candidate.
    pub work: WorkCounters,
    /// Hash of boxes and routes; equal values mean identical geometry.
    pub geometry_fingerprint: Option<String>,
    /// The first earlier record with identical geometry.
    pub duplicate_of: Option<usize>,
    /// The candidate itself, for visual comparison.
    pub snapshot: Option<arqlet_model::Snapshot>,
    /// Became the incumbent at its step of the search.
    pub accepted: bool,
    /// The final result of the refinement.
    pub selected: bool,
    /// Emitted as a publication over the input.
    pub published: bool,
}

/// What a strategy needs and where it runs. Declared once per strategy; the planner's gates must agree with it (a test
/// checks the names and the stages). Provisional, not a stable ABI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrategyCapabilities {
    /// The fixed strategy name, the same as `CandidateId::strategy`.
    pub name: &'static str,
    /// Runs in the first stage, over the input snapshot.
    pub first_stage: bool,
    /// Runs in every global routing round, over the current incumbent.
    pub global_rounds: bool,
    /// Needs the channel routing engine.
    pub channel_engine: bool,
    /// The layout option that enables it in the first stage, when it is not always planned.
    pub option: Option<&'static str>,
    /// The graph class it accepts: `any`, or `flat` (no containers, biconnected and planar).
    pub graph: &'static str,
    /// The port placement the result is drawn with when it differs from the input's (the result's options carry it).
    pub port_override: Option<&'static str>,
}
