# Corridor plan of organic areas as a measurement (#156, ADR 0020 step C1), 2026-11-26

**Not merged.** The option, the report and `organic_corridors.py` are not on `main`: branch `organic-corridor-plan`,
commit 6f8277e (pull request #202). The router patch is in `benchmarks/experiments/`. Decision and summary:
[ADR 0020](../../docs/adr/0020-single-pass-for-every-area.md), "C1 outcome".

Machine: Apple M3 (8 cores, 16 GB), macOS 26.0. Revision: `84d87af` (branch `organic-corridor-plan`: main `70d744b`
plus the report). Build: `cargo build --release -p arqlet-runtime --bin layout-command --locked` (rustc 1.99.0, thin
LTO); Node 24.21.0; Python 3.13. Load: other agents built on the machine during the runs (1-minute load 3 to 20). Route
counts, sizes and the drawings do not depend on it; times have a section of their own.

Commands (the views are in the script; `DIR` is any scratch directory):

```sh
python3 benchmarks/generate.py --no-manifest
python3 benchmarks/experiments/organic_corridors.py --work DIR --out FILE.jsonl --costs 0,1,6,24 --views NAME,...
python3 benchmarks/experiments/organic_corridors.py --summarize benchmarks/results/organic-corridor-plan-20261126.jsonl
```

Records: [`.jsonl`](organic-corridor-plan-20261126.jsonl) (one per view and route cost),
[`-router.jsonl`](organic-corridor-plan-20261126-router.jsonl) (why routes end up through boxes),
[`-counterfactual.jsonl`](organic-corridor-plan-20261126-counterfactual.jsonl) (the router without its grid bound),
[`-timing.jsonl`](organic-corridor-plan-20261126-timing.jsonl).

## Result

- **The gate does not hold.** At the default route cost the planned demand does not predict where today's routes go
  through boxes: R1 and R2 fail, R3 holds narrowly. At route cost 0 all three hold, narrowly (R2 by 0.008); at 6 and 24
  all fail. The straight line between an edge's ends, with no plan, finds the boxes hit better than the short corridors
  do (lift 21 against 9.6, recall 0.49 against 0.27).
- **Lack of room does not explain today's failures.** Of the 461 routes the router draws through a box on these views,
  437 have a way with the same clearance and stubs that the router does not search: it takes the grid of the whole view
  only up to 120 000 cells. Without that bound (an experiment, not merged) 461 become 24, at up to 37 times the CPU
  time on one view. The 247 bus taps through boxes do not change with the bound.
- **The report itself works:** the drawing is unchanged in every record, every planned edge got a way, and the planned
  port sides are those of today's routes at 97 % of the ends.
- **Sizes:** on the bus views up to 1 605 variables and 17 679 constraints per axis in the joint system, 5 to 43 times
  the constraints of a separation by demand. Neither was solved.
- **Time and memory:** the plan adds 0.07 to 0.74 s of CPU to an import of a bus view (up to half again). Its junctions
  grow faster than the boxes: an import of 2 000 boxes in one area peaks at 941 MB with the report, 66 MB without.

## What the report is

`organicCorridors` (provisional, off, omitted) adds `corridorPlans` to a snapshot, one per organic area. The drawing is
the same with and without it (96 of 96 records here, and a runtime test).

- **Corridors.** The free space around the children's boxes (their bounding rectangle with a margin of 48) is cut into
  the rectangles between two boxes that follow each other along x, and again along y. Every free point lies in one of
  each; where two meet is a junction.
- **Routes.** In the order of the edge ids, each edge gets the facing side on each end and the shortest way between those
  sides through the junctions, then the fewest turns, with `routeCost` per route already in a corridor between two boxes.
  The port may lie anywhere on the side (distance from its middle counts). One pass, no rip-up.
- **Demand and need.** The demand of a corridor is the number of routes that run along it. Its need is the ADR's: with a
  route, 2 x 12 clearance + 6 per route + a stub of 12 where a planned port faces it, at least the gap of 24; without a
  route, 24. A corridor between two boxes is **short** when its need exceeds its width. Corridors at the area's outer
  edge grow with the area and are never short.

Differences from the ADR's wording: it names `vpsc::scanline_pairs` with a search radius to find the corridors. That
finds pairs but neither the free space between them nor the junctions, and leaves space beyond the radius out, so the
free space is cut directly. The plan is made on the **final** boxes of the layout, the ones today's router routes
between, not on the separated arrangement before the geometry repair: on four of the generated views the repair moved 3
to 7 of the 100 to 130 root boxes after the arrangement, by up to 1 200 units (a temporary probe on main).

## Views

13 generated views, `bus-mesh.mjs UNITS BUSES organic lines CHILDREN EDGES SEED` (named
`mesh-UNITS-BUSES-CHILDREN-EDGES-sSEED`; the script now takes the seed, default 7, which chooses the bus memberships
only), and 11 organic views of the corpus with orthogonal routing: their routing settings are removed, because as shipped
most draw straight edges, which the plan leaves out. Today's counts differ from those of 2026-11-23
([organic-gap](organic-gap-20261123.md)): main has changed since; the cause was not investigated. A box here is a drawn
box (a bus drawn as lines has none, although the router saw one). Bus taps are drawn from the bus bars after the layout,
so they are reported apart from the routes the router draws. Only the root area is compared (the generated views have no
edges inside a container).

## Gate: does the planned demand predict where routes go through boxes?

The ADR names the question and no threshold. This reading was written into the script before its first run and is in
commit `84d87af`, made before the runs recorded here. It is over pairs (edge, box of the root area holding neither end);
a pair is a hit when the edge's route goes through the box:

- **R1, where:** at least half of the hits lie beside a short corridor of the edge's planned way, and such pairs are hits
  more often (lift) than pairs whose planned way merely runs beside the box.
- **R2, which routes:** the best score from the plan's demand separates routes through boxes from the others better (AUC,
  mean over views) than chance and than the best score without a plan (route length, boxes crossed by the straight line
  between the ends, size or degree of the end boxes).
