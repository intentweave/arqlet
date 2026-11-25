/**
 * Provisional (ADR 0021): openCypher queries over an Arqlet graph, in a WebAssembly module without the layout engine.
 * Calls are synchronous; a host that queries large graphs runs them in a worker.
 */

/** The module shipped with this package. Bundlers that understand `new URL(..., import.meta.url)` copy it along. */
export const defaultWasmUrl = new URL('./arqlet-query.wasm', import.meta.url);

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
  if (!response.ok) throw Error(`Query module not available: ${href}`);
  return response.arrayBuffer();
}

/**
 * A query engine on its own instance of the module: from the bytes of the module, or a module the host compiled already.
 * Each engine holds its own graph.
 */
export async function createQueryEngine(source) {
  const result = await WebAssembly.instantiate(source, {});
  const {exports: api} = result instanceof WebAssembly.Instance ? result : result.instance;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  // Set when a call trapped (out of memory, stack exhausted, a panic): the instance's state is then undefined.
  let stopped = null;
  const exchange = request => {
    if (stopped) return {ok: false, error: stopped};
    // Outside the `try`: a request that is not JSON (a BigInt, a cycle) is the caller's error and stops nothing.
    const encoded = encoder.encode(JSON.stringify(request));
    try {
      // `>>> 0`: an address above 2 GiB arrives as a negative number.
      const input = api.lw_alloc(encoded.length) >>> 0;
      new Uint8Array(api.memory.buffer, input, encoded.length).set(encoded);
      const packed = api.lw_run(input, encoded.length);
      const address = Number(packed & 0xffffffffn);
      const length = Number(packed >> 32n);
      const text = decoder.decode(new Uint8Array(api.memory.buffer, address, length));
      api.lw_free(input, encoded.length);
      api.lw_free(address, length);
      return JSON.parse(text);
    } catch (error) {
      stopped = `The query module stopped (${error?.message ?? error}); create a new engine and load the graph again.`;
      return {ok: false, error: stopped};
    }
  };
  return {
    /** Keeps `graph` and its optional payload, replacing what was loaded. A refused load keeps what was loaded. */
    load(graph, {payload} = {}) {
      return exchange({op: 'load', graph, payload});
    },
    /** Runs an openCypher query over the loaded graph. */
    query(text, {params, maxSteps, maxRows, maxHops} = {}) {
      return exchange({op: 'query', query: text, params, maxSteps, maxRows, maxHops});
    },
  };
}

/** Loads the module (by default the one shipped with this package) and creates an engine on it. */
export async function loadQueryEngine(url = defaultWasmUrl, fetchBytes = readWasm) {
  return createQueryEngine(await fetchBytes(url));
}
