# Structure fixtures

Small graphs for the cases the M2 analysis and placement must handle (`docs/recursive-optimization.md` §13):

- `singleton.json`: one node, no edges.
- `disconnected.json`: two chains (`a-b-c`, `d-e-f`) and an isolated node `iso`.
- `forest.json`: two trees with no shared node; no cross-links.
- `tree-cross-link.json`: a tree `a-b, a-c, b-d` plus the cross-link `c-d`, so `d` has two parents.

Used by the `crates/layout` test `structure_fixtures_place_without_overlaps`. `bench` reduces a whole-graph disconnected input to its seed's component (see `benchmarks/results/structure-fixtures-20261023.md`). Not part of the benchmark manifest.