- **R3, which boxes:** the same for boxes (without a plan: area, degree, straight lines crossing the box).

The demand predicts if all three hold. What changed after the first run changed no rule: the hits leave out the boxes
of buses drawn as lines (they were counted in a trial run), the script reads buses without taps, and two views without
buses that repeated a third (another seed changes nothing without buses) were dropped from the records. The default route
cost was chosen from the planned lengths and demands (below) before the prediction numbers were looked at.

Routes the router draws, the 10 views that have any hit (2 507 hits in 516 462 pairs):

| Route cost | R1: recall; lift short / beside; views | R2: plan / without plan (AUC); views ahead | R3: plan / without plan; views ahead | All three |
| --- | --- | --- | --- | --- |
| 0 | 0.72; 8.8 / 7.3; 9 of 10: **holds** | 0.834 / 0.826; 5 of 10: **holds**, by 0.008 | 0.86 / 0.78; 5 of 10: **holds** | holds, narrowly |
| **1 (default)** | 0.27; 9.6 / 6.9; 3 of 10: **fails** | 0.795 / 0.826; 4 of 10: **fails** | 0.82 / 0.78; 4 of 10: holds | **fails** |
| 6 | 0.16; 9.6 / 6.4; 0 of 10: fails | 0.759 / 0.826; 4 of 10: fails | 0.76 / 0.78; 2 of 10: fails | fails |
| 24 | 0.12; 8.9 / 5.7; 0 of 10: fails | 0.769 / 0.826; 3 of 10: fails | 0.71 / 0.78; 1 of 10: fails | fails |

Without any plan, "the straight line between the ends crosses the box" finds 49 % of the hits at a lift of **21.0**
(precision 0.10, against 0.05 for the short corridors at cost 1). It is no part of R1 as written, which compares with the
planned way beside the box, but it is the stronger predictor of where.

