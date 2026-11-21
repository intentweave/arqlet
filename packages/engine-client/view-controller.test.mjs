import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import {createSession} from './session.js';
import {createViewController, sceneDiff, encodeBookmark, decodeBookmark, navigateScene} from './view-controller.js';
import {createEngine} from './wasm-client.js';

const shipped = new URL('./arqlet.wasm', import.meta.url);
const built = new URL('../../target/wasm32-unknown-unknown/release/arqlet_wasm.wasm', import.meta.url);
const wasm = existsSync(shipped) ? shipped : built;
const engine = existsSync(wasm) ? await createEngine(readFileSync(wasm)) : null;

// A container `box` of 30 children (a chain c00 → c01 → … inside), a node `out` fed by the last child, and `src` feeding
// the first: collapsed at the start, so that paging, filters, paths and steps back all have something to do.
function graph() {
  const nodes = [
    {id: 'box', kind: 'ecu', label: 'box'},
    {id: 'src', kind: 'ecu', label: 'src'},
    {id: 'out', kind: 'ecu', label: 'out'},
  ];
  const edges = [{id: 'in', kind: 'k', source: 'src', target: 'c00'}, {id: 'last', kind: 'k', source: 'c29', target: 'out'}];
  for (let i = 0; i < 30; i++) {
    const id = `c${String(i).padStart(2, '0')}`;
    nodes.push({id, kind: i % 2 ? 'odd' : 'even', label: id, parentId: 'box'});
    if (i > 0) edges.push({id: `e${i}`, kind: 'k', source: `c${String(i - 1).padStart(2, '0')}`, target: id});
  }
  return {nodes, edges};
}

const children = state => state.scene.nodes.filter(n => n.parentId === 'box').length;

test('sceneDiff names what was added, removed and changed', () => {
  const node = (id, x, collapsed = false) => ({id, bounds: {x, y: 0, width: 10, height: 10}, collapsed});
  const route = (id, x) => ({id, points: [{x: 0, y: 0}, {x, y: 0}]});
  const d = sceneDiff({nodes: [node('a', 0), node('b', 0), node('c', 0)], routes: [route('r', 5)]},
    {nodes: [node('a', 0), node('b', 4), node('d', 0)], routes: [route('r', 6), route('s', 1)]});
  assert.deepEqual(d.nodes, {added: ['d'], removed: ['c'], changed: ['b']});
  assert.deepEqual(d.routes, {added: ['s'], removed: [], changed: ['r']});
  assert.deepEqual(sceneDiff(null, {nodes: [node('a', 0)], routes: []}).nodes.added, ['a']);
});

for (const resident of [false, true]) {
  test(`a view controller pages, filters, highlights and steps back (resident ${resident})`, {skip: !engine || (resident && !engine.openSession)}, async () => {
    const view = createViewController(createSession(engine, {resident}), {page: 10});
    let notified = 0;
    const stop = view.subscribe(() => { notified += 1; });
    assert.equal((await view.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}})).ok, true);
    let s = view.getState();
    assert.ok(s.scene && !s.busy && !s.error, JSON.stringify(s.error));
    assert.equal(view.getState(), s, 'the same object while nothing changes');
    assert.ok(notified > 0);
    assert.equal(s.scene.nodes.find(n => n.id === 'box').collapsed, true);
    assert.equal(s.canBack, false);

    // Details of a node inside the collapsed box: shown by the box; kept up to date after a change, until cleared.
    assert.equal((await view.select({nodeId: 'c05'})).ok, true);
    assert.equal(view.getState().details.node.shownAs, 'box');
    assert.deepEqual(view.getState().details.node.parents, ['box']);
    view.hover({edgeId: 'e5'});
    assert.deepEqual(view.getState().hover, {edgeId: 'e5'});
    const hovered = view.getState();
    view.hover({edgeId: 'e5'});
    assert.equal(view.getState(), hovered, 'the same hover changes nothing');
    view.hover(null);
    // A path into the collapsed box: highlighted, kept while the view changes.
    assert.equal((await view.trace('src', 'out')).ok, true);
    assert.equal(view.getState().highlight.kind, 'trace');

    // Paging: two children asked for, then pages of 10 from the rest.
    await view.reveal(['c05', 'c07']);
    assert.equal(children(view.getState()), 3, 'two children and the rest');
    assert.equal(view.getState().details.node.shownAs, 'c05', 'the details follow the view');
    assert.ok(view.getState().details.node.bounds.width > 0);
    // A line: the edges it stands for.
    await view.select({edgeId: 'in'});
    assert.equal(view.getState().details.edge.source, 'src');
    assert.ok(view.getState().details.route, JSON.stringify(view.getState().details));
    await view.select(null);
    assert.equal(view.getState().details, null);
    await view.showRest('box');
    s = view.getState();
    assert.equal(children(s), 13, 'twelve children and the rest');
    assert.ok(s.diff.nodes.added.length >= 10, JSON.stringify(s.diff.nodes));
    assert.ok(s.highlight, 'the highlight stays');
    assert.ok(s.canBack);

    // A filter, and a step back over it.
    await view.filter({nodeKinds: ['odd']});
    s = view.getState();
    assert.equal(s.backLabel, 'Filter');
    assert.deepEqual(s.hide, {nodeKinds: ['odd']});
    assert.ok(!s.scene.nodes.some(n => n.kind === 'odd'), 'odd children hidden');
    await view.back();
    s = view.getState();
    assert.deepEqual(s.hide, {});
    assert.ok(s.scene.nodes.some(n => n.kind === 'odd'), 'the filter undone');
    assert.equal(children(s), 13);
    // Back over the page, then forward again.
    await view.back();
    assert.equal(children(view.getState()), 3);
    assert.ok(view.getState().canForward);
    await view.forward();
    assert.equal(children(view.getState()), 13);

    // The highlight until it is cleared.
    view.clearHighlight();
    assert.equal(view.getState().highlight, null);

    // Steps queued after a cancel run; the ones queued before it and not started are dropped.
    const a = view.toggle('box');
    view.cancel();
    const b = view.toggle('box');
    const [ra, rb] = await Promise.all([a, b]);
    assert.ok(ra.ok || ra.cancelled, JSON.stringify(ra));
    assert.equal(rb.ok, true);
    assert.equal(view.getState().busy, false);
    stop();
  });
}

