//! Shadow quality evaluation (M0 of docs/recursive-optimization.md §13). Pure and deterministic. Nothing selects on it
//! yet; the published conflict key stays the acceptance rule. Weights and scales are experimental hypotheses, not a
//! calibrated default (docs/adr/0002-quality-model.md).
//!
//! Two tiers. Tier 1 is the hard violation count: node overlaps and edge-node hits. A candidate with fewer violations
//! always ranks higher, whatever its score. Tier 2 is the weighted total over the remaining terms.
//!
//! Only basic IEEE operations (+, *, /, round) are used, so native and WASM builds agree on every total.
use crate::metrics::Metrics;
use std::cmp::Ordering;

/// Version of the term mapping, weights and utility family. Bump on any change to those.
pub const QUALITY_VERSION: u32 = 2;

/// Totals are reported in millionths of the 0..100 scale, so comparisons are integer comparisons.
const TOTAL_SCALE: f64 = 1_000_000.;

/// One weighted term: a raw value from the shared metric, a weight and a scale for `u = 1 / (1 + raw / scale)`.
#[derive(Clone, Debug, PartialEq)]
pub struct Term {
    pub name: &'static str,
    pub weight: f64,
    pub scale: f64,
}

/// The weighted utility model. Every weight is finite and nonnegative, every scale positive, and the weights sum to
/// a positive total. A model that breaks these rules is an error, never a silent default.
#[derive(Clone, Debug, PartialEq)]
pub struct QualityModel {
    pub terms: Vec<Term>,
}

impl QualityModel {
    /// The shadow default. Counts use a scale of one (one violation halves the utility); length uses a scale of
    /// 1000 drawing units. These numbers are hypotheses to be calibrated on held-out graph families.
    pub fn shadow_default() -> Self {
        let term = |name, weight, scale| Term {
            name,
            weight,
            scale,
        };
        Self {
            terms: vec![
                term("sharedRoutePairs", 2., 1.),
                term("nearNodeSegments", 1., 1.),
                term("routedCrossings", 3., 1.),
                term("bends", 1., 1.),
                term("length", 1., 1000.),
            ],
        }
    }

    fn validate(&self) -> Result<(), String> {
        let mut total = 0.;
        for t in &self.terms {
            if !t.weight.is_finite() || t.weight < 0. {
                return Err(format!("quality term {} has an invalid weight", t.name));
            }
            if !t.scale.is_finite() || t.scale <= 0. {
                return Err(format!("quality term {} has an invalid scale", t.name));
            }
            total += t.weight;
        }
        if total <= 0. {
            return Err("quality model needs a positive total weight".into());
        }
        Ok(())
    }
}

/// One evaluated term: the raw value it was given, its utility in 0..=1 and its weighted contribution.
#[derive(Clone, Debug, PartialEq)]
pub struct TermValue {
    pub name: &'static str,
    pub raw: f64,
    pub utility: f64,
    pub contribution: f64,
}

/// A complete shadow evaluation. `hard_violations` is tier 1 (fewer is better); `total` is tier 2 in millionths of
/// 0..100 (larger is better).
#[derive(Clone, Debug, PartialEq)]
pub struct Evaluation {
    pub version: u32,
    pub hard_violations: usize,
    pub terms: Vec<TermValue>,
    pub total: i64,
}

/// Tier 1: node overlaps (same parent or across branches) and edge-node hits (including container traversals). These
/// are hard constraints, so they are counted apart from the weighted total and never traded against it.
pub fn hard_violations(m: &Metrics) -> usize {
    m.sibling_overlaps + m.cross_branch_overlaps + m.line_node_hits + m.container_traversals
}

