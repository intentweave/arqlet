# Experiments (not merged)

Patches that were measured and not merged. Each applies to commit `1a514c8` (A2, first step) unless its section names
another.
Apply with `git apply benchmarks/experiments/<name>.patch`.

## `local-channel-candidate.patch`

Local channel candidates for placement search: the router keeps the routes of edges that do not touch a moved
leaf (`route_channel_subset`), refuses a subset when a kept route is off the grid, and the search reroutes only
the changed edges. Placement search still accepts candidates on the engine's own quality values.

Result (`profiles-20261021T150839Z`, all eight inputs): workflow-dag and synthetic-2k improve, but
network-vlan shared pairs go 1 → 45 and state-tcp hugging 3 → 4. Cause: the search accepts candidates on the
engine's quality values, which disagree with metrics v3 on shared lines.
Speed (single refinement, direct timing, not a results file): workflow-dag 49.8 s → 7.2 s.

## `local-plus-key.patch`

The same local candidates, plus the placement search compares candidates on the published key
(`published_key`, metrics v3 in objective order).

Result (`profiles-20261021T151434Z`): all inputs better or equal, but state-tcp hugging still 3 → 4. The local
candidate, not the key comparison, causes that regression: the full reroute with the same key comparison gives
hugging 2 on state-tcp (`profiles-20261021T151732Z`).

## Status

The key comparison alone is merged (`profiles-20261021T151732Z`: all inputs better or equal, run time unchanged).
The local candidate is not merged. To reoptimize it, the local candidate needs a path to the same results as the
full reroute on state-tcp, and the run time has to be measured against the full reroute on the same machine.

## `router-grid-variants.patch`

Applies to main `231c010`. The comparison of searches for the repair of a view over the router's grid bound (#203). With
`ARQLET_GRID_DUMP=FILE`, `layout-command` appends every such repair problem (the route, the obstacles, the stubs, and
whether today's windows find a way) to `FILE`; the example `grid_variants` of `arqlet-routing` runs variants on them:
`cargo run --release -p arqlet-routing --example grid_variants -- FILE full,gap,gap-turn24,lazy,follow`. Results in
`benchmarks/results/router-sparse-grid-20261126.md`. The variant that was chosen is in the router behind
`sparseRepairGrid`; the patch is not meant to be merged.

## `bus-tap-causes.patch`

Applies to main `231c010`. With `ARQLET_TAP_LOG=1`, `layout-command` logs how every bus tap was drawn and the bars of the
bus lines. `bus_tap_causes.py --bin BINARY --work DIR` classifies the taps through boxes (#203, same results file).
