import {createEngine} from './wasm-client.js';
// The host passes the WASM URL in the worker's query string (see createEngineWorker).
const wasmUrl=new URL(self.location.href).searchParams.get('wasm');
// The request being run. Publications are forwarded while its call is still executing; the final reply follows it.
let running=null;
// The browser's HTTP cache applies: an unchanged engine is not downloaded again for every worker.
const engine=fetch(wasmUrl).then(r=>{if(!r.ok)throw Error(`Engine not available: ${wasmUrl}`);return r.arrayBuffer();})
 .then(bytes=>createEngine(bytes,{onPublish:text=>{
  if(running===null) return;
  // emittedAt and bytes let the page split the latency into computation, transfer and display.
  self.postMessage({requestId:running,publication:JSON.parse(text).publication,emittedAt:Date.now(),bytes:text.length});
 }}));
// Provisional (ADR 0019, step 3): sessions kept in this worker, by handle. `{requestId, session: 'open'}` answers
// `{session: id}`; `{requestId, session: id, request}` runs a request in it; `{requestId, session: id, close: true}` ends it.
const sessions=new Map();
let nextSession=0;
self.onmessage=async({data})=>{
 try{
  const run=await engine;
  if(data.session==='open'){
   if(!run.openSession)throw Error('This engine has no sessions');
   const id=++nextSession;sessions.set(id,run.openSession());
   self.postMessage({requestId:data.requestId,ok:true,session:id});return;
  }
  if(data.session!==undefined){
   const session=sessions.get(data.session);
   if(!session)throw Error('Unknown session');
   if(data.close){session.close();sessions.delete(data.session);self.postMessage({requestId:data.requestId,ok:true});return;}
   if(data.columns){
    // The page cannot view this module's memory: copy each column once and hand the copies over (transferred, not cloned).
    if(!session.columns)throw Error('This engine has no columns');
    const result=session.columns();const transfer=[];
    for(const [k,v] of Object.entries(result))if(ArrayBuffer.isView(v)){result[k]=v.slice();transfer.push(result[k].buffer);}
    self.postMessage({requestId:data.requestId,ok:true,columns:result},transfer);return;
   }
   if(data.overview){
    // As for the scene's columns: the numbers of the overview are copied once and transferred.
    if(!session.overview)throw Error('This engine has no columnar overview');
    const result=session.overview(data.request,{copy:true});const transfer=[];
    if(result.ok)for(const v of [...Object.values(result.overview),...Object.values(result.overview.containerStats??{})])if(ArrayBuffer.isView(v))transfer.push(v.buffer);
    self.postMessage({requestId:data.requestId,...result},transfer);return;
   }
   running=data.requestId;const result=session.run(data.request);running=null;
   self.postMessage({requestId:data.requestId,...result});return;
  }
  running=data.requestId;const result=run(data.request);running=null;self.postMessage({requestId:data.requestId,...result});
 }
 catch(e){running=null;self.postMessage({requestId:data.requestId,ok:false,error:e.message});}
};