Bus taps (566 hits): all three hold at cost 0 and 1 and fail at 6 and 24, all weakly (AUC over routes 0.60 against 0.57,
over boxes 0.65 against 0.63).

The capacity cost makes the prediction worse: it moves planned routes away from where a router that knows nothing of
capacity draws them. So the readings measure mostly how close the plan is to today's router, and at cost 0 the "demand"
is that of shortest ways. The corpus adds almost no evidence (one hit, on mixed-large).

Planned port sides equal the side today's route leaves its box on at 7 536 of 7 749 ends (97 %).

## Why routes go through boxes today

An instrumented router (patch [`corridor-router-failures.patch`](../experiments/corridor-router-failures.patch), not
merged) logs every repair of a route and, for a failed one, searches again on the grid of the whole view. The router
searches that whole grid only up to 120 000 cells, else in windows 120, 480 and 1 920 units around the route; a route it
finds no way for keeps its straight shape through the boxes between. Routes through a box in the final drawing, by their
last repair:

| View | Router: bound | no way | later | Taps: bound | no way | later |
| --- | --- | --- | --- | --- | --- | --- |
| mesh-100-30-60-250-s7 | 19 | 0 | 0 | 1 | 21 | 3 |
| mesh-100-30-120-500-s7 | 11 | 0 | 0 | 5 | 13 | 9 |
| mesh-100-30-240-1000-s7 | 4 | 0 | 0 | 1 | 17 | 2 |
| mesh-100-30-0-0-s7 | 0 | 0 | 0 | 0 | 0 | 1 |
| mesh-100-0-120-500-s7 | 11 | 0 | 0 | 0 | 0 | 0 |
| mesh-100-0-240-1000-s7 | 170 | 8 | 0 | 0 | 0 | 0 |
| mesh-100-30-60-250-s11 | 6 | 0 | 0 | 2 | 22 | 8 |
| mesh-100-30-60-250-s13 | 0 | 0 | 0 | 1 | 4 | 5 |
| mesh-100-30-120-500-s11 | 72 | 0 | 0 | 7 | 9 | 11 |
| mesh-100-30-120-500-s13 | 0 | 0 | 0 | 4 | 25 | 2 |
| mesh-200-60-120-500-s7 | 144 | 0 | 0 | 12 | 50 | 4 |
| mesh-50-15-120-500-s7 | 0 | 15 | 0 | 0 | 1 | 7 |
| mixed-large | 0 | 0 | 1 | 0 | 0 | 0 |
| **all** | 437 | 23 | 1 | 33 | 162 | 52 |

- **bound:** a way with the same boxes, clearance and stubs exists on the grid of the whole view; the router did not
  search it.
- **no way:** the search on the whole grid finds none either (for a tap: for the member's or the head's tap).
- **later:** the last repair found a way; the drawing changed afterwards (the bus lines, the overlap resolution).

For the taps the table says only what happened to the repair of the planned drawing: the next table shows that their
count does not depend on the bound, so what draws them through boxes is not the router's search (the bus bars are turned
into lines after it). That step was not investigated.

### The router without its grid bound

The same views with `GRID_LIMIT` unbounded (a one-line change on main `70d744b`, not merged), one run each, both
binaries one after the other under the same load:

