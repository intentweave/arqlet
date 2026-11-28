// A pool of WASM engine workers with admission by expected memory (ADR 0024, C3; provisional, an example, not an API).
// Each request is estimated first (`estimate`, on an engine of the same build in the main thread), waits in a queue until
// its estimate fits beside what runs, and runs on an idle worker. A request larger than the whole budget runs alone.
// A worker that exceeds a time limit is terminated and replaced (the request answers `timeout`); a worker whose linear
// memory has grown beyond `recycleAboveBytes` is replaced after its request (its memory goes with it).
import {Worker} from 'node:worker_threads';
import {createEngine} from '../../packages/engine-client/wasm-client.js';

export async function createPool({module, workers = 2, budgetBytes, timeoutMs = 60_000, recycleAboveBytes = Infinity}) {
  const estimator = await createEngine(module);
  const idle = [];
  const queue = [];
  let running = 0;
  let inFlight = 0;
  let nextId = 0;
  const stats = {admitted: 0, waited: 0, timeouts: 0, recycled: 0, peakInFlight: 0};

  const spawn = () => new Promise((resolve) => {
    const worker = new Worker(new URL('./worker.mjs', import.meta.url), {workerData: {module}});
    worker.once('message', () => resolve(worker));
  });
  for (let i = 0; i < workers; i++) idle.push(await spawn());

  const pump = () => {
    while (queue.length && idle.length) {
      const job = queue[0];
      // First come, first served: the head waits until it fits (or until nothing runs, if it is larger than the budget).
      if (running > 0 && inFlight + job.bytes > budgetBytes) return;
      queue.shift();
      start(job, idle.pop());
    }
  };

  const start = (job, worker) => {
    running++;
    inFlight += job.bytes;
    stats.admitted++;
    stats.peakInFlight = Math.max(stats.peakInFlight, inFlight);
    const id = nextId++;
    const finish = async (answer, replace) => {
      clearTimeout(timer);
      worker.off('message', onMessage);
      running--;
      inFlight -= job.bytes;
      if (replace) {
        await worker.terminate();
        idle.push(await spawn());
      } else {
        idle.push(worker);
      }
      job.resolve(answer);
      pump();
    };
    const onMessage = (message) => {
      if (message.id !== id) return;
      const recycle = message.linearMemoryBytes > recycleAboveBytes;
      if (recycle) stats.recycled++;
      finish(message.answer, recycle);
    };
    // The engine cannot be interrupted from outside: terminating the worker is the hard stop.
    const timer = setTimeout(() => {
      stats.timeouts++;
      finish({ok: false, error: 'timeout', timeout: true}, true);
    }, timeoutMs);
    worker.on('message', onMessage);
    worker.postMessage({id, request: job.request});
  };

  return {
    stats,
    /** Runs `request` when its estimated peak fits; answers what the engine answers (or `timeout`). */
    run(request) {
      const estimate = estimator({op: 'estimate', request});
      // What the estimate does not know (a read, a scene) is light: it runs without a share of the budget.
      const bytes = estimate.ok ? estimate.estimate.bytes : 0;
      return new Promise((resolve) => {
        if (queue.length || (running > 0 && inFlight + bytes > budgetBytes) || !idle.length) stats.waited++;
        queue.push({request, bytes, resolve});
        pump();
      });
    },
    async close() {
      await Promise.all(idle.splice(0).map((w) => w.terminate()));
    },
  };
}
