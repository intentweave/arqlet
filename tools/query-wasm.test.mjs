// node --test tools/query-wasm.test.mjs
// ADR 0021, stage M5c: the query-only WASM module (crates/query-wasm) against the independent oracle. Every supported case
// of examples/cypher/ is loaded into the module and queried; its rows must be the oracle's. Build the module first:
//   cargo build -p arqlet-query-wasm --target wasm32-unknown-unknown --release --locked
// Without the build the tests are skipped.
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import test from 'node:test';

const root = new URL('../', import.meta.url);
const wasmFile = new URL('target/wasm32-unknown-unknown/release/arqlet_query_wasm.wasm', root);
const read = path => readFileSync(new URL(path, root), 'utf8');

/** The module's ABI (lw_alloc, lw_free, lw_run with JSON) as a function from a request object to an answer object. */
async function module() {
  const {instance} = await WebAssembly.instantiate(readFileSync(wasmFile), {});
  const x = instance.exports;
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();
  return request => {
    const bytes = encoder.encode(JSON.stringify(request));
    const input = x.lw_alloc(bytes.length);
    new Uint8Array(x.memory.buffer, input, bytes.length).set(bytes);
    const packed = x.lw_run(input, bytes.length);
    x.lw_free(input, bytes.length);
    const length = Number(packed >> 32n);
    const address = Number(packed & 0xffffffffn);
    const text = decoder.decode(new Uint8Array(x.memory.buffer, address, length).slice());
    x.lw_free(address, length);
    return JSON.parse(text);
  };
}

const canonical = (rows, ordered) => {
  const lines = rows.map(row => JSON.stringify(row));
  return ordered ? lines : lines.sort();
};

test('every supported case gives the oracle\'s rows through the WASM module', {skip: !existsSync(wasmFile)}, async () => {
  const run = await module();
  const cases = read('examples/cypher/cases.jsonl').trim().split('\n').map(line => JSON.parse(line))
    .filter(c => c.status === 'supported');
  assert.ok(cases.length >= 40, `the fixtures hold ${cases.length} supported cases`);
  for (const c of cases) {
    const request = {op: 'load', graph: JSON.parse(read(`examples/cypher/graphs/${c.graph}.json`))};
    if (c.payload) request.payload = JSON.parse(read(`examples/cypher/payloads/${c.payload}.json`));
    assert.equal(run(request).ok, true, `${c.id}: load`);
    const answer = run({op: 'query', query: c.query});
    assert.equal(answer.ok, true, `${c.id}: ${c.query}: ${answer.error}`);
    assert.deepEqual(canonical(answer.rows, c.ordered), canonical(c.rows, c.ordered), `${c.id}: ${c.query}`);
  }
});

test('errors are answers with a class, and a failed load keeps what was loaded', {skip: !existsSync(wasmFile)}, async () => {
  const run = await module();
  assert.equal(run({op: 'query', query: 'MATCH (n) RETURN n'}).ok, false, 'nothing loaded');
  const loaded = run({op: 'load', graph: JSON.parse(read('examples/cypher/graphs/small.json'))});
  assert.deepEqual([loaded.ok, loaded.nodes, loaded.edges, loaded.unresolvedEdges], [true, 3, 5, 1]);
  assert.match(run({op: 'query', query: 'MATCH (n RETURN n'}).error, /^query syntax:/);
  assert.match(run({op: 'query', query: 'CREATE (n)'}).error, /^query unsupported:/);
  assert.match(run({op: 'query', query: 'MATCH (a), (b), (c) RETURN count(*)', maxSteps: 10}).error, /^query limit:/);
  assert.equal(run({op: 'load', graph: 3}).ok, false);
  assert.equal(run({op: 'query', query: 'MATCH (n) RETURN count(*)'}).rows[0][0], 3);
});
