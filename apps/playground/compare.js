import {evaluateSnapshot} from './comparison.js';
import {loadEngine} from '@arqlet/engine-client/engine.js';
import {renderSvg} from '@arqlet/viewer/svg.js';
import {bindNavigation} from '@arqlet/viewer/navigation.js';
const $=id=>document.getElementById(id), states={};
const engine=loadEngine('./generated/arqlet.wasm');
engine.then(()=>{$('notice').textContent='Choose snapshots.';}).catch(e=>{$('notice').textContent=e.message;});
function draw(id){const s=states[id];if(s)renderSvg($(`svg-${id}`),s.scene,{scale:s.scale});}
function fit(){
 for(const id of Object.keys(states)){const s=states[id],v=$(`canvas-${id}`);s.scale=Math.max(.02,Math.min(1,(v.clientWidth-20)/s.scene.bounds.width,(v.clientHeight-20)/s.scene.bounds.height));}
 if($('same').checked){const scale=Math.min(...Object.values(states).map(s=>s.scale));for(const s of Object.values(states))s.scale=scale;}
 for(const id of Object.keys(states)){draw(id);$(`canvas-${id}`).scrollTop=0;$(`canvas-${id}`).scrollLeft=0;}
}
const fields=[['siblingOverlaps','Sibling overlaps'],['crossBranchOverlaps','Overlaps across branches'],['lineNodeHits','Lines through nodes'],['sharedLinePairs','Overlapping edge pairs'],['sharedLineLength','Overlapping length'],['hugPairs','Border-near edges/nodes'],['hugLength','Border-near length'],['crossings','Crossings without a shared endpoint'],['sharedEndpointCrossings','Crossings with a shared endpoint'],['bends','Bends'],['length','Total length']];
function table(){
 const t=$('metrics');t.replaceChildren();
 for(const row of [['Metric','Left','Right'],...fields.map(([key,label])=>[label,...['a','b'].map(id=>states[id]?.metrics[key]??'—')])]){
  const tr=document.createElement('tr');for(const value of row){const td=document.createElement('td');td.textContent=typeof value==='number'?value.toLocaleString('en-US',{maximumFractionDigits:1}):value;tr.append(td);}t.append(tr);
 }
 if(states.a&&states.b){const same=states.a.identity===states.b.identity;$('notice').textContent=`Metric version ${states.a.metrics.version}. ${same?'Same visible graph structure.':'Warning: different visible graph structure – values are only partly comparable.'} Timing values cannot be derived from snapshots.`;}
}
for(const id of ['a','b']){
 let token=0;
 bindNavigation($(`canvas-${id}`),()=>states[id]?.scale??1,scale=>{if(states[id]){states[id].scale=scale;draw(id);if($('same').checked){const other=id==='a'?'b':'a';if(states[other]){states[other].scale=scale;draw(other);}}}});
 $(`file-${id}`).onchange=async e=>{
  const current=++token,file=e.target.files[0];if(!file)return;
  try{
   const snapshot=JSON.parse(await file.text()),run=await engine;if(current!==token)return;
   states[id]=evaluateSnapshot(run,snapshot);$(`options-${id}`).textContent=JSON.stringify(snapshot.options,null,2);fit();table();
  }catch(error){if(current!==token)return;delete states[id];$(`svg-${id}`).replaceChildren();$(`options-${id}`).textContent='';table();$('notice').textContent=`${file.name}: ${error.message}`;}
 };
}
$('fit').onclick=fit;$('same').onchange=fit;
