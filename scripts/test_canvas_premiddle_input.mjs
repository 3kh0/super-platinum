import assert from 'node:assert/strict';
import {config,actionPlan,validRequest,dispatch} from './canvas_premiddle_input.mjs';
let admissions=0,refusals=0,plans=0,failures=0;
for(const corpus of ['ascii','unicode'])for(const pre of ['correction','revert','retype'])for(const typing of ['none','correction','erase','restore'])for(const parentMode of ['bootstrap','retained','retained-zero']) {
  const c=config(corpus,pre,typing),request={ok:true,fixture:'canvas-structure',nonce:'a'.repeat(32),ordinal:0,generation:'4',visit:'1',fieldId:'canvas-text-draft',mode:'middle',corpus,typing,preEnter:pre,parentMode,
    parentId:parentMode==='retained-zero'?'temp:C:Dte0000000000000000000000001':'paragraph',seed:c.seed,offset:c.left.length,revision:0};
  assert.equal(validRequest(request,corpus,pre,typing,parentMode),true);admissions++;
  for(const [key,value] of [['preEnter','none'],['mode','end'],['seed',c.seed+'x'],['offset',c.left.length+1],['revision',1],['generation',4],['visit',1],['nonce','b'.repeat(31)],['ordinal',true],['parentId','other'],['parentMode','other']]) {
    assert.equal(validRequest({...request,[key]:value},corpus,pre,typing,parentMode),false);refusals++;
  }
  const plan=actionPlan(corpus,pre,typing);assert.equal(plan.filter(a=>a.type==='middle').length,1);assert.equal(plan.find(a=>a.type==='middle').parent,c.liveLeft);plans++;
}
// A failed observation or content dispatch must stop the visit; no retry/replay.
for(const failAt of [1,2,3,4,5,6]) {
  let count=0;const calls=[];
  const call=async(method)=>{count++;calls.push(method);if(count===failAt)throw Error('injected');return {result:{value:{ok:true}}};};
  const result={};
  await assert.rejects(dispatch(call,'owned',{corpus:'ascii',preEnter:'correction',typing:'none',seed:'Keep thisSecond paragraph'},result),/injected/);
  assert.equal(count,failAt);assert.equal(calls.length,failAt);failures++;
}
console.log(JSON.stringify({ok:true,admissions,refusals,plans,failures,scope:'finite request admission and stop-on-first-failure; actual browser evidence remains separate'}));
