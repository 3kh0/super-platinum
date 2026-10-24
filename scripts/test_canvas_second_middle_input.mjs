import assert from 'node:assert/strict';
import fs from 'node:fs';
import {config,validRequest,actionPlan,dispatch} from './canvas_second_middle_input.mjs';
let admissions=0,refusals=0,plans=0,originalChronologies=0,failures=0;
function simulate(corpus,target,cadence){
  const c=config(corpus,target,cadence),texts=[c.seed],ids=['paragraph'],reports=[],splits=[];let index=0,start=c.offset,end=start;
  for(const a of actionPlan(corpus,target,cadence)){
    if(a.type==='select'){index=a.index;start=a.start;end=a.end;assert.ok(0<=start&&start<=end&&end<=texts[index].length);continue;}
    assert.equal(a.index,index);
    if(a.type==='middle'||a.type==='second-middle'){
      assert.equal(start,end);assert.equal(texts[index].slice(0,start),a.parent);assert.equal(texts[index].slice(end),a.after);
      const child=splits.length===0?'temp:C:xyz0123456789abcdef012345678':'temp:C:xyz0123456789abcdef012345679';
      splits.push({target:ids[index],child,left:a.parent.replaceAll('&','&amp;'),right:a.after.replaceAll('&','&amp;'),visibleOffset:start});
      texts.splice(index,1,a.parent,a.after);ids.splice(index+1,0,child);index++;start=end=0;continue;
    }
    if(a.type==='backspace'&&start===end)start--;
    texts[index]=texts[index].slice(0,start)+(a.text??'')+texts[index].slice(end);start=end=start+(a.text?.length??0);assert.equal(texts[index],a.after);
    reports.push({id:ids[index],phase:splits.length===1?'between':'after-second',rtml:a.after.replaceAll('&','&amp;')});
  }
  assert.equal(splits.length,2);plans++;return {reports,splits,ids,texts};
}
for(const corpus of ['ascii','unicode'])for(const target of ['parent','child'])for(const cadence of ['straight','edited'])for(const parentMode of ['bootstrap','retained','retained-zero']){
  const c=config(corpus,target,cadence),e={ok:true,fixture:'canvas-structure',nonce:'a'.repeat(32),ordinal:0,corpus,mode:'middle',typing:'none',preEnter:'none',middleParent:'none',middleCadence:'parent-first',secondMiddle:target,secondMiddleCadence:cadence,parentMode,
    parentId:parentMode==='retained-zero'?'temp:C:Dte0000000000000000000000001':'paragraph',generation:'4',visit:'0',fieldId:'canvas-text-draft',seed:c.seed,offset:c.offset,revision:0};
  assert.equal(validRequest(e,corpus,target,cadence,parentMode),true);admissions++;
  for(const [key,value] of [['secondMiddle','none'],['secondMiddleCadence','other'],['middleParent','erase'],['middleCadence','interleaved'],['preEnter','revert'],['typing','correction'],['parentId','other'],['parentMode','other'],['seed',c.seed+'x'],['offset',0],['revision',1],['mode','end'],['nonce','b'.repeat(31)],['ordinal',true]]){
    assert.equal(validRequest({...e,[key]:value},corpus,target,cadence,parentMode),false);refusals++;
  }
  simulate(corpus,target,cadence);
}
for(const v of JSON.parse(fs.readFileSync('tmp/canvas-second-middle-20261023/reference/cadences.json','utf8')).vectors){
  const a=simulate(v.corpus,v.secondTarget,v.cadence);
  assert.deepEqual(a.reports,v.reports.filter(r=>!r.unchanged).map(({id,phase,rtml})=>({id,phase,rtml})));
  assert.deepEqual(a.splits,v.splits.map(({target,child,left,right,visibleOffset})=>({target,child,left,right,visibleOffset})));
  assert.deepEqual(a.ids,v.visualOrder);originalChronologies++;
}
for(const failAt of [1,2,3,4,5,6]){
  let calls=0;const call=async()=>{if(++calls===failAt)throw Error('injected');return {result:{value:{ok:true}}};};
  await assert.rejects(dispatch(call,'owned',{corpus:'ascii',secondMiddle:'parent',secondMiddleCadence:'edited',seed:'Keep thisSecond paragraph'},{}),/injected/);
  assert.equal(calls,failAt);failures++;
}
console.log(JSON.stringify({ok:true,admissions,refusals,plans,originalChronologies,failures,scope:'portable request/plan/original-chronology and stop-on-first-error; browser results are separate'}));
