// Portable controller contract tests. Actual owned-browser evidence is separate.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {actionPlan,validRequest,dispatch,inspect,histories,layouts,cadences} from './canvas_sequence_input.mjs';

const expected=(count,cadence,corpus='ascii',history='bootstrap',layout='no-following')=>({ok:true,corpus,mode:'sequence',typing:'none',sequenceHistory:history,sequenceLayout:layout,sequenceCount:count,sequenceCadence:cadence,fixture:'canvas-structure',nonce:'a'.repeat(32),ordinal:0,generation:'1',visit:'0',fieldId:'canvas-text-draft',seed:corpus==='ascii'?'Keep this':'Caf\u00e9 & \u65e5\u672c\u{1f600}',offset:corpus==='ascii'?9:11,revision:0});
let admissions=0,refusals=0,plans=0;
for(let count=1;count<=16;count++)for(const cadence of cadences)for(const corpus of ['ascii','unicode'])for(const history of histories)for(const layout of layouts){
  const v=expected(count,cadence,corpus,history,layout),allowed=(count>=2||cadence.startsWith('preenter-'))&&!((cadence.startsWith('revisit-')||cadence.startsWith('parent-'))&&count<3);
  assert.equal(validRequest(v,history,layout,corpus,count,cadence),allowed);admissions++;
  for(const [key,value] of [['mode','end'],['typing','correction'],['sequenceCount','3'],['revision',1],['offset',0],['nonce','bad'],['ordinal',2],['generation',1],['visit','bad'],['fieldId','other'],['seed','drift']]){
    assert.equal(validRequest({...v,[key]:value},history,layout,corpus,count,cadence),false);refusals++;
  }
  const plan=actionPlan(corpus,count,cadence);
  if(!allowed){assert.equal(plan,null);continue;}
  assert.equal(plan.filter(a=>a.type==='enter').length,count);plans++;
}
const reference=JSON.parse(fs.readFileSync(new URL('../tmp/canvas-sequence-revisit-20261023/reference/cadence-patches.json',import.meta.url),'utf8')).vectors;
for(const vector of reference){
  const plan=actionPlan(vector.corpus,3,'revisit-'+vector.revisit.replace('exact-','')),groups=[],chronology=[];let active=-1;
  for(const action of plan){
    if(action.type==='enter'){active=groups.length;groups.push([]);}
    else if(action.type==='click')active=action.activeIndex-1;
    else if(action.type!=='select'){groups[active].push(action.after.replaceAll('&','&amp;'));chronology.push(active);}
  }
  assert.deepEqual(groups[0],vector.firstMember.filter(v=>!v.unchanged).map(v=>v.rtml));
  assert.deepEqual(chronology.slice(0,3),[0,1,0]);
  assert.deepEqual(plan.filter(a=>a.type==='click').map(a=>a.activeIndex),[1,2]);
}
let dispatchChecks=0;
for(const cadence of cadences.filter(c=>c.startsWith('revisit-')||c.startsWith('parent-')))for(const corpus of ['ascii','unicode']){
  for(const failAt of [null,'mousePressed','mouseReleased','postclick']){
    const calls=[],result={},v=expected(3,cadence,corpus);let clicked=false;
    const call=async(method,params,session)=>{
      assert.equal(session,'owned-session');calls.push({method,params});
      if(method==='Input.dispatchMouseEvent'){
        assert.deepEqual([params.x,params.y],[42,84]);clicked=true;
        if(params.type===failAt)throw Error('injected dispatch failure; no replay');
      }
      if(method!=='Runtime.evaluate')return {};
      const args=JSON.parse('['+params.expression.slice(params.expression.lastIndexOf('})(')+3,-1)+']');
      assert.deepEqual(args[0],v);
      const [owner,texts,revision,selection,complete,index,point]=args;
      if(clicked&&failAt==='postclick')return {result:{value:{ok:false}}};
      return {result:{value:{ok:true,texts,revision,activeIndex:index,...(point?{point:{x:42,y:84}}:{})}}};
    };
    if(failAt){
      await assert.rejects(dispatch(call,'owned-session',v,result));
      const firstClick=calls.findIndex(c=>c.method==='Input.dispatchMouseEvent');
      assert(firstClick>=0);assert(!calls.slice(firstClick+1).some(c=>c.method==='Input.insertText'||c.method==='Input.dispatchKeyEvent'));
      assert.equal(calls.filter(c=>c.method==='Input.dispatchMouseEvent'&&c.params.type==='mousePressed').length,1);
    }else{
      await dispatch(call,'owned-session',v,result);
      assert.equal(result.completion.texts.length,4);
      assert.equal(calls.filter(c=>c.method==='Input.dispatchMouseEvent').length,4);
      assert.equal(result.completion.revision,actionPlan(corpus,3,cadence).filter(a=>!['select','click'].includes(a.type)).length);
    }
    dispatchChecks++;
  }
}
// Prelude input must use the owned live snapshot until the first Enter attests it.
let preludeChecks=0;
for(const cadence of cadences.filter(c=>c.startsWith('preenter-')))for(const corpus of ['ascii','unicode'])for(const count of [1,3]){
  const v=expected(count,cadence,corpus),result={},calls=[];
  const call=async(method,params)=>{
    calls.push({method,params});
    if(method!=='Runtime.evaluate')return {};
    const args=JSON.parse('['+params.expression.slice(params.expression.lastIndexOf('})(')+3,-1)+']');
    return {result:{value:{ok:true,texts:args[1],revision:args[2]}}};
  };
  await dispatch(call,'owned',v,result);
  assert.equal(calls[0].method,'Runtime.evaluate');
  assert.equal(result.completion.texts.length,count+1);
  assert.equal(result.completion.texts[0],cadence==='preenter-erase'?'':v.seed+(cadence==='preenter-correction'?' revised':''));
  assert(!calls.some(c=>c.method==='Input.dispatchMouseEvent'));
  let input=0;
  await assert.rejects(dispatch(async(method)=>{if(method.startsWith('Input.'))input++;return {result:{value:{ok:false}}};},'owned',v,{}));
  assert.equal(input,0);preludeChecks++;
}
const blocks=parts=>[{type:'rich_text',elements:parts.map(text=>({type:'rich_text_section',elements:text?[{type:'text',text}]:[]}))}];
const owner=expected(1,'preenter-erase'),child={contains:()=>true},field={isContentEditable:true,dataset:{canvasSplitProbe:owner.nonce+':0'},firstChild:{childNodes:[child,child]},innerHTML:''};
const root={dataset:{editorGeneration:'1',canvasVisit:'0'},querySelector:()=>field};
let snap={revision:1,composing:false,blocks:blocks([''])},captured=null;
globalThis.document={activeElement:field,querySelector:()=>root,createRange:()=>({selectNodeContents(){},setEnd(){},toString(){return '';}})};
globalThis.window={superPlatinumRichEditor:{snapshot:()=>snap},superPlatinumCanvasSplit:{capture:()=>captured}};
globalThis.getSelection=()=>({rangeCount:1,getRangeAt:()=>({startContainer:child,endContainer:child,startOffset:0,endOffset:0})});
assert.equal(inspect(owner,[''],1).ok,true);
captured={kind:'sequence',after:blocks([''])};assert.equal(inspect(owner,[''],1).ok,false);
captured=undefined;assert.equal(inspect(owner,[''],1).ok,false);
captured=null;snap.blocks=blocks(['forged']);assert.equal(inspect(owner,[''],1).ok,false);
snap.blocks=blocks(['']);snap.composing=true;assert.equal(inspect(owner,[''],1).ok,false);snap.composing=false;
field.dataset.canvasSplitProbe='foreign';assert.equal(inspect(owner,[''],1).ok,false);field.dataset.canvasSplitProbe=owner.nonce+':0';
snap={revision:2,composing:false,blocks:blocks(['',''])};assert.equal(inspect(owner,['',''],2).ok,false);
captured={kind:'sequence',sequence:{after:snap.blocks,events:[{kind:'edit'},{kind:'enter'}]}};
assert.equal(inspect(owner,['',''],2,null,true).ok,true);assert.equal(field.dataset.canvasSequenceComplete,owner.nonce+':0');
assert.equal(inspect(owner,['',''],3).ok,false);
console.log(JSON.stringify({ok:true,admissions,refusals,plans,originalPatchReportComparisons:reference.length,dispatchChecks,preludeChecks,preludeSnapshotGuards:9,actualBrowser:false}));
