import React,{useEffect,useMemo,useRef,useState} from 'react';
import {createRoot} from 'react-dom/client';
import {createPortal} from 'react-dom';
import {ReactFlow,Handle,Position,BaseEdge,Controls,ViewportPortal,applyNodeChanges,useReactFlow} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import './style.css';
import {routePath} from '@arqlet/viewer/scene.js';
import {loadEngine,sceneOf} from '@arqlet/engine-client/engine.js';
import {toReactFlow} from '@arqlet/viewer/react-flow.js';
import {applyPreset} from '@arqlet/engine-client/presets.js';
import {openSceneView,reactFlowViewport} from '@arqlet/viewer/scene-view.js';
import {bindSceneNavigation} from '@arqlet/viewer/scene-renderer.js';

const sides={left:Position.Left,right:Position.Right,top:Position.Top,bottom:Position.Bottom};
function DiagramNode({data,selected}){
 return <div className={`lw-node ${data.container?'container':''} ${selected?'selected':''} ${data.invalid?'invalid':''}`}>
  <div className="title">{data.container?'▾ ':data.collapsed?'▸ ':''}{data.label}</div>
  {data.handles.map(h=><Handle key={h.id} id={h.id} type={h.type} position={sides[h.side]} isConnectable={false} style={{left:h.x,top:h.y,right:'auto',bottom:'auto',transform:'translate(-50%,-50%)',opacity:data.showPorts?1:0,background:h.type==='source'?'#7ee1bc':'#9dbaff'}}/>)}
 </div>;
}
function DiagramEdge({id,data}){
 return <BaseEdge id={id} path={routePath(data.points)} markerEnd="url(#lw-rf-arrow)" style={{stroke:data.invalid?'#f08f80':'#73bba7',strokeWidth:Math.min(4,1.6+.4*(data.weight-1))}}/>;
}
// What is neither node nor edge (toReactFlow's overlays, with `overlayNode`): membership hulls behind everything, bus bars
// with their taps, junction dots and hub-reference stubs, in renderSvg's colours, inside one inert node over the scene.
function OverlayNode({data}){
 const o=data.origin,pt=p=>`${p.x-o.x},${p.y-o.y}`,dims=[...new Set(data.hulls.map(h=>h.dimension))];
 return <svg className="lw-overlay" width="100%" height="100%" style={{overflow:'visible',pointerEvents:'none'}}>
  {data.hulls.filter(h=>h.points?.length>=3).map(h=>{const hue=(dims.indexOf(h.dimension)*67+191)%360;return <polygon key={`${h.dimension}:${h.group}`} points={h.points.map(pt).join(' ')} fill={`hsl(${hue} 60% 55% / .12)`} stroke={`hsl(${hue} 60% 65% / .55)`} strokeWidth={1.4} strokeDasharray="6 4"/>;})}
  {data.busLines.map(b=><g key={b.id}>{b.taps.map(t=><polyline key={t.edge} points={t.points.map(pt).join(' ')} fill="none" stroke="#73bba7" strokeWidth={1.6} strokeOpacity={.75} markerEnd="url(#lw-rf-arrow)"/>)}
   <rect x={b.bounds.x-o.x} y={b.bounds.y-o.y} width={b.bounds.width} height={b.bounds.height} rx={Math.min(b.bounds.width,b.bounds.height)/2} fill="#6f93b8" stroke="#4c6682"/></g>)}
  {data.hubRefs.map(r=><g key={r.edge}><polyline points={r.points.map(pt).join(' ')} fill="none" stroke="#73bba7" strokeWidth={1.6} strokeOpacity={.85} markerEnd="url(#lw-rf-arrow)"/>
   <rect x={r.label.x-o.x} y={r.label.y-o.y} width={r.label.width} height={r.label.height} rx={3} fill="#142133" stroke="#3b5068" strokeWidth={.8}/>
   <text x={r.label.x-o.x+4} y={r.label.y-o.y+r.label.height/2} dominantBaseline="middle" fill="#9fc3e6" fontSize={10.5}>{(r.outgoing?'→ ':'← ')+r.text.slice(0,32)}</text></g>)}
  {data.junctions.map((j,i)=><circle key={i} cx={j.x-o.x} cy={j.y-o.y} r={3.5} fill="#73bba7" stroke="#142133"/>)}
 </svg>;
}
const nodeTypes={arqlet:DiagramNode,arqletOverlay:OverlayNode},edgeTypes={arqlet:DiagramEdge};
function FitScene({bounds}){const {fitBounds}=useReactFlow();useEffect(()=>{fitBounds(bounds,{padding:.1});},[bounds,fitBounds]);return null;}

const Arrow=()=><svg style={{position:'absolute',width:0,height:0}}><defs><marker id="lw-rf-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#73bba7"/></marker></defs></svg>;

