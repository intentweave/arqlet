// An ELK-style layout call on any Arqlet engine function (`run(request) -> response`: the WASM engine of
// `createEngine`, a worker or a backend): `layout(graph)` resolves with the graph in ELK's JSON form laid out (`x`, `y`,
// sizes, ports, labels and edge `sections`), as `elk.layout(graph)` does. Not a 1:1 ELK: the options without a counterpart are
// listed in `diagnostics` after each call (see crates/elk). `options` are Arqlet options that win over the translated
// ones; `refine` runs the refinement after the layout; with `timings` each call reports where its time went in `timings`
// (ms by phase; all 0 when the engine has no clock).
export function createElkLayout(run,{options,refine=false,timings=false}={}) {
  return {
    diagnostics:[],
    timings:null,
    async layout(graph) {
      const response=await run({op:'elkLayout',graph,...(options?{options}:{}),refine,...(timings?{timings:true}:{})});
      if(!response.ok) throw new Error(response.error);
      this.diagnostics=response.diagnostics;
      this.timings=response.timings??null;
      return response.graph;
    },
  };
}
