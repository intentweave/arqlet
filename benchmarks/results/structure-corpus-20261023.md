# Structure of the benchmark views (2026-10-23)

Source: `bench --threads none` on the 20 corpus views; raw rows with `structure` per parent in `structure-corpus-20261023.jsonl` (machine and revision in each row). The analysis is `layout::structure` (weak components, strongly connected components, forest and DAG flags), applied to the children of each container as `place` sees them.

| View | Nodes | Parents | Most components in one parent | Isolated nodes (size-1 components) | DAG | Forest |
| --- | --- | --- | --- | --- | --- | --- |
| cloud-architecture 100 | 34 | 15 | 2 | 8 | no | no |
| activity-order 100 | 31 | 5 | 7 | 1 | no | no |
| deployment-cluster 100 | 33 | 10 | 3 | 10 | no | no |
| state-order 100 | 11 | 2 | 1 | 0 | no | no |
| state-tcp 100 | 11 | 1 | 1 | 0 | no | no |
| workflow-dag 100 | 71 | 9 | 10 | 63 | no | no |
| layerweave-pipeline 100 | 32 | 7 | 2 | 3 | no | no |
| org-chart 100 | 104 | 34 | 4 | 55 | no | no |
| uml-class 100 | 20 | 4 | 1 | 0 | no | no |
| er-diagram 100 | 24 | 1 | 1 | 0 | no | no |
| bpmn-pools 100 | 26 | 6 | 3 | 3 | no | no |
| network-vlan 100 | 37 | 6 | 1 | 0 | no | no |
| petri-net 100 | 24 | 1 | 1 | 0 | no | no |
| layerweave-code 25 | 34 | 10 | 2 | 9 | yes | no |
| layerweave-code 50 | 60 | 11 | 2 | 6 | no | no |
| layerweave-code 100 | 117 | 18 | 3 | 15 | no | no |
| synthetic-2k 25 | 42 | 18 | 4 | 20 | no | no |
| synthetic-2k 50 | 76 | 27 | 5 | 33 | no | no |
| synthetic-2k 100 | 147 | 48 | 6 | 60 | no | no |
| intentweave-export 100 | 109 | 10 | 19 | 25 | no | no |

Reading:
- Correction: these are sibling components inside one container. Two children are linked only when an edge joins their descendants directly, so siblings connected only through another container count as separate. 17 of the 20 views have such sibling components. The views themselves are always connected: `import_graph` builds each view by breadth-first search from the highest-degree node, so a whole-graph disconnected input never reaches refinement through a view.
- Isolated nodes are common (for example 63 of 71 in workflow-dag 100). Both the graph and the view limit contribute; this is the input's structure, not a layout result.
- No view is a forest. Every view with an undirected cycle, or a directed cycle, is a case for the cycle handling in `place`.
- The analysis is reporting only. It changes no output: the 17 stable views are identical to `fingerprints-20261023-objective-order.jsonl`.
