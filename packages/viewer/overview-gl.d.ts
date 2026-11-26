import type {Expanded, OverviewLens, OverviewModel, OverviewView, pickExpanded} from './overview.js';

export interface OverviewRendererOptions {
  /** Provisional (ADR 0025 O2): a WebGPU device (from `sceneBackend`): the overview is drawn with WebGPU; without one WebGL 2, else WebGL 1. */
  device?: GPUDevice;
  /** `'webgl'` keeps WebGL even with a device; `'webgl1'` forces WebGL 1. */
  backend?: 'webgl' | 'webgl1';
  /** A 2D canvas laid over the WebGL canvas, for cluster names and the labels of structured clusters. */
  labels?: HTMLCanvasElement;
  /** Called after every view change. */
  onView?: (view: OverviewView) => void;
  /** Factor on the opacity of the overview's edges, default 1. */
  edgeAlpha?: number;
  /** Clear colour, RGBA in 0..1 (`setBackground`). A light one (luminance above 0.5) makes the tone light: dark texts,
   * and the galaxy style drawn in ink (colours subtracted) instead of light. */
  background?: [number, number, number, number];
  /** The outline per node kind; kinds not named are circles. */
  shapes?: Record<string, import('./overview.js').Outline>;
  /** Provisional: start in the galaxy style (`setGalaxy`). */
  galaxy?: boolean;
  /** Provisional: the names of nodes (`setNames`) and their details. */
  names?: boolean;
  details?: boolean;
  /** Provisional (ADR 0025 O3): structured clusters as boxes in the cluster's colour ('cluster', default) or as the scene view draws them ('scene'). */
  structured?: 'cluster' | 'scene';
  /** Provisional: the appearance (style.js, ADR 0026): trace, route, line and bus colours from its theme, cluster colours from its palette. */
  style?: import('./style.js').StyleSheet | import('./style.js').ResolvedStyle | null;
  /** Dim the edges by how many overlap on a pixel (default true). */
  edgeDimming?: boolean;
}

export interface OverviewRenderer {
  view: OverviewView;
  readonly expanded: Expanded;
  readonly collapsed: Set<number>;
  setView(view: OverviewView): void;
  /** Draws these clusters structured, in their discs. */
  setExpanded(expanded: Expanded): void;
  /** The outline per node kind (inscribed in each disc); kinds not named are circles. */
  setShapes(shapes: Record<string, import('./overview.js').Outline>): void;
  /** Provisional: whether the galaxy style is on (glows and stars instead of discs and rings, faint lines, added light). */
  readonly galaxy: boolean;
  setGalaxy(on: boolean): void;
  /** Provisional: 'light' on a light background (dark texts, the galaxy style in ink), else 'dark'. */
  readonly tone: 'dark' | 'light';
  /** Provisional: the clear colour (RGBA in 0..1; default the dark one), for a host that switches themes. */
  setBackground(rgba?: [number, number, number, number]): void;
  /** The backend drawing it. */
  readonly backend: 'webgpu' | 'webgl2' | 'webgl';
  /** Provisional: how structured clusters are drawn: 'cluster' or 'scene'. */
  setStructured(mode: 'cluster' | 'scene'): void;
  /** Provisional: sets the appearance (null: the overview's own colours). */
  setStyle(style: import('./style.js').StyleSheet | import('./style.js').ResolvedStyle | null): void;
  /** Provisional: the orbit (pseudo-3D): turned by `yaw` and `pitch` (radians) around the view's centre, points at depths
   * of up to `depth` world units; null is flat. Under the orbit a drag turns the drawing and Shift-drag pans. */
  readonly orbit: {yaw: number; pitch: number; depth: number} | null;
  setOrbit(orbit: {yaw?: number; pitch?: number; depth?: number} | null): void;
  /** Provisional: name every node whose disc is 6 pixels across or more (`nodes`), with its kind and links (`details`). */
  readonly names: {nodes: boolean; details: boolean};
  setNames(names: {nodes?: boolean; details?: boolean}): void;
  /** Draws clusters structured (`expanded`) or collapsed into one disc (`collapsed`, indices); omitted parts are kept.
   * With `fade` (milliseconds) the old drawing blends into the new one. */
  setState(state: {expanded?: Expanded; collapsed?: Set<number>; selected?: number; trace?: import('./overview.js').OverviewPath | null}, animation?: {fade?: number}): void;
  /** Moves the drawing to a new layout of the same graph (and the view to `view`) over `duration` milliseconds. Returns
   * false when the two drawings differ in their vertices and the new one was shown at once. */
  morph(next: OverviewModel, animation?: {duration?: number; view?: OverviewView | null}): boolean;
  /** The model drawn now. */
  readonly model: OverviewModel;
  /** Whether a morph or a blend is running. */
  readonly animating: boolean;
  /** Matches the canvas to its CSS size (and device pixel ratio) and draws. */
  resize(): void;
  /** Draws now; `labels: false` leaves the text canvas as it is. */
  draw(options?: {labels?: boolean}): void;
  /** The world point under a CSS pixel. */
  world(clientX: number, clientY: number): [number, number];
  /** The deepest node under a CSS pixel, or -1; inside a structured cluster, that cluster. */
  pick(clientX: number, clientY: number): number;
  /** The scene node under a CSS pixel inside a structured cluster. */
  pickScene(clientX: number, clientY: number): ReturnType<typeof pickExpanded>;
  /** The fisheye lens in CSS pixels of the canvas, or null. */
  readonly lens: OverviewLens | null;
  /** Shows the lens (drawn on the next frame), or none with null. Picking and `world` see through it. */
  setLens(lens: OverviewLens | null): void;
  /** Whether the discs breathe (fading out included). */
  readonly breathing: boolean;
  /**
   * Lets the discs grow and shrink by `amplitude` (fraction of the radius, default 0.06) over `period` ms (4000), in a
   * wave of `wavelength` CSS pixels (600) outwards from the canvas centre, fading in over `fadeIn` ms (1500). `null`
   * fades it out over `fadeOut` ms (400; 0 at once). Only the drawing changes.
   */
  breathe(settings: BreathSettings | null, stop?: {fadeOut?: number}): void;
  destroy(): void;
}

/** Draws an overview with WebGPU (with `options.device`), WebGL 2 or WebGL 1 (no dependency). Throws if none is available. */
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
    /** Provisional: a drag that starts on a leaf moves it (world point; `move` while dragging, `end` at the release);
     * without it every drag pans. */
    onNodeDrag?: (index: number, x: number, y: number, phase: 'move' | 'end') => void;
    /** Provisional: which node a drag that starts over node `index` (-1: none) moves, or -1 to pan; default: the leaf
     * itself, else pan. */
    draggable?: (index: number) => number;
    /** A lens that follows the pointer: its radius (CSS pixels) and magnification, or null for none; asked on every move. */
    lens?: () => {radius: number; magnification: number} | null;
    /** Breathing after `after` ms (default 8000) without input; stops at the next input. Not under reduced motion. */
    idle?: {after?: number; breathe?: BreathSettings};
  },
): () => void;

/** The idle breathing of `OverviewRenderer.breathe`. */
export interface BreathSettings {
  amplitude?: number;
  period?: number;
  wavelength?: number;
  fadeIn?: number;
}
