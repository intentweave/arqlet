import {test} from 'node:test';
import assert from 'node:assert/strict';
import {MoveGesture, releaseOnDrop} from './drag.js';

// A fake engine: each call returns a promise the test resolves by hand, so the order of replies is under test control.
function fakeEngine() {
  const calls = [];
  const send = request => new Promise(resolve => calls.push({request, resolve}));
  return {calls, send};
}
const base = {revision: 1, layoutVersion: 0, boxes: {}};
const next = version => ({revision: 1, layoutVersion: version, boxes: {}});
const flush = () => new Promise(resolve => setImmediate(resolve));

test('each step is numbered and carries the base of the last reply', async () => {
  const engine = fakeEngine();
  const applied = [];
  const gesture = new MoveGesture({send: engine.send, base, onSnapshot: s => applied.push(s)});
  gesture.begin('a', {x: 1, y: 2});
  assert.equal(engine.calls[0].request.seq, 1);
  assert.equal(engine.calls[0].request.phase, 'begin');
  engine.calls[0].resolve({ok: true, snapshot: next(1)});
  await flush();
  gesture.update({x: 5, y: 5});
  assert.equal(engine.calls[1].request.seq, 2);
  assert.equal(engine.calls[1].request.expectedLayoutVersion, 1, 'the next base is the last reply');
  engine.calls[1].resolve({ok: true, snapshot: next(2)});
  await flush();
  assert.equal(applied.length, 2);
  assert.equal(applied[1].layoutVersion, 2);
});

test('positions that arrive while a request runs are coalesced into the newest one', async () => {
  const engine = fakeEngine();
  const gesture = new MoveGesture({send: engine.send, base, onSnapshot: () => {}});
  gesture.begin('a', {x: 0, y: 0});
  engine.calls[0].resolve({ok: true, snapshot: next(1)});
  await flush();
  gesture.update({x: 1, y: 1});
  gesture.update({x: 2, y: 2});
  gesture.update({x: 3, y: 3});
  assert.equal(engine.calls.length, 2, 'only one update is in flight');
  engine.calls[1].resolve({ok: true, snapshot: next(2)});
  await flush();
  assert.equal(engine.calls.length, 3, 'the newest pending position is sent next');
  assert.deepEqual(engine.calls[2].request.position, {x: 3, y: 3});
  // The first update went out at once; the second was pending, and the third replaced it.
  assert.equal(gesture.stats.coalesced, 1);
});

test('finish applies the last pointer position before it keeps the pin', async () => {
  const engine = fakeEngine();
  const gesture = new MoveGesture({send: engine.send, base, onSnapshot: () => {}});
  gesture.begin('a', {x: 0, y: 0});
  engine.calls[0].resolve({ok: true, snapshot: next(1)});
  await flush();
  gesture.update({x: 9, y: 9});
  gesture.update({x: 10, y: 10});
  gesture.finish({release: false});
  engine.calls[1].resolve({ok: true, snapshot: next(2)});
  await flush();
  assert.equal(engine.calls[2].request.phase, 'update', 'the newest pending position is applied first');
  assert.deepEqual(engine.calls[2].request.position, {x: 10, y: 10});
  engine.calls[2].resolve({ok: true, snapshot: next(3)});
  await flush();
  assert.equal(engine.calls[3].request.phase, 'finish');
  assert.equal(engine.calls[3].request.release, false, 'the pin is kept');
  engine.calls[3].resolve({ok: true, snapshot: next(4)});
  await flush();
  assert.equal(gesture.active, false);
  assert.equal(gesture.stats.finished, 1);
});

test('a cancel drops the pending position and restores the previous pin', async () => {
  const engine = fakeEngine();
  const gesture = new MoveGesture({send: engine.send, base, onSnapshot: () => {}});
  gesture.begin('a', {x: 0, y: 0});
  engine.calls[0].resolve({ok: true, snapshot: next(1)});
  await flush();
  gesture.update({x: 4, y: 4});
  gesture.cancel();
  engine.calls[1].resolve({ok: true, snapshot: next(2)});
  await flush();
  assert.equal(engine.calls[2].request.phase, 'cancel');
  assert.equal(engine.calls[2].request.position, null);
});

test('a refused step discards the gesture and its later replies', async () => {
  const engine = fakeEngine();
  const outcomes = [];
  const gesture = new MoveGesture({send: engine.send, base, onSnapshot: () => {}, onStats: s => outcomes.push(s)});
  gesture.begin('a', {x: 0, y: 0});
  engine.calls[0].resolve({ok: false, error: 'Stale move step'});
  await flush();
  assert.equal(gesture.active, false);
  assert.equal(outcomes.at(-1).discarded, 1);
  assert.equal(outcomes.at(-1).error, 'Stale move step');
});

test('a second gesture cannot start while one runs; an abandoned gesture ignores its replies', async () => {
  const engine = fakeEngine();
  const applied = [];
  const gesture = new MoveGesture({send: engine.send, base, onSnapshot: s => applied.push(s)});
  gesture.begin('a', {x: 0, y: 0});
  assert.throws(() => gesture.begin('b', {x: 1, y: 1}));
  gesture.abandon();
  engine.calls[0].resolve({ok: true, snapshot: next(1)});
  await flush();
  assert.equal(applied.length, 0);
});

test('a drag on the pin fixes the position; a drag on the node leaves no pin unless the node had one', () => {
  assert.equal(releaseOnDrop(true, false), false, 'the pin was dragged: the position is kept');
  assert.equal(releaseOnDrop(true, true), false);
  assert.equal(releaseOnDrop(false, false), true, 'a plain move leaves the node free');
  assert.equal(releaseOnDrop(false, true), false, 'a node that was pinned stays pinned where it is put');
});

test('a gesture finished with release asks the engine to release the pin', async () => {
  const engine = fakeEngine();
  const gesture = new MoveGesture({send: engine.send, base, onSnapshot: () => {}});
  gesture.begin('a', {x: 0, y: 0});
  engine.calls[0].resolve({ok: true, snapshot: next(1)});
  await flush();
  gesture.finish({release: releaseOnDrop(false, false)});
  await flush();
  assert.equal(engine.calls[1].request.phase, 'finish');
  assert.equal(engine.calls[1].request.release, true);
});

test('the other nodes of a selection travel with every step of the gesture', async () => {
  const engine = fakeEngine();
  const gesture = new MoveGesture({send: engine.send, base, onSnapshot: () => {}});
  gesture.begin('a', {x: 0, y: 0}, {b: {x: 10, y: 0}});
  assert.deepEqual(engine.calls[0].request.others, {b: {x: 10, y: 0}});
  engine.calls[0].resolve({ok: true, snapshot: next(1)});
  await flush();
  gesture.update({x: 5, y: 5}, {b: {x: 15, y: 5}});
  assert.deepEqual(engine.calls[1].request.others, {b: {x: 15, y: 5}});
  engine.calls[1].resolve({ok: true, snapshot: next(2)});
  await flush();
  gesture.finish({release: true});
  assert.deepEqual(engine.calls[2].request.others, {b: {x: 15, y: 5}}, 'the finish names the same nodes');
  // A single node sends no `others` at all.
  const alone = fakeEngine();
  new MoveGesture({send: alone.send, base, onSnapshot: () => {}}).begin('a', {x: 0, y: 0});
  assert.equal('others' in alone.calls[0].request, false);
});
