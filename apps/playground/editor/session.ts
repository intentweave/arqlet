/**
 * A curation session: one draft reference, its history, and the engine calls behind every change.
 *
 * Each change is one undoable step. It produces the next document in TypeScript, then the engine refreshes and
 * seals it and reports its validation. The document, its hashes and every metric come from Rust; this module
 * never computes a metric. A change is applied only when the engine accepts it.
 */
import type {PortSide, Point, Rect, ReferenceDocument, Report, Scene, Snapshot} from '@arqlet/contracts';
import {History} from './history.js';
import {boundsAround, centre, movedWithin, normalizedRoute, overlaps, resized, routesNear, routesNearBorder, routesTouching, shiftSegment, withDogleg} from './geometry.js';

/** Clearance around a node box that the routes must keep to; a route inside it is repaired like an incident one. */
const NODE_CLEARANCE = 12;

/** The smallest leaf box a resize may produce. A documented engineering minimum, not a measured label size. */
const MIN_LEAF = {width: 60, height: 30};

/** Space a container keeps between its boundary and the boxes inside it when it is resized. */
const CONTAINER_PADDING = 12;

/** The engine's reply to a reference operation. Failures carry the engine's explicit error. */
export interface EngineReply {
  ok: boolean;
  error?: string;
  /** A note about a change that was applied, such as a move the container clamped. */
  notice?: string;
  reference?: ReferenceDocument;
  report?: Report;
  canonical?: string;
}

/** Runs one reference operation in the engine that the current mode selects. */
export type Engine = (request: Record<string, unknown>) => Promise<EngineReply>;

interface State {
  document: ReferenceDocument;
  report: Report;
}

export class CurationSession {
  private readonly history: History<State>;
  /** Changes run one after another; each one reads the document when its turn comes, so none is refused. */
  private queue: Promise<unknown> = Promise.resolve();
  /** Bumped by undo and redo. A change started before them must not overwrite the history they moved. */
  private generation = 0;

  private constructor(private readonly engine: Engine, state: State) {
    this.history = new History(state);
  }

  /** A draft of the displayed snapshot: revision 1, curated geometry equal to the baseline. */
  static async create(engine: Engine, snapshot: Snapshot, referenceId: string): Promise<CurationSession> {
    const reply = await engine({op: 'referenceCreate', snapshot, referenceId});
    const document = requireDocument(reply);
    return new CurationSession(engine, {document, report: await validateWith(engine, document)});
  }

  /** A reference from its JSON text. The engine checks it structurally first; an invalid file is refused. */
  static async load(engine: Engine, text: string): Promise<CurationSession> {
    let value: unknown;
    try {
      value = JSON.parse(text);
    } catch {
      throw Error('The file is not valid JSON.');
    }
    const reply = await engine({op: 'referenceValidate', reference: value});
    if (!reply.ok || !reply.report) throw Error(reply.error ?? 'The reference is not valid.');
    return new CurationSession(engine, {document: value as ReferenceDocument, report: reply.report});
  }

  get document(): ReferenceDocument {
    return this.history.value.document;
  }

  get report(): Report {
    return this.history.value.report;
  }

  get canUndo(): boolean {
    return this.history.canUndo;
  }

  get canRedo(): boolean {
    return this.history.canRedo;
  }

  get pending(): ReadonlySet<string> {
    return new Set(this.document.curated.pendingRoutes);
  }

  /**
   * Moves one node by (dx, dy). Without `grow` it stays inside its parent. With `grow`, each container that the node
   * leaves grows to the right and bottom, and every ancestor that grows is updated in the same step. The routes that
   * touch the node, pass its old or new box, or run along a grown container's border become pending.
   */
  moveNode(nodeId: string, dx: number, dy: number, grow = false): Promise<EngineReply> {
    return this.moveNodes([nodeId], dx, dy, grow);
  }

