# A sparse repair grid for views over the router's grid bound (#203), 2026-11-26

Machine: Apple M3 (8 cores, 16 GB), macOS 26.0. Revisions: the baseline is main `231c010`; the option is the branch
`router-grid` at `3681946`. Build: `cargo build --release -p arqlet-runtime --bin layout-command --locked` (rustc
1.99.0, thin LTO), `CARGO_BUILD_JOBS=4`; Node 24, Python 3.13. Load: other builds ran on the machine during most runs;
every record has the 1-minute load at its start. Route counts, metrics and drawings do not depend on it; times have a
section of their own.

Commands (`DIR` is any scratch directory; `main` and `sparse` are the two binaries):

```sh
python3 benchmarks/generate.py --no-manifest
python3 benchmarks/experiments/router_grid.py --work DIR --out FILE.jsonl --views report,other --arm base=main \
    --arm off=sparse --arm on=sparse,on --limit 600
python3 benchmarks/experiments/router_grid.py --work DIR --out FILE.jsonl --views corpus,large --arm base=main \
    --arm off=sparse --arm on=sparse,on --limit 600
python3 benchmarks/experiments/router_grid.py --work DIR --out FILE.jsonl --views report,other,large --runs 3 --times \
    --arm base=main --arm on=sparse,on
python3 benchmarks/experiments/router_grid.py --summarize FILE.jsonl --base base
```

Records: [`.jsonl`](router-sparse-grid-20261126.jsonl) (quality),
[`-timing.jsonl`](router-sparse-grid-20261126-timing.jsonl), [`-variants.jsonl`](router-sparse-grid-20261126-variants.jsonl),
[`-searches.txt`](router-sparse-grid-20261126-searches.txt),
[`-fingerprints.jsonl`](router-sparse-grid-20261126-fingerprints.jsonl).

## Result

