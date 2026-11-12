/**
 * Browser-side access to the Rust engine for scene requests. The engine is the only producer of scene
 * geometry; hosts call `sceneOf` and pass the result to a renderer.
 */
import {createEngine} from './wasm-client.js';

/** The engine shipped with this package. Bundlers that understand `new URL(..., import.meta.url)` copy it along. */
export const defaultWasmUrl = new URL('./arqlet.wasm', import.meta.url);

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

/** Loads the WASM engine, by default the one shipped with this package. `fetchBytes` may be replaced in tests. */
export async function loadEngine(url = defaultWasmUrl, fetchBytes = readWasm) {
  return createEngine(await fetchBytes(url));
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
 * Starts a module worker that answers engine requests for the WASM at `wasmUrl`. The worker's own URL is
 * resolved from this module, so the same package works from any host page or import map.
 */
export function createEngineWorker(wasmUrl = defaultWasmUrl) {
  const url = new URL('./worker.js', import.meta.url);
  url.searchParams.set('wasm', new URL(wasmUrl, document.baseURI).href);
  return new Worker(url, {type: 'module'});
}
