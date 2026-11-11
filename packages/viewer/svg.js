let viewerId=0;
const markerIds=new WeakMap();
/** Paint a scene. Selection is an event; this module never mutates or computes layout. */
const HOVER_STROKE='#7fb4ff';
// The signature of each element the viewer put into a host (its markup when it was made), and whether the host's last drawing was
// kept element by element.
const signatures=new WeakMap();
const keptHosts=new WeakSet();
const ELEMENT_NS='http://www.w3.org/2000/svg';
/**
 * Paints a scene into `host`. With `keyed: true` (and a real DOM) the drawing is made aside and then compared with what the host holds:
 * an element whose markup is unchanged stays where it is, so a press on it, a hover or a capture is not lost when a drag redraws
 * the scene, and only the elements that differ are replaced. The default is a complete drawing: on a view of 548 nodes the comparison
 * by markup costs about eight times what drawing everything again costs (median 480 to 550 ms against 59 to 63 ms from the pointer to
 * the preview, `browser-checks/latency.mjs`), because it serialises every element. A cheaper key would have to come from the scene's
 * data before the elements are made. The first keyed drawing, a drawing after one that was not keyed, and a host without a real DOM
 * are drawn completely. A kept element keeps the handlers it was made with; they call the host's
 * functions, which read the current state, and anything they show is part of the markup that is compared.
 */
