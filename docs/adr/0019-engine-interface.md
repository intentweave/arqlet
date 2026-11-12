# ADR 0019: An own engine interface: typed operations, sessions in Rust and JavaScript, a columnar data path

Date: 2026-11-12.
Status: accepted (2026-11-12); step 1 implemented in part (see Steps).

Related: [ADR 0006](0006-reactive-layout-session.md) (reactive sessions), [ADR 0008](0008-engine-client-viewer-boundaries.md)
(engine, client, viewer), [ADR 0018](0018-cluster-overview.md) (cluster overview).

## Context

Arqlet's interface today is one JSON dispatcher: `dispatch(&str) -> String` in Rust, `lw_run` over JSON in WASM, the
same JSON behind the loopback backend. Hosts call it in three ways:

- **ELK-compatible, one shot** (`elkLayout`, `createElkLayout`): a graph in, positions out, every call from scratch.
- **Operations on a snapshot** (`import`, `toggle`, `reveal`, `direction`, …), with revision checks; in JavaScript
  wrapped by `createSession`.
- **The overview** (`overview`), which answers with parallel arrays instead of objects.

What 0.2 and 0.3 added (sessions that keep positions, groups and drill-down, path cut-outs, the overview with in-place
structure) does not fit ELK's model, which knows neither state between calls nor views of a graph. Integrations ask for
both a Rust interface (backends) and a JavaScript one (browsers, TypeScript products).

Two properties of the current protocol limit large graphs
([message sizes](../../benchmarks/results/message-sizes-20261112.md)):

1. **The state travels with every request.** An edit sends the whole snapshot and returns a whole one. At 10 000 nodes
   with 105 visible boxes, one `toggle` request is 3.86 MB, and the answer is as large. In the browser each of them is
   copied into the worker (structured clone) and parsed into WASM memory.
2. **Everything is JSON objects keyed by string ids.** Fine for control and small views; for whole graphs and
   positions it is several times larger than columns of numbers (the overview's columnar answer is about 5× smaller
   than the snapshot of the same graph).

The goal of 100 000 nodes and more is unmeasured. The interface should not have to be replaced to reach it, so the data
path for large graphs is designed now and built when measurements ask for it.

## Decision

### 1. Three layers

1. **Protocol.** Every operation is a variant of one typed `Request` enum and answers with one typed `Response`, in a
   protocol module that the facade crate exports (`#[serde(tag = "op")]`, field names as today). JSON is its
   serialization; `dispatch` parses into `Request`, handles it, and writes `Response`. The JSON format does not change
   in this step: every request the tests and the playground send today must parse and answer byte-identically. Each
   response carries a `protocolVersion`; snapshots and scenes keep their `schemaVersion`.
2. **Sessions.** The interface for hosts. A session owns the state (snapshot, view, revisions) and offers methods:
   `import`, `toggle`, `expand`, `collapse`, `reveal`, `trace`, `apply` (any other operation), `scene`, `overview`.
   The same session exists in Rust (`arqlet::Session`) and in JavaScript (`createSession`, as today); names and
   semantics match. Revision checks live inside the session, so a host cannot apply a stale answer by mistake.
3. **Adapters** for other worlds, outside the core interface: ELK (one shot, kept for compatibility and comparison),
   Intentweave exports, React Flow output, the SVG and WebGL viewers.

### 2. Rust and JavaScript as equals

- Rust: `Session` with typed options (`#[non_exhaustive]` structs, `Default`), `Result` errors, no JSON in the API.
- JavaScript: `createSession(run)` over the WASM engine, a worker or the backend; hand-written `.d.ts`, the data types
  generated from the Rust types (`ts-rs`, as today).
- One protocol underneath both, so a backend session and a browser session can hand a view to each other (split mode).

### 3. State stays where it is computed

A session keeps its snapshot inside the engine: in WASM memory, in a native process, or in the backend. A request then
names the session and the change (`{op: "toggle", session, nodeId, expectedRevision}`), not the whole state; the answer
carries what changed (the scene or the delta), not the whole snapshot. A snapshot is exported only on request (save,
hand-over between backend and browser, undo history). The operations on a passed snapshot stay as they are, for hosts
that hold the state themselves.

The core crates stay pure and synchronous: the session store (handles, lifetimes) lives in `crates/runtime` and the
adapters (`crates/wasm`, the backend), not in `model`, `layout` or `view`.

### 4. A columnar data path for large graphs

Designed now, built step by step when measurements ask for it:

- **Interned ids.** At import every node and edge id gets a dense `u32` index; columns use indices, the string ids are
  kept once in a table.
- **Columns for bulk data** (struct of arrays): nodes (parent, kind index, size), edges (source, target), results
  (x, y, width, height per node; route points as offsets into one coordinate array). The overview already answers this
  way; `overviewBuffers` and the WebGL renderer consume it.
- **Zero-copy reads in the browser.** Bulk results are typed-array views (`Float32Array`, `Uint32Array`) into WASM memory,
  valid until the next change of that session; a host that keeps them copies them.
