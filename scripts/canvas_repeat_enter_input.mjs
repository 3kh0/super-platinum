// One Enter in an explicitly marked, owned offline Canvas WebView. No OS input.
import fs from 'node:fs/promises';
export const histories=['bootstrap','retained-positive','retained-zero'];
export const layouts=['no-following','text-following','empty-final-following','empty-nonfinal-following'];
export function validRequest(expected,history='bootstrap',layout='no-following',corpus='ascii',typing='none') {
  return histories.includes(history)&&layouts.includes(layout)&&expected?.ok===true
    && typing==='none'&&['ascii','unicode'].includes(corpus)
    && expected.mode==='repeat'&&expected.typing===typing&&expected.corpus===corpus
    && expected.repeatHistory===history&&expected.repeatLayout===layout
    && ['canvas-structure','canvas-structure-narrow'].includes(expected.fixture)
    && /^[a-f0-9]{32}$/.test(expected.nonce)&&Number.isSafeInteger(expected.ordinal)&&expected.ordinal>=0&&expected.ordinal<=1
    && typeof expected.generation==='string'&&/^\d+$/.test(expected.generation)
    && typeof expected.visit==='string'&&/^\d+$/.test(expected.visit)
    && expected.fieldId==='canvas-text-draft'&&expected.seed===(corpus==='ascii'?'Keep this':'Caf\u00e9 & \u65e5\u672c\u{1f600}')&&expected.offset===(corpus==='ascii'?9:11)&&expected.revision===0;
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

export function completion(expected, second=false) {
  const root=document.querySelector('.canvas-dialog #canvas-text-editor'),field=root?.querySelector('#canvas-text-draft');
  if(!field||!field.isContentEditable||document.activeElement!==field||root.dataset.editorGeneration!==expected.generation
    ||root.dataset.canvasVisit!==expected.visit||field.dataset.canvasSplitProbe!==expected.nonce+':'+expected.ordinal)return {ok:false};
  const witness=window.superPlatinumCanvasSplit?.capture(field,expected.generation,expected.visit),s=getSelection();
  const sections=field.querySelectorAll('.rich-editor-section'),index=second?2:1;
  if(witness?.kind!=='end'||witness.typing||witness.revision!==1||sections.length!==index+1||field.textContent!==expected.seed
    ||(second?witness.repeat?.revision!==2:!!witness.repeat)||s?.rangeCount!==1||!s.isCollapsed||!sections[index].contains(s.anchorNode)||s.anchorOffset!==0)return {ok:false,witness};
  if(second)field.dataset.canvasRepeatComplete=expected.nonce+':'+expected.ordinal;
  return {ok:true,witness,html:field.innerHTML,caretSection:index,caretOffset:0};
}

async function main() {
  const config=JSON.parse(await fs.readFile(process.argv[2],'utf8'));
  const {port,browserPath,expected,output,repeatHistory='bootstrap',repeatLayout='no-following',corpus='ascii',typing='none'}=config;
  if(!Number.isInteger(port)||port<1||port>65535||
      !/^\/devtools\/browser\/[a-zA-Z0-9-]+$/.test(browserPath)||
      !validRequest(expected,repeatHistory,repeatLayout,corpus,typing))throw Error('Invalid bounded Canvas Enter request');
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
    result.dispatchAttempted=true;
    await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Enter',code:'Enter',windowsVirtualKeyCode:13,nativeVirtualKeyCode:13,text:'\r',unmodifiedText:'\r'},match.sessionId);
    await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Enter',code:'Enter',windowsVirtualKeyCode:13,nativeVirtualKeyCode:13},match.sessionId);
    const first=await call('Runtime.evaluate',{expression:`(${completion.toString()})(${JSON.stringify(expected)},false)`,returnByValue:true},match.sessionId);
    result.first=first.result?.value;
    if(result.first?.ok!==true)throw Error('First Enter changed; second action refused');
    result.secondDispatchAttempted=true;
    await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Enter',code:'Enter',windowsVirtualKeyCode:13,nativeVirtualKeyCode:13,text:'\r',unmodifiedText:'\r'},match.sessionId);
    await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Enter',code:'Enter',windowsVirtualKeyCode:13,nativeVirtualKeyCode:13},match.sessionId);
    const final=await call('Runtime.evaluate',{expression:`(${completion.toString()})(${JSON.stringify(expected)},true)`,returnByValue:true},match.sessionId);
    result.completion=final.result?.value;
    if(result.completion?.ok!==true)throw Error('Second Enter did not retain exact owned three sections');
    result.ok=true;
  } catch(error){result.error=String(error);process.exitCode=1;}
  finally {clearTimeout(deadline);ws.close();await fs.writeFile(output,JSON.stringify(result,null,2)+'\n',{flag:'wx'});}
}
if(process.argv[1]?.replaceAll('\\','/').endsWith('/canvas_repeat_enter_input.mjs'))await main();
