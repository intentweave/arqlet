# Layerweave and elk-rs: focused in-process comparison, 2026-11-03

Machine: Apple M3, 8 logical CPUs, macOS; Rust 1.99.0, Node 24.21.0; release build. Both engines used the same nine
uniform-direction views, at the same 150-node selection limit. The bounded views contain 182 to 24 nodes because the limit
selects leaves and includes their containers. Per-container directions and strategies were cleared, and both Layerweave
options `layeredConstruction` and `hierarchicalOrdering` were enabled. Each Layerweave measurement is the median of five
runs at one thread; elk-rs 0.12.1 (pinned commit `eb586b6`) is the median of five in-process `layout()` calls. The
Layerweave source was `dac8190` plus the uncommitted changes in the worktree.

The in-process Layerweave measurements use `bench` with `benchmark-timings`. `importMs` includes view import and the first
layout, but excludes reading/deserializing the source JSON. `refineCoreMs` is the same core refinement used by the public
`refine` operation: it is additional work after import, and excludes the Optimize-only `relieve_crossings` step.
`refineMs` is the Optimize job and includes that extra step. The public comparison uses `layout-command`, so its
`importMs` and `refineMs` also include a process start and JSON transfer for each call. The `elk-rs` clock surrounds only
`layout()`; graph construction and conversion are outside it. Thus the in-process rows reduce the timing asymmetry, but
the operations still do not have identical boundaries.

The machine was shared. During the in-process run the 1-minute load average was 3.75 at start and 3.50 at end; before
the run a CPU sample showed 72% idle. During the public run it rose from 3.07 to 7.07. Treat small differences as
indicative, not as a controlled speedup claim. Raw rows:
[`inprocess-uniform-elkrs-20261103T2307.jsonl`](./inprocess-uniform-elkrs-20261103T2307.jsonl) and
[`elkrs-public-uniform-20261103T2307.jsonl`](./elkrs-public-uniform-20261103T2307.jsonl).

## Time by view

Times are milliseconds. `Optimize` is `refineMs` and contains `relieve`; `Core` is the public refinement operation
measured in-process. Public import/refine times are each medians of their own process calls.

| view (nodes/edges) | LW import, in-process | LW core refinement, in-process | LW Optimize job | `relieve` stage | LW public import | LW public refine | elk-rs `layout()` |
|---|---:|---:|---:|---:|---:|---:|---:|
| layerweave-code (182/303) | 117.81 | 220.84 | 239.67 | 19.57 | 129.24 | 237.53 | 34.52 |
| deployment-cluster (33/56) | 9.66 | 18.95 | 20.24 | 1.12 | 13.19 | 23.70 | 6.16 |
| workflow-dag (72/125) | 22.57 | 30.08 | 32.70 | 2.68 | 26.87 | 36.99 | 10.39 |
| org-chart (104/82) | 15.76 | 30.04 | 31.32 | 1.41 | 19.52 | 35.45 | 9.03 |
| cloud-architecture (34/32) | 5.79 | 9.78 | 10.31 | 0.54 | 8.92 | 13.81 | 4.89 |
| network-vlan (37/47) | 4.20 | 7.43 | 8.16 | 0.64 | 7.40 | 11.32 | 3.24 |
| petri-net (24/41) | 2.30 | 3.52 | 3.92 | 0.40 | 5.22 | 6.98 | 1.84 |
| uml-class (43/24) | 3.45 | 5.37 | 35.73 | 29.36 | 6.53 | 9.42 | 2.98 |
| layerweave-pipeline (32/33) | 4.20 | 5.96 | 6.41 | 0.33 | 7.43 | 9.63 | 3.12 |
| **sum of per-view medians** | **185.74** | **331.95** | **388.47** | **56.06** | **224.32** | **384.83** | **76.17** |

The sum of public full-pipeline medians (`layoutMs` for the refined Layerweave result) is 609.14 ms. This differs from
adding the separate import and refine medians because medians of sums are not generally the sum of medians. The
in-process import-plus-core-refinement sum is 517.69 ms. Both totals include two Layerweave operations, while elk-rs
performs one layout.

## Quality on the same nine views

All rows use the same scene-metric implementation. Values below are sums over the nine outputs.

| output | crossings | edge–node hits | shared route pairs | bends | edge length | area |
|---|---:|---:|---:|---:|---:|---:|
| Layerweave hierarchical, initial | 2009 | 0 | 3 | 2878 | 1.147 M | 156.65 M |
| Layerweave hierarchical, refined | 2002 | 0 | 2 | 2866 | 1.152 M | 156.23 M |
| elk-rs layered | 2031 | 3 | 2 | 2254 | 1.153 M | 62.81 M |

Layerweave and elk-rs are close in crossings and route length on this set. Layerweave has no measured edge–node hits and
slightly fewer crossings; elk-rs uses about 21% fewer bends and about 60% less bounding area. The area metric is not a
quality score by itself.

## What this changes in the optimization order

- The initial in-process Layerweave import sums to 185.74 ms versus 76.17 ms for elk-rs `layout()`. Public
  `layout-command` imports sum to 224.32 ms, about 39 ms more than the in-process import measurement. This is material
  for small views, but does not explain the whole algorithmic gap.
- Core refinement sums to 331.95 ms. On `layerweave-code`, the ordering candidate's placement and routing stages are
  about 65 ms and 76 ms respectively. The legacy fallback search accounts for 27,021 grid pops; one delegated edge
  (`e734`) accounts for 24,934. That is a more concentrated target than a broad array rewrite: first investigate why
  this edge falls back and reduce the work without changing the route.
- `relieve_crossings` is not part of the public `refine` operation. It is an Optimize-job stage: 29.36 ms on `uml-class`,
  but only 19.57 ms on the larger `layerweave-code` view. Improving it helps the Optimize job, not the public
  in-process core refinement.
- The hierarchical sweep is part of placement, but this run does not isolate its cost. Its optimization remains
  profile-gated; this comparison does not justify changing its configured search.
- The shared index representation is still a possible broad follow-up, after another profile verifies which maps and
  string conversions dominate the core refinement.

The run used five repetitions on a shared machine and did not prove a performance result. For a speedup claim, rerun the
same pair of commands on an idle host and preserve the machine and revision metadata.
