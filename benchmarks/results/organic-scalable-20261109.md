# Scalable organic placement (`scalableStress`) against d3-force and CoSE, 2026-11-09

Apple M3 (8 cores), Node v24.21.0, release `layout-command`; revision `603e016` plus the change that adds `scalableStress` (the
commit that adds this file). Load average 5.3 at the start and 2.0 at the end (not idle: times are indicative, the quality
measures are not affected). Command:

```sh
node benchmarks/eval/organic.mjs --out benchmarks/results/organic-scalable-20261109.json --sizes 500,1000,2000 \
  organic-25 organic-50 organic-100 organic-200 organic-300
```

Graphs, measures and engine settings as in [organic-20261109.md](organic-20261109.md): the corpus's organic ladders and
synthetic graphs of 500, 1000 and 2000 nodes (a random recursive tree plus 0.3 chords per node, seeded in the script), nodes of
180 × 54, straight centre-to-centre edges for every engine; d3-force 3.0.1 (300 ticks, seed 42), Cytoscape 3.34.4 CoSE (1000
iterations). Layerweave runs one process per call (times include process start and JSON transport). Raw values:
`organic-scalable-20261109.json`.

## What `scalableStress` does

1. Pivot MDS start (50 farthest-point pivots, as `sparseStress`).
2. Sparse stress whose pivot terms carry the size of their region (Ortmann, Klimenta and Brandes, GD 2016), plus terms for
   children two steps apart. Without those, siblings of one parent have identical targets and fall onto one point, which is why
   `sparseStress` leaves 431 overlaps at 500 nodes.
3. A deterministic force refinement in the manner of d3-force (reimplemented, no code copied): springs at √2 · the node spacing,
   Barnes–Hut repulsion, box collision with the gap, 300 ticks from temperature 0.5. The stress at the node spacing crowds
   graphs whose neighbourhoods grow fast (trees with hubs); the refinement spreads them until the boxes fit.
4. Under the option the repair uses the moderate scan budget (8000), and the conflicting routes are not routed again as a
   whole (with both engines or by `reroute`): at 2000 nodes that step did not finish in 30 minutes, and at 500 and 1000 nodes
   it removed no straight-through conflict (693 and 1990 with it, 717 and 2005 without it and with the moderate budget; the
   full budget took 6 times as long).

## Results

| graph | engine | overlaps | crossings | hits | edge CV | stress | angular | area/node | time |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| organic-100 | Layerweave (default) | 0 | **1** | **0** | 0.48 | **0.024** | **0.70** | **11.2** | 55 ms |
| organic-100 | Layerweave `scalableStress` | 0 | 3 | **0** | **0.15** | 0.029 | 0.68 | 26.7 | **47 ms** |
| organic-100 | d3-force | 0 | 6 | 4 | 0.25 | 0.133 | 0.62 | 39.1 | 56 ms |
| organic-300 | Layerweave (default) | 0 | **10** | 9 | 0.25 | **0.010** | **0.71** | **7.5** | 1.5 s |
| organic-300 | Layerweave `scalableStress` | 0 | **10** | **2** | **0.19** | 0.028 | 0.68 | 22.2 | **0.15 s** |
| organic-300 | d3-force | 0 | 53 | 37 | 0.33 | 0.226 | 0.59 | 41.2 | 0.22 s |
| synthetic 500 | Layerweave (default) | fails: bounded to 300 children per area | | | | | | | |
| synthetic 500 | Layerweave `sparseStress` | 431 | 10786 | 14192 | 0.73 | 0.366 | 0.22 | 2.4 | 26 s |
| synthetic 500 | Layerweave `scalableStress` | **0** | **1367** | **638** | 0.52 | **0.143** | **0.57** | 18.5 | 0.73 s |
| synthetic 500 | d3-force | **0** | 3563 | 929 | 0.44 | 0.190 | 0.51 | 25.4 | **0.50 s** |
| synthetic 500 | CoSE | 248 | 2152 | 2194 | **0.32** | 0.208 | 0.47 | 3.8 | 4.1 s |
| synthetic 1000 | Layerweave `scalableStress` | **0** | **5170** | **1795** | 0.57 | **0.148** | **0.59** | 19.6 | 1.9 s |
| synthetic 1000 | d3-force | **0** | 13981 | 2518 | 0.46 | 0.193 | 0.53 | 27.3 | **1.1 s** |
| synthetic 1000 | CoSE | 820 | 8309 | 7763 | **0.31** | 0.209 | 0.49 | 2.6 | 16.5 s |
| synthetic 2000 | Layerweave `scalableStress` | **0** | **21336** | **5389** | 0.64 | **0.164** | **0.59** | 19.0 | 7.0 s |
| synthetic 2000 | d3-force | **0** | 56686 | 7495 | 0.46 | 0.198 | 0.55 | 31.5 | **2.6 s** |
| synthetic 2000 | CoSE | 3259 | 37422 | 31225 | **0.30** | 0.216 | 0.49 | 2.1 | 67 s |

(25, 50 and 200 nodes in the raw file; the picture is the same.)

## Findings

- **Scale:** `scalableStress` lays out 500 to 2000 children per area without overlaps, with 2.6–2.7 times fewer crossings, 28–31 %
  fewer edges through nodes and 17–26 % lower stress than d3-force, on 27–40 % less area; d3-force is faster (0.5 against
  0.7 s at 500, 2.6 against 7.0 s at 2000). The edge lengths vary more than with d3-force (CV 0.52–0.64 against 0.44–0.46):
  hubs keep their neighbours at the stress distance, and the leaves of a hub spread around it.
- **Small areas:** up to 300 children it is as fast as the default dense placement on small areas and ten times faster at 300 and has fewer or equal
  hits, but higher stress and 2–3 times the area; the default stays the better drawing there, so the option is not a
  replacement for it.
- **Open:** the remaining edges through nodes are reported, not resolved (the repair is bounded, and the whole-drawing reroute
  is off under the option); time above 1000 nodes grows faster than linear (1.9 s at 1000, 7.0 s at 2000), and where it goes
  is not measured yet. Containers with large organic areas, continuation (drag) and the clustered force view are not tested.
