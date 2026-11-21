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
  ([results](../benchmarks/results/elk-flat-components-20261121.md)). Whether the adapter takes ELK's defaults itself is
  open (a visual comparison is planned first).
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

Domain attributes stay with the host: the engine's model has `id`, `kind`, `label`, `parentId` and `shape`; look up the
rest by id. Properties inside the engine are planned with the query layer
([ADR 0021](https://github.com/intentweave/arqlet/pull/70), [ADR 0023](https://github.com/intentweave/arqlet/pull/85),
both proposed).

## 7. Keeping and sharing a view

`controller.bookmark()` returns the view as a value: the engine's intent (open groups, pages, filter, options), the
highlight, the selection and the digest of the graph. `encodeBookmark` turns it into URL text; `openBookmark` opens it
again as one step for `back`, and refuses the bookmark of another graph unless forced. Use it for links, saved views, or
to hand a browser view back to a backend session.

`back()` and `forward()` undo and redo the view's decisions as the engine's values (`intent` and `restore`), one engine
step each, in every mode.

## 8. Concurrency

- The core is synchronous, pure and deterministic; asynchrony lives in the host.
- One interactive engine for the user's changes, another for refinement: the controller's `refine` option keeps them
  apart, and the interactive steps never wait behind a refinement.
- The controller queues its steps in order; `cancel()` drops the ones not started yet. An engine call under way finishes
  (it cannot be interrupted from outside); its result is dropped if it no longer fits.
- Output is identical for any thread count (`threads` on refinement) and on every platform.

## 9. Checklist

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
