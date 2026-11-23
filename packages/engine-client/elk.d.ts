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
  /** Reports where each call's time went in `timings` (#152). Default `false`. */
  timings?: boolean;
}

/** Where a call's time went, in ms (#152). The phases below `layout` are parts of it; 0 where the engine has no clock. */
export interface ElkTimings {
  total: number;
  layout: number;
  placement: number;
  organicStress: number;
  separation: number;
  repair: number;
  portResolution: number;
  routing: number;
  /** Single pass, parts of `routing`. */
  plan: number;
  channelGrowth: number;
  construction: number;
  fallbackRouting: number;
  finalization: number;
  buses: number;
  /** Outside `layout`: the ELK translation in and out, and a refinement on request. */
  elkTranslation: number;
  refinement: number;
}

export interface ElkLayout {
  /** ELK options without an Arqlet counterpart, reported by the last `layout` call. */
  diagnostics: string[];
  /** With `timings: true`, where the last `layout` call's time went; otherwise `null`. */
  timings: ElkTimings | null;
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
