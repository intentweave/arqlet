# Lanes and phases as a grid (ADR 0022, M4b), 2026-11-26

Machine: Apple M3, macOS. Revision: branch `membership-phases` on `daca8b0` plus the change this file came with; release
`layout-command`. Command: `node benchmarks/experiments/membership-lanes.mjs` (all runs, the engine's included). No timing, no
comparison with elk-rs. The eight grids of the fixtures: a first dimension as lanes across the flow, a second as phases along it.

Crossings / bends:

| Grid (lanes x phases) | flat | first as containers | pins (probe) | engine |
| --- | --- | --- | --- | --- |
| `pools-and-lanes` pool x phase | 0 / 6 | 0 / 14 | 2 / 12 | 0 / 10 |
| `pools-and-lanes` phase x pool | 0 / 6 | 0 / 10 | 4 / 18 | 0 / 10 |
| `three-dimensions` zone x team | 0 / 15 | 0 / 13 | 2 / 31 | 0 / 17 |
| `three-dimensions` zone x layer | 0 / 15 | 0 / 13 | 12 / 29 | 0 / 19 |
| `three-dimensions` team x zone | 0 / 15 | 0 / 29 | 2 / 29 | 0 / 17 |
| `three-dimensions` team x layer | 0 / 15 | 0 / 29 | 9 / 31 | 0 / 21 |
| `three-dimensions` layer x zone | 0 / 15 | 1 / 23 | 2 / 23 | 0 / 21 |
| `three-dimensions` layer x team | 0 / 15 | 1 / 23 | 2 / 23 | 0 / 21 |

- Every grid of the engine has no crossing, against 2 to 12 with pins; bends are below the pins' in every grid and above the
  flat drawing's (15 to 21 against 15 on `three-dimensions`).
- Phases lie behind one another along the flow and lanes across it in every grid, with the pipeline before single pass too
  (`crates/runtime/tests/lanes.rs`); no geometry issue, no line-node hit.
- An edge from a later phase to an earlier one is turned for the ranking only and keeps its direction.
- Phases need lanes first: with the parent hierarchy first, lanes of a later criterion would lie inside each owner, which is not
  built.

Not measured on the corpus: its graphs have no second dimension.