- **Between worker and page:** results move as transferable `ArrayBuffer`s (no copy, no special headers) by default.
  **`SharedArrayBuffer`** (one memory read by page and worker while the engine computes, for example for progressive
  publications and live positions) is an optional mode: it needs cross-origin isolation (COOP/COEP headers), which a
  host must opt into; the interface reports whether it is available and works the same without it.
- **Between backend and browser:** the same columns as a binary frame. Apache Arrow IPC is the first candidate (zero-copy
  readers exist for Rust and JavaScript), weighed against its dependency size; JSON stays for control and small views.
- **Natively:** slices into the session's columns; memory-mapped files for very large graphs only if a measurement asks
  for it.

Determinism does not change: columns are filled in the same fixed order as today's objects, and output stays identical
for any thread count and on every platform.

### 5. Stability rules

- The facade crate (`arqlet`) and the npm entry (`arqlet`) export the protocol, the session and the adapters; the inner
  crates stay free to change.
- New options are additive and omitted when default (as today); enums and option structs are `#[non_exhaustive]`.
- Removal goes through `#[deprecated]` and a release note; `cargo-semver-checks` in CI once the API is meant to be
  stable.
- Every provisional part stays marked provisional until accepted.

## Steps

Each step keeps every existing test and the stored refinement fingerprints, and runs in all three deployment modes.

1. **Typed protocol.** `Request`/`Response` for the existing operations; `dispatch` goes through them; a test sends every
   request of the test suite and the playground fixtures through parse and serialize and compares the JSON.
   *Done in part (2026-11-12):* `arqlet_runtime::protocol` (re-exported as `arqlet::protocol`) types the import and every
   operation on a snapshot field by field; the read-only and tool operations (scene, trace, overview, ELK, metrics,
   refinement of parts, references) are variants that still carry their fields as a JSON object, and their answers are
   still JSON text (`Response::Encoded`). `dispatch` parses, handles and writes through it; a test compares 68 requests
   (29 answered, 39 refused) byte for byte with the dispatcher before, and one that every typed request written back
   gets the same answer. Open: the fields of those operations and typed answers.
2. **Measurement.** Time per operation split into parse, compute, serialize and transfer (native, WASM in Node, worker
   in Chrome), at 10 000, 50 000 and 100 000 nodes; results with machine and revision in `benchmarks/results/`.
   *Done (2026-11-12, [results](../../benchmarks/results/protocol-20261112.md)):* in the browser path the protocol is
   83–87 % of a scene request but 2–23 % of a toggle, whose time is layout work; large inputs move in hundreds of
   milliseconds. So step 3 (resident state) comes next; steps 5–6 wait. The worker was measured with Node's
   `worker_threads`, not in Chrome.
3. **Sessions with resident state.** `arqlet::Session` in Rust; a session store in the runtime; WASM entry points that
   keep a session (`lw_session_open`, `lw_session_run`, `lw_session_close`); `createSession` uses them when available.
   Measured against step 2.
   *Done in part (2026-11-12):* `arqlet_runtime::engine_session::EngineSession` (`arqlet::Session`), a plain value that
   keeps its snapshot; requests without `snapshot` (versions default to the session's own), answers with revision,
   layout version, collapsed containers and the scene; `load`/`export` for hand-over. WASM keeps sessions by handle
   (`lw_session_open`, `lw_session_run`, `lw_session_close`), the worker by message, and `createSession` uses them
   by default where the engine offers them (`resident: 'auto'`, since the measurement below; `false` sends the
   snapshot as before). The backend keeps sessions too (2026-11-12): `layout-command --sessions`
   is one long-running process with one JSON message per line; the playground backend serves it at `/api/session` with
   random keys, one message at a time, a time limit per message (it restarts the process; the sessions are lost and
   answered as unknown) and at most 32 sessions; `httpRun` is its `run` in JavaScript. Open: a size limit in bytes for
   the sessions; deltas instead of whole scenes. Measured
   ([results](../../benchmarks/results/protocol-resident-20261112.md)): a trace 20–30× and a scene 4–6× faster in the
   browser path, an edit with its scene 1.2–1.6× (its time is layout work).
4. **Columnar results**: positions and routes as typed arrays from a session, zero-copy in the browser; the overview
   moves onto the same buffers.
5. **Columnar input and binary frames** between backend and browser, if step 2 shows that parsing or transfer
   dominates.
6. **Optional shared memory** (`SharedArrayBuffer`) for live positions and progressive publications, behind a capability
   check.

## Consequences

- One interface for every host and language: sessions in Rust and JavaScript over one protocol; ELK stays an adapter.
- The large-graph path is planned from the start without being built before it is measured: steps 4–6 only follow a
  measurement that asks for them.
- A session store adds state with a lifetime to the runtime: sessions need closing, a size limit, and an export for
  hand-over; stale answers are still refused by revision.
- `SharedArrayBuffer` constrains hosts (cross-origin isolation); it stays optional, so the interface works without it.
- Arrow would be the first larger dependency outside the core; it is decided by measurement and recorded with source,
  version and license before it is added.

## Not decided here

The binary format (Arrow or a smaller own layout), the size limit of the session store, whether deltas or whole scenes
are returned after an edit, and when the protocol is declared stable.
