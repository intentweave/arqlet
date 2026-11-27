# Integration guide

How to put Arqlet into a product: which interface to use, where the engine runs, how a view moves between backend and
browser, and how a host stays correct while several things compute at once. Everything named here exists unless it is
marked **planned**; provisional APIs are marked so in their `.d.ts` files and may still change.

The short version: Arqlet is not only a layout function. It keeps a **view** of a graph (what is open, what is hidden,
where things are) as state with revisions, the same engine runs in a browser worker and in a backend with bit-identical
results, and results move between them as values that are accepted only while they still fit. One-shot layout engines
(ELK's model) know neither state between calls nor views of a graph ([ADR 0019](adr/0019-engine-interface.md)); that is
the difference this guide builds on.

## 1. Choose an interface

| Level | What you get | Entry |
|---|---|---|
| One shot | a graph in, positions out, every call from scratch (ELK-compatible) | `createElkLayout(run)`, operation `elkLayout` |
| Session | a view that keeps positions across changes, with revision checks | `createSession(run)` (JS), `arqlet::Session` (Rust) |
| View controller | the session plus what a user does between two scenes: one state object, history, highlight, details, search, focus, keyboard, refinement | `createViewController(session)` |
| Cluster overview | a whole graph as clusters, positions as columns | `createOverviewSession(run)`, operation `overview` |

Start with the view controller for an interactive UI; drop to the session when you need an operation the controller does
not wrap (`controller.apply(request)` reaches all of them); use the one-shot form for static exports or a drop-in for an
existing ELK integration.

```js
import {loadEngine, createSession, createViewController} from 'arqlet';

const view = createViewController(createSession(await loadEngine()));
const state = useSyncExternalStore(view.subscribe, view.getState);   // React; any framework works with subscribe/getState
await view.import(graph, {options: {levelOfDetail: {depth: 0}}});
```

### `elkLayout` as a drop-in for ELK

`elkLayout` reads a graph in ELK's JSON form and answers in that form, but it is not ELK: options without a counterpart
are reported, not silently ignored (`diagnostics`). Two things differ in what you see:

- **Spacing.** The adapter takes ELK's spacings (`elk.spacing.nodeNode`, `elk.layered.spacing.nodeNodeBetweenLayers`,
  the edge spacings) only when the graph names them; otherwise Arqlet's own defaults apply, which are wider than ELK's
  (20 between nodes and between layers). If your drawings should look like ELK's, set those spacings on the root
  explicitly. On flat graphs this alone brings the area from about 1.2–1.4 to 1.16–1.28 times elk-rs's
  ([results](../benchmarks/results/elk-flat-components-20261121.md)). The adapter does not take ELK's defaults itself:
  on the 39 benchmark datasets they cost two lines through nodes, 2.9 % more crossings and 8.6 % more bends for about
  9 % less area ([results](../benchmarks/results/elk-spacing-20261122.md)). Name them when compactness matters more.
- **Components.** As ELK does by default (`separateConnectedComponents`), components are packed as boxes in rows towards
  `elk.aspectRatio` (default 1.6) when the top level has several components with edges and none of them dominates;
  `separateConnectedComponents: false` turns it off.

## 2. Where the engine runs

The same Rust dispatcher answers every request in three places. A `run` is the function that sends a request there:

| Place | `run` | Notes |
|---|---|---|
| The page | `await loadEngine()` | simplest; long computations block the page |
| A worker | `workerRun(createEngineWorker())` | the page stays responsive; the usual choice in a browser |
| A backend | `httpRun('/api')` | the backend runs `layout-command --sessions` (one long-running process, one message per line) or links the Rust crate |
| A Node server, natively | `loadNativeEngine()` of `@arqlet/node` | provisional, unpublished: the engine in the Node process, every call on libuv's thread pool, sessions in parallel (below) |