export function renderSvg(host,scene,options={}){
 const canKeep=options.keyed===true&&typeof host.insertBefore==='function'&&typeof host.removeChild==='function';
 if(!canKeep){keptHosts.delete(host);const out=drawScene(host,host,scene,options);markRests(host,scene);if(options.trace)applyTrace(host,options.trace);return out;}
 const document=options.document??host.ownerDocument;
 const stage=document.createElementNS(ELEMENT_NS,'svg');
 drawScene(stage,host,scene,options);
 markRests(stage,scene);
 if(options.trace)applyTrace(stage,options.trace);
 const next=[...stage.children];
 if(!keptHosts.has(host)){
  host.replaceChildren(...next);
  for(const el of next)signatures.set(el,el.outerHTML);
  keptHosts.add(host);
  return;
 }
 const pool=new Map();
 for(const el of [...host.children]){const sig=signatures.get(el)??el.outerHTML;if(!pool.has(sig))pool.set(sig,[]);pool.get(sig).push(el);}
 const wanted=next.map(el=>{const sig=el.outerHTML;const reuse=pool.get(sig)?.shift()??el;signatures.set(reuse,sig);return reuse;});
 wanted.forEach((el,i)=>{const at=host.children[i];if(at!==el)host.insertBefore(el,at??null);});
 while(host.children.length>wanted.length)host.removeChild(host.children[host.children.length-1]);
}
// Draws the scene's elements into `svg` (the host, or a stage that is compared with it afterwards); the host carries the view box
// and the arrow marker's identity.
function drawScene(svg,host,scene,{onPortDragStart,onLabelDragStart,selected=null,selectedRoute=null,pinned=new Set(),sized=new Map(),onSelect,onSelectRoute,onNodeDragStart,onToggle,onPin,onReset,onResize,scale=1,document=host.ownerDocument}){
 if(scene.schemaVersion!==1)throw Error('Unsupported scene schema');
 const ns='http://www.w3.org/2000/svg';
 const element=(tag,attrs={},text)=>{const e=document.createElementNS(ns,tag);for(const [k,v] of Object.entries(attrs))e.setAttribute(k,v);if(text!==undefined)e.textContent=text;return e;};
 let markerId=markerIds.get(host);if(!markerId){markerId=`lw-arrow-${++viewerId}`;markerIds.set(host,markerId);}
 svg.replaceChildren();const r=scene.bounds;
 host.setAttribute('viewBox',`${r.x} ${r.y} ${r.width} ${r.height}`);host.setAttribute('width',r.width*scale);host.setAttribute('height',r.height*scale);
 const defs=element('defs'),marker=element('marker',{id:markerId,viewBox:'0 0 10 10',refX:9,refY:5,markerUnits:'userSpaceOnUse',markerWidth:8,markerHeight:8,orient:'auto-start-reverse'});marker.append(element('path',{d:'M 0 0 L 10 5 L 0 10 z',fill:'#73bba7'}));defs.append(marker);svg.append(defs);
 const nodes=new Map(scene.nodes.map(n=>[n.id,n])),absolute=new Map(scene.nodes.map(n=>[n.id,n.bounds]));
 const parents=new Set(scene.nodes.filter(n=>n.container).map(n=>n.id));
 // Every party of every conflict is marked, whatever its kind: the nodes a conflict names (also a node a route runs through) and
 // its routes, each with the reasons as its hint.
 const conflictReasons=new Map(),conflictedRoutes=new Set();
 for(const c of scene.conflicts??[]){
  for(const id of c.nodes??[]){if(!conflictReasons.has(id))conflictReasons.set(id,[]);conflictReasons.get(id).push(c.reason);}
  for(const id of c.routes??[])conflictedRoutes.add(id);
 }
 const invalid=new Set([...scene.nodes.filter(n=>n.invalid).map(n=>n.id),...conflictReasons.keys()]);
 const hint=id=>conflictReasons.has(id)?element('title',{},conflictReasons.get(id).join('\n')):null;
 const collapsed=new Set(scene.nodes.filter(n=>n.collapsed).map(n=>n.id));
 const choose=(n,e)=>onSelect?.(n.id,e);
 const depth=n=>{let d=0;while(n.parentId){d++;n=nodes.get(n.parentId);}return d;};
 // Optional controls, drawn only when the host handles them: collapse or expand (`onToggle(id)`) and pin or unpin the
 // position (`onPin(id, pin)`). A control is its own target: a press on it never starts a drag, a pan or a selection.
 // Its size follows the screen (16 px) within the box it sits in.
 const controlSize=limit=>Math.max(4,Math.min(Math.max(16/scale,14),limit));
 const control=(x,y,size,label,action,attrs,down)=>{
  const g=element('g',{class:'lw-control',role:'button',tabindex:0,'aria-label':label,...attrs});
  g.append(element('rect',{x,y,width:size,height:size,rx:size/4,fill:'transparent'}),element('title',{},label));
  g.onpointerdown=e=>{e.stopPropagation();down?.(e);};
  g.onclick=e=>{e.stopPropagation();action();};
  g.onkeydown=e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();e.stopPropagation();action();}};
  return g;
 };
 const toggleControl=(n,x,y,size)=>{
  const open=!collapsed.has(n.id);
  const c=control(x,y,size,`${open?'Collapse':'Expand'} ${n.label}`,()=>onToggle(n.id),{'data-toggle-id':n.id});
  const at=(u,v)=>`${x+u*size},${y+v*size}`;
  c.append(element('polygon',{points:open?[at(.25,.38),at(.75,.38),at(.5,.7)].join(' '):[at(.38,.25),at(.7,.5),at(.38,.75)].join(' '),fill:'#c4d6e8','pointer-events':'none'}));
  return c;
 };
 // Reset to content, one per authored axis (`sized` maps a container to its axes, ADR 0010): the width and the height
 // each have their own control, and `onReset(id, axis)` removes that axis only.
 const resetControl=(n,axis,x,y,size)=>{
  const word=axis==='width'?'width':'height';
  const c=control(x,y,size,`Reset the ${word} of ${n.label} to content`,()=>onReset(n.id,axis),{class:'lw-control lw-reset','data-reset-id':n.id,'data-reset-axis':axis});
  c.append(element('text',{x:x+size/2,y:y+size*.78,'text-anchor':'middle','font-size':size*.9,fill:'#c4d6e8','pointer-events':'none'},axis==='width'?'↔':'↕'));
  return c;
 };
 const pinControl=(n,x,y,size)=>{
  const on=pinned.has(n.id),colour=on?'#edc477':'#8aa2bb',cx=x+size/2;
  // Dragging the pin moves the node and fixes where it is dropped; a press without movement toggles the pin. A drag on
  // the node itself only moves it (the host decides: `onNodeDragStart(node, event, {fromPin: true})`).
  const c=control(x,y,size,`${on?'Unpin':'Pin'} the position of ${n.label}${onNodeDragStart?' (drag the pin to fix a new position)':''}`,()=>onPin(n.id,!on),{class:on?'lw-control lw-pin lw-pinned':'lw-control lw-pin','data-pin-id':n.id,...(onNodeDragStart?{style:'cursor:grab'}:{})},onNodeDragStart?e=>onNodeDragStart(n,e,{fromPin:true}):undefined);
  c.append(element('line',{x1:cx,y1:y+size*.45,x2:cx,y2:y+size*.95,stroke:colour,'stroke-width':size*.1,'stroke-linecap':'round','pointer-events':'none'}));
  c.append(element('circle',{cx,cy:y+size*.35,r:size*.25,fill:on?colour:'none',stroke:colour,'stroke-width':size*.1,'pointer-events':'none'}));
  return c;
 };
 const rings=[],top=[];
 for(const n of [...nodes.values()].filter(n=>parents.has(n.id)).sort((a,b)=>depth(a)-depth(b))){const r=absolute.get(n.id);
  // A circle container is a disc inscribed in its square box, like a circle leaf; its controls sit on the disc.
  const round=n.shape==='circle',mx=r.x+r.width/2,my=r.y+r.height/2,rad=Math.min(r.width,r.height)/2,on45=rad*.62;
  const frame=round?element('circle',{cx:mx,cy:my,r:rad,fill:'#142133',stroke:invalid.has(n.id)?'#f08f80':'#344961','data-container-id':n.id,...(onNodeDragStart?{'pointer-events':'none'}:{})})
   :element('rect',{...r,rx:10,fill:'#142133',stroke:invalid.has(n.id)?'#f08f80':'#344961','data-container-id':n.id,...(onNodeDragStart?{'pointer-events':'none'}:{})});if(pinned.has(n.id))frame.setAttribute('data-pinned-id',n.id);{const why=hint(n.id);if(why)frame.append(why);}svg.append(frame);if(n.kind==='entity')svg.append(element('line',{x1:r.x,y1:r.y+34,x2:r.x+r.width,y2:r.y+34,stroke:'#344961','pointer-events':'none'}));
  const size=controlSize(Math.min(24,r.width/4));
  // The inside of the frame, 18 screen pixels wide, is the handle of a container: a press there starts a move, as a press on
  // a leaf does (ADR 0009, section 4). Top-level containers move the same way; their pins are in the view's frame. Hovering
  // the ring lights the frame, so the handle is visible.
  if(onNodeDragStart){
   const t=Math.min(Math.max(18/scale,8),r.width/2,r.height/2);
   const frameStroke=invalid.has(n.id)?'#f08f80':'#344961';
   // A rectangle has four strips along its sides; a disc has one ring of the same width inside its border.
   const zones=round?[{ring:true}]:[{x:r.x,y:r.y,width:r.width,height:t},{x:r.x,y:r.y+r.height-t,width:r.width,height:t},{x:r.x,y:r.y,width:t,height:r.height},{x:r.x+r.width-t,y:r.y,width:t,height:r.height}];
   for(const z of zones){
    const zone=z.ring?element('circle',{cx:mx,cy:my,r:Math.max(rad-t/2,1),fill:'none',stroke:'transparent','stroke-width':t,'pointer-events':'stroke',class:'lw-header','data-header-id':n.id,style:'cursor:move'})
     :element('rect',{...z,fill:'transparent',class:'lw-header','data-header-id':n.id,style:'cursor:move'});
    zone.onpointerdown=e=>onNodeDragStart(n,e);
    zone.onpointerenter=()=>frame.setAttribute('stroke',HOVER_STROKE);
    zone.onpointerleave=()=>frame.setAttribute('stroke',frameStroke);
    rings.push(zone);
   }
  }
  const title=element('text',round?{x:mx,y:my-rad+Math.min(34,rad*.3),'text-anchor':'middle',fill:'#9eb8d1','font-size':14,tabindex:0,role:'button',style:'cursor:pointer','data-container-id':n.id}:{x:onToggle?r.x+10+size:r.x+12,y:r.y+25,fill:'#9eb8d1','font-size':14,tabindex:0,role:'button',style:'cursor:pointer','data-container-id':n.id},(onToggle||round?'':'▾ ')+n.label.slice(0,55));title.onclick=e=>choose(n,e);
  // The header is the handle of a container: dragging it moves the container with its children (ADR 0009, section 4).
  if(onNodeDragStart)title.onpointerdown=e=>onNodeDragStart(n,e);title.onkeydown=e=>{if(e.key==='Enter')choose(n,e);};top.push(title);
  if(onToggle)top.push(round?toggleControl(n,mx-on45-size/2,my-on45-size/2,size):toggleControl(n,r.x+6,r.y+6,size));
  if(onPin)top.push(round?pinControl(n,mx+on45-size/2,my-on45-size/2,size):pinControl(n,r.x+r.width-size-6,r.y+6,size));
  // The resize handle at the bottom-right corner: dragging it sets the container's minimum size (ADR 0010, decision 3).
  if(onResize){const handle=element('rect',{x:round?mx+on45-6:r.x+r.width-14,y:round?my+on45-6:r.y+r.height-14,width:12,height:12,rx:2,fill:'#8aa2bb',class:'lw-resize','data-resize-id':n.id,style:'cursor:nwse-resize'});handle.onpointerdown=e=>{e.stopPropagation();onResize(n,e);};top.push(handle);}
  if(onReset)(sized.get(n.id)??[]).forEach((axis,index)=>top.push(round?resetControl(n,axis,mx-(index+1)*size-4,my+rad-size-10,size):resetControl(n,axis,r.x+r.width-(index+2)*size-12,r.y+6,size)));
 }
 // The selected route is drawn above the others; the other routes are dimmed. A wide transparent line makes a thin route clickable.
 const active=scene.routes.find(r=>r.id===selectedRoute),endpoints=new Set(active?[active.source,active.target]:[]);
 for(const route of scene.routes){const weight=route.weight??1,isActive=route===active,dim=!!active&&!isActive;
  // The engine's points already contain the arcs of a rounded route (crates/scene realize.rs); they are drawn as given.
  const points=route.points.map(p=>`${p.x},${p.y}`).join(' ');
  // A route that stands for collapsed content (communities) carries its edges in both directions: it grows wider and
  // more opaque with their number (logarithmically, so strong links stand out), and has an arrow at the start too when
  // some run back.
  const aggregated=route.backWeight!==undefined&&route.backWeight!==null,back=aggregated?route.backWeight:0,total=weight+back;
  const width=aggregated?Math.min(7,1+1.2*Math.log2(total+1)):Math.min(4,1.6+0.4*(weight-1));
  const opacity=aggregated?0.3+0.55*Math.min(1,Math.log2(total+1)/6):.75;
  const line=element('polyline',{'data-route-id':route.id,points,fill:'none',stroke:route.invalid||conflictedRoutes.has(route.id)?'#f08f80':isActive?'#c8f5e4':'#73bba7','stroke-width':width+(isActive?1.5:0),'stroke-opacity':dim?.2:isActive?1:opacity,'marker-end':`url(#${markerId})`,...(back>0?{'marker-start':`url(#${markerId})`}:{}),'pointer-events':'none'});
  if(aggregated&&total>1){line.append(element('title',{},`${weight} edge${weight===1?'':'s'} ${route.source} → ${route.target}${back?`, ${back} back`:''}`));const mid=route.points[Math.floor(route.points.length/2)];svg.append(element('text',{x:mid.x+6,y:mid.y-6,fill:'#edf5ff','font-size':12,'pointer-events':'none'},back?`${weight} ⇄ ${back}`:String(weight)));}
  else if(weight>1){line.append(element('title',{},`${weight} bundled edges${route.members?.length?': '+route.members.join(', '):''}`));const mid=route.points[Math.floor(route.points.length/2)];svg.append(element('text',{x:mid.x+6,y:mid.y-6,fill:'#edf5ff','font-size':12,'pointer-events':'none'},String(weight)));}
  svg.append(line);
  if(onSelectRoute){const hit=element('polyline',{points,fill:'none',stroke:'transparent','stroke-width':10,'pointer-events':'stroke',class:'route-hit','data-route-id':route.id});hit.onclick=()=>onSelectRoute(route.id);svg.append(hit);}
 }
  // Junctions of explicit buses: where the bus's lines branch (ADR 0005).
  for(const j of scene.junctions??[])svg.append(element('circle',{cx:j.x,cy:j.y,r:3.5,fill:'#73bba7',stroke:'#142133','stroke-width':1,'pointer-events':'none',class:'lw-junction'}));
  // The rings and the controls of the containers are drawn above the routes, so a route crossing a container's border does
  // not take the press meant for the border; the routes stay selectable everywhere else.
  for(const el of [...rings,...top])svg.append(el);
 for(const n of [...nodes.values()].filter(n=>!parents.has(n.id))){const r=absolute.get(n.id),g=element('g',{class:'node',tabindex:0,role:'button','aria-label':n.label,'data-node-id':n.id});
 {const why=hint(n.id);if(why)g.append(why);}
 // The outline is the engine's: a circle is the disc inscribed in its box, a rectangle is its box.
 const look={fill:n.id===selected?'#245e50':'#24364b',stroke:invalid.has(n.id)?'#f08f80':n.id===selected?'#90e4c3':'#4c6682'};
 // The hit area is at least 18 screen pixels, so a small node is as easy to press as a container's ring (ADR 0009).
 const hitW=Math.max(r.width,18/scale),hitH=Math.max(r.height,18/scale),cx=r.x+r.width/2,cy=r.y+r.height/2;
 // A circle node: its label is centred in the disc, and the pin control sits on the disc's upper right, not in the box corner.
 const round=n.shape==='circle'&&n.kind!=='attribute'&&!n.rows?.length,rad=Math.min(r.width,r.height)/2;
 g.append(element('rect',{x:cx-hitW/2,y:cy-hitH/2,width:hitW,height:hitH,fill:'transparent',class:'lw-hit'}));
 // Entity-relationship notation: an attribute is a row of text in its entity; relationships are labelled edges.
 const attribute=n.kind==='attribute';
 const shape=attribute?element('rect',{...r,rx:4,fill:'transparent',stroke:'transparent'})
  :n.shape==='circle'?element('circle',{cx,cy,r:Math.min(r.width,r.height)/2,...look}):element('rect',{...r,rx:7,...look});
 g.append(shape);
 g.onpointerenter=()=>shape.setAttribute('stroke',HOVER_STROKE);
 g.onpointerleave=()=>shape.setAttribute('stroke',attribute?'transparent':look.stroke);if(pinned.has(n.id)){g.setAttribute('data-pinned-id',n.id);if(!onPin){const pin=element('circle',{cx:round?cx+rad*.62:r.x+r.width-10,cy:round?cy-rad*.62:r.y+10,r:4,fill:'#edc477','pointer-events':'none'});pin.append(element('title',{},'Pinned: layout keeps this position'));g.append(pin);}}
 const size=controlSize(Math.min(r.height*.5,r.width/5,24)),toggled=onToggle&&collapsed.has(n.id);
 if(!n.rows?.length)g.append(element('text',round?{x:cx,y:cy+5,'text-anchor':'middle',fill:'#edf5ff','font-size':13,'pointer-events':'none'}:{x:toggled?r.x+8+size:r.x+10,y:n.kind==='attribute'?r.y+r.height/2+5:r.y+32,fill:'#edf5ff','font-size':13},(collapsed.has(n.id)&&!onToggle?'▸ ':'')+n.label.slice(0,23)));
 if(toggled)g.append(toggleControl(n,r.x+4,r.y+(r.height-size)/2,size));
 // Rows of a node (ADR 0011): the engine gives each row its rectangle; the viewer draws the text inside it. The first row
// is the title, a rule separates it from the others.
if(n.rows?.length){
  n.rows.forEach((row,i)=>{const b=row.bounds;g.append(element('text',{x:b.x+6,y:b.y+b.height/2+5,fill:i===0?'#edf5ff':'#c4d6e8','font-size':i===0?14:13,'font-weight':i===0?'600':'400','pointer-events':'none'},row.label));});
  const t=n.rows[0].bounds;
  if(n.rows.length>1)g.append(element('line',{x1:r.x,y1:t.y+t.height+2,x2:r.x+r.width,y2:t.y+t.height+2,stroke:'#344961','pointer-events':'none'}));
}
if(onPin&&n.kind!=='attribute')g.append(pinControl(n,round?cx+rad*.62-size/2:r.x+r.width-size-4,round?cy-rad*.62-size/2:r.y+4,size));g.append(element('title',{},n.id));const select=e=>choose(n,e);g.onclick=select;g.onkeydown=e=>{if(e.key==='Enter')select(e);};if(onNodeDragStart&&!parents.has(n.id))g.onpointerdown=e=>onNodeDragStart(n,e);if(active&&!endpoints.has(n.id))g.setAttribute('opacity','.4');svg.append(g);}
 for(const port of (scene.showPorts?scene.ports:[])){const dot=element('circle',{'data-port-id':port.id,cx:port.position.x,cy:port.position.y,r:3,fill:port.endpoint==='source'?'#7ee1bc':'#9dbaff',stroke:'#142133','stroke-width':1,'pointer-events':'none'});dot.append(element('title',{},`${port.id} · ${port.side}${onPortDragStart?' (drag along the border to place it)':''}`));svg.append(dot);
  // A port can be dragged along the border of its node: a larger invisible circle gives it a target that is easy to hit.
  if(onPortDragStart){const hit=element('circle',{'data-port-hit':port.id,cx:port.position.x,cy:port.position.y,r:Math.min(9/Math.max(scale||1,.0001),6),fill:'transparent','pointer-events':'all',style:'cursor:grab'});hit.onpointerdown=e=>{e.stopPropagation();onPortDragStart(port,e);};svg.append(hit);}}
 for(const crossing of scene.crossings){const mark=element('rect',{x:crossing.position.x-3,y:crossing.position.y-3,width:6,height:6,fill:'#edc477','pointer-events':'none'});mark.append(element('title',{},`${crossing.edgeId} → ${crossing.containerId}`));svg.append(mark);}
 for(const label of scene.labels){const r=label.bounds;const box=element('rect',{...r,rx:3,fill:'#142133','pointer-events':onLabelDragStart?'all':'none','data-label-id':label.id,...(onLabelDragStart?{style:'cursor:grab'}:{})});if(onLabelDragStart)box.onpointerdown=e=>{e.stopPropagation();onLabelDragStart(label,e);};svg.append(box);svg.append(element('text',{x:r.x+4,y:r.y+r.height/2,'dominant-baseline':'middle',fill:'#edf5ff','font-size':12,'pointer-events':'none'},label.text));}
}

