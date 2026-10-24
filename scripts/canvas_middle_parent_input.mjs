// One owned middle Enter followed by fixed parent/child report schedules.
import fs from 'node:fs/promises';
import {admission} from './canvas_sequence_input.mjs';
import {config as preConfig,actionPlan as prePlan,validRequest as preRequest} from './canvas_premiddle_input.mjs';

export function config(corpus,preEnter='none',typing='none',middleParent='correction',middleCadence='parent-first') {
  const c=preConfig(corpus,preEnter==='none'?'revert':preEnter,typing);
  return c&&['none','correction','revert','retype'].includes(preEnter)&&['correction','erase','revert'].includes(middleParent)
    &&['parent-first','child-first','interleaved'].includes(middleCadence)?c:null;
}
export function validRequest(e,corpus='ascii',preEnter='none',typing='none',middleParent='correction',middleCadence='parent-first',parentMode='bootstrap') {
  return !!config(corpus,preEnter,typing,middleParent,middleCadence)&&e?.middleParent===middleParent&&e.middleCadence===middleCadence
    &&e.preEnter===preEnter&&preRequest({...e,preEnter:preEnter==='none'?'revert':preEnter},corpus,preEnter==='none'?'revert':preEnter,typing,parentMode);
}
export function actionPlan(corpus,preEnter='none',typing='none',middleParent='correction',middleCadence='parent-first') {
  const c=config(corpus,preEnter,typing,middleParent,middleCadence);if(!c)return null;
  const row=(index,start,end,type,after,text)=>[{type:'select',index,start,end},{type,index,after,...(text===undefined?{}:{text})}];
  const L=c.liveLeft,n=L.length,tail=c.tail;
  const parent=middleParent==='erase'?[row(0,0,n,'backspace','')]:[
    row(0,n,n,'insert',L+' draftx',' draftx'),
    ...(middleParent==='revert'?[row(0,n,n+7,'backspace',L)]:[
      row(0,n+7,n+7,'backspace',L+' draft'),row(0,n+1,n+6,'insert',L+' revised','revised')])];
  const prefix=corpus==='ascii'?'New ':'New \u65e5\u672c\u{1f600} ';
  const child=typing==='none'?[]:typing==='correction'?[
    row(1,0,tail.length,'insert','New','New'),row(1,3,3,'insert',prefix+'draftx',prefix.slice(3)+'draftx'),
    row(1,prefix.length+6,prefix.length+6,'backspace',prefix+'draft'),row(1,prefix.length,prefix.length+5,'insert',prefix+'paragraph','paragraph')]:[
    row(1,tail.length,tail.length,'insert',tail+'x','x'),
    typing==='erase'?row(1,0,tail.length+1,'backspace',''):row(1,tail.length+1,tail.length+1,'backspace',tail)];
  const reports=middleCadence==='parent-first'?[...parent,...child]:middleCadence==='child-first'?[...child,...parent]
    :Array.from({length:Math.max(parent.length,child.length)},(_,i)=>[...(parent[i]?[parent[i]]:[]),...(child[i]?[child[i]]:[])]).flat();
  const before=preEnter==='none'?[{type:'select',start:n,end:n},{type:'middle',parent:L,after:tail}]:prePlan(corpus,preEnter,'none');
  return [...before.map(a=>({...a,index:0})),...reports.flat()];
}