| View | Router routes through boxes | Taps through boxes | CPU s (one run) |
| --- | --- | --- | --- |
| mesh-100-0-60-250-s7 | 0 → 0 | 0 → 0 | 0.28 → 0.27 |
| mesh-100-30-60-250-s7 | 19 → 0 | 25 → 25 | 0.49 → 3.83 |
| mesh-100-30-120-500-s7 | 11 → 0 | 27 → 27 | 3.40 → 4.79 |
| mesh-100-30-240-1000-s7 | 4 → 0 | 20 → 20 | 16.00 → 23.61 |
| mesh-100-30-0-0-s7 | 0 → 0 | 1 → 1 | 0.38 → 0.38 |
| mesh-100-0-120-500-s7 | 11 → 0 | 0 → 0 | 4.24 → 0.69 |
| mesh-100-0-240-1000-s7 | 178 → 8 | 0 → 0 | 0.81 → 29.63 |
| mesh-100-30-60-250-s11 | 6 → 0 | 32 → 32 | 5.97 → 6.74 |
| mesh-100-30-60-250-s13 | 0 → 0 | 10 → 10 | 1.62 → 2.19 |
| mesh-100-30-120-500-s11 | 72 → 0 | 27 → 27 | 1.01 → 16.77 |
| mesh-100-30-120-500-s13 | 0 → 0 | 31 → 31 | 6.06 → 7.84 |
| mesh-200-60-120-500-s7 | 144 → 0 | 66 → 66 | 13.04 → 16.39 |
| mesh-50-15-120-500-s7 | 15 → 15 | 8 → 8 | 4.48 → 5.34 |
| mixed-large | 1 → 1 | 0 → 0 | 0.03 → 0.03 |
| **all** | 461 → 24 | 247 → 247 |  |

Without the bound the router draws 437 fewer routes through boxes; what is left are the 23 with no way and one changed
later. The time goes both ways: on mesh-100-0-240-1000 the search of the whole grid costs 37 times the CPU, on
mesh-100-30-120-500-s11 17 times, while mesh-100-0-120-500 gets 6 times faster (why was not investigated). The
bound is therefore not simply to be removed: it is the router's search that needs work.

## What each corridor would need (route cost 1)

Root area. A "pair" is two boxes with a corridor between them (its narrowest width, its largest need).

| View | Through a box today: router, taps | Pairs | Short | Demand p95, max | Gap needed, max | Missing width of a short pair: median, p95, max |
| --- | --- | --- | --- | --- | --- | --- |
| mesh-100-0-60-250-s7 | 0 of 250, 0 of 0 | 448 | 144 | 38, 81 | 510 | 18, 125, 324 |
| mesh-100-30-60-250-s7 | 19 of 250, 25 of 159 | 607 | 213 | 42, 83 | 534 | 24, 118, 274 |
| mesh-100-30-120-500-s7 | 11 of 500, 27 of 159 | 561 | 183 | 47, 131 | 822 | 25, 90, 444 |
| mesh-100-30-240-1000-s7 | 4 of 1000, 20 of 159 | 583 | 186 | 30, 267 | 1638 | 25, 72, 750 |
| mesh-100-30-0-0-s7 | 0 of 0, 1 of 159 | 570 | 195 | 18, 29 | 198 | 19, 46, 72 |
| mesh-100-0-120-500-s7 | 11 of 500, 0 of 0 | 479 | 142 | 52, 174 | 1068 | 26, 174, 628 |
| mesh-100-0-240-1000-s7 | 178 of 1000, 0 of 0 | 440 | 134 | 69, 208 | 1284 | 42, 169, 858 |
| mesh-100-30-60-250-s11 | 6 of 250, 32 of 164 | 579 | 199 | 43, 80 | 504 | 30, 102, 240 |
| mesh-100-30-60-250-s13 | 0 of 250, 10 of 155 | 585 | 184 | 23, 87 | 546 | 18, 51, 258 |
| mesh-100-30-120-500-s11 | 72 of 500, 27 of 164 | 634 | 244 | 66, 127 | 798 | 36, 134, 522 |
| mesh-100-30-120-500-s13 | 0 of 500, 31 of 155 | 552 | 168 | 18, 167 | 1038 | 18, 54, 1014 |
| mesh-200-60-120-500-s7 | 144 of 500, 66 of 318 | 1275 | 603 | 71, 165 | 1014 | 52, 150, 327 |
| mesh-50-15-120-500-s7 | 15 of 500, 8 of 82 | 261 | 85 | 33, 136 | 852 | 24, 526, 634 |
| organic-25 | 0 of 29, 0 of 0 | 71 | 3 | 4, 5 | 60 | 18, 18, 18 |
| organic-50 | 0 of 59, 0 of 0 | 178 | 1 | 6, 7 | 66 | 3, 3, 3 |
| organic-100 | 0 of 119, 0 of 0 | 367 | 9 | 6, 12 | 96 | 18, 18, 18 |
| organic-200 | 0 of 239, 0 of 0 | 880 | 12 | 8, 16 | 120 | 6, 18, 18 |
| organic-300 | 0 of 359, 0 of 0 | 1338 | 42 | 8, 21 | 150 | 18, 18, 24 |
| mixed-large | 1 of 95, 0 of 0 | 52 | 12 | 22, 25 | 186 | 36, 84, 84 |
| mixed-layouts | 0 of 16, 0 of 0 | 9 | 1 | 2, 2 | 48 | 24, 24, 24 |
| self-loops | 0 of 20, 0 of 0 | 9 | 0 | 5, 5 | 66 | 0, 0, 0 |
| petri-net-shapes | 0 of 41, 0 of 0 | 84 | 1 | 11, 12 | 102 | 7, 7, 7 |
| vehicle-buses-organic | 0 of 0, 0 of 0 | 63 | 0 | 4, 4 | 48 | 0, 0, 0 |

