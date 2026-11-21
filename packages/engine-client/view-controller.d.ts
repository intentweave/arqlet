import type {DetailsAnswer, FindAnswer, Graph, Hidden, Intent, NeighbourhoodAnswer, Scene, Snapshot, TraceAnswer} from '@arqlet/contracts';
import type {DetailTarget, PeekAnswer, ReadAnswer, RefineRun, Session, SessionResult} from './session.js';
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

/** Provisional: where keyboard navigation goes (see `navigateScene`). */
export type NavigationDirection = 'left' | 'right' | 'up' | 'down' | 'parent' | 'child' | 'next' | 'previous' | 'out' | 'in';

/** The node of `scene` that `direction` leads to from `from` (without `from`, the first top-level node), or null. */
export function navigateScene(scene: Scene | null, from: string | null, direction: NavigationDirection): string | null;

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
  /** The look into a collapsed container (`peek`), or null; cleared by the next change. */
  readonly peek: {nodeId: string; answer: PeekAnswer & {ok: true}} | null;
  /** The refinement under way on the second engine (`refine`), or null; `shown` counts intermediate results applied. */
  readonly refining: {op: string; jobId: string; shown: number} | null;
  /** How the last refinement ended: applied, stale (the view had moved on), cancelled, or an error. */
  readonly refined: {op: string; applied: boolean; stale?: true; cancelled?: true; error?: string; optimization?: unknown} | null;
  /** Refinement results discarded because the view had changed. */
  readonly discarded: number;
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
  /** A look into a collapsed container without opening it (`state.peek`; not a step for `back`). */
  peek(nodeId: string, init?: {limit?: number}): Promise<ViewResult>;
  closePeek(): void;
  /** Opens what hides `nodeId` (a step for `back`), selects it, and sets `state.focus` for the host's camera. */
  focus(nodeId: string, options?: {reveal?: boolean; partial?: boolean}): Promise<ViewResult>;
  /**
   * Refines the view on the second engine (`refine`): `{op: 'refine'}` by default, `refineProgressive` or
   * `structuredRefine`; applied only if the view is still the one it started from. Not a step for `back`.
   */
  refine(
    request?: {op?: 'refine' | 'refineProgressive' | 'structuredRefine'; [field: string]: unknown},
    options?: {onApplied?: (snapshot: Snapshot, info: {final: boolean; shown: number}) => void},
  ): Promise<ViewResult>;
  /** Stops the refinement under way; its later results are dropped. */
  cancelRefinement(): void;
  /** Moves the selection the way `direction` says and sets `state.focus` (nothing opened, not a step for `back`). */
  navigate(direction: NavigationDirection): Promise<ViewResult | {ok: false; none: true; collapsed?: boolean}>;
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

export function createViewController(session: Session, options?: {page?: number; historyLimit?: number; scenes?: boolean; detailLimit?: number; refine?: RefineRun; publicationInterval?: number}): ViewController;

export function sceneDiff(before: Scene | null, after: Scene | null): SceneDiff;
