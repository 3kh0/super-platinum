// Finite, caller-selected child edits in an already admitted owned offline field.
// Selection uses the DOM Range API; every content mutation is native CDP input.
export function typingPlan(corpus, typing) {
  if (!['ascii','unicode'].includes(corpus)) return null;
  if (typing==='none') return [];
  const tail=corpus==='ascii'?'Second paragraph':'Second \u65e5\u672c\u{1f600} & paragraph';
  if (typing==='erase'||typing==='restore') return [
    {type:'select',start:tail.length,end:tail.length,after:tail},
    {type:'insert',text:'x',after:tail+'x'},
    ...(typing==='erase'?[{type:'select',start:0,end:tail.length+1,after:tail+'x'}]:[]),
    {type:'backspace',after:typing==='erase'?'':tail},
  ];
  if (typing!=='correction') return null;
  const prefix=corpus==='unicode'?'New \u65e5\u672c\u{1f600} ':'New ';
  return [
    {type:'select',start:0,end:tail.length,after:tail},
    {type:'insert',text:'New',after:'New'},
    {type:'insert',text:prefix.slice(3)+'draftx',after:prefix+'draftx'},
    {type:'backspace',after:prefix+'draft'},
    {type:'select',start:prefix.length,end:prefix.length+5,after:prefix+'draft'},
    {type:'insert',text:'paragraph',after:prefix+'paragraph'},
  ];
}

// Serialized into the explicitly admitted target. No arbitrary selector/text API.
export function typingState(expected, childText, selection=null, complete=false) {
  const root=document.querySelector('.canvas-dialog #canvas-text-editor');
  const field=root?.querySelector('#canvas-text-draft');
  if (!root || !field || document.activeElement!==field || !field.isContentEditable
      || root.dataset.editorGeneration!==expected.generation || root.dataset.canvasVisit!==expected.visit
      || field.dataset.canvasSplitProbe!==expected.nonce+':'+expected.ordinal) return {ok:false};
  const witness=window.superPlatinumCanvasSplit.capture(field,expected.generation,expected.visit);
  const blocks=witness?.typing?.after ?? witness?.after;
  const sections=blocks?.[0]?.elements;
  const actual=sections?.map(s=>s.elements?.map(r=>r.type==='text'?r.text:null).join(''));
  if (!witness || (witness.typing?witness.kind!=='middle':witness.kind!==undefined) || sections?.length!==2 || actual?.[0]!==expected.seed.slice(0,expected.offset)
      || actual?.[1]!==childText) return {ok:false,witness,actual};
  const initial=witness.after?.[0]?.elements?.map(s=>s.elements?.map(r=>r.type==='text'?r.text:null).join(''));
  if(initial?.[0]!==expected.seed.slice(0,expected.offset)||initial?.[1]!==expected.seed.slice(expected.offset))return {ok:false};
  const child=field.firstChild?.childNodes?.[1];
  if (!child || (childText!=='' && child.textContent!==childText)) return {ok:false};
  if (selection) {
    const points=[];let offset=0;
    const walker=document.createTreeWalker(child,NodeFilter.SHOW_TEXT);
    for(let node=walker.nextNode();node;node=walker.nextNode()) {
      for(const [index,position] of [selection.start,selection.end].entries())
        if (!points[index] && position>=offset && position<=offset+node.length)
          points[index]=[node,position-offset];
      offset+=node.length;
    }
    if (!points[0] || !points[1]) return {ok:false};
    const range=document.createRange();range.setStart(...points[0]);range.setEnd(...points[1]);
    const current=getSelection();current.removeAllRanges();current.addRange(range);
    if (current.toString()!==childText.slice(selection.start,selection.end)) return {ok:false};
  }
  const current=getSelection(),range=current?.rangeCount===1?current.getRangeAt(0):null;
  if (!range || !child.contains(range.startContainer) || !child.contains(range.endContainer)) return {ok:false};
  const position=(node,offset)=>{const r=document.createRange();r.selectNodeContents(child);r.setEnd(node,offset);return r.toString().length;};
  const result={ok:true,childText,whiteSpace:getComputedStyle(field).whiteSpace,
    revision:witness.typing?.revision??witness.revision,
    selection:{start:position(range.startContainer,range.startOffset),end:position(range.endContainer,range.endOffset)},
    witness};
  // Fixture synchronization only. This marker never authorizes a product edit.
  if (complete) field.dataset.canvasTypingComplete=expected.nonce+':'+expected.ordinal;
  return result;
}

export async function dispatchTyping(call, sessionId, expected, corpus, typing, result) {
  const plan=typingPlan(corpus,typing);
  if (!plan || (typing!=='none' && expected.mode!=='middle'))
    throw Error('Unknown bounded typing plan');
  if (!plan.length) return;
  result.typing={scenario:typing,actions:[],selectionMethod:'owned DOM Range; no DOM content mutation'};
  let text=expected.seed.slice(expected.offset);
  async function inspect(selection=null, complete=false) {
    const probe=await call('Runtime.evaluate',{expression:`(${typingState.toString()})(${JSON.stringify(expected)},${JSON.stringify(text)},${JSON.stringify(selection)},${JSON.stringify(complete)})`,returnByValue:true},sessionId);
    if (probe.exceptionDetails || probe.result?.value?.ok!==true)
      throw Error('Owned child typing state changed; no replay: '+JSON.stringify(probe.result?.value));
    return probe.result.value;
  }
  const initial=await inspect();
  if(initial.selection.start!==0||initial.selection.end!==0||initial.revision!==1)throw Error('Native middle Enter did not leave child caret zero');
  result.typing.initial=initial;
  for(const action of plan) {
    const before=await inspect();
    const record={action,before,dispatchAttempted:false};result.typing.actions.push(record);
    if (action.type==='select') record.after=await inspect(action);
    else {
      record.dispatchAttempted=true;
      if (action.type==='insert') await call('Input.insertText',{text:action.text},sessionId);
      else {
        await call('Input.dispatchKeyEvent',{type:'keyDown',key:'Backspace',code:'Backspace',windowsVirtualKeyCode:8,nativeVirtualKeyCode:8},sessionId);
        await call('Input.dispatchKeyEvent',{type:'keyUp',key:'Backspace',code:'Backspace',windowsVirtualKeyCode:8,nativeVirtualKeyCode:8},sessionId);
      }
      text=action.after;
      record.after=await inspect();
    }
  }
  result.typing.final=await inspect(null,true);
}
