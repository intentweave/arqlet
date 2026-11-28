// The worker pool of pool.mjs on the engine built by apps/playground/build.sh (packages/engine-client/arqlet.wasm).
// node --test examples/worker-pool/pool.test.mjs
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import {createPool} from './pool.mjs';

const wasm = new URL('../../packages/engine-client/arqlet.wasm', import.meta.url);
const graph = JSON.parse(readFileSync(new URL('../fixtures/small.json', import.meta.url), 'utf8'));

test('requests run within the memory budget, and a stale answer is refused by revision', {skip: !existsSync(wasm)}, async () => {
  const module = await WebAssembly.compile(readFileSync(wasm));
  const estimate = (await createEngineFor(module))({op: 'estimate', request: {op: 'import', graph, limit: 100}});
  assert.equal(estimate.ok, true);
  // A budget for one import at a time: four imports queue behind each other although two workers are idle.
  const pool = await createPool({module, workers: 2, budgetBytes: estimate.estimate.bytes * 1.5});
  const answers = await Promise.all([0, 1, 2, 3].map(() => pool.run({op: 'import', graph, limit: 100})));
  assert.ok(answers.every((a) => a.ok));
  assert.ok(pool.stats.peakInFlight <= estimate.estimate.bytes * 1.5);
  assert.ok(pool.stats.waited >= 3);
  // Two edits of the same revision: the second one names a revision the view has moved past and is refused.
  const s = answers[0].snapshot;
  const at = {snapshot: s, expectedRevision: s.revision, expectedLayoutVersion: s.layoutVersion};
  const first = await pool.run({op: 'direction', direction: 'lr', ...at});
  assert.equal(first.ok, true);
  const next = first.snapshot;
  const stale = await pool.run({op: 'direction', direction: 'tb', snapshot: next, expectedRevision: s.revision,
    expectedLayoutVersion: s.layoutVersion});
  assert.equal(stale.ok, false);
  await pool.close();
});

test('a worker over its time limit is replaced and the request answers timeout', {skip: !existsSync(wasm)}, async () => {
  const module = await WebAssembly.compile(readFileSync(wasm));
  const pool = await createPool({module, workers: 1, budgetBytes: Infinity, timeoutMs: 1});
  const answer = await pool.run({op: 'import', graph, limit: 100});
  assert.equal(answer.timeout, true);
  assert.equal(pool.stats.timeouts, 1);
  await pool.close();
});

test('a full queue answers busy; heavy refusals run alone; reads bypass the budget', {skip: !existsSync(wasm)}, async () => {
  const module = await WebAssembly.compile(readFileSync(wasm));
  const pool = await createPool({module, workers: 1, budgetBytes: Infinity, maxQueue: 1});
  const first = pool.run({op: 'import', graph, limit: 100});
  const second = pool.run({op: 'import', graph, limit: 100});
  const third = await pool.run({op: 'import', graph, limit: 100});
  assert.equal(third.busy, true);
  assert.equal((await first).ok, true);
  assert.equal((await second).ok, true);
  // elkLayout is not estimated but lays out: refused as heavy, it still runs (alone).
  const elk = await pool.run({op: 'elkLayout', graph: {id: 'root', children: [{id: 'a', width: 10, height: 10}]}});
  assert.equal(elk.ok, true, elk.error);
  // A read the estimate refuses as light runs without a share of the budget.
  const s = (await first).snapshot;
  const scene = await pool.run({op: 'scene', snapshot: s});
  assert.equal(scene.ok, true, scene.error);
  assert.equal(pool.stats.rejected, 1);
  await pool.close();
});

async function createEngineFor(module) {
  const {createEngine} = await import('../../packages/engine-client/wasm-client.js');
  return createEngine(module);
}
