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

// A large view without randomness: packages of modules of functions with ids that are paths, calls along each module.
function large(packages = 24, modules = 24, functions = 10) {
  const two = n => String(n).padStart(2, '0');
  const nodes = [];
  const edges = [];
  for (let p = 0; p < packages; p++) {
    const pkg = `services/domain-${two(p)}`;
    nodes.push({id: pkg, kind: 'package', label: `domain ${p}`});
    for (let m = 0; m < modules; m++) {
      const mod = `${pkg}/module-${two(m)}.ts`;
      nodes.push({id: mod, kind: 'module', label: `module ${m}`, parentId: pkg});
      for (let f = 0; f < functions; f++) {
        const id = `${mod}#function-${two(f)}`;
        nodes.push({id, kind: f % 4 === 3 ? 'test' : 'function', label: `function ${f}`, parentId: mod});
        if (f > 0) edges.push({id: `call-${edges.length}`, kind: 'calls', source: `${mod}#function-${two(f - 1)}`, target: id});
      }
    }
  }
  return {nodes, edges};
}

const drawn = state => ({nodes: state.scene.nodes, routes: state.scene.routes});

for (const resident of [false, true]) {
  test(`a compact bookmark of a large view fits a URL and opens again (resident ${resident})`, {skip: !engine || (resident && !engine.openSession)}, async () => {
    const init = {limit: 10000, options: {levelOfDetail: {depth: 1}}};
    const view = createViewController(createSession(engine, {resident}));
    await view.import(large(), init);
    const mod = (p, m) => `services/domain-0${p}/module-0${m}.ts`;
    const fn = (p, m, f) => `${mod(p, m)}#function-0${f}`;
    for (const p of [0, 1, 2, 3]) assert.equal((await view.open(mod(p, 0))).ok, true);
    await view.reveal([fn(5, 5, 5)]);
    await view.filter({nodeKinds: ['test'], nodes: [fn(0, 0, 0)]});
    await view.trace(fn(0, 0, 1), fn(0, 0, 6));
    await view.select({nodeId: fn(1, 0, 2)});
    const shown = view.getState();
    const compact = await view.compactBookmark();
    assert.equal(compact.ok, true, JSON.stringify(compact));
    assert.match(compact.text, /^b1\.[A-Za-z0-9_-]+$/);
    const {bookmark} = await view.bookmark();
    assert.ok(encodeBookmark(bookmark).length > 20000, `${encodeBookmark(bookmark).length}`);
    assert.ok(compact.text.length < 2000, `${compact.text.length}`);
    assert.throws(() => decodeBookmark(compact.text), /openBookmark/);

    // Another controller on a fresh import opens the same view, one step for `back`.
    const other = createViewController(createSession(engine, {resident}));
    await other.import(large(), init);
    const start = drawn(other.getState());
    const opened = await other.openBookmark(compact.text);
    assert.equal(opened.ok, true, JSON.stringify(opened));
    assert.deepEqual([opened.sameSource, opened.sameDefaults, opened.missing, opened.ignored], [true, true, [], []]);
    let s = other.getState();
    assert.deepEqual(drawn(s), drawn(shown));
    assert.deepEqual(s.hide, shown.hide);
    assert.equal(s.highlight.kind, 'trace');
    assert.deepEqual(s.highlight.answer.nodes, shown.highlight.answer.nodes);
    assert.deepEqual(s.selection, {nodeId: fn(1, 0, 2)});
    assert.equal(s.backLabel, 'Open bookmark');
    // The bookmark of the opened view is the same text: the defaults are those of the import, not of the opened view.
    assert.equal((await other.compactBookmark()).text, compact.text);
    await other.back();
    assert.deepEqual(drawn(other.getState()), start);

    // The next revision lacks an opened module and the selected node's module: reported, the rest restored.
    const next = large();
    const gone = id => id === mod(1, 0) || id.startsWith(`${mod(1, 0)}#`);
    next.nodes = next.nodes.filter(n => !gone(n.id));
    next.edges = next.edges.filter(e => !gone(e.source) && !gone(e.target));
    await other.import(next, init);
    const partial = await other.openBookmark(compact.text);
    assert.equal(partial.ok, true, JSON.stringify(partial));
    assert.equal(partial.sameSource, false);
    assert.deepEqual(partial.missing, [mod(1, 0), fn(1, 0, 2)]);
    s = other.getState();
    assert.equal(s.selection, null);
    assert.equal(s.highlight.kind, 'trace');
    assert.ok(s.scene.nodes.some(n => n.id === fn(0, 0, 1)), 'the other modules are open');
    // The long form of the old revision: refused, and restored as far as it goes when forced.
    assert.equal((await other.openBookmark(bookmark)).ok, false);
    const forced = await other.openBookmark(encodeBookmark({...bookmark, intent: {...bookmark.intent,
      pins: {[fn(1, 0, 3)]: {x: 1, y: 2}}}}), {force: true});
    assert.equal(forced.ok, true, JSON.stringify(forced));
    assert.deepEqual(forced.missing, [fn(1, 0, 2), fn(1, 0, 3)]);
    assert.equal((await other.openBookmark('b9.e30')).ok, false);
    assert.equal((await other.openBookmark('nonsense')).ok, false);
  });
}

