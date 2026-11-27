# ADR 0030: A native Node adapter: napi-rs, its own package, asynchronous on libuv's pool

Date: 2026-11-27.
Status: proposed. Step S1 of [ADR 0023](0023-primary-store-replicas-and-change-stream.md) (item 9) is built behind it,
provisional and unpublished: `crates/node`, the package `packages/node` (`@arqlet/node`, `private`), and the store usable
from several threads (`arqlet_runtime::shared_sessions`, step C2 of [ADR 0024](0024-concurrent-sessions-in-a-backend.md)).
Prebuilt binaries and publishing are designed here and not built; publishing follows in a separate issue (section 9).

Related: [ADR 0019](0019-engine-interface.md) (sessions, the columnar data path), [ADR 0023](0023-primary-store-replicas-and-change-stream.md)
(item 9: sessions as a cache, a native Node adapter; R7: memory and release), [ADR 0024](0024-concurrent-sessions-in-a-backend.md)
(parallel between sessions, serial within one). Requirements: the concurrency rules and the third-party rule in `CLAUDE.md`.

## Context

A Node host runs the engine as WASM today, usually in a `worker_threads` worker: one computation at a time per worker,
32-bit linear memory that never shrinks (ADR 0023 item 8), and a copy of every request and answer between the threads. A
backend that wants several sessions at once runs several workers (each with its own copy of the engine's memory) or
`layout-command --sessions` as a child process (serial, one process). ADR 0023 item 9 and ADR 0024 item 5 plan a native
adapter instead: the engine in the Node process, each call an asynchronous task on libuv's thread pool, one store of
sessions shared by them.

## Decision

### 1. The binding: napi-rs

Node-API (N-API) is Node's stable C interface for native modules: a module built against one Node-API version loads in
every later Node release without a rebuild, which is what makes prebuilt binaries practical. The candidates over it:

| Binding | For | Against |
|---|---|---|
| **napi-rs** (chosen) | the most used Rust binding; async work on libuv's pool (`AsyncTask`) without an async runtime; loads the Node-API symbols at run time on Windows (no `node.lib` to link); the npm convention for per-platform packages that we follow (section 3) | a proc-macro crate and a few small runtime dependencies (section 7) |
| neon | mature, also Node-API since 1.0 | its own channel and task model; fewer prebuilt-binary conventions to follow |
| node-bindgen | small | far less used and maintained |
| Node-API declared by hand (`extern "C"`, no dependency) | no third-party code | about 25 `unsafe` functions to declare and keep right, and the Windows link step (import library or delay loading) to solve ourselves; the fallback if napi-rs ever has to go |
| `layout-command --sessions` as a child process | exists, crash and memory isolation | serial within the process, every message through a pipe, a process per instance (ADR 0024's process pool) |
| WASM in `worker_threads` (today) | no native code, one binary for every platform | one computation per worker, 4 GB address space, the linear memory's high-water mark, a copy between threads |

Pinned exactly: `napi` =3.16.1 and `napi-derive` =3.7.0 (with `napi-derive-backend` 7.0.0 and `napi-sys` 3.4.1 in the
lock file), `napi-build` =2.6.0 as the build dependency; all MIT. These are the newest releases that were at least two
weeks old on 2026-11-27 (napi 3.16.3 and napi-derive 3.7.2 came out on 2026-11-23); `napi-derive-backend` is held at 7.0.0
in `Cargo.lock` because 7.0.1 emits code that needs napi 3.16.2. Features: `napi4` and `dyn-symbols` (Node-API 4 is in
every supported Node; `dyn-symbols` is the run-time symbol loading Windows needs). The npm side of napi-rs (`@napi-rs/cli`)
is not used: `packages/node/build.mjs` runs cargo and copies the library, so no npm dependency is added. Source, license
and notices are recorded in `docs/architecture.md`.

### 2. The package: `@arqlet/node`, beside `@arqlet/engine-client`

A new package, not an optional path inside `@arqlet/engine-client`:

- `@arqlet/engine-client` is for browsers first; bundlers resolve every import in it, and a native module (and the
  `node:` built-ins its loader needs) would have to be hidden from them in each bundler.
- The native module is per platform and large (section 3); a host that uses only WASM should not download it, and npm's
  optional dependencies per platform attach to the package that needs them.
- The interface stays the same: `loadNativeEngine()` answers a `run` shaped like `workerRun`'s (asynchronous, with
  `openSession()` whose sessions have `run`, `close`, `columns`, `overview`), so `createSession` and
  `createViewController` of `@arqlet/engine-client` take it as they take a worker, and `session.recycle()` moves a session
  between the native engine and a WASM engine in either direction (the answers are the same bytes).

`@arqlet/node` depends on `@arqlet/engine-client` for the WASM fallback and the types. It stays `private` and out of the
release list (`NPM_PACKAGES`) until the owner decides to publish it.

### 3. Prebuilt binaries (designed, not built)

- **Platforms:** those of `platforms.yml`: `darwin-arm64`, `linux-x64-gnu`, `linux-arm64-gnu`, `win32-x64-msvc`. Intel
  macOS and musl Linux are not prebuilt; they build from source (`node packages/node/build.mjs`) or use the WASM fallback.
- **Packages:** one per platform, `@arqlet/node-<key>` (`@arqlet/node-darwin-arm64`, ...), each with `os`, `cpu` (and on
  Linux `libc: ["glibc"]`) in its `package.json`, the module as `arqlet.node`, the two license files and a
  `THIRD_PARTY_NOTICES.md` of the crates linked into it (`tools/third-party-notices.mjs --package arqlet-node --target
  <triple>`; the crates differ by target, for example `libloading` on Windows). `@arqlet/node` lists all of them in
  `optionalDependencies` at the exact same version; npm installs only the one whose `os`, `cpu` and `libc` match.
- **Build:** a job in `release.yml` with the matrix of `platforms.yml` (the same runners): `cargo build --release -p
  arqlet-node --features query --locked` (the features of the shipped WASM engine), then `tools/platform-parity.mjs --node`
  on that runner, so a binary is published only if it answers byte for byte as WASM there; the module is uploaded as an
  artifact. The npm job downloads the four, packs the platform packages and publishes them before `@arqlet/node`, with
  trusted publishing and provenance as the other packages. A missing platform fails the release (no partial set).
- **Size:** the release build of the module (macOS arm64, `--features query`, not stripped) is 19.8 MB, against 18.7 MB
  for `layout-command` and 15.9 MB for the WASM module of the same build; a stripped or size-optimized profile for the
  packages is a decision of the release job.

### 4. How a host chooses native or WASM

`loadEngine({engine})` of `@arqlet/node`: `'native'` (an error without a native module), `'wasm'` (the WASM engine of
`@arqlet/engine-client`, on the calling thread) or `'auto'` (native where a module loads, else WASM); the default comes
from the environment variable `ARQLET_ENGINE`, else `'auto'`. The answer's `kind` says which engine was loaded. The module
is looked for at the path given, at `ARQLET_NODE_ADDON`, beside the loader (a local build), then in the platform's
package. It is loaded with `process.dlopen`, so a cargo build loads under its own file name. A browser never sees this
package.

### 5. Off the event loop

Every call that computes (`run`, a session's `run`, `columns`, `overview`) is a Node-API async work item
(`napi_create_async_work`): the request text is copied out of JavaScript on the main thread, the engine runs on a thread
of libuv's pool, and the answer text is turned into a JavaScript string on the main thread. JSON encoding and parsing
stay on the main thread, as with WASM. Opening, closing and listing sessions are short and synchronous.

libuv's pool has 4 threads unless `UV_THREADPOOL_SIZE` says otherwise, and it is shared with `fs`, `dns.lookup`, `crypto`
and `zlib`: a layout holds its thread for its whole duration. A host that runs layouts in parallel raises
`UV_THREADPOOL_SIZE` (before the pool's first use) and admits heavy operations by count or expected peak (ADR 0024 item
3); four concurrent layouts with the default pool leave `fs` calls waiting. Refinement with `threads` > 1 additionally
starts scoped threads inside the call (`parallel_map`), whose output does not depend on the number.

No async runtime or thread-pool crate is added: libuv's pool is Node's own, and napi-rs's `AsyncTask` uses it directly
(the `async`/tokio feature of napi-rs stays off).

### 6. Sessions: parallel between them, serial within one

- **One store per engine** (`new NativeEngine(sessionBytes)`, behind `loadNativeEngine`): an
  `arqlet_runtime::shared_sessions::SharedSessionStore`, the store of ADR 0024 C2. Its book (the map of handles, the
  weights, the least-recently-used order) is behind one `Mutex` held only to look up, account and choose what to drop;
  each session is an `Arc<Mutex<EngineSession>>`, and its request runs under that lock only, never under the book's. Two
  sessions therefore run at the same time on two pool threads; two requests to one session cannot race (the second waits
  for the lock). No data is shared between sessions, so each answers as in a serial run, for any number of threads.
- **Order within a session:** the lock serializes but does not order. The JavaScript loader queues a session's requests
  and sends the next only after the answer to the previous one, so they run in the order sent and a session never holds
  more than one pool thread.
- **Byte limit:** `sessionBytes` limits the sessions' estimated weight together (`EngineSession::heap_estimate`, as
  `SessionStore`). After each request the least recently used session that is idle (no request running or waiting for
  its lock) and holds something is dropped while the store is over the limit; a dropped session answers `Unknown
  session`, and the answer that caused it carries `evictedSessions`. A running session is never dropped, so the limit is
  soft while several sessions grow at once (ADR 0024 item 2).
- **Closing** a session removes it from the book; a request already running on it finishes and answers.
- **Failure:** a panic inside the engine is caught at the adapter boundary and answered as `{ok: false, error: "Engine
  panic: ..."}` (unwinding into Node would abort the process). The store catches the panic inside the session's call
  and removes that session at once, since its state may be half changed: it is no longer listed, its weight no longer
  counts toward `sessionBytes`, and later requests to it answer `Unknown session`.
- **Memory:** `memory` and `release` answer the module's live heap (`heapBytes`, counted by a wrapper around the system
  allocator, as in WASM); there is no linear memory natively, and freed memory goes back to the system allocator, so
  `recycle` is not needed to give memory back (it still works, for moving a session to another engine).

