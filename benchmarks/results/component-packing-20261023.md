# Component packing (opt-in), corpus run (2026-10-23)

Option: `componentPacking` (default off). With it on, each weakly connected component of a container's children is ordered on its own and occupies a consecutive block of every level; the unpacked layout of the same view is a portfolio alternative, and the published key decides. Raw rows: `component-packing-20261023.jsonl` (packed); the default reference is `fingerprints-20261023-objective-order.jsonl`. Machine and revision are in each row.

Default output: unchanged (17 stable views identical; the 3 `layerweave-code` views have no stable reference).

Packed versus default, 20 views:

- Routed crossings: 9,256 to 9,280 (+24). Route length: 1,561,247 to 1,557,847 (-3,400). Mixed per view: activity-order 100 and layerweave-pipeline 100 improve; synthetic-2k 50 gets 28 more crossings and 2,109 less length; layerweave-code 100 gets 2 more crossings.
- The unpacked alternative is chosen on 7 of 20 views (cloud-architecture, deployment-cluster, layerweave-pipeline, org-chart, bpmn-pools, network-vlan, synthetic-2k 100): the packed layout loses the key there, and the published result equals the default.
- On the other views the published result is either the packed layout or the same routing as the default.

Reading: component packing changes the orderings only where a container has several sibling components, and the key does not prefer it consistently. The effect is small and mixed on the corpus, so it stays opt-in. A default change would need a larger effect, or a per-view advantage that the corpus shows consistently; neither is present.
