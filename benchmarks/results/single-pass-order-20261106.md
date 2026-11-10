# Single pass: physical channel order and hierarchical order (2026-11-06)

Raw data: [`single-pass-order-20261106.json`](single-pass-order-20261106.json), produced by
`node benchmarks/eval/single-pass.mjs --repeat 3 --out …` on an Apple M3 (8 logical CPUs), revision
`d14db45` with uncommitted changes, release build with `benchmark-timings`. The machine was shared and
heavily loaded (load average 15–17 on 8 cores): **the times are diagnostic, not a performance result.**
Crossings, bends and line–node hits are deterministic and match the single-sample runs.

## Changes measured

1. **Physical channel order** (`abstractRouting` and `singlePass`): channel slots follow the
   dependency order of the spans (`dependent_order`) instead of the ordinal plan order, which only knows
   bundled container links by edge id. Only the public completion of a snapshot with `abstractRouting`
   uses it; the diagnostic realization trials (`realize_*`, regional and repair prototypes on default
   snapshots) keep the ordinal order their mechanisms and tests are built on.
2. **Border correction before the hierarchical order** (`singlePass` only): the first placement runs
   bottom-up and cannot know which links the areas above reverse (`reversed_above`), so its border ranks
   can be on the wrong side. The hierarchical sweep therefore ordered ranks that the backward-edge
   rebuild replaced; the given order no longer fit (org chart: sweep model 23 crossings, 101 after
   placement). Single pass now runs the backward-edge rebuild first, sweeps on the corrected plans and
   places once more. The sweep model and the placed order now agree exactly (org chart 15, workflow 222,
   code 2878 model crossings).
3. **Border ranks kept** (`singlePass` only, `AreaPlan::fixed_borders`): the placement keeps the order of
   the pure border ranks the sweep chose and improves only the inner ranks around them
   (`improve_order_around`).

## Result (crossings / bends / line–node hits, median time of 3)

| View | default | abstract | singlePass | elk-rs |
|---|---|---|---|---|
| layerweave-code | 4113 / 1844 / 0, 1492 ms | 4071 / 2064 / 0, 771 ms | 3487 / 2092 / 0, 284 ms | 3057 / 1638 / 0, 46 ms |
| workflow-dag | 256 / 618 / 0, 116 ms | 326 / 458 / 0, 120 ms | 240 / 452 / 0, 38 ms | 278 / 434 / 0, 10 ms |
| org-chart | 35 / 208 / 0, 97 ms | 53 / 192 / 0, 101 ms | 30 / 186 / 0, 37 ms | 23 / 148 / 0, 8 ms |

Before these changes, single pass drew 7222 / 456 / 97 crossings (code / workflow / org chart).
Single pass on the code view leaves 12 delegated edges as seed routes and 6 unresolved strict track pairs
(spacing below the strict track distance; it does not repair them).

## Variants tried (single-pass crossings / bends; code, workflow, org chart)

| Placement of the swept order | code | workflow | org chart |
|---|---|---|---|
| no freezing (placement re-orders every rank) | 3749 / 1976 | 244 / 452 | 30 / 186 |
| every rank frozen (`fixed`) | 3589 / 2226 | 240 / 452 | 30 / 186 |
| border ranks frozen, also against geometric re-sorting of border ranks | 3396 / 2398 | 230 / 536 | 30 / 202 |
| **border ranks frozen (chosen)** | 3487 / 2092 | 240 / 452 | 30 / 186 |

The chosen variant has the lowest sum of crossings and bends on all three views. Per-edge bundle places
for chain dummies in the sweep model changed only its start value, not its result, and were dropped.

## Not changed

The default path: all 20 `bench` configurations (`fingerprints-20261021T103351Z.jsonl` list, threads 1
and 4) are fingerprint-identical to a build of `HEAD`. The remaining gap to elk-rs is in node order
within large containers (code: `crate:layout`) and in bends.
