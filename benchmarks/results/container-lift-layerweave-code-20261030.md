# Cross-container edges lifted to their common container, layerweave-code, 2026-10-30

Revision 0bcfd56 plus uncommitted changes. View `--limit 100`. Edges: the 71 edges between different containers from
`repair-work-layerweave-code-20261030.json` (94 % of the refinement's repair search, 2,107,218 grid pops). Deterministic.

For each edge, the smallest container that holds both ends was found, and the two children of that container that hold the
ends. On the level of that container the edge is an edge between two sibling nodes (the children, mostly containers themselves).

- 71 edges become 21 distinct sibling pairs on their common level.
- On average an edge climbs 3.3 container levels (source side plus target side) to reach the common level.
- Every common container has a fixed left-to-right flow.
- Largest pairs: root level crate:runtime → crate:model (18 edges), crate:runtime → crate:layout (9), crate:runtime → crate:reference
  (6), crate:runtime → crate:view (4); inside crate:runtime, bench.rs → lib.rs (7).

Reading: on the common level these are ordinary edges between siblings in a layered area, the case the constructor already builds,
except that the endpoints are containers (the constructor rejects container endpoints today). The rest of each edge runs inside the
containers, from the end to the container's border, level by level.
