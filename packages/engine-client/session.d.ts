import type {Graph, GraphNeighbourhood, GraphPatch, GraphTrace, Hidden, NeighbourhoodAnswer, Overview, OverviewRequest, Scene, Snapshot, TraceAnswer} from '@arqlet/contracts';
import type {EngineRequest, EngineResponse, Run} from './wasm-client.js';

/** An engine answer as the session reports it. `stale`: an import or reset overtook the change, nothing was applied. */
export interface SessionResult {
  ok: boolean;
  snapshot?: Snapshot;
  error?: string;
  stale?: boolean;
  /** `expand`/`collapse`: the container was in that state already; no engine call was made. */
  unchanged?: boolean;
}

/** The answer of a read-only request (`trace`, `neighbourhood`): its fields beside `ok: true`, or the error. */
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
   * Provisional: changes the graph without a new import (removals first; a removed node takes what it holds and its
   * edges along); the view keeps its state and is laid out as a toggle lays it out (abstracting views) or again from
   * where it was.
   */
  patch(patch: GraphPatch): Promise<SessionResult>;
  /** Provisional: opens the groups that hold `nodeIds`; with `partial` (default true) only as far as needed. */
  reveal(nodeIds: string[], init?: {partial?: boolean}): Promise<SessionResult>;
  /** Provisional: hides nodes by kind or id (with what they hold) and edges by kind, keeping the place of the rest, or
   * (`compact: true`) laying the view out again without them; `hide` replaces what was hidden before, `{}` shows everything. */
  filter(hide?: Hidden): Promise<SessionResult>;
  /** The scene of the current snapshot, computed once per snapshot. Throws the engine's error. */
  scene(): Promise<Scene>;
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
 * continues from the last overview.
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
  /** Lays out the graph again; `start` without positions continues from the last overview (same cluster options). */
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
  init?: {resident?: boolean | 'auto'},
): OverviewSession;

/** The most nodes an import selects for its view; the default of `Session.import`'s limit. */
export const MAX_VIEW_LIMIT: number;

