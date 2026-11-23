# Issue #119: nodes far out at the edges, tall layered views — where it comes from (2026-11-23)

Follow-up to the open section of `elk-spacing-20261122.md`. Both findings there ("some nodes are placed far out at the
edges of the drawing with long edges into the middle" on can-bus; "layerweave-code is very tall with more crossings than
elk-rs") reproduce. They have one cause, and it is not the placement inside a rank: it is **how a hierarchical view is
layered at all**.

## Machine, build, inputs

- Apple M3, macOS 26.0, Node 24.21.0. Release `layout-command` at `1a21a6f` (origin/main); elk-rs as pinned in
  `benchmarks/eval` (`@openedges/elk-layout` 0.12.1). Single runs; the times are indicative only (the machine was not
  idle), and the quality numbers are deterministic.
- `node benchmarks/eval/hierarchy-mode.mjs --limit 150`: the 17 layered datasets of `benchmarks/datasets`, the view the
  default selects bounded to 150 nodes, posed as the elk-spacing comparison poses it (one direction for every area, no
  per-container strategies or routing, no declared buses — the ELK form carries none, so no arm gets them). Every arm is
  read back and scored by the same `metrics` operation.
- `layerweave-code` is generated from the repository's own source and drifts (see the note at the end). The copy measured
  here has **1105 nodes / 2461 edges**, sha256 `98396d49…`. It is *not* the copy behind `elk-spacing-20261122.md` or the
  0.8.0 fingerprint file (1342/2607), so its absolute numbers are not comparable with those files. The structural finding
  below was read off the stored layered plan and does not depend on the exact graph.

## What Arqlet does, and what elk-rs was asked to do

Arqlet lays every area out on its own and arranges the areas among themselves. That is what ELK calls
`SEPARATE_CHILDREN`. The elk-spacing comparison asked elk-rs for `INCLUDE_CHILDREN`: one global layering of all leaves
across the hierarchy. The adapter ignores `hierarchyHandling` (`crates/elk/src/lib.rs:352`, "the defaults of Arqlet
already are their meaning"), so the "uniform task" was uniform in spacing and direction but **not** in hierarchy mode.

elk-rs cannot be moved to the other side of that comparison on these graphs: with `SEPARATE_CHILDREN` and the
cross-hierarchy edges on the root it refuses layerweave-code outright —
`UnsupportedGraphException: The source of edge e74 could not be found in the current layout scope. An edge cannot cross
into a different layout algorithm.` So elk-rs's flat drawing on this corpus *is* its global mode, and the comparison is
between two different problems. That does not excuse the drawings, but it does reframe the gap.

### layerweave-code: 19 sibling areas in one rank, then four ranks of dummies

The stored plan of the dominant area (`crate:layout`, 31 child areas, `vertical=false`) has 5 ranks of sizes
**19, 77, 112, 132, 146**:

- rank 0 holds **19 of the 31 child areas**. They have no sibling-to-sibling predecessor, and `network_simplex` starts
  every weakly connected component at rank 0 (`crates/layout/src/ranking.rs:7`), so they stack across the flow and the
  area's cross extent is at least their summed height.
- ranks 1–4 are almost entirely long-edge and border dummies — one chain per edge that leaves the area, 146 of them in
  rank 4. Each needs its own track.

Measured from the drawing (`area-breakdown.mjs`): Arqlet reaches 19 ranks with **35 leaves in the widest**, elk-rs 28
ranks with **22**; across-flow gap 39 % against 24 %; container overhead 12 % against 6 %. Top level, the three crate
areas sit at 4 distinct x starts in Arqlet against 24 in elk-rs.

### can-bus: a channel and a border rank at every container boundary

The root plan is 3 ranks — `abs engine gps switch` / `housing limiter` / `throttle buzzer lamp`. The ordering is sound
(`switch`, whose only target is inside `limiter`, does land on the `limiter` side). What costs is the composition: each
container opens with a rank of border dummies and a header, and the channel between the parent's rank 0 and the
container's first child is 408 units wide, because eight edges cross there. Hence `along gap` 51 % against elk-rs's
31 %, rank gap 62 against 28, container overhead 19 % against 7 %.

The "long edges into the middle" is real on this view and worth a number: **total edge length 8681 against elk-rs's
4133 (2.1×)**; `abs → lim-can` is 1190 against 639, `gps → logger` 1119 against ~368. It is *not* systemic, though — see
the corpus sums.

## The two existing levers, measured

`globalLayering` (experimental, `LayoutOptions::global_layering`) is Arqlet's `INCLUDE_CHILDREN`: an expanded child
container of the same flow axis takes part in its parent's ranks as a contiguous block. `packComponents` (experimental)
packs the weakly connected components of a container's children as boxes instead of letting them share one long
drawing's ranks — the direct answer to the 19-areas-in-rank-0 finding.

**Sums over the 17 views every arm laid out.** `crossings` is the sum the other engine comparisons report; the parts are
kept apart because they fail differently — `crossed` are routes of unrelated edges crossing, `shared-endpoint` are
routes that share an endpoint crossing each other (a fan at one node drawn in the wrong order).

| arm | overlaps | hits | crossings | crossed | shared-endpoint | bends | length | area (k) | area / elk-rs (median) |
|---|---|---|---|---|---|---|---|---|---|
| default | 0 | 0 | **3064** | **2821** | 243 | **4272** | **2482075** | 227248 | 1.60 |
| globalLayering | 0 | 0 | 10928 | 3983 | 6945 | 4582 | 3072004 | **162961** | **1.24** |
| packComponents | 0 | 0 | 3236 | 2965 | 271 | 4464 | 2586425 | 221768 | 1.60 |
| elkRs | 0 | 0 | 4983 | 4651 | 332 | 4312 | 3364341 | 165444 | 1 |

Read in the order of `DEFAULT_OBJECTIVES` (overlaps, line–node hits, shared route pairs, crossings, …, bends, then cost):

- **Today's default already wins on everything above area.** Fewer crossings than elk-rs (3064 against 4983), fewer
  bends (4272 against 4312) and 26 % less total edge length (2.48 M against 3.36 M). The gap is area alone, 1.60× at the
  median. So the open finding is an area and readability finding, not a quality one.
- **`globalLayering` closes most of the area gap** (1.60 → 1.24; on layerweave-code 55.6 M → 33.0 M, below elk-rs's
  46.0 M) and keeps *fewer ordinary crossings than elk-rs* (3983 against 4651) at less length. What blocks it is
  **shared-endpoint crossings: 6945 against the default's 243**, and 6396 of those come from layerweave-code alone
  (477 ordinary crossings there, against 245). That is a fan-ordering defect at the nodes of a flattened area, not a
  fault of the layering — which makes it a bounded thing to fix rather than a trade to accept.
- Two further `globalLayering` defects: degenerate aspect ratios (er-diagram 204 × 3048, uml-class 5148 × 150, both a
  single row or column), and cost — synthetic-2k 136 → 1299 ms (9.5×), layerweave-code 61 → 342 ms (5.6×).
- **`packComponents` does not pay.** It does what the diagnosis predicts structurally (layerweave-code's widest rank
  35 → 20, height 9581 → 8357) but buys no area at the median (1.60, unchanged), because it trades height for width:
  layerweave-code 55.6 M → 65.5 M. Crossings and bends are slightly worse. Sweeping `aspect` (1.6, 2, 3) made it worse
  still, and unstably so — crossings 410, 656, 182 on layerweave-code. It changes nothing at all on can-bus, whose cause
  is the channel, not the component stacking.

Per view, including the arms' crossing split and times: run the script (the table is reproduced below).

## Where this leaves the issue

1. The cause is named and measured: **area composition, not intra-rank placement.** Arqlet's per-area layering opens a
   border rank and a channel at every container boundary and starts every unlinked sibling component at rank 0. Both
   findings in the open section follow from that.
2. The lever that addresses it already exists and is already close: `globalLayering` reaches elk-rs's area class while
   keeping Arqlet's crossing and length advantage. It is **not** ready to be offered or defaulted, and the blocker is
   concrete and local: shared-endpoint crossings in flattened areas (6945 against 243), concentrated in one view.
3. ~~Suggested next step, before any default question: fix the fan ordering at the ports of a flattened area under
   `globalLayering`.~~ **Superseded — see "Issue #119, second step" below.** That fix was built and measured inert, and
   the crossings turn out not to lie at the nodes at all: the regression is in the ordering phase, which carries 7× the
   default's plan crossings on every view. The degenerate aspect ratios and the 5–10× cost remain as stated.
4. `packComponents` should not be pursued for this issue on the present evidence.

## Note on the layerweave-code drift

`sh apps/playground/build.sh` regenerates `benchmarks/datasets/` from the current source, so `layerweave-code` grows
with the repository and a later measurement on it is not comparable with a stored one. The copy here is 1105/2461;
`elk-spacing-20261122.md` reported 7978 × 11315 and 2220 crossings for it, which this copy does not reproduce
(5804 × 9581, 292). Record nodes, edges and the hash with any layerweave-code figure, and prefer the stable datasets for
anything that has to be compared over time.

## Per view

| dataset | arm | overlaps | hits | crossings | crossed | shared-endpoint | bends | length | width × height | area (k) | ms |
|---|---|---|---|---|---|---|---|---|---|---|---|
| activity-order | default | 0 | 0 | 14 | 14 | 0 | 82 | 14583 | 1746 × 982 | 1715 | 7.3 |
| activity-order | globalLayering | 0 | 0 | 41 | 34 | 7 | 128 | 39476 | 2586 × 874 | 2260 | 9.9 |
| activity-order | packComponents | 0 | 0 | 13 | 13 | 0 | 80 | 17009 | 1277 × 1510 | 1929 | 7.1 |
| activity-order | elkRs | 0 | 0 | 36 | 29 | 7 | 90 | 29218 | 1636 × 992 | 1623 | 34.2 |
| arqlet-pipeline | default | 0 | 0 | 5 | 5 | 0 | 64 | 15681 | 1725 × 1936 | 3340 | 6.6 |
| arqlet-pipeline | globalLayering | 0 | 0 | 17 | 11 | 6 | 84 | 19218 | 2598 × 810 | 2104 | 7.1 |
| arqlet-pipeline | packComponents | 0 | 0 | 5 | 5 | 0 | 64 | 15681 | 1725 × 1936 | 3340 | 6.3 |
| arqlet-pipeline | elkRs | 0 | 0 | 5 | 5 | 0 | 56 | 17420 | 1404 × 1394 | 1957 | 5.4 |
| bpmn-pools | default | 0 | 0 | 1 | 1 | 0 | 30 | 8213 | 2626 × 519 | 1363 | 5.8 |
| bpmn-pools | globalLayering | 0 | 0 | 2 | 1 | 1 | 42 | 7377 | 2646 × 714 | 1889 | 6.9 |
| bpmn-pools | packComponents | 0 | 0 | 1 | 1 | 0 | 30 | 8213 | 2626 × 519 | 1363 | 5.1 |
| bpmn-pools | elkRs | 0 | 0 | 4 | 3 | 1 | 32 | 14326 | 2576 × 331 | 853 | 4.5 |
| can-bus | default | 0 | 0 | 6 | 4 | 2 | 40 | 8681 | 1245 × 986 | 1228 | 4.5 |
| can-bus | globalLayering | 0 | 0 | 4 | 1 | 3 | 50 | 7536 | 1254 × 798 | 1001 | 5.4 |
| can-bus | packComponents | 0 | 0 | 6 | 4 | 2 | 40 | 8681 | 1245 × 986 | 1228 | 4.7 |
| can-bus | elkRs | 0 | 0 | 5 | 5 | 0 | 30 | 4133 | 1058 × 675 | 714 | 3.6 |
| cloud-architecture | default | 0 | 0 | 14 | 13 | 1 | 98 | 33758 | 3632 × 1170 | 4249 | 7.8 |
| cloud-architecture | globalLayering | 0 | 0 | 27 | 14 | 13 | 96 | 14810 | 1818 × 906 | 1647 | 14 |
| cloud-architecture | packComponents | 0 | 0 | 14 | 13 | 1 | 98 | 33758 | 3632 × 1170 | 4249 | 8.3 |
| cloud-architecture | elkRs | 0 | 0 | 18 | 15 | 3 | 106 | 28056 | 3101 × 608 | 1884 | 6.3 |
| deployment-cluster | default | 0 | 0 | 96 | 82 | 14 | 212 | 58226 | 1769 × 2248 | 3977 | 11.4 |
| deployment-cluster | globalLayering | 0 | 0 | 245 | 196 | 49 | 172 | 63410 | 2952 × 988 | 2917 | 14.6 |
| deployment-cluster | packComponents | 0 | 0 | 91 | 84 | 7 | 212 | 61115 | 1746 × 2518 | 4396 | 11.7 |
| deployment-cluster | elkRs | 0 | 0 | 71 | 68 | 3 | 162 | 46566 | 1348 × 1730 | 2332 | 8.7 |
| er-diagram | default | 0 | 0 | 0 | 0 | 0 | 15 | 3079 | 1146 × 1502 | 1721 | 3.7 |
| er-diagram | globalLayering | 0 | 0 | 0 | 0 | 0 | 39 | 7290 | 204 × 3048 | 622 | 3.4 |
| er-diagram | packComponents | 0 | 0 | 0 | 0 | 0 | 15 | 3079 | 1146 × 1502 | 1721 | 3.6 |
| er-diagram | elkRs | 0 | 0 | 1 | 1 | 0 | 14 | 2520 | 886 × 1540 | 1364 | 4.6 |
| layerweave-code | default | 0 | 0 | 292 | 245 | 47 | 1204 | 1010756 | 5804 × 9581 | 55605 | 60.7 |
| layerweave-code | globalLayering | 0 | 0 | 6873 | 477 | 6396 | 1078 | 1332548 | 3288 × 10026 | 32965 | 341.8 |
| layerweave-code | packComponents | 0 | 0 | 317 | 242 | 75 | 1212 | 1114021 | 7842 × 8357 | 65538 | 75.8 |
| layerweave-code | elkRs | 0 | 0 | 168 | 160 | 8 | 1070 | 1111067 | 8745 × 5256 | 45967 | 38.5 |
| mixed-large | default | 0 | 0 | 531 | 497 | 34 | 421 | 276653 | 5223 × 5369 | 28044 | 55.7 |
| mixed-large | globalLayering | 0 | 0 | 550 | 490 | 60 | 671 | 261499 | 3709 × 4677 | 17347 | 151.8 |
| mixed-large | packComponents | 0 | 0 | 681 | 639 | 42 | 457 | 346770 | 6873 × 5015 | 34468 | 68.2 |
| mixed-large | elkRs | 0 | 0 | 625 | 591 | 34 | 368 | 322176 | 5201 × 3181 | 16544 | 16.1 |
| network-vlan | default | 0 | 0 | 24 | 23 | 1 | 110 | 53703 | 3864 × 1120 | 4328 | 6.4 |
| network-vlan | globalLayering | 0 | 0 | 94 | 56 | 38 | 118 | 55809 | 4788 × 828 | 3964 | 8.9 |
| network-vlan | packComponents | 0 | 0 | 24 | 23 | 1 | 110 | 53703 | 3864 × 1120 | 4328 | 6.5 |
| network-vlan | elkRs | 0 | 0 | 28 | 26 | 2 | 94 | 52067 | 3392 × 939 | 3185 | 5.3 |
| org-chart | default | 0 | 0 | 29 | 21 | 8 | 172 | 75978 | 5820 × 3418 | 19893 | 15.1 |
| org-chart | globalLayering | 0 | 0 | 60 | 46 | 14 | 214 | 94176 | 10770 × 810 | 8724 | 20.7 |
| org-chart | packComponents | 0 | 0 | 26 | 18 | 8 | 190 | 74404 | 4361 × 4276 | 18645 | 16 |
| org-chart | elkRs | 0 | 0 | 23 | 22 | 1 | 148 | 63288 | 5660 × 1846 | 10448 | 10.5 |
| petri-net | default | 0 | 0 | 32 | 32 | 0 | 84 | 24174 | 1734 × 801 | 1389 | 5.6 |
| petri-net | globalLayering | 0 | 0 | 49 | 47 | 2 | 98 | 11831 | 930 × 991 | 922 | 4.1 |
| petri-net | packComponents | 0 | 0 | 32 | 32 | 0 | 84 | 24174 | 1734 × 801 | 1389 | 5.3 |
| petri-net | elkRs | 0 | 0 | 58 | 58 | 0 | 66 | 10769 | 940 × 691 | 650 | 3.9 |
| state-order | default | 0 | 0 | 0 | 0 | 0 | 19 | 2002 | 1772 × 225 | 399 | 2.7 |
| state-order | globalLayering | 0 | 0 | 11 | 4 | 7 | 35 | 2868 | 1074 × 366 | 393 | 2.7 |
| state-order | packComponents | 0 | 0 | 0 | 0 | 0 | 19 | 2002 | 1772 × 225 | 399 | 2.9 |
| state-order | elkRs | 0 | 0 | 1 | 1 | 0 | 16 | 1792 | 1629 × 170 | 277 | 2.5 |
| state-tcp | default | 0 | 0 | 0 | 0 | 0 | 26 | 5286 | 1698 × 210 | 357 | 3.4 |
| state-tcp | globalLayering | 0 | 0 | 0 | 0 | 0 | 44 | 5355 | 1734 × 246 | 427 | 2.8 |
| state-tcp | packComponents | 0 | 0 | 0 | 0 | 0 | 26 | 5286 | 1698 × 210 | 357 | 3.1 |
| state-tcp | elkRs | 0 | 0 | 0 | 0 | 0 | 30 | 7279 | 1640 × 189 | 310 | 2.4 |
| synthetic-2k | default | 0 | 0 | 1789 | 1686 | 103 | 1358 | 838538 | 15082 × 6232 | 93987 | 136.1 |
| synthetic-2k | globalLayering | 0 | 0 | 2434 | 2168 | 266 | 1276 | 975451 | 6960 × 10872 | 75669 | 1298.6 |
| synthetic-2k | packComponents | 0 | 0 | 1787 | 1684 | 103 | 1374 | 726417 | 15628 × 4517 | 70584 | 143.2 |
| synthetic-2k | elkRs | 0 | 0 | 3660 | 3416 | 244 | 1558 | 1559571 | 15816 × 4595 | 72677 | 51.3 |
| uml-class | default | 0 | 0 | 1 | 0 | 1 | 37 | 8440 | 1730 × 1520 | 2629 | 5.8 |
| uml-class | globalLayering | 0 | 0 | 11 | 0 | 11 | 95 | 13741 | 5148 × 150 | 772 | 4.4 |
| uml-class | packComponents | 0 | 0 | 1 | 0 | 1 | 39 | 8813 | 1685 × 1604 | 2702 | 5.8 |
| uml-class | elkRs | 0 | 0 | 2 | 2 | 0 | 38 | 9413 | 1610 × 626 | 1008 | 5.3 |
| workflow-dag | default | 0 | 0 | 230 | 198 | 32 | 300 | 44324 | 2756 × 1098 | 3026 | 16.6 |
| workflow-dag | globalLayering | 0 | 0 | 510 | 438 | 72 | 342 | 159609 | 3768 × 2478 | 9337 | 64.3 |
| workflow-dag | packComponents | 0 | 0 | 238 | 207 | 31 | 414 | 83299 | 4872 × 1054 | 5134 | 21.7 |
| workflow-dag | elkRs | 0 | 0 | 278 | 249 | 29 | 434 | 84680 | 3552 × 1028 | 3650 | 12.8 |

# Issue #119, second step: the fan-ordering hypothesis is wrong (2026-11-23)

The first step (above) ended with: `globalLayering` fails only on shared-endpoint crossings, 6396 of its 6945 come from
one view, and that looks like "a fan-ordering defect at the ports of a flattened area, so bounded rather than a trade to
accept". That was measured correctly but read wrong. Three measurements, same machine and build as above
(`layout-command` at `1a21a6f` plus the reverted patch where stated):

## 1. The suggested fix was built, and it changes nothing

`layered_plan::port_keys` gives every port of a node side a key, and a side whose ports all have one is ordered by it
instead of by the geometry. The key is the chain neighbour's **index in its rank**. That index is exactly the order
across the flow while each area is placed on its own — measured on layerweave-code's hub node: **0 of 11026 inversions**
of index against the neighbour's placed position in the default, against **199** under `globalLayering`, where the
placement moves each flattened container as a block.

So the key was keyed by place instead of index (absolute, as `port_lanes` takes its rects), behind `global_layering` so
the default could not change. Result on all 17 views: **byte-identical output in every arm**, `globalLayering` included —
crossings 10928, crossed 3983, shared-endpoint 6945, bends 4582, exactly as before. The patch is reverted.

It is inert because the port places are not decided there. `plan_port_offsets` (`crates/layout/src/lib.rs`) fixes them
from `place_of`, the element's index in a *provisional* placement that is itself built rank by rank in index order, long
before the real placement runs. A geometric key applied afterwards cannot be the deciding input.

## 2. The crossings do not lie at the nodes

If a fan were drawn in the wrong order at a node, its crossings would sit next to that node. They do not. For the routes
that share a node, measured as the distance from that node's box:

| view | arm | shared-endpoint crossings | median distance | p90 | within 50 units |
|---|---|---|---|---:|---:|
| layerweave-code | default | 47 | 432 | 3168 | 0 |
| layerweave-code | globalLayering | 6396 | **3534** | 5637 | 1438 |

The drawing is 3312 × 10074, so a median of 3534 is half the drawing away. Only 1438 of 6396 are near the node. These
are long dummy chains crossing each other on their way, not a fan at a node — on the worst view the fan-at-the-node
reading accounts for at most 1438 of 6396, and on the other 16 views shared-endpoint crossings are a handful either way.

## 3. The ordering phase is what regresses, on every view

The crossings the stored rank order already implies, before any geometry (`benchmarks/eval/plan-crossings.mjs`):

| | default | globalLayering |
|---|---:|---:|
| plan crossings, summed over the 17 views | **764** | **5337** (7.0×) |

It grows on every view that has any: activity-order 3 → 42, cloud-architecture 5 → 13, deployment-cluster 10 → 139,
layerweave-code 219 → 1747, mixed-large 43 → 316, network-vlan 22 → 58, org-chart 5 → 112, petri-net 32 → 47,
synthetic-2k 415 → 2570, workflow-dag 1 → 282. The drawn ordinary crossings track it (2821 → 3983). So the regression is
systemic and it belongs to the ordering phase, not to the ports or the routing.

And the ordering search does not explore on a flattened plan. `orderSeed` shuffles the starts of the hierarchical order
from the fourth run on, and the fewest crossings win. Under `globalLayering` the seed makes no difference at all, while
the default moves with it:

| view | arm | seed 0 | another seed |
|---|---|---|---|
| layerweave-code | default | 245 crossed / 47 shared | 221 / 49 (seed 1) |
| layerweave-code | globalLayering | 477 / 6396 | 477 / 6396 — identical for seeds 1, 2 and 7 |
| workflow-dag | default | 198 / 32 | 208 / 38 (seed 3) |
| workflow-dag | globalLayering | 438 / 72 | 438 / 72 (seed 3) |

Byte-identical drawings across four seeds on one view and two on another: the flattened plan's order is settled before
the search that is supposed to improve it, so none of its eight runs can take effect.

## Corrected next step

The work is in the ordering phase, not at the ports:

1. ~~Find why the hierarchical sweep cannot move a flattened plan's ranks.~~ **Answered — see "Issue #119, third step"
   below.** Neither guess was right: the sweep runs and finds 207 crossings where the plan started at 1747, and the
   placement that follows discards the order, because the flattened branch of `place` is given no previous plan at all.
2. Then re-measure with `benchmarks/eval/plan-crossings.mjs` (plan crossings must approach the default's order) and
   `benchmarks/eval/hierarchy-mode.mjs` (the gates). The area win of `globalLayering` is unaffected by any of this —
   1.24× against the default's 1.60× — so if the ordering comes down, the option passes the gates above area and the
   default question is worth asking.
3. What is *not* worth doing: a port or fan-ordering fix at the nodes. It is built, measured and inert.

Part of the first step's conclusion stands: `globalLayering` keeps fewer ordinary crossings than elk-rs (3983 against
4651) at 26 % less edge length while reaching its area class. What stands corrected is where its crossing regression
comes from.

## Per view: plan crossings against drawn crossings

| dataset | arm | areas | sections | plan crossings | drawn crossed | drawn shared-endpoint | shared: median distance | p90 | within 50 |
|---|---|---|---|---|---|---|---|---|---|
| activity-order | default | 5 | 80 | 3 | 14 | 0 | 0 | 0 | 0 |
| activity-order | globalLayering | 1 | 70 | 42 | 34 | 7 | 200 | 574 | 1 |
| arqlet-pipeline | default | 7 | 94 | 5 | 5 | 0 | 0 | 0 | 0 |
| arqlet-pipeline | globalLayering | 1 | 48 | 7 | 11 | 6 | 27 | 234 | 4 |
| bpmn-pools | default | 6 | 60 | 1 | 1 | 0 | 0 | 0 | 0 |
| bpmn-pools | globalLayering | 1 | 41 | 3 | 1 | 1 | 12 | 12 | 1 |
| can-bus | default | 4 | 42 | 3 | 4 | 2 | 392 | 392 | 0 |
| can-bus | globalLayering | 1 | 37 | 1 | 1 | 3 | 45 | 51 | 2 |
| cloud-architecture | default | 15 | 121 | 5 | 13 | 1 | 138 | 138 | 0 |
| cloud-architecture | globalLayering | 1 | 41 | 13 | 14 | 13 | 33 | 60 | 8 |
| deployment-cluster | default | 10 | 205 | 10 | 82 | 14 | 606 | 1148 | 0 |
| deployment-cluster | globalLayering | 1 | 100 | 139 | 196 | 49 | 600 | 1134 | 10 |
| er-diagram | default | 13 | 12 | 0 | 0 | 0 | 0 | 0 | 0 |
| er-diagram | globalLayering | 1 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| layerweave-code | default | 37 | 1356 | 219 | 245 | 47 | 432 | 3168 | 0 |
| layerweave-code | globalLayering | 1 | 898 | 1747 | 477 | 6396 | 3534 | 5637 | 1438 |
| mixed-large | default | 21 | 407 | 43 | 497 | 34 | 638 | 1624 | 8 |
| mixed-large | globalLayering | 2 | 293 | 316 | 490 | 60 | 120 | 1561 | 26 |
| network-vlan | default | 6 | 84 | 22 | 23 | 1 | 132 | 132 | 0 |
| network-vlan | globalLayering | 1 | 68 | 58 | 56 | 38 | 54 | 1109 | 15 |
| org-chart | default | 34 | 207 | 5 | 21 | 8 | 246 | 297 | 4 |
| org-chart | globalLayering | 1 | 122 | 112 | 46 | 14 | 54 | 708 | 5 |
| petri-net | default | 1 | 119 | 32 | 32 | 0 | 0 | 0 | 0 |
| petri-net | globalLayering | 1 | 57 | 47 | 47 | 2 | 546 | 546 | 0 |
| state-order | default | 2 | 15 | 0 | 0 | 0 | 0 | 0 | 0 |
| state-order | globalLayering | 1 | 12 | 0 | 4 | 7 | 12 | 162 | 6 |
| state-tcp | default | 1 | 32 | 0 | 0 | 0 | 0 | 0 | 0 |
| state-tcp | globalLayering | 1 | 32 | 0 | 0 | 0 | 0 | 0 | 0 |
| synthetic-2k | default | 82 | 1565 | 415 | 1686 | 103 | 584 | 1950 | 6 |
| synthetic-2k | globalLayering | 1 | 1440 | 2570 | 2168 | 266 | 276 | 3853 | 67 |
| uml-class | default | 21 | 40 | 0 | 0 | 1 | 24 | 24 | 1 |
| uml-class | globalLayering | 1 | 0 | 0 | 0 | 11 | 12 | 12 | 11 |
| workflow-dag | default | 9 | 266 | 1 | 198 | 32 | 86 | 140 | 3 |
| workflow-dag | globalLayering | 1 | 211 | 282 | 438 | 72 | 63 | 659 | 30 |

# Issue #119, third step: why the sweep cannot move a flattened plan's ranks (2026-11-23)

Answered. The sweep is not weak on a flattened plan and it is not skipped — **it runs, it finds a very good order, and the
placement that follows throws it away.** Traced on layerweave-code with a temporary `ARQLET_TRACE_SWEEP` print at the
call site (`crates/layout/src/lib.rs`, the `else` of `if options.abstract_routing`), `globalLayering` on:

```
sweep(else): gl=true plans=1 ranks=7 elems=772 before=1747 after=207 kept=true swept_differs=true
sweep(after place): area=<root> fixed=false ranks_kept=false
```

- `before=1747 after=207`: the hierarchical sweep cuts the flattened plan's crossings by 8.4×, and **207 is below the
  default's 219** for its whole 37-area hierarchy. The ordering phase is not the weak part; its answer is already good.
- `kept=true`: `after < before`, so the result is accepted and handed to `place`.
- `ranks_kept=false`: after that placement the ranks are not the swept ones. The order is lost whole.

## Where it is lost

`place` chooses between two ordering branches:

```rust
if flattening.is_empty() {
    let reused = ordering.order(
        parent.unwrap_or("<root>"), &mut levels, &order_links, &component_of, optimize,
        options.layered_construction && layered,
        plans.get(parent.unwrap_or("<root>")),   // the plan it was handed
        &options.order_hints,
    );
    …
} else {
    placement_order::compute(
        &mut levels, &order_links, &component_of, optimize,
        false,              // layered off
        None,               // the handed plan, its order and its `fixed` flag: dropped
        &BTreeMap::new(),   // no hints
    );
}
```

`global_layering` is exactly the case where `flattening` is not empty, so the flattened area always takes the second
branch — the one that is given no previous order at all. The sweep's result cannot reach the placement by construction.

This also explains the `orderSeed` finding of the second step: the sweep's answer does depend on the seed, but since the
answer is discarded, every seed draws the same picture.

## Why `AreaPlan::fixed` is not the lever on its own

`placement_order::compute` does honour a given order — `earlier.filter(|p| p.fixed)` takes the ranks as they are and
skips `improve_order_around` — but that code sits **inside `if layered {`**. The flattened branch passes `layered:
false`, so the whole block, the `fixed` path included, is dead there. Measured: setting `plan.fixed = true` on every
swept area before handing it over changes the trace to `fixed=true` and the drawing not at all (`ranks_kept=false`,
identical metrics). The flag is read in the branch that is skipped.

So two inputs have to arrive together: the previous plan **and** a true `layered`.

## A first attempt, and why it is not yet the fix

Passing the unflattened branch's three inputs to the flattened one (`options.layered_construction && layered`,
`plans.get(…)`, `&options.order_hints`) does reach the ordering — the sweep's own starting count moves (1747 → 1905,
after 183) — but the placement still does not reproduce the swept ranks, and the quality is mixed to worse:

