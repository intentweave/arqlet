// The undo stack of the user's decisions. The engine defines a decision as an `Intent` value (operations `intent` and
// `restore`); this class only keeps pairs of such values and treats them as opaque. One entry is one action, however many
// engine steps it took. Undo and redo are two-phase: the host peeks at the entry, asks the engine to restore its value, and
// commits the move only when the engine accepted it, so a refused restore (a stale view) loses nothing.

const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

export class History {
  #undo = [];
  #redo = [];
  #limit;

  constructor(limit = 50) { this.#limit = limit; }

  get canUndo() { return this.#undo.length > 0; }
  get canRedo() { return this.#redo.length > 0; }
  /** The labels of the undo stack, oldest first. */
  get labels() { return this.#undo.map(entry => entry.label); }

  /**
   * Records an action from `before` to `after`. An action that changed no decision is ignored. A new action ends the redo
   * line. With `merge` the value `after` replaces that of the last entry (a follow-up of the same action, such as the
   * growth of a container after a drop); without an entry to merge into it is recorded as usual.
   */
  record({ label, before, after }, { merge = false } = {}) {
    const last = this.#undo.at(-1);
    if (merge && last) {
      last.after = after;
      if (same(last.before, last.after)) this.#undo.pop();
      return;
    }
    if (same(before, after)) return;
    this.#undo.push({ label, before, after });
    if (this.#undo.length > this.#limit) this.#undo.shift();
    this.#redo = [];
  }

  peekUndo() { return this.#undo.at(-1) ?? null; }
  peekRedo() { return this.#redo.at(-1) ?? null; }

  /** The engine accepted the restore of `peekUndo().before`. */
  commitUndo() {
    const entry = this.#undo.pop();
    if (entry) this.#redo.push(entry);
    return entry ?? null;
  }

  /** The engine accepted the restore of `peekRedo().after`. */
  commitRedo() {
    const entry = this.#redo.pop();
    if (entry) this.#undo.push(entry);
    return entry ?? null;
  }

  /** Another model was loaded or the graph changed: the remembered values no longer describe the view. */
  clear() { this.#undo = []; this.#redo = []; }
}
