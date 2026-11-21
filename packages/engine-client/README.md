# @arqlet/engine-client

The Arqlet engine as WebAssembly (`arqlet.wasm`, included) and its JavaScript access:

- `engine.js`: `loadEngine(url?)` (browser via `fetch`, Node from disk), `sceneOf(run, snapshot)`, `createEngineWorker(url?)`;
- `wasm-client.js`: `createEngine(bytes, {onPublish})`, the raw binding;
- `elk.js`: `createElkLayout(run)`, an `elk.layout(graph)`-style call;
- `session.js`: `createSession(run)`, a layout session that keeps positions across changes (collapse, expand, …)
  with revision checks, `createOverviewSession(run)`, a cluster overview whose graph stays in the engine (later layouts
  send only what changed), and `workerRun(worker)` / `httpRun(base)`, a worker or a backend as an asynchronous `run`;
- `view-controller.js`: `createViewController(session)`, a headless view over a session (provisional): one state object
  with `subscribe` (for `useSyncExternalStore`), open/close, paging through `+N more`, filters, a path or neighbourhood
  kept as the highlight, the details of a selected node or edge (`select`, `state.details`) and the hover, back and
  forward, search (`find`), `focus` for the camera, bookmarks (`bookmark`, `openBookmark`, `encodeBookmark`), and a
  scene diff per step, snapshots the host made (`adopt`);
  `sceneDiff(before, after)`;
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
