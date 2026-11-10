//! Host-side policy for progressive publications (provisional; pure, no I/O, no geometry).
//!
//! The host reports user edits, job starts and job ends, and offers every publication it receives. The policy says
//! what may be shown, which job is still active, and what the next job must be based on. Two states are kept apart:
//! the **base** of the active job (the input identity all its publications refer to, fixed when it starts) and the
//! **pending** publication (the newest accepted one that the host has not shown yet; latest wins).

use crate::execution::{InputIdentity, Publication};

/// The answer to an offered publication.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Offer {
    /// Accepted: it is now the pending publication, and it replaces any older pending one.
    Accepted { sequence: usize },
    /// The job is not active, or its base no longer matches the current state. Discard it.
    Stale,
    /// A publication with this sequence or a newer one was already accepted for the active job. Discard it.
    Duplicate,
}

/// How a job ended.
#[derive(Debug)]
pub enum Finish {
    /// Completed. `result` is the newest publication that was never shown, if any; the host must apply it as the
    /// final result. `improved` tells whether any publication was accepted during the job.
    Completed {
        improved: bool,
        result: Option<Publication>,
    },
    /// Failed or stopped. Pending publications are dropped; the last shown state stays. `shown` is its sequence.
    Failed { shown: Option<usize> },
    /// The job is not the active one (superseded or cancelled); nothing changes.
    Ignored,
}

#[derive(Debug)]
struct Active {
    job_id: String,
    base: InputIdentity,
    last_accepted: usize,
    shown: Option<usize>,
    pending: Option<Publication>,
}

/// The state the host currently edits, and the job working on it.
#[derive(Debug, Default)]
pub struct Session {
    current: Option<InputIdentity>,
    active: Option<Active>,
}

impl Session {
    pub fn new() -> Self {
        Self::default()
    }

    /// The identity the host currently edits.
    pub fn current(&self) -> Option<&InputIdentity> {
        self.current.as_ref()
    }

    /// The active job's ID, if any.
    pub fn active_job(&self) -> Option<&str> {
        self.active.as_ref().map(|a| a.job_id.as_str())
    }

    /// The sequence of the publication the host shows for the active job.
    pub fn shown(&self) -> Option<usize> {
        self.active.as_ref().and_then(|a| a.shown)
    }

    /// A user edit, configuration change or view change made the current state `identity`. It replaces the state
    /// and cancels the active job; the returned ID is the job the host must stop.
    pub fn edit(&mut self, identity: InputIdentity) -> Option<String> {
        self.current = Some(identity);
        self.active.take().map(|a| a.job_id)
    }

    /// Starts a job based on `base`. Refused (`false`) when `base` is not the current state: such a job would start
    /// from an outdated input. A job that is already active is superseded; its ID is returned for the host to stop.
    pub fn start(&mut self, job_id: &str, base: InputIdentity) -> Result<Option<String>, ()> {
        if self
            .current
            .as_ref()
            .is_some_and(|current| *current != base)
        {
            return Err(());
        }
        self.current = Some(base.clone());
        let superseded = self.active.take().map(|a| a.job_id);
        self.active = Some(Active {
            job_id: job_id.to_string(),
            base,
            last_accepted: 0,
            shown: None,
            pending: None,
        });
        Ok(superseded)
    }

    /// Offers a publication from a job. Only the active job's publications with its base are accepted, in increasing
    /// sequence order.
    pub fn offer(&mut self, publication: &Publication) -> Offer {
        let Some(active) = self.active.as_mut() else {
            return Offer::Stale;
        };
        if active.job_id != publication.job_id || active.base != publication.input {
            return Offer::Stale;
        }
        if publication.sequence <= active.last_accepted {
            return Offer::Duplicate;
        }
        active.last_accepted = publication.sequence;
        active.pending = Some(publication.clone());
        Offer::Accepted {
            sequence: publication.sequence,
        }
    }

    /// The newest accepted publication that has not been shown yet. Taking it makes it the shown state. Older pending
    /// publications are never shown: only the newest one counts.
    pub fn take_pending(&mut self) -> Option<Publication> {
        let active = self.active.as_mut()?;
        let pending = active.pending.take()?;
        active.shown = Some(pending.sequence);
        Some(pending)
    }

