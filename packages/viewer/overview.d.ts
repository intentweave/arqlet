/**
 * The result of the engine's `overview` operation (`Overview` in `@arqlet/contracts`, ADR 0018). Provisional. Arrays are
 * indexed by node, in input order. A viewer also takes one without labels, kinds and representation (each node is then
 * named by its id; rules and the level of detail do not apply).
 */
export type OverviewResult = Omit<import('@arqlet/contracts').Overview, 'labels' | 'kinds' | 'kind' | 'representation'>
  & Partial<Pick<import('@arqlet/contracts').Overview, 'labels' | 'kinds' | 'kind' | 'representation'>>;

export interface OverviewModel {
  overview: OverviewResult;
  n: number;
  children: number[][];
  roots: number[];
  depth: Int32Array;
  /** The clusters that give the colours: the top level, or under a single root its children. */
  tops: number[];
  /** The colour cluster of every node (its ancestor among `tops`). */
  cluster: Int32Array;
  /** The number of leaves below every node (1 for a leaf). */
  leaves: Int32Array;
  /** The number of nodes below every node. */
  size: Int32Array;
  index: Map<string, number>;
  /** The result's edge ids (see `resolvedEdgeIds`), or null. */
  edgeIds: string[] | null;
  /** The outermost node of every node's chain: a node that is its parent's only child and has the same disc (overview
   * option `chains`) is drawn, named and picked with its parent. The node itself when it is in no chain. */
  chainTop: Int32Array;
}

export interface OverviewView {
  centreX: number;
  centreY: number;
  /** CSS pixels per layout unit. */
  scale: number;
}

/** `edgeIds` (from `resolvedEdgeIds`) lets edges that leave a structured cluster start at their border port. */
export function overviewModel(overview: OverviewResult, edgeIds?: string[] | null): OverviewModel;
/** The ids of a graph's resolved edges, in input order: the edges of its overview. */
export function resolvedEdgeIds(graph: InputGraph): string[];
/** The id prefix of the stand-ins for clusters outside a structured excerpt. */
export const BORDER_PREFIX: 'border:';
/** The id prefix of the clusters the overview adds for communities. */
export const COMMUNITY_PREFIX: 'community:';
/** A short name: the last part of a path-like id; `≈ <level>.<k>` for a community. */
/** The name of node `i`: its label from the data (`overview.labels`), else `displayName` of its id; the top of a chain
 * names the chain (`A › B`, a repeated name once). */
export function nameOf(model: OverviewModel, i: number): string;
export function displayName(id: string): string;
/** The nodes of node `i`'s chain from its top down (just `i` when it is in none). */
export function chainOf(model: OverviewModel, i: number): number[];
/** RGB in 0..1 for the n-th top-level cluster. */
export function clusterColour(rootOrdinal: number): [number, number, number];
/** Floats per shape vertex and per line or triangle vertex of `overviewBuffers`. */
export const SHAPE_STRIDE: 11;
export const LINE_STRIDE: 6;

/** A structured cluster: the scene of its layered layout (the engine's `scene` operation on its excerpt). */
export interface StructuredCluster {
  scene: import('@arqlet/contracts').Scene;
}
/** Structured clusters by node index. */
export type Expanded = Map<number, StructuredCluster>;

export interface OverviewBuffers {
  discs: Float32Array;
  edges: Float32Array;
  back: Float32Array;
  routes: Float32Array;
  arrows: Float32Array;
  front: Float32Array;
  /** Vertices per part. */
  count: {discs: number; edges: number; back: number; routes: number; arrows: number; front: number};
}

/** The part of a structured cluster's scene its disc shows: the cluster's own box, or the whole scene. */
export function frameOf(model: OverviewModel, i: number, scene: import('@arqlet/contracts').Scene): import('@arqlet/contracts').Rect;
/** A route's part inside a frame and its border port; null when it does not cross the border. */
export function clipToFrame(
  points: [number, number][],
  frame: import('@arqlet/contracts').Rect,
  inward: boolean,
): {port: [number, number]; points: [number, number][]} | null;
/** Where a structured cluster's scene sits in its disc. */
export function placeScene(
  model: OverviewModel,
  i: number,
  scene: import('@arqlet/contracts').Scene,
  margin?: number,
): {scale: number; apply(p: [number, number]): [number, number]; invert(p: [number, number]): [number, number]};
/** For every node, the outermost structured or collapsed cluster it lies in, or -1. */
export function ownersOf(model: OverviewModel, expanded?: Expanded, collapsed?: Set<number>): Int32Array;
/** The outlines a disc can be drawn with (inscribed in its circle). */
export type Outline = 'circle' | 'square' | 'octagon' | 'diamond';
export const OUTLINES: Outline[];
/** Vertex data; clusters of `expanded` drawn structured, of `collapsed` as one disc with their edges bundled per pair;
 * `shapes` gives an outline per node kind (default: circle). */
