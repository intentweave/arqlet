# Polyline routing in single pass (decision record, 2026-11-21)

`polyline` and `roundedPolyline` are drawn by the scene from the orthogonal route (pulled taut); the layout routes such
edges orthogonally either way. Until now a graph with polyline routing fell back to the pipeline before single pass.
With `singlePassPolyline` (default on, `false` restores the fallback) it is laid out in single pass. Organic areas and
`straight` routing still need `singlePassMixed`; orthogonal output is unchanged.

Setup: Apple M3, macOS, loaded; release `layout-command`; before = `main` at `eb635df` plus #71, after = with this
change; single runs (the four 500-node inputs: medians of 3). Crossings and bends are counted on the layout routes
(orthogonal, horizontal × vertical strictly inside); node hits are the scene's `route-through-node` conflicts.

All benchmark datasets except the five with 10 000 nodes or more, with `defaultRouting: polyline` (36; 19 change):

| Sum | Before | After |
| --- | ---: | ---: |
| Routes through nodes (scene) | 0 | 0 |
| Crossings | 5 671 | **4 747 (−16 %)** |
| Bends | 8 789 | 8 622 (−2 %) |
| Time | 862 ms | 613 ms (−29 %) |

Per dataset (changed ones): crossings fall on 10, rise on 2 (activity-order 74 → 92 with bends 92 → 79; workflow-dag
136 → 553 with bends 572 → 789 and 4× the area). workflow-dag's polyline drawing is now its orthogonal default drawing:
the regression is the known gap between single pass and the older pipeline on that view, not a polyline effect. Bends
rise on bpmn-pools (+9), cloud-architecture (+18), network-vlan (+4), org-chart (+9), synthetic-2k (+61, crossings −434).

The explorer's two slow 500-node views (anonymized, private) through `elkLayout` with `elk.edgeRouting: POLYLINE`: the
long-edge view 1 778 → 509 ms (now the same drawing as `ORTHOGONAL`); the dense view 603 → 569 ms.

Orthogonal output byte-identical on all 41 datasets (default and explicit `orthogonal`) and the two private inputs
(84 cases); 15 refinement fingerprint views identical.
