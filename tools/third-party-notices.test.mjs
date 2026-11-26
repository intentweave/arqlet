// node --test tools/third-party-notices.test.mjs
// Needs cargo and the crate sources cargo has fetched (as after any build); reads nothing from the network.
import assert from 'node:assert/strict';
import test from 'node:test';
import {collect, notices, render} from './third-party-notices.mjs';

const crates = collect('arqlet-wasm', 'wasm32-unknown-unknown');
const names = crates.map(c => c.name);

test('the crates linked into the WASM engine are listed, and only those', () => {
  for (const expected of ['serde', 'serde_json', 'libm', 'itoa', 'memchr']) {
    assert.ok(names.includes(expected), `${expected} is linked and must be listed`);
  }
  assert.ok(!names.some(n => n.startsWith('arqlet')), 'the crates of this workspace carry the project license');
  for (const procMacro of ['serde_derive', 'syn', 'quote', 'proc-macro2']) {
    assert.ok(!names.includes(procMacro), `${procMacro} runs at build time and is not distributed`);
  }
});

test('every listed crate ships a license file', () => {
  const missing = crates.filter(c => c.files.length === 0).map(c => `${c.name} ${c.version}`);
  assert.deepEqual(missing, [], 'a crate without a license file needs a decision before a release');
});

test('the notices name each crate with its license and carry the license texts', () => {
  const text = render(crates);
  for (const c of crates) assert.ok(text.includes(`## ${c.name} ${c.version}`), `${c.name} has a section`);
  assert.match(text, /Apache License/);
  assert.match(text, /Permission is hereby granted/);
});

test('the query module (@arqlet/query) has its own list, with the parser, and no license file is missing', () => {
  const query = collect('arqlet-query-wasm', 'wasm32-unknown-unknown');
  const linked = query.map(c => c.name);
  for (const expected of ['decypher', 'rowan', 'unicode-ident', 'serde_json']) {
    assert.ok(linked.includes(expected), `${expected} is linked and must be listed`);
  }
  assert.ok(!linked.some(n => n.startsWith('arqlet')));
  assert.deepEqual(query.filter(c => c.files.length === 0).map(c => `${c.name} ${c.version}`), []);
});

test('the notices are the same on every run', () => {
  assert.equal(notices('arqlet-wasm', 'wasm32-unknown-unknown'), notices('arqlet-wasm', 'wasm32-unknown-unknown'));
});
