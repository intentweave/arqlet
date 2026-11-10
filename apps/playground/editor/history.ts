/**
 * Undo and redo over whole states. A state is stored as one value (for the curation session: the sealed document
 * with its validation and its pending routes), so undo restores geometry and derived state atomically.
 */
export class History<T> {
  private past: T[] = [];
  private future: T[] = [];
  private present: T;

  constructor(initial: T) {
    this.present = initial;
  }

  get value(): T {
    return this.present;
  }

  get canUndo(): boolean {
    return this.past.length > 0;
  }

  get canRedo(): boolean {
    return this.future.length > 0;
  }

  /** Makes `next` the present state. Any redo branch is discarded. */
  commit(next: T): void {
    this.past.push(this.present);
    this.present = next;
    this.future = [];
  }

  undo(): boolean {
    const previous = this.past.pop();
    if (previous === undefined) return false;
    this.future.push(this.present);
    this.present = previous;
    return true;
  }

  redo(): boolean {
    const next = this.future.pop();
    if (next === undefined) return false;
    this.past.push(this.present);
    this.present = next;
    return true;
  }
}
