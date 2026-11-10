// Paired release comparison, with frozen inputs and explicit timing boundaries.
// Usage: node focused-pipeline.mjs --baseline DIR --repeat 5 --out FILE [--replay FILE]
// DIR contains clean d14db45 release bench/layout-command binaries.
import assert from "node:assert/strict";
import {execFileSync, spawnSync} from "node:child_process";
import {createHash} from "node:crypto";
import {mkdtempSync, readFileSync, writeFileSync, rmSync, rmdirSync} from "node:fs";
import {cpus, loadavg, arch, platform, tmpdir} from "node:os";
import {join, resolve} from "node:path";
import ELK from "elkjs";
import ElkRs from "@openedges/elk-layout";
import {ROOT, normalizeArqlet, metrics} from "./lib.mjs";
import {elkLayout} from "./elk-adapter.mjs";

const args=process.argv.slice(2);
const option=(flag,fallback)=>{
  const i=args.indexOf(flag);
  if(i<0)return fallback;
  assert(i+1<args.length,`Missing ${flag}`);
  return args.splice(i,2)[1];
};
const baseline=option("--baseline",null);
const reps=Number(option("--repeat",5));
const out=option("--out",null);
const replay=option("--replay",null);
assert(baseline && out && args.length===0,"Required: --baseline DIR --out FILE; optional --repeat N --replay FILE");
assert(Number.isInteger(reps)&&reps>0);
const baselineRevision="d14db4513e7364102032306d560785d3927aef21";
const hash=bytes=>createHash("sha256").update(bytes).digest("hex");
const read=p=>JSON.parse(readFileSync(p,"utf8"));
const baselineSource=resolve(baseline,"../..");
const baselineHashes={};
for(const line of execFileSync("git",["ls-tree","-r",baselineRevision,"--","crates","Cargo.toml","Cargo.lock"],
  {cwd:ROOT,encoding:"utf8"}).trim().split("\n")) {
  const [meta,p]=line.split("\t");
  if(!/\.(rs|toml)$/.test(p)&&p!=="Cargo.lock")continue;
  const bytes=readFileSync(join(baselineSource,p));
  const blob=createHash("sha1").update(`blob ${bytes.length}\0`).update(bytes).digest("hex");
  assert.equal(blob,meta.split(" ")[2],`Baseline archive differs from clean revision: ${p}`);
  baselineHashes[p]=hash(bytes);
}
const bins={
  before:{bench:join(resolve(baseline),"bench"),command:join(resolve(baseline),"layout-command")},
  current:{bench:join(ROOT,"target/release/bench"),command:join(ROOT,"target/release/layout-command")},
};
const stats=a=>{
  const sorted=a.toSorted((a,b)=>a-b),mid=Math.floor(a.length/2);
  return {samples:a,medianMs:a.length%2?sorted[mid]:(sorted[mid-1]+sorted[mid])/2,minMs:sorted[0],maxMs:sorted.at(-1)};
};
const command=(binary,request)=>{
  const started=performance.now();
  const p=spawnSync(binary,[],{input:JSON.stringify(request),encoding:"utf8",maxBuffer:128*1024*1024,timeout:120_000});
  if(p.error)throw p.error;
  assert.equal(p.status,0,p.stderr);
  const response=JSON.parse(p.stdout);
  assert.equal(response.ok,true,response.error);
  return {snapshot:response.snapshot,ms:performance.now()-started};
};
const bench=(binary,path,core=false)=>{
  const result=spawnSync(binary,["--graph",path,"--name","focused","--limit","1000","--reps","1",
    "--threads",core?"none":"1",...(core?["--layout-only","1"]:[])],
    {cwd:ROOT,encoding:"utf8",maxBuffer:128*1024*1024,timeout:120_000});
  if(result.error)throw result.error;
  assert.equal(result.status,0,result.stderr);
  return JSON.parse(result.stdout);
};
const geometry=snapshot=>hash(JSON.stringify([snapshot.graph,snapshot.boxes,snapshot.routes,snapshot.resolvedPorts]));
const workOnly=value=>{
  if(Array.isArray(value))return value.map(workOnly);
  if(value && typeof value==="object")return Object.fromEntries(Object.entries(value)
    .filter(([k])=>!["plannedFailures","searchDetails","searchDetailsRecorded"].includes(k)&&!k.endsWith("Ms")&&!k.endsWith("Nanos")&&!k.endsWith("nanos"))
    .map(([k,v])=>[k,workOnly(v)]));
  return value;
};
function validateOutput(view,output) {
  assert.deepEqual(output.nodes.map(n=>n.id).sort(),view.nodes.map(n=>n.id).sort());
  const edges=new Map(view.edges.filter(e=>e.target!==null).map(e=>[e.id,e]));
  assert.deepEqual(output.edges.map(e=>e.id).sort(),[...edges.keys()].sort());
  const nodes=new Map(output.nodes.map(n=>[n.id,n]));
  let endpointGap=0;
  for(const e of output.edges) {
    const original=edges.get(e.id);
    assert.equal(e.source,original.source);assert.equal(e.target,original.target);
    assert(e.points.length>=2);
    assert(e.points.every(p=>Number.isFinite(p.x)&&Number.isFinite(p.y)));
    for(const [id,p]of [[e.source,e.points[0]],[e.target,e.points.at(-1)]]) {
      const n=nodes.get(id);
      const dx=Math.max(n.x-p.x,0,p.x-n.x-n.w),dy=Math.max(n.y-p.y,0,p.y-n.y-n.h);
      endpointGap=Math.max(endpointGap,Math.hypot(dx,dy));
    }
  }
  assert(endpointGap<1e-6,`endpoint gap ${endpointGap}`);
  return {endpointGap,metrics:metrics(output)};
}

