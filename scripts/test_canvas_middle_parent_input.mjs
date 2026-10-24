import assert from 'node:assert/strict';
import fs from 'node:fs';
import {config,validRequest,actionPlan,dispatch} from './canvas_middle_parent_input.mjs';
let admissions=0,refusals=0,plans=0,originalChronologies=0,failures=0;
function simulate(corpus,pre,typing,parent,cadence){
  const c=config(corpus,pre,typing,parent,cadence),plan=actionPlan(corpus,pre,typing,parent,cadence);
  let texts=[c.seed],index=0,start=c.left.length,end=start,enters=0;const reports=[];
  for(const a of plan){
    if(a.type==='select'){index=a.index;start=a.start;end=a.end;assert.ok(0<=start&&start<=end&&end<=texts[index].length);continue;}
    assert.equal(a.index,index);
    if(a.type==='middle'){
      assert.equal(start,end);assert.equal(texts[0].slice(0,start),a.parent);assert.equal(texts[0].slice(end),a.after);
      texts=[a.parent,a.after];index=1;start=end=0;enters++;continue;
    }
    if(a.type==='backspace'&&start===end)start--;
    texts[index]=texts[index].slice(0,start)+(a.text??'')+texts[index].slice(end);
    start=end=start+(a.text?.length??0);assert.equal(texts[index],a.after);
    reports.push({name:index===0?'paragraph':'child',phase:enters?'post':'pre',rtml:a.after.replaceAll('&','&amp;')});
  }
  assert.equal(enters,1);plans++;return reports;
}
for(const corpus of ['ascii','unicode'])for(const pre of ['none','correction','revert','retype'])for(const typing of ['none','correction','erase','restore'])
for(const parent of ['correction','erase','revert'])for(const cadence of ['parent-first','child-first','interleaved'])for(const parentMode of ['bootstrap','retained','retained-zero']){
  const c=config(corpus,pre,typing,parent,cadence),e={ok:true,fixture:'canvas-structure',nonce:'a'.repeat(32),ordinal:0,corpus,mode:'middle',typing,preEnter:pre,middleParent:parent,middleCadence:cadence,parentMode,
    parentId:parentMode==='retained-zero'?'temp:C:Dte0000000000000000000000001':'paragraph',generation:'4',visit:'0',fieldId:'canvas-text-draft',seed:c.seed,offset:c.left.length,revision:0};
  assert.equal(validRequest(e,corpus,pre,typing,parent,cadence,parentMode),true);admissions++;
  for(const [key,value] of [['middleParent','none'],['middleCadence','other'],['parentId','other'],['parentMode','other'],['seed',c.seed+'x'],['offset',0],['revision',1],['mode','end'],['nonce','b'.repeat(31)],['ordinal',true]]){
    assert.equal(validRequest({...e,[key]:value},corpus,pre,typing,parent,cadence,parentMode),false);refusals++;
  }
  simulate(corpus,pre,typing,parent,cadence);
}
const vectors=JSON.parse(fs.readFileSync('tmp/canvas-middle-parent-20261023/reference/bindings.json','utf8')).vectors;
for(const v of vectors){
  const actual=simulate(v.corpus,v.parentCadence==='untouched'?'none':v.parentCadence,v.childCadence,v.postParentCadence==='exact-revert'?'revert':v.postParentCadence,v.schedule==='parent-only'?'parent-first':v.schedule);
  assert.deepEqual(actual,v.chronology.filter(r=>!r.unchanged).map(({name,phase,rtml})=>({name,phase,rtml})));originalChronologies++;
}
for(const failAt of [1,2,3,4,5,6]){
  let calls=0;const call=async()=>{if(++calls===failAt)throw Error('injected');return {result:{value:{ok:true}}};};
  await assert.rejects(dispatch(call,'owned',{corpus:'ascii',preEnter:'none',typing:'none',middleParent:'erase',middleCadence:'parent-first',seed:'Keep thisSecond paragraph'},{}),/injected/);
  assert.equal(calls,failAt);failures++;
}
console.log(JSON.stringify({ok:true,admissions,refusals,plans,originalChronologies,failures,scope:'portable request/plan/report-order and stop-on-error checks; actual browser receipts remain separate'}));
