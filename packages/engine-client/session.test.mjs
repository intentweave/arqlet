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

test('a neighbourhood is read in both kinds of session alike', {skip: !engine}, async () => {
  const g = {nodes: ['a', 'b', 'c', 'd'].map(id => ({id, kind: 'file', label: id})), edges: [
    {id: 'ab', kind: 'calls', source: 'a', target: 'b'}, {id: 'bc', kind: 'imports', source: 'b', target: 'c'},
    {id: 'db', kind: 'calls', source: 'd', target: 'b'}]};
  const answers = [];
  for (const resident of [true, false]) {
    const session = createSession(engine, {resident});
    await session.import(g);
    const n = await session.neighbourhood('b', {direction: 'out', hops: 2});
    assert.deepEqual(n.nodes, ['b', 'c']);
    assert.deepEqual((await session.neighbourhood(['a', 'd'], {hops: 1, edgeKinds: ['calls']})).nodes, ['a', 'b', 'd']);
    assert.equal((await session.neighbourhood('b', {direction: 'sideways'})).ok, false);
    answers.push(n);
    await session.close();
  }
  assert.deepEqual(answers[0], answers[1]);
});

test('an overview session that keeps the graph gets only the positions back and still answers whole overviews', {skip: !engine}, async () => {
  const sent = [];
  const counting = Object.assign(request => engine(request), {
    openSession: () => {
      const inner = engine.openSession();
      return {run: request => { const answer = inner.run(request); sent.push({asked: !!request.positionsOnly, whole: !!answer.overview?.ids}); return answer; }, close: () => inner.close()};
    },
  });
  const kept = createOverviewSession(counting);
  const plain = createOverviewSession(engine, {resident: false});
  const options = {communities: 0};
  await kept.open(graph, {options}); await plain.open(graph, {options});
  const id = kept.overview.ids[0];
  const request = {options, radii: {[id]: kept.overview.r[0] * 2}, start: {}};
  const a = await kept.layOut(request), b = await plain.layOut(request);
  assert.deepEqual(a, b, 'the same whole overview either way');
  assert.deepEqual(sent.at(-1), {asked: true, whole: false}, 'only the positions crossed');
});

test('an overview session passes edge data and places by id on, the same in both kinds', {skip: !engine}, async () => {
  const kept = createOverviewSession(engine);
  const sent = createOverviewSession(engine, {resident: false});
  const both = async step => {
    const [a, b] = [await step(kept), await step(sent)];
    assert.equal(a.ok, true, a.error);
    assert.deepEqual(a, b);
    return a.overview;
  };
  const options = {communities: 0};
  const first = await both(s => s.open(graph, {options}));
  assert.equal(first.edgeKind, undefined);
  const places = Object.fromEntries(first.ids.map((id, i) => [id, {x: first.x[i], y: first.y[i], r: first.r[i]}]));
  // Asked for later, the edges' data come with a whole answer; then positions alone keep them.
  const withData = await both(s => s.layOut({options, edgeData: true, start: {}}));
  assert.equal(withData.edgeKind.length, withData.edges.length / 2);
  const continued = await both(s => s.layOut({options, edgeData: true, start: {nodes: places}}));
  assert.deepEqual(continued.edgeKind, withData.edgeKind);
  assert.deepEqual(continued.continuation, {continued: graph.nodes.length, anew: 0, ignored: []});
  // What became of places belongs to the request that gave them; edge data to the requests that ask.
  const plain = await both(s => s.layOut({options, edgeData: true, start: {}}));
  assert.equal(plain.continuation, undefined);
  const without = await both(s => s.layOut({options, start: {}}));
  assert.equal(without.edgeKind, undefined);
  // A changed graph continues from places by id, in a session of either kind.
  const gone = graph.nodes.at(-1).id;
  const changed = {...graph, nodes: graph.nodes.slice(0, -1),
    edges: graph.edges.filter(e => e.source !== gone && e.target !== gone)};
  const next = await both(s => s.open(changed, {options, start: {nodes: places}}));
  assert.deepEqual(next.continuation.ignored, [gone]);
  await kept.close();
});

