import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createEngine} from '@arqlet/engine-client/wasm-client.js';
import {evaluateSnapshot} from './comparison.js';
const run=await createEngine(readFileSync(new URL('./generated/arqlet.wasm',import.meta.url)));
const snapshot=JSON.parse(readFileSync(new URL('./generated/snapshot.json',import.meta.url)));
test('comparison evaluates both scenes with current shared metrics',()=>{
 const a=evaluateSnapshot(run,snapshot),b=evaluateSnapshot(run,structuredClone(snapshot));
 assert.equal(a.metrics.version,3);assert.deepEqual(a.metrics,b.metrics);assert.equal(a.identity,b.identity);
 assert.equal(a.metrics.edges,a.scene.routes.length);assert.equal(a.metrics.nodes,a.scene.nodes.length);
});
test('comparison refuses a missing route rather than reporting an improved score',()=>{
 const broken=structuredClone(snapshot);broken.routes=[];
 assert.throws(()=>evaluateSnapshot(run,broken),/Missing route/);
});
