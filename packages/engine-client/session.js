// A layout session (provisional): one graph, its current snapshot, and the changes sent to the engine as operations on
// it, so that unchanged parts keep their place. Pure: no DOM, no fetch; `run` is the engine (`loadEngine`) or a worker
// (`workerRun`).
//
// The engine refuses a change to an older snapshot ("Stale view request"); the session adds the snapshot's revision and
// layout version to every change, runs changes one after the other on the newest snapshot, and never applies an answer
// that was overtaken: after a new `import`, or `reset`, a change still under way resolves to `{ok: false, stale: true}`.

import {columnsOfScene} from './columns.js';

/**
 * Wraps a worker of `createEngineWorker` as an asynchronous `run`: each request gets an id, and the answer with that id
 * resolves it. Progressive publications are passed to `onPublication(requestId, message)`.
 */
/**
 * Provisional: a refinement engine for `createViewController({refine})` on its own worker (`createEngineWorker`), so that
 * refinement never waits behind the interactive steps: `(request, {onPublication, signal}) => Promise<answer>`, with the
 * intermediate results of a progressive request (`refineProgressive`) handed to that request's `onPublication`. An
 * aborted request resolves at once as cancelled; the worker finishes it in the background and its answer is dropped.
 */
export function workerRefineRun(worker) {
  let next = 0;
  const waiting = new Map();
  worker.addEventListener('message', ({data}) => {
    const pending = waiting.get(data?.requestId);
    if (!pending) return;
    if (data.publication !== undefined) { pending.onPublication?.(data.publication); return; }
    waiting.delete(data.requestId);
    const {requestId, ...response} = data;
    pending.resolve(response);
  });
  return (request, {onPublication, signal} = {}) => new Promise(resolve => {
    const requestId = ++next;
    if (signal?.aborted) { resolve({ok: false, cancelled: true}); return; }
    waiting.set(requestId, {resolve, onPublication});
    signal?.addEventListener('abort', () => {
      if (waiting.delete(requestId)) resolve({ok: false, cancelled: true});
    }, {once: true});
    worker.postMessage({requestId, request});
  });
}

export function workerRun(worker, {onPublication} = {}) {
  let next = 0;
  const waiting = new Map();
  worker.addEventListener('message', ({data}) => {
    const pending = waiting.get(data?.requestId);
    if (!pending) return;
    if (data.publication !== undefined) { onPublication?.(data.requestId, data); return; }
    waiting.delete(data.requestId);
    const {requestId, ...response} = data;
    pending(response);
  });
  const post = message => new Promise(resolve => {
    const requestId = ++next;
    waiting.set(requestId, resolve);
    worker.postMessage({requestId, ...message});
  });
  const run = request => post({request});
  // Provisional (ADR 0019, step 3): a session kept in the worker; its requests leave the snapshot out.
  run.openSession = async () => {
    const opened = await post({session: 'open'});
    if (!opened.ok) throw Error(opened.error);
    return {
      run: request => post({session: opened.session, request}),
      close: () => post({session: opened.session, close: true}),
      // Provisional (ADR 0019, step 4): the columns arrive as transferred buffers, copied once in the worker.
      columns: async () => {
        const answer = await post({session: opened.session, columns: true});
        if (!answer.ok) throw Error(answer.error);
        return answer.columns;
      },
      // Provisional: an overview request answered with typed arrays (see `createEngine`'s `overview`).
      overview: request => post({session: opened.session, request, overview: true}),
    };
  };
  return run;
}

/** The most nodes an import selects for its view (the engine's `MAX_VIEW_LIMIT`); the default of `import`'s limit. */
export const MAX_VIEW_LIMIT = 10000;

/**
 * Provisional (ADR 0019): a backend as an asynchronous `run`. Requests go to `${base}/op` (one native computation each);
 * `openSession()` keeps a session in the backend (`${base}/session`), whose requests leave the snapshot out. `fetchImpl`
 * replaces `fetch` (tests, other transports). The playground's backend (`apps/playground/server.py`) answers both.
 */