  /**
   * Moves several leaf nodes by the same offset in one step. Each node is held by its own container exactly as a single
   * node is; a container that grows for one node is grown in the same step for the others. Containers in the list are
   * ignored: they move with `moveContainer`. The routes that touch a moved node, pass its old or new box, or run along
   * a grown container's border become pending.
   */
  moveNodes(nodeIds: string[], dx: number, dy: number, grow = false): Promise<EngineReply> {
    const plan = this.planMoveNodes(nodeIds, dx, dy, grow);
    if (!plan.ok) return failed(plan.error);
    return this.commit(document => ({
      ...document,
      curated: {
        ...document.curated,
        nodes: plan.nodes,
        pendingRoutes: unique([...document.curated.pendingRoutes, ...plan.pending]),
      },
    })).then(reply => (plan.notice ? {...reply, notice: plan.notice} : reply));
  }

  /**
   * The geometry that a move would give, without changing the document: the node boxes, the routes it makes pending,
   * and a notice when a container clamped the move. Failures carry the reason.
   */
  planMoveNodes(
    nodeIds: string[],
    dx: number,
    dy: number,
    grow = false,
  ): {ok: true; nodes: Record<string, Rect>; pending: string[]; notice?: string} | {ok: false; error: string} {
    const curated = this.document.curated;
    const unknown = nodeIds.find(id => !curated.nodes[id]);
    if (unknown) return {ok: false, error: `Unknown node ${unknown}`};
    const leaves = nodeIds.filter(id => !this.isContainer(id));
    if (leaves.length === 0) return {ok: false, error: 'Only leaves move with this action; containers move with their header.'};
    const nodes = {...curated.nodes};
    const original = (id: string): Rect => curated.nodes[id]!;
    const grownIds = new Map<string, Rect>();
    let clamped = false;
    let moved = false;
    const touching = new Set<string>();
    const near = new Set<string>();
    for (const nodeId of leaves) {
      const current = original(nodeId);
      const ancestors = this.ancestorsIn(nodes, nodeId);
      const result = movedWithin(current, dx, dy, ancestors.map(item => item.rect), grow);
      const rect = result.rect;
      clamped ||= result.clamped;
      moved ||= rect.x !== current.x || rect.y !== current.y;
      nodes[nodeId] = rect;
      ancestors.forEach((item, index) => {
        const after = result.grown[index]!;
        if (!resized(item.rect, after)) return;
        if (!grownIds.has(item.id)) grownIds.set(item.id, original(item.id));
        nodes[item.id] = after;
      });
      for (const routeId of routesTouching(this.document.input.graph.edges, nodeId, Object.keys(curated.routes))) touching.add(routeId);
      for (const routeId of routesNear(curated.routes, [current, rect], NODE_CLEARANCE)) near.add(routeId);
    }
    // Growth may not create a new overlap with an unrelated node; an overlap that already exists is not blamed on it.
    for (const [containerId, before] of grownIds) {
      const collision = this.newCollision(containerId, before, nodes[containerId]!);
      if (collision) return {ok: false, error: `Growing ${containerId} would overlap ${collision}. Move ${collision} or the node first.`};
    }
    if (!moved && grownIds.size === 0) return {ok: false, error: 'The node is already at the border of its container.'};
    // A grown container's border moved: routes along the old or the new border are affected.
    const alongBorders = [...grownIds].flatMap(([containerId, before]) => [
      ...routesNearBorder(curated.routes, before, NODE_CLEARANCE),
      ...routesNearBorder(curated.routes, nodes[containerId]!, NODE_CLEARANCE),
    ]);
    const notice = clamped && !grow ? 'The node stops at the border of its container. Grow containers extends it.' : undefined;
    return {ok: true, nodes, pending: unique([...touching, ...near, ...alongBorders]), notice};
  }

