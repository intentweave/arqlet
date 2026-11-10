// Host-level check: the WASM engine must build the same scene as the viewer fixture. It needs the
// generated WASM from build.sh, so it lives with the demo instead of the viewer package.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import {checkScene} from '@arqlet/viewer/scene.js';
import {loadEngine, sceneOf} from '@arqlet/engine-client/engine.js';

const scene = name => checkScene(JSON.parse(readFileSync(new URL(`../../packages/viewer/fixtures/${name}.scene.json`, import.meta.url), 'utf8')));

test('the engine builds the same scene as the fixture (WASM path)', async t => {
  const wasm = new URL('./generated/arqlet.wasm', import.meta.url);
  if (!existsSync(wasm)) return t.skip('run build.sh to produce the WASM engine');
  const snapshot = JSON.parse(readFileSync(new URL('../../packages/viewer/fixtures/bpmn-pools.snapshot.json', import.meta.url), 'utf8'));
  const run = await loadEngine(wasm.pathname, async () => readFileSync(wasm));
  assert.deepEqual(sceneOf(run, snapshot), scene('bpmn-pools'));
});

test('a snapshot object is turned into a scene once; a new object is a new request', () => {
  let calls = 0;
  const run = request => { calls += 1; return {ok: true, scene: {n: calls, revision: request.snapshot.revision}}; };
  const a = {revision: 1};
  assert.equal(sceneOf(run, a), sceneOf(run, a));
  assert.equal(calls, 1);
  assert.equal(sceneOf(run, {revision: 1}).n, 2);
  const failing = () => ({ok: false, error: 'no scene'});
  assert.throws(() => sceneOf(failing, a), /no scene/);
});