export function httpRun(base = '/api', {fetchImpl = globalThis.fetch} = {}) {
  const post = async (path, body) => {
    const response = await fetchImpl(`${base}/${path}`, {
      method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body),
    });
    try { return await response.json(); } catch { return {ok: false, error: `HTTP ${response.status}`}; }
  };
  const run = request => post('op', request);
  run.openSession = async () => {
    const opened = await post('session', {session: 'open'});
    if (!opened.ok) throw Error(opened.error);
    return {
      run: request => post('session', {session: opened.session, request}),
      close: () => post('session', {session: opened.session, close: true}),
    };
  };
  return run;
}

/**
 * Creates a session on `run` (synchronous or asynchronous). `onChange(snapshot)` is called after every applied change.
 *
 * Where the engine can keep a session (`run.openSession`, which `loadEngine` and `workerRun` provide), the snapshot
 * stays in the engine (ADR 0019): requests carry only the change, answers bring the scene, and `snapshot` is the
 * session's state (`revision`, `layoutVersion`, `collapsedIds`); `exportSnapshot()` gives the whole snapshot. `resident`
 * chooses: `'auto'` (default) where the engine can, `true` always (an error without it), `false` never (every request
 * sends the snapshot, as before 0.4).
 */
let dragCounter = 0;

export function createSession(engine, {onChange, resident: wanted = 'auto'} = {}) {
  // The engine and, when resident, its session: replaced by `recycle` (a new worker, ADR 0023 R7).
  let run = engine;
  let snapshot = null;
  let generation = 0;
  // Snapshots kept by mark when the session is not resident (#148); a resident session keeps them in the engine.
  let kept = [];
  let marks = 0;
  let queue = Promise.resolve();
  const scenes = new WeakMap();
  if (wanted === true && !run.openSession) throw Error('This engine has no resident sessions');
  const resident = wanted === true || (wanted === 'auto' && typeof run.openSession === 'function');
  let opened = resident ? Promise.resolve(run.openSession()) : null;
  // The engine call: with a resident session, the request goes to it and its answer is turned into the shape of an
  // answer with a snapshot (the state), its scene remembered for that state.
  const call = async request => {
    if (!resident) return run(request);
    const {snapshot: _sent, ...rest} = request;
    const answer = await (await opened).run(rest);
    if (!answer.ok || answer.revision === undefined) return answer;
    const state = {revision: answer.revision, layoutVersion: answer.layoutVersion, collapsedIds: answer.collapsedIds};
    if (answer.scene) scenes.set(state, answer.scene);
    // The change log's number and the content's revision of a source change (ADR 0023, R1 and R2), when the answer has them.
    return {ok: true, snapshot: state, ...(answer.sequence !== undefined ? {sequence: answer.sequence} : {}),
      ...(answer.contentRevision !== undefined ? {contentRevision: answer.contentRevision} : {})};
  };

  // Runs `step` after every change before it; its answer counts only if no import or reset came in between.
  const enqueue = step => {
    const mine = generation;
    const result = queue.then(async () => {
      if (mine !== generation) return {ok: false, stale: true, error: 'The session was reset.'};
      const answer = await step();
      if (mine !== generation) return {ok: false, stale: true, error: 'The session was reset.'};
      if (answer?.ok && answer.snapshot && !answer.unchanged) {
        snapshot = answer.snapshot;
        onChange?.(snapshot);
      }
      return answer;
    });
    queue = result.catch(() => {});
    return result;
  };

  // A change to the current snapshot; called inside a queued step.
  const send = request => {
    if (!snapshot) return {ok: false, error: 'Nothing imported yet.'};
    return call({
      ...request,
      snapshot,
      expectedRevision: snapshot.revision,
      expectedLayoutVersion: snapshot.layoutVersion,
    });
  };
  const change = request => enqueue(() => send(request));

  const collapsed = id => (snapshot?.collapsedIds ?? []).includes(id);
  // Toggles `id` only when it is not yet in the wanted state, decided and sent in one step.
  const ensure = (id, wantCollapsed) => enqueue(() => (collapsed(id) === wantCollapsed
    ? {ok: true, snapshot, unchanged: true}
    : send({op: 'toggle', nodeId: id})));

  return {
    /** The current snapshot, or null before the first import. */
    get snapshot() { return snapshot; },

    /**
     * Lays out `graph` (nodes with `parentId` are containers) and starts the session on it. `limit` is the most nodes
     * the first view selects (default: all, at most `MAX_VIEW_LIMIT`; containers beyond it start collapsed); `options` are layout options (`docs/configuration.md`).
     * Any change still under way is dropped.
     */
    import(graph, {limit = Math.min(graph.nodes.length, MAX_VIEW_LIMIT), options} = {}) {
      // A new view does not wait for the changes still under way: they resolve as stale.
      generation++;
      queue = Promise.resolve();
      kept = [];
      return enqueue(() => call({op: 'import', graph, limit, ...(options ? {options} : {})}));
    },

    /**
     * Provisional: continues from `snapshot` (a host made it: a drag, a refinement, a document it loaded), after the changes
     * queued before; with a resident session the engine takes it over (`load`).
     */
    load(snapshot, {reset = false} = {}) {
      // With `reset` (another view) it does not wait for the changes still under way: they resolve as stale.
      // The same graph keeps the snapshots kept for `recall` (#148); another one (`reset`) drops them.
      if (reset) {
        generation++;
        queue = Promise.resolve();
        kept = [];
      }
      return enqueue(() => (resident ? call({op: 'load', snapshot, keep: !reset}) : {ok: true, snapshot}));
    },

    /**
     * Provisional (#148): keeps the current snapshot under a mark, `{ok, mark}` (the same mark while it is unchanged), at
     * most `limit` (default 32) of them, the oldest dropped first. With a resident session the engine keeps them (only the
     * mark crosses); otherwise this session does. A new import drops them.
     */
    keep({limit = 32} = {}) {
      return enqueue(async () => {
        if (!snapshot) return {ok: false, error: 'Nothing imported yet.'};
        if (resident) return call({op: 'keep', limit});
        const last = kept.at(-1);
        if (last && last.snapshot === snapshot) return {ok: true, mark: last.mark};
        kept.push({mark: ++marks, snapshot});
        if (kept.length > limit) kept.splice(0, kept.length - limit);
        return {ok: true, mark: marks};
      });
    },

    /**
     * Provisional (#148): makes the snapshot kept under `mark` current again (its geometry as it was) under the next layout
     * version. `{ok: false, evicted: true}` when it is no longer kept, `{ok: false, changed: true}` when the graph changed
     * since (another revision); then lay out again from the decisions (`restore`).
     */
    recall(mark) {
      return enqueue(async () => {
        if (!snapshot) return {ok: false, error: 'Nothing imported yet.'};
        if (resident) return call({op: 'recall', mark});
        const entry = kept.find(k => k.mark === mark);
        if (!entry) return {ok: false, evicted: true, error: `The snapshot of mark ${mark} is no longer kept`};
        if (entry.snapshot.revision !== snapshot.revision) {
          return {ok: false, changed: true, error: `The graph changed since mark ${mark}`};
        }
        return {ok: true, snapshot: {...entry.snapshot, layoutVersion: snapshot.layoutVersion + 1}};
      });
    },

    /** Drops the snapshot; changes still under way resolve as stale. */
    reset() {
      generation++;
      snapshot = null;
      kept = [];
    },

    /** Whether container `id` is collapsed in the current snapshot. */
    isCollapsed: collapsed,

    /** Collapses or expands container `id`. */
    toggle(id) { return change({op: 'toggle', nodeId: id}); },

    /** Expands container `id`; `{ok: true, unchanged: true}` when it is open already. */
    expand(id) { return ensure(id, false); },

    /** Collapses container `id`; `{ok: true, unchanged: true}` when it is collapsed already. */
    collapse(id) { return ensure(id, true); },

    /**
     * Any other change of the engine's protocol, for example `{op: 'direction', nodeId, direction: 'tb'}`,
     * `{op: 'configure', options}` or `{op: 'add', sourceId, label}`. The snapshot and the expected versions are added.
     */
    apply(request) { return change(request); },

    /**
     * Provisional (path cut-out): the paths from `from` to `to` in the current snapshot (read only, nothing changes):
     * `{ok, found, reversed, length, nodes, edges, visible: {nodes, routes}, distance}`. `mode`: `shortest` (default) or
     * `all`. `edgeKinds`: follow only edges of these kinds (default: every edge).
     */
    async trace(from, to, {mode = 'shortest', edgeKinds} = {}) {
      if (!snapshot) return {ok: false, error: 'Nothing imported yet.'};
      return call({op: 'trace', snapshot, from, to, mode, ...(edgeKinds ? {edgeKinds} : {})});
    },

    /**
     * Provisional: the nodes within `hops` edges (default 1) of `nodeId` (a node id or a list), read only:
     * `{ok, nodes, edges, hops, visible: {nodes, routes}, distance}` as for `trace`; `nodes` can go to `reveal`.
     * `direction`: `out`, `in` or `both` (default); `edgeKinds`: follow only edges of these kinds.
     */
    /**
     * Provisional (search, read only): the nodes of the source graph whose label or id contains `query` (ignoring case),
     * best first, with the containers above each and the box that shows it (also inside collapsed groups); at most
     * `limit` (default 20), only `nodeKinds` if given; `total` counts them all.
     */
    /**
     * Provisional (ADR 0023, R1): sets, unsets or clears properties of nodes and edges in the session's content, as one
     * change: `{set: {nodes: {id: {key: value}}, edges: {…}}, unset: {nodes: {id: [key]}}, clear, expectedContentRevision}`.
     * Kept beside the snapshot (never in it; layout never reads it); `details` answers the properties, `find` with `key`
     * searches one. Needs a resident session; a new import starts the content empty.
     */
    payload(change = {}) {
      if (!resident) return Promise.resolve({ok: false, error: 'payload needs a resident session'});
      return enqueue(() => call({op: 'payload', ...change}));
    },

    /**
     * Provisional (ADR 0023, R2): a change of structure and content as one group, all of it or nothing:
     * `{patch: {nodes, edges, removeNodes, removeEdges, memberships}, payload: {set, unset, clear}, expectedSequence}`.
     * The answer carries the group's `sequence` in the session's change log. Needs a resident session.
     */
    change(group = {}) {
      if (!resident) return Promise.resolve({ok: false, error: 'change needs a resident session'});
      return enqueue(() => call({op: 'change', ...group}));
    },

    /**
     * Provisional (ADR 0023, R2, read only): the change groups after `sequence` (`{sequence, changes}`), or `{resync: true}`
     * when some of them are no longer in the log (then take a new snapshot and the `content`).
     */
    async changesSince(sequence) {
      if (!resident) return {ok: false, error: 'changesSince needs a resident session'};
      return call({op: 'changesSince', sequence});
    },

    /**
     * Provisional (ADR 0023, R7, read only): what the session holds, part by part, in estimated heap bytes (`parts`,
     * `total`, `perNode`); from the WASM engine also the module's live heap (`heapBytes`) and its linear memory
     * (`linearMemoryBytes`), which WebAssembly never gives back (only a new worker does).
     */
    async memory() {
      if (!resident) return {ok: false, error: 'memory needs a resident session'};
      return call({op: 'memory'});
    },

    /**
     * Provisional (ADR 0023, R7): gives parts back: `content` (all, or only `keys`), `changeLog` (a replica behind it then
     * resyncs), `columns`, `overview`. The snapshot stays; the answer is `memory` after it, with `released`.
     */
    release(parts, {keys} = {}) {
      if (!resident) return Promise.resolve({ok: false, error: 'release needs a resident session'});
      return enqueue(() => call({op: 'release', parts, ...(keys ? {keys} : {})}));
    },

    /** Provisional (ADR 0023, R2, read only): the whole content and the log's sequence, for a replica that resyncs. */
    async content() {
      if (!resident) return {ok: false, error: 'content needs a resident session'};
      return call({op: 'content'});
    },

    /**
     * Provisional (read only): a look into the collapsed container `nodeId` without opening it: its contents laid out on
     * their own as a small scene (the containers inside collapsed, at most `limit` direct children, default 50), and per
     * child shown the edges that cross the container's border (`external`).
     */
    async peek(nodeId, {limit} = {}) {
      if (!snapshot) return {ok: false, error: 'Nothing imported yet.'};
      return call({op: 'peek', snapshot, nodeId, ...(limit !== undefined ? {limit} : {})});
    },

    async find(query, {limit, nodeKinds, key} = {}) {
      if (!snapshot) return {ok: false, error: 'Nothing imported yet.'};
      return call({op: 'find', snapshot, query, ...(limit !== undefined ? {limit} : {}), ...(nodeKinds ? {nodeKinds} : {}),
        ...(key ? {key} : {})});
    },

    /**
     * Provisional (ADR 0021, M5b; read only, needs an engine built with the cargo feature `query`): an openCypher query over
     * the session's source graph and its content (`payload`): `{ok, columns, rows, rowCount, nodes, edges, graph, ...}`.
     * Nodes and edges in `rows` are `{node: id}` and `{edge: id}`; `nodes` and `edges` list the ids once each. `params`
     * are the `$name` values; `maxSteps`, `maxRows` and `maxHops` lower or raise the execution limits; with `view: true`
     * the answer also has `shown`, the box that shows each node and the line that carries each edge. A failure is
     * `{ok: false, error: 'query <class>: ...'}` with the class `syntax`, `unsupported`, `runtime` or `limit`. A query
     * sees the source graph the engine kept (for a bounded import the selection; `graph` gives its size).
     */
    async query(text, {params, view, maxSteps, maxRows, maxHops} = {}) {
      if (!snapshot) return {ok: false, error: 'Nothing imported yet.'};
      return call({op: 'query', snapshot, query: text, ...(params ? {params} : {}), ...(view ? {view: true} : {}),
        ...(maxSteps !== undefined ? {maxSteps} : {}), ...(maxRows !== undefined ? {maxRows} : {}),
        ...(maxHops !== undefined ? {maxHops} : {})});
    },

    /**
     * Provisional (view details, read only): what the engine knows about a node (`{nodeId}`) or an edge (`{edgeId}`, a
     * drawn line or an edge of the source graph): the containers above a node and the box that shows it, what a collapsed
     * box holds, the edges a line stands for in either direction (at most `limit` ids each, counts complete).
     */
    async details(target, {limit} = {}) {
      if (!snapshot) return {ok: false, error: 'Nothing imported yet.'};
      return call({op: 'details', snapshot, ...target, ...(limit !== undefined ? {limit} : {})});
    },

    async neighbourhood(nodeId, {hops, direction, edgeKinds} = {}) {
      if (!snapshot) return {ok: false, error: 'Nothing imported yet.'};
      return call({op: 'neighbourhood', snapshot, ...(Array.isArray(nodeId) ? {nodeIds: nodeId} : {nodeId}),
        ...(hops !== undefined ? {hops} : {}), ...(direction ? {direction} : {}), ...(edgeKinds ? {edgeKinds} : {})});
    },

    /**
     * Provisional: changes the graph without a new import: `{nodes, edges, removeNodes, removeEdges}` (removals first; a
     * removed node takes what it holds and its edges along). The view keeps its state; with a level of detail,
     * communities or groups it is laid out as a toggle lays it out, otherwise again from where it was.
     */
    patch(patch) { return change({op: 'patch', patch}); },

    /**
     * Provisional (drag API): pins `nodeId` at `position` (in its parent's frame, as `toReactFlow` gives `position`), or
     * where it is now; the layout keeps it there.
     */
    pin(nodeId, position) { return change({op: 'pin', nodeId, ...(position ? {position} : {})}); },

    /** Provisional (drag API): releases the pin of `nodeId`; the layout may move it again. */
    unpin(nodeId) { return change({op: 'pin', nodeId, unpin: true}); },

    /**
     * Provisional (drag API): starts moving leaf `nodeId` to `position` (in its parent's frame). `others` maps further
     * nodes moved with it (a selection) to their positions; the set is fixed for the gesture. Returns a handle:
     * `move(position, others?)` sends a newer position (positions that arrive while one is computed replace each other, so
     * only the newest is sent); `drop({release, settle})` applies the last position and ends the gesture, keeping the node
     * pinned there (or releasing the pin with `release`), then with `settle` lets force-directed areas settle; `cancel()`
     * puts the node back. Every applied step changes the session's snapshot (`onChange`), so a host redraws from
     * `scene()`. Other changes wait until the gesture ends.
     */
    drag(nodeId, position, {others = {}} = {}) {
      const id = `drag-${++dragCounter}`;
      let seq = 0, pending = null, inFlight = null, ended = false, refused = null;
      const stats = {sent: 0, coalesced: 0, applied: 0};
      const step = (phase, at, release = false) => {
        seq += 1;
        stats.sent += 1;
        return enqueue(() => send({op: 'move', gestureId: id, seq, phase, node: nodeId, position: at, release,
          ...(Object.keys(others).length ? {others} : {})})).then(answer => {
          if (answer.ok) stats.applied += 1; else refused ??= answer;
          return answer;
        });
      };
      // One step at a time; the newest pending position goes next.
      const pump = () => {
        if (inFlight || !pending || refused) return inFlight;
        const at = pending;
        pending = null;
        inFlight = step('update', at).finally(() => { inFlight = null; pump(); });
        return inFlight;
      };
      const begun = step('begin', position);
      const end = async (phase, release, settle) => {
        if (ended) return {ok: false, error: 'The gesture has ended.'};
        ended = true;
        await begun;
        while (inFlight) await inFlight;
        if (phase === 'cancel') pending = null;
        if (pending && !refused) { const at = pending; pending = null; await step('update', at); }
        if (refused) return refused;
        const answer = await step(phase, null, release);
        if (answer.ok && settle) return change({op: 'settle'});
        return answer;
      };
      return {
        /** The node being moved. */
        node: nodeId,
        /** Whether the gesture still takes positions. */
        get active() { return !ended && !refused; },
        /** Steps sent, positions replaced by newer ones before they were sent, and steps applied. */
        get stats() { return {...stats}; },
        /** The answer of the first step (the engine's view after the node was picked up). */
        begun,
        move(at, moved) {
          if (ended || refused) return Promise.resolve(refused ?? {ok: false, error: 'The gesture has ended.'});
          if (pending) stats.coalesced += 1;
          pending = at;
          if (moved) others = moved;
          return begun.then(() => pump() ?? {ok: true, unchanged: true});
        },
        drop({release = false, settle = false} = {}) { return end('finish', release, settle); },
        cancel() { return end('cancel', false, false); },
      };
    },

    /** Provisional: opens the groups that hold `nodeIds`; with `partial` (default) only as far as needed. */
    reveal(nodeIds, {partial = true} = {}) { return change({op: 'reveal', nodeIds, partial}); },

    /**
     * Provisional: hides nodes by kind or id (each with what it holds) and edges by kind, keeping the place of the rest;
     * with `compact: true` the view is laid out again without them, so the rest closes up. `hide` replaces what was hidden
     * before; `{}` shows everything again.
     */
    filter(hide = {}) { return change({op: 'filter', hide}); },

    /** The scene (drawing) of the current snapshot, computed once per snapshot. */
    async scene() {
      const current = snapshot;
      if (!current) throw Error('Nothing imported yet.');
      const known = scenes.get(current);
      if (known) return known;
      const answer = await call({op: 'scene', snapshot: current});
      if (!answer.ok) throw Error(answer.error);
      scenes.set(current, answer.scene);
      return answer.scene;
    },

    /**
     * Provisional (ADR 0019, step 4): the scene of the current snapshot as columns (`SceneColumns`): id tables and typed
     * arrays for bounds, parents, flags and route points. In a resident WASM session called directly, the arrays are
     * views into the engine's memory (no copy), valid until the next request to the session; through a worker they are
     * copies transferred once; for a backend or a session that sends the snapshot they are built from the scene. The
     * content is the same in every case.
     */
    async columns() {
      if (!snapshot) throw Error('Nothing imported yet.');
      if (resident) {
        await queue;
        const kept = await opened;
        if (typeof kept.columns === 'function') return kept.columns();
      }
      return columnsOfScene(await this.scene());
    },

    /** The whole snapshot: the current one, or with `resident` the one the engine keeps (for saving or hand-over). */
    async exportSnapshot() {
      if (!snapshot) return null;
      if (!resident) return snapshot;
      const answer = await (await opened).run({op: 'export'});
      if (!answer.ok) throw Error(answer.error);
      return answer.snapshot;
    },

    /** Whether the snapshot stays in the engine (see `createSession`). */
    get resident() { return resident; },

    /**
     * Provisional (ADR 0023, R7): moves a resident session to another engine (`newEngine`, typically a new worker), so that
     * the old one can be ended and its memory comes back: WebAssembly never shrinks its linear memory, a new worker starts
     * small. The snapshot, the content (with its revision) and the change log's number move; the log's groups do not, so
     * a replica behind that number resyncs. The engines give identical results, so the view does not change. Runs after
     * the changes queued before it; on a refusal the session stays on the old engine. The host ends the old worker after.
     */
    recycle(newEngine) {
      if (!resident) return Promise.resolve({ok: false, error: 'recycle needs a resident session'});
      if (typeof newEngine?.openSession !== 'function') {
        return Promise.resolve({ok: false, error: 'recycle needs an engine with resident sessions'});
      }
      return enqueue(async () => {
        const old = await opened;
        const exported = await old.run({op: 'export'});
        if (!exported.ok) return exported;
        const content = await old.run({op: 'content'});
        if (!content.ok) return content;
        const fresh = await newEngine.openSession();
        // No scene in the answer: the host has it already, and writing it would grow the new engine's memory.
        const loaded = await fresh.run({op: 'load', scene: false, snapshot: exported.snapshot, sequence: content.sequence,
          content: {contentRevision: content.contentRevision, nodes: content.nodes, edges: content.edges}});
        if (!loaded.ok) {
          await fresh.close?.();
          return loaded;
        }
        await old.close?.();
        run = newEngine;
        opened = Promise.resolve(fresh);
        return {ok: true, unchanged: true, sequence: content.sequence, contentRevision: content.contentRevision};
      });
    },

    /** Ends a resident session in the engine (frees its snapshot). The session object is not usable afterwards. */
    async close() {
      generation++;
      snapshot = null;
      if (resident) await (await opened).close();
    },
  };
}