let inputs;
if(replay)inputs=read(replay).inputs;
else inputs=[
  {name:"layerweave-code",path:"examples/fixtures/phase-width-feedback.json"},
  {name:"workflow-dag",path:"benchmarks/datasets/workflow-dag.json"},
  {name:"org-chart",path:"benchmarks/datasets/org-chart.json"},
].map(({name,path})=>{
  const graph=read(join(ROOT,path));
  graph.containerDirections={};graph.containerStrategies={};graph.defaultStrategy="none";
  return {name,sourcePath:path,sourceSha256:hash(readFileSync(join(ROOT,path))),graph};
});
assert.equal(inputs.length,3);
const sourceHashes={};
for(const p of ["Cargo.toml","Cargo.lock",
  ...execFileSync("git",["ls-files","--cached","--others","--exclude-standard","crates"],{cwd:ROOT,encoding:"utf8"})
    .trim().split("\n").filter(p=>/\.(rs|toml)$/.test(p))])sourceHashes[p]=hash(readFileSync(join(ROOT,p)));
const report={
  kind:"focused-public-pipeline-comparison",generatedAt:new Date().toISOString(),reps,inputs,
  environment:{cpu:cpus()[0].model,logicalCpus:cpus().length,os:platform(),arch:arch(),node:process.version,
    loadStart:loadavg(),rustc:execFileSync("rustc",["--version"],{encoding:"utf8"}).trim(),
    revision:execFileSync("git",["rev-parse","HEAD"],{cwd:ROOT,encoding:"utf8"}).trim(),dirty:true},
  baseline:{revision:baselineRevision,sourceHashes:baselineHashes,note:"Clean git archive; source blobs verified against git tree; built with release + benchmark-timings. No detached feature code."},
  binaries:Object.fromEntries(Object.entries(bins).map(([name,b])=>[name,Object.fromEntries(Object.entries(b).map(([k,p])=>[k,{path:p,sha256:hash(readFileSync(p))}]))])),
  generators:Object.fromEntries(["benchmarks/eval/focused-pipeline.mjs","benchmarks/eval/elk-adapter.mjs","benchmarks/eval/lib.mjs",
    "benchmarks/eval/package.json","benchmarks/eval/package-lock.json"].map(p=>[p,hash(readFileSync(join(ROOT,p)))])),
  sourceHashes,engineVersions:{elkjs:"0.12.0",elkRs:"0.12.1"},cases:[],
  workParityExclusions:["Clock fields ending Ms/Nanos/nanos","plannedFailures/searchDetails/searchDetailsRecorded: newer per-attempt trace fields absent from clean baseline JSON; aggregate failure/search counts, states/transitions and final delegation reasons remain compared; original reports retained"],
  note:"One warmup, rotating/reversed eight-arm order per input: before/current native bench, public import+refine, core layout, elkjs and elk-rs. Native bench clocks exclude process/JSON and report import (including input clone), public core refine and Optimize job separately. Bench ancillary edit/interaction probes are outside reported timers, but can affect subsequent load. Public calls include two process launches and full JSON request/response; combined pipeline timer includes both calls, no view setup or quality scoring. Core layout receives the same preselected graph as ELK. ELK preparation/normalization are outside layout(), reported as adapter totals. Inputs are embedded for replay; view setup excluded. Every result scored through current shared metric. Baseline/current geometry and deterministic candidate work must match. Detached routing is not installed or counted as public speedup. Shared-machine diagnostic, not idle-machine sweep or quality-equivalent engine comparison.",
};
const dir=mkdtempSync(join(tmpdir(),"arqlet-focused-"));
const elk=new ELK(),elkRs=new ElkRs();
try {
  for(const input of inputs) {
    const path=join(dir,`${input.name}.json`);
    // View selection/layout is setup, outside all reported clocks.
    const setup=command(bins.current.command,{op:"import",graph:input.graph,limit:1000}).snapshot;
    const view=setup.graph;
    writeFileSync(path,JSON.stringify(view));
    const values={};
    const outputs={};
    const native={};
    const samples={};
    const stageSamples={before:[],current:[]};
    const add=(key,value)=>{(samples[key]??=[]).push(value);};
    for(let round=0;round<=reps;round++) {
      const order=["before-native","current-native","before-public","current-public","before-core","current-core","elkjs","elkRs"];
      const shift=round%order.length;order.push(...order.splice(0,shift));
      if(round%2)order.reverse();
      for(const arm of order) {
        const [variant,operation]=arm.split("-");
        if(operation==="native") {
          const r=bench(bins[variant].bench,path);
          const deterministic={input:r.inputGeometryFingerprint,geometry:r.geometryFingerprint,
            core:r.refineCoreFingerprint,final:r.refineFingerprint,key:r.publishedKey,
            work:workOnly(r.work),candidateWork:workOnly(r.candidateWork)};
          if(values[arm])assert.deepEqual(deterministic,values[arm],`${arm}: repeated output/work`);
          else values[arm]=deterministic;
          native[variant]=r;
          if(round) {
            stageSamples[variant].push({import:r.importStages,refine:r.work["1"].stageTimesMs,
              constructorPreparationMs:r.work["1"].construction.preparationMs,
              constructorSearchMs:r.work["1"].construction.searchMs});
            add(`${arm}-import`,r.importMs.medianMs);
            add(`${arm}-refineCore`,r.refineCoreMs["1"].medianMs);
            add(`${arm}-optimize`,r.refineMs["1"].medianMs);
          }
        } else if(operation==="core") {
          const r=bench(bins[variant].bench,path,true);
          const g=geometry(r.snapshot);
          if(values[arm])assert.equal(g,values[arm]);else values[arm]=g;
          outputs[arm]=normalizeArqlet(r.snapshot);
          if(round)add(arm,r.layoutMs.medianMs);
        } else if(operation==="public") {
          const started=performance.now();
          const initial=command(bins[variant].command,{op:"import",graph:view,limit:1000});
          const refined=command(bins[variant].command,{op:"refine",snapshot:initial.snapshot,
            expectedRevision:initial.snapshot.revision,expectedLayoutVersion:initial.snapshot.layoutVersion??0,threads:1});
          const total=performance.now()-started;
          const g=[geometry(initial.snapshot),geometry(refined.snapshot)];
          if(values[arm])assert.deepEqual(g,values[arm]);else values[arm]=g;
          outputs[`${arm}-initial`]=normalizeArqlet(initial.snapshot);
          outputs[`${arm}-refined`]=normalizeArqlet(refined.snapshot);
          if(round){add(`${arm}-import`,initial.ms);add(`${arm}-refine`,refined.ms);add(`${arm}-pipeline`,total);}
        } else {
          try {
            const r=await elkLayout(view,arm==="elkjs"?elk:elkRs);
            const output={nodes:r.nodes,edges:r.edges};
            const fingerprint=hash(JSON.stringify(output));
            if(values[arm])assert.equal(fingerprint,values[arm]);else values[arm]=fingerprint;
            outputs[arm]=output;
            if(round){add(arm,r.ms);add(`${arm}-adapter`,r.totalMs);}
          } catch(error) {
            if(error instanceof assert.AssertionError)throw error;
            (values[`${arm}-failures`]??=[]).push({round,error:String(error)});
          }
        }
      }
    }
    assert.deepEqual(values["before-native"],values["current-native"],"native before/current full output/work");
    assert.deepEqual(values["before-public"],values["current-public"],"public before/current geometry");
    assert.equal(values["before-core"],values["current-core"],"core before/current geometry");
    assert.equal(values["current-core"],values["current-public"][0],"core/public initial geometry");
    const quality=Object.fromEntries(Object.entries(outputs).map(([name,output])=>{
      try {return [name,validateOutput(view,output)];}
      catch(error) {
        if(!["elkjs","elkRs"].includes(name))throw error;
        console.error(`${input.name} ${name}: quality validation failed: ${error.message}`);
        return [name,{failed:error.message,output}];
      }
    }));
    for(const key of ["core","public-initial","public-refined"])
      assert.deepEqual(quality[`before-${key}`],quality[`current-${key}`],`${key}: full quality parity`);
    const row={name:input.name,inputSha256:hash(JSON.stringify(view)),view,
      sourceNodes:input.graph.nodes.length,viewNodes:view.nodes.length,viewEdges:view.edges.length,
      effectiveOptions:setup.options,timings:Object.fromEntries(Object.entries(samples).map(([k,a])=>[k,stats(a)])),
      quality,identity:values,native,stageSamples,load:loadavg(),checks:{beforeCurrentOutputAndWork:true,beforeCurrentFullQuality:true,repeatedOutputAndWork:true,
        coreMatchesPublicInitial:true,nativeLogicalDirection:true,nativeCompleteEdgePartition:true,nativeEndpointsOnNodes:true,
        externalQualityFailures:Object.entries(quality).filter(([,q])=>q.failed).map(([name])=>name)}};
    report.cases.push(row);
    console.error(`${input.name}: ${view.nodes.length} nodes, ${view.edges.length} edges, all native parity checks passed`);
  }
} finally {
  for(const input of inputs)rmSync(join(dir,`${input.name}.json`),{force:true});
  rmdirSync(dir);
}
report.environment.loadEnd=loadavg();
assert.equal(report.cases.length,3);
for(const [p,h]of Object.entries(sourceHashes))assert.equal(hash(readFileSync(join(ROOT,p))),h,`source changed during measurement: ${p}`);
writeFileSync(out,JSON.stringify(report)+"\n");
console.log(`Saved ${out}`);
