import type {Scene} from '@arqlet/contracts';

/** Provisional (ADR 0019, step 4): a session's scene as columns. Index `i` of an id table is node or route `i`. */
export interface SceneColumns {
  revision: number;
  layoutVersion: number;
  /** Node ids by node index. */
  nodeIds: string[];
  /** Route ids by route index. */
  routeIds: string[];
  /** x, y, width, height per node, in scene coordinates. */
  nodeBounds: Float32Array;
  /** Parent node index, `NONE` for a root. */
  nodeParent: Uint32Array;
  /** Bits of `NODE_FLAGS`. */
  nodeFlags: Uint32Array;
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
}

/** "No node" in an index column. */
export const NONE: number;
export const NODE_FLAGS: {readonly container: 1; readonly aggregate: 2; readonly collapsed: 4; readonly invalid: 8};
export const ROUTE_FLAGS: {readonly invalid: 1; readonly rounded: 2};

/** The columns of a scene object, as the engine builds them (`f32` coordinates). Provisional. */
export function columnsOfScene(scene: Scene): SceneColumns;
