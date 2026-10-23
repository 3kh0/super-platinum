// One Enter in an explicitly marked, owned offline Canvas WebView. No OS input.
import fs from 'node:fs/promises';
export const histories=['bootstrap','retained-positive','retained-zero'];
export const layouts=['no-following','text-following-ascii','text-following-unicode','empty-final-following','empty-nonfinal-following'];
export function validRequest(expected,history='bootstrap',layout='no-following',corpus='ascii',typing='none') {
  return histories.includes(history)&&layouts.includes(layout)&&expected?.ok===true
    && ['none','correction','erase','restore'].includes(typing)&&['ascii','unicode'].includes(corpus)
    && (typing!=='none'||corpus==='ascii')&&expected.mode==='empty'&&expected.typing===typing&&expected.corpus===corpus
    && expected.emptyHistory===history&&expected.emptyLayout===layout
    && ['canvas-structure','canvas-structure-narrow'].includes(expected.fixture)
    && /^[a-f0-9]{32}$/.test(expected.nonce)&&Number.isSafeInteger(expected.ordinal)&&expected.ordinal>=0&&expected.ordinal<=1
    && typeof expected.generation==='string'&&/^\d+$/.test(expected.generation)
    && typeof expected.visit==='string'&&/^\d+$/.test(expected.visit)
    && expected.fieldId==='canvas-text-draft'&&expected.seed===''&&expected.offset===0&&expected.revision===0;
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
    ||!Array.isArray(section.elements)||section.elements.length!==0)return {ok:false};
  const prefix=document.createRange();prefix.selectNodeContents(field);prefix.setEnd(range.startContainer,range.startOffset);
  const bounds=field.getBoundingClientRect();
  return {ok:prefix.toString().length===expected.offset&&bounds.width>0&&bounds.height>0&&
      bounds.left>=0&&bounds.right<=innerWidth+1&&bounds.top>=0&&bounds.bottom<=innerHeight+1,
      offset:prefix.toString().length,revision:snapshot.revision};
}

export function completion(expected) {
  const root=document.querySelector('.canvas-dialog #canvas-text-editor'),field=root?.querySelector('#canvas-text-draft');
  if(!field||!field.isContentEditable||document.activeElement!==field||root.dataset.editorGeneration!==expected.generation
    ||root.dataset.canvasVisit!==expected.visit||field.dataset.canvasSplitProbe!==expected.nonce+':'+expected.ordinal)return {ok:false};
  const witness=window.superPlatinumCanvasSplit?.capture(field,expected.generation,expected.visit),s=getSelection();
  const sections=field.querySelectorAll('.rich-editor-section');
  if(witness?.kind!=='empty'||witness.typing||witness.revision!==1||sections.length!==2||field.textContent!==''
    ||s?.rangeCount!==1||!s.isCollapsed||!sections[1].contains(s.anchorNode)||s.anchorOffset!==0)return {ok:false,witness};
  field.dataset.canvasEmptyComplete=expected.nonce+':'+expected.ordinal;
  return {ok:true,witness,html:field.innerHTML,caretSection:1,caretOffset:0};
}

