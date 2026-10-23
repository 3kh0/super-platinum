// One Enter in an explicitly marked, owned offline Canvas WebView. No OS input.
import fs from 'node:fs/promises';
export const histories=['bootstrap','retained-positive','retained-zero'];
export const layouts=['no-following','text-following','empty-final-following','empty-nonfinal-following'];
export const cadences=['straight','correction-before-enter','erase-before-enter','revert-before-enter','trailing-empty'];
export function validRequest(expected,history='bootstrap',layout='no-following',corpus='ascii',count=2,cadence='straight') {
  return histories.includes(history)&&layouts.includes(layout)&&cadences.includes(cadence)&&Number.isInteger(count)&&count>=2&&count<=16
    && expected?.ok===true&&['ascii','unicode'].includes(corpus)&&expected.mode==='sequence'&&expected.typing==='none'
    && expected.corpus===corpus&&expected.sequenceCount===count&&expected.sequenceHistory===history&&expected.sequenceLayout===layout&&expected.sequenceCadence===cadence
    && ['canvas-structure','canvas-structure-narrow'].includes(expected.fixture)&&/^[a-f0-9]{32}$/.test(expected.nonce)
    &&Number.isSafeInteger(expected.ordinal)&&[0,1].includes(expected.ordinal)&&typeof expected.generation==='string'&&/^\d+$/.test(expected.generation)
    &&typeof expected.visit==='string'&&/^\d+$/.test(expected.visit)&&expected.fieldId==='canvas-text-draft'
    &&expected.seed===(corpus==='ascii'?'Keep this':'Caf\u00e9 & \u65e5\u672c\u{1f600}')&&expected.offset===(corpus==='ascii'?9:11)&&expected.revision===0;
}

export function admission(expected) {
  const root=document.querySelector('.canvas-dialog #canvas-text-editor');
  const field=root?.querySelector('#canvas-text-draft');
  const selection=getSelection();
  const range=selection?.rangeCount===1?selection.getRangeAt(0):null;
  let snapshot;
  try {snapshot=window.superPlatinumRichEditor.snapshot(field);} catch {}
  if(!root||!field||document.activeElement!==field||!field.isContentEditable||
      root.dataset.editorGeneration!==expected.generation||root.dataset.canvasVisit!==expected.visit||
      field.dataset.canvasSplitProbe!==expected.nonce+':'+expected.ordinal||
      field.textContent!==expected.seed||snapshot?.revision!==expected.revision||snapshot?.composing||
      !range?.collapsed||!field.contains(range.startContainer))return {ok:false};
  const blocks=snapshot.blocks,block=Array.isArray(blocks)&&blocks.length===1?blocks[0]:null;
  if(!block||Object.keys(block).length!==2||block.type!=='rich_text'
    ||!Array.isArray(block.elements)||block.elements.length!==1)return {ok:false};
  const section=block.elements[0];
  if(!section||Object.keys(section).length!==2||section.type!=='rich_text_section'
    ||!Array.isArray(section.elements)||section.elements.length!==1
    ||Object.keys(section.elements[0]).length!==2||section.elements[0].type!=='text'||section.elements[0].text!==expected.seed)return {ok:false};
  const prefix=document.createRange();prefix.selectNodeContents(field);prefix.setEnd(range.startContainer,range.startOffset);
  const bounds=field.getBoundingClientRect();
  return {ok:prefix.toString().length===expected.offset&&bounds.width>0&&bounds.height>0&&
      bounds.left>=0&&bounds.right<=innerWidth+1&&bounds.top>=0&&bounds.bottom<=innerHeight+1,
      offset:prefix.toString().length,revision:snapshot.revision};
}

