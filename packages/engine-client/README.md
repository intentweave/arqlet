# @arqlet/engine-client

The Arqlet engine as WebAssembly (`arqlet.wasm`, included) and its JavaScript access:

- `engine.js`: `loadEngine(url?)` (browser via `fetch`, Node from disk), `sceneOf(run, snapshot)`, `createEngineWorker(url?)`;
- `wasm-client.js`: `createEngine(bytes, {onPublish})`, the raw binding;
- `elk.js`: `createElkLayout(run)`, an `elk.layout(graph)`-style call;
- `session.js`: `createSession(run)`, a layout session that keeps positions across changes (collapse, expand, …)
  with revision checks, and `workerRun(worker)`, the worker as an asynchronous `run`;
- `presets.js`: `presetState` and `applyPreset`.

Most users want the [`arqlet`](https://www.npmjs.com/package/arqlet) package, which re-exports these.
Bundlers that understand `new URL('./arqlet.wasm', import.meta.url)` (Vite, webpack 5,
esbuild with a loader) copy the engine along; otherwise pass its URL to `loadEngine`.

> **Status: prototype.** The API, the JSON protocol and the snapshot format are
> provisional and may change in any release.

Part of [Arqlet](https://github.com/intentweave/arqlet), an incremental graph-layout
engine in Rust. See the repository for documentation and the current status.

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.
