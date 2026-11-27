import type {DetailsAnswer, FindAnswer, Graph, GraphNeighbourhood, GraphPatch, GraphTrace, Hidden, NeighbourhoodAnswer, Overview, OverviewRequest, Scene, Snapshot, TraceAnswer} from '@arqlet/contracts';
import type {SceneColumns} from './columns.js';
import type {EngineRequest, EngineResponse, Run} from './wasm-client.js';

/** An engine answer as the session reports it. `stale`: an import or reset overtook the change, nothing was applied. */
/** Provisional (drag API): a move gesture of `Session.drag`. */
export interface DragHandle {
  /** The node being moved. */
  readonly node: string;
  /** Whether the gesture still takes positions (not dropped, cancelled or refused). */
  readonly active: boolean;
  /** Steps sent, positions replaced by a newer one before they were sent, and steps the engine applied. */
  readonly stats: {sent: number; coalesced: number; applied: number};
  /** The answer to picking the node up (the first step). */
  readonly begun: Promise<SessionResult>;
  /**
   * A newer position (and optionally newer positions of the others). Positions that arrive while one is computed replace
   * each other: only the newest is sent. Resolves with the answer of the step that carried it, or `{ok: true, unchanged}`
   * when a newer position took its place.
   */
  move(position: {x: number; y: number}, others?: Record<string, {x: number; y: number}>): Promise<SessionResult>;
  /**
   * Applies the last position and ends the gesture; the node stays pinned there, or with `release` the layout may move it
   * again. With `settle`, force-directed areas settle afterwards (`op: settle`).
   */
  drop(init?: {release?: boolean; settle?: boolean}): Promise<SessionResult>;
  /** Ends the gesture and puts the node (and the others) back where they were. */
  cancel(): Promise<SessionResult>;
}

export interface SessionResult {
  ok: boolean;
  snapshot?: Snapshot;
  error?: string;
  stale?: boolean;
  /** `expand`/`collapse`: the container was in that state already; no engine call was made. */
  unchanged?: boolean;
}

/** Provisional (ADR 0023, R1): one change of a session's content. Values are JSON; an integer stays an integer. */
export interface PayloadChange {
  set?: {nodes?: Record<string, Record<string, unknown>>; edges?: Record<string, Record<string, unknown>>};
  unset?: {nodes?: Record<string, string[]>; edges?: Record<string, string[]>};
  clear?: boolean;
  expectedContentRevision?: number;
}

/** Provisional (ADR 0023, R7): what a session holds, in estimated heap bytes per part. */
export interface SessionMemory {
  ok: true;
  estimated: true;
  parts: Record<string, number>;
  total: number;
  nodes: number;
  perNode: number;
  /** From the WASM engine: the module's live heap, counted. */
  heapBytes?: number;
  /** From the WASM engine: the size of its linear memory, which is never given back. */
  linearMemoryBytes?: number;
}

/** Provisional (ADR 0023, R2): one change group: a patch of the structure, a payload of the content, or both. */
export interface ChangeGroup {
  patch?: GraphPatch;
  payload?: Omit<PayloadChange, 'expectedContentRevision'>;
  expectedSequence?: number;
}

/** The content's revision and counts after a change; ids the structure does not have are kept and counted. */
export interface PayloadAnswer {
  ok: true;
  contentRevision: number;
  nodes: number;
  edges: number;
  keys: string[];
  unknownNodes: number;
  unknownEdges: number;
}

/** Provisional: a look into a container (`peek`): its contents as a scene, and the border edges per child shown. */
export interface PeekAnswer {
  nodeId: string;
  children: number;
  shown: number;
  omitted: number;
  external: Record<string, {incoming: number; outgoing: number}>;
  scene: Scene;
}

/** What `details` asks about: a node, or an edge (a drawn line or an edge of the source graph). */
export type DetailTarget = {nodeId: string; edgeId?: never} | {edgeId: string; nodeId?: never};

