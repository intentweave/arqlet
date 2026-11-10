// Complete metric measurements, not layout/refinement or transport timings.
import assert from "node:assert/strict";
import {execFileSync} from "node:child_process";
import {createHash} from "node:crypto";
import {readFileSync} from "node:fs";
import {cpus, loadavg, platform, arch} from "node:os";
import {basename, dirname, join, relative, resolve} from "node:path";
import {fileURLToPath} from "node:url";

const root=resolve(dirname(fileURLToPath(import.meta.url)),"..");
const binary=resolve(process.argv[2]??join(root,"target/fast/examples/metric-pairs"));
const reps=Number(process.argv[3]??8);
assert(Number.isInteger(reps)&&reps>0);
const source="benchmarks/results/abstract-routing-coordination-reuse-20261104.jsonl";
const frozen=JSON.parse(readFileSync(join(root,source),"utf8"));
const graph=JSON.parse(readFileSync(join(root,frozen.input.path),"utf8"));
assert.equal(createHash("sha256").update(readFileSync(join(root,frozen.input.path))).digest("hex"),frozen.input.sha256);
const containers=new Set(graph.nodes.map(n=>n.parentId).filter(Boolean));
const cases=Object.entries(frozen.variants).map(([name,v])=>{
  const nodes=new Map(v.scene.nodes.map(n=>[n.id,n.bounds]));
  const routes=new Map(v.scene.routes.map(r=>[r.id,r]));
  const ports=new Map(v.scene.ports.map(p=>[`${p.edgeId}:${p.endpoint}`,p.id]));
  return {
    name:`frozen-${name}`,
    drawing:{
      nodes:graph.nodes.map(n=>({id:n.id,parentId:n.parentId,container:containers.has(n.id),...nodes.get(n.id)})),
      edges:graph.edges.map(e=>{
        const r=routes.get(e.id);
        assert(r);
        return {id:r.id,source:r.source,target:r.target,points:r.points,
          sourcePort:ports.get(`${e.id}:source`),targetPort:ports.get(`${e.id}:target`)};
      }),
      trackSpacing:frozen.inputOptions.trackSpacing,
    },
  };
});
function synthetic(name,count,dense) {
  const drawing={nodes:[{id:"a",x:-20,y:-20,width:10,height:10}],edges:[],trackSpacing:6};
  for(let i=0;i<count;i++) {
    const x=dense?0:(i%32)*100, y=dense?0:Math.floor(i/32)*100;
    drawing.edges.push({id:`e${i}`,source:"a",target:"a",points:[{x,y},{x:x+10,y}]});
  }
  return {name,drawing};
}
cases.push(synthetic("sparse-256",256,false),synthetic("sparse-1024",1024,false),
  synthetic("dense-256",256,true));
