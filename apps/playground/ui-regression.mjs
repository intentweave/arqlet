import {bindNavigation} from '@arqlet/viewer/navigation.js';
import {evaluateSnapshot} from './comparison.js';
import {withAggregatedWeights} from './aggregation.js';
import {presetState,applyPreset} from '@arqlet/engine-client/presets.js';
import {routeDetails,routeConflictsOf} from '@arqlet/viewer/routes.js';
import {renderSvg} from '@arqlet/viewer/svg.js';
import {sceneOf} from '@arqlet/engine-client/engine.js';
import {Selection,describeSelection} from './generated/editor/index.js';
// App-level regression with a minimal DOM and actual WASM. Not a visual browser test.
import {readFileSync} from 'node:fs';
import {runInNewContext} from 'node:vm';
import assert from 'node:assert/strict';
import {createEngine} from '@arqlet/engine-client/wasm-client.js';
import {acceptPublication,refinementFits} from './revisions.js';
import {Session} from './session.js';
import {History} from './history.js';
import {GeometryReports} from './geometry.js';
import {containmentRows} from '../../packages/containment-tree/model.js';
import {settingsOf,rootSettingsOf} from '../../packages/containment-tree/settings.js';
import {renderContainmentTree,settingsElementOf,layoutFieldsElementOf} from '../../packages/containment-tree/render.js';
const initial=JSON.parse(readFileSync(new URL('./generated/snapshot.json',import.meta.url)));
const graph=JSON.parse(readFileSync(new URL('./generated/graph.json',import.meta.url)));
const exampleIndex=JSON.parse(readFileSync(new URL('./generated/examples/index.json',import.meta.url)));
const presetIndex=JSON.parse(readFileSync(new URL('./generated/presets/index.json',import.meta.url)));
const engine=await createEngine(readFileSync(new URL('./generated/arqlet.wasm',import.meta.url)));
class Element{
 constructor(){this.children=[];this.value='';this.checked=false;this.disabled=false;this.clientWidth=1200;this.clientHeight=800;this.textContent='';this.dataset={};this.style={};this.tabIndex=0;}
 contains(){return false;}
 closest(){return null;}
 focus(){}
 scrollIntoView(){}
 addEventListener(){}
 append(...items){this.children.push(...items);}
 replaceChildren(...items){this.children=items;}
 setAttribute(k,v){this[k]=v;}
 click(){if(!this.disabled)this.onclick?.();}
}
const ids=new Map();const get=id=>{if(!ids.has(id))ids.set(id,new Element());return ids.get(id);};
// The opt-in automatic single pass in a deployment mode: it reaches the engine through that mode's lane, a pin (outside single
// pass) falls back with a report instead of failing, and the view is left as it was.
const autoSinglePass=async mode=>{
 const leaf=latest.graph.nodes.find(n=>!latest.graph.nodes.some(c=>c.parentId===n.id)).id;
 get('options-json').value='{"singlePassAuto": true}';get('apply-options').click();await settle();
 assert.notEqual(latest.options?.singlePassAuto,false,`${mode}: the option reaches the view (on by default, omitted)`);
 assert.notEqual(latest.options?.singlePass,false,`${mode}: single pass runs where it works`);
 get('options-json').value=JSON.stringify({pins:{[leaf]:{x:40,y:60}}});get('apply-options').click();await settle();
 assert.equal(latest.options?.singlePass,false,`${mode}: a pin leaves single pass`);
 assert.ok(latest.layoutDiagnostics.some(d=>d.startsWith('Single pass was not used:')),`${mode}: the fallback is reported`);
 assert.deepEqual(latest.options.pins[leaf],{x:40,y:60},`${mode}: the pin holds`);
 get('options-json').value='{"pins": {}, "singlePassAuto": false}';get('apply-options').click();await settle();
 assert.equal(latest.options?.singlePassAuto,false,`${mode}: the option can be turned off`);
 console.log(`Automatic single pass passed in ${mode} mode.`);
};
get('auto').checked=true;get('name').value='Test node';get('mode').value='split';get('example').value='default';
const timers=new Map();let tid=0,backendCalls=0,latest=null;const backendOps=[];
let holdReplies=false;const heldReplies=[];
class Worker{
 postMessage({requestId,request}){const response=engine(request);if(response.ok&&response.snapshot)latest=response.snapshot;const deliver=()=>this.onmessage({data:{requestId,...response}});if(holdReplies)heldReplies.push(deliver);else queueMicrotask(deliver);}
}
const context={GeometryReports,bindNavigation,presetState,applyPreset,routeDetails,routeConflictsOf,evaluateSnapshot,withAggregatedWeights,Selection,describeSelection,containmentRows,settingsOf,rootSettingsOf,renderContainmentTree,settingsElementOf,layoutFieldsElementOf,loadEngine:async()=>engine,sceneOf,renderSvg,document:{getElementById:get,createElement:()=>new Element(),createElementNS:()=>new Element(),activeElement:null,createTextNode:text=>{const e=new Element();e.textContent=text;return e;}},Worker,createEngineWorker:()=>new Worker(),refinementFits,acceptPublication,Session,History,console,AbortController,Blob,URL,queueMicrotask,performance,
 setTimeout:fn=>{timers.set(++tid,fn);return tid;},clearTimeout:id=>timers.delete(id),
 fetch:async(url,options)=>{
   if(url.includes('snapshot.json')){latest=structuredClone(initial);return {ok:true,json:async()=>structuredClone(initial)};}
   if(url.includes('graph.json'))return {ok:true,json:async()=>structuredClone(graph)};
   if(url==='/api/op'){const request=JSON.parse(options.body);backendOps.push(request.op);if(request.op==='refine')backendCalls++;const result=engine(request);if(result.ok)latest=result.snapshot;return {ok:result.ok,json:async()=>result};}
   if(url==='./generated/examples/index.json')return {ok:true,json:async()=>structuredClone(exampleIndex)};
   if(url==='./generated/arqlet.wasm')return {ok:true,arrayBuffer:async()=>readFileSync(new URL('./generated/arqlet.wasm',import.meta.url))};
   if(url==='./generated/presets/index.json')return {ok:true,json:async()=>structuredClone(presetIndex)};
   if(url.startsWith('./generated/examples/'))return {ok:true,json:async()=>JSON.parse(readFileSync(new URL(url.slice(2),import.meta.url),'utf8'))};
   throw Error(`Unexpected fetch ${url}`);
 }};
