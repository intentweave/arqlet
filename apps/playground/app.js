import {bindNavigation} from '@arqlet/viewer/navigation.js';
import {loadEngine,sceneOf,createEngineWorker} from '@arqlet/engine-client/engine.js';
import {renderSvg} from '@arqlet/viewer/svg.js';
import {containmentRows} from '@arqlet/containment-tree/model.js';
import {settingsOf,rootSettingsOf} from '@arqlet/containment-tree/settings.js';
import {renderContainmentTree,settingsElementOf} from '@arqlet/containment-tree/render.js';
import {acceptPublication,refinementFits} from './revisions.js';
import {Session} from './session.js';
import {MoveGesture,releaseOnDrop} from './drag.js';
import {trackGesture} from './gesture.js';
import {History} from './history.js';
import {placementOnBorder,positionOnBorder} from './port-drag.js';
import {nearResizeCorner,resizedLeaf} from './resize.js';
import {GeometryReports,labelSize} from './geometry.js';
import {presetState,applyPreset} from '@arqlet/engine-client/presets.js';
import {routeDetails,routeConflictsOf} from '@arqlet/viewer/routes.js';
import {evaluateSnapshot} from './comparison.js';
import {withAggregatedWeights} from './aggregation.js';
import {CurationSession,Selection,bindCurationGestures,describeSelection,downloadText,readFileText} from './generated/editor/index.js';
const $=id=>document.getElementById(id);
const VIEW_LIMIT=40;
// Examples are imported with a larger limit than the default view: the import fills it from every component of the graph.
// The engine accepts up to 10,000 nodes in a view (MAX_VIEW_LIMIT).
const EXAMPLE_LIMIT=10000;
let snapshot,selected=null,selectedRoute=null,selectionKey=null,scale=1,pending=null;
// The view refits to the canvas on a resize until the user zooms; Fit turns it on again.
let autoFit=true;
const selection=new Selection();
// The curation draft, when manual curation is on. While it is active the view is not recomputed by other operations.
let curation=null;
let epoch=0,loadToken=0,refinement=null,refineTimer=null,requestId=0,structuring=null;
// The job lifecycle of progressive refinement: which job may still show results, and which result is pending.
const session=new Session();
// Edits and background refinement use separate workers, so a long refinement never blocks an edit.
const workers={interactive:createEngineWorker('./generated/arqlet.wasm'),background:createEngineWorker('./generated/arqlet.wasm')};
const replies=new Map();
for(const worker of Object.values(workers)){
 // A publication answers a request that is still running; the final reply ends it.
 worker.onmessage=({data})=>{const reply=replies.get(data.requestId);if(!reply)return;if(data.publication){reply.onPublication?.(data.publication,{emittedAt:data.emittedAt,bytes:data.bytes,receivedAt:Date.now()});return;}replies.delete(data.requestId);reply.resolve(data);};
 worker.onerror=e=>{for(const [id,reply] of replies)if(reply.worker===worker){replies.delete(id);reply.resolve({ok:false,error:e.message||'Worker error'});}};
}
// Where each kind of operation runs. All targets execute the same Rust dispatcher.
const placement={
 split:{edit:'interactive',refine:'backend',import:'interactive'},
 browser:{edit:'interactive',refine:'background',import:'interactive'},
 backend:{edit:'backend',refine:'backend',import:'backend'},
};
const refineLabels={split:'Optimize (backend)',browser:'Optimize (browser)',backend:'Optimize (backend)'};
const mode=()=>$('mode').value||'split';
const threads=()=>Math.min(4,(typeof navigator!=='undefined'&&navigator.hardwareConcurrency)||1);
// `onPublication` receives progressive results of a worker request while it runs (backend requests have none).
function execute(kind,request,signal,onPublication){
 const target=placement[mode()][kind];
 if(target==='backend')return fetch('/api/op',{method:'POST',headers:{'Content-Type':'application/json'},signal,body:JSON.stringify(request)}).then(r=>r.json());
 const worker=workers[target];
 return new Promise(resolve=>{const id=++requestId;replies.set(id,{worker,resolve,onPublication});worker.postMessage({requestId:id,request});});
}
function status(text){$('status').textContent=text;}
// The details line: the route's facts, then its conflicts. Each partner is a button that selects that route (or node).
function setRouteInfo(details,conflicts,key){
 const box=$('route-info');box.replaceChildren();
 if(!details){box.textContent='Click an edge to highlight it.';return;}
 box.textContent=`Edge ${details.id}${details.kind?` (${details.kind})`:''}: ${details.source} → ${details.target} · length ${conflicts.length} · bends ${conflicts.bends}`;
 const groups=[['passes through node',conflicts.lineNodeHits,'node'],['foreign container',conflicts.containerTraversals,'node'],['border proximity',conflicts.hugging,'node'],['shares a line with',conflicts.sharedLines,'route'],['runs parallel with',conflicts.nearParallel,'route'],['crosses',conflicts.crossings,'route'],['shared endpoint, crosses',conflicts.sharedEndpointCrossings,'route']];
 for(const [label,list,kind] of groups){
  if(!list.length)continue;
  const lead=document.createElement('span');lead.textContent=` · ${label} `;box.append(lead);
  list.forEach((id,k)=>{const b=document.createElement('button');b.textContent=id+(k<list.length-1?', ':'');
   b.onclick=()=>{if(kind==='route'){selectedRoute=id;selectionKey=key;}else{selected=id;}draw();};box.append(b);});
 }
 if(details.members.length){const m=document.createElement('span');m.textContent=` · bundles ${details.members.join(', ')}`;box.append(m);}
}
// The move gesture on the page: the dragged node follows the pointer at once; the engine's geometry follows step by step.
let drag=null, dragPreview=null, suppressClick=false, previewFrame=0;
// The preview moves the dragged node and the ends of the routes at it; the engine's routes replace them at each reply.
// The node and everything inside it: a container's children move with it in the preview, as the engine moves them.
function membersOf(scene,id){
 const members=new Set([id]);
 let grew=true;
 while(grew){
  grew=false;
  for(const n of scene.nodes){
   if(n.parentId&&members.has(n.parentId)&&!members.has(n.id)){members.add(n.id);grew=true;}
  }
 }
 return members;
}
// The drawing while a node is dragged or resized: a move carries the node's members and the routes at their ends; a
// resize changes only the node's own box.
function previewScene(scene,{node,bounds,others=[]}){
 const old=scene.nodes.find(n=>n.id===node);
 const dx=bounds.x-old.bounds.x, dy=bounds.y-old.bounds.y;
 const moving=bounds.width===old.bounds.width&&bounds.height===old.bounds.height;
 // A selection moves as a whole: the other selected nodes (and what they hold) follow by the same distance.
 const members=moving?new Set([...membersOf(scene,node),...others.flatMap(id=>[...membersOf(scene,id)])]):new Set([node]);
 return {...scene,
  nodes:scene.nodes.map(n=>{
   if(!members.has(n.id))return n;
   if(n.id===node)return {...n,bounds};
   return {...n,bounds:{...n.bounds,x:n.bounds.x+dx,y:n.bounds.y+dy}};
  }),
  routes:scene.routes.map(r=>{
   const source=members.has(r.source), target=members.has(r.target);
   if(!source&&!target)return r;
   const last=r.points.length-1;
   const points=r.points.map((p,i)=>(source&&target)||(source&&i===0)||(target&&i===last)?{x:p.x+dx,y:p.y+dy}:p);
   return {...r,points};
  })};
}
// The containment tree (ADR 0010) shows the scene's own IDs. It is rebuilt only when the accepted snapshot or the
// selection changes, so the immediate preview of a drag does not rebuild it on every frame.
let treeSignature=null;
// The text filter of the containment tree: it only hides rows in the list, so it never changes the view.
let treeFilter='';
// Whether the view (the root) is the active settings target: it is selected, or no container or node is.
let viewSelected=false;
function selectView(){viewSelected=true;treeSignature=null;draw();}
function renderTreeView(scene){
 const viewActive=viewSelected||!selected;
 const signature=`${snapshot.revision}:${snapshot.layoutVersion??0}:${selected??''}:${pending?1:0}:${treeFilter}:${viewActive?1:0}:${layoutError?`${layoutError.target}/${layoutError.field}`:''}`;
 if(signature===treeSignature)return;
 treeSignature=signature;
 const source=snapshot.sourceGraph??snapshot.graph;
 const parents=new Map(source.nodes.map(n=>[n.id,n.parentId??null]));
 const orientations=snapshot.areaOrientations??{},strategies=snapshot.areaStrategies??{};
 const rows=containmentRows(scene,{pins:effectiveOptions().pins,containerSizing:snapshot.options?.containerSizing??{},geometryIssues:snapshot.geometryIssues??[],settingsFor:id=>settingsOf(source,id,parents,{orientations,strategies})});
 // The view is the first row: its direction, strategy and routing are the whole view's values.
 const viewRow={id:'__view__',label:'View',root:true,container:false,depth:0,parentId:null,pinned:false,invalid:false,conflict:null,size:null,settings:rootSettingsOf(source,{orientations,strategies})};
 const target=viewActive?null:selected;
 const settings={disabled:!!pending,errors:layoutError&&layoutError.target===target?{[layoutError.field]:layoutError.reason}:{},onApply:entry=>applyContainerSizing(selected,entry),onToggle:toggleContainer,onChoice:applyLayoutSetting};
 renderContainmentTree($('containment-tree'),[viewRow,...rows],{selected:viewActive?'__view__':selected,onSelect:id=>id==='__view__'?selectView():selectNode(id),onToggle:toggleContainer,onShape:(id,shape)=>{if(!pending)edit({op:'shape',snapshot,nodeId:id,shape},{busy:'Applying shape …',remember:'Shape'});},disabled:!!pending,filter:treeFilter,document,detail:row=>settingsElementOf(document,row,settings)});
}
// The authored axes of each container, for the canvas's reset controls: `width`, `height`, or both.
function sizedAxes(){
 const sizing=snapshot.options?.containerSizing??{};
 return new Map(Object.entries(sizing).map(([id,entry])=>[id,['width','height'].filter(axis=>entry[axis])]));
}
// The canvas's reset to content: one axis of the container goes; the entry goes when no axis is left.
function resetSize(id,axis){
 const entry={...(snapshot.options?.containerSizing?.[id]??{})};
 delete entry[axis];
 applyContainerSizing(id,entry.width||entry.height?entry:null);
}
// A layout setting of the selected container (direction, strategy or routing), through the engine's operation of that
// name. `null` clears the container's own value, so it inherits again.
// A value the engine refuses is shown at the field it was set in, with the engine's reason, and stays there until the next try.
let layoutError=null;
async function applyLayoutSetting(field,value){
 if(!snapshot||pending)return;
 const nodeId=viewSelected||!selected?null:selected;
 layoutError=null;
 // A setting of one container keeps that container where it is on screen; one of the whole view fits the view.
 const result=await edit({op:field,snapshot,nodeId,[field]:value},{busy:`Applying layout ${field} …`,...(nodeId?{anchor:nodeId}:{fit:true}),remember:`Layout ${field}`});
 if(!result.ok){layoutError={target:nodeId,field,reason:result.error};treeSignature=null;draw();}
}
// Expanding or collapsing a container is an engine operation: the view changes when the accepted result arrives.
function toggleContainer(id){
 if(curation){status('Curation is active: end it to collapse or expand containers.');return;}
 if(!id||!snapshot||pending)return;
 // The opened or closed container stays where it is on screen; the rest of the view moves around it.
 edit({op:'toggle',snapshot,nodeId:id},{busy:'Recomputing view …',anchor:id,remember:'Collapse or expand'});
}
// Sets one container's size (ADR 0010): `entry` is the container's new `containerSizing` entry, or null to follow content.
// The user's decisions are values of the engine (operations `intent` and `restore`), kept as an undo stack (history.js). One
// action is one entry, however many engine steps it took. Cleared when another graph is loaded or the graph changes.
const history=new History(50);
let historyQueue=Promise.resolve();
const intentOf=async snap=>{const data=await execute('edit',{op:'intent',snapshot:snap});if(!data.ok)throw Error(data.error);return data.intent;};
// Records the action from the snapshot `before` to the snapshot `after`; recordings run one after the other.
function recordHistory(label,before,after,options){
 historyQueue=historyQueue.then(async()=>{try{history.record({label,before:await intentOf(before),after:await intentOf(after)},options);}catch(e){status(`History: ${e.message}`);}syncHistoryButtons();});
 return historyQueue;
}
// Undo or redo: the engine restores the remembered decisions in one step. A refused restore keeps the entry.
async function stepHistory(direction){
 if(!snapshot||pending||drag||resize||containerResize)return;
 await historyQueue;
 const entry=direction<0?history.peekUndo():history.peekRedo();
 if(!entry)return;
 const verb=direction<0?'Undoing':'Redoing';
 const result=await edit({op:'restore',snapshot,intent:direction<0?entry.before:entry.after},{busy:`${verb} ${entry.label} …`,fit:true});
 if(!result.ok)return;
 if(direction<0)history.commitUndo();else history.commitRedo();
 syncHistoryButtons();
 status(`${direction<0?'Undone':'Redone'}: ${entry.label}`);
}
const undo=()=>stepHistory(-1),redo=()=>stepHistory(1);
function applyContainerSizing(id,entry){
 if(!id||!snapshot||pending)return;
 edit({op:'configure',snapshot,options:{containerSizing:{[id]:entry}}},{busy:'Applying container size …',fit:true,remember:'Container size'});
}
function draw(fit=false){
 const baseline=withAggregatedWeights(sceneOf(sceneRun,snapshot),snapshot);
 const scene=curation?curation.scene(baseline):baseline;
 const shown=dragPreview?previewScene(scene,dragPreview):scene;
 if(curation)markConflicts(scene);
 if(fit){autoFit=true;scale=Math.min(1,Math.max(.02,Math.min(($('canvas').clientWidth-30)/scene.bounds.width,($('canvas').clientHeight-30)/scene.bounds.height)));}
 // A route selection belongs to one revision and layout version; a new result drops it.
 const key=`${snapshot.revision}:${snapshot.layoutVersion??0}`;
 if(selectionKey!==key||!scene.routes.some(r=>r.id===selectedRoute)){selectedRoute=null;selectionKey=null;}
 selection.prune({nodes:new Set(scene.nodes.map(n=>n.id)),routes:new Set(scene.routes.map(r=>r.id))});
 renderSvg($('graph'),shown,{document,trace:traceView(),onPortDragStart:(port,e)=>startPortDrag(port,e),onLabelDragStart:(label,e)=>startLabelDrag(label,e),onNodeDragStart:(n,e,opts)=>startDrag(n,e,opts),onToggle:toggleContainer,onPin:(id,on)=>{if(performance.now()<pinClickGuard)return;setPin(id,on);},onReset:resetSize,onResize:startResize,sized:sizedAxes(),selected,selectedRoute,scale,pinned:new Set(Object.keys(effectiveOptions().pins)),onSelect:(id,event)=>{if(suppressClick){suppressClick=false;return;}if(event?.ctrlKey||event?.metaKey)selection.toggleNode(id);else selection.replaceNode(id);selected=id;$('selected').textContent=scene.nodes.find(n=>n.id===id).label;draw();},
  onSelectRoute:id=>{selectedRoute=selectedRoute===id?null:id;selection.selectRoute(selectedRoute);selectionKey=selectedRoute?key:null;draw();}});
 renderTreeView(scene);
  if(curation){markPending();bindCurationGestures($('graph'),curation,scene,{changed:()=>draw(),status,growContainers:()=>true,selectedNodes:()=>selection.nodes});}
 renderInspector();
 scheduleAutoReroute();
 const kinds=Object.fromEntries(snapshot.graph.edges.map(e=>[e.id,e.kind]));
 const details=selectedRoute?routeDetails(scene,selectedRoute,kinds):null;
 // The conflicts come from the engine, so the list and the totals of the metrics agree.
 setRouteInfo(details,details?routeConflictsOf(sceneRun,scene,selectedRoute):null,key);
 const nodes=new Map(snapshot.graph.nodes.map(n=>[n.id,n]));
 const containers=new Set((snapshot.sourceGraph??snapshot.graph).nodes.map(n=>n.parentId).filter(Boolean));
 const collapsed=new Set(snapshot.collapsedIds??[]);
 const layoutDiagnostics=snapshot.layoutDiagnostics??[];
 $('layout-method').textContent=layoutDiagnostics.join('; ')||(
  snapshot.options?.layeredConstruction===false?'Previous layout pipeline selected.':'Hierarchical layered pipeline selected where supported.'
 );
 $('routing').textContent=(snapshot.routingIssues??[]).length?`${snapshot.routingIssues.length} open routing/label conflicts: ${snapshot.routingIssues.slice(0,3).join('; ')}`:'Edges and labels checked: no open conflicts.';
 renderConflicts(scene);
 renderExampleInfo();
 syncDetails();
 renderStats();
 syncPins();
 $('toggle').disabled=!!pending||!containers.has(selected);$('toggle').textContent=containers.has(selected)?(collapsed.has(selected)?'Expand container':'Collapse container'):'Select a container';
 $('add').disabled=!!pending||!selected||containers.has(selected);
 // The shape of the selected node. A container takes a rectangle or a circle; a square container is drawn as a rectangle.
 const shapeable=!!selected&&!curation&&snapshot.graph.nodes.some(n=>n.id===selected);
 $('shape-select').disabled=!!pending||!shapeable;
 if(shapeable)$('shape-select').value=snapshot.graph.nodes.find(n=>n.id===selected)?.shape??'rect';
 $('geometry').textContent=(snapshot.geometryIssues??[]).length?`${snapshot.geometryIssues.length} geometry conflicts: ${snapshot.geometryIssues.slice(0,4).map(i=>i.kind+' ('+i.nodeIds.join(', ')+')').join('; ')}`:'Node geometry checked: no conflicts.';
 const {preset:active,stale}=presetState(snapshot,presets);
 // A snapshot without options uses the engine defaults; that is not a custom change.
 const defaults=!active&&!stale&&!Object.keys(snapshot.options??{}).length;
 $('profile').value=active?.id??(defaults?'__default__':'__custom__');
 $('profile-info').textContent=active?`${active.description} Indicative budget ${Math.round(active.budgetMs/1000)} s; in the browser without a time limit, interrupted only on cancel.`:defaults?'Engine defaults: no profile applied. Choose a profile to apply one.':stale?'This profile is from another version; its rules no longer apply. Custom options: no longer match a profile.':'Custom options: no longer match a profile.';
 $('refine').disabled=!!pending||!!curation||active?.refine===false;
 $('structured').disabled=!!pending||!!curation;
 $('stats').textContent=`${snapshot.graph.nodes.length}/${(snapshot.sourceGraph??snapshot.graph).nodes.length} visible nodes · ${snapshot.graph.edges.length} connections · Revision ${snapshot.revision} · Layout ${snapshot.layoutVersion??0}`;$('save').disabled=false;$('bundle').value=snapshot.options?.bundling==='complete'?'complete':'none';
 if($('measure').checked)setTimeout(measureNodes,30);
 syncCuration();
}
// The conflicts of the drawing as the engine reports them, every kind, with all parties; the list is capped and says how many are shown.
const CONFLICT_LIMIT=30;
function renderConflicts(scene){
 const box=$('conflicts'),list=scene.conflicts??[];
 box.replaceChildren();
 box.hidden=list.length===0;
 if(!list.length)return;
 const title=document.createElement('p');
 title.textContent=list.length>CONFLICT_LIMIT?`Conflicts: ${CONFLICT_LIMIT} of ${list.length} shown`:`Conflicts: ${list.length} of ${list.length} shown`;
 box.append(title);
 for(const conflict of list.slice(0,CONFLICT_LIMIT)){
  const item=document.createElement('button');
  item.type='button';item.className='conflict-item';
  const parties=[...(conflict.nodes??[]),...(conflict.routes??[]).map(id=>`edge ${id}`)];
  item.textContent=`${conflict.kind}: ${conflict.reason}${parties.length?` (${parties.join(', ')})`:''}`;
  item.onclick=()=>{
   // Selecting a conflict selects every node it names, and its route if it names one.
   selectedRoute=conflict.routes?.[0]??null;
   selectionKey=selectedRoute?`${snapshot.revision}:${snapshot.layoutVersion??0}`:null;
   if(conflict.nodes?.length){
    selectNode(conflict.nodes[0]);
    for(const id of conflict.nodes.slice(1))selection.toggleNode(id);
   } else selection.clear();
   if(selectedRoute)selection.selectRoute(selectedRoute);
   draw();
  };
  box.append(item);
 }
}
// Single edit path: one edit in flight, results accepted only if the base revision still matches.
// Returns {ok, snapshot?, error?} so a caller can chain operations on the snapshot it got back.
// Where a node's box is drawn, relative to the drawing's origin, in pixels at the current scale; null when it is not drawn.
function drawnPosition(id){
 const scene=snapshot&&sceneOf(sceneRun,snapshot),box=scene?.nodes.find(n=>n.id===id)?.bounds;
 return box?{x:(box.x-scene.bounds.x)*scale,y:(box.y-scene.bounds.y)*scale}:null;
}
// FLIP animation of an edit (first, last, invert, play): before the change, where every node and container is on screen;
// after it, each starts at its old place and moves to its new one, and what is new (routes, labels, nodes) fades in. The
// engine's result is final; this only shows the way to it. Off when the user asks for reduced motion.
const ANIMATED=['data-node-id','data-container-id','data-header-id','data-toggle-id','data-pin-id','data-pinned-id','data-resize-id'];
function idOf(el){for(const a of ANIMATED){const v=el.getAttribute(a);if(v!==null)return v;}return null;}
function placesOnScreen(){
 // Only a real DOM has places on screen; an animation never stands in the way of the change itself.
 if(typeof $('graph').querySelectorAll!=='function')return null;
 const out=new Map();
 for(const el of $('graph').querySelectorAll(ANIMATED.map(a=>`[${a}]`).join(','))){const id=idOf(el);if(id!==null&&!out.has(id)){const r=el.getBoundingClientRect();out.set(id,[r.left,r.top]);}}
 return out;
}
function animateFrom(before){
 try{animate(before);}catch{/* the change is drawn; only its animation is lost */}
}
function animate(before){
 if(!before||globalThis.matchMedia?.('(prefers-reduced-motion: reduce)').matches)return;
 const after=placesOnScreen(),moved=[];
 for(const el of $('graph').querySelectorAll(ANIMATED.map(a=>`[${a}]`).join(','))){
  const id=idOf(el),from=before.get(id),to=after.get(id);
  el.style.transition='none';
  // CSS lengths in the drawing are its user units: a screen pixel is 1/scale of one.
  if(from&&to)el.style.transform=`translate(${(from[0]-to[0])/scale}px,${(from[1]-to[1])/scale}px)`;else el.style.opacity='0';
  moved.push(el);
 }
 for(const el of $('graph').querySelectorAll('[data-route-id],[data-label-id]')){el.style.transition='none';el.style.opacity='0';moved.push(el);}
 $('graph').getBoundingClientRect();
 requestAnimationFrame(()=>{for(const el of moved){el.style.transition='transform 380ms ease, opacity 380ms ease';el.style.transform='';el.style.opacity='';}});
 setTimeout(()=>{for(const el of moved)el.style.transition='';},450);
}
// `anchor` (a node id): after the change the view scrolls so that this node's box is where it was on screen, instead of
// fitting the whole drawing again.
async function edit(request,{busy,fit=false,chained=false,remember=null,anchor=null}={}){
 if(curation)return {ok:false,error:'Curation is active: the view is not recomputed.'};
 if(drag?.gesture?.active)return {ok:false,error:'A move is running: finish it or cancel it first.'};
 if(!snapshot||pending)return {ok:false,error:'An operation is already running.'};
 cancelRefinement();
 const base={epoch,revision:snapshot.revision,layout:snapshot.layoutVersion??0};
 const baseSnapshot=snapshot;
 let places=null;try{places=placesOnScreen();}catch{}
 const before=anchor?drawnPosition(anchor):null;
 pending=base;$('add').disabled=true;draw();status(busy);
 let data;
 try{data=await execute('edit',{...request,expectedRevision:base.revision,expectedLayoutVersion:base.layout});}
 catch(e){data={ok:false,error:e.message};}
 if(epoch!==base.epoch||pending!==base)return {ok:false,error:'The view was reset in the meantime.'};
 pending=null;$('add').disabled=!selected;
 if(!data.ok){status(data.error);draw();return {ok:false,error:data.error};}
 if(snapshot.revision!==base.revision||(snapshot.layoutVersion??0)!==base.layout||data.snapshot.baseRevision!==base.revision){status('Stale result discarded.');return {ok:false,error:'Stale result discarded.'};}
 snapshot=data.snapshot;status(`Computed locally · ${snapshot.recomputedContainers.length} container regions updated`);
 const after=before?drawnPosition(anchor):null;
 if(after){autoFit=false;draw(false);const c=$('canvas');c.scrollLeft+=after.x-before.x;c.scrollTop+=after.y-before.y;}
 else draw(fit);
 animateFrom(places);
 if(!chained)scheduleRefinement();
 // A new node changes the graph itself: what was remembered no longer describes it. A decision is remembered as one entry.
 if(request.op==='add')history.clear();
 else if(remember)recordHistory(typeof remember==='string'?remember:remember.label,baseSnapshot,snapshot,{merge:!!remember.merge});
 return {ok:true,snapshot};
}
// Dragging a leaf. A movement of more than 4 units starts a move gesture; below that, the pointer is a plain click.
function pointerInScene(event){
 const svg=$('graph'),point=svg.createSVGPoint();point.x=event.clientX;point.y=event.clientY;
 return point.matrixTransform(svg.getScreenCTM().inverse());
}
// The containers above a node, nearest first, with their boxes before the drag (ADR 0010, decision 3).
function ancestorsOf(scene,id){
 const out=[];
 let parentId=scene.nodes.find(n=>n.id===id)?.parentId;
 while(parentId){
  const container=scene.nodes.find(n=>n.id===parentId);
  if(!container)break;
  out.push({id:parentId,bounds:container.bounds});
  parentId=container.parentId;
 }
 return out;
}
// A press on a node's pin that does not move is a click on the pin: the drag's release toggles it, and the control's own click
// (which a browser may still deliver) is ignored for a moment.
let pinClickGuard=0;
function startDrag(node,event,opts={}){
 if(curation||!snapshot||pending||event.button!==0||drag)return;
 const scene=sceneOf(sceneRun,snapshot);
 const shape=scene.nodes.find(n=>n.id===node.id);
 // A press in the bottom-right corner of a leaf resizes it; elsewhere it moves the node.
 const corner=pointerInScene(event);
 if(!snapshot.graph.nodes.some(n=>n.parentId===node.id)&&nearResizeCorner(corner,shape.bounds)){
  resize={node:node.id,pointer:event.pointerId,start:corner,bounds:{...shape.bounds},size:null,ghost:null};
  event.preventDefault();
  $('graph').setPointerCapture?.(event.pointerId);
  track(event);
  return;
 }
 const parent=scene.nodes.find(n=>n.id===shape.parentId)?.bounds??{x:0,y:0};
 const companions=selectionCompanions(scene,node.id);
 drag={node:node.id,companions,start:pointerInScene(event),origin:{x:shape.bounds.x,y:shape.bounds.y},size:{width:shape.bounds.width,height:shape.bounds.height},parent,ancestors:ancestorsOf(scene,node.id),gesture:null,moved:false,pointer:event.pointerId,wasPinned:snapshot.options?.pins?.[node.id]!==undefined,fromPin:!!opts.fromPin};
 // The viewport's pan handler must not take this press: it would move the pointer capture away from the drawing.
 event.preventDefault();
 $('graph').setPointerCapture?.(event.pointerId);
 track(event);
}
// The other nodes of a selection that move with the grabbed one: only when the grabbed node is part of a selection of more than
// one, and never a node inside another moved node (it moves with its container) or the container of the grabbed node.
function selectionCompanions(scene,grabbed){
 const chosen=selection.nodes;
 if(chosen.length<2||!chosen.includes(grabbed))return [];
 const known=new Map(scene.nodes.map(n=>[n.id,n]));
 const inside=(id,ancestor)=>{for(let at=known.get(id)?.parentId;at;at=known.get(at)?.parentId)if(at===ancestor)return true;return false;};
 const moved=chosen.filter(id=>known.has(id));
 return moved.filter(id=>id!==grabbed&&!moved.some(other=>other!==id&&inside(id,other))&&!inside(grabbed,id)&&!inside(id,grabbed));
}
// A port is dragged along the border of its node and is placed where it is released: the engine records the side and the
// offset as the port of that end of the edge. While it is dragged, a ghost marks the place.
let portDrag=null;
function startPortDrag(port,event){
 if(curation||!snapshot||pending||event.button!==0||drag||portDrag)return;
 const shape=sceneOf(sceneRun,snapshot).nodes.find(n=>n.id===port.nodeId);
 if(!shape)return;
 portDrag={port,bounds:shape.bounds,shape:shape.shape??'rect',placement:null,ghost:null};
 event.preventDefault();
 $('graph').setPointerCapture?.(event.pointerId);
 track(event);
}
function movePort(event){
 portDrag.placement=placementOnBorder(portDrag.bounds,portDrag.shape,pointerInScene(event));
 const at=positionOnBorder(portDrag.bounds,portDrag.shape,portDrag.placement);
 if(!portDrag.ghost){
  portDrag.ghost=document.createElementNS('http://www.w3.org/2000/svg','circle');
  for(const [name,value] of Object.entries({r:5,fill:'none',stroke:'#90e4c3','stroke-width':2,'pointer-events':'none',class:'port-ghost'}))portDrag.ghost.setAttribute(name,value);
  $('graph').append(portDrag.ghost);
 }
 portDrag.ghost.setAttribute('cx',at.x);portDrag.ghost.setAttribute('cy',at.y);
 status(`Port of ${portDrag.port.edgeId}: ${portDrag.placement.side}, ${Math.round(portDrag.placement.offset*100)} %`);
}
function finishPort(){
 const done=portDrag;portDrag=null;done.ghost?.remove();
 suppressClick=true;
 if(!done.placement)return;
 edit({op:'port',snapshot,edgeId:done.port.edgeId,endpoint:done.port.endpoint,...done.placement},{busy:'Placing the port …',remember:'Port'});
}
// A label is dragged to a new place and stays there while the place is free (the option `labelPositions`); where it no longer
// fits, the engine places it again and says so. A press that does not move leaves it alone.
let labelDrag=null;
function startLabelDrag(label,event){
 if(curation||!snapshot||pending||event.button!==0||drag||portDrag||labelDrag)return;
 labelDrag={label,start:pointerInScene(event),delta:{x:0,y:0},ghost:null};
 event.preventDefault();
 $('graph').setPointerCapture?.(event.pointerId);
 track(event);
}
function moveLabel(event){
 const now=pointerInScene(event);
 labelDrag.delta={x:now.x-labelDrag.start.x,y:now.y-labelDrag.start.y};
 const r=labelDrag.label.bounds;
 if(!labelDrag.ghost){
  labelDrag.ghost=document.createElementNS('http://www.w3.org/2000/svg','rect');
  for(const [name,value] of Object.entries({fill:'none',stroke:'#90e4c3','stroke-width':2,'stroke-dasharray':'6 4','pointer-events':'none',class:'label-ghost',rx:3,width:r.width,height:r.height}))labelDrag.ghost.setAttribute(name,value);
  $('graph').append(labelDrag.ghost);
 }
 labelDrag.ghost.setAttribute('x',r.x+labelDrag.delta.x);labelDrag.ghost.setAttribute('y',r.y+labelDrag.delta.y);
}
function finishLabel(){
 const done=labelDrag;labelDrag=null;done.ghost?.remove();
 suppressClick=true;
 if(Math.hypot(done.delta.x,done.delta.y)<4)return;
 const positions={...(snapshot.options?.labelPositions??{}),[done.label.id]:{x:done.label.bounds.x+done.delta.x,y:done.label.bounds.y+done.delta.y}};
 edit({op:'configure',snapshot,options:{labelPositions:positions}},{busy:'Placing the label …',remember:'Label'});
}
// After a drag with force-directed behaviour the other nodes settle: a few bounded calls, each of at most three damped steps, until
// the largest move is below the threshold. The status says whether the placement converged or was only refined.
async function settle(calls=3){
 for(let call=0;call<calls;call++){
  if(!snapshot||pending||drag||resize||containerResize||portDrag||labelDrag||curation)return;
  const result=await edit({op:'settle',snapshot},{busy:'Settling …',chained:true});
  if(!result.ok)return;
  const report=snapshot.editReport?.settle;
  if(!report)return;
  status(`Settled: ${report.converged?'converged':'refined, not converged'} · largest move ${report.residual.toFixed(2)} after ${report.steps} step${report.steps===1?'':'s'}`);
  if(report.converged){scheduleRefinement();return;}
 }
 scheduleRefinement();
}
// A fixed size is a border the node cannot cross: the drag stops at the edge of every fixed container above the node
// (ADR 0010). The farthest one is applied first, so the nearest, which lies inside it, has the last word.
// Containers in minimum or content state grow instead, when the drop is made.
function dragPosition(event){
 const now=pointerInScene(event);
 const position={x:drag.origin.x+now.x-drag.start.x,y:drag.origin.y+now.y-drag.start.y};
 const isFixed=axis=>axis&&typeof axis==='object'&&'fixed' in axis;
 const bounded=(value,start,length,size)=>Math.min(Math.max(value,start),start+length-size);
 const kept={...position};
 const blocked=[];
 for(const container of [...drag.ancestors].reverse()){
  const sizing=snapshot.options?.containerSizing?.[container.id];
  if(!sizing)continue;
  if(isFixed(sizing.width)){kept.x=bounded(kept.x,container.bounds.x,container.bounds.width,drag.size.width);blocked.push(container.id);}
  if(isFixed(sizing.height)){kept.y=bounded(kept.y,container.bounds.y,container.bounds.height,drag.size.height);blocked.push(container.id);}
 }
 if(kept.x!==position.x||kept.y!==position.y)status(`Fixed size of ${blocked[0]}: the node stays inside its border.`);
 return kept;
}
function relativeToParent(position){return {x:position.x-drag.parent.x,y:position.y-drag.parent.y};}
// The companions move by the distance the grabbed node moved; their pins are in their own parents' frames, which only translate.
function companionPositions(position){
 const dx=position.x-drag.origin.x,dy=position.y-drag.origin.y;
 return Object.fromEntries(drag.companions.map(id=>{const box=drag.snapshotBoxes[id];return [id,{x:box.x+dx,y:box.y+dy}];}));
}
function showDragPreview(){
 previewFrame=0;
 if(drag&&dragPreview)draw();
}
// Leaf sizes the user chose. The label measurement does not overwrite them.
const userSizes=new Map();
// The outlines the page reports to the engine, each with the revision of its measurement.
const geometry=new GeometryReports();
const shapeOf=id=>snapshot.graph.nodes.find(n=>n.id===id)?.shape??'rect';
let resize=null;
function resizeTo(event){
 const point=pointerInScene(event);
 resize.size=resizedLeaf(resize.bounds,resize.start,point);
 if(!resize.ghost){
  resize.ghost=document.createElementNS('http://www.w3.org/2000/svg','rect');
  resize.ghost.setAttribute('class','resize-ghost');resize.ghost.setAttribute('fill','none');resize.ghost.setAttribute('stroke','#90e4c3');resize.ghost.setAttribute('stroke-width','2');resize.ghost.setAttribute('stroke-dasharray','6 4');resize.ghost.setAttribute('pointer-events','none');
  $('graph').append(resize.ghost);
 }
 resize.ghost.setAttribute('x',resize.bounds.x);resize.ghost.setAttribute('y',resize.bounds.y);
 resize.ghost.setAttribute('width',resize.size.width);resize.ghost.setAttribute('height',resize.size.height);
 status(`Resize ${resize.node}: ${resize.size.width} × ${resize.size.height}`);
}
// The release sends the size as a requirement: the engine lays out around it and keeps the other positions as a warm start.
function finishResize(){
 const done=resize;resize=null;done.ghost?.remove();
 suppressClick=true;
 if(!done.size)return;
 if(Math.abs(done.size.width-done.bounds.width)<=2&&Math.abs(done.size.height-done.bounds.height)<=2)return;
 userSizes.set(done.node,done.size);
 edit({op:'measure',snapshot,measurements:geometry.reports({[done.node]:done.size},shapeOf)},{busy:'Applying node size …',remember:`Size of ${done.node}`});
}
// After a drop in move-only mode: where the layout put the node, when that is not where it was dropped. Without a pin an
// organic area keeps the node and a layered area places it by its own rules, which can undo the move.
let dropNote='';
function moveOnlyNote(next){
 if(!drag||drag.cancelled||drag.fromPin||drag.wasPinned||!drag.last)return '';
 const box=next.boxes[drag.node];
 if(!box||Math.hypot(box.x-drag.last.x,box.y-drag.last.y)<=8)return '';
 return ` · ${drag.node} was placed again by the layout (no pin: drag its pin to hold a position)`;
}
function onDragMove(event){
 if(resize&&event.pointerId===resize.pointer){resizeTo(event);return;}
 if(!drag||event.pointerId!==drag.pointer)return;
 const position=dragPosition(event);
 drag.last=relativeToParent(position);
 if(!drag.moved){
  const travel=Math.hypot(position.x-drag.origin.x,position.y-drag.origin.y);
  if(travel<=4)return;
  drag.moved=true;dropNote='';
  drag.baseSnapshot=snapshot;
  drag.snapshotBoxes=snapshot.boxes;
  const startedEpoch=epoch;
  drag.gesture=new MoveGesture({
   send:request=>execute('edit',request),
   base:snapshot,
   onSnapshot:(next,info)=>{if(epoch!==startedEpoch)return;snapshot=next;if(!info.dragging){dropNote=moveOnlyNote(next);const grown=drag&&!drag.cancelled?grownSizing(next,drag.ancestors):null;const settleAfter=!!(drag&&!drag.cancelled&&next.options?.forceDirected);if(drag&&!drag.cancelled&&drag.baseSnapshot)recordHistory(`Move ${drag.node}`,drag.baseSnapshot,next);dragPreview=null;drag=null;suppressClick=true;cancelRefinement();scheduleRefinement();if(grown){edit({op:'configure',snapshot,options:{containerSizing:grown}},{busy:'Keeping the grown container size …',fit:true,remember:{label:'Grow container',merge:true}});}else if(settleAfter)setTimeout(settle,0);}draw();},
   onStats:stats=>{
    status(`Move: ${stats.sent} sent · ${stats.coalesced} coalesced · ${stats.applied} applied · ${stats.discarded} discarded${stats.error?` · ${stats.error}`:''}${dropNote}`);
    // A refused step ends the gesture: the node returns to the engine's position, and the next drag may start.
    if(stats.error&&drag&&!drag.gesture?.active){dragPreview=null;drag=null;draw();}
   },
  });
  drag.gesture.begin(drag.node,relativeToParent(position),companionPositions(position));
 } else {
  drag.gesture.update(relativeToParent(position),companionPositions(position));
 }
 dragPreview={node:drag.node,others:drag.companions,bounds:{x:position.x,y:position.y,width:drag.size.width,height:drag.size.height}};
 if(!previewFrame)previewFrame=setTimeout(showDragPreview,16);
}
// The containers the drop made larger, as container sizes: each grown axis that is not fixed gets its new extent as the
// minimum, and the other axes keep what they had. Returns null when nothing grew, so nothing is sent.
function grownSizing(next,ancestors){
 const scene=sceneOf(sceneRun,next);
 const sizing=next.options?.containerSizing??{};
 const result={};
 for(const before of ancestors){
  const now=scene.nodes.find(n=>n.id===before.id)?.bounds;
  if(!now)continue;
  // A circle container follows its content in both directions: it grows for a child dropped outward and shrinks when the
  // child comes back. A remembered minimum would only let it grow.
  if(next.graph.nodes.find(n=>n.id===before.id)?.shape==='circle')continue;
  const current=sizing[before.id]??{};
  const entry={...current};
  const isFixed=axis=>axis&&typeof axis==='object'&&'fixed' in axis;
  for(const [axis,size,grew] of [['width',now.width,now.width>before.bounds.width+0.5],['height',now.height,now.height>before.bounds.height+0.5]]){
   if(grew&&!isFixed(current[axis]))entry[axis]={min:Math.ceil(size)};
  }
  if(entry.width!==current.width||entry.height!==current.height)result[before.id]=entry;
 }
 return Object.keys(result).length?result:null;
}
// A resize handle sets the minimum width and height of a container (ADR 0010, decision 3). The preview moves the frame
// while the pointer is down; the release stores the new minimum, one change. An axis that is fixed is not changed here:
// a fixed size is set in the tree.
let containerResize=null;
function startResize(container,event){
 if(curation||!snapshot||pending||event.button!==0||drag||containerResize)return;
 const scene=sceneOf(sceneRun,snapshot);
 const shape=scene.nodes.find(n=>n.id===container.id);
 if(!shape)return;
 containerResize={id:container.id,start:pointerInScene(event),bounds:shape.bounds,width:shape.bounds.width,height:shape.bounds.height,pointer:event.pointerId};
 event.preventDefault();
 $('graph').setPointerCapture?.(event.pointerId);
 track(event);
}
function onResizeMove(event){
 if(event.pointerId!==containerResize.pointer)return;
 const now=pointerInScene(event);
 containerResize.width=Math.max(40,containerResize.bounds.width+now.x-containerResize.start.x);
 containerResize.height=Math.max(30,containerResize.bounds.height+now.y-containerResize.start.y);
 dragPreview={node:containerResize.id,bounds:{x:containerResize.bounds.x,y:containerResize.bounds.y,width:containerResize.width,height:containerResize.height}};
 if(!previewFrame)previewFrame=setTimeout(showDragPreview,16);
}
function onResizeEnd(event){
 if(event.pointerId!==containerResize.pointer)return;
 const done=containerResize;
 containerResize=null;
 dragPreview=null;
 const current=snapshot.options?.containerSizing?.[done.id]??{};
 const isFixed=axis=>axis&&typeof axis==='object'&&'fixed' in axis;
 const entry={...current};
 let changed=false;
 for(const [axis,size,old] of [['width',done.width,done.bounds.width],['height',done.height,done.bounds.height]]){
  if(Math.abs(size-old)<1)continue;
  if(isFixed(current[axis])){status(`The ${axis} of ${done.id} is fixed: set it in the tree.`);continue;}
  entry[axis]={min:Math.ceil(size)};
  changed=true;
 }
 if(!changed){draw();return;}
 edit({op:'configure',snapshot,options:{containerSizing:{[done.id]:entry}}},{busy:'Applying the container minimum …',fit:true,remember:'Container size'});
}
function onDragEnd(event){
 if(resize&&event.pointerId===resize.pointer){finishResize();return;}
 if(!drag||event.pointerId!==drag.pointer)return;
 const gesture=drag.gesture;
 // A press without movement is a click. The captured pointer sends its click to the drawing, not to the node, so the
 // node is selected here.
 if(!gesture){const node=drag.node,fromPin=drag.fromPin,pinned=drag.wasPinned;drag=null;if(fromPin){pinClickGuard=performance.now()+400;setPin(node,!pinned);}else selectNode(node,event.ctrlKey||event.metaKey);return;}
 gesture.finish({release:releaseOnDrop(drag.fromPin,drag.wasPinned)});
}
// Selects a node of the view: the same selection as a click on the drawing, and the same one the containment tree uses.
function selectNode(id,add=false){viewSelected=false;
 if(add)selection.toggleNode(id);else selection.replaceNode(id);selected=id;$('selected').textContent=snapshot.graph.nodes.find(n=>n.id===id)?.label??id;draw();
}
// Pointer capture on the drawing sends the moves and the release to it, also outside the node. Escape cancels a move.
// Ctrl/Cmd+Z undoes the last container size change, unless a text field has the focus.
document.addEventListener?.('keydown',event=>{
 if(!(event.ctrlKey||event.metaKey)||/INPUT|TEXTAREA|SELECT/.test(document.activeElement?.tagName??''))return;
 const key=event.key.toLowerCase();
 if(key==='z'&&!event.shiftKey){event.preventDefault();undo();}
 else if((key==='z'&&event.shiftKey)||key==='y'){event.preventDefault();redo();}
});
// A gesture listens on the window from its press to its end (gesture.js): a release outside the drawing or the window,
// a cancelled pointer, a lost capture and Escape all end it. Only a release commits; every other reason abandons it.
let tracker=null;
function track(event){
 tracker?.end('error');
 tracker=trackGesture({win:window,pointerId:event.pointerId,onMove:onGestureMove,onEnd:onGestureEnd});
}
function onGestureMove(event){if(labelDrag)moveLabel(event);else if(portDrag)movePort(event);else if(containerResize)onResizeMove(event);else onDragMove(event);}
function onGestureEnd(reason,event){
 tracker=null;
 if(reason==='up'){onGestureMove(event);if(labelDrag)finishLabel();else if(portDrag)finishPort();else if(containerResize)onResizeEnd(event);else onDragEnd(event);return;}
 abortGesture(reason);
}
// Puts the drawing back to the engine's state. A move that was begun is cancelled in the engine (the node returns to
// where it was); when the view or the model changed, the gesture is abandoned and its replies are ignored.
function abortGesture(reason){
 const replaced=reason==='modelchange'||reason==='viewchange';
 if(labelDrag){labelDrag.ghost?.remove();labelDrag=null;}
 else if(portDrag){portDrag.ghost?.remove();portDrag=null;}
 else if(containerResize){containerResize=null;dragPreview=null;}
 else if(resize){resize.ghost?.remove();resize=null;}
 else if(drag){
  if(drag.gesture?.active&&!replaced){drag.cancelled=true;drag.gesture.cancel();}
  else{drag.gesture?.abandon();dragPreview=null;drag=null;}
 }
 if(!replaced&&snapshot){status(`Gesture cancelled (${reason}).`);draw();}
}
$('add').onclick=()=>{if(!selected)return;edit({op:'add',snapshot,sourceId:selected,label:$('name').value||'New function'},{busy:'Local computation …'});};
// A shape change is an engine operation: the node's outline changes, the other positions are kept as a warm start.
$('shape-select').onchange=()=>{if(!selected||pending)return;edit({op:'shape',snapshot,nodeId:selected,shape:$('shape-select').value},{busy:'Applying shape …',remember:'Shape'});};
$('fit').onclick=()=>snapshot&&draw(true);$('zoom').onclick=()=>{autoFit=false;scale=Math.min(scale*1.5,2);if(snapshot)draw();};
$('save').onclick=()=>{const url=URL.createObjectURL(new Blob([JSON.stringify(snapshot,null,2)],{type:'application/json'}));const a=document.createElement('a');a.href=url;a.download=`arqlet-revision-${snapshot.revision}.json`;a.click();setTimeout(()=>URL.revokeObjectURL(url),1000);};
// The default graph uses the native precomputation in split and backend modes. Browser-only mode
// and every other example are imported through the mode's import lane (WASM worker or backend).
async function loadInitial(){
 const example=$('example').value||'default';
 // With a node budget the default graph is imported whole (the budget bounds the view), not the precomputed snapshot.
 const budget=Math.round(Number($('node-budget').value));
 // Communities (provisional) apply to an import too: the graph is imported with them.
 const communities=Math.round(Number($('communities').value));
 const collapseFirst=$('initial-collapse').value,domainGroups=$('domain-groups').checked;
 const abstracted=communities>0||collapseFirst!==''||domainGroups;
 if(example==='default'&&mode()!=='browser'&&!(budget>0)&&!abstracted){
  const r=await fetch('./generated/snapshot.json',{cache:'no-store'});
  if(!r.ok)throw Error('Snapshot missing. Please run the demo build first.');
  return {snapshot:await r.json(),message:'Natively precomputed snapshot loaded'};
 }
 const url=example==='default'?'./generated/graph.json':exampleNotes.get(example)?.url??`./generated/examples/${example}.json`;
 const graph=await fetch(url,{cache:'no-store'}).then(r=>{if(!r.ok)throw Error(`${url} is missing. Please run the demo build first.`);return r.json();});
 // Entity-relationship notation (ADR 0011): an entity is a leaf node whose rows are its title and its attributes; the
 // relationships attach to those rows. The attributes are not graph nodes in the layout.
 // A graph in ELK's JSON form (`children`): laid out through the ELK adapter, as `elkLayout` lays it out for a host;
 // the snapshot behind it continues here like any other.
 if(Array.isArray(graph.children)){
  const data=await execute('import',{op:'elkLayout',graph,snapshot:true});
  if(!data.ok)throw Error(data.error);
  const notes=(data.diagnostics??[]).length?` · ELK adapter: ${data.diagnostics.join('; ')}`:'';
  return {snapshot:data.snapshot,message:`ELK graph „${$('example').selectedOptions?.[0]?.textContent??example}“ laid out through the ELK adapter${notes}`};
 }
 // An example may bring its own groups (node id → path of group names); they travel as an option, not in the graph.
 const {groups,...plain}=graph;
 const projected=erProjection(plain);
 const extra={...(budget>0?{visibleNodeBudget:budget}:{}),...(communities>0?{communities}:{}),
  ...(domainGroups&&groups?{groups}:{}),...(collapseFirst!==''?{levelOfDetail:{depth:Number(collapseFirst)}}:{})};
 const importOptions=Object.keys(extra).length?{...(projected.options??{}),...extra}:projected.options;
 const limit=example==='default'&&!(budget>0)&&!abstracted?VIEW_LIMIT:EXAMPLE_LIMIT;
 const data=await execute('import',{op:'import',graph:projected.graph,limit,...(importOptions?{options:importOptions}:{})});
 if(!data.ok)throw Error(data.error);
 const snapshot=data.snapshot;
 return {snapshot,message:example==='default'?'Imported from the graph in the browser':`Example „${$('example').selectedOptions?.[0]?.textContent??example}“ imported`};
}
// Examples come from the demo build; without an index only the default graph is offered.
const exampleNotes=new Map();
// Local examples (server.py --local-examples, data kept outside the repository) follow them when the server has any.
Promise.all(['./generated/examples/index.json','./local/index.json'].map(u=>fetch(u,{cache:'no-store'}).then(r=>r.ok?r.json():[]).catch(()=>[]))).then(lists=>lists.flat()).then(list=>{
 for(const e of list){const option=document.createElement('option');option.value=e.name;option.textContent=`${e.title} · ${e.nodes} nodes`;option.title=e.description??'';exampleNotes.set(e.name,e);$('example').append(option);}
}).catch(()=>{});
// What the example is, where it comes from, and how much of it the view shows (the view is bounded, so a large one can be cut).
function renderExampleInfo(){
 const e=exampleNotes.get($('example').value);
 if(!e||!snapshot){$('example-info').textContent='';return;}
 const total=(snapshot.sourceGraph??snapshot.graph).nodes.length,shown=snapshot.graph.nodes.length;
 const cut=total<e.nodes?` The import is cut: ${total} of ${e.nodes} nodes (limit ${EXAMPLE_LIMIT}).`:'';
 $('example-info').textContent=`${e.description??''} Source: ${e.source??'unknown'}. ${e.nodes} nodes in the model, ${total} in the import, ${shown} visible.${cut}`;
}
$('example').onchange=()=>{$('example-info').textContent='';reset();};
// The node budget (experimental) applies to an import: a change imports the example again.
$('node-budget').onchange=()=>{$('example-info').textContent='';reset();};
$('communities').onchange=()=>{$('example-info').textContent='';reset();};
$('domain-groups').onchange=()=>{$('example-info').textContent='';reset();};
$('initial-collapse').onchange=()=>{$('example-info').textContent='';reset();};
// Scene geometry comes from the engine; the demo waits for it before the first draw.
let sceneRun=null;
const sceneEngine=loadEngine('./generated/arqlet.wasm').then(run=>{sceneRun=run;});
// Presets are versioned config sets with run policy (presets/*.json, indexed by build.sh). The selector
// shows the preset the snapshot is in; any change outside a preset clears its configId and shows
// "Custom". A preset applies its direction first, then its config set.
const presets=new Map();
fetch('./generated/presets/index.json',{cache:'no-store'}).then(r=>r.ok?r.json():[]).then(list=>{
 for(const preset of list){presets.set(preset.id,preset);const option=document.createElement('option');option.value=preset.id;option.textContent=preset.title;$('profile').append(option);}
 const custom=document.createElement('option');custom.value='__custom__';custom.disabled=true;custom.textContent='Custom';$('profile').append(custom);
 if(snapshot)draw();
}).catch(()=>{});
// The configId is "<preset id>@<version>"; a snapshot with no configId has custom options.
$('profile').onchange=async()=>{
 const preset=presets.get($('profile').value);
 if(!preset||!snapshot||pending)return;
 const started=epoch;
 const result=await applyPreset({preset,snapshot,stillCurrent:()=>epoch===started&&!pending,
   run:request=>edit({...request,snapshot:request.snapshot},{busy:'Applying preset …',fit:true,chained:true})});
 if(result.ok){draw(true);scheduleRefinement();status(`Preset "${preset.title}" applied`);}
 else{status(result.error);draw();}
};
async function reset(){
 history.clear();
 tracker?.end('modelchange');
 const token=++loadToken;epoch++;cancelRefinement();curation=null;pending=null;selected=null;treeSignature=null;userSizes.clear();geometry.clear();selection.clear();$('add').disabled=true;
 try{await sceneEngine;const loaded=await loadInitial();if(token!==loadToken)return;snapshot=loaded.snapshot;status(loaded.message);$('selected').textContent='Select a node in the diagram.';draw(true);scheduleRefinement();}
 catch(e){if(token===loadToken)status(e.message);}
}
$('reset').onclick=reset;
// Layout statistics: the shared metrics of the displayed layout. Computed once per snapshot; during curation the
// engine's report of the draft is shown instead.
const metricCache=new WeakMap();
function metricsOf(snapshot){
 if(!metricCache.has(snapshot)){
  let metrics=null;
  try{metrics=evaluateSnapshot(sceneRun,snapshot).metrics;}catch{metrics=null;}
  metricCache.set(snapshot,metrics);
 }
 return metricCache.get(snapshot);
}
function renderStats(){
 const table=$('layout-stats');
 if(!snapshot||!sceneRun){table.replaceChildren();return;}
 const m=curation?curation.report.metrics:metricsOf(snapshot);
 const q=snapshot.optimization;
 const row=(label,value,section=false)=>{
  const tr=document.createElement('tr');if(section)tr.className='section';
  const a=document.createElement('td');a.textContent=label;const b=document.createElement('td');b.textContent=value;
  tr.append(a,b);return tr;
 };
 const rows=[row('Layout',curation?'draft (current geometry)':'computed',true)];
 rows.push(row('Visible nodes / edges',`${snapshot.graph.nodes.length} / ${snapshot.graph.edges.length}`));
 rows.push(row('Unresolved edges left out',snapshot.omittedUnresolvedEdges??0));
 rows.push(row('Pinned nodes',Object.keys(effectiveOptions().pins).length));
 rows.push(row('Routing issues / geometry issues',`${(snapshot.routingIssues??[]).length} / ${(snapshot.geometryIssues??[]).length}`));
 rows.push(row('Quality (shared metrics)',m?`metric version ${m.version}`:'not available',true));
 if(m){
  rows.push(row('Crossings without a shared endpoint',m.crossings));
  rows.push(row('Crossings with a shared endpoint',m.sharedEndpointCrossings));
  rows.push(row('Edges through nodes',m.lineNodeHits));
  rows.push(row('Foreign container traversals',m.containerTraversals));
  rows.push(row('Overlapping edge pairs',m.sharedLinePairs));
  rows.push(row('Near-parallel edge pairs',m.nearParallelPairs));
  rows.push(row('Edges near a border',m.hugPairs));
  rows.push(row('Bends (total / max / mean)',`${m.bends} / ${m.bendsMax} / ${m.bendsMean.toFixed(1)}`));
  rows.push(row('Edge length (total / mean / max)',`${Math.round(m.length)} / ${Math.round(m.lengthMean)} / ${Math.round(m.lengthMax)}`));
  rows.push(row('Area',Math.round(m.area).toLocaleString('en-US')));
 }
 if(!curation&&q){
  rows.push(row('Optimization',`${q.candidatesEvaluated} layouts checked`,true));
  rows.push(row('Strategy',q.strategy));
  rows.push(row('Stop reason',q.stopReason??'complete'));
 }
 table.replaceChildren(...rows);
}

