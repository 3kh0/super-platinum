// Fixed pre-middle cohort. All content changes use trusted CDP input; no OS input.
import fs from 'node:fs/promises';
import {admission} from './canvas_sequence_input.mjs';
import {typingPlan} from './canvas_middle_typing_input.mjs';

export function config(corpus,preEnter,typing='none') {
  if(!['ascii','unicode'].includes(corpus)||!['correction','revert','retype'].includes(preEnter)||!typingPlan(corpus,typing))return null;
  const left=corpus==='ascii'?'Keep this':'Caf\u00e9 & \u65e5\u672c\u{1f600}',tail=corpus==='ascii'?'Second paragraph':'Second \u65e5\u672c\u{1f600} & paragraph';
  return {left,tail,seed:left+tail,liveLeft:preEnter==='correction'?left+' revised':left};
}
export function validRequest(e,corpus='ascii',preEnter='correction',typing='none',parentMode='bootstrap') {
  const c=config(corpus,preEnter,typing);
  return !!c&&['bootstrap','retained','retained-zero'].includes(parentMode)&&e?.parentMode===parentMode
    &&e.parentId===(parentMode==='retained-zero'?'temp:C:Dte0000000000000000000000001':'paragraph')
    &&e?.ok===true&&e.corpus===corpus&&e.mode==='middle'&&e.typing===typing&&e.preEnter===preEnter
    &&['canvas-structure','canvas-structure-narrow'].includes(e.fixture)&&/^[a-f0-9]{32}$/.test(e.nonce)
    &&Number.isSafeInteger(e.ordinal)&&[0,1].includes(e.ordinal)&&typeof e.generation==='string'&&/^\d+$/.test(e.generation)
    &&typeof e.visit==='string'&&/^\d+$/.test(e.visit)&&e.fieldId==='canvas-text-draft'&&e.seed===c.seed&&e.offset===c.left.length&&e.revision===0;
}
export function actionPlan(corpus,preEnter,typing='none') {
  const c=config(corpus,preEnter,typing);if(!c)return null;
  const {left,tail,seed,liveLeft}=c,select=(start,end)=>({type:'select',start,end}),insert=(text,after)=>({type:'insert',text,after});
  const plan=preEnter==='retype'?[select(0,seed.length),{type:'backspace',after:''},insert(seed,seed)]
    :[insert(' draftx',left+' draftx'+tail),...(preEnter==='revert'?[select(left.length,left.length+7),{type:'backspace',after:seed}]
      :[{type:'backspace',after:left+' draft'+tail},select(left.length+1,left.length+6),insert('revised',liveLeft+tail)])];
  return [...plan,select(liveLeft.length,liveLeft.length),{type:'middle',parent:liveLeft,after:tail},...typingPlan(corpus,typing)];
}

