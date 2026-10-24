// Two owned interior Enters with creation identity kept separate from DOM order.
import fs from 'node:fs/promises';
import {admission} from './canvas_sequence_input.mjs';
import {validRequest as parentRequest} from './canvas_middle_parent_input.mjs';

export function config(corpus='ascii',secondMiddle='parent',secondMiddleCadence='straight') {
  if(!['ascii','unicode'].includes(corpus)||!['parent','child'].includes(secondMiddle)||!['straight','edited'].includes(secondMiddleCadence))return null;
  const left=corpus==='ascii'?'Keep this':'Caf\u00e9 & \u65e5\u672c\u{1f600}',tail=corpus==='ascii'?'Second paragraph':'Second \u65e5\u672c\u{1f600} & paragraph';
  return {left,tail,seed:left+tail,offset:left.length,secondOffset:secondMiddle==='child'?7:corpus==='ascii'?5:7};
}
export function validRequest(e,corpus='ascii',secondMiddle='parent',secondMiddleCadence='straight',parentMode='bootstrap') {
  return !!config(corpus,secondMiddle,secondMiddleCadence)&&e?.secondMiddle===secondMiddle&&e.secondMiddleCadence===secondMiddleCadence
    &&e.preEnter==='none'&&e.typing==='none'&&e.middleParent==='none'&&e.middleCadence==='parent-first'
    &&parentRequest({...e,middleParent:'correction'},corpus,'none','none','correction','parent-first',parentMode);
}
export function actionPlan(corpus='ascii',secondMiddle='parent',secondMiddleCadence='straight') {
  const c=config(corpus,secondMiddle,secondMiddleCadence);if(!c)return null;
  const texts=[c.left,c.tail],order=[0,1],plan=[];
  const edit=(id,start,end,type,after,text)=>{const index=order.indexOf(id);plan.push({type:'select',index,start,end},{type,index,after,...(text===undefined?{}:{text})});texts[id]=after;};
  const append=(id,s)=>edit(id,texts[id].length,texts[id].length,'insert',texts[id]+s,s);
  const back=(id)=>edit(id,texts[id].length,texts[id].length,'backspace',texts[id].slice(0,-1));
  const revise=(id)=>{const n=texts[id].length;edit(id,n-5,n,'insert',texts[id].slice(0,-5)+'revised','revised');};
  plan.push({type:'select',index:0,start:c.offset,end:c.offset},{type:'middle',index:0,parent:c.left,after:c.tail});
  const target=secondMiddle==='parent'?0:1,other=1-target;
  if(secondMiddleCadence==='edited'){append(target,' draftx');append(other,' changed');back(target);revise(target);}
  const before=texts[target],left=before.slice(0,c.secondOffset),right=before.slice(c.secondOffset),index=order.indexOf(target);
  plan.push({type:'select',index,start:c.secondOffset,end:c.secondOffset},{type:'second-middle',index,parent:left,after:right});
  texts[target]=left;texts[2]=right;order.splice(index+1,0,2);
  if(secondMiddleCadence==='edited'){
    const child=texts[1];append(1,' draftx');append(2,' draftx');edit(0,0,texts[0].length,'backspace','');back(2);
    edit(1,child.length,texts[1].length,'backspace',child);revise(2);
  }
  return plan;
}

