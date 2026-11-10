import {test} from 'node:test';
import assert from 'node:assert/strict';
import {
  axisOf,
  boundsAround,
  isShiftable,
  movedInside,
  movedWithin,
  normalizedRoute,
  outwardSide,
  withDogleg,
  overlaps,
  routesNearBorder,
  nearestShiftableSegment,
  routesNear,
  routesTouching,
  segmentNear,
  shiftSegment,
  shiftableSegments,
} from '../generated/editor/geometry.js';

const route = [{x: 0, y: 0}, {x: 0, y: 40}, {x: 100, y: 40}, {x: 100, y: 80}, {x: 200, y: 80}];

test('a node stays inside its parent when dragged past the edge', () => {
  const parent = {x: 0, y: 0, width: 200, height: 100};
  const node = {x: 20, y: 20, width: 50, height: 30};
  const moved = movedInside(node, 500, -500, parent);
  assert.deepEqual(moved.rect, {x: 150, y: 0, width: 50, height: 30});
  assert.equal(moved.clamped, true);
});

test('without a parent a node moves freely', () => {
  const moved = movedInside({x: 0, y: 0, width: 10, height: 10}, -7, 3, null);
  assert.deepEqual(moved.rect, {x: -7, y: 3, width: 10, height: 10});
  assert.equal(moved.clamped, false);
});

test('growth extends containers to the right and bottom, ancestors included, and keeps their corners', () => {
  const grandparent = {x: 0, y: 0, width: 100, height: 100};
  const parent = {x: 10, y: 10, width: 80, height: 80};
  const node = {x: 50, y: 50, width: 40, height: 30};
  const moved = movedWithin(node, 60, 60, [parent, grandparent], true);
  assert.deepEqual(moved.rect, {x: 110, y: 110, width: 40, height: 30});
  assert.deepEqual(moved.grown[0], {x: 10, y: 10, width: 152, height: 142});
  assert.deepEqual(moved.grown[1], {x: 0, y: 0, width: 174, height: 164});
});

test('without growth a node stays inside, and the containers are reported unchanged', () => {
  const parent = {x: 0, y: 0, width: 200, height: 100};
  const moved = movedWithin({x: 20, y: 20, width: 50, height: 30}, 500, -500, [parent], false);
  assert.deepEqual(moved.rect, {x: 150, y: 0, width: 50, height: 30});
  assert.deepEqual(moved.grown, [parent]);
  assert.equal(moved.clamped, true);
});

test('a port moves to a side only when the pointer is clearly outside the node on exactly one other side', () => {
  const box = {x: 100, y: 100, width: 200, height: 80};
  assert.equal(outwardSide({x: 340, y: 130}, box, 'top'), 'right', 'past the right edge');
  assert.equal(outwardSide({x: 200, y: 130}, box, 'top'), null, 'inside the node');
  assert.equal(outwardSide({x: 200, y: 95}, box, 'top'), null, 'within the margin');
  assert.equal(outwardSide({x: 340, y: 60}, box, 'top'), null, 'in the corner region neither side qualifies');
  assert.equal(outwardSide({x: 340, y: 130}, box, 'right'), null, 'the current side is not a change');
});

test('a dogleg keeps the endpoints, stays orthogonal and adds one jog', () => {
  const route = [{x: 0, y: 0}, {x: 0, y: 40}, {x: 300, y: 40}, {x: 300, y: 80}, {x: 600, y: 80}];
  const jogged = withDogleg(route, 2, 30);
  assert.deepEqual(jogged[0], route[0]);
  assert.deepEqual(jogged.at(-1), route.at(-1));
  assert.equal(jogged.length, route.length + 4, "one segment becomes five");
  assert.ok(jogged.some(p => p.x === 330), 'the vertical segment gets a jog of depth 30 in x');
  for (let i = 0; i + 1 < jogged.length; i++) assert.ok(axisOf(jogged[i], jogged[i + 1]) !== null, 'every segment is orthogonal');
  assert.throws(() => withDogleg(route, 0, 30), /interior/, 'the segment at the source port is not a dogleg site');
});

test('normalizing drops repeated points and straight interior points, keeping the endpoints', () => {
  const messy = [{x: 0, y: 0}, {x: 0, y: 0}, {x: 0, y: 5}, {x: 0, y: 10}, {x: 5, y: 10}];
  assert.deepEqual(normalizedRoute(messy), [{x: 0, y: 0}, {x: 0, y: 10}, {x: 5, y: 10}]);
});

