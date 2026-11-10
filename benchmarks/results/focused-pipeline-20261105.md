# Focused public-pipeline and ELK comparison, 2026-11-05

## Conclusion

The public pipeline has not acquired the large gains observed in the detached
abstract-routing experiments. Against clean `d14db45`, current code keeps every
tested output and deterministic candidate/search aggregate unchanged, but
improves the measured codegraph core refinement only from 1,292.489 to
1,268.643 ms. Initial layout is effectively unchanged. Shared metric evaluation
is active; detached construction/coordination reuse is not.

The next step should be a bounded production-candidate integration experiment,
with the incumbent kept as a control and complete quality/eligibility checks,
not more small metric lookups. First extend detached quality checks beyond the
codegraph. Only then add the abstract-plan path as an opt-in complete candidate
on the same imported geometry; measure its actual work and accepted quality
before making any default or portfolio decision. Do not equate the detached
single-drawing cost with full refinement, replace all current candidates, or
accept the five known ineligible regional alternatives.

## Inputs, builds and timing boundaries

Raw public/core data:
[focused-pipeline-20261105.jsonl](focused-pipeline-20261105.jsonl).
Separate detached data:
[detached-coordination-release-20261105.jsonl](detached-coordination-release-20261105.jsonl).

- Three frozen uniform-direction layered inputs: codegraph (188 nodes,
  358 edges), workflow (72/125), organigram (104/82). The raw result embeds
  original inputs and the exact selected views with SHA-256 hashes.
- Before: clean archive of `d14db4513e7364102032306d560785d3927aef21`.
  Rust/manifests/lockfile are checked against that git tree before measurement.
  Current: dirty feature branch at the same HEAD, with recorded source and
  binary hashes. This comparison covers uncommitted changes after that commit,
  not all historical optimization work.
- Both native builds use `--release --locked` and `benchmark-timings`,
  including thin LTO. No build runs concurrently with the measurements.
- Apple M3, eight logical CPUs, macOS/arm64; Node 24.21.0, Rust 1.99.0.
  Five samples after one warmup, rotated/reversed eight-arm ordering.
  Start load averages 3.48/4.30/8.34; end 4.08/4.06/7.56. The machine is
  shared: these are paired diagnostic observations, not an idle-machine sweep.
- Core layout: `compute_counted` on the preselected graph, excluding
  JSON/process startup and view selection.
- Native import: `import_graph_counted`, including input clone, view selection
  and complete initial layout, excluding JSON/process startup.
- Native core refinement: the public core refinement operation after import.
  Optimize is measured separately; it additionally runs crossing relief.
- Public import/refine: two separate `layout-command` processes with complete
  JSON requests/responses. The combined pipeline has its own wall timer around
  both calls. View setup and quality scoring are outside that timer.
- ELK: `layout()` on the same selected graph, with layered orthogonal routing,
  hierarchy included and 180 x 54 leaf sizes. Preparation/normalization are
  excluded from `layout()` and separately included in adapter totals.
- Native `bench` also executes ancillary edit/interaction probes, outside the
  reported timers. They can affect subsequent load. One native warmup and
  one timed result are obtained per outer sample.

The baseline/current comparison checks initial/core/final fingerprints,
published keys, candidate identity and deterministic work aggregates on every
sample. The newer `plannedFailures`, `searchDetails` and
`searchDetailsRecorded` trace fields are absent from baseline JSON and are
excluded explicitly, as are clocks. Aggregate attempts, failure/search counts,
states/transitions and final delegation reasons still have to match. Original
native reports are retained in the raw result. Quality is measured by the same
current shared metric for all normalized engine outputs.

## Public and native before/after

All values below are per-view medians in milliseconds.

| Input | Native import before / current | Core refinement before / current | Public import before / current | Public refinement before / current | Public combined before / current |
| --- | ---: | ---: | ---: | ---: | ---: |
| codegraph | 208.993 / 209.330 | 1292.489 / 1268.643 | 234.078 / 232.090 | 1311.441 / 1287.618 | 1546.428 / 1517.769 |
| workflow | 21.035 / 20.838 | 83.585 / 82.533 | 26.232 / 26.117 | 92.412 / 91.132 | 118.686 / 117.128 |
| organigram | 15.403 / 15.364 | 71.421 / 71.275 | 20.107 / 20.041 | 78.560 / 78.283 | 98.605 / 98.253 |

These are small observed changes, not evidence that the whole abstract-routing
experiment is now active. Combined medians need not equal sums of component
medians. Differences between separately timed native and public calls must not
be labelled an exact isolated serialization cost.

Optimize-only native job times (before/current) are 1,322.381/1,292.959 ms for
codegraph, 86.186/84.932 ms for workflow and 72.825/72.327 ms for organigram.
They must not be substituted for public core refinement.

## Current initial core call versus ELK

| Input | Layerweave core | elkjs 0.12.0 `layout()` | elk-rs 0.12.1 `layout()` | elk-rs adapter total |
| --- | ---: | ---: | ---: | ---: |
| codegraph | 208.098 | 309.076 | 47.760 | 48.310 |
| workflow | 20.578 | 70.986 | 10.215 | 10.352 |
| organigram | 15.226 | 51.082 | 9.155 | 9.315 |
| Sum of per-view medians | 243.902 | 431.144 | 67.130 | 67.977 |

The codegraph core gap to elk-rs is about 4.36x; the aggregate over these three
inputs is about 3.63x. This is initial layout against initial layout, not a
comparison of Layerweave's import-plus-refinement with one ELK call.
ELK conversion does not explain that core gap. These layouts are not
quality-equivalent, and this small sample does not establish a general ranking.