test('a host hands over its own snapshots; follow-ups merge into one step', {skip: !engine}, async () => {
  const session = createSession(engine, {resident: false});
  const view = createViewController(session, {scenes: false});
  await view.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}});
  assert.equal(view.getState().scene, null, 'no scene with scenes: false');
  const start = view.getState().snapshot;
  // A snapshot the host made (here: the engine's own toggle), remembered as one step.
  const opened = engine({op: 'toggle', snapshot: start, nodeId: 'box', expectedRevision: start.revision,
    expectedLayoutVersion: start.layoutVersion ?? 0}).snapshot;
  assert.equal((await view.adopt(opened, {remember: 'Open box'})).ok, true);
  assert.equal(view.getState().backLabel, 'Open box');
  // A follow-up merged into it, and a step that is not remembered.
  await view.filter({nodeKinds: ['odd']});
  await view.apply({op: 'filter', hide: {}}, 'Show all', {merge: true});
  await view.apply({op: 'configure', options: {routingClearance: 9}}, 'clearance', {remember: false});
  assert.equal(view.getState().backLabel, 'Open box', 'a filter undone by its follow-up leaves no step');
  await view.back();
  assert.ok(view.getState().snapshot.collapsedIds.includes('box'), 'back over the adopted step');
  assert.equal(view.getState().canBack, false);
  // Another graph: the history starts empty.
  await view.adopt(start, {forget: true});
  assert.equal(view.getState().canForward, false);
});

