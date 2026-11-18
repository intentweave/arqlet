import {test} from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
import {communityGroups, importGraph} from './engine.js';
import {createEngine} from './wasm-client.js';

// The engine shipped beside this module, or a release build of the workspace.
const shipped = new URL('./arqlet.wasm', import.meta.url);
const built = new URL('../../target/wasm32-unknown-unknown/release/arqlet_wasm.wasm', import.meta.url);
const wasm = existsSync(shipped) ? shipped : built;
const engine = existsSync(wasm) ? await createEngine(readFileSync(wasm)) : null;

// Three rings of eight nodes at the top level: communities divide them.
const nodes = [];
const edges = [];
for (const ring of ['a', 'b', 'c']) {
  for (let i = 0; i < 8; i++) {
    nodes.push({id: `${ring}${i}`, kind: 'fn', label: `${ring}${i}`});
    edges.push({id: `${ring}${i}e`, kind: 'calls', source: `${ring}${i}`, target: `${ring}${(i + 1) % 8}`});
  }
}
const graph = {nodes, edges};

test('communities computed once come back as groups, and the view needs no communities again', {skip: !engine}, () => {
  const groups = communityGroups(engine, graph, {communities: 6});
  assert.equal(Object.keys(groups).length, 24);
  const byCommunities = importGraph(engine, graph, 100, {communities: 6});
  const byGroups = importGraph(engine, graph, 100, {communities: 6, groups});
  assert.equal(byGroups.collapsedIds.length, byCommunities.collapsedIds.length);
  const labels = s => s.graph.nodes.map(n => n.label).sort();
  assert.deepEqual(labels(byGroups), labels(byCommunities));
  assert.throws(() => communityGroups(engine, {nodes: [{id: 'x', kind: 'k', label: 'x', parentId: 'nope'}], edges: []}), /parent/);
});

test('an import can leave the source graph out of its snapshot', {skip: !engine}, () => {
  const lean = importGraph(engine, graph, 100, {communities: 6}, {omitSource: true});
  assert.equal(lean.sourceGraph ?? null, null);
  assert.match(lean.sourceDigest, /^[0-9a-f]{16}$/);
  const opened = engine({op: 'toggle', snapshot: lean, source: graph, nodeId: lean.collapsedIds[0],
    expectedRevision: lean.revision, expectedLayoutVersion: lean.layoutVersion});
  assert.equal(opened.ok, true, opened.error);
  assert.equal(opened.snapshot.sourceDigest, lean.sourceDigest);
});
