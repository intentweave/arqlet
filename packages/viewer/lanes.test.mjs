// Provisional (ADR 0022 M4c): the SVG viewer draws the bands of lanes behind everything else, with a line where two meet and
// the lane's name at the start of its band; none of it takes pointer events.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {renderSvg} from './svg.js';

class Element {
  constructor(tag) { this.tag = tag; this.children = []; this.attrs = {}; this.text = undefined; }
  append(...items) { this.children.push(...items); }
  replaceChildren() { this.children = []; }
  setAttribute(k, v) { this.attrs[k] = v; }
  set textContent(v) { this.text = v; }
  get textContent() { return this.text; }
  find(tag) { return this.children.flatMap(c => [...(c.tag === tag ? [c] : []), ...c.find(tag)]); }
}
const document = {createElementNS: (_, tag) => new Element(tag)};
const node = (id, bounds, extra = {}) => ({id, label: id, kind: 'node', parentId: null, bounds, container: false, aggregate: false, collapsed: false, invalid: false, ...extra});
const lane = (group, y, vertical = false) => ({dimension: 'pool', dimensionLabel: 'Pool', group, label: group.toUpperCase(), rect: {x: 0, y, width: 400, height: 100}, vertical});

test('bands come first, alternate, meet at a dashed line and are named', () => {
  const svg = new Element('svg');
  renderSvg(svg, {
    schemaVersion: 1, bounds: {x: -30, y: -30, width: 460, height: 260},
    nodes: [node('box', {x: 10, y: 10, width: 200, height: 80}, {container: true}), node('a', {x: 20, y: 40, width: 80, height: 40}, {parentId: 'box'})],
    routes: [], ports: [], crossings: [], labels: [], showPorts: false,
    membershipLanes: [lane('customer', 0), lane('supplier', 100)],
  }, {document});
  const bands = svg.children.filter(c => c.attrs.class === 'lw-lane');
  assert.equal(bands.length, 2);
  assert.notEqual(bands[0].attrs.fill, bands[1].attrs.fill, 'neighbouring bands differ');
  const first = svg.children.indexOf(bands[0]);
  const frame = svg.children.findIndex(c => c.attrs['data-container-id'] === 'box' && c.tag === 'rect');
  assert.ok(first < frame, 'behind the containers');
  const borders = svg.children.filter(c => c.attrs.class === 'lw-lane-border');
  assert.equal(borders.length, 1, 'one line between two bands');
  assert.equal(Number(borders[0].attrs.y1), 100);
  const names = svg.find('text').filter(t => t.attrs.class === 'lw-lane-label').map(t => t.textContent);
  assert.deepEqual(names, ['CUSTOMER', 'SUPPLIER']);
  assert.ok([...bands, ...borders].every(e => e.attrs['pointer-events'] === 'none'));
});

test('a scene without lanes draws none', () => {
  const svg = new Element('svg');
  renderSvg(svg, {schemaVersion: 1, bounds: {x: 0, y: 0, width: 10, height: 10}, nodes: [], routes: [], ports: [], crossings: [], labels: [], showPorts: false}, {document});
  assert.equal(svg.children.filter(c => String(c.attrs.class).startsWith('lw-lane')).length, 0);
});

test('a phase that runs across x has its name turned in its strip', () => {
  const svg = new Element('svg');
  const phase = (group, y) => ({dimension: 'phase', dimensionLabel: 'Phase', group, label: group, rect: {x: -44, y, width: 400, height: 100}, vertical: false, phase: true});
  renderSvg(svg, {schemaVersion: 1, bounds: {x: -60, y: -30, width: 460, height: 260}, nodes: [], routes: [], ports: [], crossings: [], labels: [], showPorts: false,
    membershipLanes: [phase('request', 0), phase('approval', 100)]}, {document});
  const names = svg.find('text').filter(t => String(t.attrs.class).includes('lw-phase-label'));
  assert.equal(names.length, 2);
  assert.match(names[0].attrs.transform, /^rotate\(90 /);
});
