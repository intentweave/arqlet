import {test} from 'node:test';
import assert from 'node:assert/strict';
import {Selection} from '../generated/editor/selection.js';
import {describeSelection} from '../generated/editor/inspector.js';

test('a node click replaces the selection; Ctrl or Cmd adds and removes nodes', () => {
  const selection = new Selection();
  selection.replaceNode('a');
  selection.toggleNode('b');
  assert.deepEqual(selection.nodes.sort(), ['a', 'b']);
  selection.toggleNode('a');
  assert.deepEqual(selection.nodes, ['b']);
  selection.replaceNode('c');
  assert.deepEqual(selection.nodes, ['c']);
});

test('a route and the nodes never coexist in the selection', () => {
  const selection = new Selection();
  selection.replaceNode('a');
  selection.selectRoute('e1');
  assert.deepEqual(selection.nodes, []);
  assert.equal(selection.route, 'e1');
  selection.toggleNode('b');
  assert.equal(selection.route, null);
});

test('clearing and pruning drop what the scene no longer shows', () => {
  const selection = new Selection();
  selection.toggleNode('a');
  selection.toggleNode('gone');
  selection.prune({nodes: new Set(['a']), routes: new Set()});
  assert.deepEqual(selection.nodes, ['a']);
  selection.clear();
  assert.equal(selection.empty, true);
});

const document = {
  input: {
    graph: {
      nodes: [
        {id: 'p', label: 'Parent', kind: 'module', parentId: null},
        {id: 'x', label: 'X', kind: 'symbol', parentId: 'p'},
        {id: 'y', label: 'Y', kind: 'symbol', parentId: 'p'},
      ],
      edges: [{id: 'e1', source: 'x', target: 'y'}],
    },
  },
  curated: {
    nodes: {p: {x: 0, y: 0, width: 400, height: 300}, x: {x: 20, y: 40, width: 180, height: 54}, y: {x: 220, y: 40, width: 180, height: 54}},
    routes: {e1: [{x: 200, y: 67}, {x: 220, y: 67}]},
    ports: {},
    pendingRoutes: ['e1'],
    lockedRoutes: [],
  },
};

test('the inspector names a single node, its box, its container and its routes', () => {
  const selection = new Selection();
  selection.replaceNode('x');
  const lines = describeSelection(document, selection);
  assert.equal(lines[0], 'Node X');
  assert.ok(lines.includes('container: Parent'));
  assert.ok(lines.some(line => line.startsWith('box: x 20')));
  assert.ok(lines.includes('  e1 · pending'));
});

test('the inspector names a route with its state', () => {
  const selection = new Selection();
  selection.selectRoute('e1');
  const lines = describeSelection(document, selection);
  assert.equal(lines[0], 'Route e1');
  assert.ok(lines.includes('X → Y'));
  assert.ok(lines.includes('Pending: re-route it before it is valid.'));
});

test('a multi-selection lists its nodes and says that a drag moves them all', () => {
  const selection = new Selection();
  selection.replaceNode('x');
  selection.toggleNode('y');
  const lines = describeSelection(document, selection);
  assert.equal(lines[0], '2 nodes selected');
  assert.ok(lines.includes('  X'));
  assert.ok(lines.includes('  Y'));
});
