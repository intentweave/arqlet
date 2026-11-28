// One engine in a worker thread of the pool (pool.mjs): runs one request at a time and reports its linear memory, which
// WebAssembly never gives back, so the pool can replace a worker that has grown too large. A request that throws (a
// trap) answers `failed`, and the pool replaces the worker: its memory may be in any state.
import {parentPort, workerData} from 'node:worker_threads';
import {createEngine} from '../../packages/engine-client/wasm-client.js';

const run = await createEngine(workerData.module);
// An empty session answers `memory` with the module's linear memory (ADR 0023, R7).
const probe = run.openSession();
parentPort.on('message', ({id, request}) => {
  try {
    const answer = run(request);
    parentPort.postMessage({id, answer, linearMemoryBytes: probe.run({op: 'memory'}).linearMemoryBytes});
  } catch (error) {
    parentPort.postMessage({id, answer: {ok: false, error: String(error)}, failed: true});
  }
});
parentPort.postMessage({ready: true});
