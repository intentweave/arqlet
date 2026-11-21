import type {DetailsAnswer, FindAnswer, Graph, Hidden, Intent, NeighbourhoodAnswer, Scene, Snapshot, TraceAnswer} from '@arqlet/contracts';
import type {DetailTarget, ReadAnswer, Session, SessionResult} from './session.js';
import type {EngineRequest} from './wasm-client.js';

/** Provisional: a view as a value (`bookmark`): the engine's intent, the highlight, the selection, the graph's digest. */
export interface ViewBookmark {
  version: 1;
  digest: string | null;
  intent: Intent;
  highlight?: ViewHighlight extends infer H ? (H extends unknown ? Omit<H, 'answer'> : never) : never;
  selection?: DetailTarget;
}

/** A bookmark as URL-safe text, and back (throws on text that is not one). */
export function encodeBookmark(bookmark: ViewBookmark): string;
export function decodeBookmark(text: string): ViewBookmark;

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
  /** The selected node or edge (`select`), or null. */
  readonly selection: DetailTarget | null;
  /** What the engine knows about the selection, asked again after every change; null without a selection. */
  readonly details: (DetailsAnswer & {ok: true}) | null;
  /** The last search (`find`) and its answer, or null. */
  readonly search: {query: string; answer: FindAnswer & {ok: true}} | null;
  /** The last `focus`: the node and the bounds of the box that shows it (null when hidden); `serial` counts requests. */
  readonly focus: {nodeId: string; bounds: {x: number; y: number; width: number; height: number} | null; shownAs: string | null; serial: number} | null;
  /** What the pointer is over (`hover`), or null. */
  readonly hover: DetailTarget | null;
  /** What the view hides (the last `filter`). */
  readonly hide: Hidden;
  readonly canBack: boolean;
  readonly canForward: boolean;
  /** The names of the steps `back` and `forward` would undo and redo (such as `Open group-1`), or null. */
  readonly backLabel: string | null;
  readonly forwardLabel: string | null;
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
  /** Selects a node or an edge (or nothing): `state.details` holds the engine's facts about it, kept up to date. */
  select(target: DetailTarget | null): Promise<ViewResult>;
  /** What the pointer is over (no engine call). */
  hover(target: DetailTarget | null): void;
  /** Finds nodes by label or id, also hidden ones (`state.search` until `clearSearch`). */
  find(query: string, init?: {limit?: number; nodeKinds?: string[]}): Promise<ViewResult>;
  clearSearch(): void;
  /** Opens what hides `nodeId` (a step for `back`), selects it, and sets `state.focus` for the host's camera. */
  focus(nodeId: string, options?: {reveal?: boolean; partial?: boolean}): Promise<ViewResult>;
  /** The view as a value to keep or share (`encodeBookmark` for a URL). */
  bookmark(): Promise<{ok: true; bookmark: ViewBookmark} | {ok: false; error?: string; cancelled?: boolean}>;
  /** Opens a bookmark (a step for `back`); another graph's is refused unless `force`. */
  openBookmark(bookmark: ViewBookmark, options?: {force?: boolean}): Promise<ViewResult>;
  back(): Promise<ViewResult>;
  forward(): Promise<ViewResult>;
  /** Any other change of the protocol, remembered for `back` under `label`. */
  apply(
    request: Omit<EngineRequest, 'snapshot' | 'expectedRevision' | 'expectedLayoutVersion'> & {op: string},
    label?: string,
    /** `merge`: a follow-up of the last change, remembered in its entry. */
    options?: {remember?: boolean; merge?: boolean},
  ): Promise<ViewResult>;
  /**
   * Continues from a snapshot the host made; with `forget` the history and the highlight start empty, with `remember` (a
   * label) the change from the previous snapshot is remembered for `back`.
   */
  adopt(snapshot: Snapshot, options?: {forget?: boolean; remember?: string | null}): Promise<ViewResult>;
  /** Drops the steps queued and not started; a step under way finishes. */
  cancel(): void;
}

export function createViewController(session: Session, options?: {page?: number; historyLimit?: number; scenes?: boolean; detailLimit?: number}): ViewController;

export function sceneDiff(before: Scene | null, after: Scene | null): SceneDiff;
