# Matched in-process layout comparison, 2026-11-04

## Method

This run compares Layerweave's `compute_counted` layout core with elkjs 0.12.0 and elk-rs 0.12.1
`layout()` calls on the same nine uniform-direction bounded graph views. Each engine's row has the
same per-view `inputSha256`; layouts ran five times and the report sums the per-view medians.
Layerweave used its release `bench` binary with one warm-up and one measured call per process; the
harness repeats that process five times. ELK engines likewise reuse their initialized process
objects across five calls.

The shared input view is obtained once with Layerweave import before the measured calls. That import
is a setup operation, **not** a measurement of view selection alone: it also performs a complete
layout. Its `referenceImportMs` values are recorded in the raw rows but excluded from the core
comparison. The measured Layerweave call reads and deserializes the selected graph before starting
its timer, then runs `compute_counted`. ELK graph-object construction is also outside `layoutMs`;
the harness reports it as `adapterPrepareMs`. ELK output conversion is outside `layoutMs` and is
reported as `adapterNormalizeMs`; `adapterTotalMs` covers conversion, `layout()`, and normalization.
Snapshot serialization and Layerweave process startup are excluded from this in-process comparison.

Thus the engine-call boundaries are substantially closer than the public-process benchmark, but not
perfectly identical: Layerweave's core receives its Rust `Graph` and options, while ELK receives the
corresponding JavaScript ELK graph object. Configuration and algorithms are matched to layered,
uniform-direction, orthogonal layouts where supported; algorithm internals are not expected to match.
The machine was shared (Apple M3, macOS, 8 logical CPUs, Node 24.21.0, Rust 1.99.0) with load
averages around 4–5. The worktree was dirty at revision `e966e93`; treat the data as a current
working-tree comparison, not a clean release result.

Raw data: [inprocess-core-elk-20261104.jsonl](./inprocess-core-elk-20261104.jsonl).

## Results

Times are sums of the nine per-view medians. Quality metrics are sums over the nine outputs.

| Engine | Core time (ms) | ELK adapter total (ms) | Crossings | Edge-node hits | Bends |
|---|---:|---:|---:|---:|---:|
| Layerweave `compute_counted` | 281.76 | — | 3544 | 0 | 3449 |
| elkjs 0.12.0 | 513.99 | 515.11 | 3314 | 2 | 2724 |
| elk-rs 0.12.1 | 89.93 | 90.85 | 2957 | 1 | 2742 |

On these inputs, Layerweave's core call is about 1.82× faster than elkjs's core call. elk-rs is
about 3.13× faster than Layerweave's core call. ELK input conversion and output normalization add
about 1.1 ms to the sum of per-view medians; they do not explain the elk-rs difference. These are
not quality-equivalent outputs: crossings, bends and edge-node hits differ, and Layerweave has no
edge-node hits in this run.

This result supersedes the earlier apples-to-oranges public timing for statements about core layout
time only. Public end-to-end import/refinement still needs its own table: it includes process,
serialization and view preparation on Layerweave's side, while elkjs/elk-rs are libraries called
in-process. Refinement is not included in the table above.

## Reproduction

```sh
cargo build --release --locked -p layerweave-runtime \
  --bin bench --bin layout-command --features benchmark-timings
node benchmarks/eval/compare.mjs --inprocess-core --uniform-direction --repeat 5 \
  --out benchmarks/results/inprocess-core-elk-20261104.jsonl \
  layerweave-code deployment-cluster workflow-dag org-chart cloud-architecture network-vlan petri-net uml-class layerweave-pipeline
```
