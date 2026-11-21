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
 * time; `historyLimit` how many steps back are kept; with `scenes: false` the state carries no scene and no diff (a host
 * that draws from the snapshot itself saves the engine call); `detailLimit` caps the edge ids of a line's details.
 */
export function createViewController(session, {page = 12, historyLimit = 50, scenes = true, detailLimit} = {}) {
  let state = Object.freeze({
    snapshot: null, scene: null, diff: null, busy: false, error: null,
    highlight: null, selection: null, details: null, hover: null, hide: {}, canBack: false, canForward: false, backLabel: null, forwardLabel: null, steps: 0,
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
  // `fresh`: the step starts a new chain (another view), not waiting for the steps before it, which no longer publish.
  const enqueue = (step, {fresh = false} = {}) => {
    if (fresh) {
      generation += 1;
      queue = Promise.resolve();
    }
    const mine = generation;
    pending += 1;
    if (!state.busy) publish({busy: true});
    const result = queue.then(async () => {
      if (mine !== generation) return {ok: false, cancelled: true};
      try {
        return await step(() => mine === generation);
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

  // The details of the selection on the current view (the box that shows a node and the edges a line stands for depend on
  // what is open); null when the selection is gone (a drawn line the change removed).
  const detailsOf = async selection => {
    if (!selection) return null;
    const answer = await session.details(selection, detailLimit === undefined ? {} : {limit: detailLimit});
    return answer.ok ? answer : null;
  };

  // After an applied change: the new scene, its difference to the one before, the highlight and the details asked again.
  const settle = async (extra = {}, current = () => true) => {
    const scene = scenes ? await session.scene() : null;
    const highlight = await refreshHighlight(state.highlight);
    const details = await detailsOf(state.selection);
    if (!current()) return;
    publish({
      details, selection: details ? state.selection : null,
      snapshot: session.snapshot, scene, diff: scenes ? sceneDiff(state.scene, scene) : null, highlight, error: null,
      canBack: back.length > 0, canForward: forward.length > 0,
      backLabel: back.at(-1)?.label ?? null, forwardLabel: forward.at(-1)?.label ?? null, steps: state.steps + 1, ...extra,
    });
  };

  // A change the user makes: its decisions before and after are remembered (unless `remember` is false), so that `back`
  // undoes it.
  const change = (label, run, extra = () => ({}), remember = true, merge = false) => enqueue(async current => {
    if (!session.snapshot) return {ok: false, error: 'Nothing imported yet.'};
    const before = remember ? await intent() : null;
    const result = await run();
    if (!current()) return {ok: false, cancelled: true};
    if (!result.ok) {
      publish({error: result.error ?? 'The engine refused the change.'});
      return result;
    }
    const after = remember ? await intent() : null;
    if (!current()) return {ok: false, cancelled: true};
    if (remember && merge && back.length) {
      // A follow-up of the last change (such as a container keeping the size a drop gave it): one entry for both.
      const last = back.at(-1);
      last.after = after;
      if (same(last.before, last.after)) back.pop();
      forward = [];
    } else if (remember && !same(before, after)) {
      back.push({label, before, after});
      if (back.length > historyLimit) back.shift();
      forward = [];
    }
    await settle(extra(), current);
    return result;
  });

  // Back and forward: the engine restores the remembered decisions; a refused restore keeps the entry.
  const step = direction => enqueue(async current => {
    const from = direction < 0 ? back : forward;
    const entry = from.at(-1);
    if (!entry) return {ok: false, error: direction < 0 ? 'Nothing to undo.' : 'Nothing to redo.'};
    const result = await session.apply({op: 'restore', intent: direction < 0 ? entry.before : entry.after});
    if (!current()) return {ok: false, cancelled: true};
    if (!result.ok) {
      publish({error: result.error ?? 'The engine refused the restore.'});
      return result;
    }
    from.pop();
    (direction < 0 ? forward : back).push(entry);
    const hidden = (direction < 0 ? entry.before : entry.after).hidden ?? {};
    await settle({hide: hidden}, current);
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
      back = [];
      forward = [];
      return enqueue(async current => {
        const result = await session.import(graph, init);
        if (!current()) return {ok: false, cancelled: true};
        if (!result.ok) {
          publish({error: result.error ?? 'The import failed.'});
          return result;
        }
        state = Object.freeze({...state, scene: null, highlight: null, selection: null, details: null, hover: null,
          hide: {}, steps: 0});
        await settle({}, current);
        return result;
      }, {fresh: true});
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

    /**
     * Selects a node (`{nodeId}`) or an edge (`{edgeId}`: a drawn line or an edge of the source graph), or nothing
     * (`null`): `state.details` then holds what the engine knows about it (see `session.details`), asked again after
     * every change until the selection is cleared. How it is shown (a panel, a tooltip, a zoom to `details.node.bounds`)
     * is the host's; domain attributes the host looks up by id. Not a step for `back`.
     */
    select(target) {
      const selection = target ? ('nodeId' in target ? {nodeId: target.nodeId} : {edgeId: target.edgeId}) : null;
      if (!selection) {
        if (state.selection || state.details) publish({selection: null, details: null});
        return Promise.resolve({ok: true});
      }
      return enqueue(async current => {
        const answer = await session.details(selection, detailLimit === undefined ? {} : {limit: detailLimit});
        if (!current()) return {ok: false, cancelled: true};
        if (!answer.ok) {
          publish({error: answer.error ?? 'No details.'});
          return answer;
        }
        publish({selection, details: answer, error: null});
        return answer;
      });
    },

    /**
     * What the pointer is over (`{nodeId}`, `{edgeId}` or `null`), for a renderer's hover state (a tooltip from the
     * scene's label, a lighter outline); no engine call. A tooltip with the engine's facts asks `select` or
     * `session.details` itself.
     */
    hover(target) {
      const next = target ? ('nodeId' in target ? {nodeId: target.nodeId} : {edgeId: target.edgeId}) : null;
      if (same(next, state.hover)) return;
      publish({hover: next});
    },

    /** Undoes the last change (its decisions before it are restored). */
    back() { return step(-1); },
    /** Redoes the last undone change. */
    forward() { return step(1); },

    /**
     * Any other change of the engine's protocol (see `session.apply`), remembered for `back` under `label` (not with
     * `remember: false`, for steps that change no decision, such as settling).
     */
    apply(request, label = request.op, {remember = true, merge = false} = {}) {
      return change(label, () => session.apply(request), () => ({}), remember, merge);
    },

    /**
     * Continues from `snapshot`, made by the host (a drag, a refinement, a loaded document); with `forget` the history and
     * the highlight start empty (another graph).
     */
    adopt(snapshot, {forget = false, remember = null} = {}) {
      return enqueue(async current => {
        // With `remember` (a label) the change from the session's snapshot to this one is remembered for `back`.
        const before = remember && session.snapshot ? await intent() : null;
        const result = await session.load(snapshot, {reset: forget});
        if (!current()) return {ok: false, cancelled: true};
        if (!result.ok) {
          publish({error: result.error ?? 'The snapshot was refused.'});
          return result;
        }
        if (forget) {
          back = [];
          forward = [];
          state = Object.freeze({...state, highlight: null, selection: null, details: null, hover: null, hide: {}});
        } else if (before) {
          const after = await intent();
          if (!same(before, after)) {
            back.push({label: remember, before, after});
            if (back.length > historyLimit) back.shift();
            forward = [];
          }
        }
        await settle({hide: snapshot.options?.hidden ?? (forget ? {} : state.hide)}, current);
        return result;
      }, {fresh: forget});
    },

    /**
     * Drops the steps queued and not started yet (they resolve `{ok: false, cancelled: true}`); a step under way finishes,
     * since an engine call cannot be stopped from outside.
     */
    cancel() { generation += 1; },
  };
  return controller;
}
