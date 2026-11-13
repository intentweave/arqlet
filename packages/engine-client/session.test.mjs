import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import {createSession, createOverviewSession, workerRun, httpRun} from './session.js';
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

test('a session traces a path and reveals it', {skip: !engine}, async () => {
  const session = createSession(engine);
  await session.import(graph);
  await session.collapse('root');
  const path = await session.trace('a', 'b');
  assert.equal(path.ok, true, path.error);
  assert.equal(path.found, true);
  assert.deepEqual(path.visible.nodes, ['root'], 'both ends are inside the collapsed root');
  const shown = await session.reveal(path.nodes);
  assert.equal(shown.ok, true, shown.error);
  assert.equal(session.isCollapsed('root'), false);
  const again = await session.trace('a', 'b');
  assert.ok(again.visible.nodes.includes('a') && again.visible.nodes.includes('b'));
});

test('a session hides by id and kind and keeps the place of the rest', {skip: !engine}, async () => {
  for (const resident of [false, true]) {
    const session = createSession(engine, {resident});
    await session.import(graph);
    const before = await session.scene();
    const box = scene => Object.fromEntries(scene.nodes.map(n => [n.id, n.bounds]));
    const hidden = await session.filter({nodes: ['b']});
    assert.equal(hidden.ok, true, hidden.error);
    const after = await session.scene();
    assert.ok(!after.nodes.some(n => n.id === 'b'), 'b is hidden');
    for (const [id, bounds] of Object.entries(box(after))) assert.deepEqual(bounds, box(before)[id], `${id} keeps its place`);
    assert.equal((await session.filter({})).ok, true);
    assert.deepEqual(box(await session.scene()), box(before), 'an empty filter shows everything again');
  }
});

test('a resident session gives the same scenes and snapshot as one that sends the snapshot', {skip: !engine}, async () => {
  const sent = createSession(engine, {resident: false});
  const kept = createSession(engine);
  assert.equal(kept.resident, true, 'resident by default where the engine can');
  assert.equal(sent.resident, false);
  for (const session of [sent, kept]) {
    assert.equal((await session.import(graph)).ok, true);
  }
  assert.deepEqual(await kept.scene(), await sent.scene(), 'after the import');
  for (const step of [s => s.collapse('root'), s => s.expand('root'), s => s.apply({op: 'direction', nodeId: null, direction: 'tb'})]) {
    const a = await step(sent), b = await step(kept);
    assert.equal(a.ok, true, a.error);
    assert.equal(b.ok, true, b.error);
    assert.deepEqual(await kept.scene(), await sent.scene());
    assert.equal(kept.snapshot.layoutVersion, sent.snapshot.layoutVersion);
    assert.deepEqual(kept.snapshot.collapsedIds, sent.snapshot.collapsedIds);
  }
  assert.equal(kept.snapshot.graph, undefined, 'the state only: the snapshot stays in the engine');
  assert.deepEqual(await kept.exportSnapshot(), sent.snapshot);
  const [ta, tb] = [await sent.trace('a', 'b'), await kept.trace('a', 'b')];
  assert.deepEqual(tb, ta);
  await kept.close();
});

test('a resident session refuses a stale change and keeps its view', {skip: !engine}, async () => {
  const session = engine.openSession();
  const imported = session.run({op: 'import', graph, limit: 100, scene: false});
  assert.equal(imported.ok, true);
  assert.equal(imported.scene, undefined, 'scene: false leaves it out');
  const stale = session.run({op: 'toggle', nodeId: 'root', expectedLayoutVersion: 5});
  assert.deepEqual(stale, {ok: false, error: 'Stale view request'});
  const fine = session.run({op: 'toggle', nodeId: 'root'});
  assert.equal(fine.ok, true);
  assert.deepEqual(fine.collapsedIds, ['root']);
  session.close();
  assert.deepEqual(session.run({op: 'export'}), {ok: false, error: 'Unknown session'});
});

test('workerRun opens a session in the worker and sends its requests with the handle', async () => {
  const listeners = [];
  const posted = [];
  const worker = {addEventListener: (type, f) => listeners.push(f), postMessage: m => { posted.push(m); queueMicrotask(() => answer(m)); }};
  const answer = m => {
    const reply = m.session === 'open' ? {ok: true, session: 7} : m.close ? {ok: true} : {ok: true, echo: m.request, session: m.session};
    listeners.forEach(f => f({data: {requestId: m.requestId, ...reply}}));
  };
  const run = workerRun(worker);
  const session = await run.openSession();
  assert.deepEqual(await session.run({op: 'scene'}), {ok: true, echo: {op: 'scene'}, session: 7});
  await session.close();
  assert.deepEqual(posted.map(m => [m.session, m.request?.op ?? null, m.close ?? false]), [['open', null, false], [7, 'scene', false], [7, null, true]]);
});

