// Provisional (ADR 0022, first slice): the renderer draws a membership hull as a polygon behind everything else, takes no
// pointer events on it, and gives each dimension its own hue so two dimensions over the same nodes stay apart.
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
const node = (id, bounds) => ({id, label: id, kind: 'node', parentId: null, bounds, container: false, aggregate: false, collapsed: false, invalid: false});

/** A scene with two nodes and the hulls given. */
const sceneWith = hulls => ({
  schemaVersion: 1,
  bounds: {x: -40, y: -40, width: 400, height: 240},
  nodes: [node('a', {x: 10, y: 10, width: 80, height: 40}), node('b', {x: 200, y: 10, width: 80, height: 40})],
  routes: [], ports: [], crossings: [], labels: [], showPorts: false,
  membershipHulls: hulls,
});

const hull = (dimension, group, points, extra = {}) =>
  ({dimension, dimensionLabel: dimension, group, label: group, points, boxes: ['a'], intruders: [], ...extra});

const square = (x, y, w, h) => [{x, y}, {x: x + w, y}, {x: x + w, y: y + h}, {x, y: y + h}];

test('a hull is drawn as a polygon that does not take pointer events', () => {
  const svg = new Element('svg');
  renderSvg(svg, sceneWith([hull('zone', 'dmz', square(0, 0, 100, 60))]), {document});
  const polygons = svg.find('polygon');
  assert.equal(polygons.length, 1);
  assert.equal(polygons[0].attrs.points, '0,0 100,0 100,60 0,60');
  assert.equal(polygons[0].attrs['pointer-events'], 'none', 'a node under a hull must stay reachable');
  assert.equal(polygons[0].attrs['data-hull-dimension'], 'zone');
  assert.equal(polygons[0].attrs['data-hull-group'], 'dmz');
});

test('the hull is drawn before the nodes, so the nodes cover it', () => {
  const svg = new Element('svg');
  renderSvg(svg, sceneWith([hull('zone', 'dmz', square(0, 0, 300, 100))]), {document});
  const polygon = svg.children.findIndex(c => c.tag === 'polygon');
  const firstNode = svg.children.findIndex(c => c.tag === 'g');
  assert.ok(polygon >= 0 && firstNode >= 0, 'both are drawn');
  assert.ok(polygon < firstNode, `the hull (${polygon}) must come before the first node (${firstNode})`);
});

test('each dimension gets its own hue, and a group is named at its hull', () => {
  const svg = new Element('svg');
  renderSvg(svg, sceneWith([
    hull('zone', 'dmz', square(0, 0, 120, 60)),
    hull('team', 'platform', square(20, 20, 120, 60)),
  ]), {document});
  const [first, second] = svg.find('polygon');
  assert.notEqual(first.attrs.stroke, second.attrs.stroke, 'two dimensions must be told apart by colour');
  const labels = svg.find('text').filter(t => t.attrs.class === 'lw-hull-label');
  assert.deepEqual(labels.map(t => t.textContent), ['dmz', 'platform']);
  // The name sits at the hull's topmost point, which for the second hull is 20 units lower.
  assert.equal(Number(labels[0].attrs.y), -5);
  assert.equal(Number(labels[1].attrs.y), 15);
});

test('a hull with foreign boxes inside says how many in its tooltip', () => {
  const svg = new Element('svg');
  renderSvg(svg, sceneWith([hull('zone', 'dmz', square(0, 0, 300, 100), {intruders: ['b', 'c']})]), {document});
  const titles = svg.find('title').filter(t => (t.textContent ?? '').includes('dmz'));
  assert.equal(titles.length, 1);
  assert.match(titles[0].textContent, /2 foreign inside/);
});

test('a scene without hulls draws none, and a degenerate hull is skipped', () => {
  const plain = new Element('svg');
  const scene = sceneWith([]);
  delete scene.membershipHulls;
  renderSvg(plain, scene, {document});
  assert.equal(plain.find('polygon').length, 0, 'every earlier drawing is unchanged');

  const thin = new Element('svg');
  renderSvg(thin, sceneWith([hull('zone', 'dmz', [{x: 0, y: 0}, {x: 10, y: 0}])]), {document});
  assert.equal(thin.find('polygon').length, 0, 'two points are not a polygon');
});

// Provisional (ADR 0022 §3, M2b): a group too scattered for hulls is marked member by member.
test('marks sit on their boxes in the hue of their dimension, step left when several, and take no pointer events', () => {
  const svg = new Element('svg');
  const scene = sceneWith([hull('zone', 'dmz', square(0, 0, 100, 60))]);
  scene.membershipMarks = [
    {dimension: 'team', dimensionLabel: 'Team', group: 'spread', label: 'Spread', node: 'a'},
    {dimension: 'zone', dimensionLabel: 'Zone', group: 'far', label: 'Far', node: 'a'},
    {dimension: 'team', dimensionLabel: 'Team', group: 'spread', label: 'Spread', node: 'b'},
    {dimension: 'team', dimensionLabel: 'Team', group: 'spread', label: 'Spread', node: 'missing'},
  ];
  renderSvg(svg, scene, {document});
  const marks = svg.find('circle').filter(c => c.attrs.class === 'lw-mark');
  assert.equal(marks.length, 3, 'a mark on a box the scene does not have is skipped');
  assert.ok(marks.every(m => m.attrs['pointer-events'] === 'none'));
  const onA = marks.filter(m => m.attrs['data-mark-node'] === 'a');
  assert.equal(onA.length, 2);
  assert.ok(Number(onA[1].attrs.cx) < Number(onA[0].attrs.cx), 'the second mark on a box steps to the left');
  // Node a is at x 10..90: the first mark sits inside its top right corner.
  assert.ok(Number(onA[0].attrs.cx) < 90 && Number(onA[0].attrs.cx) > 70);
  // The zone's mark has the hue of the zone's hull, the team's another.
  const zoneHull = svg.find('polygon')[0];
  const hueOf = s => s.match(/hsl\((\d+)/)[1];
  const zoneMark = onA.find(m => m.attrs['data-mark-dimension'] === 'zone');
  const teamMark = onA.find(m => m.attrs['data-mark-dimension'] === 'team');
  assert.equal(hueOf(zoneMark.attrs.fill), hueOf(zoneHull.attrs.stroke), 'one hue per dimension, hulls and marks alike');
  assert.notEqual(hueOf(teamMark.attrs.fill), hueOf(zoneMark.attrs.fill));
  const titles = svg.find('title').map(t => t.textContent);
  assert.ok(titles.includes('Team: Spread'));
});

test('a hull drawn in pieces says which piece it is', () => {
  const svg = new Element('svg');
  renderSvg(svg, sceneWith([hull('zone', 'dmz', square(0, 0, 50, 50), {piece: 1, pieces: 2})]), {document});
  const titles = svg.find('title').map(t => t.textContent ?? '');
  assert.ok(titles.some(t => t.includes('piece 2 of 2')), titles.join(' | '));
});
