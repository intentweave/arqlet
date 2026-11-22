import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {sceneBuffers, sceneIndex, sceneExcerpt, nearPolicy, medianBoxPixels, triangulate, arrowCorners, viewRect, BOX_STRIDE, SEGMENT_STRIDE, TRIANGLE_STRIDE, LAYERS} from './scene-buffers.js';

const fixture = name => JSON.parse(readFileSync(new URL(`./fixtures/${name}.scene.json`, import.meta.url)));

const box = (id, x, y, w, h, extra = {}) => ({id, kind: 'leaf', label: id, parentId: null, bounds: {x, y, width: w, height: h}, container: false, ...extra});
const tiny = () => ({
  schemaVersion: 1, revision: 1, layoutVersion: 1, bounds: {x: 0, y: 0, width: 300, height: 200}, showPorts: false,
  nodes: [box('g', 0, 0, 300, 200, {container: true, kind: 'group'}), box('a', 20, 40, 60, 40, {parentId: 'g'}), box('b', 200, 40, 60, 40, {parentId: 'g'})],
  routes: [{id: 'a->b', source: 'a', target: 'b', points: [{x: 80, y: 60}, {x: 140, y: 60}, {x: 140, y: 70}, {x: 200, y: 70}], weight: 1}],
  ports: [], labels: [{id: 'l', kind: 'edge', text: 'go', bounds: {x: 120, y: 10, width: 30, height: 14}}], crossings: [],
});

test('every element of a scene becomes instances in the drawing order of renderSvg', () => {
  const b = sceneBuffers(tiny());
  assert.deepEqual(LAYERS, ['hulls', 'containers', 'routes', 'leaves', 'marks']);
  const [hulls, containers, routes, leaves, marks] = b.layers;
  assert.equal(hulls.boxes.length + hulls.segments.length + hulls.triangles.length, 0);
  assert.equal(containers.boxes.length / BOX_STRIDE, 1);
  assert.equal(routes.segments.length / SEGMENT_STRIDE, 3, 'one segment per pair of points');
  assert.equal(routes.triangles.length / TRIANGLE_STRIDE, 3, 'one arrowhead');
  assert.equal(leaves.boxes.length / BOX_STRIDE, 2);
  assert.equal(marks.boxes.length / BOX_STRIDE, 1, 'the edge label box');
  // The container is the scene's box, corner radius 10, stroke 1.
  assert.deepEqual([...containers.boxes.slice(0, 7)], [0, 0, 300, 200, 10, 0, 1]);
  assert.deepEqual(b.texts.map(t => t.text), ['▾ g', 'a', 'b', 'go']);
});

test('the coordinates are the scene\'s: no instance moves anything', () => {
  for (const name of ['state-tcp', 'bpmn-pools']) {
    const scene = fixture(name), b = sceneBuffers(scene);
    const leaves = scene.nodes.filter(n => !n.container && n.kind !== 'attribute' && n.shape !== 'circle');
    const drawn = b.layers[3].boxes;
    for (const n of leaves) {
      const found = [...Array(drawn.length / BOX_STRIDE).keys()].some(i => Math.abs(drawn[i * BOX_STRIDE] - n.bounds.x) < 1e-3 && Math.abs(drawn[i * BOX_STRIDE + 1] - n.bounds.y) < 1e-3 && Math.abs(drawn[i * BOX_STRIDE + 2] - n.bounds.width) < 1e-3);
      assert.ok(found, `${name}: ${n.id}`);
    }
    const segments = b.layers[2].segments.length / SEGMENT_STRIDE;
    assert.equal(segments, scene.routes.reduce((s, r) => s + Math.max(0, r.points.length - 1), 0), name);
  }
});

test('later lines lie nearer, so a line blends once and covers earlier ones', () => {
  const s = tiny();
  s.routes.push({...s.routes[0], id: 'b->a', source: 'b', target: 'a'});
  const seg = sceneBuffers(s).layers[2].segments;
  const depth = i => seg[i * SEGMENT_STRIDE + 5];
  assert.equal(depth(0), depth(2), 'one depth per line');
  assert.ok(depth(3) < depth(0), 'the later line is nearer');
});

