import type {Scene} from '@arqlet/contracts';
import type {SceneColumnsLike} from './scene-columns.js';
import type {SceneHit, SceneIndex, SceneViewTransform} from './scene-buffers.js';

export interface SceneRendererOptions {
  /** A 2D canvas laid over the WebGL canvas for the texts. */
  labels?: HTMLCanvasElement;
  /** The first view; default: the whole scene. */
  view?: SceneViewTransform;
  /** Clear colour, RGBA in 0..1; default transparent. */
  background?: [number, number, number, number];
  /** Texts smaller than this on screen (CSS pixels) are left out; default 6. */
  minTextPixels?: number;
  /** false leaves the texts out. */
  text?: boolean;
  fontFamily?: string;
  selected?: string | null;
  selectedRoute?: string | null;
  pinned?: Iterable<string> | null;
  /** The appearance (style.js). */
  style?: import('./style.js').StyleSheet | import('./style.js').ResolvedStyle | null;
  trace?: import('./scene-buffers.js').SceneDrawState['trace'];
  /** Keeps the drawing readable after the frame (for pixel checks). */
  preserveDrawingBuffer?: boolean;
  onView?: (view: SceneViewTransform) => void;
}

export interface SceneRenderer {
  /** The backend drawing it. */
  readonly kind: 'webgpu' | 'webgl2';
  readonly view: SceneViewTransform;
  readonly scene: Scene;
  readonly index: SceneIndex;
  readonly state: import('./scene-buffers.js').SceneDrawState;
  /** The node outlined as hovered, or null. */
  readonly hovered: string | null;
  /** Outlines a node as hovered (null: none); rebuilds only the overlay layer. */
  setHover(id: string | null): void;
  /** Sets the view; drawn on the next animation frame. */
  setView(view: SceneViewTransform): void;
  /** Shows the whole scene. */
  fit(): SceneViewTransform;
  /** A new scene; `animate` (milliseconds) morphs from the drawing now: boxes move by id, lines fade over, texts wait. */
  setScene(scene: Scene | SceneColumnsLike, options?: {keepView?: boolean; animate?: number}): void;
  /** Whether a morph is running. */
  readonly animating: boolean;
  setState(state: import('./scene-buffers.js').SceneDrawState): void;
  /** Matches the canvases to their CSS size and the device pixel ratio, and draws. */
  resize(): void;
  /** Draws now. */
  draw(): void;
  world(clientX: number, clientY: number): [number, number];
  /** The element under a CSS pixel (lines within 5 CSS pixels). */
  pick(clientX: number, clientY: number): SceneHit | null;
  destroy(): void;
}

/** Whether WebGL 2 is available. */
export function webgl2Available(doc?: Document): boolean;
/** Provisional (ADR 0025, V1): draws a scene with WebGL 2, the fallback when WebGPU is missing. Throws if WebGL 2 is not available. */
export function createSceneRenderer(canvas: HTMLCanvasElement, scene: Scene | SceneColumnsLike, options?: SceneRendererOptions): SceneRenderer;
export {bindSceneNavigation} from './scene-renderer.js';