// A request whose `start` positions are typed arrays, with plain arrays instead (JSON has no typed arrays).
const plain = request => {
  const start = request.start;
  if (!start || !['x', 'y', 'r'].some(k => ArrayBuffer.isView(start[k]))) return request;
  const fixed = {...start};
  for (const k of ['x', 'y', 'r']) if (ArrayBuffer.isView(fixed[k])) fixed[k] = Array.from(fixed[k]);
  return {...request, start: fixed};
};

// An overview's places by node id, as `start.nodes` takes them.
const placesOf = o => Object.fromEntries(Array.from(o.ids, (id, i) => [id, {x: o.x[i], y: o.y[i], r: o.r[i]}]));

// The options the overview's clusters depend on; a continuation needs the same ones (as the engine's session checks).
// Defaults as the engine's (`communities` 120, `satellites` 0); `groups` and `hubs` compared as given.
const clustersOf = options => JSON.stringify([options?.communities ?? 120, options?.satellites ?? 0, options?.groups ?? null, options?.hubs ?? null]);

/**
 * Provisional (ADR 0019 for the overview): an overview of one graph, laid out again as the host asks, with the graph kept
 * in the engine where it can keep a session (`run.openSession`). `open(graph, request)` lays out a graph; `layOut(request)`
 * lays it out again (other options, `radii`, `intent`, `configSet`); a `start` without positions (`{settle}` or `{}`)
 * continues from the last overview, and `{nodes}` (places by node id) from those places, also on a new graph (`open`).
 * Without a resident session the graph and the last positions stay here and are sent
 * along; the answers are the same. A new `open` overtakes what is still under way (`{ok: false, stale: true}`).
 */
