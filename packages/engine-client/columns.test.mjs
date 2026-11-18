import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import {createSession} from './session.js';
import {createEngine} from './wasm-client.js';
import {columnsOfScene, NONE} from './columns.js';

const candidates = ['./arqlet.wasm', '../../target/wasm32-unknown-unknown/release/arqlet_wasm.wasm', '../../target/wasm32-unknown-unknown/fast/arqlet_wasm.wasm']
  .map(p => new URL(p, import.meta.url)).filter(existsSync);
// The newest build that has the columns export (an older shipped engine does not).
let engine = null;
for (const wasm of candidates) {
  const e = await createEngine(readFileSync(wasm));
  if (e.openSession && WebAssembly.Module.exports(new WebAssembly.Module(readFileSync(wasm))).some(x => x.name === 'lw_session_column')) { engine = e; break; }
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
  const file = candidates.find(u => WebAssembly.Module.exports(new WebAssembly.Module(readFileSync(u))).some(x => x.name === 'lw_session_column'));
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
  const {workerRun} = await import('./session.js');
  const session = createSession(workerRun(worker));
  await session.import(graph);
  const columns = await session.columns();
  const reference = columnsOfScene(await session.scene());
  for (const name of names) assert.deepEqual(Array.from(columns[name]), Array.from(reference[name]), name);
  const sent = posted.findLast(p => p.message.columns);
  assert.equal(sent.transfer.length, names.length, 'every column travels as a transferred buffer');
  assert.equal(columns.nodeBounds.buffer.byteLength, columns.nodeBounds.byteLength, 'a copy of the column, not the whole memory');
  delete globalThis.self;
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