export function actionPlan(corpus,count,cadence){
  if(!['ascii','unicode'].includes(corpus)||!Number.isInteger(count)||count<1||count>16||!cadences.includes(cadence))return null;
  const target=count===8?5:count===16?14:0,result=[];
  const insert=(text,after)=>({type:'insert',text,after}),select=(start,end)=>({type:'select',start,end}),erase=after=>({type:'backspace',after});
  for(let i=0;i<count;i++){
    const name=i<3?['New','Next','Last'][i]:'Row '+(i+1),prefix=corpus==='unicode'?name+' \u65e5\u672c\u{1f600} &':name;
    result.push({type:'enter',after:''});
    if(i===count-1&&cadence==='trailing-empty')continue;
    if(i===target&&cadence!=='straight'&&cadence!=='trailing-empty'){
      result.push(insert(prefix,prefix));
      if(cadence==='erase-before-enter')result.push(select(0,prefix.length),erase(''));
      else if(cadence==='revert-before-enter')result.push(insert(' draftx',prefix+' draftx'),select(prefix.length,prefix.length+7),erase(prefix),select(0,prefix.length),erase(''));
      else result.push(insert(' draftx',prefix+' draftx'),erase(prefix+' draft'),select(prefix.length+1,prefix.length+6),insert('paragraph',prefix+' paragraph'));
    }else{const text=prefix+' paragraph';result.push(insert(text,text));}
  }
  return result;
}
export function inspect(expected,texts,revision,selection=null,complete=false){
  const root=document.querySelector('.canvas-dialog #canvas-text-editor'),field=root?.querySelector('#canvas-text-draft');
  if(!root||!field||!field.isContentEditable||document.activeElement!==field||root.dataset.editorGeneration!==expected.generation
    ||root.dataset.canvasVisit!==expected.visit||field.dataset.canvasSplitProbe!==expected.nonce+':'+expected.ordinal)return {ok:false};
  const witness=window.superPlatinumCanvasSplit?.capture(field,expected.generation,expected.visit);
  const blocks=witness?.sequence?.after??witness?.typing?.after??witness?.repeat?.after??witness?.after;
  const actual=blocks?.[0]?.elements?.map(s=>s.elements?.map(r=>r.type==='text'?r.text:null).join(''));
  const snapshot=window.superPlatinumRichEditor.snapshot(field);
  if(JSON.stringify(actual)!==JSON.stringify(texts)||snapshot.revision!==revision||snapshot.composing)return {ok:false,actual,revision:snapshot.revision,witness};
  const child=field.firstChild?.childNodes[texts.length-1],text=texts.at(-1);
  if(!child)return {ok:false};
  if(selection){
    const points=[],walker=document.createTreeWalker(child,NodeFilter.SHOW_TEXT);let offset=0;
    for(let node=walker.nextNode();node;node=walker.nextNode()){
      for(const [i,p]of[selection.start,selection.end].entries())if(!points[i]&&p>=offset&&p<=offset+node.length)points[i]=[node,p-offset];offset+=node.length;
    }
    if(!points[0]||!points[1])return {ok:false};
    const r=document.createRange();r.setStart(...points[0]);r.setEnd(...points[1]);getSelection().removeAllRanges();getSelection().addRange(r);
    if(getSelection().toString()!==text.slice(selection.start,selection.end))return {ok:false};
  }
  const current=getSelection(),range=current?.rangeCount===1?current.getRangeAt(0):null;
  if(!range||!child.contains(range.startContainer)||!child.contains(range.endContainer))return {ok:false};
  const at=(node,offset)=>{const r=document.createRange();r.selectNodeContents(child);r.setEnd(node,offset);return r.toString().length;};
  if(complete){if(witness?.kind!=='sequence'||witness.sequence.events.filter(e=>e.kind==='enter').length!==expected.sequenceCount)return {ok:false,witness};field.dataset.canvasSequenceComplete=expected.nonce+':'+expected.ordinal;}
  return {ok:true,witness,revision,html:field.innerHTML,texts,selection:{start:at(range.startContainer,range.startOffset),end:at(range.endContainer,range.endOffset)}};
}
async function dispatch(call,sessionId,expected,result){
 const plan=actionPlan(expected.corpus,expected.sequenceCount,expected.sequenceCadence);let texts=[expected.seed],revision=0;result.actions=[];
 const check=async(selection=null,complete=false)=>{const r=await call('Runtime.evaluate',{expression:`(${inspect.toString()})(${JSON.stringify(expected)},${JSON.stringify(texts)},${revision},${JSON.stringify(selection)},${complete})`,returnByValue:true},sessionId);if(r.exceptionDetails||r.result?.value?.ok!==true)throw Error('Sequence inspection failed; no replay: '+JSON.stringify(r.result?.value));return r.result.value;};
 for(const action of plan){
   const entry={action,dispatchAttempted:false};result.actions.push(entry);
   if(revision)entry.before=await check();
   if(action.type==='select'){entry.after=await check(action);continue;}
   entry.dispatchAttempted=true;result.dispatchAttempted=true;
   if(action.type==='insert')await call('Input.insertText',{text:action.text},sessionId);
   else{const enter=action.type==='enter',key=enter?'Enter':'Backspace',code=enter?13:8;
     await call('Input.dispatchKeyEvent',{type:'keyDown',key,code:key,windowsVirtualKeyCode:code,nativeVirtualKeyCode:code,...(enter?{text:'\r',unmodifiedText:'\r'}:{})},sessionId);
     await call('Input.dispatchKeyEvent',{type:'keyUp',key,code:key,windowsVirtualKeyCode:code,nativeVirtualKeyCode:code},sessionId);
   }
   if(action.type==='enter')texts.push('');else texts[texts.length-1]=action.after;
   revision++;entry.after=await check();
 }
 result.completion=await check(null,true);
}
async function main() {
  const config=JSON.parse(await fs.readFile(process.argv[2],'utf8'));
  const {port,browserPath,expected,output,sequenceHistory='bootstrap',sequenceLayout='no-following',corpus='ascii',sequenceCount=2,sequenceCadence='straight'}=config;
  if(!Number.isInteger(port)||port<1||port>65535||
      !/^\/devtools\/browser\/[a-zA-Z0-9-]+$/.test(browserPath)||
      !validRequest(expected,sequenceHistory,sequenceLayout,corpus,sequenceCount,sequenceCadence))throw Error('Invalid bounded Canvas Enter request');
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
if(process.argv[1]?.replaceAll('\\','/').endsWith('/canvas_sequence_input.mjs'))await main();
