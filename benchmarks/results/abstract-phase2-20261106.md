# Abstract completion cost and crossing-quality findings (phase 2), 2026-11-06

## Change

Fixed-track repair now computes each route's segment/bounds projection once and
reuses it while checking other routes and evaluating repair candidates. A
projection is refreshed only when that route is moved. Fixed routes remain
immutable. The conflict-free case returns the original moving routes after
checking cancellation, without building obstacle preparation or entering a
repair pass.

This changes neither route proposals nor their acceptance: candidates still
must preserve endpoints and stub directions, reduce the current conflict
partners without adding another, and introduce no forbidden node hit. There is
no search-budget, portfolio, ordering, or quality-key change.

## Correctness and cost observations

The full abstract snapshots (`initial` and `refined`) from all three frozen
views are byte-for-byte identical to phase 1; default final fingerprints also
match. Fixed-track unit tests cover conflict-free return, cancellation,
determinism, idempotence, both axes, obstacle constraints and unresolved
conflicts.

One instrumented sample per phase on a shared machine; medians across the four
ordering candidates, milliseconds:

| View | Track repair phase 1 → phase 2 | Routing phase 1 → phase 2 |
|---|---:|---:|
| Codegraph | 68.625 → 65.714 | 127.599 → 124.797 |
| Workflow | 0.166 → 0.060 | 6.278 → 6.168 |
| Organigram | 0.245 → 0.244 | 5.691 → 5.755 |

These are unpaired, single-sample timings, not evidence of a speedup. The
implementation removes repeated track-segment extraction by construction, but
the benchmark does not yet expose the number of avoided extractions.

## Crossing-quality blocker remains

Candidate logging on the codegraph clarifies why the abstract refinement can
retain a candidate with a higher crossing count. The selected alternating
barycenter candidate has no shared-line pairs and a published key beginning
`[0, 0, 0, 7334, ...]`; the routing-only candidate has one shared-line pair and
key `[0, 0, 1, 4108, ...]`. The default key is lexicographic and places
`sharedRoutePairs` before `routedCrossings`, so the former correctly wins under
the current quality contract despite its higher crossing count. `routedCrossings`
includes proper and shared-endpoint crossings.

The metrics are not a reason to reorder or weaken the published key. Phase 2
has not improved abstract crossing quality; the next quality work must find
routes/orders that reduce the full published key without changing the existing
lexicographic objectives or candidate set.

## Follow-up ordering probes

The historical phase-2 result above remains as recorded. These follow-up
probes used the current working tree; its one-run codegraph baseline selected
key `[0, 0, 0, 6786, 0, 4093, 2144, 120170374]`. This is a focused diagnostic,
not a replacement for the saved phase-2 sweep.

Two one-variable codegraph probes narrowed the likely ordering lever. Removing
the layered-plan port order keys from abstract completion produced identical
input, geometry and refinement fingerprints (`01515ad41a932ce4`) and the same
published key. The port-key override therefore did not affect this selected
result.

Disabling the abstract channel `quality_order` changed the refinement
fingerprint from `01515ad41a932ce4` to `8a29d4248fb49f98` and worsened the
pre-optimization routed-crossing count from 4,670 to 4,674, but left the
selected post-optimization metrics and published key unchanged. Keep the
existing local channel heuristic; it helps an intermediate drawing but is not
the selected candidate's crossing-quality bottleneck here.

These are single-run, codegraph-only correctness/quality probes, not timing
measurements or evidence about other graphs. The next useful experiment is
crossing-aware feedback into the same candidate's channel/route ordering,
measured against the full published key. Port ordering alone is ruled out for
this case; candidate count and selection semantics remain unchanged.

## Validation and limits

```sh
cargo test --profile fast -p layerweave-routing fixed_tracks::tests
cargo build --release --locked -p layerweave-runtime --bins --features benchmark-timings
node benchmarks/eval/abstract-pipeline.mjs --repeat 1 --out benchmarks/results/abstract-phase2-20261106.jsonl
```

The benchmark verifies repeat identities and default historical geometry. The
saved phase-2 result records the frozen inputs and measurement provenance. Its
timings are diagnostic only; the host was shared and the phase-1/phase-2 runs
were not paired under controlled load.

## Phase-2 close-out

The final integration pass adds deterministic repair counters for materialized
segment projections (initial projections plus refreshes after accepted moves)
and pair checks used to report initial and remaining fixed-track conflicts.
These counters are exposed in each candidate's benchmark work record. They
measure concrete operations, not a reconstruction of how many projections a
different implementation would have computed.
Raw repeated results: [abstract-phase2-closeout-20261105.jsonl](abstract-phase2-closeout-20261105.jsonl).

Validation on the current dirty worktree:

- `cargo fmt --all -- --check`, `cargo check --workspace --locked`, and
  `cargo test --workspace --profile fast --locked` passed.
- `sh apps/playground/build.sh` passed, including native/WASM parity, stale
  request handling, and split, browser-only, and backend-only UI checks.
- The integrated abstract-pipeline benchmark ran with three measured rounds
  after warmup on the three frozen views. Output/work identities repeated and
  the default historical geometry check passed.

The repeated public pipeline medians (ms) were:

| View | Default | Abstract opt-in | Abstract total crossings vs default |
|---|---:|---:|---:|
| Codegraph | 1506.271 | 1076.136 | 6786 vs 4113 |
| Workflow | 116.862 | 122.275 | 456 vs 256 |
| Organigram | 98.058 | 101.281 | 95 vs 35 |

These are not equivalent-quality speedups: crossings include shared-endpoint
crossings, and the abstract path loses on all three selected outputs. The
machine was shared (load average 16.66 at start, falling to 6.67); the medians
are diagnostic. Defaults remain unchanged and abstract routing remains opt-in.

Across the four ordering candidates, the new counters reported:

| View | Segment projections computed | Reported pair checks |
|---|---:|---:|
| Codegraph | 1462 | 39918 |
| Workflow | 500 | 124 |
| Organigram | 330 | 648 |

Pair checks here mean the initial/final conflict-pair scans only; they exclude
the other occupancy checks during candidate generation and acceptance. The
counts confirm the reused-projection path is exercised, but do not by
themselves establish elapsed-time savings over the previous implementation.

The phase-2 investigation is closed with a negative quality result, not a
quality fix. The port-order ablation was output-identical on the codegraph;
removing the abstract channel heuristic worsened its pre-optimization crossing
count and did not improve the selected key. Neither supports changing port
ordering, the candidate portfolio, or the lexicographic quality key. A
crossing-aware abstract ordering experiment is a separate follow-up; it must
improve the full published key on the frozen views before consideration.