### 7. Determinism and what is checked

The adapter only moves text: the answers are those of `dispatch_streaming` and `EngineSession::run`, the same functions
as in WASM. `tools/platform-parity.mjs --node` compares every parity request and a sequence of session requests (import,
direction, toggle, scene, columns, payload, details, export, memory, overview, intent) byte for byte between the native
module and the WASM module; `platforms.yml` runs it on all four platforms. `packages/node/index.test.mjs` checks that a
timer fires during a long layout, that parallel sessions answer as serial ones, the byte limit, `memory`, the columns
against WASM, and `createSession` with `recycle` into WASM.

### 8. Accepted gaps (owner, 2026-11-27)

These are accepted for the adapter as built and documented in `packages/node/README.md`; each is revisited when a host
needs it.

- **Progressive publications are not delivered.** `refineProgressive` answers with its final result only, as the WASM
  engine does without `onPublish`. Delivering them needs a thread-safe function into JavaScript (Node-API 4) and an
  order between the publications and the final answer.
- **Phase timings are unreliable under concurrent calls.** `timings: true` reads one collector per process
  (`arqlet_model::clock`): phases of calls running at the same time land in it together, and the `take()` at the end of
  one request returns and resets what the others collected so far. A request's `timings` can therefore include another
  request's phases or miss some of its own. The layout never reads the clock, so the answers are unaffected; a host
  that measures phases runs those requests one at a time.
