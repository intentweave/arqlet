// `onPublish(message)` receives each progressive publication as JSON while the call that produced it is still running.
// The module always imports `lw_publish`; without a callback the messages are dropped.
export async function createEngine(bytes,{onPublish}={}) {
  const decoder=new TextDecoder();
  let api;
  const imports={env:{
    lw_publish:(ptr,len)=>{
      if(!onPublish) return;
      // Copy out of linear memory now: the module releases the buffer when this returns.
      onPublish(decoder.decode(new Uint8Array(api.memory.buffer,ptr,len)));
    },
  }};
  // The bytes of the module, or a module the host compiled already (then it is not compiled again).
  const result=await WebAssembly.instantiate(bytes,imports);
  const instance=result instanceof WebAssembly.Instance?result:result.instance;
  api=instance.exports;
  // Copies a request into the module, runs `call` on it, and reads the answer (both buffers freed afterwards).
  const exchange=(input,call)=>{
    const encoded=new TextEncoder().encode(JSON.stringify(input));
    const ptr=api.lw_alloc(encoded.length);
    let out=0,len=0;
    try {
      new Uint8Array(api.memory.buffer,ptr,encoded.length).set(encoded);
      const result=call(ptr,encoded.length);
      out=Number(result & 0xffffffffn);len=Number(result >> 32n);
      return JSON.parse(decoder.decode(new Uint8Array(api.memory.buffer,out,len)));
    } finally {api.lw_free(ptr,encoded.length);if(out)api.lw_free(out,len);}
  };
  const run=input=>exchange(input,(ptr,len)=>api.lw_run(ptr,len));
  // Provisional (ADR 0019, step 3): a session whose snapshot stays in the module; requests leave the snapshot out.
  if(api.lw_session_open){
    run.openSession=()=>{
      const id=api.lw_session_open();
      return {run:input=>exchange(input,(ptr,len)=>api.lw_session_run(id,ptr,len)),close:()=>api.lw_session_close(id)};
    };
  }
  return run;
}
