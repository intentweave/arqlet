# Toggle layout of abstracted views (2026-11-12)

Apple M3 (8 cores), Darwin 25.0.0, rustc 1.99.0, Node 24.21.0.

**Finding.** Step 2 of ADR 0019 ([results](protocol-20261112.md)) showed a toggle at 50 000 nodes taking 9.7 s natively.
A profile (`sample` on `layout-command`) put almost all of it in the completing router (`arqlet_routing::shares_probe`,
`crosses_points` under `best_route`/`improve_raw`), which compares every candidate route with all others. The local
layout that a toggle used for views with groups, communities or an initial collapse (introduced with communities;
before, only a node budget used it) handed it hundreds of routes that single pass could not construct.

**Comparison** (a probe that lays out the same toggled view three ways, native, single run each, revision 088596f plus
the probe; same graphs and views as step 2):

| graph | container opened | local (as before) | warm start, all placed again | from scratch |
|---|---|---|---|---|
| 10 000 | `d101` (2 children) | 0.32 s; 24 delegated, 94 unresolved pairs; 70 of 105 boxes moved | 0.18 s; 0 / 0; 105 moved | 0.23 s; 0 / 0; all moved |
| 10 000 | `f420` (400 children) | 7.64 s; 292 / 1 298; 79 moved | 1.05 s; 57 / 157; 105 moved | 0.62 s; 2 / 3 |
| 50 000 | `d117` (22 children) | 9.56 s; 484 / 2 140; 220 of 291 moved | 0.69 s; 1 / 0; 290 moved | 1.44 s; 19 / 41 |
| 100 000 | `d101` (1 child) | 1.69 s; 24 / 61; 286 of 372 moved | 0.93 s; 0 / 0; 340 moved | 0.85 s; 0 / 0 |
| 100 000 | `f1234` (134 children) | 4.52 s; 196 / 753; 291 moved | 1.39 s; 18 / 61; 357 moved | 0.89 s; 0 / 0 |

("delegated": routes single pass handed to the completing router; "unresolved pairs": strict track pairs it left.)
The local layout was the slowest and the least clean in every case, and it kept few boxes in place anyway.

**Change.** Toggles and reveals of abstracted views lay the view out from scratch, as before communities existed; only
a node budget keeps the local layout. The drawing of a view then depends only on the graph and the view (test
`opening_and_closing_a_group_returns_to_the_drawing_of_the_import`).

**Result** ([raw](protocol-20261112T204745Z.jsonl), `benchmarks/protocol/measure.py --sizes 10000 50000 100000 --reps 5`,
revision 1e4fd7c, clean, load 2.34 at the start and 2.15–2.37 during the runs; medians of 5):

| nodes | toggle, native `handle` | toggle, WASM `lw_run` | browser path, toggle + scene: snapshot sent / resident |
|---|---|---|---|
| 10 000 | 342 → 279 ms | 556 → 448 ms | 687 / 378 ms |
| 50 000 | 9 689 → 1 564 ms | 15 276 → 2 292 ms | 2 785 / 2 204 ms |
| 100 000 | 1 831 → 965 ms | 2 694 → 1 362 ms | 1 802 / 1 279 ms |

(Before: step 2, revision 62e90bd.)
