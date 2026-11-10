# Integrated abstract-routing pipeline, 2026-11-05

> **Superseded figures (2026-11-06).** The crossing counts below describe the build of 2026-11-05. With
> the later physical channel order, the same frozen code view draws 4071 crossings with `abstractRouting`
> (not 6430), and the opt-in `singlePass` mode draws 3373 crossings and 1586 bends (elk-rs 3057 / 1638).
> See [single-pass-blocks-20261106.md](single-pass-blocks-20261106.md) and
> [single-pass-order-20261106.md](single-pass-order-20261106.md). The text below is kept as recorded.

## Result

The abstract router now runs in the public engine, opt-in with
`{"abstractRouting":true}`. It executes directly after placement, before the
legacy constructor/probe loop, in imports and ordering candidates. It is not a
detached drawing timed in place of refinement. Native and WASM use the same path.

On the frozen codegraph the observed public import-plus-refinement cost falls
from **1,510.525 to 1,192.714 ms**, but the refined total crossing count rises
from **4,113 to 6,430**. Workflow and organigram are slightly more expensive.
This is **not a demonstrated speedup at equivalent quality** and does not
justify a default switch. It does establish that the construction search can
be removed from these complete public candidates, not just from diagnostics.

Raw results: [abstract-public-pipeline-20261105.jsonl](abstract-public-pipeline-20261105.jsonl).

## What is connected

- Placement -> plan-keyed ports -> abstract demand -> flow growth -> chain
  construction with shared coordination projections -> legacy fallback only
  for delegated edges -> strict track repair -> complete snapshot/scene
  diagnostics. There is no prior full legacy route pass.
- The handoff includes grown local-coordinate boxes **and grown stored plans**,
  resolved absolute ports, logical routes, boundaries and labels. Supported
  subsequent edits use the same path.
- Four ordering strategies and the routing-only job are still evaluated.
  Global rounds keep their existing stopping rule. Legacy routing-only jobs,
  rounds and Optimize crossing relief retain their algorithms and acceptance
  rules; they have not been replaced by another abstract construction.
- Abstract ordering candidates with unresolved strict track conflicts are
  ineligible. Other geometry/clearance/key checks and the incumbent remain.
  The opt-in changes candidate drawings, not default serialization or outputs.
- Unsupported capabilities fail explicitly: currently only unflattened,
  uniformly positive-flow layered hierarchies (`lr`/`tb`), default orthogonal
  routing, no bundling, pins, fixed container sizes, extent bounds, placement
  grids/search or view budgets. Organic/tree/radial and reversed/mixed flows
  are not implemented by this path.
- Construction/coordination reuse is local to each drawing; this does not add
  a persistent or cross-candidate geometry cache.

