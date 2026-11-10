# Bounded ordinal capacity trials, 2026-11-05

## Result

Two exploratory variants were compared with the current full ordinal preallocation
on the three frozen views. The candidate portfolio, selection rules, and physical
completion checks were unchanged:

- **No preallocation:** place with the existing base rank gap; let physical
  completion grow a channel only if the realized geometry requires it.
- **Half excess:** add half of the estimated width above the base rank gap.
- **Full estimate:** the current opt-in behavior, using the full ordinal
  estimate above the base gap.

The no-preallocation variant produced the smallest drawings on all three
views. Its route-crossing metric was lower on codegraph and unchanged on the
other two; bends were unchanged or slightly lower. The half-excess variant was
between the other two in size, but did not reduce crossings relative to no
preallocation.

| View | Variant | Area | Width × height | Crossings | Bends | Channels with late shift | Total shift | Final physical missing |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Codegraph | No preallocation | 88.61M | 11,682 × 7,585 | 4,144 | 2,100 | 6 | 186 | 0 |
| Codegraph | Half excess | 91.79M | 12,101 × 7,585 | 4,144 | 2,100 | 5 | 174 | 0 |
| Codegraph | Full estimate | 99.20M | 13,078 × 7,585 | 4,248 | 2,110 | 1 | 60 | 0 |
| Workflow | No preallocation | 2.36M | 2,742 × 860 | 280 | 464 | 0 | 0 | 0 |
| Workflow | Half excess | 2.66M | 3,095 × 860 | 280 | 464 | 0 | 0 | 0 |
| Workflow | Full estimate | 3.06M | 3,558 × 860 | 280 | 464 | 0 | 0 | 0 |
| Org-chart | No preallocation | 32.51M | 8,951 × 3,632 | 18 | 188 | 0 | 0 | 0 |
| Org-chart | Half excess | 32.98M | 8,951 × 3,684 | 18 | 190 | 0 | 0 | 0 |
| Org-chart | Full estimate | 33.46M | 8,951 × 3,738 | 18 | 190 | 0 | 0 | 0 |

All variants had zero geometry and routing issues in these outputs. The
no-preallocation codegraph grew later across more channels than the full
estimate (six instead of one), but every channel completed without physical
missing width. This is the expected tradeoff: less initial space versus more
late growth. The half-excess setting does not establish a principled compromise;
it is only a sensitivity point.

Public pipeline times from the single samples varied substantially and were
collected under different high machine loads. They do not establish a speedup
or slowdown and are omitted from the quality comparison.

## Decision

Keep the current full-estimate behavior unchanged for now. The no-preallocation
trial is promising for compactness and did not lose measured crossing quality
on these inputs, but its additional late growth could cost time elsewhere. The
single samples cannot resolve that tradeoff. Do not select the half-excess
factor by intuition.

The next measurement should compare full and no preallocation with repeated,
rotated samples on an idle machine, while checking identical candidate work,
published keys, final quality, and physical missing width. Keep the existing
opt-in/default boundary until that measurement supports a behavior change.

## Method and provenance

- One timed sample per arm after a warmup, with repeated output/work identity
  checks, using `benchmarks/eval/abstract-pipeline.mjs`. Frozen inputs:
  codegraph (188 nodes/358 edges), workflow-dag (72/125), and org-chart
  (104/82).
- Full-estimate control: [placement-seed coverage audit](abstract-capacity-diagnostics-audit-20261105.md).
- No-preallocation trial: `abstract_capacity = 0` at rank placement.
- Half-excess trial: `base_gap + 0.5 * max(estimated_capacity - base_gap, 0)`.
- Release/thin-LTO build with `benchmark-timings`; Apple M3, eight logical
  CPUs, macOS/arm64; Node 24.21.0, Rust 1.99.0. All runs used the same HEAD,
  `d14db4513e7364102032306d560785d3927aef21`, with a dirty worktree. Start
  load averages were 69.11/25.23/15.53 for no-preallocation and
  45.46/26.21/17.09 for half-excess. The control's separate audit started at
  5.65/11.62/11.54.
- Raw timings, quality metrics, full output fingerprints, source/input/binary
  hashes and provenance are in
  [no-preallocation results](abstract-capacity-no-seed-trial-20261105.jsonl)
  and [half-excess results](abstract-capacity-half-seed-trial-20261105.jsonl).
