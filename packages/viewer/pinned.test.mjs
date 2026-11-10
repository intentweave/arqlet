// Pinned leaves are marked in the SVG viewer; without a pinned set the markup is unchanged.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {renderSvg} from './svg.js';

class Element {
  constructor(tag) { this.tag = tag; this.children = []; this.attrs = {}; this.textContent = ''; }
  append(...items) { this.children.push(...items); }
  replaceChildren() { this.children = []; }
  setAttribute(k, v) { this.attrs[k] = v; }
}
const document = {createElementNS: (_, tag) => new Element(tag)};

function scene() {
  return {
    schemaVersion: 1,
    bounds: {x: 0, y: 0, width: 400, height: 200},
    nodes: [
      {id: 'a', label: 'a', bounds: {x: 20, y: 20, width: 120, height: 54}},
      {id: 'b', label: 'b', bounds: {x: 220, y: 20, width: 120, height: 54}},
    ],
    routes: [],
    ports: [],
    crossings: [],
    labels: [],
    showPorts: false,
  };
}

function flatten(node, out = []) {
  if (node instanceof Element) {
    out.push(node);
    node.children.forEach(child => flatten(child, out));
  }
  return out;
}

test('a pinned leaf carries the pin marker', () => {
  const svg = new Element('svg');
  renderSvg(svg, scene(), {document, pinned: new Set(['a'])});
  const marked = flatten(svg).filter(e => e.attrs['data-pinned-id'] !== undefined);
  assert.deepEqual(marked.map(e => e.attrs['data-pinned-id']), ['a']);
  const dot = flatten(svg).find(e => e.tag === 'circle' && e.attrs.fill === '#edc477');
  assert.ok(dot, 'the pin dot is drawn');
});

test('without a pinned set no node is marked', () => {
  const svg = new Element('svg');
  renderSvg(svg, scene(), {document});
  assert.equal(flatten(svg).some(e => e.attrs['data-pinned-id'] !== undefined), false);
});
