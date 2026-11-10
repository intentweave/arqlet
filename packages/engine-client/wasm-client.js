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
  const {instance}=await WebAssembly.instantiate(bytes,imports);
  api=instance.exports;
  return input=>{
    const encoded=new TextEncoder().encode(JSON.stringify(input));
    const ptr=api.lw_alloc(encoded.length);
    let out=0,len=0;
    try {
      new Uint8Array(api.memory.buffer,ptr,encoded.length).set(encoded);
      const result=api.lw_run(ptr,encoded.length);
      out=Number(result & 0xffffffffn);len=Number(result >> 32n);
      return JSON.parse(decoder.decode(new Uint8Array(api.memory.buffer,out,len)));
    } finally {api.lw_free(ptr,encoded.length);if(out)api.lw_free(out,len);}
  };
}
