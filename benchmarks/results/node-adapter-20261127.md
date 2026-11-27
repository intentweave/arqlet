# Native Node module against WASM in a worker (ADR 0030, #223), 2026-11-27

This compares the native Node module (`crates/node`) with the WASM engine running in a `worker_threads` worker, on the
same machine, the same build and the same generated graphs. Raw lines: `node-adapter-20261127.jsonl`. Script:
`benchmarks/node-adapter.mjs`.

- **Machine:** Mac15,13, Apple M3, 8 cores, 16 GB, macOS (Darwin 25.0.0). Node v24.21.0.
- **Revision:** `e55e2ac` (branch `node-adapter`).
- **Build:** `cargo --release` (thin LTO), `--features query`, for both the `arqlet-node` cdylib and the `arqlet-wasm`
  module (`wasm32-unknown-unknown`, the playground's build, not the smaller `wasm-size` profile shipped on npm). Module
  SHA-256 prefixes: native `52af8df6fa2eac86`, WASM `db710757f179018d`. libuv's pool had its default 4 threads.
- **Load:** another agent was building Rust on the same machine. The load averages were 3.2 / 6.3 / 10.5 before the
  sweep and 2.9 / 4.2 / 8.3 after it (1, 5 and 15 minutes), and 5.3 / 4.9 / 8.3 during the peak runs. Treat every
  number as a single observation under load.
- **Inputs:** deterministic generated graphs (`graph(n)` in the script) of `n` leaves in containers of ten, with 1.3·n
  random edges (cycles, self-loops and parallel edges included). The view limit is n plus the number of containers.
- **Method:** each engine and size runs in a fresh process. Each operation runs once to warm up, then 3 more times; the
  table gives the median in ms. Times are round trips as a host sees them:
  - native: `JSON.stringify` and `JSON.parse` on the main thread, the computation on a pool thread;
  - worker: the structured clone of request and answer, plus the JSON work inside the worker.

  `dispatch import` sends the graph and receives the whole snapshot. The session operations leave the snapshot in
  the engine: `import` and `toggle` run with `scene: false`, and `scene` answers the scene of the current view.

## Time per operation (ms, median of 3)

| nodes (view limit) | engine | dispatch import | session import | session toggle | session scene | 4 sessions serial | 4 sessions at once |
|---|---|---|---|---|---|---|---|
| 100 (110) | native | 42.9 | 42.0 | 40.6 | 3.0 | 145 | 78.8 |
| 100 (110) | wasm-worker | 64.6 | 46.6 | 42.4 | 6.7 | 206 | 168 |
| 1000 (1100) | native | 1250 | 1189 | 1165 | 41.6 | 5110 | 2335 |
| 1000 (1100) | wasm-worker | 1712 | 1238 | 1130 | 47.1 | 4690 | 4142 |
| 3000 (3300) | native | 4958 | 5605 | 4477 | 84.7 | 19908 | 9775 |
| 3000 (3300) | wasm-worker | 10994 | 6420 | 6635 | 163 | 29712 | 30809 |

The two "4 sessions" columns import four different graphs of the given size into four sessions:

- **serial:** one session after the other;
- **at once:** all four sent together. Natively they run in parallel on the pool. The single WASM worker can only run
  them one after another.

## Memory

| nodes | engine | session estimate (MB) | module heap after import (MB) | WASM linear memory (MB) | peak RSS, one import alone (MB) | peak RSS, whole child run (MB) |
|---|---|---|---|---|---|---|
| 100 | native | 0.2 | 0.3 | – | 61 | 84 |
| 100 | wasm | 0.2 | 0.2 | 4.2 | 186 | 215 |
| 1000 | native | 3.7 | 4.6 | – | 183 | 529 |
| 1000 | wasm | 3.3 | 3.3 | 59.2 | 207 | 320 |
| 3000 | native | 13.0 | 15.9 | – | 610 | 2044 |
| 3000 | wasm | 11.9 | 11.4 | 229.6 | 340 | 660 |

What the columns mean:

- **Session estimate:** the session's `memory` answer (`total`).
- **Module heap:** the adapter's count of live heap (`heapBytes`).
- **Peak RSS, one import alone:** the process's peak resident memory after a single session import in a fresh
  process (`--peak`). WASM runs on the main thread there, without a worker. Before the import, the base was 49 MB
  for native and 81 MB for WASM, after compiling the module.
- **Peak RSS, whole child run:** includes the four sessions at once. Natively those are four layouts held at the same
  time.

### Re-measured after the review (2026-11-28)

The review found that the counting allocator did not forward `alloc_zeroed`. Every zeroed allocation was therefore
allocated and then written with zeros instead of taking the system's lazily zeroed pages, which can raise resident
memory. The allocator now forwards it (`df9a181`). The other times are not affected beyond noise: the counting
itself did not change.

The 3000-node single-import peak was redone with the fixed build:

| engine | before | after |
|---|---|---|
| native | 610 MB | 599 MB |
| WASM (unchanged) | 340 MB | 350 MB |

The load average was about 39 / 42 / 35, so this is a single observation. The difference is within the noise between
runs, so the conclusion stands: one native 3000-node import peaks at about 1.7–1.8× the WASM module's resident memory.
The lines are appended to `node-adapter-20261127.jsonl`.

## Reading

- **Computation is about equal.** The native module computes each layout at about the speed of WASM: session import
  and toggle are within ±10 % at 100 and 1000 nodes. At 3000 nodes native is 13 % and 33 % faster. At this load that
  is not separable from noise.
- **Transfer is where WASM loses.** Native is faster when a whole snapshot crosses the boundary, as in
  `dispatch import`: 1.4× at 1000 nodes and 2.2× at 3000. The worker clones the object both ways, and parses and
  writes JSON on top of that.
- **Parallel sessions are the gain.** Four sessions at once took 2.2× less time than in series at 1000 nodes, and 2.0×
  less at 3000, on a machine already under load. With one WASM worker they cannot overlap. Several workers would
  overlap, but each holds its own linear memory.
- **Memory is the cost.** A single 3000-node import peaks at 610 MB of resident memory natively, against 340 MB for
  WASM. The likely causes are 64-bit pointers and sizes against wasm32, and the system allocator against WASM's
  linear memory; the session estimates show the same direction (13.0 against 11.9 MB). Four layouts at once peak at
  about four single peaks (2.0 GB), which is why ADR 0024 asks for admission by memory.
- **Memory is returned.** Natively, freed memory goes back to the allocator. WASM keeps its linear memory's
  high-water mark until the worker ends.
- **Limits of this run.** One machine, one run, high and changing load. Not measured: other platforms, larger views,
  and the `wasm-size` build. An earlier attempt at 4000 nodes, under a load average of about 15–25, failed: the WASM
  child exceeded its 15-minute limit for the full set of operations. The native 4000-node line of that attempt is not
  used here.