| view | plan crossings (gl, today) | with the pass-through | drawn crossed | drawn shared-endpoint |
|---|---:|---:|---|---|
| layerweave-code | 1747 | **2521** | 477 → 583 | 6396 → 6794 |
| workflow-dag | 282 | 283 | 438 → **379** | 72 → **50** |
| org-chart | 112 | **106** | 46 → **36** | 14 → 14 |
| synthetic-2k | 2570 | **2458** | 2168 → 2394 | 266 → 317 |

Better on two views, worse on the two that matter most. Reverted. Why the order still does not survive is **not
measured**. The likely place is `layered_plan::follow_order`, which carries a given order over by element id: an element
of the new pass whose id the swept plan does not have gets no place of its own and stays behind its neighbour in the old
order. If the second placement pass names its dummies differently (their ids carry their rank), most of the swept order
would be lost that way. That is a hypothesis; the trace only shows `ranks_kept=false` with `fixed` set and `layered` true.

## Next step, now precisely bounded

**Superseded by the correction below:** this step works on the legacy pipeline only, since `globalLayering` cannot run
with single pass. Kept as the record of what the trace showed.

The ordering phase needs no work — its answer is already better than the default's. The work is to carry one order
across:

1. Make the flattened placement take a given order **as given**: give the `else` branch the previous plan and a true
   `layered` so `compute`'s `fixed` path applies, and make the flattened `levels` and the swept plan agree on their
   elements (the dummy ids above all) so `follow_order` can reproduce the ranks. The check is the trace line:
   `ranks_kept` must become `true`.
