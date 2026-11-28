import type {Scene, Snapshot} from '@arqlet/contracts';
import type {Run} from './wasm-client.js';

export type {EngineRequest, EngineResponse, Run} from './wasm-client.js';

/** URL of the engine shipped with this package (`arqlet.wasm` beside this module). */
export const defaultWasmUrl: URL;

/**
 * Provisional: URL of the engine built with the operation `query` (`arqlet-engine-query.wasm` beside this module). Larger
 * than `defaultWasmUrl` (see the package README); fetched only when a host asks for it.
 */
export const queryWasmUrl: URL;

/** Which module to load: a URL, `{query: true}` (the engine with the operation `query`) or `{url}`; none is the default engine. */
export type EngineModule = string | URL | {query?: boolean; url?: string | URL};

/**
 * Loads the WASM engine, by default the one shipped with this package. In a browser it is fetched; from a `file:` URL
 * (Node) it is read from disk. `{query: true}` loads the engine with the operation `query` instead, so that
 * `session.query(text, {view: true})` answers; the default engine refuses it. `fetchBytes` replaces the loader, for example
 * in tests.
 */
export function loadEngine(
  which?: EngineModule,
  fetchBytes?: (url: string | URL) => Promise<BufferSource>,
): Promise<Run>;

/** Scene v1 for a snapshot, computed by the engine and cached per snapshot object. Throws the engine's error. */
export function sceneOf(run: Run, snapshot: Snapshot): Scene;

/**
 * Provisional: the groups and communities the view of `graph` adds with `options` (`groups`, `communities`,
 * `communityHubs`), as the `groups` option takes them: node id → path of labels. Given back as `options.groups`, the view
 * gets the same containers (of kind `group`) without computing communities again. Throws the engine's error.
 */
export function communityGroups(
  run: Run,
  graph: import('@arqlet/contracts').Graph,
  options?: import('@arqlet/contracts').LayoutOptions,
): {[nodeId: string]: string[]};

/**
 * Provisional: the first view of `graph` (`import`). With `omitSource`, the snapshot leaves the source graph out and
 * carries `sourceDigest`; later requests on it send the graph as `source`. Throws the engine's error.
 */
export function importGraph(
  run: Run,
  graph: import('@arqlet/contracts').Graph,
  limit: number,
  options?: import('@arqlet/contracts').LayoutOptions,
  delivery?: {omitSource?: boolean},
): Snapshot;

/**
 * Starts a module worker that answers engine requests (browser only). Post `{requestId, request}`; the worker answers
 * `{requestId, ...response}` and, while a request runs, `{requestId, publication, emittedAt, bytes}` for each
 * progressive publication.
 */
export function createEngineWorker(which?: EngineModule): Worker;

/** A message to the worker of `createEngineWorker`. */
export interface WorkerRequest {
  requestId: string | number;
  request: import('./wasm-client.js').EngineRequest;
}
