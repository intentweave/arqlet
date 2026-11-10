import test from 'node:test';
import assert from 'node:assert/strict';
import {placementOnBorder, positionOnBorder} from './port-drag.js';

const box = {x: 100, y: 50, width: 200, height: 80};
const near = (a, b) => Math.abs(a - b) < 1e-9;

test('a rectangle takes the nearest side and the offset along it', () => {
  assert.deepEqual(placementOnBorder(box, 'rect', {x: 90, y: 90}), {side: 'left', offset: 0.5});
  assert.deepEqual(placementOnBorder(box, 'rect', {x: 310, y: 50}), {side: 'right', offset: 0});
  assert.deepEqual(placementOnBorder(box, 'rect', {x: 150, y: 20}), {side: 'top', offset: 0.25});
  // Beyond the corner both sides are equally far: the side that comes first (right before bottom) wins, at the end of the side.
  assert.deepEqual(placementOnBorder(box, 'rect', {x: 400, y: 230}), {side: 'right', offset: 1});
});

test('a pointer inside the box goes to the nearest side', () => {
  assert.equal(placementOnBorder(box, 'rect', {x: 105, y: 90}).side, 'left');
  assert.equal(placementOnBorder(box, 'rect', {x: 200, y: 125}).side, 'bottom');
});

test('a circle takes the side in whose direction the pointer lies, and the position within its 90 degrees', () => {
  const disc = {x: 0, y: 0, width: 100, height: 100};
  assert.deepEqual(placementOnBorder(disc, 'circle', {x: 200, y: 50}), {side: 'right', offset: 0.5});
  assert.equal(placementOnBorder(disc, 'circle', {x: 50, y: 200}).side, 'bottom');
  assert.equal(placementOnBorder(disc, 'circle', {x: -80, y: 50}).side, 'left');
  assert.equal(placementOnBorder(disc, 'circle', {x: 50, y: -80}).side, 'top');
  const lower = placementOnBorder(disc, 'circle', {x: 200, y: 140});
  assert.equal(lower.side, 'right');
  assert.ok(lower.offset > 0.5 && lower.offset < 1);
});

test('the preview position is the inverse of the placement', () => {
  for (const [shape, bounds] of [['rect', box], ['circle', {x: 0, y: 0, width: 100, height: 100}]]) {
    for (const placement of [{side: 'left', offset: 0.3}, {side: 'right', offset: 0.9}, {side: 'top', offset: 0.1}, {side: 'bottom', offset: 0.5}]) {
      const at = positionOnBorder(bounds, shape, placement);
      const back = placementOnBorder(bounds, shape, at);
      assert.equal(back.side, placement.side, `${shape} ${placement.side}`);
      assert.ok(near(back.offset, placement.offset), `${shape} ${placement.side} ${back.offset}`);
    }
  }
});
