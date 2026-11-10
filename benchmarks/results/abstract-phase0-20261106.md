# Abstract refinement attribution (phase 0), 2026-11-06

## Result

Per-candidate counters now include each root placement's reason and the timed
subphases of public abstract completion. The harness prints these beside each
ordering candidate and stores them in the JSON report.

The measurements attribute the elapsed time well enough to choose the next
optimization:

- The abstract completion subphases account for a median **99.2% / 97.5% /
  97.3%** of the reported routing stage in codegraph, workflow and organigram.
- Existing top-level candidate stage timers account for **98.6% / 97.2% /
  95.7%** of each ordering job's time, respectively.
- Codegraph and workflow each perform **three root placement passes for every
  ordering candidate**: `initial`, `backward_edge_rebuild`, then
  `hierarchical_ordering_rebuild`. The organigram performs two or three.

This identifies repeated placement as a concrete follow-up, and shows that
codegraph's largest abstract-routing subphase is track repair—not plan
generation or fallback. It does not show that removing a pass or repair is
semantics-preserving.

## Median of the four ordering candidates

Milliseconds; one instrumented sample per candidate. Abstract subphase times
are nested within routing and must not be added to it.

| View | Placement passes | Placement | Routing | Abstract plan | Growth | Construction | Seed prep. | Fallback | Track repair | Finalization | Conflict check | Scoring | Whole candidate job |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Codegraph (188/358) | 3 each | 63.950 | 136.216 | 7.026 | 4.854 | 19.779 | 0.442 | 23.927 | 70.804 | 6.216 | 1.198 | 12.768 | 220.042 |
| Workflow (72/125) | 3 each | 10.819 | 6.345 | 1.066 | 0.861 | 2.880 | 0.102 | 0.043 | 0.166 | 0.889 | 0.041 | 1.938 | 20.303 |
| Organigram (104/82) | 2–3 | 6.220 | 5.554 | 0.694 | 0.690 | 1.746 | 0.059 | 0.504 | 0.170 | 1.526 | 0.014 | 0.949 | 14.107 |

Median values are calculated independently by phase, so the columns need not
sum to the median candidate time. Codegraph's track repair varies substantially
by candidate: 7–11 ms for two orderings and 130–172 ms for the other two.
Fallback receives 12–19 edges there, versus none or one on workflow and one on
each organigram ordering.

## Measurement and limits

```sh
cargo build --release --locked -p layerweave-runtime --bins --features benchmark-timings
node benchmarks/eval/abstract-pipeline.mjs --repeat 1 \
  --out benchmarks/results/abstract-phase0-20261106.jsonl
```

Three frozen views; four ordering candidates and the routing-only job per view.
The same harness checks repeated output/work identities and verifies that the
default geometry is unchanged. The environment was Apple M3, macOS arm64, Rust
1.99.0 and Node 24.21.0. The host was shared (load averages 5.3 / 8.3 / 6.6
at start and 3.9 / 7.6 / 6.4 at end); timings are diagnostic, not a controlled
performance comparison. Revision `d14db4513e7364102032306d560785d3927aef21`
was dirty. The JSON records hashes, candidate rows, timings, inputs and load.

The completion subphase `trackConflictCheck` only times the strict route-track
check; runtime candidate scoring remains a separate stage. Public refinement
uses `run_until`, so it does not build a diagnostic scene or run the standalone
completion evaluator. The track-repair timer is the largest codegraph phase;
its cause and the geometry/quality effects of changing it remain to be
investigated. No behavior, candidate set or acceptance rule changed.

## Next step

Profile the two repeated placement causes separately. Test whether the
`backward_edge_rebuild` and hierarchical ordering sweep can be reconciled into
one placement for abstract candidates, comparing complete geometry, crossings,
bends, strict-track conflicts, default fingerprints and per-candidate work.
Do not remove either pass merely to reduce the counter. In parallel, trace why
two codegraph candidates spend about 130–172 ms in track repair and the other
two only about 7–11 ms before attempting to narrow or skip repair.
