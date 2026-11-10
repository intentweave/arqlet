# A released drag on single pass: the drop as an order hint, 2026-11-07

Revision `b77ca74` plus these changes, Apple M3. One sample per view; no time is claimed.

## What changed

- `orderHints` (provisional `LayoutOptions` field, omitted when empty): node → the sibling it follows in its rank, or first.
  `placement_order::compute` puts the node there after all of the ordering, on the hierarchical pipeline of the default as
  well as under single pass. A hint whose sibling is in another rank has no effect. Hints are validated (a sibling under the
  same parent), mark their node for re-placement, and are part of the undo value (`Intent`).
- A drag of one node on a single-pass drawing (`singlePassAuto`): begin and updates only move the node as a preview and stay on
  single pass. A release turns the drop into a hint: after the last node of the rank whose middle across the flow lies before the
  drop's, then single pass lays the drawing out again. A finish without release pins the node, which still leaves single pass. A
  cancel puts the node back. A drag that moves several nodes keeps the old behaviour.
- The rank a drop lands in is not used yet: the node stays in its rank.

## Checks

- `crates/runtime/tests/single_pass.rs`: a released drag stays on single pass at every step, leaves the hint and no pin, and the
  node stands after its sibling, also after a later edit; a cancel restores the node and the options; the intent from before the
  drag undoes the hint; a hint set by `configure` holds on the default pipeline; invalid hints are rejected.
- `placement_order` unit test: the hint against the ordering, and a changed hint is not served from the order memo.
- `smoke.mjs`: the drag matches between WASM and native. `ui-regression.mjs` in all three modes passes.
- Default fingerprints: 19 configurations, threads 1 and 4, identical to a build of `d14db45`.

## Measured

`node benchmarks/eval/single-pass-edits.mjs`, raw [single-pass-drag-20261107.json](single-pass-drag-20261107.json). The drag moves
the first node of a rank with two nodes beyond the last one, across the flow, and releases it.

| View | stays single pass | order held | other nodes moved / max |
|---|---|---|---|
| layerweave-code (80 nodes) | yes | yes | 79 / 3330 |
| petri-net | yes | yes | 24 / 567 |
| state-tcp | yes | yes | 8 / 129 |
| workflow-dag, uniform | yes | yes | 72 / 995 |
| org-chart, uniform | yes | yes | 103 / 1287 |

On the default the same drag holds the order on 2 of 5 views, because a released node is placed again by the layout's rules.
A single-pass re-layout moves most other nodes; by the decision of 2026-11-07 that is reported, not gated.
