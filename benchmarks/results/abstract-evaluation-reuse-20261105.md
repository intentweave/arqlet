# Avoiding duplicate abstract-candidate evaluation, 2026-11-05

## Result

The public abstract-routing path completed the drawing, built a renderer-neutral
scene, ran `published_evaluation` inside the layout call, and decorated labels.
Runtime candidate selection then measured the same drawing again, while the
outer layout call decorated those labels again. The scene was not used by
candidate selection. Production completion now returns the finished snapshot
and the strict track diagnostics it needs without building the extra scene,
computing a throwaway key/metrics result, or repeating label decoration.

The standalone completion/diagnostic API still builds the scene and computes
the full metrics and key. Runtime scoring and all geometry, route, endpoint,
and strict-track checks remain in place. No candidate, stopping rule, quality
key, or default option changed.

Five-sample medians, summed across the three frozen views:

| Measurement | Duplicate evaluation retained | Production reuses runtime evaluation | Change |
|---|---:|---:|---:|
| Public abstract import + refinement | 1,405.630 ms | 1,338.228 ms | -4.80% |
| Public abstract refinement | 1,189.301 ms | 1,135.800 ms | -4.50% |
| Native abstract core refinement | 1,162.464 ms | 1,096.408 ms | -5.68% |

These are sums of per-view medians, not one application's latency. All three
views improved in public refinement (codegraph 1,000.212 -> 953.759 ms;
workflow 106.785 -> 103.296 ms; org-chart 82.304 -> 78.744 ms), reductions of
3.27-4.64%. Public import also improved. Initial/refined output fingerprints,
published keys, quality metrics, and deterministic candidate/work identities
remained exact.

This is a measured gain for the opt-in abstract candidate path, not a 4% general
Layerweave speedup and not evidence of parity with elk-rs. The abstract path
still has a quality gap on the codegraph and is not the default.

## Method and provenance

- `node benchmarks/eval/abstract-pipeline.mjs --repeat 5` on identical frozen
  selected views: codegraph (188 nodes/358 edges), workflow-dag (72/125), and
  org-chart (104/82). The harness performs a warmup, rotates/reverses arms,
  checks repeated output/work identity, and records complete quality and
  source/input/binary provenance.
- Baseline run retains scene construction, label decoration and
  `published_evaluation` inside the production completion call. Optimized run
  skips the unused scene and throwaway evaluation and does not decorate labels
  a second time; the runtime's normal candidate evaluation still runs. The
  only Rust source files with changed hashes between runs are
  `crates/layout/src/constructive/abstract_realization/completion.rs` and
  `crates/layout/src/lib.rs`.
- Both builds use release/thin-LTO with `benchmark-timings`; Apple M3, eight
  logical CPUs, macOS/arm64; Node 24.21.0 and Rust 1.99.0. Same HEAD,
  `d14db4513e7364102032306d560785d3927aef21`, dirty worktree. Start load
  averages were 22.03/9.23/6.10 and 22.59/12.76/8.08. This is a paired
  diagnostic, not an idle-machine sweep.
- Raw per-sample timings, stage work, output fingerprints, quality, and hashes:
  [duplicate-evaluation baseline](abstract-evaluation-baseline-20261105.jsonl)
  and [evaluation reuse](abstract-evaluation-reuse-final-20261105.jsonl).
