# Brute-force reference for the rank order, 2026-10-30

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, revision 0bcfd56 plus uncommitted changes, load about 5 (not idle).
Views: `--limit 100`. Script: `exact-order.mjs`. Raw rows: `exact-order-20261030.jsonl`.

For every fixed-flow area whose ranks admit at most 200,000 orders, the exact minimum of the bilayer crossings over all orders is
computed. Compared: the order the layout produced ("current") and the barycenter order (24 sweeps). Only edges between neighbouring
ranks count.

- Areas solved exactly: 29. Areas too large (more than 200,000 orders): 4 (petri-net root area with ranks 7-5-7-5, about 3.7 · 10^11
  orders; activity-order lane:warehouse; two layerweave-code modules).
- Barycenter equals the exact optimum in all 29 areas (total 0 crossings, same as the exact total).
- The current order is worse than the optimum in 2 areas (activity-order lane:customer: 1 crossing; layerweave-pipeline crate:runtime:
  2 crossings).

Reading: on the instances where the optimum is known, the barycenter order is optimal. The crossings of petri-net (30 in the
current order, 23 with barycenter, see order-probe-20261030.md) lie in the area that is too large to check exhaustively, so for that
area the optimum is not known. To get a reference there, the exhaustive search has to be replaced by an exact method that can use
bounds (branch and bound) or a per-pair exact minimisation over subsets, with the sweep order as the start.