  /**
   * The routes that a move would give, from the engine, on a draft that is never committed. It serves the live preview
   * while a node is dragged. `null` when there is no such move or the engine cannot route it.
   */
  async previewMoveNodes(nodeIds: string[], dx: number, dy: number, grow = false): Promise<Record<string, Point[]> | null> {
    const plan = this.planMoveNodes(nodeIds, dx, dy, grow);
    if (!plan.ok || plan.pending.length === 0) return null;
    const current = this.document;
    const draft: ReferenceDocument = {
      ...current,
      revision: current.revision + 1,
      curated: {
        ...current.curated,
        nodes: plan.nodes,
        pendingRoutes: unique([...current.curated.pendingRoutes, ...plan.pending]),
      },
    };
    const reply = await this.engine({op: 'referenceReroute', reference: draft});
    if (!reply.ok || !reply.reference) return null;
    return reply.reference.curated.routes;
  }

  /** Shifts one interior orthogonal segment of a route. Endpoints stay where they are. */
  shiftSegment(routeId: string, index: number, delta: number): Promise<EngineReply> {
    const points = this.document.curated.routes[routeId];
    if (!points) return failed(`Edge ${routeId} has no curated route.`);
    if (this.isLocked(routeId)) return failed(`Route ${routeId} is locked.`);
    let moved: Point[];
    try {
      moved = shiftSegment(points, index, delta);
    } catch (error) {
      return failed((error as Error).message);
    }
    return this.commit(document => ({
      ...document,
      curated: {...document.curated, routes: {...document.curated.routes, [routeId]: moved}},
    }));
  }

  /**
   * Adds a dogleg to an interior orthogonal segment: a jog of `depth` perpendicular to it. The endpoints and the
   * arrival sides stay; the route is normalized. Whether the new route is valid is decided by the engine's validation.
   */
  addDogleg(routeId: string, index: number, depth: number): Promise<EngineReply> {
    const points = this.document.curated.routes[routeId];
    if (!points) return failed(`Edge ${routeId} has no curated route.`);
    if (this.isLocked(routeId)) return failed(`Route ${routeId} is locked.`);
    let jogged: Point[];
    try {
      jogged = withDogleg(points, index, depth);
    } catch (error) {
      return failed((error as Error).message);
    }
    return this.commit(document => ({
      ...document,
      curated: {...document.curated, routes: {...document.curated.routes, [routeId]: jogged}},
    }));
  }

  /**
   * Resizes a leaf node to `width` by `height`, never below the minimum leaf box. Its ports stay on their sides, so
   * they move with the box; the routes that touch the old or new box become pending.
   */
  resizeNode(nodeId: string, width: number, height: number): Promise<EngineReply> {
    const current = this.document.curated.nodes[nodeId];
    if (!current) return failed(`Unknown node ${nodeId}`);
    if (this.document.curated.sizePins?.[nodeId]) return failed(`The size of ${nodeId} is pinned. Unpin it to resize.`);
    if (this.isContainer(nodeId)) return failed('Use the container resize for a container.');
    const next: Rect = {
      ...current,
      width: Math.max(MIN_LEAF.width, width),
      height: Math.max(MIN_LEAF.height, height),
    };
    if (next.width === current.width && next.height === current.height) return failed('The node already has that size.');
    const routeIds = Object.keys(this.document.curated.routes);
    const touching = routesTouching(this.document.input.graph.edges, nodeId, routeIds);
    const near = routesNear(this.document.curated.routes, [current, next], NODE_CLEARANCE);
    return this.commit(document => ({
      ...document,
      curated: {
        ...document.curated,
        nodes: {...document.curated.nodes, [nodeId]: next},
        pendingRoutes: unique([...document.curated.pendingRoutes, ...touching, ...near]),
      },
    }));
  }

