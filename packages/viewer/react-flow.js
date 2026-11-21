/** Pure adapter: no React dependency and no secondary routing. */
export function toReactFlow(scene,{overlayNode=false}={}){
 if(scene.schemaVersion!==1)throw Error('Unsupported scene schema');
 const byId=new Map(scene.nodes.map(n=>[n.id,n])),handles=new Map(scene.nodes.map(n=>[n.id,[]]));
 for(const route of scene.routes){
  for(const [endpoint,id,point] of [['source',route.source,route.points[0]],['target',route.target,route.points.at(-1)]]){
   if(!point)continue;
   const n=byId.get(id);const port=scene.ports.find(p=>p.edgeId===route.id&&p.endpoint===endpoint);
   handles.get(id).push({id:`${endpoint}:${route.id}`,type:endpoint,side:port?.side??(endpoint==='source'?'right':'left'),x:point.x-n.bounds.x,y:point.y-n.bounds.y});
  }
 }
 const depth=n=>{let d=0;while(n.parentId){n=byId.get(n.parentId);d++;}return d;};
 const nodes=[...scene.nodes].sort((a,b)=>depth(a)-depth(b)).map(n=>{
  const p=byId.get(n.parentId)?.bounds??{x:0,y:0};
  return {id:n.id,type:'arqlet',parentId:n.parentId??undefined,position:{x:n.bounds.x-p.x,y:n.bounds.y-p.y},
    width:n.bounds.width,height:n.bounds.height,style:{width:n.bounds.width,height:n.bounds.height},
    draggable:false,connectable:false,deletable:false,zIndex:n.container?0:2,
    data:{...n,handles:handles.get(n.id),showPorts:scene.showPorts}};
 });
 const edges=scene.routes.filter(r=>r.points.length>=2).map(r=>({id:r.id,source:r.source,target:r.target,sourceHandle:`source:${r.id}`,targetHandle:`target:${r.id}`,type:'arqlet',zIndex:1,deletable:false,data:r}));
 // What is neither a node nor an edge between two nodes, in flow coordinates (the scene's): bus bars with their taps, the
 // junction dots where shared lines branch, and the stubs of hub references.
 const overlays={busLines:scene.busLines??[],junctions:scene.junctions??[],hubRefs:scene.hubRefs??[]};
 if(overlayNode&&(overlays.busLines.length||overlays.junctions.length||overlays.hubRefs.length)){
  // One node over the whole scene that draws them (type `arqletOverlay`); `origin` turns flow into node coordinates.
  const b=scene.bounds;
  nodes.push({id:'arqlet:overlay',type:'arqletOverlay',position:{x:b.x,y:b.y},width:b.width,height:b.height,
   style:{width:b.width,height:b.height,pointerEvents:'none'},draggable:false,selectable:false,connectable:false,
   deletable:false,focusable:false,zIndex:1,data:{...overlays,origin:{x:b.x,y:b.y}}});
 }
 return {nodes,edges,labels:scene.labels,crossings:scene.crossings,...overlays};
}
