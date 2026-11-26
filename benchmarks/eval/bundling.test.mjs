// node --test benchmarks/eval/bundling.test.mjs
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {viewOf, structureOf, eligibility, controlPolygon, straighten, bspline, parallelOffsets, enters, cut, freePlace, gate, bundleEdge, inkCells, lengthOf} from './bundling.mjs';

const near = (a, b, tolerance = 1e-9) => assert.ok(Math.abs(a - b) <= tolerance, `${a} != ${b}`);
const same = (p, q, tolerance) => { near(p.x, q.x, tolerance); near(p.y, q.y, tolerance); };
const node = (id, x, y, width, height, parentId, shape) => ({id, bounds: {x, y, width, height}, ...(parentId ? {parentId} : {}), ...(shape ? {shape} : {})});
const route = (id, source, target, points) => ({id, source, target, points});

// Two containers side by side, each with two leaves stacked, so that the straight edges between them are free.
function fixture(extra = [], routes) {
  const nodes = [node('A', 0, 0, 200, 400), node('B', 600, 0, 200, 400),
    node('a1', 20, 60, 100, 40, 'A'), node('a2', 20, 320, 100, 40, 'A'),
    node('b1', 680, 60, 100, 40, 'B'), node('b2', 680, 320, 100, 40, 'B'), ...extra];
  const list = routes ?? [route('e1', 'a1', 'b1', [{x: 120, y: 80}, {x: 680, y: 80}]), route('e2', 'a2', 'b2', [{x: 120, y: 340}, {x: 680, y: 340}])];
  const scene = {nodes, routes: list};
  const snapshot = {straightEdges: list.map(r => r.id), graph: {edgePorts: []}, options: {}};
  return viewOf(scene, snapshot);
}

test('straightening: strength 1 keeps the polygon, 0 puts every point on the chord', () => {
  const polygon = [{x: 0, y: 0}, {x: 10, y: 50}, {x: 90, y: 50}, {x: 90, y: 0}];
  assert.deepEqual(straighten(polygon, 1), polygon);
  straighten(polygon, 0).forEach((p, i) => same(p, {x: 90 * i / 3, y: 0}));
});

test('the spline starts and ends at the end points; four points give the Bezier curve', () => {
  const polygon = [{x: 0, y: 0}, {x: 0, y: 80}, {x: 80, y: 80}, {x: 80, y: 0}];
  const curve = bspline(polygon, 8);
  assert.equal(curve.length, 9);
  same(curve[0], polygon[0]);
  same(curve[8], polygon[3]);
  same(curve[4], {x: 40, y: 60});
  for (const n of [2, 3, 5, 7]) {
    const points = Array.from({length: n}, (_, i) => ({x: i * 10, y: (i % 2) * 10}));
    const out = bspline(points, 16);
    same(out[0], points[0]);
    same(out[16], points[n - 1]);
  }
});

test('the curve of the reversed polygon is the reversed curve', () => {
  const polygon = [{x: 0, y: 0}, {x: 10, y: 50}, {x: 40, y: -20}, {x: 90, y: 50}, {x: 120, y: 0}];
  const forward = bspline(polygon, 20), back = bspline([...polygon].reverse(), 20).reverse();
  forward.forEach((p, i) => same(p, back[i], 1e-9));
});

test('eligibility counts each excluded edge by its reason', () => {
  const view = fixture([node('c1', 20, 150, 100, 40, 'A')], [
    route('ok', 'a1', 'b1', [{x: 120, y: 80}, {x: 680, y: 80}]),
    route('loop', 'a1', 'a1', [{x: 0, y: 0}, {x: 1, y: 1}]),
    route('up', 'a1', 'A', [{x: 0, y: 0}, {x: 1, y: 1}]),
    route('sib', 'a1', 'a2', [{x: 70, y: 100}, {x: 70, y: 320}]),
    route('det', 'a2', 'b2', [{x: 0, y: 0}, {x: 1, y: 1}, {x: 2, y: 0}]),
    route('orth', 'c1', 'b2', [{x: 0, y: 0}, {x: 1, y: 1}]),
  ]);
  view.snapshot.straightEdges = ['ok', 'up', 'sib', 'det'];
  const {eligible, excluded} = eligibility(view);
  assert.deepEqual(eligible.map(e => e.id), ['ok']);
  assert.deepEqual(excluded, {selfLoop: 1, containment: 1, bus: 0, notStraight: 1, detoured: 1, explicitPort: 0, labelled: 0, sameContainer: 1});
});

test('the control polygon leaves out the common ancestor and lists the containers in order', () => {
  const view = fixture();
  const [edge] = eligibility(view).eligible;
  assert.deepEqual(controlPolygon(view, edge, 'centre'), [{x: 70, y: 80}, {x: 100, y: 200}, {x: 700, y: 200}, {x: 730, y: 80}]);
  const gates = controlPolygon(view, edge, 'gate');
  same(gates[1], {x: 200, y: 200});
  same(gates[2], {x: 600, y: 200});
});

