import test from 'node:test';
import assert from 'node:assert/strict';
import {zoomAnchor,bindNavigation} from './navigation.js';
test('zoom preserves the world coordinate under the pointer',()=>{
 const scroll=90,anchor=130,old=.5,next=2;
 assert.equal((zoomAnchor(scroll,anchor,old,next)+anchor)/next,(scroll+anchor)/old);
});
test('a press that an element claimed (defaultPrevented) does not start a pan or take the capture',()=>{
 const listeners={};let captured=false;
 const viewport={scrollLeft:100,scrollTop:100,addEventListener:(n,f)=>listeners[n]=f,setPointerCapture:()=>{captured=true;}};
 bindNavigation(viewport,()=>1,()=>{});
 listeners.pointerdown({button:0,pointerId:1,clientX:10,clientY:10,defaultPrevented:true});
 listeners.pointermove({pointerId:1,clientX:60,clientY:60,preventDefault(){}});
 assert.equal(viewport.scrollLeft,100);assert.equal(viewport.scrollTop,100);assert.equal(captured,false);
});
test('drag pans and suppresses the click, a fresh click is still selectable',()=>{
 const listeners={};const viewport={scrollLeft:100,scrollTop:100,addEventListener:(n,f)=>listeners[n]=f,setPointerCapture:()=>{}};
 bindNavigation(viewport,()=>1,()=>{});
 listeners.pointerdown({button:0,pointerId:1,clientX:10,clientY:10});
 listeners.pointermove({pointerId:1,clientX:30,clientY:40,preventDefault(){}});
 assert.equal(viewport.scrollLeft,80);assert.equal(viewport.scrollTop,70);
 listeners.pointerup();let suppressed=false;
 listeners.click({preventDefault(){},stopImmediatePropagation(){suppressed=true;}});assert.ok(suppressed);
 suppressed=false;listeners.pointerdown({button:0,pointerId:2,clientX:10,clientY:10});listeners.pointerup();
 listeners.click({preventDefault(){},stopImmediatePropagation(){suppressed=true;}});assert.equal(suppressed,false);
});