Sessions keep their snapshot inside the engine where the engine offers it (`resident: 'auto'`): a request then names the
change, not the whole state. Without a resident session the snapshot travels with each request.

The product requirement is that all three **deployment modes** keep working:

| Mode | Import and layout | Local changes | Refinement |
|---|---|---|---|
| Browser-only | browser worker | browser worker | a second browser worker |
| Split | backend (native) | browser worker, from the backend's layout | backend |
| Backend-only | backend | backend | backend |

The playground (`apps/playground`) implements all three behind one switch; `app.js` routes every operation through one
`execute()` by kind (edit, refine, import), and its UI tests run each mode.

### A Node server: the native module (provisional)

`@arqlet/node` (ADR 0030; unpublished for now, build the module with `node packages/node/build.mjs`) runs the engine in
the Node process without WASM and without a child process. Every call that computes is an asynchronous task on libuv's
thread pool, so the event loop keeps serving requests during a layout; the answers are the same bytes as the WASM
engine's.

```js
import {loadEngine} from '@arqlet/node';
import {createSession} from '@arqlet/engine-client/session.js';

// 'native', 'wasm', or 'auto' (native where a module for this platform loads); default: ARQLET_ENGINE, else 'auto'.
const run = await loadEngine({engine: 'auto', sessionBytes: 2 ** 30});
const session = createSession(run);          // resident: the snapshot stays in the module
await session.import(graph, {limit: 2000});
await session.toggle('payments');
run.kind;                                    // 'native' or 'wasm'
```

- **Sessions in parallel, each in order.** Different sessions run at the same time on the pool's threads; the requests of
  one session run one after the other, in the order sent (the loader queues them). One `loadNativeEngine()` is one store
  of sessions; open one per client view and close it when the view goes.
- **The pool is shared.** libuv's pool has 4 threads unless `UV_THREADPOOL_SIZE` (set before the pool is first used)
  says otherwise, and `fs`, `dns.lookup`, `crypto` and `zlib` use it too: a layout holds a thread for its duration. Admit
  heavy operations (import, layout, refinement) by count, and raise the pool size to that count plus what the rest of
  the server needs (ADR 0024, item 3).
- **Memory.** `sessionBytes` limits the sessions' estimated weight together: the least recently used idle session is
  dropped beyond it and answers `Unknown session` afterwards (a running one is never dropped, so the limit is soft while
  several grow at once). `session.memory()` adds the module's live heap (`heapBytes`). Native memory goes back to the
  system allocator, so recycling is not needed to shrink, but `session.recycle(otherEngine)` still moves a session to a
  WASM engine and back.
- **Not yet:** progressive publications (`refineProgressive` answers with its final result only), prebuilt binaries.
  A crash in native code ends the process (panics are caught and answered as errors); a host that needs isolation keeps
  `layout-command --sessions` processes.

## 3. Revisions: never apply a stale result

