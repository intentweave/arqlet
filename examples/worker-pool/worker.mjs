// One engine in a worker thread of the pool (pool.mjs): runs one request at a time and reports its linear memory, which
// WebAssembly never gives back, so the pool can replace a worker that has grown too large.
import {parentPort, workerData} from 'node:worker_threads';
import {createEngine} from '../../packages/engine-client/wasm-client.js';

const run = await createEngine(workerData.module);
// An empty session answers `memory` with the module's linear memory (ADR 0023, R7).
const probe = run.openSession();
parentPort.on('message', ({id, request}) => {
  let answer;
  try {
    answer = run(request);
  } catch (error) {
    answer = {ok: false, error: String(error)};
  }
  parentPort.postMessage({id, answer, linearMemoryBytes: probe.run({op: 'memory'}).linearMemoryBytes});
});
parentPort.postMessage({ready: true});
