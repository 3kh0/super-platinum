(()=>{
  const e=document.getElementById('profile-section-rich'),api=window.superPlatinumRichEditor;
  if(!e?.isContentEditable)return{ok:false};
  const initial=api.resolved(e),section=e.querySelector('.rich-editor-section');
  const tail=document.createElement('span');tail.textContent=' query';section.append(tail);
  e.focus();const r=document.createRange();r.setStart(tail.firstChild,tail.firstChild.length);r.collapse(true);
  const sel=getSelection();sel.removeAllRanges();sel.addRange(r);
  e.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'insertText'}));
  const before=api.resolved(e),ticket=api.caret(e);
  const completion=ticket&&api.complete(e,ticket,5,{type:'text',text:'completed'},'completed','');
  if(!completion)return{ok:false,error:'Ordinary completion refused'};
  const completed=api.resolved(e);
  const unitsEqual=(a,b)=>JSON.stringify(a.tokens)===JSON.stringify(b.tokens);
  if(!unitsEqual(before,completed))return{ok:false,error:'Completion changed section custody'};
  const undone=document.execCommand('undo');
  const afterUndo=api.resolved(e);
  if(!undone||!unitsEqual(before,afterUndo)||JSON.stringify(before.snapshot.blocks)!==JSON.stringify(afterUndo.snapshot.blocks))return{ok:false,error:'Native Undo changed section custody',before,completed,afterUndo};
  // The typed query was a fixture input preceding native completion; remove
  // that input only after exact Undo equality has been recorded.
  const currentTail=section.lastElementChild;
  if(currentTail?.textContent!==' query')return{ok:false,error:'Undo query tail changed'};
  currentTail.remove();e.dispatchEvent(new InputEvent('input',{bubbles:true,inputType:'deleteContentBackward'}));
  const restored=api.resolved(e);
  const equal=JSON.stringify(initial.snapshot.blocks)===JSON.stringify(restored.snapshot.blocks)&&unitsEqual(initial,restored);
  return{ok:equal,ordinaryCompletion:true,nativeUndo:true,rawSnapshotRestored:equal,before,completed,afterUndo,restored};
})()
