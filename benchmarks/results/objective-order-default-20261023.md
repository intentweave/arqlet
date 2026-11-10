# Default objective order: near-node pairs after routed crossings (2026-10-23)

Decision: ADR 0002 (objective order). The default order changes from `..., sharedRoutePairs, nearNodeSegments, routedCrossings, ...` to `..., sharedRoutePairs, routedCrossings, nearNodeSegments, ...`.

Baseline: `fingerprints-20261023T103351Z`-era default (previous main), recorded as `fp-margin-default` in the working session; new reference: `fingerprints-20261023-objective-order.jsonl` (20 views, one thread, one repetition, machine and revision in each row).

Result against the previous default (`benchmarks/fingerprint_compare.py`):

- Identical: 14 views.
- Geometry changed: 6 views (activity-order 100, state-tcp 100, er-diagram 100, network-vlan 100, petri-net 100, synthetic-2k 50).
- Routed crossings: 30 fewer in total; no view gets more.
- Route length: net -313 (mixed per view).
- Bends: net 0 in total, but up on some views (for example er-diagram 100, 44 to 47) and down on others (petri-net 100, 126 to 116).

Tests: all 37 test groups pass. One test fixture (session publications) needed at least two intermediate publications; it now pins the previous order explicitly, so the session tests do not depend on the objective order.
