import type {Rect, Scene} from '@arqlet/contracts';
import type {SceneColumnsLike} from './scene-columns.js';

/** Provisional (ADR 0025): floats per box instance (x, y, width, height, radius, kind, stroke width, dash, fill RGBA, stroke RGBA). */
export const BOX_STRIDE: 16;
/** Provisional: floats per segment instance (x0, y0, x1, y1, width, depth, dash on, dash off, start distance, RGBA). */
export const SEGMENT_STRIDE: 13;
/** Provisional: floats per triangle vertex (x, y, depth, RGBA). */
export const TRIANGLE_STRIDE: 7;
/** The layers in drawing order. */
export const LAYERS: readonly ['hulls', 'containers', 'routes', 'leaves', 'marks', 'overlay'];

export interface SceneText {
  x: number; y: number; text: string; size: number; colour: string;
  align?: 'start' | 'center'; baseline?: 'alphabetic' | 'middle'; weight?: number; angle?: number;
}
export interface SceneLayerBuffers { boxes: Float32Array; segments: Float32Array; triangles: Float32Array }
export interface SceneHit { kind: 'node' | 'route'; id: string }
export interface SceneIndex {
  /** The node item (leaf, container or bus bar) of an id. */
  node(id: string): {id: string; container: boolean; bounds: Rect} | undefined;
  /** The ids of the nodes and lines that touch `rect` (world). */
  query(rect: Rect): {nodes: Set<string>; routes: Set<string>};
  /** The element under a world point in renderSvg's hit order (leaf or bus bar, line within `tolerance`, deepest container). */
  pick(x: number, y: number, tolerance?: number): SceneHit | null;
}
/** What the far layer draws besides the scene, as renderSvg's options of the same names. */
export interface SceneDrawState {
  selected?: string | null;
  selectedRoute?: string | null;
  /** Pinned node ids: a dot on each pinned leaf. */
  pinned?: Iterable<string> | null;
  /** The path in amber, the rest receding (renderSvg's `trace`). */
  trace?: {nodes?: string[]; routes?: string[]; distance?: Record<string, number> | Map<string, number>; fade?: boolean} | null;
}
export interface SceneBuffers {
  scene: Scene;
  state: SceneDrawState;
  bounds: Rect;
  layers: SceneLayerBuffers[];
  texts: SceneText[];
  index: SceneIndex;
}
export interface SceneViewTransform { centreX: number; centreY: number; scale: number }

/** The scene object of a scene or of its columns. */
export function sceneOf(scene: Scene | SceneColumnsLike): Scene;
/** Provisional (ADR 0025, V1): what the WebGL renderer draws, in renderSvg's order, colours and outlines. */
export function sceneBuffers(scene: Scene | SceneColumnsLike, state?: SceneDrawState): SceneBuffers;
/** The overlay layer with renderSvg's hover outline on the node `id` (leaf, container or bus bar); empty for null. */
export function hoverBuffers(scene: Scene, id: string | null): SceneLayerBuffers;
/** A uniform grid over the scene's elements, for picking and the elements in view. */
export function sceneIndex(scene: Scene): SceneIndex;
/** Ear clipping of a simple polygon: triangles as index triples. */
export function triangulate(points: {x: number; y: number}[]): [number, number, number][];
/** The corners of renderSvg's arrowhead for a line ending at `end` in direction (dx, dy). */
export function arrowCorners(end: {x: number; y: number}, dx: number, dy: number): [number, number][];
/** The world rectangle a view shows on a canvas of `width` by `height` CSS pixels. */
export function viewRect(view: SceneViewTransform, width: number, height: number): Rect;
/** Provisional (ADR 0025, V2): which layer draws, with hysteresis. Defaults: budget 400, minPixels 24, hysteresis 1.25. */
export function nearPolicy(input: {elements: number; boxPixels: number; current?: 'far' | 'near'; budget?: number; minPixels?: number; hysteresis?: number}): 'far' | 'near';
/** The median on-screen size (smaller side, CSS pixels) of the leaves among `ids`; 0 for none. */
export function medianBoxPixels(index: SceneIndex, ids: Iterable<string>, scale: number): number;
/** The part of a scene that touches `rect`, as a scene with `rect` as its bounds (coordinates unchanged). */
export function sceneExcerpt(scene: Scene, index: SceneIndex, rect: Rect): Scene;
