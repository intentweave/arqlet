# ADR 0012: Local edits: lay out and route only what an edit touches

Date: 2026-10-27.
Status: accepted and implemented for the legacy router (layered and organic areas); not for the channel router.

Extends [ADR 0006](0006-reactive-layout-session.md) (delta-driven repair). Measurements and findings:
`benchmarks/results/interaction-20261027/README.md`.

## Context

On the 585-node model of the repository's own code every edit that reached the layout took about as long as the import (65 s),
and laid out all 56 areas again. A drag step, a pin, a size report and a new node are small changes. The cost was in the repair
router's grid search (98 % of the samples), then in the corridor widening, which re-places the whole drawing from scratch as soon as one
corridor is too narrow, and then in routing every edge again.

## Decision

An edit that has an earlier drawing and a set of nodes to place again (a pin, a requested size, a new node, a size report) is a
**local edit**:

1. Only the edited nodes' areas and the containers above them are placed again (`local_dirty`, `place`). Other areas keep their boxes.
2. The corridor widening re-places only the areas it widens, starting from the widths the earlier drawing had
   (`Snapshot.corridorGaps`), and widens only corridors the edit touched.
3. The routes are made with `local_routes`: an edge whose two ends moved by the same distance keeps its earlier route, carried along;
   every other edge, and every kept route that now runs into a node that did not move with it, is routed again by the repair router
   (`repair_subset_counted`). A kept route that still runs into a node is repaired too.
4. After the edit the routes that run into a node are counted (only edges the earlier drawing had) and compared with the drawing
   before. If there are more, the complete layout decides and is kept when it has fewer (`editReport.completeFallback`). A step of a
   drag only reports it (`editReport.preview`, and a routing issue): the step that ends the gesture decides.

## Contract

A local edit is **not** byte-identical to a complete layout. It is:

- deterministic: the same edit on the same drawing gives the same result;
- binding: boxes, pins, requested sizes and container constraints hold;
- stable: the routes of edges the edit does not touch stay as they were;
- not worse: no more routes through nodes than before the edit, or than a complete layout with the same options.

Pure speed-ups (the obstacle grid of the repair router, the sibling grouping of `corridor_gaps`, the bounding-box prefilters) keep
the contract of [ADR 0001](0001-incremental-first.md): the output is byte-identical, checked on the 19 configurations of the
fingerprint comparison.

## Consequences

- A local edit on `layerweave-code` takes 0.27 s instead of 65 s (one machine under load; see the results file). That is usable, not
  smooth at 60 frames per second.
- The snapshot carries `corridorGaps` where a corridor was widened and `editReport` after a local edit. Against `origin/main` (729d818) the geometry of all 19 sweep configurations
  is identical; the full-snapshot fingerprint of 17 of them changed with `corridorGaps`, 2 are byte-identical.
- The corridor widths are only ever widened by local edits; a complete layout (or a refinement) recomputes them.
- The repair router used to give up above 120,000 grid cells, which this model was just below: a pin at a moved place or a widened
  corridor pushed many routes over it (node hits 43 to over 3,000; the same cause as the 121 routes through nodes of the import). Above
  the limit the grid is now taken from the obstacles near the route, in windows that grow; below it nothing changed (see the results file).
- Open: the same local routing for the channel router; stop signals and work budgets inside the placement (the core runs without a
  clock, a host that must interrupt uses a worker).
- Terminology: *warm start* (begin from the earlier positions), *reused analysis* (the graph distances of an organic area are kept for
  the next call with the same children and links), *continued solve* (`settle`: at most three damped stress steps per call, with the
  largest move reported). No solver state is kept in the snapshot.