const code=readFileSync(new URL('./app.js',import.meta.url),'utf8').replace(/^import .*\n/gm,'');
runInNewContext(code,context);
const settle=async()=>{for(let i=0;i<300;i++)await Promise.resolve();};
const flush=async()=>{for(const [id,fn] of [...timers]){timers.delete(id);await fn();}await settle();};
// Like flush, without running the scheduled refinements: only for checks that read options, not a refined layout.
const quiet=async()=>{timers.clear();await settle();};
await settle();assert.equal(timers.size,1,'initial checked auto must schedule refinement');
for(const [id,fn] of [...timers]){timers.delete(id);await fn();}await settle();assert.equal(backendCalls,1);
const container=initial.graph.nodes.find(n=>initial.graph.nodes.some(c=>c.parentId===n.id));
assert.ok(container,'fixture must contain container');
let last=latest;
const toggleOp=(id)=>{const reply=engine({op:'toggle',snapshot:last,nodeId:id,expectedRevision:last.revision,expectedLayoutVersion:last.layoutVersion??0});assert.equal(reply.ok,true,JSON.stringify(reply));last=reply.snapshot;return reply;};
toggleOp(container.id);assert.ok(last.collapsedIds.includes(container.id));assert.ok(last.graph.nodes.length<initial.graph.nodes.length);
toggleOp(container.id);assert.equal(last.graph.nodes.length,initial.graph.nodes.length);
// The layout settings the containment tree sends: the engine applies each one and answers with the snapshot.
const setting=(op,field,nodeId,value)=>{const reply=engine({op,snapshot:last,nodeId,[field]:value,expectedRevision:last.revision,expectedLayoutVersion:last.layoutVersion??0});if(reply.ok)last=reply.snapshot;return reply;};
assert.ok(setting('direction','direction',null,'tb').ok);
assert.equal(last.sourceGraph.defaultDirection,'tb');assert.equal(last.geometryIssues.length,0);
assert.ok(setting('direction','direction',container.id,'lr').ok);assert.equal(last.sourceGraph.containerDirections[container.id],'lr');
assert.ok(setting('direction','direction',container.id,null).ok);assert.equal(last.sourceGraph.containerDirections[container.id],undefined);
// Allowed set: the list reaches the contract and is honoured by the layout.
assert.ok(setting('direction','direction',container.id,['tb','bt']).ok);
assert.deepEqual(last.sourceGraph.containerDirections[container.id],['tb','bt'],'the allowed set reaches the contract');
assert.ok(setting('direction','direction',container.id,'auto').ok);assert.equal(last.sourceGraph.containerDirections[container.id],'auto','free reaches the contract');
// Tree over a container with parallel links is refused with its reason; the previous constraint stays in place.
const refused=setting('strategy','strategy',container.id,'tree');
assert.equal(refused.ok,false);assert.match(refused.error,/refuses/,'a tree over a container with parallel links is refused with its reason');
assert.equal(last.sourceGraph.containerStrategies?.[container.id],undefined,'the refused strategy is not stored');
assert.equal(last.sourceGraph.containerDirections[container.id],'auto','the direction stays unchanged');
assert.ok(setting('direction','direction',null,'auto').ok);assert.equal(last.sourceGraph.defaultDirection,'auto');
// Routing: the view takes a declared style, a container takes its own, and clearing a container entry inherits again.
assert.ok(setting('routing','routing',null,'orthogonal').ok);assert.equal(last.sourceGraph.defaultRouting,'orthogonal','an explicit root orthogonal is kept');
assert.ok(setting('routing','routing',container.id,'orthogonal').ok);assert.equal(last.sourceGraph.containerRouting?.[container.id],'orthogonal','the container takes its own style');
assert.ok(setting('routing','routing',container.id,null).ok);assert.equal(last.sourceGraph.containerRouting?.[container.id],undefined,'clearing a container entry inherits again');

