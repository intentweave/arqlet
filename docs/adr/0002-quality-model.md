# ADR 0002: Constraint tier, then score, for candidate acceptance

Status: proposed. Tier decisions recorded below; calibration and candidate-level evidence still open, so the rule is not yet accepted.

Context: candidates are accepted today by a lexicographic integer key (`layout::published_key`, `conflict_key`). Its first elements are sibling and cross-branch overlaps, edge-node hits and container traversals, so these are absolute priorities only by ordering. A parallel `Quality` / `better_by` ordering also exists and is used by tests and reporting. [recursive-optimization.md](../recursive-optimization.md) §9 proposes a weighted score, which would make those priorities tradeable.

Decision (proposed):

1. Tier 1 is a hard constraint set: finite complete geometry and valid containment (`scene::validate_geometry`), plus any requirement-policy rule a request declares mandatory. Only tier-1-valid candidates are compared.
2. Tier 2 is the score. Decided (2026-10-23):
   - Node overlaps and edge-node hits (including container traversals) are tier 1. They are never traded against the score.
   - Routed crossings are a weighted tier-2 term, and not a hard constraint: they cannot always be avoided.
   - Near-node proximity is tier 1 when a required clearance is set for the request, for the edges' own nodes as well as foreign nodes (ADR 0005). Without a required clearance it is a tier-2 term with a weight below crossings.
   - Unintended shared edge segments (overlap beyond tolerance, outside a declared bundle or bus) are tier 1, like node overlaps. Crossings at points and shared endpoints are not overlap.
   - Reason: the disagreements with the published key show that the layout needs more space in general, not that crossings should be traded for proximity. The lever is a required margin and space requests (M3), not weight tuning.
   - Each later change of tier needs an explicit requirement decision recorded here.

Tier 1 is a publication rule for a valid result: a published result has zero tier-1 violations, as defined in point 1. Intermediate repair states may carry violations, but they are never published. A result that cannot reach zero is published as an unresolved conflict with its diagnostics, never as an optimized result. Hard constraints are not traded against the score.
3. The score uses fixed quantisation and a fixed reduction order. Native and WASM must produce identical acceptance decisions.
4. The default remains the current lexicographic key until M0 shadow results justify a change. Changing the default requires a documented requirement-policy change and fresh fingerprint baselines, not a weight edit.

Consequences:

- One evaluator. `Quality` / `better_by` is folded into the shared evaluator or retired.
- New options are added with default-omitted serialization so existing `options_key` values and stored fingerprints remain valid.
- Any change to acceptance needs a before/after sweep on the same machine, with `benchmarks/fingerprint_compare.py` output recorded.

## Objective order

The published key ranks near-node pairs before routed crossings. The decision above asks for the reverse order. Measured on the 20 benchmark views with only that swap (`benchmarks/results/objective-swap-20261023.jsonl`, option override, defaults unchanged):

- Routed crossings: 30 fewer in total. Six views improve (`er-diagram 100` 3 to 1, `petri-net 100` 55 to 52, among others); none gets worse.
- Route length: net change of -313, mixed per view.
- Bends and shared-route pairs: essentially unchanged.
- Every refined snapshot differs in its serialized options, so the fingerprints all change. The geometry effects above are what matter.

Accepted (2026-10-23): the default order now ranks routed crossings before near-node pairs. New baseline: `benchmarks/results/fingerprints-20261023-objective-order.jsonl`; summary in `benchmarks/results/objective-order-default-20261023.md` (6 of 20 views change geometry).