for (const resident of [false, true]) {
  test(`search, focus and bookmarks (resident ${resident})`, {skip: !engine || (resident && !engine.openSession)}, async () => {
    const view = createViewController(createSession(engine, {resident}), {page: 10});
    await view.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}});
    // Search: hidden children are found, shown by their box.
    const found = await view.find('C1');
    assert.equal(found.ok, true, JSON.stringify(found));
    assert.equal(found.total, 10);
    assert.equal(found.matches[0].id, 'c10');
    assert.equal(found.matches[0].shownAs, 'box');
    assert.equal(view.getState().search.query, 'C1');
    view.clearSearch();
    assert.equal(view.getState().search, null);
    // Focus: opens what hides the node (one step back), selects it, names the bounds for the camera.
    const focused = await view.focus('c12');
    assert.equal(focused.ok, true, JSON.stringify(focused));
    let s = view.getState();
    assert.equal(s.focus.nodeId, 'c12');
    assert.equal(s.focus.shownAs, 'c12');
    assert.ok(s.focus.bounds.width > 0);
    assert.equal(s.details.node.id, 'c12');
    assert.equal(s.backLabel, 'Show c12');
    await view.focus('c12');
    assert.equal(view.getState().focus.serial, s.focus.serial + 1, 'the same node again');
    // A bookmark: the view with a filter and a path, as URL text and back.
    await view.filter({nodeKinds: ['odd']});
    await view.trace('src', 'out');
    const {bookmark} = await view.bookmark();
    const text = encodeBookmark(bookmark);
    assert.match(text, /^[A-Za-z0-9_-]+$/);
    assert.deepEqual(decodeBookmark(text), bookmark);
    assert.throws(() => decodeBookmark('bm90IGEgYm9va21hcms'));
    const shown = view.getState().scene.nodes.map(n => n.id).sort();
    // Elsewhere, then the bookmark again: the same view, the filter and the path back, one step for `back`.
    await view.back();
    await view.back();
    view.clearHighlight();
    assert.equal((await view.openBookmark(decodeBookmark(text))).ok, true);
    s = view.getState();
    assert.deepEqual(s.scene.nodes.map(n => n.id).sort(), shown);
    assert.deepEqual(s.hide, {nodeKinds: ['odd']});
    assert.equal(s.highlight.kind, 'trace');
    assert.ok(s.highlight.answer, 'the path asked again');
    assert.equal(s.backLabel, 'Open bookmark');
    // Another graph's bookmark is refused unless forced.
    const other = {...bookmark, digest: 'another-graph'};
    assert.match(bookmark.digest, /\S/, 'the graph is named');
    assert.equal((await view.openBookmark(other)).ok, false);
    assert.equal((await view.openBookmark(other, {force: true})).ok, true);
  });
}

test('navigateScene moves by space, hierarchy, siblings and edges', () => {
  const box = (id, x, y, w, h, parentId) => ({id, bounds: {x, y, width: w, height: h}, ...(parentId ? {parentId} : {})});
  const scene = {
    nodes: [box('P', 0, 0, 200, 100), box('a', 10, 10, 40, 20, 'P'), box('b', 100, 10, 40, 20, 'P'),
      box('c', 10, 60, 40, 20, 'P'), box('x', 300, 10, 40, 20)],
    routes: [{id: 'ab', source: 'a', target: 'b'}, {id: 'bx', source: 'b', target: 'x'}, {id: 'xa', source: 'x', target: 'a'}],
  };
  const go = (from, d) => navigateScene(scene, from, d);
  assert.equal(go(null, 'right'), 'P', 'without a selection: the first top-level node');
  assert.deepEqual([go('a', 'right'), go('b', 'right'), go('x', 'left'), go('a', 'down'), go('c', 'up')], ['b', 'x', 'b', 'c', 'a']);
  assert.equal(go('x', 'up'), null, 'nothing lies that way');
  assert.equal(go('P', 'right'), 'x', 'not into its own contents');
  assert.equal(go('a', 'left'), null, 'not out to its own container');
  assert.deepEqual([go('a', 'parent'), go('P', 'child'), go('x', 'parent'), go('a', 'child')], ['P', 'a', null, null]);
  assert.deepEqual([go('a', 'next'), go('b', 'next'), go('c', 'next'), go('a', 'previous')], ['b', 'c', 'a', 'c']);
  assert.deepEqual([go('a', 'out'), go('a', 'in'), go('x', 'in'), go('c', 'out')], ['b', 'x', 'b', null]);
  assert.throws(() => go('a', 'sideways'));
});

test('a view controller navigates by keyboard directions', {skip: !engine}, async () => {
  const view = createViewController(createSession(engine, {resident: false}), {scenes: false});
  await view.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}});
  const first = await view.navigate('right');
  assert.equal(first.ok, true, JSON.stringify(first));
  assert.equal(view.getState().selection.nodeId, first.nodeId);
  assert.ok(view.getState().focus.bounds, 'the camera target');
  // The box is closed: no child until it is opened, then the first child; back up to it.
  await view.select({nodeId: 'box'});
  const closed = await view.navigate('child');
  assert.deepEqual([closed.ok, closed.collapsed], [false, true]);
  await view.open('box');
  const inside = await view.navigate('child');
  assert.equal(inside.ok, true, JSON.stringify(inside));
  assert.equal(view.getState().details.node.parents.at(-1), 'box');
  assert.equal((await view.navigate('parent')).nodeId, 'box');
  assert.equal(view.getState().backLabel, 'Open box', 'navigating is not a step for back');
  // Along the edges: from src out into the box.
  await view.select({nodeId: 'src'});
  const out = await view.navigate('out');
  assert.equal(out.ok, true, JSON.stringify(out));
  assert.notEqual(out.nodeId, 'src');
});
