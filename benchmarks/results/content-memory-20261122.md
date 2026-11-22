# Memory of a session's content per node, 2026-11-22

Question (ADR 0023, step R1, exit check): how much heap does a resident session keep for the content (properties) of a
graph, per node with properties and per property?

## Machine, build, inputs

- Apple M3, macOS 26.0, rustc 1.99.0; `benchmarks/session-memory` in release (counting allocator, exact heap bytes; not
  affected by the machine's load).
- Revision f45682f plus the change of this measurement (the `payload` operation).
- The synthetic graphs of `session-memory-20261121.md` (2k, 20k, 100k), imported with `limit` 100 and
  `levelOfDetail: {depth: 0}`; then one `payload` that sets `n` properties on every node of the session's source graph
  (a string of a few characters, an integer, a float, in turn). At 100k the import keeps 60 242 of the 114 196 nodes in
  the source graph (the selection within the limit), so the content covers those.

```sh
SESSION_MEMORY_KEYS=3 target/release/session-memory benchmarks/datasets/synthetic-20k.json 100 '{"levelOfDetail":{"depth":0}}'
```

## Results (heap the session keeps for the content, per node with properties)

| Graph | 1 property | 3 properties | 10 properties |
|---|---|---|---|
| 2 286 nodes | 741 B | 749 B | 801 B |
| 22 819 nodes | 742 B | 750 B | 802 B |
| 60 242 nodes (of 114 196) | 743 B | 751 B | 802 B |

For comparison, the session keeps about 0.8 KB per node for the structure (`session-memory-20261121.md`).

## What it means

- Content doubles a session's memory per node with a few properties, and the cost is almost all **per node, not per
  property**: about 740 B for a node's own map of properties (an id copy and a B-tree node sized for several entries),
  then about 6 B per further property of these small values.
- ADR 0023 §1 plans typed columns per property key "when a measurement shows the need". This is that measurement: a
  column per key indexed by node would cost a few bytes per value plus the values themselves, instead of a map per
  node. It is the next step for content if large graphs carry properties; R1 keeps the simple form.
