// ADR 0019, step 4: the SVG viewer draws the columns of a scene as it draws the scene. A scene goes through the engine
// client's `columnsOfScene` (what the engine's own columns equal, tested there) and back through `sceneOfColumns`; the scene
// that comes back, and the markup drawn from the columns, equal the scene's and its markup with `f32` coordinates.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {renderSvg} from './svg.js';
import {isSceneColumns, sceneOfColumns} from './scene-columns.js';
import {columnsOfScene} from '../engine-client/columns.js';

class Element {
  constructor(tag) { this.tag = tag; this.children = []; this.attrs = {}; this.textContent = ''; }
  append(...items) { this.children.push(...items); }
  replaceChildren() { this.children = []; }
  setAttribute(k, v) { this.attrs[k] = v; }
}
const document = {createElementNS: (_, tag) => new Element(tag)};
const escape = s => String(s).replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
const normalize = s => s.replace(/lw-arrow-\d+/g, 'lw-arrow-N');
function markup(node) {
  if (!(node instanceof Element)) return '';
  const attrs = Object.entries(node.attrs).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => ` ${k}="${escape(v)}"`).join('');
  return `<${node.tag}${attrs}>${escape(node.textContent)}${node.children.map(markup).join('')}</${node.tag}>`;
}
const draw = (scene, options = {}) => { const svg = new Element('svg'); renderSvg(svg, scene, {document, ...options}); return normalize(markup(svg)); };

// The scene as the columns hold it: coordinates narrowed to f32, except the extent (kept in full precision).
const narrow = value => {
  if (typeof value === 'number') return Math.fround(value);
  if (Array.isArray(value)) return value.map(narrow);
  if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([k, v]) => [k, narrow(v)]));
  return value;
};
const narrowed = scene => ({...narrow(scene), bounds: scene.bounds});

const load = name => JSON.parse(readFileSync(new URL(`fixtures/${name}.scene.json`, import.meta.url), 'utf8'));
// One scene with the parts the fixtures lack: a bundle, a two-way route, rows, a conflict, a boundary crossing, ports and labels.
const rich = {
  schemaVersion: 1, revision: 3, layoutVersion: 4, showPorts: true, bounds: {x: -30, y: -30, width: 260.5, height: 160.25},
  nodes: [
    {id: 'a', kind: 'group', label: 'Group', parentId: null, bounds: {x: 0, y: 0, width: 200.3, height: 100}, container: true, aggregate: false, collapsed: false, invalid: false},
    {id: 'b', kind: 'entity', label: 'Order', parentId: 'a', bounds: {x: 10.1, y: 20.7, width: 60.3, height: 60}, container: false, aggregate: false, collapsed: false, invalid: true,
      rows: [{id: 'r0', label: 'id', bounds: {x: 11, y: 21, width: 58, height: 20}}, {id: 'r1', label: 'placed_at', bounds: {x: 11, y: 43, width: 58, height: 20}}]},
    {id: 'c', kind: 'service', label: 'Service', parentId: 'a', shape: 'circle', bounds: {x: 100, y: 20, width: 40, height: 40}, container: false, aggregate: true, collapsed: true, invalid: false},
    {id: 'rest:a', kind: 'rest', label: '+3', parentId: 'a', bounds: {x: 150, y: 20, width: 30, height: 30}, container: false, aggregate: true, collapsed: true, invalid: false},
  ],
  routes: [
    {id: 'e0', kind: 'calls', source: 'b', target: 'c', weight: 3, members: ['x1', 'x2', 'x3'], invalid: false, points: [{x: 70.4, y: 50.1}, {x: 85.2, y: 50.1}, {x: 100, y: 40.9}]},
    {id: 'e1', source: 'c', target: 'rest:a', weight: 2, backWeight: 1, invalid: false, rounded: true, points: [{x: 140, y: 40}, {x: 150, y: 35}]},
    {id: 'e2', source: 'b', target: 'c', weight: 1, invalid: true, points: [{x: 70.4, y: 60}, {x: 100, y: 55}]},
  ],
  ports: [
    {id: 'p0', nodeId: 'b', edgeId: 'e0', endpoint: 'source', side: 'right', position: {x: 70.4, y: 50.1}, explicit: true},
    {id: 'p1', nodeId: 'c', edgeId: 'e0', endpoint: 'target', side: 'left', position: {x: 100, y: 40.9}, explicit: false},
  ],
  labels: [{id: 'e0', kind: 'edge', text: 'calls', bounds: {x: 72, y: 36, width: 28, height: 12}}],
  crossings: [{edgeId: 'e2', containerId: 'a', ordinal: 0, side: 'left', position: {x: 0, y: 55.5}, derived: false}],
  conflicts: [{kind: 'node_overlap', nodes: ['b', 'c'], routes: ['e2'], reason: 'b and c overlap'}],
  buses: [['e0', 'e2']], junctions: [{x: 85.2, y: 50.1}],
};

for (const [name, scene] of [['state-tcp', load('state-tcp')], ['bpmn-pools', load('bpmn-pools')], ['a scene with every part', rich]]) {
  test(`${name}: the scene comes back from its columns, and the viewer draws the same markup`, () => {
    const columns = columnsOfScene(scene);
    assert.equal(isSceneColumns(columns), true);
    assert.equal(isSceneColumns(scene), false);
    const back = sceneOfColumns(columns);
    assert.deepEqual(back, narrowed(scene));
    assert.equal(draw(columns), draw(narrowed(scene)), 'the same markup');
    // With every control and handler the page uses, the markup is still the same (they are drawn from the same data).
    const handlers = {onSelect() {}, onSelectRoute() {}, onToggle() {}, onPin() {}, onNodeDragStart() {}, onResize() {}, onPortDragStart() {}, onLabelDragStart() {}, onReset() {}, pinned: new Set(['b']), sized: new Map([['a', ['width']]]), selected: 'b', selectedRoute: 'e0'};
    assert.equal(draw(columns, handlers), draw(narrowed(scene), handlers));
  });
}

test('a conflict leaves out a party outside the scene, and a bundle keeps its members', () => {
  const scene = {...rich, conflicts: [{kind: 'route-through-node', nodes: ['c', 'missing'], routes: ['e2', 'unknown'], reason: 'e2 runs through c'}]};
  const back = sceneOfColumns(columnsOfScene(scene));
  assert.deepEqual(back.conflicts, [{kind: 'route-through-node', nodes: ['c'], routes: ['e2'], reason: 'e2 runs through c'}]);
  assert.deepEqual(back.routes[0].members, ['x1', 'x2', 'x3']);
  assert.equal(back.routes[1].backWeight, 1);
  assert.equal(back.routes[0].backWeight, undefined);
});
