# Single pass without the whole-drawing engine choice (2026-11-12)

Apple M3, macOS 26 (Darwin 25.0.0), rustc 1.99.0, Node 24.21.0; single observations, not a measurement series.

**Finding.** A user's report (an external evaluation, data not in the repository) toggled a container in a graph whose
edges mostly cross container boundaries: 150 visible nodes, 1 000 edges. The toggle took 42 s. A profile
(`sample` on `layout-command`) put it all in `engine_choice::improve`: after single pass and the reroute of the
conflicting routes, 6 routes still ran through nodes, and `improve` routed the whole drawing again with both routing
engines and then ran the side search. It found nothing better (6 conflicts before and after).

**Change.** Under single pass (the default since 0.2), a drawing whose conflicting routes are still in conflict after
their reroute is no longer routed again as a whole by both engines; the conflicts are reported, as ADR 0016 has single
pass construct once without later correction. The pipeline before single pass keeps the engine choice.

**Output.** Unchanged on the measured views: bench from `main` and this branch agree on all 36 rows of the
fingerprint views (20 views, threads 1 and 4), and the 794 tests pass unchanged. The report's six ELK graphs (36–500
nodes) give the same bounding boxes and crossings before and after.

**Time** (WASM in Node, the report's toggle sequence: the graph cut at the second level of its hierarchy, collapsed
from depth 0, then one container opened):

| edges | visible routes after the toggle | before | after |
|---|---|---|---|
| 0 | 0 | 6 ms | 6 ms |
| 1 000 | 481 | 42 446 ms | 162 ms |
| 3 000 | 534 | 53 400 ms | 188 ms |
| 9 598 | 263 | 5 395 ms | 82 ms |

The routes that remain in conflict are the same 6 either way.
