/**
 * The curator's selection: a set of leaf nodes for a joint move, or one route. Ids only; the scene decides what is shown.
 * A node selection and a route selection never coexist, so the inspector always describes one kind of thing.
 */
export class Selection {
  private nodeIds = new Set<string>();
  private routeId: string | null = null;

  get nodes(): string[] {
    return [...this.nodeIds];
  }

  get route(): string | null {
    return this.routeId;
  }

  get empty(): boolean {
    return this.nodeIds.size === 0 && this.routeId === null;
  }

  /** Selects one node and drops everything else. */
  replaceNode(id: string): void {
    this.nodeIds = new Set([id]);
    this.routeId = null;
  }

  /** Adds the node to the selection, or removes it when it is already selected. */
  toggleNode(id: string): void {
    if (this.nodeIds.has(id)) this.nodeIds.delete(id);
    else this.nodeIds.add(id);
    this.routeId = null;
  }

  /** Selects one route and drops the node selection; `null` clears the route. */
  selectRoute(id: string | null): void {
    this.routeId = id;
    if (id !== null) this.nodeIds.clear();
  }

  clear(): void {
    this.nodeIds.clear();
    this.routeId = null;
  }

  /** Drops the ids that the scene no longer shows, after a change of projection. */
  prune(known: {nodes: ReadonlySet<string>; routes: ReadonlySet<string>}): void {
    for (const id of [...this.nodeIds]) if (!known.nodes.has(id)) this.nodeIds.delete(id);
    if (this.routeId !== null && !known.routes.has(this.routeId)) this.routeId = null;
  }
}
