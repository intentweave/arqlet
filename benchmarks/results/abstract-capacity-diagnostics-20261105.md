# Per-channel abstract capacity diagnostics, 2026-11-05

## Result

Abstract-routing snapshots now report, per physical realization channel, the
ordinal track estimate and its seeded width, the available gap after placement
but before abstract completion growth, physical width demand, final available
width, actual later gap shift, remaining shortfall/surplus, and physical missing
width. Channels with no matching placement-seed record are marked explicitly
and excluded from matched aggregates.

On the three frozen views, 184 physical channels were reported; 163 have a
matching placement record and 21 do not. Matched counts are 89/95 for
codegraph, 21/21 for workflow, and 53/68 for org-chart. (The first number is
matched; the second is physical channels reported.) Unmatched rows remain in
the raw output, not silently discarded.

Only one matched channel needed additional space after placement: codegraph,
`crate:layout` rank 3, with 61 estimated tracks. Its pre-completion gap was
390, physical demand was 450, and the later shift was 60; final missing width
was zero. Workflow and org-chart had no matched channel with a later shift.
This confirms the growth safety net still matters for at least one channel,
while also exposing many channels whose initial available space exceeds
measured physical demand.

## Matched channel aggregates

| View | Matched / physical | Pre-growth space >= physical demand | Channels needing late shift | Sum of positive pre-growth surplus | Actual late shift sum | Final physical missing |
|---|---:|---:|---:|---:|---:|---:|
| Codegraph | 89 / 95 | 88 | 1 | 1,228 | 60 | 0 |
| Workflow | 21 / 21 | 21 | 0 | 490 | 0 | 0 |
| Org-chart | 53 / 68 | 53 | 0 | 286 | 0 | 0 |

For codegraph, 68 channels had a positive surplus before completion growth,
20 were within 0.01 units of physical demand, and one was short by 60 units.
Workflow had 19 positive-surplus channels and two within tolerance. Org-chart
had 25 positive-surplus channels and 28 within tolerance. The positive-surplus
sums are channel-wise space above physical demand; they are not added diagram
area and do not account for interactions between adjacent ranks.

The estimate's sum over matched channels is 6,564 / 1,614 / 1,344 units for
codegraph / workflow / org-chart, versus 6,510 / 1,398 / 1,998 units of
physical demand. These totals alone hide the key fact: org-chart's estimate is
below demand in aggregate while every matched physical channel already has
enough available space, because the base layout gap often exceeds the estimate.
The local per-channel records are therefore more informative than a global
capacity sum.

## Interpretation

The new metric does not show a broad reduction in post-placement widening:
the only measured late shift is the one 60-unit codegraph channel. It also
shows substantial pre-existing slack, especially on the codegraph/workflow
matched sets. This is evidence to refine the model, not to increase its
capacity or claim faster routing.

There are still unmatched physical channels: 6 in codegraph and 15 in
org-chart. They are marked `seed=unavailable` in the snapshot diagnostics and
excluded from matched aggregates. Before using these counts as complete
coverage, the mapping between placement rank transitions and physical
realization channels needs a separate audit.

## Method and provenance

- Five-sample same-harness run on the frozen codegraph (188 nodes/358 edges),
  workflow-dag (72/125), and org-chart (104/82); rotated/reversed arm order,
  default/abstract/common-incumbent arms. The existing harness verified output
  and work identity between repeated samples.
- Release/thin-LTO build with `benchmark-timings`; Apple M3, eight logical
  CPUs, macOS/arm64; Node 24.21.0, Rust 1.99.0. Shared-machine load averages
  were 16.73/9.34/8.96 at start and 4.49/7.12/8.12 at end. The recorded
  timings are diagnostic and are not compared as a controlled speedup.
- HEAD `d14db4513e7364102032306d560785d3927aef21`, dirty worktree. Raw
  per-sample timing, full outputs, channel diagnostics, input/source/binary
  hashes and replay provenance are in
  [abstract-capacity-diagnostics-20261105.jsonl](abstract-capacity-diagnostics-20261105.jsonl).

The next step is to audit the unmatched channels and, separately, measure
whether a targeted reduction of clear per-channel over-allocation preserves
physical feasibility and quality. Keep the candidate set and acceptance rules
unchanged.