// Large views (ADR 0025, V4): the scene view draws from afar on the GPU (WebGPU, else WebGL 2), and React Flow takes the
// near layer's place: it gets the excerpt in view (plus a margin) and a viewport set from the view, so both layers show
// the same geometry and the switch moves nothing. The scene view navigates (wheel, drag); React Flow's own panning and
// zooming are off. Views from FAR_AUTO elements open this way.
const FAR_AUTO=1500;
function FarSceneView({scene,selected,onSelect}){
 const host=useRef(null),view=useRef(null),select=useRef(onSelect);select.current=onSelect;
 const [layer]=useState(()=>{const d=document.createElement('div');d.style.width='100%';d.style.height='100%';return d;});
 const [near,setNear]=useState(null),[viewport,setViewport]=useState({x:0,y:0,zoom:1}),[info,setInfo]=useState('');
 useEffect(()=>{
  let alive=true,unbind=null;
  const describe=v=>setInfo(`${v.backend==='webgpu'?'WebGPU':v.backend==='webgl2'?'WebGL 2':'no GPU'} · ${v.layer==='near'?'near (React Flow)':'far'}`);
  const nearLayer={element:layer,draw:(excerpt,f)=>{setNear(excerpt);setViewport(reactFlowViewport(f.view,f.width,f.height));},move:f=>setViewport(reactFlowViewport(f.view,f.width,f.height)),clear:()=>setNear(null)};
  openSceneView(host.current,scene,{near:nearLayer,state:{selected},onLayer:()=>view.current&&describe(view.current)}).then(v=>{
   if(!alive){v.destroy();return;}
   view.current=v;describe(v);
   unbind=bindSceneNavigation(host.current,v,{onClick:(x,y)=>{if(v.layer!=='far')return;const hit=v.pick(x,y);if(hit?.kind==='node')select.current(hit.id);}});
  });
  return()=>{alive=false;unbind?.();view.current?.destroy();view.current=null;};
 },[]);
 useEffect(()=>{view.current?.setScene(scene);},[scene]);
 useEffect(()=>{view.current?.setState({selected});},[selected]);
 const mapped=useMemo(()=>near?toReactFlow(near,{overlayNode:true}):null,[near]);
 const nodes=useMemo(()=>mapped?mapped.nodes.map(n=>({...n,selected:n.id===selected})):[],[mapped,selected]);
 return <div className="far-host" ref={host}><span className="far-info">{info}</span>{createPortal(mapped&&<ReactFlow nodes={nodes} edges={mapped.edges} nodeTypes={nodeTypes} edgeTypes={edgeTypes} viewport={viewport} onNodeClick={(_,node)=>select.current(node.id)}
  nodesDraggable={false} nodesConnectable={false} elementsSelectable={false} panOnDrag={false} panOnScroll={false} zoomOnScroll={false} zoomOnPinch={false} zoomOnDoubleClick={false} preventScrolling={false} deleteKeyCode={null} proOptions={{hideAttribution:true}}>
  <Arrow/><ViewportPortal>{mapped.labels.map(l=><div className="scene-label" key={`${l.kind}:${l.id}`} style={{left:l.bounds.x,top:l.bounds.y,width:l.bounds.width,height:l.bounds.height}}>{l.text}</div>)}</ViewportPortal>
 </ReactFlow>,layer)}</div>;
}

async function getJson(url){const response=await fetch(url,{cache:'no-store'});if(!response.ok)throw Error(`${url} is not available`);return response.json();}