2. Then re-measure with `benchmarks/eval/plan-crossings.mjs` — the flattened plan's crossings should land near the
   sweep's 207 rather than today's 1747 — and with `benchmarks/eval/hierarchy-mode.mjs` for the gates.
3. The prize is already quantified: the ordering found 207 where the drawing keeps 1747, and `globalLayering` reaches
   1.24× elk-rs's area against the default's 1.60× while keeping fewer ordinary crossings than elk-rs. If the swept
   order survives the placement, the crossing objection to `globalLayering` largely disappears.

## Correction: `globalLayering` cannot run with single pass

Every `globalLayering` figure in this file is the **legacy pipeline**, not single pass. Single pass needs abstract routing
(`singlePass requires abstractRouting`), and abstract routing refuses a flattened placement (`Abstract routing requires
unflattened hierarchical placement…`, `crates/layout/src/lib.rs`, `compute_requested`). With `singlePassAuto` on, the
layout then falls back silently and reports it — on layerweave-code the default's diagnostics read `Single pass: 283
constructed, 0 delegated…`, the `globalLayering` arm's `Single pass was not used: Abstract routing requires unflattened
hierarchical placement and default orthogonal routing without a view budget`.

So the comparison of the two arms was a comparison of two pipelines, and the sweep site traced above (the `else` of `if
options.abstract_routing`) is legacy-only — which is also why the default printed no sweep line there. Single pass stays
the default (decision 2026-11-23), so `globalLayering` as it exists is not a path for this issue, and neither is
carrying the swept order through its placement. What remains usable from this work: the cause (per-area composition:
unlinked sibling areas stacked in rank 0, a channel and a border rank at every container boundary), the measurement that
sharing ranks across areas would buy most of the area gap (1.60× → 1.24× elk-rs), and the two probes. Getting that
benefit inside single pass is a design question, not a fix.

# Issue #119, fourth step: where single pass spends the extent (2026-11-23)

Single pass stays and quality must not drop (decision of 2026-11-23), so this measures the default pipeline only.
`node benchmarks/eval/area-budget.mjs --limit 150`, release `layout-command` at `1a21a6f` (clean build), the 17 layered
views posed as in the elk-rs comparison; **single pass ran on all 17** (checked per view on its diagnostics). The probe
breaks the drawing's extent along the flow and across it into parts that add up exactly (the `other` column is 0 on
every view): along the flow it follows the longest element of every rank down the hierarchy, across the flow the rank
with the most content. Area is the product of the two, so the two budgets together say where the area goes.

Before reading it: `packComponents` runs under single pass (checked), so its verdict above (no area win) stands;
`globalLayering` does not (checked again on the clean build).

## The gap is on both axes, in three different ways

Per view against elk-rs, sorted by the area ratio. "Container share" is what the container boundaries take of that
extent (along: the channel at a border rank, the border rank itself and the frames; across: the gaps between stacked
sibling areas and the frames); "long-edge share" is what the long edges take across (dummy tracks and the gaps beside
them).

| dataset | along / elk-rs | across / elk-rs | area / elk-rs | container share along | container share across | long-edge share across |
|---|---:|---:|---:|---:|---:|---:|
| uml-class | 2.43 | 1.07 | 2.61 | 60 % | 26 % | 0 % |
| cloud-architecture | 1.17 | 1.93 | 2.26 | 21 % | 53 % | 22 % |
| petri-net | 1.84 | 1.16 | 2.14 | 0 % | 0 % | 0 % |
| org-chart | 1.85 | 1.03 | 1.90 | 49 % | 5 % | 38 % |
| can-bus | 1.46 | 1.18 | 1.72 | 19 % | 4 % | 0 % |
| arqlet-pipeline | 1.39 | 1.23 | 1.71 | 32 % | 3 % | 24 % |
| deployment-cluster | 1.30 | 1.31 | 1.71 | 38 % | 8 % | 58 % |
| mixed-large | 1.00 | 1.69 | 1.70 | 27 % | 8 % | 76 % |
| bpmn-pools | 1.02 | 1.57 | 1.60 | 12 % | 28 % | 17 % |
| state-order | 1.09 | 1.38 | 1.50 | 5 % | 31 % | 13 % |
| state-tcp | 1.04 | 1.42 | 1.47 | 0 % | 0 % | 22 % |
| network-vlan | 1.19 | 1.14 | 1.36 | 30 % | 6 % | 12 % |
| synthetic-2k | 0.95 | 1.36 | 1.29 | 20 % | 7 % | 77 % |
| er-diagram | 1.29 | 0.97 | 1.26 | 27 % | 29 % | 9 % |
| layerweave-code | 0.66 | 1.82 | 1.21 | 17 % | 61 % | 7 % |
| activity-order | 0.99 | 1.07 | 1.06 | 31 % | 3 % | 19 % |
| workflow-dag | 0.78 | 1.07 | 0.83 | 26 % | 7 % | 25 % |
| **median** | 1.17 | 1.23 | 1.60 | | | |

Three patterns, plus one outlier:

- **Container boundaries along the flow**: uml-class (60 % of its length is container cost; 2.43× elk-rs along),
  org-chart (49 %), deployment-cluster (38 %), arqlet-pipeline (32 %), activity-order and network-vlan (30–31 %). Every
  nested area adds a border rank, the channel beside it and a header along the flow.
- **Containers across the flow**: layerweave-code (61 %: the gap between stacked sibling areas alone is 4235 of 9581 —
  the 19 unlinked areas in rank 0 found in the first step, now measured inside single pass), cloud-architecture (53 %,
  mostly frames of deep nesting: 594 of 1170), bpmn-pools, state-order, er-diagram.
- **Long edges across the flow**: synthetic-2k (77 %), mixed-large (76 %), deployment-cluster (58 %), org-chart (38 %).
  Long-edge dummies stand on the lines that keep their edges straight, so a rank of them spreads.
- petri-net (no containers, 1.84× along) is neither: its length is leaves and ordinary channels; elk-rs uses fewer
  ranks there. A layering difference, not a placement one.

## Corpus sums, as shares of the extent

| | along the flow (53683; elk-rs 52288) | across the flow (44677; elk-rs 33697) |
|---|---|---|
| leaves | 55 % | 32 % |
| container boundaries | **23 %** — channel at a border rank 13 %, frames 8 %, border rank 2 % | **20 %** — gaps between stacked sibling areas 10 %, frames 10 % |
| edges | ordinary channels 21 % | **long edges 34 %** — gaps between dummies 15 %, dummy tracks 13 %, node–dummy gaps 6 % |
| other | rank alignment 1 % (mixed-large only) | alignment slack 8 %, node–node gaps 6 % |

Summed over the corpus the extent along the flow is close to elk-rs's (+3 %) and across it is a third more (+33 %);
per view both axes matter (median 1.17 along, 1.23 across, 1.60 in area).

## Which parts can move without costing quality

The constraint rules out three of the five large parts:

- **Long edges across (34 %)** are what keeps long edges straight. Packing them tighter costs bends, and bends come
  before length and area in `DEFAULT_OBJECTIVES`. Not a lever.
- **Ordinary channels (21 % along)** and **frames (8 % along, 10 % across)** are spacings — the layer gap, the padding
  and the header. Tighter spacings were measured and rejected on 2026-11-22 (more bends and crossings, two line–node
  hits; `elk-spacing-20261122.md`). Not a lever under this constraint.

Two structural parts remain, and neither is a spacing:

1. **The channel at a border rank, 13 % of the length along the flow** (uml-class 420 of 1520, org-chart 840 of 3418,
   cloud-architecture 516 of 3632). A container that edges enter opens a rank of border nodes, and the edges change lane
   in the channel between that rank and the first child — a second channel next to the one outside. The border rank
   itself is thin (2 %); the channel beside it is the cost. If the edges took their lanes outside, in the parent's
   channel (where `border_alignment` already aligns border nodes for single pass), that channel could shrink towards a
   track. The larger of the two, and it touches every nested view. **Measured in the fifth step: it does not pay** —
   four fifths of that channel are margins the construction needs; below them routes fall back to the search.
2. **Gaps between stacked sibling areas, 10 % across, concentrated** (layerweave-code 4235 of 9581; er-diagram,
   uml-class, cloud-architecture smaller). Unlinked sibling areas all start in rank 0 and stand side by side across the
   flow. `packComponents` repacks them as boxes but trades height for width (no area win); the open question is whether
   they can take ranks *along* the flow instead, as elk-rs's global layering does, inside single pass's per-area
   construction.

Caveats: the across budget follows the fullest rank, and ranks are coupled by the alignment, so its gaps include
spreading that other ranks cause — it attributes, it does not prove a counterfactual. A lever's real effect needs its
own before-and-after run through `hierarchy-mode.mjs` and the fingerprint check. Small negative "frame end" values
(−25 along in sum) are dummies that overhang their container's box at the far end. layerweave-code is the drifted copy
(1105 nodes / 2461 edges) noted above.

## Per view


**Along the flow** (units; the parts add up to the extent)

| dataset | single pass | extent | elk-rs | leaf | channel | channel@border | border rank | dummy rank | rank align | frame start | frame end | other |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| activity-order | yes | 982 | 992 | 378 | 300 | 132 | 24 | 0 | 0 | 144 | 4 | 0 |
| arqlet-pipeline | yes | 1936 | 1394 | 810 | 504 | 300 | 60 | 0 | 0 | 288 | -26 | 0 |
| bpmn-pools | yes | 2626 | 2576 | 1980 | 342 | 150 | 30 | 0 | 0 | 96 | 28 | 0 |
| can-bus | yes | 986 | 675 | 486 | 310 | 96 | 18 | 0 | 0 | 96 | -20 | 0 |
| cloud-architecture | yes | 3632 | 3101 | 2340 | 514 | 516 | 102 | 0 | 0 | 216 | -56 | 0 |
| deployment-cluster | yes | 2248 | 1730 | 702 | 682 | 414 | 78 | 0 | 0 | 384 | -12 | 0 |
| er-diagram | yes | 1146 | 886 | 720 | 120 | 180 | 36 | 0 | 0 | 96 | -6 | 0 |
| layerweave-code | yes | 5804 | 8745 | 3420 | 1408 | 750 | 60 | 0 | 0 | 168 | -2 | 0 |
| mixed-large | yes | 5223 | 5201 | 2412 | 848 | 654 | 126 | 0 | 562 | 288 | 333 | 0 |
| network-vlan | yes | 1120 | 939 | 432 | 348 | 162 | 30 | 0 | 0 | 144 | 4 | 0 |
| org-chart | yes | 3418 | 1846 | 972 | 758 | 840 | 168 | 0 | 0 | 816 | -136 | 0 |
| petri-net | yes | 1734 | 940 | 1440 | 294 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| state-order | yes | 1772 | 1629 | 1440 | 246 | 60 | 12 | 0 | 0 | 24 | -10 | 0 |
| state-tcp | yes | 1698 | 1640 | 1440 | 258 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| synthetic-2k | yes | 15082 | 15816 | 8640 | 3422 | 1962 | 324 | 0 | 0 | 792 | -58 | 0 |
| uml-class | yes | 1520 | 626 | 378 | 228 | 420 | 84 | 0 | 0 | 432 | -22 | 0 |
| workflow-dag | yes | 2756 | 3552 | 1440 | 606 | 480 | 84 | 0 | 0 | 192 | -46 | 0 |
| **sum** | | 53683 | 52288 | 29430 (55 %) | 11188 (21 %) | 7116 (13 %) | 1236 (2 %) | 0 (0 %) | 562 (1 %) | 4176 (8 %) | -25 (0 %) | 0 (0 %) |

**Across the flow** (units; the parts add up to the extent)

| dataset | single pass | extent | elk-rs | leaf | gap node–node | gap area–area | gap node–dummy | gap dummy–dummy | dummy track | border track | across slack | frame start | frame end | other |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| activity-order | yes | 1746 | 1636 | 1260 | 24 | 0 | 288 | 0 | 42 | 0 | 84 | 24 | 24 | 0 |
| arqlet-pipeline | yes | 1725 | 1404 | 900 | 96 | 0 | 75 | 300 | 42 | 0 | 264 | 24 | 24 | 0 |
| bpmn-pools | yes | 519 | 331 | 108 | 24 | 0 | 24 | 45 | 18 | 0 | 156 | 96 | 48 | 0 |
| can-bus | yes | 1245 | 1058 | 900 | 270 | 0 | 0 | 0 | 0 | 0 | 27 | 24 | 24 | 0 |
| cloud-architecture | yes | 1170 | 608 | 216 | 0 | 24 | 168 | 51 | 42 | 0 | 75 | 384 | 210 | 0 |
| deployment-cluster | yes | 1769 | 1348 | 540 | 48 | 0 | 72 | 648 | 312 | 0 | 5 | 72 | 72 | 0 |
| er-diagram | yes | 1502 | 1540 | 702 | 192 | 72 | 125 | 0 | 6 | 0 | 45 | 240 | 120 | 0 |
| layerweave-code | yes | 9581 | 5256 | 2160 | 783 | 4235 | 354 | 222 | 138 | 0 | 51 | 1056 | 582 | 0 |
| mixed-large | yes | 5369 | 3181 | 684 | 123 | 24 | 651 | 941 | 2462 | 0 | 93 | 232 | 160 | 0 |
| network-vlan | yes | 3864 | 3392 | 2880 | 264 | 48 | 144 | 288 | 42 | 0 | 6 | 96 | 96 | 0 |
| org-chart | yes | 5820 | 5660 | 1260 | 123 | 60 | 177 | 789 | 1260 | 0 | 1911 | 120 | 120 | 0 |
| petri-net | yes | 801 | 691 | 378 | 423 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| state-order | yes | 234 | 170 | 108 | 24 | 0 | 24 | 0 | 6 | 0 | 0 | 48 | 24 | 0 |
| state-tcp | yes | 269 | 189 | 162 | 48 | 0 | 53 | 0 | 6 | 0 | 0 | 0 | 0 | 0 |
| synthetic-2k | yes | 6232 | 4595 | 270 | 24 | 0 | 337 | 3280 | 1212 | 0 | 682 | 144 | 284 | 0 |
| uml-class | yes | 1730 | 1610 | 1080 | 24 | 108 | 0 | 0 | 0 | 0 | 182 | 168 | 168 | 0 |
| workflow-dag | yes | 1103 | 1028 | 540 | 216 | 0 | 68 | 123 | 84 | 0 | 0 | 48 | 24 | 0 |
| **sum** | | 44677 | 33697 | 14148 (32 %) | 2706 (6 %) | 4571 (10 %) | 2558 (6 %) | 6687 (15 %) | 5672 (13 %) | 0 (0 %) | 3581 (8 %) | 2776 (6 %) | 1979 (4 %) | 0 (0 %) |

**The root's fullest rank across the flow**

| dataset | elements | of them areas |
|---|---:|---:|
| activity-order | 1 | 1 |
| arqlet-pipeline | 2 | 1 |
| bpmn-pools | 1 | 1 |
| can-bus | 4 | 1 |
| cloud-architecture | 1 | 1 |
| deployment-cluster | 1 | 1 |
| er-diagram | 6 | 5 |
| layerweave-code | 2 | 2 |
| mixed-large | 17 | 1 |
| network-vlan | 1 | 1 |
| org-chart | 3 | 1 |
| petri-net | 7 | 0 |
| state-order | 2 | 1 |
| state-tcp | 4 | 0 |
| synthetic-2k | 1 | 1 |
| uml-class | 2 | 2 |
| workflow-dag | 3 | 1 |

# Issue #119, fifth step: lever 1 (the channel at a border rank) prototyped — it does not pay (2026-11-23)

## What the 13 % is made of

Single pass gives every channel the width its lane changes need, `24 + 6 × tracks`, and never less than a floor of
`2 × margin + track` = 30 (`realize_channel_widths`, with `channelsByDemand` on by default). Counted over the corpus from
the single-pass diagnostics, the channels beside a border rank are mostly empty:

| channels beside a border rank | count | width summed |
|---|---:|---:|
| no edge changes lane (the floor of 30) | 212 | 6354 |
| with lane changes | 119 | 4560 |

So the first idea was to let an empty channel beside a border rank narrow below the floor, since its edges only run
straight from the border nodes to the children.

## The prototype

An experimental option `borderChannelGap` (patch: `border-channel-gap-20261123.patch`, 138 lines; model, runtime
validation, layout, regenerated TypeScript contract): beside a border rank, a channel with no counted track and nothing
the construction reported short narrows to the given width; every other channel keeps the floor. Default unchanged
(`None`, omitted) — the default arm below reproduces the earlier default run exactly on all 17 views.

`node benchmarks/eval/border-channel.mjs` (needs the patch), release build of `1a21a6f` plus the patch, same machine.

**Uniform task** (the elk-rs comparison's), sums over 17 views:

| arm | single pass | delegated | overlaps | hits | crossings | bends | length | area (k) | area / default (median) | area / elk-rs (median) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| default | 17/17 | 16 | 0 | 0 | 3064 | 4272 | 2482075 | 227248 | 1.000 | 1.60 |
| gap 0 | 17/17 | 486 | 0 | **28** | **9655** | 4172 | 2363251 | 213898 | 0.964 | 1.47 |
| gap 6 | 17/17 | 488 | 0 | 0 | **9724** | 4168 | 2373440 | 216445 | 0.971 | 1.52 |
| gap 12 | 17/17 | 488 | 0 | 0 | **9252** | 4116 | 2383699 | 219112 | 0.978 | 1.56 |
| gap 18 | 17/17 | 462 | 0 | 0 | **10305** | 4050 | 2397581 | 221864 | 0.986 | 1.58 |
| gap 21 | 17/17 | 462 | 0 | 0 | **10310** | 4139 | 2403399 | 223201 | 0.989 | 1.59 |
| gap 24 | 17/17 | 16 | 0 | 0 | 3064 | 4274 | 2470479 | 224546 | 0.993 | 1.59 |
| gap 27 | 17/17 | 16 | 0 | 0 | 3066 | 4276 | 2476217 | 225883 | 0.996 | 1.59 |

Worse than the default on a gate above area: below 24 on 8 to 12 of the 17 views (layerweave-code 292 → about 6000
crossings, line–node hits on er-diagram and uml-class at 0); at 24 only mixed-large, bends 421 → 423; at 27 mixed-large,
crossings 531 → 533 and bends 421 → 425.

**Datasets as configured** (their own directions, strategies and buses; single pass runs on 15 of 17 there in both arms):

| arm | delegated | overlaps | hits | crossings | bends | length | area (k) | area / default (median) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| default | 32 | 0 | 0 | 5310 | 4934 | 2676331 | 196959 | 1.000 |
| gap 24 | 32 | 0 | 0 | 5310 | 4934 | 2670276 | 196371 | 1.000 |

## Reading

- **The threshold is exactly two margins (24).** At 24 and above, single pass delegates the same 16 routes as the default.
  At 21 it delegates 462 and the search draws them: crossings triple. The construction keeps a margin on each side of
  whatever it places in a channel, also in a channel the planning counted empty — most likely a lane change it decides
  itself; a single delegation was not traced, so this reason is not proven.
- **So only the empty track is free**, 6 of the 30. Taking it buys 0.7 % area at the median on the uniform task and
  nothing measurable on the configured datasets (0.3 % in sum, median 1.000), and on mixed-large it already costs two
  bends — under the rule that quality must not get worse, that is a fail too.
- The 13 % of the length that the fourth step attributed to "the channel at a border rank" is four fifths the margins
  the construction needs. It is not slack.

Decision: the option is not kept. The patch is saved beside this file so the run can be repeated; the engine is
unchanged.

## What this leaves for issue #119

With single pass fixed and no loss of quality allowed, the parts of the extent that are not quality trade-offs are
small. Straight long edges (34 % across) cost bends if packed; spacings and frames were rejected on 2026-11-22; the
border channels are, as measured here, margins the construction needs. The one structural part not yet tried is the
**stacked sibling areas** (10 % across in sum, concentrated: layerweave-code 4235 of 9581) — and `packComponents`,
the existing way to repack them, was measured to trade height for width without an area gain. A real change there
means giving unlinked sibling areas ranks along the flow inside single pass's per-area construction: a design
question, not a parameter.

# Issue #119, sixth step: `simplexRanks` under single pass (2026-11-23)

Before proposing a design for the stacked sibling areas, the one existing option that changes the ranks inside an area
was measured where it had never been measured: under single pass, for area. `simplexRanks` (experimental, off) ranks
every layered area by network simplex — the summed spans of its links are least — instead of by longest paths; it was
last measured on 2026-11-03 on the old pipeline, for crossings and bends only.

`node benchmarks/eval/option-gates.mjs --arms '[["simplexRanks",{"simplexRanks":true}],["packComponents",{"packComponents":{}}]]'`
(and `--pose native`), release build of `1a21a6f`, clean; single pass ran wherever it runs in the default.

**Uniform task**, sums over 17 views:

| arm | single pass | delegated | overlaps | hits | crossings | bends | length | area (k) | area / default (median) | area / elk-rs (median) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| default | 17/17 | 16 | 0 | 0 | 3064 | 4272 | 2482075 | 227248 | 1.000 | 1.60 |
| simplexRanks | 17/17 | 12 | 0 | 0 | **2931** | 4290 | **2287983** | **206002** | 1.000 | **1.39** |
| packComponents | 17/17 | 21 | 0 | 0 | 3236 | 4464 | 2586425 | 221768 | 1.000 | 1.60 |

Per view, judged in objective order (overlaps, hits, crossings, then bends):

| dataset | area default→simplex (k) | change | width×height default → simplex | crossings | bends | by objective order |
|---|---|---:|---|---|---|---|
| activity-order | 1715 → 1715 | 0.0 % | 1746×982 → 1746×982 | 14→14 | 82→82 | equal |
| arqlet-pipeline | 3340 → 3235 | -3.1 % | 1725×1936 → 1666×1942 | 5→6 | 64→68 | worse |
| bpmn-pools | 1363 → 1363 | 0.0 % | 2626×519 → 2626×519 | 1→1 | 30→30 | equal |
| can-bus | 1228 → 1228 | 0.0 % | 1245×986 → 1245×986 | 6→6 | 40→40 | equal |
| cloud-architecture | 4249 → 4338 | 2.1 % | 3632×1170 → 3622×1198 | 14→14 | 98→100 | worse |
| deployment-cluster | 3977 → 4128 | 3.8 % | 1769×2248 → 1942×2126 | 96→87 | 212→202 | better |
| er-diagram | 1721 → 1846 | 7.3 % | 1146×1502 → 1146×1611 | 0→1 | 15→17 | worse |
| layerweave-code | 55605 → 53568 | -3.7 % | 5804×9581 → 5724×9359 | 292→267 | 1204→1184 | better |
| mixed-large | 28044 → 23002 | -18.0 % | 5223×5369 → 5379×4276 | 531→485 | 421→403 | better |
| network-vlan | 4328 → 4328 | 0.0 % | 3864×1120 → 3864×1120 | 24→24 | 110→110 | equal |
| org-chart | 19893 → 13369 | -32.8 % | 5820×3418 → 3993×3348 | 29→16 | 172→174 | better |
| petri-net | 1389 → 1396 | 0.5 % | 1734×801 → 1756×795 | 32→27 | 84→72 | better |
| state-order | 399 → 399 | 0.0 % | 1772×225 → 1772×225 | 0→0 | 19→19 | equal |
| state-tcp | 357 → 357 | 0.0 % | 1698×210 → 1698×210 | 0→0 | 26→26 | equal |
| synthetic-2k | 93987 → 86064 | -8.4 % | 15082×6232 → 15098×5700 | 1789→1753 | 1358→1424 | better |
| uml-class | 2629 → 2642 | 0.5 % | 1730×1520 → 1752×1508 | 1→0 | 37→39 | better |
| workflow-dag | 3026 → 3026 | 0.0 % | 2756×1098 → 2756×1098 | 230→230 | 300→300 | equal |

**Datasets as configured**, sums over 17 views (single pass on 15 of them in both arms):

| arm | delegated | overlaps | hits | crossings | bends | length | area (k) | area / default (median) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| default | 32 | 0 | 0 | 5310 | 4934 | 2676331 | 196959 | 1.000 |
| simplexRanks | 33 | 0 | 0 | 5213 | 4943 | 2570804 | 191298 | 1.000 |

Worse than the default on a gate above area: arqlet-pipeline (crossings 25 → 32), deployment-cluster (109 → 114),
org-chart (48 → 52), uml-class (6 → 9, bends 49 → 53); mixed-large and synthetic-2k have more bends (150 → 163,
1570 → 1633) but fewer crossings (1030 → 1009, 3037 → 2975), so they are better in objective order.

## Reading

- **The largest area lever measured inside single pass**: 9.3 % less area in sum on the uniform task, 2.9 % on the
  configured datasets, with fewer crossings in sum on both (−133, −97) and less edge length (−8 %, −4 %). The median of
  the per-view ratio to elk-rs moves from 1.60 to 1.39 because several views cross the middle; the sum is the steadier
  figure. The gain is concentrated: org-chart −32.8 %, mixed-large −18.0 %, synthetic-2k −8.4 %.
- **Not uniformly better.** In objective order it is worse on 3 of 17 views on the uniform task (arqlet-pipeline and
  er-diagram one crossing more, cloud-architecture two bends more) and on 4 of 17 configured ones (three to seven
  crossings more). Under the rule that quality must not get worse, it cannot become the default as it is.
- **It does not solve the stacking.** layerweave-code gains 3.7 % only (9581 → 9359 across): the 19 unlinked areas move
  from the first rank towards the border rank, which shortens their chains, but they still stand in one rank.
- `packComponents` confirms its earlier verdict under single pass: no area gain, more crossings and bends.

This is the basis of ADR 0027 (proposed): keep simplex ranks per area only where they do not cost crossings, and spread
the unlinked sibling areas along the flow as a second, separate step.
