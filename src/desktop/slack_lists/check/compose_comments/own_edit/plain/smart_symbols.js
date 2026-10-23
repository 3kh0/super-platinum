((action,arg)=>{
  const root=document.getElementById('slack-list-comment-edit-draft'),input=root?.querySelector('.ql-editor');
  const api=window.superPlatinumCommentDelta,owner=window.__ownEditOriginalPlainOwner;
  const fail=error=>({ok:false,error});
  if(root!==window.__ownEditOriginalEditor||input!==window.__ownEditOriginalInput||!input?.isConnected||!input.isContentEditable
      ||root.closest('[inert],[hidden]'))return fail('Original editable input absent');
  input.scrollIntoView({block:'nearest'});input.focus();
  const r=input.getBoundingClientRect(),rect={left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height};
  const hit=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);
  const exposed={rect,visible:r.width>0&&r.height>0&&r.left>=0&&r.top>=0&&r.right<=innerWidth+1&&r.bottom<=innerHeight+1,hit:!!hit&&(hit===input||input.contains(hit))};
  if(!exposed.visible||!exposed.hit)return fail('Original input clipped or covered');
  const before=api?.snapshot(root);
  if(!before||before.generation!==owner?.generation||document.getElementById('slack-list-comment-edit')?.dataset.listCommentsEditScope!==owner.scope)return fail('Original owner unavailable');
  const selection=()=>{const s=getSelection();return {collapsed:s?.isCollapsed===true,contained:!!s&&input.contains(s.anchorNode)&&input.contains(s.focusNode),anchorOffset:s?.anchorOffset,focusOffset:s?.focusOffset,text:s?.toString()??''}};
  const result={ok:true,action,scope:owner.scope,generation:owner.generation,sameNode:true,before,exposed};
  if(action==='state'){
    const save=document.getElementById('slack-list-comment-edit-save'),cancel=document.getElementById('slack-list-comment-edit-cancel');
    return {...result,ok:!before.pending&&!before.composing&&save?.disabled===false&&cancel?.disabled===false
      &&(!arg.ops||JSON.stringify(before.ops)===JSON.stringify(arg.ops)),after:before,selection:selection(),controls:{saveDisabled:save?.disabled,cancelDisabled:cancel?.disabled}};
  }
  if(before.pending)return fail('Original editor queue pending');
  if(action==='select'){
    if(before.composing||!before.ops.every(o=>typeof o.insert==='string'))return fail('Selection requires literal input');
    const nodes=[],walk=document.createTreeWalker(input,NodeFilter.SHOW_TEXT);let n;while((n=walk.nextNode()))nodes.push(n);
    const point=index=>{for(const n of nodes){if(index<=n.length)return [n,index];index-=n.length}return null};
    const a=point(arg.start),b=point(arg.end);if(!a||!b||arg.start<0||arg.end<arg.start)return fail('Selection outside original input');
    const range=document.createRange();range.setStart(...a);range.setEnd(...b);const s=getSelection();s.removeAllRanges();s.addRange(range);
    return {...result,start:arg.start,end:arg.end,after:api.snapshot(root),selection:selection()};
  }
  if(action==='composition'){
    const e=new CompositionEvent(arg.active?'compositionstart':'compositionend',{bubbles:true,data:arg.active?'pending':''});input.dispatchEvent(e);
    return {...result,active:arg.active,isTrusted:e.isTrusted,after:api.snapshot(root),selection:selection()};
  }
  if(action==='insert'){
    if(typeof arg.text!=='string'||before.composing!==!!arg.composing)return fail('Unobserved original input/composition phase');
    const s=selection();if(!s.contained)return fail('Foreign input selection');
    if(!document.execCommand('insertText',false,arg.text))return fail('Original default text insertion refused');
    return {...result,text:arg.text,composing:before.composing,selectionBefore:s,selectionAfter:selection(),after:api.snapshot(root),inputCommand:'insertText'};
  }
  if(action!=='key'||!['Space','Period','Enter','Undo','Redo'].includes(arg.key))return fail('Unknown original key');
  const s=selection();if(!s.contained)return fail('Foreign original key selection');
  const key=arg.key==='Space'?' ':arg.key==='Period'?'.':arg.key==='Enter'?'Enter':'z';
  const code=arg.key==='Space'?32:arg.key==='Period'?190:arg.key==='Enter'?13:90;
  const e=new KeyboardEvent('keydown',{key,code:arg.key==='Space'?'Space':arg.key==='Period'?'Period':arg.key==='Enter'?'Enter':'KeyZ',keyCode:code,which:code,
    ctrlKey:arg.key==='Undo'||arg.key==='Redo',shiftKey:arg.key==='Redo',altKey:false,metaKey:false,bubbles:true,cancelable:true});
  input.dispatchEvent(e);let defaultApplied=false;
  // Synthetic keydown has no browser default. The exact unconsumed literal
  // Space/Period is inserted through the real input command, never as a Delta.
  if(['Space','Period'].includes(arg.key)&&!e.defaultPrevented&&!before.composing){
    if(!document.execCommand('insertText',false,key))return fail('Original default insertion refused');defaultApplied=true;
  }
  return {...result,key:arg.key,keyCode:code,isTrusted:e.isTrusted,defaultPrevented:e.defaultPrevented,defaultApplied,ctrlKey:e.ctrlKey,shiftKey:e.shiftKey,altKey:e.altKey,metaKey:e.metaKey,
    selectionBefore:s,selectionAfter:selection(),after:api.snapshot(root)};
})
