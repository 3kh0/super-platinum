(()=>{
 const e=document.getElementById('profile-section-rich'),v=e&&window.superPlatinumRichEditor?.resolved(e),s=getSelection();
 const n=e?.querySelector('.rich-editor-section > span:not(.rich-editor-token)')?.firstChild;
 if(!v||!e.isContentEditable||!n||n.length<8||!s)return{ok:false};
 const range=document.createRange();range.setStart(n,2);range.setEnd(n,8);s.removeAllRanges();s.addRange(range);
 window.__knownGroupEditor={e,pane:e.parentElement,blocks:v.snapshot.blocks,tokens:v.tokens,
  anchor:s.anchorNode,focus:s.focusNode,start:s.anchorOffset,end:s.focusOffset};
 return{ok:true,blocks:v.snapshot.blocks,tokens:v.tokens,start:s.anchorOffset,end:s.focusOffset};
})()