test('an overview session passes a work budget on and reports the work only to the request that asked', {skip: !engine}, async () => {
  const kept = createOverviewSession(engine);
  const sent = createOverviewSession(engine, {resident: false});
  const both = async step => {
    const [a, b] = [await step(kept), await step(sent)];
    assert.equal(a.ok, true, a.error);
    assert.deepEqual(a, b);
    return a.overview;
  };
  const options = {communities: 0};
  const first = await both(s => s.open(graph, {options}));
  assert.equal(first.work, undefined);
  const counted = await both(s => s.layOut({options, start: {}, workBudget: Number.MAX_SAFE_INTEGER}));
  assert.equal(counted.work.limited, false);
  assert.ok(counted.work.used > 0);
  // No work at all: valid discs, the containers packed; refusing instead leaves the session as it was.
  const packed = await both(s => s.layOut({options, workBudget: 0}));
  assert.equal(packed.work.limited, true);
  assert.equal(packed.stats.overlaps, 0);
  const refused = await kept.layOut({options, workBudget: 0, overBudget: 'refuse'});
  assert.equal(refused.ok, false);
  assert.match(refused.error, /^Overview work budget exhausted: /);
  // A later request without a budget has no report, also when only the positions came back.
  const plain = await both(s => s.layOut({options, start: {}}));
  assert.equal(plain.work, undefined);
  await kept.close();
});

test('an overview session fills packed containers one by one and reports the fill only to the request that asked', {skip: !engine}, async () => {
  const kept = createOverviewSession(engine);
  const sent = createOverviewSession(engine, {resident: false});
  const both = async step => {
    const [a, b] = [await step(kept), await step(sent)];
    assert.equal(a.ok, true, a.error);
    assert.deepEqual(a, b);
    return a.overview;
  };
  const options = {communities: 0};
  let o = await both(s => s.open(graph, {options, workBudget: 0}));
  const packed = o.work.packed.filter(c => c >= 0);
  assert.ok(packed.length > 0, JSON.stringify(o.work));
  for (const c of packed) {
    const next = await both(s => s.layOut({options, start: {fill: [o.ids[c]]}}));
    assert.deepEqual(next.fill.filled, [c]);
    assert.equal(next.stats.overlaps, 0);
    // Nothing outside the filled container moved.
    const inside = i => { for (let p = o.parent[i]; p >= 0; p = o.parent[p]) if (p === c) return true; return false; };
    for (let i = 0; i < o.ids.length; i++) {
      if (!inside(i)) assert.deepEqual([next.x[i], next.y[i], next.r[i]], [o.x[i], o.y[i], o.r[i]], o.ids[i]);
    }
    o = next;
  }
  const plain = await both(s => s.layOut({options, start: {}}));
  assert.equal(plain.fill, undefined);
  await kept.close();
});

test('an overview continued by id keeps its clusters over later layouts, in both kinds of session', {skip: !engine}, async () => {
  // Four dense rings without containers: communities. With the first ring last, clustering anew names them otherwise.
  const ids = Array.from({length: 160}, (_, i) => `r${Math.floor(i / 40)}n${i % 40}`);
  const rings = {nodes: ids.map(id => ({id, kind: 'fn', label: id})), edges: ids.flatMap((id, i) => [1, 2, 3].map(d => (
    {id: `${id}-${d}`, kind: 'call', source: id, target: ids[i - i % 40 + (i + d) % 40]})))};
  const turned = {...rings, nodes: [...rings.nodes.slice(40), ...rings.nodes.slice(0, 40)]};
  const kept = createOverviewSession(engine);
  const sent = createOverviewSession(engine, {resident: false});
  const both = async step => {
    const [a, b] = [await step(kept), await step(sent)];
    assert.equal(a.ok, true, a.error);
    assert.deepEqual(a, b);
    return a.overview;
  };
  const options = {communities: 60};
  const first = await both(s => s.open(rings, {options}));
  const places = Object.fromEntries(first.ids.map((id, i) => [id, {x: first.x[i], y: first.y[i], r: first.r[i]}]));
  const continued = await both(s => s.open(turned, {options, start: {nodes: places}}));
  const later = await both(s => s.layOut({options, start: {}}));
  assert.deepEqual(later.ids, continued.ids);
  assert.deepEqual(later.parent, continued.parent);
  const again = await both(s => s.layOut({options, start: {settle: true}}));
  assert.deepEqual(again.parent, continued.parent);
  // A cluster set smaller than its content keeps its members, in both kinds: the parents go with the places.
  const community = again.ids[again.parent[0]];
  const radii = {[community]: again.r[again.parent[0]] / 3};
  await both(s => s.layOut({options, radii, start: {}}));
  const clipped = await both(s => s.layOut({options, radii, start: {}}));
  assert.deepEqual(clipped.parent, continued.parent);
  const anew = await both(s => s.layOut({options}));
  assert.notDeepEqual(anew.parent, continued.parent);
  await kept.close();
});