Every snapshot carries a `revision` (the graph and the view's decisions) and a `layoutVersion` (the geometry). A change
names the versions it expects (`expectedRevision`, `expectedLayoutVersion`); the engine refuses it when they no longer
match. The session adds these for you and resolves a change that an import or reset overtook as `stale`.

The rule for a host: a result that arrives for a state the view has moved past is **discarded and reported**, never
applied silently. The view controller does this for refinement (`state.refined.stale`, counted in `state.discarded`), for
the highlight and the details (asked again after every change), and for its queued steps (`cancel()`).

## 4. Precompute in the backend, continue in the browser

Because native and WASM builds give identical results (checked on every platform, `tools/platform-parity.mjs`), a layout
computed in the backend can be continued in the browser exactly as the backend would have continued it.

**Today (split mode):**

1. The backend imports and lays out natively: the Rust crate (`arqlet::dispatch`, `arqlet::Session`), the `snapshot`
   binary, or `importGraph(run, graph, limit, options)` from Node.
2. The browser receives the snapshot and hands it to its session: `controller.adopt(snapshot, {forget: true})` (or
   `session.load(snapshot)`).
3. Local changes (open, close, page, filter, trace and reveal, drag) run in the browser worker from that layout.

A host that keeps the source graph itself can ask for lean snapshots (`omitSource` on the import): the answer leaves the
graph out and carries `sourceDigest`; later requests bring the graph as `source`, and the engine checks it against the
digest before use.

**Planned ([ADR 0023](https://github.com/intentweave/arqlet/pull/85), step H1, proposed):** the backend sends the
finished scene first and the layout state as a lean snapshot; the browser renders the scene before it has loaded the
engine, and starts the engine at the first local change, taking the structure from its replica.

### The cluster overview as a server result (provisional)

A backend may compute the cluster overview ([ADR 0018](adr/0018-cluster-overview.md)) and give the browser only the
answer.

**Edge kinds and weights.** The browser has no graph, so ask for them with `edgeData: true`. The answer then also holds,
per edge `k` (the pair `edges[2k]`, `edges[2k + 1]`):

| Field | Meaning |
|---|---|
| `edgeKinds` | the edge kinds, in order of first use |
| `edgeKind[k]` | the edge's kind, an index into `edgeKinds` |
| `edgeWeight[k]` | how many edges it stands for (`Edge.weight`); the field is left out when every edge counts 1 |

Without `edgeData` the answer is as before. With it, the gzipped answer grew by 3 to 4 % for six kinds and 8 to 9 %
with weights too, on generated code graphs of 10 000 and 100 000 nodes
([sizes](../benchmarks/results/overview-edge-data-20261126.md)). The taps of buses given as data are edges of the overview and carry their
kinds. With `columns: true`, `edgeKind` and `edgeWeight` are two more `u32` columns after `links`; `edgeKinds` stays in
the JSON. A positions-only answer leaves them out with the edges: a session answers one only while `edgeData` is the same
as in its last overview, and a whole overview when it changed.

**Continuing a changed graph, without a session.** Keep the places of an answer by id and send them with the next graph:

```js
const places = o => Object.fromEntries(Array.from(o.ids, (id, i) => [id,
  {x: o.x[i], y: o.y[i], r: o.r[i], parent: o.ids[o.parent[i]]}]));   // no parent at the top level
const next = run({op: 'overview', graph: changed, start: {nodes: places(previous)}});
next.overview.continuation;   // {continued, anew, ignored}
```

Give every node of the previous answer, **the added clusters included**, each with its `parent`. `start.nodes` excludes
`start.x`, `y` and `r` (the same graph, by node index); a request with both is refused. `settle` and `moved` (node id →
new centre `{x, y}`) work as before; a moved node needs an earlier place in its cluster. A session takes `start.nodes`
too, also together with a new graph.

What continues:

- **The added clusters stay** (`community:…`, communities and satellite groups), with their ids. Their ids are positions
  in a clustering: a changed graph clustered anew (a request without `start`) would group much of it differently and may
  give the same id to other members. With `start.nodes`, each node stays in its earlier cluster (its entry's `parent`,
  else the cluster whose earlier disc held it), so `intent` and `radii` by cluster id still name the same cluster. A cluster left with fewer than two children is
  dissolved. Only a container without earlier clusters (a new one, or one that now has more than `communities` children)
  is clustered, and what its new clusters hold is laid out anew. `communities: 0` or `satellites: 0` keeps no cluster of
  that sort.
- A node continues from its entry. It is **placed anew** when it has none, when its container has none (a new container
  and what it holds), or when it changed its container: its `parent` names another one, or, without a `parent`, its
  entry lies outside its container's earlier disc. A container that changed its parent keeps its inner arrangement.
- A node placed anew joins the cluster of the nodes it is linked to most and starts next to them (else outside what is
  there). Overlaps are then removed, which can push others; with `settle` the changed containers settle by forces.
  Without a `parent`, a node that moved to a container around its old one is not seen to have moved: it starts where it
  was and the two part.
- Entries for ids the graph no longer has are **ignored** and listed in `continuation.ignored`. Entries of clusters that
  are gone are not listed.
- Unchanged graph and options: the same answer as `start` by node index.

**Keep continuing by id.** The clusters of such an answer are not those the graph alone gives, so a later request without
a session must send `start.nodes` again; `start` by node index belongs to a request without them. A session (and
`createOverviewSession`, resident or not) does this itself for a `start` without positions.

Without `settle` the nodes move least, and gaps stay open, so the drawing grows slowly over many changes; with it the
changed containers close up and move more. Lay out without `start` when the old places and clusters no longer help.
On two generated graphs with 0.1 to 5 % of the nodes changed, nodes moved less without `settle` than in a layout without
`start`; with `settle` not in every case ([movements](../benchmarks/results/overview-continuation-20261126.md)). This is
no general guarantee.

**`parent`.** Optional, the id of the node's parent in the earlier answer (`ids[parent[i]]`). Without it the earlier
discs tell what held what, and they cannot for a cluster or container the host set smaller than its content (`radii`):
members outside the small disc leave it. With it they stay, and such an overview continues as by node index. How a named
parent is read, for a node now in container C (the top level included):

| `parent` names | The node |
|---|---|
| nothing: left out, `null`, or an id that is no node of the graph and has no entry | judged by the discs, as without it |
| C | stays, in no cluster |
| a cluster with an entry whose id belongs to C | stays, in that cluster, wherever its disc is (the cluster grows to hold it) |
| anything else: another node of the graph (itself included), a cluster of another container, a node that is gone but has an entry | changed its container: placed anew |

A cluster entry's `parent` may name its container or another cluster of that container. Anything else, and parents that
go round in a circle, count as left out. A named cluster that is not kept (its sort switched off) leaves its members to
the discs. A `parent` that is not a string is refused. Parents are taken at their word: no check against the discs.

Limits: other cluster options do not regroup a container that keeps its clusters.

## 5. Offers from another engine: refinement

A refinement improves a layout without changing the view's decisions. It can run anywhere and is applied only if the
view is still the one it started from:

```js
import {workerRefineRun, createEngineWorker, httpRun} from 'arqlet';

// A second browser worker (browser-only mode) …
const view = createViewController(session, {refine: workerRefineRun(createEngineWorker())});
await view.refine({op: 'refineProgressive'});   // intermediate results shown throttled; state.refining meanwhile

// … or the backend (split mode): any (request, {onPublication, signal}) => answer. Over HTTP there are no intermediate
// results, so ask for the whole refinement.
const backend = httpRun('/api');
const viaBackend = createViewController(session, {refine: request => backend(request)});
await viaBackend.refine({op: 'refine'});
```

Any user change, step back or import stops the refinement under way; its later results are dropped. Results are not a
step for `back`. The same acceptance rule is how ADR 0023 plans to take any precomputed view the backend offers.

## 6. Reading the graph from the view

These operations read the **source graph**, also what the view hides, and answer which visible boxes and lines carry the
result, so the host can highlight, reveal or focus it:

| Operation | Answer | Controller |
|---|---|---|
| `trace` | the paths between two nodes | `trace(from, to)`, kept as `state.highlight` |
| `neighbourhood` | the nodes within some hops | `neighbourhood(id)` |
| `details` | the containers above a node, the box that shows it, what a box holds, the edges a line stands for | `select(target)`, `state.details` |
| `find` | nodes by label or id, best first | `find(query)`, then `focus(id)` |
| `peek` | the contents of a collapsed container as a small scene | `peek(id)`, `state.peek` |

The engine's model has `id`, `kind`, `label`, `parentId` and `shape`; domain attributes stay with the host by default.
A resident session can also keep them as **content** (provisional, ADR 0023 step R1): `session.payload({set: {nodes:
{id: {owner: 'Ann'}}}})` (or `controller.payload`) stores properties beside the snapshot, never in it, with their own
`contentRevision`; `details` then answers `properties`, and `find(query, {key: 'owner'})` searches one property. Layout
never reads content, so it changes no drawing. The query layer of ADR 0021 will read the same content.

Every change of the source in a resident session, structure (`patch`) or content (`payload`), is a numbered group in the
session's change log (provisional, ADR 0023 step R2). `session.change({patch, payload})` applies both as one group, all
or nothing; `session.changesSince(n)` answers the groups after `n` (or `resync` when they are gone), so a second session
that holds the snapshot at `n` can apply them and arrive at the same state, byte for byte.

## 7. Keeping and sharing a view

`controller.bookmark()` returns the view as a value: the engine's intent (open groups, pages, filter, options), the
highlight, the selection and the digest of the graph. `encodeBookmark` turns it into URL text; `openBookmark` opens it
again as one step for `back`, and refuses the bookmark of another graph unless forced. Use it for saved views, or to hand
a browser view back to a backend session. On a large view this long form is tens of kilobytes.

**Which graph is another one.** The digest (the engine's `digest`) is of the whole source graph as the view had it, with
the shapes, directions, strategies, routing and ports it changed; the intent holds those. So a bookmark belongs to a graph
when its digest is that of the graph as it is, or of the graph with the intent's values written into it: a view that
changed them opens on a new import of the same source. Any other difference (nodes, edges and their direction, labels,
kinds, containment, weights, buses, memberships, also their order) makes another graph. A revision that differs only in
declared shapes, directions, strategies, routing or ports counts as the same graph, as for the compact form below, and the
bookmark restores its own values over the new ones. The engine answers this as `sameSource` of `readBookmark`; comparing
`digest` alone is not enough. Bookmarks written by 0.8.0 and 0.9.0 are read the same way.

### Links: the compact bookmark (provisional)

```js
const {text} = await view.compactBookmark();     // 'b1.…', base64url
location.hash = `view=${text}`;
// later, on an import of the same graph or of its next revision:
const opened = await view.openBookmark(text);    // {ok, sameSource, sameDefaults, missing, ignored}
```

Sizes for generated views are in
[`benchmarks/results/bookmarks-20261126.md`](../benchmarks/results/bookmarks-20261126.md): the text grows with what the
user changed, not with the view. With path-like ids of about 30 characters it is about 850 characters for 12 opened
modules, 1 600 for 50 and 4 000 for 200, against 18 000 to 37 000 for the long form of the same views.

**Format.** `b1.` and the base64url of a small JSON document. `b1` is the version: a reader refuses any other, and a
later format gets another number. The long form's text holds no `.`, so the two cannot be confused, and the long form
keeps working unchanged. The document's inside is private to the engine.

- **Only what differs from the defaults.** The collapsed containers are compared with those an import of this graph with
  these options starts with (the shorter of that difference and the whole list). Every other decision is compared with the
  view as it was imported, which the controller takes from the decisions it reads before the first change (`intent`). The
  defaults are unknown, and everything is kept, when the view was adopted from a snapshot that had already gone through
  changes (`layoutVersion` above 0), or when its first change was not remembered for `back` (`remember: false`, or
  `adopt` without `remember`; settling does not count). Changes made on the session beside the controller are not seen.
- **Every id once**, in a table that stores for each id only what differs from the one before; the decisions name places
  in the table. The ids travel, not positions in the graph, so another revision can say which ones it lost.
- **Two fingerprints**: of the source graph without the fields a decision writes into it (shapes, directions, strategies,
  routing, ports), and of the defaults compared with.

**What a mismatch does.** Opening never refuses a compact bookmark for what the graph lost. Every decision that fits is
restored; `missing` lists the ids that no longer exist and `ignored` the decisions left out (`{field, id}`: a missing id,
or one that no longer fits, such as a container that became a leaf). `sameSource: false` says the graph differs,
`sameDefaults: false` that the view as imported differs (another revision, other import options): the user's changes
are then applied to the new defaults, so containers and declared values the user did not touch follow the new graph. When
the whole list of collapsed containers was the shorter form (such as after opening everything), containers new in the
revision start open. Limits:

- The ids of communities (`communities`) are positions in a clustering; on another revision such an id may be gone, or
  name another community. Node ids of the graph and of given groups are stable.
- A decision that exists but cannot be laid out in the new graph (a tree strategy on what is no longer a tree) makes the
  engine refuse the restore, with its message.
- A graph that declares shapes, directions, strategies, routing or ports needs the defaults to be read (the controller
  supplies them); a controller whose view has unknown defaults refuses such a bookmark and says so. A compact bookmark
  made without defaults holds those declared values itself and restores them over a new revision's.
- There is no checksum: truncated text is refused, a hand-edited one may be read as another view.

Both engine operations are read only and go through `dispatch`, so they run in all three modes:
`{op: 'compactBookmark', snapshot, defaults?, highlight?, selection?}` answers `{text}`, and
`{op: 'readBookmark', snapshot, defaults?, text}` answers `{bookmark, sameSource, sameDefaults, missing, ignored}`, the
bookmark in the long form, for `restore`. `readBookmark` also takes a long-form `bookmark` instead of `text`:
`openBookmark` asks it when the digest differs from the graph's, and uses what it read for another graph's bookmark
opened with `force`.

`back()` and `forward()` undo and redo the view's decisions as the engine's values (`intent` and `restore`), one engine
step each, in every mode.

## 8. Concurrency

- The core is synchronous, pure and deterministic; asynchrony lives in the host.
- One interactive engine for the user's changes, another for refinement: the controller's `refine` option keeps them
  apart, and the interactive steps never wait behind a refinement.
- The controller queues its steps in order; `cancel()` drops the ones not started yet. An engine call under way finishes
  (it cannot be interrupted from outside); its result is dropped if it no longer fits.
- Output is identical for any thread count (`threads` on refinement) and on every platform.

## 9. Memory

A resident session answers what it holds (`session.memory()`: its parts in estimated heap bytes; from the WASM engine
also the live heap and the linear memory) and gives parts back (`session.release(['content'], {keys})`, `changeLog`,
`columns`, `overview`; the view stays). WebAssembly never shrinks its linear memory: after a large import or layout, the
worker keeps that high-water mark until it ends. `session.recycle(newEngine)` moves the session to a new engine (the
view, the content and the change log's number), after which the host ends the old worker; the engines give identical
results, so nothing changes in the view (provisional, ADR 0023 step R7):

```js
const next = createEngineWorker();
await session.recycle(workerRun(next));   // the session now lives in `next`
oldWorker.terminate();                    // its high-water mark goes with it
```

## 10. Checklist

- Every result you apply carries the revision it was computed for; you compare before applying.
- Every operation goes through the engine (`dispatch` or a session), so it runs in all three modes.
- Edge direction is kept in every output; cycles, self-loops and parallel edges are normal input.
- Unresolved edges are left out of the view and counted (`omittedUnresolvedEdges`); the source graph stays unchanged.
- Data you must not publish stays out of URLs: a bookmark carries ids and view decisions, not attributes.

## Further reading

- [ADR 0019](adr/0019-engine-interface.md): protocol, sessions, state kept in the engine, the columnar data path.
- [ADR 0023](https://github.com/intentweave/arqlet/pull/85) (proposed): a primary store, replicas and a change
  stream between backend and browser, and the layout hand-over.
- [Specification](specification.md), section 8: the status of every capability.
- `packages/engine-client/*.d.ts`: the JavaScript API with its provisional markers.
