import React,{useEffect,useMemo,useRef,useState} from 'react';
import {createRoot} from 'react-dom/client';
import {ReactFlow,Handle,Position,BaseEdge,Controls,ViewportPortal,applyNodeChanges,useReactFlow} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import './style.css';
import {routePath} from '@arqlet/viewer/scene.js';
import {loadEngine,sceneOf} from '@arqlet/engine-client/engine.js';
import {toReactFlow} from '@arqlet/viewer/react-flow.js';
import {applyPreset} from '@arqlet/engine-client/presets.js';

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
const nodeTypes={arqlet:DiagramNode},edgeTypes={arqlet:DiagramEdge};
function FitScene({bounds}){const {fitBounds}=useReactFlow();useEffect(()=>{fitBounds(bounds,{padding:.1});},[bounds,fitBounds]);return null;}

async function getJson(url){const response=await fetch(url,{cache:'no-store'});if(!response.ok)throw Error(`${url} is not available`);return response.json();}

function App(){
 const [snapshot,setSnapshot]=useState(null),[error,setError]=useState(''),[selected,setSelected]=useState(null),[busy,setBusy]=useState(false),[nodes,setNodes]=useState([]);
 const [engine,setEngine]=useState(null),[profiles,setProfilees]=useState([]);
 const epoch=useRef(0),fileInput=useRef(null);
 // The engine computes scene geometry; this host only maps and renders it.
 const built=useMemo(()=>{if(!snapshot||!engine)return null;try{return {scene:sceneOf(engine,snapshot)};}catch(e){return {error:e.message};}},[snapshot,engine]);
 const scene=built?.scene??null;
 const sceneError=built?.error??'';
 const mapped=useMemo(()=>scene?toReactFlow(scene):null,[scene]);
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
 return <main><header><strong>Arqlet · React Flow</strong><button onClick={load}>Initial snapshot</button><button onClick={()=>fileInput.current.click()}>Open snapshot</button><input ref={fileInput} type="file" accept=".json" hidden onChange={openFile}/>
  <select aria-label="Profile" disabled={busy||!profiles.length} value="" onChange={e=>applyProfilee(e.target.value)}><option value="" disabled>Choose profile …</option>{profiles.map(p=><option key={p.id} value={p.id}>{p.title??p.id}</option>)}</select>
  <button disabled={busy||!active?.aggregate} onClick={toggle}>{active?.collapsed?'Expand':'Collapse'}</button><span>{selected??'Select a node'} · {scene?.nodes.length??0} nodes</span></header>
 {error&&<p role="alert">{error}</p>}
 {scene&&<ReactFlow nodes={nodes} edges={mapped.edges} nodeTypes={nodeTypes} edgeTypes={edgeTypes} onNodesChange={changes=>setNodes(ns=>applyNodeChanges(changes,ns))} onNodeClick={(_,node)=>setSelected(node.id)} nodesDraggable={false} nodesConnectable={false} deleteKeyCode={null} elevateEdgesOnSelect={false} elevateNodesOnSelect={false} minZoom={.02} fitView>
  <svg style={{position:'absolute',width:0,height:0}}><defs><marker id="lw-rf-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerUnits="userSpaceOnUse" markerWidth="8" markerHeight="8" orient="auto-start-reverse"><path d="M 0 0 L 10 5 L 0 10 z" fill="#73bba7"/></marker></defs></svg>
  <ViewportPortal>{mapped.labels.map(l=><div className="scene-label" key={`${l.kind}:${l.id}`} style={{left:l.bounds.x,top:l.bounds.y,width:l.bounds.width,height:l.bounds.height}}>{l.text}</div>)}{mapped.crossings.map((c,i)=><div className="crossing" key={i} style={{left:c.position.x-3,top:c.position.y-3}}/>)}</ViewportPortal>
  <FitScene bounds={scene.bounds}/><Controls showInteractive={false}/>
 </ReactFlow>}
 </main>;
}
createRoot(document.getElementById('root')).render(<App/>);
