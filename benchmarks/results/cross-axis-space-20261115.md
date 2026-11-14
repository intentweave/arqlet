# Free space beside stacked containers (2026-11-15)

Machine: Apple M3, macOS (Darwin 25.0.0), not idle. Build: `cargo build --release -p arqlet-runtime --bins` at e05add4 plus the uncommitted change. Each dataset of `benchmarks/datasets` at limit 150 (as `s1-4-defaults.mjs`), default against the variant, one metric implementation (`benchmarks/eval/lib.mjs`); only datasets whose metrics differ are listed. Columns: h line–node hits, c crossings, b bends, a area (k units²).

Occasion: in cloud-architecture the subnets of an availability zone (lr containers in a tb zone) stood offset from each other, and a private subnet had about 140 units free before its first node.

## `sharedBorderRanks`

A container across its parent's flow put a crossing edge's border node in a rank of its own before a rank of side nodes only, so one rank held nothing but that edge's long-edge dummy (private-a: border, dummy, then app-a). The border nodes now share such a rank with the side nodes, and a rank of border and side nodes counts as a border rank (border nodes ordered by their lanes on their own places, the narrow gap).

```
cloud-architecture           DIFF  default h0 c22 b114 a4637k | variant h0 c22 b114 a4429k
org-chart                    DIFF  default h0 c37 b337 a19821k | variant h0 c37 b335 a17884k
synthetic-2k                 DIFF  default h0 c2792 b1613 a61421k | variant h0 c2779 b1594 a62193k
```

Crossings and bends never worse; synthetic-2k 1.3 % more area.

## `straightenOffsets` (containers only, crossings of whole chains guarded)

After the placement across the flow, a container moves across the flow where that puts one of its edges on the line of the port it meets in the neighbouring rank (the side ports as drawn, `layered_plan::side_port`), pushing virtual nodes beside it, within its gaps and the drawing's extent, when fewer segments stay bent and the channels of the moved edges get no crossing more. cloud-architecture: the three subnets of zone a stand flush, web-a → app-a → db-a run straight.

```
activity-order               DIFF  default h0 c69 b79 a1935k | variant h0 c65 b92 a1404k
arqlet-pipeline              DIFF  default h0 c22 b80 a3380k | variant h0 c22 b74 a3056k
bpmn-pools                   DIFF  default h0 c10 b59 a1940k | variant h0 c10 b57 a1946k
cloud-architecture           DIFF  default h0 c22 b114 a4637k | variant h0 c22 b106 a4646k
deployment-cluster           DIFF  default h0 c95 b227 a4711k | variant h0 c95 b227 a4720k
layerweave-code              DIFF  default h0 c655 b1127 a81360k | variant h0 c655 b1145 a81339k
network-vlan                 DIFF  default h0 c23 b106 a3043k | variant h0 c23 b104 a3043k
org-chart                    DIFF  default h0 c37 b337 a19821k | variant h0 c59 b326 a15585k
synthetic-2k                 DIFF  default h0 c2792 b1613 a61421k | variant h0 c3214 b1478 a50805k
uml-class                    DIFF  default h0 c3 b49 a3627k | variant h0 c3 b45 a3627k
workflow-dag                 DIFF  default h0 c433 b785 a11827k | variant h0 c433 b779 a11588k
```

Better on cloud-architecture, network-vlan, arqlet-pipeline, bpmn-pools, uml-class and workflow-dag. Worse on org-chart (+22 crossings), synthetic-2k (+422 crossings), activity-order and layerweave-code (+13 and +18 bends), unchanged by the local crossing guard and with no area turning its direction: the cause lies outside the moved channels; open. Earlier tries on the way: leaves too (network-vlan 23 → 38 crossings, org-chart the same as now).

## Rejected: `exactSideLanes`

Giving the placement the side ports as drawn (instead of a share of the child's side) repeated 2026-11-07: fewer bends (org-chart −62, synthetic-2k −135) but much more area (org-chart +48 %, workflow-dag +86 %) and more crossings on network-vlan and workflow-dag; removed.

## Rejected: stacking sibling containers of a cycle in one rank

A prototype ranked strongly connected sibling containers as one (their edges delegated to the general router): the zones of cloud-architecture stood one above the other, 8 bends fewer, but 9 % more area and a much taller drawing; dropped in favour of the two options above.

Default fingerprints identical on all 15 views; 852 tests.

## Replaced: `alignStacked` instead of `straightenOffsets` (decided in the plan)

The pass that moved containers after the placement was replaced by a decision of the plan: a stack edge (the upper
container's only edge toward the next rank and the lower one's only edge from the previous rank) forms a block of the
placement across the flow (`block_placement`) with priority, at the containers' exact ports (side port as drawn, border
node lane); crossing segments give way as for segments between dummies. Without the stack condition (any single edge
between two containers) uml-class got 2 bends and layerweave-code 6 crossings and 64 bends more.

`alignStacked` alone:

```
arqlet-pipeline              DIFF  default h0 c22 b80 a3380k | variant h0 c22 b78 a3390k
cloud-architecture           DIFF  default h0 c22 b114 a4637k | variant h0 c22 b108 a4569k
synthetic-2k                 DIFF  default h0 c2792 b1613 a61421k | variant h0 c2788 b1601 a61421k
```

With `sharedBorderRanks` (the new default):

```
arqlet-pipeline              DIFF  default h0 c22 b80 a3380k | variant h0 c22 b78 a3390k
cloud-architecture           DIFF  default h0 c22 b114 a4637k | variant h0 c22 b108 a4308k
org-chart                    DIFF  default h0 c37 b337 a19821k | variant h0 c37 b335 a17884k
synthetic-2k                 DIFF  default h0 c2792 b1613 a61421k | variant h0 c2780 b1578 a62193k
```

No crossing and no bend more on any dataset. Fingerprints: 4 of 15 views changed (cloud-architecture, arqlet-pipeline,
org-chart, synthetic-2k), new reference `fingerprints-20261115-stacked-default.jsonl`; with both options `false` the
geometry equals `fingerprints-20261114-channels-by-demand-default.jsonl` (metadata only); threads 1 and 4 identical.
