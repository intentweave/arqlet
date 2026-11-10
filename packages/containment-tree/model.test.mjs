import test from 'node:test';
import assert from 'node:assert/strict';
import {axisState, conflictsByNode, conflictsOf, containmentRows, nextRowId, sizingEntryOf} from './model.js';

// A small scene: a root container with a child container that is open, and a collapsed sibling container.
function node(id, parentId, extra = {}) {
  return {id, kind: 'symbol', label: id.toUpperCase(), parentId, container: false, aggregate: false, collapsed: false, invalid: false, bounds: {x: 0, y: 0, width: 1, height: 1}, ...extra};
}
const scene = {nodes: [
  node('outer', null, {container: true}),
  node('inner', 'outer', {container: true}),
  node('a', 'inner'),
  node('b', 'outer'),
  node('closed', null, {container: true, collapsed: true}),
  node('x', null),
]};

test('rows follow the scene as a depth-first tree with levels', () => {
  const rows = containmentRows(scene);
  assert.deepEqual(rows.map(r => [r.id, r.depth]), [['outer', 0], ['inner', 1], ['a', 2], ['b', 1], ['closed', 0], ['x', 0]]);
});

test('a collapsed container is a container row even when the scene does not flag it as one', () => {
  const engine = {nodes: [node('folded', null, {container: false, collapsed: true}), node('leaf', null)]};
  const rows = containmentRows(engine);
  assert.equal(rows[0].container, true);
  assert.equal(rows[0].expanded, false);
  assert.equal(rows[1].container, false);
});

test('a container shows whether it is open, and a collapsed container has no children in the scene', () => {
  const rows = containmentRows(scene);
  const closed = rows.find(r => r.id === 'closed');
  assert.equal(closed.expanded, false);
  assert.equal(closed.hasChildren, false);
  assert.equal(rows.find(r => r.id === 'outer').expanded, true);
  assert.equal(rows.find(r => r.id === 'x').size, null);
});

test('the size of each axis states its value and where it is set', () => {
  const rows = containmentRows(scene, {containerSizing: {inner: {width: {min: 240}}, outer: {height: {fixed: 300}}}});
  const inner = rows.find(r => r.id === 'inner');
  assert.deepEqual(inner.size.width, {state: 'min', value: 240, label: 'min 240', source: 'set here'});
  assert.equal(inner.size.height.label, 'auto');
  assert.equal(inner.size.height.source, 'default');
  const outer = rows.find(r => r.id === 'outer');
  assert.equal(outer.size.height.label, 'fixed 300');
});

test('pinned nodes, invalid nodes and container conflicts are named on the row', () => {
  const invalid = {...scene, nodes: scene.nodes.map(n => (n.id === 'a' ? {...n, invalid: true} : n))};
  const issues = [{kind: 'container_bounds', nodeIds: ['a', 'inner'], message: 'Child intrudes'}];
  const rows = containmentRows(invalid, {pins: {b: {x: 1, y: 2}}, geometryIssues: issues});
  assert.equal(rows.find(r => r.id === 'b').pinned, true);
  assert.equal(rows.find(r => r.id === 'a').invalid, true);
  assert.equal(rows.find(r => r.id === 'inner').conflict, 'Child intrudes');
  assert.equal(rows.find(r => r.id === 'outer').conflict, null);
});

test('only container-bounds issues become conflicts, and the container is the second node', () => {
  const issues = [
    {kind: 'overlap', nodeIds: ['a', 'b'], message: 'overlap'},
    {kind: 'container_bounds', nodeIds: ['a', 'inner'], message: 'bounds'},
  ];
  assert.deepEqual([...conflictsOf(issues)], [['inner', 'bounds']]);
});

test('axis state without a value is content', () => {
  assert.deepEqual(axisState(undefined), {state: 'auto', value: null, label: 'auto', source: 'default'});
});

test('keys move between rows like the tree pattern, and Left and Right follow containment', () => {
  const rows = containmentRows(scene);
  assert.equal(nextRowId(rows, 'outer', 'ArrowDown'), 'inner');
  assert.equal(nextRowId(rows, 'x', 'ArrowDown'), 'x');
  assert.equal(nextRowId(rows, 'x', 'ArrowUp'), 'closed');
  assert.equal(nextRowId(rows, 'b', 'Home'), 'outer');
  assert.equal(nextRowId(rows, 'outer', 'End'), 'x');
  assert.equal(nextRowId(rows, 'outer', 'ArrowRight'), 'inner');
  assert.equal(nextRowId(rows, 'closed', 'ArrowRight'), 'closed');
  assert.equal(nextRowId(rows, 'a', 'ArrowLeft'), 'inner');
  assert.equal(nextRowId(rows, 'outer', 'ArrowLeft'), 'outer');
  assert.equal(nextRowId(rows, 'outer', 'Escape'), 'outer');
});

test('a form state becomes the sizing entry of its container, or null when both axes are content', () => {
  assert.equal(sizingEntryOf({width: {mode: 'auto'}, height: {mode: 'auto'}}), null);
  assert.deepEqual(sizingEntryOf({width: {mode: 'min', value: 240}, height: {mode: 'auto'}}), {width: {min: 240}});
  assert.deepEqual(sizingEntryOf({width: {mode: 'fixed', value: '300'}, height: {mode: 'min', value: 120}}), {width: {fixed: 300}, height: {min: 120}});
});

test('a size that is not a positive number is refused before anything is sent', () => {
  for (const value of [0, -4, NaN, '', null, 'wide']) {
    assert.throws(() => sizingEntryOf({width: {mode: 'min', value}, height: {mode: 'auto'}}), /positive size/, `${value}`);
  }
});

test('every kind of conflict names every node it involves, with the other parties', () => {
  const conflicts = [
    {kind: 'node_overlap', nodes: ['a', 'b'], routes: [], reason: 'Unrelated node rectangles overlap'},
    {kind: 'route-through-node', nodes: ['b'], routes: ['e1'], reason: 'Edge e1 passes through node b'},
    {kind: 'container-fixed-below-content', nodes: ['inner'], routes: [], reason: 'fixed below content'},
  ];
  const byNode = conflictsByNode(conflicts);
  assert.deepEqual(byNode.get('a').map(c => [c.kind, c.with]), [['node_overlap', ['b']]]);
  assert.deepEqual(byNode.get('b').map(c => c.kind), ['node_overlap', 'route-through-node']);
  assert.deepEqual(byNode.get('b')[1].routes, ['e1']);
  const rows = containmentRows({...scene, conflicts});
  assert.equal(rows.find(r => r.id === 'a').conflicts.length, 1);
  assert.equal(rows.find(r => r.id === 'b').conflicts.length, 2);
  assert.equal(rows.find(r => r.id === 'x').conflicts.length, 0);
});

test('a scene without a conflict list falls back to the geometry issues', () => {
  const issues = [{kind: 'minimum_gap', nodeIds: ['a', 'b'], message: 'too close'}];
  const rows = containmentRows(scene, {geometryIssues: issues});
  assert.deepEqual(rows.find(r => r.id === 'a').conflicts.map(c => c.reason), ['too close']);
  assert.deepEqual(rows.find(r => r.id === 'b').conflicts.map(c => c.reason), ['too close']);
});
