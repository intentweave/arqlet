// The two engine modules of the package (#210): the default one refuses the operation `query`; the one built with the cargo
// feature `query` (`arqlet-engine-query.wasm`) answers it, in a resident session, in a plain one, and through the worker.
// Needs both files beside this module (`sh apps/playground/build.sh` or `pnpm pack` copies them), else the tests are skipped.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {defaultWasmUrl, queryWasmUrl, loadEngine} from './engine.js';
import {createSession, workerRun} from './session.js';

const hasDefault = existsSync(defaultWasmUrl);
const hasQuery = existsSync(queryWasmUrl);
const graph = JSON.parse(readFileSync(new URL('../../examples/fixtures/small.json', import.meta.url), 'utf8'));

test('the module names: the default stays arqlet.wasm, the query module is a file of its own', () => {
  assert.match(fileURLToPath(defaultWasmUrl), /[/\\]arqlet\.wasm$/);
  assert.match(fileURLToPath(queryWasmUrl), /[/\\]arqlet-engine-query\.wasm$/);
});

test('loadEngine selects the module: none or a URL is that one, {query: true} the query module, {url} a module of the host', async () => {
  const asked = [];
  const bytes = async url => { asked.push(String(url)); throw Error('stop'); };
  for (const which of [undefined, {}, {query: false}, {query: true}, {url: 'http://host/own.wasm'}, 'http://host/s.wasm', new URL('http://host/u.wasm')]) {
    await assert.rejects(loadEngine(which, bytes), /stop/);
  }
  assert.deepEqual(asked, [String(defaultWasmUrl), String(defaultWasmUrl), String(defaultWasmUrl), String(queryWasmUrl),
    'http://host/own.wasm', 'http://host/s.wasm', 'http://host/u.wasm']);
});

test('the default engine refuses query, as before', {skip: !hasDefault}, async () => {
  const run = await loadEngine();
  for (const resident of [true, false]) {
    const session = createSession(run, {resident});
    assert.equal((await session.import(graph)).ok, true);
    const answer = await session.query('MATCH (n) RETURN count(*)');
    assert.equal(answer.ok, false);
    assert.match(answer.error, /not built into/);
  }
});

test('the query module answers query, with view: true mapped onto drawn boxes and routes, in a resident and a plain session', {skip: !hasQuery}, async () => {
  const run = await loadEngine({query: true});
  assert.equal(typeof run.openSession, 'function', 'the query module is the whole engine: sessions, layout, scene');
  for (const resident of [true, false]) {
    const session = createSession(run, {resident});
    assert.equal((await session.import(graph)).ok, true);
    const count = await session.query('MATCH (n) RETURN count(*)');
    assert.equal(count.ok, true, count.error);
    assert.equal(count.rows[0][0], count.graph.nodes);
    const hits = await session.query('MATCH (a)-[r]->(b) RETURN a, r, b', {view: true});
    assert.equal(hits.ok, true, hits.error);
    assert.ok(hits.edges.length > 0 && hits.nodes.length > 0);
    assert.deepEqual(Object.keys(hits.shown.nodes).sort(), [...hits.nodes].sort());
    assert.deepEqual(Object.keys(hits.shown.edges).sort(), [...hits.edges].sort());
    // Every hit stands on a box of the scene; every edge on a drawn line or on none (null).
    const scene = await session.scene();
    const boxes = new Set(scene.nodes.map(n => n.id));
    const lines = new Set(scene.routes.map(e => e.id));
    for (const box of Object.values(hits.shown.nodes)) assert.ok(box === null || boxes.has(box), `box ${box}`);
    for (const line of Object.values(hits.shown.edges)) assert.ok(line === null || lines.has(line), `line ${line}`);
    assert.ok(Object.values(hits.shown.nodes).some(b => b !== null), 'some node is drawn');
    assert.ok(Object.values(hits.shown.edges).some(l => l !== null), 'some edge is drawn');
    assert.equal((await session.query('MATCH (n RETURN n')).ok, false);
  }
  // Without view: true the answer has no mapping.
  const plain = createSession(run);
  await plain.import(graph);
  const rows = await plain.query('MATCH (n) RETURN n');
  assert.equal(rows.ok, true, rows.error);
  assert.equal(rows.shown, undefined);
  assert.equal((await createSession(run).query('RETURN 1')).ok, false, 'nothing imported yet');
});

test('the query module answers through the worker, which loads the module named in its URL', {skip: !hasQuery}, async () => {
  const bytes = readFileSync(queryWasmUrl);
  const listeners = [];
  const fetched = [];
  globalThis.self = {
    location: {href: `http://host/worker.js?wasm=${encodeURIComponent(String(queryWasmUrl))}`},
    postMessage: message => listeners.forEach(l => l({data: message})),
  };
  globalThis.fetch = async url => { fetched.push(String(url)); return {ok: true, arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength)}; };
  await import('./worker.js');
  const worker = {addEventListener: (_, l) => listeners.push(l), postMessage: data => globalThis.self.onmessage({data})};
  const session = createSession(workerRun(worker));
  assert.equal((await session.import(graph)).ok, true);
  const hits = await session.query('MATCH (a)-[r]->(b) RETURN r', {view: true});
  assert.equal(hits.ok, true, hits.error);
  assert.ok(hits.edges.length > 0);
  assert.deepEqual(Object.keys(hits.shown.edges).sort(), [...hits.edges].sort());
  assert.deepEqual(fetched, [String(queryWasmUrl)], 'the worker fetched the query module and no other');
});