function App(){
 const [snapshot,setSnapshot]=useState(null),[error,setError]=useState(''),[selected,setSelected]=useState(null),[busy,setBusy]=useState(false),[nodes,setNodes]=useState([]);
 const [engine,setEngine]=useState(null),[profiles,setProfilees]=useState([]),[farChoice,setFarChoice]=useState(null);
 const epoch=useRef(0),fileInput=useRef(null);
 // The engine computes scene geometry; this host only maps and renders it.
 const built=useMemo(()=>{if(!snapshot||!engine)return null;try{return {scene:sceneOf(engine,snapshot)};}catch(e){return {error:e.message};}},[snapshot,engine]);
 const scene=built?.scene??null;
 const sceneError=built?.error??'';
 const mapped=useMemo(()=>scene?toReactFlow(scene,{overlayNode:true}):null,[scene]);
 useEffect(()=>{if(mapped)setNodes(mapped.nodes);},[mapped]);
 useEffect(()=>{setError(sceneError);},[sceneError]);

 useEffect(()=>{
  let alive=true;
  loadEngine('/generated/arqlet.wasm').then(run=>{if(alive)setEngine(()=>run);}).catch(e=>{if(alive)setError(e.message);});
  getJson('/generated/presets/index.json').then(list=>{if(alive)setProfilees(list);}).catch(()=>{});
  return()=>{alive=false;};
 },[]);

 async function load(){const token=++epoch.current;setBusy(false);try{const data=await getJson('/generated/snapshot.json');if(token===epoch.current){setSnapshot(data);setSelected(null);setError('');}}catch(e){if(token===epoch.current)setError(e.message.replace(' is not available','')+' – zuerst die SVG-Demo bauen/starten.');}}
 useEffect(()=>{load();return()=>{epoch.current++;};},[]);

 // Every change goes to the engine as a versioned command; stale answers are dropped by the epoch check.
 // Returns {ok, snapshot?, error?}; a chained request may name the snapshot it builds on.
 async function send(request){
  const token=epoch.current;setBusy(true);
  try{
   const base=request.snapshot??snapshot;
   const response=await fetch('/api/op',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({...request,snapshot:base,expectedRevision:base.revision,expectedLayoutVersion:base.layoutVersion??0})});
   const data=await response.json();if(!data.ok)throw Error(data.error);
   if(token===epoch.current){setSnapshot(data.snapshot);setError('');return {ok:true,snapshot:data.snapshot};}
   return {ok:false,error:'The view changed in the meantime.'};
  }catch(e){if(token===epoch.current)setError(e.message);return {ok:false,error:e.message};}
  finally{if(token===epoch.current)setBusy(false);}
 }
 async function toggle(){if(!snapshot||busy||!selected)return;await send({op:'toggle',nodeId:selected});}
 // Same two-step sequence as the demo (direction, then config set), shared through viewer/presets.js.
 async function applyProfilee(id){
  const profile=profiles.find(p=>p.id===id);if(!snapshot||busy||!profile)return;
  const started=epoch.current;
  const result=await applyPreset({preset:profile,snapshot,stillCurrent:()=>epoch.current===started,run:send});
  if(!result.ok)setError(result.error);
 }
 async function openFile(event){const file=event.target.files?.[0];if(!file)return;const token=++epoch.current;setBusy(false);try{const value=JSON.parse(await file.text());if(token===epoch.current){setSnapshot(value);setSelected(null);setError('');}}catch(e){if(token===epoch.current)setError(e.message);}event.target.value='';}

 const active=scene?.nodes.find(n=>n.id===selected);
 const far=!!scene&&(farChoice??(scene.nodes.length+scene.routes.length>=FAR_AUTO));
 return <main><header><strong>Arqlet · React Flow</strong><button onClick={load}>Initial snapshot</button><button onClick={()=>fileInput.current.click()}>Open snapshot</button><input ref={fileInput} type="file" accept=".json" hidden onChange={openFile}/>
  <select aria-label="Profile" disabled={busy||!profiles.length} value="" onChange={e=>applyProfilee(e.target.value)}><option value="" disabled>Choose profile …</option>{profiles.map(p=><option key={p.id} value={p.id}>{p.title??p.id}</option>)}</select>
  <button disabled={busy||!active?.aggregate} onClick={toggle}>{active?.collapsed?'Expand':'Collapse'}</button><label><input type="checkbox" checked={far} disabled={!scene} onChange={e=>setFarChoice(e.target.checked)}/> GPU far layer</label><span>{selected??'Select a node'} · {scene?.nodes.length??0} nodes</span></header>
 {error&&<p role="alert">{error}</p>}
 {scene&&far&&<FarSceneView scene={scene} selected={selected} onSelect={setSelected}/>}
 {scene&&!far&&<ReactFlow nodes={nodes} edges={mapped.edges} nodeTypes={nodeTypes} edgeTypes={edgeTypes} onNodesChange={changes=>setNodes(ns=>applyNodeChanges(changes,ns))} onNodeClick={(_,node)=>setSelected(node.id)} nodesDraggable={false} nodesConnectable={false} deleteKeyCode={null} elevateEdgesOnSelect={false} elevateNodesOnSelect={false} minZoom={.02} fitView>
  <svg style={{position:'absolute',width:0,height:0}}><defs><marker id="lw-rf-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#73bba7"/></marker></defs></svg>
  <ViewportPortal>{mapped.labels.map(l=><div className="scene-label" key={`${l.kind}:${l.id}`} style={{left:l.bounds.x,top:l.bounds.y,width:l.bounds.width,height:l.bounds.height}}>{l.text}</div>)}{mapped.crossings.map((c,i)=><div className="crossing" key={i} style={{left:c.position.x-3,top:c.position.y-3}}/>)}</ViewportPortal>
  <FitScene bounds={scene.bounds}/><Controls showInteractive={false}/>
 </ReactFlow>}
 </main>;
}
createRoot(document.getElementById('root')).render(<App/>);