test('a free place keeps its room from the children and is the centre when that is free', () => {
  const box = {x: 0, y: 0, width: 200, height: 200};
  assert.deepEqual(freePlace(box, []), {x: 100, y: 100});
  const covering = {x: 60, y: 60, width: 80, height: 80};
  const p = freePlace(box, [covering]);
  const distance = Math.hypot(Math.max(60 - p.x, 0, p.x - 140), Math.max(60 - p.y, 0, p.y - 140));
  assert.ok(distance >= 12 - 1e-9 && distance < 16, `${distance}`);
  assert.deepEqual(freePlace(box, [box]), {x: 100, y: 100});
});

test('a gate lies on the border, of a circle too', () => {
  same(gate(node('A', 0, 0, 100, 50), {x: 500, y: 25}), {x: 100, y: 25});
  same(gate(node('C', 0, 0, 100, 100, null, 'circle'), {x: 50, y: 500}), {x: 50, y: 100});
});

test('parallel and antiparallel edges are translated to opposite sides by edge id', () => {
  const view = fixture();
  const offsets = parallelOffsets(view, [{id: 'x', source: 'a1', target: 'b1'}, {id: 'y', source: 'b1', target: 'a1'}]);
  same(offsets.get('x'), {x: 0, y: -3});
  same(offsets.get('y'), {x: 0, y: 3});
  const many = parallelOffsets(view, Array.from({length: 21}, (_, i) => ({id: `m${String(i).padStart(2, '0')}`, source: 'a1', target: 'b1'})));
  near(many.get('m00').y, -19); // half the smaller node's height less one
});

test('the cut ends the curve on both borders and keeps the direction', () => {
  const a = node('a', 0, 0, 20, 20), b = node('b', 100, 0, 20, 20);
  const out = cut([{x: 10, y: 10}, {x: 50, y: 10}, {x: 110, y: 10}], a, b);
  same(out[0], {x: 20, y: 10}, 1e-9);
  same(out.at(-1), {x: 100, y: 10}, 1e-9);
  assert.equal(cut([{x: 10, y: 10}, {x: 12, y: 10}], a, b), null);
});

test('entering is judged as the engine judges it', () => {
  const r = node('r', 0, 0, 10, 10);
  assert.ok(enters({x: -5, y: 5}, {x: 15, y: 5}, r));
  assert.ok(!enters({x: -5, y: 0}, {x: 15, y: 0}, r), 'along the border');
  assert.ok(enters({x: -5, y: -2}, {x: 15, y: -2}, r, 3), 'within the margin');
  assert.ok(!enters({x: -1, y: 1}, {x: 1, y: -1}, node('c', 0, 0, 10, 10, null, 'circle')), 'the empty corner of a circle');
});

test('the guard weakens a curve that would enter a node, and keeps the base route when none is clear', () => {
  const free = fixture();
  const [edge] = eligibility(free).eligible;
  const out = bundleEdge(free, edge, {x: 0, y: 0}, 'centre', 0.85);
  assert.equal(out.level, 'full');
  assert.ok(lengthOf(out.points) > lengthOf(edge.base));
  // A node under the bundle's middle that the straight route misses.
  const blocked = fixture([node('wall', 340, 120, 120, 200)]);
  const kept = bundleEdge(blocked, eligibility(blocked).eligible[0], {x: 0, y: 0}, 'centre', 0.85);
  assert.notEqual(kept.level, 'full');
  assert.equal(kept.refused, 'node');
  if (kept.level === 'kept') assert.deepEqual(kept.points, eligibility(blocked).eligible[0].base);
});

test('edges that share four control points share a section and take less ink', () => {
  const ends = Array.from({length: 10}, (_, i) => [{x: 0, y: i * 40}, {x: 1000, y: i * 40}]);
  const trunk = [250, 400, 600, 750].map(x => ({x, y: 180}));
  const curves = ends.map(([a, b]) => bspline(straighten([a, ...trunk, b], 1), 200));
  assert.ok(inkCells(curves, 2) / inkCells(ends, 2) < 0.9);
});

test('the structure counts shared sections only from four shared containers on', () => {
  const view = fixture();
  const {eligible} = eligibility(view);
  const out = structureOf(view, eligible);
  assert.deepEqual(out.controlPoints, {4: 2});
  assert.equal(out.withCompanion, 2);
  assert.equal(out.withSharedSection, 0);
});

test('ink counts the raster cells passed through, once', () => {
  assert.equal(inkCells([[{x: 0.5, y: 0.5}, {x: 9.5, y: 0.5}]], 1), 10);
  assert.equal(inkCells([[{x: 0.5, y: 0.5}, {x: 9.5, y: 0.5}], [{x: 9.5, y: 0.5}, {x: 0.5, y: 0.5}]], 1), 10);
  assert.equal(inkCells([[{x: -3, y: -3}, {x: 3, y: -3}]], 2), 4);
});