// Structured optimization: a click sends the view through /api/op; an applied result advances the layout version by one
// and keeps the graph revision; the report names the approach.
const layoutBefore=latest.layoutVersion??0,revisionBefore=latest.revision,opsBefore=backendOps.length;
get('structured').click();await settle();
assert.ok(backendOps.slice(opsBefore).includes('structuredRefine'),'structured optimization goes through /api/op in backend mode');
assert.equal(latest.layoutVersion,layoutBefore+1,'an applied result advances the layout version');
assert.equal(latest.revision,revisionBefore,'the graph revision stays');
assert.match(get('structure-summary').textContent,/Chosen:/);
await autoSinglePass('split');
console.log('Split mode passed: initial auto, collapse/expand, LR/TB/hybrid and inherited override with real WASM.');

// Browser-only: import, edits and refinement stay in the WASM workers; the backend is never called.
const splitOps=backendOps.length,splitRefines=backendCalls;
get('mode').value='browser';get('mode').onchange();await flush();
assert.equal(latest.revision,0);assert.ok(latest.graph.nodes.length>0);
get('options-json').value='{"routingClearance": 14}';get('apply-options').click();await settle();
assert.equal(latest.options.routingClearance,14,'the browser lane applies the edit');
await flush();assert.equal(backendOps.length,splitOps,'browser mode must not contact the backend');
assert.equal(backendCalls,splitRefines);
// Structured optimization runs in the background WASM worker: the backend is not called, and the result is applied.
const layoutBeforeBrowser=latest.layoutVersion??0,opsBeforeBrowser=backendOps.length;
get('structured').click();await settle();
assert.equal(backendOps.length,opsBeforeBrowser,'browser-only structured optimization never calls the backend');
assert.equal(latest.layoutVersion,layoutBeforeBrowser+1,'the browser result is applied with the layout version advanced');
await autoSinglePass('browser');
console.log('Browser-only mode passed: graph imported in WASM, toggle and background refinement without backend.');

