# Required clearance sweep: contacts at the preferred margin versus cost (2026-10-23)

Runs: `bench --threads 1 --reps 1` on the 20 views, default options and `requiredClearance` in {3, 6, 7, 8, 9, 12, 18}. Raw rows: `clearance-sweep-<value>-20261023.jsonl`. Each row has `publishedHugPairsAtPreferred`: near-node pairs of the result measured at the preferred margin (12), whatever clearance was required. Routes are the same for both measures.

| Required clearance | Contacts at 12 (20 views) | Routed crossings vs default | Route length vs default |
| --- | --- | --- | --- |
| none (default) | 2,567 | (9,256 total) | (1,561,247 total) |
| 3 | 1,610 | +659 | -6,063 |
| 6 | 1,321 | +1,029 | +92,907 |
| 7 | 0 | +360 | +115,955 |
| 8 | 0 | +337 | +124,990 |
| 9 | 0 | +456 | +128,048 |
| 12 | 0 | +1,164 | +191,836 |
| 18 | 0 | +425 | +283,612 |

Reading:
- The minimal clearance that removes all contacts at the preferred margin is 7. At 6 the routes lie exactly on the 12-unit boundary and still count as contacts. The routing margin is clearance + track spacing (6), so the threshold is 12 - 6 = 6, plus any positive amount.
- Crossing cost is about +4 to +13 percent over the sweep and is not monotone in the clearance: the search is noisy at this scale, so single values should not be ranked finely. Length cost grows with the clearance.
- The space is reserved in placement (gaps of 2 x clearance + track spacing); without it no candidate met the clearance (see `clearance-placement-20261023.md`).
- Which margin is required is a requirement decision. This sweep does not choose it.