/** The answer of a read-only request (`trace`, `neighbourhood`): its fields beside `ok: true`, or the error. */
/** Provisional (ADR 0021, M5b): what a query is given besides its text. */
export interface QueryInit {
  /** The values of the `$name` parameters (JSON values; an object is a map). */
  params?: Record<string, unknown>;
  /** Also map the answer onto the current view (`shown`); costs a few milliseconds on a large view. */
  view?: boolean;
  /** Execution limits (steps of work, rows held, hops of an unbounded `*`). */
  maxSteps?: number;
  maxRows?: number;
  maxHops?: number;
}

/**
 * A value in a row of a query answer: a JSON value or a list. Every object is tagged: a node is `{node: id}`, an edge
 * `{edge: id}`, a map `{map: {key: value}}`.
 */
export type QueryValue =
  | null
  | boolean
  | number
  | string
  | {node: string}
  | {edge: string}
  | {map: {[key: string]: QueryValue}}
  | QueryValue[];

/** Provisional (ADR 0021, M5b): the answer of a query. */
export interface QueryAnswer {
  columns: string[];
  rows: QueryValue[][];
  rowCount: number;
  /** The ids of the nodes and edges in the rows, once each, in the order they first appear. */
  nodes: string[];
  edges: string[];
  /** What the query could see: the session's source graph (resolved edges only). */
  graph: {nodes: number; edges: number};
  /** The content's revision, when the session has content. */
  contentRevision?: number;
  /** Content for ids the graph does not have (kept, ignored by the query). */
  ignoredContent?: number;
  /** Content values the query layer has no type for (an integer beyond 64 bits), read as null. */
  skippedValues?: number;
  /** With `view: true`: the box that shows each node and the drawn line that carries each edge (null when none). */
  shown?: {nodes: Record<string, string | null>; edges: Record<string, string | null>};
}

export type ReadAnswer<T> = ({ok: true} & T) | {ok: false; error: string};

