import test from 'node:test';
import assert from 'node:assert/strict';
import {bendsOf, pathLength, crosses, routeDetails} from './routes.js';

const p = (x, y) => ({x, y});
const scene = {routes: [
  {id: 'h', source: 'a', target: 'b', points: [p(0, 10), p(100, 10)]},
  {id: 'v', source: 'c', target: 'd', points: [p(50, 0), p(50, 20)]},
  {id: 'touch', source: 'e', target: 'f', points: [p(0, 30), p(50, 30), p(50, 10)]},
]};

test('a straight route has no bends and its length is the segment length', () => {
  assert.equal(bendsOf(scene.routes[0].points), 0);
  assert.equal(pathLength(scene.routes[0].points), 100);
});

test('repeated points and collinear points are not bends', () => {
  assert.equal(bendsOf([p(0, 0), p(0, 0), p(10, 0), p(20, 0), p(20, 10)]), 1);
});

test('crossings are interior crossings; a route that only touches another is not counted', () => {
  assert.equal(crosses(scene.routes[0].points, scene.routes[1].points), true);
  assert.equal(crosses(scene.routes[0].points, scene.routes[2].points), false, 'a touching end is not a crossing');
});

test('details name the crossing partners and keep the logical direction', () => {
  const details = routeDetails(scene, 'h', {h: 'calls'});
  assert.equal(details.source, 'a');
  assert.equal(details.target, 'b');
  assert.equal(details.kind, 'calls');
  assert.deepEqual(details.crossings, ['v']);
  assert.equal(routeDetails(scene, 'missing'), null);
});

test('the drawing for the engine uses absolute bounds and the ports that identify each route end', async () => {
  const {drawingOf} = await import('./routes.js');
  const scene = {
    nodes: [{id: 'box', parentId: null, container: true, bounds: {x: 0, y: 0, width: 100, height: 100}}],
    routes: [{id: 'e', source: 'box', target: 'box', points: [p(0, 0), p(10, 10)]}],
    ports: [{id: 'out', edgeId: 'e', endpoint: 'source'}],
  };
  const drawing = drawingOf(scene);
  assert.deepEqual(drawing.nodes[0], {id: 'box', parentId: null, container: true, x: 0, y: 0, width: 100, height: 100});
  assert.equal(drawing.edges[0].sourcePort, 'out');
  assert.equal(drawing.edges[0].targetPort, null);
});
