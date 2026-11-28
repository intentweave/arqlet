# A worker pool with admission by memory (example, provisional)

A small Node pool of WASM engine workers that admits requests by their estimated peak memory (ADR 0024, step C3; the
operation `estimate`). It belongs to the [integration guide](../../docs/integration-guide.md#10-a-worker-pool-with-admission-by-memory-provisional),
which explains the choices; this is the shortest code that shows them, not an API.

- `pool.mjs`: `createPool({module, workers, budgetBytes, timeoutMs, recycleAboveBytes})`. Every request is estimated
  first (`{op: 'estimate', request}` on an engine of the same build), waits first come, first served until its estimate
  fits beside what runs, and runs on an idle worker. A request larger than the whole budget runs alone. A worker over
  its time limit is terminated and replaced (the request answers `{ok: false, timeout: true}`); a worker whose linear
  memory grew beyond `recycleAboveBytes` is replaced after its request, since WebAssembly never gives memory back.
- `worker.mjs`: one engine per worker thread; answers a request and its linear memory.
- `pool.test.mjs`: four imports under a budget for one at a time queue although two workers are idle; an edit for a
  revision the view has moved past is refused; a request over its time limit answers `timeout`.

The workers are stateless: each request carries its snapshot. A pool of resident sessions binds a session to its worker
and moves it with `session.recycle(newEngine)` instead of dropping the worker.

```sh
sh apps/playground/build.sh examples/fixtures/small.json   # builds packages/engine-client/arqlet.wasm
node --test examples/worker-pool/pool.test.mjs
```