export function createOverviewSession(run, {resident: wanted = 'auto', columns = false} = {}) {
  if (wanted === true && !run.openSession) throw Error('This engine has no resident sessions');
  const resident = wanted === true || (wanted === 'auto' && typeof run.openSession === 'function');
  const opened = resident ? Promise.resolve(run.openSession()) : null;
  // `graph` is kept here only without a resident session; `started`: an `open` was asked for (it may still run).
  let graph = null, started = false, last = null, clusters = null, generation = 0, queue = Promise.resolve();
  // Whether the last overview's clusters are those of places by id: an engine given the graph again would cluster it
  // anew, so a continuation of it is sent by id too.
  let byPlaces = false;

  const send = async request => {
    if (resident) {
      // On the graph the engine has, only what moves comes back (`positionsOnly`) while the clusters stay; it is put
      // together with the last overview here, so every answer is a whole overview.
      const asked = !request.graph && last ? {...request, positionsOnly: true} : request;
      const kept = await opened;
      // With `columns` the numbers come back as typed arrays (views are copied: `last` outlives the next request); a
      // `start` a host built from them is sent as plain arrays.
      if (columns && typeof kept.overview !== 'function') return {ok: false, error: 'This engine has no columnar overview'};
      const answer = await (columns ? kept.overview(plain(asked), {copy: true}) : kept.run(asked));
      if (answer.ok && answer.overview?.positionsOnly) {
        const {positionsOnly: _moved, ...moved} = answer.overview;
        // What became of places by id belongs to the answer that was asked with them.
        const {continuation: _earlier, ...kept} = last;
        return {...answer, overview: {...kept, ...moved}};
      }
      return answer;
    }
    const sent = {...request, graph: request.graph ?? graph};
    const start = request.start;
    if (start && start.x === undefined && start.nodes === undefined) {
      if (!last || request.graph) return {ok: false, error: 'No overview to continue from: lay out without start first.'};
      if (clustersOf(request.options) !== clusters) return {ok: false, error: 'The clusters changed since the last overview; lay out without start.'};
      if (byPlaces) {
        const answer = await run({...sent, start: {...start, nodes: placesOf(last)}});
        if (!answer.ok) return answer;
        const {continuation: _own, ...overview} = answer.overview;
        return {...answer, overview};
      }
      sent.start = {...start, x: last.x, y: last.y, r: last.r};
    }
    return run(sent);
  };
  // A read of the overview's graph, after the changes before it: the session's graph, or the one kept here.
  const read = request => {
    const mine = generation;
    const result = queue.then(async () => {
      if (!started) return {ok: false, error: 'No graph yet: open one first.'};
      const answer = resident ? await (await opened).run(request) : await run({...request, graph});
      return mine === generation ? answer : {ok: false, stale: true, error: 'A new graph overtook this read.'};
    });
    queue = result.catch(() => {});
    return result;
  };
  const enqueue = (request, fresh) => {
    if (fresh) generation++;
    const mine = generation;
    const result = queue.then(async () => {
      if (mine !== generation) return {ok: false, stale: true, error: 'A new graph overtook this overview.'};
      const answer = await send(request);
      if (mine !== generation) return {ok: false, stale: true, error: 'A new graph overtook this overview.'};
      if (answer.ok) {
        last = answer.overview;
        clusters = clustersOf(request.options);
        byPlaces = !!request.start && (request.start.nodes !== undefined || (byPlaces && request.start.x === undefined));
        if (fresh && !resident) graph = request.graph;
      }
      return answer;
    });
    queue = result.catch(() => {});
    return result;
  };

  return {
    /** Lays out `graph` with `request` (`options`, `intent`, `configSet`, `radii`, `threads`). */
    open(newGraph, request = {}) {
      started = true;
      return enqueue({...request, op: 'overview', graph: newGraph}, true);
    },
    /** Lays out the graph again; `start` without positions continues from the last overview. */
    layOut(request = {}) {
      if (!started) return Promise.resolve({ok: false, error: 'No graph yet: open one first.'});
      const {graph: _ignored, ...rest} = request;
      return enqueue({...rest, op: 'overview'}, false);
    },
    /**
     * Provisional: the paths between two nodes of the overview's graph (`overviewTrace`: `{ok, found, reversed, length,
     * nodes, edges}`), read only; `mode` and `edgeKinds` as for `trace`. The viewer's `pathOf` maps them to its discs.
     */
    trace(from, to, {mode, edgeKinds} = {}) {
      return read({op: 'overviewTrace', from, to, ...(mode ? {mode} : {}), ...(edgeKinds ? {edgeKinds} : {})});
    },
    /** Provisional: the nodes within `hops` of a node of the overview's graph (`overviewNeighbourhood`), read only. */
    neighbourhood(nodeId, {hops, direction, edgeKinds} = {}) {
      return read({op: 'overviewNeighbourhood', ...(Array.isArray(nodeId) ? {nodeIds: nodeId} : {nodeId}),
        ...(hops !== undefined ? {hops} : {}), ...(direction ? {direction} : {}), ...(edgeKinds ? {edgeKinds} : {})});
    },
    /** The last overview (`{ids, parent, x, y, r, …}`), or null. */
    get overview() { return last; },
    /** Whether the graph stays in the engine. */
    get resident() { return resident; },
    /** Ends the session (frees the graph in the engine). */
    async close() {
      generation++;
      graph = last = null;
      started = false;
      if (resident) await (await opened).close();
    },
  };
}
