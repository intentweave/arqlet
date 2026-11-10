# Clearance-aware placement and routing (2026-10-23)

Runs: `bench --threads 1 --reps 1` on the 20 views, default options and `requiredClearance` 6 and 12. Raw rows: `clearance-placement-default-20261023.jsonl` (default, identical to the objective-order reference), `clearance-placement-6-20261023.jsonl`, `clearance-placement-12-20261023.jsonl`.

Change: with a required clearance, (a) placement keeps gaps between siblings and levels at least `2 x clearance + track spacing`, (b) routing keeps routes a track outside the clearance (both the channel router margin and the legacy router obstacles). Default options are untouched.

| Clearance | Near-node pairs, input and published (all 20 views) | Views where the input was kept | Routing issues | Routed crossings vs default | Route length vs default |
| --- | --- | --- | --- | --- | --- |
| none (default) | not constrained | - | 0 | - | - |
| 6 | 0 and 0 | 0 of 20 | 0 | +1,000 (9,228 total) | +92,806 (1,553,316 total) |
| 12 | 0 and 0 | 0 of 20 | 0 | +1,072 (9,228 total) | +192,844 (1,553,316 total) |

Reading: the clearance is now met everywhere and the refinement improves on the input in every view. The cost is measurable: about 11 percent more routed crossings and 6 to 12 percent more route length. The earlier result (`required-clearance-20261023.md`, routing only) is superseded: without the placement space, no candidate qualified on any view.

Not measured: the effect on quality beyond these metrics, and the placement cost on very dense views beyond the 20 corpus views.
