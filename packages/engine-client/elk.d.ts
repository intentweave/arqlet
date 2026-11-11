import type {EngineRequest, EngineResponse} from './wasm-client.js';

/** A node, port, label or graph in ELK's JSON form. Only the fields Arqlet reads or writes are named. */
export interface ElkShape {
  id: string;
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  labels?: ElkShape[];
  layoutOptions?: Record<string, string>;
  [field: string]: unknown;
}

export interface ElkPoint {
  x: number;
  y: number;
}

export interface ElkEdgeSection {
  id: string;
  startPoint: ElkPoint;
  endPoint: ElkPoint;
  bendPoints?: ElkPoint[];
  incomingShape?: string;
  outgoingShape?: string;
}

export interface ElkEdge {
  id: string;
  sources: string[];
  targets: string[];
  sections?: ElkEdgeSection[];
  junctionPoints?: ElkPoint[];
  labels?: ElkShape[];
  layoutOptions?: Record<string, string>;
  [field: string]: unknown;
}

export interface ElkNode extends ElkShape {
  children?: ElkNode[];
  ports?: ElkShape[];
  edges?: ElkEdge[];
}

export interface ElkLayoutOptions {
  /** Arqlet layout options; they win over the ones translated from `layoutOptions`. */
  options?: Record<string, unknown>;
  /** Runs the refinement after the layout. Default `false`. It removes conflicts (routes through nodes, shared lines);
   * a drawing without them comes back the same, a little later. */
  refine?: boolean;
}

export interface ElkLayout {
  /** ELK options without an Arqlet counterpart, reported by the last `layout` call. */
  diagnostics: string[];
  /** Resolves with a copy of `graph`, laid out: positions, sizes, ports, labels and edge `sections`. */
  layout(graph: ElkNode): Promise<ElkNode>;
}

/**
 * An `elk.layout(graph)`-style call on any Arqlet engine function: the WASM engine, a worker or a backend. Not a 1:1
 * ELK. Arqlet contains no ELK code and is not affiliated with the Eclipse Foundation.
 */
export function createElkLayout(
  run: (request: EngineRequest) => EngineResponse | Promise<EngineResponse>,
  options?: ElkLayoutOptions,
): ElkLayout;
