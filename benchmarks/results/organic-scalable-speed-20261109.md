# Scalable organic placement: speed and edges through nodes, 2026-11-09

Follow-up to [organic-scalable-20261109.md](organic-scalable-20261109.md), same machine (Apple M3, 8 cores), Node v24.21.0,
release `layout-command`, revision `52d847f` plus the changes of the commit that adds this file. Load average 12.6 at the start
and 3.7 at the end (not idle: wall times are indicative). Command:

```sh
node benchmarks/eval/organic.mjs --out benchmarks/results/organic-scalable-speed-20261109.json --sizes 500,1000,2000 \
  organic-25 organic-50 organic-100 organic-200 organic-300
```

## Changes

Pure speedups (byte-identical output; the 64 corpus runs at threads 1 and 4 and 9 runs with options compared with `594151e`):

- `area_ranks` (every area, every layout): the Kahn ordering kept in-degrees and per-child link lists instead of scanning every
  link for every remaining child in every step, and the global layering reads each child's links instead of every link. Same
  choice rule (first child in id order without a link from a remaining child, else the first remaining child). It was 28 % of
  the time at 2000 children.

Under `scalableStress` only:

- When every edge can only be drawn straight, the orthogonal routes are not computed (the routes from port to port stand in
  for them; the edges a straight line cannot take are routed around the nodes afterwards, as before). The drawings of the
  synthetic graphs were identical with and without it (boxes and routes); it was 26 % of the time at 2000 children.
- The collision pass of the force refinement reads the neighbours of a node in one query (`SpatialHash::neighbors_after`).
- **Edge push** (algorithm change): in the last 150 of the 300 ticks, a node whose box (with half the gap) a straight edge
  crosses is pushed off the edge by half the depth per tick, the edge's ends a quarter each the other way.
- The repair uses the local scan budget (1500) instead of the moderate one: with the edge push, the same result at half the
  CPU time (500: 0.36 against 0.68 s CPU, 388 remaining straight-through reports either way; 2000: 2.23 against 3.1 s).

## Results

| graph | engine | overlaps | crossings | hits | edge CV | stress | angular | area/node | time |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| synthetic 500 | `scalableStress` before | 0 | **1367** | 638 | 0.52 | **0.143** | **0.57** | **18.5** | 0.73 s |
| synthetic 500 | `scalableStress` now | 0 | 1669 | **168** | 0.55 | **0.143** | **0.57** | **18.5** | **0.38 s** |
| synthetic 500 | d3-force | 0 | 3563 | 929 | **0.44** | 0.190 | 0.51 | 25.4 | 0.54 s |
| synthetic 1000 | `scalableStress` before | 0 | **5170** | 1795 | 0.57 | **0.148** | **0.59** | **19.6** | 1.9 s |
| synthetic 1000 | `scalableStress` now | 0 | 6124 | **510** | 0.59 | 0.149 | **0.59** | **19.6** | **0.90 s** |
| synthetic 1000 | d3-force | 0 | 13981 | 2518 | **0.46** | 0.193 | 0.53 | 27.3 | 1.18 s |
| synthetic 2000 | `scalableStress` before | 0 | **21336** | 5389 | 0.64 | **0.164** | **0.59** | **19.0** | 7.0 s |
| synthetic 2000 | `scalableStress` now | 0 | 24845 | **2163** | 0.64 | **0.164** | 0.57 | **19.0** | **2.3 s** |
| synthetic 2000 | d3-force | 0 | 56686 | 7495 | **0.46** | 0.198 | 0.55 | 31.5 | 2.7 s |

Organic ladders up to 300 nodes: no overlaps and no hits at every size (before: up to 4), crossings 0–10, 7–104 ms. Raw values
for every engine (default, `sparseStress`, CoSE as well): `organic-scalable-speed-20261109.json`.

## Findings

- **Time:** `scalableStress` is now faster than d3-force at 500, 1000 and 2000 nodes (0.38 / 0.90 / 2.3 s against 0.54 / 1.18
  / 2.7 s), although its times include process start and JSON transport and d3-force's do not.
- **Edges through nodes:** 3.5 to 5.5 times fewer than d3-force (168 against 929 at 500, 2163 against 7495 at 2000), and 60 to
  74 % fewer than before the edge push.
- **Crossings:** the edge push costs 16–22 % more crossings than before; they stay at less than half of d3-force's. A line
  through a node is a hard violation and ranks above a crossing, so the trade is accepted for the option.
- **Not measured:** where the remaining time goes above 2000 nodes, and the browser (WASM) times.