  /**
   * Resizes a container's boundary only: the boxes inside it keep their geometry. The box never shrinks below the
   * boxes inside it plus padding. Routes along the old or new border become pending.
   */
  resizeContainer(containerId: string, width: number, height: number): Promise<EngineReply> {
    const curated = this.document.curated;
    const current = curated.nodes[containerId];
    if (!current) return failed(`Unknown node ${containerId}`);
    if (curated.sizePins?.[containerId]) return failed(`The size of ${containerId} is pinned. Unpin it to resize.`);
    const members = Object.keys(curated.nodes).filter(id => this.isWithin(id, containerId));
    if (members.length === 0) return failed(`${containerId} is not a container.`);
    const right = Math.max(...members.map(id => curated.nodes[id]!.x + curated.nodes[id]!.width));
    const bottom = Math.max(...members.map(id => curated.nodes[id]!.y + curated.nodes[id]!.height));
    const next: Rect = {
      ...current,
      width: Math.max(right + CONTAINER_PADDING - current.x, width),
      height: Math.max(bottom + CONTAINER_PADDING - current.y, height),
    };
    if (next.width === current.width && next.height === current.height) return failed('The container already has that size.');
    const pending = [
      ...routesNearBorder(curated.routes, current, NODE_CLEARANCE),
      ...routesNearBorder(curated.routes, next, NODE_CLEARANCE),
    ];
    return this.commit(document => ({
      ...document,
      curated: {
        ...document.curated,
        nodes: {...document.curated.nodes, [containerId]: next},
        pendingRoutes: unique([...document.curated.pendingRoutes, ...pending]),
      },
    }));
  }

  /**
   * Locks or unlocks a route. A locked route keeps its geometry: its segments do not move, and no change may re-route
   * it. A route that is pending cannot be locked; re-route it first.
   */
  lockRoute(routeId: string, locked: boolean): Promise<EngineReply> {
    const curated = this.document.curated;
    if (!curated.routes[routeId]) return failed(`Edge ${routeId} has no curated route.`);
    if (this.isLocked(routeId) === locked) return failed(locked ? 'The route is already locked.' : 'The route is not locked.');
    if (locked && curated.pendingRoutes.includes(routeId)) return failed('Re-route the pending route before locking it.');
    const current = [...(curated.lockedRoutes ?? [])];
    const next = locked ? [...current, routeId].sort() : current.filter(id => id !== routeId);
    return this.commit(document => ({...document, curated: {...document.curated, lockedRoutes: next}}));
  }

  /**
   * Moves a label box by (dx, dy). The label keeps its measured size; its position is saved with the document. Label
   * boxes are not measured by the metrics yet, so a moved label does not change the conflict counts.
   */
  moveLabel(labelId: string, dx: number, dy: number): Promise<EngineReply> {
    const label = this.document.baseline.labels.find(item => item.id === labelId);
    if (!label) return failed(`Unknown label ${labelId}`);
    if (dx === 0 && dy === 0) return failed('The label did not move.');
    const at = this.document.curated.labels?.[labelId] ?? {x: label.bounds.x, y: label.bounds.y};
    const next = {x: at.x + dx, y: at.y + dy};
    return this.commit(document => ({
      ...document,
      curated: {...document.curated, labels: {...(document.curated.labels ?? {}), [labelId]: next}},
    }));
  }

  /** Adds a curation note to a node, a route or a label. A note is evidence: it never changes the geometry. */
  addNote(target: string, text: string, category?: string, severity?: string): Promise<EngineReply> {
    const body = text.trim();
    if (!body) return failed('A note needs text.');
    const curated = this.document.curated;
    const known = Boolean(curated.nodes[target] || curated.routes[target]) || this.document.baseline.labels.some(label => label.id === target);
    if (!known) return failed(`Unknown target ${target}`);
    const numbers = this.document.annotations.map(note => Number(note.id.replace(/^note-/, ''))).filter(Number.isFinite);
    const id = `note-${Math.max(0, ...numbers) + 1}`;
    // Category and severity are optional; an empty choice stores nothing, so the engine omits the field.
    const note = {id, target, text: body, ...(category ? {category} : {}), ...(severity ? {severity} : {})};
    return this.commit(document => ({...document, annotations: [...document.annotations, note]}));
  }

