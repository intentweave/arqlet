import type {Graph, Scene, Snapshot} from '@arqlet/contracts';
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
  /** The current snapshot, or null before the first import. */
  readonly snapshot: Snapshot | null;
  /**
   * Lays out `graph` (nodes with `parentId` are containers) and starts the session on it; changes still under way are
   * dropped. `limit`: the most nodes the first view shows (default: all). `options`: layout options.
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
  /** The scene of the current snapshot, computed once per snapshot. Throws the engine's error. */
  scene(): Promise<Scene>;
}

/** Creates a session on an engine `run`, synchronous (`loadEngine`) or asynchronous (`workerRun`). */
export function createSession(
  run: Run | ((request: EngineRequest) => Promise<EngineResponse>),
  init?: {onChange?: (snapshot: Snapshot) => void},
): Session;

/**
 * Wraps a worker of `createEngineWorker` as an asynchronous run. `onPublication` receives the progressive publications
 * of a running request.
 */
export function workerRun(
  worker: Worker,
  init?: {onPublication?: (requestId: number, message: unknown) => void},
): (request: EngineRequest) => Promise<EngineResponse>;
