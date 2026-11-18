# Route search on large views: byte-identical speedups and `routeSearchWindow` (2026-11-18)

Machine: Apple M3, macOS (Darwin 25.0.0), not idle (other work running; single runs, times are indications). Build:
`cargo build --release -p arqlet-runtime --bins`, branch `packing` on `671ffe4` plus this change. Graph of the large rows:
`benchmarks/datasets/synthetic-20k.json` (22 819 nodes, 61 082 edges, 2 819 containers). Each time is one
`layout-command` process with `{op: "import", graph, limit}`, reading and writing JSON included.

## Where the time went

A `sample` of the 5 000-node import (6 991 visible nodes, 10 079 edges): 73 % in the constructive routing, most of it the
monotone route search. Its lanes across the flow are the free gaps across the *whole* drawing (up to 4 005 lanes), and
the change of lane in a corridor tries every pair of lanes in a run of free pieces: 9.7 billion transitions for the lane
plans and 6.3 billion for the routes. More than half of the searches could not succeed at all: 4 254 had their first
step (from the port to the first corridor) blocked and 4 349 their last step, both independent of the lanes between.

## Byte-identical speedups (default output unchanged)

1. A search whose first or last step is blocked returns at once (the counters of searches, failures and states stay; only
   the bench counter of transitions drops).
2. An edge's gaps (`edge_gaps`) depend only on its obstacles: computed at its first search, shared by its copies.
3. An edge's obstacle index is built at its first search; many edges are never searched.
4. An edge's obstacles are taken from the boxes as id ranges between the ids it skips, instead of a set lookup per box.
5. The fallback repair of the routes (`routing::repair_raw`) looks the boxes' shapes up once and leaves out boxes by
   index; the cheap box test comes before the id test.
6. `loopSpace`'s prefilter reads the boxes by index instead of by id.

Checked: the import of every dataset in `benchmarks/datasets` (limit 1000, the protocol and overview sets left out), the
fixtures in `examples/fixtures` and synthetic-20k at limits 1 000, 2 000 and 5 000 are byte-identical to `671ffe4`
(snapshot JSON compared); the 15 refinement fingerprint views equal
`fingerprints-20261115-loop-space-default.jsonl` (`fingerprint_compare.py`: identical 15).

Tried and dropped: a change of lane priced for every lane at once by separable keys (ordered sets) instead of the pair
scan. To stay bit-identical it must price every candidate within rounding of the best, and the lanes reached in the
previous corridor all tie, so it was no faster.

| synthetic-20k, limit 5 000 | import |
|---|---|
| `671ffe4` | 77 s |
| 1. blocked ends | 63 s |
| + 2. and 4. | 58 s |
| + 3. and 6. | 49 s |
| + 5. | 47 s |

## `routeSearchWindow` (provisional, off)

With the option, a search takes only the gaps across the flow that reach within `reach` of its ends' lanes, `reach`
being its length along the flow and at least 16 tracks; when it finds no way there, it searches the whole width as
before. The lanes in the window are a subset of the full ones, so a route found in it lies on the same tracks.
`node benchmarks/eval/search-window.mjs DATASET[@LIMIT] ...` imports each dataset with and without it and scores both
with the shared metrics.

Same metrics with and without the window (line–node hits, sibling overlaps, crossings, bends, area, length):
activity-order, arqlet-pipeline, bpmn-pools, can-bus, cloud-architecture, deployment-cluster, er-diagram, fixed-ports, layerweave-code, mixed-large, mixed-layouts, nested-tree-radial, network-vlan, org-chart, organic-100, organic-200, organic-25, organic-300, organic-50, petri-net-shapes, petri-net, self-loops, state-order, state-tcp, synthetic-2k, tree-100, tree-200, tree-25, tree-300, tree-50, uml-class, workflow-dag, synthetic-20k@2000.

Different:

| view | variant | hits | crossings | bends | length | import |
|---|---|---|---|---|---|---|
| synthetic-20k@1000 | default | 678 | 17639 | 7606 | 6240449 | 3.8 s |
| synthetic-20k@1000 | window | 678 | 17611 | 7602 | 6237541 | 1.6 s |
| synthetic-20k@5000 | default | 18099 | 547449 | 41852 | 142140171 | 45.6 s |
| synthetic-20k@5000 | window | 18099 | 547015 | 41842 | 142118047 | 21.6 s |

In both, the window gives slightly fewer crossings and bends and shorter routes, at the same hits and area; on the views of
the benchmark set it changes nothing measured. Timing of the other views is the same (they have few lanes across).
Raw rows: `route-search-window-20261118.jsonl`.

The 10 000-node view of synthetic-20k (12 633 visible nodes, 23 584 edges), one run each after this change: 398 s with
the default (912 s before the loop-corner prefilter and this change), 214 s with the window. Its metrics were not scored.

Whether it becomes the default is open: the comparison against elk-rs and the refinement path are not measured yet.
