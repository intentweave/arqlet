import {createStyle, glyphOf, routeToGlyphs} from './style.js';
/** Pure adapter: no React dependency and no secondary routing. */
export function toReactFlow(scene,{overlayNode=false,draggable=false,style:sheet}={}){
 // With a style (style.js), every node and edge carries its resolved look (`data.look`), the theme filled in, so a host's
 // node and edge types draw what renderSvg and the GPU layer draw.
 const style=sheet?createStyle(sheet):null,T=style?.theme;
 const looks=style?.bind(scene);
 const nodeLook=n=>{const o=looks.node(n);return {fill:o.fill??(n.container?T.frameFill:T.leafFill),stroke:o.stroke??(n.container?T.frameStroke:T.leafStroke),text:o.text??(n.container?T.title:T.text),
  strokeWidth:o.strokeWidth??1,...(o.dash?{dash:o.dash}:{}),...(o.radius!==undefined?{radius:o.radius}:{}),...(o.glyph?{glyph:o.glyph,glyphSize:o.glyphSize??7}:{}),selectedFill:T.selectedFill,selectedStroke:T.selectedStroke,invalid:T.invalid};};
 // A glyph's routes run on to the dot's middle (`routeToGlyphs`); the look carries those points.
 const glyphs=new Map();
 if(style)for(const n of scene.nodes)if(!n.container&&n.kind!=='attribute'&&!n.rows?.length){const g=glyphOf(looks.node(n),n.bounds);if(g)glyphs.set(n.id,g);}
 const edgeLook=r=>{const o=looks.edge(r),gs=glyphs.get(r.source),gt=glyphs.get(r.target);return {...(gs||gt?{points:routeToGlyphs(r.points,gs,gt)}:{}),stroke:o.stroke??T.route,arrow:o.arrow??T.arrow,...(o.width!==undefined?{width:o.width}:{}),...(o.opacity!==undefined?{opacity:o.opacity}:{}),...(o.dash?{dash:o.dash}:{}),invalid:T.invalid};};
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
    // With `draggable`, leaves can be dragged (a host passes the drag to `session.drag`); containers stay fixed.
    draggable:draggable&&!n.container,connectable:false,deletable:false,zIndex:n.container?0:2,
    data:{...n,handles:handles.get(n.id),showPorts:scene.showPorts,...(style?{look:nodeLook(n)}:{})}};
 });
 const edges=scene.routes.filter(r=>r.points.length>=2).map(r=>({id:r.id,source:r.source,target:r.target,sourceHandle:`source:${r.id}`,targetHandle:`target:${r.id}`,type:'arqlet',zIndex:1,deletable:false,data:style?{...r,look:edgeLook(r)}:r}));
 // What is neither a node nor an edge between two nodes, in flow coordinates (the scene's): bus bars with their taps, the
 // junction dots where shared lines branch, the stubs of hub references and the membership hulls.
 // Membership hulls (ADR 0022) too: polygons behind the nodes, one hue per dimension as renderSvg draws them.
 const overlays={busLines:scene.busLines??[],junctions:scene.junctions??[],hubRefs:scene.hubRefs??[],hulls:scene.membershipHulls??[]};
 if(overlayNode&&(overlays.busLines.length||overlays.junctions.length||overlays.hubRefs.length||overlays.hulls.length)){
  // One node over the whole scene that draws them (type `arqletOverlay`); `origin` turns flow into node coordinates.
  const b=scene.bounds;
  nodes.push({id:'arqlet:overlay',type:'arqletOverlay',position:{x:b.x,y:b.y},width:b.width,height:b.height,
   style:{width:b.width,height:b.height,pointerEvents:'none'},draggable:false,selectable:false,connectable:false,
   deletable:false,focusable:false,zIndex:1,data:{...overlays,origin:{x:b.x,y:b.y}}});
 }
 return {nodes,edges,labels:scene.labels,crossings:scene.crossings,...overlays};
}
