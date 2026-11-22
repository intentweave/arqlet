// node --test tools/check-query-isolation.test.mjs
// The rules of tools/check-query-isolation.mjs on made-up trees (so that a broken rule is seen to be caught), and on the
// real workspace.
import assert from 'node:assert/strict';
import test from 'node:test';
import {QUERY_CRATES, crateNames, forbiddenIn, ownThirdParty, violations} from './check-query-isolation.mjs';

const TREE = `arqlet-query v0.8.0 (/w/crates/query)
arqlet-layout v0.8.0 (/w/crates/layout)
arqlet-model v0.8.0 (/w/crates/model)
serde v1.0.228
serde v1.0.228 (*)
decypher v0.2.0
`;

test('crate names are read from cargo tree output, once each', () => {
  assert.deepEqual([...crateNames(TREE)].sort(), ['arqlet-layout', 'arqlet-model', 'arqlet-query', 'decypher', 'serde']);
});

test('a layout crate in a tree is found, an internal model crate is not', () => {
  assert.deepEqual(forbiddenIn(crateNames(TREE)), ['arqlet-layout']);
  assert.deepEqual(forbiddenIn(crateNames('arqlet-query v0.8.0\narqlet-model v0.8.0\nserde v1.0.0')), []);
});

test('every layout-side crate is forbidden, the facade and the runtime included', () => {
  for (const name of ['arqlet', 'arqlet-layout', 'arqlet-routing', 'arqlet-scene', 'arqlet-view', 'arqlet-topology',
    'arqlet-elk', 'arqlet-reference', 'arqlet-runtime', 'arqlet-wasm']) {
    assert.deepEqual(forbiddenIn(new Set([name])), [name]);
  }
});

test('a third-party crate is own unless arqlet-model has it too', () => {
  const model = crateNames('arqlet-model v0.8.0\nserde v1.0.0\nlibm v0.2.16');
  assert.deepEqual(ownThirdParty(crateNames('arqlet-query v0.8.0\narqlet-model v0.8.0\nserde v1.0.0'), model), []);
  assert.deepEqual(ownThirdParty(crateNames('arqlet-query v0.8.0\nserde v1.0.0\nregex v1.0.0'), model), ['regex']);
});

test('violations name the crate, the culprit and the target', () => {
  const fake = (crate, target) => {
    if (crate === 'arqlet-query-cypher') return crateNames('arqlet-query-cypher v0.8.0\narqlet-runtime v0.8.0');
    if (crate === 'arqlet-model') return crateNames('arqlet-model v0.8.0\nserde v1.0.0');
    return crateNames('arqlet-query v0.8.0\nregex v1.0.0');
  };
  const problems = violations(fake);
  assert.ok(problems.some(p => p.includes('arqlet-query-cypher') && p.includes('arqlet-runtime') && p.includes('wasm32-unknown-unknown')));
  assert.ok(problems.some(p => p.includes('third-party crates of its own') && p.includes('regex')));
});

test('the query crates of this workspace are isolated, for the host and for wasm32', () => {
  assert.deepEqual(violations(), []);
});

test('the adapter, the JSON layer, the tool and the module are among the checked crates', () => {
  for (const crate of ['arqlet-query-model', 'arqlet-query-json', 'arqlet-query-cli', 'arqlet-query-wasm']) {
    assert.ok(QUERY_CRATES.includes(crate), crate);
  }
});