/// The raw value of each weighted term, read from the shared metric.
fn raw_values(m: &Metrics) -> [(&'static str, f64); 5] {
    [
        ("sharedRoutePairs", m.shared_line_pairs as f64),
        ("nearNodeSegments", m.hug_pairs as f64),
        (
            "routedCrossings",
            (m.crossings + m.shared_endpoint_crossings) as f64,
        ),
        ("bends", m.bends as f64),
        ("length", m.length),
    ]
}

/// Scores a measured drawing under a model. Every raw value must be finite and nonnegative; otherwise the candidate
/// is unevaluable and the caller must not compare it.
pub fn evaluate(metrics: &Metrics, model: &QualityModel) -> Result<Evaluation, String> {
    evaluate_with(metrics, model, false)
}

/// As `evaluate`. With `required_clearance`, near-node proximity is a hard constraint: its pairs count as tier-1
/// violations and it leaves the weighted total, so it is never traded against the score.
pub fn evaluate_with(
    metrics: &Metrics,
    model: &QualityModel,
    required_clearance: bool,
) -> Result<Evaluation, String> {
    model.validate()?;
    let raws = raw_values(metrics);
    let mut weight_total = 0.;
    let mut weighted = 0.;
    let mut terms = Vec::with_capacity(model.terms.len());
    for term in &model.terms {
        if required_clearance && term.name == "nearNodeSegments" {
            continue;
        }
        let raw = raws
            .iter()
            .find(|(name, _)| *name == term.name)
            .map(|(_, value)| *value)
            .ok_or_else(|| format!("no metric for quality term {}", term.name))?;
        if !raw.is_finite() || raw < 0. {
            return Err(format!(
                "quality term {} has a non-finite raw value",
                term.name
            ));
        }
        let utility = 1. / (1. + raw / term.scale);
        let contribution = term.weight * utility;
        weight_total += term.weight;
        weighted += contribution;
        terms.push(TermValue {
            name: term.name,
            raw,
            utility,
            contribution,
        });
    }
    let total = (100. * weighted / weight_total * TOTAL_SCALE).round() as i64;
    let proximity = if required_clearance {
        metrics.hug_pairs
    } else {
        0
    };
    Ok(Evaluation {
        version: QUALITY_VERSION,
        hard_violations: hard_violations(metrics) + proximity,
        terms,
        total,
    })
}

/// The order a candidate pair is ranked in: fewer hard violations first, then the higher weighted total. Returns
/// `Greater` when `a` ranks above `b`.
pub fn tier_order(a: &Evaluation, b: &Evaluation) -> Ordering {
    b.hard_violations
        .cmp(&a.hard_violations)
        .then(a.total.cmp(&b.total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_drawing_scores_the_maximum() {
        let eval = evaluate(&Metrics::default(), &QualityModel::shadow_default()).unwrap();
        assert_eq!(eval.total, 100 * TOTAL_SCALE as i64);
        assert_eq!(eval.hard_violations, 0);
        assert!(eval.terms.iter().all(|t| t.utility == 1.));
    }

    #[test]
    fn a_crossing_lowers_the_total_and_only_its_term_changes() {
        let model = QualityModel::shadow_default();
        let clean = evaluate(&Metrics::default(), &model).unwrap();
        let crossed = Metrics {
            crossings: 1,
            ..Metrics::default()
        };
        let worse = evaluate(&crossed, &model).unwrap();
        assert!(worse.total < clean.total);
        for (a, b) in clean.terms.iter().zip(&worse.terms) {
            if a.name == "routedCrossings" {
                assert_eq!(b.utility, 0.5);
            } else {
                assert_eq!(a, b);
            }
        }
    }

    #[test]
    fn overlaps_and_edge_node_hits_are_tier_one_not_weighted() {
        let model = QualityModel::shadow_default();
        let overlapping = Metrics {
            sibling_overlaps: 1,
            cross_branch_overlaps: 1,
            line_node_hits: 2,
            container_traversals: 1,
            ..Metrics::default()
        };
        let eval = evaluate(&overlapping, &model).unwrap();
        assert_eq!(eval.hard_violations, 5);
        assert_eq!(eval.total, 100 * TOTAL_SCALE as i64);
        assert!(eval.terms.iter().all(|t| t.name != "siblingOverlaps"));
    }

    #[test]
    fn a_hard_violation_outranks_any_weighted_total() {
        let model = QualityModel::shadow_default();
        let clean_but_crossed = evaluate(
            &Metrics {
                crossings: 1000,
                length: 1e6,
                ..Metrics::default()
            },
            &model,
        )
        .unwrap();
        let overlapping = evaluate(
            &Metrics {
                sibling_overlaps: 1,
                ..Metrics::default()
            },
            &model,
        )
        .unwrap();
        assert_eq!(
            tier_order(&clean_but_crossed, &overlapping),
            Ordering::Greater
        );
    }

    #[test]
    fn equal_violations_are_ordered_by_the_weighted_total() {
        let model = QualityModel::shadow_default();
        let few = evaluate(
            &Metrics {
                crossings: 1,
                ..Metrics::default()
            },
            &model,
        )
        .unwrap();
        let many = evaluate(
            &Metrics {
                crossings: 9,
                ..Metrics::default()
            },
            &model,
        )
        .unwrap();
        assert_eq!(tier_order(&few, &many), Ordering::Greater);
    }

    #[test]
    fn the_same_inputs_give_the_same_total() {
        let model = QualityModel::shadow_default();
        let m = Metrics {
            sibling_overlaps: 2,
            length: 1234.5,
            bends: 7,
            ..Metrics::default()
        };
        assert_eq!(evaluate(&m, &model).unwrap(), evaluate(&m, &model).unwrap());
    }

    #[test]
    fn a_required_clearance_makes_proximity_a_hard_violation() {
        let model = QualityModel::shadow_default();
        let near = Metrics {
            hug_pairs: 3,
            ..Metrics::default()
        };
        let preferred = evaluate_with(&near, &model, false).unwrap();
        let required = evaluate_with(&near, &model, true).unwrap();
        assert_eq!(preferred.hard_violations, 0);
        assert_eq!(required.hard_violations, 3);
        assert!(required.terms.iter().all(|t| t.name != "nearNodeSegments"));
        assert!(preferred.terms.iter().any(|t| t.name == "nearNodeSegments"));
        assert_eq!(required.total, 100 * TOTAL_SCALE as i64);
    }

    #[test]
    fn invalid_models_and_values_are_errors() {
        let mut model = QualityModel::shadow_default();
        model.terms[0].weight = -1.;
        assert!(evaluate(&Metrics::default(), &model).is_err());

        let mut model = QualityModel::shadow_default();
        model.terms.iter_mut().for_each(|t| t.weight = 0.);
        assert!(evaluate(&Metrics::default(), &model).is_err());

        let mut model = QualityModel::shadow_default();
        model.terms[4].scale = 0.;
        assert!(evaluate(&Metrics::default(), &model).is_err());

        let bad = Metrics {
            length: f64::NAN,
            ..Metrics::default()
        };
        assert!(evaluate(&bad, &QualityModel::shadow_default()).is_err());
    }
}