/** Provisional: one graph, its current snapshot, and changes applied in order with revision checks. */
export interface Session {
  /**
   * The current snapshot, or null before the first import. With a resident session only its state (`revision`,
   * `layoutVersion`, `collapsedIds`); `exportSnapshot()` gives the whole snapshot.
   */
  readonly snapshot: Snapshot | null;
  /**
   * Lays out `graph` (nodes with `parentId` are containers) and starts the session on it; changes still under way are
   * dropped. `limit`: the most nodes the first view selects (default: all, at most `MAX_VIEW_LIMIT`). `options`: layout
   * options.
   */
  import(graph: Graph, init?: {limit?: number; options?: Record<string, unknown>}): Promise<SessionResult>;
  /** Drops the snapshot; changes still under way resolve as stale. */
  /**
   * Provisional: continues from `snapshot` (made by the host), after the changes queued before; with `reset` (another
   * view) at once, the changes still under way resolving as stale.
   */
  load(snapshot: Snapshot, options?: {reset?: boolean}): Promise<SessionResult>;
  /** Provisional (#148): keeps the current snapshot under a mark (the same mark while it is unchanged); at most `limit` (default 32). */
  keep(options?: {limit?: number}): Promise<{ok: boolean; mark?: number; error?: string}>;
  /**
   * Provisional (#148): the snapshot kept under `mark` becomes current again with its geometry (next layout version);
   * `evicted` when it is no longer kept, `changed` when the graph changed since.
   */
  recall(mark: number): Promise<SessionResult & {evicted?: boolean; changed?: boolean}>;
  reset(): void;
  /** Whether container `id` is collapsed in the current snapshot. */
  isCollapsed(id: string): boolean;
  /** Collapses or expands container `id`. */
  toggle(id: string): Promise<SessionResult>;
  /** Expands container `id` unless it is open. */
  expand(id: string): Promise<SessionResult>;
  /** Collapses container `id` unless it is collapsed. */
  collapse(id: string): Promise<SessionResult>;
  /** Any other change of the protocol (`direction`, `configure`, `add`, …); snapshot and expected versions are added. */
  apply(request: Omit<EngineRequest, 'snapshot' | 'expectedRevision' | 'expectedLayoutVersion'> & {op: string}): Promise<SessionResult>;
  /**
   * Provisional (path cut-out): the paths between two nodes of the current snapshot, read only. `visible` names the boxes
   * and routes of the current view that carry them; `distance` the hops of every visible node from the path.
   * `edgeKinds`: the paths follow only edges of these kinds (default: every edge).
   */
  trace(from: string, to: string, init?: {mode?: 'shortest' | 'all'; edgeKinds?: string[]}): Promise<ReadAnswer<TraceAnswer>>;
  /**
   * Provisional: the nodes within `hops` edges (default 1) of a node or of each of a list, read only, answered as
   * `trace` (`nodes`, `edges`, `visible`, `distance`) plus the `hops` of every node; `nodes` can go to `reveal`.
   * `direction`: `out` (what it leads to), `in` (what leads to it) or `both` (default); `edgeKinds`: only these kinds.
   */
  neighbourhood(
    nodeId: string | string[],
    init?: {hops?: number; direction?: 'out' | 'in' | 'both'; edgeKinds?: string[]},
  ): Promise<ReadAnswer<NeighbourhoodAnswer>>;
  /**
   * Provisional (read only): what the engine knows about a node or an edge (a drawn line or an edge of the source graph):
   * the containers above a node and the box that shows it, what a collapsed box holds, the edges a line stands for.
   */
  details(target: DetailTarget, init?: {limit?: number}): Promise<ReadAnswer<DetailsAnswer>>;
  /**
   * Provisional (read only): the nodes whose label or id contains `query` (ignoring case), best first, with the box that
   * shows each; at most `limit` (default 20), only `nodeKinds` if given.
   */
  find(query: string, init?: {limit?: number; nodeKinds?: string[]; key?: string}): Promise<ReadAnswer<FindAnswer>>;
  /**
   * Provisional (ADR 0021, M5b; read only): an openCypher query over the session's source graph and its content. Needs an
   * engine built with the cargo feature `query`; otherwise `{ok: false, error}` says so. See {@link QueryAnswer}.
   */
  query(text: string, init?: QueryInit): Promise<ReadAnswer<QueryAnswer>>;
  /**
   * Provisional (ADR 0023, R1): sets, unsets or clears properties in the session's content (beside the snapshot, never in
   * it). Needs a resident session.
   */
  payload(change: PayloadChange): Promise<PayloadAnswer | {ok: false; error?: string; stale?: true}>;
  /** Provisional (ADR 0023, R2): structure and content changed as one group; the answer has its `sequence`. */
  change(group: ChangeGroup): Promise<SessionResult & {sequence?: number; contentRevision?: number}>;
  /** Provisional (ADR 0023, R2): the change groups after `sequence`, or `resync` when the log no longer has them. */
  changesSince(sequence: number): Promise<{ok: true; sequence: number; changes?: Array<ChangeGroup & {sequence: number}>; resync?: true} | {ok: false; error?: string}>;
  /**
   * Provisional (ADR 0023, R7): moves a resident session to another engine (a new worker) so that the old one can be
   * ended and its memory comes back; the view, the content and the change log's number move with it.
   */
  recycle(newEngine: Run): Promise<{ok: true; unchanged: true; sequence: number; contentRevision: number} | {ok: false; error?: string; stale?: true}>;
  /** Provisional (ADR 0023, R7): what the session holds, part by part (estimated), and from WASM the module's heap. */
  memory(): Promise<SessionMemory | {ok: false; error?: string}>;
  /** Provisional (ADR 0023, R7): gives parts back; the snapshot stays. */
  release(parts: Array<'content' | 'changeLog' | 'columns' | 'overview'>, init?: {keys?: string[]}): Promise<(SessionMemory & {released: number}) | {ok: false; error?: string; stale?: true}>;
  /** Provisional (ADR 0023, R2): the whole content and the log's sequence (for a resync). */
  content(): Promise<{ok: true; contentRevision: number; sequence: number; nodes: Record<string, Record<string, unknown>>; edges: Record<string, Record<string, unknown>>} | {ok: false; error?: string}>;
  /** Provisional (read only): a look into a collapsed container without opening it. */
  peek(nodeId: string, init?: {limit?: number}): Promise<ReadAnswer<PeekAnswer>>;
  /**
   * Provisional: changes the graph without a new import (removals first; a removed node takes what it holds and its
   * edges along); the view keeps its state and is laid out as a toggle lays it out (abstracting views and views with
   * buses) or again from where it was.
   */
  patch(patch: GraphPatch): Promise<SessionResult>;
  /**
   * Provisional (drag API): pins `nodeId` at `position` (in its parent's frame, as `toReactFlow` gives `position`), or
   * where it is now. The layout keeps a pinned node in place.
   */
  pin(nodeId: string, position?: {x: number; y: number}): Promise<SessionResult>;
  /** Provisional (drag API): releases the pin of `nodeId`; the layout may move it again. */
  unpin(nodeId: string): Promise<SessionResult>;
  /**
   * Provisional (drag API): starts moving leaf `nodeId` to `position` (in its parent's frame). `others` maps further
   * nodes moved with it (a selection) to their positions; the set is fixed for the gesture. Each applied step changes the
   * session's snapshot (`onChange`); other changes wait until the gesture ends.
   */
  drag(nodeId: string, position: {x: number; y: number}, init?: {others?: Record<string, {x: number; y: number}>}): DragHandle;
  /** Provisional: opens the groups that hold `nodeIds`; with `partial` (default true) only as far as needed. */
  reveal(nodeIds: string[], init?: {partial?: boolean}): Promise<SessionResult>;
  /** Provisional: hides nodes by kind or id (with what they hold) and edges by kind, keeping the place of the rest, or
   * (`compact: true`) laying the view out again without them; `hide` replaces what was hidden before, `{}` shows everything. */
  filter(hide?: Hidden): Promise<SessionResult>;
  /** The scene of the current snapshot, computed once per snapshot. Throws the engine's error. */
  scene(): Promise<Scene>;
  /**
   * Provisional (ADR 0019, step 4): the scene of the current snapshot as columns. In a resident WASM session called
   * directly, the arrays are views into the engine's memory, valid until the next request to the session (copy what
   * you keep); through a worker they are copies transferred once; otherwise they are built from the scene.
   */
  columns(): Promise<SceneColumns>;
  /** The whole snapshot: the current one, or with `resident` the one the engine keeps (for saving or hand-over). */
  exportSnapshot(): Promise<Snapshot | null>;
  /** Whether the snapshot stays in the engine. */
  readonly resident: boolean;
  /** Ends a resident session in the engine (frees its snapshot); the session is not usable afterwards. */
  close(): Promise<void>;
}

