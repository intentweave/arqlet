// The keyed drawing: elements whose markup is unchanged stay in the host (so a press, a hover or a capture is not lost when a drag
// redraws the scene), the others are replaced. Uses a DOM with the few calls the comparison needs.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {renderSvg} from './svg.js';

class Element {
  constructor(tag) { this.tag = tag; this.children = []; this.attrs = {}; this.textContent = ''; this.parent = null; }
  append(...items) { for (const i of items) { i.parent = this; this.children.push(i); } }
  replaceChildren(...items) { this.children = []; this.append(...items); }
  insertBefore(item, before) {
    if (item.parent) item.parent.children.splice(item.parent.children.indexOf(item), 1);
    item.parent = this;
    const at = before ? this.children.indexOf(before) : this.children.length;
    this.children.splice(at, 0, item);
  }
  removeChild(item) { this.children.splice(this.children.indexOf(item), 1); item.parent = null; }
  setAttribute(k, v) { this.attrs[k] = String(v); }
  get outerHTML() {
    const attrs = Object.entries(this.attrs).sort(([a], [b]) => a.localeCompare(b)).map(([k, v]) => ` ${k}="${v}"`).join('');
    return `<${this.tag}${attrs}>${this.textContent}${this.children.map(c => c.outerHTML).join('')}</${this.tag}>`;
  }
}
const document = {createElementNS: (_, tag) => new Element(tag)};
const scene = JSON.parse(readFileSync(new URL('fixtures/state-tcp.scene.json', import.meta.url), 'utf8'));
const first = scene.nodes[0].id;

test('the same scene drawn again keeps every element', () => {
  const host = new Element('svg'); host.ownerDocument = document;
  renderSvg(host, scene, {document, keyed: true});
  const before = [...host.children];
  assert.ok(before.length > 10);
  renderSvg(host, scene, {document, keyed: true});
  assert.equal(host.children.length, before.length);
  assert.ok(host.children.every((el, i) => el === before[i]), 'every element is the one drawn before');
});

test('a change of the selection replaces only the elements that differ', () => {
  const host = new Element('svg'); host.ownerDocument = document;
  renderSvg(host, scene, {document, keyed: true});
  const before = [...host.children];
  renderSvg(host, scene, {document, keyed: true, selected: first});
  const kept = host.children.filter(el => before.includes(el)).length;
  assert.ok(kept >= before.length - 4, `${kept} of ${before.length} kept`);
  assert.ok(kept < before.length, 'the selected node is drawn differently');
});

test('a moved node replaces its own elements and the routes at it, and leaves the rest', () => {
  const host = new Element('svg'); host.ownerDocument = document;
  renderSvg(host, scene, {document, keyed: true});
  const before = [...host.children];
  const moved = structuredClone(scene);
  const leaf = moved.nodes.find(n => !n.container);
  leaf.bounds = {...leaf.bounds, x: leaf.bounds.x + 25};
  renderSvg(host, moved, {document, keyed: true});
  const kept = host.children.filter(el => before.includes(el)).length;
  assert.ok(kept > before.length / 2, `${kept} of ${before.length} kept`);
  assert.ok(kept < before.length);
});

test('a drawing that is not keyed, and the one after it, are drawn completely', () => {
  const host = new Element('svg'); host.ownerDocument = document;
  renderSvg(host, scene, {document, keyed: true});
  const before = [...host.children];
  renderSvg(host, scene, {document});
  assert.ok(host.children.every(el => !before.includes(el)), 'a drawing that is not keyed replaces everything');
  const middle = [...host.children];
  renderSvg(host, scene, {document, keyed: true});
  assert.ok(host.children.every(el => !middle.includes(el)), 'the drawing after it starts from nothing');
});

test('without keyed: true every drawing is complete', () => {
  const host = new Element('svg'); host.ownerDocument = document;
  renderSvg(host, scene, {document});
  const before = [...host.children];
  renderSvg(host, scene, {document});
  assert.ok(host.children.every(el => !before.includes(el)));
});

test('a host without a real DOM is drawn completely, as before', () => {
  const plain = {tag: 'svg', children: [], attrs: {}, append(...i) { this.children.push(...i); }, replaceChildren() { this.children = []; }, setAttribute(k, v) { this.attrs[k] = v; }};
  renderSvg(plain, scene, {document, keyed: true});
  assert.ok(plain.children.length > 10);
});
