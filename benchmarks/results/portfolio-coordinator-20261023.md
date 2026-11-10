# Portfolio coordinator against the specification (2026-10-23)

Specification: `docs/recursive-optimization.md` §13 M1 acceptance: complete candidates, bounded archive, staged refinement, shared acceptance everywhere, deterministic ties, progressive best score, cancellation keeping a complete result.

| Acceptance item | Status | Where |
| --- | --- | --- |
| Complete candidates | done | every job returns a complete snapshot (`evaluate_against_until`) |
| Bounded archive | **done now**: kept during the reduction and carried in every streamed publication (version 3) | `Archive`, `ARCHIVE_CAPACITY`, `Publication.archive` (before: only in `bench` and tests) |
| Staged refinement | done | first stage over the input, global routing rounds on the incumbent |
| Shared acceptance | done | `layout::improves` at every site |
| Deterministic ties | done | fixed reduction order; archive ordered by key, then candidate index |
| Progressive best score | done | publications strictly decrease in key |
| Cancellation keeps a complete result | done | refinement and placement search; the legacy routing-only job is now interruptible too |

Remaining M1 gaps: first-stage planning is still gated by options rather than by the capability declarations (P05); no cost-aware allocation (no deterministic cost per view beyond the legacy counters); no publication-driven selection (needs an ADR).

## The smallest missing chain, and what it changed

The chain candidate → bounded archive → streamed publication was missing: the archive existed as a function over the log. It is now kept during the reduction, with the same rule as `archive_indices`. A test checks that the archive kept during a run equals the archive of the log, entry for entry. Cost: a fingerprint per measured candidate; a single timing showed about +7 percent refinement time (not a benchmark claim).

## Bottleneck comparison

Final result by source, 20 views (candidate logs): routing-only supplies the final result on 13 views, placement on 7.

On several routing-only views, the best ordering candidate has fewer routed crossings than the final result: deployment-cluster 143 against 196, layerweave-code 100 at 466 against 655, petri-net 39 against 58, network-vlan 62 against 83, synthetic-2k 50 at 882 against 992. The lexicographic key decides these with shared-route pairs, which come before routed crossings: the ordering candidates have 21, 61, 0 and 2 shared pairs against 16, 43, 0 and 0 for the routed final result. So the ordering candidates are not routed, and their shared routes are a routing artefact. The routing rounds only re-route the incumbent, so an ordering candidate never gets a routing pass unless it wins first.

## The targeted change: routed ordering candidates (opt-in)

`routedCandidates` (default off) adds one job per ordering mode: the ordering candidate is re-routed (`reroute_counted`) before the key compares it. Measured on the 20 views against the same run without it:

- Crossings: 9,495 → 9,553 (+58). Length: 1,623,737 → 1,631,001 (+7,264). Eight views change, the routed ordering wins seven of them, and the others get worse.
- So the chain works end to end, but on this corpus a routing pass on ordering candidates does not pay off as a default. It stays opt-in (`portfolio-routed-candidates-20261023.jsonl`, `portfolio-default-20261023.jsonl`).

## Reading

The bottleneck is real: ordering candidates lose on shared routes they were never routed for. But re-routing them only moves the trade-off between shared pairs and crossings, and the corpus shows no consistent gain. The next routing change should target shared-route pairs directly, at the level of the track search, and it needs its own before/after sweep.