- With `sparseRepairGrid` the router routes through boxes go from **1 091 to 52** on the 24 generated views with
  containers and bus meshes (the 13 of the report on #156: 460 to 8, where removing the bound gives 23). Lines through
  nodes (metrics) go from 10 048 to 1 529, and no view has more of them or more routes through boxes.
- Over the same views: crossings -20 %, bends -39 %, shared line pairs -15 %, length +5 %, the boxes unchanged.
- Not on every view: of the 22 whose drawing changes, shared line pairs rise on 9, crossings on 5, hugging pairs on 4,
  length on 14, bends on none. In the order of the default objectives 20 are better, 1 is worse
  (`mesh-100-30-240-1000-s23`, shared line pairs +1.6 %), 1 equal.
- **The quality gates do not all hold**: line-node hits are not 0 (52 router routes and all 521 bus taps through boxes
  remain), and elk-rs was not compared. So the option stays off.
- Time: the sum of the median CPU times of 28 views falls from 130.8 to 124.8 s, but 7 views take at least a quarter
  longer (up to 13.7 times, 0.48 to 6.57 s) and 9 at least a fifth less. The large increases come from a second routing
  pass of the layout that these views now get, not from the search.
- Views within the bound are drawn as before: option off, all 65 views identical to main; option on, 39 of the 41 corpus
  and large views identical (the two that change are over the bound).
- The bus taps through boxes have causes of their own, not the router's bound (last section).

## The cause, reproduced

The numbers of the report on #156 (`organic-corridor-plan-20261126.md`, branch `organic-corridor-plan`) reproduce on
`231c010` with its script and its patches: of the routes through boxes on its 13 generated views and `mixed-large` with
orthogonal edges, the router's 461 are 437 with a way on the whole-view grid, 23 without and 1 changed later; the taps'
247 are 33, 162 and 52; without the bound 461 become 24 and the 247 stay. CPU time of one run each, main to unbounded:
mesh-100-0-240-1000 1.08 to 39.8 s, mesh-100-30-120-500-s11 1.32 to 21.7 s, mesh-100-0-120-500 5.51 to 0.94 s.

Why: the repair searches the grid with a line on both sides of every box only up to 120 000 cells. The report's views
that are over it have 185 to 380 boxes and 135 000 to 570 000 cells on average per repair. Above the bound the repair
searches windows 120, 480 and 1 920 units around the route, with lines only from the boxes that meet the window: a way
that leaves them is not on that grid, and a window over the bound ends the search. The route then keeps its shape through
the boxes.

Removing the bound costs mostly not the search. On mesh-100-0-240-1000 (the 37 times), sampled with macOS `sample` every
5 ms over a whole run of the unbounded binary (5 560 samples), 93 % are in the second routing pass that the view now gets
(see Time) and 77 % in that pass's overlap resolution; the grid repair has 11 %. The unbounded search is still the
costlier one: on the dumped problems it takes 6.6 times the time of the sparse grid without a cost per bend and 8 times
with one (table below).

## Searches compared

The repair problems of the 13 views whose grid is over the bound were dumped with
[`router-grid-variants.patch`](../experiments/router-grid-variants.patch) and run through each search (a problem that
repeats with the same geometry counts once). `windows` is what today's windows find.

| Search | Ways found (of 8 908) | Missed that `full` finds | Found beyond `full` | Mean cells | Time, s | Heap pops, million | Mean length of its ways | Mean bends |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| windows (today) | 7 157 | | | | 2.6 | | | |
| full | 7 831 | 0 | 0 | 279 809 | 9.9 | 117 | 2 564 | 14.9 |
| full-turn12 | 7 831 | 0 | 0 | 279 809 | 20.0 | 240 | 2 564 | 5.7 |
| gap | 8 120 | 0 | 289 | 26 399 | 1.5 | 11 | 2 473 | 12.6 |
| gap-turn4 | 8 120 | 0 | 289 | 26 399 | 2.5 | 21 | 2 473 | 5.9 |
| gap-turn12 | 8 120 | 0 | 289 | 26 399 | 2.5 | 22 | 2 473 | 5.9 |
| gap-turn24 | 8 120 | 0 | 289 | 26 399 | 2.5 | 22 | 2 474 | 5.8 |
| gap-turn48 | 8 120 | 0 | 289 | 26 399 | 2.6 | 23 | 2 478 | 5.7 |
| gap2 | 7 902 | 112 | 183 | 27 442 | 1.6 | 13 | 2 518 | 12.3 |
| gap2-turn12 | 7 902 | 112 | 183 | 27 442 | 2.8 | 27 | 2 518 | 5.7 |
| lazy | 7 732 | 99 | 0 | 35 435 | 15.9 | 196 | 2 718 | 5.6 |
| follow | 7 752 | 79 | 0 | 84 351 | 13.4 | 161 | 2 602 | 5.7 |

Times of one run each, one after the other, 1-minute load 2.5 to 3.5; the windows' time was taken while the problems were
dumped. The lengths are means over the ways each search finds, so they compare different sets.

- **full**: the unbounded grid with today's search. **full-turn12**: the same grid, a cost of 12 per bend: fewer bends,
  twice the work.
- **gap**: a line only in the gaps between box edges that offer a way no neighbouring gap offers, in their middle;
  **gap2** two lines in a wide gap. **-turnN** adds a cost of N per bend.
- **lazy**: the lines of the ends and of the boxes in the way first; the boxes that block a step are added and the search
  runs again. **follow**: windows grown around the route and the boxes it runs through. Both cost more than `gap` and find
  no more than the full grid.
- A coarser grid (every k-th line) and a hierarchy of grids were not tried: the gap grid already has a tenth of the
  cells and loses no way.

## What was built

`sparseRepairGrid` (`LayoutOptions`, provisional, off, omitted when off), `Params::sparse_grid` in `arqlet-routing`.
When the whole-view grid is over 120 000 cells, the repair first searches the **sparse grid** of the whole view, and the
windows as before only if that finds no way or has more than 480 000 cells.

- **Lines.** Between two neighbouring box edges every position meets the same boxes. A gap gets a line only after an edge
  where a box ends and before one where a box begins; any other gap meets all the boxes of a neighbour and more, so a way
  along it can be moved to that neighbour. With a line beyond each outermost edge and the lines through the two stubs, the
  grid holds a way between the stubs whenever the rectangles leave one (a shifting argument per axis). One exception:
  boxes that touch up to a rounding error leave a channel of no width, which the grid does not use (below). Discs, and the
  edge's own boxes where a clearance is required, keep lines beside them. A test compares it with a grid that has a line in every
  gap on 600 random arrangements.
- **Search.** A* with a state per cell and direction and 24 units per bend (as the bus taps' search takes it), ties
  broken by state, so the result depends on the input only.
- The idea is derived here; no code or paper was used.

Variants of the built search, end to end with the option on (sums over the generated views):

| 24 generated views | Routes through boxes | Lines through nodes | Shared line pairs | Hugging pairs | Crossings | Bends | Length |
| --- | --- | --- | --- | --- | --- | --- | --- |
| main | 1 091 | 10 048 | 367 930 | 18 076 | 1 342 481 | 136 755 | 43 308 001 |
| **built**: sparse before the windows, 24 per bend | 52 | 1 529 | 311 834 | 15 224 | 1 072 252 | 83 190 | 45 393 787 |
| sparse after the windows | 52 | 1 527 | 311 758 | 17 405 | 1 528 250 | 141 338 | 47 628 085 |
| no cost per bend | 52 | 1 530 | 324 114 | 16 351 | 1 259 037 | 114 617 | 45 863 447 |
| 48 per bend | 52 | 1 529 | 312 203 | 15 241 | 1 067 234 | 82 426 | 45 432 906 |

| 23 views (without the one over 600 s) | Routes through boxes | Lines through nodes | Shared line pairs | Hugging pairs | Crossings | Bends | Length |
| --- | --- | --- | --- | --- | --- | --- | --- |
| main | 966 | 9 122 | 296 248 | 15 624 | 1 227 383 | 126 045 | 40 269 214 |
| built | 18 | 1 274 | 216 328 | 13 027 | 994 378 | 76 216 | 42 122 738 |
| channels of no width | 7 | 1 247 | 315 871 | 29 317 | 881 655 | 77 286 | 36 017 712 |

| the report's 13 views | Routes through boxes | Lines through nodes | Shared line pairs | Hugging pairs | Crossings | Bends | Length |
| --- | --- | --- | --- | --- | --- | --- | --- |
| main | 460 | 4 521 | 124 369 | 8 876 | 618 305 | 65 518 | 16 792 407 |
| built | 8 | 666 | 92 183 | 7 378 | 488 341 | 41 582 | 17 645 611 |
| unbounded grid | 23 | 690 | 105 173 | 8 392 | 835 568 | 84 792 | 19 522 893 |

- **After the windows** (the sparse grid only for what they give up on) leaves the routes the windows find as they are,
  with their many bends: more crossings, bends and time.
- **No cost per bend** costs bends and crossings; 48 is about the same as 24.
- **Channels of no width** (edges taken as the search's free test sees them, 1e-6 inside each box) find ways between
  touching boxes: 7 routes through boxes instead of 18 on 23 views, but 46 % more shared line pairs and 2.3 times the
  hugging pairs, since no second track fits, and one view ran over 600 s (21 s on main). Not taken.

Of the 52 router routes still through boxes (an instrumented build that searches each failed route again on the grid with
every gap): 34 have no way for their stubs on that grid either, 11 only through channels of no width, and 7 have a stub
that runs into a box.

## Quality, option on against main

`through boxes` counts routes through a drawn box that is neither an end nor holds one; `lines through nodes`, shared
line pairs and the rest are the `metrics` operation on the drawing (an explicit bus's edges may share lines); crossings
include those of edges with a common end. `In objective order`: the first of overlaps, lines through nodes, shared
lines, crossings, hugging, bends, length that differs.

### Generated views

| View | Through boxes: router, taps | Lines through nodes | Shared line pairs | Crossings | Bends | Length | In objective order |
| --- | --- | --- | --- | --- | --- | --- | --- |
| mesh-100-0-60-250-s7 | 0, 0 | 0 | 6 888 | 5 928 | 3 356 | 670 904 | same drawing |
| mesh-100-30-60-250-s7 | 19 → 0, 25 | 145 → 26 | 6 505 → 1 823 | 10 831 → 11 086 | 2 969 → 2 009 | 608 311 → 657 112 | better |
| mesh-100-30-120-500-s7 | 11 → 0, 27 | 99 → 73 | 2 888 → 2 930 | 37 547 → 28 240 | 5 128 → 3 161 | 1 192 257 → 1 228 993 | better |
| mesh-100-30-240-1000-s7 | 4 → 0, 20 | 109 → 73 | 15 636 → 16 822 | 174 701 → 123 745 | 10 047 → 5 879 | 3 204 652 → 3 004 816 | better |
| mesh-100-30-0-0-s7 | 0, 1 | 4 | 237 | 1 415 | 916 | 127 239 | same drawing |
| mesh-100-0-120-500-s7 | 11 → 0, 0 | 61 → 0 | 3 970 → 12 560 | 39 517 → 18 848 | 5 447 → 2 810 | 1 122 033 → 1 112 395 | better |
| mesh-100-0-240-1000-s7 | 178 → 8, 0 | 1 296 → 21 | 46 615 → 10 360 | 117 938 → 105 399 | 8 806 → 4 828 | 3 099 018 → 3 742 370 | better |
| mesh-100-30-60-250-s11 | 6 → 0, 32 | 145 → 36 | 1 581 → 1 018 | 23 870 → 11 198 | 4 811 → 2 091 | 922 420 → 889 599 | better |
| mesh-100-30-60-250-s13 | 0, 10 | 28 | 1 468 → 1 224 | 14 049 → 11 299 | 2 884 → 1 950 | 513 087 → 516 165 | better |
| mesh-100-30-120-500-s11 | 72 → 0, 27 | 433 → 54 | 19 999 → 10 767 | 36 860 → 45 678 | 5 614 → 4 160 | 1 347 012 → 1 407 326 | better |
| mesh-100-30-120-500-s13 | 0, 31 | 93 | 7 216 → 7 163 | 62 267 → 46 916 | 5 134 → 2 964 | 1 164 318 → 1 078 909 | better |
| mesh-200-60-120-500-s7 | 144 → 0, 66 | 2 078 → 249 | 8 329 → 17 243 | 46 741 → 46 499 | 5 161 → 4 927 | 1 640 268 → 2 075 772 | better |
| mesh-50-15-120-500-s7 | 15 → 0, 8 | 30 → 9 | 3 037 → 3 148 | 46 641 → 32 090 | 5 245 → 2 531 | 1 180 888 → 1 134 013 | better |
| mesh-100-30-60-250-s17 | 0, 28 | 46 | 683 | 1 350 | 1 122 | 137 715 | equal |
| mesh-100-30-120-500-s19 | 0, 15 | 24 | 5 764 → 5 739 | 75 270 → 53 221 | 5 945 → 3 235 | 1 408 266 → 1 307 609 | better |
| mesh-100-30-240-1000-s23 | 0, 14 | 80 | 17 056 → 17 330 | 151 691 → 106 083 | 11 110 → 6 003 | 2 740 546 → 2 453 927 | worse |
| mesh-100-20-120-500-s29 | 99 → 0, 41 | 857 → 136 | 8 348 → 3 740 | 36 428 → 42 015 | 4 858 → 3 658 | 1 150 792 → 1 560 245 | better |
| mesh-150-45-120-500-s7 | 35 → 0, 58 | 388 → 135 | 10 758 → 14 591 | 35 457 → 36 541 | 5 869 → 3 991 | 1 236 323 → 1 264 546 | better |
| mesh-150-0-200-600-s7 | 72 → 10, 0 | 482 → 45 | 15 674 → 9 594 | 44 199 → 47 376 | 7 750 → 3 612 | 1 684 394 → 1 719 280 | better |
| mesh-80-0-300-900-s7 | 218 → 0, 0 | 1 902 → 0 | 36 452 → 58 391 | 78 421 → 52 598 | 7 466 → 4 552 | 2 662 764 → 3 354 565 | better |
| mesh-200-60-240-800-s31 | 125 → 34, 58 | 926 → 255 | 71 682 → 95 506 | 115 098 → 77 874 | 10 710 → 6 974 | 3 038 787 → 3 271 049 | better |
| mesh-300-90-120-500-s7 | 28 → 0, 43 | 261 → 63 | 2 502 → 1 820 | 69 629 → 66 233 | 4 695 → 2 843 | 7 511 010 → 7 511 132 | better |
| mesh-480-0-60-250-s7 | 0, 0 | 0 | 828 → 500 | 8 051 → 7 975 | 1 038 → 580 | 1 254 641 → 1 255 314 | better |
| mesh-60-18-360-900-s37 | 54 → 0, 17 | 561 → 79 | 73 814 → 11 757 | 108 582 → 92 645 | 10 674 → 5 038 | 3 690 355 → 3 912 793 | better |
| **sum** | 1 091 → 52 (-95 %), 521 | 10 048 → 1 529 (-85 %) | 367 930 → 311 834 (-15 %) | 1 342 481 → 1 072 252 (-20 %) | 136 755 → 83 190 (-39 %) | 43 308 001 → 45 393 787 (+5 %) | |

Checks over these 24 views and `mixed-large` (as shipped, within the bound, unchanged), option on against main:

| Gate | Result |
| --- | --- |
| Valid geometry | geometry issues equal on every view (326 in sum, all of them already on main); no more sibling overlaps; boxes and drawn area unchanged |
| Complete edges in logical direction | every edge has a route on both; the 75 routes that end beside their end's box on main (bus taps and straight edges) are the same edges, none new; oblique routes 121 to 78, more on no view |
| Line-node hits | **not 0**: 10 048 to 1 529, more on no view |
| Crossings, bends against main | crossings -20 % in sum (-18 % counting only edges without a common end), more on 5 views; bends -39 %, more on no view |
| Shared lines, hugging (ranked above and beside them) | shared line pairs -15 % in sum, **more on 9 views**, up to 3.2 times (mesh-100-0-120-500-s7); hugging pairs -16 %, more on 4 |
| elk-rs on the same views | **not compared** |
| Timing | below |

### Corpus and large flat views

38 corpus views (`benchmarks/datasets`) and the flat organic views of 500, 1 000 and 2 000 boxes of the #156 report: 39
drawn the same with the option on. The two that change:

| View | Shared line pairs | Crossings | Bends | In objective order |
| --- | --- | --- | --- | --- |
| large-500 | 1 → 0 | 60 → 59 | 1 276 → 1 236 | better |
| large-1000 | 2 → 7 | 44 → 37 | 2 486 → 2 426 | worse |

## Time

CPU time (user and system) of one `layout-command` import, release build, the two binaries in turn, 3 runs each, 1-minute load: 1.9 to 6.9, median 4.8.
Other programs ran: not an idle machine.

| View | CPU s, base: median (range) | CPU s, on: median (range) | Ratio of the medians | Runs |
| --- | --- | --- | --- | --- |
| mesh-100-0-60-250-s7 | 0.27 (0.26 to 0.28) | 0.28 (0.26 to 0.32) | 1.04 | 3 |
| mesh-100-30-60-250-s7 | 0.52 (0.48 to 0.52) | 1.47 (1.41 to 1.48) | 2.82 | 3 |
| mesh-100-30-120-500-s7 | 3.39 (3.38 to 3.49) | 1.81 (1.81 to 2.00) | 0.53 | 3 |
| mesh-100-30-240-1000-s7 | 18.57 (15.98 to 18.75) | 9.94 (8.90 to 10.59) | 0.54 | 3 |
| mesh-100-30-0-0-s7 | 0.40 (0.39 to 0.41) | 0.41 (0.39 to 0.42) | 1.03 | 3 |
| mesh-100-0-120-500-s7 | 4.84 (4.71 to 4.97) | 0.47 (0.46 to 0.52) | 0.10 | 3 |
| mesh-100-0-240-1000-s7 | 0.85 (0.80 to 0.92) | 5.11 (5.09 to 5.67) | 6.02 | 3 |
| mesh-100-30-60-250-s11 | 6.74 (6.03 to 6.77) | 1.96 (1.71 to 2.05) | 0.29 | 3 |
| mesh-100-30-60-250-s13 | 1.61 (1.60 to 1.62) | 1.05 (1.05 to 1.06) | 0.65 | 3 |
| mesh-100-30-120-500-s11 | 1.00 (1.00 to 1.01) | 5.94 (5.92 to 5.96) | 5.93 | 3 |
| mesh-100-30-120-500-s13 | 6.24 (6.03 to 6.74) | 2.49 (2.33 to 2.69) | 0.40 | 3 |
| mesh-200-60-120-500-s7 | 15.69 (15.30 to 15.86) | 16.19 (15.84 to 16.42) | 1.03 | 3 |
| mesh-50-15-120-500-s7 | 5.39 (4.90 to 5.41) | 1.91 (1.57 to 1.91) | 0.35 | 3 |
| corpus:mixed-large | 0.79 (0.79 to 0.80) | 0.80 (0.80 to 0.80) | 1.00 | 3 |
| mesh-100-30-60-250-s17 | 0.42 (0.42 to 0.42) | 0.45 (0.45 to 0.45) | 1.06 | 3 |
| mesh-100-30-120-500-s19 | 6.11 (6.10 to 6.11) | 3.90 (3.90 to 3.90) | 0.64 | 3 |
| mesh-100-30-240-1000-s23 | 20.79 (20.78 to 20.84) | 11.66 (11.63 to 11.68) | 0.56 | 3 |
| mesh-100-20-120-500-s29 | 0.65 (0.65 to 0.65) | 2.77 (2.77 to 2.77) | 4.28 | 3 |
| mesh-150-45-120-500-s7 | 4.81 (4.81 to 4.82) | 4.91 (4.90 to 4.93) | 1.02 | 3 |
| mesh-150-0-200-600-s7 | 6.90 (6.89 to 6.90) | 19.11 (19.08 to 20.05) | 2.77 | 3 |
| mesh-80-0-300-900-s7 | 0.55 (0.55 to 0.55) | 0.72 (0.69 to 0.72) | 1.30 | 3 |
| mesh-200-60-240-800-s31 | 19.75 (19.69 to 19.75) | 20.70 (19.87 to 20.72) | 1.05 | 3 |
| mesh-300-90-120-500-s7 | 1.47 (1.47 to 1.48) | 1.66 (1.65 to 1.66) | 1.13 | 3 |
| mesh-480-0-60-250-s7 | 0.24 (0.24 to 0.25) | 0.27 (0.27 to 0.27) | 1.10 | 3 |
| mesh-60-18-360-900-s37 | 0.48 (0.48 to 0.48) | 6.57 (6.56 to 6.57) | 13.74 | 3 |
| large-500 | 0.15 (0.15 to 0.15) | 0.15 (0.15 to 0.15) | 1.03 | 3 |
| large-1000 | 0.41 (0.41 to 0.41) | 0.42 (0.42 to 0.42) | 1.03 | 3 |
| large-2000 | 1.71 (1.70 to 1.74) | 1.75 (1.74 to 1.83) | 1.02 | 3 |
| sum of the medians | 130.76 | 124.84 | 0.95 | |

The sum falls by 5 %, but not evenly: 7 views take at least a quarter longer, up to 13.7 times
(0.48 to 6.57 s) and 12 s more (mesh-150-0-200-600-s7), and 9 take at least a fifth less, down to a tenth.

**Why:** the layout routes a second time, around the other routes, when the first routing leaves between 1 and 500
conflicts (`MAX_REROUTED_ROUTES`, `crates/layout/src/lib.rs`), and keeps that drawing when it has fewer. With the option
some views fall from above 500 to below it and now pay for that pass; others fall to 0 and skip it. Counted with an
instrumented build; every view whose CPU time rises by more than 1.4 times is one of the first kind:

| View | Conflicts after the first routing: main → option | Second pass: main → option | CPU ratio |
| --- | --- | --- | --- |
| mesh-100-0-120-500-s7 | 281 → 0 | runs → none | 0.10 |
| mesh-100-30-60-250-s7 | 519 → 131 | none → runs | 2.82 |
| mesh-100-0-240-1000-s7 | 1 783 → 44 | none → runs | 6.02 |
| mesh-100-30-120-500-s11 | 942 → 36 | none → runs | 5.93 |
| mesh-100-20-120-500-s29 | 1 293 → 162 | none → runs | 4.28 |
| mesh-150-0-200-600-s7 | 545 → 100 | none → runs | 2.77 |
| mesh-60-18-360-900-s37 | 787 → 33 | none → runs | 13.74 |

The search itself is cheaper than before: on the dumped problems it takes 2.5 s for all 8 908, the windows 2.6 s for
fewer ways, and the unbounded grid 9.9 s.

The same pass spreads routes that share a track, which explains part of the shared line pairs: mesh-100-0-120-500-s7
has 12 694 pairs after the first routing and 3 970 after the second on main; with the option no pass runs and 12 560
remain. Views without the pass on either side (mesh-200-60-120-500-s7) show the other part: longer routes through the
same gaps.

## Why 247 bus taps run through boxes

With [`bus-tap-causes.patch`](../experiments/bus-tap-causes.patch) and `bus_tap_causes.py` on `231c010`. The bus lines
are drawn from the bars after the layout (`crates/layout/src/bus_line.rs`: `draw`, `bars_to_lines`); the router's
repair does not draw them, so its bound does not change them. Of the 247 tap routes through boxes on the report's views
(1 674 tap routes):

| | Routes |
| --- | --- |
| in no box deeper than half a unit | 175 |
| a tap whose search found no clear way, so it kept the straight tap through the box: its grid over 40 000 cells | 35 |
| the same: no way within 240 units around the member and the near stretch of the bar | 18 |
| only the stretch along the bar to the head runs through a box (mostly another member's) | 19 |

- **175 grazes.** The tap drawing treats half a unit as free (`path_hits`, `tap_routed`'s `open`, and `tap_routed`
  merges grid lines closer than 0.5), and so does the engine's own conflict check for these lines; the counts here and the
  metrics do not. Example: a tap 0.26 units inside a box's lower edge.
- **53 failed tap searches**: `tap_routed` has its own bound and window, the same kind of limit as the router's.
- **19 along the bar**: a bar keeps clear of boxes that are not members of its bus, and `bars_to_lines` runs each line
  along the bar's middle, through members that stand on it.
- Also seen: some head taps end a few units beside their box (3.7 in one case; among the routes that end beside their box
  above).

None of this is fixed here: it changes the default drawing of bus views and needs an option and gates of its own.
Proposed as its own issue.

## Limits

- One generator family (`bus-mesh.mjs`, several sizes and seeds) carries all views over the bound that have hits, apart
  from the large flat views. The 11 views called `other` were chosen after the search was fixed.
- Edits: only a collapse and an expand of one container on two views, one run each (the option travels with the
  snapshot, every edge keeps a route, no geometry issues). Router routes through boxes after the collapse and after the
  expand, main → option: mesh-100-0-120-500 59 → 55 and 11 → 0; mesh-100-30-120-500 123 → 0 and 11 → 0. Drags, pins and
  refinements of over-bound views were not measured.
- Native times only; the WASM engine gives identical responses (`tools/platform-parity.mjs` has a request with the
  option) but was not timed.
- Memory was not measured; a sparse search holds at most 480 000 cells, 24 bytes each plus its queue.
