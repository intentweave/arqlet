# Global layering, iteration 3: every level of same-axis containers in the common ranks, 2026-11-01

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`, `--reps 3 --threads 1`,
limit 100 (er-diagram and tree-100: 1000). Revision 371a5c3 plus uncommitted changes. layerweave-code is the graph of 0bcfd56
(a7636dbb). Default output unchanged (16 of 16 refinement fingerprints); with `layeredConstruction` alone the output is that
of 7d85608 (16 of 16). The machine was not idle; times are rough.

What iteration 3 adds to iteration 2 (option `globalLayering`):
- Flattening is recursive (`global::Flattening`): every container of the same flow axis and way inside the area, down to the
  leaves; a unit knows its path of containers. The dummies of an edge belong to the deepest container holding both ends,
  as a chain block of their own.
- Order (`layered_plan::hierarchical_order`): barycentre sweeps down and up; a block's key is the mean of its elements' keys
  over all its ranks; every rank is sorted along the paths, so nested blocks stay contiguous and keep one place among their
  siblings in every rank; the best order (fewest bilayer crossings) is kept.
- Ranks: with flattened containers, the cycles run across levels; the order in which the cycle break took the nodes is
  used as a topological order (links against it reversed) and the ranks are the longest paths along it, so no link lies
  inside one rank. (Without it, network-vlan put fw-a and core-a in one rank and the general router drew e3 between them
  4 units from both, under the required clearance of 6: 2 hard violations.)
- Coordinates: nested skyline packing (`global::skyline`), margins along the flow summed over nested containers that begin
  or end in one rank (`global::main_margins`), rectangles from the inside out (`global::frames`).

Columns: per-area layout (7d85608, `layered-tracks-20261101.jsonl`) / iteration 2 (one level) / iteration 3.

| dataset | hard | crossings | bends | length | refinement ms |
|---|---|---|---|---|---|
| petri-net | 0 / 0 / 0 | 62 / 62 / 62 | 92 / 92 / 92 | 15k / 15k / 15k | 21 / 21 |
| er-diagram | 0 / 0 / 0 | 0 / 0 / 0 | 29 / 29 / 29 | 3k / 3k / 3k | 11 / 11 |
| state-order | 0 / 0 / 0 | 0 / 0 / 0 | 31 / 31 / 31 | 3k / 3k / 3k | 3 / 3 |
| activity-order | 0 / 0 / 0 | 54 / 54 / 54 | 70 / 70 / 70 | 19k / 19k / 19k | 40 / 33 |
| uml-class | 0 / 0 / 0 | 5 / 6 / 6 | 61 / 79 / 79 | 12k / 13k / 13k | 19 / 22 |
| bpmn-pools | 0 / 0 / 0 | 6 / 7 / 7 | 46 / 46 / 46 | 11k / 11k / 11k | 19 / 14 |
| network-vlan | 0 / 0 / 0 | 62 / 45 / 43 | 106 / 108 / 104 | 29k / 31k / 32k | 32 / 41 |
| layerweave-pipeline | 0 / 0 / 0 | 38 / 38 / 38 | 74 / 74 / 74 | 16k / 16k / 16k | 33 / 33 |
| cloud-architecture | 0 / 0 / 0 | 30 / 34 / 31 | 83 / 92 / 82 | 27k / 31k / 25k | 196 / 21 |
| mixed-layouts | 25 / 25 / 25 | 5 / 5 / 5 | 0 / 0 / 0 | 4k / 4k / 4k | 23 / 22 |
| nested-tree-radial | 0 / 0 / 0 | 0 / 0 / 0 | 88 / 88 / 88 | 18k / 18k / 18k | 72 / 72 |
| tree-100 | 196 / 196 / 196 | 0 / 0 / 0 | 0 / 0 / 0 | 28k / 28k / 28k | 31 / 31 |
| state-tcp | 0 / 0 / 0 | 0 / 0 / 0 | 36 / 36 / 36 | 6k / 6k / 6k | 5 / 5 |
| deployment-cluster | 0 / 0 / 0 | 107 / 174 / 138 | 136 / 146 / 140 | 60k / 40k / 40k | 87 / 108 |
| self-loops | 23 / 23 / 23 | 0 / 0 / 0 | 30 / 30 / 30 | 3k / 3k / 3k | 7 / 7 |
| layerweave-code | 0 / 0 / 10 | 900 / 1408 / 1098 | 493 / 563 / 640 | 380k / 323k / 290k | 1317 / 875 |
| **all 16** | 244 / 244 / 254 | 1269 / 1833 / 1482 | 1375 / 1484 / 1541 | 633k / 563k / 524k | 1915 / 1320 |

Where the crossings arise (a count after the import, temporary test): with global layering the routes follow the order
almost exactly on layerweave-code (1010 bilayer crossings of the order, 1011 routed); per area the order has 216 within its
areas and the drawing 867. On deployment-cluster it is the other way round: the global order has 26, the drawing 164 (per
area 9 and 124); 56 of 56 edges have a route from the plan, 36 are built from it, 8 are delegated (TrackTaken).

Tried and not kept (measured on the datasets that change):

| variant | uml-class | bpmn-pools | network-vlan | cloud-architecture | deployment-cluster | layerweave-code |
|---|---:|---:|---:|---:|---:|---:|
| crossings, sweeps only (kept) | 6 | 7 | 43 | 31 | 138 | 1098 |
| + transpositions inside a block | 6 | 7 | 43 | 31 | 138 | 1177 |
| + transpositions of sibling blocks | 6 | 7 | 38 | 25 | 151 | 1207 (4.2 s) |

The transpositions lower the crossings of the order (layerweave-code 1010 → 932 → 820) but not those of the drawing, and the
block transpositions recount every crossing for each trial.

Reading:
- All levels together beat one level (crossings 1833 → 1482) and the per-area layout on network-vlan (62 → 43); the length
  is 17 % shorter and the refinement faster in total (1.9 s → 1.3 s). They lose on layerweave-code (900 → 1098) and
  deployment-cluster (107 → 138), and in total (1269 → 1482); bends rise (1375 → 1541).
- On layerweave-code the drawing is as good as the order: the order of the common ranks is the weak part (1010 crossings;
  per area the orders add up to 216, and the search between the areas adds the rest to 867).
- layerweave-code keeps 10 hard violations: searched routes of edges that run backwards in the common ranks pass along
  their own end node 3 units away (required 6), e.g. e728 below `compute`.

Open: a better order of the common ranks (the order per area is a strong start: it could be the initial order of the
blocks), routes of backward edges in the common ranks, and the edges the construction delegates on deployment-cluster.
