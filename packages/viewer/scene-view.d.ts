import type {Scene} from '@arqlet/contracts';
import type {SceneColumnsLike} from './scene-columns.js';
import type {SceneHit, SceneIndex, SceneViewTransform} from './scene-buffers.js';
import type {SceneRenderer} from './scene-gl.js';
import type {SceneBackend} from './scene-renderer.js';
import type {RenderSvgOptions} from './svg.js';

export interface SceneViewOptions {
  /** The near layer takes over at or below this many elements (nodes and lines) in view; default 400 (provisional). */
  budget?: number;
  /** ...and when the median leaf in view is at least this many CSS pixels across; default 24 (provisional). */
  minPixels?: number;
  /** It hands back above budget × hysteresis or below minPixels / hysteresis; default 1.25. */
  hysteresis?: number;
  /** The near layer holds the view plus this share of its size on every side; default 0.5. */
  margin?: number;
  /** Milliseconds of the fade at a switch; none under reduced motion; default 150. */
  fade?: number;
  /** Options of renderSvg for the near layer (handlers, selection, pins); `scale` and `document` are the view's. */
  svg?: Partial<RenderSvgOptions>;
  /** The far layer's backend (from `sceneBackend`); default WebGL 2 when available, else SVG alone. */
  backend?: SceneBackend;
  /** false without `backend`: SVG only. */
  webgl?: boolean;
  view?: SceneViewTransform;
  background?: [number, number, number, number];
  preserveDrawingBuffer?: boolean;
  onLayer?: (layer: 'far' | 'near') => void;
  onView?: (view: SceneViewTransform, layer: 'far' | 'near') => void;
}

export interface SceneView {
  readonly view: SceneViewTransform;
  readonly layer: 'far' | 'near';
  /** The far layer's backend, 'svg' when there is none. */
  readonly backend: 'webgpu' | 'webgl2' | 'svg';
  /** Elements in view (-1 when decided without a query), the median leaf on screen, elements in the near layer. */
  readonly counts: {elements: number; boxPixels: number; held: number};
  readonly scene: Scene;
  readonly index: SceneIndex;
  readonly far: SceneRenderer | null;
  readonly near: SVGSVGElement;
  setView(view: SceneViewTransform): void;
  fit(): SceneViewTransform;
  setScene(scene: Scene | SceneColumnsLike, options?: {keepView?: boolean}): void;
  setState(state: {selected?: string | null; selectedRoute?: string | null}): void;
  resize(): void;
  /** The element under a CSS pixel from the index. */
  pick(clientX: number, clientY: number): SceneHit | null;
  destroy(): void;
}

/**
 * Provisional (ADR 0025, V2): a scene in `host` with a WebGL far layer and an SVG near layer over the same geometry,
 * switched by the elements in view and their size on screen. Without WebGL 2, SVG alone.
 */
export function createSceneView(host: HTMLElement, scene: Scene | SceneColumnsLike, options?: SceneViewOptions): SceneView;

/** The scene view with the best backend this browser offers: WebGPU, else WebGL 2, else SVG alone. */
export function openSceneView(host: HTMLElement, scene: Scene | SceneColumnsLike, options?: SceneViewOptions & {prefer?: 'webgl2' | 'svg'}): Promise<SceneView>;
