import {readFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import assert from 'node:assert/strict';
import {createEngine} from '@arqlet/engine-client/wasm-client.js';
// The native engine build.sh makes (optimised, `fast` profile).
const NATIVE=fileURLToPath(new URL('../../target/fast/layout-command',import.meta.url));
const snapshot=JSON.parse(readFileSync(new URL('./generated/snapshot.json',import.meta.url)));
const run=await createEngine(readFileSync(new URL('./generated/arqlet.wasm',import.meta.url)));
const sourceId=snapshot.graph.edges[0]?.source ?? snapshot.graph.nodes.at(-1).id;
const request={op:'add',snapshot,expectedRevision:snapshot.revision,sourceId,label:'WASM parity test'};
const wasm=run(request);assert.equal(wasm.ok,true);
const native=spawnSync(NATIVE,[],{input:JSON.stringify(request),encoding:'utf8'});
assert.equal(native.status,0,native.stderr);assert.deepEqual(wasm,JSON.parse(native.stdout));
assert.equal(wasm.snapshot.revision,snapshot.revision+1);
assert.equal(run({...request,snapshot:wasm.snapshot}).ok,false);
// Exercise allocation/free and sequential edits, not just module instantiation.
let state=wasm.snapshot;for(let i=0;i<10;i++){const r=run({...request,snapshot:state,expectedRevision:state.revision});assert.equal(r.ok,true);state=r.snapshot;}
console.log(`Native/WASM parity passed; stale edit rejected; 11 sequential edits completed (${snapshot.graph.nodes.length} initial nodes).`);

// The refinement portfolio is that of the pipeline before single pass (ADR 0016: single pass plans no candidates).
const legacyOptions={singlePassAuto:false,singlePass:false,abstractRouting:false};
const legacy=run({op:'import',graph:JSON.parse(readFileSync(new URL('./generated/graph.json',import.meta.url))),limit:40,options:legacyOptions}).snapshot;
const refineRequest={op:'refine',snapshot:legacy,expectedRevision:legacy.revision,expectedLayoutVersion:legacy.layoutVersion??0};
const refined=run(refineRequest);
const nativeRefined=spawnSync(NATIVE,[],{input:JSON.stringify(refineRequest),encoding:'utf8'});
assert.equal(refined.ok,true);assert.equal(nativeRefined.status,0,nativeRefined.stderr);
assert.deepEqual(refined,JSON.parse(nativeRefined.stdout));
// Baseline + five candidates, then one to three complete global routing rounds.
assert.ok(refined.snapshot.optimization.candidatesEvaluated>=7 && refined.snapshot.optimization.candidatesEvaluated<=9);
console.log('Refinement including global rounds has native/WASM parity.');
const threadedRequest={...refineRequest,threads:4};
const nativeThreaded=spawnSync(NATIVE,[],{input:JSON.stringify(threadedRequest),encoding:'utf8'});
assert.equal(nativeThreaded.status,0,nativeThreaded.stderr);assert.equal(nativeThreaded.stdout,nativeRefined.stdout);
assert.deepEqual(run(threadedRequest),refined);
const imported=run({op:'import',graph:JSON.parse(readFileSync(new URL('./generated/graph.json',import.meta.url))),limit:40});
assert.equal(imported.ok,true);assert.deepEqual(imported.snapshot.boxes,snapshot.boxes);
console.log('Threaded refinement is byte-identical to serial; WASM import matches the native snapshot.');

// The opt-in abstract path must survive the same JSON handoff, not only native diagnostics.
const abstractGraph=JSON.parse(readFileSync(new URL('../../examples/fixtures/small.json',import.meta.url)));
let abstractRequest={op:'import',graph:abstractGraph,limit:1000,options:{abstractRouting:true,singlePass:false,singlePassAuto:false}};
let abstractState;
for(const operation of ['import','refine','add']){
 if(operation==='refine')abstractRequest={op:'refine',snapshot:abstractState,expectedRevision:abstractState.revision,expectedLayoutVersion:abstractState.layoutVersion,threads:4};
 if(operation==='add')abstractRequest={op:'add',snapshot:abstractState,expectedRevision:abstractState.revision,expectedLayoutVersion:abstractState.layoutVersion,sourceId:'b',label:'Abstract continuation'};
 const result=run(abstractRequest);
 const native=spawnSync(NATIVE,[],{input:JSON.stringify(abstractRequest),encoding:'utf8'});
 assert.equal(result.ok,true,JSON.stringify(result));assert.equal(native.status,0,native.stderr);
 assert.deepEqual(result,JSON.parse(native.stdout));
 assert.notEqual(result.snapshot.options?.abstractRouting,false);
 abstractState=result.snapshot;
}
assert.equal(run({op:'refine',snapshot:abstractState,expectedRevision:abstractState.revision,expectedLayoutVersion:0}).ok,false);
console.log('Opt-in abstract import/refinement/edit match native/WASM; stale handoff rejected.');

// The automatic single pass: the same result in WASM and native, on single pass and after the fallback a pin causes.
let autoState;
for(const operation of ['import','refine','pin','refine after the fallback']){
 const state=autoState&&{snapshot:autoState,expectedRevision:autoState.revision,expectedLayoutVersion:autoState.layoutVersion??0};
 const leaf=autoState?.graph.nodes.find(n=>!autoState.graph.nodes.some(c=>c.parentId===n.id)).id;
 const request=operation==='import'?{op:'import',graph:abstractGraph,limit:1000,options:{singlePassAuto:true}}
  :operation==='pin'?{op:'configure',...state,options:{pins:{[leaf]:{x:40,y:60}}}}:{op:'refine',...state,threads:1};
 const result=run(request);
 const native=spawnSync(NATIVE,[],{input:JSON.stringify(request),encoding:'utf8',maxBuffer:64*1024*1024});
 assert.equal(result.ok,true,JSON.stringify(result));assert.equal(native.status,0,native.stderr);
 assert.deepEqual(result,JSON.parse(native.stdout));
 autoState=result.snapshot;
 assert.equal(autoState.options?.singlePass!==false,!operation.includes('pin')&&!operation.includes('fallback'),operation);
}
assert.ok(autoState.layoutDiagnostics.some(d=>d.startsWith('Single pass was not used:'))||autoState.options?.pins);
console.log('Automatic single pass matches native/WASM, including its fallback.');

// A released drag on a single-pass drawing stays on single pass and leaves an order hint, in WASM as natively.
{
 let state=run({op:'import',graph:abstractGraph,limit:1000,options:{singlePassAuto:true}}).snapshot;
 const node=state.graph.nodes.find(n=>!state.graph.nodes.some(c=>c.parentId===n.id)).id;
 const at=state.boxes[node];
 for(const [seq,phase] of [[1,'begin'],[2,'update'],[3,'finish']]){
  const request={op:'move',snapshot:state,expectedRevision:state.revision,expectedLayoutVersion:state.layoutVersion??0,gestureId:'drag',node,release:true,seq,phase,position:{x:at.x+30,y:at.y+30}};
  const result=run(request);
  const native=spawnSync(NATIVE,[],{input:JSON.stringify(request),encoding:'utf8',maxBuffer:64*1024*1024});
  assert.equal(result.ok,true,JSON.stringify(result));assert.equal(native.status,0,native.stderr);
  assert.deepEqual(result,JSON.parse(native.stdout));
  state=result.snapshot;assert.notEqual(state.options?.singlePass,false,phase);
 }
 assert.ok(state.options?.orderHints?.[node],'the drop leaves an order hint');
 console.log('A released drag on single pass stays on single pass and matches native/WASM.');
}

const container=snapshot.graph.nodes.find(n=>snapshot.graph.nodes.some(c=>c.parentId===n.id));
if(container){
 const request={op:'toggle',snapshot,expectedRevision:snapshot.revision,expectedLayoutVersion:snapshot.layoutVersion??0,nodeId:container.id};
 const closed=run(request);assert.equal(closed.ok,true);
 const native=spawnSync(NATIVE,[],{input:JSON.stringify(request),encoding:'utf8'});
 assert.equal(native.status,0,native.stderr);assert.deepEqual(closed,JSON.parse(native.stdout));
 const reopened=run({...request,snapshot:closed.snapshot,expectedLayoutVersion:closed.snapshot.layoutVersion});
 assert.equal(reopened.ok,true);assert.deepEqual(reopened.snapshot.graph,snapshot.graph);
 console.log('Collapse/expand roundtrip preserves the graph and matches native execution.');
}

const labelledRequest={op:'configure',snapshot,expectedRevision:snapshot.revision,expectedLayoutVersion:snapshot.layoutVersion??0,options:{edgeLabels:{[snapshot.graph.edges[0].id]:{text:'calls',width:48,height:22}},portLabels:{[snapshot.resolvedPorts[0].id]:{text:'out',width:32,height:22}}}};
const labelled=run(labelledRequest);
const nativeLabels=spawnSync(NATIVE,[],{input:JSON.stringify(labelledRequest),encoding:'utf8'});
assert.equal(labelled.ok,true);assert.equal(nativeLabels.status,0,nativeLabels.stderr);assert.deepEqual(labelled,JSON.parse(nativeLabels.stdout));
console.log('Measured edge/port labels have native/WASM parity.');

const configRequest={op:'configure',snapshot,expectedRevision:snapshot.revision,expectedLayoutVersion:snapshot.layoutVersion??0,options:{configSet:{schemaVersion:1,id:'smoke',layout:{algorithm:'hierarchical',candidates:[1,2,3,4],hybridBy:'compact'},routing:{style:'orthogonal',bundling:'none',routingCandidate:true},appearance:{showPorts:false}}}};
const configResult=run(configRequest);
const nativeConfig=spawnSync(NATIVE,[],{input:JSON.stringify(configRequest),encoding:'utf8'});
assert.equal(configResult.ok,true);assert.equal(nativeConfig.status,0,nativeConfig.stderr);assert.deepEqual(configResult,JSON.parse(nativeConfig.stdout));assert.deepEqual(configResult.snapshot.routes,snapshot.routes);
console.log('Config sets have native/WASM parity; appearance preserves route geometry.');

const overlapRequest={op:'configure',snapshot,expectedRevision:snapshot.revision,expectedLayoutVersion:snapshot.layoutVersion??0,options:{vpscOverlapRemoval:true}};
const overlapWasm=run(overlapRequest);
const nativeOverlap=spawnSync(NATIVE,[],{input:JSON.stringify(overlapRequest),encoding:'utf8'});
assert.equal(overlapWasm.ok,true);assert.equal(nativeOverlap.status,0,nativeOverlap.stderr);assert.deepEqual(overlapWasm,JSON.parse(nativeOverlap.stdout));
assert.equal(overlapWasm.snapshot.options.vpscOverlapRemoval,true);
console.log('Constraint-overlap option configuration has native/WASM parity.');

// Curated references: the same operations give the same document and the same canonical bytes in both engines.
const referenceCreate={op:'referenceCreate',snapshot,referenceId:'smoke'};
const referenceWasm=run(referenceCreate);
const nativeReference=spawnSync(NATIVE,[],{input:JSON.stringify(referenceCreate),encoding:'utf8'});
assert.equal(referenceWasm.ok,true);assert.equal(nativeReference.status,0,nativeReference.stderr);assert.deepEqual(referenceWasm,JSON.parse(nativeReference.stdout));
const canonicalRequest={op:'referenceCanonical',reference:referenceWasm.reference};
const canonicalWasm=run(canonicalRequest);
const nativeCanonical=spawnSync(NATIVE,[],{input:JSON.stringify(canonicalRequest),encoding:'utf8'});
assert.equal(canonicalWasm.ok,true);assert.equal(nativeCanonical.status,0,nativeCanonical.stderr);assert.equal(canonicalWasm.canonical,JSON.parse(nativeCanonical.stdout).canonical);
const validationWasm=run({op:'referenceValidate',reference:JSON.parse(canonicalWasm.canonical)});
assert.equal(validationWasm.ok,true);assert.equal(validationWasm.report.payloadHash,referenceWasm.reference.integrity.payloadHash);
console.log('Curated reference create, canonical text and validation have native/WASM parity.');