test('a compacting filter lays the view out again without the hidden, in both kinds of session', {skip: !engine}, async () => {
  const g = {nodes: [{id: 'p', kind: 'package', label: 'p'}, {id: 'q', kind: 'package', label: 'q'},
    ...['a', 'b'].map(id => ({id, kind: 'file', label: id, parentId: 'p'})),
    ...['c', 'd'].map(id => ({id, kind: 'file', label: id, parentId: 'q'}))],
  edges: [{id: 'ab', kind: 'calls', source: 'a', target: 'b'}, {id: 'cd', kind: 'calls', source: 'c', target: 'd'}]};
  const scenes = [];
  for (const resident of [true, false]) {
    const session = createSession(engine, {resident});
    await session.import(g);
    const full = await session.scene();
    assert.equal((await session.filter({nodes: ['q'], compact: true})).ok, true);
    const compact = await session.scene();
    assert.ok(!compact.nodes.some(n => n.id === 'q' || n.id === 'c'));
    assert.ok(compact.bounds.width * compact.bounds.height < full.bounds.width * full.bounds.height);
    await session.filter({});
    assert.ok((await session.scene()).nodes.some(n => n.id === 'd'), 'everything back');
    scenes.push(compact);
    await session.close();
  }
  assert.deepEqual(scenes[0], scenes[1]);
});

test('an overview session traces on its graph, resident or sending it', {skip: !engine}, async () => {
  const g = {nodes: [{id: 'p', kind: 'package', label: 'p'}, ...['a', 'b', 'c'].map(id => ({id, kind: 'file', label: id, parentId: 'p'}))],
    edges: [{id: 'ab', kind: 'calls', source: 'a', target: 'b'}, {id: 'bc', kind: 'imports', source: 'b', target: 'c'}]};
  const answers = [];
  for (const resident of [true, false]) {
    const o = createOverviewSession(engine, {resident});
    assert.equal((await o.trace('a', 'c')).ok, false, 'nothing opened yet');
    await o.open(g);
    const t = await o.trace('a', 'c');
    assert.equal(t.ok, true, t.error);
    assert.deepEqual(t.edges, ['ab', 'bc']);
    assert.equal((await o.trace('a', 'c', {edgeKinds: ['calls']})).found, false);
    const n = await o.neighbourhood('b', {hops: 1, direction: 'out'});
    assert.deepEqual(n.nodes, ['b', 'c']);
    answers.push([t, n]);
    await o.close();
  }
  assert.deepEqual(answers[0], answers[1]);
});

test('a session patches the graph without a new import and keeps its view', {skip: !engine}, async () => {
  const g = {nodes: [{id: 'p', kind: 'package', label: 'p'}, {id: 'q', kind: 'package', label: 'q'},
    ...['a', 'b'].map(id => ({id, kind: 'file', label: id, parentId: 'p'})), {id: 'c', kind: 'file', label: 'c', parentId: 'q'}],
  edges: [{id: 'pq', kind: 'calls', source: 'p', target: 'q', weight: 2}]};
  const states = [];
  for (const resident of [true, false]) {
    const session = createSession(engine, {resident});
    await session.import(g, {options: {levelOfDetail: {depth: 0}}});
    await session.expand('p');
    const r = await session.patch({edges: [{id: 'ac', kind: 'calls', source: 'a', target: 'c'}], removeEdges: ['pq']});
    assert.equal(r.ok, true, r.error);
    assert.equal(session.isCollapsed('q'), true, 'q stays collapsed');
    assert.ok((await session.scene()).routes.some(x => x.id === 'ac'));
    assert.equal((await session.patch({removeEdges: ['nowhere']})).ok, false);
    states.push(session.snapshot.collapsedIds);
    await session.close();
  }
  assert.deepEqual(states[0], states[1]);
});