// Curation feedback. A route with a conflict is drawn red; the conflicts are the engine's, the same as the
// route details. A pending route is dashed: it waits for re-routing.
function markConflicts(scene){
 for(const route of scene.routes){
  try{
   const c=routeConflictsOf(sceneRun,scene,route.id);
   route.invalid=c.lineNodeHits.length+c.containerTraversals.length+c.crossings.length+c.sharedLines.length>0;
  }catch{route.invalid=false;}
 }
}
function markPending(){
 const pending=new Set(curation.document.curated.pendingRoutes);
 for(const line of $('graph').querySelectorAll('polyline[data-route-id]')){
  if(pending.has(line.getAttribute('data-route-id')))line.setAttribute('stroke-dasharray','8 5');
 }
}
// The badge on the canvas: the draft's state at a glance, in the same words as the sidebar.
function syncBadge(){
 const badge=$('curation-badge');
 if(!curation){badge.hidden=true;return;}
 const r=curation.report;
 badge.hidden=false;badge.className=r.valid?'ok':'warn';
 const headline=r.valid?`Draft valid · revision ${curation.document.revision}`:`${r.pendingRoutes} routes pending · ${r.hardViolations} hard violations`;
 const legend=document.createElement('small');
 legend.textContent='Dashed: waiting for re-routing · red: conflict · drag to move';
 badge.replaceChildren(document.createTextNode(headline),legend);
}