// A change while the structured optimization runs: both replies are held. The edit's reply is released first and its
// view is shown; then the older structured result is released and must not replace the diagram.
holdReplies=true;heldReplies.length=0;
get('structured').click();await settle();
get('options-json').value='{"routingClearance": 12}';get('apply-options').click();await settle();
holdReplies=false;
const structuredReply=heldReplies.shift(),editReply=heldReplies.shift();
editReply();await settle();
const statsAfterEdit=get('stats').textContent;
structuredReply();await settle();
assert.equal(get('stats').textContent,statsAfterEdit,'a structured result based on an older view does not replace the diagram');
assert.doesNotMatch(get('backend').textContent,/^Approach:/,'the stale result is not reported as applied');
console.log('Change during structured optimization passed: the stale result is discarded, the edit stays.');

// Backend-only: every operation, including refinement, is sent to /api/op.
get('mode').value='backend';get('mode').onchange();await flush();
const backendStart=backendOps.length;
get('options-json').value='{"routingClearance": 12}';get('apply-options').click();await settle();
assert.equal(latest.options.routingClearance,12,'the edit reaches the view through the backend lane');
await flush();
assert.deepEqual(backendOps.slice(backendStart,backendStart+2),['configure','refine']);
assert.ok(backendOps.slice(backendStart).includes('refine'),'backend mode must send refinement to /api/op');
await autoSinglePass('backend');
console.log('Backend-only mode passed: edits and refinement run through /api/op.');

// Examples: any demo graph loads through the mode's import lane; the default graph stays native in split mode.
assert.ok(exampleIndex.some(e=>e.name==='state-tcp'),'state-tcp example must be offered');
get('example').value='state-tcp';get('example').onchange();await flush();
assert.equal(latest.graph.nodes.length,11);assert.equal(backendOps.at(-1),'import','backend mode imports examples at /api/op');
get('mode').value='split';get('mode').onchange();await flush();
const beforeSplitExample=backendOps.length;
get('example').value='state-tcp';get('example').onchange();await flush();
assert.equal(latest.graph.nodes.length,11);assert.equal(backendOps.length,beforeSplitExample,'split mode imports examples in the worker');
get('example').value='default';get('example').onchange();await flush();
assert.equal(latest.graph.nodes.length,initial.graph.nodes.length,'default example returns to the native snapshot');
console.log(`Example selection passed: ${exampleIndex.length} demo graphs offered; import via the mode's lane.`);

// Options: preset and bundling go through `configure`; the page reflects bundling from the snapshot.
get('mode').value='split';get('mode').onchange();await flush();
// Presets: applying one names the snapshot with its configId; a custom change clears the name.
get('profile').value='raster';get('profile').onchange();await flush();
assert.deepEqual(latest.options.candidates,[1]);assert.equal(latest.options.configId,'raster@1');
assert.equal(get('profile').value,'raster');assert.equal(get('refine').disabled,false,'raster refines');
get('bundle').value='complete';get('bundle').onchange();await flush();
assert.equal(latest.options.bundling,'complete');assert.equal(latest.options.configId,undefined,'a custom change is not a preset');
assert.equal(get('profile').value,'__custom__',"the selector shows 'Custom'");
get('profile').value='vorschau';get('profile').onchange();await flush();
assert.equal(latest.options.configId,'vorschau@1');assert.equal(get('refine').disabled,true,'vorschau has no refinement');
assert.ok(get('profile-info').textContent.includes('budget'),'the budget is shown as indicative');
get('profile').value='lesbar';get('profile').onchange();await flush();
assert.equal(latest.options.configId,'lesbar@1');assert.equal(latest.graph.defaultDirection,'auto','the preset direction is applied first');
assert.equal(get('refine').disabled,false);
console.log('Presets passed: named configs apply via direction then configure; custom changes show Custom; preview does not refine.');

