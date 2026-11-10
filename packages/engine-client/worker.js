import {createEngine} from './wasm-client.js';
// The host passes the WASM URL in the worker's query string (see createEngineWorker).
const wasmUrl=new URL(self.location.href).searchParams.get('wasm');
// The request being run. Publications are forwarded while its call is still executing; the final reply follows it.
let running=null;
const engine=fetch(wasmUrl,{cache:'no-store'}).then(r=>{if(!r.ok)throw Error(`Engine not available: ${wasmUrl}`);return r.arrayBuffer();})
 .then(bytes=>createEngine(bytes,{onPublish:text=>{
  if(running===null) return;
  // emittedAt and bytes let the page split the latency into computation, transfer and display.
  self.postMessage({requestId:running,publication:JSON.parse(text).publication,emittedAt:Date.now(),bytes:text.length});
 }}));
self.onmessage=async({data})=>{
 try{const run=await engine;running=data.requestId;const result=run(data.request);running=null;self.postMessage({requestId:data.requestId,...result});}
 catch(e){running=null;self.postMessage({requestId:data.requestId,ok:false,error:e.message});}
};