## Route cost

The 13 generated views, root area, against cost 0:

| Route cost | Planned length, all views (median, largest view) | Demand on a corridor between boxes: p95, max (median over views) | Short pairs (of 7 574) | Missing width, sum |
| --- | --- | --- | --- | --- |
| 0 | 1.00 | 79, 165 | 2 900 | 241 591 |
| **1** | 1.06 (1.03, 1.17) | 42, 131 | 2 680 | 119 454 |
| 6 | 1.16 (1.13, 1.43) | 30, 101 | 2 565 | 92 822 |
| 24 | 1.32 (1.34, 1.84) | 22, 95 | 2 516 | 88 399 |

The default is 1: it halves the missing width and the 95th percentile of the demand for 6 % more planned length; 24, the
first value tried, lengthens the ways by a third. The default was set after a sweep of 0, 0.25, 1, 6, 24 and 96 on four
of the views. The cost is not scale-free: it grows with the demand, so the same value spreads dense views more eagerly.

Between 33 and 38 % of the neighbour pairs would be short whatever the cost: one route already needs 30, and 42 where a
port faces the corridor, against the gap of 24.

## Size of the constraint systems (route cost 1)

Joint system per axis (the ADR's refinement): variables are the node positions and the interior route segments (those at
a port move with their node); constraints are node pairs with a corridor, segment against the boxes beside its
corridors, and consecutive segments in a corridor (ordered by their planned position; step C3 would order them by
crossings). Separation by demand (phase 3 as written) has the node variables and node pairs only.

