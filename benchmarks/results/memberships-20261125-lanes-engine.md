# Lanes in the engine's ordering (ADR 0022, M4a), 2026-11-25

Machine: Apple M3, macOS. Revision: branch `membership-lanes-order` on `0d52a71` plus the change this file came with. Build: release
`layout-command`. Command: `node benchmarks/experiments/membership-lanes.mjs --engine`; two runs identical. No timing, no comparison with
elk-rs. Twelve views: six fixtures' dimensions and six corpus graphs without their containers, which become the lanes.

| View | flat | containers | pins (probe) | engine |
| --- | --- | --- | --- | --- |
| crossings `bpmn-pools` | 1 | 2 | 16 | 2 |
| `network-vlan` | 33 | 37 | 147 | 44 |
| `org-chart` | 47 | 19 | 165 | 82 |
| `workflow-dag` | 200 | 230 | 506 | 286 |
| `layerweave-code` | 422 | 469 | 2559 | 726 |

- Every lane lies behind the one before it in the declared order in all twelve views; no geometry issue, no line-node hit, no overlap and
  no shared line.
- Crossings stay above the flat drawing and, on the corpus, at or above the containers. The fixtures have 0 to 1.
- Each band is aligned on its own and stacked. Aligning all together and moving the bands apart drew the same crossings with several
  times the area (`layerweave-code` 146 million against 28, flat 26).
- The dummies of a long edge are in the earlier lane of its ends. Six rules, sum of the crossings over the twelve views: half and half
  1569, source 1160, target 1392, in steps 1241, later lane 1400, earlier lane 1143; one shared line (`layerweave-code`) with all but the
  last. Six rules on twelve views are a weak basis: provisional.
- `workflow-dag` grows from 6 to 15 million area units: eight lanes, each as high as its fullest rank.

Not covered: lanes below a higher rank, lanes along the flow (M4b), containers inside lanes, drawing the bands (M4c), run time.

## Runs

```
org-chart-projects.json site (7 nodes, 9 edges, 2 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 13 len 1662 area 0.20M
  containers hits 0 overlaps 0 shared 0 x 0 b 13 len 1106 area 0.32M
  engine hits 0 overlaps 0 shared 0 x 0 b 11 len 1752 area 0.29M bands out of order 0 issues 0
pools-and-lanes.json pool (7 nodes, 7 edges, 2 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 6 len 611 area 0.18M
  containers hits 0 overlaps 0 shared 0 x 0 b 14 len 1606 area 0.23M
  engine hits 0 overlaps 0 shared 0 x 0 b 10 len 1382 area 0.32M bands out of order 0 issues 0
pools-and-lanes.json phase (7 nodes, 7 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 6 len 611 area 0.18M
  containers hits 0 overlaps 0 shared 0 x 0 b 10 len 1383 area 0.38M
  engine hits 0 overlaps 0 shared 0 x 0 b 8 len 804 area 0.29M bands out of order 0 issues 0
three-dimensions.json zone (9 nodes, 12 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 0 b 13 len 2049 area 0.52M
  engine hits 0 overlaps 0 shared 0 x 0 b 15 len 3152 area 0.34M bands out of order 0 issues 0
three-dimensions.json team (9 nodes, 12 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 0 b 29 len 4854 area 0.60M
  engine hits 0 overlaps 0 shared 0 x 2 b 27 len 3933 area 0.44M bands out of order 0 issues 0
three-dimensions.json layer (9 nodes, 12 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 15 len 3038 area 0.26M
  containers hits 0 overlaps 0 shared 0 x 1 b 23 len 3050 area 0.51M
  engine hits 0 overlaps 0 shared 0 x 1 b 21 len 3825 area 0.36M bands out of order 0 issues 0
bpmn-pools level 0 (21 nodes, 24 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 1 b 22 len 5382 area 0.72M
  containers hits 0 overlaps 0 shared 0 x 2 b 30 len 7270 area 1.33M
  engine hits 0 overlaps 0 shared 0 x 2 b 30 len 6067 area 1.32M bands out of order 0 issues 0
network-vlan level 0 (32 nodes, 47 edges, 2 lanes)
  flat hits 0 overlaps 0 shared 0 x 33 b 98 len 16780 area 1.94M
  containers hits 0 overlaps 0 shared 0 x 37 b 98 len 19164 area 2.26M
  engine hits 0 overlaps 0 shared 0 x 44 b 104 len 18888 area 1.92M bands out of order 0 issues 0
workflow-dag level 0 (64 nodes, 125 edges, 8 lanes)
  flat hits 0 overlaps 0 shared 0 x 200 b 236 len 76838 area 5.98M
  containers hits 0 overlaps 0 shared 0 x 230 b 300 len 44324 area 3.03M
  engine hits 0 overlaps 0 shared 0 x 286 b 402 len 109156 area 14.58M bands out of order 0 issues 0
org-chart level 0 (71 nodes, 82 edges, 5 lanes)
  flat hits 0 overlaps 0 shared 0 x 47 b 130 len 30710 area 4.78M
  containers hits 0 overlaps 0 shared 0 x 19 b 160 len 41024 area 8.89M
  engine hits 0 overlaps 0 shared 0 x 82 b 142 len 37231 area 5.26M bands out of order 0 issues 0
uml-class level 0 (23 nodes, 0 edges, 3 lanes)
  flat hits 0 overlaps 0 shared 0 x 0 b 0 len 0 area 0.32M
  containers hits 0 overlaps 0 shared 0 x 0 b 0 len 0 area 0.45M
  engine hits 0 overlaps 0 shared 0 x 0 b 0 len 0 area 0.32M bands out of order 0 issues 0
layerweave-code level 0 (127 nodes, 227 edges, 8 lanes)
  flat hits 0 overlaps 0 shared 0 x 422 b 516 len 290865 area 26.04M
  containers hits 0 overlaps 0 shared 0 x 469 b 652 len 345477 area 32.65M
  engine hits 0 overlaps 0 shared 0 x 726 b 918 len 405161 area 28.74M bands out of order 0 issues 0
{"views":20,"laneViolations":0,"phaseViolations":0,"withoutViolation":0,"hard":0,"refused":0,"pins":0,"pinsOff":0,"notSingle":0,"sortedClean":0,"engineBad":0,"engineIssues":0,"engine":[[0,0,13,11,0,0,0,0],[0,0,6,10,0,0,0,0],[0,0,6,8,0,0,0,0],[0,0,15,15,0,0,0,0],[0,2,15,27,0,0,0,0],[0,1,15,21,0,0,0,0],[1,2,22,30,0,0,0,0],[33,44,98,104,0,0,0,0],[200,286,236,402,0,0,0,0],[47,82,130,142,0,0,0,0],[0,0,0,0,0,0,0,0],[422,726,516,918,0,0,0,0]],"x":[],"b":[]}
```
