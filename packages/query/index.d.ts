import type {Graph} from '@arqlet/contracts';

export type {Graph} from '@arqlet/contracts';

/**
 * Provisional (ADR 0021): openCypher queries over an Arqlet graph, in a WebAssembly module without the layout engine.
 * Calls are synchronous; a host that queries large graphs runs them in a worker.
 */

/** URL of the module shipped with this package (`arqlet-query.wasm` beside this module). */
export const defaultWasmUrl: URL;

/** A value a query sees or takes: a JSON value without maps. */
export type PropertyValue = null | boolean | number | string | PropertyValue[];

/**
 * Properties beside the graph, by node id and by edge id. A key may not be a property the graph has itself (`id`, `kind`,
 * `label`, `parentId` for a node; `id`, `kind`, `weight` for an edge): the graph's value is what a query reads.
 */
export interface Payload {
  nodes?: {[nodeId: string]: {[key: string]: PropertyValue}};
  edges?: {[edgeId: string]: {[key: string]: PropertyValue}};
}

export interface QueryInit {
  /** The values of the `$name` parameters. */
  params?: {[name: string]: PropertyValue};
  /** Execution limits: steps of work (default 20 000 000), rows held (2 000 000), hops of an unbounded `*` (64). */
  maxSteps?: number;
  maxRows?: number;
  maxHops?: number;
}

/** A value in a row of an answer: a JSON value, a node as `{node: id}`, an edge as `{edge: id}`, or a list. */
export type QueryValue = null | boolean | number | string | {node: string} | {edge: string} | QueryValue[];

export interface LoadAnswer {
  /** The nodes and edges of the graph as given. */
  nodes: number;
  edges: number;
  /** The edges a query does not see: no target, or an end that is no node. */
  unresolvedEdges: number;
}

export interface QueryAnswer {
  columns: string[];
  rows: QueryValue[][];
  rowCount: number;
  /** The ids of the nodes and edges in the rows, once each, in the order they first appear. */
  nodes: string[];
  edges: string[];
  /** What the query could see (resolved edges only). */
  graph: {nodes: number; edges: number};
  /** Payload entries for ids the graph does not have (or unresolved edges), ignored by the query. */
  ignoredContent?: number;
}

/** An error is an answer, not an exception. A query error reads `query <class>: message`. */
export type Answer<T> = ({ok: true} & T) | {ok: false; error: string};

export interface QueryEngine {
  /** Keeps `graph` and its optional payload, replacing what was loaded. A refused load keeps what was loaded. */
  load(graph: Graph, init?: {payload?: Payload}): Answer<LoadAnswer>;
  /** Runs an openCypher query (read only) over the loaded graph. */
  query(text: string, init?: QueryInit): Answer<QueryAnswer>;
}

/**
 * A query engine on its own instance of the module, from the bytes of the module or a compiled `WebAssembly.Module`.
 * Each engine holds its own graph. After a call that stopped the module (out of memory, stack exhausted), the engine
 * answers errors only: create a new one.
 */
export function createQueryEngine(source: BufferSource | WebAssembly.Module): Promise<QueryEngine>;

/**
 * Loads the module (by default the one shipped with this package) and creates an engine on it. In a browser it is fetched;
 * from a `file:` URL (Node) it is read from disk. `fetchBytes` replaces the loader, for example in tests.
 */
export function loadQueryEngine(
  url?: string | URL,
  fetchBytes?: (url: string | URL) => Promise<BufferSource | WebAssembly.Module>,
): Promise<QueryEngine>;
