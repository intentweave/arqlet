// A headless view controller (provisional, request 31): what a user does between two scenes (opening and closing groups,
// paging through `+N more`, filtering, a path or a neighbourhood kept as the highlight, a step back and forward) on top of
// a layout session (`createSession`), with one state object and `subscribe`, so that a React binding is
// `useSyncExternalStore(controller.subscribe, controller.getState)` and nothing else. No DOM, no framework, no fetch:
// loading data, access control, presets and drawing stay with the host.
//
// The view's decisions are the engine's values (operations `intent` and `restore`): a step back restores the decisions
// before a change, in one engine step. The highlight is a read-only query (`trace`, `neighbourhood`) that is asked again
// after every change, so it stays on the view until it is cleared. Every step carries the difference between the scenes
// before and after it (`sceneDiff`), so every renderer can animate the same way.

/**
 * What changed between two scenes: nodes and routes added, removed, and changed (nodes moved or resized, routes with other
 * points), by id. Either scene may be null (everything added or removed).
 */
export function sceneDiff(before, after) {
  const byId = list => new Map((list ?? []).map(x => [x.id, x]));
  const compare = (a, b, same) => {
    const out = {added: [], removed: [], changed: []};
    for (const [id, x] of b) {
      const old = a.get(id);
      if (!old) out.added.push(id);
      else if (!same(old, x)) out.changed.push(id);
    }
    for (const id of a.keys()) if (!b.has(id)) out.removed.push(id);
    return out;
  };
  const sameBounds = (p, q) => p.bounds.x === q.bounds.x && p.bounds.y === q.bounds.y
    && p.bounds.width === q.bounds.width && p.bounds.height === q.bounds.height && !!p.collapsed === !!q.collapsed;
  const samePoints = (p, q) => p.points.length === q.points.length
    && p.points.every((pt, k) => pt.x === q.points[k].x && pt.y === q.points[k].y);
  return {
    nodes: compare(byId(before?.nodes), byId(after?.nodes), sameBounds),
    routes: compare(byId(before?.routes), byId(after?.routes), samePoints),
  };
}

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/**
 * Creates a view controller on `session` (from `createSession`). `page` is how many children `showRest` reveals at a
 * time; `historyLimit` how many steps back are kept.
 */
