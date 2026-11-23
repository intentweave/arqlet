# ADR 0027, step 0: the plan-crossing guard, checked offline (2026-11-23)

Question (ADR 0027, step 0, from the review of #162): would the guard of step 1 — keep simplex ranks for a layered area
only when its plan is not worse in what the plan can see, first crossings, then long-edge dummies — reject simplex on the
areas that make the 3 (uniform) and 4 (configured) views worse, and keep it on the areas that carry the area gain? No
engine change.

**Answer: no.** The guard fails the check on both poses. Most regressions do not lie in an area whose ranks changed at
all, and inside the changed areas the plan's verdict agrees with the drawing on only 12 of 15 (uniform) and 11 of 17
(configured) areas.

## Machine, build, inputs

- Apple M3, macOS 26.0, Node 24.21.0; release `layout-command` built from `1a21a6f` (this branch adds no engine code).
  The times quoted are single runs on a loaded machine: indicative only.
- `node benchmarks/eval/simplex-guard.mjs --pose uniform` and `--pose native`: the 17 layered views of
  `benchmarks/datasets` bounded to 150 nodes, as in the earlier steps; `layerweave-code` is the drifted copy (1105 nodes /
  2461 edges). The probe lays every view out twice, with the default ranks and with `simplexRanks`, reads every area's
  stored plan from both, and gives every drawn crossing and bend to the innermost container around it. **The attribution
  adds up exactly to the `metrics` totals on every view, both arms, both poses.** Overlaps and line–node hits are 0 in
  both arms throughout, so only crossings, then bends, decide.
- Limit of the offline reading: in the simplex layout every area uses simplex ranks, so an area's simplex plan is ordered
  around children that use them too; step 1 would order it around the children it chose. It is the closest comparison
  without an engine change.

## Where the losses lie

A view worse than the default in objective order has loss areas: areas whose own drawn crossings, then bends, got worse.

| pose | views worse | loss areas | in an area whose ranks changed | of them rejected by the guard | outside the changed areas |
|---|---:|---:|---:|---:|---:|
| uniform | 3 | 5 | 2 | **0** | 3 (an ancestor, a child, one apart) |
| configured | 4 | 11 | 3 | 2 | 8 (an ancestor, four children, three apart) |
| **both** | 7 | **16** | 5 | **2** | **11** |

- **11 of 16 losses lie outside the areas whose ranks changed** — not only in ancestors, as ADR 0027 assumed when it
  separated a local guard from the global acceptance, but also in **children** of a changed area (5: a parent's new
  ranks move where its edges enter a child, and the child draws them with more bends) and in areas **apart** from it (4:
  siblings and cousins whose neighbours moved). No guard that compares two layouts of one area — from its plan or from
  its constructed routes — can see them.
- Of the 5 losses inside a changed area, the guard rejects simplex on 2 (deployment-cluster `ns-app`, org-chart `div-2`)
  and keeps it on 3: arqlet-pipeline `crate:runtime` and er-diagram's root (uniform: plan crossings equal, one dummy
  fewer, drawn +6 bends / +1 crossing +2 bends) and uml-class `pkg:domain` (configured: plan equal, four dummies fewer,
  drawn +2 bends).

## How well the plan predicts the drawing, inside the changed areas

| pose | areas with changed ranks | drawn equal | guard keeps, area better | keeps, area worse | rejects, area worse | rejects, area better | agrees |
|---|---:|---:|---:|---:|---:|---:|---:|
| uniform | 19 | 4 | 8 | 3 | 4 | 0 | 12 of 15 |
| configured | 19 | 2 | 5 | 4 | 6 | 2 | 11 of 17 |

Where the plan's crossings differ a lot, the drawing usually follows on the uniform pose (mixed-large `data` 4 → 31 plan,
+31 drawn; synthetic-2k `g0-214` 36 → 165, +129). On the configured pose it does not always: synthetic-2k `g0-215` has
fewer plan crossings (144 → 125) and 40 more drawn; mixed-large `data` an equal plan and 64 more drawn; `g0-214` more plan
crossings (89 → 151) and 2 fewer drawn.

## The gain side