// Manual curation: an explicit draft of the displayed snapshot. Each change is one undo step; the engine refreshes,
// seals and validates every change, and a change the engine refuses leaves the draft as it was.
const runReference=request=>execute('edit',request);
function syncCuration(){
 $('curate').textContent=curation?'End legacy curation':'Legacy curation (start a draft)';
 $('curate').disabled=!!pending||!snapshot;
 syncHistoryButtons();
 $('save-ref').disabled=!snapshot||!!pending;$('load-ref').disabled=!snapshot||!!pending;
 $('reroute').disabled=!curation||rerouting||curation.report.pendingRoutes===0;
 $('reroute-all').disabled=!curation;
 syncBadge();
 $('curation-info').textContent=curation
  ?`Draft "${curation.document.referenceId}" · Revision ${curation.document.revision} · ${curation.report.pendingRoutes} routes pending · ${curation.report.hardViolations} hard violations · ${curation.report.valid?'valid':'not valid yet'}`
  :'The view is computed, not edited.';
}
$('curate').onclick=async()=>{
 if(curation){curation=null;status('Curation ended. The view is computed again.');draw(true);scheduleRefinement();return;}
 if(!snapshot||pending)return;
 cancelRefinement();status('Creating draft …');
 try{curation=await CurationSession.create(runReference,snapshot,`${$('example').value||'default'}-r${snapshot.revision}`);status('Curation active: drag nodes, move segments. Undo is always available.');}
 catch(e){status(e.message);}
 draw(true);
};
// The buttons follow what is being edited: the legacy draft has its own stack, the direct view uses the engine's decisions.
function syncHistoryButtons(){
 $('undo').disabled=curation?!curation.canUndo:(!history.canUndo||!!pending);
 $('redo').disabled=curation?!curation.canRedo:(!history.canRedo||!!pending);
}
$('undo').onclick=()=>{if(curation){if(curation.undo()){status('Undone.');draw();}}else undo();};
$('redo').onclick=()=>{if(curation){if(curation.redo()){status('Redone.');draw();}}else redo();};
// Re-routing runs in the engine worker and can take seconds. The status names it while it runs, the buttons wait,
// and the result reports the time. Edits made meanwhile queue behind it in the session.
let rerouting=false,autoRerouteTimer=null,autoRerouteBlockedAt=null;
async function runReroute(label,start){
 if(!curation||rerouting)return;
 rerouting=true;$('reroute').disabled=true;$('reroute-all').disabled=true;
 status(`${label} … ${curation.document.curated.pendingRoutes.length} pending`);
 const began=Date.now();
 let reply;
 try{reply=await start();}finally{rerouting=false;}
 const seconds=((Date.now()-began)/1000).toFixed(1);
 if(!reply.ok){
  // A failure is not retried automatically until the curation changes again.
  autoRerouteBlockedAt=curation?.document.revision??null;
  status(`${reply.error} (after ${seconds} s)`);
  draw();return;
 }
 if(reply.report.pendingRoutes>0){
  // Some routes could not be found: they stay pending, and the automatic re-route does not retry them until the curation changes.
  autoRerouteBlockedAt=curation.document.revision;
  status(`${curation.document.curated.pendingRoutes.join(', ')} could not be routed in ${seconds} s; the others are re-routed · ${reply.report.hardViolations} hard violations`);
  draw();return;
 }
 status(`${label.replace(' …','')} in ${seconds} s · ${reply.report.pendingRoutes} pending · ${reply.report.hardViolations} hard violations`);
 draw();
}
// Pending routes are re-routed on their own shortly after the last change; the editor stays usable meanwhile.
function scheduleAutoReroute(){
 clearTimeout(autoRerouteTimer);
 if(!curation||rerouting||!curation.document.curated.pendingRoutes.length||autoRerouteBlockedAt===curation.document.revision)return;
 autoRerouteTimer=setTimeout(()=>{
  if(!curation||rerouting||!curation.document.curated.pendingRoutes.length)return;
  runReroute('Re-routing pending routes …',()=>curation.reroute());
 },400);
}
$('reroute-all').onclick=()=>{if(curation)runReroute('Re-routing all routes …',()=>curation.rerouteAll());};
$('reroute').onclick=()=>{if(curation)runReroute('Re-routing pending routes …',()=>curation.reroute());};
// A reference is saved from the draft when there is one, else from the view as it is: a draft is made for the file only.
$('save-ref').onclick=async()=>{
 try{
  const draft=curation??await CurationSession.create(runReference,snapshot,`${$('example').value||'default'}-r${snapshot.revision}`);
  downloadText(`${draft.document.referenceId}.lwref.json`,await draft.canonical());status('Reference saved.');
 }catch(e){status(e.message);}
};
// What a reference holds beyond its baseline view: the curated parts that only the legacy draft shows and edits.
function curatedParts(document){
 const curated=document.curated??{};
 const baseline=new Map((document.baseline?.routes??[]).filter(r=>!r.bundleOf).map(r=>[r.id,JSON.stringify(r.points)]));
 const changedRoutes=Object.entries(curated.routes??{}).filter(([id,points])=>baseline.has(id)&&baseline.get(id)!==JSON.stringify(points)).length;
 return {
  'locked routes':(curated.lockedRoutes??[]).length,
  'size pins':Object.keys(curated.sizePins??{}).length,
  'moved labels':Object.keys(curated.labelPositions??{}).length,
  'pending routes':(curated.pendingRoutes??[]).length,
  'changed routes':changedRoutes,
 };
}
$('load-ref').onchange=async()=>{
 const file=$('load-ref').files?.[0];$('load-ref').value='';if(!file)return;
 try{
  const loaded=await CurationSession.load(runReference,await readFileText(file));
  const parts=Object.entries(curatedParts(loaded.document)).filter(([,count])=>count>0);
  cancelRefinement();selected=null;selection.clear();history.clear();
  if(parts.length){
   // The direct view cannot show these parts, so the reference opens as a legacy draft, where they are kept and editable.
   curation=loaded;snapshot=loaded.document.baseline;
   status(`Reference "${loaded.document.referenceId}" opened as a legacy draft: it holds ${parts.map(([name,count])=>`${count} ${name}`).join(', ')}.`);
  } else {
   curation=null;snapshot=loaded.document.baseline;
   status(`Reference "${loaded.document.referenceId}" opened.`);
  }
  draw(true);
 }
 catch(e){status(e.message);}
};
$('reset').onclick=reset;
// Empty canvas or Escape clears the route selection; clicks on nodes and routes bubble to their own handlers.
$('graph').onclick=e=>{if(e.target===$('graph')&&(selectedRoute||!selection.empty)){selection.clear();selectedRoute=null;selectionKey=null;draw();}};
document.addEventListener?.('keydown',e=>{if(e.key==='Escape'&&(selectedRoute||!selection.empty)){selection.clear();selectedRoute=null;selectionKey=null;draw();}});
// The inspector shows the selection; its buttons lock a selected route, pin a selected node's size, or note the selection.
function noteTarget(){return selection.route??(selection.nodes.length===1?selection.nodes[0]:null);}
function renderInspector(){
 $('inspector').textContent=curation?describeSelection(curation.document,selection).join('\n'):'Open or create a reference to inspect a selection.';
 const route=selection.route,single=selection.nodes.length===1?selection.nodes[0]:null,curated=curation?.document.curated;
 $('inspector-lock').disabled=!curation||!route;
 $('inspector-lock').textContent=route&&curated?.lockedRoutes?.includes(route)?'Unlock route':'Lock route';
 $('inspector-pin').disabled=!curation||!single;
 $('inspector-pin').textContent=single&&curated?.sizePins?.[single]?'Unpin size':'Pin size';
 const target=noteTarget();
 $('note-add').disabled=!curation||!target;
 $('note-remove').disabled=!curation||!target||!(curation?.document.annotations??[]).some(note=>note.target===target);
}
$('inspector-lock').onclick=async()=>{
 const route=selection.route;if(!curation||!route)return;
 const locked=curation.document.curated.lockedRoutes?.includes(route);
 const reply=await curation.lockRoute(route,!locked);
 if(!reply.ok){status(reply.error);return;}
 status(locked?'Route unlocked.':'Route locked.');draw();
};
$('note-add').onclick=async()=>{
 const target=noteTarget();if(!curation||!target)return;
 const reply=await curation.addNote(target,$('note-text').value,$('note-category').value,$('note-severity').value);
 if(!reply.ok){status(reply.error);return;}
 $('note-text').value='';status('Note added.');draw();
};
$('note-remove').onclick=async()=>{
 const target=noteTarget();if(!curation||!target)return;
 const reply=await curation.removeNotes(target);
 if(!reply.ok){status(reply.error);return;}
 status('Notes removed.');draw();
};
$('inspector-pin').onclick=async()=>{
 const single=selection.nodes.length===1?selection.nodes[0]:null;if(!curation||!single)return;
 const reply=await curation.toggleSizePin(single);
 if(!reply.ok){status(reply.error);return;}
 status('Size pin changed.');draw();
};
$('mode').onchange=()=>{$('refine').textContent=refineLabels[mode()];reset();};
// Detail settings: the effective options (snapshot options completed by the engine defaults) are shown, and each
// change is one configure patch. The engine clears the profile name for any change outside a profile.
let optionDefaults=null;
function effectiveOptions(){
 const base=optionDefaults??{};const given=snapshot?.options??{};
 return {...base,...given,appearance:{...(base.appearance??{}),...(given.appearance??{})},pins:given.pins??{},portPlacement:given.portPlacement??'symmetric',configId:given.configId??null};
}
function syncDetails(){
 if(!optionDefaults)return;
 const o=effectiveOptions();
 for(const n of [1,2,3,4])$(`cand-${n}`).checked=o.candidates.includes(n);
 $('routing-candidate').checked=!!o.routingCandidate;$('hybrid-by').value=o.hybridBy;$('bundle').value=o.bundling;
 $('port-placement').value=o.portPlacement;$('routing-engine').value=o.routingEngine;
 $('placement-grid').value=o.placementGrid;$('placement-search').value=o.placementSearch;$('min-port-spacing').value=o.minPortSpacing;
 $('free-placement').checked=!!o.freePlacement;$('routing-clearance').value=o.routingClearance;$('track-spacing').value=o.trackSpacing;
 $('routing-corridors').checked=!!o.routingCorridors;$('routing-track-separation').checked=!!o.routingTrackSeparation;
 $('show-ports').checked=o.appearance.showPorts!==false;
 $('layered-construction').checked=!!o.layeredConstruction&&!!o.hierarchicalOrdering;
 for(const [id,key] of FLAGS)$(id).checked=!!o[key];
 for(const [id,key] of OPTIONAL_NUMBERS)$(id).value=o[key]??'';
 $('archive-refinement').value=o.archiveRefinement??0;
 syncObjectives(o);
 $('edge-labels').checked=Object.keys(o.edgeLabels??{}).length>0;$('port-labels').checked=Object.keys(o.portLabels??{}).length>0;
}
// Objectives in priority order: a tick appends or removes one, the arrows move an active one. The list keeps at
// least one objective; the snapshot's order is the order of the list.
const OBJECTIVES=['siblingOverlaps','edgeNodeHits','sharedRoutePairs','nearNodeSegments','routedCrossings','projectedCrossings','bends','cost','endToEnd'];
function syncObjectives(o){
 const active=o.objectives??[];
 for(const k of OBJECTIVES){
  const at=active.indexOf(k);
  $(`obj-${k}`).checked=at>=0;
  $(`rank-${k}`).textContent=at>=0?String(at+1):'';
  $(`up-${k}`).disabled=at<=0;
  $(`down-${k}`).disabled=at<0||at===active.length-1;
 }
}
function changeObjectives(next,label){
 if(!next.length){status('At least one objective stays active.');syncDetails();return;}
 return setOption({objectives:next},label);
}
function moveObjective(k,step){
 const active=[...(effectiveOptions().objectives??[])];
 const at=active.indexOf(k),to=at+step;
 if(at<0||to<0||to>=active.length)return;
 [active[at],active[to]]=[active[to],active[at]];
 return setOption({objectives:active},'Objective order');
}
for(const k of OBJECTIVES){
 $(`obj-${k}`).onchange=()=>{
  const active=effectiveOptions().objectives??[];
  changeObjectives($(`obj-${k}`).checked?[...active,k]:active.filter(x=>x!==k),'Objectives');
 };
 $(`up-${k}`).onclick=()=>moveObjective(k,-1);
 $(`down-${k}`).onclick=()=>moveObjective(k,1);
}

