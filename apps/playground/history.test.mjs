import test from 'node:test';
import assert from 'node:assert/strict';
import {History} from './history.js';

const entry = (label, before, after) => ({label, before: {v: before}, after: {v: after}});

test('an action is one entry; undo and redo move it between the lines', () => {
  const h = new History();
  h.record(entry('a', 0, 1));
  h.record(entry('b', 1, 2));
  assert.deepEqual(h.labels, ['a', 'b']);
  assert.deepEqual(h.peekUndo().before, {v: 1});
  h.commitUndo();
  assert.equal(h.canRedo, true);
  assert.deepEqual(h.peekRedo().after, {v: 2});
  h.commitRedo();
  assert.deepEqual(h.labels, ['a', 'b']);
  assert.equal(h.canRedo, false);
});

test('an action that changed nothing is not recorded, and a new action ends the redo line', () => {
  const h = new History();
  h.record(entry('same', 3, 3));
  assert.equal(h.canUndo, false);
  h.record(entry('a', 0, 1));
  h.commitUndo();
  assert.equal(h.canRedo, true);
  h.record(entry('b', 0, 5));
  assert.equal(h.canRedo, false);
});

test('a refused restore loses nothing: the entry stays until it is committed', () => {
  const h = new History();
  h.record(entry('a', 0, 1));
  h.peekUndo();
  assert.equal(h.canUndo, true);
  assert.equal(h.canRedo, false);
});

test('a follow-up merges into the last entry, and a merge back to the start removes it', () => {
  const h = new History();
  h.record(entry('drop', 0, 1));
  h.record({label: 'grow', before: {v: 1}, after: {v: 2}}, {merge: true});
  assert.deepEqual(h.labels, ['drop']);
  assert.deepEqual(h.peekUndo().after, {v: 2});
  h.record({label: 'back', before: {v: 2}, after: {v: 0}}, {merge: true});
  assert.equal(h.canUndo, false);
});

test('the stack keeps the newest fifty entries and clear empties both lines', () => {
  const h = new History(50);
  for (let i = 0; i < 60; i++) h.record(entry(`e${i}`, i, i + 1));
  assert.equal(h.labels.length, 50);
  assert.equal(h.labels[0], 'e10');
  h.commitUndo();
  h.clear();
  assert.equal(h.canUndo || h.canRedo, false);
});
