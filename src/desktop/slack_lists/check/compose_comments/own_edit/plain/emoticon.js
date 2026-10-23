((action,arg)=>{
  const root=document.getElementById('slack-list-comment-edit-draft'), input=root?.querySelector('.ql-editor');
  const api=window.superPlatinumCommentDelta, owner=window.__ownEditOriginalPlainOwner;
  const fail=error=>({ok:false,error});
  if(root!==window.__ownEditOriginalEditor||input!==window.__ownEditOriginalInput||!input?.isConnected
      ||!input.isContentEditable||root.closest('[inert],[hidden]'))return fail('Original editable input absent');
  input.scrollIntoView({block:'nearest'});input.focus();
  const rect=(()=>{const r=input.getBoundingClientRect();return {left:r.left,top:r.top,right:r.right,bottom:r.bottom,width:r.width,height:r.height}})();
  const hit=document.elementFromPoint((rect.left+rect.right)/2,(rect.top+rect.bottom)/2);
  const exposed={rect,visible:rect.width>0&&rect.height>0&&rect.left>=0&&rect.top>=0&&rect.right<=innerWidth+1&&rect.bottom<=innerHeight+1,
    hit:!!hit&&(hit===input||input.contains(hit))};
  if(!exposed.visible||!exposed.hit)return fail('Original input clipped or covered');
  const before=api?.snapshot(root);
  if(!before||before.pending||before.generation!==owner?.generation
      ||document.getElementById('slack-list-comment-edit')?.dataset.listCommentsEditScope!==owner.scope)return fail('Original owner unavailable');
  const selection=()=>{const s=getSelection();return {collapsed:s?.isCollapsed===true,contained:!!s&&input.contains(s.anchorNode)&&input.contains(s.focusNode),
    anchorOffset:s?.anchorOffset,focusOffset:s?.focusOffset,text:s?.toString()??''}};
  const result={ok:true,action,scope:owner.scope,generation:owner.generation,sameNode:true,before,exposed};
  if(action==='select'){
    if(before.composing||!before.ops.every(o=>typeof o.insert==='string'))return fail('Selection requires original literal input');
    const nodes=[],walk=document.createTreeWalker(input,NodeFilter.SHOW_TEXT);let n;while((n=walk.nextNode()))nodes.push(n);
    const point=index=>{for(const n of nodes){if(index<=n.length)return [n,index];index-=n.length}return null};
    const a=point(arg.start),b=point(arg.end);if(!a||!b||arg.start<0||arg.end<arg.start)return fail('Selection outside input');
    const r=document.createRange();r.setStart(...a);r.setEnd(...b);const s=getSelection();s.removeAllRanges();s.addRange(r);
    return {...result,start:arg.start,end:arg.end,selection:selection(),after:api.snapshot(root)};
  }
  if(action==='composition'){
    const e=new CompositionEvent(arg.active?'compositionstart':'compositionend',{bubbles:true,data:arg.active?'pending':''});
    input.dispatchEvent(e);return {...result,active:arg.active,isTrusted:e.isTrusted,after:api.snapshot(root),selection:selection()};
  }
  if(action!=='key'||!['Space','Enter','Undo','Redo'].includes(arg.key))return fail('Unknown original key');
  const key=arg.key==='Space'?' ':arg.key==='Enter'?'Enter':'z',code=arg.key==='Space'?32:arg.key==='Enter'?13:90;
  const s=selection();if(!s.contained)return fail('Selection left original input');
  const e=new KeyboardEvent('keydown',{key,code:arg.key==='Space'?'Space':arg.key==='Enter'?'Enter':'KeyZ',keyCode:code,which:code,
    ctrlKey:arg.key==='Undo'||arg.key==='Redo',shiftKey:arg.key==='Redo',bubbles:true,cancelable:true});
  input.dispatchEvent(e);
  let defaultApplied=false;
  // Synthetic keydown has no browser default. Model only ordinary Space text
  // insertion explicitly; composition is observed without inventing IME input.
  if(arg.key==='Space'&&!e.defaultPrevented&&!before.composing){
    if(!document.execCommand('insertText',false,' '))return fail('Original default text insertion refused');defaultApplied=true;
  }
  return {...result,key:arg.key,keyCode:code,isTrusted:e.isTrusted,defaultPrevented:e.defaultPrevented,defaultApplied,
    ctrlKey:e.ctrlKey,shiftKey:e.shiftKey,selectionBefore:s,selectionAfter:selection(),after:api.snapshot(root)};
})