  /** Removes every note on a target. */
  removeNotes(target: string): Promise<EngineReply> {
    if (!this.document.annotations.some(note => note.target === target)) return failed('There is no note on this target.');
    return this.commit(document => ({...document, annotations: document.annotations.filter(note => note.target !== target)}));
  }

  /** Pins the current size of a node, or unpins it. A pinned size is kept through resizing. */
  toggleSizePin(nodeId: string): Promise<EngineReply> {
    const curated = this.document.curated;
    const box = curated.nodes[nodeId];
    if (!box) return failed(`Unknown node ${nodeId}`);
    const pins = {...(curated.sizePins ?? {})};
    if (pins[nodeId]) delete pins[nodeId];
    else pins[nodeId] = {width: box.width, height: box.height};
    return this.commit(document => ({...document, curated: {...document.curated, sizePins: pins}}));
  }

  private isLocked(routeId: string): boolean {
    return (this.document.curated.lockedRoutes ?? []).includes(routeId);
  }

  private isContainer(nodeId: string): boolean {
    return Object.keys(this.document.curated.nodes).some(id => this.isWithin(id, nodeId));
  }

  /**
   * Moves a port along its side by `delta` scene units. Explicit ports are locked. The route the port serves
   * becomes pending: its endpoint moves, so it must be re-routed.
   */
  movePort(portId: string, delta: number): Promise<EngineReply> {
    const port = this.document.curated.ports[portId];
    if (!port) return failed(`Unknown port ${portId}`);
    if (port.explicit) return failed('Explicit port settings are locked.');
    const rect = this.document.curated.nodes[port.nodeId];
    if (!rect) return failed(`Port ${portId} has no node.`);
    const horizontal = port.side === 'top' || port.side === 'bottom';
    const length = horizontal ? rect.width : rect.height;
    const offset = clamp(port.offset + delta / length, 0, 1);
    if (offset === port.offset) return failed('The port is already at the end of its side.');
    const low = Math.min(port.offset, offset) + 1e-9;
    const high = Math.max(port.offset, offset) - 1e-9;
    const onSide = Object.entries(this.document.curated.ports).filter(
      ([id, other]) => id !== portId && other.nodeId === port.nodeId && other.side === port.side,
    );
    // Passing an explicit port on the same side is refused; passing an automatic one swaps the two.
    if (onSide.some(([, other]) => other.explicit && other.offset > low && other.offset < high)) {
      return failed('A port cannot move past an explicit port on the same side.');
    }
    const passed = onSide.find(([, other]) => other.offset > low && other.offset < high);
    const ports = {...this.document.curated.ports, [portId]: {...port, offset}};
    if (passed) ports[passed[0]] = {...passed[1], offset: port.offset};
    const served = [port.edgeId, ...(passed ? [passed[1].edgeId] : [])].filter(id => this.document.curated.routes[id]);
    return this.commit(document => ({
      ...document,
      curated: {
        ...document.curated,
        ports,
        pendingRoutes: unique([...document.curated.pendingRoutes, ...served]),
      },
    }));
  }

  /** Re-searches the pending routes in the engine. Their endpoints stay on their ports; the interiors are new. */
  reroute(): Promise<EngineReply> {
    if (this.document.curated.pendingRoutes.length === 0) return failed('No pending routes.');
    return this.change(document => ({...document, revision: document.revision + 1}), 'referenceReroute', false);
  }

  /**
   * Re-routes every route, not only the pending ones, through the same engine operation. Routes that were fine
   * may change: the result is validated like any other change and is one undo step.
   */
  rerouteAll(): Promise<EngineReply> {
    const locked = new Set(this.document.curated.lockedRoutes ?? []);
    const routes = Object.keys(this.document.curated.routes).filter(id => !locked.has(id));
    if (routes.length === 0) return failed('No routes present.');
    return this.change(
      document => ({
        ...document,
        revision: document.revision + 1,
        curated: {...document.curated, pendingRoutes: routes},
      }),
      'referenceReroute',
      false,
    );
  }

