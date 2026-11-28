# A worker pool with admission by memory (example, provisional)

A small Node pool of WASM engine workers that admits requests by their estimated peak memory (ADR 0024, step C3; the
operation `estimate`). It belongs to the [integration guide](../../docs/integration-guide.md#10-a-worker-pool-with-admission-by-memory-provisional),
which explains the choices; this is the shortest code that shows them, not an API.

- `pool.mjs`: `createPool({module, workers, budgetBytes, timeoutMs, recycleAboveBytes, maxQueue, readyTimeoutMs})`.
  - **Admission.** Every request is estimated first (`{op: 'estimate', request}`, on an engine of the same build in
    the main thread). It then waits, first come first served, until its estimate fits beside what runs, and runs on an
    idle worker.
  - **Refused estimates.** A request larger than the whole budget runs alone, and so does one the estimate refuses as
    heavy (`heavy: true`: `elkLayout`, `structuredRefine`, …). A refusal is never taken for 0 bytes. A read refused as
    light (`heavy: false`) bypasses the budget.
  - **Backpressure.** The queue is capped: a full queue answers `{ok: false, busy: true}` at once.
  - **Failures.** A worker over its time limit is terminated and replaced, and its request answers
    `{ok: false, timeout: true}`. A worker whose request threw, or that died, is replaced too. A worker must answer
    ready within `readyTimeoutMs`; a failed start is retried.
  - **Recycling.** A worker whose linear memory grew beyond `recycleAboveBytes` is replaced after its request, since
    WebAssembly never gives memory back.
  - **The estimator's own memory.** The estimating engine reads every request once, so it keeps the high-water mark of
    the largest request it estimated. Count it in the budget, or recycle it.
- `worker.mjs`: one engine per worker thread. It answers a request and its linear memory, or `failed` when the
  request threw.
- `pool.test.mjs`: the tests check that:
  - four imports under a budget for one at a time queue although two workers are idle;
  - an edit for a revision the view has moved past is refused;
  - a request over its time limit answers `timeout`;
  - a full queue answers `busy`;
  - a heavy unestimated request runs, and a read runs.

The estimate is calibrated, not a guarantee: keep headroom in the budget.

The workers are stateless: each request carries its snapshot. A pool of resident sessions binds a session to its worker
and moves it with `session.recycle(newEngine)` instead of dropping the worker.

```sh
sh apps/playground/build.sh examples/fixtures/small.json   # builds packages/engine-client/arqlet.wasm
node --test examples/worker-pool/pool.test.mjs
```
