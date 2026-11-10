// Every party of a conflict is marked in the drawing, whatever the kind, with the reasons as its hint.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {renderSvg} from './svg.js';

class Element {
  constructor(tag) { this.tag = tag; this.children = []; this.attrs = {}; this.textContent = ''; }
  append(...items) { this.children.push(...items); }
  replaceChildren() { this.children = []; }
  setAttribute(k, v) { this.attrs[k] = v; }
}
const document = {createElementNS: (_, tag) => new Element(tag)};
const scene = () => JSON.parse(readFileSync(new URL('fixtures/state-tcp.scene.json', import.meta.url), 'utf8'));
const all = (el, out = []) => { out.push(el); el.children.forEach(c => all(c, out)); return out; };
const nodeGroup = (svg, id) => svg.children.find(e => e.tag === 'g' && e.attrs['data-node-id'] === id);
const shape = g => g.children.find(c => c.attrs.stroke);
const RED = '#f08f80';

test('the nodes and the route of a conflict are marked red and carry the reasons', () => {
  const s = scene();
  const [a, b] = s.nodes.filter(n => !n.container);
  const route = s.routes[0];
  s.conflicts = [
    {kind: 'node_overlap', nodes: [a.id, b.id], routes: [], reason: 'Unrelated node rectangles overlap'},
    {kind: 'route-through-node', nodes: [b.id], routes: [route.id], reason: `Edge ${route.id} passes through node ${b.id}`},
  ];
  const svg = new Element('svg');
  renderSvg(svg, s, {document});
  for (const n of [a, b]) assert.equal(shape(nodeGroup(svg, n.id)).attrs.stroke, RED, n.id);
  const hint = id => nodeGroup(svg, id).children.find(c => c.tag === 'title')?.textContent;
  assert.equal(hint(a.id), 'Unrelated node rectangles overlap');
  assert.equal(hint(b.id).split('\n').length, 2, 'a node in two conflicts shows both reasons');
  const line = all(svg).find(e => e.tag === 'polyline' && e.attrs['data-route-id'] === route.id && e.attrs['pointer-events'] === 'none');
  assert.equal(line.attrs.stroke, RED);
});

test('a drawing without conflicts is drawn as before', () => {
  const svg = new Element('svg');
  renderSvg(svg, scene(), {document});
  assert.ok(all(svg).filter(e => e.tag === 'title').every(t => !/overlap|passes through/.test(t.textContent)));
  const other = nodeGroup(svg, scene().nodes.find(n => !n.container).id);
  assert.notEqual(shape(other).attrs.stroke, RED);
});
