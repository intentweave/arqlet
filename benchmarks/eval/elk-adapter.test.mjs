import assert from "node:assert/strict";
import test from "node:test";
import {elkLayout} from "./elk-adapter.mjs";

const view={
  defaultDirection:"lr",
  nodes:[{id:"g"},{id:"a",parentId:"g"},{id:"b",parentId:"g"}],
  edges:[{id:"loop",source:"a",target:"a"},{id:"ab",source:"a",target:"b"}],
};
const section=(x,y)=>({startPoint:{x,y},endPoint:{x:x+10,y}});
function engine(metadata=true) {
  return {layout:async input=>{
    assert.equal(input.layoutOptions["elk.hierarchyHandling"],"INCLUDE_CHILDREN");
    return {
      id:"root",children:[{id:"g",x:100,y:200,width:300,height:200,children:[
        {id:"a",x:20,y:30,width:180,height:54},{id:"b",x:20,y:100,width:180,height:54},
      ]}],
      edges:[
        {id:"loop",sources:["a"],targets:["a"],...(metadata?{container:"g"}:{}),sections:[section(40,30)]},
        {id:"ab",sources:["a"],targets:["b"],...(metadata?{container:"g"}:{}),sections:[section(50,84)]},
      ],
    };
  }};
}
for(const metadata of [false,true])test(`nested/self-loop frames, metadata ${metadata}`,async()=>{
  const result=await elkLayout(view,engine(metadata));
  assert.deepEqual(result.edges[0].points,[{x:140,y:230},{x:150,y:230}]);
  assert.deepEqual(result.edges[1].points,[{x:150,y:284},{x:160,y:284}]);
  assert.equal(result.nodes.find(n=>n.id==="a").x,120);
});
test("unknown frames and multiple sections fail explicitly",async()=>{
  const original=engine().layout;
  for(const kind of ["frame","sections"])await assert.rejects(elkLayout(view,{layout:async input=>{
    const laid=await original(input);
    if(kind==="frame")laid.edges[0].container="missing";
    else laid.edges[0].sections.push(section(0,0));
    return laid;
  }}),kind==="frame"?/unknown coordinate frame/:/exactly one section/);
});

test("container directions: own direction per container, separate children when mixed",async()=>{
  const seen=[];
  const capture={layout:async input=>{seen.push(input);return engine().layout({...input,layoutOptions:{...input.layoutOptions,"elk.hierarchyHandling":"INCLUDE_CHILDREN"}});}};
  await elkLayout({...view,containerDirections:{g:"tb"}},capture,{containerDirections:true});
  assert.equal(seen[0].layoutOptions["elk.hierarchyHandling"],"SEPARATE_CHILDREN");
  assert.equal(seen[0].children[0].layoutOptions["elk.direction"],"DOWN");
  await elkLayout({...view,containerDirections:{g:"lr"}},capture,{containerDirections:true});
  assert.equal(seen[1].layoutOptions["elk.hierarchyHandling"],"INCLUDE_CHILDREN");
  await elkLayout({...view,containerDirections:{g:"tb"}},capture);
  assert.equal(seen[2].layoutOptions["elk.hierarchyHandling"],"INCLUDE_CHILDREN");
  assert.equal(seen[2].children[0].layoutOptions["elk.direction"],undefined);
});
