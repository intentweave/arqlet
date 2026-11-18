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
      const run=input=>exchange(input,(ptr,len)=>api.lw_session_run(id,ptr,len));
      // Provisional (ADR 0019, step 4): the scene as columns. The numbers are typed-array views into the module's memory,
      // not copies: valid until the next request to this session (a change drops them) or until the memory grows, so a
      // host that keeps them copies them (`slice()`). The id tables are copies.
      const columns=()=>{
        const meta=run({op:'columns'});
        if(!meta.ok) throw Error(meta.error);
        const {ok,columns:listed,...result}=meta;
        listed.forEach((c,index)=>{
          const answer=api.lw_session_column(id,index);
          const ptr=Number(answer & 0xffffffffn),bytes=Number(answer >> 32n);
          const View=c.type==='f32'?Float32Array:Uint32Array;
          result[c.name]=new View(api.memory.buffer,ptr,bytes/4);
        });
        return result;
      };
      return {run,close:()=>api.lw_session_close(id),...(api.lw_session_column?{columns}:{})};
    };
  }
  return run;
}
