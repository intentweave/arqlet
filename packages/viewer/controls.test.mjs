// Collapse and pin controls on the drawing: drawn only when the host handles them, and a press on a control never
// reaches the node or container under it.
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
    bounds: {x: 0, y: 0, width: 600, height: 300},
    nodes: [
      {id: 'p', label: 'p', container: true, bounds: {x: 0, y: 0, width: 400, height: 200}},
      {id: 'a', label: 'a', parentId: 'p', bounds: {x: 20, y: 60, width: 160, height: 54}},
      {id: 'c', label: 'c', collapsed: true, bounds: {x: 420, y: 20, width: 160, height: 54}},
    ],
    routes: [],
    ports: [],
    crossings: [],
    labels: [],
    showPorts: false,
  };
}

const flatten = (node, out = []) => {
  if (node instanceof Element) {
    out.push(node);
    node.children.forEach(child => flatten(child, out));
  }
  return out;
};
const event = () => {
  const e = {stopped: false, prevented: false};
  e.stopPropagation = () => { e.stopped = true; };
  e.preventDefault = () => { e.prevented = true; };
  return e;
};

test('without handlers no control is drawn', () => {
  const svg = new Element('svg');
  renderSvg(svg, scene(), {document});
  assert.equal(flatten(svg).some(e => e.attrs.class?.includes('lw-control')), false);
});

test('containers and collapsed nodes get a toggle; every node and container gets a pin control', () => {
  const svg = new Element('svg');
  const toggled = [], pins = [];
  renderSvg(svg, scene(), {document, pinned: new Set(['a']), onToggle: id => toggled.push(id), onPin: (id, pin) => pins.push([id, pin])});
  const all = flatten(svg);
  const toggles = all.filter(e => e.attrs['data-toggle-id'] !== undefined);
  assert.deepEqual(toggles.map(e => e.attrs['data-toggle-id']).sort(), ['c', 'p']);
  assert.equal(toggles.find(e => e.attrs['data-toggle-id'] === 'p').attrs['aria-label'], 'Collapse p');
  assert.equal(toggles.find(e => e.attrs['data-toggle-id'] === 'c').attrs['aria-label'], 'Expand c');
  const pinControls = all.filter(e => e.attrs['data-pin-id'] !== undefined);
  assert.deepEqual(pinControls.map(e => e.attrs['data-pin-id']).sort(), ['a', 'c', 'p']);
  for (const control of [...toggles, ...pinControls]) {
    const down = event(), click = event();
    control.onpointerdown(down);
    control.onclick(click);
    assert.ok(down.stopped && click.stopped, 'a press on a control does not reach the node, the drag or the pan');
  }
  assert.deepEqual(toggled.sort(), ['c', 'p']);
  assert.deepEqual(pins.sort(), [['a', false], ['c', true], ['p', true]], 'a pinned node unpins, a free one pins');
  // The pinned leaf shows the control instead of the plain dot, and is still marked.
  assert.ok(all.some(e => e.attrs['data-pinned-id'] === 'a'));
  assert.ok(pinControls.find(e => e.attrs['data-pin-id'] === 'a').attrs.class.includes('lw-pinned'));
});

test('a control answers Enter and Space from the keyboard', () => {
  const svg = new Element('svg');
  const pins = [];
  renderSvg(svg, scene(), {document, onPin: (id, pin) => pins.push([id, pin])});
  const control = flatten(svg).find(e => e.attrs['data-pin-id'] === 'a');
  control.onkeydown({...event(), key: 'Enter'});
  control.onkeydown({...event(), key: ' '});
  control.onkeydown({...event(), key: 'x'});
  assert.deepEqual(pins, [['a', true], ['a', true]]);
});

test('a container gets one reset control per authored axis, and only then', () => {
  const svg = new Element('svg');
  const resets = [];
  renderSvg(svg, scene(), {document, sized: new Map([['p', ['width', 'height']]]), onReset: (id, axis) => resets.push([id, axis])});
  const controls = flatten(svg).filter(e => e.attrs['data-reset-id'] !== undefined);
  assert.deepEqual(controls.map(e => e.attrs['data-reset-axis']), ['width', 'height'], 'one control per sized axis');
  assert.equal(controls[0].attrs['aria-label'], 'Reset the width of p to content');
  const down = event(), click = event();
  controls[1].onpointerdown(down);
  controls[1].onclick(click);
  assert.ok(down.stopped && click.stopped, 'a press on the reset does not start a drag or a selection');
  assert.deepEqual(resets, [['p', 'height']], 'the height reset names the height only');
  // A container with one authored axis has one control; without a handler nothing is drawn.
  const one = new Element('svg');
  renderSvg(one, scene(), {document, sized: new Map([['p', ['width']]]), onReset: () => {}});
  assert.equal(flatten(one).filter(e => e.attrs['data-reset-id'] !== undefined).length, 1);
  const plain = new Element('svg');
  renderSvg(plain, scene(), {document, sized: new Map([['p', ['width']]])});
  assert.equal(flatten(plain).some(e => e.attrs['data-reset-id'] !== undefined), false);
});
