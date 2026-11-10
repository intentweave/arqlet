# Required clearance on the benchmark corpus (2026-10-23)

Runs: `bench --threads 1 --reps 1 --options '{"requiredClearance": C}'` on the 20 views, C = 6 and 12. Raw rows: `required-clearance-6-20261023.jsonl`, `required-clearance-12-20261023.jsonl`. Default reference: `fingerprints-20261023-objective-order.jsonl` (default order), same machine class.

| Clearance | Views where no candidate qualified (input kept) | Routed crossings vs default | Route length vs default |
| --- | --- | --- | --- |
| 6 | 19 of 20 | -788 (the input is kept, so this is the unrefined input) | +81,932 |
| 12 | 20 of 20 | -787 | +81,923 |

Reading: a required clearance enforced only by routing does not make the refinement better; it makes it a no-op. The routing-only candidates cannot keep every route outside the clearance of every foreign node, because the boxes are packed closer than the clearance. The kept input is unrefined, so its crossings and length are the unoptimized values, which is why the crossing totals look better while the length grows.

Consequence: a required clearance needs space in placement (node and container spacing reserved for the clearance), not only a routing gate. That is the space-request mechanism of M3 (`docs/recursive-optimization.md` §10). Until it exists, the option stays unset by default and should be used only with the expectation that the refinement keeps the input.