// Serialized into only the admitted fixture target; never mutates DOM content.
export function inspect(expected,texts,revision,selection=null,complete=false) {
  const root=document.querySelector('.canvas-dialog #canvas-text-editor'),field=root?.querySelector('#canvas-text-draft');
  if(!root||!field||!field.isContentEditable||document.activeElement!==field||root.dataset.editorGeneration!==expected.generation
    ||root.dataset.canvasVisit!==expected.visit||field.dataset.canvasSplitProbe!==expected.nonce+':'+expected.ordinal)return {ok:false};
  const snapshot=window.superPlatinumRichEditor.snapshot(field),witness=window.superPlatinumCanvasSplit?.capture(field,expected.generation,expected.visit);
  const prelude=texts.length===1;
  if(prelude?witness!==null:witness?.kind!=='middle-sequence')return {ok:false,witness};
  const blocks=prelude?snapshot.blocks:witness.sequence.after;
  const actual=blocks?.[0]?.elements?.map(s=>s.elements?.map(r=>r.type==='text'?r.text:null).join(''));
  if(JSON.stringify(actual)!==JSON.stringify(texts)||snapshot.revision!==revision||snapshot.composing)return {ok:false,actual,revision:snapshot.revision};
  const index=prelude?0:1,child=field.firstChild?.childNodes[index],text=texts[index];if(!child)return {ok:false};
  if(selection){
    const points=[],walker=document.createTreeWalker(child,NodeFilter.SHOW_TEXT);let offset=0;
    for(let node=walker.nextNode();node;node=walker.nextNode()){
      for(const [i,p]of[selection.start,selection.end].entries())if(!points[i]&&p>=offset&&p<=offset+node.length)points[i]=[node,p-offset];offset+=node.length;
    }
    if(!points[0]||!points[1])return {ok:false};
    const r=document.createRange();r.setStart(...points[0]);r.setEnd(...points[1]);getSelection().removeAllRanges();getSelection().addRange(r);
    if(getSelection().toString()!==text.slice(selection.start,selection.end))return {ok:false};
  }
  const s=getSelection(),r=s?.rangeCount===1?s.getRangeAt(0):null;
  if(!r||!child.contains(r.startContainer)||!child.contains(r.endContainer))return {ok:false};
  const at=(node,n)=>{const p=document.createRange();p.selectNodeContents(child);p.setEnd(node,n);return p.toString().length;};
  if(complete){
    const middles=witness?.sequence?.events.filter(e=>e.kind==='middle');
    if(prelude||middles?.length!==1||witness.sequence.events.some(e=>e.kind==='enter')||witness.offset!==middles[0].start||witness.revision!==middles[0].revision)return {ok:false};
    field.dataset.canvasPreMiddleComplete=expected.nonce+':'+expected.ordinal;
  }
  return {ok:true,witness,revision,html:field.innerHTML,texts,selection:{start:at(r.startContainer,r.startOffset),end:at(r.endContainer,r.endOffset)}};
}
export async function dispatch(call,sessionId,expected,result) {
  const plan=actionPlan(expected.corpus,expected.preEnter,expected.typing);if(!plan)throw Error('Unknown pre-middle plan');
  let texts=[expected.seed],revision=0;result.actions=[];
  const check=async(selection=null,complete=false)=>{const r=await call('Runtime.evaluate',{expression:`(${inspect.toString()})(${JSON.stringify(expected)},${JSON.stringify(texts)},${revision},${JSON.stringify(selection)},${complete})`,returnByValue:true},sessionId);if(r.exceptionDetails||r.result?.value?.ok!==true)throw Error('Pre-middle inspection failed; no replay: '+JSON.stringify(r.result?.value));return r.result.value;};
  for(const action of plan){
    const entry={action,dispatchAttempted:false,before:await check()};result.actions.push(entry);
    if(action.type==='select'){entry.after=await check(action);continue;}
    entry.dispatchAttempted=true;result.dispatchAttempted=true;
    if(action.type==='insert')await call('Input.insertText',{text:action.text},sessionId);
    else {const enter=action.type==='middle',key=enter?'Enter':'Backspace',code=enter?13:8;
      await call('Input.dispatchKeyEvent',{type:'keyDown',key,code:key,windowsVirtualKeyCode:code,nativeVirtualKeyCode:code,...(enter?{text:'\r',unmodifiedText:'\r'}:{})},sessionId);
      await call('Input.dispatchKeyEvent',{type:'keyUp',key,code:key,windowsVirtualKeyCode:code,nativeVirtualKeyCode:code},sessionId);
    }
    if(action.type==='middle')texts=[action.parent,action.after];else texts[texts.length-1]=action.after;
    revision++;entry.after=await check();
  }
  result.completion=await check(null,true);
}
async function main() {
  const config=JSON.parse(await fs.readFile(process.argv[2],'utf8'));
  const {port,browserPath,expected,output,corpus='ascii',preEnter='correction',typing='none',parentMode='bootstrap'}=config;
  if(!Number.isInteger(port)||port<1||port>65535||
      !/^\/devtools\/browser\/[a-zA-Z0-9-]+$/.test(browserPath)||
      !validRequest(expected,corpus,preEnter,typing,parentMode))throw Error('Invalid bounded Canvas Enter request');
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
if(process.argv[1]?.replaceAll('\\','/').endsWith('/canvas_premiddle_input.mjs'))await main();
