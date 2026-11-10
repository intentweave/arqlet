# Where the crossings of the layered construction arise, 2026-10-31

Revision 029b5e0 (joint order) for "on" (`layeredConstruction`), the existing path for "off". Refined snapshots from the layout command
(`import` with the option, `refine` with one thread), limit 100; constructed edges from `bench --repair-work 1` on the same drawing.
Script: `benchmarks/eval/crossing-diagnosis.py` (crossing points of axis-aligned segments, strictly inside both segments; pairs of edges
that share an end node are left out, as in the metric's crossing count). Deterministic.

Segment kinds: "port" is the first or last segment of a route; "across" a segment across the area's flow (a lane change); "along" a
segment along the flow that is not a port segment. Route kinds: C constructed, G routed by the general router.

| dataset | crossing points off → on | of "on": pairs that did not cross off | C–C | C–G | G–G | across + port | across + along | other |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| petri-net | 52 → 91 | 66 | 29 | 46 | 16 | 54 | 36 | 1 |
| activity-order | 35 → 87 | 76 | 1 | 38 | 48 | 61 | 26 | 0 |
| network-vlan | 31 → 37 | 24 | 0 | 9 | 28 | 19 | 9 | 9 |
| layerweave-pipeline | 16 → 16 | 8 | 3 | 6 | 7 | 5 | 8 | 3 |
| layerweave-code | 1040 → 1490 | 1065 | 821 | 580 | 89 | 801 | 689 | 0 |
| total | 1174 → 1721 | 1239 | 854 | 679 | 188 | 940 | 768 | 13 |

Length of the port segments (first and last segment of each route with at least one bend): constructed routes have a median of 30–52
units on all datasets; on layerweave-code their mean is 267 units against 140 for the existing path (88 of 276 longer than 100 units),
on the small datasets their mean is 33–36 units.

Reading:
- Almost every crossing (99 %) is a lane change across the flow that crosses a segment along the flow: another edge's port segment
  (55 %) or its run along the flow (45 %).
- Crossings between two constructed routes are 50 % of all; on layerweave-code 55 %. Most of the extra crossings are pairs that did not
  cross in the existing path (1239 of 1721 points).
- The general router scores every candidate by its crossings with the routes already drawn before its length. The construction's search
  scores only length and bends: a lane change that crosses five runs of other edges costs the same as one that crosses none. That fits
  the segment statistics, and it explains why ordering the lane changes, the passages and the ports did not lower the crossings: none of
  them made a crossing cost anything in the search.
- Next step: count the crossings with the routes already built as a cost in the construction's search (before length, as the general
  router does), using the index that the search already has for the built segments.
