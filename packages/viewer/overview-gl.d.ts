import type {Expanded, OverviewModel, OverviewView, pickExpanded} from './overview.js';

export interface OverviewRendererOptions {
  /** A 2D canvas laid over the WebGL canvas, for cluster names and the labels of structured clusters. */
  labels?: HTMLCanvasElement;
  /** Called after every view change. */
  onView?: (view: OverviewView) => void;
  /** Factor on the opacity of the overview's edges, default 1. */
  edgeAlpha?: number;
  /** Clear colour, RGBA in 0..1. */
  background?: [number, number, number, number];
}

export interface OverviewRenderer {
  view: OverviewView;
  readonly expanded: Expanded;
  readonly collapsed: Set<number>;
  setView(view: OverviewView): void;
  /** Draws these clusters structured, in their discs. */
  setExpanded(expanded: Expanded): void;
  /** Draws clusters structured (`expanded`) or collapsed into one disc (`collapsed`, indices); omitted parts are kept.
   * With `fade` (milliseconds) the old drawing blends into the new one. */
  setState(state: {expanded?: Expanded; collapsed?: Set<number>; selected?: number}, animation?: {fade?: number}): void;
  /** Moves the drawing to a new layout of the same graph (and the view to `view`) over `duration` milliseconds. Returns
   * false when the two drawings differ in their vertices and the new one was shown at once. */
  morph(next: OverviewModel, animation?: {duration?: number; view?: OverviewView | null}): boolean;
  /** The model drawn now. */
  readonly model: OverviewModel;
  /** Whether a morph or a blend is running. */
  readonly animating: boolean;
  /** Matches the canvas to its CSS size (and device pixel ratio) and draws. */
  resize(): void;
  draw(): void;
  /** The world point under a CSS pixel. */
  world(clientX: number, clientY: number): [number, number];
  /** The deepest node under a CSS pixel, or -1; inside a structured cluster, that cluster. */
  pick(clientX: number, clientY: number): number;
  /** The scene node under a CSS pixel inside a structured cluster. */
  pickScene(clientX: number, clientY: number): ReturnType<typeof pickExpanded>;
  destroy(): void;
}

/** Draws an overview with WebGL 1 (no dependency). Throws if WebGL is not available. */
export function createOverviewRenderer(
  canvas: HTMLCanvasElement,
  model: OverviewModel,
  options?: OverviewRendererOptions,
): OverviewRenderer;

/** Wheel zoom at the pointer, drag panning, and click and hover callbacks. Returns a function that removes them. */
export function bindOverviewNavigation(
  canvas: HTMLCanvasElement,
  renderer: OverviewRenderer,
  handlers?: {
    onClick?: (index: number, event: PointerEvent) => void;
    onHover?: (index: number, event: PointerEvent) => void;
    onDoubleClick?: (index: number, event: MouseEvent) => void;
  },
): () => void;