    /// The active job completed. A job without accepted publications completes without improvement.
    pub fn finish(&mut self, job_id: &str) -> Finish {
        match self.active.as_ref() {
            Some(active) if active.job_id == job_id => {}
            _ => return Finish::Ignored,
        }
        let mut active = self.active.take().expect("checked above");
        let improved = active.last_accepted > 0;
        Finish::Completed {
            improved,
            result: active.pending.take(),
        }
    }

    /// The active job failed or was stopped. Pending publications are dropped, and the shown state stays.
    pub fn fail(&mut self, job_id: &str) -> Finish {
        match self.active.as_ref() {
            Some(active) if active.job_id == job_id => {}
            _ => return Finish::Ignored,
        }
        let active = self.active.take().expect("checked above");
        Finish::Failed {
            shown: active.shown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{refine_snapshot_progressive, tests::dense_graph};
    use arqlet_model::{Graph, LayoutOptions, Objective};

    /// Every publication of one refinement, plus the input identity they all refer to.
    fn publications() -> (Vec<Publication>, InputIdentity) {
        let graph: Graph = serde_json::from_value(dense_graph()).unwrap();
        let mut input = crate::import_graph_with(
            graph,
            40,
            &LayoutOptions {
                layered_construction: false,
                hierarchical_ordering: false,
                ..LayoutOptions::default()
            },
        )
        .unwrap();
        // The session tests check publication, not the clearance: the fixture is pinned to proximity as a weighted term
        // (the opt-out of the default required clearance), which is where it yields several improvements.
        input.options.required_clearance = None;
        // This fixture needs several improvements to test the session. The previous default order (near-node pairs
        // before crossings) gives them; the session tests do not depend on the objective order itself.
        input.options.objectives = vec![
            Objective::SiblingOverlaps,
            Objective::EdgeNodeHits,
            Objective::SharedRoutePairs,
            Objective::NearNodeSegments,
            Objective::RoutedCrossings,
            Objective::ProjectedCrossings,
            Objective::Bends,
            Objective::Cost,
        ];
        let identity =
            InputIdentity::new(input.revision, input.layout_version, &input.options).unwrap();
        let mut out = Vec::new();
        refine_snapshot_progressive(
            "job-a",
            &input,
            input.revision,
            input.layout_version,
            1,
            None,
            None,
            &mut |p: &Publication| out.push(p.clone()),
        )
        .unwrap();
        assert!(
            out.len() >= 2,
            "the fixture publishes at least two improvements"
        );
        (out, identity)
    }

    fn other_identity(base: &InputIdentity) -> InputIdentity {
        InputIdentity {
            graph_revision: base.graph_revision + 1,
            ..base.clone()
        }
    }

    /// The rules as shared cases (examples/fixtures/session/cases.json): apps/playground/session.js runs the same cases, so the
    /// page's policy and this one cannot drift apart. A publication is a real one with the job, the sequence and the input of the step.
    #[test]
    fn shared_cases() {
        let (published, base) = publications();
        let text = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/fixtures/session/cases.json"),
        )
        .unwrap();
        let file: serde_json::Value = serde_json::from_str(&text).unwrap();
        let identity = |name: &str| match name {
            "base" => base.clone(),
            "newer" => other_identity(&base),
            other => panic!("unknown identity {other}"),
        };
        for case in file["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let mut session = Session::new();
            for (index, step) in case["steps"].as_array().unwrap().iter().enumerate() {
                let job = step["job"].as_str().unwrap_or("");
                let got = match step["do"].as_str().unwrap() {
                    "start" => match session.start(job, identity(step["base"].as_str().unwrap())) {
                        Ok(superseded) => {
                            serde_json::json!({"refused": false, "superseded": superseded})
                        }
                        Err(()) => serde_json::json!({"refused": true, "superseded": null}),
                    },
                    "offer" => {
                        let mut publication = published[0].clone();
                        publication.job_id = job.to_string();
                        publication.sequence = step["sequence"].as_u64().unwrap() as usize;
                        publication.input = identity(step["base"].as_str().unwrap());
                        let verdict = match session.offer(&publication) {
                            Offer::Accepted { .. } => "accepted",
                            Offer::Stale => "stale",
                            Offer::Duplicate => "duplicate",
                        };
                        serde_json::json!({ "verdict": verdict })
                    }
                    "take" => {
                        serde_json::json!({"sequence": session.take_pending().map(|p| p.sequence)})
                    }
                    "edit" => {
                        serde_json::json!({"cancelled": session.edit(identity(step["base"].as_str().unwrap()))})
                    }
                    "finish" => match session.finish(job) {
                        Finish::Completed { improved, result } => serde_json::json!({
                            "outcome": "completed", "improved": improved,
                            "resultSequence": result.map(|p| p.sequence)
                        }),
                        Finish::Ignored => {
                            serde_json::json!({"outcome": "ignored", "improved": null, "resultSequence": null})
                        }
                        Finish::Failed { .. } => panic!("finish never fails"),
                    },
                    "fail" => match session.fail(job) {
                        Finish::Failed { shown } => {
                            serde_json::json!({"outcome": "failed", "shown": shown})
                        }
                        Finish::Ignored => serde_json::json!({"outcome": "ignored", "shown": null}),
                        Finish::Completed { .. } => panic!("fail never completes"),
                    },
                    "state" => {
                        serde_json::json!({"activeJob": session.active_job(), "shown": session.shown()})
                    }
                    other => panic!("unknown step {other}"),
                };
                assert_eq!(got, step["expect"], "{name}: step {}", index + 1);
            }
        }
    }

    #[test]
    fn a_user_edit_between_publications_makes_the_job_stale() {
        let (published, base) = publications();
        let mut session = Session::new();
        session.start("job-a", base.clone()).unwrap();
        assert_eq!(
            session.offer(&published[0]),
            Offer::Accepted { sequence: 1 }
        );
        assert!(session.take_pending().is_some());
        assert_eq!(
            session.edit(other_identity(&base)).as_deref(),
            Some("job-a")
        );
        assert_eq!(session.offer(&published[1]), Offer::Stale);
        assert!(matches!(session.finish("job-a"), Finish::Ignored));
    }

    #[test]
    fn late_duplicate_and_superseded_publications_are_discarded() {
        let (published, base) = publications();
        let mut session = Session::new();
        session.start("job-a", base.clone()).unwrap();
        assert_eq!(
            session.offer(&published[0]),
            Offer::Accepted { sequence: 1 }
        );
        assert_eq!(session.offer(&published[0]), Offer::Duplicate);
        // A new job on the same base supersedes job-a; its late publications must not be shown.
        assert_eq!(
            session.start("job-b", base).unwrap().as_deref(),
            Some("job-a")
        );
        assert_eq!(session.offer(&published[1]), Offer::Stale);
        assert_eq!(session.active_job(), Some("job-b"));
    }

    #[test]
    fn several_valid_intermediate_states_show_only_the_newest() {
        let (published, base) = publications();
        let mut session = Session::new();
        session.start("job-a", base).unwrap();
        for p in &published {
            assert!(matches!(session.offer(p), Offer::Accepted { .. }));
        }
        let shown = session.take_pending().expect("the newest state is pending");
        assert_eq!(shown.sequence, published.last().unwrap().sequence);
        assert!(
            session.take_pending().is_none(),
            "older states are never shown"
        );
        assert_eq!(session.shown(), Some(published.last().unwrap().sequence));
    }

    #[test]
    fn completion_without_improvement_is_reported() {
        let (_, base) = publications();
        let mut session = Session::new();
        session.start("job-a", base).unwrap();
        assert!(matches!(
            session.finish("job-a"),
            Finish::Completed {
                improved: false,
                result: None
            }
        ));
        assert!(matches!(session.finish("job-a"), Finish::Ignored));
    }

    #[test]
    fn completion_hands_over_an_unshown_result() {
        let (published, base) = publications();
        let mut session = Session::new();
        session.start("job-a", base).unwrap();
        for p in &published {
            session.offer(p);
        }
        match session.finish("job-a") {
            Finish::Completed { improved, result } => {
                assert!(improved);
                assert_eq!(result.unwrap().sequence, published.last().unwrap().sequence);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn failure_keeps_the_last_shown_state_and_drops_pending_work() {
        let (published, base) = publications();
        let mut session = Session::new();
        session.start("job-a", base).unwrap();
        session.offer(&published[0]);
        session.take_pending();
        session.offer(&published[1]);
        assert!(matches!(
            session.fail("job-a"),
            Finish::Failed { shown: Some(1) }
        ));
        assert!(matches!(session.fail("job-a"), Finish::Ignored));
        assert!(session.active_job().is_none());
    }

    #[test]
    fn a_job_cannot_start_from_an_outdated_base() {
        let (_, base) = publications();
        let mut session = Session::new();
        session.edit(other_identity(&base));
        assert!(session.start("job-a", base).is_err());
    }
}
