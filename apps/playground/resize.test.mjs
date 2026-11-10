import { test } from 'node:test';
import assert from 'node:assert/strict';
import { MIN_LEAF, RESIZE_CORNER, nearResizeCorner, resizedLeaf } from './resize.js';

const box = { x: 100, y: 50, width: 180, height: 54 };

test('a press in the bottom-right corner area starts a resize; the middle of the box does not', () => {
  assert.equal(nearResizeCorner({ x: box.x + box.width, y: box.y + box.height }, box), true);
  assert.equal(nearResizeCorner({ x: box.x + box.width - RESIZE_CORNER, y: box.y + box.height - 1 }, box), true);
  assert.equal(nearResizeCorner({ x: box.x + 20, y: box.y + 20 }, box), false);
  assert.equal(nearResizeCorner({ x: box.x + box.width + RESIZE_CORNER + 1, y: box.y + box.height }, box), false);
});

test('the size follows the pointer by its travel, in whole units', () => {
  const start = { x: box.x + box.width, y: box.y + box.height };
  assert.deepEqual(resizedLeaf(box, start, { x: start.x + 40.4, y: start.y + 20.6 }), { width: 220, height: 75 });
});

test('a leaf never shrinks below the minimum', () => {
  const start = { x: box.x + box.width, y: box.y + box.height };
  assert.deepEqual(resizedLeaf(box, start, { x: start.x - 1000, y: start.y - 1000 }), MIN_LEAF);
});