| View | Boxes | Routes | Corridors | Junctions | Joint x: variables, constraints | Joint y | By demand: variables, constraints x / y | Report, KB |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| mesh-100-0-60-250-s7 | 100 | 250 | 596 | 3092 | 369, 4797 | 512, 3140 | 100, 174 / 274 | 213 |
| mesh-100-30-60-250-s7 | 130 | 439 | 764 | 3871 | 434, 4329 | 711, 3801 | 130, 265 / 342 | 262 |
| mesh-100-30-120-500-s7 | 130 | 689 | 750 | 4816 | 556, 5379 | 435, 2409 | 130, 247 / 314 | 331 |
| mesh-100-30-240-1000-s7 | 130 | 1189 | 772 | 4635 | 942, 6277 | 459, 2540 | 130, 270 / 313 | 518 |
| mesh-100-30-0-0-s7 | 130 | 189 | 777 | 3956 | 224, 1376 | 344, 1713 | 130, 221 / 349 | 159 |
| mesh-100-0-120-500-s7 | 100 | 500 | 602 | 3832 | 382, 4311 | 623, 4459 | 100, 201 / 278 | 305 |
| mesh-100-0-240-1000-s7 | 100 | 1000 | 598 | 4657 | 1157, 8108 | 493, 6195 | 100, 195 / 245 | 475 |
| mesh-100-30-60-250-s11 | 130 | 444 | 773 | 4903 | 532, 9845 | 804, 5818 | 130, 228 / 351 | 297 |
| mesh-100-30-60-250-s13 | 130 | 435 | 772 | 4282 | 348, 2941 | 400, 2253 | 130, 245 / 340 | 248 |
| mesh-100-30-120-500-s11 | 130 | 694 | 779 | 4668 | 865, 6775 | 665, 5898 | 130, 281 / 353 | 358 |
| mesh-100-30-120-500-s13 | 130 | 685 | 776 | 4871 | 535, 3185 | 359, 1899 | 130, 203 / 349 | 332 |
| mesh-200-60-120-500-s7 | 259 | 877 | 1495 | 11835 | 1095, 17679 | 1605, 13416 | 259, 598 / 677 | 595 |
| mesh-50-15-120-500-s7 | 65 | 597 | 386 | 2038 | 413, 2242 | 268, 1289 | 65, 113 / 148 | 244 |
| organic-25 | 25 | 29 | 152 | 775 | 26, 44 | 28, 42 | 25, 39 / 32 | 26 |
| organic-50 | 50 | 59 | 300 | 1654 | 50, 70 | 53, 121 | 50, 70 / 108 | 53 |
| organic-100 | 100 | 119 | 602 | 6161 | 101, 200 | 109, 199 | 100, 197 / 170 | 108 |
| organic-200 | 200 | 239 | 1202 | 13060 | 205, 476 | 215, 482 | 200, 456 / 424 | 224 |
| organic-300 | 300 | 359 | 1802 | 18464 | 309, 858 | 337, 650 | 300, 817 / 521 | 333 |
| mixed-large | 19 | 86 | 111 | 380 | 56, 208 | 77, 336 | 19, 19 / 33 | 40 |
| mixed-layouts | 6 | 7 | 38 | 84 | 8, 9 | 7, 8 | 6, 4 / 5 | 12 |
| self-loops | 6 | 7 | 38 | 92 | 8, 13 | 6, 3 | 6, 6 / 3 | 6 |
| petri-net-shapes | 24 | 41 | 146 | 573 | 34, 100 | 36, 101 | 24, 38 / 46 | 28 |
| vehicle-buses-organic | 22 | 21 | 130 | 453 | 22, 18 | 22, 45 | 22, 18 / 45 | 23 |
| large-500 | 500 | 599 | 3002 | 63731 | 666, 2536 | 562, 419 | 500, 1418 / 243 | 544 |
| large-1000 | 1000 | 1199 | 6002 | 229817 | 1139, 3579 | 1005, 74 | 1000, 2798 / 59 | 1069 |
| large-2000 | 2000 | 2399 | 12002 | 2528051 | 2509, 7060 | 2003, 39 | 2000, 4784 / 30 | 2168 |

On the bus views the joint system has 1.7 to 12 times the variables and 5 to 43 times the constraints of the separation
by demand (means 4.7 and 18); 65 to 90 % of them are segment against box, counted for every box beside a corridor a
segment runs along, so a solver could drop the implied ones. The most constraints measured are 17 679 with 1 095
variables (mesh-200-60-120-500, x axis); the flat views of 500 to 2 000 boxes (`large-N`, see Time) stay under 7 100,
as they carry little demand. `vpsc::project_axis`, which both would use, is an iterative projection with a budget of
8 000 000 constraint updates and at most 4 096 sweeps, and gives nothing when it has not converged to 1e-8: at 17 679
constraints that is 452 sweeps.
**Whether it converges on either system was not tried** (that is step C2 or C3).

## Time

CPU time (user and system) of one `layout-command` import, release build, runs with and without the option in turn;
1-minute load 4.1 to 5.6 (other programs ran; not an idle machine). The difference holds the plan and the larger answer
written out.