// Detail settings mirror the profile. These checks read the options, the source and the form, not a refined layout,
// so their scheduled refinements are dropped rather than run: a profile refinement takes about 50 s on the demo view.
// The refinement itself is covered by the sections above and below.
// One detail change makes the profile custom and leaves every other value alone.
assert.equal(get('routing-engine').value,'channel','the lesbar profile shows its routing engine');
assert.equal(get('free-placement').checked,true,'the lesbar profile uses free placement');
assert.equal(Number(get('track-spacing').value),6,'the profile value is shown in the detail field');
get('routing-clearance').value='24';get('routing-clearance').onchange();await quiet();
assert.equal(latest.options.routingClearance,24);
assert.equal(latest.options.configId,undefined,'a detail change makes the profile custom');
assert.equal(get('profile').value,'__custom__');
assert.equal(get('routing-engine').value,'channel','the other details stay as they were');
assert.equal(get('free-placement').checked,true,'the other details stay as they were');
assert.equal(Number(get('track-spacing').value),6,'the other details stay as they were');
// An unchanged value sends nothing, so it does not clear the profile.
get('profile').value='lesbar';get('profile').onchange();await quiet();
const before=latest.revision;
get('routing-clearance').value='0';get('routing-clearance').onchange();await quiet();
assert.equal(latest.options.configId,'lesbar@1','an unchanged value keeps the profile');
assert.equal(latest.revision,before,'an unchanged value creates no revision');
assert.ok(get('layout-stats').children.length>=10,'the layout statistics table is filled from the engine metrics');
// Pins: pinning the selected leaf stores its position and makes the profile custom; unpinning removes it.
const leafId=initial.graph.nodes.find(n=>!initial.graph.nodes.some(c=>c.parentId===n.id)).id;
runInNewContext(`selected=${JSON.stringify(leafId)}`,context);runInNewContext('draw()',context);
get('pin-node').click();await quiet();
assert.ok(latest.options.pins[leafId],'the pin is stored in the snapshot options');
assert.equal(get('profile').value,'__custom__','a pin is a custom change');
get('unpin-node').click();await quiet();
assert.equal(latest.options.pins?.[leafId],undefined,'unpinning removes the pin');
// Canvas controls: a container's pin control fixes its position in its parent's frame, and pressing it again releases
// it; its toggle control collapses it through the engine.
{
  const pinControl=()=>get('graph').children.find(e=>e['data-pin-id']===container.id);
  const press=()=>pinControl().onclick({stopPropagation(){}});
  assert.ok(pinControl(),'a container has a pin control on the canvas');
  press();await quiet();
  assert.deepEqual(latest.options.pins[container.id],{x:latest.boxes[container.id].x,y:latest.boxes[container.id].y},'the container is pinned where it is');
  assert.equal(pinControl().class,'lw-control lw-pin lw-pinned','the control shows the pin');
  press();await quiet();
  assert.equal(latest.options.pins?.[container.id],undefined,'a second press unpins the container');
  const toggle=get('graph').children.find(e=>e['data-toggle-id']===container.id);
  toggle.onclick({stopPropagation(){}});await quiet();
  assert.ok(latest.collapsedIds.includes(container.id),'the toggle control collapses the container');
  const expand=get('graph').children.find(e=>e.children?.some?.(c=>c['data-toggle-id']===container.id))?.children.find(c=>c['data-toggle-id']===container.id);
  assert.ok(expand,'the collapsed container has an expand control');
  expand.onclick({stopPropagation(){}});await quiet();
  assert.ok(!latest.collapsedIds.includes(container.id),'the expand control opens it again');
}
// Whole-view layout form: auto, an allowed set (sent once two are ticked), and clearing the routing declaration.
{
  // The view's settings sit under the View row of the containment tree: the panel after that row holds the layout section.
  const form=()=>{const list=get('containment-tree').children[0];const detail=list.children.find(li=>li.role==='none');return detail.children[0].children.find(c=>c.children?.some?.(x=>x.textContent==='direction '));};
  // Select the View row: its settings are the whole view's (a selected node or container would show its own).
  get('containment-tree').children[0].children.find(li=>li.dataset?.id==='__view__').onclick();
  const field=name=>{const items=form().children;const at=items.findIndex(e=>e.textContent===`${name} `);return {select:items[at].children[0],allowed:items[at+2]};};
  let f=field('direction');f.select.value='auto';f.select.onchange();await quiet();
  assert.equal(latest.sourceGraph.defaultDirection,'auto','auto reaches the view direction');
  f=field('direction');f.select.value='allowed';f.select.onchange();await quiet();
  assert.equal(latest.sourceGraph.defaultDirection,'auto','choosing an allowed set sends nothing until two are ticked');
  const ticks=f.allowed.children.filter(e=>e.className==='tick').map(t=>t.children[0]);
  ticks[0].checked=true;ticks[0].onchange();await quiet();
  assert.equal(latest.sourceGraph.defaultDirection,'auto','one tick is not sent');
  ticks[2].checked=true;ticks[2].onchange();await quiet();
  assert.deepEqual(latest.sourceGraph.defaultDirection,['lr','tb'],'two ticks send the allowed set');
  assert.equal(field('direction').select.value,'allowed','the form shows the accepted allowed set');
  f=field('strategy');f.select.value='auto';f.select.onchange();await quiet();
  assert.equal(latest.sourceGraph.defaultStrategy,'auto','auto reaches the view strategy');
  f=field('routing');f.select.value='auto';f.select.onchange();await quiet();
  assert.equal(latest.sourceGraph.defaultRouting,'auto','auto reaches the view routing');
  f=field('routing');f.select.value='default';f.select.onchange();await quiet();
  assert.equal(latest.sourceGraph.defaultRouting,undefined,'"not declared" clears the view routing');
  f=field('direction');f.select.value='lr';f.select.onchange();await quiet();
  f=field('strategy');f.select.value='none';f.select.onchange();await quiet();
  assert.equal(latest.sourceGraph.defaultDirection,'lr');
}
// Placement flags and optional numbers reach the engine; an empty number clears the option.
{
  get('component-packing').checked=true;get('component-packing').onchange();await quiet();
  assert.equal(latest.options.componentPacking,true,'component packing reaches the engine');
  get('vpsc-overlap-removal').checked=true;get('vpsc-overlap-removal').onchange();await quiet();
  assert.equal(latest.options.vpscOverlapRemoval,true,'constraint-based overlap removal reaches the engine');
  get('vpsc-overlap-removal').checked=false;get('vpsc-overlap-removal').onchange();await quiet();
  get('directed-repair').checked=true;get('directed-repair').onchange();await quiet();
  assert.equal(latest.options.directedRepair,true,'directed organic repair reaches the engine');
  get('directed-repair').checked=false;get('directed-repair').onchange();await quiet();
  get('node-edge-repulsion').checked=true;get('node-edge-repulsion').onchange();await quiet();
  assert.equal(latest.options.nodeEdgeRepulsion,true,'node-edge repulsion reaches the engine');
  get('node-edge-repulsion').checked=false;get('node-edge-repulsion').onchange();await quiet();
  get('sparse-stress').checked=true;get('sparse-stress').onchange();await quiet();
  assert.equal(latest.options.sparseStress,true,'sparse organic stress reaches the engine');
  get('sparse-stress').checked=false;get('sparse-stress').onchange();await quiet();
  get('scalable-stress').checked=true;get('scalable-stress').onchange();await quiet();
  assert.equal(latest.options.scalableStress,true,'scalable organic placement reaches the engine');
  get('scalable-stress').checked=false;get('scalable-stress').onchange();await quiet();
  get('organic-iterations').value='500';get('organic-iterations').onchange();await quiet();
  assert.equal(latest.options.organicIterations,500);
  get('organic-iterations').value='';get('organic-iterations').onchange();await quiet();
  assert.equal(latest.options.organicIterations,undefined,'an empty field clears the option');
  get('component-packing').checked=false;get('component-packing').onchange();await quiet();
}
// Objectives: a tick appends an objective; an arrow moves an active one.
get('obj-endToEnd').checked=true;get('obj-endToEnd').onchange();await quiet();
assert.equal(latest.options.objectives.at(-1),'endToEnd','a ticked objective is appended');
get('up-endToEnd').onclick();await quiet();
assert.equal(latest.options.objectives.at(-2),'endToEnd','the arrow moves it one place up');
console.log('Detail settings passed: they mirror the profile; one change switches to custom and keeps the rest.');

