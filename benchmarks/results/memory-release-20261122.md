# What a session holds, and giving it back, 2026-11-22

Questions (ADR 0023, R7): how close is a session's own estimate (`memory`) to the heap it really keeps, and what did the
measurement turn up?

## Machine, build, inputs

- Apple M3, macOS 26.0, rustc 1.99.0; `benchmarks/session-memory` in release (counting allocator: exact heap bytes; not
  affected by the machine's load). Revision 92384a4 plus the change of this measurement.
- The synthetic graphs of `session-memory-20261121.md`, imported with `limit` 100 and `levelOfDetail: {depth: 0}`; with
  `SESSION_MEMORY_KEYS=3`, three properties set on every node of the source graph in one `payload`.

```sh
SESSION_MEMORY_KEYS=3 target/release/session-memory benchmarks/datasets/synthetic-100k.json 100 '{"levelOfDetail":{"depth":0}}'
```

## The estimate against the counted heap

| Graph | Counted after the import | `memory` estimate | With content: counted | estimate |
|---|---|---|---|---|
| 2 286 nodes | 1.4 MB | 1.3 MB (91 %) | 4.7 MB | 4.5 MB (96 %) |
| 22 819 nodes | 16.5 MB | 15.0 MB (91 %) | 32.9 MB | 31.0 MB (94 %) |
| 114 196 nodes (60 242 kept) | 67.4 MB | 57.1 MB (85 %) | 110.6 MB | 99.5 MB (90 %) |

The estimate is a little low, never high: it covers the parts by their sizes and lengths (strings and vectors by capacity,
B-tree maps by the nodes their length needs, the rest of the snapshot by the length of its JSON text) and leaves out what
it cannot see cheaply. A host can treat it as a lower bound within 15 %.

## What the measurement turned up (fixed here)

- **The import kept the room of the nodes it dropped.** It selects the nodes of the view with `retain`, which keeps the
  capacity of the source graph's lists: on the 114k graph, half of it. Trimming them after the selection: a session keeps
  67.4 MB instead of 89.0 MB (−24 %), output unchanged.
- **A large payload was held twice**, as content and as a group in the change log (whose bound counted the JSON text,
  several times smaller than the heap, and never dropped a single group). The log is now bounded by the estimated heap,
  and a group larger than the whole bound is not kept: a replica behind it resyncs. Content per node with three
  properties: 1 516 B → 751 B on the 20k and 114k graphs (the 2k graph's small payload stays in the log, as it should).
- **The layered plans** (`layeredPlans`, kept so that later layouts continue from them) are the largest part of the
  snapshot after the source graph on a large first view: 21.6 MB of JSON on the 114k graph. `memory` names them; they
  are not released (later layouts depend on them).

## `release`

Content (all of it, or named keys, which goes into the change log as `dropKeys`), the change log, the columns and the
overview. The snapshot stays: releasing never changes the view (tested). Freed room goes back to the allocator; in WASM
the linear memory stays at its high-water mark (`linearMemoryBytes`), which only a new worker gives back.

## Moving a session to a new engine (`session.recycle`)

A graph of 20 containers with 150 functions each (3 020 nodes, 3 019 edges), imported with `limit` 3 100 (laid out in
full) into a resident session of the WASM engine in Node 24, then moved to a fresh engine instance:

| | Linear memory | Live heap (counted) | `memory` estimate |
|---|---|---|---|
| Old engine after the import | 39.9 MB | 3.7 MB | 3.5 MB |
| New engine after `recycle` | 9.8 MB | 3.9 MB | 3.6 MB |
| A fresh engine, nothing loaded | 1.3 MB | 0.03 MB | |

The new engine holds the session's heap plus what reading the snapshot's text (3.2 MB of JSON) left as its high-water
mark. Two things had to change for this: `load` reads the snapshot in one pass from text (no JSON value of it), and the
recycle loads without a scene in the answer (writing the scene of 3 000 nodes had grown the new engine to 27.6 MB).
