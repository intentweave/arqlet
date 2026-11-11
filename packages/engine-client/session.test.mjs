import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import {createSession, workerRun} from './session.js';
import {createEngine} from './wasm-client.js';

// The engine shipped beside this module, or a release build of the workspace.
const shipped = new URL('./arqlet.wasm', import.meta.url);
const built = new URL('../../target/wasm32-unknown-unknown/release/arqlet_wasm.wasm', import.meta.url);
const wasm = existsSync(shipped) ? shipped : built;
const engine = existsSync(wasm) ? await createEngine(readFileSync(wasm)) : null;
const graph = JSON.parse(readFileSync(new URL('../../examples/fixtures/small.json', import.meta.url), 'utf8'));

test('a session imports, collapses and expands a container, each time on the newest snapshot', {skip: !engine}, async () => {
  const seen = [];
  const session = createSession(engine, {onChange: s => seen.push(s.layoutVersion)});
  assert.equal((await session.import(graph)).ok, true);
  assert.equal(session.isCollapsed('root'), false);
  const closed = await session.collapse('root');
  assert.equal(closed.ok, true, closed.error);
  assert.equal(session.isCollapsed('root'), true);
  assert.equal((await session.collapse('root')).unchanged, true, 'collapsed already: no engine call');
  const open = await session.expand('root');
  assert.equal(open.ok, true, open.error);
  assert.equal(session.isCollapsed('root'), false);
  assert.deepEqual(seen, [0, 1, 2], 'one change per applied answer');
  const scene = await session.scene();
  assert.ok(scene.nodes.some(n => n.id === 'a'), 'the expanded container shows its children');
  assert.equal(await session.scene(), scene, 'one scene per snapshot');
});

test('changes sent at once run one after the other, each on the snapshot the previous one returned', {skip: !engine}, async () => {
  const session = createSession(engine);
  await session.import(graph);
  const results = await Promise.all([session.toggle('root'), session.toggle('root'), session.toggle('root')]);
  assert.deepEqual(results.map(r => r.ok), [true, true, true], 'none is refused as stale');
  assert.equal(session.isCollapsed('root'), true);
});

test('a change overtaken by a new import is reported stale and not applied', async () => {
  let release;
  const gate = new Promise(r => { release = r; });
  // A fake engine whose toggle answers late.
  const run = async request => {
    if (request.op === 'import') return {ok: true, snapshot: {revision: 0, layoutVersion: 0, collapsedIds: [], marker: request.graph.name}};
    await gate;
    return {ok: true, snapshot: {...request.snapshot, layoutVersion: 1, marker: 'toggled'}};
  };
  const session = createSession(run);
  await session.import({name: 'first', nodes: [], edges: []});
  const toggled = session.toggle('x');
  const second = session.import({name: 'second', nodes: [], edges: []});
  release();
  assert.deepEqual(await toggled, {ok: false, stale: true, error: 'The session was reset.'});
  assert.equal((await second).ok, true);
  assert.equal(session.snapshot.marker, 'second', 'the old toggle did not overwrite the new import');
});

test('every change carries the snapshot and its expected versions', async () => {
  const requests = [];
  const run = request => {
    requests.push(request);
    return {ok: true, snapshot: {revision: 3, layoutVersion: requests.length, collapsedIds: []}};
  };
  const session = createSession(run);
  assert.equal((await session.toggle('x')).error, 'Nothing imported yet.');
  await session.import({nodes: [], edges: []}, {limit: 7, options: {isolatedGrid: true}});
  assert.equal(requests[0].limit, 7);
  assert.deepEqual(requests[0].options, {isolatedGrid: true});
  await session.apply({op: 'direction', nodeId: null, direction: 'tb'});
  const [, direction] = requests;
  assert.equal(direction.op, 'direction');
  assert.equal(direction.expectedRevision, 3);
  assert.equal(direction.expectedLayoutVersion, 1);
  assert.equal(direction.snapshot.layoutVersion, 1);
});

test('workerRun matches answers to requests by id and passes publications on', async () => {
  const listeners = [];
  const posted = [];
  const worker = {
    addEventListener: (type, f) => listeners.push(f),
    postMessage: message => posted.push(message),
  };
  const publications = [];
  const run = workerRun(worker, {onPublication: (id, m) => publications.push([id, m.publication])});
  const a = run({op: 'scene'});
  const b = run({op: 'import'});
  const emit = data => listeners.forEach(f => f({data}));
  emit({requestId: posted[1].requestId, ok: true, which: 'b'});
  emit({requestId: posted[0].requestId, publication: 1, emittedAt: 0, bytes: 3});
  emit({requestId: posted[0].requestId, ok: true, which: 'a'});
  assert.deepEqual(await a, {ok: true, which: 'a'});
  assert.deepEqual(await b, {ok: true, which: 'b'});
  assert.deepEqual(publications, [[posted[0].requestId, 1]]);
});
