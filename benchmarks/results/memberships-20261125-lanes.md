# Lanes with what the engine has: feasibility (ADR 0022, M4), 2026-11-25

Machine: Apple M3, macOS. Engine revision `4b69900`, release build of `layout-command`. No timing. Command:
`node benchmarks/experiments/membership-lanes.mjs`. 20 views: six with one dimension and eight grids of two on the fixtures, and six
corpus graphs whose containers become the lanes. `x`: crossings with shared ends.

## Findings

- **The geometry works.** Nodes pinned across the flow into bands, and helper edges for bands along the flow: no node outside its
  lane or phase in any of the 20 views, `pools-and-lanes` as the 2x3 grid included; all 454 pins honoured; no new line-node hit,
  overlap or shared line; all by single pass.
- Along the flow this needs the edges that run back between phases to be turned for the layout. Without that the cycle break turns
  helper edges instead, and all eight grids failed (87 nodes in the stretch of another phase).
- **The order does not.** Crossings against the flat drawing without lanes, declared order: `bpmn-pools` 1 to 16, `network-vlan` 33 to
  147, `org-chart` 47 to 165, `workflow-dag` 200 to 506, `layerweave-code` 422 to 2559. The same groups as containers: 2, 37, 19, 230
  and 469. The grids of the fixtures are 2 to 12.
- The order of the bands matters (`bpmn-pools` 16 to 5 with the best of six, `network-vlan` 147 to 99), but the declared order is the
  user's choice (ADR 0022 section 1), so it cannot be the engine's lever by default.
- Sorting the nodes inside a band by the mean place of their neighbours changes nothing in 13 views (the order is the same),
  makes 6 worse and `bpmn-pools` better (16 to 14). Pins fix nodes only: the dummies of long edges are ordered by the engine, without the bands.
  That they cause the crossings is an assumption, not measured.

Lanes on flat graphs: containers inside lanes (rank 2) were not tried.

## Runs