export function overviewBuffers(
  model: OverviewModel,
  expanded?: Expanded,
  collapsed?: Set<number>,
  selected?: number,
  options?: {shapes?: Record<string, Outline>},
): OverviewBuffers;

/** How a container is shown; the engine's type (ADR 0018, addendum). */
export type {Representation} from '@arqlet/contracts';
import type {Representation} from '@arqlet/contracts';
export const REPRESENTATIONS: Representation[];
export interface RepresentationOptions {
  /** The user's decisions by node id; they come first. */
  decisions?: Map<string, Representation>;
  /** Rules of the data by node kind; they come second. */
  rules?: Record<string, Representation>;
  /** The automatic policy of the zoom; default true. */
  auto?: boolean;
  /** The most nodes below a structured cluster; default 300. */
  limit?: number;
  /** Clusters shown structured now (indices), for the hysteresis. */
  current?: Set<number>;
  /** The world point the user zooms at; only the cluster holding it is structured by the zoom. Default: the view's centre. */
  focus?: [number, number] | null;
  collapsePixels?: number;
  structureShare?: number;
  rulePixels?: number;
  /** The level of detail: clusters at `depth` (0: the top level) or deeper and of the `kinds` listed are collapsed, after
   * the user's decisions and the rules, before the zoom. The same value as an import's `levelOfDetail`. Default null. */
  levelOfDetail?: import('@arqlet/contracts').LevelOfDetail | null;
}
/** The representation of every container for a view (ADR 0018, addendum): user, then rules, then zoom. */
export function representationsOf(
  model: OverviewModel,
  view: OverviewView,
  width: number,
  height: number,
  options?: RepresentationOptions,
): {collapsed: Set<number>; structured: Set<number>};
/** The scene node under a world point inside a structured cluster; `node` null on no box; null outside all of them. */
export function pickExpanded(
  model: OverviewModel,
  expanded: Expanded,
  x: number,
  y: number,
): {cluster: number; node: import('@arqlet/contracts').SceneNode | null} | null;
export interface OverviewLabel {
  text: string;
  x: number;
  y: number;
  size: number;
  weight: number;
  kind: string;
}
/** Texts to write over a view: cluster names and the labels of structured clusters, in CSS pixels. */
export function labelsOf(
  model: OverviewModel,
  expanded: Expanded,
  view: OverviewView,
  width: number,
  height: number,
  options?: {minPixels?: number; limit?: number; collapsed?: Set<number>; lens?: OverviewLens | null},
): OverviewLabel[];
/** A fisheye lens in CSS pixels of the canvas: centre, radius and the enlargement in its middle (> 1). */
export interface OverviewLens {
  x: number;
  y: number;
  radius: number;
  magnification: number;
}
/** Where the lens shows a screen point, and its enlargement there: `[x, y, factor]` (the point and 1 outside it). */
export function lensAt(lens: OverviewLens | null, x: number, y: number): [number, number, number];
/** The screen point the lens shows at (`x`, `y`): the inverse of `lensAt`. */
export function lensInverse(lens: OverviewLens | null, x: number, y: number): [number, number];
/** The deepest node whose disc holds the world point, or -1; a collapsed cluster is picked as a whole. */
export function pickNode(model: OverviewModel, x: number, y: number, collapsed?: Set<number>): number;
/** The view that shows node `i` (-1: everything) in a viewport of `width` × `height` CSS pixels. */
export function viewOf(model: OverviewModel, i: number, width: number, height: number, margin?: number): OverviewView;
export function subtreeOf(model: OverviewModel, i: number): string[];

/** A graph in Arqlet's input form, as far as a drill-down reads it. */
export interface InputGraph {
  nodes: {id: string; parentId?: string | null; [field: string]: unknown}[];
  edges: {id: string; source: string; target?: string | null; [field: string]: unknown}[];
  [field: string]: unknown;
}

/**
 * The excerpt a structured cluster lays out: the cluster as a top-level container with everything below it, its inner
 * edges, and its leaving edges (id and direction kept) to stand-ins for the clusters outside, so that they cross its box
 * at border ports.
 */
export function structuredExcerpt(
  graph: InputGraph,
  model: OverviewModel,
  id: string,
  options?: {direction?: FlowDirection | null},
): {
  graph: InputGraph;
  /** The layout options to import the excerpt with (`isolatedGrid`: unconnected children in a grid, not one row). */
  options: {isolatedGrid: boolean};
  leaving: number;
  inner: number;
  standIns: number;
};

export type FlowDirection = 'lr' | 'rl' | 'tb' | 'bt';
/** The flow for a structured cluster that faces its neighbours, or null when there is no clear side. */
export function flowDirectionOf(model: OverviewModel, i: number): FlowDirection | null;