See [configuration](../../docs/configuration.md#provisional-abstract-routing-execution).

## Method and provenance

- Three identical frozen selected views from the preceding focused comparison:
  codegraph 188 nodes/358 edges, workflow 72/125, organigram 104/82.
- Same release/thin-LTO binary, built with `benchmark-timings`; options off/on,
  not two differently built engines. HEAD
  `d14db4513e7364102032306d560785d3927aef21`, dirty feature worktree.
- Apple M3, eight logical CPUs, macOS/arm64; Node 24.21.0, Rust 1.99.0.
  The result records 135 Rust/manifest/lockfile hashes, binary/generator/input
  hashes and embedded inputs. These hashes were checked after measurement.
- Five samples after warmup, rotated/reversed six-arm order: native and public
  calls for default, abstract import/refinement, and common-incumbent refinement.
  All complete output/work identities repeat. Default geometry and final keys
  match the preceding frozen measurements.
- Native clocks separately measure import, core refinement and Optimize
  (including its additional crossing relief). Public clocks include JSON,
  process startup and response parsing, through `layout-command`.
- `--pipeline-only 1` omits ancillary interaction probes, not candidates.
  A native outer sample contains its own warmup and timed invocation.
- The machine was shared and busy: load averages at start
  12.89/12.90/11.68, at end 5.23/9.79/10.60. No builds ran alongside the final
  comparison. An earlier attempt timed out during a host-load spike to 61.58;
  another was stopped for a final acceptance-rule correction. Neither is pooled
  into this result. This is a bounded diagnostic, not an idle-machine sweep.
- Full normalized quality is recorded separately from native published keys.
  The common-incumbent arm starts from exactly the default boxes/routes/plans,
  changing only the execution option. Its import time is default setup, not an
  abstract import measurement.

## Complete public and native costs

Milliseconds, median of five samples:

| View | Native import default / abstract | Native core refinement, own input default / abstract | Native abstract refinement, common incumbent | Public import + refinement default / abstract |
| --- | ---: | ---: | ---: | ---: |
| Codegraph | 209.685 / 163.208 | 1270.312 / 989.674 | 913.780 | 1510.525 / 1192.714 |
| Workflow | 20.889 / 21.677 | 82.500 / 94.515 | 94.721 | 117.093 / 129.510 |
| Organigram | 15.416 / 17.143 | 70.937 / 72.777 | 72.798 | 97.992 / 101.285 |

Public refinement alone (default / abstract own input / abstract common input):

| View | Public core refinement |
| --- | ---: |
| Codegraph | 1290.061 / 1017.322 / 937.949 |
| Workflow | 90.873 / 102.864 / 103.467 |
| Organigram | 78.082 / 79.277 / 79.593 |

Native Optimize is a separate operation: codegraph
1301.624 / 1019.940 / 949.015 ms; workflow 85.273 / 96.904 / 97.203 ms;
organigram 72.190 / 73.972 / 73.942 ms. It is not the public core-refinement
clock. Component medians need not sum to the separately timed combined median.

## Work actually removed, and work still present

Every arm starts/completes the same five first-stage jobs. Abstract arms execute
four complete abstract ordering passes; abstract imports execute one more.

| View | Constructor searches default / abstract | Constructor transitions default / abstract | Placement passes in refinement default / abstract | Global rounds default / abstract |
| --- | ---: | ---: | ---: | ---: |
| Codegraph | 435 / 0 | 55,055,505 / 0 | 26 / 12 | 2 / 1 |
| Workflow | 11 / 0 | 1,345 / 0 | 12 / 12 | 1 / 1 |
| Organigram | 20 / 0 | 408,512 / 0 | 10 / 10 | 1 / 1 |

The codegraph's second default round disappears under the **existing**
no-improvement stopping rule on different geometry. No round budget was
reduced. Thus its timing difference cannot all be credited to search removal.
Fallback, coordination, track repair and scoring still run.

Codegraph median cumulative Optimize-work timers:

| Stage | Default | Abstract, own input | Abstract, common input |
| --- | ---: | ---: | ---: |
| Placement | 333.604 | 253.941 | 253.483 |
| Routing | 800.667 | 573.445 | 484.977 |
| Delegated fallback (inside routing) | 98.609 | 97.356 | 84.388 |
| Runtime scoring | 108.519 | 89.119 | 90.539 |

These are nested cumulative work timers, not additive slices of the public
clock. The integrated completion currently also builds/measures its diagnostic
drawing within routing; runtime candidate scoring then still runs its normal
gate. Sharing that evaluation and avoiding diagnostic-only scene work is an
open integration optimization, particularly for smaller views. It does not
explain away the quality regressions below.

Initial construction/delegation: codegraph 345/13, workflow 125/0, organigram
81/1. The organigram still reports two unresolved strict track pairs; it is
not silently certified valid by being imported.

## Quality is the current blocker

All tested outputs have complete logical edge partitions, finite orthogonal
routes, matching resolved endpoints, zero line-node hits and zero geometry
issues. Those checks do **not** make their visual quality equivalent.

After public refinement, default / abstract own input:

| View | Proper crossings | Total crossings including shared endpoints | Shared-line pairs | Bends |
| --- | ---: | ---: | ---: | ---: |
| Codegraph | 3126 / 4144 | 4113 / 6430 | 0 / 0 | 1844 / 2100 |
| Workflow | 222 / 280 | 256 / 400 | 0 / 0 | 618 / 464 |
| Organigram | 20 / 18 | 35 / 74 | 2 / 2 | 208 / 188 |

The abstract-own-input refinement keeps its initial drawing in all three views.
It still pays to evaluate the four orderings and routing-only job.

From the common default incumbent, workflow and organigram keep the input.
Codegraph selects the abstract alternating-barycenter drawing: shared-line pairs
fall from 10 to 0, but total crossings rise from 4113 to 7334. This improves the
input's shared key because shared lines precede crossings. It nevertheless loses
to the **default refined** result, which has zero shared-line pairs and 4113
total crossings. Retaining the incumbent is not a guarantee of matching the
winner of the old candidate drawings.

Consequently this is an observed cheaper codegraph execution with a quality
trade-off, not an output-equivalent or quality-equivalent optimization.

## Validation and next step

Passed: formatting, workspace check, workspace tests under the `fast` profile,
generated TypeScript contract drift check, and the complete playground build
and test script. Seven new integration tests cover both axes, graph families,
explicit ports/labels, cycles/loops/parallel edges, snapshot continuation,
thread identity, stale requests, cancellation, capability failures, and
unchanged acceptance for legacy-only routing jobs. WASM smoke tests directly
compare abstract import/refinement/edit with native dispatch. Existing default
frozen output checks still match.

Next work should improve abstract channel/port ordering and the complete
candidate's crossing key on these frozen views, retaining the current result as
a control. Remove repeated diagnostic evaluation only as a separate measured
integration optimization. Do not enable this path by default, delete old
strategies, or describe the smaller codegraph clock as equal-quality speedup.
No commit or push was made.

Reproduce on an otherwise idle machine:

```sh
cargo build --release --locked -p layerweave-runtime --bins --features benchmark-timings
node benchmarks/eval/abstract-pipeline.mjs --repeat 5 \
  --out benchmarks/results/abstract-public-pipeline-new.jsonl
```
