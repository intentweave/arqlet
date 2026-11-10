# Local expansion of a collapsed container within a node budget, 2026-11-02

Machine: Apple M3, release `layout-command`, revision c1496ef plus uncommitted changes. Times are whole `layout-command`
calls (process start and JSON included), one run each; the machine was not idle. layerweave-code is the graph of 0bcfd56
(a7636dbb), imported whole (limit 10 000) with `visibleNodeBudget: 50` (66 visible nodes). Default output unchanged (16 of
16 refinement fingerprints).

Change (only in a view within a node budget):
- `toggle` that expands a container continues the earlier drawing instead of laying the view out anew: the container and
  the areas around it are the dirty areas; the other containers keep their boxes and plans.
- An area whose children all have their earlier boxes keeps their places (`push_grown`): a child that grew moves the
  children wholly right of its earlier box on by its growth in width and those wholly below it by its growth in height
  (the dummies of the area's plan likewise), and the area grows by what its content grew. Only the expanded container,
  whose children are new, is arranged.
- The local routing treats the edges whose ends moved (the edges the container aggregated, now ending at its children) as
  new edges; the routes whose ends moved together are carried along.
- Collapsing, and views without a budget, still lay the view out anew.

Expanding the smallest, a middle and the largest collapsed container ("moved": nodes outside the container whose absolute
place changed; "swapped": pairs of nodes outside it, apart along an axis before, that swap sides on that axis):

| options | container | children | full: ms / moved / swapped | local: ms / moved / swapped | routes rerouted (local) |
|---|---|---:|---|---|---:|
| default | components.rs | 2 | 122 / 5 / 1 | 41 / 20 / 0 | 72 |
| default | reproduce.rs | 8 | 138 / 22 / 10 | 45 / 22 / 19 | 90 |
| default | model/lib.rs | 75 | 367 / 60 / 7 | 108 / 40 / 5 | 202 |
| layeredConstruction | components.rs | 2 | 520 / 41 / 3 | 139 / 26 / 12 | 9 |
| layeredConstruction | reproduce.rs | 8 | 524 / 11 / 17 | 157 / 19 / 8 | 9 |
| layeredConstruction | model/lib.rs | 75 | 5009 / 59 / 96 | 553 / 41 / 6 | 10 |

(of about 3500 pairs; no geometry issues in any result.)

Reading:
- Local expansion is 3 to 9 times faster; with the layered construction it keeps 150–240 routes and builds 9–10 again.
- "Moved" counts the push by the growth, which is intended: the miniature is a quarter of the estimated size, so an
  expanded container grows four- to fivefold and what lies after it moves on by that.
- Inside one area the push cannot swap sides. The remaining swaps come from nesting: a node pushed on inside its container
  passes a node of another container that lies above or below it (no overlap results).

Open: collapsing locally, a miniature closer to the expanded size (less push), and a check in a browser.