test('without sessions in the engine, a session sends the snapshot; resident: true then refuses', () => {
  const run = request => ({ok: true, snapshot: {revision: 0, layoutVersion: 0, collapsedIds: []}, request});
  assert.equal(createSession(run).resident, false);
  assert.throws(() => createSession(run, {resident: true}), /no resident sessions/);
});

test('httpRun sends operations and session messages to the backend', async () => {
  const sent = [];
  const fetchImpl = async (url, init) => {
    const body = JSON.parse(init.body);
    sent.push([url, body]);
    const answer = url.endsWith('/op') ? {ok: true, echo: body.op}
      : body.session === 'open' ? {ok: true, session: 'k1'} : body.close ? {ok: true} : {ok: true, ran: body.request.op, in: body.session};
    return {status: 200, json: async () => answer};
  };
  const run = httpRun('/api', {fetchImpl});
  assert.deepEqual(await run({op: 'optionDefaults'}), {ok: true, echo: 'optionDefaults'});
  const session = await run.openSession();
  assert.deepEqual(await session.run({op: 'toggle', nodeId: 'a'}), {ok: true, ran: 'toggle', in: 'k1'});
  await session.close();
  assert.deepEqual(sent.map(([u, b]) => [u, b.session ?? null, b.close ?? false]),
    [['/api/op', null, false], ['/api/session', 'open', false], ['/api/session', 'k1', false], ['/api/session', 'k1', true]]);
  assert.equal(createSession(run).resident, true, 'a backend with sessions is resident by default');
});

test('an engine can be created from a module compiled by the host', {skip: !engine}, async () => {
  const module = await WebAssembly.compile(readFileSync(wasm));
  const run = await createEngine(module);
  assert.equal(run({op: 'optionDefaults'}).ok, true);
});


test('an overview session keeps the graph in the engine and answers as one that sends it', {skip: !engine}, async () => {
  const kept = createOverviewSession(engine);
  const sent = createOverviewSession(engine, {resident: false});
  assert.equal(kept.resident, true);
  assert.equal((await kept.layOut()).ok, false, 'nothing to lay out before open');
  const options = {communities: 0};
  const first = [await kept.open(graph, {options}), await sent.open(graph, {options})];
  assert.equal(first[0].ok, true, first[0].error);
  assert.deepEqual(first[0], first[1]);
  const id = first[0].overview.ids.find(i => i === 'root') ?? first[0].overview.ids[0];
  const r = first[0].overview.r[first[0].overview.ids.indexOf(id)];
  const request = {options, radii: {[id]: r * 2}, start: {settle: true}};
  const grown = [await kept.layOut(request), await sent.layOut(request)];
  assert.equal(grown[0].ok, true, grown[0].error);
  assert.deepEqual(grown[0], grown[1], 'a continuation from the last positions, the same either way');
  assert.equal(kept.overview, grown[0].overview);
  const other = {options: {communities: 3}, start: {}};
  const refused = [await kept.layOut(other), await sent.layOut(other)];
  assert.equal(refused[0].ok, false);
  assert.equal(refused[0].error, refused[1].error, 'other clusters cannot continue');
  // A new open overtakes a layout still under way.
  const overtaken = kept.layOut({options});
  const reopened = kept.open(graph, {options});
  assert.equal((await overtaken).stale, true);
  assert.equal((await reopened).ok, true);
  await kept.close();
});

test('a trace follows only the edge kinds asked for', {skip: !engine}, async () => {
  const kinds = {nodes: ['a', 'b', 'c'].map(id => ({id, kind: 'file', label: id})), edges: [
    {id: 'ab', kind: 'calls', source: 'a', target: 'b'}, {id: 'bc', kind: 'imports', source: 'b', target: 'c'}]};
  for (const resident of [true, false]) {
    const session = createSession(engine, {resident});
    await session.import(kinds);
    assert.deepEqual((await session.trace('a', 'c')).edges, ['ab', 'bc']);
    assert.equal((await session.trace('a', 'c', {edgeKinds: ['calls']})).found, false);
    assert.deepEqual((await session.trace('a', 'b', {edgeKinds: ['calls']})).edges, ['ab']);
    await session.close();
  }
});
