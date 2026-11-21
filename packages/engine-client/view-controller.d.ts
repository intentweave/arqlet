import type {Graph, Hidden, NeighbourhoodAnswer, Scene, Snapshot, TraceAnswer} from '@arqlet/contracts';
import type {ReadAnswer, Session, SessionResult} from './session.js';
import type {EngineRequest} from './wasm-client.js';

/** What changed between two scenes, by id. */
export interface SceneDiff {
  nodes: {added: string[]; removed: string[]; changed: string[]};
  routes: {added: string[]; removed: string[]; changed: string[]};
}

/** Provisional: the highlight a view keeps until it is cleared; asked again after every change. */
export type ViewHighlight =
  | {kind: 'trace'; from: string; to: string; init?: {mode?: 'shortest' | 'all'; edgeKinds?: string[]}; answer: {ok: true} & TraceAnswer}
  | {
      kind: 'neighbourhood';
      nodeId: string | string[];
      init?: {hops?: number; direction?: 'out' | 'in' | 'both'; edgeKinds?: string[]};
      answer: {ok: true} & NeighbourhoodAnswer;
    };

/** Provisional: the state of a view controller (frozen; a new object after every change). */
export interface ViewState {
  /** The session's snapshot (with a resident session only its state). */
  readonly snapshot: Snapshot | null;
  /** The scene to draw. */
  readonly scene: Scene | null;
  /** What the last step changed in the scene, for animation. */
  readonly diff: SceneDiff | null;
  /** Whether a step is running or queued. */
  readonly busy: boolean;
  /** The last error, cleared by the next successful step. */
  readonly error: string | null;
  readonly highlight: ViewHighlight | null;
  /** What the view hides (the last `filter`). */
  readonly hide: Hidden;
  readonly canBack: boolean;
  readonly canForward: boolean;
  /** How many steps changed the scene since the import. */
  readonly steps: number;
}

/** A step's answer: the engine's, or `cancelled` when `cancel()` dropped it before it started. */
export type ViewResult = SessionResult & {cancelled?: boolean};

/**
 * Provisional (request 31): a headless view controller over a layout session. `subscribe` and `getState` fit React's
 * `useSyncExternalStore`. Steps run one after the other.
 */
export interface ViewController {
  getState(): ViewState;
  subscribe(listener: () => void): () => void;
  import(graph: Graph, init?: {limit?: number; options?: Record<string, unknown>}): Promise<ViewResult>;
  open(id: string): Promise<ViewResult>;
  close(id: string): Promise<ViewResult>;
  toggle(id: string): Promise<ViewResult>;
  /** The next `count` (default `page`) children of a `+N more` box; `container` or `rest:<container>`. */
  showRest(container: string, count?: number): Promise<ViewResult>;
  reveal(nodeIds: string[], init?: {partial?: boolean}): Promise<ViewResult>;
  filter(hide?: Hidden): Promise<ViewResult>;
  trace(from: string, to: string, init?: {mode?: 'shortest' | 'all'; edgeKinds?: string[]}): Promise<ReadAnswer<TraceAnswer> & {cancelled?: boolean}>;
  neighbourhood(
    nodeId: string | string[],
    init?: {hops?: number; direction?: 'out' | 'in' | 'both'; edgeKinds?: string[]},
  ): Promise<ReadAnswer<NeighbourhoodAnswer> & {cancelled?: boolean}>;
  /** Opens the groups the highlight passes through. */
  revealHighlight(): Promise<ViewResult>;
  clearHighlight(): void;
  back(): Promise<ViewResult>;
  forward(): Promise<ViewResult>;
  /** Any other change of the protocol, remembered for `back` under `label`. */
  apply(request: Omit<EngineRequest, 'snapshot' | 'expectedRevision' | 'expectedLayoutVersion'> & {op: string}, label?: string): Promise<ViewResult>;
  /** Drops the steps queued and not started; a step under way finishes. */
  cancel(): void;
}

export function createViewController(session: Session, options?: {page?: number; historyLimit?: number}): ViewController;

export function sceneDiff(before: Scene | null, after: Scene | null): SceneDiff;
