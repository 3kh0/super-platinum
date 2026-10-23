(function(action){
 const e=document.getElementById('slack-list-comment-draft'),api=window.superPlatinumRichEditor,s=getSelection();
 if(action==='arm'){
  if(!e?.isContentEditable||!api?.snapshot(e)||!s?.anchorNode||!e.contains(s.anchorNode))return {ok:false};
  window.__emptyReplyOriginal={e,anchor:s.anchorNode,focus:s.focusNode,ao:s.anchorOffset,fo:s.focusOffset,blocks:api.snapshot(e).blocks,tokens:api.resolved(e)?.tokens};
  return {ok:true,blocks:api.snapshot(e).blocks,tokens:api.resolved(e)?.tokens,selection:{anchor:s.anchorOffset,focus:s.focusOffset},focused:document.activeElement===e};
 }
 if(action!=='verify')return {ok:false};
 const original=window.__emptyReplyOriginal;
 if(!original)return {ok:false,missingOriginal:true};
 if(!original.verifyStarted){original.verifyStarted=true;requestAnimationFrame(()=>requestAnimationFrame(()=>{original.framesReady=true;}));}
 if(!original.framesReady)return {ok:false,pending:true};
 const o=window.__emptyReplyOriginal,current=document.getElementById('slack-list-comment-draft'),selection=getSelection(),v=api.snapshot(current),tokens=api.resolved(current)?.tokens;
 const sameNode=current===o?.e,sameSelection=selection?.anchorNode===o?.anchor&&selection?.focusNode===o?.focus&&selection?.anchorOffset===o?.ao&&selection?.focusOffset===o?.fo;
 return {ok:sameNode&&sameSelection&&JSON.stringify(v?.blocks)===JSON.stringify(o?.blocks)&&JSON.stringify(tokens)===JSON.stringify(o?.tokens),sameNode,sameSelection,blocks:v?.blocks,tokens,selection:{anchor:selection?.anchorOffset,focus:selection?.focusOffset},focused:document.activeElement===current};
})
