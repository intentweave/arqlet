# Single pass: fewer placement passes, fewer sweep runs, no repeated coordination, 2026-11-07

Phase-level work on the provisional `singlePass` mode (`{"abstractRouting":true,"singlePass":true}`). The goal is to
run whole phases fewer times, not to make lookups cheaper. Revision `d14db45` plus uncommitted changes, Apple M3
(8 logical CPUs), release builds. The default path is unchanged: all 20 stored default fingerprints are identical to a
build of `HEAD` (`benchmarks/fingerprint_compare.py`, threads 1 and 4).

## Changes

1. **Reversals before the first placement.** An area's reversed links depend only on its own child links (Kahn order
   and simplex ranks), not on coordinates. `known_reversals` computes them for every area before the first placement;
   the border sides and the structure cache key read them from there. The separate backward-edge rebuild pass is no
   longer needed. Checked: the precomputed reversals equal the plans' reversed links in all 39 areas of the code view.
2. **First pass without geometry.** With the reversals known, the first placement only orders (ranks, chains,
   borders) and computes no coordinates. The hierarchical sweep then runs on that plan, and the final placement is the
   only pass with geometry. Before: three placement passes with geometry (initial, backward rebuild, hierarchical
   rebuild). Now: one order-only pass and one geometry pass. Applies when no area chooses a strategy and nothing is
   flattened; otherwise the old flow runs.
3. **Five sweep runs instead of eight** under `singlePass` (`RUNS_SINGLE_PASS`; the default keeps `RUNS = 8`).
4. **No repeated coordination** (output-identical, all abstract-routing modes). The chain step probes the joint and
   continuity track assignment and derives chain orders from it. When the probe derives no orders, the following
   ordering pass would run the same assignment on the same input; it now takes the probe's result instead. The probe
   derived no orders in every realization of the three frozen views.
5. **No repeated measurement in the refinement** (output-identical). The input's published key and its near-node
   count come from one measurement; under `singlePass`, with no candidates and no rounds, the key is not measured at
   all, because nothing is compared with it.

Items 4 and 5 leave the output byte-identical: same geometry and refinement fingerprints on the three frozen views, and
the same corpus result on all 30 views whose input did not change. `layerweave-code` is generated from the repository
source; its corpus view changed with the source (197 → 198 nodes) and is not comparable.

## Quality: seed means

Changing only the shuffle seed of the sweep runs ≥ 3 moves the frozen code view between 2881 and 3693 crossings, so a
single run does not decide anything. Each variant ran with 5 seeds on 34 views (31 corpus views and the 3 frozen views);
sums of the single-pass arm, mean over the seeds:

| Variant | Crossings | Bends |
|---|---:|---:|
| old flow (three geometry passes, 8 runs) | 10487 | 8545 |
| order-only, 8 runs | 10110 | 8598 |
| **order-only, 5 runs (chosen)** | **9919** | **8559** |
| order-only, 3 runs | 10165 | 8554 |

Line–node hits and issues are unchanged in every variant (corpus 101 hits in mixed-large, petri-net-shapes,
mixed-layouts and self-loops, as before this work). The chosen variant is not better everywhere: the frozen code view's
seed mean is 3439 / 1594 against 3322 / 1569 in the old flow, while the corpus code view improves (4049 → 3277
crossings).

Rejected, measured with the same seeds and views:

- merging the backward rebuild into the hierarchical rebuild: corpus crossings 6867 → 8442;
- two order-only passes: no gain over one;
- keeping the sweep's border order in the placement instead of re-sorting it by lanes: crossings fall (code 3226),
  bends rise sharply (code 1802, workflow 542);
- three sweep runs: worse than five (table).

## Quality gate, frozen views

`node benchmarks/eval/single-pass.mjs --repeat 3`, raw: [single-pass-phase-20261107.json](single-pass-phase-20261107.json),
load 3.5 at the start and 3.2 at the end. Public path: two `layout-command` processes with JSON transport.

| View | singlePass | default | elk-rs |
|---|---|---|---|
| layerweave-code | 3362 cr / 1618 b, 0 hits, 122 ms | 4113 / 1844, 1358 ms | 3057 / 1638 |
| workflow-dag | 234 / 448, 0 hits, 25 ms | 256 / 618, 101 ms | 278 / 434 |
| org-chart | 30 / 190, 0 hits, 27 ms | 35 / 208, 85 ms | 23 / 148 |

Corpus, single-pass arm only (`--corpus --arms singlePass --no-elk`, raw:
[single-pass-phase-corpus-20261107.json](single-pass-phase-corpus-20261107.json)): 6336 crossings / 6264 bends,
101 hits, 8 issues; 6203 / 6285 with the previous source's code view.

## Instruction counts

Retired instructions per import, `/usr/bin/time -l bench --graph VIEW --limit 1000 --reps N --threads none
--pipeline-only 1 --import-options '{"abstractRouting":true,"singlePass":true}'`, (T(5) − T(1)) / 4. Wall-clock times on
this machine were bimodal under load, so these counts decide.

| View | before (3 passes, 8 runs) | + order-only, 5 runs | + no repeated coordination |
|---|---:|---:|---:|
| layerweave-code | 1435 M | 1071 M | **1004 M (−30 %)** |
| workflow-dag | 230 M | 159 M | **149 M (−35 %)** |
| org-chart | 216 M | 164 M | **158 M (−27 %)** |

Refinement of the code view (`--threads 1`, each repetition runs it twice): about 331 M → 229 M instructions per
run after item 5.

## Timing in one process

`node benchmarks/eval/single-pass-timing.mjs --repeat 21 --bench /tmp/lw-plain/release/bench`, raw:
[single-pass-phase-timing-20261107.json](single-pass-phase-timing-20261107.json). Medians of 21. **Loaded machine:**
load 8.1 at the start, 3.7 at the end; the figures are indicative, the instruction counts above decide.

| View | default | abstract | singlePass import + refine | elk-rs | elkjs |
|---|---:|---:|---:|---:|---:|
| layerweave-code | 1348 ms | 673 ms | 77 + 15 = **92 ms** | 50 ms | 285 ms |
| workflow-dag | 90 ms | 91 ms | 12 + 2 = **13 ms** | 11 ms | 61 ms |
| org-chart | 75 ms | 73 ms | 12 + 1 = **14 ms** | 9 ms | 43 ms |

Against the quiet-machine table of [2026-11-06](single-pass-timing-20261106.md) (code 106 + 19 = 124 ms), the code view
is about 1.8 times slower than elk-rs instead of 2.5 times. That comparison mixes machine states; a repeat on a quiet
machine is still open.

## Where the code-view import spends its time now

Sampled profile of `compute_once` after the changes: geometry placement pass 28 %, hierarchical sweep about 14 %,
chain probe (joint and continuity assignment) 11 %, channel growth 8.5 % (it realizes the plan once without
coordination to measure channel demand), abstract channel planning 7.5 %. Nothing is rerouted after construction: on
the frozen code view 345 edges are constructed and 13 delegated edges are drawn as seed routes, without fallback search
or track repair, with 0 line–node hits; 10 strict track pairs stay unresolved. Constructing those 13 edges directly is
the open part of "construct correctly instead of rerouting"; the 101 corpus hits remain too.