  undo(): boolean {
    this.generation++;
    return this.history.undo();
  }

  redo(): boolean {
    this.generation++;
    return this.history.redo();
  }

  /** The canonical JSON text of the current document (validated by the engine first). */
  async canonical(): Promise<string> {
    const reply = await this.engine({op: 'referenceCanonical', reference: this.document});
    if (!reply.ok || reply.canonical === undefined) throw Error(reply.error ?? 'Saving is not possible.');
    return reply.canonical;
  }

  /**
   * The scene shown while curating: the baseline's geometry replaced by the curated geometry. Routes that are
   * pending are drawn as provisional connectors between their node centres. Ports are the curated ones. Labels and
   * crossings belong to the baseline layout, so they are not shown while curating.
   */
  scene(baseline: Scene): Scene {
    const {nodes, routes, pendingRoutes} = this.document.curated;
    const pending = new Set(pendingRoutes);
    const bounds = (id: string, fallback: Rect): Rect => nodes[id] ?? fallback;
    const byId = new Map(baseline.nodes.map(node => [node.id, node]));
    // A pending route is drawn between its two ports, which lie on the node sides. Centre to centre would run
    // under the node boxes and disappear.
    const attachment = (routeId: string, endpoint: string): Point | undefined =>
      Object.values(this.document.curated.ports).find(port => port.edgeId === routeId && port.endpoint === endpoint)
        ?.position;
    const centres = (routeId: string, fallback: Point[]): Point[] => {
      const sourcePort = attachment(routeId, 'source');
      const targetPort = attachment(routeId, 'target');
      if (sourcePort && targetPort) return [sourcePort, targetPort];
      const route = baseline.routes.find(item => item.id === routeId);
      if (!route) return fallback;
      const source = byId.get(route.source);
      const target = byId.get(route.target);
      if (!source || !target) return fallback;
      const from = bounds(source.id, source.bounds);
      const to = bounds(target.id, target.bounds);
      return [centre(from), centre(to)];
    };
    const placedPoints = Object.values(routes).flat();
    return {
      ...baseline,
      // The view grows with the curation: a grown container or a route the curator moved must stay in the viewport.
      bounds: boundsAround(baseline.bounds, Object.values(nodes), placedPoints, 24),
      nodes: baseline.nodes.map(node => ({...node, bounds: bounds(node.id, node.bounds)})),
      routes: baseline.routes.map(route => ({
        ...route,
        points: pending.has(route.id) ? centres(route.id, route.points) : (routes[route.id] ?? route.points),
      })),
      ports: Object.entries(this.document.curated.ports).map(([id, port]) => ({
        id,
        nodeId: port.nodeId,
        edgeId: port.edgeId,
        endpoint: port.endpoint,
        side: port.side,
        position: port.position,
        explicit: port.explicit,
      })),
      crossings: [],
      // A label keeps its measured size; a moved label sits where the curator put it.
      labels: baseline.labels.map(label => {
        const at = this.document.curated.labels?.[label.id];
        return at ? {...label, bounds: {...label.bounds, x: at.x, y: at.y}} : label;
      }),
    };
  }

  /** A node that a grown container would newly overlap: not inside the container, and not one of its ancestors. */
  private newCollision(containerId: string, before: Rect, after: Rect): string | null {
    for (const [id, rect] of Object.entries(this.document.curated.nodes)) {
      if (id === containerId || this.isWithin(id, containerId) || this.isWithin(containerId, id)) continue;
      if (overlaps(after, rect) && !overlaps(before, rect)) return id;
    }
    return null;
  }

  /** Whether `nodeId` lies inside `containerId`, at any depth. */
  private isWithin(nodeId: string, containerId: string): boolean {
    for (let parentId = this.parentOf(nodeId); parentId; parentId = this.parentOf(parentId)) {
      if (parentId === containerId) return true;
    }
    return false;
  }

