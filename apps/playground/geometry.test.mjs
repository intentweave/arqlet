import { test } from 'node:test';
import assert from 'node:assert/strict';
import { GeometryReports, contourOf, labelSize } from './geometry.js';

test('a circle is reported by its diameter, anything else as a rectangle', () => {
  assert.deepEqual(contourOf('circle', { width: 150, height: 120 }), { kind: 'circle', diameter: 150 });
  assert.deepEqual(contourOf('rect', { width: 180, height: 54 }), { kind: 'rect', width: 180, height: 54 });
  assert.deepEqual(contourOf('square', { width: 90, height: 90 }), { kind: 'rect', width: 90, height: 90 });
  assert.deepEqual(contourOf(undefined, { width: 10, height: 20 }), { kind: 'rect', width: 10, height: 20 });
});

test('each report of an element has a higher revision than the one before; elements count separately', () => {
  const reports = new GeometryReports();
  const size = { width: 100, height: 40 };
  assert.equal(reports.report('a', 'rect', size).geometryRevision, 1);
  assert.equal(reports.report('a', 'rect', size).geometryRevision, 2);
  assert.equal(reports.report('b', 'rect', size).geometryRevision, 1);
  assert.equal(reports.report('a', 'circle', size).geometryRevision, 3);
});

test('several sizes are reported with their own shapes and revisions', () => {
  const reports = new GeometryReports();
  const shapes = { a: 'rect', b: 'circle' };
  const result = reports.reports(
    { a: { width: 200, height: 60 }, b: { width: 120, height: 120 } },
    (id) => shapes[id],
  );
  assert.deepEqual(result, {
    a: { contour: { kind: 'rect', width: 200, height: 60 }, geometryRevision: 1 },
    b: { contour: { kind: 'circle', diameter: 120 }, geometryRevision: 1 },
  });
});

test('clearing starts the numbering again', () => {
  const reports = new GeometryReports();
  reports.report('a', 'rect', { width: 1, height: 1 });
  reports.clear();
  assert.equal(reports.report('a', 'rect', { width: 1, height: 1 }).geometryRevision, 1);
});

test('a circle is sized to hold its label, a rectangle keeps the 180 x 54 minimum', () => {
  const small = labelSize('circle', 40);
  assert.equal(small.width, small.height, 'a circle box is square');
  assert.equal(small.width, 61, 'the diameter is the diagonal of the label box');
  assert.equal(labelSize('circle', 0).width, 56, 'a circle is never smaller than 56');
  const wide = labelSize('circle', 120);
  assert.ok(wide.width < 180, 'a short label does not make a 180 wide circle');
  assert.ok(wide.width >= Math.hypot(120, 22), 'the label box fits the disc');
  assert.deepEqual(labelSize('rect', 40), { width: 180, height: 54 });
  assert.deepEqual(labelSize('rect', 200), { width: 228, height: 54 });
});
