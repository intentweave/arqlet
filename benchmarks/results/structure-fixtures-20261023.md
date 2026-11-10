# Structure fixtures: placement and what the bench can show (2026-10-23)

Fixtures: `examples/fixtures/structure/` (singleton, disconnected, forest, tree with a cross-link). Tests: `structure_fixture_tests::structure_fixtures_place_without_overlaps` in `crates/layout` check that each fixture places with no geometry issues, one box per node and one route per edge.

Two findings:

1. The fixtures are placed correctly. The test runs `layout::compute` on the whole graph, which is the only way a disconnected graph reaches placement.
2. The bench cannot show the disconnected fixture. `import_graph` builds the view by breadth-first search from the highest-degree node, so `disconnected.json` (two chains and an isolated node) is reduced to the seed's chain (3 nodes, 2 edges). Views are always connected. A whole-graph disconnected input is therefore only seen by placement itself, not by refinement or the bench.

Baseline numbers from the bench for the fixtures are not recorded, because for `disconnected.json` they describe one chain, not the fixture. The singleton, forest and tree-with-cross-link fixtures are connected views and are reported by the bench as usual (`crossings`, `length`, `routingIssues`).

Consequence for M2: the component case that matters in practice is sibling components inside one container (`structure-corpus-20261023.md`), not disconnected whole graphs. The packing candidate should be measured on the corpus views' containers, not on the disconnected fixture alone.
