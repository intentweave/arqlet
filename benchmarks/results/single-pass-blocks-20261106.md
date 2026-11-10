# Single pass: block placement across the flow, and the 31-view corpus (2026-11-06)

> **Update (2026-11-06):** single pass no longer runs the global routing rounds or the Optimize-time crossing step;
> the code view is now 3291 crossings / 1592 bends. See [single-pass-timing-20261106.md](single-pass-timing-20261106.md).

Raw data: [`single-pass-blocks-20261106.json`](single-pass-blocks-20261106.json) (three frozen views,
`node benchmarks/eval/single-pass.mjs --repeat 3 --out …`) and
[`single-pass-corpus-20261106.json`](single-pass-corpus-20261106.json) (31 views,
`node benchmarks/eval/single-pass.mjs --corpus --repeat 1 --out …`). Apple M3 (8 logical CPUs), revision
`d14db45` with uncommitted changes, release build with `benchmark-timings`. The machine was shared and
heavily loaded (load average 16–24 on 8 cores; the corpus ran next to a test build): **the times are
diagnostic, not a performance result.** Crossings, bends and line–node hits are deterministic.

## Diagnosis

Almost all single-pass bends came from the placement, not from the router. For each edge the router
chooses one lane per run of junctions whose lane domains (the extent of the border node or dummy across
the flow) intersect; when two consecutive domains are disjoint, the edge must change lane (two bends).
Counted on the code view before this change: 1028 of 1033 lane changes were forced that way (about 2056
of 2092 bends), mostly by large offsets (≥ 100 px in 513 cases) between dummies of one chain in one area,
and between a port and the first or last dummy. The cause: the cross-axis placement (`align_lanes`: eight
median sweeps with isotonic regression per rank) does not converge to straight chains. In the org chart,
a whole rank stayed 80.6 px beside its targets after its upstream container moved in a later sweep.

## Change (`singlePass` only)

`crates/layout/src/block_placement.rs`: cross-axis coordinates by block alignment and compaction after
Brandes and Köpf (GD 2001), with the class shifts of the erratum by Brandes, Walter and Zink (2020) and
with port offsets inside elements (written from the papers; no code copied):

- the four variants (aligned up or down, compacted left or right), inner segments (dummy to dummy) win
  type 1 conflicts;
- balanced by the average of the two median places of every element when that keeps every rank's order
  and gaps, otherwise the narrowest variant;
- edge straightening afterwards: an element with no straight segment moves onto a neighbour's port line
  when its rank's order and gaps allow;
- runs on the ranks after the existing sweeps, so border ranks keep the order of their lanes.

The sweeps alone, and a final pass that moved only dummies, were measured and were not enough (code
2092 → 2080 bends at best).

## Variants (single-pass crossings / bends; code, workflow, org chart)

| Placement across the flow | code | workflow | org chart |
|---|---|---|---|
| sweeps (before) | 3487 / 2092 | 240 / 452 | 30 / 186 |
| blocks replacing the sweeps, balanced | 3607 / 1926 | 240 / 540 | 30 / 208 |
| blocks replacing the sweeps, narrowest variant | 3236 / 1726 | 224 / 532 | 30 / 198 |
| blocks after the sweeps, balanced | 3801 / 1750 | 250 / 452 | 30 / 204 |
| blocks after the sweeps, narrowest variant | 3497 / 1622 | 228 / 530 | 30 / 196 |
| **blocks after the sweeps, balanced, straightened (chosen)** | **3373 / 1586** | **240 / 460** | **30 / 190** |

## Result on the three frozen views (crossings / bends / hits, median time of 3)

| View | default | abstract | singlePass | elk-rs |
|---|---|---|---|---|
| layerweave-code | 4113 / 1844 / 0, 1519 ms | 4071 / 2064 / 0, 775 ms | 3373 / 1586 / 0, 273 ms | 3057 / 1638 / 0, 46 ms |
| workflow-dag | 256 / 618 / 0, 117 ms | 326 / 458 / 0, 121 ms | 240 / 460 / 0, 39 ms | 278 / 434 / 0, 10 ms |
| org-chart | 35 / 208 / 0, 98 ms | 53 / 192 / 0, 101 ms | 30 / 190 / 0, 37 ms | 23 / 148 / 0, 8 ms |

Single pass on the code view now leaves 13 delegated edges as seed routes (before 12) and 11 unresolved
strict track pairs (before 6).

## Corpus: 31 views (limit 150)

Every dataset in `benchmarks/datasets` except `synthetic-20k` and the Intentweave export, laid out as the
task elk-rs solves: one direction for every container (`auto` and `hybrid` become `lr`, as the elk-rs
adapter maps them), layered strategy, orthogonal routing; the view is cut by a default import. A first
run without the routing and direction normalization failed 16 views for `abstractRouting` and
`singlePass` (they reject straight routing and free directions); with it, all 31 views pass in every arm.

| Sum over 31 views | crossings | bends | line–node hits | views with hits |
|---|---|---|---|---|
| default | 7286 | 7224 | 102 | 4 |
| abstract | 8561 | 7566 | 102 | 4 |
| **singlePass** | **6667** | **6410** | 101 | 4 |
| elk-rs | 7472 | 5912 | 0 | 0 |

- Single pass against elk-rs, crossings per view: better in 10 (largest: synthetic-2k 1682 against 3660,
  activity-order 14 against 36, workflow-dag 240 against 278), equal in 13, worse in 8: layerweave-code
  3866 against 2604 at this cut (`benchmarks/generate.py` builds this dataset from the working tree, so it
  is not the frozen code view above), deployment-cluster 89 against 71, org-chart 30 against 23,
  petri-net-shapes 63 against 58, mixed-layouts 6 against 0, self-loops 4 against 0, organic-300 3 against
  0, fixed-ports 1 against 0.
- Bends remain above elk-rs in most views (sum 6410 against 5912), most in the tree and organic datasets
  under a uniform layered layout (for example tree-300: 198 against 148).
- **Finding:** line–node hits in mixed-large, mixed-layouts, petri-net-shapes and self-loops occur in
  every Layerweave arm, the default included (shaped nodes and self-loops); they are not caused by single
  pass, and they are not fixed.

## Not changed

The default path and `abstractRouting` without `singlePass`: all 20 `bench` configurations
(`fingerprints-20261021T103351Z.jsonl` list, threads 1 and 4) are fingerprint-identical to a build of
`HEAD`.