const input=JSON.stringify(cases);
const result=JSON.parse(execFileSync(binary,[String(reps)],{
  cwd:root,input,encoding:"utf8",maxBuffer:8*1024*1024,
}));
assert.equal(result.cases.length,cases.length);
const median=values=>{
  const a=values.toSorted((a,b)=>a-b),mid=Math.floor(a.length/2);
  return a.length%2?a[mid]:(a[mid-1]+a[mid])/2;
};
for (const [i,c] of result.cases.entries()) {
  assert.equal(c.name,cases[i].name);
  assert.equal(c.exhaustiveMs.length,reps);
  assert.equal(c.indexedMs.length,reps);
  assert.equal(c.sharedMs.length,reps);
  assert.equal(c.exhaustiveWork.boundsTests,c.edges*(c.edges-1)/2);
  assert.equal(c.exhaustiveWork.candidatePairs,c.indexedWork.candidatePairs);
  assert.equal(c.sharedWork.candidatePairs,c.indexedWork.candidatePairs);
  assert.equal(c.sharedWork.boundsTests,c.exhaustiveWork.boundsTests);
  assert.equal(c.sharedWork.segmentPairVisits*2,c.exhaustiveWork.segmentPairVisits);
  assert.equal(c.sharedWork.segmentPairVisits,c.indexedWork.segmentPairVisits);
  assert.equal(c.sharedWork.segmentBoundsComputed,c.indexedWork.segmentBoundsComputed);
  assert([...c.exhaustiveMs,...c.sharedMs,...c.indexedMs].every(t=>Number.isFinite(t)&&t>0));
  if(c.name.startsWith("frozen-")) {
    const old=frozen.variants[c.name.slice("frozen-".length)].metrics;
    c.historicalMetricDifferences=Object.keys(old).filter(k=>c.metrics[0][k]!==old[k])
      .map(field=>({field,snapshot:old[field],sceneDerived:c.metrics[0][field]}));
    assert(c.historicalMetricDifferences.every(d=>["hugPairs","hugLength"].includes(d.field)),
      `${c.name}: historical pair/geometry metrics must match`);
  }
  if(c.name.startsWith("sparse-")) {
    assert.equal(c.indexedWork.candidatePairs,0);
    assert(c.indexedWork.boundsTests<c.exhaustiveWork.boundsTests/8);
  }
  if(c.name.startsWith("dense-")) assert.equal(c.indexedWork.candidatePairs,c.exhaustiveWork.boundsTests);
  c.exhaustiveMedianMs=median(c.exhaustiveMs);
  c.indexedMedianMs=median(c.indexedMs);
  c.sharedMedianMs=median(c.sharedMs);
}
const hash=bytes=>createHash("sha256").update(bytes).digest("hex");
const hashes={};
for(const p of ["Cargo.toml","Cargo.lock",
  ...execFileSync("git",["ls-files","--cached","--others","--exclude-standard","crates"],
    {cwd:root,encoding:"utf8"}).trim().split("\n").filter(p=>/\.(rs|toml)$/.test(p))]) {
  hashes[p]=hash(readFileSync(join(root,p)));
}
result.kind="exact-metric-pair-generation";
result.generatedAt=new Date().toISOString();
result.environment={
  cpu:cpus()[0].model,logicalCpus:cpus().length,os:platform(),arch:arch(),loadAverage:loadavg(),
  rustc:execFileSync("rustc",["--version"],{encoding:"utf8"}).trim(),
  gitRevision:execFileSync("git",["rev-parse","HEAD"],{cwd:root,encoding:"utf8"}).trim(),
  gitDirty:execFileSync("git",["status","--porcelain"],{cwd:root,encoding:"utf8"}).trim().length>0,
};
result.binary={path:relative(root,binary),sha256:hash(readFileSync(binary)),profile:basename(dirname(dirname(binary)))};
result.generator={path:relative(root,fileURLToPath(import.meta.url)),sha256:hash(readFileSync(fileURLToPath(import.meta.url)))};
result.source={path:source,sha256:hash(readFileSync(join(root,source)))};
result.graph={path:frozen.input.path,sha256:frozen.input.sha256};
result.input={sha256:hash(input),synthetic:"Fixed grid of 10-unit segments, stride 100; dense cases place all segments at the origin. No RNG."};
result.sourceHashes=hashes;
result.command=[relative(root,binary),String(reps)];
result.note="One warmup, three rotated/reversed arms: previous exhaustive iteration/pair evaluator, exhaustive iteration with shared segment bounds and one overlap/crossing pass (production), and diagnostic spatial route-pair generation with that same shared evaluator. Complete validated metrics for both clearances; normalized routes, index construction/querying/sorting where applicable, node interactions and metric aggregation included. Work counters collected outside clocks; timed arms are uninstrumented. Segment visits count loop slots in overlap/parallel/crossing passes, excluding touch checks. Identical (i,j) accumulation order preserves floating sums. No cache between calls, search budget or quality change. Dense route envelopes retain quadratic candidate work; index overhead is not hidden or enabled in production. Frozen inputs reconstruct published drawing order, container membership and resolved port identities from stored scenes. Scene-derived node hugging differs from the original snapshot evaluation; every difference is recorded explicitly. All other historical fields and all three arms on the same input match exactly; native snapshot keys/output parity is checked separately. Shared-machine diagnostic, not refinement or a controlled end-to-end speedup.";
result.checks={completeMetricParity:true,historicalPairMetricParity:true,sceneDerivedClearanceDifferencesDisclosed:true,exactPairCounts:true,repeatedParity:true,sparseWorkReduction:true,denseWorkExplicit:true};
console.log(JSON.stringify(result));