export function typingPlan(corpus, typing) {
  if(!['ascii','unicode'].includes(corpus))return null;
  if(typing==='none')return [];
  const prefix=corpus==='unicode'?'New \u{65e5}\u{672c}\u{1f600} &':'New';
  const select=(start,end,after)=>({type:'select',start,end,after});
  const erase=after=>({type:'backspace',after});
  const insert=(text,after)=>({type:'insert',text,after});
  const initial=insert(prefix,prefix);
  if(typing==='erase')return [initial,select(0,prefix.length,prefix),erase('')];
  if(typing==='restore')return [initial,insert(' draftx',prefix+' draftx'),
    select(prefix.length,prefix.length+7,prefix+' draftx'),erase(prefix),select(0,prefix.length,prefix),erase('')];
  if(typing==='correction')return [initial,insert(' draftx',prefix+' draftx'),erase(prefix+' draft'),
    select(prefix.length+1,prefix.length+6,prefix+' draft'),insert('paragraph',prefix+' paragraph')];
  return null;
}
export function typingState(expected, childText, selection=null, complete=false) {
  const root=document.querySelector('.canvas-dialog #canvas-text-editor'),field=root?.querySelector('#canvas-text-draft');
  if(!root||!field||!field.isContentEditable||document.activeElement!==field
    ||root.dataset.editorGeneration!==expected.generation||root.dataset.canvasVisit!==expected.visit
    ||field.dataset.canvasSplitProbe!==expected.nonce+':'+expected.ordinal)return {ok:false};
  const witness=window.superPlatinumCanvasSplit?.capture(field,expected.generation,expected.visit);
  const blocks=witness?.typing?.after??witness?.after,parts=blocks?.[0]?.elements;
  const actual=parts?.map(s=>s.elements?.map(r=>r.type==='text'?r.text:null).join(''));
  if(witness?.kind!=='empty'||parts?.length!==2||actual?.[0]!==''||actual?.[1]!==childText)return {ok:false,witness,actual};
  const child=field.firstChild?.childNodes?.[1];
  if(!child||(childText!==''&&child.textContent!==childText))return {ok:false};
  if(selection){
    const points=[];let offset=0;
    const walker=document.createTreeWalker(child,NodeFilter.SHOW_TEXT);
    for(let node=walker.nextNode();node;node=walker.nextNode()){
      for(const [index,position] of [selection.start,selection.end].entries())
        if(!points[index]&&position>=offset&&position<=offset+node.length)points[index]=[node,position-offset];
      offset+=node.length;
    }
    if(!points[0]||!points[1])return {ok:false};
    const range=document.createRange();range.setStart(...points[0]);range.setEnd(...points[1]);
    const current=getSelection();current.removeAllRanges();current.addRange(range);
    if(current.toString()!==childText.slice(selection.start,selection.end))return {ok:false};
  }
  const current=getSelection(),range=current?.rangeCount===1?current.getRangeAt(0):null;
  if(!range||!child.contains(range.startContainer)||!child.contains(range.endContainer))return {ok:false};
  const position=(node,offset)=>{const r=document.createRange();r.selectNodeContents(child);r.setEnd(node,offset);return r.toString().length;};
  const result={ok:true,childText,revision:witness.typing?.revision??witness.revision,
    selection:{start:position(range.startContainer,range.startOffset),end:position(range.endContainer,range.endOffset)},witness};
  if(complete){
    field.dataset.canvasTypingComplete=expected.nonce+':'+expected.ordinal;
    field.dataset.canvasEmptyComplete=expected.nonce+':'+expected.ordinal;
  }
  return result;
}
async function dispatchTyping(call,sessionId,expected,result){
  const plan=typingPlan(expected.corpus,expected.typing);
  if(!plan?.length)throw Error('Unknown bounded empty typing plan');
  result.typing={scenario:expected.typing,actions:[],selectionMethod:'owned DOM Range; no DOM content mutation'};
  let text='';
  async function inspect(selection=null,complete=false){
    const probe=await call('Runtime.evaluate',{expression:`(${typingState.toString()})(${JSON.stringify(expected)},${JSON.stringify(text)},${JSON.stringify(selection)},${complete})`,returnByValue:true},sessionId);
    if(probe.exceptionDetails||probe.result?.value?.ok!==true)throw Error('Owned empty child typing changed; no replay: '+JSON.stringify(probe.result?.value));
    return probe.result.value;
  }
  for(const action of plan){
    const record={action,before:await inspect(),dispatchAttempted:false};result.typing.actions.push(record);
    if(action.type==='select')record.after=await inspect(action);
    else{
      record.dispatchAttempted=true;
      if(action.type==='insert')await call('Input.insertText',{text:action.text},sessionId);
      else{
        await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Backspace',code:'Backspace',windowsVirtualKeyCode:8,nativeVirtualKeyCode:8},sessionId);
        await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Backspace',code:'Backspace',windowsVirtualKeyCode:8,nativeVirtualKeyCode:8},sessionId);
      }
      text=action.after;record.after=await inspect();
    }
  }
  result.typing.final=await inspect(null,true);
  result.completion=result.typing.final;
}

async function main() {
  const config=JSON.parse(await fs.readFile(process.argv[2],'utf8'));
  const {port,browserPath,expected,output,emptyHistory='bootstrap',emptyLayout='no-following',corpus='ascii',typing='none'}=config;
  if(!Number.isInteger(port)||port<1||port>65535||
      !/^\/devtools\/browser\/[a-zA-Z0-9-]+$/.test(browserPath)||
      !validRequest(expected,emptyHistory,emptyLayout,corpus,typing))throw Error('Invalid bounded Canvas Enter request');
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
    if(typing!=='none')await dispatchTyping(call,match.sessionId,expected,result);
    else {
    const final=await call('Runtime.evaluate',{expression:`(${completion.toString()})(${JSON.stringify(expected)})`,returnByValue:true},match.sessionId);
    result.completion=final.result?.value;
    }
    if(result.completion?.ok!==true)throw Error('Empty Enter did not retain exact owned pair');
    result.ok=true;
  } catch(error){result.error=String(error);process.exitCode=1;}
  finally {clearTimeout(deadline);ws.close();await fs.writeFile(output,JSON.stringify(result,null,2)+'\n',{flag:'wx'});}
}
if(process.argv[1]?.replaceAll('\\','/').endsWith('/canvas_empty_enter_input.mjs'))await main();
