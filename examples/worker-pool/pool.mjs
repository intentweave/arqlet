// A pool of WASM engine workers with admission by estimated memory (ADR 0024, C3; provisional; an example, not an API).
//
// - Each request is estimated first (`estimate`, on an engine of the same build in the main thread; that engine's own
//   memory is the reading of the largest request it estimated, and it keeps that high-water mark like any WASM engine).
// - A request with an estimate waits, first come first served, until its estimate fits beside what runs. One larger than
//   the whole budget, or one the estimate refuses as heavy (`heavy: true`: it lays out or refines), runs alone.
// - A light request (refused with `heavy: false`: a read) bypasses the budget and the queue's order: it runs on the next
//   idle worker.
// - The queue is capped (`maxQueue`): beyond it a request answers `{ok: false, busy: true}` at once (backpressure).
// - A worker over its time limit is terminated and replaced, and its request answers `{ok: false, timeout: true}`. A
//   worker whose request threw, or that died (`error`, `exit`), is replaced too. A worker whose linear memory grew beyond
//   `recycleAboveBytes` is replaced after its request: WebAssembly never gives memory back.
//
// The estimate is calibrated, not a guarantee (benchmarks/results/memory-estimate-20261127.md): keep headroom in the
// budget and watch `heapPeakBytes` against it.
import {Worker} from 'node:worker_threads';
import {createEngine} from '../../packages/engine-client/wasm-client.js';

export async function createPool({
  module, workers = 2, budgetBytes, timeoutMs = 60_000, recycleAboveBytes = Infinity, maxQueue = 1000,
  readyTimeoutMs = 10_000,
}) {
  const estimator = await createEngine(module);
  const idle = [];
  const queue = [];   // estimated requests, first come first served
  const light = [];   // reads: no share of the budget
  let running = 0;
  let inFlight = 0;
  let nextId = 0;
  let closed = false;
  const stats = {admitted: 0, waited: 0, rejected: 0, timeouts: 0, failed: 0, recycled: 0, peakInFlight: 0, spawnFailures: 0};

  // A worker that answers `{ready: true}` within the time limit, or a rejection.
  const spawn = () => new Promise((resolve, reject) => {
    const worker = new Worker(new URL('./worker.mjs', import.meta.url), {workerData: {module}});
    const timer = setTimeout(() => { worker.terminate(); reject(new Error('worker not ready in time')); }, readyTimeoutMs);
    worker.once('error', (e) => { clearTimeout(timer); reject(e); });
    worker.once('message', () => { clearTimeout(timer); resolve(worker); });
  });
  // Adds a fresh worker; on failure tries again a second later, so the pool does not shrink for good.
  const replenish = async () => {
    if (closed) return;
    try {
      idle.push(await spawn());
      pump();
    } catch {
      stats.spawnFailures++;
      setTimeout(replenish, 1000);
    }
  };
  for (let i = 0; i < workers; i++) idle.push(await spawn());

  const pump = () => {
    while (idle.length && light.length) start(light.shift(), idle.pop());
    while (queue.length && idle.length) {
      const job = queue[0];
      // The head waits until it fits, or until nothing runs if it is larger than the budget (or heavy and unestimated).
      if (running > 0 && inFlight + job.bytes > budgetBytes) return;
      queue.shift();
      start(job, idle.pop());
    }
  };

  const start = (job, worker) => {
    running++;
    const share = Number.isFinite(job.bytes) ? job.bytes : budgetBytes;
    inFlight += share;
    stats.admitted++;
    stats.peakInFlight = Math.max(stats.peakInFlight, inFlight);
    const id = nextId++;
    let done = false;
    const finish = (answer, replace) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      worker.off('message', onMessage);
      worker.off('error', onDeath);
      worker.off('exit', onDeath);
      running--;
      inFlight -= share;
      if (replace) {
        worker.terminate();
        replenish();
      } else {
        idle.push(worker);
      }
      job.resolve(answer);
      pump();
    };
    const onMessage = (message) => {
      if (message.id !== id) return;
      if (message.failed) stats.failed++;
      const recycle = message.linearMemoryBytes > recycleAboveBytes;
      if (recycle) stats.recycled++;
      finish(message.answer, recycle || message.failed);
    };
    const onDeath = (cause) => {
      stats.failed++;
      finish({ok: false, error: `worker died: ${cause instanceof Error ? cause.message : `exit ${cause}`}`}, true);
    };
    // The engine cannot be interrupted from outside: terminating the worker is the hard stop.
    const timer = setTimeout(() => {
      stats.timeouts++;
      finish({ok: false, error: 'timeout', timeout: true}, true);
    }, timeoutMs);
    worker.on('message', onMessage);
    worker.once('error', onDeath);
    worker.once('exit', onDeath);
    worker.postMessage({id, request: job.request});
  };

  return {
    stats,
    /** Runs `request` when its estimated peak fits; answers what the engine answers, `timeout`, or `busy`. */
    run(request) {
      if (queue.length + light.length >= maxQueue) {
        stats.rejected++;
        return Promise.resolve({ok: false, busy: true, error: 'queue full'});
      }
      const estimate = estimator({op: 'estimate', request});
      return new Promise((resolve) => {
        if (!estimate.ok && estimate.heavy === false) {
          light.push({request, bytes: 0, resolve});
        } else {
          // Unestimated but heavy (or unreadable): alone.
          const bytes = estimate.ok ? estimate.estimate.bytes : Infinity;
          if (queue.length || !idle.length || (running > 0 && inFlight + bytes > budgetBytes)) stats.waited++;
          queue.push({request, bytes, resolve});
        }
        pump();
      });
    },
    async close() {
      closed = true;
      await Promise.all(idle.splice(0).map((w) => w.terminate()));
    },
  };
}
