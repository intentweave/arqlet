import {test} from 'node:test';
import assert from 'node:assert/strict';
import {acceptPublication,refinementFits} from './revisions.js';
const base={epoch:7,revision:2,layoutVersion:3},current={revision:2,layoutVersion:3},result={revision:2,baseRevision:2,layoutVersion:4};
test('accept compatible refinement',()=>assert.equal(refinementFits(current,result,base,7,false),true));
test('reject while local edit computes',()=>assert.equal(refinementFits(current,result,base,7,true),false));
test('reject newer edit and newer layout',()=>{assert.equal(refinementFits({...current,revision:3},result,base,7,false),false);assert.equal(refinementFits({...current,layoutVersion:4},result,base,7,false),false);});
test('reject after reset even with matching numeric revision',()=>assert.equal(refinementFits(current,result,base,8,false),false));
test('reject unrelated result version',()=>assert.equal(refinementFits(current,{...result,layoutVersion:8},base,7,false),false));

const job={id:'refine-1',epoch:7,revision:2,layoutVersion:3,lastSequence:0};
const publication=(sequence,extra={})=>({jobId:'refine-1',sequence,revision:2,baseRevision:2,layoutVersion:4,snapshot:{revision:2,baseRevision:2,layoutVersion:4},...extra});
test('accept the next publication of the job, also after an earlier one was shown',()=>{
  assert.equal(acceptPublication(publication(1),job,7,false),true);
  assert.equal(acceptPublication(publication(2),{...job,lastSequence:1},7,false),true);
});
test('reject a repeated or older publication',()=>assert.equal(acceptPublication(publication(2),{...job,lastSequence:2},7,false),false));
test('reject publications of another job',()=>assert.equal(acceptPublication(publication(1,{jobId:'refine-0'}),job,7,false),false));
test('reject publications while a local edit is pending or after a reset',()=>{
  assert.equal(acceptPublication(publication(1),job,7,true),false);
  assert.equal(acceptPublication(publication(1),job,8,false),false);
});
test('reject a publication from an edited graph',()=>assert.equal(acceptPublication({...publication(1),revision:3,baseRevision:3,snapshot:{revision:3,baseRevision:3,layoutVersion:4}},job,7,false),false));