  private parentOf(nodeId: string): string | null {
    return this.document.input.graph.nodes.find(item => item.id === nodeId)?.parentId ?? null;
  }

  /** The containers of a node, from its direct parent upwards, with their curated rectangles. */
  private ancestors(nodeId: string): {id: string; rect: Rect}[] {
    return this.ancestorsIn(this.document.curated.nodes, nodeId);
  }

  /** The containers of a node in the given node map, from its direct parent upwards. */
  private ancestorsIn(nodes: Record<string, Rect>, nodeId: string): {id: string; rect: Rect}[] {
    const found: {id: string; rect: Rect}[] = [];
    let parentId = this.document.input.graph.nodes.find(item => item.id === nodeId)?.parentId ?? null;
    while (parentId) {
      const rect = nodes[parentId];
      if (!rect) break;
      found.push({id: parentId, rect});
      parentId = this.document.input.graph.nodes.find(item => item.id === parentId)?.parentId ?? null;
    }
    return found;
  }

  /**
   * Moves a container with everything inside it. The descendants keep their relative geometry: they, and the routes
   * that lie wholly inside the container, translate by the same offset. Routes that cross its border or attach to a
   * moved node become pending, because their endpoints or obstacles changed. `grow` lets the ancestors grow when the
   * container leaves them, as for a node.
   */
  moveContainer(containerId: string, dx: number, dy: number, grow = false): Promise<EngineReply> {
    const curated = this.document.curated;
    const current = curated.nodes[containerId];
    if (!current) return failed(`Unknown node ${containerId}`);
    const members = Object.keys(curated.nodes).filter(id => this.isWithin(id, containerId));
    if (members.length === 0) return failed(`${containerId} is not a container.`);
    const ancestors = this.ancestors(containerId);
    const {rect, grown} = movedWithin(current, dx, dy, ancestors.map(item => item.rect), grow);
    const grownContainers = ancestors.filter((item, index) => resized(item.rect, grown[index]!));
    for (const item of grownContainers) {
      const after = grown[ancestors.indexOf(item)]!;
      const collision = this.newCollision(item.id, item.rect, after);
      if (collision) return failed(`Growing ${item.id} would overlap ${collision}. Move ${collision} or the container first.`);
    }
    const shiftX = rect.x - current.x;
    const shiftY = rect.y - current.y;
    if (shiftX === 0 && shiftY === 0 && grownContainers.length === 0) {
      return failed('The container is already at the border of its parent.');
    }
    const nodes = {...curated.nodes};
    for (const id of members) nodes[id] = {...nodes[id]!, x: nodes[id]!.x + shiftX, y: nodes[id]!.y + shiftY};
    nodes[containerId] = rect;
    ancestors.forEach((item, index) => {
      if (resized(item.rect, grown[index]!)) nodes[item.id] = grown[index]!;
    });
    const inside = (p: Point) =>
      p.x >= current.x && p.x <= current.x + current.width && p.y >= current.y && p.y <= current.y + current.height;
    const moving = new Set([containerId, ...members]);
    const routes = {...curated.routes};
    const translated = new Set<string>();
    const pending = new Set<string>();
    for (const [routeId, points] of Object.entries(curated.routes)) {
      if (points.every(inside)) {
        routes[routeId] = points.map(p => ({x: p.x + shiftX, y: p.y + shiftY}));
        translated.add(routeId);
        continue;
      }
      const edge = this.document.input.graph.edges.find(item => item.id === routeId);
      if (edge && (moving.has(edge.source) || (edge.target !== null && moving.has(edge.target)))) pending.add(routeId);
    }
    // Routes that run along the old or the new border of the container are affected by its move.
    for (const routeId of [...routesNearBorder(curated.routes, current, NODE_CLEARANCE), ...routesNearBorder(curated.routes, rect, NODE_CLEARANCE)]) {
      if (!translated.has(routeId)) pending.add(routeId);
    }
    return this.commit(document => ({
      ...document,
      curated: {
        ...document.curated,
        nodes,
        routes,
        pendingRoutes: unique([...document.curated.pendingRoutes, ...pending]),
      },
    }));
  }

