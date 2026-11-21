import type {BoundaryCrossing, PlacedLabel, Point, Scene, SceneBusLine, SceneHubRef, SceneNode, SceneRoute} from '@arqlet/contracts';

export interface ArqletFlowHandle {
  id: string;
  type: 'source' | 'target';
  side: string;
  x: number;
  y: number;
}

export interface ArqletFlowNode {
  id: string;
  type: 'arqlet';
  parentId?: string;
  position: {x: number; y: number};
  width: number;
  height: number;
  style: {width: number; height: number};
  draggable: false;
  connectable: false;
  deletable: false;
  zIndex: number;
  data: SceneNode & {handles: ArqletFlowHandle[]; showPorts: boolean};
}

export interface ArqletFlowEdge {
  id: string;
  source: string;
  target: string;
  sourceHandle: string;
  targetHandle: string;
  type: 'arqlet';
  zIndex: number;
  deletable: false;
  data: SceneRoute;
}

/** What a scene draws besides nodes and edges between nodes, in flow coordinates (the scene's own). */
export interface ArqletFlowOverlays {
  /** Bus bars and their taps (`busStyle: 'bar'`), each tap a polyline from its member to the bar. */
  busLines: SceneBusLine[];
  /** Junction dots where shared bus lines branch (`busStyle: 'trunk'` or `'lines'`); the lines are ordinary edges. */
  junctions: Point[];
  /** Stubs of hub references (`hubRefs`): a short line at the edge's other end and a label naming the hub. */
  hubRefs: SceneHubRef[];
}

/** With `overlayNode`: one node over the whole scene that a node type `arqletOverlay` draws. */
export interface ArqletFlowOverlayNode {
  id: 'arqlet:overlay';
  type: 'arqletOverlay';
  position: {x: number; y: number};
  width: number;
  height: number;
  style: {width: number; height: number; pointerEvents: 'none'};
  draggable: false;
  selectable: false;
  connectable: false;
  deletable: false;
  focusable: false;
  zIndex: number;
  /** The overlays in flow coordinates; subtract `origin` to draw them inside the node. */
  data: ArqletFlowOverlays & {origin: {x: number; y: number}};
}

export interface ToReactFlowOptions {
  /** Add an `arqletOverlay` node that carries the overlays (only when the scene has any). Default false. */
  overlayNode?: boolean;
}

/**
 * Maps a scene to React Flow nodes and edges (parents before children, positions relative to the parent). No React
 * dependency and no routing of its own: register node and edge types named `arqlet` that draw `data`. Bus bars, taps,
 * junction dots and hub-reference stubs are no node or edge: they come back as overlays in flow coordinates, for a host to
 * draw (in a viewport portal, or through the `arqletOverlay` node that `overlayNode` adds).
 */
export function toReactFlow(scene: Scene, options?: ToReactFlowOptions): {
  nodes: Array<ArqletFlowNode | ArqletFlowOverlayNode>;
  edges: ArqletFlowEdge[];
  labels: PlacedLabel[];
  crossings: BoundaryCrossing[];
} & ArqletFlowOverlays;