// One detail change. Unchanged values send nothing, so a no-op does not clear the profile.
function setOption(patch,label){
 if(!snapshot||pending||curation){syncDetails();return;}
 const current=effectiveOptions();
 const changed=Object.entries(patch).some(([key,value])=>JSON.stringify(current[key])!==JSON.stringify(value));
 if(!changed)return;
 return edit({op:'configure',snapshot,options:patch},{busy:`Applying ${label} …`,fit:true,remember:label});
}
const CANDIDATES=[1,2,3,4];
for(const n of CANDIDATES)$(`cand-${n}`).onchange=()=>{
 const next=CANDIDATES.filter(k=>$(`cand-${k}`).checked);
 if(!next.length){status('At least one ordering strategy stays active.');syncDetails();return;}
 setOption({candidates:next},'Ordering strategies');
};
$('routing-candidate').onchange=()=>setOption({routingCandidate:$('routing-candidate').checked},'Routing candidate');
$('hybrid-by').onchange=()=>setOption({hybridBy:$('hybrid-by').value},'hybrid criterion');
$('routing-engine').onchange=()=>setOption({routingEngine:$('routing-engine').value},'Routing method');
$('free-placement').onchange=()=>setOption({freePlacement:$('free-placement').checked},'free placement');
$('routing-corridors').onchange=()=>setOption({routingCorridors:$('routing-corridors').checked},'corridors');
$('routing-track-separation').onchange=()=>setOption({routingTrackSeparation:$('routing-track-separation').checked},'track separation');
function numberOption(id,key,label,{min,max,integer=false}){
 $(id).onchange=()=>{
  const value=Number($(id).value);
  if(!Number.isFinite(value)){syncDetails();return;}
  const bounded=Math.min(max,Math.max(min,integer?Math.round(value):value));
  setOption({[key]:bounded},label);
 };
}
numberOption('placement-grid','placementGrid','Grid size',{min:0,max:100});
numberOption('placement-search','placementSearch','placement search',{min:0,max:2000,integer:true});
numberOption('min-port-spacing','minPortSpacing','port spacing',{min:0,max:200});
numberOption('routing-clearance','routingClearance','Clearance to nodes',{min:0,max:50});
numberOption('track-spacing','trackSpacing','track spacing',{min:1,max:50});
numberOption('archive-refinement','archiveRefinement','archive refinement',{min:0,max:50,integer:true});
// On/off options and optional numbers; an empty number field clears its option, so the engine default applies.
const FLAGS=[['component-packing','componentPacking'],['vpsc-overlap-removal','vpscOverlapRemoval'],['directed-repair','directedRepair'],['node-edge-repulsion','nodeEdgeRepulsion'],['sparse-stress','sparseStress'],['scalable-stress','scalableStress'],['force-directed','forceDirected'],['strict-clearance','strictClearance'],['extended-conflict-selection','extendedConflictSelection'],['routed-candidates','routedCandidates'],['flat-region','flatRegion'],['global-layering','globalLayering'],['simplex-ranks','simplexRanks']];
for(const [id,key] of FLAGS)$(id).onchange=()=>setOption({[key]:$(id).checked},key);
$('layered-construction').onchange=()=>{
 const enabled=$('layered-construction').checked;
 setOption({layeredConstruction:enabled,hierarchicalOrdering:enabled},enabled?'Hierarchical layered pipeline':'Previous layout pipeline');
};
const OPTIONAL_NUMBERS=[['organic-iterations','organicIterations',{min:1,max:5000,integer:true}],['max-extent','maxExtent',{min:1,max:1e6}],['required-clearance','requiredClearance',{min:1,max:50}]];
for(const [id,key,{min,max,integer=false}] of OPTIONAL_NUMBERS)$(id).onchange=()=>{
 const text=String($(id).value).trim();
 if(text===''){if(effectiveOptions()[key]!=null)setOption({[key]:null},key);return;}
 const value=Number(text);
 if(!Number.isFinite(value)){syncDetails();return;}
 setOption({[key]:Math.min(max,Math.max(min,integer?Math.round(value):value))},key);
};
// Pins: the current position of a node or a container, relative to its parent, is fixed. A pinned container keeps its
// place and its contents are still arranged inside it. The canvas controls and the buttons use this one path.
function isContainer(id){
 return !!id&&(snapshot.sourceGraph??snapshot.graph).nodes.some(n=>n.parentId===id);
}
function setPin(id,pin){
 if(!snapshot||!id)return;
 if(curation){status('Curation is active: end it to pin or unpin positions.');return;}
 if(pending){status('An operation is already running.');return;}
 const pins={...effectiveOptions().pins};
 if(pin){const box=snapshot.boxes?.[id];if(!box)return;pins[id]={x:box.x,y:box.y};}
 else delete pins[id];
 return setOption({pins},pin?'Pin':'Unpin');
}
function syncPins(){
 const pinned=!!selected&&!!effectiveOptions().pins[selected];
 const usable=!!selected&&!!snapshot.boxes?.[selected]&&!curation&&!pending;
 const kind=isContainer(selected)?'container':'node';
 $('pin-node').disabled=!usable||pinned;$('unpin-node').disabled=!usable||!pinned;
 $('pin-info').textContent=!selected?'Select a node or container to pin its position.'
  :pinned?`This ${kind} is pinned: layout keeps it in place${kind==='container'?' and arranges its contents inside':''}.`
  :`Pinning keeps this ${kind} where it is when the layout is recomputed.`;
}
$('pin-node').onclick=()=>setPin(selected,true);
$('unpin-node').onclick=()=>setPin(selected,false);
// The engine defaults are read once; until then the controls keep their markup values.
execute('edit',{op:'optionDefaults'}).then(reply=>{if(reply.ok){optionDefaults=reply.options;if(snapshot)draw();}}).catch(()=>{});