| View | Boxes | Routes | CPU s without the report: median (range) | With it | Difference | Runs |
| --- | --- | --- | --- | --- | --- | --- |
| mesh-100-0-60-250-s7 | 100 | 250 | 0.25 (0.25 to 0.26) | 0.32 (0.32 to 0.32) | +0.07 | 5 |
| mesh-100-30-120-500-s7 | 130 | 689 | 3.51 (3.49 to 3.52) | 3.67 (3.59 to 3.67) | +0.15 | 5 |
| mesh-100-30-240-1000-s7 | 130 | 1189 | 16.59 (16.54 to 16.61) | 16.82 (16.77 to 16.85) | +0.23 | 5 |
| mesh-100-0-240-1000-s7 | 100 | 1000 | 0.83 (0.83 to 0.83) | 1.23 (1.22 to 1.23) | +0.40 | 5 |
| mesh-200-60-120-500-s7 | 259 | 877 | 13.49 (13.33 to 13.54) | 14.23 (14.14 to 14.25) | +0.74 | 5 |
| organic-300 | 300 | 359 | 0.16 (0.15 to 0.16) | 0.17 (0.16 to 0.17) | +0.01 | 5 |
| large-500 | 500 | 599 | 0.15 (0.15 to 0.16) | 0.18 (0.18 to 0.19) | +0.03 | 3 |
| large-1000 | 1000 | 1199 | 0.41 (0.41 to 0.42) | 0.55 (0.54 to 0.55) | +0.13 | 3 |
| large-2000 | 2000 | 2399 | 1.76 (1.75 to 1.77) | 3.22 (3.20 to 3.25) | +1.46 | 3 |

The plan costs up to half again the CPU on a dense view of 100 boxes (mesh-100-0-240-1000) and 83 % more on 2 000
boxes. Its memory grows with the junctions, which grow faster than the boxes: the corridors stay at about 6 per box (as
the ADR expects), but junctions are 60 per box at 300 boxes, 230 at 1 000 and 1 264 at 2 000 (2 528 051). Peak memory
footprint of one import (`/usr/bin/time -l`, one run each) on 1 000 boxes: 41 MB without the report, 108 MB with it; on
2 000 boxes: 66 MB and **941 MB**, which a browser engine may not get. The report itself is 0.5, 1.1 and 2.2 MB at 500,
1 000 and 2 000 boxes.

The flat views of 500 to 2 000 boxes (`large-N`: `generate.organic_ladder(N)` with orthogonal routing and
`scalableStress`) are a poor stand-in for a large region: the layout draws the chain as a very long thin band (at 2 000
boxes about 28 million by 16 000 units), with hardly any demand (at most 19 routes in a corridor at 500 boxes, 4 at
2 000). Their times and junction counts are what was measured, not what a compact region of that size would give.

## Fingerprints and determinism

- With the option off: the 15 default views (`bench`, release, `--limit 100 --reps 1 --threads 1,4`) give the same
  `refineFingerprint` and `geometryFingerprint` on `84d87af` as on main `70d744b`, built and run in the same way
  (`fingerprint_compare.py`: identical 15), and the same on 1 and 4 threads. Against the committed
  `fingerprints-20261122-default-0.8.0.jsonl` 14 views differ in metadata only; layerweave-code is regenerated from the
  source, so its input differs.
- With the option on: the drawing is unchanged (every record above). The plan is computed in integers (1 024 steps per
  layout unit); `tools/platform-parity.mjs` has a request with the report, which gave identical responses natively and
  in WASM on this machine (macOS arm64; 20 of 20 requests identical, after merging main `231c010`).

## Limits

- One router and one family of generated views (one generator, seeds and sizes) carry nearly all hits.
- The baselines are simple geometric ones; a better one would only make the comparison harder for the plan.
- The plan's lengths are estimates over the junctions (a straight run through rectangles of different sizes is counted
  from centre to centre), and a pair of boxes can have more than one corridor.
- One pass in the order of the edge ids; no rip-up and reroute, no order of the tracks.
- Containers take part as boxes; edges that leave an area are counted, not planned (step C4).