test('a selected route is lit and the others dimmed; the nodes off it fade', () => {
  const s = tiny();
  s.nodes.push(box('c', 100, 150, 40, 20, {parentId: 'g'}));
  const b = sceneBuffers(s, {selectedRoute: 'a->b', selected: 'a'});
  const seg = b.layers[2].segments;
  assert.ok(Math.abs(seg[4] - 3.1) < 1e-6, 'wider');
  assert.equal(seg[12], 1, 'opaque');
  const leaves = b.layers[3].boxes, alphaOf = i => leaves[i * BOX_STRIDE + 11];
  assert.equal(alphaOf(0), 1); assert.ok(Math.abs(alphaOf(2) - 0.4) < 1e-6);
});

test('ear clipping covers a concave polygon with n - 2 triangles of the same area', () => {
  const l = [{x: 0, y: 0}, {x: 4, y: 0}, {x: 4, y: 1}, {x: 1, y: 1}, {x: 1, y: 3}, {x: 0, y: 3}];
  const tris = triangulate(l);
  assert.equal(tris.length, 4);
  const area = ([a, b, c]) => Math.abs((l[b].x - l[a].x) * (l[c].y - l[a].y) - (l[c].x - l[a].x) * (l[b].y - l[a].y)) / 2;
  assert.equal(tris.reduce((s, t) => s + area(t), 0), 6);
});

test('the arrowhead is the SVG marker: tip 0.8 past the end, base 7.2 before it, 8 wide', () => {
  const [a, tip, c] = arrowCorners({x: 100, y: 50}, 1, 0);
  assert.deepEqual(tip.map(v => +v.toFixed(6)), [100.8, 50]);
  assert.deepEqual(a.map(v => +v.toFixed(6)), [92.8, 46]);
  assert.deepEqual(c.map(v => +v.toFixed(6)), [92.8, 54]);
});

test('picking follows renderSvg\'s hit order: leaf, then a line within the tolerance, then the deepest container', () => {
  const index = sceneIndex(tiny());
  assert.deepEqual(index.pick(30, 50), {kind: 'node', id: 'a'});
  assert.deepEqual(index.pick(110, 61, 2), {kind: 'route', id: 'a->b'});
  assert.deepEqual(index.pick(110, 120), {kind: 'node', id: 'g'});
  assert.equal(index.pick(400, 400), null);
});

test('the query finds what touches a rectangle, and the excerpt keeps the containers of what it holds', () => {
  const s = tiny(), index = sceneIndex(s);
  const q = index.query({x: 0, y: 30, width: 90, height: 40});
  assert.deepEqual([...q.nodes].sort(), ['a', 'g']);
  assert.deepEqual([...q.routes], ['a->b']);
  const e = sceneExcerpt(s, index, {x: 190, y: 30, width: 20, height: 40});
  assert.deepEqual(e.nodes.map(n => n.id).sort(), ['b', 'g']);
  assert.deepEqual(e.routes.map(r => r.id), ['a->b'], 'a line in view is kept whole');
  assert.deepEqual(e.labels, []);
  assert.deepEqual(e.bounds, {x: 190, y: 30, width: 20, height: 40});
});

test('the switch: near below the budget with large boxes, back to far only past the hysteresis', () => {
  const p = (elements, boxPixels, current) => nearPolicy({elements, boxPixels, current, budget: 100, minPixels: 20});
  assert.equal(p(100, 20, 'far'), 'near');
  assert.equal(p(101, 40, 'far'), 'far');
  assert.equal(p(10, 19, 'far'), 'far');
  assert.equal(p(120, 20, 'near'), 'near', 'within the hysteresis it stays near');
  assert.equal(p(125, 16, 'near'), 'near');
  assert.equal(p(126, 40, 'near'), 'far');
  assert.equal(p(10, 15.9, 'near'), 'far');
});

test('the median box on screen and the view rectangle', () => {
  const index = sceneIndex(tiny());
  assert.equal(medianBoxPixels(index, ['a', 'b', 'g'], 2), 80, 'leaves only, the smaller side');
  assert.equal(medianBoxPixels(index, [], 2), 0);
  assert.deepEqual(viewRect({centreX: 100, centreY: 50, scale: 2}, 400, 200), {x: 0, y: 0, width: 200, height: 100});
});

test('the columns of a scene draw as the scene', async () => {
  const {columnsOfScene} = await import('../engine-client/columns.js').catch(() => ({}));
  if (!columnsOfScene) return;
  const scene = fixture('state-tcp');
  const a = sceneBuffers(scene), b = sceneBuffers(columnsOfScene(scene));
  for (let i = 0; i < a.layers.length; i++) for (const k of ['boxes', 'segments', 'triangles']) assert.equal(a.layers[i][k].length, b.layers[i][k].length, `${LAYERS[i]} ${k}`);
});