$('refine').textContent=refineLabels[mode()];reset();

function cancelRefinement(){clearTimeout(refineTimer);refineTimer=null;if(refinement)refinement.abort();refinement=null;session.cancel();if(structuring)structuring.abort();structuring=null;}
function scheduleRefinement(){clearTimeout(refineTimer);if(snapshot&&!curation&&$('auto').checked&&presetState(snapshot,presets).preset?.refine!==false)refineTimer=setTimeout(refine,350);}
// Why a refinement stopped early; the published result is always the last complete candidate.
const STOP_TEXT={workBudget:'Work limit reached: last complete result.',timeBudget:'Time limit reached: last complete result.',cancelled:'Cancelled: last complete result.'};
// Host-side throttle for shown intermediate results: at most one redraw per interval; the newest result wins.
const PUBLICATION_INTERVAL_MS=120;
let jobCounter=0;
// Results that arrive for a view that has changed since (or for a job that a newer one superseded) are not applied. They are counted
// and shown, so a result that does not appear is explained.
let staleDiscarded=0;
function noteDiscarded(){
 staleDiscarded++;
 const note=$('discarded');
 note.hidden=false;note.textContent=`Stale results discarded: ${staleDiscarded}`;
}
async function refine(){
 if(!snapshot||pending||drag||resize||containerResize||portDrag||labelDrag)return;
 cancelRefinement();const controller=new AbortController();refinement=controller;
 // The job's base is the state it starts from. Shown publications move the displayed snapshot forward, so every
 // publication and the final result are checked against this base, never against what is displayed.
 const job={id:`refine-${++jobCounter}`,epoch,revision:snapshot.revision,layoutVersion:snapshot.layoutVersion??0,lastSequence:0};
 // The page's current state is the snapshot the job starts from. The session policy keeps the job's base apart from
 // the displayed snapshot, which moves forward with every shown publication.
 session.edit({graphRevision:snapshot.revision,baseLayoutVersion:snapshot.layoutVersion??0});
 session.start(job.id,session.current);
 const streaming=placement[mode()].refine!=='backend';
 $('backend').textContent='Optimizing the whole view …';
 const started=performance.now(),sentAt=Date.now();let firstShownMs=null;
 // Latency of each shown publication: computation (request to emission), transfer and display.
 const timeline=globalThis.lwRefineTimeline=[];
 let showTimer=null;
 const show=()=>{
   const next=session.takePending();
   if(!next)return;
   snapshot=next.snapshot;
   if(firstShownMs===null)firstShownMs=Math.round(performance.now()-started);
   draw();
   const shownAt=Date.now(),meta=next.meta??{};
   timeline.push({sequence:next.sequence,bytes:meta.bytes??null,computeMs:meta.emittedAt-sentAt,transferMs:meta.receivedAt-meta.emittedAt,displayMs:shownAt-meta.receivedAt,shownMs:shownAt-sentAt});
   $('backend').textContent=`Improving … ${next.sequence} intermediate layouts, newest shown.`;
 };
 // Leading and trailing throttle: the first result is shown at once; results that arrive within the interval are
 // merged, and the newest one is shown when the interval ends.
 const onPublication=(publication,meta)=>{
   if(refinement!==controller||!acceptPublication(publication,job,epoch,!!pending)){noteDiscarded();return;}
   if(session.offer(publication).verdict!=='accepted')return;
   job.lastSequence=publication.sequence;
   publication.meta=meta;
   if(showTimer===null){show();showTimer=setTimeout(()=>{showTimer=null;show();},PUBLICATION_INTERVAL_MS);}
 };
 const op=streaming?'refineProgressive':'refine';
 try{
   const data=await execute('refine',{op,jobId:job.id,snapshot,expectedRevision:job.revision,expectedLayoutVersion:job.layoutVersion,threads:threads()},controller.signal,streaming?onPublication:undefined);
   if(!data.ok)throw Error(data.error||'Optimization not available');
   clearTimeout(showTimer);showTimer=null;
   if(refinement!==controller){noteDiscarded();return;}
   // The final result replaces whatever was shown or pending; the job is complete.
   session.finish(job.id);
   if(!refinementFits({revision:job.revision,layoutVersion:job.layoutVersion},data.snapshot,job,epoch,!!pending)){noteDiscarded();$('backend').textContent='Stale refinement discarded.';return;}
   snapshot=data.snapshot;draw();const q=snapshot.optimization;const timing=firstShownMs===null?'':` First improvement shown after ${firstShownMs} ms.`;$('backend').textContent=q?`${q.stopReason?STOP_TEXT[q.stopReason]+' ':''}${q.candidatesEvaluated} layouts checked · ${q.strategy==='keep-existing'?'existing layout kept':'new layout adopted'}. Edges through nodes: ${q.before.edgeNodeHits} → ${q.after.edgeNodeHits}. Crossing edge pairs: ${q.before.routedCrossings} → ${q.after.routedCrossings}. Shared lines (pairs): ${q.before.sharedRoutePairs} → ${q.after.sharedRoutePairs}. Edge length: ${Math.round(q.before.routeLength)} → ${Math.round(q.after.routeLength)}.${timing}`:'Refinement complete.';
 }catch(e){clearTimeout(showTimer);session.fail(job.id);if(e.name!=='AbortError'&&refinement===controller)$('backend').textContent=`Refinement: ${e.message}. Local editing stays available.`;}
 finally{if(refinement===controller)refinement=null;}
}
$('refine').onclick=refine;
// The approaches the structured optimization can report, in plain words. A route is an edge's path; these are the
// approaches of the whole drawing.
const approachLabels={components:'Components optimized separately and placed side by side',container:'Composed from containers with fixed frames',hierarchy:'Composed hierarchically, from the inside out',joint:'Joint layout'};
// The structured optimization follows the refinement's rules: it starts from the view's revision and layout version,
// and its result is applied only if the view is still that one. A lost composition is shown as the joint layout.
async function structuredRefine(){
 if(!snapshot||pending||curation)return;
 cancelRefinement();
 const controller=new AbortController();structuring=controller;
 const base={revision:snapshot.revision,layoutVersion:snapshot.layoutVersion??0};
 const startedEpoch=epoch;
 $('backend').textContent='Structured optimization running …';
 try{
   const data=await execute('refine',{op:'structuredRefine',snapshot,expectedRevision:base.revision,expectedLayoutVersion:base.layoutVersion},controller.signal);
   if(structuring!==controller)return;
   if(!data.ok)throw Error(data.error||'Structured optimization not available');
   // The view may have changed while the result was computed; the gate decides, not the timing.
   if(!refinementFits({revision:snapshot.revision,layoutVersion:snapshot.layoutVersion??0},data.snapshot,{...base,epoch:startedEpoch},startedEpoch,!!pending)){
     noteDiscarded();$('backend').textContent='Stale result discarded: the view changed in the meantime.';return;
   }
   snapshot=data.snapshot;draw();
   showApproach(data);
   $('backend').textContent=`Approach: ${approachLabels[data.route]??data.route}.`;
 }catch(e){
   if(e.name!=='AbortError'&&structuring===controller)$('backend').textContent=`Structured optimization: ${e.message}. The diagram stays unchanged.`;
 }finally{if(structuring===controller)structuring=null;}
}
// The report: the approach taken, both candidates' keys (smaller is better), why other approaches did not apply, and each
// part's own strategy.
function showApproach(decision){
 const report=$('structure-report');report.hidden=false;
 const lines=[`Chosen: ${approachLabels[decision.route]??decision.route}.`];
 if(decision.decision.composedKey&&decision.decision.jointKey){
   lines.push(`Comparison (smaller is better): composed [${decision.decision.composedKey.join(', ')}] · joint [${decision.decision.jointKey.join(', ')}].`);
 }else{lines.push('Comparison: no composition evaluated.');}
 for(const {path,reason} of decision.notApplied)lines.push(`${path}: ${reason}.`);
 $('structure-summary').textContent=lines.join(' ');
 const parts=$('structure-parts');parts.replaceChildren();
 for(const {part,strategy} of decision.decision.details){const li=document.createElement('li');li.textContent=`${part}: ${strategy}`;parts.append(li);}
}
$('structured').onclick=structuredRefine;
$('auto').onchange=()=>{if($('auto').checked)scheduleRefinement();else{cancelRefinement();$('backend').textContent='Auto-optimize paused.';}};

