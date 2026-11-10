# Experiments (not merged)

Patches that were measured and not merged. Each applies to commit `1a514c8` (A2, first step).
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
