import {sceneOf} from '@arqlet/engine-client/engine.js';
/** Both panels use this same evaluator, not historical cached quality fields. */
export function evaluateSnapshot(run,snapshot){
 const scene=sceneOf(run,snapshot);
 const routes=new Map(snapshot.routes.map(r=>[r.id,r]));
 for(const edge of snapshot.graph.edges.filter(e=>e.target!=null)){
  const route=routes.get(edge.id),drawn=route?.bundle_of?routes.get(route.bundle_of):route;
  if(!drawn||drawn.points.length<2)throw Error(`Missing route: ${edge.id}`);
 }
 const result=run({op:'metrics',drawing:{nodes:scene.nodes.map(n=>({id:n.id,parentId:n.parentId,container:n.container,...n.bounds})),edges:scene.routes}});
 if(!result.ok)throw Error(result.error);
 const identity=JSON.stringify({nodes:snapshot.graph.nodes.map(n=>[n.id,n.parentId??null]).sort(),edges:snapshot.graph.edges.map(e=>[e.id,e.source,e.target]).sort()});
 return {scene,metrics:result.metrics,scale:1,identity};
}