// Provisional (path cut-out): marks a trace on a drawn scene. The routes and nodes of the path are drawn in amber and
// stronger; the other routes recede. With `fade`, every node is the fainter the farther it is from the path (`distance`,
// hops through the drawn edges; nodes the path cannot reach are faintest). Only attributes change, not the geometry.
const TRACE_COLOUR='#ffc861';
function applyTrace(root,{nodes=[],routes=[],distance={},fade=false}={}){
 const onNode=new Set(nodes),onRoute=new Set(routes);
 const hops=id=>distance instanceof Map?distance.get(id):distance[id];
 const faint=id=>{if(!fade||onNode.has(id))return 1;const d=hops(id);return d===undefined?0.12:Math.max(0.15,1-0.3*d);};
 const all=selector=>typeof root.querySelectorAll==='function'?[...root.querySelectorAll(selector)]:[];
 for(const line of all('polyline[data-route-id]')){
  if(line.getAttribute('class')==='route-hit')continue;
  const id=line.getAttribute('data-route-id');
  if(onRoute.has(id)){line.setAttribute('stroke',TRACE_COLOUR);line.setAttribute('stroke-opacity','1');line.setAttribute('stroke-width',String(Number(line.getAttribute('stroke-width')??2)+1.5));}
  else line.setAttribute('stroke-opacity',String(fade?0.08:0.25));
 }
 for(const g of all('g[data-node-id]')){
  const id=g.getAttribute('data-node-id');
  if(onNode.has(id)){const shape=g.querySelector('rect:not(.lw-hit), circle, ellipse, path, polygon');shape?.setAttribute('stroke',TRACE_COLOUR);shape?.setAttribute('stroke-width','2.5');}
  g.setAttribute('opacity',String(faint(id)));
 }
 for(const frame of all('[data-container-id]')){
  const id=frame.getAttribute('data-container-id');
  if(onNode.has(id)){frame.setAttribute('stroke',TRACE_COLOUR);frame.setAttribute('stroke-width','2.5');}
  frame.setAttribute('opacity',String(faint(id)));
 }
}

// Provisional (partly shown containers): a rest (`rest:<container>`, kind `rest`) holds the children a container does not
// show; its outline is dashed, so that it reads as "more here".
function markRests(root,scene){
 if(typeof root.querySelector!=='function')return;
 for(const n of scene.nodes){
  if(n.kind!=='rest')continue;
  const g=root.querySelector(`g[data-node-id="${globalThis.CSS?.escape?globalThis.CSS.escape(n.id):n.id.replace(/"/g,'\\"')}"]`);
  const shape=g?.querySelector('rect:not(.lw-hit), circle');
  shape?.setAttribute('stroke-dasharray','6 4');
 }
}

