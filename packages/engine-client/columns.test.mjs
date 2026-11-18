import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import {createSession, createOverviewSession, workerRun} from './session.js';
import {createEngine} from './wasm-client.js';
import {columnsOfScene, NONE} from './columns.js';

const candidates = ['./arqlet.wasm', '../../target/wasm32-unknown-unknown/release/arqlet_wasm.wasm', '../../target/wasm32-unknown-unknown/fast/arqlet_wasm.wasm']
  .map(p => new URL(p, import.meta.url)).filter(existsSync);
// A build that exports the scene's and the overview's columns (an older shipped or release build may not).
const offersColumns = url => {
  const names = WebAssembly.Module.exports(new WebAssembly.Module(readFileSync(url))).map(x => x.name);
  return names.includes('lw_session_column') && names.includes('lw_session_overview_column');
};
// The first such build.
let engine = null;
for (const wasm of candidates) {
  const e = await createEngine(readFileSync(wasm));
  if (e.openSession && offersColumns(wasm)) { engine = e; break; }
}
const graph = JSON.parse(readFileSync(new URL('../../examples/fixtures/small.json', import.meta.url), 'utf8'));
const names = ['nodeBounds', 'nodeParent', 'nodeFlags', 'routeSource', 'routeTarget', 'routeWeight', 'routeFlags', 'routeOffsets', 'routePoints'];

test('the engine\'s columns equal the columns built from the scene', {skip: !engine}, async () => {
  const session = createSession(engine);
  await session.import(graph);
  const fromEngine = await session.columns();
  const fromScene = columnsOfScene(await session.scene());
  assert.ok(fromScene.nodeIds.length > 0 && fromScene.routeIds.length > 0);
  assert.deepEqual(fromEngine.nodeIds, fromScene.nodeIds);
  assert.deepEqual(fromEngine.routeIds, fromScene.routeIds);
  assert.equal(fromEngine.revision, fromScene.revision);
  assert.equal(fromEngine.layoutVersion, fromScene.layoutVersion);
  for (const name of names) {
    assert.equal(fromEngine[name].constructor, fromScene[name].constructor, name);
    assert.deepEqual(Array.from(fromEngine[name]), Array.from(fromScene[name]), name);
  }
  assert.equal(fromEngine.routeOffsets.length, fromEngine.routeIds.length + 1);
  assert.equal(fromEngine.routeOffsets.at(-1) * 2, fromEngine.routePoints.length);
  assert.ok(fromEngine.nodeParent.includes(NONE), 'a root has no parent');
});

test('the engine\'s arrays are views into its memory, and a copy survives the next change', {skip: !engine}, async () => {
  const session = createSession(engine);
  await session.import(graph);
  const before = await session.columns();
  assert.ok(before.nodeBounds.buffer instanceof ArrayBuffer && before.nodeBounds.buffer.byteLength > before.nodeBounds.byteLength,
    'a view into the whole memory, not a copy of the column');
  const kept = before.nodeBounds.slice();
  await session.toggle('root');
  const after = await session.columns();
  assert.notEqual(after.layoutVersion, before.layoutVersion);
  assert.ok(after.nodeIds.length < before.nodeIds.length, 'a collapsed container shows fewer boxes');
  assert.equal(kept.length, before.nodeIds.length * 4, 'the copy kept the old view');
});

test('without a resident session the columns are built from the scene', {skip: !engine}, async () => {
  const session = createSession(engine, {resident: false});
  await session.import(graph);
  const columns = await session.columns();
  assert.deepEqual(columns, columnsOfScene(await session.scene()));
});

test('the worker hands the columns over as transferred copies', {skip: !engine}, async () => {
  const file = candidates.find(offersColumns);
  const bytes = readFileSync(file);
  const posted = [];
  const listeners = [];
  globalThis.self = {
    location: {href: 'http://host/worker.js?wasm=engine.wasm'},
    postMessage: (message, transfer) => { posted.push({message, transfer}); listeners.forEach(l => l({data: message})); },
  };
  globalThis.fetch = async () => ({ok: true, arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)});
  await import('./worker.js');
  // The page side: a worker object that hands messages to the worker's handler.
  const worker = {addEventListener: (_, l) => listeners.push(l), postMessage: data => globalThis.self.onmessage({data})};
  const session = createSession(workerRun(worker));
  await session.import(graph);
  const columns = await session.columns();
  const reference = columnsOfScene(await session.scene());
  for (const name of names) assert.deepEqual(Array.from(columns[name]), Array.from(reference[name]), name);
  const sent = posted.findLast(p => p.message.columns);
  assert.equal(sent.transfer.length, names.length, 'every column travels as a transferred buffer');
  assert.equal(columns.nodeBounds.buffer.byteLength, columns.nodeBounds.byteLength, 'a copy of the column, not the whole memory');
  // The overview through the worker: typed arrays that were transferred, equal to the JSON arrays of a plain session.
  const options = {communities: 0};
  const viaWorker = createOverviewSession(workerRun(worker), {columns: true});
  const direct = createOverviewSession(engine);
  const w = await viaWorker.open(overviewGraph, {options});
  const p = await direct.open(overviewGraph, {options});
  assert.equal(w.ok, true, JSON.stringify(w).slice(0, 200));
  for (const key of ['x', 'y', 'r', 'parent', 'kind', 'edges', 'links']) {
    assert.ok(ArrayBuffer.isView(w.overview[key]), key);
    assert.deepEqual(Array.from(w.overview[key]), p.overview[key], key);
  }
  const moved = posted.findLast(m => m.message.overview);
  assert.ok(moved.transfer.length >= 7, 'the overview\'s numbers were transferred');
  const w2 = await viaWorker.layOut({options, radii: {g1: 120}, start: {}});
  const p2 = await direct.layOut({options, radii: {g1: 120}, start: {}});
  assert.deepEqual(Array.from(w2.overview.x), p2.overview.x);
  assert.equal(posted.findLast(m => m.message.overview).transfer.length, 3, 'a continuation moves x, y and r only');
  delete globalThis.self;
});