test('the view grows to include what the curator placed, and never shrinks below the baseline', () => {
  const base = {x: 0, y: 0, width: 400, height: 300};
  const grown = boundsAround(base, [{x: 300, y: 100, width: 300, height: 200}], [{x: -50, y: 10}], 24);
  assert.equal(grown.x, -50, 'a route point left of the baseline');
  assert.equal(grown.x + grown.width, 300 + 300 + 24, 'the box plus its margin on the right');
  assert.equal(grown.y, 0, 'the baseline top is kept');
  assert.deepEqual(boundsAround(base, [], [], 24), base);
});

test('overlap means shared interior area; touching edges do not overlap', () => {
  const box = {x: 0, y: 0, width: 100, height: 100};
  assert.equal(overlaps(box, {x: 50, y: 50, width: 100, height: 100}), true);
  assert.equal(overlaps(box, {x: 100, y: 0, width: 50, height: 50}), false, 'touching edges do not overlap');
  assert.equal(overlaps(box, {x: 300, y: 300, width: 10, height: 10}), false);
});

test('a route along a container border is near it; a route inside the container is not', () => {
  const box = {x: 0, y: 0, width: 100, height: 100};
  const routes = {
    along: [{x: 50, y: 50}, {x: 50, y: 300}],
    inside: [{x: 20, y: 20}, {x: 80, y: 80}],
    far: [{x: 300, y: 300}, {x: 400, y: 300}],
  };
  assert.deepEqual(routesNearBorder(routes, box, 12), ['along']);
});

test('only interior orthogonal segments are shiftable', () => {
  assert.equal(isShiftable(route, 0), false, 'the first segment carries the source endpoint');
  assert.equal(isShiftable(route, route.length - 2), false, 'the last segment carries the target endpoint');
  assert.equal(isShiftable(route, 1), true);
  assert.equal(isShiftable(route, 2), true);
  assert.deepEqual(
    shiftableSegments(route).map(segment => [segment.index, segment.axis]),
    [[1, 'horizontal'], [2, 'vertical']],
  );
});

test('shifting a segment moves both corners and keeps every segment orthogonal', () => {
  const shifted = shiftSegment(route, 1, 15);
  assert.deepEqual(shifted[1], {x: 0, y: 55});
  assert.deepEqual(shifted[2], {x: 100, y: 55});
  assert.deepEqual(shifted[0], route[0], 'endpoints stay attached');
  assert.deepEqual(shifted[4], route[4], 'endpoints stay attached');
  for (let index = 0; index + 1 < shifted.length; index++) {
    assert.notEqual(axisOf(shifted[index], shifted[index + 1]), null);
  }
  assert.deepEqual(route[1], {x: 0, y: 40}, 'the input is not mutated');
});

test('an endpoint segment cannot be shifted', () => {
  assert.throws(() => shiftSegment(route, 0, 5), /interior orthogonal segment/);
});

test('the segment nearest to the pointer is chosen among shiftable ones', () => {
  assert.deepEqual(nearestShiftableSegment(route, {x: 50, y: 42}), {index: 1, axis: 'horizontal'});
  assert.deepEqual(nearestShiftableSegment(route, {x: 98, y: 60}), {index: 2, axis: 'vertical'});
});

test('routes touching a moved node are found by endpoint', () => {
  const edges = [
    {id: 'a-b', source: 'a', target: 'b'},
    {id: 'c-a', source: 'c', target: 'a'},
    {id: 'c-d', source: 'c', target: 'd'},
  ];
  assert.deepEqual(routesTouching(edges, 'a', ['a-b', 'c-a', 'c-d']), ['a-b', 'c-a']);
  assert.deepEqual(routesTouching(edges, 'a', ['c-d']), [], 'routes without a curated geometry are not listed');
});

test('a segment is near a box when it passes within the margin, and not otherwise', () => {
  const box = {x: 100, y: 100, width: 50, height: 50};
  assert.equal(segmentNear({x: 0, y: 120}, {x: 300, y: 120}, box, 0), true, 'crosses the box');
  assert.equal(segmentNear({x: 0, y: 90}, {x: 300, y: 90}, box, 12), true, 'within the clearance above');
  assert.equal(segmentNear({x: 0, y: 80}, {x: 300, y: 80}, box, 12), false, 'outside the clearance');
  assert.equal(segmentNear({x: 0, y: 0}, {x: 10, y: 0}, box, 12), false, 'far away');
});

test('routes near either the old or the new box are found, and only those', () => {
  const routes = {
    across: [{x: 0, y: 120}, {x: 300, y: 120}],
    away: [{x: 0, y: 0}, {x: 0, y: 10}],
  };
  assert.deepEqual(routesNear(routes, [{x: 100, y: 100, width: 50, height: 50}], 12), ['across']);
  assert.deepEqual(routesNear(routes, [{x: 500, y: 100, width: 50, height: 50}], 12), []);
});
