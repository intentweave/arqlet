// The renderer draws the engine's outline: a circle is the disc inscribed in its box, a rectangle is its box.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {renderSvg} from './svg.js';

class Element {
  constructor(tag) { this.tag = tag; this.children = []; this.attrs = {}; }
  append(...items) { this.children.push(...items); }
  replaceChildren() { this.children = []; }
  setAttribute(k, v) { this.attrs[k] = v; }
  find(tag) { return this.children.flatMap(c => [...(c.tag === tag ? [c] : []), ...c.find(tag)]); }
}
const document = {createElementNS: (_, tag) => new Element(tag)};
const node = (id, shape, bounds) => ({id, label: id, kind: 'node', parentId: null, shape, bounds, container: false, aggregate: false, collapsed: false, invalid: false});

test('a circle node is drawn as its inscribed disc, a rectangle node as its box', () => {
  const scene = {
    schemaVersion: 1,
    bounds: {x: 0, y: 0, width: 300, height: 120},
    nodes: [node('round', 'circle', {x: 10, y: 10, width: 100, height: 60}), node('box', undefined, {x: 150, y: 10, width: 100, height: 60})],
    routes: [], ports: [], crossings: [], labels: [], showPorts: false,
  };
  const svg = new Element('svg');
  renderSvg(svg, scene, {document});
  const circles = svg.find('circle').filter(c => c.attrs['cx'] !== undefined);
  assert.equal(circles.length, 1);
  assert.equal(circles[0].attrs.cx, 60);
  assert.equal(circles[0].attrs.cy, 40);
  assert.equal(circles[0].attrs.r, 30, 'the radius is half the smaller side');
  const rects = svg.find('rect').filter(r => r.attrs.rx === 7);
  assert.equal(rects.length, 1);
  assert.equal(rects[0].attrs.x, 150);
});

test('a circle label is centred in the disc and its pin sits on the disc, not in the box corner', () => {
  const scene = {
    schemaVersion: 1,
    bounds: {x: 0, y: 0, width: 200, height: 160},
    nodes: [node('round', 'circle', {x: 10, y: 10, width: 100, height: 100}), node('box', undefined, {x: 120, y: 10, width: 70, height: 50})],
    routes: [], ports: [], crossings: [], labels: [], showPorts: false,
  };
  const svg = new Element('svg');
  renderSvg(svg, scene, {document, pinned: new Set(['round'])});
  const texts = svg.find('text');
  const round = texts.find(t => t.attrs['text-anchor'] === 'middle');
  assert.ok(round, 'the circle label is middle-anchored');
  assert.equal(round.attrs.x, 60);
  assert.equal(round.attrs.y, 65);
  const plain = texts.find(t => t.attrs['text-anchor'] === undefined);
  assert.equal(plain.attrs.x, 130, 'a rectangle label keeps its left alignment');
  const pin = svg.find('circle').find(c => c.attrs.r === 4);
  assert.ok(Math.hypot(pin.attrs.cx - 60, pin.attrs.cy - 60) < 50, 'the pin marker lies inside the disc');
});

test('a circle container is drawn as a disc with a ring handle, its title centred at the top, and its controls on the disc', () => {
  const container = {...node('ring', 'circle', {x: 0, y: 0, width: 300, height: 300}), container: true};
  const child = {...node('inner', undefined, {x: 100, y: 100, width: 100, height: 50}), parentId: 'ring'};
  const scene = {
    schemaVersion: 1,
    bounds: {x: 0, y: 0, width: 300, height: 300},
    nodes: [container, child],
    routes: [], ports: [], crossings: [], labels: [], showPorts: false,
  };
  const svg = new Element('svg');
  renderSvg(svg, scene, {document, onNodeDragStart: () => {}, onPin: () => {}, onResize: () => {}});
  const frame = svg.find('circle').find(c => c.attrs['data-container-id'] === 'ring' && c.attrs.fill === '#142133');
  assert.ok(frame, 'the frame is a circle');
  assert.deepEqual([frame.attrs.cx, frame.attrs.cy, frame.attrs.r], [150, 150, 150]);
  assert.equal(svg.find('rect').filter(r => r.attrs['data-container-id'] === 'ring').length, 0, 'no rectangle frame');
  const ring = svg.find('circle').find(c => c.attrs.class === 'lw-header');
  assert.ok(ring, 'the move handle is a ring inside the border');
  assert.equal(ring.attrs['pointer-events'], 'stroke');
  const title = svg.find('text').find(t => t.attrs['data-container-id'] === 'ring');
  assert.equal(title.attrs['text-anchor'], 'middle');
  assert.equal(title.attrs.x, 150);
  assert.ok(title.attrs.y < 150, 'the title is in the upper half');
  const handle = svg.find('rect').find(r => r.attrs['data-resize-id'] === 'ring');
  assert.ok(Math.hypot(handle.attrs.x + 6 - 150, handle.attrs.y + 6 - 150) < 150, 'the resize handle lies inside the disc');
});

test('a press on the pin starts a drag that fixes the position; a press on the node starts a plain move', () => {
  const scene = {
    schemaVersion: 1,
    bounds: {x: 0, y: 0, width: 200, height: 120},
    nodes: [node('a', undefined, {x: 20, y: 20, width: 120, height: 54})],
    routes: [], ports: [], crossings: [], labels: [], showPorts: false,
  };
  const starts = [];
  const svg = new Element('svg');
  renderSvg(svg, scene, {document, onNodeDragStart: (n, e, opts) => starts.push({id: n.id, opts}), onPin: () => {}});
  const control = svg.children.flatMap(function walk(c) { return [c, ...c.children.flatMap(walk)]; }).find(c => c.attrs['data-pin-id'] === 'a');
  assert.ok(control, 'the node has a pin control');
  let stopped = false;
  control.onpointerdown({stopPropagation: () => { stopped = true; }});
  assert.ok(stopped, 'the press does not also start a pan or a plain move');
  assert.deepEqual(starts, [{id: 'a', opts: {fromPin: true}}]);
  const body = svg.children.find(c => c.tag === 'g' && c.attrs['data-node-id'] === 'a');
  body.onpointerdown({});
  assert.equal(starts.length, 2);
  assert.equal(starts[1].opts, undefined, 'a press on the node itself is a plain move');
});