/** A session whose snapshot stays in the engine (`openSession`); requests leave the snapshot out. Provisional. */
export interface ResidentSession {
  run(request: EngineRequest): EngineResponse | Promise<EngineResponse>;
  close(): unknown;
  /** Provisional (ADR 0019, step 4): the scene as columns, where the engine keeps them (WASM, a worker). */
  columns?(): SceneColumns | Promise<SceneColumns>;
  /** Provisional (step 4): an `overview` request answered with typed arrays for the numbers, where the engine can. */
  overview?(request: EngineRequest, init?: {copy?: boolean}): EngineResponse | Promise<EngineResponse>;
}

/** Creates a session on an engine `run`, synchronous (`loadEngine`) or asynchronous (`workerRun`). */
export function createSession(
  run: (Run | ((request: EngineRequest) => Promise<EngineResponse>)) & {
    openSession?: () => ResidentSession | Promise<ResidentSession>;
  },
  init?: {
    onChange?: (snapshot: Snapshot) => void;
    /**
     * Provisional (ADR 0019): keep the snapshot in the engine. Requests carry only the change and answers bring the
     * scene; `snapshot` is then the session's state (revision, layout version, collapsed containers). `'auto'` (default):
     * where the engine can (`openSession`); `true`: always; `false`: never.
     */
    resident?: boolean | 'auto';
  },
): Session;

/**
 * Wraps a worker of `createEngineWorker` as an asynchronous run. `onPublication` receives the progressive publications
 * of a running request.
 */
