# ADR 0023: A primary store, replicas and a change stream between backend and browser

Date: 2026-11-21.
Status: proposed. Docs only; no code and no capability status change.

Related: [ADR 0019](0019-engine-interface.md) (protocol, sessions, state stays where it is computed, the columnar data
path, prepared source and lean snapshots), [ADR 0021](https://github.com/intentweave/arqlet/pull/70) (proposed: a Cypher
query layer, its store seam and the shared in-memory graph question), [ADR 0022](0022-memberships-across-the-hierarchy.md)
(memberships as graph data). Requirements: the three deployment modes and the concurrency rules in `CLAUDE.md`.

## Context

Three things now pull on the same question, *where the graph lives and how the parts that read it stay in step*:

- **Queries** (ADR 0021) read relations and, in general, every property of every node and edge. The canonical graph has
  no properties (a node is `id`, `kind`, `label`, `parentId`, `shape`), so properties must come from the host, and a
  query has to run where the structure and the properties are together.
- **View features** of the view controller (request 31, merged in #75 to #83): `details`, `find`, `peek` read the
  source graph and answer ids that `ViewMap` maps onto the current view. `details` deliberately leaves domain attributes
  with the host; a query cannot.
- **The deployment modes.** Split mode today hands whole snapshots or bounded views between backend and browser; a
  session keeps its snapshot in the engine (ADR 0019 §3), and the playground backend serves resident sessions
  (`/api/session`). What is missing is a way for a browser to hold a *part* of a large graph, keep it current, and ask
  the backend only for what it does not have.

ADR 0019 already decided half of this: state stays where it is computed, the source graph is shared inside a session
(`Arc<Graph>` with its `GraphIndex`, "prepared source"), a host may keep the source itself and send a digest ("lean
snapshots"), and `patch` changes a graph without a new import. ADR 0021's last section asked how layout and query read
the same graph without copies, and measured that reading in place is two to three orders of magnitude cheaper than any
copy (`benchmarks/results/copy-cost-20261121.md`). This ADR takes that section over and answers it across the boundary
between backend and browser.

Two alternatives frame the choice:

- **Browser asks the backend for views** (whole or filtered): simple, but a view is derived and layout-specific, every
  request recomputes it, and the browser cannot compute anything the backend has not prepared.
- **Drop queries in the browser** and run them in the backend only: simpler data handling, but it gives up the
  browser-only deployment mode, which `CLAUDE.md` requires, and the executor costs nothing extra in WASM (the core is
  pure Rust and bit-identical on every platform). The limit in the browser is data volume, not code.

## Decision

The graph has **one primary store** and any number of **replicas**; they stay in step through a **change stream** with
revisions, the same model as today's host-to-layouter protocol. Every function derives its own projections from its local
store. Where a query runs is decided per query.

### 1. Two parts, each with its own revision

- **Structure**: nodes, edges, kinds, containment, buses, memberships (ADR 0022), as the canonical source graph declares
  them, **without** what the engine derives from them (the synthetic bus nodes and edges that `navigable` adds for
  `trace`, `details` and `find`, communities, groups). Primary and replica therefore agree on what a node set is; each
  side derives the rest itself (item 4). Layout, query, filter, trace and details read it. It is read in place (ADR 0021
  measurement); interned dense ids and columns follow ADR 0019 §4 when a measurement asks for them.
- **Content**: properties per node and per edge (a payload: null, boolean, `i64`, `f64`, string, list, map). Only query,
  `details`, `find` and later search read it; **layout never does**, and the canonical graph's serialized form does not
  change. Content is optional, loaded per property key, and kept beside the structure, keyed by node or edge id (by
  interned index once those exist).

**Ids the structure does not have** (a membership member, a content key) are **kept and counted**, never refused and
never dropped: they take effect when a later change adds the node or edge, as ADR 0022 already says for memberships. One
rule for memberships and content, so that replaying the change log on an old snapshot gives the same state as the
primary's.

Each part carries a revision (`structureRevision`, `contentRevision`), derived from the one sequence of the change stream
(item 3); a result names the revisions it read.

### 2. One primary, replicas with a subset

- The **primary** is where the data comes from and where changes are serialized: normally the backend (large graphs,
  later a database behind the store seam of ADR 0021); in browser-only mode the browser worker.
- A **replica** holds a subset: the structure whole or bounded, and of the content only the property keys it has needed.
  It knows what it covers (which nodes, which keys, at which revisions).
- **One writer.** The primary orders all changes. No CRDT and no merge until several concurrent writers exist; then a CRDT
  is the candidate (ADR 0021 research pass), not before.

### 3. The change stream

- **Initial sync:** a snapshot of what the replica asks for (structure, optionally content keys). JSON first; the
  columnar binary frame of ADR 0019 §4 (Arrow IPC as first candidate) when a measurement asks for it.
- **Changes:** typed and numbered in **one sequence** for structure and content, each with the sequence number it
  applies to: for the structure the operations of today's `patch` (add or remove nodes and edges, with what a removed
  node holds), for the content set and unset of properties. A change is a **group** applied atomically: a node added with
  its properties, or removed together with its content, is one group with one number, never two half-states. The
  primary keeps a bounded change log; `changesSince(sequence)` answers the groups or tells the replica to resync with a
  new snapshot when it has fallen out of the log.
- **Versions:** the stream has a protocol version of its own (ADR 0019 plans `protocolVersion` for responses; it is not
  built yet), and every snapshot names its `engineVersion` as today. A replica whose engine differs from the primary's
  does not continue a layout bit-identically by assumption: it takes a new snapshot or refuses, never computes on
  silently.
- **Upstream:** a replica sends a change as an intent with its base revision. The primary applies it or refuses it as
  stale (as `expectedRevision` does today) and publishes the confirmed change to every replica, including the sender. A
  replica never applies its own intent as confirmed before the primary has; an optimistic preview is the host's display
  concern and must be replaced by the confirmed change.
- **What the stream does not carry:** derived views, layouts and query indexes are not how the *data* is kept in step;
  each side derives them from its own store. A finished layout may still be handed over as a starting state or offered
  as a result; that is a different kind of message with its own rule (item 5). A layout's view state (collapsed,
  partial, pins: the engine's `intent`) stays a session matter as today; a bookmark (#79) carries it explicitly.

### 4. Projections are local

Each side derives what it needs from its own store: the layout session its view (as today, from `Arc<Graph>` and the kept
`GraphIndex`), the query layer its plan inputs, the filter its hidden set. A change from the stream invalidates or updates
the projections that depend on it; whether a projection is recomputed or updated incrementally is that function's
decision and is tested by "incremental equals recompute" over seeded change sequences.

### 5. Layout hand-over: render first, compute locally when needed

Two kinds of messages cross the boundary, with different rules:

- **Source changes** (structure and content) come from the primary through the stream (item 3) and are always applied in
  order.
- **View results** (a scene, a layout state) are *offered* by whoever computed them and *accepted* only while they fit:
  the same revisions and the next layout version of the state they were computed from, the rule refinement results
  already follow (#82). A result that arrives after a local change is discarded and counted, never applied silently.

On this rule, a backend can precompute and the browser can start drawing before it computes anything:

1. **Scene first.** The backend imports and lays out natively and sends the finished scene (JSON today; columnar and
   progressive, coarse first, as the cluster overview already builds, when measured). The browser renders it at once,
   without loading the WASM engine and without a local layout. For these first moments it is a pure renderer, as in
   backend-only mode.
2. **Layout state without the source.** With the scene, or right after it, the backend sends the layout state as a lean
   snapshot (`omitSource` with `sourceDigest`, ADR 0019): positions, view decisions, revisions, no graph.
3. **The engine when it is needed.** The WASM engine starts in the background. At the first local change (filter,
   trace and reveal, toggle, drag) it loads the lean snapshot and takes the structure from the replica, checked against
   the digest, and computes the change locally from the backend's layout. Native and WASM give bit-identical results
   (`platform-parity`), so continuing in the browser gives exactly what the backend would have computed. The step from
   rendering to computing locally is seamless and leaves backend-only and split mode as two points of one range.
4. **Who owns the view.** The view (open groups, pages, pins, filter) belongs to the user's session: in split mode the
   browser's. The backend supplies the starting value and later offers: a better refinement, a precomputed drill-down of
   a container that is often opened, a layout under other options. The view controller already accepts such offers: its
   `refine` option may point at the backend (`httpRun`), and a result that no longer fits is discarded (#82).
5. **Back to the backend.** A local view can be handed back as a lean snapshot or a bookmark (#79), so the backend can
   continue from it (backend-only mode, another device, a saved view).

### 6. Where a query runs

The plan IR of ADR 0021 tells, before execution, which property keys and which part of the structure a query reads.

**What covers means.** A pattern reads every edge of its kinds, so a query on a structure that is only partly there can
give a different answer without any sign of it: the same class of failure as a parser dropping a clause. Therefore:

- A partial replica holds only **closed subsets**: a container subtree together with every edge incident to it, its
  boundary known (which edges leave it, to where). A query runs locally only if it can be shown to stay inside a closed
  subset (all its patterns anchored in it, no unbounded expansion leaving it); `ViewMap` needs the same closure, since it
  walks ancestors and edges.
- **Dynamic reads** count as "every key": `RETURN n`, `keys(n)`, `properties(n)` and `n[$key]`. Such a query runs at the
  primary, or the replica loads every key first.
- Every answer says whether it is `complete` or `partial`; when in doubt the query runs at the primary.
- **The same budget in both places:** the work budget and row limit of ADR 0021 (item 11) are part of the query, so a
  local and a primary run stop at the same step. A stop is an answer (`stopped after N steps`), not a silent retry at the
  other place.

- If the local store **covers** it in this sense (and every key is loaded, the revisions are current), it runs
  **locally**: at once, also offline.
- Otherwise it runs at the **primary**, with the same executor (native instead of WASM).
- Either way the answer has the same form: ids, the revisions read, and, mapped locally, the visible parts of the current
  view (`ViewMap`, as `trace`, `details` and `find` answer them). A result for revisions the view has moved past is
  discarded, as refinement results are (#82).
- A test runs every query of the fixture corpus both ways and asserts identical answers (on top of `platform-parity`):
  the cases of `examples/cypher/cases.jsonl` (#81), executable ones with expected rows and the ones that must be
  refused.
- A host may force a place (`local`, `primary`); the default is the rule above.

### 7. The three deployment modes

| Mode | Primary | The browser holds | Queries |
|---|---|---|---|
| Browser-only | browser worker | everything | local |
| Split | backend | the backend's scene and lean layout state first; structure (whole or bounded) and content on demand | local when covered, else at the backend |
| Backend-only | backend | scenes only | at the backend |

All three keep working with one code path: the difference is where the primary is and what the replica covers. Any new
operation still goes through `dispatch`.

### 8. What stays as it is

- `dispatch`, the typed protocol, sessions and their revision checks (ADR 0019).
- Snapshot operations for hosts that hold the state themselves.
- `patch` keeps its meaning; it becomes the structure half of the change stream rather than a separate path.
- Lean snapshots (`omitSource` and `sourceDigest`) stay the way for a host that owns the source and runs no replica.

## Steps

Each step is useful on its own, behind an option or a new operation, and measured before the next. No default changes.

- **R1 — content in the engine session.** An operation `payload` (set, unset, per key, with `contentRevision`); `details`
  answers the properties of a node or edge, `find` can search a named key. *Exit:* serialized snapshots unchanged when no
  payload is loaded; unknown ids ignored and counted; memory per node with and without a payload measured and recorded.
- **R2 — a change log in the session.** `patch` and `payload` produce numbered changes; `changesSince(revision)` answers
  them or asks for a resync; the log is bounded by count and bytes. *Exit:* replaying the log on the old snapshot equals
  the new snapshot, byte for byte, over seeded sequences; stale intents refused.
- **R3 — a replica in `@arqlet/engine-client`**, started only when a measurement of R1 and R2 (or a host's need) shows
  that whole snapshots and bounded views are not enough; until then a query runs in one process. `createReplica(primary)` loads a snapshot of what it asks for and keeps
  it current from the stream (pull first; a push channel is the host's transport choice). The playground's split mode
  uses it against `/api/session`. *Exit:* the replica equals the primary after every change in the UI tests; a replica
  that falls out of the log resyncs.
- **R4 — the query executor** (ADR 0021 M1 to M4) over structure and content through the store seam.
- **R5 — placement.** Local when covered, else the primary; the same answer both ways for the whole fixture corpus.
- **R6 — columns, interned ids and a binary frame** (ADR 0019 §4) when the measurements of R1 to R3 ask for them.
- **H1 — scene first, engine on demand** (item 5), independent of R1 to R5 (it may move into an ADR of its own; neither
  blocks the other): the backend sends the scene and the lean
  layout state; the playground's split mode renders before the WASM engine has loaded and starts it at the first local
  change. *Exit:* the first local change gives the same snapshot as with the full snapshot sent up front (byte for byte);
  an offer that arrives after a local change is discarded and counted; time to first render recorded both ways.

## Measurements before defaults

Written down before the first run, recorded in `benchmarks/results/` with machine, build and revision, as the rules
require. Each question has a correctness check, and its gate is fixed before the first run (open until then):

| Question | Measured | Correctness check |
|---|---|---|
| Edit cost | applying one change against the size of the snapshot's graph | the result is identical to today's |
| Projection refresh | recompute against incremental, per function | equal byte for byte over seeded change sequences |
| Boundary crossing | serialize, transfer and parse; copy against transfer; worker against process; size and apply time of a change | the receiver sees identical content |
| Consistency | stale changes refused, sequence and revisions increase, the same output for any thread count and on every platform | equal `platform-parity` digests |
| Staleness window | from an accepted change until every replica is at the new sequence | as above |

Also measured:

- memory of a session per node, without and with content (per property key);
- size and apply time of a change against a whole snapshot, for typical edits;
- time from an accepted change to every replica at the new revision (with several replicas and coalesced edits);
- initial sync size and time, JSON against a columnar frame;
- time to the first rendered frame: the backend's scene drawn directly against the full snapshot laid out into a scene
  by the browser's engine; the sizes of the scene and the lean layout state against the full snapshot;
- query time local against at the primary for the fixture corpus, including the transfer.

## Consequences

- The browser keeps every capability; what it can do locally depends on what its replica covers, not on the architecture.
- Backend and browser keep the data in step through source changes and ids, not derived views: the smallest messages,
  one place where staleness is detected, and every function free to keep its own projection. Finished layouts still
  travel, as starting states and offers, under the acceptance rule of item 5.
- A browser can show a precomputed drawing before it has loaded the engine, and compute locally only once the user
  changes something.
- The same mechanism carries a database as the primary later (through the store seam) and several clients (one writer
  first).
- Content adds memory to a session that is already the largest consumer per node; it is optional, per key, and measured
  in R1 before anything depends on it.
- A change log needs a retention rule; a replica that falls behind pays a resync, which must stay correct and observable.
- Ids on the wire stay strings (or primary-assigned ids); interned indices are local to a process and never travel.

## Not decided here

- The transport (WebSocket, Server-Sent Events, polling) and the binary encoding: host choices (ADR 0019 left them open).
- Concurrent writers and merging.
- A database as the primary, its schema and pushdown (ADR 0021, "Not decided").
- Which functions update their projections incrementally and which recompute; each decides with the "incremental equals
  recompute" test.
- Access control on content (which keys a replica may load): a host concern, but the coverage rule of item 6 must respect
  it.