  /**
   * Moves an automatic port to another side of its node. The port keeps its share of the side. The route it serves
   * becomes pending, because its endpoint moves. Explicit ports are locked.
   */
  movePortSide(portId: string, side: PortSide): Promise<EngineReply> {
    const port = this.document.curated.ports[portId];
    if (!port) return failed(`Unknown port ${portId}`);
    if (port.explicit) return failed('Explicit port settings are locked.');
    if (port.side === side) return failed('The port already sits on that side.');
    const served = this.document.curated.routes[port.edgeId] ? [port.edgeId] : [];
    return this.commit(document => ({
      ...document,
      curated: {
        ...document.curated,
        ports: {...document.curated.ports, [portId]: {...port, side}},
        pendingRoutes: unique([...document.curated.pendingRoutes, ...served]),
      },
    }));
  }

  /** One edit of the curated geometry: refreshed by the engine, validated, then made the present state. */
  private commit(next: (document: ReferenceDocument) => ReferenceDocument): Promise<EngineReply> {
    return this.change(next, 'referenceRefresh');
  }

  /** One change: the engine computes the next document (refresh or re-route), then it becomes the present state. */
  private change(
    next: (document: ReferenceDocument) => ReferenceDocument,
    op: 'referenceRefresh' | 'referenceReroute',
    dependsOnState = true,
  ): Promise<EngineReply> {
    // The change was computed from the document as it is now. If it waits and the document moves on, it is stale.
    const base = {revision: this.document.revision, generation: this.generation};
    const run = this.queue.then(() => this.apply(next, op, dependsOnState, base));
    this.queue = run.catch(() => undefined);
    return run;
  }

  private async apply(
    next: (document: ReferenceDocument) => ReferenceDocument,
    op: 'referenceRefresh' | 'referenceReroute',
    dependsOnState: boolean,
    base: {revision: number; generation: number},
  ): Promise<EngineReply> {
    const generation = base.generation;
    try {
      if (generation !== this.generation) return failed('The change was discarded because undo or redo ran in the meantime.');
      if (dependsOnState && base.revision !== this.document.revision) {
        return failed('The document changed while this change waited. Try again.');
      }
      const current = this.document;
      const draft = next({...current, revision: current.revision + 1});
      const locked = new Set(draft.curated.lockedRoutes ?? []);
      const blocked = draft.curated.pendingRoutes.find(id => locked.has(id));
      if (blocked) throw Error(`Route ${blocked} is locked: unlock it before this change re-routes it.`);
      const refreshed = requireDocument(await this.engine({op, reference: draft}));
      const report = await validateWith(this.engine, refreshed);
      // Undo or redo happened while the engine worked: the result was computed for a state that is gone.
      if (generation !== this.generation) return failed('The change was discarded because undo or redo ran in the meantime.');
      this.history.commit({document: refreshed, report});
      return {ok: true, reference: refreshed, report};
    } catch (error) {
      return failed((error as Error).message);
    }
  }
}

function requireDocument(reply: EngineReply): ReferenceDocument {
  if (!reply.ok || !reply.reference) throw Error(reply.error ?? 'The reference could not be created.');
  return reply.reference;
}

async function validateWith(engine: Engine, document: ReferenceDocument): Promise<Report> {
  const reply = await engine({op: 'referenceValidate', reference: document});
  if (!reply.ok || !reply.report) throw Error(reply.error ?? 'Validation failed.');
  return reply.report;
}

function failed(error: string): Promise<EngineReply> {
  return Promise.resolve({ok: false, error});
}

function unique(values: string[]): string[] {
  return [...new Set(values)];
}

function clamp(value: number, low: number, high: number): number {
  return Math.min(Math.max(value, low), high);
}
