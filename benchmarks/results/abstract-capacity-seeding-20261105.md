# Pre-placement ordinal channel-capacity seeding, 2026-11-05

## Result

The opt-in abstract path now estimates ordinal channel interval overlap before
assigning rank coordinates and uses it to seed the rank gaps. The later
geometry-based width check remains authoritative.

The estimate executes, but this comparison does **not** show a quality or
general performance benefit from seeding. On the abstract path's own input, the
selected output is the initial drawing for all three views. Relative to the
previous abstract-path run, the estimate increases the initial drawing's area
on all three views without reducing its crossing counts. Keep it provisional
and opt-in; do not describe it as a speedup or a proven reduction in later
growth.

## Method and provenance

- Run:

  ```sh
  cargo build --release --locked -p layerweave-runtime --bins --features benchmark-timings
  node benchmarks/eval/abstract-pipeline.mjs --repeat 5 --out benchmarks/results/abstract-capacity-seeding-20261105.jsonl
  ```

- Same harness, release/thin-LTO instrumented binaries and three frozen inputs
  as the integrated comparison: `layerweave-code` (188 nodes/358 edges),
  `workflow-dag` (72/125), and `org-chart` (104/82). Five samples after one
  warmup, with rotated/reversed arm order. Each default, abstract, and
  common-incumbent arm is repeated and checked for output/work identity.
- The previous abstract-path comparison used the same inputs and harness.
  Between those runs, the only Rust source hashes that changed were
  `crates/layout/src/lib.rs` and `crates/layout/src/abstract_routing.rs`.
- Apple M3, eight logical CPUs, macOS/arm64; Node 24.21.0 and Rust 1.99.0.
  HEAD `d14db4513e7364102032306d560785d3927aef21`, dirty worktree. Load
  averages were 36.77/15.42/13.43 at start and 10.45/12.31/12.45 at end.
  This is shared-machine diagnostic data, not an idle-machine sweep.
- Raw timings, per-sample outputs and work, full quality metrics, source/input
  hashes and binary hashes are in
  [abstract-capacity-seeding-20261105.jsonl](abstract-capacity-seeding-20261105.jsonl).

## Public timing

Median milliseconds. `Prior abstract` is the earlier run before rank-gap
seeding; it is historical context, not a simultaneous control.

| View | Default import + refine | Prior abstract import + refine | Capacity-seeded abstract import + refine |
|---|---:|---:|---:|
| Codegraph | 1,511.444 | 1,192.714 | 1,147.824 |
| Workflow | 116.884 | 129.510 | 134.066 |
| Organigram | 98.238 | 101.285 | 104.880 |

The codegraph's capacity-seeded public call is 3.8% below the earlier abstract
run; workflow and organigram are 3.5% and 3.6% above it. Shared-machine load,
the changed candidate geometries and the small sample prevent attributing
these differences to faster execution. The rank-gap estimate did not reduce
placement-pass counts relative to the prior abstract run: 12/12/10 passes in
both. Abstract routing still removes constructor transitions in these cases,
but that benefit predates capacity seeding.

## Quality and geometry

`Proper / total crossings` separates the route-crossing metric from crossings
that include shared endpoints. Quality is measured on the selected public
refinement result from each arm.

| View | Default proper / total | Capacity-seeded abstract proper / total | Bends default / abstract | Area default / abstract |
|---|---:|---:|---:|---:|
| Codegraph | 3,126 / 4,113 | 4,248 / 6,579 | 1,844 / 2,110 | 89.24M / 99.20M |
| Workflow | 222 / 256 | 280 / 400 | 618 / 464 | 2.36M / 3.06M |
| Organigram | 20 / 35 | 18 / 74 | 208 / 190 | 36.19M / 33.46M |

Compared with the prior abstract import, the capacity-seeded initial geometry
changes as follows:

| View | Prior width × height | Seeded width × height | Prior / seeded proper / total crossings |
|---|---:|---:|---:|
| Codegraph | 11,682 × 7,585 | 13,078 × 7,585 | 4,144 / 6,430 -> 4,248 / 6,579 |
| Workflow | 2,742 × 860 | 3,558 × 860 | 280 / 400 -> 280 / 400 |
| Organigram | 8,951 × 3,632 | 8,951 × 3,738 | 18 / 74 -> 18 / 74 |

The added flow-axis space is substantial for codegraph and workflow, with no
crossing reduction on the abstract path's own imported drawing. The
common-incumbent arm preserves the original incumbent; its selected codegraph
result improves against the prior abstract common-incumbent result but still
has 7,162 total crossings versus 4,113 for default. The organigram continues
to report two unresolved strict track pairs on abstract import. All tested
outputs have zero geometry issues and zero line-node hits.

## Decision and next measurement

Keep the option disabled by default. Before treating this as an optimization,
instrument each channel's seeded width, its width after the geometric
realization check, and any remaining late shift. That will distinguish useful
preallocation from conservative over-allocation. Then compare a bounded
estimate against the physical lane demand while retaining final geometry and
quality checks. No candidate or search budget should change as part of that
experiment.