// Only caret selection mutates DOM. Actual content arrives through trusted CDP.
export function inspect(expected,texts,revision,index,selection=null,complete=false) {
  const root=document.querySelector('.canvas-dialog #canvas-text-editor'),field=root?.querySelector('#canvas-text-draft');
  if(!root||!field||!field.isContentEditable||document.activeElement!==field||root.dataset.editorGeneration!==expected.generation
    ||root.dataset.canvasVisit!==expected.visit||field.dataset.canvasSplitProbe!==expected.nonce+':'+expected.ordinal)return {ok:false};
  const snapshot=window.superPlatinumRichEditor.snapshot(field),witness=window.superPlatinumCanvasSplit?.capture(field,expected.generation,expected.visit);
  if(texts.length===1?witness!==null:!witness||![undefined,'middle','middle-sequence','second-middle-sequence'].includes(witness.kind))return {ok:false,witness};
  const blocks=texts.length===1?snapshot.blocks:witness.sequence?.after??witness.typing?.after??witness.after;
  const actual=blocks?.[0]?.elements?.map(s=>s.elements?.map(r=>r.type==='text'?r.text:null).join(''));
  if(JSON.stringify(actual)!==JSON.stringify(texts)||snapshot.revision!==revision||snapshot.composing)return {ok:false,actual,revision:snapshot.revision};
  const section=field.firstChild?.childNodes[index],text=texts[index];if(!section)return {ok:false};
  if(selection){
    const points=[],walker=document.createTreeWalker(section,NodeFilter.SHOW_TEXT);let offset=0;
    for(let node=walker.nextNode();node;node=walker.nextNode()){
      for(const [i,p]of[selection.start,selection.end].entries())if(!points[i]&&p>=offset&&p<=offset+node.length)points[i]=[node,p-offset];offset+=node.length;
    }
    if(!text&&selection.start===0&&selection.end===0)points[0]=points[1]=[section,0];
    if(!points[0]||!points[1])return {ok:false};
    const r=document.createRange();r.setStart(...points[0]);r.setEnd(...points[1]);getSelection().removeAllRanges();getSelection().addRange(r);
    if(getSelection().toString()!==text.slice(selection.start,selection.end))return {ok:false};
  }
  const s=getSelection(),r=s?.rangeCount===1?s.getRangeAt(0):null;
  if(!r||!section.contains(r.startContainer)||!section.contains(r.endContainer))return {ok:false};
  const at=(node,n)=>{const p=document.createRange();p.selectNodeContents(section);p.setEnd(node,n);return p.toString().length;};
  if(complete){
    const events=witness?.sequence?.events,first=events?.filter(e=>e.kind==='middle'),second=events?.filter(e=>e.kind==='second-middle');
    if(texts.length!==3||witness.kind!=='second-middle-sequence'||first?.length!==1||second?.length!==1||events.some(e=>e.kind==='enter')
      ||second[0].revision<=first[0].revision||second[0].activeIndex!==(expected.secondMiddle==='parent'?1:2)
      ||witness.offset!==first[0].start||witness.revision!==first[0].revision)return {ok:false};
    field.dataset.canvasSecondMiddleComplete=expected.nonce+':'+expected.ordinal;
  }
  return {ok:true,witness,revision,html:field.innerHTML,texts,activeIndex:index,selection:{start:at(r.startContainer,r.startOffset),end:at(r.endContainer,r.endOffset)}};
}
export async function dispatch(call,sessionId,expected,result) {
  const plan=actionPlan(expected.corpus,expected.secondMiddle,expected.secondMiddleCadence);if(!plan)throw Error('Unknown second middle plan');
  let texts=[expected.seed],revision=0,index=0;result.actions=[];
  const check=async(selection=null,complete=false)=>{const r=await call('Runtime.evaluate',{expression:`(${inspect.toString()})(${JSON.stringify(expected)},${JSON.stringify(texts)},${revision},${index},${JSON.stringify(selection)},${complete})`,returnByValue:true},sessionId);if(r.exceptionDetails||r.result?.value?.ok!==true)throw Error('Second middle inspection failed; no replay: '+JSON.stringify(r.result?.value));return r.result.value;};
  for(const action of plan){
    const entry={action,dispatchAttempted:false,before:await check()};result.actions.push(entry);
    if(action.type==='select'){index=action.index;entry.after=await check(action);continue;}
    if(action.index!==index)throw Error('Second middle selection target changed');
    entry.dispatchAttempted=true;result.dispatchAttempted=true;
    if(action.type==='insert')await call('Input.insertText',{text:action.text},sessionId);
    else {const enter=action.type==='middle'||action.type==='second-middle',key=enter?'Enter':'Backspace',code=enter?13:8;
      await call('Input.dispatchKeyEvent',{type:'keyDown',key,code:key,windowsVirtualKeyCode:code,nativeVirtualKeyCode:code,...(enter?{text:'\r',unmodifiedText:'\r'}:{})},sessionId);
      await call('Input.dispatchKeyEvent',{type:'keyUp',key,code:key,windowsVirtualKeyCode:code,nativeVirtualKeyCode:code},sessionId);
    }
    if(action.type==='middle'||action.type==='second-middle'){texts.splice(index,1,action.parent,action.after);index++;}else texts[index]=action.after;
    revision++;entry.after=await check();
  }
  result.completion=await check(null,true);
}
async function main() {
  const config=JSON.parse(await fs.readFile(process.argv[2],'utf8'));
  const {port,browserPath,expected,output,corpus='ascii',secondMiddle='parent',secondMiddleCadence='straight',parentMode='bootstrap'}=config;
  if(!Number.isInteger(port)||port<1||port>65535||
      !/^\/devtools\/browser\/[a-zA-Z0-9-]+$/.test(browserPath)||
      !validRequest(expected,corpus,secondMiddle,secondMiddleCadence,parentMode))throw Error('Invalid bounded Canvas Enter request');
  const result={ok:false,expected,dispatchAttempted:false,osInput:false};
  const ws=new WebSocket(`ws://127.0.0.1:${port}${browserPath}`);
  let next=0;const pending=new Map();
  const deadline=setTimeout(()=>ws.close(),20000);
  ws.addEventListener('message',event=>{const r=JSON.parse(event.data),p=pending.get(r.id);if(!p)return;pending.delete(r.id);clearTimeout(p.timer);r.error?p.reject(Error(JSON.stringify(r.error))):p.resolve(r.result);});
  ws.addEventListener('close',()=>{for(const p of pending.values()){clearTimeout(p.timer);p.reject(Error('Owned CDP socket closed'));}pending.clear();});
  function call(method,params={},sessionId){return new Promise((resolve,reject)=>{const id=++next;const timer=setTimeout(()=>{pending.delete(id);reject(Error(method+' timed out; do not replay'));},4000);pending.set(id,{resolve,reject,timer});ws.send(JSON.stringify({id,method,params,...(sessionId?{sessionId}:{})}));});}
  try {
    await new Promise((resolve,reject)=>{ws.addEventListener('open',resolve,{once:true});ws.addEventListener('error',()=>reject(Error('Owned CDP connection failed')),{once:true});});
    const targets=(await call('Target.getTargets')).targetInfos.filter(t=>t.type==='page');
    const matches=[];
    for(const target of targets){
      const {sessionId}=await call('Target.attachToTarget',{targetId:target.targetId,flatten:true});
      const probe=await call('Runtime.evaluate',{expression:`(${admission.toString()})(${JSON.stringify(expected)})`,returnByValue:true},sessionId);
      if(probe.result?.value?.ok===true)matches.push({sessionId,targetId:target.targetId,admission:probe.result.value});
      else await call('Target.detachFromTarget',{sessionId});
    }
    if(matches.length!==1)throw Error('Expected exactly one marked Canvas caret; no input sent');
    const match=matches[0];result.targetId=match.targetId;result.admission=match.admission;
    // Revalidate immediately before the one non-replayed browser action.
    const check=await call('Runtime.evaluate',{expression:`(${admission.toString()})(${JSON.stringify(expected)})`,returnByValue:true},match.sessionId);
    if(check.result?.value?.ok!==true)throw Error('Canvas caret changed before Enter');
    await dispatch(call,match.sessionId,expected,result);
    result.ok=true;
  } catch(error){result.error=String(error);process.exitCode=1;}
  finally {clearTimeout(deadline);ws.close();await fs.writeFile(output,JSON.stringify(result,null,2)+'\n',{flag:'wx'});}
}
if(process.argv[1]?.replaceAll('\\','/').endsWith('/canvas_second_middle_input.mjs'))await main();