$('toggle').onclick=()=>toggleContainer(selected);
$('tree-filter').oninput=()=>{treeFilter=$('tree-filter').value.trim().toLowerCase();draw();};

// Node-size feedback: the rendered label widths are reported back to the engine. The engine
// recomputes with those sizes; a change is sent only when a width differs by more than 4 px, so the
// loop settles instead of oscillating.
let measuring=false;
function measureNodes(){
 if(measuring||!snapshot||pending||curation||!$('measure').checked)return;
 const sizes={};
 for(const g of $('graph').querySelectorAll('g.node')){
  const id=g.querySelector('title')?.textContent, text=g.querySelector('text');
  const box=id&&snapshot.boxes[id];
  if(!box||!text||userSizes.has(id))continue;
  const attribute=snapshot.graph.nodes.find(n=>n.id===id)?.kind==='attribute';
  const fit=labelSize(shapeOf(id),text.getComputedTextLength());
  const width=attribute?Math.max(96,Math.ceil(text.getComputedTextLength())+24):fit.width, height=attribute?26:fit.height;
  if(Math.abs(width-box.width)>4||Math.abs(height-box.height)>4)sizes[id]={width,height};
 }
 if(!Object.keys(sizes).length)return;
 measuring=true;
 edit({op:'measure',snapshot,measurements:geometry.reports(sizes,shapeOf)},{busy:'Applying node sizes …',fit:true}).finally(()=>{measuring=false;});
}
$('measure').onchange=()=>{if($('measure').checked)measureNodes();};
// Port placement: symmetric (evenly spaced) or optimized (towards the peers, with the minimum spacing).
$('port-placement').onchange=()=>setOption({portPlacement:$('port-placement').value},'port distribution');
$('bundle').onchange=()=>setOption({bundling:$('bundle').value},'Bundling');

