# Candidate-track trace for the detoured edge (2026-10-23)

Edge `import:59e929ff687129ae` in intentweave-export 100 (the largest detour, +2,439 units at clearance 9). Trace: a temporary print in `best_route` (routing crate), not committed; the first search of each run.

## What the trace shows

- The first search evaluates 289 candidate routes in both runs (default and clearance 9); the candidate generator yields the same number.
- Default: 51 candidates have zero obstacle hits, and the search picks one of them (score 0 hits, 2 shares, 22 crossings).
- Clearance 9: no candidate has zero hits (the minimum is one). The search must pick a candidate with one hit, and it picks one that shares more and crosses more (1 hit, 7 shares, 19 crossings) and then detours.
- Corridor centres on (`routingCorridors`) do not add a zero-hit candidate for this edge: still none.
- The default's zero-hit tracks lie 12 units from the nodes. The clearance-9 candidate tracks lie 15 units from them (the offset of 6 beyond the inflated band). Tracks between 9 and 15 units are not generated, so the tight passages that the default uses are absent from the candidate set.

## Offsets tested (corpus, clearance 9)

| Candidate track offset beyond the clearance band | Routed crossings | Route length | Contacts at 12 |
| --- | --- | --- | --- |
| 6 (current; 15 from nodes) | 9,543 | 1,682,816 | 0 |
| 3 (12 from nodes) | 9,933 | 1,742,808 | 0 |
| 1 (10 from nodes) | 9,918 | 1,742,262 | 0 |

Raw rows: `candidate-trace-rail3-20261023.jsonl`, `candidate-trace-rail1-20261023.jsonl`.

## Reading

- The mechanism is confirmed: under clearance 9 the first search has no obstacle-free track in its candidate set, whereas the default has 51. The candidate set, not the scoring, forces the detour.
- Yet offering the tight tracks (offset 1 or 3) makes the corpus worse, not better: more crossings and longer routes. The legacy search scores candidates only on hits, shares, crossings and length; it has no term for proximity, so a track close to a node is chosen as readily as a far one. The trade-off between proximity and crossings therefore cannot be made in the candidate generator.
- The corpus result suggests that the fix belongs in the search's scoring, not in the candidate set. That is a design change: a proximity term in `best_route`'s score (within the required clearance it is a hit; within the preferred margin it is a cost), which changes which candidate is chosen even without a requirement, so it needs the default-output check.

## Decision needed

Whether to add a proximity term to the legacy track search's score. It is the first change in this thread that targets the cause, and it changes routing output for views with a preferred or required margin.
