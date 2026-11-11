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
  return request => new Promise(resolve => {
    const requestId = ++next;
    waiting.set(requestId, resolve);
    worker.postMessage({requestId, request});
  });
}

/** Creates a session on `run` (synchronous or asynchronous). `onChange(snapshot)` is called after every applied change. */
export function createSession(run, {onChange} = {}) {
  let snapshot = null;
  let generation = 0;
  let queue = Promise.resolve();
  const scenes = new WeakMap();

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
    return run({
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
     * the first view shows (containers beyond it start collapsed); `options` are layout options (`docs/configuration.md`).
     * Any change still under way is dropped.
     */
    import(graph, {limit = graph.nodes.length, options} = {}) {
      generation++;
      return enqueue(() => run({op: 'import', graph, limit, ...(options ? {options} : {})}));
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

    /** The scene (drawing) of the current snapshot, computed once per snapshot. */
    async scene() {
      const current = snapshot;
      if (!current) throw Error('Nothing imported yet.');
      const known = scenes.get(current);
      if (known) return known;
      const answer = await run({op: 'scene', snapshot: current});
      if (!answer.ok) throw Error(answer.error);
      scenes.set(current, answer.scene);
      return answer.scene;
    },
  };
}
