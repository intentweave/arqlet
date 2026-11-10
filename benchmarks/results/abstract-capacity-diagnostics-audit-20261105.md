# Placement-seed coverage audit, 2026-11-05

## Result

The earlier per-channel diagnostic left 21 physical channels without placement
records. The gap was in telemetry: a reused region returned before the
placement-time seed capture ran. Region-cache entries now carry the seeds
captured with their coordinates, and a cache hit reuses those records without
recomputing them.

On the three frozen views, every physical channel now has a matching seed:

| View | Matched / physical | Pre-completion space >= physical demand | Late-shift channels | Final physical missing |
|---|---:|---:|---:|---:|
| Codegraph | 95 / 95 | 94 | 1 | 0 |
| Workflow | 21 / 21 | 21 | 0 | 0 |
| Org-chart | 68 / 68 | 68 | 0 | 0 |

The one late shift is codegraph `crate:layout`, rank 3: 390 units of
post-placement availability for 450 units of physical demand, followed by a
60-unit shift. There is no final physical deficit. The newly mapped channels
show that the earlier apparent coverage gap was not a geometric failure.

The geometric fingerprints for default, abstract, and common-incumbent
initial/refined outputs exactly match the previous diagnostic run on all three
views. All 163 seed records that were already matched also retain identical
track estimates, seeded widths, and initial availability. Candidate portfolio
and acceptance rules were unchanged.

## Method and limitations

- One frozen-harness sample on the same codegraph (188 nodes/358 edges),
  workflow-dag (72/125), and org-chart (104/82) inputs. The harness also ran a
  warmup and checked repeated output/work identity.
- Load averages at start were 5.65/11.62/11.54 on the shared machine. This is
  a coverage and output-identity audit, not a timing comparison.
- Release/thin-LTO build with `benchmark-timings`; Apple M3, eight logical
  CPUs, macOS/arm64; Node 24.21.0, Rust 1.99.0. HEAD
  `d14db4513e7364102032306d560785d3927aef21`, dirty worktree.
- Raw diagnostics, fingerprints, timings, hashes and provenance are in
  [abstract-capacity-diagnostics-audit-20261105.jsonl](abstract-capacity-diagnostics-audit-20261105.jsonl).

The audit closes the seed-to-physical-channel mapping gap for these frozen
views. It does not show that ordinal capacity seeding improves quality or
speed, and it does not justify reducing capacity. Any such change still needs
physical feasibility and output-quality checks.
