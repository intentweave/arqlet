import type {Scene} from '@arqlet/contracts';
import type {SceneColumnsLike} from './scene-columns.js';
import type {SceneBuffers, SceneViewTransform} from './scene-buffers.js';
import type {SceneRenderer, SceneRendererOptions} from './scene-gl.js';

/** The far layer's backend a browser offers: WebGPU with its device, else WebGL 2, else none. */
export type SceneBackend = {kind: 'webgpu'; device: GPUDevice} | {kind: 'webgl2'} | {kind: 'svg'};

/** Provisional (ADR 0025): asks for WebGPU first, then WebGL 2; `prefer` skips the better ones. */
export function sceneBackend(options?: {prefer?: 'webgl2' | 'svg'; doc?: Document}): Promise<SceneBackend>;

/** A backend for `rendererShell`: uploads the buffers and draws them for a view. */
export interface SceneBackendDriver {
  upload(buffers: SceneBuffers): void;
  draw(view: SceneViewTransform, ratio: number): void;
  destroy(): void;
}

/** The renderer around a backend: view, state, texts, picking, frame scheduling. */
/** Provisional, exported for tests: draws the labels of a fixed screen size for the texts too small at their own size
 * (container titles, then leaf and glyph names where they do not overlap) on `ctx`, for `view` on a w×h CSS-pixel
 * canvas showing `rect`. */
export function drawLodLabels(
  ctx: CanvasRenderingContext2D,
  texts: import('./scene-buffers.js').SceneText[],
  view: {centreX: number; centreY: number; scale: number},
  w: number,
  h: number,
  rect: {x: number; y: number; width: number; height: number},
  options: {minTextPixels: number; font: string},
): void;
export function rendererShell(canvas: HTMLCanvasElement, scene: Scene | SceneColumnsLike, options: SceneRendererOptions, backend: SceneBackendDriver, kind: 'webgpu' | 'webgl2'): SceneRenderer;

/** Wheel zoom at the pointer and drag panning; a click without movement calls `onClick`. Returns a function that removes them. */
export function bindSceneNavigation(
  target: HTMLElement,
  viewer: {readonly view: SceneViewTransform; setView(view: SceneViewTransform): void},
  options?: {onClick?: (clientX: number, clientY: number, event: PointerEvent) => void; minScale?: number; maxScale?: number},
): () => void;
