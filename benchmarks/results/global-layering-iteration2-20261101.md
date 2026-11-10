# Global layering, iteration 2: one level of containers in the common ranks, 2026-11-01

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release, bench with `--features benchmark-timings`, `--reps 3 --threads 1`,
limit 100 (er-diagram and tree-100: 1000). Revision 940dde0 plus uncommitted changes. layerweave-code is the graph of
0bcfd56 (a7636dbb). Default output unchanged (16 of 16 refinement fingerprints); with `layeredConstruction` alone the
output is that of 7d85608 (16 of 16). The machine was not idle.

What iteration 2 is (option `globalLayering`, needs `layeredConstruction`):
- In a layered area with a fixed, not reversed flow, every child container whose flow runs the same axis and way (with
  children, not pinned, no authored size, not a circle, no pinned child) is flattened: its children are units of the
  area's arrangement (`flattened_containers`). One level: a container inside a flattened one stays atomic (laid out on
  its own, inside out).
- Ranks, long-edge dummies and the order are those of the area's layered construction, over the units.
- Blocks: a flattened container (its children and the dummies of edges inside it), every other child, and the dummies of
  an edge between two blocks (the chain). Their order is the mean place of their elements after the sweeps; it is kept in
  every rank (each block one group of `improve_order`), the order inside a block is free.
- Coordinates across the flow: the blocks are packed one after another over the ranks they span (`block_skyline`), a
  container with its header and padding; along the flow the ranks where a container begins or ends get room for them
  (`container_main_margins`). Median sliding and straightening are not applied (a chain block is straight by
  construction); the channel widening is.
- A flattened container is the rectangle around its children; they are given in its frame.
- The plans: an edge's ends are the elements of the smallest area's plan that stand for them (`layered_plan::plan_ends`), so
  edges into and inside a flattened container are built from the common plan.

Columns: 7d85608 (per-area layout, `layered-tracks-20261101.jsonl`) / global layering.

| dataset | hard | crossings | bends | length |
|---|---|---|---|---|
| petri-net | 0 / 0 | 62 / 62 | 92 / 92 | 15.4k / 15.4k |
| er-diagram | 0 / 0 | 0 / 0 | 29 / 29 | 3.1k / 3.1k |
| state-order | 0 / 0 | 0 / 0 | 31 / 31 | 2.8k / 2.8k |
| activity-order | 0 / 0 | 54 / 54 | 70 / 70 | 19.0k / 19.0k |
| uml-class | 0 / 0 | 5 / 6 | 61 / 79 | 12.2k / 12.8k |
| bpmn-pools | 0 / 0 | 6 / 7 | 46 / 46 | 11.3k / 10.5k |
| network-vlan | 0 / 0 | 62 / 45 | 106 / 108 | 29.3k / 31.3k |
| layerweave-pipeline | 0 / 0 | 38 / 38 | 74 / 74 | 15.8k / 15.8k |
| cloud-architecture | 0 / 0 | 30 / 34 | 83 / 92 | 26.7k / 31.4k |
| mixed-layouts | 25 / 25 | 5 / 5 | 0 / 0 | 3.7k / 3.7k |
| nested-tree-radial | 0 / 0 | 0 / 0 | 88 / 88 | 18.1k / 18.1k |
| tree-100 | 196 / 196 | 0 / 0 | 0 / 0 | 27.9k / 27.9k |
| state-tcp | 0 / 0 | 0 / 0 | 36 / 36 | 5.9k / 5.9k |
| deployment-cluster | 0 / 0 | 107 / 174 | 136 / 146 | 59.9k / 40.4k |
| self-loops | 23 / 23 | 0 / 0 | 30 / 30 | 2.6k / 2.6k |
| layerweave-code | 0 / 0 | 900 / 1408 | 493 / 563 | 379.8k / 322.6k |
| **all 16** | 244 / 244 | 1269 / 1833 | 1375 / 1484 | 633k / 563k |

The construction after the import (a count, temporary test): deployment-cluster 9 of 18 section routes built (per area) and
10 of 20 (global), delegated 4 / 15 (TrackTaken); layerweave-code 103 of 110 / 90 of 95, delegated 4 / 25 (NotForward 8,
PortSide 5, TrackTaken 11).

Reading:
- One level in common ranks is worse than the per-area layout: crossings 1269 → 1833 (layerweave-code 900 → 1408,
  deployment-cluster 107 → 174), bends 1375 → 1484. network-vlan is better (62 → 45); petri-net and the datasets without
  same-axis containers are unchanged.
- In layerweave-code the crates are flattened into the root, but the edges run between functions in modules one level
  further down, which stay atomic: they are still searched, now between modules standing in three crowded common ranks.
- The order of the blocks is fixed from their mean place after the sweeps and is not improved afterwards; deployment-cluster
  delegates more edges (TrackTaken 4 → 15).

So "only the second level" does not pay on its own. What remains to be measured is all levels together (iteration 3): every
same-axis container flattened down to the leaves, which needs an order that keeps nested blocks contiguous while the
elements inside each block stay free (a hierarchical order, not one fixed group per block), and a block order that is
improved with the sweeps instead of fixed once.
