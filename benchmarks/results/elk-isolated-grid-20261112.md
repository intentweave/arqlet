# ELK adapter: unconnected nodes in a grid, edge–node spacing (2026-11-12)

Apple M3, Darwin 25.0.0, rustc 1.99.0, Node 24.21.0; `elkLayout` in WASM under Node, single runs. Graphs: the six
anonymized ELK graphs of an external evaluation (36–500 nodes; not in the repository), all with
`elk.separateConnectedComponents: true`, `elk.spacing.edgeNode: 48`, `elk.layered.spacing.edgeNodeBetweenLayers: 48`.
Crossings: proper crossings between segments of different edges' sections.

**1. Unconnected nodes.** The adapter now turns on `isolatedGrid` unless `separateConnectedComponents` is `false` (ELK's
default is `true`), but only where nodes without any edge are at least half of the nodes.

| graph | nodes / unconnected | before | grid for all | grid where at least half (kept) |
|---|---|---|---|---|
| G1 | 36 / 0 | 4 668 × 1 236, 0 | same | same |
| G2 | 74 / 74 | 19 624 × 172, 0 | 2 520 × 1 972, 0 | 2 520 × 1 972, 0 |
| G3 | 256 / 256 | 60 032 × 172, 0 | 4 392 × 3 172, 0 | 4 392 × 3 172, 0 |
| G4 | 500 / 13 | 33 022 × 7 372, 616 | 40 244 × 7 644, 624 | 33 022 × 7 372, 616 |
| G5, G6 | 500 / 0, 2 | unchanged | unchanged | unchanged |

(ELK on G2 and G3: 1 896 × 1 084 and 3 192 × 1 804.) On G4 the grid, appended after the connected part, made the
drawing 26 % larger and added crossings: unconnected nodes mixed with several connected components need real
component packing (each component laid out on its own and the boxes packed), which is not done yet.

**2. Edge–node spacing.** `spacing.edgeNode` and `spacing.edgeNodeBetweenLayers` were reported as unknown; they now set
`routingClearance` (the larger of the two). Same crossings and times; the drawings grow to keep 48 px between routes and
nodes: G1 4 668 × 1 236 → 4 668 × 1 368, G4 × 7 372 → × 8 120, G5 × 61 948 → × 62 056, G6 × 23 244 → × 24 236.