- **No musl Linux or Intel macOS binaries.** Those hosts build from source (`node packages/node/build.mjs`) or use the
  WASM fallback (`loadEngine({engine: 'auto'})` falls back by itself).
- No cancel flag for a running call (ADR 0024 item 6): a host drops the answer by revision, as everywhere.

### 9. Publishing is a separate step

The owner decided on 2026-11-27 that publishing is not part of the change that built the adapter: `@arqlet/node` stays
`private` and out of the release (`NPM_PACKAGES`, `release.yml`), and section 3 is the plan for a separate issue. The
license notices are ready for it: the napi-rs MIT text, which the `napi` and `napi-sys` crates do not ship, is supplied
from the upstream repository at the recorded revisions (`tools/third-party-licenses/napi-rs-LICENSE`,
`tools/third-party-notices.mjs`), and `node tools/third-party-notices.mjs --package arqlet-node --target <triple> --check`
passes for the four targets.

## Consequences

- A Node backend runs sessions in parallel in one process, with one copy of the engine and without a child process; the
  event loop stays free during a layout.
- The project ships native code per platform once published: four binaries per release, each checked against WASM on
  its own runner before publishing, and a third-party record for the binding (`docs/architecture.md`).
- A crash in native code takes the Node process with it (WASM traps and a child process do not); panics are caught, an
  abort or a stack overflow is not. A host that needs isolation keeps the process pool of ADR 0024.

## Measurements

`benchmarks/results/node-adapter-20261127.md` (Apple M3, under load from another build; single observations): per
layout the native module computes at about the speed of WASM; it is faster where a whole snapshot crosses the boundary
(`dispatch import` 1.4× at 1000 nodes, 2.2× at 3000) and where sessions overlap (four at once 2.0–2.2× faster than in
series; one WASM worker cannot overlap them). It costs memory: one 3000-node import peaks at 610 MB resident natively
against 340 MB in WASM (64-bit sizes, the system allocator), and four at once at about four times that, which is why
admission by memory (ADR 0024 item 3) matters for a native host.

## Not decided here

- When `@arqlet/node` and its platform packages are published (section 9: a separate issue; the changes to
  `release.yml`, `tools/release-check.mjs` and `docs/releasing.md` are in the plan of section 3 and the pull request).
- Transferring columns without a copy (an external buffer over the session's memory needs a lifetime rule across
  requests; the copies are made on the pool thread).
