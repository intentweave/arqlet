use std::collections::BTreeSet;

const SEPARATION_EPSILON: f64 = 1e-7;
const SOLVER_TOLERANCE: f64 = 1e-8;
const MAX_PROJECTION_SWEEPS: usize = 4096;
const MAX_PROJECTION_WORK: usize = 8_000_000;
pub(crate) const MAX_CONSTRAINT_ROUNDS: usize = 16;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AxisBounds {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct PositionBounds {
    pub x: AxisBounds,
    pub y: AxisBounds,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    X,
    Y,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Separation {
    pub axis: Axis,
    pub before: usize,
    pub after: usize,
    pub distance: f64,
}

#[derive(Clone, Copy)]
struct Term {
    variable: usize,
    coefficient: f64,
}

#[derive(Clone, Copy)]
struct Inequality {
    terms: [Term; 2],
    term_count: usize,
    lower: f64,
    norm_squared: f64,
}

/// Finds rectangle overlaps by sweeping the left edges in stable order.
pub(crate) fn scanline_pairs(
    positions: &[(f64, f64)],
    extents: &[(f64, f64)],
    gap: f64,
) -> Option<Vec<(usize, usize)>> {
    if positions.len() != extents.len()
        || !gap.is_finite()
        || gap < 0.
        || positions
            .iter()
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
        || extents
            .iter()
            .any(|(w, h)| !w.is_finite() || !h.is_finite() || *w < 0. || *h < 0.)
    {
        return None;
    }

    let mut order: Vec<usize> = (0..positions.len()).collect();
    order.sort_by(|&a, &b| {
        let left_a = positions[a].0 - extents[a].0 / 2.;
        let left_b = positions[b].0 - extents[b].0 / 2.;
        left_a.total_cmp(&left_b).then_with(|| a.cmp(&b))
    });

    let mut active: Vec<usize> = Vec::new();
    let mut pairs = BTreeSet::new();
    for j in order {
        let left = positions[j].0 - extents[j].0 / 2.;
        active.retain(|&i| positions[i].0 + extents[i].0 / 2. + gap >= left);
        for &i in &active {
            let overlap_x =
                (extents[i].0 + extents[j].0) / 2. + gap - (positions[j].0 - positions[i].0).abs();
            let overlap_y =
                (extents[i].1 + extents[j].1) / 2. + gap - (positions[j].1 - positions[i].1).abs();
            if overlap_x > 0. && overlap_y > 0. {
                pairs.insert((i.min(j), i.max(j)));
            }
        }
        active.push(j);
    }
    Some(pairs.into_iter().collect())
}

/// Chooses the smaller-overlap axis and orients its constraint by the target order.
pub(crate) fn constraint_for_pair(
    a: usize,
    b: usize,
    targets: &[(f64, f64)],
    current: &[(f64, f64)],
    extents: &[(f64, f64)],
    gap: f64,
) -> Option<Separation> {
    if a == b
        || a >= targets.len()
        || b >= targets.len()
        || targets.len() != current.len()
        || targets.len() != extents.len()
        || !gap.is_finite()
        || gap < 0.
        || targets
            .iter()
            .chain(current)
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
        || extents
            .iter()
            .any(|(w, h)| !w.is_finite() || !h.is_finite() || *w < 0. || *h < 0.)
    {
        return None;
    }

    let (wa, ha) = extents[a];
    let (wb, hb) = extents[b];
    let overlap_x = (wa + wb) / 2. + gap - (current[b].0 - current[a].0).abs();
    let overlap_y = (ha + hb) / 2. + gap - (current[b].1 - current[a].1).abs();
    if overlap_x <= 0. || overlap_y <= 0. {
        return None;
    }

    let axis = if overlap_x <= overlap_y {
        Axis::X
    } else {
        Axis::Y
    };
    let (a_target, b_target) = match axis {
        Axis::X => (targets[a].0, targets[b].0),
        Axis::Y => (targets[a].1, targets[b].1),
    };
    let (before, after) = if a_target < b_target || (a_target == b_target && a < b) {
        (a, b)
    } else {
        (b, a)
    };
    let distance = match axis {
        Axis::X => (wa + wb) / 2.,
        Axis::Y => (ha + hb) / 2.,
    } + gap
        + SEPARATION_EPSILON;
    distance.is_finite().then_some(Separation {
        axis,
        before,
        after,
        distance,
    })
}

/// Projects the target positions onto the requested separation and bound constraints.
pub(crate) fn solve(
    targets: &[(f64, f64)],
    movable: &[bool],
    bounds: &[PositionBounds],
    separations: &[Separation],
) -> Option<Vec<(f64, f64)>> {
    if targets.len() != movable.len()
        || targets.len() != bounds.len()
        || targets
            .iter()
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return None;
    }
    let x_targets: Vec<f64> = targets.iter().map(|p| p.0).collect();
    let y_targets: Vec<f64> = targets.iter().map(|p| p.1).collect();
    let x_bounds: Vec<AxisBounds> = bounds.iter().map(|b| b.x).collect();
    let y_bounds: Vec<AxisBounds> = bounds.iter().map(|b| b.y).collect();
    let x = project_axis(&x_targets, movable, &x_bounds, separations, Axis::X)?;
    let y = project_axis(&y_targets, movable, &y_bounds, separations, Axis::Y)?;
    Some((0..targets.len()).map(|i| (x[i], y[i])).collect())
}

fn project_axis(
    targets: &[f64],
    movable: &[bool],
    bounds: &[AxisBounds],
    separations: &[Separation],
    axis: Axis,
) -> Option<Vec<f64>> {
    let mut free_index = vec![None; targets.len()];
    let mut free_nodes = Vec::new();
    for (node, &can_move) in movable.iter().enumerate() {
        if can_move {
            free_index[node] = Some(free_nodes.len());
            free_nodes.push(node);
        }
    }
    let mut inequalities = Vec::new();
    for separation in separations.iter().filter(|c| c.axis == axis) {
        if separation.before >= targets.len()
            || separation.after >= targets.len()
            || separation.before == separation.after
            || !separation.distance.is_finite()
        {
            return None;
        }
        match (free_index[separation.before], free_index[separation.after]) {
            (Some(before), Some(after)) => {
                inequalities.push(inequality(
                    &[
                        Term {
                            variable: before,
                            coefficient: -1.,
                        },
                        Term {
                            variable: after,
                            coefficient: 1.,
                        },
                    ],
                    separation.distance,
                )?);
            }
            (None, Some(after)) => {
                inequalities.push(inequality(
                    &[Term {
                        variable: after,
                        coefficient: 1.,
                    }],
                    separation.distance + targets[separation.before],
                )?);
            }
            (Some(before), None) => {
                inequalities.push(inequality(
                    &[Term {
                        variable: before,
                        coefficient: -1.,
                    }],
                    separation.distance - targets[separation.after],
                )?);
            }
            (None, None) => {
                if targets[separation.after] - targets[separation.before]
                    < separation.distance - SOLVER_TOLERANCE
                {
                    return None;
                }
            }
        }
    }

    for (node, &variable) in free_index.iter().enumerate() {
        let Some(variable) = variable else {
            continue;
        };
        let range = bounds[node];
        if range.min.is_some_and(|v| !v.is_finite())
            || range.max.is_some_and(|v| !v.is_finite())
            || range.min.zip(range.max).is_some_and(|(min, max)| min > max)
        {
            return None;
        }
        if let Some(min) = range.min {
            inequalities.push(inequality(
                &[Term {
                    variable,
                    coefficient: 1.,
                }],
                min,
            )?);
        }
        if let Some(max) = range.max {
            inequalities.push(inequality(
                &[Term {
                    variable,
                    coefficient: -1.,
                }],
                -max,
            )?);
        }
    }

    if inequalities.is_empty() {
        return Some(targets.to_vec());
    }
    let mut values: Vec<f64> = free_nodes.iter().map(|&i| targets[i]).collect();
    let mut corrections = vec![0.; inequalities.len()];
    let max_sweeps = (MAX_PROJECTION_WORK / inequalities.len()).clamp(1, MAX_PROJECTION_SWEEPS);
    let mut converged = false;
    for _ in 0..max_sweeps {
        let previous = values.clone();
        for (index, constraint) in inequalities.iter().enumerate() {
            let correction = corrections[index];
            let dot = constraint.terms[..constraint.term_count]
                .iter()
                .map(|term| values[term.variable] * term.coefficient)
                .sum::<f64>();
            let shifted_dot = dot - correction * constraint.norm_squared;
            let next_correction =
                ((constraint.lower - shifted_dot) / constraint.norm_squared).max(0.);
            let delta = next_correction - correction;
            for term in &constraint.terms[..constraint.term_count] {
                values[term.variable] += delta * term.coefficient;
            }
            corrections[index] = next_correction;
        }
        if values.iter().any(|value| !value.is_finite()) {
            return None;
        }
        let max_change = values
            .iter()
            .zip(&previous)
            .map(|(value, old)| (value - old).abs())
            .fold(0., f64::max);
        let max_violation = inequalities
            .iter()
            .map(|constraint| {
                let dot = constraint.terms[..constraint.term_count]
                    .iter()
                    .map(|term| values[term.variable] * term.coefficient)
                    .sum::<f64>();
                (constraint.lower - dot).max(0.)
            })
            .fold(0., f64::max);
        if max_change <= SOLVER_TOLERANCE && max_violation <= SOLVER_TOLERANCE {
            converged = true;
            break;
        }
    }
    if !converged {
        return None;
    }

    let mut result = targets.to_vec();
    for (index, &node) in free_nodes.iter().enumerate() {
        result[node] = values[index];
    }
    Some(result)
}

fn inequality(terms: &[Term], lower: f64) -> Option<Inequality> {
    if terms.is_empty() || terms.len() > 2 || !lower.is_finite() {
        return None;
    }
    let mut stored = [Term {
        variable: 0,
        coefficient: 0.,
    }; 2];
    let mut norm_squared = 0.;
    for (index, term) in terms.iter().copied().enumerate() {
        if !term.coefficient.is_finite() {
            return None;
        }
        stored[index] = term;
        norm_squared += term.coefficient * term.coefficient;
    }
    (norm_squared > 0.).then_some(Inequality {
        terms: stored,
        term_count: terms.len(),
        lower,
        norm_squared,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanline_enumerates_only_clearance_overlaps() {
        let positions = [(0., 0.), (20., 0.), (0., 100.)];
        let extents = [(40., 40.); 3];
        assert_eq!(
            scanline_pairs(&positions, &extents, 24.),
            Some(vec![(0, 1)])
        );
    }

    #[test]
    fn projection_minimizes_displacement_for_one_separation() {
        let positions = [(0., 0.), (20., 0.)];
        let result = solve(
            &positions,
            &[true, true],
            &[PositionBounds::default(); 2],
            &[Separation {
                axis: Axis::X,
                before: 0,
                after: 1,
                distance: 64.,
            }],
        )
        .unwrap();
        assert!((result[0].0 + 22.).abs() < 1e-7, "{result:?}");
        assert!((result[1].0 - 42.).abs() < 1e-7, "{result:?}");
        assert_eq!((result[0].1, result[1].1), (0., 0.));
    }

    #[test]
    fn projection_keeps_fixed_nodes_and_honours_bounds() {
        let positions = [(0., 0.), (20., 0.)];
        let result = solve(
            &positions,
            &[false, true],
            &[
                PositionBounds::default(),
                PositionBounds {
                    x: AxisBounds {
                        min: Some(70.),
                        max: Some(80.),
                    },
                    y: AxisBounds::default(),
                },
            ],
            &[Separation {
                axis: Axis::X,
                before: 0,
                after: 1,
                distance: 64.,
            }],
        )
        .unwrap();
        assert_eq!(result[0], positions[0]);
        assert!((result[1].0 - 70.).abs() < 1e-7, "{result:?}");
    }

    #[test]
    fn infeasible_pinned_constraints_are_reported() {
        let positions = [(0., 0.), (20., 0.)];
        let result = solve(
            &positions,
            &[false, false],
            &[PositionBounds::default(); 2],
            &[Separation {
                axis: Axis::X,
                before: 0,
                after: 1,
                distance: 64.,
            }],
        );
        assert!(result.is_none());
    }
}
