# Exact abstract channel slots before coordinate placement, 2026-11-05

## Change under test

With `abstractRouting` enabled, local rank-channel demand is now scheduled
before coordinate assignment by the same ordinal channel-slot allocator used by
the complete abstract routing plan. Edges with explicit endpoint ports are
excluded because the complete planner delegates them. The placement still
converts the ordinal track count to a rank-gap seed; the later physical geometry
check, growth safety net, fallback routing and quality gates remain unchanged.

Focused tests compare pre-placement and complete-plan channel track counts for
crossings, parallel edges, long-edge chains and an explicitly delegated edge.
All match. This proves consistency of the ordinal scheduler for these cases,
not physical feasibility or equivalence of final drawings.

## Frozen-view diagnostic

Command:

```sh
cargo build --release --locked -p layerweave-runtime --bins --features benchmark-timings
node benchmarks/eval/abstract-pipeline.mjs --repeat 1 \
  --out benchmarks/results/abstract-capacity-exact-slots-20261105.jsonl
```

The one-sample run uses the three frozen views and unchanged candidate portfolio.
It ran on an Apple M3 / macOS arm64 with Rust 1.99.0 and Node 24.21.0. The host
was shared and load was nontrivial; timings are diagnostic only. Raw results,
environment and hashes are in
[abstract-capacity-exact-slots-20261105.jsonl](abstract-capacity-exact-slots-20261105.jsonl).

| View | Initial area before exact slots | Initial area after | Initial crossings before / after | Placement passes before / after | Constructor searches / transitions after |
|---|---:|---:|---:|---:|---:|
| Codegraph | 99.20M | 113.56M | 4,248 / 4,248 | 12 / 12 | 0 / 0 |
| Workflow | 3.06M | 3.46M | 280 / 280 | 12 / 12 | 0 / 0 |
| Organigram | 33.46M | 34.00M | 18 / 18 | 10 / 10 | 0 / 0 |

The before values are the five-sample capacity-seeding record; after values
come from this one-sample run. The same abstract-path option already had zero
constructor searches, so this change does not eliminate additional searches or
placement passes. Crossings and bends are unchanged, while initial area grows
by about 14.5% for codegraph, 13.0% for workflow and 1.6% for organigram.
The exact ordinal slot count is therefore too conservative as a physical
capacity estimate in these inputs. Do not claim a performance or quality
improvement from this trial.

## Decision

Keep the physical width check and safety net. The implementation establishes
the requested pre-placement connection to the abstract channel scheduler, but
the area result shows that slot count cannot be translated directly into
`tracks × spacing` as a useful physical estimate. The next step is to derive a
tighter geometry-aware lower bound from endpoint lane domains and actual
channel occupancy, then compare its predicted requirement with the existing
post-placement physical demand. Do not reduce candidate work or remove the
fallback based on this trial.
