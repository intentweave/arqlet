import {test} from 'node:test';
import assert from 'node:assert/strict';
import {presetState, applyPreset} from './presets.js';

const presets = new Map([['lesbar', {id: 'lesbar', version: 1, graph: {defaultDirection: 'hybrid'}, configSet: {id: 'lesbar@1'}}]]);

test('a snapshot is in a preset only for the exact shipped version', () => {
  assert.equal(presetState({options: {configId: 'lesbar@1'}}, presets).preset.id, 'lesbar');
  const old = presetState({options: {configId: 'lesbar@0'}}, presets);
  assert.equal(old.preset, null);
  assert.equal(old.stale, true, 'an older version is reported as stale, not as the current preset');
  assert.equal(presetState({options: {}}, presets).preset, null);
  assert.equal(presetState({options: {configId: 'unknown@1'}}, presets).preset, null);
});

test('the direction step runs first and the configure step uses the snapshot it returned', async () => {
  const seen = [];
  const run = async request => {
    seen.push(request.op);
    return {ok: true, snapshot: {...request.snapshot, marker: request.op}};
  };
  const result = await applyPreset({
    preset: presets.get('lesbar'), snapshot: {graph: {defaultDirection: 'lr'}}, run, stillCurrent: () => true,
  });
  assert.deepEqual(seen, ['direction', 'configure']);
  assert.equal(result.snapshot.marker, 'configure');
});

test('a failed direction step stops the sequence: no configure is sent', async () => {
  const seen = [];
  const run = async request => { seen.push(request.op); return {ok: false, error: 'stale'}; };
  const result = await applyPreset({
    preset: presets.get('lesbar'), snapshot: {graph: {defaultDirection: 'lr'}}, run, stillCurrent: () => true,
  });
  assert.deepEqual(seen, ['direction']);
  assert.equal(result.ok, false);
  assert.match(result.error, /not applied/);
});

test('a view that changed between the steps stops the sequence', async () => {
  const seen = [];
  const run = async request => { seen.push(request.op); return {ok: true, snapshot: request.snapshot}; };
  const result = await applyPreset({
    preset: presets.get('lesbar'), snapshot: {graph: {defaultDirection: 'lr'}}, run, stillCurrent: () => false,
  });
  assert.deepEqual(seen, ['direction'], 'no configure after the view changed');
  assert.equal(result.ok, false);
});

test('no direction step is sent when the view already has the preset direction', async () => {
  const seen = [];
  const run = async request => { seen.push(request.op); return {ok: true, snapshot: request.snapshot}; };
  await applyPreset({
    preset: presets.get('lesbar'), snapshot: {graph: {defaultDirection: 'auto'}}, run, stillCurrent: () => true,
  });
  assert.deepEqual(seen, ['configure']);
});
