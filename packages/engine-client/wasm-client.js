// `onPublish(message)` receives each progressive publication as JSON while the call that produced it is still running.
// The module always imports `lw_publish` and `lw_now`; without a callback the messages are dropped.
export async function createEngine(bytes,{onPublish}={}) {
  const decoder=new TextDecoder();
  let api;
  const imports={env:{
    // The clock for phase timings (`timings: true` on a request); the layout itself never reads it.
    lw_now:()=>performance.now(),
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
      const views={f32:Float32Array,u32:Uint32Array,i32:Int32Array,f64:Float64Array};
      // `listed`: name, type and length of each column as the engine described them; `read(index)` its address and bytes.
      const viewsOf=(listed,read,copy)=>{
        const result={};
        listed.forEach((c,index)=>{
          const answer=read(index);
          const ptr=Number(answer & 0xffffffffn),bytes=Number(answer >> 32n);
          const view=new views[c.type](api.memory.buffer,ptr,bytes/views[c.type].BYTES_PER_ELEMENT);
          result[c.name]=copy?view.slice():view;
        });
        return result;
      };
      const columns=()=>{
        const meta=run({op:'columns'});
        if(!meta.ok) throw Error(meta.error);
        const {ok,columns:listed,...result}=meta;
        return {...result,...viewsOf(listed,index=>api.lw_session_column(id,index),false)};
      };
      // Provisional: an `overview` request whose numbers come back as typed arrays instead of JSON arrays (`parent`, `x`,
      // `y`, `r`, `kind`, `edges`, `links`, with `edgeData` `edgeKind` and `edgeWeight`, and with `containerStats` its
      // arrays; a continuation with `positionsOnly` only `x`, `y` and `r`), in an answer shaped
      // like the JSON one. `copy` (default false) copies them out of the module's memory, for a host that keeps them past
      // its next request; views are valid only until then.
      const overview=(request,{copy=false}={})=>{
        const answer=run({...request,columns:true});
        if(!answer.ok) return answer;
        const {columns:listed,...rest}=answer.overview;
        const result={...rest};
        // A column named `object.field` (`containerStats.share`) goes into that object, beside its JSON fields.
        for(const [name,view] of Object.entries(viewsOf(listed,index=>api.lw_session_overview_column(id,index),copy))){
          const dot=name.indexOf('.');
          if(dot<0){result[name]=view;continue;}
          const key=name.slice(0,dot);
          result[key]={...result[key],[name.slice(dot+1)]:view};
        }
        return {...answer,overview:result};
      };
      return {run,close:()=>api.lw_session_close(id),...(api.lw_session_column?{columns}:{}),...(api.lw_session_overview_column?{overview}:{})};
    };
  }
  return run;
}
