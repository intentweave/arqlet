import type {Graph, Hidden, Scene, Snapshot} from '@arqlet/contracts';
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
   */
  trace(from: string, to: string, init?: {mode?: 'shortest' | 'all'}): Promise<EngineResponse>;
  /** Provisional: opens the groups that hold `nodeIds`; with `partial` (default true) only as far as needed. */
  reveal(nodeIds: string[], init?: {partial?: boolean}): Promise<SessionResult>;
  /** Provisional: hides nodes by kind or id (with what they hold) and edges by kind, keeping the place of the rest;
   * `hide` replaces what was hidden before, `{}` shows everything. */
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

/** The most nodes an import selects for its view; the default of `Session.import`'s limit. */
export const MAX_VIEW_LIMIT: number;