/** Provisional: what `createViewController({refine})` runs refinements with. */
export type RefineRun = (
  request: EngineRequest,
  init?: {onPublication?: (publication: {jobId: string; sequence: number; snapshot: Snapshot}) => void; signal?: AbortSignal},
) => Promise<EngineResponse>;

/** Provisional: a refinement engine on its own worker, with each progressive request's intermediate results. */
export function workerRefineRun(worker: Worker): RefineRun;

export function workerRun(
  worker: Worker,
  init?: {onPublication?: (requestId: number, message: unknown) => void},
): ((request: EngineRequest) => Promise<EngineResponse>) & {openSession(): Promise<ResidentSession>};

/**
 * Provisional: a backend as an asynchronous run (`${base}/op`), with sessions kept in the backend
 * (`${base}/session`). `fetchImpl` replaces `fetch`.
 */
export function httpRun(
  base?: string,
  init?: {fetchImpl?: typeof fetch},
): ((request: EngineRequest) => Promise<EngineResponse>) & {openSession(): Promise<ResidentSession>};

/**
 * An overview request without `op` and `graph` (`OverviewRequest` in `@arqlet/contracts`): `start` without positions
 * continues from the last overview, `start.nodes` (places by node id) from the places given, also on a new graph, and
 * keeps their clusters.
 */
export type OverviewSessionRequest = Omit<OverviewRequest, 'graph'>;

/** The answer to an overview request: `overview` (ids, parent, x, y, r, labels, kinds, …) when `ok`. */
export interface OverviewAnswer {
  ok: boolean;
  overview?: Overview;
  error?: string;
  /** A new `open` overtook it; nothing was applied. */
  stale?: boolean;
}

/** Provisional: an overview of one graph, laid out again as the host asks; the graph stays in the engine where it can. */
export interface OverviewSession {
  /** Lays out `graph`; replaces the graph the session had. */
  open(graph: Graph, request?: OverviewSessionRequest): Promise<OverviewAnswer>;
  /** Lays out the graph again; `start` without positions continues from the last overview (same cluster options).
   * With `edgeData: true` the answer has each edge's kind and weight (ask each time). */
  layOut(request?: OverviewSessionRequest): Promise<OverviewAnswer>;
  /** Provisional: the paths between two nodes of the overview's graph, read only (`GraphTrace`). */
  trace(from: string, to: string, init?: {mode?: 'shortest' | 'all'; edgeKinds?: string[]}): Promise<ReadAnswer<GraphTrace>>;
  /** Provisional: the nodes within `hops` of a node (or of each of a list) of the overview's graph, read only. */
  neighbourhood(
    nodeId: string | string[],
    init?: {hops?: number; direction?: 'out' | 'in' | 'both'; edgeKinds?: string[]},
  ): Promise<ReadAnswer<GraphNeighbourhood>>;
  /** The last overview, or null. */
  readonly overview: OverviewAnswer['overview'] | null;
  /** Whether the graph stays in the engine. */
  readonly resident: boolean;
  /** Ends the session and frees the graph in the engine. */
  close(): Promise<void>;
}

/** Creates an overview session on an engine `run`; `resident` as for `createSession`. */
export function createOverviewSession(
  run: (Run | ((request: EngineRequest) => Promise<EngineResponse>)) & {
    openSession?: () => ResidentSession | Promise<ResidentSession>;
  },
  init?: {
    resident?: boolean | 'auto';
    /**
     * Provisional (ADR 0019, step 4): the overview's numbers (`parent`, `x`, `y`, `r`, `kind`, `edges`, `links`, and
     * with `edgeData` `edgeKind` and `edgeWeight`) as typed arrays instead of JSON arrays, moved without JSON (copied once out of the engine's memory, transferred from a
     * worker). Needs a resident session in an engine that offers `overview` (WASM, a worker); an answer says so
     * otherwise. Default false: the arrays are plain, as before. Typed arrays index like arrays; a `start` built from
     * them is sent as plain arrays.
     */
    columns?: boolean;
  },
): OverviewSession;

/** The most nodes an import selects for its view; the default of `Session.import`'s limit. */
export const MAX_VIEW_LIMIT: number;