// The entity-relationship projection of a graph (ADR 0011): an entity becomes a leaf node, its rows are its title and its
// attributes, measured in the text's font. Other graphs are returned unchanged, with no rows.
function erProjection(graph){
 const entities=graph.nodes.filter(n=>n.kind==='entity');
 if(!entities.length)return {graph,options:null};
 const attributes=new Map();
 for(const n of graph.nodes){
  if(n.kind!=='attribute'||!n.parentId)continue;
  attributes.set(n.parentId,[...(attributes.get(n.parentId)??[]),n]);
 }
 const rows=Object.fromEntries(entities.map(e=>[e.id,[
  {id:'title',label:e.label,width:Math.ceil(textWidth(e.label,'600 14px system-ui'))+12,height:22},
  ...(attributes.get(e.id)??[]).map(a=>({id:a.id,label:a.label,width:Math.ceil(textWidth(a.label,'13px system-ui'))+12,height:20})),
 ]]));
 // A relationship leaves its source at the entity's key row (its first attribute, else the title) and enters its target at
 // the title, so the edges meet the rows they belong to (ADR 0011). Each port is on the box's side at its row.
 const ports=[],edgePorts=[];
 for(const e of graph.edges){
  if(!e.target)continue;
  const key=(attributes.get(e.source)??[])[0]?.id??'title';
  const source={id:`port:${e.id}:source`,nodeId:e.source,side:'right',offset:0.5,row:key};
  const target={id:`port:${e.id}:target`,nodeId:e.target,side:'left',offset:0.5,row:'title'};
  ports.push(source,target);
  edgePorts.push({edgeId:e.id,sourcePort:source.id,targetPort:target.id});
 }
 const edgeLabels=Object.fromEntries(graph.edges.filter(e=>e.kind).map(e=>[e.id,labelSpec(e.kind)]));
 const projected={...graph,nodes:graph.nodes.filter(n=>n.kind!=='attribute'),ports,edgePorts};
 // The rows, labels and loop clearance are given with the import, so the first layout already checks them.
 // Edges keep a clearance from foreign nodes: a route that hugs a node border is not an accepted layout (a requirement).
 return {graph:projected,options:{nodeRows:rows,edgeLabels,selfLoopClearance:40,requiredClearance:12}};
}
// The width of a text in a font, measured on a canvas; a rough estimate where there is no canvas.
function textWidth(text,font){
 const ctx=document.createElement('canvas').getContext?.('2d');
 if(ctx)ctx.font=font;
 return ctx?ctx.measureText(text).width:text.length*8;
}
// Measured label sizes enter the portable layout options, not an SVG-only overlay.
function labelSpec(text){
 const canvas=document.createElement('canvas'),ctx=canvas.getContext?.('2d');
 if(ctx)ctx.font='12px system-ui';
 return {text,width:Math.ceil(ctx?ctx.measureText(text).width:text.length*8)+8,height:22};
}
$('edge-labels').onchange=()=>{
 const edgeLabels={};if($('edge-labels').checked){for(const e of snapshot.graph.edges)edgeLabels[e.id]=labelSpec(e.kind||e.id);
  // A bus's name (ADR 0005): placed by the engine at the bus's trunk.
  for(const b of snapshot.graph.buses??[])if(b.label)edgeLabels[`bus:${b.id}`]=labelSpec(b.label);}
 setOption({edgeLabels},'Edge labels');
};
$('port-labels').onchange=()=>{
 const portLabels={};if($('port-labels').checked)for(const p of snapshot.resolvedPorts??[])portLabels[p.id]=labelSpec(p.explicit?p.id:(p.endpoint==='source'?'out':'in'));
 setOption({portLabels},'Port labels');
};

