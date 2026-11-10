# Global layering, iteration 1: first view within a node budget, 2026-11-01

Machine: Apple M3, 8 logical CPUs, macOS 25.0.0, release `layout-command`, revision 693d6a0 plus uncommitted changes. Times
are whole `layout-command` calls (process start, JSON in and out), median of 3, option `layeredConstruction` on, import
with limit 10 000 (the whole graph). layerweave-code is the graph of 0bcfd56 (a7636dbb, 669 nodes, 1084 edges). The machine
was not idle; default output unchanged (16 of 16 refinement fingerprints).

What iteration 1 is (plan section 4a, "Global layering and a node budget"):
- `visibleNodeBudget` (experimental option, omitted when 0): the import collapses containers so that at most this many
  nodes are visible (leaves and collapsed containers; expanded containers are frames): the outermost containers first,
  then level by level (id order within a level) each container whose children fit is expanded
  (`crates/runtime/src/budget.rs`).
- A collapsed container takes a size estimated inside out (`collapsedSizes`, derived, travels with the snapshot): its
  children laid out in its flow (longest-path ranks along it, the largest rank across it, child containers estimated the
  same way), drawn as a miniature of a quarter of that size, never smaller than a leaf.
- The edges a collapsed container aggregates are drawn once per visible pair and direction (`view::merge_aggregated`);
  edges between two nodes visible as themselves stay as they are, parallel ones included. Without a budget nothing of this
  applies.
- Expanding is the existing toggle (a new layout of the view); with a budget it keeps the estimates and the merging.
- Playground: a "Node budget" field next to the example (an import; a change imports again). The default example in split
  mode loads the native snapshot and does not use it.

| dataset | budget | import | visible nodes (counted / with frames) | edges drawn | collapsed | extent |
|---|---|---:|---|---:|---:|---|
| layerweave-code | off | 117,449 ms | 606 / 669 | 1084 | 0 | 28714 x 17071 |
| layerweave-code | 30 | 176 ms | 30 / 42 | 101 | 24 | 6999 x 2039 |
| layerweave-code | 50 | 484 ms | 50 / 67 | 159 | 39 | 8658 x 3311 |
| layerweave-code | 100 | 2,993 ms | 100 / 126 | 221 | 37 | 11773 x 5439 |
| layerweave-code | 200 | 18,634 ms | 200 / 229 | 417 | 34 | 16202 x 14301 |
| deployment-cluster | off | 53 ms | 24 / 33 | 56 | 0 | 3388 x 1808 |
| deployment-cluster | 20 | 16 ms | 20 / 27 | 37 | 2 | 2730 x 1482 |

Without merging the aggregated edges, budget 50 drew 362 edges (most of them parallel between collapsed containers) and
took 3593 ms; budget 100 8301 ms, budget 200 35124 ms.

Expanding one collapsed container of the budget-50 view (`toggle`, a new layout of the view):

- expand mod:crates/runtime/src/components.rs (2 children): 521 ms; visible 69 ; box 180x54 -> 516x246
- expand mod:crates/runtime/src/reproduce.rs (8 children): 521 ms; visible 75 ; box 180x123 -> 518x546
- expand mod:crates/model/src/lib.rs (75 children): 4996 ms; visible 142 ; box 357x884 -> 1938x4098

Reading:
- The first view of the whole layerweave-code graph comes in about half a second with budget 50 (0.2 s with 30), against
  almost two minutes for all 669 nodes. The time grows steeply with the budget (3 s at 100, 19 s at 200): the layout of
  the view, not the selection, is what costs.
- An expanded container is four to five times its miniature (the quarter scale), so expanding still moves the outer
  picture; the estimate's proportions are those of the content.
- Expanding costs a new layout of the whole view (0.5 s, 5 s for a container of 75 children). Iteration 2 (children in
  the parent's common ranks) is where an expansion can be local.

Not done in this iteration: strategy 2 (a viewport with one hop beyond it), a mark of how many edges a drawn aggregated
edge stands for, the choice of which containers to expand by anything but level and id, and any check in a browser.
