// Adapter invariants on engine scenes. Geometry itself is tested in Rust (crates/scene); these tests
// check that the React Flow mapping and the SVG viewer consume scenes faithfully.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {checkScene, routePath} from './scene.js';
import {toReactFlow} from './react-flow.js';
import {renderSvg} from './svg.js';

const scene = name => checkScene(JSON.parse(readFileSync(new URL(`fixtures/${name}.scene.json`, import.meta.url), 'utf8')));
class Element {
  constructor(tag) { this.tag = tag; this.children = []; this.attrs = {}; }
  append(...items) { this.children.push(...items); }
  replaceChildren() { this.children = []; }
  setAttribute(k, v) { this.attrs[k] = v; }
}
const document = {createElementNS: (_, tag) => new Element(tag)};

test('React Flow positions are parent-relative and parents precede children', () => {
  const s = scene('bpmn-pools');
  const mapped = toReactFlow(s);
  const byId = new Map(mapped.nodes.map((n, i) => [n.id, i]));
  for (const node of s.nodes.filter(n => n.parentId)) {
    assert.ok(byId.get(node.parentId) < byId.get(node.id), `${node.parentId} must precede ${node.id}`);
    const parent = s.nodes.find(n => n.id === node.parentId).bounds;
    const mappedNode = mapped.nodes[byId.get(node.id)];
    assert.equal(mappedNode.position.x, node.bounds.x - parent.x);
    assert.equal(mappedNode.position.y, node.bounds.y - parent.y);
    assert.equal(mappedNode.parentId, node.parentId);
  }
  assert.deepEqual(mapped.labels, s.labels);
});

test('React Flow edges carry the engine route points exactly and expose endpoint handles', () => {
  const s = scene('state-tcp');
  const mapped = toReactFlow(s);
  assert.equal(mapped.edges.length, s.routes.filter(r => r.points.length >= 2).length);
  for (const edge of mapped.edges) {
    const route = s.routes.find(r => r.id === edge.id);
    assert.deepEqual(edge.data.points, route.points);
    assert.equal(routePath(edge.data.points).split(' ')[0], 'M');
  }
  const source = mapped.nodes.find(n => n.id === s.routes[0].source);
  assert.ok(source.data.handles.some(h => h.id === `source:${s.routes[0].id}`));
});

test('hidden markers keep React Flow endpoint handles; SVG marker IDs are isolated per viewer', () => {
  const s = scene('state-tcp');
  const hidden = {...s, showPorts: false};
  const a = new Element('svg'), b = new Element('svg');
  renderSvg(a, hidden, {document});
  renderSvg(b, s, {document});
  assert.equal(a.children.filter(e => e.tag === 'circle').length, 0);
  assert.notEqual(
    a.children.find(e => e.tag === 'polyline').attrs['marker-end'],
    b.children.find(e => e.tag === 'polyline').attrs['marker-end']);
  assert.ok(toReactFlow(hidden).nodes.every(n => n.data.handles.length >= 0));
  assert.ok(toReactFlow(hidden).edges.length > 0);
});

test('a conflicted route is highlighted in both viewers', () => {
  const s = scene('state-tcp');
  const flagged = {...s, routes: s.routes.map((r, i) => i === 0 ? {...r, invalid: true} : r)};
  const svg = new Element('svg');
  renderSvg(svg, flagged, {document});
  const lines = svg.children.filter(e => e.tag === 'polyline');
  assert.equal(lines[0].attrs.stroke, '#f08f80');
  assert.equal(toReactFlow(flagged).edges[0].data.invalid, true);
});

test('unsupported scene schemas fail explicitly', () => {
  assert.throws(() => checkScene({schemaVersion: 2}), /Unsupported scene schema/);
});

