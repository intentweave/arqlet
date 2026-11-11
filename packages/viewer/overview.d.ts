/** The result of the engine's `overview` operation (ADR 0018). Provisional. Arrays are indexed by node, in input order. */
export interface OverviewResult {
  schemaVersion: 1;
  ids: string[];
  /** The parent's index, or -1 for a node at the top level. */
  parent: number[];
  x: number[];
  y: number[];
  /** Disc radii: leaves share the leaf radius, a container holds its children. */
  r: number[];
  /** The radius of the disc around everything, centred on the origin. */
  rootRadius: number;
  /** Resolved edges as index pairs, source first, in input order. */
  edges: number[];
  /** Per container, the links between its children: child, child, number of edges (flat triples). */
  links: number[];
  stats: {nodes: number; containers: number; communities: number; levels: number; edges: number; unresolvedEdges: number; overlaps: number};
  /** The values of the order user, rules, zoom, as the engine checked them (ADR 0018, addendum). */
  representation?: {
    decisions: Record<string, Representation>;
    rules: Record<string, Representation>;
    auto: boolean;
    unknownKinds: string[];
  };
  /** Node kinds in order of first use, and the kind of every node as an index into them. */
  kinds?: string[];
  kind?: number[];
}

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
export function displayName(id: string): string;
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
/** Vertex data; clusters of `expanded` drawn structured, of `collapsed` as one disc with their edges bundled per pair. */
export function overviewBuffers(model: OverviewModel, expanded?: Expanded, collapsed?: Set<number>, selected?: number): OverviewBuffers;

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
): {graph: InputGraph; leaving: number; inner: number; standIns: number};

export type FlowDirection = 'lr' | 'rl' | 'tb' | 'bt';
/** The flow for a structured cluster that faces its neighbours, or null when there is no clear side. */
export function flowDirectionOf(model: OverviewModel, i: number): FlowDirection | null;
