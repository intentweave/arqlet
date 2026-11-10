import {test} from 'node:test';
import assert from 'node:assert/strict';
import {derivePosition, previewRoute} from '../generated/editor/preview.js';

test('a port sits on its side at its offset, as the engine places it', () => {
  const box = {x: 100, y: 50, width: 200, height: 80};
  assert.deepEqual(derivePosition(box, 'top', 0.25), {x: 150, y: 50});
  assert.deepEqual(derivePosition(box, 'bottom', 0.5), {x: 200, y: 130});
  assert.deepEqual(derivePosition(box, 'left', 0.5), {x: 100, y: 90});
  assert.deepEqual(derivePosition(box, 'right', 1), {x: 300, y: 130});
});

test('the preview shape is orthogonal and keeps both ends', () => {
  const a = {x: 0, y: 0};
  const b = {x: 200, y: 100};
  const z = previewRoute(a, 'right', b, 'left');
  assert.deepEqual(z[0], a);
  assert.deepEqual(z.at(-1), b);
  for (let i = 0; i + 1 < z.length; i++) {
    assert.ok(z[i].x === z[i + 1].x || z[i].y === z[i + 1].y, 'every segment is axis-aligned');
  }
  assert.equal(z.length, 4, 'a Z between two horizontal sides');
  const l = previewRoute(a, 'bottom', b, 'left');
  assert.equal(l.length, 3, 'an L when the sides differ in axis');
  assert.deepEqual(l[1], {x: 0, y: 100}, 'the L leaves the vertical side first');
});
