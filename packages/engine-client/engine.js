/**
 * Browser-side access to the Rust engine for scene requests. The engine is the only producer of scene
 * geometry; hosts call `sceneOf` and pass the result to a renderer.
 */
import {createEngine} from './wasm-client.js';

/** The engine shipped with this package. Bundlers that understand `new URL(..., import.meta.url)` copy it along. */
export const defaultWasmUrl = new URL('./arqlet.wasm', import.meta.url);

/**
 * Provisional (#210): the engine built with its cargo feature `query` (`arqlet-engine-query.wasm` beside this module): the
 * default engine plus the operation `query`. A separate file, so that a host that does not ask for it downloads nothing more.
 */
export const queryWasmUrl = new URL('./arqlet-engine-query.wasm', import.meta.url);

// The module a host asked for: a URL (the default engine when omitted), or `{query: true}` for the engine with the
// operation `query`, or `{url}` for a module of the host's own.
function moduleUrl(which) {
  if (which === undefined || which === null) return defaultWasmUrl;
  if (typeof which === 'string' || which instanceof URL) return which;
  return which.url ?? (which.query ? queryWasmUrl : defaultWasmUrl);
}

// Reads the module: over HTTP(S) with `fetch`, and from a `file:` URL (Node) from disk. The module name is held in a
// variable so that browser bundlers do not try to resolve the Node built-in.
async function readWasm(url) {
  const href = new URL(url, globalThis.document?.baseURI ?? import.meta.url);
  if (href.protocol === 'file:') {
    const fs = 'node:fs/promises';
    const {readFile} = await import(fs);
    return readFile(href);
  }
  // The browser's HTTP cache applies, as for any asset.
  const response = await fetch(href);
  if (!response.ok) throw Error(`Engine not available: ${href}`);
  return response.arrayBuffer();
}

/**
 * Loads the WASM engine, by default the one shipped with this package. `{query: true}` loads the one with the operation
 * `query` instead (`session.query()`, also with `view: true`). `fetchBytes` may be replaced in tests.
 */
export async function loadEngine(which, fetchBytes = readWasm) {
  return createEngine(await fetchBytes(moduleUrl(which)));
}

// The scene of a snapshot object, per engine. A snapshot is replaced by every change and never edited in place, so the
// object identifies its scene; the drawing, the statistics and the tree then share one engine call.
const scenes = new WeakMap();

/** Scene v1 for a snapshot, computed by the engine. Throws the engine's error explicitly. */
export function sceneOf(run, snapshot) {
  let perSnapshot = scenes.get(run);
  if (!perSnapshot) scenes.set(run, perSnapshot = new WeakMap());
  const known = perSnapshot.get(snapshot);
  if (known) return known;
  const result = run({op: 'scene', snapshot});
  if (!result.ok) throw Error(result.error);
  perSnapshot.set(snapshot, result.scene);
  return result.scene;
}

/**
 * Provisional: the groups and communities the view of `graph` adds with `options` (`groups`, `communities`,
 * `communityHubs`), as the `groups` option takes them: node id → path of labels. Computed once (in a backend, for a large
 * graph) and given back as `options.groups`, the view gets the same containers without computing communities again.
 * Throws the engine's error.
 */
export function communityGroups(run, graph, options = {}) {
  const result = run({op: 'communities', graph, options});
  if (!result.ok) throw Error(result.error);
  return result.groups;
}

/**
 * Provisional: the first view of `graph` (`import`). With `omitSource`, the snapshot leaves the source graph out and
 * carries its fingerprint (`sourceDigest`); later requests on it send the graph as `source`. Throws the engine's error.
 */
export function importGraph(run, graph, limit, options = {}, {omitSource = false} = {}) {
  const result = run({op: 'import', graph, limit, options, ...(omitSource ? {omitSource: true} : {})});
  if (!result.ok) throw Error(result.error);
  return result.snapshot;
}

/**
 * Starts a module worker that answers engine requests for the WASM at `wasmUrl` (or, with `{query: true}`, for the
 * engine with the operation `query`). The worker's own URL is
 * resolved from this module, so the same package works from any host page or import map.
 */
export function createEngineWorker(which) {
  const url = new URL('./worker.js', import.meta.url);
  url.searchParams.set('wasm', new URL(moduleUrl(which), document.baseURI).href);
  return new Worker(url, {type: 'module'});
}
