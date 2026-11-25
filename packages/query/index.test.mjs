// node --test packages/query/*.test.mjs
// The package around the query-only module (ADR 0021, M5c). It needs the module: the release build in `target/`, the copy
// `prepack` leaves in the package, or the file named by ARQLET_QUERY_WASM. Without any, the tests that run it are skipped.
//   cargo build -p arqlet-query-wasm --target wasm32-unknown-unknown --release --locked
// The oracle's cases run through the module's ABI in tools/query-wasm.test.mjs.
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import {createServer} from 'node:http';
import test from 'node:test';
import {createQueryEngine, defaultWasmUrl, loadQueryEngine} from './index.js';

const built = new URL('../../target/wasm32-unknown-unknown/release/arqlet_query_wasm.wasm', import.meta.url);
const wasm = process.env.ARQLET_QUERY_WASM
  ? new URL(process.env.ARQLET_QUERY_WASM, `file://${process.cwd()}/`)
  : [built, defaultWasmUrl].find(url => existsSync(url));
const skip = !wasm && 'the query module is not built';

const graph = {
  nodes: [
    {id: 'pkg', kind: 'package', label: 'app', parentId: null},
    {id: 'main', kind: 'fn', label: 'main', parentId: 'pkg'},
    {id: 'parse', kind: 'fn', label: 'parse', parentId: 'pkg'},
  ],
  edges: [{id: 'e1', kind: 'calls', source: 'main', target: 'parse'}],
};
const payload = {nodes: {parse: {loc: 120}}};

test('the example of the README gives the answer it shows', {skip}, async () => {
  const engine = await loadQueryEngine(wasm);
  assert.deepEqual(engine.load(graph, {payload}), {ok: true, nodes: 3, edges: 1, unresolvedEdges: 0});
  const answer = engine.query('MATCH (a:fn)-[:calls]->(b:fn) WHERE b.loc > $min RETURN a.label, b, b.loc', {params: {min: 100}});
  assert.deepEqual(answer.rows, [['main', {node: 'parse'}, 120]]);
  assert.deepEqual([answer.columns, answer.rowCount, answer.nodes, answer.graph],
    [['a.label', 'b', 'b.loc'], 1, ['parse'], {nodes: 3, edges: 1}]);
  const readme = readFileSync(new URL('./README.md', import.meta.url), 'utf8');
  assert.ok(readme.includes("// answer.rows: [['main', {node: 'parse'}, 120]]"));
});

test('functions and UNION answer through the package', {skip}, async () => {
  const engine = await loadQueryEngine(wasm);
  engine.load(graph);
  const answer = engine.query(
    "MATCH (n:fn) RETURN toUpper(n.label) AS x, id(n) AS i UNION ALL MATCH ()-[r]->() RETURN type(r) AS x, id(r) AS i");
  assert.deepEqual(answer.rows, [['MAIN', 'main'], ['PARSE', 'parse'], ['calls', 'e1']]);
  assert.match(engine.query("WITH 'aaaaaaaaaaaaaaaa' AS a" + ' WITH a + a AS a'.repeat(40) + ' RETURN a').error, /value limit/);
  assert.deepEqual(engine.query('RETURN 1 AS one').rows, [[1]], 'the engine goes on after a limit');
});

test('maps come from the payload and from parameters, and are tagged in a row', {skip}, async () => {
  const engine = await loadQueryEngine(wasm);
  engine.load(graph, {payload: {nodes: {parse: {meta: {node: 'x', lines: [10, 20]}}}}});
  const answer = engine.query(
    'MATCH (n:fn) WHERE n.meta.lines[-1] > $p.over RETURN n, n.meta, [x IN n.meta.lines | x + 1]', {params: {p: {over: 15}}});
  assert.deepEqual(answer.rows, [[{node: 'parse'}, {map: {lines: [10, 20], node: 'x'}}, [11, 21]]]);
  assert.deepEqual(answer.nodes, ['parse'], 'a key `node` of a map is not a node');
});

test('errors are answers, and a refused load keeps what was loaded', {skip}, async () => {
  const engine = await loadQueryEngine(wasm);
  assert.equal(engine.query('MATCH (n) RETURN n').ok, false, 'nothing loaded');
  assert.equal(engine.load(graph).ok, true);
  assert.match(engine.query('MATCH (n RETURN n').error, /^query syntax:/);
  assert.match(engine.query('CREATE (n)').error, /^query unsupported:/);
  assert.match(engine.query('MATCH (a), (b), (c) RETURN count(*)', {maxSteps: 5}).error, /^query limit:/);
  assert.match(engine.load(graph, {payload: {nodes: {main: {label: 'x'}}}}).error, /`label` is a property of the model/);
  assert.equal(engine.load({nodes: 3}).ok, false);
  assert.deepEqual(engine.query('MATCH (n) RETURN count(*)').rows, [[3]]);
});

test('engines are independent and take a compiled module', {skip}, async () => {
  const module = await WebAssembly.compile(readFileSync(wasm));
  const [a, b] = await Promise.all([createQueryEngine(module), createQueryEngine(module)]);
  a.load(graph);
  b.load({nodes: [graph.nodes[0]], edges: []});
  assert.deepEqual(a.query('MATCH (n) RETURN count(*)').rows, [[3]]);
  assert.deepEqual(b.query('MATCH (n) RETURN count(*)').rows, [[1]]);
});

test('over HTTP the module is fetched, and a missing one is an error', {skip}, async context => {
  const bytes = readFileSync(wasm);
  const server = createServer((request, response) => {
    const found = request.url === '/arqlet-query.wasm';
    response.writeHead(found ? 200 : 404, {'content-type': 'application/wasm'});
    response.end(found ? bytes : undefined);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  context.after(() => server.close());
  const base = `http://127.0.0.1:${server.address().port}`;
  const engine = await loadQueryEngine(`${base}/arqlet-query.wasm`);
  assert.equal(engine.load(graph).ok, true);
  await assert.rejects(loadQueryEngine(`${base}/missing.wasm`), /Query module not available/);
});

test('after a call that traps, the engine answers errors and does not run again', async context => {
  let runs = 0;
  const exports = {
    memory: new WebAssembly.Memory({initial: 1}),
    lw_alloc: () => 0,
    lw_free: () => {},
    lw_run: () => { runs++; throw new WebAssembly.RuntimeError('unreachable'); },
  };
  context.mock.method(WebAssembly, 'instantiate', async () => ({instance: {exports}}));
  const engine = await createQueryEngine(new Uint8Array());
  // A request that is not JSON is the caller's error and stops nothing.
  assert.throws(() => engine.query('RETURN $n', {params: {n: 1n}}), TypeError);
  const first = engine.query('MATCH (n) RETURN n');
  assert.equal(first.ok, false);
  assert.match(first.error, /stopped \(unreachable\)/);
  assert.deepEqual(engine.load(graph), first);
  assert.equal(runs, 1);
});

test('the package does not bring the layout engine', () => {
  const manifest = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'));
  assert.deepEqual(Object.keys(manifest.dependencies), ['@arqlet/contracts'], 'types only: no engine package');
});
