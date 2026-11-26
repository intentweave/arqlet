# ADR 0027, step 1: network-simplex ranks as a refinement candidate (`simplexRanksCandidate`) (2026-11-26)

**Not merged.** The option, the code and the scripts named here are not on `main`: branch `simplex-refinement`, commit
df418da (pull request #201). Decision and summary: [ADR 0027](../../docs/adr/0027-area-inside-single-pass.md).

Issue #119. The refinement of a single-pass drawing gets one candidate: the view laid out again with `simplexRanks`,
taken only when its published key is better. Off by default.

**Answer.** The acceptance on quality and identity holds; the expected gain does not.

- No view is worse than the default in objective order on either pose. With the option off, import and refinement are
  byte for byte those of the build without the change.
- Uniform task: simplex ranks taken on **5 of 17** views (ADR: 7), crossings 4245 → 4181, area **−5.5 %** (ADR: −9.4 %).
- Configured datasets: taken on **3 of 16** (ADR: 6), crossings 6491 → 6483, area **+0.2 %** (ADR: −3.2 %).
- Area is not part of the published key: 6 of the 8 taken drawings are larger than the default.

## Machine, build, inputs

- Apple M3 (8 logical CPUs, 16 GB), macOS 26.0, rustc 1.99.0, Node 24.21.0. Release builds (`--release`, thin LTO) of
  `layout-command` and of `bench` (`--features benchmark-timings`), `CARGO_BUILD_JOBS=4`.
- Baseline: `origin/main` at `bfa8283`, built from an exported source tree into a target directory of its own. The
  change: that revision plus this branch (engine 0.9.0 in both). `origin/main` was then merged (`70d744b`: bus lines and
  lanes) and, on the same datasets, the gates gave the same figures on every view, the 15 fingerprints stayed identical
  (option off against the baseline, option on against the run before the merge) and the identity checks held. One
  process call of the edit study failed there (no engine answer; the error text was cut off, a time limit is the likely
  cause); its view was repeated: 24 of 24 edits, the same figures.
- Datasets: `python3 benchmarks/generate.py --no-manifest`. `layerweave-code`, generated from the source, is
  **1474 nodes / 3254 edges**, sha256 `7376a5b77a1400fe…` — the ADR's figures were measured on 1105 / 2461, so its sums
  are not comparable with these; compare per view.
- The machine was shared with other builds throughout. Quality figures are deterministic; for the times see their section.

## Commands

```sh
node benchmarks/eval/option-gates.mjs --refine --pose uniform --arms '[["simplexRanksCandidate",{"simplexRanksCandidate":true}]]'
node benchmarks/eval/option-gates.mjs --pose uniform --arms '[["simplexRanks",{"simplexRanks":true}]]'   # layout only
node benchmarks/eval/simplex-refinement.mjs --pose uniform --baseline <baseline layout-command>
# the same with --pose native (the configured datasets); fingerprints per reference view (limit 100):
target/release/bench --graph benchmarks/datasets/<name>.json --limit 100 --reps 1 --threads 1,4 --name <name> \
  [--refine-options '{"simplexRanksCandidate":true}']
python3 benchmarks/fingerprint_compare.py benchmarks/results/simplex-refinement-20261126-fingerprints-baseline.jsonl \
  benchmarks/results/simplex-refinement-20261126-fingerprints-off.jsonl
node benchmarks/eval/elk-layerers.mjs --pose uniform                                                      # step 2 bound
node benchmarks/eval/simplex-refinement-timing.mjs --out <file>                                            # times
```

Data: `simplex-refinement-20261126-{gates,ranks,identity}-{uniform,configured}.json`,
`-fingerprints-{baseline,off,on}.jsonl`, `-elk-layerers-{uniform,configured}.json`, `-timing.jsonl`.

## Gates (limit 150)

`option-gates.mjs` now has `--refine`: every arm, the default included, imports and refines, and the refined drawing is
scored. The keys in the last column are those of the two layouts of the view (`bench --threads none`, `inputPublishedKey`,
import with and without `simplexRanks`).

### Uniform task

| view | default: crossings / bends / area (k) | `simplexRanks`: crossings / bends / area (k) | refinement takes | refined: area (k) | change | why, by published key (overlaps, hits, shared pairs, crossings, near-node, projected crossings, bends, length) |
|---|---|---|---|---:|---:|---|
| activity-order | 14 / 82 / 1715 | 14 / 82 / 1715 | default | 1715 | – | same key |
| arqlet-pipeline | 5 / 64 / 3340 | 6 / 68 / 3235 | default | 3340 | – | crossings 5 → 6 |
| bpmn-pools | 1 / 30 / 1363 | 1 / 30 / 1363 | default | 1363 | – | equal up to bends, longer |
| can-bus | 6 / 40 / 1228 | 6 / 40 / 1228 | default | 1228 | – | same key |
| cloud-architecture | 14 / 98 / 4249 | 14 / 100 / 4338 | default | 4249 | – | projected crossings 20 → 21 |
| deployment-cluster | 96 / 212 / 3977 | 87 / 202 / 4128 | simplex ranks | 4128 | +3.8 % | **taken:** crossings 96 → 87 |
| er-diagram | 0 / 15 / 1721 | 1 / 17 / 1846 | default | 1721 | – | crossings 0 → 1 |
| layerweave-code | 1473 / 1171 / 86419 | 1723 / 1207 / 90198 | default | 86419 | – | crossings 1473 → 1723 |
| mixed-large | 531 / 421 / 28044 | 485 / 403 / 23002 | default | 28044 | – | refused by the geometry rules (container bounds; the default reports the same kind), although its key is better |
| network-vlan | 24 / 110 / 4328 | 24 / 110 / 4328 | default | 4328 | – | same key |
| org-chart | 29 / 172 / 19893 | 16 / 174 / 13369 | simplex ranks | 13369 | -32.8 % | **taken:** crossings 29 → 16 |
| petri-net | 32 / 84 / 1389 | 27 / 72 / 1396 | simplex ranks | 1396 | +0.5 % | **taken:** crossings 32 → 27 |
| state-order | 0 / 19 / 399 | 0 / 19 / 399 | default | 399 | – | same key |
| state-tcp | 0 / 26 / 357 | 0 / 26 / 357 | default | 357 | – | same key |
| synthetic-2k | 1789 / 1358 / 93987 | 1753 / 1424 / 86064 | simplex ranks | 86064 | -8.4 % | **taken:** crossings 1789 → 1753 |
| uml-class | 1 / 37 / 2629 | 0 / 39 / 2642 | simplex ranks | 2642 | +0.5 % | **taken:** crossings 1 → 0 |
| workflow-dag | 230 / 300 / 3026 | 230 / 300 / 3026 | default | 3026 | – | same key |

**Sums over the 17 views every arm laid out (uniform task).**

| arm | single pass | delegated | overlaps | hits | crossings | bends | length | area (k) | area / default (median) | area / elk-rs (median) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| default | 17/17 | 16 | 0 | 0 | 4245 | 4239 | 2873044 | 258062 | 1.000 | 1.70 |
| simplexRanksCandidate | 17/17 | 16 | 0 | 0 | 4181 | 4287 | 2741424 | 243786 | 1.000 | 1.60 |

**Views worse than the default on a gate above area**

- simplexRanksCandidate: org-chart (crossings 29→16, bends 172→174); synthetic-2k (crossings 1789→1753, bends 1358→1424); uml-class (crossings 1→0, bends 37→39)

### Configured datasets

| view | default: crossings / bends / area (k) | `simplexRanks`: crossings / bends / area (k) | refinement takes | refined: area (k) | change | why, by published key (overlaps, hits, shared pairs, crossings, near-node, projected crossings, bends, length) |
|---|---|---|---|---:|---:|---|
| activity-order | 92 / 79 / 1935 | 92 / 79 / 1935 | default | 1935 | – | same key |
| arqlet-pipeline | 25 / 78 / 3390 | 32 / 68 / 3338 | default | 3390 | – | crossings 25 → 32 |
| bpmn-pools | 12 / 59 / 1940 | 10 / 51 / 2165 | simplex ranks | 2165 | +11.6 % | **taken:** crossings 12 → 10 |
| can-bus | 0 / 32 / 1207 | 0 / 32 / 1207 | – | – | – | `refine` fails, with and without the option (see below) |
| cloud-architecture | 31 / 108 / 4308 | 30 / 106 / 4561 | simplex ranks | 4561 | +5.9 % | **taken:** crossings 31 → 30 |
| deployment-cluster | 109 / 227 / 4711 | 114 / 219 / 4561 | default | 4711 | – | crossings 109 → 114 |
| er-diagram | 0 / 23 / 1664 | 0 / 23 / 1764 | default | 1664 | – | equal up to bends, longer |
| layerweave-code | 1473 / 1171 / 86419 | 1723 / 1207 / 90198 | default | 86419 | – | crossings 1473 → 1723 |
| mixed-large | 1030 / 150 / 21452 | 1009 / 163 / 18485 | default | 21452 | – | not a single-pass drawing: no candidate |
| network-vlan | 43 / 106 / 3043 | 43 / 106 / 3043 | default | 3043 | – | same key |
| org-chart | 48 / 335 / 17884 | 52 / 324 / 18446 | default | 17884 | – | crossings 48 → 52 |
| petri-net | 32 / 84 / 1389 | 27 / 72 / 1396 | simplex ranks | 1396 | +0.5 % | **taken:** crossings 32 → 27 |
| state-order | 0 / 19 / 427 | 0 / 19 / 427 | default | 427 | – | same key |
| state-tcp | 0 / 26 / 357 | 0 / 26 / 357 | default | 357 | – | same key |
| synthetic-2k | 3037 / 1570 / 62193 | 2975 / 1633 / 60492 | default | 62193 | – | shared route pairs 9 → 13 |
| uml-class | 6 / 49 / 3627 | 9 / 53 / 3727 | default | 3627 | – | crossings 6 → 9 |
| workflow-dag | 553 / 785 / 11827 | 553 / 785 / 11827 | default | 11827 | – | same key |

**Sums over the 16 views every arm laid out (configured).**

| arm | single pass | delegated | overlaps | hits | crossings | bends | length | area (k) | area / default (median) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| default | 15/16 | 32 | 0 | 0 | 6491 | 4869 | 3058108 | 226566 | 1.000 |
| simplexRanksCandidate | 15/16 | 32 | 0 | 0 | 6483 | 4847 | 3047266 | 227052 | 1.000 |

**Views worse than the default on a gate above area**

- simplexRanksCandidate: none

`area / default (median)` is 1.000 because most views keep their drawing.

**`can-bus` as configured:** `refine` answers `Routing completion requires the legacy router and unbundled default
orthogonal routing`, on `origin/main` too. A view with buses that falls back from single pass keeps single pass in its
options (`bus_layout::collapse` restores the requested ones), and `refine` then checks the single-pass capability and
refuses. This change does not touch that; the candidate is not planned where the diagnostic says single pass was not
used. The row is left out of that pose's sums.

**Delegated routes and single pass** are the same in both arms on every view.

## Why the expectation does not hold

Step 0 counted a choice per view by overlaps, line–node hits, crossings, bends and then area, from gate runs. The same
rule on today's drawings (from the layout-only runs, `ranks-*.json`) would take 6 uniform views with area −7.5 % and 5
configured ones with −1.8 % (the ADR's figures: 7 and −9.4 %, 6 and −3.2 %). So part of the gap is the corpus
(`layerweave-code` has grown, and simplex now costs it 250 crossings), the rest is what a refinement candidate is; of the
views that rule would take, the candidate leaves these:

| rule or restriction | view |
|---|---|
| shared route pairs rank above bends in the published key | `synthetic-2k` configured: 9 → 13 |
| the geometry rules refuse an ineligible candidate | `mixed-large` uniform: container-bounds issues in the candidate as in the default (key better: container traversals 1 → 0, crossings 531 → 485) |
| only single-pass drawings | `mixed-large` configured |

The uniform `mixed-large` default drawing reports geometry issues itself (`Child intrudes into header/padding`, in the
container `mesh`), on `origin/main` too: a finding of its own.

## Identity (`simplex-refinement.mjs`, both poses)

| check | uniform | configured |
|---|---|---|
| option off: import and refinement byte for byte as `origin/main` | 17 of 17 | 17 of 17 (`can-bus`: the same error) |
| default ranks kept: snapshot as without the option, but for the option and the candidate count | 12 of 12 | 13 of 13 |
| simplex ranks taken: boxes, routes, ports and labels equal an import with `simplexRanks` | 5 of 5 | 3 of 3 |
| 1 and 4 threads: the same bytes | 17 of 17 | 16 of 16 |

Reference fingerprints (15 views, limit 100, threads 1 and 4, all identical across thread counts):

- option off against the baseline build on the same graphs: **15 identical**;
- option off against the stored `fingerprints-20261124-default-0.9.0.jsonl`: 14 identical, `layerweave-code` "input
  changed" (1362 / 2673 stored, 1474 / 3254 here);
- option on against the baseline: 10 "metadata only" (one more candidate evaluated), 5 "geometry changed", the five views
  that take the simplex ranks at this limit (`bpmn-pools`, `cloud-architecture`, `layerweave-code`, `petri-net`,
  `synthetic-2k`).

Native and WASM: `tools/platform-parity.mjs` has a request that refines a view where the candidate is taken.

## Kept ranks after edits

On the views that took the simplex ranks, every visible container was collapsed and, apart from that, a node added to a
leaf, up to 12 of each, once on the refined drawing and once on the default one; the two results are compared in
objective order.

| pose | edits | keep `simplexRanks` | better / equal / worse than the same edit of the default drawing | area, sum |
|---|---:|---:|---|---:|
| uniform | 105 | 105 | 93 / 3 / 9 | −10.8 % |
| configured | 53 | 53 | 42 / 3 / 8 | +5.2 % |

The kept ranking is usually still the better one, not always (17 of 158 edits are worse), and no later refinement tries
the default ranks again.

**Movement.** The candidate is a layout of the view from the start (what an import with `simplexRanks` draws, and what
these figures measure). A drawing an edit continued can differ from that: with the default ranks, adding a node to each of
up to 8 leaves of nine configured views gave boxes unlike those of a fresh layout of the edited graph in 43 of 72 cases.
A refinement of such a drawing that takes the candidate moves more than the ranks require; the published key has no
movement term. A candidate started from the drawing's places (as `configure` with `simplexRanks` lays out) draws 11 of 13
checked views differently from the measured fresh layout, and was not taken. (Both counts are from a one-off
check over `layout-command` that is not committed.)

## Times

**Noisy.** The machine was never idle: the 1-minute load average before the runs was 9.7 to 41.5 on 8 cores
(other builds and benchmarks). These figures give the order of magnitude of the cost only; the ADR asks for an idle
machine before acceptance, and that measurement is still open.

`node benchmarks/eval/simplex-refinement-timing.mjs --out <file>` (data: `-timing.jsonl`): `bench` (release,
`benchmark-timings`; the build of the merged branch), 9 repetitions, 1 thread, on the views of both poses at limit 150,
the two arms of a view one after the other. The data were recorded with an earlier form of the script (the same `bench`
calls without `--import-options`, which is empty for these views). Medians in ms; `import` is the first layout in the arm
without the option. The configured `mixed-large` has no candidate and ran once.

| pose | view | nodes | import | refine, option off | refine, option on | taken |
|---|---|---:|---:|---:|---:|---|
| uniform | activity-order | 31 | 5.7 | 0.7 | 8.1 | no |
| uniform | arqlet-pipeline | 32 | 6.1 | 0.6 | 6.1 | no |
| uniform | bpmn-pools | 26 | 2.5 | 0.2 | 2.7 | no |
| uniform | can-bus | 21 | 1.5 | 0.1 | 1.9 | no |
| uniform | cloud-architecture | 34 | 4.8 | 0.8 | 5.6 | no |
| uniform | deployment-cluster | 33 | 8.1 | 0.6 | 10.0 | yes |
| uniform | er-diagram | 44 | 1.2 | 0.1 | 1.6 | no |
| uniform | layerweave-code | 188 | 70.2 | 10.1 | 139.5 | no |
| uniform | mixed-large | 96 | 94.4 | 4.0 | 83.9 | no |
| uniform | network-vlan | 37 | 5.9 | 0.6 | 7.0 | no |
| uniform | org-chart | 104 | 17.2 | 2.4 | 23.0 | yes |
| uniform | petri-net | 24 | 4.9 | 0.5 | 3.6 | yes |
| uniform | state-order | 11 | 1.3 | 0.1 | 0.8 | no |
| uniform | state-tcp | 11 | 0.9 | 0.1 | 0.9 | no |
| uniform | synthetic-2k | 231 | 157.0 | 15.6 | 196.8 | yes |
| uniform | uml-class | 43 | 5.3 | 0.6 | 5.6 | yes |
| uniform | workflow-dag | 72 | 20.2 | 2.2 | 25.3 | no |
| configured | activity-order | 31 | 3.9 | 0.4 | 4.8 | no |
| configured | arqlet-pipeline | 32 | 3.8 | 0.3 | 4.3 | no |
| configured | bpmn-pools | 26 | 2.6 | 0.2 | 3.5 | yes |
| configured | can-bus | 21 | – | fails | fails | – |
| configured | cloud-architecture | 34 | 6.2 | 0.5 | 7.5 | yes |
| configured | deployment-cluster | 33 | 9.7 | 0.7 | 15.0 | no |
| configured | er-diagram | 44 | 1.8 | 0.3 | 2.0 | no |
| configured | layerweave-code | 188 | 105.7 | 18.5 | 103.8 | no |
| configured | mixed-large | 96 | 1057.7 | 7191.4 | 6493.0 | no |
| configured | network-vlan | 37 | 3.7 | 0.5 | 4.9 | no |
| configured | org-chart | 104 | 12.0 | 1.8 | 17.6 | no |
| configured | petri-net | 24 | 3.6 | 0.3 | 4.1 | yes |
| configured | state-order | 11 | 0.6 | 0.1 | 0.9 | no |
| configured | state-tcp | 11 | 1.2 | 0.1 | 1.1 | no |
| configured | synthetic-2k | 231 | 139.3 | 18.6 | 196.6 | no |
| configured | uml-class | 43 | 3.0 | 0.2 | 4.4 | no |
| configured | workflow-dag | 72 | 15.9 | 2.3 | 21.5 | no |

- uniform, sums over the 17 single-pass views: refinement 39 ms without the option, 522 ms with it; the imports 407 ms. The added refinement time per view is 1.1 times its import at the median (0.6 to 1.8).
- configured, sums over the 15 single-pass views: refinement 45 ms without the option, 392 ms with it; the imports 313 ms. The added refinement time per view is 1.2 times its import at the median (0.8 to 1.5).

Without a candidate the refinement of a single-pass view measures nothing; with it the input's drawing is measured, the
view is laid out again and that drawing is measured, so the added time is more than the second layout. The candidate
looks at the stop signal before it starts, not inside its layout: a time budget cannot cut it short.

## Step 2, the cheap upper bound: ELK's width-bounded layerers

Step 1 is complete and this needs no engine change (`elk-layerers.mjs`; the pinned `@openedges/elk-layout` 0.12.1,
default parameters, the layering option on the root and on every container; ELK against ELK, scored by the shared
`metrics` operation).

| uniform task (`INCLUDE_CHILDREN`) | views | crossings | bends | area, sum | area / ELK default (median) | views smaller | area, the smaller of both per view |
|---|---:|---|---|---:|---:|---:|---:|
| `MIN_WIDTH` | 17 | 5695 → 5543 | 4456 → 4724 | +60.6 % | 1.196 | 2 | −0.2 % |
| `STRETCH_WIDTH` | 10 (7 do not finish within 90 s) | 4924 → 4987 | 3532 → 3744 | +39.8 % | 1.543 | 0 | 0.0 % |

As configured (the containers' own directions, so `SEPARATE_CHILDREN` where they differ) elk-rs refuses 13 of the 17
views (`UnsupportedGraphException`, an edge across the layout scope); on the other 4 both layerers are larger on every
view (sum +142 % and +57 %).

**Reading:** ELK's width-bounded layerers buy almost no area on these views. The bound is not tight for step 2: under
`INCLUDE_CHILDREN` ELK layers the whole hierarchy at once and counts leaves, whereas step 2 would spread sibling areas
inside one area and count an area by its box; the layerers' width parameters were not tuned. By the ADR's rule ("if they
buy little area there, step 2 is not built") it speaks against step 2.
