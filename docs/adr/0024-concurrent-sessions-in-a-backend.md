# ADR 0024: Concurrent sessions in a backend

Date: 2026-11-22.
Status: proposed. C2 is built (provisional, 2026-11-27): `arqlet_runtime::shared_sessions::SharedSessionStore`, used by
the native Node adapter ([ADR 0030](0030-native-node-adapter.md)). C3's estimate is built (provisional, 2026-11-27):
`estimate`, `benchmarks/results/memory-estimate-20261127.md`, and the worker pool with admission by memory in the
integration guide. The playground backend still runs one `layout-command --sessions` process and feeds it one message at
a time (C1 is open).

Related: [ADR 0019](0019-engine-interface.md) (sessions, the session store belongs to the runtime),
[ADR 0023](0023-primary-store-replicas-and-change-stream.md) (item 9: sessions as a cache, a native Node adapter; step
S4 points here). Requirements: the concurrency rules in `CLAUDE.md` (core synchronous and pure, parallelism in the
runtime, asynchrony in hosts, output identical for any thread count).

## Context

With sessions as a cache (ADR 0023, item 9), one backend instance holds many sessions. Run serially, one large layout
(a 114k-node graph's first view peaks at about 650 MB and takes seconds) blocks every other session of the instance. The
session store (`SessionStore`, ADR 0023 R7) is single-threaded: it takes one message, runs it and weighs the session.

## Decision

### 1. Parallel between sessions, serial within one

- **Within a session** the order stays fixed: every answer builds on the revision before. Each session has its own queue;
  only a request that a newer one replaces completely may be dropped before it runs (a move to an absolute position
  while dragging, a refinement, a read whose answer nobody waits for any more). A change of the graph or the content
  (`patch`, `change`, `payload`: whatever enters the change log) is never dropped; each one builds on the one before.
  A result whose revision is stale is rejected, as everywhere else.
- **Between sessions** nothing is shared, so they run in parallel. Determinism is kept: no session sees another, so each
  answer is the same as in a serial run, for any number of threads.

### 2. The store locks briefly, a session long

- The store's map, the weights and the least-recently-used order sit behind one short lock (look up, account, choose
  what to evict). A session sits behind its own lock (`Arc<Mutex<EngineSession>>` or a queue owned by one worker); the
  computation runs under that lock only, never under the store's.
- **A session that runs or has queued work is never evicted**; eviction picks among idle sessions only. With sessions
  growing at the same time, the byte limit is a soft limit; the admission below keeps the overshoot bounded.

### 3. Admission by memory, not only by cores

The peak of a layout, not the number of cores, bounds how many may run at once (eight 650 MB peaks are 5 GB). A host
admits work by a budget: a limit on concurrent heavy operations (import, load, layout, refinement) and, once the engine
estimates it, a budget by the expected peak (from the view's size). Light operations (`details`, `find`, `peek`,
`changesSince`, `memory`) bypass it.

### 4. One thread budget

The refinement already runs independent jobs on scoped threads (`parallel_map`, `threads`). A host gives each admitted
operation a share of one budget, so parallelism across sessions and within one do not oversubscribe the machine. The
output does not depend on the share (`refinement_is_identical_for_any_thread_count`).

### 5. Where it lives

- **Core:** unchanged, synchronous and pure.
- **Runtime:** a session store usable from several threads (the locks of item 2, eviction of idle sessions only).
- **Hosts:**
  - *Process pool* (smallest first step, no Rust change): N `layout-command --sessions` processes, a session bound to one
    by its key; a crash or a time limit costs only that process's sessions, and each process gets a hard memory limit
    from the operating system.
  - *Node adapter* (ADR 0023 S1, napi-rs): each call an asynchronous task on the libuv thread pool, on one shared store.
  - *Native server:* threads over the shared store.
- No async runtime or thread-pool crate without a measured need (`CLAUDE.md`).

### 6. Cancellation

Stale work of a session is dropped from its queue; a running one finishes and its result is rejected by revision. An
in-process cancel flag, checked between independent jobs, is added when a measurement shows the need; terminating the
process stays the hard stop.

## Steps

- **C1 — process pool in the playground backend:** N processes, sessions bound by key, admission by a count of heavy
  operations. *Exit:* two sessions' layouts overlap in time (measured); answers byte-identical to the serial backend
  (`server_test.py`); evicted keys answered as unknown, as today.
- **C2 — a store usable from several threads in the runtime** (item 2), eviction of idle sessions only. *Exit:* a test
  with concurrent sessions gives the same answers as the serial run; an active session is never evicted.
- **C3 — admission by expected peak:** an estimate of a layout's peak from the view's size, recorded against
  `benchmarks/session-memory`. *Exit:* the estimate within a stated error on the reference graphs.

## Consequences

- One instance serves many sessions without one large layout blocking the rest.
- The byte limit becomes soft while sessions run; admission bounds the overshoot.
- The process pool keeps crash and memory isolation; the shared store saves the copies a pool cannot share (a graph used
  by sessions in two processes is held twice).

## Not decided here

- The number of processes or threads, and the admission limits: host configuration, set by measurement.
- Whether sessions of one graph share its index across sessions (one immutable graph by digest, ADR 0023 item 9) in a
  shared store.