On the uniform pose the guard keeps simplex on every area whose box shrank (mixed-large's root, org-chart's `div-2` and
`dep-0-0`, synthetic-2k's `g0-215`). But synthetic-2k's view gain (−8.4 %) is not in any changed area's box (they shrank by
22k in sum against 7923k for the view; the root's ranks did not change): it spreads through the hierarchy, so whether a
guarded layout keeps it cannot be read offline. On the configured pose the guard would keep only 14 % of the box
reduction of synthetic-2k's changed areas, and org-chart's area grows under simplex there (17884k → 18446k).

## A fallback the ADR did not list: a choice per view

Since the regressions spread across areas, the one guard that sees all of them is the view itself: lay the view out with
both rankings and keep simplex only where the view is not worse in objective order (ties: the smaller area). That is a
choice of the kind the portfolio already makes elsewhere. It can be evaluated exactly from the gate runs of the sixth step
(`option-gates.mjs`), so its numbers are not offline estimates:

| pose | views on simplex | crossings | bends | area | layout time (both arms) |
|---|---:|---|---|---:|---|
| uniform | 7 of 17 | 3064 → 2929 | 4272 → 4282 | **−9.4 %** | 392 → 758 ms |
| configured | 6 of 17 | 5310 → 5194 | 4934 → 4968 | **−3.2 %** | 2921 → 14672 ms |

No view is worse in objective order, by construction; bends rise in sum only on views that trade them for fewer
crossings, as the objectives allow. The cost is a second layout per view — and more: on the configured mixed-large, which
does not run single pass (it falls back to the legacy pipeline, as can-bus does there), `simplexRanks` makes the layout
4.5 to 7 times slower (1.9–3.2 s → 13.0–14.6 s over three single runs). A choice per view would need to be limited to views
that run single pass, and its time measured on an idle machine before anything is decided.

## What this means for ADR 0027

Step 0 fails, so by the ADR the fallback is decided before step 1 is built. The options, as measured:

1. **No step 1.** The default stays as it is.
2. **A choice per view** between the two rankings, behind an option, limited to views that run single pass: passes the
   acceptance by construction on the measured views; −9.4 % area on the uniform task and −3.2 % on the configured
   datasets; costs a second layout per view (time to be measured on an idle machine).
3. **A guard on the constructed routes of an area** (the ADR's listed fallback): would see at most the 5 losses inside
   changed areas, not the 11 outside, so it cannot pass the acceptance. Not recommended.

## Per view and area


### Uniform pose

pose uniform, limit 150

Attribution adds up to the `metrics` totals on every view (crossings and bends, both arms).

| view | worse in objective order | areas | ranks changed | guard keeps simplex | guard keeps longest |
|---|---|---:|---:|---:|---:|
| activity-order | no | 5 | 0 | 0 | 0 |
| arqlet-pipeline | **yes** | 7 | 1 | 1 | 0 |
| bpmn-pools | no | 6 | 1 | 0 | 1 |
| can-bus | no | 4 | 0 | 0 | 0 |
| cloud-architecture | **yes** | 15 | 1 | 1 | 0 |
| deployment-cluster | no | 10 | 2 | 2 | 0 |
| er-diagram | **yes** | 13 | 1 | 1 | 0 |
| layerweave-code | no | 37 | 3 | 2 | 1 |
| mixed-large | no | 21 | 2 | 1 | 1 |
| network-vlan | no | 6 | 0 | 0 | 0 |
| org-chart | no | 34 | 2 | 2 | 0 |
| petri-net | no | 1 | 1 | 1 | 0 |
| state-order | no | 2 | 0 | 0 | 0 |
| state-tcp | no | 1 | 0 | 0 | 0 |
| synthetic-2k | no | 82 | 4 | 2 | 2 |
| uml-class | no | 21 | 1 | 1 | 0 |
| workflow-dag | no | 9 | 0 | 0 | 0 |

**arqlet-pipeline** (worse: crossings 5 → 6, bends 64 → 68)

| area | ranks changed | plan crossings L → S | dummies L → S | guard | drawn Δ crossings | drawn Δ bends | loss |
|---|---|---|---|---|---:|---:|---|
| <root> | no | 0 → 0 | 7 → 7 | same | 1 | -4 | **yes** (ancestor of a changed area) |
| crate:model | no | 0 → 0 | 0 → 0 | same | 0 | 2 | **yes** (apart) |
| crate:runtime | yes | 5 → 5 | 16 → 15 | simplex | 0 | 6 | **yes** |

loss areas 3: in areas whose ranks changed 1 (guard rejects simplex on 0), outside the changed areas 2.

**cloud-architecture** (worse: crossings 14 → 14, bends 98 → 100)

| area | ranks changed | plan crossings L → S | dummies L → S | guard | drawn Δ crossings | drawn Δ bends | loss |
|---|---|---|---|---|---:|---:|---|
| az-b | no | 0 → 0 | 2 → 2 | same | 0 | 4 | **yes** (child of a changed area) |
| region | yes | 0 → 0 | 8 → 7 | simplex | 0 | -2 |  |

loss areas 1: in areas whose ranks changed 0 (guard rejects simplex on 0), outside the changed areas 1.

**er-diagram** (worse: crossings 0 → 1, bends 15 → 17)

| area | ranks changed | plan crossings L → S | dummies L → S | guard | drawn Δ crossings | drawn Δ bends | loss |
|---|---|---|---|---|---:|---:|---|
| <root> | yes | 0 → 0 | 1 → 0 | simplex | 1 | 2 | **yes** |

loss areas 1: in areas whose ranks changed 1 (guard rejects simplex on 0), outside the changed areas 0.

**bpmn-pools** — every area whose ranks changed (view area 1363k → 1363k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| lane:sales | 0 → 0 | 12 → 15 | longest | 1548×312 → 1548×323 | 0 | 0 |

**layerweave-code** — every area whose ranks changed (view area 55605k → 53568k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| crate:layout | 98 → 93 | 309 → 293 | simplex | 4544×9243 → 4476×9021 | -3 | -6 |
| …crates/layout/src/constructive.rs | 64 → 61 | 79 → 73 | simplex | 1178×1205 → 1172×1172 | -3 | -8 |
| …tes/layout/src/hierarchy_order.rs | 4 → 5 | 42 → 37 | longest | 1334×572 → 1346×590 | 1 | 0 |

**mixed-large** — every area whose ranks changed (view area 28044k → 23002k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| <root> | 38 → 19 | 57 → 44 | simplex | 5223×5369 → 5379×4276 | -74 | 8 |
| data | 4 → 31 | 35 → 27 | longest | 910×1148 → 974×1089 | 31 | -6 |

**org-chart** — every area whose ranks changed (view area 19893k → 13369k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| dep-0-0 | 0 → 0 | 6 → 5 | simplex | 2259×538 → 2214×528 | -10 | 2 |
| div-2 | 3 → 1 | 7 → 6 | simplex | 3059×982 → 2958×922 | -2 | -6 |

**synthetic-2k** — every area whose ranks changed (view area 93987k → 86064k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| g0-214 | 36 → 165 | 172 → 166 | longest | 1656×1218 → 1694×1218 | 129 | 6 |
| g0-215 | 40 → 35 | 108 → 107 | simplex | 1038×1218 → 1020×1218 | 1 | 2 |
| g0-217 | 106 → 169 | 67 → 91 | longest | 1214×1038 → 1250×1044 | 65 | 14 |
| g1-253 | 0 → 0 | 3 → 2 | simplex | 884×419 → 884×483 | 0 | 0 |

**mixed-large** (gain view: area 28044k → 23002k)

| area | plan crossings L → S | dummies L → S | guard | box Δ (k) | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---:|---:|---:|
| <root> | 38 → 19 | 57 → 44 | simplex | -5042 | -74 | 8 |

areas with changed ranks and a smaller box: 1; the guard keeps simplex on 1, holding 100 % of their summed box reduction.

**org-chart** (gain view: area 19893k → 13369k)

| area | plan crossings L → S | dummies L → S | guard | box Δ (k) | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---:|---:|---:|
| div-2 | 3 → 1 | 7 → 6 | simplex | -276 | -2 | -6 |
| dep-0-0 | 0 → 0 | 6 → 5 | simplex | -46 | -10 | 2 |

areas with changed ranks and a smaller box: 2; the guard keeps simplex on 2, holding 100 % of their summed box reduction.

**synthetic-2k** (gain view: area 93987k → 86064k)

| area | plan crossings L → S | dummies L → S | guard | box Δ (k) | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---:|---:|---:|
| g0-215 | 40 → 35 | 108 → 107 | simplex | -22 | 1 | 2 |

areas with changed ranks and a smaller box: 1; the guard keeps simplex on 1, holding 100 % of their summed box reduction.

### Configured datasets

pose native, limit 150

Attribution adds up to the `metrics` totals on every view (crossings and bends, both arms).

| view | worse in objective order | areas | ranks changed | guard keeps simplex | guard keeps longest |
|---|---|---:|---:|---:|---:|
| activity-order | no | 5 | 0 | 0 | 0 |
| arqlet-pipeline | **yes** | 7 | 1 | 1 | 0 |
| bpmn-pools | no | 6 | 1 | 0 | 1 |
| can-bus | no | 3 | 0 | 0 | 0 |
| cloud-architecture | no | 15 | 1 | 0 | 1 |
| deployment-cluster | **yes** | 10 | 2 | 1 | 1 |
| er-diagram | no | 13 | 1 | 1 | 0 |
| layerweave-code | no | 37 | 3 | 2 | 1 |
| mixed-large | no | 18 | 1 | 1 | 0 |
| network-vlan | no | 6 | 0 | 0 | 0 |
| org-chart | **yes** | 34 | 3 | 1 | 2 |
| petri-net | no | 1 | 1 | 1 | 0 |
| state-order | no | 2 | 0 | 0 | 0 |
| state-tcp | no | 1 | 0 | 0 | 0 |
| synthetic-2k | no | 82 | 4 | 2 | 2 |
| uml-class | **yes** | 21 | 1 | 1 | 0 |
| workflow-dag | no | 9 | 0 | 0 | 0 |

**arqlet-pipeline** (worse: crossings 25 → 32, bends 78 → 68)

| area | ranks changed | plan crossings L → S | dummies L → S | guard | drawn Δ crossings | drawn Δ bends | loss |
|---|---|---|---|---|---:|---:|---|
| <root> | no | 0 → 0 | 7 → 7 | same | 7 | 0 | **yes** (ancestor of a changed area) |
| crate:model | no | 0 → 0 | 0 → 0 | same | 0 | -3 |  |
| crate:runtime | yes | 10 → 9 | 3 → 0 | simplex | 0 | -7 |  |

loss areas 1: in areas whose ranks changed 0 (guard rejects simplex on 0), outside the changed areas 1.

**deployment-cluster** (worse: crossings 109 → 114, bends 227 → 219)

| area | ranks changed | plan crossings L → S | dummies L → S | guard | drawn Δ crossings | drawn Δ bends | loss |
|---|---|---|---|---|---:|---:|---|
| <root> | yes | 0 → 0 | 1 → 0 | simplex | 0 | 0 |  |
| cluster | no | 2 → 2 | 1 → 1 | same | -11 | -2 |  |
| deploy-api | no | 0 → 0 | 0 → 0 | same | 0 | 2 | **yes** (child of a changed area) |
| deploy-worker | no | 0 → 0 | 0 → 0 | same | 0 | 4 | **yes** (child of a changed area) |
| ns-app | yes | 6 → 7 | 40 → 39 | longest | 14 | -12 | **yes** |
| ns-data | no | 0 → 0 | 3 → 3 | same | 2 | 2 | **yes** (child of a changed area) |
| sts-postgres | no | 0 → 0 | 4 → 4 | same | 0 | -2 |  |

loss areas 4: in areas whose ranks changed 1 (guard rejects simplex on 1), outside the changed areas 3.

**org-chart** (worse: crossings 48 → 52, bends 335 → 324)

| area | ranks changed | plan crossings L → S | dummies L → S | guard | drawn Δ crossings | drawn Δ bends | loss |
|---|---|---|---|---|---:|---:|---|
| dep-0-0 | yes | 0 → 0 | 2 → 1 | simplex | -10 | -6 |  |
| dep-0-1 | no | 0 → 0 | 0 → 0 | same | 6 | -3 | **yes** (apart) |
| dep-2-1 | no | 1 → 0 | 1 → 0 | same | 0 | -2 |  |
| dep-2-2 | yes | 0 → 0 | 0 → 0 | longest | 0 | -2 |  |
| div-0 | no | 7 → 7 | 2 → 2 | same | 0 | -1 |  |
| div-2 | yes | 0 → 2 | 2 → 1 | longest | 2 | 1 | **yes** |
| team-0-1-0 | no | 0 → 0 | 0 → 0 | same | 5 | 0 | **yes** (apart) |
| team-2-2-0 | no | 0 → 0 | 0 → 0 | same | 1 | 2 | **yes** (child of a changed area) |

loss areas 4: in areas whose ranks changed 1 (guard rejects simplex on 1), outside the changed areas 3.

**uml-class** (worse: crossings 6 → 9, bends 49 → 53)

| area | ranks changed | plan crossings L → S | dummies L → S | guard | drawn Δ crossings | drawn Δ bends | loss |
|---|---|---|---|---|---:|---:|---|
| <root> | no | 0 → 0 | 0 → 0 | same | 0 | -1 |  |
| pkg:domain | yes | 0 → 0 | 4 → 0 | simplex | 0 | 2 | **yes** |
| pkg:persistence | no | 0 → 0 | 0 → 0 | same | 3 | 3 | **yes** (apart) |

loss areas 2: in areas whose ranks changed 1 (guard rejects simplex on 0), outside the changed areas 1.

**bpmn-pools** — every area whose ranks changed (view area 1940k → 2165k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| lane:sales | 1 → 2 | 5 → 2 | longest | 635×910 → 726×834 | 2 | -7 |

**cloud-architecture** — every area whose ranks changed (view area 4308k → 4561k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| region | 0 → 1 | 8 → 7 | longest | 2461×1489 → 2467×1574 | 1 | -2 |

**deployment-cluster** — every area whose ranks changed (view area 4711k → 4561k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| <root> | 0 → 0 | 1 → 0 | simplex | 3064×1538 → 2972×1535 | 0 | 0 |
| ns-app | 6 → 7 | 40 → 39 | longest | 1242×945 → 1162×882 | 14 | -12 |

**layerweave-code** — every area whose ranks changed (view area 55605k → 53568k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| crate:layout | 98 → 93 | 309 → 293 | simplex | 4544×9243 → 4476×9021 | -3 | -6 |
| …crates/layout/src/constructive.rs | 64 → 61 | 79 → 73 | simplex | 1178×1205 → 1172×1172 | -3 | -8 |
| …tes/layout/src/hierarchy_order.rs | 4 → 5 | 42 → 37 | longest | 1334×572 → 1346×590 | 1 | 0 |

**mixed-large** — every area whose ranks changed (view area 21452k → 18485k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| data | 0 → 0 | 1 → 0 | simplex | 1616×618 → 2072×695 | 64 | 5 |

**org-chart** — every area whose ranks changed (view area 17884k → 18446k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| dep-0-0 | 0 → 0 | 2 → 1 | simplex | 899×986 → 914×1034 | -10 | -6 |
| dep-2-2 | 0 → 0 | 0 → 0 | longest | 418×568 → 448×502 | 0 | -2 |
| div-2 | 0 → 2 | 2 → 1 | longest | 1808×1541 → 1768×1729 | 2 | 1 |

**synthetic-2k** — every area whose ranks changed (view area 62193k → 60492k)

| area | plan crossings L → S | dummies L → S | guard | box w × h default → simplex | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---|---:|---:|
| g0-214 | 89 → 151 | 18 → 20 | longest | 1892×1008 → 1920×794 | -2 | 22 |
| g0-215 | 144 → 125 | 96 → 91 | simplex | 1186×1218 → 1128×1218 | 40 | 1 |
| g0-217 | 114 → 148 | 16 → 27 | longest | 1356×888 → 1398×762 | 17 | 4 |
| g1-253 | 0 → 0 | 1 → 0 | simplex | 1048×425 → 1020×420 | 2 | -1 |

**mixed-large** (gain view: area 21452k → 18485k)

| area | plan crossings L → S | dummies L → S | guard | box Δ (k) | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---:|---:|---:|

areas with changed ranks and a smaller box: 0; the guard keeps simplex on 0, holding 0 % of their summed box reduction.

**org-chart** (gain view: area 17884k → 18446k, also worse in objective order)

| area | plan crossings L → S | dummies L → S | guard | box Δ (k) | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---:|---:|---:|
| dep-2-2 | 0 → 0 | 0 → 0 | longest | -13 | 0 | -2 |

areas with changed ranks and a smaller box: 1; the guard keeps simplex on 0, holding 0 % of their summed box reduction.

**synthetic-2k** (gain view: area 62193k → 60492k)

| area | plan crossings L → S | dummies L → S | guard | box Δ (k) | drawn Δ crossings | drawn Δ bends |
|---|---|---|---|---:|---:|---:|
| g0-214 | 89 → 151 | 18 → 20 | longest | -384 | -2 | 22 |
| g0-217 | 114 → 148 | 16 → 27 | longest | -139 | 17 | 4 |
| g0-215 | 144 → 125 | 96 → 91 | simplex | -71 | 40 | 1 |
| g1-253 | 0 → 0 | 1 → 0 | simplex | -16 | 2 | -1 |

areas with changed ranks and a smaller box: 4; the guard keeps simplex on 2, holding 14 % of their summed box reduction.