for (const resident of [false, true]) {
  test(`a long bookmark of changed shapes, directions and ports opens on a fresh import (resident ${resident})`, {skip: !engine || (resident && !engine.openSession)}, async () => {
    const init = {limit: 100, options: {levelOfDetail: {depth: 0}}};
    const fresh = async (of = graph()) => {
      const view = createViewController(createSession(engine, {resident}));
      assert.equal((await view.import(of, init)).ok, true);
      return view;
    };
    const view = await fresh();
    await view.open('box');
    for (const request of [
      {op: 'shape', nodeId: 'src', shape: 'circle'},
      {op: 'direction', direction: 'tb'},
      {op: 'strategy', nodeId: 'box', strategy: 'tree'},
      {op: 'routing', nodeId: 'box', routing: 'straight'},
      {op: 'port', edgeId: 'last', endpoint: 'target', side: 'top', offset: 0.3},
    ]) {
      const changed = await view.apply(request);
      assert.equal(changed.ok, true, JSON.stringify(changed));
    }
    const {bookmark} = await view.bookmark();

    const other = await fresh();
    const start = (await other.bookmark()).bookmark;
    assert.notEqual(bookmark.digest, start.digest, 'the digest is of the graph as the view changed it');
    // As a value and as URL text, also forced: the same graph, so nothing is reported.
    for (const [given, options] of [[bookmark], [encodeBookmark(bookmark)], [bookmark, {force: true}]]) {
      const opened = await other.openBookmark(given, options);
      assert.equal(opened.ok, true, JSON.stringify(opened));
      assert.equal(opened.sameSource, undefined);
      assert.deepEqual((await other.bookmark()).bookmark, bookmark);
      assert.equal(other.getState().backLabel, 'Open bookmark');
      assert.equal((await other.back()).ok, true);
      assert.deepEqual((await other.bookmark()).bookmark, start);
      assert.equal((await other.forward()).ok, true);
      assert.deepEqual((await other.bookmark()).bookmark, bookmark);
      // The bookmark of the view before the changes opens on the changed one.
      assert.equal((await other.openBookmark(start)).ok, true);
      assert.deepEqual((await other.bookmark()).bookmark, start);
    }

    // Another graph stays refused, and is read in part when forced.
    const revised = graph();
    revised.nodes[2].label = 'renamed';
    const another = await fresh(revised);
    const refused = await another.openBookmark(bookmark);
    assert.equal(refused.ok, false);
    assert.equal(refused.error, 'The bookmark belongs to another graph.');
    assert.equal(another.getState().error, refused.error);
    assert.equal(another.getState().canBack, false);
    const forced = await another.openBookmark(bookmark, {force: true});
    assert.equal(forced.ok, true, JSON.stringify(forced));
    assert.deepEqual([forced.sameSource, forced.missing, forced.ignored], [false, [], []]);
  });
}

