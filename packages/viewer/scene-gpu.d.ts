import type {Scene} from '@arqlet/contracts';
import type {SceneColumnsLike} from './scene-columns.js';
import type {SceneRenderer, SceneRendererOptions} from './scene-gl.js';

/** Whether `navigator.gpu` exists (the device is asked for by `sceneBackend`). */
export function webgpuAvailable(): boolean;
/**
 * Provisional (ADR 0025, V1): draws a scene with WebGPU on `options.device`; the same view, options and methods as the
 * WebGL 2 renderer (`preserveDrawingBuffer` does not apply).
 */
export function createSceneGpuRenderer(canvas: HTMLCanvasElement, scene: Scene | SceneColumnsLike, options: SceneRendererOptions & {device: GPUDevice}): SceneRenderer;
