// Page-side policy for progressive publications. It mirrors `runtime::session` (crates/runtime/src/session.rs) rule
// for rule: the base of the active job is fixed when it starts, only the active job's publications with that base
// and an increasing sequence are accepted, the newest accepted publication is pending (latest wins), and completion
// and failure are explicit. It never computes geometry.

// Identities compare on the graph revision and the base layout version; the options key is compared when both sides
// know it (the page learns it from the engine's publications, not from the snapshot).
function sameIdentity(a, b) {
  return a.graphRevision===b.graphRevision && a.baseLayoutVersion===b.baseLayoutVersion &&
    (a.optionsKey===undefined || b.optionsKey===undefined || a.optionsKey===b.optionsKey);
}

export class Session {
  #current=null;
  #active=null;

  /** The identity the page currently edits. */
  get current() { return this.#current; }

  /** The active job's ID, if any. */
  get activeJob() { return this.#active?.id ?? null; }

  /** The sequence of the publication the page shows for the active job. */
  get shown() { return this.#active?.shown ?? null; }

  /** A user edit or a view change made `identity` the current state; it cancels the active job. Returns that job. */
  edit(identity) {
    this.#current = identity;
    return this.cancel();
  }

  /** Starts a job based on `base`. Refused when `base` is not the current state. Returns the superseded job, or null. */
  start(id, base) {
    if (this.#current && !sameIdentity(this.#current, base)) return {refused: true};
    this.#current = base;
    const superseded = this.#active?.id ?? null;
    this.#active = {id, base, lastAccepted: 0, shown: null, pending: null};
    return {refused: false, superseded};
  }

  /** Offers a publication from a job. */
  offer(publication) {
    const active = this.#active;
    if (!active || publication.jobId!==active.id || !sameIdentity(active.base, publication.input)) return {verdict: 'stale'};
    if (publication.sequence<=active.lastAccepted) return {verdict: 'duplicate'};
    active.lastAccepted = publication.sequence;
    active.pending = publication;
    return {verdict: 'accepted', sequence: publication.sequence};
  }

  /** The newest accepted publication not shown yet; taking it makes it the shown state. */
  takePending() {
    const active = this.#active;
    if (!active || !active.pending) return null;
    const publication = active.pending;
    active.pending = null;
    active.shown = publication.sequence;
    return publication;
  }

  /** The job completed. `result` is the newest publication that was never shown; the caller applies it. */
  finish(id) {
    if (!this.#active || this.#active.id!==id) return {outcome: 'ignored'};
    const active = this.#active;
    this.#active = null;
    return {outcome: 'completed', improved: active.lastAccepted>0, result: active.pending};
  }

  /** The job failed or was stopped: pending work is dropped, and the shown state stays. */
  fail(id) {
    if (!this.#active || this.#active.id!==id) return {outcome: 'ignored'};
    const shown = this.#active.shown;
    this.#active = null;
    return {outcome: 'failed', shown};
  }

  /** Stops the active job without a result (a user action superseded it). Returns its ID, or null. */
  cancel() {
    const id = this.#active?.id ?? null;
    this.#active = null;
    return id;
  }
}