test('a compact bookmark is compared with the view as imported, also when it was adopted', {skip: !engine}, async () => {
  // A graph that declares a shape: the compact bookmark leaves it to the defaults, if the view's are known.
  const shaped = () => {
    const g = graph();
    for (const n of g.nodes.slice(3)) n.shape = 'square';
    return g;
  };
  const init = {limit: 100, options: {levelOfDetail: {depth: 0}}};
  const imported = engine({op: 'import', graph: shaped(), ...init}).snapshot;
  const session = createSession(engine, {resident: false});
  const view = createViewController(session, {scenes: false});
  await view.adopt(imported, {forget: true});
  await view.open('box');
  const relative = (await view.compactBookmark()).text;
  assert.ok(relative.length < 300, `${relative.length}`);
  // A snapshot that went through changes holds decisions: nothing is left to defaults then, and it opens anywhere.
  const changed = session.snapshot;
  await view.adopt(changed, {forget: true});
  const whole = (await view.compactBookmark()).text;
  assert.ok(whole.length > 2 * relative.length, `${whole.length}`);
  for (const text of [relative, whole]) {
    const fresh = createViewController(createSession(engine, {resident: false}), {scenes: false});
    await fresh.import(shaped(), init);
    const opened = await fresh.openBookmark(text);
    assert.equal(opened.ok, true, JSON.stringify(opened));
    assert.ok(!fresh.getState().snapshot.collapsedIds?.includes('box'));
    assert.equal(fresh.getState().snapshot.graph.nodes.find(n => n.id === 'c04').shape, 'square');
  }
});

test('the defaults of a compact bookmark cost a step no engine call of their own', {skip: !engine}, async () => {
  const ops = [];
  const counted = request => { ops.push(request.op); return engine(request); };
  const view = createViewController(createSession(counted, {resident: false}), {scenes: false, keepGeometry: false});
  await view.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}});
  ops.length = 0;
  await view.apply({op: 'toggle', nodeId: 'box'}, 'Open', {remember: false});
  assert.deepEqual(ops, ['toggle']);
  // The change was not remembered, so the defaults stay unknown: the bookmark keeps every decision, also this one.
  const {text} = await view.compactBookmark();
  const fresh = createViewController(createSession(engine, {resident: false}), {scenes: false});
  await fresh.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}});
  assert.equal((await fresh.openBookmark(text)).ok, true);
  assert.ok(!fresh.getState().snapshot.collapsedIds?.includes('box'));
});

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

// A second engine for refinement, as a second worker would be: it reports the intermediate results of the request it runs.
async function refineEngine() {
  let onPublication = null;
  const second = await createEngine(readFileSync(wasm), {onPublish: text => onPublication?.(JSON.parse(text).publication)});
  return (request, init = {}) => {
    onPublication = init.onPublication ?? null;
    try { return second(request); } finally { onPublication = null; }
  };
}

