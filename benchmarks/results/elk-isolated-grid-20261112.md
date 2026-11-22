# ELK adapter: unconnected nodes in a grid, edge–node spacing (2026-11-12)

Apple M3, Darwin 25.0.0, rustc 1.99.0, Node 24.21.0; `elkLayout` in WASM under Node, single runs. Graphs: six ELK graphs
of an external evaluation (36–500 nodes; not in the repository, their figures not published), all with
`elk.separateConnectedComponents: true`, `elk.spacing.edgeNode: 48`, `elk.layered.spacing.edgeNodeBetweenLayers: 48`.
Crossings: proper crossings between segments of different edges' sections.

**1. Unconnected nodes.** The adapter now turns on `isolatedGrid` unless `separateConnectedComponents` is `false` (ELK's
default is `true`), but only where nodes without any edge are at least half of the nodes.

On the graphs made only of unconnected nodes the drawing went from one long row to a compact grid, close to ELK's area.
On a graph with a few unconnected nodes among several connected components, a grid for all of them, appended after the
connected part, made the drawing larger and added crossings; the rule "at least half unconnected" leaves such a graph
unchanged. Unconnected nodes mixed with several connected components need real component packing (each component laid
out on its own and the boxes packed), which is not done yet.

**2. Edge–node spacing.** `spacing.edgeNode` and `spacing.edgeNodeBetweenLayers` were reported as unknown; they now set
`routingClearance` (the larger of the two). Same crossings and times; the drawings grow to keep 48 px between routes and
nodes.
