import type {BoundaryCrossing, PlacedLabel, Scene, SceneNode, SceneRoute} from '@arqlet/contracts';

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

/**
 * Maps a scene to React Flow nodes and edges (parents before children, positions relative to the parent). No React
 * dependency and no routing of its own: register node and edge types named `arqlet` that draw `data`.
 */
export function toReactFlow(scene: Scene): {
  nodes: ArqletFlowNode[];
  edges: ArqletFlowEdge[];
  labels: PlacedLabel[];
  crossings: BoundaryCrossing[];
};