test('refinement runs on a second engine; stale results are discarded', {skip: !engine}, async () => {
  const refine = await refineEngine();
  const view = createViewController(createSession(engine, {resident: false}), {refine, publicationInterval: 0});
  // A graph and the earlier pipeline, where refinement finds an improvement and publishes it before the end.
  const improvable = {
    nodes: [{id: 'root', kind: 'directory', label: 'root'}, {id: 'a', kind: 'symbol', label: 'A', parentId: 'root'},
      {id: 'b', kind: 'symbol', label: 'B', parentId: 'root'}],
    edges: [{id: '1', kind: 'calls', source: 'a', target: 'b'}, {id: '2', kind: 'calls', source: 'b', target: 'a'},
      {id: '3', kind: 'calls', source: 'a', target: 'a'}, {id: '4', kind: 'calls', source: 'a', target: 'b'}],
  };
  await view.import(improvable, {limit: 20, options: {singlePassAuto: false, singlePass: false, abstractRouting: false,
    layeredConstruction: false, hierarchicalOrdering: false}});
  const before = view.getState().snapshot;
  // Applied: the next layout version of the same revision, not a step for back.
  let shown = 0;
  const stop = view.subscribe(() => { shown = Math.max(shown, view.getState().refining?.shown ?? 0); });
  const done = await view.refine({op: 'refineProgressive', threads: 1});
  stop();
  assert.equal(done.ok, true, JSON.stringify(done));
  assert.ok(shown > 0, 'intermediate results were shown');
  let s = view.getState();
  assert.equal(s.refining, null);
  assert.equal(s.refined.applied, true, JSON.stringify(s.refined));
  assert.equal(s.snapshot.revision, before.revision);
  assert.equal(s.snapshot.layoutVersion, (before.layoutVersion ?? 0) + 1);
  assert.equal(s.canBack, false, 'refining changes no decision');
  // A change while it runs: the refinement is stopped and its result dropped; the change stays.
  const slow = (request, init) => new Promise(resolve => setTimeout(() => resolve(refine(request, init)), 30));
  const view2 = createViewController(createSession(engine, {resident: false}), {refine: slow});
  await view2.import(graph(), {limit: 100});
  const running = view2.refine();
  await new Promise(r => setTimeout(r, 0));
  assert.ok(view2.getState().refining, 'named while it runs');
  await view2.toggle('box');
  const late = await running;
  assert.equal(late.ok, false);
  s = view2.getState();
  assert.equal(s.refined.cancelled, true, JSON.stringify(s.refined));
  assert.ok(s.snapshot.collapsedIds.includes('box'), 'the toggle stays');
  assert.equal(s.canBack, true);
  // A result for a view that has moved on is discarded and counted: here the layout changes underneath (as a host's own
  // snapshot would) while the refinement runs.
  const session3 = createSession(engine, {resident: false});
  const view3 = createViewController(session3, {
    refine: async (request, init) => {
      const answer = refine(request, init);
      const moved = engine({op: 'configure', snapshot: request.snapshot, options: {routingClearance: 9},
        expectedRevision: request.expectedRevision, expectedLayoutVersion: request.expectedLayoutVersion});
      assert.equal(moved.ok, true, JSON.stringify(moved));
      await session3.load(moved.snapshot);
      return answer;
    },
  });
  await view3.import(graph(), {limit: 100});
  const stale = await view3.refine();
  assert.equal(stale.ok, false);
  assert.equal(view3.getState().refined.stale, true, JSON.stringify(view3.getState().refined));
  assert.equal(view3.getState().discarded, 1);
  assert.equal((await view3.refine({op: 'nothing-like-it'})).ok, false, 'an unknown operation is an error');
  assert.ok(view3.getState().refined.error);
  // Without a second engine there is no refinement.
  assert.equal((await view.refine()).ok, true);
  const plain = createViewController(createSession(engine, {resident: false}));
  assert.match((await plain.refine()).error, /No refinement engine/);
});

test('a look into a collapsed container leaves the view as it is', {skip: !engine}, async () => {
  const view = createViewController(createSession(engine, {resident: false}));
  await view.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}});
  const before = view.getState().snapshot;
  const look = await view.peek('box', {limit: 12});
  assert.equal(look.ok, true, JSON.stringify(look));
  assert.deepEqual([look.children, look.shown, look.omitted], [30, 12, 18]);
  assert.ok(look.scene.nodes.some(n => n.id === 'c00'));
  assert.deepEqual(look.external.c00, {incoming: 1, outgoing: 0}, 'src calls c00 from outside');
  let s = view.getState();
  assert.equal(s.peek.nodeId, 'box');
  assert.equal(s.snapshot, before, 'nothing changed');
  assert.equal(s.canBack, false);
  assert.ok(s.scene.nodes.find(n => n.id === 'box').collapsed);
  view.closePeek();
  assert.equal(view.getState().peek, null);
  // The next change ends a look.
  await view.peek('box');
  await view.open('box');
  assert.equal(view.getState().peek, null);
  assert.equal((await view.peek('src')).ok, false, 'a leaf has nothing to look into');
});

