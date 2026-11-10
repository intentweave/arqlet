/** Scroll-based navigation; zoom keeps the world point under the pointer fixed. */
export function zoomAnchor(scroll, anchor, oldScale, newScale) {
 return (scroll + anchor) * newScale / oldScale - anchor;
}
export function bindNavigation(viewport, getScale, setScale) {
 viewport.addEventListener('wheel', e => {
  e.preventDefault();
  const rect=viewport.getBoundingClientRect(), old=getScale();
  const next=Math.max(.02,Math.min(4,old*Math.exp(-e.deltaY*.002)));
  const x=zoomAnchor(viewport.scrollLeft,e.clientX-rect.left,old,next);
  const y=zoomAnchor(viewport.scrollTop,e.clientY-rect.top,old,next);
  setScale(next);viewport.scrollLeft=x;viewport.scrollTop=y;
 },{passive:false});
 let drag=null,moved=false;
 viewport.addEventListener('pointerdown',e=>{
  // A press that an element has claimed (for example a node drag) belongs to that element, not to a pan.
  if(e.defaultPrevented||(e.button!==0&&e.button!==1))return;
  drag={id:e.pointerId,x:e.clientX,y:e.clientY,left:viewport.scrollLeft,top:viewport.scrollTop};moved=false;
 });
 viewport.addEventListener('pointermove',e=>{
  if(!drag||e.pointerId!==drag.id)return;
  const dx=e.clientX-drag.x,dy=e.clientY-drag.y;
  if(!moved&&Math.hypot(dx,dy)<4)return;
  moved=true;viewport.setPointerCapture(e.pointerId);
  viewport.scrollLeft=drag.left-dx;viewport.scrollTop=drag.top-dy;e.preventDefault();
 });
 for(const event of ['pointerup','pointercancel','lostpointercapture'])viewport.addEventListener(event,()=>{drag=null;});
 viewport.addEventListener('click',e=>{if(moved){e.preventDefault();e.stopImmediatePropagation();moved=false;}},true);
}
