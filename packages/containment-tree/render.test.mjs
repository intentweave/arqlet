// The layout settings form: every scope offers auto, the fixed values and an allowed set; containers can inherit, and
// only the root's routing can be left undeclared. An allowed set is sent once two values are ticked.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {layoutFieldsElementOf} from './render.js';
import {rootSettingsOf, settingsOf} from './settings.js';

class Element {
  constructor(tag) { this.tag = tag; this.children = []; this.textContent = ''; this.value = ''; this.hidden = false; }
  append(...items) { this.children.push(...items.map(i => typeof i === 'string' ? Object.assign(new Element('#text'), {textContent: i}) : i)); }
}
const document = {createElement: tag => new Element(tag)};

const source = {
  defaultDirection: 'lr',
  containerDirections: {p: ['tb', 'bt']},
  containerStrategies: {p: 'auto'},
  nodes: [{id: 'p'}, {id: 'x', parentId: 'p'}],
};

// The select, the provenance line and the allowed-set ticks of one field.
function field(form, name) {
  const at = form.children.findIndex(e => e.tag === 'label' && e.textContent === `${name} `);
  const select = form.children[at].children[0];
  const allowed = form.children[at + 2];
  return {
    select,
    options: select.children.map(o => o.value),
    info: form.children[at + 1].textContent.trim(),
    allowed,
    ticks: allowed.children.filter(e => e.className === 'tick').map(t => t.children[0]),
  };
}

test('a container can inherit, choose auto, a fixed value or an allowed set', () => {
  const form = layoutFieldsElementOf(document, settingsOf(source, 'p'));
  const direction = field(form, 'direction');
  assert.deepEqual(direction.options, ['inherit', 'auto', 'lr', 'rl', 'tb', 'bt', 'allowed']);
  assert.equal(direction.select.value, 'allowed');
  assert.equal(direction.allowed.hidden, false, 'an allowed set shows its ticks');
  assert.deepEqual(direction.ticks.map(t => t.checked), [false, false, true, true]);
  assert.equal(direction.info, 'allowed: top → bottom / bottom → top · set here');
  const strategy = field(form, 'strategy');
  assert.equal(strategy.select.value, 'auto', 'an auto value is shown as such, not as a blank field');
  assert.deepEqual(strategy.options, ['inherit', 'auto', 'none', 'tree', 'radial', 'organic', 'allowed']);
  assert.deepEqual(field(form, 'routing').options, ['inherit', 'auto', 'orthogonal', 'roundedOrthogonal', 'polyline', 'roundedPolyline', 'straight', 'allowed']);
});

test('the root cannot inherit; only its routing can be left undeclared', () => {
  const form = layoutFieldsElementOf(document, rootSettingsOf(source));
  assert.equal(field(form, 'direction').options[0], 'auto');
  assert.equal(field(form, 'strategy').options[0], 'auto');
  assert.equal(field(form, 'routing').options[0], 'default');
  assert.equal(field(form, 'routing').select.value, 'default');
  assert.equal(field(form, 'direction').select.value, 'lr');
  assert.equal(field(form, 'strategy').select.value, 'none', 'an undeclared root strategy shows the engine default');
});

test('choices are sent as the engine reads them; an allowed set needs two ticks', () => {
  const sent = [];
  const form = layoutFieldsElementOf(document, settingsOf(source, 'p'), {onChoice: (f, v) => sent.push([f, v])});
  const routing = field(form, 'routing');
  routing.select.value = 'auto';
  routing.select.onchange();
  routing.select.value = 'inherit';
  routing.select.onchange();
  routing.select.value = 'allowed';
  routing.select.onchange();
  assert.equal(routing.allowed.hidden, false);
  routing.ticks[4].checked = true;
  routing.ticks[4].onchange();
  routing.ticks[0].checked = true;
  routing.ticks[0].onchange();
  assert.deepEqual(sent, [['routing', 'auto'], ['routing', null], ['routing', ['orthogonal', 'straight']]]);
});

test('a refused value is shown at its field with the reason, and the origin and the choice are in the line', () => {
  const settings = settingsOf({...source, containerDirections: {p: 'auto'}}, 'p', undefined, {orientations: {p: 'tb'}});
  const form = layoutFieldsElementOf(document, settings, {errors: {strategy: 'tree needs a forest'}});
  const direction = field(form, 'direction');
  assert.match(direction.info, /chose tb/);
  const problems = form.children.filter(e => e.className === 'field-error');
  assert.equal(problems.length, 1);
  assert.match(problems[0].textContent, /tree needs a forest/);
  assert.equal(problems[0].dataset?.field ?? 'strategy', 'strategy');
  // No error, no element.
  assert.equal(layoutFieldsElementOf(document, settings).children.filter(e => e.className === 'field-error').length, 0);
});

// The shape of the selected node is chosen in its tree row; a container offers no square, a leaf does.
import {renderContainmentTree} from './render.js';
import {containmentRows} from './model.js';

class Node2 {
  constructor(tag) { this.tag = tag; this.children = []; this.dataset = {}; this.style = {}; this.attrs = {}; this.value = ''; this.textContent = ''; }
  append(...items) { this.children.push(...items.map(i => typeof i === 'string' ? Object.assign(new Node2('#text'), {textContent: i}) : i)); }
  setAttribute(k, v) { this.attrs[k] = v; }
  replaceChildren(...items) { this.children = items; }
  contains() { return false; }
  get ownerDocument() { return doc2; }
}
const doc2 = {createElement: tag => new Node2(tag), createTextNode: text => Object.assign(new Node2('#text'), {textContent: text}), activeElement: null};
const treeScene = {nodes: [
  {id: 'c', kind: 'container', label: 'C', parentId: null, container: true, collapsed: false, invalid: false, shape: 'circle', bounds: {x: 0, y: 0, width: 9, height: 9}},
  {id: 'a', kind: 'symbol', label: 'A', parentId: 'c', container: false, collapsed: false, invalid: false, shape: 'square', bounds: {x: 0, y: 0, width: 9, height: 9}},
]};
const find = (node, test, out = []) => { if (test(node)) out.push(node); node.children.forEach(c => find(c, test, out)); return out; };

test('the selected row offers a shape select with the shapes the node takes, and a change is reported', () => {
  const root = new Node2('div');
  const seen = [];
  for (const [selected, options, value] of [['c', ['rect', 'circle'], 'circle'], ['a', ['rect', 'square', 'circle'], 'square']]) {
    renderContainmentTree(root, containmentRows(treeScene), {selected, onShape: (id, shape) => seen.push([id, shape]), document: doc2});
    const selects = find(root, n => n.tag === 'select' && n.className === 'tree-shape');
    assert.equal(selects.length, 1, 'only the selected row has one');
    assert.deepEqual(selects[0].children.map(o => o.value), options);
    assert.equal(selects[0].value, value);
    selects[0].value = 'rect';
    selects[0].onchange();
  }
  assert.deepEqual(seen, [['c', 'rect'], ['a', 'rect']]);
});