test('content in a resident session: details show it, find searches a key', {skip: !engine || !engine.openSession}, async () => {
  const view = createViewController(createSession(engine, {resident: true}));
  await view.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}});
  await view.select({nodeId: 'c05'});
  assert.equal(view.getState().details.node.properties, undefined);
  const set = await view.payload({set: {nodes: {c05: {owner: 'Ann', loc: 12}, nowhere: {owner: 'Gil'}}}});
  assert.equal(set.ok, true, JSON.stringify(set));
  assert.deepEqual([set.contentRevision, set.unknownNodes], [1, 1]);
  assert.deepEqual(view.getState().details.node.properties, {loc: 12, owner: 'Ann'}, 'the selection shows the properties');
  assert.equal(view.getState().contentRevision, 1);
  assert.equal(view.getState().canBack, false, 'content is not a view decision');
  const found = await view.find('ann', {key: 'owner'});
  assert.deepEqual([found.total, found.matches[0].id, found.matches[0].value], [1, 'c05', 'Ann']);
  assert.equal((await view.payload({set: {nodes: {c05: {label: 'x'}}}})).ok, false, 'a key of the model itself');
  // Without a resident session there is no content.
  const plain = createViewController(createSession(engine, {resident: false}));
  await plain.import(graph(), {limit: 100});
  assert.match((await plain.payload({set: {nodes: {c05: {owner: 'Ann'}}}})).error, /resident session/);
});

test('the change log of a resident session: groups in one sequence, replayed on a second session', {skip: !engine || !engine.openSession}, async () => {
  const primary = createSession(engine, {resident: true});
  await primary.import(graph(), {limit: 100});
  const start = (await primary.content()).sequence;
  const added = await primary.change({patch: {nodes: [{id: 'new', kind: 'odd', label: 'new', parentId: 'box'}],
    edges: [{id: 'to-new', kind: 'k', source: 'src', target: 'new'}]}, payload: {set: {nodes: {new: {owner: 'Ann'}}}}});
  assert.equal(added.ok, true, JSON.stringify(added));
  assert.deepEqual([added.sequence, added.contentRevision], [start + 1, 1]);
  const log = await primary.changesSince(start);
  assert.deepEqual(log.changes.map(g => g.sequence), [start + 1]);
  assert.equal((await primary.change({expectedSequence: start, payload: {set: {nodes: {new: {x: 1}}}}})).ok, false, 'stale');
  // A second session applies the group and has the node and its property.
  const replica = createSession(engine, {resident: true});
  await replica.import(graph(), {limit: 100});
  const {sequence: _s, ...group} = log.changes[0];
  assert.equal((await replica.change(group)).ok, true);
  assert.equal((await replica.details({nodeId: 'new'})).node.properties.owner, 'Ann');
  // Without a resident session there is no log.
  assert.match((await createSession(engine, {resident: false}).changesSince(0)).error, /resident session/);
});

test('a resident session tells what it holds and gives parts back', {skip: !engine || !engine.openSession}, async () => {
  const session = createSession(engine, {resident: true});
  await session.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}});
  const owners = Object.fromEntries(Array.from({length: 30}, (_, i) => [`c${String(i).padStart(2, '0')}`, {owner: `t${i % 3}`, loc: i}]));
  await session.payload({set: {nodes: owners}});
  const held = await session.memory();
  assert.equal(held.ok, true, JSON.stringify(held));
  assert.ok(held.parts.sourceGraph > 0 && held.parts.content > 0 && held.total > 0);
  assert.ok(held.heapBytes > 0, 'the WASM engine counts its heap');
  assert.ok(held.linearMemoryBytes >= held.heapBytes, 'and names its linear memory');
  const freed = await session.release(['content'], {keys: ['loc']});
  assert.ok(freed.parts.content < held.parts.content, JSON.stringify(freed.parts));
  assert.equal((await session.details({nodeId: 'c05'})).node.properties.loc, undefined);
  const all = await session.release(['content', 'changeLog']);
  assert.deepEqual([all.parts.content, all.parts.changeLog], [0, 0]);
  assert.match((await createSession(engine, {resident: false}).memory()).error, /resident session/);
});