$('show-ports').onchange=()=>setOption({appearance:{...effectiveOptions().appearance,showPorts:$('show-ports').checked}},'port markers');

bindNavigation($('canvas'),()=>scale,value=>{autoFit=false;scale=value;if(snapshot)draw();});
$('zoom-out').onclick=()=>{autoFit=false;scale=Math.max(.02,scale/1.5);if(snapshot)draw();};
if(typeof ResizeObserver!=='undefined'){
 let frame=0;
 new ResizeObserver(()=>{if(frame)return;frame=requestAnimationFrame(()=>{frame=0;if(snapshot&&autoFit&&!drag)draw(true);});}).observe($('canvas'));
}
// Either side panel can be hidden to give the canvas room; the choice is kept in this browser.
for(const [button,name] of [['toggle-tree-panel','hide-tree'],['toggle-side-panel','hide-side']]){
 const apply=hidden=>{document.body?.classList.toggle(name,hidden);$(button).setAttribute('aria-pressed',String(!hidden));};
 try{apply(localStorage.getItem(`arqlet.${name}`)==='1');}catch{apply(false);}
 $(button).onclick=()=>{const hidden=!document.body.classList.contains(name);apply(hidden);try{localStorage.setItem(`arqlet.${name}`,hidden?'1':'0');}catch{}};
}
// View shortcuts: F fits, + and - zoom. Ignored while typing in a field or with a modifier key.
document.addEventListener?.('keydown',e=>{
 if(!snapshot||e.metaKey||e.ctrlKey||e.altKey||/^(INPUT|SELECT|TEXTAREA)$/.test(e.target?.tagName??'')||e.target?.isContentEditable)return;
 const action={f:'fit',F:'fit','+':'zoom','=':'zoom','-':'zoom-out'}[e.key];
 if(!action)return;
 e.preventDefault();$(action).onclick();
});
$('apply-options').onclick=()=>{try{const options=JSON.parse($('options-json').value);if(!options||Array.isArray(options)||typeof options!=='object')throw Error('A JSON object is required');edit({op:'configure',snapshot,options},{busy:'Applying options …',fit:true});}catch(e){status(e.message);}};

// Path cut-out (provisional): `trace` is read only and runs on the local engine like the scene; `reveal` is a view
// change and goes through the mode's lane like a toggle. The trace follows every new snapshot.
let tracePath=null,traceResult=null,traceSnapshot=null,traceListOf=null;
function traceView(){
 if(snapshot&&snapshot!==traceListOf){
  // Every node of the source graph can be an end, also one inside a collapsed group.
  traceListOf=snapshot;filterView();const list=$('trace-nodes');
  if(list)list.replaceChildren(...(snapshot.sourceGraph??snapshot.graph).nodes.map(n=>{const o=document.createElement('option');o.value=n.id;o.label=n.label;return o;}));
 }
 if(tracePath&&snapshot&&sceneRun&&traceSnapshot!==snapshot){
  traceSnapshot=snapshot;
  // With “Only shown edge kinds”, the paths follow the edge kinds the filter shows (provisional `edgeKinds`).
  const hiddenKinds=snapshot.options?.hidden?.edgeKinds??[];
  const along=tracePath.shownKinds&&hiddenKinds.length?{edgeKinds:[...new Set((snapshot.sourceGraph??snapshot.graph).edges.map(e=>e.kind))].filter(k=>!hiddenKinds.includes(k))}:{};
  // A neighbourhood (provisional `neighbourhood`) is shown like a path.
  const {shownKinds,op='trace',...path}=tracePath;
  const r=along.edgeKinds&&!along.edgeKinds.length?{ok:false,error:'the filter hides every edge kind'}:sceneRun({op,snapshot,...path,...along});
  if(op==='neighbourhood'&&r.ok)r.found=true;
  traceResult=r.ok?r:null;
  $('trace-info').textContent=!r.ok?`Not traced: ${r.error}`:op==='neighbourhood'?
   `${r.nodes.length} nodes and ${r.edges.length} edges within ${path.hops} hop${path.hops===1?'':'s'} of ${path.nodeId} (${path.direction==='both'?'both ways':path.direction==='out'?'outgoing':'incoming'}); ${r.visible.nodes.length} shown boxes hold them.`:
   !r.found?'No path between these nodes, in either direction.':
   `${r.length} step${r.length===1?'':'s'}${r.reversed?' (from “To” to “From”: no path the other way)':''} · ${r.nodes.length} nodes and ${r.edges.length} edges on the ${tracePath.mode==='all'?'paths':'shortest paths'}; ${r.visible.nodes.length} shown boxes carry them.`;
  $('trace-reveal').disabled=!traceResult?.found;$('trace-clear').disabled=false;
 }
 if(!traceResult?.found)return null;
 return {nodes:traceResult.visible.nodes,routes:traceResult.visible.routes,distance:traceResult.distance,fade:$('trace-fade').checked};
}
$('trace').onclick=()=>{
 const from=$('trace-from').value.trim(),to=$('trace-to').value.trim();
 if(!from||!to){$('trace-info').textContent='Choose a node for From and for To.';return;}
 tracePath={from,to,mode:$('trace-mode').value,shownKinds:$('trace-shown-kinds').checked};traceSnapshot=null;if(snapshot)draw();
};
$('near').onclick=()=>{
 const nodeId=$('trace-from').value.trim(),hops=Math.max(0,Math.min(20,Math.round(Number($('near-hops').value)||0)));
 if(!nodeId){$('trace-info').textContent='Choose a node for From.';return;}
 tracePath={op:'neighbourhood',nodeId,hops,direction:$('near-direction').value,shownKinds:$('trace-shown-kinds').checked};traceSnapshot=null;if(snapshot)draw();
};
$('trace-clear').onclick=()=>{tracePath=null;traceResult=null;traceSnapshot=null;$('trace-info').textContent='';$('trace-reveal').disabled=true;$('trace-clear').disabled=true;if(snapshot)draw();};
// Filter (provisional): a checkbox per node kind and edge kind of the source graph, ticked while shown. A change sends
// `filter` with what is unticked; the engine keeps the geometry, so what stays keeps its place.
function filterView(){
 const source=snapshot.sourceGraph??snapshot.graph,hidden=snapshot.options?.hidden??{};
 const boxes=(id,kinds,off,label)=>$(id).replaceChildren(...[...new Set(kinds)].sort().map(kind=>{
  const l=document.createElement('label'),c=document.createElement('input');
  c.type='checkbox';c.checked=!(off??[]).includes(kind);c.dataset.kind=kind;c.onchange=applyFilter;
  l.append(c,` ${label}${kind}`);return l;
 }));
 boxes('filter-node-kinds',source.nodes.map(n=>n.kind),hidden.nodeKinds,'');
 boxes('filter-edge-kinds',source.edges.map(e=>e.kind),hidden.edgeKinds,'→ ');
 $('filter-show-all').disabled=!(hidden.nodeKinds?.length||hidden.edgeKinds?.length||hidden.nodes?.length);
}
function applyFilter(){
 if(!snapshot||pending)return;
 const off=id=>[...$(id).querySelectorAll('input')].filter(c=>!c.checked).map(c=>c.dataset.kind);
 const hide={...(snapshot.options?.hidden?.nodes?.length?{nodes:snapshot.options.hidden.nodes}:{})};
 const nodeKinds=off('filter-node-kinds'),edgeKinds=off('filter-edge-kinds');
 if(nodeKinds.length)hide.nodeKinds=nodeKinds;
 if(edgeKinds.length)hide.edgeKinds=edgeKinds;
 edit({op:'filter',snapshot,hide},{busy:'Filtering …',remember:'Filter'});
}
$('filter-show-all').onclick=()=>{if(snapshot&&!pending)edit({op:'filter',snapshot,hide:{}},{busy:'Showing everything …',remember:'Show all'});};
$('trace-fade').onchange=()=>{if(snapshot)draw();};
$('trace-mode').onchange=()=>{if(tracePath&&!tracePath.op){tracePath={...tracePath,mode:$('trace-mode').value};traceSnapshot=null;if(snapshot)draw();}};
$('trace-from-selected').onclick=()=>{if(selected)$('trace-from').value=selected;};
$('trace-to-selected').onclick=()=>{if(selected)$('trace-to').value=selected;};
$('trace-reveal').onclick=()=>{
 if(!traceResult?.found||!snapshot||pending)return;
 edit({op:'reveal',snapshot,nodeIds:traceResult.nodes,partial:$('reveal-partial').checked},{busy:'Opening the groups on the path …',remember:'Reveal path'});
};
// Any node by its id, also one inside collapsed groups: its groups open (in part, by default) and it is selected.
$('show').onclick=async()=>{
 const id=$('show-node').value.trim();
 if(!id||!snapshot||pending)return;
 const r=await edit({op:'reveal',snapshot,nodeIds:[id],partial:$('reveal-partial').checked},{busy:'Opening the groups that hold it …',remember:'Show node'});
 if(r?.ok&&snapshot.graph.nodes.some(n=>n.id===id)){selected=id;draw();}
 else if(r&&!r.ok)$('trace-info').textContent=`Not shown: ${r.error}`;
};

