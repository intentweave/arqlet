// A layout session (provisional): one graph, its current snapshot, and the changes sent to the engine as operations on
// it, so that unchanged parts keep their place. Pure: no DOM, no fetch; `run` is the engine (`loadEngine`) or a worker
// (`workerRun`).
//
// The engine refuses a change to an older snapshot ("Stale view request"); the session adds the snapshot's revision and
// layout version to every change, runs changes one after the other on the newest snapshot, and never applies an answer
// that was overtaken: after a new `import`, or `reset`, a change still under way resolves to `{ok: false, stale: true}`.

/**
 * Wraps a worker of `createEngineWorker` as an asynchronous `run`: each request gets an id, and the answer with that id
 * resolves it. Progressive publications are passed to `onPublication(requestId, message)`.
 */
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
    return {run: request => post({session: opened.session, request}), close: () => post({session: opened.session, close: true})};
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
export function createSession(run, {onChange, resident: wanted = 'auto'} = {}) {
  let snapshot = null;
  let generation = 0;
  let queue = Promise.resolve();
  const scenes = new WeakMap();
  if (wanted === true && !run.openSession) throw Error('This engine has no resident sessions');
  const resident = wanted === true || (wanted === 'auto' && typeof run.openSession === 'function');
  const opened = resident ? Promise.resolve(run.openSession()) : null;
  // The engine call: with a resident session, the request goes to it and its answer is turned into the shape of an
  // answer with a snapshot (the state), its scene remembered for that state.
  const call = async request => {
    if (!resident) return run(request);
    const {snapshot: _sent, ...rest} = request;
    const answer = await (await opened).run(rest);
    if (!answer.ok || answer.revision === undefined) return answer;
    const state = {revision: answer.revision, layoutVersion: answer.layoutVersion, collapsedIds: answer.collapsedIds};
    if (answer.scene) scenes.set(state, answer.scene);
    return {ok: true, snapshot: state};
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
      generation++;
      return enqueue(() => call({op: 'import', graph, limit, ...(options ? {options} : {})}));
    },

    /** Drops the snapshot; changes still under way resolve as stale. */
    reset() {
      generation++;
      snapshot = null;
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
    async neighbourhood(nodeId, {hops, direction, edgeKinds} = {}) {
      if (!snapshot) return {ok: false, error: 'Nothing imported yet.'};
      return call({op: 'neighbourhood', snapshot, ...(Array.isArray(nodeId) ? {nodeIds: nodeId} : {nodeId}),
        ...(hops !== undefined ? {hops} : {}), ...(direction ? {direction} : {}), ...(edgeKinds ? {edgeKinds} : {})});
    },

    /** Provisional: opens the groups that hold `nodeIds`; with `partial` (default) only as far as needed. */
    reveal(nodeIds, {partial = true} = {}) { return change({op: 'reveal', nodeIds, partial}); },

    /**
     * Provisional: hides nodes by kind or id (each with what it holds) and edges by kind, keeping the place of the rest.
     * `hide` replaces what was hidden before; `{}` shows everything again.
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

    /** Ends a resident session in the engine (frees its snapshot). The session object is not usable afterwards. */
    async close() {
      generation++;
      snapshot = null;
      if (resident) await (await opened).close();
    },
  };
}

// The options the overview's clusters depend on; a continuation needs the same ones (as the engine's session checks).
// Defaults as the engine's (`communities` 120, `satellites` 0); `groups` compared as given.
const clustersOf = options => JSON.stringify([options?.communities ?? 120, options?.satellites ?? 0, options?.groups ?? null]);

/**
 * Provisional (ADR 0019 for the overview): an overview of one graph, laid out again as the host asks, with the graph kept
 * in the engine where it can keep a session (`run.openSession`). `open(graph, request)` lays out a graph; `layOut(request)`
 * lays it out again (other options, `radii`, `intent`, `configSet`); a `start` without positions (`{settle}` or `{}`)
 * continues from the last overview. Without a resident session the graph and the last positions stay here and are sent
 * along; the answers are the same. A new `open` overtakes what is still under way (`{ok: false, stale: true}`).
 */
export function createOverviewSession(run, {resident: wanted = 'auto'} = {}) {
  if (wanted === true && !run.openSession) throw Error('This engine has no resident sessions');
  const resident = wanted === true || (wanted === 'auto' && typeof run.openSession === 'function');
  const opened = resident ? Promise.resolve(run.openSession()) : null;
  // `graph` is kept here only without a resident session; `started`: an `open` was asked for (it may still run).
  let graph = null, started = false, last = null, clusters = null, generation = 0, queue = Promise.resolve();

  const send = async request => {
    if (resident) return (await opened).run(request);
    const sent = {...request, graph: request.graph ?? graph};
    const start = request.start;
    if (start && start.x === undefined) {
      if (!last || request.graph) return {ok: false, error: 'No overview to continue from: lay out without start first.'};
      if (clustersOf(request.options) !== clusters) return {ok: false, error: 'The clusters changed since the last overview; lay out without start.'};
      sent.start = {...start, x: last.x, y: last.y, r: last.r};
    }
    return run(sent);
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