## Quality and the corrected self-loop adapter

The preflight exposed a frame error in the earlier ELK adapter: self-loop
sections were offset by their endpoint leaf's origin, rather than their
container's origin. The shared adapter now uses ELK's explicit `container`
metadata, or a container-only common ancestor when metadata is absent. Tests
cover both forms, nested loops, invalid frames and unsupported multi-section
edges. Unconverted/multi-section output is an explicit finding, not silently
truncated. The regular comparison harness uses this adapter too.

All engines on these three views have complete logical edge identity, finite
geometry and endpoint-to-node gaps below 1e-6 after correction. Native
baseline/current quality is exactly equal. Earlier ELK quality tables involving
self-loops must not be treated as directly corrected by this run; their
affected rows need recomputation.

Proper crossings exclude shared-endpoint crossings in this table:

| Input / engine | Proper crossings | Crossings incl. shared endpoints | Edge-node hits | Shared line pairs | Bends |
| --- | ---: | ---: | ---: | ---: | ---: |
| codegraph Layerweave initial | 3130 | 4113 | 0 | 10 | 1860 |
| codegraph Layerweave refined | 3126 | 4113 | 0 | 0 | 1844 |
| codegraph elkjs | 2863 | 3192 | 0 | 0 | 1584 |
| codegraph elk-rs | 2507 | 3057 | 0 | 0 | 1638 |
| workflow Layerweave initial/refined | 222 | 256 | 0 | 0 | 618 |
| workflow elkjs / elk-rs | 249 | 278 | 0 | 0 | 434 |
| organigram Layerweave initial | 18 | 36 | 0 | 2 | 206 |
| organigram Layerweave refined | 20 | 35 | 0 | 2 | 208 |
| organigram elkjs | 18 | 19 | 0 | 0 | 152 |
| organigram elk-rs | 22 | 23 | 0 | 0 | 148 |

The organigram refinement is not a reduction in every individual metric:
proper crossings and bends rise, while total routed crossings fall.
Acceptance still uses the configured full key. Workflow refinement retains
the initial drawing; the cost is real even when no candidate wins.
Full area, lengths, proximity and other metrics are in the raw result.

## What still costs time

Native Optimize work-counter stage medians, current codegraph:

| Stage | ms |
| --- | ---: |
| placement | 333.045 |
| routing | 791.983 |
| scoring | 108.038 |
| crossing relief (Optimize-only) | 26.642 |
| delegated routing, nested inside routing | 98.481 |
| constructor preparation, nested inside routing | 147.723 |
| constructor search, nested inside routing | 333.813 |

Stage timers are cumulative and nested; do not sum this table or add it to
wall time. Constructor searches execute 1,388,434 states and 55,055,505
transitions across 435 searches, with 1,396 planned attempts. Those totals,
candidate work and final output are unchanged before/current. Scoring falls
from 123.191 to 108.038 ms, but cannot eliminate the much larger repeated
placement/routing cost.

Workflow is more placement-heavy (42.931 ms placement, 16.142 ms routing,
11.650 ms scoring); organigram records 24.934/17.530/12.743 ms respectively.
There is not one universally dominant phase across all graph families.

## Detached release control, not public refinement

The separate five-sample codegraph experiment reuses the established frozen
input, all eight regional alternatives and three controls. Repair reuse remains
disabled. The old control already shares construction preparation; only the
new paths share coordination projections within one drawing.

| Drawing / phase | Shared-construction cold | New shared cold | Primed fork, new coordination context |
| --- | ---: | ---: | ---: |
| baseline complete detached drawing | 90.771 | 75.535 | 74.523 |
| baseline coordination | 28.496 | 13.057 | 13.259 |
| reference complete detached drawing | 100.263 | 84.520 | 83.424 |
| reference coordination | 28.560 | 13.113 | 13.100 |

All nine scenes, full keys/metrics, conflicts and eligibility match the previous
detached record exactly, including baseline `bb7b0374007882a5` and reference
`c9db33e8371676d2`. Five regional alternatives remain ineligible. A complete
detached drawing includes routing/repair/scoring on its grown geometry, but
excludes public import, the production candidate portfolio and transport.
Its approximately 75--84 ms is **not** replacement refinement latency, and its
scene/quality is not identical to the production initial/refined output above.
Do not divide public refinement time by this number to claim a speedup.

## Reproduction and checks

Build baseline from a clean archive at the revision above, then:

```sh
cargo build --release --locked -p layerweave-runtime \
  --bin bench --bin layout-command --features benchmark-timings
node --test benchmarks/eval/elk-adapter.test.mjs
node benchmarks/eval/focused-pipeline.mjs \
  --baseline /path/to/clean-baseline/target/release --repeat 5 \
  --replay benchmarks/results/focused-pipeline-20261105.jsonl \
  --out /path/to/new-result.jsonl
node benchmarks/routing-completion-check.mjs target/release/bench 5 \
  --coordination-reuse
```

`--replay` avoids regenerating repository-derived datasets and reuses the
embedded frozen graph inputs. Use new output paths rather than overwriting
historical results. Engine versions are pinned by the evaluation lockfile.
The source/binary/generator/input hashes were verified after measurement.
Three focused adapter tests, syntax checks, a regular `compare.mjs` smoke run
on `petri-net`, native before/current/repeated output/work checks, common
quality checks and full detached historical parity pass. No engine code,
dependencies, production defaults, candidate set or acceptance rule changed
in this comparison step. No full benchmark sweep or controlled idle-machine
run, commit or push was made.