// Selection is the only DOM mutation; content enters through trusted CDP input.
export function inspect(expected,texts,revision,index,selection=null,complete=false) {
  const root=document.querySelector('.canvas-dialog #canvas-text-editor'),field=root?.querySelector('#canvas-text-draft');
  if(!root||!field||!field.isContentEditable||document.activeElement!==field||root.dataset.editorGeneration!==expected.generation
    ||root.dataset.canvasVisit!==expected.visit||field.dataset.canvasSplitProbe!==expected.nonce+':'+expected.ordinal)return {ok:false};
  const snapshot=window.superPlatinumRichEditor.snapshot(field),witness=window.superPlatinumCanvasSplit?.capture(field,expected.generation,expected.visit);
  const prelude=texts.length===1;
  if(prelude?witness!==null:!witness||![undefined,'middle','middle-sequence'].includes(witness.kind))return {ok:false,witness};
  const blocks=prelude?snapshot.blocks:witness.sequence?.after??witness.typing?.after??witness.after;
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
    const middles=witness?.sequence?.events.filter(e=>e.kind==='middle');
    if(prelude||witness.kind!=='middle-sequence'||middles?.length!==1||!witness.sequence.events.some(e=>e.kind==='edit'&&e.activeIndex===0&&e.revision>middles[0].revision)
      ||witness.sequence.events.some(e=>e.kind==='enter')||witness.offset!==middles[0].start||witness.revision!==middles[0].revision)return {ok:false};
    field.dataset.canvasMiddleParentComplete=expected.nonce+':'+expected.ordinal;
  }
  return {ok:true,witness,revision,html:field.innerHTML,texts,activeIndex:index,selection:{start:at(r.startContainer,r.startOffset),end:at(r.endContainer,r.endOffset)}};
}
export async function dispatch(call,sessionId,expected,result) {
  const plan=actionPlan(expected.corpus,expected.preEnter,expected.typing,expected.middleParent,expected.middleCadence);if(!plan)throw Error('Unknown middle parent plan');
  let texts=[expected.seed],revision=0,index=0;result.actions=[];
  const check=async(selection=null,complete=false)=>{const r=await call('Runtime.evaluate',{expression:`(${inspect.toString()})(${JSON.stringify(expected)},${JSON.stringify(texts)},${revision},${index},${JSON.stringify(selection)},${complete})`,returnByValue:true},sessionId);if(r.exceptionDetails||r.result?.value?.ok!==true)throw Error('Middle parent inspection failed; no replay: '+JSON.stringify(r.result?.value));return r.result.value;};
  for(const action of plan){
    const entry={action,dispatchAttempted:false,before:await check()};result.actions.push(entry);
    if(action.type==='select'){index=action.index;entry.after=await check(action);continue;}
    if(action.index!==index)throw Error('Middle parent selection target changed');
    entry.dispatchAttempted=true;result.dispatchAttempted=true;
    if(action.type==='insert')await call('Input.insertText',{text:action.text},sessionId);
    else {const enter=action.type==='middle',key=enter?'Enter':'Backspace',code=enter?13:8;
      await call('Input.dispatchKeyEvent',{type:'keyDown',key,code:key,windowsVirtualKeyCode:code,nativeVirtualKeyCode:code,...(enter?{text:'\r',unmodifiedText:'\r'}:{})},sessionId);
      await call('Input.dispatchKeyEvent',{type:'keyUp',key,code:key,windowsVirtualKeyCode:code,nativeVirtualKeyCode:code},sessionId);
    }
    if(action.type==='middle'){texts=[action.parent,action.after];index=1;}else texts[index]=action.after;
    revision++;entry.after=await check();
  }
  result.completion=await check(null,true);
}
async function main() {
  const config=JSON.parse(await fs.readFile(process.argv[2],'utf8'));
  const {port,browserPath,expected,output,corpus='ascii',preEnter='none',typing='none',middleParent='correction',middleCadence='parent-first',parentMode='bootstrap'}=config;
  if(!Number.isInteger(port)||port<1||port>65535||
      !/^\/devtools\/browser\/[a-zA-Z0-9-]+$/.test(browserPath)||
      !validRequest(expected,corpus,preEnter,typing,middleParent,middleCadence,parentMode))throw Error('Invalid bounded Canvas Enter request');
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
if(process.argv[1]?.replaceAll('\\','/').endsWith('/canvas_middle_parent_input.mjs'))await main();