// A small hierarchical graph for the overview: groups of files of symbols, with calls between them.
const overviewGraph = (() => {
  const nodes = [], edges = [];
  for (let g = 0; g < 4; g++) {
    nodes.push({id: `g${g}`, kind: 'directory', label: `g${g}`});
    for (let f = 0; f < 3; f++) {
      nodes.push({id: `g${g}/f${f}`, kind: 'file', label: `f${f}`, parentId: `g${g}`});
      for (let s = 0; s < 5; s++) nodes.push({id: `g${g}/f${f}/n${s}`, kind: 'symbol', label: `n${s}`, parentId: `g${g}/f${f}`});
    }
  }
  const leaves = nodes.filter(n => n.kind === 'symbol');
  leaves.forEach((n, i) => { edges.push({id: `e${i}`, kind: 'calls', source: n.id, target: leaves[(i * 7 + 3) % leaves.length].id}); });
  return {nodes, edges};
})();
const overviewEngine = engine && engine.openSession().overview ? engine : null;

test('a columnar overview session gives the same overviews as one with JSON arrays', {skip: !overviewEngine}, async () => {
  const plain = createOverviewSession(overviewEngine);
  const typed = createOverviewSession(overviewEngine, {columns: true});
  const options = {communities: 0};
  const a = await plain.open(overviewGraph, {options});
  const b = await typed.open(overviewGraph, {options});
  assert.equal(a.ok && b.ok, true, JSON.stringify(b).slice(0, 200));
  const same = (p, t) => {
    for (const key of ['x', 'y', 'r', 'parent', 'kind', 'edges', 'links']) {
      assert.ok(ArrayBuffer.isView(t[key]), `${key} is a typed array`);
      assert.deepEqual(Array.from(t[key]), p[key], key);
    }
    for (const key of ['ids', 'labels', 'kinds', 'rootRadius', 'stats', 'representation', 'schemaVersion']) assert.deepEqual(t[key], p[key], key);
  };
  same(a.overview, b.overview);
  // The viewer draws the same from either: model and vertex buffers are equal.
  const {overviewModel, overviewBuffers} = await import('../viewer/overview.js');
  const edgeIds = overviewGraph.edges.map(e => e.id);
  const fromPlain = overviewModel(a.overview, edgeIds), fromTyped = overviewModel(b.overview, edgeIds);
  assert.deepEqual(Array.from(fromTyped.chainTop), Array.from(fromPlain.chainTop));
  assert.deepEqual(Array.from(fromTyped.cluster), Array.from(fromPlain.cluster));
  const bufferOf = m => Object.fromEntries(Object.entries(overviewBuffers(m)).map(([k, v]) => [k, ArrayBuffer.isView(v) ? Array.from(v) : v]));
  assert.deepEqual(bufferOf(fromTyped), bufferOf(fromPlain));
  // A continuation with new radii: only what moves comes back, merged into a whole overview.
  const next = {options, radii: {g1: 120}, start: {}};
  const c = await plain.layOut(next);
  const d = await typed.layOut(next);
  assert.equal(c.ok && d.ok, true);
  same(c.overview, d.overview);
  assert.equal(typed.overview.parent.buffer.byteLength, typed.overview.parent.byteLength, 'copies, not views into the memory');
  // Positions of a typed overview go back as a start without becoming objects in JSON.
  const e = await typed.open(overviewGraph, {options, start: {x: d.overview.x, y: d.overview.y, r: d.overview.r}});
  assert.equal(e.ok, true, JSON.stringify(e).slice(0, 200));
  await plain.close();
  await typed.close();
});

test('columns: true without an engine that offers them says so', async () => {
  const run = async () => ({ok: true});
  run.openSession = async () => ({run, close() {}});
  const typed = createOverviewSession(run, {columns: true});
  const answer = await typed.open(overviewGraph);
  assert.equal(answer.ok, false);
  assert.match(answer.error, /columnar overview/);
});

test('a session whose engine has no columns (a backend) falls back to the scene', async () => {
  const scene = {revision: 0, layoutVersion: 0, nodes: [
    {id: 'a', bounds: {x: 0, y: 0, width: 10, height: 5}, container: true},
    {id: 'b', parentId: 'a', bounds: {x: 1, y: 1, width: 2, height: 2}, collapsed: true},
  ], routes: [{id: 'e', source: 'b', target: 'outside', weight: 3, rounded: true, points: [{x: 1, y: 1}, {x: 4, y: 2.5}]}]};
  const run = async request => ({ok: true, revision: 0, layoutVersion: 0, collapsedIds: [], scene});
  run.openSession = async () => ({run, close() {}});
  const session = createSession(run);
  await session.import({nodes: [], edges: []});
  const c = await session.columns();
  assert.deepEqual(c.nodeIds, ['a', 'b']);
  assert.deepEqual(Array.from(c.nodeParent), [NONE, 0]);
  assert.deepEqual(Array.from(c.nodeFlags), [1, 4]);
  assert.deepEqual(Array.from(c.routeSource), [1]);
  assert.deepEqual(Array.from(c.routeTarget), [NONE], 'an end outside the scene has no index');
  assert.deepEqual(Array.from(c.routeFlags), [2]);
  assert.deepEqual(Array.from(c.routeOffsets), [0, 2]);
  assert.deepEqual(Array.from(c.routePoints), [1, 1, 4, 2.5]);
});
