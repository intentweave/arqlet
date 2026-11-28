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
  heavy operations (import, layout, refinement) by count and by their estimated memory
  ([section 10](#10-a-worker-pool-with-admission-by-memory-provisional)), and raise the pool size to that count plus what
  the rest of the server needs (ADR 0024, item 3).
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

**Opening a cluster in the browser, without the graph.** The drill-down into a cluster lays out a graph of that cluster:
`structuredExcerpt(graph, model, id)` builds it from the input graph. A browser that has only the answer builds it with
`structuredExcerptOf(model, id)` (or `structuredExcerpt(null, model, id)`), from `@arqlet/viewer/overview.js`:

```js
// Server: the overview with edge data; send the answer only.
const answer = run({op: 'overview', graph, edgeData: true}).overview;

// Browser: the model of the answer, the excerpt of a cluster, and a view controller on it.
const model = overviewModel(answer);
const direction = flowDirectionOf(model, model.index.get(id));   // border ports facing the neighbours
const excerpt = structuredExcerptOf(model, id, {direction});
await controller.import(excerpt.graph, {limit: excerpt.graph.nodes.length, options: excerpt.options});
```

It follows the rule of `structuredExcerpt`: the cluster is the one top-level container with everything below it (the
overview's nesting, its added clusters as containers), with its inner edges; every edge that leaves it runs to or from
one stand-in per neighbouring cluster (`border:` + its id: the outer end's ancestor that is a sibling of one of the
cluster's ancestors), in its own direction. Where both can run they give the same graph, apart from what the answer does
not carry (checked on generated graphs and on engine answers with communities, `packages/viewer/overview.test.mjs`):

| | From the graph | From the answer alone |
|---|---|---|
| edge ids | the input ids | `edge:k`, `k` the edge's index in the answer's `edges` (`EXCERPT_EDGE_PREFIX`) |
| edge kinds and weights | the input edge's | `edgeKinds[edgeKind[k]]` and `edgeWeight[k]`; without `edgeData`, kind `''` and weight 1 for every edge, and `excerpt.edgeData` is false |
| nodes | the input nodes, every field | `id`, `kind`, `label` (from `kinds`/`kind` and `labels`) and `parentId`; no `shape` or other field, so a graph that uses them lays out differently |

`overviewTrace` and `overviewNeighbourhood` answer input edge ids, which the excerpt's names cannot be mapped onto without
the ids; the answer does not carry them yet (open in [#209](https://github.com/intentweave/arqlet/issues/209)). A host
with the ids maps `edge:k` back through them (`resolvedEdgeIds(graph)[k]`).

`bundle: true` draws the leaving edges of one inner node to one stand-in, of one kind and direction, as one edge whose
`weight` is the sum of theirs, for a cluster with many leaving edges: fewer routes, each weighted. The graph then differs
from `structuredExcerpt`'s. Provisional, as the overview.

### Showing your own partition (provisional)

A host that computes communities itself (for example with Leiden on a server) gives the overview that partition as the
option `groups`: node id → path of group names, outermost first. With `communities: 0` the overview shows exactly
those groups and runs no Louvain:

```js
// Flat: one level of groups.
run({op: 'overview', graph, options: {communities: 0, groups: {
  'svc/a': ['billing'], 'svc/b': ['billing'], 'svc/c': ['search']}}});

// Nested: package → community → module (the modules are nodes of the graph).
const leiden = run({op: 'overview', graph, options: {communities: 0, groups: {
  'mod/parse': ['core', 'c7'], 'mod/lex': ['core', 'c7'], 'mod/http': ['net', 'c2']}}});
```

- **Groups are nodes of kind `group`**, placed between a node and its container: the path is read inside the node's
  container (its `parentId`), and a node without a path (or with `[]`) stays a direct child of its container. Each
  group is **labelled with its name**, the last part of its path (`overview.labels`); there is no separate label.
- **Ids are stable**: `group:<container>:<path joined by />`, with `#` for the top level (`group:#:core`,
  `group:#:core/c7`; inside container `pkg`: `group:pkg:core`). The same path gives the same id in every request and
  every revision of the graph, so `intent`, `radii` and `start` can name a group. Renaming a group changes its id. Ids in
  `groups` the graph does not have are ignored; a group id that a node of the graph already has is refused.
- **`communities` > 0** (default 120): Louvain also divides every container with more children than that, the groups
  included, inside it: in a group of 500 modules the communities are clusters `community:group:#:core:<level>.<k>`
  (kind `community`, labelled after their most connected member, `≈ <name>`). Groups with fewer children are shown as
  given. Use `communities: 0` when the partition is to be shown as it is.
- `edgeData`, `columns`, `positionsOnly`, the work per container (`ticks`, the force steps of each container's
  simulation), `radii`, `intent` and the level of detail of a `configSet` work as without `groups`. A session lays out
  again on the clusters it has while `groups` (with `communities`, `satellites`, `hubs`) stays the same.

**Switching between two partitions of the same nodes.** Send the other partition as `groups` and the places of the last
answer, with their parents, as `start.nodes` (the `places` function above). A session needs `start.nodes` too: a
`start` without positions after a change of `groups` is refused (`clusters changed`). What happens
(`a_partition_of_the_host_is_shown_as_given_and_a_switch_keeps_the_groups_both_have` in
`crates/runtime/tests/overview.rs`):

- A group whose id is in both partitions (the same container and the same path) **continues with its inner
  arrangement**: its members keep their places relative to it. The group itself may move, since its container changed
  (another group gained or lost members, so its container is laid out again around it).
- A node whose path changed has another parent and is **placed anew**, next to the nodes it is linked to, in its new
  group; so is a group that is new. They count as `anew` in `continuation`; groups count like nodes.
- The groups of the old partition that the new one lacks are **listed in `continuation.ignored`** (unlike the clusters
  the overview adds itself, which are not listed).
- Two partitions that share no path: every node is placed anew, and the answer is the same as without `start`.
- The answer is valid in every case: no overlaps, every disc inside its parent.

To switch back without moving anything, keep the places of each partition's last answer and send those: partition A
with the places of A's answer continues all of it (`anew: 0`) and nothing moves. Sent with the places of B's answer
instead, what B shares with A continues and the rest is placed anew again.

### Bounding an overview's work (provisional)

A host that bounds the time of an `overview` request need not terminate its worker and load the engine again: give the
request a **work budget** ([ADR 0018, addendum of 2026-11-27](adr/0018-cluster-overview.md)). It is deterministic: the
same budget gives the same answer on every machine, natively and in WASM, for any `threads`. A wall-clock limit inside
the engine does not exist on purpose.

```js
const answer = run({op: 'overview', graph, workBudget: 30_000_000});
answer.overview.work;   // {budget, used, clustering, limited, unclustered, packed}
```

| Field | Meaning |
|---|---|
| `workBudget` | units of work for the iterative phases: clustering (Louvain) and arranging each container (forces, separation) |
| `overBudget` | `"answer"` (default): answer what is ready; `"refuse"`: fail with an error starting `Overview work budget exhausted: ` |
| `work.used` | units of the work the answer holds, never more than `budget` |
| `work.clustering` | the part of `used` that went into communities |
| `work.limited` | the budget ran out |
| `work.unclustered` | containers left without their communities (node indices, -1: the top level) |
| `work.packed` | containers whose children are packed in rows instead of arranged by forces |
| `work.startIgnored` | only when true: a `start` by node index made for the complete clustering did not fit the cut one and was left out |
| `work.radiiIgnored` | only when not empty: ids in `radii` of added clusters the cut clustering does not have, left out |

When the budget runs out, the first container that does not fit ends the budgeted work: what is not split yet stays
unsplit, what is not arranged yet is packed. The answer is valid all the same: every node has its disc, no discs of one
container overlap, children lie inside their parent's disc, edges keep their direction. An unchanged container of a
continuation keeps its arrangement within any budget. Without `workBudget` the answer is exactly as before and has no
`work`. `work` also comes with `positionsOnly` and `columns` answers; `createOverviewSession` passes both fields on.

Choose `"refuse"` when the host would rather show the last overview: a refused request changes nothing in a session.

Know these limits:

- **Clustering comes first.** A large clustering (a big flat graph split into communities) can use up the budget, and
  then every container is packed.
- **Packed stays packed.** A continuation (`start`) keeps an unchanged packed container as it is, like any unchanged
  arrangement, until the host lays out without `start`.
- **Threads.** With threads a level may take up to about twice its remaining budget in wall time (a container stopped
  by the shared counter is arranged again alone). On one thread and in WASM the work exceeds the budget by at most one
  step (one tick, round or sweep of one container).
- **Drags** (`start.moved`) are applied in a container kept as it was when the moved node overlaps nothing there;
  otherwise that container is packed and listed.
- A continuation by index made for the complete clustering does not fit an answer whose clustering the budget cut:
  with `"answer"` the start is left out (`startIgnored`), as are `radii` of clusters that are missing (`radiiIgnored`);
  with `"refuse"` the request is refused with the prefix above.
An answer with `unclustered` containers has other nodes than the complete overview: continuing it by node index (and
with `start: {}` in a session) is refused once the clustering completes (the number of values differs); lay out without
`start` then. By id (`start.nodes`) it continues, and those containers are clustered as new ones.

**Choosing a budget.** The units are not time; calibrate them on your machine:

1. Lay out graphs of the sizes you serve with `workBudget: Number.MAX_SAFE_INTEGER` and time each request:
   `work.used` is the work of the whole overview.
2. Time the same requests with `workBudget: 0`: this is the part the budget does not bound (reading the graph, the
   hierarchy, packing, the answer); it grows with the graph's size.
3. Units per second = `used / (time − time at budget 0)`. The budget for a time limit `T` is about
   `(T − time at budget 0 − margin) × units per second`.

On the generated code indexes of `benchmarks/overview/generate.py`, one Apple M3 under load, the native engine and the
WASM engine in Node both spent about 60 million units per second on one thread (157 to 206 million with 4 native
threads); the whole overview took about 4.4 million units per 1 000 nodes, and the unbudgeted part 52 to 64 ms at 10 000
nodes and 0.5 to 0.7 s at 100 000 ([results](../benchmarks/results/overview-work-budget-20261127.md),
`benchmarks/overview/work-rate.mjs`). These figures come from a **loaded** machine (load average 4.4 to 6.0 during the
runs, recorded per row); other machines, loads and graphs differ: measure your own.

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
- To bound an engine call without terminating the worker, give it a deterministic work budget: `workBudget` on
  `refine` (jobs) and on `overview` (units, [above](#bounding-an-overviews-work-provisional)).
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

## 10. A worker pool with admission by memory (provisional)

A server that runs several engines at once (WASM workers, `layout-command --sessions` processes, later the native Node
adapter) has to decide how many requests fit into memory together and when one waits. The number of cores is the wrong
bound: the peak of one large layout is hundreds of megabytes ([ADR 0024](adr/0024-concurrent-sessions-in-a-backend.md),
item 3). The engine estimates a request's peak before it runs:

```js
const answer = run({op: 'estimate', request: {op: 'import', graph, limit: 100, options}});
answer.estimate;   // {op, kind, target, bytes, parts: {request, input, structure, layout, content, floor}, view, ...}
await session.estimate({op: 'toggle', nodeId});   // a resident session estimates on its own view
```

`estimate` takes the request exactly as it would be sent (to `dispatch` with its snapshot, or to a session without it),
reads it into its types, finds the view the operation will lay out the way the operation finds it (an import's selection
and projection, the view after opening a container) and adds up the parts with coefficients calibrated per target, at
least a structural floor for reading the request and handling the whole source graph. It lays nothing out and changes
nothing; the same request gives the same answer. It does not validate the request: a stale revision or an unknown node
is estimated all the same, and the operation refuses it afterwards.

**A calibrated estimate, not a guarantee** ([results](../benchmarks/results/memory-estimate-20261127.md); one Apple M3,
reference datasets and generated graphs up to 114k nodes):

- On every calibration row the estimate came out at or above the measured peak (natively the counted heap, in WASM
  the linear memory a fresh worker grew to). That includes a margin of 1.15.
- Above 4 MB the median for imports, edits and overviews was 1.15 to 2.0 times the peak natively and 1.16 to 1.46
  in WASM. The maximum was 3.8 to 10.7 natively and 1.5 to 3.2 in WASM. Refinements are estimated far higher, at
  one layout per thread.
- On graph shapes left out of the fit it can fall below the peak. With all code indexes left out, natively the
  dispatch imports came to 0.89 of the peak. With the full layouts of 3 300 nodes left out, it came to 0.36
  natively and 0.13 in WASM.
- Natively the resident memory can be up to 3 times the counted heap it estimates (macOS's system allocator).
- Check it against your own graphs before you rely on it. The WASM engine's `memory()` answers `heapPeakBytes`,
  the counted peak since the previous `memory`.

**Recalibrate whenever layout code changes.** The coefficients stand for the memory the layout code uses today. Any
change to layout code can raise the peaks, even one that keeps the output the same. Measure again and refit with
`benchmarks/memory-estimate/fit.py`; the steps are in
[`benchmarks/memory-estimate/README.md`](../benchmarks/memory-estimate/README.md). Do the same when your graphs have a
shape the calibration lacks.

Know these limits:

- **Per target.** Type sizes differ between 64-bit builds and wasm32, so each build estimates for itself (`target`).
  Ask a WASM engine about a WASM worker. In WASM the estimate stands for the growth of a fresh worker's linear memory,
  the memory a worker keeps since WebAssembly never gives it back. Natively it stands for the counted heap, and the
  resident memory of the process is larger by the allocator's overhead (up to 3 times on macOS).
- **Not a forecast.** An edit that continues the earlier layout (`direction`, a drag) usually needs less than the
  from-scratch layout the estimate assumes. A refinement is counted as one layout per thread (`jobs`).
- **What it refuses.** An operation the estimate does not know answers `{ok: false, heavy}`. With `heavy: true` it
  lays out or refines (`elkLayout`, `structuredRefine`, `containerRefine`, `componentRefine`, the reference
  operations, `load`, a session's later overview without its graph); run it alone. With `heavy: false` it is a read
  (`scene`, `details`, `find`, `trace`, `query`, …), light enough to run beside the rest. Never treat a refusal as
  0 bytes. A session's `change` is estimated like a `patch`, plus its payload's content.
- **The estimate itself costs** one reading of the request:
  - an import of a 114k-node graph: 1.8 to 2 s and 90 MB of its own, against 45 to 59 s and 554 MB for the import;
  - a toggle through `dispatch` with a 38 MB snapshot: 0.7 s;
  - an edit in a resident session: milliseconds;
  - an import whose first view is small: almost as much as the import (code-100000: 1.0 s against 1.1 s).

  Run it where the request would run, or in a light worker of its own, and count that worker's memory too.

A pool built on it, in the order a request goes through it (a runnable Node example:
[`examples/worker-pool`](../examples/worker-pool/README.md)):

1. **Estimate, then queue.** Each request gets its estimate and waits, first come first served, until the sum of the
   estimates of what runs plus its own fits the budget. A request larger than the whole budget runs alone when nothing
   else runs, so it is never starved, and so does a request refused as `heavy`. Light requests (refused with `heavy:
   false`: reads, `memory`, `changesSince`) bypass the budget. Cap the queue and answer "busy" beyond it
   (backpressure). Keep headroom in the budget: the estimate is calibrated, not a guarantee.
2. **Within a session, in order.** A session's requests run one after the other on the worker that holds it (ADR 0024
   item 1); only a request that a newer one replaces completely (a drag position, a refinement) may be dropped from the
   queue before it runs. Changes of the graph (`patch`, `change`, `payload`) are never dropped.
3. **Stale answers are refused by revision** (section 3): an answer for a revision the view has moved past is
   discarded, never applied; a queued edit that names an old `expectedRevision` is refused by the engine.
4. **Overviews within a work budget.** Give `overview` a `workBudget` ([above](#bounding-an-overviews-work-provisional))
   so its time is bounded without terminating the worker; its memory is estimated like any request.
5. **Recycle a grown worker.** A WASM worker keeps its high-water mark. When its linear memory (`memory()` answers
   `linearMemoryBytes`) is far above what its sessions hold, move them with `session.recycle(newEngine)` and end the old
   worker; a stateless worker is simply replaced. `heapPeakBytes` in the same answer is the counted peak since the
   previous `memory`, for checking the estimate against your own requests.
6. **On a timeout, terminate.** The engine cannot be interrupted from outside. Terminate the worker (or the
   `layout-command` process) that exceeded its time limit, answer the request as timed out, start a new worker, and let
   the sessions it held reopen from their last snapshot. Prefer a work budget (`workBudget` on `overview` and `refine`)
   where the operation has one: it ends in an answer, not in a lost worker.
   Replace a worker after any request that threw (a trap) or when it dies: its memory may be in any state.

**Native or WASM workers.** WASM workers are isolated (a worker that dies takes only its sessions), give identical
answers on every platform, and keep their high-water mark until they end. Native engines (`layout-command`, the Rust
crate, the [Node module](#a-node-server-the-native-module-provisional)) give freed memory back to the allocator and run
sessions in parallel threads, but a crash takes the whole process, and their resident memory is larger than the counted
heap the estimate stands for (up to 3 times on macOS's allocator in the measurement). Budget each kind with its own estimate:
a native engine answers `native64`, a WASM engine `wasm32`.

## 11. Checklist

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