// A reset may reuse numeric revisions. Old edits must not replace the new view
// or release a newer edit's pending lock.
get('auto').checked=false;get('mode').value='split';await get('reset').onclick();await settle();
holdReplies=true;
runInNewContext("selected=snapshot.graph.nodes.find(n=>!snapshot.graph.nodes.some(c=>c.parentId===n.id)).id",context);
get('add').onclick();await settle();
await get('reset').onclick();await settle();
const resetRevision=runInNewContext('snapshot.revision',context);
runInNewContext("selected=snapshot.graph.nodes.find(n=>!snapshot.graph.nodes.some(c=>c.parentId===n.id)).id",context);
get('add').onclick();await settle();
assert.equal(heldReplies.length,2);
heldReplies.shift()();await settle();
assert.equal(runInNewContext('snapshot.revision',context),resetRevision);
assert.ok(runInNewContext('pending',context),'old response must not release current edit');
heldReplies.shift()();await settle();holdReplies=false;
assert.equal(runInNewContext('snapshot.revision',context),resetRevision+1);
console.log('Reset race passed: stale edits discarded and newer pending edit retained.');

get('bundle').value='complete';get('bundle').onchange();await settle();
// Visible routes only: the transparent hit lines carry the class route-hit.
const rendered=get('graph').children.filter(e=>e.points!==undefined&&e.class!=='route-hit').length;
assert.equal(rendered,runInNewContext('snapshot.routes.filter(r=>!r.bundle_of).length',context));
get('edge-labels').checked=true;get('edge-labels').onchange();await settle();
assert.ok(runInNewContext('Object.keys(snapshot.options.edgeLabels).length>0',context));
console.log('Renderer bundle count and label configuration passed.');