for (const resident of [false, true]) {
  test(`a drag moves a node step by step, drops it pinned, and pin/unpin work (resident ${resident})`, {skip: !engine || (resident && !engine.openSession)}, async () => {
    const changes = [];
    const session = createSession(engine, {resident, onChange: s => changes.push(s.layoutVersion)});
    assert.equal((await session.import(graph)).ok, true);
    const boundsOf = async id => (await session.scene()).nodes.find(n => n.id === id).bounds;
    const parent = await boundsOf('root');
    const start = await boundsOf('a');
    const rel = {x: start.x - parent.x, y: start.y - parent.y};
    const handle = session.drag('a', rel);
    assert.equal((await handle.begun).ok, true, (await handle.begun).error);
    // Three positions at once: the ones overtaken before they were sent are dropped, the newest is applied.
    const moves = [1, 2, 3].map(k => handle.move({x: rel.x + 40 * k, y: rel.y + 10 * k}));
    for (const m of await Promise.all(moves)) assert.equal(m.ok, true, m.error);
    assert.ok(handle.stats.coalesced >= 1, JSON.stringify(handle.stats));
    const dropped = await handle.drop();
    assert.equal(dropped.ok, true, dropped.error);
    assert.equal(handle.active, false);
    const exported = await session.exportSnapshot();
    assert.deepEqual(exported.options.pins.a, {x: rel.x + 120, y: rel.y + 30}, 'pinned where it was dropped');
    // The drawing follows the pin within the parent's frame.
    const now = await boundsOf('a'), p2 = await boundsOf('root');
    assert.deepEqual({x: now.x - p2.x, y: now.y - p2.y}, {x: rel.x + 120, y: rel.y + 30});
    // Unpin, then pin where it is.
    assert.equal((await session.unpin('a')).ok, true);
    assert.equal((await session.exportSnapshot()).options.pins?.a, undefined);
    assert.equal((await session.pin('a')).ok, true);
    assert.ok((await session.exportSnapshot()).options.pins.a);
    // After a drop the handle takes nothing more; a cancelled drag puts the node back.
    assert.equal((await handle.move({x: 0, y: 0})).ok, false);
    const before = await boundsOf('b');
    const pb = await boundsOf('root');
    const other = session.drag('b', {x: before.x - pb.x + 60, y: before.y - pb.y});
    await other.begun;
    assert.equal((await other.cancel()).ok, true);
    const after = await boundsOf('b'), pa = await boundsOf('root');
    assert.deepEqual({x: after.x - pa.x, y: after.y - pa.y}, {x: before.x - pb.x, y: before.y - pb.y});
    assert.ok(changes.length >= 6);
    await session.close();
  });
}

test('query: rows over the source graph and the content, in a resident and a plain session', {skip: !engine}, async () => {
  const resident = createSession(engine, {resident: engine.openSession ? true : false});
  await resident.import(graph);
  const first = await resident.query('MATCH (n) RETURN count(*)');
  // An engine without the cargo feature `query` refuses it; an older shipped engine does not know the operation at all.
  if (!first.ok && /not built into|expected struct Snapshot/.test(first.error)) return;
  assert.equal(first.ok, true, first.error);
  assert.equal(first.rows[0][0], first.graph.nodes);
  if (engine.openSession) {
    assert.equal((await resident.payload({set: {nodes: {a: {loc: 5}}}})).ok, true);
    const q = await resident.query('MATCH (n) WHERE n.loc > $min RETURN n', {params: {min: 1}, view: true});
    assert.deepEqual(q.rows, [[{node: 'a'}]]);
    assert.equal(q.contentRevision, 1);
    assert.deepEqual(Object.keys(q.shown.nodes), ['a']);
  }
  const bad = await resident.query('MATCH (n RETURN n');
  assert.equal(bad.ok, false);
  assert.match(bad.error, /^query syntax:/);
  assert.equal((await createSession(engine).query('RETURN 1')).ok, false, 'nothing imported yet');
});