export function createViewController(session, {page = 12, historyLimit = 50} = {}) {
  let state = Object.freeze({
    snapshot: null, scene: null, diff: null, busy: false, error: null,
    highlight: null, hide: {}, canBack: false, canForward: false, steps: 0,
  });
  const listeners = new Set();
  const publish = patch => {
    state = Object.freeze({...state, ...patch});
    for (const listener of [...listeners]) listener();
  };
  let back = [];
  let forward = [];
  let queue = Promise.resolve();
  let generation = 0;
  let pending = 0;

  // Runs `step` after every step before it; a step queued before `cancel()` and not yet started is dropped.
  const enqueue = step => {
    const mine = generation;
    pending += 1;
    if (!state.busy) publish({busy: true});
    const result = queue.then(async () => {
      if (mine !== generation) return {ok: false, cancelled: true};
      try {
        return await step();
      } catch (error) {
        publish({error: error.message ?? String(error)});
        return {ok: false, error: error.message ?? String(error)};
      }
    }).finally(() => {
      pending -= 1;
      if (pending === 0) publish({busy: false});
    });
    queue = result.catch(() => {});
    return result;
  };

  const intent = async () => {
    const answer = await session.apply({op: 'intent'});
    if (!answer.ok) throw Error(answer.error ?? 'No intent');
    return answer.intent;
  };

  // Asks the highlight's query again on the current view (its visible nodes and routes depend on what is open).
  const refreshHighlight = async highlight => {
    if (!highlight) return null;
    const answer = highlight.kind === 'trace'
      ? await session.trace(highlight.from, highlight.to, highlight.init)
      : await session.neighbourhood(highlight.nodeId, highlight.init);
    return answer.ok ? {...highlight, answer} : null;
  };

  // After an applied change: the new scene, its difference to the one before, the highlight asked again.
  const settle = async (extra = {}) => {
    const scene = await session.scene();
    const highlight = await refreshHighlight(state.highlight);
    publish({
      snapshot: session.snapshot, scene, diff: sceneDiff(state.scene, scene), highlight, error: null,
      canBack: back.length > 0, canForward: forward.length > 0, steps: state.steps + 1, ...extra,
    });
  };

  // A change the user makes: its decisions before and after are remembered, so that `back` undoes it.
  const change = (label, run, extra = () => ({})) => enqueue(async () => {
    if (!session.snapshot) return {ok: false, error: 'Nothing imported yet.'};
    const before = await intent();
    const result = await run();
    if (!result.ok) {
      publish({error: result.error ?? 'The engine refused the change.'});
      return result;
    }
    const after = await intent();
    if (!same(before, after)) {
      back.push({label, before, after});
      if (back.length > historyLimit) back.shift();
      forward = [];
    }
    await settle(extra());
    return result;
  });

  // Back and forward: the engine restores the remembered decisions; a refused restore keeps the entry.
  const step = direction => enqueue(async () => {
    const from = direction < 0 ? back : forward;
    const entry = from.at(-1);
    if (!entry) return {ok: false, error: direction < 0 ? 'Nothing to undo.' : 'Nothing to redo.'};
    const result = await session.apply({op: 'restore', intent: direction < 0 ? entry.before : entry.after});
    if (!result.ok) {
      publish({error: result.error ?? 'The engine refused the restore.'});
      return result;
    }
    from.pop();
    (direction < 0 ? forward : back).push(entry);
    const hidden = (direction < 0 ? entry.before : entry.after).hidden ?? {};
    await settle({hide: hidden});
    return result;
  });

  const controller = {
    /** The current state (a new frozen object after every change; the same object while nothing changed). */
    getState: () => state,

    /** Calls `listener` after every change of the state; returns the function that stops it. */
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },

    /** Lays out `graph` and starts the view on it (see `session.import`); the history and the highlight start empty. */
    import(graph, init) {
      generation += 1;
      back = [];
      forward = [];
      return enqueue(async () => {
        const result = await session.import(graph, init);
        if (!result.ok) {
          publish({error: result.error ?? 'The import failed.'});
          return result;
        }
        state = Object.freeze({...state, scene: null, highlight: null, hide: {}, steps: 0});
        await settle();
        return result;
      });
    },

    /** Opens container `id` (nothing when it is open). */
    open(id) { return change(`Open ${id}`, () => session.expand(id)); },
    /** Closes container `id` (nothing when it is closed). */
    close(id) { return change(`Close ${id}`, () => session.collapse(id)); },
    /** Opens or closes container `id`. */
    toggle(id) { return change(`Toggle ${id}`, () => session.toggle(id)); },

    /**
     * Shows the next `count` (default `page`) children of `container`'s `+N more` box; on the last page the container is
     * whole again. `container` may also be the rest's id (`rest:<container>`).
     */
    showRest(container, count = page) {
      const id = container.startsWith('rest:') ? container.slice(5) : container;
      return change(`Next ${count} of ${id}`, () => session.apply({op: 'reveal', more: {container: id, count}}));
    },

    /** Opens the groups that hold `nodeIds` (only as far as needed with `partial`, the default). */
    reveal(nodeIds, init) { return change('Reveal', () => session.reveal(nodeIds, init)); },

    /** Hides nodes by kind or id and edges by kind (see `session.filter`); `{}` shows everything again. */
    filter(hide = {}) { return change('Filter', () => session.filter(hide), () => ({hide})); },

    /** Highlights the paths from `from` to `to` (see `session.trace`) until it is cleared; the view does not change. */
    trace(from, to, init) {
      return enqueue(async () => {
        const answer = await session.trace(from, to, init);
        if (!answer.ok) {
          publish({error: answer.error ?? 'No path.'});
          return answer;
        }
        publish({highlight: {kind: 'trace', from, to, init, answer}, error: null});
        return answer;
      });
    },

    /** Highlights the neighbourhood of `nodeId` (one id or a list; see `session.neighbourhood`) until it is cleared. */
    neighbourhood(nodeId, init) {
      return enqueue(async () => {
        const answer = await session.neighbourhood(nodeId, init);
        if (!answer.ok) {
          publish({error: answer.error ?? 'No neighbourhood.'});
          return answer;
        }
        publish({highlight: {kind: 'neighbourhood', nodeId, init, answer}, error: null});
        return answer;
      });
    },

    /** Opens what the highlight passes through (the groups on a path or around a neighbourhood). */
    revealHighlight() {
      const nodes = state.highlight?.answer?.nodes;
      return nodes?.length ? controller.reveal(nodes) : Promise.resolve({ok: true, unchanged: true});
    },

    /** Clears the highlight. */
    clearHighlight() { if (state.highlight) publish({highlight: null}); },

    /** Undoes the last change (its decisions before it are restored). */
    back() { return step(-1); },
    /** Redoes the last undone change. */
    forward() { return step(1); },

    /** Any other change of the engine's protocol (see `session.apply`), remembered for `back` under `label`. */
    apply(request, label = request.op) { return change(label, () => session.apply(request)); },

    /**
     * Drops the steps queued and not started yet (they resolve `{ok: false, cancelled: true}`); a step under way finishes,
     * since an engine call cannot be stopped from outside.
     */
    cancel() { generation += 1; },
  };
  return controller;
}