const beforePorts=runInNewContext('JSON.stringify(snapshot.routes)',context);
get('show-ports').checked=false;get('show-ports').onchange();await settle();
assert.equal(runInNewContext('snapshot.options.appearance.showPorts',context),false);
assert.equal(runInNewContext('JSON.stringify(snapshot.routes)',context),beforePorts);
assert.equal(get('graph').children.filter(e=>e.cx!==undefined).length,0);
console.log('Port visibility changes appearance without moving routes.');

// Route selection: a click on a route's hit line names it in the details line; a second click clears it.
{
  const hit=get('graph').children.find(e=>e.class==='route-hit');
  assert.ok(hit,'routes have hit lines');
  hit.onclick();await settle();
  assert.match(get('route-info').textContent,/→/,'the details line names source and target');
  assert.ok(get('graph').children.some(e=>e.points!==undefined&&e.class!=='route-hit'&&e['stroke-opacity']===1),'the selected route is drawn fully opaque');
  hit.onclick();await settle();
  assert.match(get('route-info').textContent,/to highlight it/,'a second click clears the selection');
  console.log('Route selection passed: click names the route, a second click clears it.');
}

// A conflict partner is a button in the details line; clicking a route partner selects that route.
{
  let clicked=null;
  for(const hit of get('graph').children.filter(e=>e.class==='route-hit')){
    hit.onclick();await settle();
    // A route partner (not a node partner): its id is a route id of the snapshot.
    const routeIds=new Set(runInNewContext('snapshot.routes.map(r=>r.id)',context));
    const partner=get('route-info').children.find(e=>e.onclick&&routeIds.has(e.textContent.replace(/,\s*$/,'')));
    if(partner){partner.onclick();await settle();clicked=partner.textContent;break;}
    hit.onclick();await settle();
  }
  if(clicked!==null){
    const partnerId=clicked.replace(/,\s*$/,'');
    assert.ok(get('route-info').textContent.startsWith('Edge '+partnerId),'the partner route is now selected');
    console.log('Conflict partner passed: clicking a partner selects that route.');
  }else console.log('Conflict partner skipped: the default view has no route with a conflict partner.');
}

// Port placement is an option: optimized moves ports, routes follow, and the selector reflects the snapshot.
{
  get('port-placement').value='optimized';get('port-placement').onchange();await settle();
  assert.equal(runInNewContext('snapshot.options.portPlacement',context),'optimized');
  assert.equal(get('port-placement').value,'optimized');
  console.log('Port placement passed: the option reaches the engine and the selector follows the snapshot.');
}
