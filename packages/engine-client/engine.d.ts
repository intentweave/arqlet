import type {Scene, Snapshot} from '@arqlet/contracts';
import type {Run} from './wasm-client.js';

export type {EngineRequest, EngineResponse, Run} from './wasm-client.js';

/** URL of the engine shipped with this package (`arqlet.wasm` beside this module). */
export const defaultWasmUrl: URL;

/**
 * Loads the WASM engine, by default the one shipped with this package. In a browser it is fetched; from a `file:` URL
 * (Node) it is read from disk. `fetchBytes` replaces the loader, for example in tests.
 */
export function loadEngine(
  url?: string | URL,
  fetchBytes?: (url: string | URL) => Promise<BufferSource>,
): Promise<Run>;

/** Scene v1 for a snapshot, computed by the engine and cached per snapshot object. Throws the engine's error. */
export function sceneOf(run: Run, snapshot: Snapshot): Scene;

/**
 * Starts a module worker that answers engine requests (browser only). Post `{requestId, request}`; the worker answers
 * `{requestId, ...response}` and, while a request runs, `{requestId, publication, emittedAt, bytes}` for each
 * progressive publication.
 */
export function createEngineWorker(wasmUrl?: string | URL): Worker;

/** A message to the worker of `createEngineWorker`. */
export interface WorkerRequest {
  requestId: string | number;
  request: import('./wasm-client.js').EngineRequest;
}
