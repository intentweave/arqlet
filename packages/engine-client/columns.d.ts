import type {Scene} from '@arqlet/contracts';

/**
 * Provisional (ADR 0019, step 4): a session's scene as columns. Index `i` of an id or text table is node, route, port, label,
 * row, conflict or edge `i`, and the typed arrays of the same part are indexed alike. Coordinates are `f32`.
 */
export interface SceneColumns {
  schemaVersion: number;
  revision: number;
  layoutVersion: number;

  /** Node ids, labels and kinds (`nodeKind` indexes `nodeKinds`) by node index. */
  nodeIds: string[];
  nodeLabels: string[];
  nodeKinds: string[];
  /** x, y, width, height per node, in scene coordinates. */
  nodeBounds: Float32Array;
  /** Parent node index, `NONE` for a root. */
  nodeParent: Uint32Array;
  /** Bits of `NODE_FLAGS`. */
  nodeFlags: Uint32Array;
  nodeKind: Uint32Array;
  /** Values of `SHAPES`. */
  nodeShape: Uint32Array;

  /** Route ids by route index. */
  routeIds: string[];
  /** Node index of each route's ends, `NONE` when the end is not a node of the scene. */
  routeSource: Uint32Array;
  routeTarget: Uint32Array;
  routeWeight: Uint32Array;
  /** Bits of `ROUTE_FLAGS`. */
  routeFlags: Uint32Array;
  /** One more than the routes: route `i` is the points `routeOffsets[i]` to `routeOffsets[i + 1]` of `routePoints`. */
  routeOffsets: Uint32Array;
  /** x and y alternating, all routes one after the other. */
  routePoints: Float32Array;
  /** Each route's kind (index into `routeKinds`, '' for none), the edges it stands for in the other direction (`NONE`
   * when it is not a two-way route), and the edges of a bundle (indices into `edgeIds`, behind `routeMemberOffsets`). */
  routeKinds: string[];
  routeKind: Uint32Array;
  routeBackWeight: Uint32Array;
  routeMemberOffsets: Uint32Array;
  routeMembers: Uint32Array;

  /** Port ids by port index. */
  portIds: string[];
  /** The edges that ports and boundary crossings name, each once. */
  edgeIds: string[];
  /** Node index of each port's node. */
  portNode: Uint32Array;
  /** Index into `edgeIds`. */
  portEdge: Uint32Array;
  /** Bits of `PORT_FLAGS`. */
  portFlags: Uint32Array;
  /** Values of `SIDES`. */
  portSide: Uint32Array;
  /** x and y alternating. */
  portPosition: Float32Array;

  /** Boundary crossings: edge (index into `edgeIds`), container (node index), ordinal, side, flags (bit 0: derived), position. */
  crossingEdge: Uint32Array;
  crossingContainer: Uint32Array;
  crossingOrdinal: Uint32Array;
  crossingSide: Uint32Array;
  crossingFlags: Uint32Array;
  crossingPosition: Float32Array;

  /** Placed labels: id and text by label index, `labelKind` indexes `labelKinds`. */
  labelIds: string[];
  labelTexts: string[];
  labelKinds: string[];
  labelKind: Uint32Array;
  /** x, y, width, height per label. */
  labelBounds: Float32Array;

  /** Rows of the nodes' content, node by node: id and label by row index, the node (index), bounds (four per row). */
  rowIds: string[];
  rowLabels: string[];
  rowNode: Uint32Array;
  rowBounds: Float32Array;

  /** Conflicts: kind (index into `conflictKinds`) and reason by conflict index; the nodes and routes each names are indices
   * (`NONE` for one outside the scene) in `conflictNodes` and `conflictRoutes`, behind the offsets (conflicts + 1). */
  conflictKinds: string[];
  conflictReasons: string[];
  conflictKind: Uint32Array;
  conflictNodeOffsets: Uint32Array;
  conflictNodes: Uint32Array;
  conflictRouteOffsets: Uint32Array;
  conflictRoutes: Uint32Array;

  /** Where bus lines branch: x and y alternating. */
  junctions: Float32Array;
  /** x, y, width, height of everything drawn, with its margin (full precision). */
  bounds: [number, number, number, number];
  showPorts: boolean;
  /** The explicit buses, each as its edges' ids. */
  buses: string[][];
}

/** "No node" in an index column. */
export const NONE: number;
export const NODE_FLAGS: {readonly container: 1; readonly aggregate: 2; readonly collapsed: 4; readonly invalid: 8};
export const ROUTE_FLAGS: {readonly invalid: 1; readonly rounded: 2};
export const PORT_FLAGS: {readonly explicit: 1; readonly target: 2};
export const SIDES: {readonly left: 0; readonly right: 1; readonly top: 2; readonly bottom: 3};
export const SHAPES: {readonly rect: 0; readonly square: 1; readonly circle: 2};

/** The columns of a scene object, as the engine builds them (`f32` coordinates). Provisional. */
export function columnsOfScene(scene: Scene): SceneColumns;
