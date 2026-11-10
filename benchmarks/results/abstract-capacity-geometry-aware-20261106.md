# Geometry-derived abstract channel capacity, 2026-11-06

## Change under test

Placement now uses track counts derived from actual cross-axis lane positions
and occupied channel spans to widen rank gaps. The ordinal interval-overlap
estimate is no longer translated directly into physical width. This keeps the
physical clearance check and growth path authoritative.

The regression test verifies that the same lane domains require the same
capacity after translation along the flow axis, and that realized rank spacing
meets the clearance-plus-track requirement. This is a geometry invariant, not
a before/after performance comparison.

## Frozen-view diagnostic

Command:

```sh
cargo build --release --locked -p layerweave-runtime --bins --features benchmark-timings
node benchmarks/eval/abstract-pipeline.mjs --repeat 1 \
  --out benchmarks/results/abstract-capacity-geometry-aware-20261106.jsonl
```

The harness compares default and `abstractRouting` paths on the same three
frozen views, retaining the existing candidate portfolio and checking repeated
output/work identities. One warmup precedes the single measured sample per arm.
The host was an Apple M3 / macOS arm64 with Rust 1.99.0 and Node 24.21.0; it was
heavily loaded (load averages 17.4 / 9.7 / 9.0). Treat all times as
non-controlled diagnostics. The worktree was dirty at revision
`d14db4513e7364102032306d560785d3927aef21`; raw inputs, source and binary hashes,
and results are in
[abstract-capacity-geometry-aware-20261106.jsonl](abstract-capacity-geometry-aware-20261106.jsonl).

| View | Nodes / edges | Native import ms: default / abstract | Native refine-core ms: default / abstract | Public pipeline ms: default / abstract |
|---|---:|---:|---:|---:|
| Codegraph | 188 / 358 | 210.466 / 154.625 | 1265.332 / 950.273 | 1503.915 / 1146.518 |
| Workflow | 72 / 125 | 21.110 / 20.857 | 82.380 / 90.864 | 117.072 / 124.629 |
| Organigram | 104 / 82 | 15.471 / 16.313 | 71.123 / 69.359 | 97.859 / 98.170 |

| View | Refined area: default / abstract | Crossings: default / abstract | Bends: default / abstract | Constructor searches / transitions: default / abstract |
|---|---:|---:|---:|---:|
| Codegraph | 89.24M / 88.61M | 3,126 / 4,144 | 1,844 / 2,100 | 435 / 55,055,505 vs. 0 / 0 |
| Workflow | 2.36M / 2.36M | 222 / 280 | 618 / 464 | 11 / 1,345 vs. 0 / 0 |
| Organigram | 36.19M / 32.51M | 20 / 18 | 208 / 188 | 20 / 408,512 vs. 0 / 0 |

Abstract routing avoids constructor search in these runs, but it does not
consistently improve public pipeline time or quality: the codegraph is faster
in this one sample but has 1,018 more crossings; the workflow is slower and has
58 more crossings; the organigram times are effectively inconclusive. The
timings do not isolate the effect of the geometry-derived capacity calculation
from the rest of the abstract-routing path. This result is therefore not
evidence for a general performance gain or a default switch.
