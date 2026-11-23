# Route repair through narrow gaps, and a faster overlap resolution (#156), 2026-11-23

Machine: Apple M3 (MacBook), macOS, loaded during the runs (load average 8 to 15). Revision: `1a21a6f` (main after
#154) with the engine choice work limit of #157, plus this change. Build: `cargo build --release -p arqlet-runtime --bin
layout-command --locked`; before = the same tree with `crates/routing/src/lib.rs` as on main. Inputs:
`node benchmarks/experiments/bus-mesh.mjs UNITS BUSES organic lines CHILDREN EDGES` (seeded, public; BUSES 0 = no bus),
one import each, 3 runs, wall time. "Routes through boxes": routes of the final drawing that cross a box other than their
ends and the ends' ancestors (axis-parallel segment test against the absolute boxes).

| Input | Before: routes through boxes (box hits), time (s) | After: routes through boxes (box hits), time (s) |
| --- | --- | --- |
| 100 units, 0 buses, 60 children, 250 edges | 54 of 250 (583), 0.88 0.24 0.23 | 0 (0), 0.90 0.28 0.28 |
| 100 units, 30 buses, 60, 250 | 23 of 409 (146), 0.51 0.50 0.50 | 21 (139), 0.52 0.52 0.52 |
| 100 units, 30 buses, 120, 500 | 192 of 659 (1 149), 0.46 0.47 0.46 | 24 (101), 3.45 3.46 3.46 |
| 100 units, 30 buses, 240, 1 000 | 518 of 1 159 (3 524), 1.17 1.16 1.20 | 18 (163), 16.27 16.19 16.23 |

Cause: the repair's grid search starts each end with a stub of the own clearance (12) toward the route; where the end's
node faces a neighbour closer than that (organic packing leaves such gaps), the stub was blocked and the route kept its
straight shape through the boxes between. The repair now tries shorter stubs (6, 3), toward the route and straight out of
the port's side.

The rescued routes run through the narrow gaps and share segments there, which made the overlap resolution
(`resolve_overlaps`, `pair_move`) explode: 120 children took 117 s with the stub change alone (95 s of it in the overlap
resolution, 85 s in `pair_move`). Pure speedups, each checked byte-identical on these inputs (hashes of boxes and routes)
and on the default fingerprints: a move's overlap count once per move instead of once per pair of moves, the routes'
bounds kept instead of recomputed, the obstacle outlines once per route, a route without a move not searched again until
a route moves near it, the candidate moves cached per route and segment, and a segment index by line instead of comparing
every route (`SegmentIndex`, tested against `shares_segment` on random routes). 117 s became 3.5 s.

Fingerprints: the 15 default views (`fingerprints-20261122-default-0.8.0.jsonl`, release bench, threads 1 and 4) are
identical; `layerweave-code` (regenerated dataset) is identical with and without this change on the current dataset.
Timing of the default views (bench, 5 runs, synthetic-2k at limit 100): import 69.8 ms before, 67.0 ms after; refine
10.3 ms before, 10.0 ms after.

Open: the dense cases still cost 7 to 14 times more than before (the overlaps are real now); a meshed view of 480
children and 2 100 edges still takes about 90 s (the overlap repair of the placement, `repair::greedy`). Next: wider gaps
in the organic placement, behind an option.
