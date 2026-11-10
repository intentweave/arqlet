// The move gesture as the page runs it. One request is in flight at a time; while it runs, the pointer's newest position
// is the only pending one (earlier positions are coalesced into it). Every step carries the next sequence number, and the
// base of the next step is the snapshot the last reply returned. The node under the pointer is drawn at the pointer, not at
// the engine's box, until the gesture ends: a reply never moves it back.

let gestureCounter = 0;

/**
 * Whether a finished drag releases the pin the gesture made. A drag on the node only moves it: no pin is left, unless the node
 * already had one (then the pin moves with it, as the user placed it). A drag on the node's pin fixes the position it ends at.
 */
export function releaseOnDrop(fromPin, wasPinned) {
  return !fromPin && !wasPinned;
}

export class MoveGesture {
  #send;
  #onSnapshot;
  #onStats;
  #base;
  #id = null;
  #node = null;
  #seq = 0;
  #inFlight = false;
  #pending = null;
  #others = {};
  #end = null;
  #counts = {sent: 0, coalesced: 0, applied: 0, discarded: 0, finished: 0};

  /**
   * @param {object} options
   * @param {(request: object) => Promise<{ok: boolean, snapshot?: object, error?: string}>} options.send engine call
   * @param {object} options.base the snapshot the gesture starts from
   * @param {(snapshot: object, info: {dragging: boolean, node: string}) => void} options.onSnapshot a reply was applied
   * @param {(stats: object) => void} [options.onStats]
   */
  constructor({send, base, onSnapshot, onStats = () => {}}) {
    this.#send = send;
    this.#base = base;
    this.#onSnapshot = onSnapshot;
    this.#onStats = onStats;
  }

  /** True while a gesture runs (begun and not yet finished, cancelled or refused). */
  get active() { return this.#id !== null; }

  /** The node the gesture moves, or null. */
  get node() { return this.#node; }

  /** Counts of this gesture's updates: sent, coalesced into a newer position, applied, discarded, and finished gestures. */
  get stats() { return {...this.#counts}; }

  /**
   * Starts a gesture on `node` at `position` (in the parent's space). `others` maps the other nodes of a selection that moves
   * with it to their positions in their own parents' spaces; the set stays the same for the whole gesture.
   */
  begin(node, position, others = {}) {
    if (this.active) throw Error('A move gesture is already running');
    this.#others = others;
    this.#id = `move-${++gestureCounter}`;
    this.#node = node;
    this.#seq = 0;
    this.#end = null;
    this.#pending = null;
    this.#step('begin', position, false);
  }

  /** A newer pointer position. Positions that arrive while a request runs replace each other. */
  update(position, others = this.#others) {
    if (!this.active || this.#end) return;
    if (this.#pending) this.#counts.coalesced += 1;
    this.#pending = position;
    this.#others = others;
    this.#flush();
  }

  /** The pointer was released: the last position is applied, then the pin is kept (or released). */
  finish({release = false} = {}) {
    if (!this.active || this.#end) return;
    this.#end = {phase: 'finish', release};
    this.#flush();
  }

  /** The gesture is abandoned: the node returns to the pin it had before the gesture. */
  cancel() {
    if (!this.active || this.#end) return;
    this.#pending = null;
    this.#end = {phase: 'cancel', release: false};
    this.#flush();
  }

  #flush() {
    if (this.#inFlight) return;
    if (this.#pending) {
      const position = this.#pending;
      this.#pending = null;
      this.#step('update', position, false);
      return;
    }
    if (this.#end) {
      const {phase, release} = this.#end;
      this.#step(phase, null, release);
    }
  }

  #step(phase, position, release) {
    this.#seq += 1;
    const base = this.#base;
    const request = {
      op: 'move',
      snapshot: base,
      expectedRevision: base.revision,
      expectedLayoutVersion: base.layoutVersion ?? 0,
      gestureId: this.#id,
      seq: this.#seq,
      phase,
      node: this.#node,
      position,
      release,
      ...(Object.keys(this.#others).length ? {others: this.#others} : {}),
    };
    this.#counts.sent += 1;
    this.#inFlight = true;
    const id = this.#id;
    Promise.resolve(this.#send(request)).then(reply => this.#reply(id, phase, reply), error => this.#reply(id, phase, {ok: false, error: error.message}));
  }

  #reply(id, phase, reply) {
    // A reply for a gesture that is no longer current is discarded; so is any reply after a refusal.
    if (id !== this.#id) return;
    this.#inFlight = false;
    if (!reply.ok) {
      this.#counts.discarded += 1;
      this.#id = null;
      this.#end = null;
      this.#pending = null;
      this.#onStats({...this.#counts, error: reply.error});
      return;
    }
    this.#base = reply.snapshot;
    this.#counts.applied += 1;
    if (phase === 'finish' || phase === 'cancel') {
      this.#counts.finished += 1;
      this.#id = null;
      this.#end = null;
      this.#onSnapshot(reply.snapshot, {dragging: false, node: this.#node});
      this.#onStats({...this.#counts});
      return;
    }
    this.#onSnapshot(reply.snapshot, {dragging: true, node: this.#node});
    this.#onStats({...this.#counts});
    this.#flush();
  }

  /** The view was replaced: replies to this gesture are ignored from now on. */
  abandon() {
    this.#id = null;
    this.#end = null;
    this.#pending = null;
  }
}