test('a resident session moves to a new engine and the memory of the old one can go', {skip: !engine || !engine.openSession}, async () => {
  // A graph large enough to grow the old engine's memory: 20 containers of 150 functions in a chain each.
  const nodes = [];
  const edges = [];
  for (let c = 0; c < 20; c++) {
    nodes.push({id: `g${c}`, kind: 'group', label: `g${c}`});
    for (let i = 0; i < 150; i++) {
      nodes.push({id: `g${c}-${i}`, kind: 'fn', label: `f${i}`, parentId: `g${c}`});
      if (i) edges.push({id: `e${c}-${i}`, kind: 'calls', source: `g${c}-${i - 1}`, target: `g${c}-${i}`});
    }
    if (c) edges.push({id: `x${c}`, kind: 'calls', source: `g${c - 1}-0`, target: `g${c}-0`});
  }
  const old = await createEngine(readFileSync(wasm));
  const session = createSession(old, {resident: true});
  // The whole graph laid out, so that the old engine's memory grows with the layout.
  assert.equal((await session.import({nodes, edges}, {limit: 3100})).ok, true);
  await session.payload({set: {nodes: {'g3-7': {owner: 'Ann'}}}});
  await session.payload({set: {nodes: {'g3-7': {owner: 'Bob'}}}});
  const before = await session.memory();
  const state = session.snapshot;

  const fresh = await createEngine(readFileSync(wasm));
  const moved = await session.recycle(fresh);
  assert.equal(moved.ok, true, JSON.stringify(moved));
  assert.deepEqual([moved.contentRevision, moved.sequence], [2, 2]);
  const after = await session.memory();
  assert.ok(after.linearMemoryBytes < before.linearMemoryBytes,
    `the new engine holds less: ${after.linearMemoryBytes} < ${before.linearMemoryBytes}`);
  assert.equal(session.snapshot, state, 'the view did not change');
  assert.equal((await session.details({nodeId: 'g3-7'})).node.properties.owner, 'Bob', 'the content moved');
  assert.equal((await session.details({nodeId: 'g3-7'})).contentRevision, 2, 'with its revision');
  assert.equal((await session.changesSince(1)).resync, true, 'the log stayed behind: a replica resyncs');
  // The session goes on on the new engine.
  assert.equal((await session.collapse('g3')).ok, true);
  assert.equal((await session.payload({set: {nodes: {'g3-7': {owner: 'Cid'}}}})).sequence, 3, 'the numbers go on');
  assert.match((await createSession(engine, {resident: false}).recycle(fresh)).error, /resident session/);
});

for (const resident of [false, true]) {
  test(`back and forward restore the geometry they left (#148, resident ${resident})`, {skip: !engine || (resident && !engine.openSession)}, async () => {
    const view = createViewController(createSession(engine, {resident}), {page: 10});
    assert.equal((await view.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}})).ok, true);
    const geometry = () => JSON.stringify(view.getState().scene.nodes.map(n => [n.id, n.bounds])) +
      JSON.stringify(view.getState().scene.routes.map(r => [r.id, r.points]));
    const start = geometry();
    assert.equal((await view.open('box')).ok, true);
    const opened = geometry();
    assert.notEqual(opened, start);
    const back = await view.back();
    assert.equal(back.ok, true);
    assert.equal(back.relaidOut, false, 'restored from the kept snapshot');
    assert.equal(geometry(), start, 'every box and route where it was');
    const forth = await view.forward();
    assert.equal(forth.relaidOut, false);
    assert.equal(geometry(), opened);
    // A changed graph cannot come back from a snapshot of the earlier one: the decisions are laid out again.
    assert.equal((await view.apply({op: 'patch', patch: {nodes: [{id: 'extra', kind: 'ecu', label: 'extra'}]}}, 'Add', {remember: false})).ok, true);
    const relaid = await view.back();
    assert.equal(relaid.ok, true);
    assert.equal(relaid.relaidOut, true);
  });
}

test('keepGeometry: false lays the view out again from the decisions, as before', {skip: !engine}, async () => {
  const view = createViewController(createSession(engine, {resident: false}), {keepGeometry: false});
  assert.equal((await view.import(graph(), {limit: 100, options: {levelOfDetail: {depth: 0}}})).ok, true);
  assert.equal((await view.open('box')).ok, true);
  const back = await view.back();
  assert.equal(back.ok, true);
  assert.equal(back.relaidOut, true);
});
