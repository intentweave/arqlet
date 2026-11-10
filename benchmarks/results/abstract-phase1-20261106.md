# Abstract placement reuse prototype (phase 1), 2026-11-06

## Result

For `abstractRouting` only, apply the hierarchical ordering sweep to the first
placement's plans before the backward-edge correction rebuild. If either
correction is needed, the resulting plans are used in one shared rebuild.
Default routing and placement retain their original ordering and rebuilds.

The four codegraph and workflow ordering candidates now use two root placement
passes instead of three. The organigram's two candidates that previously used
three passes also use two. The candidate portfolio and selection rules are
unchanged. This is not a one-pass layout: the initial placement still produces
the plans needed by both corrections.

The combined rebuild is **not output-equivalent** to the former abstract
sequence. Abstract geometry changes on all three frozen views, with mixed
quality results. The prototype is retained only behind the existing opt-in;
the default input and final fingerprints match phase 0 on all three views.

## Candidate work

Median across the four ordering candidates, milliseconds. Each phase is one
instrumented sample on a shared machine, so these numbers are diagnostic only.

| View | Root passes, phase 0 → phase 1 | Placement, phase 0 → phase 1 | Routing, phase 0 → phase 1 |
|---|---:|---:|---:|
| Codegraph (188 nodes / 358 edges) | 3 → 2 | 63.950 → 57.365 | 136.216 → 127.599 |
| Workflow (72 / 125) | 3 → 2 | 10.819 → 10.546 | 6.345 → 6.278 |
| Organigram (104 / 82) | 2–3 → 2 | 6.220 → 7.718 | 5.554 → 5.691 |

The placement-time movement is not consistent: the codegraph and workflow
medians decrease slightly, while the organigram median increases. Routing
work, including track repair, is not removed by this change. No speedup is
claimed.

## Abstract quality

Metrics for the initial imported drawing and the refined result; each cell is
`phase 0 → phase 1`. Lower values are smaller area, fewer crossings, or fewer
bends.

| View | Initial area | Initial crossings | Initial bends | Refined area | Refined crossings | Refined bends |
|---|---:|---:|---:|---:|---:|---:|
| Codegraph | 88,607,947 → 84,864,428 | 4,144 → 4,011 | 2,100 → 2,194 | 88,607,947 → 65,945,507 | 4,144 → 4,132 | 2,100 → 2,144 |
| Workflow | 2,357,632 → 2,487,657 | 280 → 328 | 464 → 470 | 2,357,632 → 2,487,657 | 280 → 328 | 464 → 470 |
| Organigram | 32,510,344 → 26,080,849 | 18 → 43 | 188 → 198 | 32,510,344 → 26,080,849 | 18 → 43 | 188 → 196 |

The crossing/bend tradeoffs rule out treating fewer placement calls as a
quality-preserving optimization. The default geometry fingerprints are
unchanged. The benchmark now asserts two root passes and one combined rebuild
for all four codegraph ordering candidates.

## Validation and limits

Run with:

```sh
cargo build --release --locked -p layerweave-runtime --bins --features benchmark-timings
node benchmarks/eval/abstract-pipeline.mjs --repeat 1 --out benchmarks/results/abstract-phase1-20261106.jsonl
```

The harness compares default, abstract, and common-incumbent native/public
paths, checks repeat identities, validates the four-candidate portfolio,
reports full quality and per-phase work, and verifies historical default
geometry. One sample per candidate on a shared host is not controlled timing
evidence. The JSON records inputs, environment, load, source and binary hashes;
the source revision was dirty.

The current approach trades the post-backward-rebuild sweep for using the
first-placement plans as sweep input. The changed abstract output is therefore
expected. A quality-preserving one-pass design would need to make final
backward-border and hierarchy-order decisions before coordinate placement,
rather than merely reorder the two existing rebuild triggers. The major
abstract routing and track-repair work remains untouched.
