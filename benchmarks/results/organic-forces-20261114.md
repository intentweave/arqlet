# organicForces and repairCompaction (2026-11-14)

Machine: Apple M3, macOS (Darwin 25.0.0), not idle. Build: `cargo build --release -p arqlet-runtime --bins` at 5451ef6 plus the uncommitted change on branch `packing`. Each dataset of `benchmarks/datasets` (as in `s1-4-defaults.mjs`, limit 150) imported once with the default options and once with the variant; one metric implementation (`benchmarks/eval/lib.mjs`). Single runs: times are indicative. Lines show only datasets whose metrics differ; all others are identical.

Columns: h line–node hits, c crossings, b bends, a area (k units²), g geometry issues.

## `organicForces: true`

```
can-bus                      DIFF  default h0 c0 b32 a1207k g0 8ms | variant h0 c0 b32 a1210k g0 8ms
mixed-layouts                DIFF  default h0 c5 b0 a1330k g0 80ms | variant h0 c0 b0 a991k g0 25ms
organic-200                  DIFF  default h0 c5 b0 a12848k g0 299ms | variant h0 c2 b0 a11125k g0 115ms
organic-25                   DIFF  default h0 c0 b0 a1487k g0 12ms | variant h0 c1 b0 a1105k g0 20ms
organic-50                   DIFF  default h0 c2 b0 a3826k g0 44ms | variant h0 c2 b0 a2290k g0 24ms
petri-net-shapes             DIFF  default h0 c27 b0 a1614k g0 536ms | variant h0 c26 b0 a946k g0 107ms
self-loops                   DIFF  default h0 c0 b30 a255k g0 5ms | variant h0 c0 b30 a526k g0 12ms
```

## `repairCompaction: true`

```
organic-25                   DIFF  default h0 c0 b0 a1487k g0 10ms | variant h0 c0 b0 a1351k g0 34ms
petri-net-shapes             DIFF  default h0 c27 b0 a1614k g0 535ms | variant h0 c27 b0 a1247k g0 676ms
```

## both

```
can-bus                      DIFF  default h0 c0 b32 a1207k g0 7ms | variant h0 c0 b32 a1210k g0 8ms
mixed-layouts                DIFF  default h0 c5 b0 a1330k g0 20ms | variant h0 c0 b0 a991k g0 24ms
organic-200                  DIFF  default h0 c5 b0 a12848k g0 284ms | variant h0 c2 b0 a11125k g0 308ms
organic-25                   DIFF  default h0 c0 b0 a1487k g0 9ms | variant h0 c1 b0 a1099k g0 49ms
organic-50                   DIFF  default h0 c2 b0 a3826k g0 41ms | variant h0 c2 b0 a2290k g0 85ms
petri-net-shapes             DIFF  default h0 c27 b0 a1614k g0 564ms | variant h0 c26 b0 a946k g0 188ms
self-loops                   DIFF  default h0 c0 b30 a255k g0 6ms | variant h0 c0 b30 a526k g0 12ms
```

Default fingerprints (`fingerprints-20261114-channels-by-demand-default.jsonl`): identical on all 15 views.

## Second version: anchors, room for loops, conflicts in the choice

`organicForces` after three changes: the springs and the push off the edges take an edge where it ends inside a child (a
container), not at the child's centre; the forces' arrangement is taken only when its straight edges cross no more often
and pass through no more boxes than after the stress; and the loops of a node take a corner where no neighbour lies.
organic-25 is unchanged now (2 boxes in the way after the stress, 7 after the forces), self-loops is smaller with the same
bends. mixed-large: fewer crossings and less area, but one straight edge through a node remains (a geometry issue) and 16
bends more; open.

```
can-bus                      DIFF  default h0 c0 b32 a1207k g0 8ms | variant h0 c0 b32 a1210k g0 8ms
mixed-large                  DIFF  default h0 c976 b150 a21452k g0 1113ms | variant h1 c789 b166 a16412k g1 879ms
mixed-layouts                DIFF  default h0 c5 b0 a1330k g0 20ms | variant h0 c0 b0 a1038k g0 32ms
organic-200                  DIFF  default h0 c5 b0 a12848k g0 280ms | variant h0 c2 b0 a11125k g0 89ms
organic-50                   DIFF  default h0 c2 b0 a3826k g0 40ms | variant h0 c2 b0 a2290k g0 23ms
petri-net-shapes             DIFF  default h0 c27 b0 a1614k g0 535ms | variant h0 c26 b0 a946k g0 107ms
self-loops                   DIFF  default h0 c0 b30 a255k g0 5ms | variant h0 c0 b30 a245k g0 4ms
synthetic-2k                 DIFF  default h0 c2792 b1613 a61421k g0 147ms | variant h0 c2792 b1605 a61421k g0 155ms
```

Default fingerprints: identical on all 15 views. Tests: 850 passed.

## Third version, the default

A fresh layout that keeps a geometry issue is laid out again without the forces (mixed-large: identical to before); the
loop corner rule acts in organic areas only. "default" below is the new default (forces on), "variant" is
`organicForces: false`:

```
can-bus                      DIFF  default h0 c0 b32 a1210k g0 11ms | variant h0 c0 b32 a1207k g0 9ms
mixed-layouts                DIFF  default h0 c0 b0 a1038k g0 36ms | variant h0 c5 b0 a1330k g0 22ms
organic-200                  DIFF  default h0 c2 b0 a11125k g0 97ms | variant h0 c5 b0 a12848k g0 305ms
organic-50                   DIFF  default h0 c2 b0 a2290k g0 25ms | variant h0 c2 b0 a3826k g0 44ms
petri-net-shapes             DIFF  default h0 c26 b0 a946k g0 114ms | variant h0 c27 b0 a1614k g0 568ms
self-loops                   DIFF  default h0 c0 b30 a245k g0 5ms | variant h0 c0 b30 a255k g0 5ms
```

Default fingerprints: identical on all 15 views. Tests: 851 passed. `apps/playground/build.sh`: passed.
