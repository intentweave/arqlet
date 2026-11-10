import {test} from 'node:test';
import assert from 'node:assert/strict';
import {existsSync, readFileSync} from 'node:fs';
import {createEngine} from './wasm-client.js';

// The module and the demo snapshot are written by apps/playground/build.sh. LW_WASM overrides the module path.
const wasmPath=process.env.LW_WASM ?? new URL('../../apps/playground/generated/arqlet.wasm',import.meta.url).pathname;
const snapshotPath=new URL('../../apps/playground/generated/snapshot.json',import.meta.url);
const built=existsSync(wasmPath)&&existsSync(snapshotPath);

// Lexicographic comparison of published keys (arrays of integers), in the objective order.
function before(a,b){
  for(let i=0;i<Math.min(a.length,b.length);i++){ if(a[i]!==b[i]) return a[i]<b[i]; }
  return a.length<b.length;
}

test('progressive publications arrive while the refinement is still running',{skip:built?false:'run apps/playground/build.sh first'},async()=>{
  const imported=JSON.parse(readFileSync(snapshotPath,'utf8'));
  const messages=[];
  let running=false;
  const run=await createEngine(readFileSync(wasmPath),{onPublish:text=>messages.push({message:JSON.parse(text),during:running})});
  // This small demo graph has no accepted improvement with the hierarchical default; the
  // publication contract is exercised with the retained comparison pipeline instead.
  const configured=run({op:'configure',snapshot:imported,expectedRevision:imported.revision,expectedLayoutVersion:imported.layoutVersion??0,options:{layeredConstruction:false,hierarchicalOrdering:false}});
  assert.equal(configured.ok,true);
  const snapshot=configured.snapshot;
  running=true;
  const response=run({op:'refineProgressive',jobId:'job-1',snapshot,expectedRevision:snapshot.revision,expectedLayoutVersion:snapshot.layoutVersion??0,threads:2});
  running=false;
  assert.equal(response.ok,true);
  assert.equal(response.jobId,'job-1');
  assert.ok(messages.length>0,'the demo snapshot publishes at least one improvement');
  assert.ok(messages.every(m=>m.during),'every publication arrived inside the call, before the response');
  messages.forEach((m,k)=>{
    assert.equal(m.message.publication.jobId,'job-1');
    assert.equal(m.message.publication.sequence,k+1);
  });
  for(let k=1;k<messages.length;k++){
    assert.ok(before(messages[k].message.publication.key,messages[k-1].message.publication.key),'keys strictly decrease');
  }
  assert.deepEqual(messages.at(-1).message.publication.snapshot.boxes,response.snapshot.boxes,'the last publication is the result');
});

test('a call without a publication callback still returns the result',{skip:built?false:'run apps/playground/build.sh first'},async()=>{
  const snapshot=JSON.parse(readFileSync(snapshotPath,'utf8'));
  const run=await createEngine(readFileSync(wasmPath));
  const response=run({op:'refineProgressive',jobId:'job-2',snapshot,expectedRevision:snapshot.revision,expectedLayoutVersion:snapshot.layoutVersion??0});
  assert.equal(response.ok,true);
});