```
org-chart-projects.json site (7 nodes, 9 edges, 2 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 13 len 1662 area 0.20M
  containers hits 0 overlaps 0 shared 0 x 0 b 13 len 1106 area 0.32M
  lanes hits 0 overlaps 0 shared 0 x 5 b 17 len 2066 area 0.24M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 5 b 17 len 2066 area 0.24M violations 0/0
  best hits 0 overlaps 0 shared 0 x 1 b 17 len 2000 area 0.24M order munich,berlin
pools-and-lanes.json pool (7 nodes, 7 edges, 2 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 6 len 611 area 0.18M
  containers hits 0 overlaps 0 shared 0 x 0 b 14 len 1606 area 0.23M
  lanes hits 0 overlaps 0 shared 0 x 2 b 14 len 1750 area 0.40M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 2 b 14 len 1750 area 0.40M violations 0/0
  best hits 0 overlaps 0 shared 0 x 2 b 14 len 1750 area 0.40M order customer,supplier
pools-and-lanes.json phase (7 nodes, 7 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 6 len 611 area 0.18M
  containers hits 0 overlaps 0 shared 0 x 0 b 10 len 1383 area 0.38M
  lanes hits 0 overlaps 0 shared 0 x 0 b 12 len 912 area 0.34M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 0 b 12 len 912 area 0.34M violations 0/0
  best hits 0 overlaps 0 shared 0 x 0 b 12 len 912 area 0.34M order request,approval,fulfil
pools-and-lanes.json pool x phase (7 nodes, 7 edges, 2 lanes, 3 phases)
  flat hits 0 overlaps 0 shared 0 x 0 b 6 len 611 area 0.18M
  containers hits 0 overlaps 0 shared 0 x 0 b 14 len 1606 area 0.23M
  lanes hits 0 overlaps 0 shared 0 x 2 b 12 len 1222 area 0.29M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 2 b 12 len 1222 area 0.29M violations 0/0
  best hits 0 overlaps 0 shared 0 x 0 b 14 len 1304 area 0.30M order supplier,customer
pools-and-lanes.json phase x pool (7 nodes, 7 edges, 3 lanes, 2 phases)
  flat hits 0 overlaps 0 shared 0 x 0 b 6 len 611 area 0.18M
  containers hits 0 overlaps 0 shared 0 x 0 b 10 len 1383 area 0.38M
  lanes hits 0 overlaps 0 shared 0 x 4 b 18 len 1839 area 0.22M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 4 b 18 len 1839 area 0.22M violations 0/0
  best hits 0 overlaps 0 shared 0 x 4 b 18 len 1839 area 0.22M order request,approval,fulfil
three-dimensions.json zone (9 nodes, 12 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 0 b 13 len 2049 area 0.52M
  lanes hits 0 overlaps 0 shared 0 x 11 b 33 len 4661 area 0.45M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 11 b 33 len 4661 area 0.45M violations 0/0
  best hits 0 overlaps 0 shared 0 x 6 b 35 len 5212 area 0.47M order internal,dmz,restricted
three-dimensions.json team (9 nodes, 12 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 0 b 29 len 4854 area 0.60M
  lanes hits 0 overlaps 0 shared 0 x 5 b 31 len 4037 area 0.45M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 5 b 31 len 4037 area 0.45M violations 0/0
  best hits 0 overlaps 0 shared 0 x 5 b 31 len 4037 area 0.45M order platform,payments,data
three-dimensions.json layer (9 nodes, 12 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 1 b 23 len 3050 area 0.51M
  lanes hits 0 overlaps 0 shared 0 x 14 b 33 len 5343 area 0.46M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 14 b 33 len 5343 area 0.46M violations 0/0
  best hits 0 overlaps 0 shared 0 x 14 b 33 len 5343 area 0.46M order api,logic,store
three-dimensions.json zone x team (9 nodes, 12 edges, 3 lanes, 3 phases)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 0 b 13 len 2049 area 0.52M
  lanes hits 0 overlaps 0 shared 0 x 2 b 31 len 3174 area 0.62M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 2 b 31 len 3174 area 0.62M violations 0/0
  best hits 0 overlaps 0 shared 0 x 1 b 29 len 3642 area 0.61M order restricted,dmz,internal
three-dimensions.json zone x layer (9 nodes, 12 edges, 3 lanes, 3 phases)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 0 b 13 len 2049 area 0.52M
  lanes hits 0 overlaps 0 shared 0 x 12 b 29 len 3921 area 0.61M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 12 b 29 len 3921 area 0.61M violations 0/0
  best hits 0 overlaps 0 shared 0 x 6 b 29 len 4357 area 0.61M order dmz,restricted,internal
three-dimensions.json team x zone (9 nodes, 12 edges, 3 lanes, 3 phases)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 0 b 29 len 4854 area 0.60M
  lanes hits 0 overlaps 0 shared 0 x 2 b 29 len 3341 area 0.54M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 2 b 29 len 3341 area 0.54M violations 0/0
  best hits 0 overlaps 0 shared 0 x 0 b 29 len 3615 area 0.54M order payments,platform,data
three-dimensions.json team x layer (9 nodes, 12 edges, 3 lanes, 3 phases)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 0 b 29 len 4854 area 0.60M
  lanes hits 0 overlaps 0 shared 0 x 9 b 31 len 3981 area 0.60M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 9 b 31 len 3981 area 0.60M violations 0/0
  best hits 0 overlaps 0 shared 0 x 3 b 33 len 4467 area 0.61M order platform,data,payments
three-dimensions.json layer x zone (9 nodes, 12 edges, 3 lanes, 3 phases)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 1 b 23 len 3050 area 0.51M
  lanes hits 0 overlaps 0 shared 0 x 2 b 23 len 3484 area 0.70M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 2 b 23 len 3484 area 0.70M violations 0/0
  best hits 0 overlaps 0 shared 0 x 0 b 23 len 3428 area 0.69M order logic,api,store
three-dimensions.json layer x team (9 nodes, 12 edges, 3 lanes, 3 phases)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 1 b 23 len 3050 area 0.51M
  lanes hits 0 overlaps 0 shared 0 x 2 b 23 len 3022 area 0.61M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 2 b 23 len 3022 area 0.61M violations 0/0
  best hits 0 overlaps 0 shared 0 x 1 b 29 len 3534 area 0.61M order store,api,logic
bpmn-pools level 0 (21 nodes, 24 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 1 b 22 len 5382 area 0.72M
  containers hits 0 overlaps 0 shared 0 x 2 b 30 len 7270 area 1.33M
  lanes hits 0 overlaps 0 shared 0 x 16 b 50 len 7754 area 1.34M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 14 b 50 len 7442 area 1.33M violations 0/0
  best hits 0 overlaps 0 shared 0 x 5 b 46 len 6876 area 1.33M order pool:logistics,pool:shop,pool:customer
network-vlan level 0 (32 nodes, 47 edges, 2 lanes)
  flat hits 0 overlaps 0 shared 0 x 33 b 98 len 16780 area 1.94M
  containers hits 0 overlaps 0 shared 0 x 37 b 98 len 19164 area 2.26M
  lanes hits 0 overlaps 0 shared 0 x 147 b 126 len 30194 area 2.58M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 147 b 126 len 30194 area 2.58M violations 0/0
  best hits 0 overlaps 0 shared 0 x 99 b 132 len 26280 area 2.46M order dc,none
workflow-dag level 0 (64 nodes, 125 edges, 8 lanes)
  flat hits 0 overlaps 0 shared 0 x 200 b 236 len 76838 area 5.98M
  containers hits 0 overlaps 0 shared 0 x 230 b 300 len 44324 area 3.03M
  lanes hits 0 overlaps 0 shared 0 x 506 b 328 len 227126 area 16.98M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 506 b 328 len 227126 area 16.98M violations 0/0
org-chart level 0 (71 nodes, 82 edges, 5 lanes)
  flat hits 0 overlaps 0 shared 0 x 47 b 130 len 30710 area 4.78M
  containers hits 0 overlaps 0 shared 0 x 19 b 160 len 41024 area 8.89M
  lanes hits 0 overlaps 0 shared 0 x 165 b 222 len 65207 area 6.64M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 165 b 222 len 65207 area 6.64M violations 0/0
uml-class level 0 (23 nodes, 0 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 0 len 0 area 0.32M
  containers hits 0 overlaps 0 shared 0 x 0 b 0 len 0 area 0.45M
  lanes hits 0 overlaps 0 shared 0 x 0 b 0 len 0 area 0.40M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 0 b 0 len 0 area 0.40M violations 0/0
  best hits 0 overlaps 0 shared 0 x 0 b 0 len 0 area 0.40M order pkg:domain,pkg:persistence,pkg:api
layerweave-code level 0 (127 nodes, 227 edges, 8 lanes)
  flat hits 0 overlaps 0 shared 0 x 422 b 516 len 290865 area 26.04M
  containers hits 0 overlaps 0 shared 0 x 469 b 652 len 345477 area 32.65M
  lanes hits 0 overlaps 0 shared 0 x 2559 b 888 len 602956 area 24.00M violations 0/0 pins off 0
  sorted hits 0 overlaps 0 shared 0 x 2559 b 888 len 602956 area 24.00M violations 0/0
{"views":20,"laneViolations":0,"phaseViolations":0,"withoutViolation":20,"hard":0,"refused":0,"pins":454,"pinsOff":0,"notSingle":0,"sortedClean":20,"x":[[0,5,5],[0,2,2],[0,0,0],[0,2,2],[0,4,4],[0,11,11],[0,5,5],[0,14,14],[0,2,2],[0,12,12],[0,2,2],[0,9,9],[0,2,2],[0,2,2],[1,16,14],[33,147,147],[200,506,506],[47,165,165],[0,0,0],[422,2559,2559]],"b":[[13,17,17],[6,14,14],[6,12,12],[6,12,12],[6,18,18],[15,33,33],[15,31,31],[15,33,33],[15,31,31],[15,29,29],[15,29,29],[15,31,31],[15,23,23],[15,23,23],[22,50,50],[98,126,126],[236,328,328],[130,222,222],[0,0,0],[516,888,888]]}
```
