# Where the refinement time of the layered construction goes, 2026-10-31

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release. Revision f5675c1 (option `layeredConstruction`), layerweave-code graph
of 0bcfd56 (669 / 1084), limit 100. Sources: `bench --repair-work 1` and the bench's stage times and candidate work (one
run each), temporary timers around the parts of placement, routing and construction (removed again), `sample` profiles,
and a timing of `refine_snapshot` against `refine_snapshot_job` (three runs, the fastest). The machine was not idle; the
numbers are shares of one run, not benchmark results.

## layerweave-code (refinement 2.6–2.7 s)

| part | time | note |
|---|---:|---|
| routing stage | 1786 ms | 16 constructions (4 ordering candidates, each with routing and widening rounds) |
| – construction (`construct_planned`) | 1154 ms | 72 ms per call |
| – – building with the crossing-aware search (`build_edge`, `search_on`) | 765 ms | |
| – – planning the ways over the gaps' centres (`lane_changes`) | 241 ms | for every prepared edge |
| – – port reordering, corridor order and planning again | 130 ms | |
| – – preparation and sections | 15 ms | |
| – general router for the delegated edges (`improve_until_counted_with_fixed`) | 347 ms | |
| – initial routes | 11 ms | |
| – rest of the routing stage | ~270 ms | port resolution and the rounds around the construction |
| placement stage | 448 ms | |
| – improved rank order (`improve_order`) | 290 ms | 432 calls (every area in every placement) |
| – alignment and channel widths | 22 ms | |
| scoring | 125 ms | |

Grid pops of the refinement: 604,379. The 18 edges delegated as `TrackTaken` take 281,323 (47 %); e922 alone 196,059, e915
191,772 (constructed in the final drawing, delegated in other candidates). The profile's top entries are the constructor's
search (`search_on` and its index queries), string comparisons (`memcmp`, lookups by id), and the general router
(`shares_probe`, `crosses_points`, `best_route`).

## The Optimize step of the job

`refine_snapshot_job` (the bench, the playground's background job) runs `relieve_crossings` after the refinement core; it is
outside the stage times. Its cost: cloud-architecture 155 ms (core 76 ms, job 232 ms; it changes the routes), er-diagram 24 ms
(changes the routes), layerweave-code 16 ms, all others 0–3 ms.

## Other datasets (refinement core)

deployment-cluster 179 ms (routing 124 ms: construction 62, general router 29), cloud-architecture 76 ms (construction 38 ms,
no grid search), petri-net 45 ms, activity-order 107 ms, nested-tree-radial 134 ms (not layered).

## Candidates for a shorter refinement (not yet measured)

1. The constructor searches a failed edge again without the built routes, only to name the reason (`TrackTaken` or
   `NoFreeTrack`); the reason does not change the drawing. Every failed edge costs one full search more.
2. Every search builds its own index over the obstacles (all boxes but the edge's ends) and over the built segments; one index
   per construction, with the ends filtered at the query, gives the same answers.
3. The ways over the gaps' centres are planned for every edge, also for those built from sections, where only the port
   reordering and the corridor order use them.
4. The 16 constructions per refinement repeat their work for every routing and widening round of a candidate.
5. A few edges dominate the grid search (e922, e915); why they fail the construction is open.
6. `relieve_crossings` costs two thirds of cloud-architecture's job.
