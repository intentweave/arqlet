import {test} from 'node:test';
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import {createElkLayout} from './elk.js';
import {createEngine} from './wasm-client.js';

const wasmPath=process.env.LW_WASM ?? new URL('../../apps/playground/generated/arqlet.wasm',import.meta.url).pathname;

test('layout() sends one elkLayout request and returns the graph, with the diagnostics',async()=>{
  const requests=[];
  const elk=createElkLayout(request=>{requests.push(request);return {ok:true,graph:{id:'root',width:1},diagnostics:['d']};},{refine:true});
  assert.deepEqual(await elk.layout({id:'root'}),{id:'root',width:1});
  assert.deepEqual(requests,[{op:'elkLayout',graph:{id:'root'},refine:true}]);
  assert.deepEqual(elk.diagnostics,['d']);
  await assert.rejects(createElkLayout(()=>({ok:false,error:'broken'})).layout({}),/broken/);
});

test('the WASM engine lays out an ELK graph',{skip:existsSync(wasmPath)?false:'run apps/playground/build.sh first'},async()=>{
  const elk=createElkLayout(await createEngine(readFileSync(wasmPath)));
  const laid=await elk.layout({id:'root',children:[{id:'a',width:40,height:30},{id:'b',width:40,height:30}],
    edges:[{id:'ab',sources:['a'],targets:['b']}]});
  assert.ok(laid.children.every(c=>Number.isFinite(c.x)&&Number.isFinite(c.y)));
  const section=laid.edges[0].sections[0];
  assert.equal(section.incomingShape,'a');assert.equal(section.outgoingShape,'b');
  assert.ok(laid.children[1].x>laid.children[0].x,'flows to the right, as ELK by default');
});

test('timings: true reports where each call went (#152)',async()=>{
  const requests=[];
  const elk=createElkLayout(request=>{requests.push(request);return {ok:true,graph:{id:'root'},diagnostics:[],timings:{total:2,layout:1}};},{timings:true});
  await elk.layout({id:'root'});
  assert.equal(requests[0].timings,true);
  assert.deepEqual(elk.timings,{total:2,layout:1});
  const plain=createElkLayout(request=>{requests.push(request);return {ok:true,graph:{id:'root'},diagnostics:[]};});
  await plain.layout({id:'root'});
  assert.equal('timings' in requests[1],false);
  assert.equal(plain.timings,null);
});

test('the WASM engine reports phase timings for an ELK graph',{skip:existsSync(wasmPath)?false:'run apps/playground/build.sh first'},async()=>{
  const elk=createElkLayout(await createEngine(readFileSync(wasmPath)),{timings:true});
  await elk.layout({id:'root',children:[{id:'a',width:40,height:30},{id:'b',width:40,height:30}],
    edges:[{id:'ab',sources:['a'],targets:['b']}]});
  for(const key of ['total','layout','routing','plan','construction','elkTranslation']) assert.equal(typeof elk.timings[key],'number',key);
  assert.ok(elk.timings.total>=elk.timings.layout);
});
