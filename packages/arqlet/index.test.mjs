import {test} from 'node:test';
import assert from 'node:assert/strict';
import {existsSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {createElkLayout, createSession, workerRun, defaultWasmUrl, loadEngine, renderSvg, toReactFlow} from './index.js';

test('the entry point re-exports the engine and viewer functions', () => {
  for (const f of [createElkLayout, createSession, workerRun, loadEngine, renderSvg, toReactFlow]) assert.equal(typeof f, 'function');
});

// Needs the WASM next to the engine client (`pnpm pack` builds it; build.sh and CI copy it there too).
test('the bundled engine lays out an ELK graph in Node', {skip: !existsSync(fileURLToPath(defaultWasmUrl))}, async () => {
  const run = await loadEngine();
  const elk = createElkLayout(run);
  const graph = await elk.layout({
    id: 'root',
    children: [{id: 'a', width: 40, height: 30}, {id: 'b', width: 40, height: 30}],
    edges: [{id: 'e1', sources: ['a'], targets: ['b']}, {id: 'e2', sources: ['b'], targets: ['a']}],
  });
  assert.ok(graph.children.every(n => Number.isFinite(n.x) && Number.isFinite(n.y)));
  // Logical direction is kept: e2 still runs from b to a.
  assert.equal(graph.edges[1].sections[0].incomingShape, 'b');
});
