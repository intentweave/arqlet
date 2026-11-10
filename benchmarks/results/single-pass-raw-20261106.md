# Single pass on the real task, 2026-11-06

M0 of the plan to make `singlePass` the default: what it does on the datasets as they are, before any capability work. Revision
`5c2431e` plus the evaluation scripts, Apple M3 (8 logical CPUs), release builds. The default arm includes its refinement. The
judgement is per view, lexicographic in the order of `DEFAULT_OBJECTIVES` over seed means: sibling overlaps, line–node hits,
shared line pairs, crossings, hugging pairs, bends, length (`ProjectedCrossings` is not in the measured metrics).

Method: `node benchmarks/eval/single-pass.mjs --corpus [--raw] --repeat 1 --arms default,singlePass --no-elk --binary PATH`, with
five shuffle seeds from a copy of the source with the seed read from the environment. The copy is not committed. Datasets cut to
150 nodes, without `synthetic-20k` and the local export. Raw summary: [single-pass-raw-20261106.json](single-pass-raw-20261106.json).
Edits: `node benchmarks/eval/single-pass-edits.mjs`, one sample, raw [single-pass-edits-20261106.json](single-pass-edits-20261106.json).

## 1. Which datasets run

Of 31 unmodified datasets, 5 run and 26 are rejected with an error.

| Datasets | Reason | What it means |
|---:|---|---|
| 5 | run: er-diagram, fixed-ports, layerweave-code, petri-net, state-tcp | one flow axis |
| 11 | "requires one positive flow axis" | mixed `lr` and `tb` containers, none uses `rl` or `bt`. Includes org-chart and workflow-dag, two of the three reference views, which are only measured without their directions |
| 12 | "requires unflattened hierarchical placement" | no ranked area (tree, organic, radial): the engine does not use the layered path for them anyway |
| 3 | "requires the legacy router" | mixed-large, mixed-layouts, self-loops: layered areas with organic or straight regions |

So nothing is known about quality on the 11 + 3 views that the mode should eventually serve.

## 2. Against the refined default

**The 5 views that run** (seed means, default → single pass):

| View | Verdict | Decided by | Crossings | Bends | Shared line pairs |
|---|---|---|---:|---:|---:|
| er-diagram | worse | shared line pairs | 0 → 0 | 29 → 21 | 0 → 1 |
| fixed-ports | better | bends | 1 → 1 | 16 → 12 | 0 → 0 |
| layerweave-code (204 nodes) | worse | shared line pairs | 2189 → 1179 | 1529 → 1219 | 0 → 2 |
| petri-net | better | crossings | 50 → 32 | 102 → 84 | 0 → 0 |
| state-tcp | better | bends | 0 → 0 | 44 → 26 | 0 → 0 |

Both "worse" verdicts come from shared line pairs alone, which rank above crossings. On the code view single pass has 46 % fewer
crossings and 20 % fewer bends.

**Normalized corpus** (31 views, uniform direction, the simplified task): 19 better, 12 worse. Sums: crossings 5627 → 3967, bends
7046 → 6096, hits 101 → 95, shared line pairs 5 → 8, geometry issues 0 → 4, all issues 1 → 6.

The 12 worse:

| Views | Cause | Relevant to the real task? |
|---|---|---|
| tree-25 to tree-300 (5), organic-25, organic-200 | bends or a few crossings | no: on real input they do not use this path |
| mixed-large | hits 45 → 46, shared 0 → 2, **3 geometry and 1 routing issues against none**, although crossings fall 691 → 529 | yes |
| self-loops | shared 3 → 4 | yes |
| deployment-cluster | crossings 88 → 95 | yes |
| layerweave-pipeline | crossings 6.2 → 6.8 | yes |
| layerweave-code | shared 0 → 2 | yes |

mixed-layouts counts as better by its hits (8.4 → 7), but it has 1 geometry and 1 routing issue against none, and crossings 0.2 → 4.8.

**Frozen views:** all three better. Crossings / bends: code 4208 / 1877 → 3784 / 1634, workflow 244 / 618 → 232 / 452, org chart
30 / 207 → 32 / 186 (decided by shared line pairs 2 → 0).

**Answer to the question of refinement:** on these views the unrefined single pass mostly beats the refined default, so a
refinement without effect does not cost quality against today, except on the views listed above.

## 3. Against elk-rs

Only on the simplified task. The adapter sets one direction at the root, so elk-rs is no reference on mixed views.

| | single pass | elk-rs |
|---|---:|---:|
| Corpus, crossings | 3967 | 6213 |
| Corpus, bends | 6096 | **5632** |
| Corpus, hits | 95 | **0** |
| Code view | 3784 / 1634 | **3057** / 1638 |
| Workflow | 232 / 452 | 278 / **434** |
| Org chart | 32 / 186 | **23 / 148** |

Single-pass values are seed means, elk-rs is one deterministic run. The corpus crossings are dominated by few views.

## 4. Edits

On a drawing that single pass made, each operation starting from the import, five views:

| Operation | Single pass |
|---|---|
| add, shape, measure, settle, refine, structuredRefine, restore, toggle | run |
| **move (a drag)** | **rejected on all 5 views**, because its first step pins the node |
| pin | rejected on all 5 |
| change a container's direction | rejected on all 3 views with containers |
| routing straight | rejected on all 3 |
| strategy tree | rejected on workflow, the only view where the default accepts it |
| port | not covered |

A normal drag therefore ends in an error on a single-pass drawing. Earlier probes had a malformed `move` request and missed this.

Displacement of the other nodes, moved nodes / maximum distance, default → single pass:

| Operation | layerweave-code (77 others) | workflow (70) | org chart (103) | petri-net (23) |
|---|---|---|---|---|
| add | 4 / 232 → **51** / 246 | 70 / 488 → 70 / 557 | 103 / 937 → 48 / 204 | 21 / 106 → 16 / 48 |
| shape | 4 / 108 → **66** / 240 | 14 / 37 → **70** / 76 | 0 / 0 → **83** / 18 | 22 / 16 → 19 / **108** |
| measure | 4 / 62 → **25** / 82 | 70 / 86 → 64 / 104 | 0 / 0 → **83** / 36 | 22 / 96 → 19 / 82 |
| toggle | not comparable | 63 / 306 → 63 / **665** | 65 / 1677 → 65 / **5292** | no containers |

Single pass is not more stable than the default. It is much less stable where the default is local (shape and measure on the code
view and the org chart), and a toggle moves nodes further. One sample per view and operation, one seed; whole-drawing shifts are not
removed. An earlier one-view probe had suggested the opposite, on the `add` operation only.

## 5. Time

Median of five runs per view, import plus refine as two processes with JSON transport, on a machine shared with these runs, so
indicative only. Unmodified: layerweave-code at 204 nodes 1234 → 91 ms, petri-net 16 → 8 ms. Normalized: synthetic-2k at 231 nodes
1311 → 147 ms, workflow 84 → 23 ms.
